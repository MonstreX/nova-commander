//! FAR's questions before the editor saves (`FileEditor::SaveFile`, `DoNotOverwrite`; review C1, C3):
//! Shift+F2 onto a file that is there "already exists. Do you wish to overwrite it?" (always for an
//! editor of a file not its own yet, FAR's `FFILEEDIT_EPHEMERAL`, whose F2 is Shift+F2); its own file
//! changed since it was read or saved "The file was changed by an external program. Save?" (Yes /
//! Save as / Cancel); a read-only one "is a read-only file. Do you wish to overwrite it?" (Yes takes
//! the attribute off first, as FAR's). Then the save itself on a worker, Esc stopping it.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::dialog::{ConfirmAction, Dialog};
use super::{App, Screen};

/// A file's time and size, as FAR compares them (`LastWriteTime`, `FileSize`).
pub type Stamp = (Option<SystemTime>, u64);

/// `path`'s stamp; none when it is not there. The name exactly, as every look at the file here: `a `
/// is never its sibling `a` (R7 review C4).
pub fn stamp_of(path: &Path) -> Option<Stamp> {
    std::fs::metadata(exact(path))
        .ok()
        .map(|meta| (meta.modified().ok(), meta.len()))
}

fn exact(path: &Path) -> PathBuf {
    crate::platform::files::exact_path(path)
}

/// The very file at `path` (its volume and index).
fn identity_of(path: &Path) -> Option<(u32, u64)> {
    crate::platform::files::file_identity(&exact(path)).ok()
}

/// What the editor knows of its file for saving.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Saving {
    /// The file's stamp when it was read or last saved.
    pub stamp: Option<Stamp>,
    /// And the very file it was (its volume and index): changed and still that one, another program
    /// rewrote it in place; changed and another one, it was put in its place.
    pub identity: Option<(u32, u64)>,
    /// FAR's `FFILEEDIT_EPHEMERAL`: a file not its own yet (the "All" list's new editor).
    pub ephemeral: bool,
    /// Shift+F2's conversion for the save under way (R7 review D5).
    pub convert: Option<super::editor_convert::Convert>,
}

impl Saving {
    pub fn of(path: &Path) -> Self {
        Self {
            stamp: stamp_of(path),
            identity: identity_of(path),
            ephemeral: false,
            convert: None,
        }
    }

    /// `path`'s stamp and identity looked at on a thread of their own, told through the receiver
    /// (R7 review F5: a share that stopped answering held the program as the editor opened); none when
    /// no thread could be had (nothing then is compared).
    pub fn later(path: &Path) -> Option<std::sync::mpsc::Receiver<Self>> {
        let (tell, told) = std::sync::mpsc::channel();
        let path = path.to_path_buf();
        let looking = std::thread::Builder::new().name("editor's file".to_string());
        looking
            .spawn(move || drop(tell.send(Self::of(&path))))
            .ok()?;
        Some(told)
    }

    /// Whether the file the document was read from (`source`, the editor's own) was rewritten in place
    /// by another program since: the bytes the document did not change are no longer on the disk.
    fn rewritten_in_place(&self, own: &Path, source: &Path) -> bool {
        let changed = self
            .stamp
            .is_some_and(|then| stamp_of(source) != Some(then));
        let same = self
            .identity
            .is_some_and(|then| identity_of(source) == Some(then));
        same_file(own, source) && changed && same
    }
}

/// A question asked before a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveQuestion {
    Overwrite,
    Changed,
    ReadOnly,
}

/// The questions answered Yes already.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Asked {
    pub overwrite: bool,
    pub changed: bool,
    pub read_only: bool,
}

/// What the editor does once saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterSave {
    Stay,
    Close,
    /// F6's "Save?" answered Yes: the viewer on the file (R7 review D6).
    View,
}

/// A save waiting for its answer: the file its bytes come from, where it goes, what the editor does
/// after it, what was answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSave {
    pub source: Option<PathBuf>,
    pub target: Option<PathBuf>,
    pub then: AfterSave,
    pub asked: Asked,
    pub question: SaveQuestion,
}

/// Whether two names are one file's (FAR's `equal_icase` of the full names).
fn same_file(one: &Path, other: &Path) -> bool {
    let full = |path: &Path| {
        let full = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        full.to_string_lossy().to_lowercase()
    };
    full(one) == full(other)
}

/// What a save looks at on the disk before it starts, taken from the editor: off the screen's thread
/// (R7 review F5: a share that stopped answering held the program at every F2).
struct Looks {
    path: PathBuf,
    own: PathBuf,
    source: PathBuf,
    saving: Saving,
    save_as: bool,
    asked: Asked,
}

impl Looks {
    /// The question FAR would ask before saving to `path` now, if any.
    fn question(&self) -> Option<SaveQuestion> {
        let (path, saving, asked) = (&self.path, &self.saving, self.asked);
        let now = stamp_of(path)?;
        let own = same_file(path, &self.own);
        if !asked.overwrite && self.save_as && (saving.ephemeral || !own) {
            return Some(SaveQuestion::Overwrite);
        }
        let changed = saving.stamp.is_some_and(|then| then != now);
        if !asked.changed && own && !saving.ephemeral && changed {
            return Some(SaveQuestion::Changed);
        }
        let read_only =
            std::fs::metadata(exact(path)).is_ok_and(|meta| meta.permissions().readonly());
        (!asked.read_only && read_only).then_some(SaveQuestion::ReadOnly)
    }
}

impl App {
    /// F2 (no `target`), Shift+F2's name, Shift+F10 and "Save" of Esc's question (`then` closes) or
    /// of F6's (`then` views).
    pub(super) fn save_editor(
        &mut self,
        targets: (Option<PathBuf>, Option<PathBuf>),
        then: AfterSave,
    ) {
        if let Screen::Editor(screen) = &mut self.screen {
            screen.saving.convert = None;
        }
        self.save_editor_asked(targets, then, Asked::default());
    }

    /// Shift+F2's "Save": to `targets`, converted as `convert` says (none: as it is).
    pub(super) fn save_editor_as(
        &mut self,
        targets: (Option<PathBuf>, Option<PathBuf>),
        convert: Option<super::editor_convert::Convert>,
    ) {
        if let Screen::Editor(screen) = &mut self.screen {
            screen.saving.convert = convert;
        }
        self.save_editor_asked(targets, AfterSave::Stay, Asked::default());
    }

    fn save_editor_asked(
        &mut self,
        (source, target): (Option<PathBuf>, Option<PathBuf>),
        then: AfterSave,
        asked: Asked,
    ) {
        let Screen::Editor(screen) = &self.screen else {
            return;
        };
        // FAR's F2 of a file not its own yet is Shift+F2.
        if target.is_none() && screen.saving.ephemeral {
            return self.ask_save_as();
        }
        let looks = Looks {
            path: target.clone().unwrap_or_else(|| screen.path.clone()),
            own: screen.path.clone(),
            source: source.clone().unwrap_or_else(|| screen.path.clone()),
            saving: screen.saving.clone(),
            save_as: target.is_some(),
            asked,
        };
        let editor = self.front_window();
        self.wait_to_start(None, move || {
            let question = looks.question();
            let in_place = (looks.saving).rewritten_in_place(&looks.own, &looks.source);
            Box::new(move |app: &mut App| {
                // The editor asked from no longer in front: nothing done.
                if app.front_window() != editor {
                    return;
                }
                if let Some(question) = question {
                    let pending = PendingSave {
                        source,
                        target,
                        then,
                        asked,
                        question,
                    };
                    return app.ask_save_question(&looks.path, pending);
                }
                app.save_editor_now((looks.source, looks.path), then, (asked, in_place));
            })
        });
        self.label_waiting("Looking at the file");
    }

    /// The save once its looks at the disk asked nothing: the document streamed into `path`, its bytes
    /// not edited from `source`.
    fn save_editor_now(
        &mut self,
        (source, path): (PathBuf, PathBuf),
        then: AfterSave,
        (asked, in_place): (Asked, bool),
    ) {
        let Screen::Editor(screen) = &self.screen else {
            return;
        };
        let Some(doc) = screen.text.document() else {
            return;
        };
        let held = screen.text.held_file();
        let convert = screen.saving.convert;
        let line = format!("Saving the file {}", path.display());
        let read_only = asked.read_only;
        // FAR's progress of the save (`MEditSaving` with its bar): the bytes written of the document's.
        let written = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let counted = std::sync::Arc::clone(&written);
        let total = doc.size();
        self.wait_for_stoppable(vec![line], move || {
            // FAR takes the read-only attribute off a file it was told to overwrite.
            let unlocked = match read_only {
                true => crate::platform::files::set_read_only(&exact(&path), false),
                false => Ok(()),
            };
            let saved = unlocked.and_then(|()| {
                let from = super::editor_save_file::Original {
                    path: &source,
                    held,
                    in_place,
                };
                super::editor_save_file::save_to(from, (&path, convert), &doc, &counted)
            });
            Box::new(move |app: &mut App| match saved {
                Ok(moved) => {
                    app.editor_saved(&path, moved, then);
                    // Converted: read again in the code page it was written in.
                    if let (Some(convert), Screen::Editor(editor)) = (convert, &mut app.screen) {
                        editor.text.kept_page = Some(convert.page);
                    }
                }
                Err(error) => app.show_error("Editor", &error),
            })
        });
        self.show_progress(written, total);
    }

    fn ask_save_question(&mut self, path: &Path, pending: PendingSave) {
        let name = path.display().to_string();
        let (lines, buttons) = match pending.question {
            SaveQuestion::Overwrite => (
                vec![
                    name,
                    "already exists".to_string(),
                    "Do you wish to overwrite it?".to_string(),
                ],
                vec!["&Yes", "&No"],
            ),
            SaveQuestion::Changed => (
                vec!["The file was changed by an external program. Save?".to_string()],
                vec!["&Yes", "Save &as", "&Cancel"],
            ),
            SaveQuestion::ReadOnly => (
                vec![
                    name,
                    "is a read-only file".to_string(),
                    "Do you wish to overwrite it?".to_string(),
                ],
                vec!["&Yes", "&No"],
            ),
        };
        self.show_dialog(Dialog::Confirm {
            title: "Editor".to_string(),
            lines,
            buttons,
            focused: 0,
            danger: true,
            action: ConfirmAction::EditorSave(Box::new(pending)),
        });
    }

    /// The answer: Yes goes on to the next question or the save; "Save as" asks for a name; the rest
    /// leaves the file unsaved (the editor stays).
    pub(super) fn answer_save_question(&mut self, mut pending: PendingSave, button: usize) {
        match (pending.question, button) {
            (SaveQuestion::Overwrite, 0) => pending.asked.overwrite = true,
            (SaveQuestion::Changed, 0) => pending.asked.changed = true,
            (SaveQuestion::Changed, 1) => return self.ask_save_as(),
            (SaveQuestion::ReadOnly, 0) => pending.asked.read_only = true,
            _ => return,
        }
        let targets = (pending.source, pending.target);
        self.save_editor_asked(targets, pending.then, pending.asked);
    }
}

#[cfg(test)]
#[path = "editor_save_tests.rs"]
mod tests;
