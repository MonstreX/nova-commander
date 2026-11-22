//! FTP's file system (WinSCP's `TFTPFileSystem`): the control connection straight or through the session's
//! proxy (passive data connections through it too), TLS explicit (`AUTH TLS`) or implicit, the login
//! (WinSCP's anonymous one when no user is given), `FEAT`'s MLSD, MFMT and UTF8; folders by MLSD, else LIST
//! read as Unix's `ls -l`; files down by RETR (REST resuming) and up by STOR or APPE; MKD, RMD, DELE,
//! RNFR / RNTO, `SITE CHMOD`, MFMT. The control connection serves one call at a time (behind a lock); a
//! file's data connection is opened at its first read or write, after a seek set where it starts. The
//! FTP(S) tab's settings (S6.6e, `ftp_connect`): MLSD's use, `LIST -a`, the passive address, HOST, the
//! account, the commands after the login.

use std::sync::Arc;
use std::sync::Mutex as Settled;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use suppaftp::Status;
use suppaftp::tokio::{
    AsyncRustlsConnector, AsyncRustlsFtpStream, AsyncRustlsStream, TransferStream,
};
use suppaftp::tokio_rustls::TlsConnector;
use suppaftp::types::FileType;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::link_options::Link;
use crate::protocol_options::{Ftp as Options, Switch};
use crate::session::Encryption;
use crate::sftp::{RemoteEntry, child};

/// WinSCP's anonymous login.
pub const ANONYMOUS_USER: &str = "anonymous";
pub const ANONYMOUS_PASSWORD: &str = "anonymous@example.com";

pub type Stream = AsyncRustlsFtpStream;
type Data = TransferStream<AsyncRustlsStream>;

/// What an FTP session connects with.
pub struct Connect<'a> {
    pub host: &'a str,
    pub port: u16,
    pub encryption: Encryption,
    pub link: &'a Link,
    /// The FTP(S) tab's settings: passive mode, its address, HOST.
    pub options: &'a Options,
    /// The TLS configuration when encrypted.
    pub tls: Option<suppaftp::tokio_rustls::rustls::ClientConfig>,
}

/// The control connection, one call at a time, with when it was used last.
pub struct Control {
    stream: Mutex<Stream>,
    used: Settled<Instant>,
    /// It no longer answered.
    lost: AtomicBool,
}

impl Control {
    fn new(stream: Stream) -> Self {
        Self {
            stream: Mutex::new(stream),
            used: Settled::new(Instant::now()),
            lost: AtomicBool::new(false),
        }
    }

    /// The connection for one call, its use noted.
    async fn lock(&self) -> tokio::sync::MutexGuard<'_, Stream> {
        let stream = self.stream.lock().await;
        if let Ok(mut used) = self.used.lock() {
            *used = Instant::now();
        }
        stream
    }

    fn idle(&self) -> Duration {
        (self.used.lock()).map_or(Duration::ZERO, |used| used.elapsed())
    }
}

/// How long a NOOP may take before the session counts as lost (WinSCP's default timeout).
const NOOP_TIMEOUT: Duration = Duration::from_secs(15);

/// An FTP session logged in.
pub struct Ftp {
    stream: Arc<Control>,
    mlsd: bool,
    mfmt: bool,
    /// The session's time zone offset in seconds (LIST's times).
    shift: i64,
    /// `LIST -a`: WinSCP's `FListAll`, Auto settled by the first listings.
    list_all: Settled<Switch>,
}

fn text(error: suppaftp::FtpError) -> String {
    match error {
        suppaftp::FtpError::UnexpectedResponse(response) => {
            let body = String::from_utf8_lossy(&response.body).trim().to_string();
            match body.is_empty() {
                true => format!("Unexpected response: {:?}", response.status),
                false => body,
            }
        }
        other => other.to_string(),
    }
}

fn connector(tls: &suppaftp::tokio_rustls::rustls::ClientConfig) -> AsyncRustlsConnector {
    AsyncRustlsConnector::from(TlsConnector::from(Arc::new(tls.clone())))
}

/// The control connection opened as `connect` says (not logged in), HOST sent when the session says so;
/// with the server's address (its name through a proxy) for the passive connections.
pub async fn open(connect: &Connect<'_>) -> Result<Stream, String> {
    let (host, port, link) = (connect.host, connect.port, connect.link);
    let tls = connect.tls.as_ref();
    let (mut stream, server) = match (connect.encryption, tls) {
        (Encryption::Implicit, Some(tls)) => {
            // Both the address's look-up and suppaftp's own connection within the session's timeout.
            let work = async {
                let address = crate::route::direct(host, port, link.family).await?;
                let address = address.peer_addr().map_err(|error| error.to_string())?;
                let stream = Stream::connect_secure_implicit(address, connector(tls), host)
                    .await
                    .map_err(text)?;
                Ok((stream, address.ip().to_string()))
            };
            crate::route::within(link, work).await?
        }
        _ => {
            let tcp = crate::route::tcp(host, port, link).await?;
            let server = match (crate::route::proxied(host, link), tcp.peer_addr()) {
                (false, Ok(address)) => address.ip().to_string(),
                _ => host.to_string(),
            };
            let stream = Stream::connect_with_stream(tcp).await.map_err(text)?;
            let stream = match (connect.encryption, tls) {
                (Encryption::Explicit, Some(tls)) => stream
                    .into_secure(connector(tls), host)
                    .await
                    .map_err(text)?,
                _ => stream,
            };
            (stream, server)
        }
    };
    let options = connect.options;
    if options.host_command == Switch::On {
        crate::ftp_connect::host_command(&mut stream, host).await;
    }
    let mode = (options.passive, options.force_pasv_ip);
    Ok(crate::ftp_connect::with_mode(stream, mode, link, server))
}

impl Ftp {
    /// The session logged in as `user` (WinSCP's anonymous one when empty) with the account the server may
    /// ask for: what the server can do read, the commands after the login sent.
    pub async fn log_in(
        mut stream: Stream,
        (user, password): (&str, &str),
        shift: i64,
        options: &Options,
    ) -> Result<Self, String> {
        let (user, password) = match user.trim() {
            "" => (ANONYMOUS_USER, ANONYMOUS_PASSWORD),
            user => (user, password),
        };
        let login = (user, password);
        (crate::ftp_connect::log_in(&mut stream, login, &options.account).await).map_err(text)?;
        let features = stream.feat().await.unwrap_or_default();
        let has = |name: &str| features.keys().any(|key| key.eq_ignore_ascii_case(name));
        if has("UTF8") {
            let _ = stream
                .custom_command(
                    "OPTS UTF8 ON",
                    &[Status::CommandOk, Status::CommandNotImplemented],
                )
                .await;
        }
        stream.transfer_type(FileType::Binary).await.map_err(text)?;
        crate::ftp_connect::post_login(&mut stream, &options.post_login_commands).await?;
        Ok(Self {
            mlsd: match options.use_mlsd {
                Switch::Auto => has("MLSD") || has("MLST"),
                switch => switch == Switch::On,
            },
            mfmt: has("MFMT"),
            stream: Arc::new(Control::new(stream)),
            shift,
            list_all: Settled::new(options.list_all),
        })
    }

    /// Whether the control connection still answers, asked by a NOOP once it was not used for `idle`:
    /// a server drops an idle one (421, or the connection closed) and the session is then opened again
    /// before the next operation (WinSCP's `QueryReopen`).
    pub async fn alive(&self, idle: Duration) -> bool {
        if self.stream.lost.load(Ordering::Relaxed) {
            return false;
        }
        if self.stream.idle() < idle {
            return true;
        }
        let mut stream = self.stream.lock().await;
        let answered = matches!(
            tokio::time::timeout(NOOP_TIMEOUT, stream.noop()).await,
            Ok(Ok(()))
        );
        // Lost once, lost for good: the session is opened anew in its place.
        self.stream.lost.store(!answered, Ordering::Relaxed);
        answered
    }

    /// The folder's items.
    pub async fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, String> {
        let mut stream = self.stream.lock().await;
        if self.mlsd {
            let lines = stream.mlsd(Some(path)).await.map_err(text)?;
            return Ok(crate::ftp_listing::mlsd(&lines));
        }
        let list_all = self.list_all.lock().map_or(Switch::Off, |switch| *switch);
        let mut empty_with_all = false;
        if list_all != Switch::Off {
            let all = format!("-a {path}");
            match stream.list(Some(&all)).await {
                Ok(lines) => {
                    let listed = self.listed(&lines);
                    if list_all == Switch::On || !listed.is_empty() {
                        self.settle(Switch::On);
                        return Ok(listed);
                    }
                    // Some servers take "-a" as a mask and list nothing: tried without it.
                    empty_with_all = true;
                }
                Err(error) if list_all == Switch::On => return Err(text(error)),
                // A server without "-a": without it from now on.
                Err(_) => {}
            }
            self.settle(Switch::Off);
        }
        let lines = stream.list(Some(path)).await.map_err(text)?;
        let listed = self.listed(&lines);
        if empty_with_all && listed.is_empty() {
            // Empty without "-a" too: not its fault, decided again next time.
            self.settle(Switch::Auto);
        }
        Ok(listed)
    }

    /// LIST's lines read, the session's time zone offset applied.
    fn listed(&self, lines: &[String]) -> Vec<RemoteEntry> {
        let mut listed = crate::ftp_listing::list(lines, crate::scp::today_utc());
        crate::scp::zone_fixed(&mut listed, self.shift);
        listed.into_iter().map(|one| one.entry).collect()
    }

    /// `LIST -a` decided (WinSCP's `FListAll`).
    fn settle(&self, switch: Switch) {
        if let Ok(mut list_all) = self.list_all.lock() {
            *list_all = switch;
        }
    }

    /// The item `path` as its folder lists it (the root as a folder).
    pub async fn entry(&self, path: &str) -> Result<RemoteEntry, String> {
        let trimmed = path.trim_end_matches('/');
        let Some((folder, name)) = trimmed.rsplit_once('/') else {
            return Err(format!("{path}: No such file"));
        };
        if name.is_empty() {
            return Ok(RemoteEntry {
                name: "/".to_string(),
                is_dir: true,
                ..RemoteEntry::default()
            });
        }
        let folder = if folder.is_empty() { "/" } else { folder };
        let listed = self.list(folder).await?;
        (listed.into_iter().find(|item| item.name == name))
            .ok_or_else(|| format!("{path}: No such file"))
    }

    /// The server's full path of `path` (CWD then PWD).
    pub async fn canonicalize(&self, path: &str) -> Result<String, String> {
        let mut stream = self.stream.lock().await;
        if !matches!(path, "" | ".") {
            stream.cwd(path).await.map_err(text)?;
        }
        stream.pwd().await.map_err(text)
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), String> {
        self.stream.lock().await.mkdir(path).await.map_err(text)
    }

    pub async fn rmdir(&self, path: &str) -> Result<(), String> {
        self.stream.lock().await.rmdir(path).await.map_err(text)
    }

    pub async fn remove(&self, path: &str) -> Result<(), String> {
        self.stream.lock().await.rm(path).await.map_err(text)
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        self.stream
            .lock()
            .await
            .rename(from, to)
            .await
            .map_err(text)
    }

    /// The rights by `SITE CHMOD`, the time by MFMT where the server has it (owners are not FTP's).
    pub async fn set_attributes(
        &self,
        path: &str,
        attributes: &russh_sftp::protocol::FileAttributes,
    ) -> Result<(), String> {
        let mut stream = self.stream.lock().await;
        if let Some(rights) = attributes.permissions {
            let command = format!("CHMOD {:o} {path}", rights & 0o7777);
            stream.site(command).await.map_err(text)?;
        }
        if let (Some(modified), true) = (attributes.mtime, self.mfmt) {
            let command = format!("MFMT {} {path}", mfmt_time(modified));
            let done = [
                Status::File,
                Status::CommandOk,
                Status::RequestedFileActionOk,
            ];
            stream.custom_command(command, &done).await.map_err(text)?;
        }
        Ok(())
    }

    /// A file of `path` to read (from `offset` when seeked) or write.
    pub fn file(&self, path: &str, way: Way) -> FtpFile {
        FtpFile {
            stream: Arc::clone(&self.stream),
            path: path.to_string(),
            way,
            offset: 0,
            data: None,
        }
    }

    /// A folder's items under it with their paths, for walking.
    pub fn child(folder: &str, name: &str) -> String {
        child(folder, name)
    }
}

/// MFMT's `YYYYMMDDHHMMSS` of `seconds` since 1970 (UTC).
pub fn mfmt_time(seconds: u32) -> String {
    let stamp = crate::scp::touch_time(seconds);
    stamp.replace('.', "")
}

/// What a file is opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    Read,
    /// Made anew (STOR).
    Write,
    /// Written at its end (APPE), resuming and appending.
    Append,
}

/// A file's transfer: its data connection opened at the first read or write.
pub struct FtpFile {
    stream: Arc<Control>,
    path: String,
    way: Way,
    offset: u64,
    data: Option<Data>,
}

fn io(error: String) -> std::io::Error {
    std::io::Error::other(error)
}

impl FtpFile {
    async fn opened(&mut self) -> std::io::Result<&mut Data> {
        if self.data.is_none() {
            let mut stream = self.stream.lock().await;
            let data = match self.way {
                Way::Read => {
                    let offset = usize::try_from(self.offset).unwrap_or(usize::MAX);
                    stream
                        .resume_transfer(offset)
                        .await
                        .map_err(|e| io(text(e)))?;
                    stream.retr_as_stream(&self.path).await
                }
                Way::Write => stream.put_with_stream(&self.path).await,
                Way::Append => stream.append_with_stream(&self.path).await,
            };
            self.data = Some(data.map_err(|e| io(text(e)))?);
        }
        self.data
            .as_mut()
            .ok_or_else(|| io("no data connection".to_string()))
    }

    pub async fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.opened().await?.read(buffer).await?;
        if read == 0
            && let Some(data) = self.data.take()
        {
            data.finish().await.map_err(|e| io(text(e)))?;
        }
        Ok(read)
    }

    pub async fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.opened().await?.write_all(bytes).await
    }

    /// Where a read starts (REST), before the transfer opens; an append starts at the end anyway.
    pub fn seek(&mut self, offset: u64) -> std::io::Result<u64> {
        match self.data {
            None => {
                self.offset = offset;
                Ok(offset)
            }
            Some(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Operation not supported.",
            )),
        }
    }

    /// The transfer ended: its data connection closed, the server's reply read.
    pub async fn shutdown(&mut self) -> std::io::Result<()> {
        if self.way != Way::Read {
            // An empty file still opens its transfer.
            self.opened().await?;
        }
        match self.data.take() {
            Some(data) => data.finish().await.map_err(|e| io(text(e))),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "ftp_tests.rs"]
mod tests;
