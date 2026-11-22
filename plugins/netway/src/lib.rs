//! NetWay (ADR-0023): remote file systems after FAR's NetBox (`netbox-source/`, its behaviour only: NetBox
//! is GPL, none of its code is taken). S6.4a: the plugin's runtime, an SSH connection with its host key and
//! logins, an SFTP folder's listing. S6.4b: the panel of stored sessions (`sessions`, `session_keys`), a
//! file per session in the plugin's data folder (`store`, its passwords kept by DPAPI: `secret`), NetBox's
//! session dialog's Session tab (`session_dialog`); the plugin's items in the Plugins menu (F11) and the
//! Disks menu open the panel. S6.4c: F5 / F6's export and import of sessions as files (`transfer`), the
//! command line's prefixes `netway:`, `sftp:` and the others (`address`). S6.5a: connecting (`login`,
//! `connecting`, the user asked from the runtime's work through `ask`) and an SFTP session's panel
//! (`remote`, `remote_panel`). S6.5b: copying both ways, F3 / F4 (`copy_panel` with NetBox's copy
//! dialog, questions and progress window; the work in `copy_work`, `download`, `upload`). S6.5c: the
//! transfer settings (`copy_params`, `file_masks`, `text_mode`, NetBox's dialog in `transfer_settings`),
//! the plugin's configuration (`options`, `configuration`), the editor's upload at each save. S6.5d: the
//! operations on the server (`remote_ops`, `remote_dialogs`, `remote_keys`, `rights_box`). S6.5e: resume
//! through a temporary name, reconnecting (`reopen`), "Endurance settings" (`endurance`).

pub mod address;
pub mod ask;
pub mod background_files;
mod background_panel;
pub mod bookmarks;
pub mod browsing;
mod browsing_panel;
pub mod cleanup;
mod commands_menu;
pub mod configuration;
mod connecting;
pub mod copy_dialog;
pub mod copy_names;
mod copy_panel;
pub mod copy_params;
pub mod copy_progress;
pub mod copy_questions;
pub mod copy_work;
pub mod custom_command;
pub mod digest;
mod download;
pub mod endurance;
pub mod file_masks;
pub mod fs;
pub mod ftp;
pub mod ftp_connect;
pub mod ftp_listing;
pub mod ftp_login;
pub mod ftp_tls;
pub mod generate_url;
pub mod keep_up_to_date;
pub mod link_options;
mod location_profiles;
pub mod login;
mod open_directory;
pub mod options;
mod placed;
pub mod protocol_options;
pub mod proxy;
pub mod remote;
pub mod remote_dialogs;
mod remote_keys;
pub mod remote_ops;
mod remote_panel;
mod reopen;
pub mod rights_box;
pub mod route;
pub mod runtime;
pub mod s3;
pub mod s3_login;
pub mod s3_xml;
pub mod scp;
pub mod scp_listing;
pub mod secret;
pub mod session;
pub mod session_dialog;
mod session_keys;
pub mod session_link_tabs;
pub mod session_options;
pub mod session_protocol_tabs;
pub mod session_ssh_tabs;
pub mod session_tabs;
pub mod sessions;
pub mod sftp;
pub mod shell;
pub mod sigv4;
pub mod ssh;
pub mod ssh_options;
pub mod store;
pub mod sync_collect;
pub mod sync_dialog;
mod sync_panel;
pub mod sync_work;
pub mod tab_bar;
pub mod text_mode;
pub mod transfer;
pub mod transfer_settings;
mod upload;
pub mod webdav;
pub mod webdav_login;
pub mod webdav_xml;

#[cfg(test)]
mod test_dav;
#[cfg(test)]
mod test_ftp;
#[cfg(test)]
mod test_proxy;
#[cfg(test)]
mod test_s3;
#[cfg(test)]
mod test_server;
#[cfg(test)]
mod test_shell;

use std::collections::HashMap;
use std::path::Path;

use nc_plugin_sdk::{
    Error, ErrorKind, FileEntry, GetMode, GetRequest, Host, Info, KeyAnswer, KeyGo, KeyPress,
    OpenHow, PanelFlags, PanelKey, PanelMode, PutRequest, export_plugin,
};

use address::{Address, PREFIXES, Target};
use remote::Remote;
use runtime::Runtime;
use sessions::{FORMAT, TITLE, folder_of};
use store::Store;

/// The sessions' folder in the plugin's data folder.
pub const SESSIONS_FOLDER: &str = "Sessions";
/// The start of a connected session's panel instance's name.
pub const REMOTE_PREFIX: &str = "sftp-";
/// The session files Enter on a disk connects to (NetBox's `.netbox` files of `OPEN_ANALYSE`).
pub const SESSION_MASK: &str = "*.netway";

pub struct NetWay {
    host: Host,
    store: Store,
    /// The panels opened, for their instances' names.
    opened: usize,
    /// The runtime of the connections, made at the first.
    runtime: Option<Runtime>,
    /// The connected sessions' panels by their instances.
    remotes: HashMap<String, Remote>,
    /// The settings of the last download for the editor (NetBox's `FLastEditCopyParam`): its upload's.
    last_edit: Option<copy_params::CopyParams>,
}

impl NetWay {
    /// A new sessions panel's instance.
    fn open_sessions(&mut self) -> String {
        self.opened += 1;
        format!("sessions-{}", self.opened)
    }

    /// `address` connected to, its panel in the active panel.
    fn connect_answer(&mut self, address: Address) -> KeyAnswer {
        match self.connect(address) {
            Some(instance) => KeyAnswer::go(KeyGo::Open(instance)),
            None => KeyAnswer::default(),
        }
    }
}

impl nc_plugin_sdk::Plugin for NetWay {
    fn new(host: Host) -> Self {
        Self {
            host,
            store: Store::new(host.data_folder().join(SESSIONS_FOLDER)),
            opened: 0,
            runtime: None,
            remotes: HashMap::new(),
            last_edit: None,
        }
    }

    fn info(&self) -> Info {
        Info {
            title: TITLE.to_string(),
            description: "Remote file systems: SFTP, SCP, FTP, WebDAV and S3 sessions".to_string(),
            author: "nova-commander (after FAR's NetBox)".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// NetBox's item in the Plugins menu.
    fn commands(&self) -> Vec<String> {
        vec![TITLE.to_string()]
    }

    fn run_command(&mut self, _index: usize) -> Result<Option<String>, String> {
        Ok(Some(self.open_sessions()))
    }

    /// NetBox's item in the Disks menu.
    fn disks(&self) -> Vec<String> {
        vec![TITLE.to_string()]
    }

    fn open_disk(&mut self, _index: usize) -> Result<Option<String>, String> {
        Ok(Some(self.open_sessions()))
    }

    /// NetBox's item in Options > Plugins configuration.
    fn config_items(&self) -> Vec<String> {
        vec![TITLE.to_string()]
    }

    fn configure(&mut self, _index: usize) -> bool {
        configuration::configure(&self.host)
    }

    /// NetBox's `CommandPrefixes`.
    fn prefixes(&self) -> Vec<String> {
        PREFIXES.map(str::to_string).to_vec()
    }

    /// NetBox's `OpenPluginEx` from the command line: nothing named, the sessions panel; a session,
    /// connected to.
    fn open_command(
        &mut self,
        prefix: &str,
        command: &str,
        _folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        let stored = self.store.sessions_under("");
        match address::parse(prefix, command, &stored) {
            Target::Sessions => Ok(KeyAnswer::go(KeyGo::Open(self.open_sessions()))),
            Target::Connect(address) => Ok(self.connect_answer(address)),
        }
    }

    /// The session files Enter connects to.
    fn file_masks(&self) -> Vec<String> {
        vec![SESSION_MASK.to_string()]
    }

    /// NetBox's `OPEN_ANALYSE` of its session files: Enter or Ctrl+PgDn on a session file connects to it;
    /// any other file of the mask is passed on.
    fn open_file(
        &mut self,
        path: &Path,
        how: OpenHow,
        _folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        if !matches!(how, OpenHow::Enter | OpenHow::PgDn) {
            return Ok(KeyAnswer::pass());
        }
        let text = std::fs::read_to_string(path).unwrap_or_default();
        if !session::Session::is_session_text(&text) {
            return Ok(KeyAnswer::pass());
        }
        let loaded = (store::load_file(path)).map_err(|error| {
            Error::new(ErrorKind::Other, format!("{}: {error}", path.display()))
        })?;
        // A file from anywhere (an archive, a share) is no session the user made: a local command it
        // would run to connect is asked about first (NetBox runs it as it is).
        let link = link_options::Link::read(&loaded.session.other);
        let command = link.local_command.trim();
        if link.proxy == link_options::ProxyMethod::Local && !command.is_empty() {
            let lines = [
                "The session connects through a local command:",
                command,
                "Run it?",
            ];
            let question = nc_plugin_sdk::Message::new("Warning", &lines)
                .warning()
                .buttons(&["&Yes", "&No"]);
            if self.host.show(&question) != Some(0) {
                return Ok(KeyAnswer::default());
            }
        }
        let address = Address {
            session: loaded.session,
            ..Address::default()
        };
        Ok(self.connect_answer(address))
    }

    fn list(&mut self, instance: &str, path: &str) -> Result<Vec<FileEntry>, Error> {
        if instance.starts_with(REMOTE_PREFIX) {
            return self.list_remote(instance, path);
        }
        let items = (self.store.items(folder_of(path)))
            .map_err(|error| Error::new(ErrorKind::NotFound, format!("{path}: {error}")))?;
        Ok(sessions::entries(&items))
    }

    fn panel_title(&self, instance: &str, path: &str) -> String {
        match self.remotes.get(instance) {
            Some(remote) => {
                remote.noticed(path, &self.host);
                remote.title(path)
            }
            None => sessions::title(path),
        }
    }

    fn panel_format(&self, instance: &str, _path: &str) -> String {
        match self.remotes.get(instance) {
            Some(remote) => remote.format(),
            None => FORMAT.to_string(),
        }
    }

    fn panel_modes(&self, instance: &str, _path: &str) -> Vec<Option<PanelMode>> {
        match self.remotes.contains_key(instance) {
            true => remote::modes(),
            false => sessions::modes(),
        }
    }

    /// NetBox's `OPIF_SHOWPRESERVECASE`; F5 / F6 out of either panel and into it its own (`GetFilesEx`,
    /// `PutFilesEx`); on the sessions F7 and F8 too (`MakeDirectoryEx`, `DeleteFilesEx`); a connected
    /// session's edited file back at each save with "Upload after every save" (its `EE_SAVE`).
    fn panel_flags(&self, instance: &str) -> PanelFlags {
        let sessions = !instance.starts_with(REMOTE_PREFIX);
        let on_save = !sessions && options::editor_options(&self.host).upload_on_save;
        PanelFlags {
            preserve_case: true,
            deletes: true,
            makes_folders: true,
            gives_files: true,
            takes_paths: true,
            puts_on_save: on_save,
            ..PanelFlags::default()
        }
    }

    fn panel_keys(&self, instance: &str) -> Vec<PanelKey> {
        match instance.starts_with(REMOTE_PREFIX) {
            true => remote_keys::keys(),
            false => sessions::keys(),
        }
    }

    fn synchro(&mut self, param: u64) {
        self.go_to_start(param);
    }

    fn close_panel(&mut self, instance: &str) {
        self.disconnect(instance);
    }

    // The program's file calls (its background tasks, S6.8b): a connected panel's own connection.
    fn stat(&mut self, instance: &str, path: &str) -> Result<FileEntry, Error> {
        if !instance.starts_with(REMOTE_PREFIX) {
            return Err(Error::new(ErrorKind::Unsupported, "Not a connected panel"));
        }
        self.file_stat(instance, path)
    }

    fn read(
        &mut self,
        instance: &str,
        path: &str,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize, Error> {
        self.file_read(instance, path, offset, buffer)
    }

    fn write(
        &mut self,
        instance: &str,
        path: &str,
        mode: nc_plugin_sdk::WriteMode,
        data: &[u8],
    ) -> Result<(), Error> {
        self.file_write(instance, path, mode, data)
    }

    fn write_end(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        self.file_end(instance, path)
    }

    fn write_abort(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        self.file_abort(instance, path)
    }

    fn make_dir(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        self.file_make_dir(instance, path)
    }

    fn rename(&mut self, instance: &str, from: &str, to: &str, replace: bool) -> Result<(), Error> {
        self.file_rename(instance, (from, to), replace)
    }

    fn remove(&mut self, instance: &str, path: &str, folder: bool) -> Result<(), Error> {
        self.file_remove(instance, path, folder)
    }

    fn set_times(
        &mut self,
        instance: &str,
        path: &str,
        times: &nc_plugin_sdk::Times,
    ) -> Result<(), Error> {
        self.file_set_times(instance, path, times)
    }

    fn panel_key(
        &mut self,
        instance: &str,
        path: &str,
        press: &KeyPress,
    ) -> Result<KeyAnswer, Error> {
        Ok(match instance.starts_with(REMOTE_PREFIX) {
            true => self.remote_key(instance, path, press),
            false => self.session_key(path, press),
        })
    }

    fn make_folder(
        &mut self,
        instance: &str,
        path: &str,
        name: &str,
        mode: GetMode,
    ) -> Result<String, Error> {
        match instance.starts_with(REMOTE_PREFIX) {
            true => self.make_remote_folder(instance, path, name, mode),
            false => self.make_session_folder(path, name, mode),
        }
    }

    fn delete_files(
        &mut self,
        instance: &str,
        path: &str,
        names: &[String],
        mode: GetMode,
    ) -> Result<(), Error> {
        match instance.starts_with(REMOTE_PREFIX) {
            true => self.delete_remote(instance, path, names, mode),
            false => self.delete_sessions(path, names, mode),
        }
    }

    fn get(&mut self, instance: &str, path: &str, request: &GetRequest) -> Result<(), Error> {
        match instance.starts_with(REMOTE_PREFIX) {
            true => self.get_remote(instance, path, request),
            false => self.export_sessions(path, request),
        }
    }

    fn put(&mut self, instance: &str, path: &str, request: &PutRequest) -> Result<(), Error> {
        match instance.starts_with(REMOTE_PREFIX) {
            true => self.put_remote(instance, path, request),
            false => self.import_sessions(path, request),
        }
    }
}

export_plugin!(NetWay);
