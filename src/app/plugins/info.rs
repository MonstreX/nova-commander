//! F3 in F11's "Plugin commands", in "Plugins configuration" and on a plugin's item of the drive menu
//! (FAR's `ShowPluginInfo`, `plugins.cpp`
//! 1724-1767): "Plugin information" with the plugin's title, description, author, version, module path,
//! its identity and the item's, and its command prefixes, each in a line that can be read and copied but
//! not changed (`AddConstEditField`, 36 wide), then OK; the menu comes back after it. FAR's plugins have
//! UUIDs; ours are known by their names, so those two lines name the plugin and the item (deviation D20).

use super::super::dialog::{Dialog, FormAction};
use super::super::form::{Control, Form, Item};
use super::super::line_input::LineInput;
use super::super::{App, Side};
use super::HotkeyMenu;

/// FAR's field width.
const WIDTH: u16 = 36;

impl App {
    /// "Plugin information" for the plugin `plugin`'s item `label`; the menu `menu` (the drive menu: of
    /// `side`) back at `selected` after it.
    pub(in crate::app) fn show_plugin_info(
        &mut self,
        (plugin, label): (usize, &str),
        (menu, selected, side): (HotkeyMenu, usize, Side),
    ) {
        let Some(loaded) = self.plugins.host.plugins.get(plugin) else {
            return;
        };
        let lines = [
            ("&Title:", loaded.info.title.clone()),
            ("&Description:", loaded.info.description.clone()),
            ("&Author:", loaded.info.author.clone()),
            ("&Version:", loaded.info.version.clone()),
            ("&Module path:", loaded.library.display().to_string()),
            ("Plugin &name:", loaded.name.clone()),
            ("Plugin &item:", label.to_string()),
            ("Plugin &prefix:", loaded.prefixes.join(":")),
        ];
        let mut items = Vec::new();
        let mut row = 2;
        for (title, value) in lines {
            items.push(Item::new(5, row, Control::Label(title.to_string())));
            let field = Control::Edit(LineInput::with_text(&value));
            items.push(Item::new(5, row + 1, field).wide(WIDTH).read_only());
            row += 2;
        }
        items.push(Item::new(0, row, Control::Separator));
        items.push(Item::new(
            0,
            row + 1,
            Control::Button {
                label: "OK".to_string(),
                default: true,
            },
        ));
        let form = Form::new("Plugin information", WIDTH + 10, row + 3, items);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::PluginInfo(menu, selected, side),
        });
    }

    /// "Plugin information" closed: its menu back where it was.
    pub(in crate::app) fn plugin_info_closed(
        &mut self,
        menu: HotkeyMenu,
        selected: usize,
        side: Side,
    ) {
        match menu {
            HotkeyMenu::Disks => self.reopen_drive_menu(side, selected),
            HotkeyMenu::Commands | HotkeyMenu::Config => {
                self.open_program_plugin_menu(menu, selected)
            }
        }
    }
}

#[cfg(test)]
#[path = "info_tests.rs"]
mod tests;
