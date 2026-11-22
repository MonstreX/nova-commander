//! The keys of what stands over the screen: the menu bar, pop-up menus, the drive menu, quick search
//! and dialogs.

use ratatui::crossterm::event::KeyEvent;

use super::{App, Command, Overlay, attributes, dialog, edit_insert, menu, popup};

impl App {
    /// A menu or dialog a closed dialog stood over comes back; true when one did.
    pub(super) fn resume_dialogs_behind(&mut self) -> bool {
        self.resume_task_list()
            || self.resume_menu_item_form()
            || self.resume_user_menu()
            || self.reread_user_menu()
            || self.resume_history_menu()
            || self.resume_user_shells()
            || self.resume_edit_list()
            || self.resume_shortcut_menu()
            || self.resume_filter_dialog()
            || self.resume_filters_menu()
            || self.resume_under_filters()
            || self.resume_find_dialog()
            || self.resume_find_results()
            || self.resume_stopped_under()
            || self.resume_code_page_menu()
    }

    /// FAR's menu sends these to the active panel, which takes them only when it shows files; the keys
    /// are held back in `keys` the same way.
    fn menu_command_applies(&self, command: Command) -> bool {
        let info = !self.shows_files(self.active);
        // The tree takes these keys for the folder under its cursor (`tree_panel::operations`).
        let on_tree = matches!(
            command,
            Command::MakeDir
                | Command::Copy
                | Command::Move
                | Command::Link
                | Command::Attributes
                | Command::Delete
                | Command::DeletePermanently
        );
        if on_tree && self.tree_has_keys() {
            return true;
        }
        match command {
            Command::MakeDir | Command::Filter => !info,
            Command::View
            | Command::Edit
            | Command::Copy
            | Command::Move
            | Command::CopyCurrent
            | Command::MoveCurrent
            | Command::Link
            | Command::Attributes
            | Command::Apply
            | Command::AddArchive
            | Command::ExtractArchive
            | Command::ArchiveCommands
            | Command::Delete
            | Command::DeleteCurrent
            | Command::DeletePermanently => !info && self.active_panel().visible,
            _ => true,
        }
    }

    pub(super) fn handle_overlay_key(&mut self, overlay: Overlay, key: KeyEvent) {
        match overlay {
            Overlay::Menu(mut state) => match menu::handle_key(&mut state, key) {
                menu::MenuOutcome::Stay => self.overlay = Some(Overlay::Menu(state)),
                menu::MenuOutcome::Close => {}
                menu::MenuOutcome::Run(command) => {
                    self.remembered.last_menu_item = Some(state);
                    if self.menu_command_applies(command) {
                        self.execute(command);
                    }
                }
            },
            Overlay::Popup(mut popup) => match popup::handle_key(&mut popup, key) {
                popup::PopupOutcome::Stay => self.overlay = Some(Overlay::Popup(popup)),
                popup::PopupOutcome::Close => {}
                popup::PopupOutcome::Choose(index, how) => match popup.action {
                    popup::PopupAction::Sort(side) => self.finish_sort_menu(side, index, how),
                    popup::PopupAction::InfoSections => self.finish_info_sections_menu(index, how),
                },
            },
            Overlay::DriveMenu(menu) => self.handle_drive_menu_key(menu, key),
            Overlay::QuickSearch(search) => self.handle_quick_search_key(search, key),
            Overlay::History(menu) => self.handle_history_key(menu, key),
            Overlay::UserMenu(menu) => self.handle_user_menu_key(menu, key),
            Overlay::EditList(list) => self.handle_edit_list_key(list, key),
            Overlay::Shortcuts(menu) => self.handle_shortcut_menu_key(menu, key),
            Overlay::Filters(menu) => self.handle_filters_menu_key(menu, key),
            Overlay::FindResults(results) => self.handle_find_results_key(results, key),
            Overlay::FolderTree(found) => self.handle_folder_tree_key(found, key),
            Overlay::PluginMenu(menu) if menu.viewer_mode => self.viewer_mode_key(menu, key),
            Overlay::PluginMenu(menu) if menu.code_pages => self.code_page_menu_key(menu, key),
            Overlay::PluginMenu(menu) if menu.colorer.is_some() => self.colorer_menu_key(menu, key),
            Overlay::PluginMenu(menu) if menu.found_all.is_some() => self.found_all_key(menu, key),
            Overlay::PluginMenu(menu) if menu.completion.is_some() => {
                self.completion_key(menu, key)
            }
            Overlay::PluginMenu(menu) => self.handle_plugin_menu_key(menu, key),
            Overlay::Dialog(dialog) if !list_in_front(&dialog) && self.dialog_move_key(key) => {
                self.overlay = Some(Overlay::Dialog(dialog));
            }
            Overlay::Dialog(dialog) => {
                // F10, Alt+F10, Shift+F10 in the copy, move and link dialogs: the tree.
                let Some(dialog) = self.transfer_tree_key(dialog, key) else {
                    return;
                };
                // FAR's path keys in an edit line (Ctrl+[, Shift+Enter, ...): the panels give the text.
                let Some(mut dialog) = (match list_in_front(&dialog) {
                    true => Some(dialog),
                    false => edit_insert::insert(self, dialog, key),
                }) else {
                    return;
                };
                let outcome = if attributes::intercept(&mut dialog, key) {
                    dialog::DialogOutcome::Stay
                } else {
                    dialog::handle_key(&mut dialog, key, self.clipboard.as_mut())
                };
                attributes::follow(&mut dialog);
                super::find::follow(&mut dialog, key);
                super::viewer::search::follow_dialog(&mut dialog);
                super::filter_edit::follow(&mut dialog);
                super::transfer_filter::follow(&mut dialog);
                super::make_dir_form::follow(&mut dialog);
                super::editor_save_as::follow(&mut dialog, &self.code_pages);
                match outcome {
                    dialog::DialogOutcome::Stay => {
                        self.preview_theme(&dialog);
                        self.overlay = Some(Overlay::Dialog(dialog));
                    }
                    outcome => {
                        self.themes.preview = None;
                        self.finish_dialog(dialog, outcome);
                    }
                }
            }
        }
    }
}

/// A form's open list (a drop-down field's, an edit line's history) takes every key itself.
fn list_in_front(dialog: &dialog::Dialog) -> bool {
    matches!(dialog, dialog::Dialog::Form { form, .. } if super::tree_panel::target_tree::list_open(form))
}
