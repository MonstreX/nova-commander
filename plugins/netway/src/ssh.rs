//! An SSH connection as NetBox opens one (its SSH and Authentication tabs; WinSCP's `TSecureShell`), on
//! `russh` (ADR-0023 §2): the host's key checked against the plugin's own known hosts, the user asked when it
//! is new or changed (NetBox's "Host key not found in cache" / "WARNING - POTENTIAL SECURITY BREACH!"), then
//! the login by password, a private key (OpenSSH, PEM, PuTTY's `.ppk`), the agent (Pageant on Windows) or
//! the server's questions (keyboard-interactive). [`open`] checks the host and leaves the login to its
//! caller, one way after another ([`Opened`]); [`connect`] logs in by one way.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use russh::client::{self, Handle};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};

/// How the user logs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth {
    Password(String),
    /// A private key file and its passphrase when it is encrypted.
    Key {
        path: PathBuf,
        passphrase: Option<String>,
    },
    /// The keys of the user's agent (Pageant on Windows, `SSH_AUTH_SOCK` elsewhere).
    Agent,
}

/// Where and as whom to connect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: Auth,
}

/// What the known hosts say of the host's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Known {
    /// The key is the one kept for the host.
    Yes,
    /// No key of its kind is kept for the host.
    New,
    /// Another key of its kind is kept for the host.
    Changed,
}

/// The host's key as the user is asked about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKey {
    pub host: String,
    pub port: u16,
    /// `ssh-ed25519`, `rsa-sha2-512` and so on.
    pub algorithm: String,
    /// `SHA256:...`, as NetBox and OpenSSH show it.
    pub fingerprint: String,
    pub known: Known,
}

/// The user's answer about a new or changed key (NetBox's Yes / No / Cancel: kept, this time only, refused).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Keep,
    Once,
    Refuse,
}

/// Who answers about a new or changed key: awaited on the runtime's thread (the plugin's thread shows the
/// question meanwhile).
pub type Trust = Arc<dyn Fn(HostKey) -> Pin<Box<dyn Future<Output = Answer> + Send>> + Send + Sync>;

/// A trust answering at once as `answer` says (a fixed policy, the tests').
pub fn trust_by(answer: impl Fn(&HostKey) -> Answer + Send + Sync + 'static) -> Trust {
    let answer = Arc::new(answer);
    Arc::new(move |key: HostKey| {
        let answer = Arc::clone(&answer);
        Box::pin(async move { answer(&key) })
    })
}

/// The server's questions of a keyboard-interactive login (WinSCP's prompts): its name, its
/// instructions and each prompt with whether what is typed is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Questions {
    pub name: String,
    pub instructions: String,
    pub prompts: Vec<(String, bool)>,
}

/// Why a connection failed.
#[derive(Debug)]
pub enum Failure {
    /// The host could not be reached or the connection broke.
    Connection(String),
    /// The user refused the host's key.
    HostKey(HostKey),
    /// The server refused the login.
    Refused,
    /// The private key could not be read (its file, its format or its passphrase).
    Key(String),
    /// The agent could not be reached or has no key the server takes.
    Agent(String),
    /// The user gave up a question of the login (Esc).
    Cancelled,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Cancelled"),
            Self::Connection(text) => write!(formatter, "Network error: {text}"),
            Self::HostKey(key) => write!(formatter, "Host key {} refused", key.fingerprint),
            Self::Refused => formatter.write_str("Access denied"),
            Self::Key(text) => write!(formatter, "Cannot read the private key: {text}"),
            Self::Agent(text) => write!(formatter, "Authentication agent: {text}"),
        }
    }
}

impl std::error::Error for Failure {}

/// The client's side of the connection: the host key's check.
pub(crate) struct Client {
    host: String,
    port: u16,
    known_hosts: PathBuf,
    trust: Trust,
    refused: Arc<Mutex<Option<HostKey>>>,
    /// The server's key's SHA256 fingerprint, as checked.
    seen: Arc<Mutex<Option<String>>>,
}

impl Client {
    fn known(&self, key: &PublicKey) -> Known {
        match russh::keys::check_known_hosts_path(&self.host, self.port, key, &self.known_hosts) {
            Ok(true) => Known::Yes,
            Ok(false) => Known::New,
            Err(_) => Known::Changed,
        }
    }
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = server_key.public_key();
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(key.fingerprint(HashAlg::Sha256).to_string());
        }
        let known = self.known(&key);
        if known == Known::Yes {
            return Ok(true);
        }
        let host_key = HostKey {
            host: self.host.clone(),
            port: self.port,
            algorithm: key.algorithm().to_string(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
            known,
        };
        match (self.trust)(host_key.clone()).await {
            Answer::Keep => {
                // A changed key replaces the one kept (any other of its kind would stay "changed"); a
                // key kept that cannot be written is still trusted this time.
                if known == Known::Changed {
                    let _ = forget_kind(&self.host, self.port, &key, &self.known_hosts);
                }
                let _ = russh::keys::known_hosts::learn_known_hosts_path(
                    &self.host,
                    self.port,
                    &key,
                    &self.known_hosts,
                );
                Ok(true)
            }
            Answer::Once => Ok(true),
            Answer::Refuse => {
                if let Ok(mut refused) = self.refused.lock() {
                    *refused = Some(host_key);
                }
                Ok(false)
            }
        }
    }
}

/// WinSCP's `MSG_SFTP_INITIALIZE_ERROR`.
pub const SFTP_REFUSED: &str =
    "Cannot initialize SFTP protocol. Is the host running an SFTP server?";

/// A connection logged in.
pub struct Connection {
    handle: Arc<Handle<Client>>,
    /// The tunnel's connection the session runs through, kept while it runs.
    via: Option<Box<Connection>>,
    /// The server's host key's SHA256 fingerprint ("Server and protocol information").
    pub fingerprint: Option<String>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Connection")
    }
}

/// The lines of `known_hosts` keeping a key of `key`'s kind for the host removed (counted as `russh`
/// counts them: comments not).
fn forget_kind(host: &str, port: u16, key: &PublicKey, known_hosts: &Path) -> std::io::Result<()> {
    let kept = russh::keys::known_hosts::known_host_keys_path(host, port, known_hosts)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let lines: Vec<usize> = (kept.into_iter())
        .filter(|(_, recorded)| recorded.algorithm() == key.algorithm())
        .map(|(line, _)| line)
        .collect();
    if lines.is_empty() {
        return Ok(());
    }
    let text = std::fs::read_to_string(known_hosts)?;
    let mut counted = 0;
    let left: String = (text.split_inclusive('\n'))
        .filter(|line| {
            if line.starts_with('#') {
                return true;
            }
            counted += 1;
            !lines.contains(&counted)
        })
        .collect();
    std::fs::write(known_hosts, left)
}

/// A connection whose host is checked, not logged in yet: each way tried in turn, then
/// [`Opened::logged_in`].
pub struct Opened {
    handle: Handle<Client>,
    seen: Arc<Mutex<Option<String>>>,
}

impl std::fmt::Debug for Opened {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Opened")
    }
}

/// The connection to `host`'s `port`, its key checked against `known_hosts` (`trust` asked when it is new
/// or changed).
pub async fn open(
    host: &str,
    port: u16,
    known_hosts: &Path,
    trust: Trust,
) -> Result<Opened, Failure> {
    let refused = Arc::new(Mutex::new(None));
    let seen = Arc::new(Mutex::new(None));
    let client = Client {
        host: host.to_string(),
        port,
        known_hosts: known_hosts.to_path_buf(),
        trust,
        refused: Arc::clone(&refused),
        seen: Arc::clone(&seen),
    };
    let config = Arc::new(client::Config::default());
    match client::connect(config, (host, port), client).await {
        Ok(handle) => Ok(Opened { handle, seen }),
        Err(error) => {
            let refused = refused.lock().ok().and_then(|mut key| key.take());
            Err(refused.map_or_else(|| Failure::Connection(error.to_string()), Failure::HostKey))
        }
    }
}

/// The SSH connection to `host`'s `port` over `wire` (the session's way: straight, a proxy, a tunnel),
/// keepalives sent as the session's Connection tab says (null packets and dummy commands both as
/// OpenSSH's keepalive requests).
pub async fn open_over(
    wire: crate::route::Stream,
    (host, port): (&str, u16),
    known_hosts: &Path,
    trust: Trust,
    link: &crate::link_options::Link,
    ssh: &crate::ssh_options::Ssh,
) -> Result<Opened, Failure> {
    let refused = Arc::new(Mutex::new(None));
    let seen = Arc::new(Mutex::new(None));
    let client = Client {
        host: host.to_string(),
        port,
        known_hosts: known_hosts.to_path_buf(),
        trust,
        refused: Arc::clone(&refused),
        seen: Arc::clone(&seen),
    };
    let keepalive = (link.ping != crate::link_options::Ping::Off && link.ping_interval > 0)
        .then(|| std::time::Duration::from_secs(link.ping_interval.into()));
    let config = Arc::new(client::Config {
        keepalive_interval: keepalive,
        preferred: ssh.preferred(),
        limits: ssh.limits(),
        ..client::Config::default()
    });
    match client::connect_stream(config, wire, client).await {
        Ok(handle) => Ok(Opened { handle, seen }),
        Err(error) => {
            let refused = refused.lock().ok().and_then(|mut key| key.take());
            Err(refused.map_or_else(|| Failure::Connection(error.to_string()), Failure::HostKey))
        }
    }
}

/// The connection to `login`'s host, its key checked against `known_hosts` (`trust` asked when it is new or
/// changed), logged in by `login`'s way.
pub async fn connect(
    login: &Login,
    known_hosts: &Path,
    trust: Trust,
) -> Result<Connection, Failure> {
    let mut opened = open(&login.host, login.port, known_hosts, trust).await?;
    let user = login.user.as_str();
    let taken = match &login.auth {
        Auth::Password(password) => opened.password(user, password).await?,
        Auth::Key { path, passphrase } => opened.key(user, path, passphrase.as_deref()).await?,
        Auth::Agent => opened.agent(user).await?,
    };
    if !taken {
        return Err(Failure::Refused);
    }
    Ok(opened.logged_in())
}

fn lost(error: russh::Error) -> Failure {
    Failure::Connection(error.to_string())
}

impl Opened {
    /// The login by `password`: whether the server took it.
    /// WinSCP's "Bypass authentication entirely": the `none` method; whether the server took it.
    pub async fn none(&mut self, user: &str) -> Result<bool, Failure> {
        let answer = (self.handle.authenticate_none(user).await).map_err(lost)?;
        Ok(answer.success())
    }

    pub async fn password(&mut self, user: &str, password: &str) -> Result<bool, Failure> {
        let answer = (self.handle.authenticate_password(user, password).await).map_err(lost)?;
        Ok(answer.success())
    }

    /// The login by the private key at `path` (its `passphrase` when it is encrypted): whether the server
    /// took it.
    pub async fn key(
        &mut self,
        user: &str,
        path: &Path,
        passphrase: Option<&str>,
    ) -> Result<bool, Failure> {
        let key = russh::keys::load_secret_key(path, passphrase)
            .map_err(|error| Failure::Key(error.to_string()))?;
        let hash = (self.handle.best_supported_rsa_hash().await)
            .map_err(lost)?
            .flatten();
        let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
        let answer = (self.handle.authenticate_publickey(user, key).await).map_err(lost)?;
        Ok(answer.success())
    }

    /// The login by each of the agent's keys in turn until the server takes one.
    pub async fn agent(&mut self, user: &str) -> Result<bool, Failure> {
        agent_log_in(&mut self.handle, user).await
    }

    /// The keyboard-interactive login: each round of the server's questions answered by `answer` (none:
    /// the user gave up); whether the server took the answers.
    pub async fn keyboard_interactive(
        &mut self,
        user: &str,
        mut answer: impl AsyncFnMut(&Questions) -> Option<Vec<String>>,
    ) -> Result<bool, Failure> {
        use russh::client::KeyboardInteractiveAuthResponse as Response;
        let mut response = (self.handle)
            .authenticate_keyboard_interactive_start(user, None)
            .await
            .map_err(lost)?;
        loop {
            let questions = match response {
                Response::Success => return Ok(true),
                Response::Failure { .. } => return Ok(false),
                Response::InfoRequest {
                    name,
                    instructions,
                    prompts,
                } => Questions {
                    name,
                    instructions,
                    prompts: (prompts.into_iter())
                        .map(|one| (one.prompt, one.echo))
                        .collect(),
                },
            };
            let Some(answers) = answer(&questions).await else {
                return Ok(false);
            };
            response = (self.handle)
                .authenticate_keyboard_interactive_respond(answers)
                .await
                .map_err(lost)?;
        }
    }

    /// The connection logged in.
    pub fn logged_in(self) -> Connection {
        let fingerprint = self.seen.lock().ok().and_then(|seen| seen.clone());
        Connection {
            handle: Arc::new(self.handle),
            via: None,
            fingerprint,
        }
    }
}

/// The login by each of the agent's keys in turn until the server takes one.
async fn agent_log_in(handle: &mut Handle<Client>, user: &str) -> Result<bool, Failure> {
    let failed = |error: russh::keys::Error| Failure::Agent(error.to_string());
    #[cfg(windows)]
    let mut agent = russh::keys::agent::client::AgentClient::connect_pageant()
        .await
        .map_err(failed)?;
    #[cfg(not(windows))]
    let mut agent = russh::keys::agent::client::AgentClient::connect_env()
        .await
        .map_err(failed)?;
    let identities = agent.request_identities().await.map_err(failed)?;
    let hash = handle
        .best_supported_rsa_hash()
        .await
        .map_err(|error| Failure::Connection(error.to_string()))?
        .flatten();
    for identity in identities {
        let key = identity.public_key().into_owned();
        let answer = handle
            .authenticate_publickey_with(user, key, hash, &mut agent)
            .await
            .map_err(|error| Failure::Agent(error.to_string()))?;
        if answer.success() {
            return Ok(true);
        }
    }
    Ok(false)
}

impl Connection {
    /// The SFTP subsystem over a new channel of the connection.
    /// SFTP by the `sftp` subsystem, or by `server`'s command when the session names one (its SFTP tab).
    pub async fn sftp(&self, server: &str) -> Result<russh_sftp::client::SftpSession, Failure> {
        let lost = |error: &dyn std::fmt::Display| Failure::Connection(error.to_string());
        let channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| lost(&e))?;
        let requested = match server.trim() {
            "" => channel.request_subsystem(true, "sftp").await,
            command => channel.exec(true, command).await,
        };
        requested.map_err(|e| lost(&e))?;
        let mut channel = channel;
        if !crate::shell::granted(&mut channel).await {
            return Err(Failure::Connection(SFTP_REFUSED.to_string()));
        }
        russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| lost(&e))
    }

    /// The shell's commands over the connection (SCP's; SFTP's copy on the server).
    pub fn commands(&self) -> crate::shell::Commands {
        crate::shell::Commands::new(Arc::clone(&self.handle))
    }

    /// The connection is lost (or closed): its session must be opened again.
    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    /// The connection closed, the server told.
    pub async fn close(&self) {
        let _ = self
            .handle
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await;
        if let Some(via) = &self.via {
            Box::pin(via.close()).await;
        }
    }

    /// The connection run through `tunnel` (kept as long as it is).
    pub fn through(mut self, tunnel: Option<Connection>) -> Self {
        self.via = tunnel.map(Box::new);
        self
    }

    /// A way to `host`'s `port` through this connection (WinSCP's tunnel: a direct-tcpip channel).
    pub async fn forward(&self, host: &str, port: u16) -> Result<crate::route::Stream, Failure> {
        let channel = (self.handle)
            .channel_open_direct_tcpip(host, u32::from(port), "127.0.0.1", 0)
            .await
            .map_err(|error| Failure::Connection(error.to_string()))?;
        Ok(Box::new(channel.into_stream()))
    }
}

#[cfg(test)]
#[path = "ssh_tests.rs"]
mod tests;
