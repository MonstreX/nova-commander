//! The viewer's byte editing (ADR-0027, Volkov Commander's hex editing): F2 in hex turns it on - the
//! hex cursor on the first row's first byte, typing over bytes, Tab between the hex and the text
//! columns - and F2 again writes the bytes typed into the file in place on a worker (the file keeps its
//! size: only those bytes are written). Esc, F3 and F10 leave it, asking first when bytes are typed.
//! The modes stay while it is on.

use std::collections::BTreeMap;
use std::path::Path;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Viewer, ViewerAction};
use crate::app::dialog::{ConfirmAction, Dialog};
use crate::app::{App, Screen};
use crate::editor::view::hex_cursor::HexMove;
use crate::editor::view::{Mode, Move};
use crate::vfs::local::ops::write_bytes_in_place as write_bytes;

/// The cursor's move of a key (none: not one).
fn byte_move(key: KeyEvent) -> Option<HexMove> {
    let ctrl = key.modifiers == KeyModifiers::CONTROL;
    if !key.modifiers.is_empty() && !ctrl {
        return None;
    }
    let moved = match (key.code, ctrl) {
        (KeyCode::Up, false) => HexMove::Up,
        (KeyCode::Down, false) => HexMove::Down,
        (KeyCode::Left | KeyCode::Backspace, false) => HexMove::Left,
        (KeyCode::Right, false) => HexMove::Right,
        (KeyCode::Home, false) => HexMove::Home,
        (KeyCode::End, false) => HexMove::End,
        (KeyCode::PageUp, false) => HexMove::PageUp,
        (KeyCode::PageDown, false) => HexMove::PageDown,
        (KeyCode::Home | KeyCode::PageUp, true) => HexMove::Start,
        (KeyCode::End | KeyCode::PageDown, true) => HexMove::Finish,
        _ => return None,
    };
    Some(moved)
}

/// Keys that change the mode or the rows (F2, F4 with any modifier, the rows' shifts with
/// Left / Right) or leave for the editor (F6, which would drop the bytes typed: review C4): nothing
/// while the editing is on.
fn held_still(key: KeyEvent) -> bool {
    matches!(
        key.code,
        KeyCode::F(2 | 4 | 6) | KeyCode::Left | KeyCode::Right
    )
}

impl Viewer {
    /// The bytes typed and not written (none: not editing).
    pub fn typed_bytes(&self) -> Option<&BTreeMap<u64, u8>> {
        Some(&self.text.view()?.byte_edit()?.changed)
    }

    /// The byte editing off, the bytes typed let go.
    pub(super) fn stop_editing(&mut self) {
        if let Some(view) = self.text.view_mut() {
            view.stop_editing();
        }
    }

    /// F2's label in hex: "Edit", "Save" while editing.
    pub(super) fn f2_hex_label(&self) -> &'static str {
        match self.typed_bytes() {
            Some(_) => "Save",
            None if self.copy => "",
            None => "Edit",
        }
    }

    /// A key of the byte editing (none: the viewer's own): F2 in hex turns it on; while it is on the
    /// moves, Tab and typing are its, F2 saves, Esc / F3 / F10 leave it.
    pub(super) fn byte_key(&mut self, key: KeyEvent) -> Option<ViewerAction> {
        let plain = key.modifiers.is_empty();
        let Some(changed) = self.typed_bytes().map(|changed| !changed.is_empty()) else {
            let hex = self.text.mode() == Mode::Hex;
            if plain && hex && key.code == KeyCode::F(2) && !self.copy {
                self.text.view_mut().map(|view| view.edit_bytes());
                return Some(ViewerAction::None);
            }
            return None;
        };
        let step = match key.code {
            KeyCode::F(2) if plain => return Some(ViewerAction::SaveBytes),
            KeyCode::Esc | KeyCode::F(3) | KeyCode::F(10) if plain => {
                if changed {
                    return Some(ViewerAction::AskBytes);
                }
                self.stop_editing();
                return Some(ViewerAction::None);
            }
            KeyCode::Tab if plain => Move::ByteColumn,
            KeyCode::Char(ch) if super::super::editing::is_text_input(key.modifiers) => {
                Move::TypeByte(ch)
            }
            _ => match byte_move(key) {
                Some(moved) => Move::Byte(moved),
                None if held_still(key) => return Some(ViewerAction::None),
                None => return None,
            },
        };
        self.text.press(step);
        Some(ViewerAction::None)
    }
}

impl App {
    /// F2 while editing: the bytes typed written into the file on a worker (Esc stops it); the editing
    /// left after when `stop`.
    pub(in crate::app) fn save_viewer_bytes(&mut self, stop: bool) {
        let Screen::Viewer(viewer) = &mut self.screen else {
            return;
        };
        let changed = viewer.typed_bytes().cloned().unwrap_or_default();
        if changed.is_empty() {
            if stop {
                viewer.stop_editing();
            }
            return;
        }
        let path = viewer.path.clone();
        let read = viewer.text.view().map(|view| view.len());
        let line = format!("Saving the file {}", path.display());
        self.wait_for_stoppable(vec![line], move || {
            let written = write_bytes(&path, &changed, read);
            Box::new(move |app: &mut App| match written {
                Ok(()) => app.viewer_bytes_written(&path, stop),
                Err(error) => app.show_error("Viewer", &error),
            })
        });
    }

    /// The bytes written: the viewer's blocks show them, the editing goes on (left when `stop`).
    fn viewer_bytes_written(&mut self, path: &Path, stop: bool) {
        let Screen::Viewer(viewer) = &mut self.screen else {
            return;
        };
        if viewer.path != path {
            return;
        }
        if let Some(view) = viewer.text.view_mut() {
            view.bytes_written();
        }
        if stop {
            viewer.stop_editing();
        }
    }

    /// Esc with bytes typed: save them, let them go, or go on.
    pub(in crate::app) fn ask_viewer_bytes(&mut self) {
        self.show_dialog(Dialog::Confirm {
            title: "Viewer".to_string(),
            lines: vec!["The bytes have been modified. Save?".to_string()],
            buttons: vec!["&Yes", "&No", "&Cancel"],
            focused: 0,
            danger: false,
            action: ConfirmAction::ViewerBytes,
        });
    }

    /// The question answered: Yes writes them and leaves the editing, No leaves it.
    pub(in crate::app) fn answer_viewer_bytes(&mut self, button: usize) {
        match button {
            0 => self.save_viewer_bytes(true),
            1 => {
                if let Screen::Viewer(viewer) = &mut self.screen {
                    viewer.stop_editing();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "viewer_bytes_tests.rs"]
mod tests;
