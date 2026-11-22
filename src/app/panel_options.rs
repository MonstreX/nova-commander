//! What the settings say to a panel.

/// FAR's Panel settings the panels go by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelOptions {
    pub highlight: bool,
    pub select_folders: bool,
    pub allow_reverse: bool,
    /// The right button selects (FAR's `RightClickSelect`); off: it shows Explorer's menu when a plugin
    /// gives it (EMenu), and selects otherwise.
    pub right_click_select: bool,
}

impl Default for PanelOptions {
    fn default() -> Self {
        Self {
            highlight: true,
            select_folders: false,
            allow_reverse: true,
            right_click_select: false,
        }
    }
}
