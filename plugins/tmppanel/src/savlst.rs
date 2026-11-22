//! The temporary panel's common panels and list files, as FAR's TmpPanel (`TmpClass.cpp:660-808`):
//! Alt+Shift+F12 "Available temporary panels" and Alt+Shift+0..9 make another of the ten common panels the
//! current one, which every temporary panel shows; Alt+Shift+F2 "SavLst" saves the list in the other
//! panel's folder as `panel<n>.<extension>` (`panel.<extension>` without the common panel), in UTF-8 with
//! its BOM or, with ListUTF8 off, in UTF-16 with its BOM.

use std::path::{Path, PathBuf};

use nc_plugin_sdk::{Error, InputOptions, KeyAnswer, Menu, MenuItem, Message};

use crate::{ITEM, PANELS, TmpPanel};

impl TmpPanel {
    /// Alt+Shift+F12: TmpPanel's `ProcessPanelSwitchMenu` - "Available temporary panels", each line
    /// "&n. Total files: k", the current one selected.
    pub(crate) fn switch_menu(&mut self) -> KeyAnswer {
        let items = (self.panels.iter().enumerate())
            .map(|(at, list)| MenuItem::new(&format!("&{at}. Total files: {}", list.paths().len())))
            .collect();
        // TmpPanel's `FMENU_AUTOHIGHLIGHT | FMENU_WRAPMODE`.
        let menu = (Menu::new("Available temporary panels", items).selected(self.current))
            .auto_hotkeys()
            .wrap();
        match self.host.menu(&menu).and_then(|chosen| chosen.item) {
            Some(panel) => self.switch_to(panel),
            None => KeyAnswer::default(),
        }
    }

    /// Alt+Shift+n and the menu: TmpPanel's `SwitchToPanel` - the common panel `panel` the current one;
    /// both panels read again (the other one may show it too).
    pub(crate) fn switch_to(&mut self, panel: usize) -> KeyAnswer {
        if panel >= PANELS || panel == self.current {
            return KeyAnswer::default();
        }
        self.current = panel;
        KeyAnswer {
            reread_passive: true,
            ..KeyAnswer::reread()
        }
    }

    /// Alt+Shift+F2: TmpPanel's `ProcessSaveListKey` - asked for the file (the other panel's folder,
    /// `panel<n>.<the mask's extension>`), the list saved there, a line a name; the other panel read again.
    pub(crate) fn save_list(
        &mut self,
        instance: &str,
        press: &nc_plugin_sdk::KeyPress,
    ) -> Result<KeyAnswer, Error> {
        let paths = self.list(instance)?.paths().to_vec();
        if paths.is_empty() {
            return Ok(KeyAnswer::default());
        }
        // The other panel's folder (FAR's `FCTL_GETPANELDIRECTORY`); the program hands the folder it works
        // in when that one is on no disk.
        let folder = press.passive.clone().unwrap_or_default();
        let offered = folder.join(self.list_name());
        // TmpPanel's `InputBox` with `FIB_BUTTONS` (its history "TmpPanel.SaveList" waits for the program's
        // dialog histories).
        let buttons = InputOptions {
            buttons: true,
            ..InputOptions::default()
        };
        let Some(typed) = (self.host).input_with(
            ITEM,
            "Save file list as",
            &offered.to_string_lossy(),
            buttons,
        ) else {
            return Ok(KeyAnswer::default());
        };
        // A name without its folder is in FAR's current folder (`GetFullPath`): the one the panel was
        // opened from, which a plugin's panel never changes.
        let base = press.folder.clone().unwrap_or(folder);
        let target = base.join(PathBuf::from(typed));
        if let Err(error) = save(&target, &paths, self.options.list_utf8) {
            // FAR's `FMSG_WARNING | FMSG_ERRORTYPE`: "Error" with the system's reason.
            let message = Message::new("Error", &[&error.to_string()])
                .buttons(&["OK"])
                .warning();
            let _ = self.host.show(&message);
        }
        Ok(KeyAnswer {
            reread_passive: true,
            ..KeyAnswer::default()
        })
    }

    /// The list file's name offered (`TmpClass.cpp:676-684`): "panel", the current common panel's number
    /// with the common panel, the first mask's extension when it has one without wildcards.
    fn list_name(&self) -> String {
        let number = match self.options.common_panel {
            true => self.current.to_string(),
            false => String::new(),
        };
        match self.options.list_extension() {
            Some(extension) => format!("panel{number}.{extension}"),
            None => format!("panel{number}"),
        }
    }
}

/// TmpPanel's `SaveListFile` (`TmpClass.cpp:696-751`): with `utf8` (ListUTF8) the UTF-8 BOM and each path
/// with CR LF in UTF-8, else the UTF-16 LE BOM and each path with CR LF in UTF-16 LE.
pub fn save(target: &Path, paths: &[PathBuf], utf8: bool) -> std::io::Result<()> {
    if !utf8 {
        let bytes: Vec<u8> = utf16_lines(paths)
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        return std::fs::write(target, bytes);
    }
    let mut text = String::from("\u{feff}");
    for path in paths {
        text.push_str(&path.to_string_lossy());
        text.push_str("\r\n");
    }
    std::fs::write(target, text)
}

/// The BOM and each path with CR LF in UTF-16: on Windows the name's own units (FAR writes its `wchar_t`
/// name as it is, an unpaired surrogate too), elsewhere its text.
fn utf16_lines(paths: &[PathBuf]) -> Vec<u16> {
    let mut units = vec![0xFEFF];
    for path in paths {
        #[cfg(windows)]
        units.extend(std::os::windows::ffi::OsStrExt::encode_wide(
            path.as_os_str(),
        ));
        #[cfg(not(windows))]
        units.extend(path.to_string_lossy().encode_utf16());
        units.extend([u16::from(b'\r'), u16::from(b'\n')]);
    }
    units
}

#[cfg(test)]
#[path = "savlst_tests.rs"]
mod tests;
