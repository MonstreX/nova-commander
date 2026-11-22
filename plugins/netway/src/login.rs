//! A session started as WinSCP starts one (`TSecureShell::Open`, its statuses and prompts): "Connecting to
//! host..." with the host key's question, "Authenticating..." by the private key (its passphrase asked
//! when it is encrypted) or else the agent, then the password - the session's, else asked - with the
//! server's questions (keyboard-interactive) after it, three times at most; "Starting the session..." with
//! SFTP and the folder the panel opens at (the user's home folder when none is given).

use std::path::{Path, PathBuf};

use crate::fs::Fs;
use crate::session::Protocol;

use crate::ask::{Asker, passphrase_prompt};
use crate::route;
use crate::ssh::{self, Connection, Failure, Opened, Questions};

/// WinSCP's `MSG_STATUS_CONNECT`, `MSG_STATUS_AUTHENTICATE`, `MSG_STATUS_STARTUP`.
pub const CONNECTING: &str = "Connecting to host...";
pub const AUTHENTICATING: &str = "Authenticating...";
pub const STARTING: &str = "Starting the session...";
/// WinSCP's `MSG_OPEN_TUNNEL`.
pub const OPENING_TUNNEL: &str = "Opening tunnel...";
/// WinSCP's `MSG_USERNAME_PROMPT2` and `MSG_PASSWORD_PROMPT`.
pub const USER_PROMPT: &str = "&Username:";
pub const PASSWORD_PROMPT: &str = "&Password:";

/// How many passwords are tried before the login is refused (WinSCP asks again after "Access denied").
const TRIES: usize = 3;

/// What a session starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub host: String,
    pub port: u16,
    /// Empty: asked.
    pub user: String,
    /// The session's password, empty: none kept.
    pub password: String,
    /// The private key's file, none: the agent.
    pub key: Option<PathBuf>,
    /// The plugin's known hosts.
    pub known_hosts: PathBuf,
    /// The folder to open at, empty: the home folder; relative: under the home folder.
    pub folder: String,
    /// SFTP, SCP or FTP.
    pub protocol: Protocol,
    /// FTP's TLS: none, explicit or implicit.
    pub encryption: crate::session::Encryption,
    /// SFTP's "Allow SCP fallback": SCP when the server has no SFTP.
    pub scp_fallback: bool,
    /// The session's other tabs' settings.
    pub options: crate::session_options::Options,
}

/// The session's file system: SCP's for an SCP session or when SFTP fails and the session falls back
/// (WinSCP's `fsSFTP`), else SFTP's.
async fn file_system(connection: &Connection, plan: &Plan) -> Result<Fs, Failure> {
    let commands = connection.commands().with_shell(&plan.options.shell);
    if plan.protocol == Protocol::Scp {
        return Ok(Fs::scp(commands, &plan.options));
    }
    match connection.sftp(&plan.options.sftp_server).await {
        Ok(sftp) => Ok(Fs::sftp(sftp, Some(commands), &plan.options)),
        Err(_) if plan.scp_fallback => Ok(Fs::scp(commands, &plan.options)),
        Err(failure) => Err(failure),
    }
}

/// A session started: its connection, its file system (SFTP's or SCP's) and the folder it opens at.
pub struct Started {
    /// The SSH connection (none for FTP).
    pub connection: Option<Connection>,
    pub fs: Fs,
    pub user: String,
    /// The password that logged in (empty for a key or the agent), for a reconnection.
    pub password: String,
    pub folder: String,
}

impl std::fmt::Debug for Started {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&format!("Started({})", self.folder))
    }
}

/// The session `plan` started, `asker` asked and told along the way.
pub async fn start(plan: &Plan, asker: &Asker) -> Result<Started, Failure> {
    match plan.protocol {
        Protocol::Ftp => return crate::ftp_login::start(plan, asker).await,
        Protocol::WebDav => return crate::webdav_login::start(plan, asker).await,
        Protocol::S3 => return crate::s3_login::start(plan, asker).await,
        Protocol::Sftp | Protocol::Scp => {}
    }
    let tunnel = match plan.options.link.tunnel {
        true => Some(open_tunnel(plan, asker).await?),
        false => None,
    };
    asker.tell(CONNECTING);
    let wire = match &tunnel {
        Some(tunnel) => tunnel.forward(&plan.host, plan.port).await?,
        None => (route::stream(&plan.host, plan.port, &plan.options.link).await)
            .map_err(Failure::Connection)?,
    };
    let (connection, user, password) = logged_in(wire, plan, asker).await?;
    let connection = connection.through(tunnel);
    asker.tell(STARTING);
    let fs = file_system(&connection, plan).await?;
    let folder = opening_folder(&fs, &plan.folder)
        .await
        .map_err(Failure::Connection)?;
    Ok(Started {
        connection: Some(connection),
        fs,
        user,
        password,
        folder,
    })
}

/// The SSH connection over `wire` logged into as `plan` says: it, its user and the password that logged
/// in (empty for a key or the agent).
async fn logged_in(
    wire: route::Stream,
    plan: &Plan,
    asker: &Asker,
) -> Result<(Connection, String, String), Failure> {
    let host = (plan.host.as_str(), plan.port);
    let link = &plan.options.link;
    let ssh = &plan.options.ssh;
    let mut opened =
        ssh::open_over(wire, host, &plan.known_hosts, asker.trust(), link, ssh).await?;
    asker.tell(AUTHENTICATING);
    let user = match plan.user.trim() {
        "" => asker
            .text(USER_PROMPT, false)
            .await
            .ok_or(Failure::Cancelled)?,
        user => user.to_string(),
    };
    let Some(password) = log_in(&mut opened, plan, &user, asker).await? else {
        return Err(Failure::Refused);
    };
    Ok((opened.logged_in(), user, password))
}

/// The session's tunnel (its Tunnel tab, WinSCP's "Opening tunnel..."): the tunnel's host logged into by
/// its own user, password or key, through the session's proxy.
async fn open_tunnel(plan: &Plan, asker: &Asker) -> Result<Connection, Failure> {
    asker.tell(OPENING_TUNNEL);
    let link = &plan.options.link;
    let mut options = plan.options.clone();
    options.link.tunnel = false;
    let key = link.tunnel_key.trim();
    let tunnel = Plan {
        host: link.tunnel_host.trim().to_string(),
        port: link.tunnel_port,
        user: link.tunnel_user.clone(),
        password: link.tunnel_password.clone(),
        key: (!key.is_empty()).then(|| PathBuf::from(key)),
        known_hosts: plan.known_hosts.clone(),
        folder: String::new(),
        protocol: Protocol::Sftp,
        encryption: crate::session::Encryption::None,
        scp_fallback: false,
        options,
    };
    let wire = (route::stream(&tunnel.host, tunnel.port, &tunnel.options.link).await)
        .map_err(Failure::Connection)?;
    Ok(logged_in(wire, &tunnel, asker).await?.0)
}

/// The ways tried in turn: the key or the agent, then passwords with the server's questions. The
/// password that logged in (empty for a key or the agent); none when nothing did.
async fn log_in(
    opened: &mut Opened,
    plan: &Plan,
    user: &str,
    asker: &Asker,
) -> Result<Option<String>, Failure> {
    let ssh = &plan.options.ssh;
    if ssh.no_user_auth && opened.none(user).await? {
        return Ok(Some(String::new()));
    }
    let by_key = match &plan.key {
        Some(key) => by_key(opened, user, key, asker).await?,
        // No agent, or none of its keys: the password comes next, as WinSCP's.
        None if ssh.try_agent => opened.agent(user).await.unwrap_or(false),
        None => false,
    };
    if by_key {
        return Ok(Some(String::new()));
    }
    let mut kept = (!plan.password.is_empty()).then(|| plan.password.clone());
    for _ in 0..TRIES {
        let password = match kept.take() {
            Some(password) => password,
            None => (asker.text(PASSWORD_PROMPT, true).await).ok_or(Failure::Cancelled)?,
        };
        if opened.password(user, &password).await? {
            return Ok(Some(password));
        }
        if !ssh.keyboard_interactive {
            continue;
        }
        // "Respond with password to the first prompt".
        let mut given = ssh.ki_password.then(|| password.clone());
        let answered = opened
            .keyboard_interactive(user, async |questions: &Questions| {
                match given.take().filter(|_| is_password_question(questions)) {
                    Some(password) => Some(vec![password]),
                    None => asker.answers(questions).await,
                }
            })
            .await?;
        if answered {
            return Ok(Some(password));
        }
    }
    Ok(None)
}

/// The key's login, its passphrase asked when the key does not read without one.
async fn by_key(
    opened: &mut Opened,
    user: &str,
    key: &Path,
    asker: &Asker,
) -> Result<bool, Failure> {
    match opened.key(user, key, None).await {
        Err(Failure::Key(_)) if russh::keys::load_secret_key(key, None).is_err() => {
            let passphrase =
                (asker.text(&passphrase_prompt(key), true).await).ok_or(Failure::Cancelled)?;
            opened.key(user, key, Some(&passphrase)).await
        }
        other => other,
    }
}

/// A round of one question whose answer is not shown: the password's, answered by the one typed.
pub fn is_password_question(questions: &Questions) -> bool {
    matches!(questions.prompts.as_slice(), [(_, false)])
}

/// The folder the panel opens at: `folder` on the server (relative ones under the home folder), the home
/// folder when it is empty.
async fn opening_folder(fs: &Fs, folder: &str) -> Result<String, String> {
    let wanted = match folder.trim() {
        "" => ".",
        folder => folder,
    };
    let found = (fs.canonicalize(wanted).await).map_err(|error| format!("{wanted}: {error}"))?;
    Ok(panel_folder(&found))
}

/// A server's folder as the panel's path: `/` alone at its end only for the root.
pub fn panel_folder(folder: &str) -> String {
    match folder.trim_end_matches('/') {
        "" => "/".to_string(),
        trimmed => trimmed.to_string(),
    }
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
