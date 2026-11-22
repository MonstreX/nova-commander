//! Application state and behaviour: panels, command line, menus, dialogs, viewer, editor and the
//! link to the embedded terminal. Rendering lives in `ui`, which reads this state; `app` never
//! depends on `ui`.

mod actions;
mod app_state;
mod associations;
mod associations_edit;
mod attributes;
pub mod clipboard;
mod cmdline_insert;
mod code_page_menu;
mod colorer_actions;
mod colorer_menu;
mod colorer_outline;
mod command;
mod completion;
pub mod console_input;
mod delete;
mod detached;
mod dialog;
mod dialog_history;
mod dialog_move;
pub use dialog_move::DialogPlace;
mod drive_actions;
mod drive_menu;
mod drive_retry;
mod edit_insert;
mod edit_list;
mod edit_new;
mod editing;
mod editor_actions;
mod editor_code;
mod editor_convert;
mod editor_found;
mod editor_insert;
mod editor_mouse;
mod editor_replace;
mod editor_save;
mod editor_save_as;
mod editor_save_file;
mod editor_search;
mod editor_search_form;
mod editor_syntax;
mod editor_text;
mod elevation_question;
mod extracted;
mod fileops;
mod filter_dialog;
mod filter_edit;
mod filters;
mod filters_menu;
mod find;
pub mod form;
mod guest_link;
mod held;
mod help;
mod highlighting;
mod history;
mod history_menu;
mod info_description;
mod info_panel;
mod keyboard;
mod keys;
mod layout;
mod line_edit;
mod line_input;
mod make_dir_form;
#[cfg(test)]
mod marks_tests;
mod mask_groups;
pub mod masked_input;
mod menu;
#[cfg(test)]
mod menu_commands_tests;
pub mod menu_filter;
mod metasymbols;
mod mouse;
mod mouse_bars;
#[cfg(test)]
mod mouse_bars_tests;
mod mouse_dialogs;
#[cfg(test)]
mod mouse_dialogs_tests;
mod mouse_drag;
#[cfg(test)]
mod mouse_drag_tests;
mod mouse_menus;
#[cfg(test)]
mod mouse_menus_tests;
mod mouse_scroll_bar;
mod mouse_terminal;
#[cfg(test)]
mod mouse_tests;
mod name_cases;
mod options;
mod options_pages;
mod overlay;
mod overlay_keys;
mod panel;
mod panel_clipboard;
pub mod panel_modes;
mod panel_modes_menu;
mod panel_options;
mod panel_selection;
mod panel_sizes;
mod panel_totals;
mod persist;
mod plugins;
mod popup;
mod provider_ops;
mod pumps;
mod quick_search;
mod quick_view;
mod quick_view_text;
#[cfg(test)]
mod reading_tests;
mod screen_switch;
mod screens;
mod select_dialog;
mod selection;
mod shortcuts;
mod shortcuts_edit;
mod shortcuts_menu;
mod sorting;
mod task_windows;
pub use task_windows::TASK_LIST_ROWS_LEFT;
mod terminal_keys;
pub mod text_cut;
mod text_screen_keys;
mod themes;
mod transfer;
mod transfer_dialog;
mod transfer_filter;
mod transfer_question;
#[cfg(test)]
mod transfer_tests;
pub mod tree_panel;
mod user_menu;
mod user_menu_edit;
mod user_menu_run;
mod user_screen_link;
mod user_shells;
mod view_mode;
mod viewer;
mod viewer_actions;
mod vmenu_keys;
mod waiting;
mod windows;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyEvent;

/// What the progress dialog shows (`ui` reads it through `app`).
pub use crate::jobs::attributes::Progress as AttributesProgress;
pub use crate::jobs::delete::Progress as DeleteProgress;
pub use crate::jobs::folder_info::{Counted as FolderCounted, FolderLink};
pub use crate::jobs::tasks::{Kind as TaskKind, Status as TaskStatus, Task};
pub use crate::jobs::transfer::Progress as TransferProgress;
pub use command::{Command, Side};
pub use completion::{Completion, Item as CompletionItem};
pub use dialog::{
    Dialog, FolderCount, FormAction, ListAction, PluginRow, PluginState, WaitBar, WaitWindow,
};
pub use drive_menu::{DiskItem, DriveMenu};
#[cfg(test)]
pub use edit_list::EditListKind;
pub use edit_list::{EDIT_LIST_KEYS, EditList};
pub use editing::EditorScreen;
pub use filters_menu::{FILTERS_KEYS, FILTERS_TITLE, FiltersMenu};
pub use find::results::{FindResults, ResultLine};
pub use find::search_label;
pub use highlighting::{HighlightElement, HighlightState, Highlighting, Paint};
pub use history_menu::{HistoryLine, HistoryMenu, history_title};
pub use info_description::Description;
pub use info_panel::{InfoPanel, InfoSections, Section};
pub use layout::panel_widths;
pub use line_input::LineInput;
pub use menu::{MENUS, MenuItem, MenuState, title_positions};
pub use mouse::{ListArea, PanelArea};
pub use mouse_bars::{CommandLineArea, KeyBarArea};
pub use mouse_dialogs::{ControlKind, DialogArea, DialogControl};
pub use mouse_menus::{Cells, MenuArea, keep_in_view};
pub use options::OptionsPage;
pub use overlay::Overlay;
use panel::Polled;
pub use panel::{NON_LIST_ROWS, Panel, remove_copies_now};
pub use panel_modes::{ColumnDef, ColumnFlags, ColumnKind, ModeSpec, PanelModes};
pub use plugins::{PLUGIN_MENU_ROWS_LEFT, PLUGINS_KEYS, PluginMenu};
pub use popup::{Popup, PopupAnchor};
pub use quick_search::QuickSearch;
pub use quick_view::{QuickView, Showing, Shown};
pub use quick_view_text::FileText;
pub use screens::{TaskTitle, task_label, task_percent};
pub use shortcuts_menu::{ShortcutLine, ShortcutMenu};
pub use sorting::mode_letter;
pub use transfer::{BACKGROUND, CANCEL, PAUSE, ShownTask};
#[cfg(test)]
pub use user_menu::MenuPlace;
pub use user_menu::{USER_MENU_KEYS, UserMenu, UserMenuLevel, UserMenuLine};
pub use user_shells::SHELLS_KEYS;
pub use view_mode::ViewMode;
pub use viewer::Viewer;
pub use vmenu_keys::{shown_chars, with_hotkey};

use crate::jobs::tasks::TaskList;
use crate::settings::Settings;
use crate::terminal::{Environment, Session, Shell, UserScreen};

const MIN_PANEL_ROWS: u16 = 6;

pub enum Screen {
    Panels,
    Viewer(Box<Viewer>),
    Editor(Box<EditorScreen>),
}

/// What the main screen shows (ADR-0006).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainView {
    /// The panels (or, with both hidden by Ctrl+O, the user screen) and the command line.
    Panels,
    /// A command from the command line is running: the user screen fills the console and every key
    /// goes to the command. The panels come back when it ends.
    CommandOutput,
    /// Alt+Shift+T: the guest shell fills the console and gets every key.
    Guest,
    /// A command in the background brought from the Screens list: it fills the console and gets the keys.
    Detached(usize),
}

pub struct App {
    pub panels: [Panel; 2],
    pub active: Side,
    pub screen: Screen,
    pub overlay: Option<Overlay>,
    pub command_line: LineInput,
    /// Where Ctrl+Ins copies to and Shift+Ins pastes from.
    pub clipboard: Box<dyn clipboard::Clipboard>,
    /// FAR's user screen, where command lines run (ADR-0006).
    pub user_screen: Option<UserScreen>,
    /// Commands sent to the background with their user screens (the Screens list).
    pub detached: Vec<detached::DetachedCommand>,
    /// Variables set with `set` on the command line; every command gets them.
    environment: Environment,
    /// The guest shell (Alt+Shift+T), started on first use.
    pub guest: Option<Session>,
    /// Which shell the guest is.
    pub shell: Shell,
    /// The user's own shells, after the system's in the lists.
    pub user_shells: Vec<Shell>,
    /// FAR's files highlighting: the groups as kept and as made for the panels.
    pub highlighting: highlighting::Highlighting,
    pub panel_modes: PanelModes,
    /// FAR's filters (Ctrl+I).
    filters: filters::FilterState,
    /// Alt+F7.
    find: find::FindState,
    /// The code page menu's favourites, names and mode (Shift+F8).
    code_pages: crate::settings::CodePageSettings,
    syntax_types: crate::settings::SyntaxTypeSettings,
    viewer_settings: crate::settings::ViewerSettings,
    editor_settings: crate::settings::EditorSettings,
    /// Ctrl+O in the viewer or the editor: the user screen shown until a key (FAR's
    /// `ShowBackground`).
    pub backdrop: bool,
    /// The viewer's places kept between runs (`settings::positions`); none in the tests.
    pub positions: Option<std::path::PathBuf>,
    /// The viewer's search as its dialog was left (FAR's shared search parameters, for the session).
    viewer_search: viewer::search::SearchParams,
    pub main_view: MainView,
    /// Rows the panels are made shorter by to show terminal lines (FAR's Ctrl+Up/Ctrl+Down).
    pub height_decrement: u16,
    /// Columns the left panel is made narrower by (FAR's `WidthDecrement`, Ctrl+Left / Ctrl+Right).
    pub width_decrement: i16,
    /// The key bar is shown (Ctrl+B).
    pub show_key_bar: bool,
    /// The command line just typed into the guest shell and when: until the shell echoes it, its
    /// screen does not show it yet.
    given_input: Option<(String, usize, Instant)>,
    /// Hidden and system files are listed (Ctrl+H).
    pub show_hidden: bool,
    /// Disk work the keys wait for (`waiting`).
    waiting: Option<waiting::Waiting>,
    /// The window Esc's notice came over when it stopped waiting for work, back after the notice.
    stopped_under: Option<Overlay>,
    /// Files of panels that are no disk's copied for the viewer or the editor (`extracted`).
    extracted: Vec<extracted::Extracted>,
    /// The viewers, the editors and the panels behind the window in front (`windows`).
    windows: windows::Windows,
    /// A device came or left while work ran: the open drive menu is read again after it.
    devices_pending: bool,
    /// Ctrl+L: the info panel in place of one side's files.
    pub info_panel: Option<InfoPanel>,
    pub tree_panel: Option<tree_panel::TreePanel>,
    /// Ctrl+Q.
    pub quick_view: Option<QuickView>,
    plugins: plugins::Plugins,
    /// Options > Themes: the chosen theme and the user's theme files.
    pub themes: themes::Themes,
    /// The console's 16 colours are set to the theme's (`screen.consolePalette`).
    pub console_palette: bool,
    /// Characters pressed and not yet released: tells an Alt code from a key release.
    releases: keyboard::Releases,
    /// Modifier keys held now, for FAR's key bar labels (set by the event loop).
    pub held: crate::platform::input::Modifiers,
    /// The mouse: whether it is the program's, where the panels are, a button held.
    pub mouse: mouse::MouseState,
    /// Where the dialog in front stands (`dialog_move`).
    pub dialog_place: dialog_move::DialogPlace,
    /// Shift+F9 / Options > Save setup: the caller saves the settings now.
    save_requested: bool,
    /// The switches of Options that live nowhere else.
    pub prefs: options::Preferences,
    /// Copies and moves (F5 / F6) running, queued or ended (ADR-0010).
    pub tasks: TaskList,
    /// The task whose progress dialog is in front; `None`: all run in the background.
    pub shown_task: Option<ShownTask>,
    /// The shells' dialog to come back to (`user_shells`).
    shells: user_shells::ShellsState,
    user_menu: user_menu::UserMenuState,
    edit_lists: edit_list::EditListState,
    shortcuts: shortcuts::ShortcutState,
    /// Folders that could not be entered after the panel stopped waiting: shown when no dialog is up,
    /// so the one the user opened meanwhile is not replaced.
    late_failures: Vec<(String, std::io::Error)>,
    history: history::HistoryState,
    /// The list under the command line while typing.
    pub completion: Option<completion::Completion>,
    width: u16,
    height: u16,
    quit: bool,
    /// What the settings give the work (`app_state::Config`).
    config: app_state::Config,
    /// The tasks' side of the screen.
    work: app_state::Work,
    /// What the session keeps for the next time.
    remembered: app_state::Remembered,
    /// Ctrl+W's task list: what it stands over.
    task_list: task_windows::TaskListState,
}

impl App {
    /// Builds the application from saved settings; `start_dir` replaces saved paths that vanished.
    pub fn new(start_dir: &Path, width: u16, height: u16, settings: &Settings) -> Self {
        let mut app = Self {
            panels: persist::restored_panels(settings, start_dir),
            active: persist::side_from_id(&settings.active_panel),
            screen: Screen::Panels,
            overlay: None,
            command_line: LineInput::default(),
            clipboard: clipboard::for_app(),
            user_screen: None,
            detached: Vec::new(),
            environment: Environment::default(),
            guest: None,
            shell: persist::shell_from_id(&settings.shell, &persist::user_shells(settings)),
            user_shells: persist::user_shells(settings),
            filters: filters::FilterState::saved(settings),
            highlighting: persist::highlighting(settings),
            panel_modes: persist::panel_modes(settings),
            find: find::FindState::saved(settings),
            code_pages: settings.code_pages.clone(),
            syntax_types: settings.syntax_types.clone(),
            viewer_settings: settings.viewer.clone(),
            editor_settings: settings.editor.clone(),
            backdrop: false,
            positions: None,
            viewer_search: viewer::search::SearchParams::default(),
            main_view: MainView::Panels,
            height_decrement: settings.height_decrement,
            width_decrement: settings.width_decrement,
            show_key_bar: settings.show_key_bar,
            given_input: None,
            releases: keyboard::Releases::default(),
            themes: themes::Themes::new(&settings.theme),
            console_palette: settings.console_palette,
            mouse: mouse::MouseState::new(settings.mouse),
            dialog_place: dialog_move::DialogPlace::default(),
            save_requested: false,
            prefs: options::Preferences::from_settings(settings),
            tasks: TaskList::new(Duration::from_secs(settings.keep_finished_tasks)),
            shown_task: None,
            shells: user_shells::ShellsState::default(),
            user_menu: user_menu::UserMenuState::default(),
            edit_lists: edit_list::EditListState::default(),
            shortcuts: shortcuts::ShortcutState::new(&settings.folder_shortcuts),
            late_failures: Vec::new(),
            held: crate::platform::input::Modifiers::default(),
            show_hidden: settings.show_hidden,
            waiting: None,
            stopped_under: None,
            extracted: Vec::new(),
            windows: windows::Windows::default(),
            devices_pending: false,
            info_panel: None,
            tree_panel: None,
            quick_view: None,
            plugins: plugins::Plugins::keeping(settings),
            history: history::HistoryState::default(),
            completion: None,
            width,
            height,
            quit: false,
            config: app_state::Config::saved(settings),
            work: app_state::Work::default(),
            remembered: app_state::Remembered::saved(settings),
            task_list: task_windows::TaskListState::default(),
        };
        app.clamp_height_decrement();
        app.clamp_width_decrement();
        app.apply_filters();
        app.make_highlighting();
        app.restore_panel_types(settings);
        app.panels[0].visible = settings.left_visible;
        app.panels[1].visible = settings.right_visible;
        app
    }

    /// Records the held modifier keys; returns whether they changed (the key bar must be redrawn).
    pub fn set_held_modifiers(&mut self, held: crate::platform::input::Modifiers) -> bool {
        let changed = self.held != held;
        self.held = held;
        if !held.shift {
            for panel in &mut self.panels {
                panel.end_shift_selection();
            }
        }
        changed
    }

    /// Whether "Save setup" was chosen since the last call.
    pub fn take_save_request(&mut self) -> bool {
        std::mem::take(&mut self.save_requested)
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn active_panel(&self) -> &Panel {
        &self.panels[self.active.index()]
    }

    /// The OS folder commands and shells run in: the active panel's; on a plugin's panel the folder it was
    /// opened from (FAR's `GetCurDir`), else the system folder.
    pub(super) fn active_folder(&self) -> PathBuf {
        self.command_panel()
            .local_folder()
            .map_or_else(crate::platform::process::system_folder, Path::to_path_buf)
    }

    /// The panel commands act on: the active one, or the other while the info panel has the keys
    /// (FAR's info panel takes the other panel's folder).
    pub fn command_panel(&self) -> &Panel {
        &self.panels[self.command_side().index()]
    }

    /// The side of [`App::command_panel`] (FAR's `CommandLine::IntChDir`: the passive file panel when the
    /// active one is not a file panel).
    pub(super) fn command_side(&self) -> Side {
        if !self.shows_files(self.active) {
            self.active.other()
        } else {
            self.active
        }
    }

    pub fn active_panel_mut(&mut self) -> &mut Panel {
        &mut self.panels[self.active.index()]
    }

    pub fn passive_panel(&self) -> &Panel {
        &self.panels[self.active.other().index()]
    }

    /// The window lost or got the keyboard: keys held now were released in another window.
    pub fn focus_changed(&mut self) {
        self.releases.forget_held();
        self.mouse_focus_lost();
        self.mouse.forget_front();
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Windows reports key releases too; only presses, auto-repeats and Alt codes are actions.
        let Some(key) = self.releases.action(key) else {
            return;
        };
        self.mouse.forget_front();
        let key = keyboard::normalize(key, &keyboard::SYSTEM);
        self.dispatch_key(key);
    }

    /// A key, once it is an action: to work that runs (it waits), F3's count, a dialog, a task in
    /// front, the screen.
    fn dispatch_key(&mut self, key: KeyEvent) {
        if self.editor_input_key(key)
            || self.hold_plugin_key(key)
            || self.hold_key(key)
            || self.counting_key(key)
            || self.plugin_dialog_key(key)
            || self.task_list_key(key)
            || self.switch_screen_key(key)
            || self.window_key(key)
        {
            return;
        }
        if let Some(overlay) = self.overlay.take() {
            self.handle_overlay_key(overlay, key);
            self.resume_dialogs_behind();
            // A plugin's dialog's handler hears each key's change (FAR's `DN_EDITCHANGE` per key).
            self.tell_plugin_dialog();
            return;
        }
        if self.shown_task.is_some() {
            self.handle_transfer_key(key);
            return;
        }
        if !matches!(self.screen, Screen::Panels) && self.text_screen_key(key) {
            return;
        }
        match &mut self.screen {
            Screen::Panels => match self.main_view {
                MainView::Panels => keys::handle_panels_key(self, key),
                MainView::CommandOutput => self.handle_command_output_key(key),
                MainView::Guest => self.handle_guest_key(key),
                MainView::Detached(index) => self.handle_detached_key(index, key),
            },
            Screen::Viewer(_) => self.viewer_key(key),
            Screen::Editor(_) => self.editor_key(key),
        }
    }

    /// Folders read in the background: their results, and their "Reading" counts; folders changed on
    /// disk read again while the panels are on screen.
    fn pump_panels(&mut self) -> bool {
        let shown = self.panels_on_screen();
        let mut changed = false;
        for panel in &mut self.panels {
            let shown = shown && panel.visible;
            changed |= panel.poll_folder_sizes();
            changed |= panel.refresh_if_changed(shown, self.config.auto_update_limit);
            match panel.poll() {
                Polled::Quiet => {}
                Polled::Changed => changed = true,
                Polled::Failed(title, error) => {
                    // The title's "Reading" goes even while a dialog holds the message back.
                    changed = true;
                    self.late_failures.push((title, error));
                }
            }
            if let Some(folder) = panel.take_pushed() {
                self.environment.push_dir(&folder);
            }
        }
        if !self.screen_busy() && !self.late_failures.is_empty() {
            let (title, error) = self.late_failures.remove(0);
            self.show_error(&title, &error);
            changed = true;
        }
        changed
    }

    /// FAR's `IsPanelsActive() && IsVisible()`: the panels are what the user sees, with no dialog, menu,
    /// viewer, editor or running command over them.
    fn panels_on_screen(&self) -> bool {
        matches!(self.screen, Screen::Panels)
            && self.main_view == MainView::Panels
            && self.overlay.is_none()
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.mouse.forget_front();
        self.width = width;
        self.height = height;
        self.clamp_height_decrement();
        self.clamp_width_decrement();
        self.resize_terminal();
    }

    pub fn show_dialog(&mut self, mut dialog: Dialog) {
        self.forget_dialog_place();
        if let Dialog::Form { form, .. } = &mut dialog {
            self.give_histories(form);
        }
        self.overlay = Some(Overlay::Dialog(dialog));
    }

    pub fn show_error(&mut self, title: &str, error: &std::io::Error) {
        self.show_dialog(Dialog::error(title, vec![error.to_string()]));
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
