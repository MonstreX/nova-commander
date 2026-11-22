//! The tree of the copy, move and link dialogs (FAR's `copy.cpp` `DM_CALLTREE`): F10 or the "F10-Tree"
//! button opens "Find folder" on the active panel's folder (`MODALTREE_ACTIVE`), Alt+F10 on the passive
//! one's (`MODALTREE_PASSIVE`, the active one's when it is hidden), Shift+F10 on the first folder of the
//! target line (`MODALTREE_FREE`; one written without its folder taken where the operation would put
//! it). The dialog waits under the window; the folder chosen goes to its target line with a slash at
//! its end, added to what is there after a `;` when several destinations are on (quoted when it has `,`
//! or `;`), and the line takes the focus. Nothing with the tree turned off.

use std::path::{MAIN_SEPARATOR, Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::dialog::{Dialog, FormAction};
use super::super::form::{Control, Form};
use super::super::transfer_dialog::{
    LINK_MULTIPLE_TARGETS, MULTIPLE_TARGETS, Operation, TARGET, split_targets, unquote,
};
use super::super::{App, Overlay};
use super::Purpose;

/// Which panel's folder the window opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeMode {
    /// F10 and the button.
    Active,
    /// Alt+F10.
    Passive,
    /// Shift+F10: the target line's.
    Free,
}

impl TreeMode {
    /// The mode of a key in the dialog (FAR matches them exactly).
    fn of(key: KeyEvent) -> Option<Self> {
        if key.code != KeyCode::F(10) {
            return None;
        }
        match key.modifiers {
            KeyModifiers::NONE => Some(Self::Active),
            KeyModifiers::ALT => Some(Self::Passive),
            KeyModifiers::SHIFT => Some(Self::Free),
            _ => None,
        }
    }
}

/// "Process multiple destinations" of `operation`'s dialog.
fn multiple_targets(operation: Operation) -> usize {
    if operation == Operation::Link {
        LINK_MULTIPLE_TARGETS
    } else {
        MULTIPLE_TARGETS
    }
}

fn several(form: &Form, operation: Operation) -> bool {
    let index = multiple_targets(operation);
    form.is_checked(index) && !form.items[index].disabled
}

/// A drop-down list or an edit line's history open takes the keys itself (FAR's modal `VMenu`).
pub(in crate::app) fn list_open(form: &Form) -> bool {
    let drop_down = (form.items.iter())
        .any(|item| matches!(item.control, Control::DropDown { open: Some(_), .. }));
    drop_down || form.open_history().is_some()
}

/// The folder chosen into the target line (FAR's `DM_CALLTREE` after the tree).
pub(super) fn put_target(dialog: &mut Dialog, folder: &Path) {
    let Dialog::Form {
        form,
        action: FormAction::Transfer { operation, .. },
    } = dialog
    else {
        return;
    };
    let mut target = folder.display().to_string();
    if !target.ends_with(MAIN_SEPARATOR) {
        target.push(MAIN_SEPARATOR);
    }
    if several(form, *operation) {
        if target.contains([',', ';']) {
            target = format!("\"{target}\"");
        }
        let old = form.edit_text(TARGET);
        if !old.is_empty() {
            target = format!("{old};{target}");
        }
    }
    form.set_edit_text(TARGET, &target);
    form.focus_item(TARGET);
}

impl App {
    /// F10, Alt+F10 or Shift+F10 in the copy, move or link dialog: the window opened over it (none
    /// returned); any other key, or a dialog of another kind, gives the dialog back.
    pub(in crate::app) fn transfer_tree_key(
        &mut self,
        dialog: Dialog,
        key: KeyEvent,
    ) -> Option<Dialog> {
        let Some(mode) = TreeMode::of(key) else {
            return Some(dialog);
        };
        match &dialog {
            Dialog::Form {
                form,
                action: FormAction::Transfer { .. },
            } if !self.prefs.tree.turn_off_completely && !list_open(form) => {}
            _ => return Some(dialog),
        }
        self.open_target_tree(dialog, mode);
        None
    }

    /// The window over the dialog on the folder of `mode`; the dialog back as it was when there is
    /// none (a panel off the disks, an empty line, a disk that is not there).
    pub(in crate::app) fn open_target_tree(&mut self, dialog: Dialog, mode: TreeMode) {
        let folder = self.target_tree_folder(&dialog, mode);
        self.overlay = Some(Overlay::Dialog(dialog));
        let Some(folder) = folder else {
            return;
        };
        // Its disk asked off the screen's thread, the dialog under the wait (ADR-0019, review R22).
        self.wait_for(Vec::new(), move || {
            let there = super::root_of(&folder).is_dir();
            Box::new(move |app: &mut App| {
                if !there || !matches!(app.overlay, Some(Overlay::Dialog(_))) {
                    return;
                }
                if let Some(Overlay::Dialog(dialog)) = app.overlay.take() {
                    app.open_folder_tree_at(folder, Purpose::Target(Box::new(dialog)));
                }
            })
        });
        self.label_waiting("Reading the folder tree…");
    }

    fn target_tree_folder(&self, dialog: &Dialog, mode: TreeMode) -> Option<PathBuf> {
        match mode {
            TreeMode::Active => self.folder_of(self.active),
            TreeMode::Passive => {
                let passive = self.active.other();
                let side = if self.panels[passive.index()].visible {
                    passive
                } else {
                    self.active
                };
                self.folder_of(side)
            }
            TreeMode::Free => {
                let Dialog::Form {
                    form,
                    action:
                        FormAction::Transfer {
                            sources, operation, ..
                        },
                } = dialog
                else {
                    return None;
                };
                let text = form.edit_text(TARGET);
                let first = if several(form, *operation) {
                    split_targets(text).into_iter().next()?
                } else {
                    unquote(text)
                };
                if first.is_empty() {
                    return None;
                }
                let folder = self.operation_base(sources).resolve_exact(&first);
                folder.as_local().map(Path::to_path_buf)
            }
        }
    }
}

#[cfg(test)]
#[path = "target_tree_tests.rs"]
mod tests;
