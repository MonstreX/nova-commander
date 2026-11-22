//! The program's file calls on a session (API 9, S6.8b): its background tasks read and write a remote
//! file in pieces by offset (ADR-0016 §13) - here kept open between the calls: a read going on where the
//! last one stopped reads on, another place opens it again there; a file written anew is streamed to the
//! server as it comes (SFTP, FTP, WebDAV) and finished at its end (`WRITE_END`), or, for a protocol that
//! must know its size first (SCP's record, S3's PUT or multipart plan), kept in a file of the disk's
//! temporary folder and sent whole at its end. A copy that failed or stopped (`WRITE_ABORT`) lets go of
//! what it kept: never sent as the file. A call stopped halfway leaves nothing open behind it.

use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use nc_plugin_sdk::{Error, ErrorKind, WriteMode};

use crate::fs::{Fs, FsError, FsFile};
use crate::sftp::RemoteEntry;

/// A file being read: where its next byte is.
struct Reading {
    path: String,
    file: FsFile,
    at: u64,
}

/// A file of the disk's temporary folder that holds what is written until its end; removed when let go.
struct Spool {
    file: std::fs::File,
    place: PathBuf,
}

impl Drop for Spool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.place);
    }
}

/// A file being written.
enum Writing {
    /// Sent to the server as it comes.
    Streamed { path: String, file: FsFile },
    /// Kept on the disk until its end (its size known then).
    Spooled { path: String, spool: Spool },
}

impl Writing {
    fn path(&self) -> &str {
        match self {
            Self::Streamed { path, .. } | Self::Spooled { path, .. } => path,
        }
    }
}

/// The open files of a session's file calls.
#[derive(Default)]
pub struct Files {
    reading: Option<Reading>,
    writing: Option<Writing>,
}

/// The rights a file made by the program's copy gets (WinSCP's default).
const RIGHTS: u16 = 0o644;

fn failed(error: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::Other, error.to_string())
}

/// Whether a failure says the entry is not there: SFTP's status tells; the other protocols' texts do
/// not, and count as not there (as the panel's own copy takes them).
fn missing(error: &FsError) -> bool {
    match error {
        FsError::Sftp(russh_sftp::client::error::Error::Status(status)) => {
            status.status_code == russh_sftp::protocol::StatusCode::NoSuchFile
        }
        FsError::Sftp(_) => false,
        FsError::Shell(_) => true,
    }
}

/// A failure of a call on `path`: `NotFound` only when it says so.
fn on_path(error: &FsError) -> Error {
    let kind = match missing(error) {
        true => ErrorKind::NotFound,
        false => ErrorKind::Other,
    };
    Error::new(kind, error.to_string())
}

/// A spool's place in the disk's temporary folder, one of its own each time.
fn spool_place() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let number = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("netway-{}-{number}.spool", std::process::id()))
}

/// Whether the protocol must know a file's size before its first byte.
fn needs_size(fs: &Fs) -> bool {
    matches!(fs.protocol(), "SCP" | "S3")
}

/// The entry `path` of the server as the program's file calls see it: what a link leads to, and that it
/// is one (R7 review C1: the program's move walked into a link as a folder and removed what it led to).
pub async fn stat(fs: &Fs, path: &str) -> Result<RemoteEntry, Error> {
    let meta = fs.metadata(path).await.map_err(|error| on_path(&error))?;
    let link = (fs.symlink_metadata(path).await).is_ok_and(|own| own.is_symlink());
    let name = path.rsplit('/').next().unwrap_or_default();
    Ok(RemoteEntry {
        name: name.to_string(),
        size: meta.size.unwrap_or(0),
        is_dir: meta.is_dir(),
        is_link: link,
        modified: meta.mtime,
        permissions: meta.permissions,
        ..RemoteEntry::default()
    })
}

/// Fails with `Exists` when `path` is there; a failure that does not say it is missing is the call's.
pub async fn absent(fs: &Fs, path: &str) -> Result<(), Error> {
    match fs.metadata(path).await {
        Ok(_) => Err(Error::new(ErrorKind::Exists, format!("{path} is there"))),
        Err(error) if missing(&error) => Ok(()),
        Err(error) => Err(failed(error)),
    }
}

impl Files {
    /// Whether a file is being written (its transfer going on: the connection is asked nothing else).
    pub fn writes(&self) -> bool {
        self.writing.is_some()
    }

    /// The file being read let go: its next read opens it again there.
    pub fn let_go_reading(&mut self) {
        self.reading = None;
    }

    /// Up to `buffer.len()` bytes of `path` from `offset` (fewer only at its end).
    pub async fn read(
        &mut self,
        fs: &Fs,
        path: &str,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize, Error> {
        // Taken out while read: a call stopped halfway keeps nothing whose place is not known.
        let kept = (self.reading.take()).filter(|it| it.path == path && it.at == offset);
        let mut reading = match kept {
            Some(reading) => reading,
            None => {
                let mut file = fs.open(path).await.map_err(|error| on_path(&error))?;
                if offset > 0 {
                    file.seek(SeekFrom::Start(offset)).await.map_err(failed)?;
                }
                Reading {
                    path: path.to_string(),
                    file,
                    at: offset,
                }
            }
        };
        let mut done = 0;
        while done < buffer.len() {
            let count = reading.file.read(&mut buffer[done..]).await;
            match count.map_err(failed)? {
                0 => break,
                count => done += count,
            }
        }
        reading.at += done as u64;
        // At its end nothing more is kept open.
        if done == buffer.len() {
            self.reading = Some(reading);
        }
        Ok(done)
    }

    /// `data` written to `path`: `New` makes it (never over an entry there), `Append` adds to it.
    pub async fn write(
        &mut self,
        fs: &Fs,
        path: &str,
        mode: WriteMode,
        data: &[u8],
    ) -> Result<(), Error> {
        // Taken out while written: a call failed or stopped halfway leaves nothing to go on with.
        let kept = self.writing.take();
        let mut writing = match (mode, kept) {
            (WriteMode::New, before) => {
                // One written before it and never ended or abandoned: let go, never sent as a file.
                drop(before);
                absent(fs, path).await?;
                Self::started(fs, path).await?
            }
            (_, Some(writing)) if writing.path() == path => writing,
            (_, before) => {
                drop(before);
                let mut file = fs.open_for_writing(path).await.map_err(failed)?;
                file.seek(SeekFrom::End(0)).await.map_err(failed)?;
                Writing::Streamed {
                    path: path.to_string(),
                    file,
                }
            }
        };
        match &mut writing {
            Writing::Streamed { file, .. } => file.write_all(data).await.map_err(failed)?,
            Writing::Spooled { spool, .. } => spool.file.write_all(data).map_err(failed)?,
        }
        self.writing = Some(writing);
        Ok(())
    }

    async fn started(fs: &Fs, path: &str) -> Result<Writing, Error> {
        if needs_size(fs) {
            let place = spool_place();
            let file = (std::fs::OpenOptions::new())
                .read(true)
                .write(true)
                .create_new(true)
                .open(&place)
                .map_err(failed)?;
            return Ok(Writing::Spooled {
                path: path.to_string(),
                spool: Spool { file, place },
            });
        }
        let file = fs.create(path, 0, RIGHTS).await.map_err(failed)?;
        Ok(Writing::Streamed {
            path: path.to_string(),
            file,
        })
    }

    /// `path` is whole: a streamed file finished, a kept one sent whole.
    pub async fn end(&mut self, fs: &Fs, path: &str) -> Result<(), Error> {
        if self.writing.as_ref().is_none_or(|it| it.path() != path) {
            return Ok(());
        }
        match self.writing.take() {
            Some(Writing::Streamed { mut file, .. }) => file.shutdown().await.map_err(failed),
            Some(Writing::Spooled { path, mut spool }) => {
                Self::sent_whole(fs, &path, &mut spool.file).await
            }
            None => Ok(()),
        }
    }

    async fn sent_whole(fs: &Fs, path: &str, spool: &mut std::fs::File) -> Result<(), Error> {
        let size = spool.seek(SeekFrom::End(0)).map_err(failed)?;
        spool.seek(SeekFrom::Start(0)).map_err(failed)?;
        let mut file = fs.create(path, size, RIGHTS).await.map_err(failed)?;
        let mut piece = vec![0; 256 * 1024];
        loop {
            let count = spool.read(&mut piece).map_err(failed)?;
            if count == 0 {
                break;
            }
            file.write_all(&piece[..count]).await.map_err(failed)?;
        }
        file.shutdown().await.map_err(failed)
    }

    /// Where the file being written is kept until its end, if it is.
    #[cfg(test)]
    pub fn spool(&self) -> Option<&std::path::Path> {
        match &self.writing {
            Some(Writing::Spooled { spool, .. }) => Some(&spool.place),
            _ => None,
        }
    }

    /// `path`'s copy failed or stopped: what is open for it let go (a spool removed), nothing sent.
    pub fn abort(&mut self, path: &str) {
        if self.writing.as_ref().is_some_and(|it| it.path() == path) {
            self.writing = None;
        }
        if self.reading.as_ref().is_some_and(|it| it.path == path) {
            self.reading = None;
        }
    }
}
