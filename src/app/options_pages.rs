//! The fields of each page of Options, with FAR's labels (`far/farlang.templ.m4`): where each value
//! lives and what changing it does.

use super::App;
use super::options::{MenuClick, OptionsPage};
use crate::platform;
use crate::terminal;

/// One field of a settings dialog.
pub enum Field {
    Check {
        label: &'static str,
        get: fn(&App) -> bool,
        set: fn(&mut App, bool),
    },
    /// FAR's three-state box: off, on, `[?]` (`None`).
    Tristate {
        label: &'static str,
        get: fn(&App) -> Option<bool>,
        set: fn(&mut App, Option<bool>),
    },
    /// A label and, under it, a list to choose from.
    Choice {
        label: &'static str,
        lines: fn(&App) -> Vec<String>,
        get: fn(&App) -> usize,
        set: fn(&mut App, usize),
    },
    /// A button that opens another dialog; this one comes back when that is done.
    Button {
        item: fn(u16, u16) -> super::form::Item,
        /// Given the dialog as it was left, to come back to.
        press: fn(&mut App, &super::form::Form),
    },
    /// A label and a number beside it.
    Number {
        label: &'static str,
        width: u16,
        get: fn(&App) -> usize,
        set: fn(&mut App, usize),
    },
    /// A label and a line of text under it (FAR's `AddText` and `AddEditField`).
    Text {
        label: &'static str,
        width: u16,
        get: fn(&App) -> String,
        set: fn(&mut App, String),
    },
    /// A check box and a number under it, 0 when it is off.
    Limit {
        label: &'static str,
        text: &'static str,
        get: fn(&App) -> usize,
        set: fn(&mut App, usize),
    },
}

#[path = "options_editor.rs"]
mod editor_pages;
#[path = "options_viewer.rs"]
mod viewer_pages;

fn check(label: &'static str, get: fn(&App) -> bool, set: fn(&mut App, bool)) -> Field {
    Field::Check { label, get, set }
}

/// The title and the fields of a page.
pub fn page(options: OptionsPage) -> (&'static str, Vec<Field>) {
    match options {
        OptionsPage::System => ("System settings", system()),
        OptionsPage::Panel => ("Panel settings", panel()),
        OptionsPage::Tree => ("Tree settings", tree()),
        OptionsPage::Interface => ("Interface settings", interface()),
        OptionsPage::Dialog => ("Dialog settings", dialog()),
        OptionsPage::Menu => ("Menu settings", menu()),
        OptionsPage::CommandLine => ("Command line settings", command_line()),
        OptionsPage::AutoComplete => ("AutoComplete settings", auto_complete()),
        OptionsPage::InfoPanel => ("InfoPanel settings", info_panel()),
        OptionsPage::Confirmations => ("Confirmations", confirmations()),
        OptionsPage::Viewer => ("Viewer", viewer_pages::viewer()),
        OptionsPage::ViewerLocal => ("Viewer", viewer_pages::viewer_local()),
        OptionsPage::Editor => ("Editor", editor_pages::editor()),
        OptionsPage::EditorLocal => ("Editor", editor_pages::editor_local()),
    }
}

fn system() -> Vec<Field> {
    vec![
        check(
            "&Delete to Recycle Bin",
            |app| app.config.delete_settings.to_recycle_bin,
            |app, on| app.config.delete_settings.to_recycle_bin = on,
        ),
        check(
            "Save commands &history",
            |app| app.prefs.save_history,
            |app, on| app.prefs.save_history = on,
        ),
        check(
            "Save &folders history",
            |app| app.prefs.save_folders_history,
            |app, on| app.prefs.save_folders_history = on,
        ),
        check(
            "Save &view and edit history",
            |app| app.prefs.save_view_history,
            |app, on| app.prefs.save_view_history = on,
        ),
        check(
            "Automatic update of &environment variables",
            |app| app.prefs.update_environment,
            |app, on| app.prefs.update_environment = on,
        ),
        // The program's own (ADR-0010): FAR's operations do not go to the background.
        Field::Number {
            label: "Keep finished &tasks, seconds:",
            width: 5,
            get: |app| usize::try_from(app.tasks.keep_done.as_secs()).unwrap_or(usize::MAX),
            set: |app, seconds| {
                let seconds = u64::try_from(seconds).unwrap_or(u64::MAX);
                app.tasks.keep_done = std::time::Duration::from_secs(seconds);
            },
        },
    ]
}

fn panel() -> Vec<Field> {
    vec![
        check(
            "Show &hidden and system files",
            |app| app.show_hidden,
            App::set_show_hidden,
        ),
        check(
            "Hi&ghlight files",
            |app| app.panels[0].options.highlight,
            |app, on| app.panels.iter_mut().for_each(|p| p.options.highlight = on),
        ),
        check(
            "Select &folders",
            |app| app.panels[0].options.select_folders,
            |app, on| {
                app.panels
                    .iter_mut()
                    .for_each(|p| p.options.select_folders = on);
            },
        ),
        check(
            "Right clic&k selects files",
            |app| app.panels[0].options.right_click_select,
            |app, on| {
                app.panels
                    .iter_mut()
                    .for_each(|p| p.options.right_click_select = on);
            },
        ),
        check(
            "Allow re&verse sort modes",
            |app| app.panels[0].options.allow_reverse,
            |app, on| {
                app.panels
                    .iter_mut()
                    .for_each(|p| p.options.allow_reverse = on);
            },
        ),
        Field::Limit {
            label: "&Disable automatic panel update",
            text: "if ob&ject count exceeds",
            get: |app| app.config.auto_update_limit,
            set: |app, limit| app.config.auto_update_limit = limit,
        },
        check(
            "Network drives autor&efresh",
            |app| app.config.auto_update_remote_drives,
            App::set_auto_update_remote,
        ),
        check(
            "Show scroll&bar",
            |app| app.prefs.panel_scrollbar,
            |app, on| app.prefs.panel_scrollbar = on,
        ),
    ]
}

/// FAR's `TreeSettings` (FAR 3 builds it without `TREEFILE_PROJECT`'s file names and places).
fn tree() -> Vec<Field> {
    vec![
        check(
            "&Auto change folder",
            |app| app.prefs.tree.auto_change_folder,
            |app, on| app.prefs.tree.auto_change_folder = on,
        ),
        Field::Number {
            label: "&Minimum number of folders:",
            width: 3,
            get: |app| app.prefs.tree.min_tree_count,
            set: |app, count| app.prefs.tree.min_tree_count = count,
        },
    ]
}

fn interface() -> Vec<Field> {
    vec![
        check(
            "&Clock",
            |app| app.prefs.clock,
            |app, on| app.prefs.clock = on,
        ),
        check("M&ouse", |app| app.mouse.enabled, App::set_mouse),
        check(
            "Show &key bar",
            |app| app.show_key_bar,
            |app, on| {
                app.show_key_bar = on;
                app.clamp_height_decrement();
            },
        ),
        check(
            "Show total &delete progress indicator",
            |app| app.config.delete_settings.show_total,
            |app, on| app.config.delete_settings.show_total = on,
        ),
        // FAR's three states: off, on, `[?]` (2: a network drive's share in the network plugin first).
        Field::Tristate {
            label: "Use Ctrl+Pg&Up to change drive",
            get: |app| match app.prefs.ctrl_pg_up_changes_drive {
                0 => Some(false),
                1 => Some(true),
                _ => None,
            },
            set: |app, state| {
                app.prefs.ctrl_pg_up_changes_drive = state.map_or(2, u8::from);
            },
        },
    ]
}

fn dialog() -> Vec<Field> {
    vec![
        check(
            "&History in dialog edit controls",
            |app| app.prefs.dialogs.edit_history,
            |app, on| app.prefs.dialogs.edit_history = on,
        ),
        check(
            "Mouse click &outside a dialog closes it",
            |app| app.prefs.click_outside_dialog,
            |app, on| app.prefs.click_outside_dialog = on,
        ),
    ]
}

fn click_lines(_: &App) -> Vec<String> {
    ["Cancel menu", "Execute selected item", "Do nothing"]
        .map(String::from)
        .to_vec()
}

fn click_index(click: MenuClick) -> usize {
    MenuClick::ALL
        .iter()
        .position(|known| *known == click)
        .unwrap_or(0)
}

fn click_at(index: usize) -> MenuClick {
    MenuClick::ALL
        .get(index)
        .copied()
        .unwrap_or(MenuClick::Cancel)
}

fn menu() -> Vec<Field> {
    vec![
        Field::Choice {
            label: "&Left mouse click outside a menu",
            lines: click_lines,
            get: |app| click_index(app.prefs.menu_left),
            set: |app, index| app.prefs.menu_left = click_at(index),
        },
        Field::Choice {
            label: "&Right mouse click outside a menu",
            lines: click_lines,
            get: |app| click_index(app.prefs.menu_right),
            set: |app, index| app.prefs.menu_right = click_at(index),
        },
        Field::Choice {
            label: "&Middle mouse click outside a menu",
            lines: click_lines,
            get: |app| click_index(app.prefs.menu_middle),
            set: |app, index| app.prefs.menu_middle = click_at(index),
        },
    ]
}

fn command_line() -> Vec<Field> {
    let shells = Field::Button {
        item: super::user_shells::shells_button,
        press: App::open_user_shells,
    };
    let default_shell = Field::Choice {
        label: "Default &shell",
        lines: |app| {
            terminal::available_shells(&app.user_shells)
                .iter()
                .map(|shell| shell.title().to_string())
                .collect()
        },
        get: |app| {
            terminal::available_shells(&app.user_shells)
                .iter()
                .position(|shell| shell.id() == app.shell.id())
                .unwrap_or(0)
        },
        set: |app, index| {
            let shell = terminal::available_shells(&app.user_shells)
                .into_iter()
                .nth(index);
            if let Some(shell) = shell.filter(|shell| *shell != app.shell) {
                app.choose_guest_shell(shell);
            }
        },
    };
    vec![default_shell, shells]
}

fn auto_complete() -> Vec<Field> {
    vec![check(
        "&Show a list",
        |app| app.prefs.completion_list,
        |app, on| app.prefs.completion_list = on,
    )]
}

/// FAR's `InfoPanelSettings`.
fn info_panel() -> Vec<Field> {
    use crate::platform::system::{ComputerNameFormat, UserNameFormat};
    vec![
        check(
            "Show &power status",
            |app| app.info_sections().power,
            App::set_info_power,
        ),
        check(
            "Show &CD drive parameters",
            |app| app.config.info_options.show_cd_info,
            |app, on| app.set_info_options(|options| options.show_cd_info = on),
        ),
        Field::Choice {
            label: "Computer &name format",
            lines: |_| COMPUTER_NAMES.map(String::from).to_vec(),
            get: |app| {
                (ComputerNameFormat::ALL.iter())
                    .position(|format| *format == app.config.info_options.names.computer)
                    .unwrap_or(0)
            },
            set: |app, at| {
                let format = ComputerNameFormat::ALL.get(at).copied().unwrap_or_default();
                app.set_info_options(|options| options.names.computer = format);
            },
        },
        Field::Choice {
            label: "&User name format",
            lines: |_| USER_NAMES.map(String::from).to_vec(),
            get: |app| {
                (UserNameFormat::ALL.iter())
                    .position(|format| *format == app.config.info_options.names.user)
                    .unwrap_or(0)
            },
            set: |app, at| {
                let format = UserNameFormat::ALL.get(at).copied().unwrap_or_default();
                app.set_info_options(|options| options.names.user = format);
            },
        },
    ]
}

/// FAR's `CNListItems`, in `ComputerNameFormat::ALL`'s order.
const COMPUTER_NAMES: [&str; 8] = [
    "NetBIOS",
    "DNS hostname",
    "DNS domain",
    "DNS fully-qualified (FQDN)",
    "Physical NetBIOS",
    "Physical DNS hostname",
    "Physical DNS domain",
    "Physical DNS fully-qualified (FQDN)",
];

/// FAR's `UNListItems`, in `UserNameFormat::ALL`'s order (its "Principial" spelt right).
const USER_NAMES: [&str; 11] = [
    "Logon name",
    "Fully Qualified Domain Name (FQDN)",
    "Sam Compatible",
    "Display Name",
    "Unique Id",
    "Canonical Name",
    "User Principal Name",
    "Service Principal",
    "DNS domain",
    "Given name",
    "Surname",
];

fn confirmations() -> Vec<Field> {
    vec![
        check(
            "&Overwrite and delete R/O files",
            |app| app.config.delete_settings.confirm_read_only,
            |app, on| app.config.delete_settings.confirm_read_only = on,
        ),
        check(
            "&Drag and drop",
            |app| app.prefs.confirm_drag,
            |app, on| app.prefs.confirm_drag = on,
        ),
        check(
            "De&lete",
            |app| app.config.delete_settings.confirm,
            |app, on| app.config.delete_settings.confirm = on,
        ),
        check(
            "Delete non-empty &folders",
            |app| app.config.delete_settings.confirm_folders,
            |app, on| app.config.delete_settings.confirm_folders = on,
        ),
        check(
            "&Interrupt operation",
            |app| app.prefs.confirm_interrupt,
            |app, on| app.prefs.confirm_interrupt = on,
        ),
        check(
            "Disconnect &network drive",
            |app| app.prefs.confirm_connection,
            |app, on| app.prefs.confirm_connection = on,
        ),
        check(
            "Delete &SUBST-disk",
            |app| app.prefs.confirm_subst,
            |app, on| app.prefs.confirm_subst = on,
        ),
        check(
            "Detach &virtual disk",
            |app| app.prefs.confirm_virtual,
            |app, on| app.prefs.confirm_virtual = on,
        ),
        check(
            "Hot&Plug-device removal",
            |app| app.prefs.confirm_hotplug,
            |app, on| app.prefs.confirm_hotplug = on,
        ),
        check(
            "&Reload edited file",
            |app| app.prefs.confirm_reedit,
            |app, on| app.prefs.confirm_reedit = on,
        ),
        check(
            "Clear &history list",
            |app| app.prefs.confirm_history_clear,
            |app, on| app.prefs.confirm_history_clear = on,
        ),
        check(
            "E&xit",
            |app| app.prefs.confirm_exit,
            |app, on| app.prefs.confirm_exit = on,
        ),
    ]
}

impl App {
    fn set_show_hidden(&mut self, show: bool) {
        if self.show_hidden == show {
            return;
        }
        self.show_hidden = show;
        for panel in &mut self.panels {
            panel.set_show_hidden(show);
        }
    }

    /// Takes effect when a panel next reads its folder (FAR updates the panels after the dialog).
    fn set_auto_update_remote(&mut self, on: bool) {
        if self.config.auto_update_remote_drives == on {
            return;
        }
        self.config.auto_update_remote_drives = on;
        for panel in &mut self.panels {
            panel.set_watch_remote(on);
            panel.reload();
        }
    }

    /// FAR's `SetFarConsoleMode` after the Interface dialog.
    fn set_mouse(&mut self, on: bool) {
        if self.mouse.enabled != on {
            self.mouse.enabled = on;
            // A console that refuses keeps the mode it had: the keys go on working either way.
            platform::input::set_console_mouse(on).ok();
        }
    }

    fn info_sections(&self) -> super::InfoSections {
        self.info_panel
            .as_ref()
            .map_or(self.remembered.info_sections, |info| info.sections)
    }

    /// Changed options: the info panel read again with them (FAR redraws it after the dialog).
    fn set_info_options(&mut self, change: impl FnOnce(&mut crate::vfs::local::InfoOptions)) {
        let before = self.config.info_options;
        change(&mut self.config.info_options);
        if self.config.info_options != before {
            self.reread_info_panel();
        }
    }

    fn set_info_power(&mut self, on: bool) {
        self.remembered.info_sections.power = on;
        if let Some(info) = &mut self.info_panel {
            info.sections.power = on;
        }
    }
}
