//! Where the panels go after a plugin's answer (FAR's `ProcessPanelInput` / `OpenW` results): a folder or
//! a file in the active or the passive panel, the command line, a file opened by the system, another of
//! the plugin's panels in place of its own.

use crate::app::{App, Side};
use crate::plugin_host::KeyGo;
use crate::vfs::VfsPath;

impl App {
    /// Where the panel `side` (the key's) or the other one goes after the key.
    pub(in crate::app) fn go_after_key(&mut self, side: Side, go: KeyGo) -> std::io::Result<()> {
        let other = side.other();
        match go {
            KeyGo::PassiveFolder(folder) => {
                self.panels[other.index()].change_dir(&VfsPath::local(folder), None)
            }
            KeyGo::PassiveFile(file) => self.go_to_file(other, &file),
            KeyGo::ActiveFolder(folder) => {
                self.panels[side.index()].change_dir(&VfsPath::local(folder), None)
            }
            KeyGo::ActiveFile(file) => self.go_to_file(side, &file),
            KeyGo::CommandLine(text) => {
                self.command_line.set(&text);
                Ok(())
            }
            // FAR's `ShellExecute` "open" (TmpPanel's menu from a list).
            KeyGo::ShellOpen(path) => {
                self.open_by_system(path, "Open");
                Ok(())
            }
            KeyGo::Open(instance) => {
                let panel = &mut self.panels[side.index()];
                match panel.path.provider().map(str::to_string) {
                    Some(provider) => {
                        let path = VfsPath::in_session(&provider, &instance, "/");
                        panel.switch_plugin_panel();
                        panel.change_dir(&path, None)
                    }
                    None => Ok(()),
                }
            }
        }
    }

    /// TmpPanel's `GoToFile`: the panel `side` to the file's folder, the cursor on it.
    fn go_to_file(&mut self, side: Side, file: &std::path::Path) -> std::io::Result<()> {
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        match file.parent() {
            Some(folder) => {
                self.panels[side.index()].change_dir(&VfsPath::local(folder), name.as_deref())
            }
            None => Ok(()),
        }
    }
}
