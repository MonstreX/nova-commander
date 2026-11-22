//! File operation flows (critical area): ask, confirm, run, report every failure. Copy, move and
//! delete run as jobs (`transfer`, ADR-0009; `delete`, FAR's `delete.cpp`); the safety rules live in
//! `jobs` and `vfs::local::ops`.

use std::path::{Path, PathBuf};

use super::App;
use super::dialog::{ConfirmAction, Dialog, FormAction};
use super::form::Form;
use super::make_dir_form;
use crate::jobs::transfer::{Report, Request};
use crate::jobs::tree_cache::{TreeCache, first_missing};
use crate::platform;
use crate::platform::files::NewLink;

/// How many lines of a report a dialog shows.
const REPORT_LINES: usize = 20;

impl App {
    /// A form's button pressed (`None`: Esc).
    pub(super) fn finish_form(&mut self, action: FormAction, form: &Form, button: Option<usize>) {
        match action {
            action @ FormAction::Transfer { .. } => self.finish_transfer_form(action, form, button),
            FormAction::Answer {
                task,
                replies,
                remember,
            } => {
                self.answer_transfer(task, form, &replies, remember, button);
            }
            FormAction::RenameAnswer { task, target } => {
                self.finish_rename(form, button, (task, target));
            }
            FormAction::Delete { request, yes } => {
                // The question's count goes on in the deletion, or stops with the question.
                let counter = self.take_delete_count();
                if button == Some(yes) {
                    self.start_delete(request, counter);
                }
            }
            FormAction::AnswerDelete { task, replies } => {
                self.answer_delete(task, form, &replies, button);
            }
            FormAction::Attributes(session) => self.finish_attributes(*session, form, button),
            FormAction::AttributesMore { session, main } => {
                self.finish_more(*session, *main, form, button);
            }
            FormAction::AnswerAttributes { task } => self.answer_attributes(task, form, button),
            FormAction::Options(page) => self.finish_options(form, button, page),
            FormAction::Plugin(window) => self.finish_plugin_form(window, form, button),
            FormAction::PluginHotkey(edit) => self.finish_plugin_hotkey(*edit, form, button),
            FormAction::PluginInfo(menu, selected, side) => {
                self.plugin_info_closed(menu, selected, side);
            }
            FormAction::UserShell(place) => self.finish_user_shell(form, button, place),
            FormAction::MenuItem(edit) => self.finish_menu_item(form, button, *edit),
            FormAction::MenuInput(expanded) => self.finish_menu_input(form, button, &expanded),
            FormAction::Association { at, insert } => {
                self.finish_association(form, button, (at, insert));
            }
            FormAction::MaskGroup(at) => self.finish_mask_group(form, button, at),
            FormAction::FindMaskGroup => self.finish_find_mask_group(form, button),
            FormAction::PanelMode(edit) => self.finish_panel_mode(form, button, edit),
            action @ (FormAction::CodePageName(_)
            | FormAction::SyntaxHotkey(_)
            | FormAction::ViewerGoTo
            | FormAction::EditorGoTo
            | FormAction::EditorSearch
            | FormAction::EditorReplace
            | FormAction::EditorReplaceAsk { .. }
            | FormAction::EditorSearchWrap { .. }
            | FormAction::ViewerSearch(_)
            | FormAction::ViewerSearchWrap { .. }
            | FormAction::EditNew
            | FormAction::EditorSaveAs) => self.finish_text_form(action, form, button),
            FormAction::FindFile => self.finish_find_form(form, button),
            FormAction::MakeDir(tree) => self.finish_make_dir_form(form, button, tree),
            FormAction::FindAdvanced => self.finish_find_advanced(form, button),
            FormAction::Filter(edit) => self.finish_filter_form(*edit, form, button),
            FormAction::FilterAttributes { edit, main } => {
                self.finish_filter_attributes(*edit, *main, form, button);
            }
            FormAction::FolderShortcut { list, at, raw } => {
                self.finish_shortcut(form, button, (list, at, raw));
            }
            action @ (FormAction::DriveColumns { .. }
            | FormAction::Disconnect { .. }
            | FormAction::NetworkLogin { .. }
            | FormAction::DriveRetry { .. }) => self.finish_drive_form(action, form, button),
            FormAction::SelectGroup { side, on } => {
                self.finish_select_form(form, button, (side, on));
            }
        }
    }

    /// The drive menu's forms: its columns, a disconnection, a share's login, a drive's Retry.
    fn finish_drive_form(&mut self, action: FormAction, form: &Form, button: Option<usize>) {
        match action {
            FormAction::DriveColumns { side, selected } => {
                self.finish_drive_columns(form, button, side, selected);
            }
            FormAction::Disconnect {
                root,
                place,
                yes,
                reconnect,
            } => self.finish_disconnect(form, button, (root, place, yes, reconnect)),
            FormAction::NetworkLogin { root, share, place } => {
                self.finish_network_login(form, button, (root, share, place));
            }
            FormAction::DriveRetry { side, root } => {
                self.finish_drive_retry(form, button, (side, root));
            }
            _ => {}
        }
    }

    /// The viewer's and the editor's forms (S2d): code page names, go to, search, replace.
    fn finish_text_form(&mut self, action: FormAction, form: &Form, button: Option<usize>) {
        match action {
            FormAction::CodePageName(page) => self.finish_code_page_name(form, button, page),
            FormAction::SyntaxHotkey(name) => self.finish_type_hotkey(&name, form, button),
            FormAction::ViewerGoTo => self.finish_viewer_go_to(form, button),
            FormAction::EditorGoTo => self.finish_editor_go_to(form, button),
            FormAction::EditorSearch => self.finish_editor_search(form, button),
            FormAction::EditorReplace => self.finish_editor_replace(form, button),
            FormAction::EditorReplaceAsk { at, size, backward } => {
                self.answer_editor_replace(button, (at, size, backward));
            }
            FormAction::EditorSearchWrap { backward, start } => {
                self.finish_editor_search_wrap(button, (backward, start));
            }
            FormAction::ViewerSearch(_) => self.finish_viewer_search(form, button),
            FormAction::ViewerSearchWrap { backward, start } => {
                self.finish_viewer_search_wrap(button, (backward, start));
            }
            FormAction::EditNew => self.finish_edit_new(form, button),
            FormAction::EditorSaveAs => self.finish_save_as(form, button),
            _ => {}
        }
    }

    /// Shift+F5 / Shift+F6 on the files (FAR's `KEY_SHIFTF5`: `Copy` with `CurrentOnly`): the entry
    /// under the cursor whatever is selected, its own name in the line (quoted as FAR quotes it). Into
    /// a plugin's panel only from one without real files (FAR's `ToPlugin = 0` with real names).
    pub(super) fn ask_transfer_current(&mut self, moving: bool) {
        let operation = if moving {
            super::transfer_dialog::Operation::Move
        } else {
            super::transfer_dialog::Operation::Copy
        };
        // A plugin that writes its files out: the entry under the cursor to it, its own dialog (FAR's
        // `ProcessCopyKeys` with `ReturnCurrentFile`, the selection kept).
        if self
            .active_panel()
            .current()
            .is_some_and(|entry| !entry.is_parent)
            && self.get_current_from_plugin(operation, self.plugin_target().is_some())
        {
            return;
        }
        let panel = self.active_panel();
        let Some(entry) = panel.current().filter(|entry| !entry.is_parent) else {
            return;
        };
        let source = panel.item_path(entry);
        let has_files = !entry.is_dir;
        let (target, to_plugin) = match source.as_local() {
            Some(_) => (
                super::transfer_dialog::quote_target(entry.name.clone(), moving),
                None,
            ),
            // FAR's `ProcessCopyKeys` for a plugin's file with no real one: F5's target.
            None => {
                let target = super::transfer_dialog::initial_target(
                    std::slice::from_ref(&source),
                    &panel.path.to_string(),
                    &self.transfer_target_dir(),
                    moving,
                );
                (target, self.plugin_target())
            }
        };
        self.ask_transfer_of(operation, vec![source], &target, has_files, to_plugin);
    }

    pub(super) fn ask_make_dir(&mut self) {
        // On the tree in the folder under its cursor as the key was pressed; an empty tree has none.
        let tree = match self.tree_target() {
            Some(None) => return,
            // A plugin that makes folders itself asks the name itself (FAR's `MakeDirectoryW`).
            None if self.make_folder_by_plugin() => return,
            tree => tree.flatten(),
        };
        // FAR's "Make folder" (R7 review D4: a plain line before).
        let form = super::make_dir_form::form(self.prefs.multi_make_dir);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::MakeDir(tree),
        });
    }

    /// "Make folder" closed: OK makes its names (none: FAR's "Incorrect folders list").
    fn finish_make_dir_form(&mut self, form: &Form, button: Option<usize>, tree: Option<PathBuf>) {
        use super::make_dir_form::{LINK_TYPE, MULTIPLE, NAME, OK, TARGET, link_of, names};
        if button != Some(OK) {
            return;
        }
        let multiple = form.is_checked(MULTIPLE);
        self.prefs.multi_make_dir = multiple;
        let names = names(form.edit_text(NAME), multiple);
        if names.is_empty() {
            let lines = vec!["Incorrect folders list".to_string()];
            return self.show_dialog(Dialog::error("Warning", lines));
        }
        let link =
            link_of(form.chosen(LINK_TYPE)).map(|kind| (kind, form.edit_text(TARGET).to_string()));
        self.make_dirs(names, link, tree);
    }

    /// F7's folder in the active panel's folder.
    #[cfg(test)]
    pub(super) fn make_dir(&mut self, name: &str) {
        self.make_dir_in(name, None);
    }

    /// F7's folder, in `tree` (the tree's folder) or the active panel's folder, is made on a worker (a
    /// network folder may take its time).
    #[cfg(test)]
    pub(super) fn make_dir_in(&mut self, name: &str, tree: Option<PathBuf>) {
        self.make_dirs(vec![name.to_string()], None, tree);
    }

    /// F7's `names`, each the folders on its way and the last a folder or (`link`) a link to its target,
    /// made in turn on a worker until one fails; on a provider's panel through its provider.
    fn make_dirs(
        &mut self,
        names: Vec<String>,
        link: Option<(NewLink, String)>,
        tree: Option<PathBuf>,
    ) {
        if tree.is_none() && self.active_panel().path.as_local().is_none() {
            return self.make_dir_mounted(&names);
        }
        let local = || self.active_panel().path.as_local().map(Path::to_path_buf);
        let Some(parent) = tree.or_else(local) else {
            return;
        };
        let tree_cache = self.tasks.tree_cache.clone();
        self.wait_for(Vec::new(), move || {
            let link = link.as_ref().map(|(kind, target)| (*kind, target.as_str()));
            let mut last = None;
            for name in &names {
                let make = |parent: &Path, name: &str| make_dir_form::make_one(parent, name, link);
                match make_dir_keeping(tree_cache.as_ref(), &parent, name, make) {
                    Ok(made) => last = Some(Ok(made)),
                    Err(error) => {
                        last = Some(Err((name.clone(), error)));
                        break;
                    }
                }
            }
            let linked = link.is_some();
            Box::new(move |app: &mut App| match last {
                // A link is no folder the helper makes: its error stands.
                Some(Err((name, error)))
                    if platform::elevation::needs_rights(&error) && !linked =>
                {
                    app.make_dir_denied(parent, name, &error);
                }
                Some(made) => app.made_dir(&parent, made.map_err(|(_, error)| error)),
                None => {}
            })
        });
        self.label_waiting("Making the folder…");
    }

    pub(super) fn made_dir(&mut self, parent: &Path, made: std::io::Result<PathBuf>) {
        match made {
            Ok(created) => {
                self.reread_panels();
                let above = created.parent().map(Path::to_path_buf);
                self.tree_files_changed(above.into_iter().collect());
                let focus = created
                    .strip_prefix(parent)
                    .ok()
                    .and_then(|relative| relative.components().next())
                    .map(|first| first.as_os_str().to_string_lossy().into_owned());
                // Made from the tree, it is in no panel's list yet.
                let panel = self.active_panel_mut();
                let shown = panel.path.as_local() == Some(parent);
                if let Some(index) = (focus.filter(|_| shown))
                    .and_then(|name| panel.entries.iter().position(|entry| entry.name == name))
                {
                    panel.cursor = index;
                }
            }
            Err(error) => {
                // The names made before the failure shown (FAR's `Update` after its loop).
                self.reread_panels();
                self.show_error("Make directory", &error);
            }
        }
    }

    /// What is still marked was skipped, failed or not reached: it stays marked, as in FAR.
    pub(super) fn finish_operation(
        &mut self,
        title: &str,
        report: &Report,
        retry: Option<Request>,
    ) {
        self.reread_panels();
        let lines = report_lines(report);
        if !lines.is_empty() {
            let failed = !report.failures.is_empty();
            self.show_dialog(report_dialog(title, lines, failed, retry));
        }
    }

    /// One item renamed or copied to a bare name: the active panel on its folder read again onto the
    /// new name (FAR's `GoToFile(strRenamedName)`, R7 review D3: the cursor stayed where it was).
    pub(super) fn go_to_renamed(&mut self, renamed: Option<&crate::vfs::VfsPath>) {
        let Some((folder, name)) =
            renamed.and_then(|path| Some((path.parent()?, path.file_name()?)))
        else {
            return;
        };
        let panel = self.active_panel_mut();
        if panel.path == folder {
            panel.reload_onto(&name);
        }
    }
}

/// F7's folder made by `make` on a worker, then put into its disk's tree file (FAR's `AddTreeName`
/// of each folder made: the first that was not there, and what is in it).
pub(super) fn make_dir_keeping(
    tree_cache: Option<&TreeCache>,
    parent: &Path,
    name: &str,
    make: impl FnOnce(&Path, &str) -> std::io::Result<PathBuf>,
) -> std::io::Result<PathBuf> {
    let top = tree_cache.and_then(|_| first_missing(&parent.join(name.trim())));
    let made = make(parent, name);
    if let (Ok(_), Some(cache), Some(top)) = (&made, tree_cache, top) {
        cache.made(&top);
    }
    made
}

/// A report as a dialog, with "Retry skipped" when errors left something out.
pub(super) fn report_dialog(
    title: &str,
    lines: Vec<String>,
    failed: bool,
    retry: Option<Request>,
) -> Dialog {
    match retry {
        Some(request) => Dialog::Confirm {
            title: title.to_string(),
            lines,
            buttons: vec!["OK", "Retry skipped"],
            focused: 0,
            danger: true,
            action: ConfirmAction::RetrySkipped(Box::new(request)),
        },
        None if failed => Dialog::error(title, lines),
        None => Dialog::message(title, lines),
    }
}

/// A report as lines to show: what failed, then what the target could not keep; a long list is cut.
pub(super) fn report_lines(report: &Report) -> Vec<String> {
    let all = report.failures.iter().chain(&report.not_kept);
    let count = report.failures.len() + report.not_kept.len();
    let mut lines: Vec<String> = all.take(REPORT_LINES).cloned().collect();
    if count > REPORT_LINES {
        lines.push(format!("…and {} more", count - REPORT_LINES));
    }
    lines
}

#[cfg(test)]
#[path = "fileops_tests.rs"]
pub(in crate::app) mod tests;
