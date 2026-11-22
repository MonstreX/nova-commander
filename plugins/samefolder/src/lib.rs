//! Same Folder (FAR's samefolder, `far-source/plugins/samefolder/SameFolder.cpp`): from the plugin
//! commands (F11) the passive panel goes to the active one's folder; from a side's drive menu that side
//! goes to the other's. The cursor is on the item the source panel's cursor is on, made the top line
//! (FAR's `FCTL_REDRAWPANEL` with `CurrentItem` and `TopPanelItem` both the item found), else on the first;
//! the item found by its name among all the items (FAR's `ComparePPI` compares every field too, among as
//! many items as the source has).
//! "Same Folder" is in the plugin commands (`Add2PlugMenu`, on) and the drive menu (`Add2DisksMenu`,
//! off) as its configuration says. A plugin's panel as the source gives the folder on the disk under it
//! (FAR opens that plugin's panel anew through its shortcut, which the program has not).

use nc_plugin_sdk::{Dialog, DialogItem, Host, Info, Plugin, Which, export_plugin};

/// Its item in the menus and its configuration's title (`SameFolderEng.lng`).
const ITEM: &str = "Same Folder";
/// Its settings, FAR's names.
const PLUGINS_MENU: &str = "Add2PlugMenu";
const DISKS_MENU: &str = "Add2DisksMenu";
/// The configuration's OK button.
const OK: usize = 2;

pub struct SameFolder {
    host: Host,
    plugins_menu: bool,
    disks_menu: bool,
}

/// The panels for a drive menu's item: the source and the panel the menu is for (`OPEN_LEFTDISKMENU`,
/// `OPEN_RIGHTDISKMENU`); `left_is_target` whether that is the left one.
pub fn disk_panels(left_is_target: bool) -> (Which, Which) {
    if left_is_target {
        (Which::Right, Which::Left)
    } else {
        (Which::Left, Which::Right)
    }
}

/// FAR's configuration dialog (`ConfigureW`: its two check boxes, OK and Cancel).
pub fn config_dialog(plugins_menu: bool, disks_menu: bool) -> Dialog {
    Dialog::new(
        ITEM,
        vec![
            DialogItem::check("Add \"Same Folder\" to the &Plugins menu", plugins_menu),
            DialogItem::check("Add \"Same Folder\" to the &Disks menu", disks_menu),
            DialogItem::button("&OK").default_button(),
            DialogItem::button("Cancel"),
        ],
    )
}

impl SameFolder {
    /// The panel `to` in the folder of `from`, its cursor on `from`'s item.
    fn same_folder(&self, from: Which, to: Which) {
        let Some(source) = self.host.panel(from) else {
            return;
        };
        let folder = match source.plugin {
            Some(_) => source.disk_folder,
            None => source.folder,
        };
        let name = (self.host.panel_items(from).get(source.current)).map(|item| item.name.clone());
        // FAR looks at neither answer: its cursor is set on that panel either way, the folder that
        // could not be read having said so by itself.
        let _ = self.host.set_folder(to, &folder, None);
        let found = name
            .and_then(|name| (self.host.panel_items(to).iter()).position(|item| item.name == name));
        let at = found.unwrap_or(0);
        let _ = self.host.set_cursor(to, at, Some(at));
    }
}

impl Plugin for SameFolder {
    fn new(host: Host) -> Self {
        let plugins_menu = host.bool_setting(PLUGINS_MENU, true);
        let disks_menu = host.bool_setting(DISKS_MENU, false);
        Self {
            host,
            plugins_menu,
            disks_menu,
        }
    }

    fn info(&self) -> Info {
        Info {
            title: ITEM.to_string(),
            description: "One panel goes to the other's folder, its cursor on the same item"
                .to_string(),
            author: "nova-commander".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<String> {
        (self.plugins_menu.then(|| ITEM.to_string()))
            .into_iter()
            .collect()
    }

    fn run_command(&mut self, _index: usize) -> Result<Option<String>, String> {
        self.same_folder(Which::Active, Which::Passive);
        Ok(None)
    }

    fn disks(&self) -> Vec<String> {
        (self.disks_menu.then(|| ITEM.to_string()))
            .into_iter()
            .collect()
    }

    fn open_disk(&mut self, _index: usize) -> Result<Option<String>, String> {
        let left_is_target = (self.host.panel(Which::Left)).is_some_and(|left| left.call_target);
        let (from, to) = disk_panels(left_is_target);
        self.same_folder(from, to);
        Ok(None)
    }

    fn config_items(&self) -> Vec<String> {
        vec![ITEM.to_string()]
    }

    fn configure(&mut self, _index: usize) -> bool {
        let closed = self
            .host
            .dialog(&config_dialog(self.plugins_menu, self.disks_menu));
        if closed.button != Some(OK) {
            return false;
        }
        let checked = |item: usize| closed.values.get(item).is_some_and(|value| value.checked());
        self.plugins_menu = checked(0);
        self.disks_menu = checked(1);
        self.host.set_bool(PLUGINS_MENU, self.plugins_menu);
        self.host.set_bool(DISKS_MENU, self.disks_menu);
        true
    }
}

export_plugin!(SameFolder);

#[cfg(test)]
mod tests {
    use super::*;
    use nc_plugin_sdk::ItemKind;

    #[test]
    fn a_drive_menu_brings_its_own_side_to_the_other_ones_folder() {
        assert_eq!(disk_panels(true), (Which::Right, Which::Left));
        assert_eq!(disk_panels(false), (Which::Left, Which::Right));
    }

    #[test]
    fn the_configuration_is_fars_two_boxes_with_ok_and_cancel() {
        let dialog = config_dialog(true, false);
        assert_eq!(dialog.title, "Same Folder");
        let kinds: Vec<ItemKind> = dialog.items.iter().map(|item| item.kind).collect();
        assert_eq!(
            kinds,
            [
                ItemKind::Check,
                ItemKind::Check,
                ItemKind::Button,
                ItemKind::Button
            ]
        );
        assert_eq!(dialog.items[0].state, 1);
        assert_eq!(dialog.items[1].state, 0);
        assert!(dialog.items[OK].default);
    }
}
