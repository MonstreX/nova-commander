//! Executing commands and finishing dialogs.

use super::associations::AssocMode;
use super::command::Command;
use super::dialog::{ConfirmAction, Dialog, DialogOutcome, InputAction, ListAction};
use super::drive_actions::DriveAction;
use super::menu::MenuState;
use super::viewer::Viewer;
use super::{App, Overlay, Panel, Screen, Side, ViewMode};
use crate::settings::history::Kind;
use crate::terminal;

impl App {
    /// Ctrl+0..9 and the menus' view modes (FAR's `ChangePanelToFilled`): a view of files is a files
    /// panel, and it shows.
    fn set_view_mode(&mut self, side: Side, mode: ViewMode) {
        // Ctrl+6..9: only a provider's own mode there (the program has none of its own yet).
        if let ViewMode::Provider(number) = mode
            && self.panels[side.index()].look.mode(number).is_none()
        {
            return;
        }
        if !self.shows_files(side) {
            self.close_info_panel(side);
            self.close_tree_panel(side);
            self.close_quick_view(side);
            self.panels[side.index()].visible = true;
        }
        self.panels[side.index()].set_view_mode(mode);
        self.sync_sort_names();
    }

    /// Shift+F10: the menu bar at the item chosen last.
    fn open_last_menu_item(&mut self) {
        let last = self.remembered.last_menu_item;
        let state = (last.map(|state| self.fresh_menu(state))).unwrap_or(self.first_menu());
        self.overlay = Some(Overlay::Menu(state));
    }

    pub fn execute(&mut self, command: Command) {
        match command {
            Command::Help => self.show_help(),
            Command::SaveSetup => self.show_dialog(save_setup_question()),
            Command::ChooseShell => self.choose_shell(),
            Command::UserMenu => self.open_user_menu(),
            Command::EditUserMenu => self.ask_user_menu_type(),
            Command::FileAssociations => self.open_associations(),
            Command::FolderShortcuts => self.open_folder_shortcuts(),
            Command::MaskGroups => self.open_mask_groups(),
            Command::PanelModes => self.open_panel_modes(),
            Command::DescriptionFiles => self.ask_description_files(),
            Command::Filter => self.open_filters_menu(),
            Command::FindFile => self.open_find_file(),
            Command::FindFolder => self.open_folder_tree(),
            Command::ViewAlt => self.open_viewer_by(AssocMode::AltView),
            Command::EditAlt => self.open_editor_by(AssocMode::AltEdit),
            Command::EditNew => self.ask_edit_new(),
            Command::ChooseTheme => self.choose_theme(),
            Command::OpenMenu => self.overlay = Some(Overlay::Menu(self.first_menu())),
            Command::OpenLastMenuItem => self.open_last_menu_item(),
            Command::View => self.open_viewer_by(AssocMode::View),
            Command::Edit => self.open_editor_by(AssocMode::Edit),
            Command::Copy => self.ask_copy(),
            Command::Move => self.ask_move(),
            Command::CopyCurrent | Command::MoveCurrent => {
                self.ask_transfer_current(command == Command::MoveCurrent);
            }
            Command::Link => self.ask_link(),
            Command::Attributes => self.ask_attributes(),
            Command::Apply => self.ask_apply_command(),
            Command::MakeDir => self.ask_make_dir(),
            Command::AddArchive => self.add_to_new_by_plugin(),
            Command::ExtractArchive => self.extract_from_host_file(),
            Command::ArchiveCommands => self.host_file_commands(),
            Command::Delete => self.ask_delete(false, false),
            Command::DeleteCurrent => self.ask_delete(true, false),
            Command::DeletePermanently => self.ask_delete(false, true),
            Command::Quit => self.ask_quit(),
            Command::Screens => self.show_screens(),
            Command::TaskList => self.show_task_list(),
            Command::PluginCommands => self.open_plugin_commands(),
            Command::ContextMenu => {
                self.context_menu_key(None, self.active, None);
            }
            Command::PluginsConfig => self.open_plugin_configs(),
            Command::Plugins => self.open_plugin_manager(0),
            Command::History(list) => self.open_history_menu(list),
            Command::SwapPanels => self.swap_panels(),
            Command::TogglePanels => self.toggle_panels(),
            Command::ToggleGuestShell => self.toggle_guest(),
            Command::PanelsShorter => self.change_height_decrement(1),
            Command::PanelsTaller => self.change_height_decrement(-1),
            Command::SplitLeft => self.change_width_decrement(1),
            Command::SplitRight => self.change_width_decrement(-1),
            Command::TogglePanel(side) => self.toggle_panel(side),
            Command::TogglePassivePanel => self.toggle_passive_panel(),
            Command::ToggleKeyBar => self.toggle_key_bar(),
            Command::ChangeDrive(side) => self.open_drive_menu(side),
            Command::Sort(side, mode) => self.panels[side.index()].set_sort(mode),
            Command::SortMenu(side) => self.sort_menu(side),
            Command::ToggleSelectedFirst(side) => self.panels[side.index()].toggle_selected_first(),
            Command::ToggleSortGroups(side) => self.panels[side.index()].toggle_sort_groups(),
            Command::SetViewMode(side, mode) => self.set_view_mode(side, mode),
            Command::Reread(side) => self.reread(side),
            Command::Options(page) => self.ask_options(page),
            Command::InfoPanel(side) => self.show_info_panel_on(side),
            Command::TreePanel(side) => self.show_tree_panel_on(side),
            Command::QuickView(side) => self.show_quick_view_on(side),
            Command::SelectGroup => self.ask_group_of_active(true),
            Command::UnselectGroup => self.ask_group_of_active(false),
            Command::InvertSelection => self.invert_active_selection(),
            // FAR calls it on the panel, seen or not.
            Command::RestoreSelection => {
                if self.shows_files(self.active) {
                    self.active_panel_mut().restore_selection();
                }
            }
        }
    }

    /// Gray `*`: the active panel's selection inverted, its folders as the panel's options say.
    fn invert_active_selection(&mut self) {
        self.on_active_files(|panel| {
            let folders = panel.options.select_folders;
            panel.invert_selection(folders);
        });
    }

    /// Ctrl+F12 on that side: a side not shown takes it no more than FAR's; the info panel's is its
    /// "Show Info" menu.
    fn sort_menu(&mut self, side: Side) {
        if !self.panels[side.index()].visible {
            return;
        }
        if self.info_side() == Some(side) {
            self.open_info_sections_menu();
        } else if self.shows_files(side) {
            self.open_sort_menu(side);
        }
    }

    /// Every panel read again.
    pub(super) fn reread_panels(&mut self) {
        for panel in &mut self.panels {
            panel.reload();
        }
    }

    /// Ctrl+R on that side.
    fn reread(&mut self, side: Side) {
        if !self.panels[side.index()].visible {
            return;
        }
        if self.info_side() == Some(side) {
            self.reread_info_panel();
        } else if self.tree_side() == Some(side) {
            self.reread_tree();
        } else if self.quick_view_side() == Some(side) {
            self.reread_quick_view();
        } else {
            self.panels[side.index()].reload();
        }
    }

    /// The menu F9 opens: the right one when the right panel is active and shown (FAR).
    fn first_menu(&self) -> MenuState {
        let side = if self.active_panel().visible {
            self.active
        } else {
            Side::Left
        };
        self.fresh_menu(MenuState::for_side(side))
    }

    /// A menu as it opens: the tree's items as the options say, the panels' providers' own modes.
    pub(super) fn fresh_menu(&self, state: MenuState) -> MenuState {
        let modes = |side: Side| {
            let look = &self.panels[side.index()].look;
            (6..=9)
                .filter(|number| look.mode(*number).is_some())
                .fold(0u8, |bits, number| bits | 1 << (number - 6))
        };
        let last = (state.items().get(state.item)).and_then(super::MenuItem::command);
        let mut fresh = MenuState {
            no_tree: self.prefs.tree.turn_off_completely,
            provider_modes: [modes(Side::Left), modes(Side::Right)],
            ..state
        };
        // The items a menu has change with the panels (a plugin's modes): the one run last is found
        // again by what it does, else the first.
        let items = fresh.items();
        fresh.item = (last.and_then(|command| {
            items
                .iter()
                .position(|item| item.command() == Some(command))
        }))
        .unwrap_or(0);
        fresh
    }

    /// The active panel when it shows files.
    fn on_active_files(&mut self, act: impl FnOnce(&mut Panel)) {
        if self.active_panel().visible && self.shows_files(self.active) {
            act(self.active_panel_mut());
        }
    }

    pub(super) fn finish_dialog(&mut self, dialog: Dialog, outcome: DialogOutcome) {
        self.forget_dialog_place();
        match (dialog, outcome) {
            (Dialog::Confirm { action, .. }, DialogOutcome::Button(button)) => {
                self.finish_confirm(action, button);
            }
            (Dialog::Input { action, .. }, DialogOutcome::Submit(text)) => {
                self.finish_input(action, &text);
            }
            (Dialog::List { action, items, .. }, DialogOutcome::Choose(index)) => {
                self.finish_list(action, &items, index);
            }
            (
                Dialog::List {
                    action: ListAction::Screens(ids),
                    ..
                },
                DialogOutcome::Button(index),
            ) => self.remove_screen(&ids, index),
            (
                Dialog::List {
                    action: ListAction::Plugins(names),
                    ..
                },
                DialogOutcome::Button(index),
            ) => self.restart_listed_plugin(&names, index),
            (
                Dialog::List {
                    action: ListAction::UserShells,
                    items,
                    ..
                },
                DialogOutcome::Button(index),
            ) => {
                if index == items.len() {
                    self.edit_user_shell(index);
                } else {
                    self.ask_remove_user_shell(index);
                }
            }
            (Dialog::Form { mut form, action }, DialogOutcome::Button(index)) => {
                let closes = self.closes_for_good(&action, &form, index);
                self.keep_form_histories(&mut form, closes);
                self.finish_form(action, &form, Some(index));
            }
            (Dialog::Form { mut form, action }, DialogOutcome::Cancel) => {
                self.keep_form_histories(&mut form, false);
                self.finish_form(action, &form, None);
            }
            (dialog, DialogOutcome::Cancel) => self.drive_dialog_cancelled(&dialog),
            _ => {}
        }
    }

    pub(super) fn finish_confirm(&mut self, action: ConfirmAction, button: usize) {
        match (action, button) {
            (ConfirmAction::Quit, 0) => self.finish_quit(),
            (ConfirmAction::SaveSetup, 0) => self.save_requested = true,
            (ConfirmAction::EditNewPath(path, page), 0) => {
                return self.edit_new_in_missing_folder(&path, page);
            }
            (ConfirmAction::EditorToViewer, button) => {
                return self.answer_editor_to_viewer(button);
            }
            (ConfirmAction::KillTask(pid), 0) => self.kill_task(pid),
            (ConfirmAction::ClearHistory(list), 0) => self.clear_history(list),
            (ConfirmAction::RemoveUserShell(index), 0) => self.remove_user_shell(index),
            (ConfirmAction::RemoveAssociation(at), 0) => self.remove_association(at),
            (ConfirmAction::RemoveMaskGroup(at), 0) => self.remove_mask_group(at),
            (ConfirmAction::RemoveFilter(at), 0) => self.remove_filter(at),
            (ConfirmAction::RestoreMaskGroups, 0) => self.restore_mask_groups(),
            (ConfirmAction::SaveShortcut { list, at, shortcut }, 0) => {
                self.save_shortcut(list, at, *shortcut);
            }
            (ConfirmAction::InsertMenuItem, button) => self.insert_menu_item(button == 1),
            (ConfirmAction::RemoveMenuItem, 0) => self.remove_menu_item(),
            (ConfirmAction::SaveMenuItem, button) => self.answer_save_menu_item(button),
            (ConfirmAction::ChooseUserMenu, button @ (0 | 1)) => {
                self.open_chosen_user_menu(button == 1);
            }
            (
                ConfirmAction::MakeDirElevated {
                    parent,
                    name,
                    error,
                },
                0,
            ) => self.make_dir_elevated(parent, name, error),
            (ConfirmAction::MakeDirElevated { error, .. }, 1) => {
                self.show_dialog(Dialog::error("Make directory", vec![error]));
            }
            (ConfirmAction::RetrySkipped(request), 1) => self.retry_skipped(*request),
            (ConfirmAction::StopTransfer(id), 0) => self.stop_transfer(id),
            (stop @ (ConfirmAction::StopFind | ConfirmAction::StopReading), button) => {
                return self.answer_stop(&stop, button);
            }
            (quit @ (ConfirmAction::QuitWithTasks | ConfirmAction::QuitNotStopping), button) => {
                self.answer_quit_with_tasks(&quit, button);
            }
            (ConfirmAction::CloseEditor, button) => return self.answer_close_editor(button),
            (ConfirmAction::ViewerBytes, button) => return self.answer_viewer_bytes(button),
            (action @ ConfirmAction::Reedit { .. }, button) => {
                return self.answer_reedit(action, button);
            }
            (ConfirmAction::EditorSave(pending), button) => {
                return self.answer_save_question(*pending, button);
            }
            (
                ConfirmAction::EditReadOnly {
                    path,
                    remember,
                    place,
                },
                0,
            ) => return self.open_editor_unasked(&path, remember, place),
            (ConfirmAction::Drive(action, place), 0) => {
                return self.run_drive_action(action, place, true);
            }
            (ConfirmAction::Drive(_, place) | ConfirmAction::BackToDriveMenu(place), _) => {
                self.back_to_drive_menu(place);
            }
            _ => {}
        }
        self.reread_panels();
    }

    /// "Stop the search?" / "Stop reading?" answered (Yes: the first button); no reread after it.
    fn answer_stop(&mut self, stop: &ConfirmAction, button: usize) {
        if button != 0 {
            return;
        }
        match stop {
            // The results come back with the search stopped, or going on (No, Esc).
            ConfirmAction::StopFind => self.stop_find(),
            // No reread: that would start again the reading just stopped.
            _ => {
                for panel in &mut self.panels {
                    panel.stop_reading();
                }
            }
        }
    }

    fn finish_input(&mut self, action: InputAction, text: &str) {
        match action {
            InputAction::ApplyCommand => self.apply_command(text),
            InputAction::DescriptionFiles => self.set_description_files(text),
            InputAction::VolumeLabel { root, place } => {
                let label = text.to_string();
                self.run_drive_action(DriveAction::Label { root, label }, place, false);
            }
        }
    }

    fn finish_list(&mut self, action: ListAction, items: &[String], index: usize) {
        match action {
            ListAction::Shell(shells) => {
                if let Some(shell) = shells.get(index) {
                    self.choose_guest_shell(shell.clone());
                }
            }
            ListAction::Screens(ids) => self.choose_screen(&ids, index),

            ListAction::UserShells => self.edit_user_shell(index),
            ListAction::Plugins(names) => self.toggle_listed_plugin(&names, index),
            ListAction::Association(commands, mode, target) => {
                if let Some(command) = commands.get(index) {
                    self.run_association(command, mode, *target);
                }
            }
            ListAction::Theme => {
                if let Some(name) = items.get(index) {
                    self.themes.chosen.clone_from(name);
                }
            }
        }
    }

    fn choose_shell(&mut self) {
        let shells = terminal::available_shells(&self.user_shells);
        let items = shells
            .iter()
            .map(|shell| shell.title().to_string())
            .collect();
        let cursor = shells
            .iter()
            .position(|shell| shell.id() == self.shell.id())
            .unwrap_or(0);
        self.show_dialog(Dialog::List {
            title: "Shell".to_string(),
            items,
            cursor,
            top: Default::default(),
            action: ListAction::Shell(shells),
        });
    }

    fn current_file(&self) -> Option<std::path::PathBuf> {
        let panel = self.active_panel();
        panel
            .current()
            .filter(|entry| !entry.is_dir)
            .and_then(|_| panel.current_path())
    }

    /// F3: a file is viewed, a folder counted (FAR's `CountDirSize`).
    fn open_viewer(&mut self) {
        if self.active_panel().current().is_some_and(|e| e.is_dir) {
            self.active_panel_mut().count_folder_sizes();
            return;
        }
        if let Some(path) = self.current_file() {
            self.view_file(&path, true);
        } else {
            // A file of a panel that is no disk's: copied first (review R55).
            self.open_extracted(false);
        }
    }

    /// The viewer on `path`; `remember`: the view history notes it (FAR's `DisableHistory` off).
    pub(super) fn view_file(&mut self, path: &std::path::Path, remember: bool) {
        self.view_file_listed(path, remember, Vec::new());
    }

    /// [`view_file`](Self::view_file) with the files Gray + / Gray - go through (`list`). Opened at
    /// once: its blocks are read on a thread of their own as the screen asks (ADR-0015).
    pub(super) fn view_file_listed(
        &mut self,
        path: &std::path::Path,
        remember: bool,
        list: Vec<std::path::PathBuf>,
    ) {
        let viewer = self.new_viewer(path, list);
        self.show_window(Screen::Viewer(viewer));
        if remember {
            self.remember_file(path, Kind::View);
        }
    }

    /// A viewer on `path` with the settings and the place kept for the file.
    fn new_viewer(&self, path: &std::path::Path, list: Vec<std::path::PathBuf>) -> Box<Viewer> {
        let mut viewer = Box::new(Viewer::open(path));
        viewer.apply_settings(&self.viewer_settings);
        viewer.copy = self.is_extracted(path);
        if let (true, Some(store)) = (self.viewer_settings.keeps_positions(), &self.positions) {
            let file = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
            if let Some(place) = crate::settings::positions::viewer_position(store, &file) {
                viewer.restore_position(place);
            }
        }
        viewer.list = list;
        viewer
    }

    /// The viewer's reader: what it brought (a file that cannot be read says why on its screen).
    pub(super) fn pump_viewer(&mut self) -> bool {
        match &mut self.screen {
            // Drawn again while it reads with nothing to show: its notice's time goes on.
            Screen::Viewer(viewer) => viewer.pump() | viewer.text().reading_for().is_some(),
            _ => false,
        }
    }

    /// Gray + / Gray - in the viewer: the next file of its list in its place (the same window), the
    /// list kept.
    pub(super) fn view_next(&mut self, path: &std::path::Path) {
        let Screen::Viewer(viewer) = &self.screen else {
            return;
        };
        let list = viewer.list.clone();
        self.keep_viewer_position();
        self.screen = Screen::Viewer(self.new_viewer(path, list));
        self.remember_file(path, Kind::View);
    }

    /// FAR's F3 / Alt+F3 (`FileList::ProcessKey`): the file's association, else the external viewer
    /// when the key is its (F3 with "Use external viewer", Alt+F3 without) and it has a command, else
    /// the program's viewer.
    fn open_viewer_by(&mut self, mode: AssocMode) {
        if self.open_by_association(mode) {
            return;
        }
        let settings = &self.viewer_settings;
        let external = (mode == AssocMode::AltView) != settings.use_external;
        let command = settings.external_command.clone();
        if !(external && self.run_external(&command, AssocMode::View)) {
            self.open_viewer();
        }
    }

    /// FAR's `ProcessExternal`: the external viewer's or editor's command for the current file, its
    /// metasymbols the file's; false without a command or a file.
    fn run_external(&mut self, command: &str, mode: AssocMode) -> bool {
        let command = command.trim();
        let Some(path) = self.current_file().filter(|_| !command.is_empty()) else {
            return false;
        };
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let target = super::associations::AssocTarget {
            name: name.unwrap_or_default(),
            current: self.menu_facts().active.current,
            dir: None,
        };
        self.run_association(command, mode, target);
        true
    }

    /// FAR's F4 / Alt+F4: the file's association, else the external editor when the key is its (F4
    /// with "Use external editor", Alt+F4 without) and it has a command, else the program's editor.
    /// On a folder: its attributes, as Ctrl+A (FAR's `FileList::ProcessKey`, R7 review D8).
    fn open_editor_by(&mut self, mode: AssocMode) {
        if self
            .active_panel()
            .current()
            .is_some_and(|entry| entry.is_dir)
        {
            return self.execute(Command::Attributes);
        }
        if self.open_by_association(mode) {
            return;
        }
        let settings = &self.editor_settings;
        let external = (mode == AssocMode::AltEdit) != settings.use_external;
        let command = settings.external_command.clone();
        if !(external && self.run_external(&command, AssocMode::Edit)) {
            self.open_editor();
        }
    }

    fn open_editor(&mut self) {
        if let Some(path) = self.current_file() {
            self.edit_file(&path, true);
        } else {
            self.open_extracted(true);
        }
    }

    /// The editor on `path`, opened at once (its blocks read as the screen needs them, ADR-0015).
    pub(super) fn edit_file(&mut self, path: &std::path::Path, remember: bool) {
        self.edit_file_at(path, remember, (0, 0));
    }

    /// [`edit_file`](Self::edit_file) with the cursor at `line` and `column` (1-based; 0: the start; FAR's
    /// `Editor`'s `StartLine`, `StartChar`).
    pub(super) fn edit_file_at(
        &mut self,
        path: &std::path::Path,
        remember: bool,
        place: (usize, usize),
    ) {
        if !self.ask_reedit(path, remember, place) {
            self.open_editor_at(path, remember, place);
        }
    }
}

/// FAR's question before "Save setup" (`Options::Save(Manual)`, R7 review D9).
fn save_setup_question() -> Dialog {
    Dialog::Confirm {
        title: "Save setup".to_string(),
        lines: vec![
            "Do you wish to save".to_string(),
            "current setup?".to_string(),
        ],
        buttons: vec!["Save", "Cancel"],
        focused: 0,
        danger: false,
        action: ConfirmAction::SaveSetup,
    }
}
