//! The tests' SSH server inside their own process (ADR-0023 §4): `russh`'s server side with `russh-sftp`'s,
//! a temporary folder as its root, the user `user` by the password `pass` or by the keys of
//! `testdata/user_ed25519` (also as `.ppk`) and `testdata/user_ed25519_encrypted`, the host key
//! `testdata/server_ed25519`.

use std::collections::HashMap;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use russh::server::{Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, SftpHandle, Status, StatusCode,
    Version,
};
use tokio::sync::Mutex;

pub const USER: &str = "user";
pub const PASSWORD: &str = "pass";
/// A user the server asks its questions (keyboard-interactive) and refuses a password to.
pub const QUESTIONED_USER: &str = "asked";
/// A user whose session has no SFTP (its subsystem refused): only the shell's commands, SCP's.
pub const SHELL_USER: &str = "shell";
/// The server's prompt to `QUESTIONED_USER`, answered by `PASSWORD`.
pub const QUESTION: &str = "Password: ";
pub const SERVER_KEY: &str = include_str!("../testdata/server_ed25519");
pub const USER_PUBLIC_KEY: &str = include_str!("../testdata/user_ed25519.pub");
pub const USER_ENCRYPTED_PUBLIC_KEY: &str = include_str!("../testdata/user_ed25519_encrypted.pub");

/// The server running on its own runtime, at a port of the loopback.
pub struct TestServer {
    pub port: u16,
    _runtime: tokio::runtime::Runtime,
}

impl TestServer {
    /// A server whose SFTP root is `root` (the tests').
    #[cfg(test)]
    pub fn start(root: &Path) -> Self {
        Self::try_start(root).unwrap()
    }

    /// A server over `root` at the loopback's `port` (a server gone and back, the tests').
    #[cfg(test)]
    pub fn start_at(root: &Path, port: u16) -> Self {
        Self::try_start_at(root, port).unwrap()
    }

    /// A server whose SFTP root is `root`, at a free port of the loopback.
    pub fn try_start(root: &Path) -> std::io::Result<Self> {
        Self::try_start_at(root, 0)
    }

    /// A server whose SFTP root is `root` at the loopback's `port` (0: a free one).
    pub fn try_start_at(root: &Path, port: u16) -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let address = format!("127.0.0.1:{port}");
        let listener = runtime.block_on(tokio::net::TcpListener::bind(address))?;
        let port = listener.local_addr()?.port();
        let key = russh::keys::decode_secret_key(SERVER_KEY, None)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let config = Arc::new(russh::server::Config {
            keys: vec![key],
            auth_rejection_time: std::time::Duration::from_millis(10),
            auth_rejection_time_initial: Some(std::time::Duration::ZERO),
            ..Default::default()
        });
        let mut server = Server {
            root: root.to_path_buf(),
        };
        runtime.spawn(async move { server.run_on_socket(config, &listener).await });
        Ok(Self {
            port,
            _runtime: runtime,
        })
    }

    /// The server's host key's fingerprint, as the client reports it (the tests').
    #[cfg(test)]
    pub fn fingerprint() -> String {
        Self::try_fingerprint().unwrap()
    }

    /// The server's host key's fingerprint, as the client reports it.
    pub fn try_fingerprint() -> Result<String, String> {
        let key =
            russh::keys::decode_secret_key(SERVER_KEY, None).map_err(|error| error.to_string())?;
        Ok(key
            .public_key()
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string())
    }
}

#[derive(Clone)]
struct Server {
    root: PathBuf,
}

impl russh::server::Server for Server {
    type Handler = Connection;

    fn new_client(&mut self, _: Option<SocketAddr>) -> Connection {
        Connection {
            root: self.root.clone(),
            channels: Arc::new(Mutex::new(HashMap::new())),
            user: String::new(),
        }
    }
}

struct Connection {
    root: PathBuf,
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
    /// The user logged in.
    user: String,
}

fn rejected() -> Auth {
    Auth::Reject {
        proceed_with_methods: None,
        partial_success: false,
    }
}

impl russh::server::Handler for Connection {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if !matches!(user, USER | SHELL_USER) || password != PASSWORD {
            return Ok(rejected());
        }
        self.user = user.to_string();
        Ok(Auth::Accept)
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        user: &str,
        _submethods: &str,
        response: Option<russh::server::Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        if user != QUESTIONED_USER {
            return Ok(rejected());
        }
        let Some(mut response) = response else {
            return Ok(Auth::Partial {
                name: "".into(),
                instructions: "".into(),
                prompts: vec![(QUESTION.into(), false)].into(),
            });
        };
        let answered = response
            .next()
            .is_some_and(|text| text.as_ref() == PASSWORD.as_bytes());
        Ok(if answered { Auth::Accept } else { rejected() })
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        key: russh::server::VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        let known: Vec<_> = [USER_PUBLIC_KEY, USER_ENCRYPTED_PUBLIC_KEY]
            .iter()
            .filter_map(|line| {
                let base64 = line.split_whitespace().nth(1)?;
                russh::keys::parse_public_key_base64(base64).ok()
            })
            .collect();
        Ok(match user == USER && known.contains(key.public_key) {
            true => Auth::Accept,
            false => rejected(),
        })
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.lock().await.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    /// A tunnel's channel (direct-tcpip): joined to a TCP connection to its target.
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let target = format!("{host_to_connect}:{port_to_connect}");
        let Ok(mut tcp) = tokio::net::TcpStream::connect(target).await else {
            reply.reject(russh::ChannelOpenFailure::ConnectFailed).await;
            return Ok(());
        };
        reply.accept().await;
        tokio::spawn(async move {
            let mut stream = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
        });
        Ok(())
    }

    /// A command run on its channel by the tests' shell (`test_shell`).
    async fn exec_request(
        &mut self,
        id: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let Some(channel) = self.channels.lock().await.remove(&id) else {
            return session.channel_failure(id);
        };
        session.channel_success(id)?;
        let command = String::from_utf8_lossy(data).into_owned();
        // An SFTP server run as a command (a session's "SFTP server").
        if command.trim_end().ends_with("sftp-server") {
            let sftp = Sftp {
                root: self.root.clone(),
                listings: HashMap::new(),
                files: HashMap::new(),
                opened: 0,
            };
            tokio::spawn(russh_sftp::server::run(channel.into_stream(), sftp));
            return Ok(());
        }
        tokio::spawn(crate::test_shell::serve(
            self.root.clone(),
            command,
            channel,
        ));
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let channel = self.channels.lock().await.remove(&id);
        match (name, channel) {
            ("sftp", Some(channel)) if self.user != SHELL_USER => {
                session.channel_success(id)?;
                let sftp = Sftp {
                    root: self.root.clone(),
                    listings: HashMap::new(),
                    files: HashMap::new(),
                    opened: 0,
                };
                russh_sftp::server::run(channel.into_stream(), sftp).await;
            }
            _ => session.channel_failure(id)?,
        }
        Ok(())
    }
}

/// The SFTP side: a folder's listing, a path's attributes, files read and written, folders made, items
/// removed.
struct Sftp {
    root: PathBuf,
    /// Each open folder's items not yet sent.
    listings: HashMap<String, Option<Vec<File>>>,
    /// The open files by their handles.
    files: HashMap<String, std::fs::File>,
    opened: u32,
}

impl Sftp {
    fn local(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }

    fn file(&mut self, handle: &str) -> Result<&mut std::fs::File, StatusCode> {
        self.files.get_mut(handle).ok_or(StatusCode::Failure)
    }
}

/// A disk's failure as SFTP's status.
fn status(error: &std::io::Error) -> StatusCode {
    match error.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    }
}

/// The Unix rights set on the server's paths (its disk keeps none on Windows), by their disk's paths.
static RIGHTS: std::sync::Mutex<Option<HashMap<PathBuf, u32>>> = std::sync::Mutex::new(None);

/// The rights set on the server's file `path` of its disk, if any were.
pub fn rights_of(path: &Path) -> Option<u32> {
    let kept = RIGHTS.lock().ok()?;
    kept.as_ref()?.get(path).copied()
}

/// Rights set on `path`: refused for a name with "locked" in it (a permission error), else kept.
fn set_rights(path: &Path, attrs: &FileAttributes) -> Result<(), StatusCode> {
    let Some(rights) = attrs.permissions else {
        return Ok(());
    };
    match keep_rights(path, rights) {
        true => Ok(()),
        false => Err(StatusCode::PermissionDenied),
    }
}

/// `rights` kept for `path`; false for a name with "locked" in it (refused).
pub(crate) fn keep_rights(path: &Path, rights: u32) -> bool {
    if path.to_string_lossy().contains("locked") {
        return false;
    }
    if let Ok(mut kept) = RIGHTS.lock() {
        kept.get_or_insert_with(HashMap::new)
            .insert(path.to_path_buf(), rights & 0o7777);
    }
    true
}

/// A path's attributes as the client gets them, the rights set on it in theirs.
/// An item's own attributes, a link (a junction here) marked as one (`S_IFLNK`), as a POSIX server lists
/// it.
fn own_attributes(meta: &std::fs::Metadata) -> FileAttributes {
    let mut attrs = FileAttributes::from(meta);
    if meta.file_type().is_symlink() {
        attrs.permissions = Some(0o120_777);
    }
    attrs
}

pub(crate) fn attributes(path: &Path) -> Result<FileAttributes, StatusCode> {
    let meta = std::fs::metadata(path).map_err(|error| status(&error))?;
    let mut attrs = FileAttributes::from(&meta);
    if let (Some(rights), Some(permissions)) = (rights_of(path), attrs.permissions) {
        attrs.permissions = Some((permissions & !0o7777) | rights);
    }
    Ok(attrs)
}

/// The times of `attrs` set on the file `path` (a folder's are left).
pub(crate) fn set_times(path: &Path, attrs: &FileAttributes) -> Result<(), StatusCode> {
    let Some(mtime) = attrs.mtime else {
        return Ok(());
    };
    if path.is_dir() {
        return Ok(());
    }
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime.into());
    let accessed = attrs.atime.map_or(time, |atime| {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(atime.into())
    });
    let file = std::fs::OpenOptions::new().write(true).open(path);
    let times = std::fs::FileTimes::new()
        .set_modified(time)
        .set_accessed(accessed);
    (file.and_then(|file| file.set_times(times))).map_err(|error| status(&error))
}

/// A handle as the server's own key: the text it gave the handle as (russh-sftp 4's handles are bytes).
fn key(handle: &SftpHandle) -> String {
    String::from_utf8_lossy(handle.as_bytes()).into_owned()
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_string(),
        language_tag: "en-US".to_string(),
    }
}

impl russh_sftp::server::Handler for Sftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let path = match path.as_str() {
            "" | "." => "/".to_string(),
            other => other.to_string(),
        };
        Ok(Name {
            id,
            files: vec![File::dummy(path)],
        })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let read = std::fs::read_dir(self.local(&path)).map_err(|_| StatusCode::NoSuchFile)?;
        let files = read
            .flatten()
            .filter_map(|item| {
                let meta = item.metadata().ok()?;
                let name = item.file_name().to_string_lossy().into_owned();
                Some(File::new(name, own_attributes(&meta)))
            })
            .collect();
        self.listings.insert(path.clone(), Some(files));
        Ok(Handle {
            id,
            handle: path.into(),
        })
    }

    async fn readdir(&mut self, id: u32, handle: SftpHandle) -> Result<Name, Self::Error> {
        let handle = key(&handle);
        match self.listings.get_mut(&handle).and_then(Option::take) {
            Some(files) => Ok(Name { id, files }),
            None => Err(StatusCode::Eof),
        }
    }

    async fn close(&mut self, id: u32, handle: SftpHandle) -> Result<Status, Self::Error> {
        let handle = key(&handle);
        self.listings.remove(&handle);
        self.files.remove(&handle);
        Ok(ok(id))
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let file = std::fs::OpenOptions::new()
            .read(pflags.contains(OpenFlags::READ))
            .write(pflags.contains(OpenFlags::WRITE))
            .create(pflags.contains(OpenFlags::CREATE))
            .truncate(pflags.contains(OpenFlags::TRUNCATE))
            .open(self.local(&filename))
            .map_err(|error| status(&error))?;
        self.opened += 1;
        let handle = format!("file-{}", self.opened);
        self.files.insert(handle.clone(), file);
        Ok(Handle {
            id,
            handle: handle.into(),
        })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: SftpHandle,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let file = self.file(&key(&handle))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| status(&error))?;
        let mut data = vec![0; usize::try_from(len).unwrap_or(0)];
        let read = file.read(&mut data).map_err(|error| status(&error))?;
        if read == 0 {
            return Err(StatusCode::Eof);
        }
        data.truncate(read);
        Ok(Data { id, data })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: SftpHandle,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let file = self.file(&key(&handle))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| status(&error))?;
        file.write_all(&data).map_err(|error| status(&error))?;
        Ok(ok(id))
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let attrs = attributes(&self.local(&path))?;
        Ok(Attrs { id, attrs })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        // A link (a junction here) as the link it is, as a POSIX server's `lstat`.
        let own = std::fs::symlink_metadata(self.local(&path));
        match own {
            Ok(meta) if meta.file_type().is_symlink() => Ok(Attrs {
                id,
                attrs: own_attributes(&meta),
            }),
            _ => self.stat(id, path).await,
        }
    }

    async fn fstat(&mut self, id: u32, handle: SftpHandle) -> Result<Attrs, Self::Error> {
        let meta = (self.file(&key(&handle))?.metadata()).map_err(|error| status(&error))?;
        let attrs = FileAttributes::from(&meta);
        Ok(Attrs { id, attrs })
    }

    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        set_rights(&self.local(&path), &attrs)?;
        set_times(&self.local(&path), &attrs)?;
        Ok(ok(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        std::fs::create_dir(self.local(&path)).map_err(|error| status(&error))?;
        Ok(ok(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        let local = self.local(&filename);
        // A link to a folder (a junction here) goes as a link, as a POSIX server's `unlink`.
        let folder_link = std::fs::symlink_metadata(&local)
            .is_ok_and(|meta| meta.file_type().is_symlink() && local.is_dir());
        let removed = match folder_link {
            true => std::fs::remove_dir(&local),
            false => std::fs::remove_file(&local),
        };
        removed.map_err(|error| status(&error))?;
        Ok(ok(id))
    }

    /// SFTP 3's rename: never over an existing item.
    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        let to = self.local(&newpath);
        if to.exists() {
            return Err(StatusCode::Failure);
        }
        std::fs::rename(self.local(&oldpath), to).map_err(|error| status(&error))?;
        Ok(ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        std::fs::remove_dir(self.local(&path)).map_err(|error| status(&error))?;
        Ok(ok(id))
    }
}
