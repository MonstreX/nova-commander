//! The plugins' hotkeys, as FAR's (`plugins.cpp` `GetPluginHotKey`, `SetHotKeyDialog`, `CommandsMenu`;
//! `diskmenu.cpp` `AddPluginItems`): none by default, the user's own given with F4 in F11's "Plugin
//! commands" and in the drive menu ("Assign plugin hot key", a letter or a digit; empty or a space takes
//! it away), kept in the settings by the plugin's name, the menu and the item's text (FAR keys them by the
//! item's GUID: our plugins' items have none). F11's menu: with no hotkey given its items' own and
//! automatic ones (`EnableAutoHighlight`), else each "&k  text" or "   text" (`AddHotkey`); sorted by
//! their text; "Shift+F1, F4, F3" under it (F3 "Plugin information", `plugins::info`). Options > Plugins configuration is the same menu of the
//! plugins' configurations (FAR's `Configure`), its hotkeys its own.

use ratatui::crossterm::event::KeyEvent;

use super::super::dialog::{Dialog, FormAction};
use super::super::form::{Control, Form, Item};
use super::super::masked_input::MaskedInput;
use super::super::vmenu_keys::with_hotkey;
use super::super::{App, Overlay, Side};
use super::menu::MenuOutcome;
use super::{PluginMenu, Plugins};
use crate::plugin_host::windows::{MenuLine, MenuRequest};

/// FAR's `Options::SetDriveMenuHotkeys` (`config.cpp:2329`): the drive menu's hotkeys its own plugins get
/// once - ProcList "0", TmpPanel "1", NetBox (ours: NetWay) "2", Network "3" (the plugin, its item, the key).
pub(in crate::app) const DRIVE_MENU_HOTKEYS: [(&str, &str, &str); 4] = [
    ("proclist", "Process list", "0"),
    ("tmppanel", "Temporary panel", "1"),
    ("netway", "NetWay", "2"),
    ("network", "network", "3"),
];

/// FAR's `Interface.InitDriveMenuHotkeys`: false once they were set.
const INIT_DRIVE_MENU_HOTKEYS: &str = "plugins.initDriveMenuHotkeys";

impl Plugins {
    /// The drive menu's default hotkeys, once (FAR's `SetDriveMenuHotkeys`, on its first start): after
    /// that only the user's.
    pub(super) fn set_drive_menu_hotkeys(&mut self) {
        let done = self.kept_settings.get(INIT_DRIVE_MENU_HOTKEYS)
            == Some(&serde_json::Value::Bool(false));
        if done {
            return;
        }
        for (plugin, label, hotkey) in DRIVE_MENU_HOTKEYS {
            let key = format!("plugins.{plugin}.hotkeys.disks.{label}");
            self.kept_settings.insert(key, hotkey.into());
        }
        (self.kept_settings).insert(INIT_DRIVE_MENU_HOTKEYS.to_string(), false.into());
    }
}

/// Which menu a hotkey is for (FAR's `hotkey_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyMenu {
    /// F11's "Plugin commands" (`plugins_menu`).
    Commands,
    /// The drive menu (`drive_menu`).
    Disks,
    /// Options > Plugins configuration (`config_menu`).
    Config,
}

impl HotkeyMenu {
    fn id(self) -> &'static str {
        match self {
            Self::Commands => "commands",
            Self::Disks => "disks",
            Self::Config => "config",
        }
    }
}

/// F4's dialog on an item: whose it is and the menu that comes back after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyEdit {
    pub plugin: usize,
    pub menu: HotkeyMenu,
    pub label: String,
    /// The line the menu comes back at; the drive menu's side.
    pub selected: usize,
    pub side: Side,
}

/// The items of "Assign plugin hot key".
const KEY: usize = 1;

/// The menus' break keys (FAR's `CommandsMenu` and `Configure`).
const F3: &str = "F3";
const F4: &str = "F4";
const SHIFT_F9: &str = "Shift+F9";
const ALT_SHIFT_F9: &str = "Alt+Shift+F9";

/// F11's break keys, or (`config`) "Plugins configuration"'s, in their order.
fn break_keys(config: bool) -> Vec<&'static str> {
    match config {
        true => vec![F4, F3],
        false => vec![F4, SHIFT_F9, ALT_SHIFT_F9, F3],
    }
}
const OK: usize = 4;

impl App {
    /// The settings' key of a plugin item's hotkey.
    fn hotkey_setting(&self, plugin: usize, menu: HotkeyMenu, label: &str) -> Option<String> {
        let name = &self.plugins.host.plugins.get(plugin)?.name;
        Some(format!("plugins.{name}.hotkeys.{}.{label}", menu.id()))
    }

    /// The hotkey the user gave a plugin's item.
    pub(in crate::app) fn plugin_hotkey(
        &self,
        plugin: usize,
        menu: HotkeyMenu,
        label: &str,
    ) -> Option<char> {
        let key = self.hotkey_setting(plugin, menu, label)?;
        let value = self.plugins.kept_settings.get(&key)?.as_str()?;
        value.chars().next()
    }

    /// Whether any item of `menu` has a hotkey (FAR's `HotkeysPresent`).
    /// Only keys of the shape `plugins.<name>.hotkeys.<menu>.<label>` (a plugin's name has no dot): a
    /// plugin's own setting `plugins.<name>.settings.x.hotkeys...` is none.
    fn any_plugin_hotkey(&self, menu: HotkeyMenu) -> bool {
        let part = format!("hotkeys.{}.", menu.id());
        (self.plugins.kept_settings.keys()).any(|key| {
            (key.strip_prefix("plugins."))
                .and_then(|rest| rest.split_once('.'))
                .is_some_and(|(_, rest)| rest.starts_with(&part))
        })
    }

    /// F11: FAR's "Plugin commands".
    pub(in crate::app) fn open_plugin_commands_at(&mut self, selected: usize) {
        self.open_program_plugin_menu(HotkeyMenu::Commands, selected);
    }

    /// F11's "Plugin commands" or Options' "Plugins configuration" (FAR's `CommandsMenu`, `Configure`):
    /// every plugin's items of that menu, sorted by their text, `VMENU_WRAPMODE`, "Shift+F1, F4, F3".
    /// The commands' menu is the editor's while the editor is in front (review D3: back from F3, F4 or
    /// Shift+F9 of the editor's F11 it was the panels', Enter running a panel's command).
    pub(in crate::app) fn open_program_plugin_menu(&mut self, menu: HotkeyMenu, selected: usize) {
        let editor =
            menu == HotkeyMenu::Commands && matches!(self.screen, crate::app::Screen::Editor(_));
        self.open_plugin_menu_of(menu, selected, editor);
    }

    /// F11 in the editor: the plugins' editor items (FAR's `PF_EDITOR`, API 10) as F11's menu.
    pub(in crate::app) fn open_editor_plugin_commands(&mut self) {
        self.open_plugin_menu_of(HotkeyMenu::Commands, 0, true);
    }

    fn open_plugin_menu_of(&mut self, menu: HotkeyMenu, selected: usize, editor: bool) {
        self.plugins.host.refresh_items(super::ITEMS_WAIT);
        let config = menu == HotkeyMenu::Config;
        let mut items: Vec<(String, (usize, usize))> = (self.plugins.host.plugins.iter())
            .enumerate()
            .flat_map(|(plugin, loaded)| {
                let labels = match (config, editor) {
                    (true, _) => &loaded.configs,
                    (false, true) => &loaded.editor,
                    (false, false) => &loaded.commands,
                };
                (labels.iter().enumerate())
                    .map(move |(index, label)| (label.clone(), (plugin, index)))
            })
            .collect();
        // The program's own Colorer among the editor's commands (FarColorer's item).
        if editor && !config {
            let colorer = crate::app::colorer_menu::COLORER;
            items.push((crate::app::colorer_menu::ITEM.to_string(), (colorer, 0)));
        }
        items.sort_by(|(a, _), (b, _)| menu_order(a, b));
        let present = self.any_plugin_hotkey(menu);
        let lines = (items.iter())
            .map(|(label, (plugin, _))| MenuLine {
                text: match present {
                    true => with_hotkey(label, self.plugin_hotkey(*plugin, menu, label)),
                    false => label.clone(),
                },
                ..MenuLine::default()
            })
            .collect();
        // F11's Shift+F9 configures the item's plugin, its Alt+Shift+F9 opens "Plugins configuration"
        // (FAR's `CommandsMenu`).
        let break_keys = break_keys(config).into_iter().map(String::from).collect();
        let title = match config {
            true => "Plugins configuration",
            false => "Plugin commands",
        };
        let request = MenuRequest {
            title: title.to_string(),
            bottom: "Shift+F1, F4, F3".to_string(),
            lines,
            selected,
            break_keys,
            auto_hotkeys: true,
            wrap: true,
            ..MenuRequest::default()
        };
        let mut plugin_menu = PluginMenu::new(request);
        plugin_menu.commands = Some(items.into_iter().map(|(label, at)| (at, label)).collect());
        plugin_menu.config = config;
        plugin_menu.editor = editor;
        self.overlay = Some(Overlay::PluginMenu(Box::new(plugin_menu)));
    }

    /// The keys of F11's menu and of "Plugins configuration" (F3 "Plugin information"): Enter runs the command (configures the
    /// plugin) on its plugin's thread, F4 asks its hotkey, F11's Shift+F9 configures the item's plugin and
    /// its Alt+Shift+F9 opens "Plugins configuration" (on an item, as FAR's).
    pub(in crate::app) fn plugin_commands_key(&mut self, mut menu: Box<PluginMenu>, key: KeyEvent) {
        let outcome = menu.handle_key(key);
        let Some(commands) = menu.commands.take() else {
            return;
        };
        let kind = match menu.config {
            true => HotkeyMenu::Config,
            false => HotkeyMenu::Commands,
        };
        let (item, key) = match outcome {
            MenuOutcome::Esc => {
                if menu.config {
                    self.plugin_configs_closed();
                }
                return;
            }
            MenuOutcome::Chosen {
                item: Some(item),
                key,
            } => (item, key),
            // FAR's F4 with no item does nothing (`if (ItemPtr)`): the menu stays.
            MenuOutcome::Stay | MenuOutcome::Chosen { item: None, .. } => {
                menu.commands = Some(commands);
                self.overlay = Some(Overlay::PluginMenu(menu));
                return;
            }
        };
        let Some(&((plugin, index), ref label)) = commands.get(item) else {
            return;
        };
        let selected = menu.selected;
        let name = key.and_then(|key| break_keys(menu.config).get(key).copied());
        if plugin == crate::app::colorer_menu::COLORER {
            // No plugin's: its keys but Enter do nothing.
            match name {
                None => self.open_colorer_menu(),
                Some(_) => self.open_plugin_menu_of(kind, selected, true),
            }
            return;
        }
        match name {
            Some(F3) => self.show_plugin_info((plugin, label), (kind, selected, self.active)),
            Some(F4) => self.ask_plugin_hotkey(HotkeyEdit {
                plugin,
                menu: kind,
                label: label.clone(),
                selected,
                side: self.active,
            }),
            Some(SHIFT_F9) => self.configure_plugin(plugin, 0, (HotkeyMenu::Commands, selected)),
            Some(_) => self.open_plugin_configs_from_commands(selected),
            None if menu.config => self.configure_plugin(plugin, index, (kind, selected)),
            None if menu.editor => self.run_editor_plugin_command(plugin, index),
            None => {
                let at: Vec<(usize, usize)> = commands.iter().map(|(at, _)| *at).collect();
                self.run_plugin_command(&at, item);
            }
        }
    }

    /// FAR's `SetHotKeyDialog`: "Assign plugin hot key" for the item `edit` names.
    pub(in crate::app) fn ask_plugin_hotkey(&mut self, edit: HotkeyEdit) {
        let current = self.plugin_hotkey(edit.plugin, edit.menu, &edit.label);
        let items = vec![
            Item::new(
                5,
                2,
                Control::Text("Enter hot key (letter or digit)".to_string()),
            ),
            // FAR's `AddFixEditField(strHotKey, 1)`: one place, typed over, the cursor on it.
            Item::new(
                5,
                3,
                Control::Masked(MaskedInput::new(
                    "X",
                    &current.map(String::from).unwrap_or_default(),
                )),
            ),
            Item::new(7, 3, Control::Label(edit.label.clone())),
            Item::new(0, 4, Control::Separator),
            Item::new(
                0,
                5,
                Control::Button {
                    label: "OK".to_string(),
                    default: true,
                },
            ),
            Item::new(
                0,
                5,
                Control::Button {
                    label: "Cancel".to_string(),
                    default: false,
                },
            ),
        ];
        let width = u16::try_from(edit.label.chars().count() + 14)
            .unwrap_or(u16::MAX)
            .max(44);
        let form = Form::new("Assign plugin hot key", width, 7, items);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::PluginHotkey(Box::new(edit)),
        });
    }

    /// The hotkey's dialog closed: OK keeps the key typed (empty or a space: none); the menu comes back.
    pub(in crate::app) fn finish_plugin_hotkey(
        &mut self,
        edit: HotkeyEdit,
        form: &Form,
        button: Option<usize>,
    ) {
        if button == Some(OK)
            && let Some(key) = self.hotkey_setting(edit.plugin, edit.menu, &edit.label)
        {
            match form
                .masked_text(KEY)
                .chars()
                .next()
                .filter(|key| *key != ' ')
            {
                Some(typed) => {
                    self.plugins
                        .kept_settings
                        .insert(key, typed.to_string().into());
                }
                None => {
                    self.plugins.kept_settings.remove(&key);
                }
            }
        }
        match edit.menu {
            HotkeyMenu::Commands | HotkeyMenu::Config => {
                self.open_program_plugin_menu(edit.menu, edit.selected);
            }
            HotkeyMenu::Disks => self.reopen_drive_menu(edit.side, edit.selected),
        }
    }
}

/// FAR's order of F11's items (`VMenu::SortItems`, `vmenu.cpp:3500-3513`, review R42): their texts
/// without the hotkey marks (`remove_highlight`), compared as names are (`string_sort::less`:
/// linguistic, case-insensitive, digits as numbers).
pub(in crate::app) fn menu_order(a: &str, b: &str) -> std::cmp::Ordering {
    use crate::app::user_menu::without_marks;
    crate::platform::files::compare_names(&without_marks(a), &without_marks(b))
}

#[cfg(test)]
#[path = "hotkeys_tests.rs"]
mod tests;
