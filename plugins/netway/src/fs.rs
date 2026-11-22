//! The server's file system as a session reaches it (WinSCP's `TCustomFileSystem`): SFTP, or SCP through
//! the shell's commands, later FTP, WebDAV and S3 - each answering the same calls (`SftpSession`'s names),
//! each saying what it cannot do (WinSCP's `IsCapable`). A file opened is read, written and sought through
//! [`FsFile`]. The connection's commands, when the server runs them, also copy on the server for SFTP
//! (WinSCP's secondary shell for `cp`).

use std::io::SeekFrom;

use russh_sftp::client::SftpSession;
use russh_sftp::client::fs::{File, Metadata};
use russh_sftp::protocol::{FileAttributes, OpenFlags};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::ftp::{Ftp, FtpFile, Way};
use crate::s3::{S3, S3File};
use crate::scp::{self, Scp};
use crate::session_options::Options;
use crate::sftp::RemoteEntry;
use crate::shell::{Commands, quoted};
use crate::webdav::{Dav, DavFile};

/// A failure of a call, as its protocol tells it.
#[derive(Debug)]
pub enum FsError {
    Sftp(russh_sftp::client::error::Error),
    Shell(String),
}

impl std::fmt::Display for FsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sftp(error) => error.fmt(formatter),
            Self::Shell(text) => formatter.write_str(text),
        }
    }
}

impl From<russh_sftp::client::error::Error> for FsError {
    fn from(error: russh_sftp::client::error::Error) -> Self {
        Self::Sftp(error)
    }
}

impl From<String> for FsError {
    fn from(text: String) -> Self {
        Self::Shell(text)
    }
}

pub type FsResult<T> = Result<T, FsError>;

/// What a file system may not do (WinSCP's `TFSCapability`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// Writing from an offset: append and resume.
    Resume,
    HardLink,
}

/// The protocol's side of a session.
enum Files {
    Sftp(SftpSession),
    Scp(Scp),
    Ftp(Ftp),
    WebDav(Dav),
    S3(S3),
}

/// A session's file system.
pub struct Fs {
    files: Files,
    /// The connection's commands, when the session may run them.
    commands: Option<Commands>,
    /// The session's other tabs' settings.
    options: Options,
}

/// A file open on the server.
pub enum FsFile {
    Sftp(Box<File>),
    ScpIn(scp::Receiving),
    ScpOut(scp::Sending),
    Ftp(Box<FtpFile>),
    WebDav(Box<DavFile>),
    S3(Box<S3File>),
}

fn unsupported() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Unsupported, "Operation not supported.")
}

impl FsFile {
    pub async fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Sftp(file) => file.read(buffer).await,
            Self::ScpIn(file) => file.read(buffer).await,
            Self::Ftp(file) => file.read(buffer).await,
            Self::WebDav(file) => file.read(buffer).await,
            Self::S3(file) => file.read(buffer).await,
            Self::ScpOut(_) => Err(unsupported()),
        }
    }

    pub async fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        match self {
            Self::Sftp(file) => file.write_all(data).await,
            Self::ScpOut(file) => file.write_all(data).await,
            Self::Ftp(file) => file.write_all(data).await,
            Self::WebDav(file) => file.write_all(data).await,
            Self::S3(file) => file.write_all(data).await,
            Self::ScpIn(_) => Err(unsupported()),
        }
    }

    /// The position moved (SCP's files only stay at their start).
    pub async fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        match (self, position) {
            (Self::Sftp(file), _) => file.seek(position).await,
            (Self::Ftp(file), SeekFrom::Start(offset)) => file.seek(offset),
            (Self::WebDav(file), SeekFrom::Start(offset)) => file.seek(offset),
            (Self::S3(file), SeekFrom::Start(offset)) => file.seek(offset),
            (_, SeekFrom::Start(0)) => Ok(0),
            _ => Err(unsupported()),
        }
    }

    pub async fn shutdown(&mut self) -> std::io::Result<()> {
        match self {
            Self::Sftp(file) => file.shutdown().await,
            Self::ScpOut(file) => file.shutdown().await,
            Self::Ftp(file) => file.shutdown().await,
            Self::WebDav(file) => file.shutdown().await,
            Self::S3(file) => file.shutdown().await,
            Self::ScpIn(_) => Ok(()),
        }
    }
}

/// A command's failure; a channel the server refuses: `None`.
async fn run_if_allowed(commands: &Commands, command: &str) -> Option<Result<(), String>> {
    let output = commands.run(command).await.ok()?;
    Some(match output.status {
        0 => Ok(()),
        _ => Err(output.failure()),
    })
}

impl Fs {
    /// SFTP's, with the connection's `commands` when the session may run them, and its `options`.
    pub fn sftp(sftp: SftpSession, commands: Option<Commands>, options: &Options) -> Self {
        Self {
            files: Files::Sftp(sftp),
            commands,
            options: options.clone(),
        }
    }

    /// SCP's over the connection's `commands`, with the session's `options`.
    pub fn scp(commands: Commands, options: &Options) -> Self {
        Self {
            files: Files::Scp(Scp::new(commands.clone(), options)),
            commands: Some(commands),
            options: options.clone(),
        }
    }

    /// The server's ends of lines are CR/LF: the text mode leaves them (the session's Environment).
    pub fn crlf(&self) -> bool {
        self.options.eol_crlf
    }

    /// The recycle bin a deleted (or, over SFTP, an overwritten) item goes to instead (WinSCP's
    /// `DeleteToRecycleBin`, `OverwrittenToRecycleBin`, `RecycleBinPath`); none: removed.
    pub fn recycle_bin(&self, overwritten: bool) -> Option<&str> {
        let options = &self.options;
        let wanted = match overwritten {
            true => options.overwritten_to_recycle_bin && matches!(self.files, Files::Sftp(_)),
            false => options.delete_to_recycle_bin,
        };
        let path = options.recycle_bin_path.trim();
        (wanted && !path.is_empty()).then_some(path)
    }

    /// The connection's commands, when the session may run them.
    pub fn commands(&self) -> Option<&Commands> {
        self.commands.as_ref()
    }

    /// The session's settings.
    pub fn options(&self) -> &Options {
        &self.options
    }

    /// FTP's over its control connection, with the session's `options`.
    pub fn ftp(ftp: Ftp, options: &Options) -> Self {
        Self {
            files: Files::Ftp(ftp),
            commands: None,
            options: options.clone(),
        }
    }

    /// WebDAV's over HTTP(S), with the session's `options`.
    pub fn webdav(dav: Dav, options: &Options) -> Self {
        Self {
            files: Files::WebDav(dav),
            commands: None,
            options: options.clone(),
        }
    }

    /// S3's over HTTP(S), with the session's `options`.
    pub fn s3(s3: S3, options: &Options) -> Self {
        Self {
            files: Files::S3(s3),
            commands: None,
            options: options.clone(),
        }
    }

    /// WinSCP's protocol name: "SFTP", "SCP", "FTP", "WebDAV" or "S3".
    pub fn protocol(&self) -> &'static str {
        match self.files {
            Files::Sftp(_) => "SFTP",
            Files::Scp(_) => "SCP",
            Files::Ftp(_) => "FTP",
            Files::WebDav(_) => "WebDAV",
            Files::S3(_) => "S3",
        }
    }

    /// Whether the session still answers (FTP's control connection asked once idle for `idle`; the other
    /// protocols' are SSH's, watched by its connection, or HTTP's, opened anew as needed).
    pub async fn alive(&self, idle: std::time::Duration) -> bool {
        match &self.files {
            Files::Ftp(ftp) => ftp.alive(idle).await,
            _ => true,
        }
    }

    /// Whether it can do `capability`.
    pub fn can(&self, capability: Capability) -> bool {
        match (&self.files, capability) {
            (Files::Sftp(_), _) => true,
            (Files::Scp(_), Capability::HardLink) => true,
            (Files::Scp(_), Capability::Resume) => false,
            (Files::Ftp(_), Capability::Resume) => true,
            (Files::Ftp(_), Capability::HardLink) => false,
            (Files::WebDav(_) | Files::S3(_), _) => false,
        }
    }

    /// The item `path` as what it points to.
    pub async fn metadata(&self, path: impl AsRef<str>) -> FsResult<Metadata> {
        let path = path.as_ref();
        match &self.files {
            Files::Sftp(sftp) => Ok(sftp.metadata(path).await?),
            Files::Scp(scp) => Ok(scp::attributes(&scp.entry(path, true).await?.entry)),
            Files::Ftp(ftp) => Ok(scp::attributes(&ftp.entry(path).await?)),
            Files::WebDav(dav) => Ok(scp::attributes(&dav.entry(path).await?)),
            Files::S3(s3) => Ok(scp::attributes(&s3.entry(path).await?)),
        }
    }

    /// The item `path` as it is (a link as a link).
    pub async fn symlink_metadata(&self, path: impl AsRef<str>) -> FsResult<Metadata> {
        let path = path.as_ref();
        match &self.files {
            Files::Sftp(sftp) => Ok(sftp.symlink_metadata(path).await?),
            Files::Scp(scp) => Ok(scp::attributes(&scp.entry(path, false).await?.entry)),
            Files::Ftp(ftp) => Ok(scp::attributes(&ftp.entry(path).await?)),
            Files::WebDav(dav) => Ok(scp::attributes(&dav.entry(path).await?)),
            Files::S3(s3) => Ok(scp::attributes(&s3.entry(path).await?)),
        }
    }

    pub async fn set_metadata(
        &self,
        path: impl AsRef<str>,
        attributes: FileAttributes,
    ) -> FsResult<()> {
        let path = path.as_ref();
        match &self.files {
            Files::Sftp(sftp) => Ok(sftp.set_metadata(path, attributes).await?),
            Files::Scp(scp) => Ok(scp.set_attributes(path, &attributes).await?),
            Files::Ftp(ftp) => Ok(ftp.set_attributes(path, &attributes).await?),
            // WebDAV and S3 have no way to set times or rights (WinSCP's: not kept, not asked).
            Files::WebDav(_) | Files::S3(_) => Ok(()),
        }
    }

    pub async fn remove_file(&self, path: impl AsRef<str>) -> FsResult<()> {
        let path = path.as_ref();
        match &self.files {
            Files::Sftp(sftp) => Ok(sftp.remove_file(path).await?),
            Files::Scp(scp) => Ok(scp.run(&format!("rm -f {}", quoted(path))).await?),
            Files::Ftp(ftp) => Ok(ftp.remove(path).await?),
            Files::WebDav(dav) => Ok(dav.delete(path).await?),
            Files::S3(s3) => Ok(s3.remove(path).await?),
        }
    }

    pub async fn remove_dir(&self, path: impl AsRef<str>) -> FsResult<()> {
        let path = path.as_ref();
        match &self.files {
            Files::Sftp(sftp) => Ok(sftp.remove_dir(path).await?),
            Files::Scp(scp) => Ok(scp.run(&format!("rmdir {}", quoted(path))).await?),
            Files::Ftp(ftp) => Ok(ftp.rmdir(path).await?),
            Files::WebDav(dav) => Ok(dav.delete(path).await?),
            Files::S3(s3) => Ok(s3.rmdir(path).await?),
        }
    }

    pub async fn create_dir(&self, path: impl AsRef<str>) -> FsResult<()> {
        let path = path.as_ref();
        match &self.files {
            Files::Sftp(sftp) => Ok(sftp.create_dir(path).await?),
            Files::Scp(scp) => Ok(scp.run(&format!("mkdir {}", quoted(path))).await?),
            Files::Ftp(ftp) => Ok(ftp.mkdir(path).await?),
            Files::WebDav(dav) => Ok(dav.mkdir(path).await?),
            Files::S3(s3) => Ok(s3.mkdir(path).await?),
        }
    }

    pub async fn rename(&self, from: impl AsRef<str>, to: impl AsRef<str>) -> FsResult<()> {
        let (from, to) = (from.as_ref(), to.as_ref());
        match &self.files {
            Files::Sftp(sftp) => Ok(sftp.rename(from, to).await?),
            Files::Ftp(ftp) => Ok(ftp.rename(from, to).await?),
            Files::WebDav(dav) => Ok(dav.rename(from, to).await?),
            Files::S3(s3) => Ok(s3.rename(from, to).await?),
            Files::Scp(scp) => {
                let command = format!("mv -f {} {}", quoted(from), quoted(to));
                Ok(scp.run(&command).await?)
            }
        }
    }

    /// A symbolic link `path` to `target`.
    pub async fn symlink(&self, path: impl AsRef<str>, target: impl AsRef<str>) -> FsResult<()> {
        let (path, target) = (path.as_ref(), target.as_ref());
        match &self.files {
            Files::Sftp(sftp) => Ok(sftp.symlink(path, target).await?),
            Files::Ftp(_) | Files::WebDav(_) | Files::S3(_) => {
                Err(FsError::Shell(unsupported().to_string()))
            }
            Files::Scp(scp) => {
                let command = format!("ln -s {} {}", quoted(target), quoted(path));
                Ok(scp.run(&command).await?)
            }
        }
    }

    pub async fn read_link(&self, path: impl AsRef<str>) -> FsResult<String> {
        let path = path.as_ref();
        match &self.files {
            Files::Sftp(sftp) => Ok(sftp.read_link(path).await?),
            Files::Ftp(_) | Files::WebDav(_) | Files::S3(_) => {
                Err(FsError::Shell(unsupported().to_string()))
            }
            Files::Scp(scp) => (scp.entry(path, false).await?.link_to)
                .ok_or_else(|| FsError::Shell(format!("{path}: not a link"))),
        }
    }

    /// A hard link; false when the server makes none.
    pub async fn hardlink(&self, from: impl AsRef<str>, to: impl AsRef<str>) -> FsResult<bool> {
        let (from, to) = (from.as_ref(), to.as_ref());
        match &self.files {
            Files::Sftp(sftp) => Ok(sftp.hardlink(from, to).await?),
            Files::Ftp(_) | Files::WebDav(_) | Files::S3(_) => Ok(false),
            Files::Scp(scp) => {
                let command = format!("ln {} {}", quoted(from), quoted(to));
                scp.run(&command).await?;
                Ok(true)
            }
        }
    }

    pub async fn canonicalize(&self, path: impl AsRef<str>) -> FsResult<String> {
        let path = path.as_ref();
        match &self.files {
            Files::Sftp(sftp) => Ok(sftp.canonicalize(path).await?),
            Files::Scp(scp) => Ok(scp.canonicalize(path).await?),
            Files::Ftp(ftp) => Ok(ftp.canonicalize(path).await?),
            Files::WebDav(dav) => Ok(dav.canonicalize(path).await?),
            Files::S3(s3) => Ok(s3.canonicalize(path).await?),
        }
    }

    /// The file `path` open for reading from its start.
    pub async fn open(&self, path: impl AsRef<str>) -> FsResult<FsFile> {
        let path = path.as_ref();
        match &self.files {
            Files::Sftp(sftp) => Ok(FsFile::Sftp(Box::new(sftp.open(path).await?))),
            Files::Scp(scp) => Ok(FsFile::ScpIn(scp.receive(path).await?)),
            Files::Ftp(ftp) => Ok(FsFile::Ftp(Box::new(ftp.file(path, Way::Read)))),
            Files::WebDav(dav) => Ok(FsFile::WebDav(Box::new(dav.file(path, false)))),
            Files::S3(s3) => Ok(FsFile::S3(Box::new(s3.file(path, None)))),
        }
    }

    /// The file `path` made anew for `size` bytes with `rights` (SCP's record needs both first).
    pub async fn create(&self, path: impl AsRef<str>, size: u64, rights: u16) -> FsResult<FsFile> {
        let path = path.as_ref();
        match &self.files {
            Files::Sftp(sftp) => Ok(FsFile::Sftp(Box::new(sftp.create(path).await?))),
            Files::Scp(scp) => Ok(FsFile::ScpOut(scp.send(path, size, rights).await?)),
            Files::Ftp(ftp) => Ok(FsFile::Ftp(Box::new(ftp.file(path, Way::Write)))),
            Files::WebDav(dav) => Ok(FsFile::WebDav(Box::new(dav.file(path, true)))),
            Files::S3(s3) => Ok(FsFile::S3(Box::new(s3.file(path, Some(size))))),
        }
    }

    /// The file `path` open for writing where it is (append, resume: not over SCP).
    pub async fn open_for_writing(&self, path: impl AsRef<str>) -> FsResult<FsFile> {
        match &self.files {
            Files::Sftp(sftp) => {
                let file = sftp
                    .open_with_flags(path.as_ref(), OpenFlags::WRITE)
                    .await?;
                Ok(FsFile::Sftp(Box::new(file)))
            }
            Files::Ftp(ftp) => Ok(FsFile::Ftp(Box::new(ftp.file(path.as_ref(), Way::Append)))),
            Files::Scp(_) | Files::WebDav(_) | Files::S3(_) => {
                Err(FsError::Shell(unsupported().to_string()))
            }
        }
    }

    /// `from` copied to `to` on the server by WinSCP's `cp -p -r -f`; false when the session runs no
    /// commands (then the files are copied through the client).
    pub async fn copy(&self, from: &str, to: &str) -> FsResult<bool> {
        // FTP's one control connection carries no two transfers at once (WinSCP's `fcRemoteCopy`).
        if let Files::WebDav(dav) = &self.files {
            // WebDAV's COPY: the server copies.
            return dav
                .copy(from, to)
                .await
                .map(|()| true)
                .map_err(FsError::Shell);
        }
        if let Files::S3(s3) = &self.files {
            // S3's CopyObject copies an object; a folder's objects are copied through the client.
            return match s3.entry(from).await?.is_dir {
                true => Ok(false),
                false => s3
                    .copy(from, to)
                    .await
                    .map(|()| true)
                    .map_err(FsError::Shell),
            };
        }
        if matches!(self.files, Files::Ftp(_)) {
            return Err(FsError::Shell(unsupported().to_string()));
        }
        let Some(commands) = &self.commands else {
            return Ok(false);
        };
        let command = format!("cp -p -r -f {} {}", quoted(from), quoted(to));
        match run_if_allowed(commands, &command).await {
            None if matches!(self.files, Files::Sftp(_)) => Ok(false),
            None => Err(FsError::Shell("Cannot execute command.".to_string())),
            Some(done) => done.map(|()| true).map_err(FsError::Shell),
        }
    }

    /// The folder's items, a link to a folder taken as a folder while the session resolves links (the
    /// panel's listing).
    pub async fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, String> {
        self.list_any(path).await.map(valid_only)
    }

    async fn list_any(&self, path: &str) -> Result<Vec<RemoteEntry>, String> {
        let follow = self.options.resolve_symlinks;
        match &self.files {
            Files::Sftp(sftp) if follow => crate::sftp::list(sftp, path).await,
            Files::Sftp(sftp) => crate::sftp::read(sftp, path).await,
            Files::Scp(scp) => scp.list(path, follow).await,
            Files::Ftp(ftp) => ftp.list(path).await,
            Files::WebDav(dav) => dav.list(path).await,
            Files::S3(s3) => s3.list(path).await,
        }
    }

    /// The folder's items as they are, a link as a link (deleting, walking).
    pub async fn list_raw(&self, path: &str) -> Result<Vec<RemoteEntry>, String> {
        let listed = match &self.files {
            Files::Sftp(sftp) => crate::sftp::read(sftp, path).await,
            Files::Scp(scp) => scp.list(path, false).await,
            Files::Ftp(ftp) => ftp.list(path).await,
            Files::WebDav(dav) => dav.list(path).await,
            Files::S3(s3) => s3.list(path).await,
        };
        listed.map(valid_only)
    }
}

/// WinSCP's `TTerminal::IsValidFile`: a listing's name that is empty, "." or "..", or has a "/" (or a
/// NUL) in it is no item of the folder - whatever the server says, a name never walks out of it.
pub fn valid_name(name: &str) -> bool {
    !matches!(name, "" | "." | "..") && !name.contains(['/', '\0'])
}

fn valid_only(mut entries: Vec<RemoteEntry>) -> Vec<RemoteEntry> {
    entries.retain(|entry| valid_name(&entry.name));
    entries
}
