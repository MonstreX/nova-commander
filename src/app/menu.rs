//! The F9 menu bar with its pull-down menus, as FAR builds it (`far/config.cpp` `ShowMainMenu`): item
//! labels with `&` hotkeys, accelerator keys, separators. Only items this program can already do are
//! listed; the rest of FAR's items come with their stages (docs/far-parity.md section 9).

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::ViewMode;
use super::command::{Command, Side};
use super::options::OptionsPage;
use crate::settings::history::List;

/// A menu line: an item or a separator (`LIF_SEPARATOR`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Item {
        /// Label with `&` before the hotkey letter, as in FAR's language file.
        label: &'static str,
        /// Accelerator shown on the right ("" for none).
        key: &'static str,
        command: Command,
    },
    Separator,
}

impl MenuItem {
    pub fn command(&self) -> Option<Command> {
        match self {
            Self::Item { command, .. } => Some(*command),
            Self::Separator => None,
        }
    }

    /// The hotkey letter (after `&`), lower-case.
    pub fn hotkey(&self) -> Option<char> {
        let Self::Item { label, .. } = self else {
            return None;
        };
        super::vmenu_keys::marked_hotkey(label)
    }
}

pub struct Menu {
    /// Title with `&` before the hotkey letter.
    pub title: &'static str,
    pub items: &'static [MenuItem],
}

const fn item(label: &'static str, key: &'static str, command: Command) -> MenuItem {
    MenuItem::Item {
        label,
        key,
        command,
    }
}

const SEPARATOR: MenuItem = MenuItem::Separator;

/// The "Left" and "Right" menus differ only in the panel they act on and the drive key.
const fn panel_items(
    side: Side,
    toggle_key: &'static str,
    drive_key: &'static str,
) -> [MenuItem; 15] {
    [
        item(
            "&Brief",
            "Ctrl+1",
            Command::SetViewMode(side, ViewMode::Brief),
        ),
        item(
            "&Medium",
            "Ctrl+2",
            Command::SetViewMode(side, ViewMode::Medium),
        ),
        item(
            "&Full",
            "Ctrl+3",
            Command::SetViewMode(side, ViewMode::Full),
        ),
        item(
            "&Wide",
            "Ctrl+4",
            Command::SetViewMode(side, ViewMode::Wide),
        ),
        item(
            "Detai&led",
            "Ctrl+5",
            Command::SetViewMode(side, ViewMode::Detailed),
        ),
        item(
            "&Alternative full",
            "Ctrl+0",
            Command::SetViewMode(side, ViewMode::AlternativeFull),
        ),
        SEPARATOR,
        item("&Info panel", "Ctrl+L", Command::InfoPanel(side)),
        item("&Tree panel", "Ctrl+T", Command::TreePanel(side)),
        item("Quick &view", "Ctrl+Q", Command::QuickView(side)),
        SEPARATOR,
        item("&Sort modes", "Ctrl+F12", Command::SortMenu(side)),
        item("Panel &On/Off", toggle_key, Command::TogglePanel(side)),
        item("&Re-read", "Ctrl+R", Command::Reread(side)),
        item("&Change drive", drive_key, Command::ChangeDrive(side)),
    ]
}

const LEFT_ITEMS: [MenuItem; 15] = panel_items(Side::Left, "Ctrl+F1", "Alt+F1");
const RIGHT_ITEMS: [MenuItem; 15] = panel_items(Side::Right, "Ctrl+F2", "Alt+F2");

const FILES_ITEMS: [MenuItem; 15] = [
    item("&View", "F3", Command::View),
    item("&Edit", "F4", Command::Edit),
    item("&Copy", "F5", Command::Copy),
    item("&Rename or move", "F6", Command::Move),
    item("Lin&k", "Alt+F6", Command::Link),
    item("&Make folder", "F7", Command::MakeDir),
    item("&Delete", "F8", Command::Delete),
    SEPARATOR,
    item("File &attributes", "Ctrl+A", Command::Attributes),
    item("A&pply command", "Ctrl+G", Command::Apply),
    SEPARATOR,
    item("Select &group", "Gray +", Command::SelectGroup),
    item("U&nselect group", "Gray -", Command::UnselectGroup),
    item("&Invert selection", "Gray *", Command::InvertSelection),
    item("Re&store selection", "Ctrl+M", Command::RestoreSelection),
];

const COMMANDS_ITEMS: [MenuItem; 19] = [
    item("&Find file", "Alt+F7", Command::FindFile),
    item("&History", "Alt+F8", Command::History(List::Commands)),
    item("Fi&nd folder", "Alt+F10", Command::FindFolder),
    item(
        "File vie&w history",
        "Alt+F11",
        Command::History(List::Views),
    ),
    item(
        "F&olders history",
        "Alt+F12",
        Command::History(List::Folders),
    ),
    SEPARATOR,
    item("&Swap panels", "Ctrl+U", Command::SwapPanels),
    item("&Panels On/Off", "Ctrl+O", Command::TogglePanels),
    SEPARATOR,
    item("Edit &user menu", "", Command::EditUserMenu),
    item("File &associations", "", Command::FileAssociations),
    item("Fol&der shortcuts", "", Command::FolderShortcuts),
    item("File panel f&ilter", "Ctrl+I", Command::Filter),
    SEPARATOR,
    item("Sc&reens list", "F12", Command::Screens),
    item("Task &list", "Ctrl+W", Command::TaskList),
    SEPARATOR,
    item("&Guest shell", "Alt+Shift+T", Command::ToggleGuestShell),
    item("Choose sh&ell", "", Command::ChooseShell),
];

const OPTIONS_ITEMS: [MenuItem; 22] = [
    item(
        "S&ystem settings",
        "",
        Command::Options(OptionsPage::System),
    ),
    item("&Panel settings", "", Command::Options(OptionsPage::Panel)),
    // FAR's labels: T is AutoComplete's too, the first of them takes it (FAR hides this one by default).
    item("&Tree settings", "", Command::Options(OptionsPage::Tree)),
    item(
        "&Interface settings",
        "",
        Command::Options(OptionsPage::Interface),
    ),
    // FAR's place: after Interface settings and Languages (`config.cpp` `OptionsMenu`).
    item("Pl&ugins configuration", "", Command::PluginsConfig),
    item("Plu&gins", "", Command::Plugins),
    item(
        "Di&alog settings",
        "",
        Command::Options(OptionsPage::Dialog),
    ),
    item("Menu settings", "", Command::Options(OptionsPage::Menu)),
    item(
        "&Command line settings",
        "",
        Command::Options(OptionsPage::CommandLine),
    ),
    item(
        "Au&toComplete settings",
        "",
        Command::Options(OptionsPage::AutoComplete),
    ),
    item(
        "Inf&oPanel settings",
        "",
        Command::Options(OptionsPage::InfoPanel),
    ),
    item("Groups of file mas&ks", "", Command::MaskGroups),
    SEPARATOR,
    item(
        "Co&nfirmations",
        "",
        Command::Options(OptionsPage::Confirmations),
    ),
    item("File panel &modes", "", Command::PanelModes),
    item("&Folder description files", "", Command::DescriptionFiles),
    SEPARATOR,
    item(
        "&Viewer settings",
        "",
        Command::Options(OptionsPage::Viewer),
    ),
    item(
        "&Editor settings",
        "",
        Command::Options(OptionsPage::Editor),
    ),
    SEPARATOR,
    item("T&hemes", "", Command::ChooseTheme),
    item("&Save setup", "Shift+F9", Command::SaveSetup),
];

pub const MENUS: [Menu; 5] = [
    Menu {
        title: "&Left",
        items: &LEFT_ITEMS,
    },
    Menu {
        title: "&Files",
        items: &FILES_ITEMS,
    },
    Menu {
        title: "&Commands",
        items: &COMMANDS_ITEMS,
    },
    Menu {
        title: "&Options",
        items: &OPTIONS_ITEMS,
    },
    Menu {
        title: "&Right",
        items: &RIGHT_ITEMS,
    },
];

/// Where each title of the menu bar starts (FAR's `XPos`): two columns in, each `  Title  ` after
/// the one before.
pub fn title_positions() -> Vec<u16> {
    let mut x = 2u16;
    MENUS
        .iter()
        .map(|menu| {
            let at = x;
            let shown = menu.title.chars().filter(|&ch| ch != '&').count() + 4;
            x = x.saturating_add(u16::try_from(shown).unwrap_or(u16::MAX));
            at
        })
        .collect()
}

/// Which pull-down menu is open and which item is highlighted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuState {
    pub menu: usize,
    pub item: usize,
    /// FAR's `TurnOffCompletely`: the tree's items are hidden (`LIF_HIDDEN`).
    pub no_tree: bool,
    /// The left and right panels' providers' own modes at Ctrl+6..9 (bit 0: Ctrl+6): listed after
    /// Detailed as FAR lists its modes 6..9 (the program has none of its own there yet).
    pub provider_modes: [u8; 2],
}

impl MenuState {
    /// Opens the menu of the active panel's side, as FAR does.
    pub fn for_side(side: Side) -> Self {
        let menu = match side {
            Side::Left => 0,
            Side::Right => MENUS.len() - 1,
        };
        Self {
            menu,
            item: 0,
            no_tree: false,
            provider_modes: [0; 2],
        }
    }

    /// The open menu's items, without those hidden; a panel's menu with its provider's modes 6..9.
    pub fn items(&self) -> Vec<MenuItem> {
        let side = match self.menu {
            0 => Some(Side::Left),
            menu if menu == MENUS.len() - 1 => Some(Side::Right),
            _ => None,
        };
        let mut items: Vec<MenuItem> = (MENUS[self.menu].items.iter())
            .filter(|item| {
                !(self.no_tree
                    && matches!(
                        item.command(),
                        Some(
                            Command::TreePanel(_)
                                | Command::Options(OptionsPage::Tree)
                                | Command::FindFolder
                        )
                    ))
            })
            .copied()
            .collect();
        if let Some(side) = side {
            let modes = self.provider_modes[side.index()];
            // After Detailed (Ctrl+5), before Alternative full (Ctrl+0), as FAR's `RealModeToDisplay`.
            let at = (items.iter())
                .position(|item| {
                    item.command() == Some(Command::SetViewMode(side, ViewMode::Detailed))
                })
                .map_or(0, |at| at + 1);
            let own = (PROVIDER_MODES.iter().enumerate().rev())
                .filter(|(bit, _)| modes & (1 << bit) != 0)
                .map(|(bit, (label, key))| {
                    item(
                        label,
                        key,
                        Command::SetViewMode(side, ViewMode::Provider(6 + bit)),
                    )
                });
            for added in own {
                items.insert(at, added);
            }
        }
        items
    }
}

/// FAR's names of its modes 6..9 (`MMenuDizView` ...) and their keys.
const PROVIDER_MODES: [(&str, &str); 4] = [
    ("&Descriptions", "Ctrl+6"),
    ("Lon&g descriptions", "Ctrl+7"),
    ("File own&ers", "Ctrl+8"),
    ("File lin&ks", "Ctrl+9"),
];

pub enum MenuOutcome {
    Stay,
    Close,
    Run(Command),
}

pub fn handle_key(state: &mut MenuState, key: KeyEvent) -> MenuOutcome {
    let items = state.items();
    let items = items.as_slice();
    match key.code {
        KeyCode::Left => switch_menu(state, MENUS.len() - 1),
        KeyCode::Right => switch_menu(state, 1),
        KeyCode::Up => state.item = step(items, state.item, items.len() - 1),
        KeyCode::Down => state.item = step(items, state.item, 1),
        KeyCode::Home => state.item = step(items, items.len() - 1, 1),
        KeyCode::End => state.item = step(items, 0, items.len() - 1),
        KeyCode::Enter => {
            if let Some(command) = items.get(state.item).and_then(MenuItem::command) {
                return MenuOutcome::Run(command);
            }
        }
        KeyCode::Esc | KeyCode::F(9) | KeyCode::F(10) => return MenuOutcome::Close,
        KeyCode::Char(ch) => {
            let ch = super::vmenu_keys::lower(ch);
            if let Some((index, command)) = items
                .iter()
                .enumerate()
                .find(|(_, item)| item.hotkey() == Some(ch))
                .and_then(|(index, item)| Some((index, item.command()?)))
            {
                state.item = index;
                return MenuOutcome::Run(command);
            }
        }
        _ => {}
    }
    MenuOutcome::Stay
}

/// Left/Right: the next menu opens on its first item, as FAR's menus do (`LIF_SELECTED`).
fn switch_menu(state: &mut MenuState, offset: usize) {
    state.menu = (state.menu + offset) % MENUS.len();
    let items = state.items();
    state.item = step(&items, items.len() - 1, 1);
}

/// The next item `offset` positions away (wrapping), skipping separators.
fn step(items: &[MenuItem], from: usize, offset: usize) -> usize {
    let mut index = from;
    for _ in 0..items.len() {
        index = (index + offset) % items.len();
        if items[index] != MenuItem::Separator {
            return index;
        }
    }
    from
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyModifiers;

    fn press(state: &mut MenuState, code: KeyCode) -> MenuOutcome {
        handle_key(state, KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn navigation_wraps_around_menus_and_items() {
        let mut state = MenuState::for_side(Side::Left);
        press(&mut state, KeyCode::Left);
        assert_eq!(state.menu, MENUS.len() - 1);
        press(&mut state, KeyCode::Up);
        assert_eq!(state.item, MENUS[state.menu].items.len() - 1);
        press(&mut state, KeyCode::Down);
        assert_eq!(state.item, 0);
    }

    #[test]
    fn separators_are_skipped() {
        let mut state = MenuState {
            menu: 0,
            item: 5,
            no_tree: false,
            provider_modes: [0; 2],
        };
        press(&mut state, KeyCode::Down);
        assert_eq!(
            state.item, 7,
            "Alternative full -> Info panel, over the separator"
        );
        press(&mut state, KeyCode::Up);
        assert_eq!(state.item, 5);
    }

    #[test]
    fn switching_menus_starts_on_the_first_item() {
        let mut state = MenuState {
            menu: 0,
            item: 9,
            no_tree: false,
            provider_modes: [0; 2],
        };
        press(&mut state, KeyCode::Right);
        assert_eq!((state.menu, state.item), (1, 0));
    }

    #[test]
    fn enter_runs_the_highlighted_command() {
        let mut state = MenuState {
            menu: 1,
            item: 0,
            no_tree: false,
            provider_modes: [0; 2],
        };
        let outcome = press(&mut state, KeyCode::Enter);
        assert!(matches!(outcome, MenuOutcome::Run(Command::View)));
    }

    #[test]
    fn hotkey_letters_run_items() {
        let mut state = MenuState {
            menu: 0,
            item: 0,
            no_tree: false,
            provider_modes: [0; 2],
        };
        let outcome = press(&mut state, KeyCode::Char('L'));
        assert!(matches!(
            outcome,
            MenuOutcome::Run(Command::SetViewMode(Side::Left, ViewMode::Detailed))
        ));
        assert_eq!(MENUS[2].items[11].hotkey(), Some('d'));
        assert_eq!(MENUS[2].items[12].hotkey(), Some('i'));
        assert_eq!(MENUS[2].items[15].hotkey(), Some('l'), "FAR's Task list");
        assert_eq!(MENUS[2].items[15].command(), Some(Command::TaskList));
        assert_eq!(MENUS[2].items[17].hotkey(), Some('g'));
        assert_eq!(
            MENUS[2].items[18].hotkey(),
            Some('e'),
            "Choose shell: L is FAR's Task list, C its Compare folders"
        );
        assert_eq!(MENUS[2].items[0].hotkey(), Some('f'));
        assert_eq!(MENUS[2].items[1].hotkey(), Some('h'));
        assert_eq!(MENUS[2].items[2].hotkey(), Some('n'), "FAR's Find folder");
        assert_eq!(MenuItem::Separator.hotkey(), None);
    }

    #[test]
    fn with_the_tree_turned_off_its_items_are_hidden() {
        let mut state = MenuState {
            menu: 3,
            item: 0,
            no_tree: true,
            provider_modes: [0; 2],
        };
        assert_eq!(state.items().len(), MENUS[3].items.len() - 1);
        press(&mut state, KeyCode::Down);
        press(&mut state, KeyCode::Down);
        assert_eq!(
            state.items()[state.item].command(),
            Some(Command::Options(OptionsPage::Interface)),
            "System, Panel, then Interface: no Tree settings"
        );
        state.menu = 0;
        assert!(
            state
                .items()
                .iter()
                .all(|item| !matches!(item.command(), Some(Command::TreePanel(_))))
        );
        state.no_tree = false;
        assert_eq!(state.items().len(), MENUS[0].items.len());
        // FAR's Commands > Find folder goes with the tree.
        state.menu = 2;
        state.no_tree = true;
        assert_eq!(state.items().len(), MENUS[2].items.len() - 1);
    }

    #[test]
    fn right_panel_opens_right_menu() {
        let state = MenuState::for_side(Side::Right);
        assert_eq!(MENUS[state.menu].title, "&Right");
    }
}
