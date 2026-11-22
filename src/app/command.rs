use super::ViewMode;
use crate::vfs::SortMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }

    pub fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// Everything the user can trigger from a hotkey or a menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Help,
    SaveSetup,
    ChooseShell,
    ChooseTheme,
    OpenMenu,
    /// Shift+F10: FAR's menu bar at the item chosen last.
    OpenLastMenuItem,
    View,
    Edit,
    Copy,
    Move,
    /// Shift+F5 / Shift+F6: the entry under the cursor only, its own name in the line (FAR's
    /// `KEY_SHIFTF5`, `CurrentOnly`).
    CopyCurrent,
    MoveCurrent,
    /// Alt+F6.
    Link,
    /// Ctrl+A: FAR's "Attributes".
    Attributes,
    /// Ctrl+G: FAR's Apply command.
    Apply,
    /// Shift+F1: the panel's files into a new archive of a plugin's (FAR's `PluginPutFilesToNew`).
    AddArchive,
    /// Shift+F2: a plugin's panel opened from a file copied out (FAR's `KEY_SHIFTF2`).
    ExtractArchive,
    /// Shift+F3: the plugin's commands on its panel's file (FAR's `ProcessHostFile`, "Archive
    /// commands").
    ArchiveCommands,
    MakeDir,
    /// F8, Del with the command line empty: to the Recycle Bin or past it, as the settings say.
    Delete,
    /// Shift+F8: the entry under the cursor only.
    DeleteCurrent,
    /// Shift+Del: past the Recycle Bin.
    DeletePermanently,
    Quit,
    /// F12: FAR's Screens list, with the background tasks.
    Screens,
    /// Ctrl+W: FAR's task list, the windows Alt+Tab shows.
    TaskList,
    /// F11: FAR's "Plugin commands".
    PluginCommands,
    /// Apps, Shift+Apps: Explorer's context menu by the plugin that has `rclk_apps:` (FAR's call of EMenu).
    ContextMenu,
    /// Options > Plugins configuration (FAR's `PluginManager::Configure`).
    PluginsConfig,
    /// Options > Plugins: loaded and unloaded while the program runs (P6.0e, the owner's).
    Plugins,
    /// F2: FAR's user menu.
    UserMenu,
    /// Commands > Edit user menu.
    EditUserMenu,
    /// Commands > File associations.
    FileAssociations,
    /// Commands > Folder shortcuts.
    FolderShortcuts,
    /// Options > Groups of file masks.
    MaskGroups,
    /// Options > File panel modes.
    PanelModes,
    /// Options > Folder description files.
    DescriptionFiles,
    /// Ctrl+I, Commands > File panel filter.
    Filter,
    /// Alt+F7.
    FindFile,
    /// Alt+F10: FAR's "Find folder".
    FindFolder,
    /// Alt+F3, Alt+F4: the file's association for them, else F3's and F4's way.
    ViewAlt,
    EditAlt,
    /// Shift+F4: FAR's "Editor" dialog for a file to open or make.
    EditNew,
    /// Alt+F8, Alt+F12, Alt+F11.
    History(crate::settings::history::List),
    SwapPanels,
    /// Ctrl+O: both panels off, or back as they were (FAR).
    TogglePanels,
    /// Alt+Shift+T: the guest shell (ADR-0006).
    ToggleGuestShell,
    PanelsShorter,
    PanelsTaller,
    /// Ctrl+Left: the split moves left.
    SplitLeft,
    /// Ctrl+Right: the split moves right.
    SplitRight,
    /// Ctrl+F1 / Ctrl+F2.
    TogglePanel(Side),
    /// Ctrl+P.
    TogglePassivePanel,
    /// Ctrl+B.
    ToggleKeyBar,
    ChangeDrive(Side),
    Sort(Side, SortMode),
    SortMenu(Side),
    ToggleSelectedFirst(Side),
    /// Shift+F11: FAR's "Use sort groups".
    ToggleSortGroups(Side),
    SetViewMode(Side, ViewMode),
    Reread(Side),
    /// A page of the Options menu.
    Options(super::OptionsPage),
    /// The menu's "Info panel" of that side (Ctrl+L goes by the panel's own rule).
    InfoPanel(Side),
    /// The menu's "Tree panel" (Ctrl+T toggles it).
    TreePanel(Side),
    /// The menu's "Quick view" (Ctrl+Q toggles it).
    QuickView(Side),
    /// The menu's "Select group" and "Unselect group": Gray+ and Gray-.
    SelectGroup,
    UnselectGroup,
    /// The menu's "Invert selection": Gray*.
    InvertSelection,
    /// The menu's "Restore selection": Ctrl+M.
    RestoreSelection,
}
