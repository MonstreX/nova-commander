//! What the editor asks of the app (S2d.4b): F2's save - the document streamed on a worker into a file
//! beside the original, then put in its place (ADR-0015 §4; FAR's "Saving the file" with Esc), the
//! file read again after it; Shift+F2's other name; Esc / F10 with FAR's "File has been modified.
//! Save?" (Yes, No, Cancel); Shift+F10; F6 to the viewer; the clipboard's block; Alt+F8's line.

use std::path::Path;

use super::dialog::{ConfirmAction, Dialog, FormAction};
use super::editing::{EditorAction, EditorScreen};
use super::editor_save::AfterSave;
use super::{App, Screen};
use crate::editor::view::edit_view::Step;
use crate::editor::view::pieces::PieceTable;
use crate::settings::history::Kind;
use crate::vfs::RandomRead;

/// A file with nothing in it (the original of a file made new).
pub(super) struct Empty;

impl RandomRead for Empty {
    fn read_at(&mut self, _: u64, _: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }

    fn len(&mut self) -> std::io::Result<u64> {
        Ok(0)
    }
}

impl App {
    /// The editor on `path` at `line` and `column` (1-based; 0: the start), opened at once; FAR's
    /// question first for a read-only file with "Warn when opening read-only files".
    pub(super) fn open_editor_at(&mut self, path: &Path, remember: bool, place: (usize, usize)) {
        if self.editor_settings.read_only_warn && is_read_only(path) {
            self.show_dialog(Dialog::Confirm {
                title: "Editor".to_string(),
                lines: vec![
                    path.display().to_string(),
                    "is a read-only file".to_string(),
                    "Do you wish to edit it?".to_string(),
                ],
                buttons: vec!["&Yes", "&No"],
                focused: 0,
                danger: true,
                action: ConfirmAction::EditReadOnly {
                    path: path.to_path_buf(),
                    remember,
                    place,
                },
            });
            return;
        }
        self.open_editor_unasked(path, remember, place);
    }

    /// The editor on `path` with no question: in lock mode for a read-only file with "Lock editing of
    /// read-only files" (FAR's `FileEditor::LoadFile`).
    pub(super) fn open_editor_unasked(
        &mut self,
        path: &Path,
        remember: bool,
        (line, column): (usize, usize),
    ) {
        let mut screen = EditorScreen::open(path);
        screen.settings = self.editor_settings.clone();
        screen.text.apply_settings(&screen.settings);
        if screen.settings.read_only_lock && is_read_only(path) {
            screen.text.set_locked(true);
        }
        let kept = self.kept_editor_position(path);
        if line > 0 || column > 0 {
            let line = u64::try_from(line.saturating_sub(1)).unwrap_or(u64::MAX);
            screen
                .text
                .press(Step::GoTo(line, column.saturating_sub(1)));
        } else if let Some(place) = kept.as_ref().filter(|_| self.editor_settings.save_position) {
            screen.text.restore(place);
        }
        if let Some(place) = &kept {
            screen.text.restore_marks(&place.bookmarks);
        }
        self.show_window(Screen::Editor(Box::new(screen)));
        if remember {
            self.remember_file(path, Kind::Edit);
        }
    }

    /// Whether the cursor is FAR's big one: the editor's overtype (Ins) with nothing in front of it.
    pub fn block_cursor(&self) -> bool {
        match &self.screen {
            Screen::Editor(editor) if self.overlay.is_none() => {
                editor.text.view().is_some_and(|view| view.overtype)
            }
            _ => false,
        }
    }

    /// The editor on a file not there yet (made by its first save).
    pub(super) fn open_new_file(&mut self, path: &Path) {
        self.show_window(Screen::Editor(Box::new(EditorScreen {
            path: path.to_path_buf(),
            text: super::editor_text::EditorText::new_file(),
            pending: None,
            menu_behind: None,
            settings: self.editor_settings.clone(),
            quoting: false,
            plugin_bars: super::editing::PluginBars::default(),
            saving: super::editor_save::Saving::default(),
            saving_told: None,
        })));
    }

    /// The editor's reader: what it brought; an action asked ahead done once the steps before it are.
    pub(super) fn pump_editor(&mut self) -> bool {
        let Screen::Editor(editor) = &mut self.screen else {
            return false;
        };
        // Drawn again while it reads with nothing to show: its notice's time goes on.
        let changed = editor.text.pump() | editor.text.reading_for().is_some();
        if let Some(told) = editor
            .saving_told
            .as_ref()
            .and_then(|told| told.try_recv().ok())
        {
            (editor.saving.stamp, editor.saving.identity) = (told.stamp, told.identity);
            editor.saving_told = None;
        }
        if editor.text.settled()
            && let Some(action) = editor.pending.take()
        {
            self.handle_editor_action(action);
            return true;
        }
        changed
    }

    /// A key in the editor in front: its action, then the plugins that hear the editor's keys - not
    /// when the key closed it and brought another window (review C8: they heard it as that one's).
    pub(super) fn editor_key(&mut self, key: ratatui::crossterm::event::KeyEvent) {
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        let action = editor.handle_key(key);
        let window = self.front_window();
        self.handle_editor_action(action);
        if self.front_window() == window {
            self.editor_key_to_plugins(key);
        }
    }

    pub(super) fn handle_editor_action(&mut self, action: EditorAction) {
        let waits = !matches!(action, EditorAction::None);
        if let Screen::Editor(editor) = &mut self.screen
            && waits
            && !editor.text.settled()
        {
            // Closing never waits for a file that cannot be read, nor a second time for one whose
            // blocks do not come (R7 review F1: Esc and F10 waited for ever, the editor could not be
            // left).
            let closing = matches!(action, EditorAction::Close);
            let unread = editor.text.failed.is_some();
            let again = matches!(editor.pending, Some(EditorAction::Close));
            if !(closing && (unread || again)) {
                editor.pending = Some(action);
                return;
            }
            editor.pending = None;
        }
        match action {
            EditorAction::None => {}
            EditorAction::Save => self.save_editor((None, None), AfterSave::Stay),
            EditorAction::SaveAndClose => self.save_editor((None, None), AfterSave::Close),
            EditorAction::SaveAs => self.ask_save_as(),
            EditorAction::EditNew => self.ask_edit_new(),
            EditorAction::Close => self.close_editor(),
            EditorAction::View => self.editor_to_viewer(),
            EditorAction::GoTo => self.show_editor_go_to(),
            EditorAction::Copy { cut } => self.copy_block(cut),
            EditorAction::Paste => self.paste_text(),
            EditorAction::AppendCopy => self.append_block(),
            EditorAction::Search => self.show_editor_search(),
            EditorAction::Complete => self.show_completion(),
            EditorAction::Colorer(action) => self.colorer_action(action),
            EditorAction::SearchOn(backward) => self.editor_search_from(backward, true),
            EditorAction::Replace => {
                // FAR's Ctrl+F7 does nothing in lock mode.
                if !matches!(&self.screen, Screen::Editor(editor) if editor.text.is_locked()) {
                    self.show_editor_replace();
                }
            }
            EditorAction::ReplaceFrom { pos, backward, all } => match all {
                true => self.replace_all_from(pos, backward),
                false => self.replace_from(pos, backward, false),
            },
            EditorAction::NextPage => self.editor_next_page(),
            EditorAction::GoToFile => {
                if let Screen::Editor(editor) = &self.screen {
                    let path = editor.path.clone();
                    self.panel_to_file(&path);
                }
            }
            EditorAction::LineNumbers | EditorAction::Lock | EditorAction::Settings => {
                self.editor_own_action(&action);
            }
            EditorAction::InsertPath(_) | EditorAction::InsertFileName => {
                self.editor_insert(&action);
            }
            EditorAction::Plugins => self.open_editor_plugin_commands(),
            EditorAction::PageMenu => {
                let shown = match &self.screen {
                    Screen::Editor(editor) => editor.text.page_number(),
                    _ => None,
                };
                self.show_code_page_menu(shown);
            }
        }
    }

    /// This editor's own options: Ctrl+F3's line numbers, Ctrl+L's lock mode (FAR's editor `EdOpt` and
    /// flags), Alt+Shift+F9's local settings.
    fn editor_own_action(&mut self, action: &EditorAction) {
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        match action {
            EditorAction::LineNumbers => {
                editor.settings.show_line_numbers = !editor.settings.show_line_numbers;
                editor.text.numbers = editor.settings.show_line_numbers;
            }
            EditorAction::Lock => {
                let locked = !editor.text.is_locked();
                editor.text.set_locked(locked);
            }
            _ => self.ask_options(super::options::OptionsPage::EditorLocal),
        }
    }

    /// F8: OEM after ANSI, ANSI after any other; an edited document keeps its page.
    fn editor_next_page(&mut self) {
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        let ansi = crate::platform::locale::ansi_code_page();
        let next = match editor.text.page_number() {
            Some(page) if page == ansi => crate::platform::locale::oem_code_page(),
            _ => ansi,
        };
        if !editor.text.set_page(Some(next)) {
            self.editor_page_refused();
        }
    }

    /// The editor's place kept for `path` (FAR's `SaveEditorPos`), its bookmarks only with
    /// `SaveEditorShortPos`.
    fn kept_editor_position(
        &self,
        path: &Path,
    ) -> Option<crate::settings::positions::EditorPosition> {
        let store = self.positions.as_ref()?;
        let settings = &self.editor_settings;
        if !settings.save_position && !settings.save_bookmarks {
            return None;
        }
        let file = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let editor = crate::settings::positions::editor_positions_path(store);
        let mut place: crate::settings::positions::EditorPosition =
            crate::settings::positions::position(&editor, &file)?;
        if !settings.save_bookmarks {
            place.bookmarks.clear();
        }
        Some(place)
    }

    /// The editor's file's place kept as it is left (FAR's `FileEditor::SaveToCache`).
    pub(super) fn keep_editor_position(&self) {
        let (Screen::Editor(screen), Some(store)) = (&self.screen, &self.positions) else {
            return;
        };
        let Some(place) = screen.text.position() else {
            return;
        };
        if !self.editor_settings.save_position && !self.editor_settings.save_bookmarks {
            return;
        }
        let file = std::path::absolute(&screen.path).unwrap_or_else(|_| screen.path.clone());
        let editor = crate::settings::positions::editor_positions_path(store);
        // A place not kept is no failure the user has to hear of (FAR says nothing either).
        let _ = crate::settings::positions::keep_position(&editor, &file, place);
    }

    /// Ctrl+F10 in the viewer and the editor (FAR's `FilePanels::GoToFile`): the file panel in the
    /// file's folder, the cursor on it (the folder read anew first when it is there: a file just made
    /// shows), behind the viewer or the editor left open.
    pub(super) fn panel_to_file(&mut self, path: &Path) {
        let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
            return;
        };
        let (folder, name) = (crate::vfs::VfsPath::local(folder), name.to_string_lossy());
        let side = self.command_side();
        let panel = &mut self.panels[side.index()];
        if panel.path.same_place(&folder) {
            panel.visible = true;
            panel.reload_onto(&name);
            self.active = side;
            return;
        }
        self.go_to_found(&folder, Some(&name));
    }

    /// The editor left for the panels, its place kept.
    fn leave_editor(&mut self) {
        self.keep_editor_position();
        self.close_window();
        self.reread_panels();
    }

    /// Esc / F10: closed; FAR's question when it was edited.
    pub(super) fn close_editor(&mut self) {
        // Edits not done yet (their blocks not come) count as changes: asked about, never let go
        // unasked.
        let changed = matches!(&self.screen, Screen::Editor(screen)
            if screen.text.is_changed() || screen.text.has_steps_waiting());
        match changed {
            true => self.show_dialog(confirm_save(ConfirmAction::CloseEditor)),
            false => self.leave_editor(),
        }
    }

    /// FAR's question answered: Yes saves and closes, No closes.
    pub(super) fn answer_close_editor(&mut self, button: usize) {
        match button {
            0 => self.save_editor((None, None), AfterSave::Close),
            1 => self.leave_editor(),
            _ => {}
        }
    }

    /// F6: the viewer on the file; FAR's "Save?" first when it was edited (R7 review D6: nothing
    /// was done), a file not its own yet (`FFILEEDIT_EPHEMERAL`) too.
    fn editor_to_viewer(&mut self) {
        let Screen::Editor(screen) = &self.screen else {
            return;
        };
        // FAR's F6 needs `FFILEEDIT_ENABLEF6`: not the user menu's editor or a plugin's file's
        // (review C6; Find file's editor has it).
        if self.user_menu.reread.is_some() || self.edits_extracted(&screen.path) {
            return;
        }
        let unsaved = screen.text.is_changed() || screen.text.has_steps_waiting();
        if unsaved || screen.saving.ephemeral {
            return self.show_dialog(confirm_save(ConfirmAction::EditorToViewer));
        }
        self.switch_to_viewer();
    }

    /// F6's question answered: Yes saves, then views; No views, the edits let go (FAR's
    /// `TextChanged(false)`).
    pub(super) fn answer_editor_to_viewer(&mut self, button: usize) {
        match button {
            0 => self.save_editor((None, None), AfterSave::View),
            1 => self.switch_to_viewer(),
            _ => {}
        }
    }

    /// The editor's window let go for the viewer on its file, from its first row.
    fn switch_to_viewer(&mut self) {
        let Screen::Editor(screen) = &self.screen else {
            return;
        };
        let path = screen.path.clone();
        // FAR's viewer from the editor shows from its first row (`GetCurPos`), in its code page.
        let place = screen.text.position().map(|place| (place.top, place.page));
        self.keep_editor_position();
        self.close_window();
        self.view_file(&path, true);
        if let (Screen::Viewer(viewer), Some((top, page))) = (&mut self.screen, place) {
            viewer.start_at(top, page);
        }
    }

    /// Saved (`editor_save`): the editor on the saved file at the same place, its undo kept (`moved`,
    /// FAR's undo over a save; none: read anew), its own settings and lock, the file's stamp taken
    /// again; the edited copy of a plugin's file put back; closed when asked.
    pub(super) fn editor_saved(&mut self, path: &Path, moved: Option<PieceTable>, then: AfterSave) {
        self.put_on_save(path);
        self.keep_saved_file(path);
        match then {
            AfterSave::Close => return self.leave_editor(),
            AfterSave::View => return self.switch_to_viewer(),
            AfterSave::Stay => {}
        }
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        let place = editor.text.position();
        let hex = (editor.text.view()).and_then(|view| view.hex_cursor());
        let file = crate::vfs::VfsPath::local(path);
        let mut text = match moved {
            Some(doc) => super::editor_text::EditorText::open_saved(file, doc),
            None => super::editor_text::EditorText::open(file),
        };
        text.apply_settings(&editor.settings);
        text.set_locked(editor.text.is_locked());
        if let Some(place) = &place {
            text.restore(place);
        }
        // The hex mode kept on its byte (ADR-0027).
        if let Some(cursor) = hex {
            text.press(crate::editor::view::edit_view::Step::HexAt(
                cursor.pos,
                cursor.text,
            ));
        }
        editor.text = text;
        editor.path = path.to_path_buf();
        editor.saving = super::editor_save::Saving::default();
        editor.saving_told = super::editor_save::Saving::later(path);
    }

    /// Ctrl+C / Ctrl+Ins (Ctrl+X / Shift+Del: and out): the block's text to the clipboard; with no
    /// block Ctrl+C / Ctrl+Ins mark the cursor's line and copy it (FAR's `KEY_CTRLC`).
    fn copy_block(&mut self, cut: bool) {
        let Screen::Editor(screen) = &mut self.screen else {
            return;
        };
        let no_block = matches!(screen.text.view().map(|view| view.block()), Some(Ok(None)));
        if no_block && !cut && !screen.text.view().is_some_and(|view| view.is_column()) {
            screen.text.press(Step::MarkLine);
        }
        let Some(text) = self.block_for_clipboard(EditorAction::Copy { cut }) else {
            return;
        };
        let Screen::Editor(screen) = &mut self.screen else {
            return;
        };
        let column = screen.text.view().is_some_and(|view| view.is_column());
        if cut {
            screen.text.press(Step::Cut);
        }
        if let Err(error) = self.clipboard_put(&text, column) {
            self.show_error("Editor", &error);
        }
    }

    /// The block's text for `action`; none yet while its blocks are read (`action` done again once
    /// they come), none without a block, none (said) when it is too large to be read so.
    fn block_for_clipboard(&mut self, action: EditorAction) -> Option<String> {
        let Screen::Editor(screen) = &mut self.screen else {
            return None;
        };
        match screen.text.block_text_now() {
            super::editor_text::BlockText::Text(text) => text,
            super::editor_text::BlockText::Reading => {
                screen.pending = Some(action);
                None
            }
            super::editor_text::BlockText::TooLarge => {
                let lines = vec![
                    "The block is too large to be copied".to_string(),
                    "from a file this big".to_string(),
                ];
                self.show_dialog(Dialog::error("Editor", lines));
                None
            }
        }
    }

    /// The editor's block's text on the clipboard, a vertical block's marked as one.
    fn clipboard_put(&mut self, text: &str, column: bool) -> std::io::Result<()> {
        match column {
            true => self.clipboard.set_column_text(text),
            false => self.clipboard.set_text(text),
        }
    }

    /// Ctrl+Gray +: the block's text after the clipboard's (FAR's `Copy(TRUE)`; a vertical block's
    /// `VCopy(TRUE)`, the whole a vertical block).
    fn append_block(&mut self) {
        let Some(text) = self.block_for_clipboard(EditorAction::AppendCopy) else {
            return;
        };
        let Screen::Editor(screen) = &self.screen else {
            return;
        };
        let column = screen.text.view().is_some_and(|view| view.is_column());
        let before = self.clipboard.text().ok().flatten().unwrap_or_default();
        if let Err(error) = self.clipboard_put(&format!("{before}{text}"), column) {
            self.show_error("Editor", &error);
        }
    }

    /// Ctrl+V / Shift+Ins: the clipboard's text at the cursor (a vertical block's as one, FAR's
    /// `PasteFromClipboard`).
    fn paste_text(&mut self) {
        let text = match self.clipboard.text() {
            Ok(Some(text)) => text,
            Ok(None) => return,
            Err(error) => return self.show_error("Editor", &error),
        };
        let step = match self.clipboard.is_column() {
            Ok(true) => Step::PasteColumn(text),
            _ => Step::Paste(text),
        };
        if let Screen::Editor(screen) = &mut self.screen {
            screen.text.press(step);
        }
    }

    /// Alt+F8: FAR's "Go to" (a line, `+` / `-` from the cursor's, a percentage of the lines; the
    /// column after ` .,;:`).
    fn show_editor_go_to(&mut self) {
        let form = super::viewer::goto_form(false);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::EditorGoTo,
        });
    }

    pub(super) fn finish_editor_go_to(&mut self, form: &super::form::Form, button: Option<usize>) {
        let Some((row, col)) = super::viewer::goto_answer(form, button) else {
            return;
        };
        let Screen::Editor(screen) = &mut self.screen else {
            return;
        };
        let (line, total, column, _) = screen.text.place().unwrap_or((1, 1, 1, 1));
        let target = match (row.exist, row.percent, row.relative) {
            (false, _, _) => line,
            (true, true, _) => (total * row.value.min(100) / 100).max(1),
            (true, false, 0) => row.value,
            (true, false, sign) if sign < 0 => line.saturating_sub(row.value),
            (true, false, _) => line.saturating_add(row.value),
        };
        let col = match (col.exist, col.relative) {
            (false, _) => column.saturating_sub(1),
            (true, 0) => usize::try_from(col.value).unwrap_or(1).saturating_sub(1),
            (true, sign) if sign < 0 => {
                column.saturating_sub(1 + usize::try_from(col.value).unwrap_or(0))
            }
            (true, _) => column - 1 + usize::try_from(col.value).unwrap_or(0),
        };
        screen.text.press(Step::GoTo(target.saturating_sub(1), col));
    }
}

/// FAR's `confirm_save`, a `MSG_WARNING` (R7 review D10), before closing or F6.
fn confirm_save(action: ConfirmAction) -> Dialog {
    Dialog::Confirm {
        title: "Editor".to_string(),
        lines: vec!["File has been modified. Save?".to_string()],
        buttons: vec!["&Yes", "&No", "&Cancel"],
        focused: 0,
        danger: true,
        action,
    }
}

/// Whether `path` has the read-only attribute (FAR's `get_read_only_attributes` without its hidden
/// and system bits, which only its config editor sets).
fn is_read_only(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.permissions().readonly())
}

#[cfg(test)]
#[path = "editor_actions_tests.rs"]
mod tests;
