//! Commands on the server over the SSH connection (WinSCP's `TSCPFileSystem` shell, `TSecureShell`'s
//! exec): each command on a channel of its own, its output, error output and exit status collected; a
//! path quoted as WinSCP quotes it (`"..."` with `\`, `$`, `` ` `` and `"` escaped).

use std::sync::Arc;

use russh::ChannelMsg;
use russh::client::{Handle, Msg};

use crate::ssh::Client;

/// The status of a command whose end the server did not tell.
pub const NO_STATUS: u32 = u32::MAX;

/// A command's results.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub status: u32,
    pub out: String,
    pub err: String,
}

impl Output {
    /// WinSCP's failure of a command: its error output, else its output, else its status.
    pub fn failure(&self) -> String {
        let text = match self.err.trim() {
            "" => self.out.trim(),
            err => err,
        };
        match text {
            "" => format!("Command failed with return code {}.", self.status),
            text => text.to_string(),
        }
    }
}

/// `text` in double quotes for the shell, its specials escaped.
pub fn quoted(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for ch in text.chars() {
        if matches!(ch, '\\' | '$' | '`' | '"') {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    quoted
}

/// The server's answer to a channel's request (a command, a subsystem): granted or refused.
pub async fn granted(channel: &mut russh::Channel<Msg>) -> bool {
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => return true,
            Some(ChannelMsg::Failure | ChannelMsg::Close | ChannelMsg::Eof) | None => return false,
            Some(_) => {}
        }
    }
}

/// The connection's commands.
#[derive(Clone)]
pub struct Commands {
    handle: Arc<Handle<Client>>,
    /// The session's shell (its SCP/Shell tab), none: the user's.
    shell: Option<String>,
}

/// `command` as `shell` runs it (`shell -c "command"`).
pub fn in_shell(shell: Option<&str>, command: &str) -> String {
    match shell {
        Some(shell) => format!("{shell} -c {}", quoted(command)),
        None => command.to_string(),
    }
}

impl Commands {
    pub(crate) fn new(handle: Arc<Handle<Client>>) -> Self {
        Self {
            handle,
            shell: None,
        }
    }

    /// The commands run by `shell` (empty: the user's).
    pub fn with_shell(mut self, shell: &str) -> Self {
        self.shell = (!shell.trim().is_empty()).then(|| shell.trim().to_string());
        self
    }

    /// A channel running `command`, for a protocol over its input and output (SCP's); the server's
    /// refusal is WinSCP's `MSG_EXECUTE_APP_ERROR`.
    pub async fn open(&self, command: &str) -> Result<russh::Channel<Msg>, String> {
        let mut channel =
            (self.handle.channel_open_session().await).map_err(|error| error.to_string())?;
        let line = in_shell(self.shell.as_deref(), command);
        (channel.exec(true, line).await).map_err(|error| error.to_string())?;
        match granted(&mut channel).await {
            true => Ok(channel),
            false => Err(format!("Cannot execute '{command}'.")),
        }
    }

    /// `command` run to its end.
    pub async fn run(&self, command: &str) -> Result<Output, String> {
        let mut channel = self.open(command).await?;
        // No status told (a command killed by a signal, a server that tells none) is no success.
        let mut output = Output {
            status: NO_STATUS,
            ..Output::default()
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => out.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => err.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => output.status = exit_status,
                ChannelMsg::ExitSignal { signal_name, .. } => {
                    output.status = NO_STATUS;
                    err.extend_from_slice(format!("Killed by signal {signal_name:?}.").as_bytes());
                }
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        output.out = String::from_utf8_lossy(&out).into_owned();
        output.err = String::from_utf8_lossy(&err).into_owned();
        Ok(output)
    }

    /// `command` run, its failure an error.
    pub async fn check(&self, command: &str) -> Result<Output, String> {
        let output = self.run(command).await?;
        match output.status {
            0 => Ok(output),
            _ => Err(output.failure()),
        }
    }
}

#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;
