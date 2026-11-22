//! The overlays: menus, lists, dialogs and boxes over the main screen.

use super::tree_panel::FolderTree;
use super::{
    Dialog, DriveMenu, EditList, FiltersMenu, FindResults, HistoryMenu, MenuState, PluginMenu,
    Popup, QuickSearch, ShortcutMenu, UserMenu,
};

/// What stands in front of the main screen, taking the keys.
pub enum Overlay {
    Menu(MenuState),
    Popup(Popup),
    /// Alt+F1 / Alt+F2.
    DriveMenu(DriveMenu),
    Dialog(Dialog),
    /// Alt+letter: FAR's quick search in the active panel.
    QuickSearch(QuickSearch),
    /// Alt+F8, Alt+F11, Alt+F12.
    History(HistoryMenu),
    /// F2.
    UserMenu(UserMenu),
    /// Commands > File associations.
    EditList(EditList),
    /// RCtrl+0..9, Commands > Folder shortcuts.
    Shortcuts(ShortcutMenu),
    /// Ctrl+I.
    Filters(FiltersMenu),
    /// Alt+F7's results.
    FindResults(Box<FindResults>),
    /// Alt+F10: FAR's "Find folder".
    FolderTree(Box<FolderTree>),
    /// A plugin's menu: the plugin waits for the choice.
    PluginMenu(Box<PluginMenu>),
}
