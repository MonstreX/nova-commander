//! Panel keys that put files on the clipboard (FAR's `FileList::CopyNames` and `CopyFiles`):
//! - Ctrl+Ins (with an empty command line) and Ctrl+Shift+Ins: the names;
//! - Alt+Shift+Ins: the full names; Ctrl+Alt+Ins: the network (UNC) names;
//! - Ctrl+Shift+C / Ctrl+Shift+X: the files themselves, for Explorer's Paste (copy / move).
//!
//! They take the selected files, or the one under the cursor; ".." stands for the panel's folder.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::App;
use super::cmdline_insert::{NameKind, full_name, provider_name, put_text};
use super::name_cases::NameCases;
use crate::vfs::Entry;

const CTRL: KeyModifiers = KeyModifiers::CONTROL;
const CTRL_SHIFT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::SHIFT);
const CTRL_ALT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::ALT);
const ALT_SHIFT: KeyModifiers = KeyModifiers::ALT.union(KeyModifiers::SHIFT);

/// Handles the key if it is one of these; returns whether it was.
pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Insert, CTRL) if app.command_line.is_empty() => copy_names(app, NameKind::Name),
        (KeyCode::Insert, CTRL_SHIFT) => copy_names(app, NameKind::Name),
        (KeyCode::Insert, ALT_SHIFT) => copy_names(app, NameKind::Full),
        (KeyCode::Insert, CTRL_ALT) => copy_names(app, NameKind::Network),
        (KeyCode::Char(letter @ ('c' | 'x')), CTRL_SHIFT) => {
            let files = chosen(app).into_iter().map(|(path, ..)| path).collect();
            if let Err(error) = copy_files(app, files, letter == 'x') {
                app.show_error("Clipboard", &error);
            }
        }
        _ => return false,
    }
    true
}

/// The names, one per line (FAR quotes them only when inserting into the command line); network names
/// are asked for on a worker.
fn copy_names(app: &mut App, kind: NameKind) {
    if app.active_panel().path.as_local().is_none() {
        let names = provider_names(app.active_panel(), kind);
        if !names.is_empty() {
            onto_clipboard(app, names);
        }
        return;
    }
    let chosen = chosen(app);
    if chosen.is_empty() {
        return;
    }
    let cases = NameCases::of_active(app);
    let make = move || {
        let names: Vec<String> = (chosen.into_iter())
            .map(|(path, name, directory)| match kind {
                NameKind::Name => name,
                _ => cases.apply(&full_name(&path, kind).display().to_string(), directory),
            })
            .collect();
        names.join(crate::platform::LINE_BREAK)
    };
    put_text(app, kind == NameKind::Network, make, onto_clipboard);
}

/// FAR's `CopyNames` on a plugin's panel: each name, or the provider's own path and the name; ".." is
/// that path's last part for the names (none at its root), the path and `..` for the full names.
fn provider_names(panel: &super::Panel, kind: NameKind) -> String {
    let current: Vec<&Entry> = if panel.selected.is_empty() {
        panel.current().into_iter().collect()
    } else {
        (panel.entries.iter())
            .filter(|entry| panel.selected.contains(&entry.name))
            .collect()
    };
    let name = |entry: &Entry| match (kind, entry.is_parent) {
        (NameKind::Name, true) => panel.path.file_name().unwrap_or_default(),
        (_, true) => format!("{}{}", panel.folder_text(), entry.name),
        _ => provider_name(panel, entry, kind),
    };
    current
        .into_iter()
        .map(name)
        .collect::<Vec<_>>()
        .join(crate::platform::LINE_BREAK)
}

fn onto_clipboard(app: &mut App, text: String) {
    if let Err(error) = app.clipboard.set_text(&text) {
        app.show_error("Clipboard", &error);
    }
}

fn copy_files(app: &mut App, files: Vec<PathBuf>, cut: bool) -> std::io::Result<()> {
    app.clipboard.set_files(&files, cut)
}

/// The selected entries in panel order, or the one under the cursor: each as its full path, the name the
/// panel shows and whether it is a folder. ".." is the panel's folder (its name, or the whole path for a root). On a
/// plugin's panel its real files (FAR's real names, `item_path`); an entry with none on a disk is left
/// out.
fn chosen(app: &App) -> Vec<(PathBuf, String, bool)> {
    let panel = app.active_panel();
    let on_disk = |entry: &Entry| {
        (panel.item_path(entry).as_local())
            .map(|path| (path.to_path_buf(), entry.name.clone(), entry.is_dir))
    };
    let as_folder = || {
        let dir = panel.path.as_local()?;
        let name = dir.file_name().map_or_else(
            || dir.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        Some((dir.to_path_buf(), name, true))
    };
    if panel.selected.is_empty() {
        return (panel.current())
            .and_then(|entry| {
                if entry.is_parent {
                    as_folder()
                } else {
                    on_disk(entry)
                }
            })
            .into_iter()
            .collect();
    }
    (panel.entries.iter())
        .filter(|entry| panel.selected.contains(&entry.name))
        .filter_map(on_disk)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::clipboard::Memory;
    use crate::settings::Settings;

    fn app() -> (tempfile::TempDir, App, Memory) {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b.txt", "a b.txt", "c.txt"] {
            std::fs::write(dir.path().join(name), "x").unwrap();
        }
        let mut app = App::new(dir.path(), 120, 40, &Settings::default());
        let memory = Memory::default();
        app.clipboard = Box::new(memory.clone());
        (dir, app, memory)
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_key(KeyEvent::new(code, modifiers));
        crate::app::waiting::finish_waiting(app);
    }

    #[test]
    fn ctrl_ins_family_copies_names_of_the_selection_in_panel_order() {
        let (dir, mut app, memory) = app();
        press(&mut app, KeyCode::Insert, CTRL);
        let folder = dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(memory.contents().text, Some(folder), "on '..': the folder");

        // Insert selects and moves down.
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Insert, KeyModifiers::NONE);
        press(&mut app, KeyCode::Insert, KeyModifiers::NONE);
        press(&mut app, KeyCode::Insert, CTRL_SHIFT);
        assert_eq!(memory.contents().text.as_deref(), Some("a b.txt\r\nb.txt"));
        press(&mut app, KeyCode::Insert, ALT_SHIFT);
        let full = |name: &str| dir.path().join(name).display().to_string();
        let expected = format!("{}\r\n{}", full("a b.txt"), full("b.txt"));
        assert_eq!(memory.contents().text, Some(expected));

        app.command_line.set("dir");
        press(&mut app, KeyCode::Insert, CTRL);
        assert_eq!(
            memory.contents().text.as_deref(),
            Some("dir"),
            "with a typed command Ctrl+Ins copies the command line"
        );
    }

    #[test]
    /// crossterm reports Ctrl+Shift letters in upper case.
    fn ctrl_shift_c_and_x_put_the_files_for_explorer() {
        let (dir, mut app, memory) = app();
        press(&mut app, KeyCode::End, KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('X'), CTRL_SHIFT);
        assert_eq!(
            memory.contents().files,
            Some((vec![dir.path().join("c.txt")], true))
        );
        press(&mut app, KeyCode::Char('C'), CTRL_SHIFT);
        assert_eq!(
            memory.contents().files,
            Some((vec![dir.path().join("c.txt")], false))
        );
    }
}
