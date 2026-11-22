//! Panel paths and names put into a dialog's edit line, as FAR's `Edit::ProcessInsPath` /
//! `MakePathForUI` (`far/edit.cpp`, `far/panelmix.cpp`) do in every edit line:
//! - Ctrl+[ / Ctrl+] the left / right panel's folder, Ctrl+Shift+[ / ] the active / passive one's;
//! - Ctrl+Alt+[ / ] (left / right) and Alt+Shift+[ / ] (active / passive) the network (UNC) name;
//! - Shift+Enter the name under the cursor in the active panel, Ctrl+Shift+Enter in the passive one.
//!
//! A folder ends with a slash; a text with spaces is quoted (FAR's "quoted names" on insert). An
//! untouched field (its text all selected, as F5's target) is replaced, as typing would replace it.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::cmdline_insert::{
    NameKind, folder_source, full_name, put_text, side_of, with_end_slash,
};
use super::dialog::Dialog;
use super::form::Control;
use super::line_input::LineInput;
use super::{App, Overlay};

/// The key's text into the dialog's focused edit line, the dialog back on screen; the dialog is
/// given back when the key is not one of these. A network name is asked for on a worker and put into
/// the line on screen when it comes.
pub(super) fn insert(app: &mut App, mut dialog: Dialog, key: KeyEvent) -> Option<Dialog> {
    // RCtrl+0..9: a folder shortcut's folder (FAR's `ProcessInsPath`), chosen from its list if need be.
    if let Some(list) = super::console_input::right_ctrl_digit(&key)
        && focused_input(&mut dialog).is_some()
    {
        app.shortcut_into_dialog(dialog, list);
        return None;
    }
    if let Some(folder) = network_folder(app, key)
        && focused_input(&mut dialog).is_some()
    {
        app.overlay = Some(Overlay::Dialog(dialog));
        let make = move || quote_spaces(&with_end_slash(&full_name(&folder, NameKind::Network)));
        put_text(app, true, make, into_focused_line);
        return None;
    }
    let Some(text) = insertion(app, key) else {
        return Some(dialog);
    };
    let Some(input) = focused_input(&mut dialog) else {
        return Some(dialog);
    };
    input.insert_str(&text);
    app.overlay = Some(Overlay::Dialog(dialog));
    None
}

fn into_focused_line(app: &mut App, text: String) {
    if let Some(Overlay::Dialog(dialog)) = &mut app.overlay
        && let Some(input) = focused_input(dialog)
    {
        input.insert_str(&text);
    }
}

/// Ctrl+Alt+[ / ] and Alt+Shift+[ / ]: the folder whose network name goes in.
pub(super) fn network_folder(app: &App, key: KeyEvent) -> Option<PathBuf> {
    let KeyCode::Char(bracket @ ('[' | ']')) = key.code else {
        return None;
    };
    let (source, network) = folder_source(bracket, key.modifiers)?;
    let side = side_of(app, source);
    let folder = app.panels[side.index()]
        .path
        .as_local()
        .map(Path::to_path_buf);
    folder.filter(|_| network)
}

/// The text a key inserts at once, `None` when it is not one of these keys (or there is nothing to
/// insert); the network names are [`insert`]'s.
pub(super) fn insertion(app: &App, key: KeyEvent) -> Option<String> {
    let shift = KeyModifiers::SHIFT;
    let ctrl = KeyModifiers::CONTROL;
    let text = match (key.code, key.modifiers) {
        (KeyCode::Char(bracket @ ('[' | ']')), modifiers) => {
            let (source, network) = folder_source(bracket, modifiers)?;
            let panel = &app.panels[side_of(app, source).index()];
            // A disk's network name is asked on a worker (`network_folder`); a provider's panel has its
            // own path either way (FAR's `MakePath` on a plugin's panel).
            if network && panel.path.as_local().is_some() {
                return None;
            }
            panel.folder_text()
        }
        (KeyCode::Enter, m) if m == shift => current_name(app, true)?,
        (KeyCode::Enter, m) if m == ctrl | shift => current_name(app, false)?,
        _ => return None,
    };
    Some(quote_spaces(&text))
}

/// The name under the cursor (FAR's `GetCurName`); on ".." the panel's folder.
fn current_name(app: &App, active: bool) -> Option<String> {
    let panel = if active {
        app.active_panel()
    } else {
        app.passive_panel()
    };
    let entry = panel.current()?;
    Some(if entry.is_parent {
        panel.folder_text()
    } else {
        entry.name.clone()
    })
}

/// FAR's `QuoteSpace`.
pub(super) fn quote_spaces(text: &str) -> String {
    if text.contains(' ') {
        format!("\"{text}\"")
    } else {
        text.to_string()
    }
}

/// The edit line that has the focus in a dialog, if one has.
pub(super) fn focused_input(dialog: &mut Dialog) -> Option<&mut LineInput> {
    match dialog {
        Dialog::Input { input, .. } => Some(input),
        Dialog::Form { form, .. } => {
            let focus = form.focus;
            match form.items.get_mut(focus).map(|item| &mut item.control) {
                Some(Control::Edit(input)) => Some(input),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::dialog::InputAction;

    #[test]
    fn brackets_put_panel_folders_and_shift_enter_the_names_into_the_focused_line() {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::Builder::new()
            .prefix("with space")
            .tempdir()
            .unwrap();
        std::fs::write(left.path().join("a.txt"), "x").unwrap();
        let mut app = App::new(left.path(), 100, 40, &crate::settings::Settings::default());
        app.panels[1]
            .change_dir(&crate::vfs::VfsPath::local(right.path()), None)
            .unwrap();
        app.active_panel_mut().cursor = 1;
        let key = |code, modifiers| KeyEvent::new(code, modifiers);
        let ctrl = KeyModifiers::CONTROL;

        let left_folder = with_end_slash(left.path());
        assert_eq!(
            insertion(&app, key(KeyCode::Char('['), ctrl)),
            Some(left_folder)
        );
        let right_folder = insertion(&app, key(KeyCode::Char(']'), ctrl)).unwrap();
        assert!(right_folder.starts_with('"') && right_folder.ends_with("\\\""));
        assert_eq!(
            insertion(&app, key(KeyCode::Char(']'), ctrl | KeyModifiers::SHIFT)),
            Some(right_folder),
            "the passive panel is the right one"
        );
        assert_eq!(
            insertion(&app, key(KeyCode::Enter, KeyModifiers::SHIFT)).as_deref(),
            Some("a.txt")
        );
        assert_eq!(insertion(&app, key(KeyCode::Char('a'), ctrl)), None);

        let mut dialog = Dialog::Input {
            title: String::new(),
            prompt: String::new(),
            input: LineInput::with_text("old"),
            action: InputAction::ApplyCommand,
        };
        assert!(focused_input(&mut dialog).is_some());
        assert!(focused_input(&mut Dialog::message("x", Vec::new())).is_none());
    }

    #[test]
    fn a_network_name_comes_into_the_dialogs_line_once_it_is_known() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::new(temp.path(), 100, 40, &crate::settings::Settings::default());
        app.overlay = Some(Overlay::Dialog(Dialog::Input {
            title: String::new(),
            prompt: String::new(),
            input: LineInput::default(),
            action: InputAction::ApplyCommand,
        }));
        let alt_ctrl = KeyModifiers::ALT | KeyModifiers::CONTROL;
        app.handle_key(KeyEvent::new(KeyCode::Char('['), alt_ctrl));
        crate::app::waiting::finish_waiting(&mut app);
        let Some(Overlay::Dialog(Dialog::Input { input, .. })) = &app.overlay else {
            panic!("the dialog stays");
        };
        // A local folder's network name is the folder itself.
        assert_eq!(input.text(), quote_spaces(&with_end_slash(temp.path())));
    }
}
