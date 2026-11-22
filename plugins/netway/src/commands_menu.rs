//! NetBox's commands of a connected panel (`TWinSCPPlugin::CommandsMenu`, `TWinSCPFileSystem::
//! ApplyCommand`, `ShowInformation`, `HomeDirectory`): Ctrl+G "Apply command" - the command with WinSCP's
//! patterns run on the server for the selected files (once with `!&`), directories and recursion as
//! chosen, its output shown in the viewer or put on the clipboard - and "Server and protocol information".

use nc_plugin_sdk::{Dialog, DialogItem, Message, Value, Which};

use crate::NetWay;
use crate::custom_command::{self, Data};
use crate::sftp::child;
use crate::shell::quoted;

/// WinSCP's `ccApplyToDirectories`, `ccRecursive`, `ccShowResults`, `ccCopyResults` (its `ccLocal`, 0x100,
/// comes with the local command).
pub const TO_DIRECTORIES: i64 = 0x01;
pub const RECURSIVE: i64 = 0x02;
pub const SHOW_RESULTS: i64 = 0x200;
pub const COPY_RESULTS: i64 = 0x400;

const COMMAND: usize = 1;
const TO_DIRECTORIES_ITEM: usize = 10;
const RECURSIVE_ITEM: usize = 11;
const SHOW_ITEM: usize = 12;
const COPY_ITEM: usize = 13;
const OK: usize = 15;

/// NetBox's "Apply command" dialog for `command` with `params`.
pub fn apply_items(command: &str, params: i64) -> Vec<DialogItem> {
    let set = |flag: i64| params & flag != 0;
    vec![
        DialogItem::text("Enter &command to process selected files"),
        DialogItem::edit(command)
            .history("WinscpApplyCmd", false)
            .focus(),
        DialogItem::text("Patterns: !! - exclamation mark; ! - file name"),
        DialogItem::text("!&& - list of selected files (quoted, space-delimited)"),
        DialogItem::text("!/ - current remote path; !@ - current hostname"),
        DialogItem::text("!U - current username; !P - current password (if known)"),
        DialogItem::text("!?prompt[\\]?default! - prompts user for parameter value"),
        DialogItem::separator(),
        DialogItem::radio("&Remote command", true).group(),
        DialogItem::radio("&Local command", false)
            .same_row()
            .disabled(true),
        DialogItem::check("&Apply to directories", set(TO_DIRECTORIES)),
        DialogItem::check("&Execute recursively", set(RECURSIVE)).same_row(),
        DialogItem::check("&Show results in console", set(SHOW_RESULTS)),
        DialogItem::check("Copy results to clip&board", set(COPY_RESULTS)).same_row(),
        DialogItem::separator(),
        DialogItem::button("OK").default_button(),
        DialogItem::button("Cancel").same_row(),
    ]
}

/// The command and the params the dialog's `values` give.
pub fn applied(values: &[Value]) -> (String, i64) {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let flag = |item: usize, bit: i64| if value(item).checked() { bit } else { 0 };
    let params = flag(TO_DIRECTORIES_ITEM, TO_DIRECTORIES)
        | flag(RECURSIVE_ITEM, RECURSIVE)
        | flag(SHOW_ITEM, SHOW_RESULTS)
        | flag(COPY_ITEM, COPY_RESULTS);
    (value(COMMAND).text.trim().to_string(), params)
}

/// The command lines run for `items` (names under the folder, a folder's marked): one with `!&`, else one
/// per item a command applies to (folders only with "Apply to directories").
pub fn command_lines(
    command: &str,
    data: &Data,
    items: &[(String, bool)],
    (params, answers): (i64, &[String]),
) -> Vec<String> {
    let cd = format!("cd {} && ", quoted(&data.folder));
    let applies = |(_, folder): &&(String, bool)| !folder || params & TO_DIRECTORIES != 0;
    let names: Vec<String> = items
        .iter()
        .filter(applies)
        .map(|(name, _)| name.clone())
        .collect();
    if custom_command::takes_list(command) {
        let line = custom_command::expanded(command, data, ("", &names), answers);
        return vec![cd + &line];
    }
    (names.iter())
        .map(|name| cd.clone() + &custom_command::expanded(command, data, (name, &names), answers))
        .collect()
}

impl NetWay {
    /// Ctrl+G: NetBox's "Apply command" on the panel `instance`'s selected files at `path`.
    pub(crate) fn apply_command(&mut self, instance: &str, path: &str, targets: &[String]) -> bool {
        if targets.is_empty() {
            let message = Message::new("Apply command", &["No files selected"]).buttons(&["&OK"]);
            self.host.show(&message);
            return false;
        }
        let command = self.host.setting("ApplyCommandCommand").unwrap_or_default();
        let params = self.host.int_setting("ApplyCommandParams", 0);
        let dialog = Dialog::new("Apply command", apply_items(&command, params));
        let closed = self.host.dialog(&dialog);
        if closed.button != Some(OK) {
            return false;
        }
        let (command, params) = applied(&closed.values);
        self.host.set_setting("ApplyCommandCommand", &command);
        self.host.set_int("ApplyCommandParams", params);
        let mut answers = Vec::new();
        for prompt in custom_command::prompts(&command) {
            let title = "Apply command parameter";
            match self.host.input(title, &prompt.prompt, &prompt.default) {
                Some(answer) => answers.push(answer),
                None => return false,
            }
        }
        match self.run_command(instance, path, targets, (&command, params, &answers)) {
            Ok(output) => self.show_output(&command, params, &output),
            Err(failure) => self.error(&failure),
        }
        true
    }

    /// The command run on the server; its output.
    fn run_command(
        &self,
        instance: &str,
        path: &str,
        targets: &[String],
        (command, params, answers): (&str, i64, &[String]),
    ) -> Result<String, String> {
        let remote = self.remotes.get(instance).ok_or("Lost connection.")?;
        let runtime = self.runtime.as_ref().ok_or("Lost connection.")?;
        let commands = (remote.fs.commands()).ok_or("Cannot execute arbitrary command.")?;
        let data = Data {
            folder: path.to_string(),
            host: remote.plan.host.clone(),
            user: remote.plan.user.clone(),
            password: remote.plan.password.clone(),
        };
        let fs = &remote.fs;
        let work = async {
            let items = selected(fs, path, targets, params & RECURSIVE != 0).await?;
            let mut output = String::new();
            for line in command_lines(command, &data, &items, (params, answers)) {
                let ran = commands.run(&line).await?;
                output.push_str(&ran.out);
                output.push_str(&ran.err);
                if ran.status != 0 {
                    return Err(ran.failure());
                }
            }
            Ok(output)
        };
        let host = self.host;
        (runtime.wait(work, &|| host.cancelled())).map_err(|_| "Cancelled".to_string())?
    }

    /// "Show results in console": the output in the viewer; "Copy results to clipboard".
    fn show_output(&self, command: &str, params: i64, output: &str) {
        if params & COPY_RESULTS != 0 {
            self.host.set_clipboard(output);
        }
        if params & SHOW_RESULTS == 0 {
            return;
        }
        let file = std::env::temp_dir().join(format!("netway-{}.txt", std::process::id()));
        let text = format!("$ {command}\n{output}");
        if std::fs::write(&file, text).is_ok() {
            self.host.view(&file, true);
        }
    }

    /// NetBox's "Server and protocol information" of the panel `instance`.
    pub(crate) fn information(&self, instance: &str) {
        let Some(remote) = self.remotes.get(instance) else {
            return;
        };
        let yes = |can: bool| if can { "Yes" } else { "No" };
        let fs = &remote.fs;
        // WinSCP's `TSessionInfo` of each file system: its base protocol and what it can do.
        let (session, transfer) = match fs.protocol() {
            "SCP" => ("SSH-2", "SCP"),
            "SFTP" => ("SSH-2", "SFTP-3"),
            "FTP" => ("FTP", "FTP"),
            "WebDAV" => ("HTTP", "WebDAV"),
            other => ("HTTP", other),
        };
        let ssh = session == "SSH-2";
        let lines = [
            " Server information ".to_string(),
            format!("Session protocol: {session}"),
            format!("File transfer protocol: {transfer}"),
            format!(
                "Server host key fingerprint (SHA256): {}",
                (remote.connection.as_ref())
                    .and_then(|connection| connection.fingerprint.as_deref())
                    .unwrap_or("Unknown")
            ),
            " Protocol capabilities/information ".to_string(),
            format!("Can change permissions: {}", yes(ssh || transfer == "FTP")),
            format!("Can change owner/group: {}", yes(ssh)),
            format!(
                "Can execute arbitrary command: {}",
                yes(fs.commands().is_some())
            ),
            format!("Can create symlink/hardlink: {}", yes(ssh)),
            format!(
                "Can duplicate remote files: {}",
                yes(fs.commands().is_some())
            ),
            format!("Native text (ASCII) mode transfers: {}", yes(false)),
            format!(
                "Can resume transfers: {}",
                yes(fs.can(crate::fs::Capability::Resume))
            ),
        ];
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let message = Message::new("Server and protocol information", &lines)
            .left_align()
            .buttons(&["&Copy to Clipboard", "&Close"]);
        if self.host.show(&message) == Some(0) {
            self.host.set_clipboard(&lines.join("\r\n"));
        }
    }

    /// NetBox's "Home directory": the panel at the user's home folder.
    pub(crate) fn home_directory(&self, instance: &str) {
        let (Some(remote), Some(runtime)) = (self.remotes.get(instance), self.runtime.as_ref())
        else {
            return;
        };
        let home = runtime.wait(remote.fs.canonicalize("."), &|| false);
        match home {
            Ok(Ok(home)) => {
                remote.unfollowed_to(&home);
                let _ = self.host.set_folder(Which::Active, &home, None);
            }
            Ok(Err(error)) => self.error(&error.to_string()),
            Err(_) => {}
        }
    }
}

/// The selected items with whether each is a folder; recursively, a folder's items too (as paths under
/// the folder).
async fn selected(
    fs: &crate::fs::Fs,
    path: &str,
    targets: &[String],
    recursive: bool,
) -> Result<Vec<(String, bool)>, String> {
    let listed = fs.list_raw(path).await?;
    let mut items = Vec::new();
    let mut folders: Vec<String> = Vec::new();
    for name in targets {
        let folder = (listed.iter()).any(|item| &item.name == name && item.is_dir && !item.is_link);
        items.push((name.clone(), folder));
        if folder && recursive {
            folders.push(name.clone());
        }
    }
    while let Some(folder) = folders.pop() {
        for item in fs.list_raw(&child(path, &folder)).await? {
            let name = format!("{folder}/{}", item.name);
            let inner = item.is_dir && !item.is_link;
            if inner {
                folders.push(name.clone());
            }
            items.push((name, inner));
        }
    }
    Ok(items)
}

#[cfg(test)]
#[path = "commands_menu_tests.rs"]
mod tests;
