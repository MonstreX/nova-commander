//! Themes (ADR-0008): colours by role from a theme, FAR's palette (`far/palette.cpp`) being the built-in
//! `far` theme. Drawing code asks for a role's style by name (`theme::panel()`); the theme in use is
//! set once at start and when the user picks another one.

mod console;
pub mod console_colors;
mod look;
mod role;
mod spec;

use std::cell::RefCell;
use std::rc::Rc;

use ratatui::style::Style;

pub use console::{ConsolePalette, ConsoleTable, restore_sequence};
pub use look::{Buttons, Look};
pub use role::Role;
use spec::{Colors, ThemeFile};

const FAR_THEME: &str = include_str!("far.json");

/// The built-in themes by name (compiled in): FAR's palette; Norton Commander 5.51 as it shows itself;
/// DOS Navigator: FAR's `dn_like` panels with Turbo Vision's dialogs and menus; FAR's black and white
/// (`Addons/Colors/Interface`). Sources: `docs/themes-reference.md`.
const BUILT_IN: &[(&str, &str)] = &[
    ("far", FAR_THEME),
    ("nc5", include_str!("nc5.json")),
    ("dn", include_str!("dn.json")),
    ("mono", include_str!("mono.json")),
];

/// How deep `inherits` may go: deeper is taken for a loop.
const MAX_INHERITANCE: usize = 8;

pub fn built_in_names() -> Vec<String> {
    BUILT_IN.iter().map(|(name, _)| name.to_string()).collect()
}

/// The theme `name`: a theme file read by `read_file` (the user's themes folder) or a built-in one,
/// over the themes it inherits. A theme that cannot be found or read is FAR's, with the problem noted.
pub fn resolve(name: &str, read_file: &dyn Fn(&str) -> Option<String>) -> Theme {
    let mut chain = Vec::new();
    let mut problems = Vec::new();
    let mut next = Some(name.to_string());
    while let Some(current) = next.take() {
        if chain.len() == MAX_INHERITANCE {
            problems.push(format!("{name}: \"inherits\" goes too deep (a loop?)"));
            chain.clear();
            break;
        }
        let file = match read_file(&current) {
            Some(json) => ThemeFile::parse(&json).map_err(|error| format!("{current}: {error}")),
            None => built_in_file(&current).ok_or_else(|| format!("no theme \"{current}\"")),
        };
        match file {
            Ok(mut file) => {
                if file.name.is_empty() {
                    file.name.clone_from(&current);
                }
                next = file.inherits.clone();
                chain.push(file);
            }
            Err(problem) => problems.push(problem),
        }
    }
    if chain.is_empty() {
        let mut far = Theme::far();
        far.name = name.to_string();
        far.problems = problems;
        return far;
    }
    chain.reverse();
    let mut theme = Theme::from_files(name, &chain);
    problems.append(&mut theme.problems);
    theme.problems = problems;
    theme
}

/// A theme ready to draw with: every role has its style (given, from a parent role, or plain).
#[derive(Debug)]
pub struct Theme {
    pub name: String,
    styles: Vec<Style>,
    look: Look,
    console: ConsolePalette,
    /// What could not be read from the theme's files.
    pub problems: Vec<String>,
}

impl Theme {
    /// FAR's own colours.
    pub fn far() -> Self {
        Self::built_in("far").unwrap_or_else(|| Self::from_colors("far", Colors::empty()))
    }

    pub fn built_in(name: &str) -> Option<Self> {
        let file = built_in_file(name)?;
        Some(Self::from_files(&file.name.clone(), &[file]))
    }

    /// `files` from the base theme to the chosen one, each laid over the ones before it.
    fn from_files(name: &str, files: &[ThemeFile]) -> Self {
        let colors = files.iter().fold(Colors::empty(), Colors::overlay);
        let mut theme = Self::from_colors(name, colors);
        for file in files {
            theme.console = theme.console.overlay(file, &mut theme.problems);
            let mut problems = Vec::new();
            theme.look = theme.look.overlay(&file.look, &mut problems);
            theme.problems.extend(
                problems
                    .into_iter()
                    .map(|problem| format!("{}: {problem}", file.name)),
            );
        }
        theme
    }

    fn from_colors(name: &str, colors: Colors) -> Self {
        let styles = Role::ALL
            .iter()
            .map(|&role| {
                let mut from = Some(role);
                while let Some(role) = from {
                    if let Some(style) = colors.given[role.index()] {
                        return style;
                    }
                    from = role.parent();
                }
                Style::new()
            })
            .collect();
        Self {
            name: name.to_string(),
            styles,
            look: Look::default(),
            console: ConsolePalette::default(),
            problems: colors.problems,
        }
    }

    pub fn style(&self, role: Role) -> Style {
        self.styles.get(role.index()).copied().unwrap_or_default()
    }
}

fn built_in_file(name: &str) -> Option<ThemeFile> {
    let (_, json) = BUILT_IN.iter().find(|(built_in, _)| *built_in == name)?;
    ThemeFile::parse(json).ok()
}

thread_local! {
    /// The theme drawing uses. Drawing runs on one thread; tests on other threads keep their own.
    static CURRENT: RefCell<Rc<Theme>> = RefCell::new(Rc::new(Theme::far()));
}

/// Makes `theme` the one everything is drawn with.
pub fn set_current(theme: Theme) {
    CURRENT.with(|current| *current.borrow_mut() = Rc::new(theme));
}

/// Draws with the theme `name` from now on, reading it only when it is not the one in use. Returns
/// what could not be read when it was read now.
pub fn use_named(name: &str, read_file: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    if CURRENT.with(|current| current.borrow().name == name) {
        return Vec::new();
    }
    let mut theme = resolve(name, read_file);
    let problems = std::mem::take(&mut theme.problems);
    set_current(theme);
    problems
}

/// The look of elements in the theme in use.
pub fn look() -> Look {
    CURRENT.with(|current| current.borrow().look)
}

/// What the console's own colours should be in the theme in use.
pub fn console_palette() -> ConsolePalette {
    CURRENT.with(|current| current.borrow().console)
}

/// The style of `role` in the theme in use.
pub fn style(role: Role) -> Style {
    CURRENT.with(|current| current.borrow().style(role))
}

// --- panels -------------------------------------------------------------------------------------

pub fn panel() -> Style {
    style(Role::Panel)
}

pub fn panel_border() -> Style {
    style(Role::PanelFrame)
}

/// The corners of a dialog moved by the keys.
pub fn dialog_drag() -> Style {
    style(Role::DialogDrag)
}

pub fn panel_scrollbar() -> Style {
    style(Role::PanelScrollbar)
}

/// The focused panel's title differs.
pub fn panel_title(focused: bool) -> Style {
    style(if focused {
        Role::PanelTitleFocused
    } else {
        Role::PanelTitle
    })
}

/// The note following the mouse while files are dragged.
pub fn panel_drag() -> Style {
    style(Role::PanelDrag)
}

/// Column titles and the sort-mode letter.
pub fn panel_header() -> Style {
    style(Role::PanelHeader)
}

/// Marked files.
pub fn selected() -> Style {
    style(Role::PanelSelected)
}

pub fn cursor(selected: bool) -> Style {
    style(if selected {
        Role::PanelCursorSelected
    } else {
        Role::PanelCursor
    })
}

/// Totals on the bottom frame.
pub fn total_info() -> Style {
    style(Role::PanelTotal)
}

/// A value in the info panel (Ctrl+L).
pub fn panel_info() -> Style {
    style(Role::PanelInfo)
}

/// Quick view's numbers of a count not ended (FAR's `COL_PANELHIGHLIGHTTEXT`).
pub fn panel_highlight() -> Style {
    style(Role::PanelHighlight)
}

/// The size of the selection on the status separator.
pub fn selected_info() -> Style {
    style(Role::PanelSelectedInfo)
}

/// Error text inside a panel (FAR shows such messages in a dialog; this keeps the panel readable).
pub fn error_text() -> Style {
    style(Role::PanelError)
}

/// The clock in the top-right corner.
pub fn clock() -> Style {
    style(Role::Clock)
}

// --- menus --------------------------------------------------------------------------------------

/// The top menu bar.
pub fn menu_bar() -> Style {
    style(Role::MenuBar)
}

/// The selected item of the menu bar and of a pull-down menu.
pub fn menu_highlight() -> Style {
    style(Role::MenuSelected)
}

/// Hotkey letters on the top menu bar.
pub fn menu_bar_hotkey() -> Style {
    style(Role::MenuBarHotkey)
}

/// Hotkey letters in a pull-down menu (and of the selected menu bar item).
pub fn menu_hotkey(selected: bool) -> Style {
    style(if selected {
        Role::MenuSelectedHotkey
    } else {
        Role::MenuHotkey
    })
}

/// A menu item shown but not chosen (FAR's `Menu.GrayText`), under the cursor or not.
pub fn menu_disabled(selected: bool) -> Style {
    style(if selected {
        Role::MenuSelectedDisabled
    } else {
        Role::MenuDisabled
    })
}

/// Pull-down menu items.
pub fn menu_text() -> Style {
    style(Role::Menu)
}

/// The frame and separators of a pull-down menu.
pub fn menu_frame() -> Style {
    style(Role::MenuFrame)
}

/// The shadow of menus and dialogs.
pub fn shadow() -> Style {
    style(Role::MenuShadow)
}

// --- bars ---------------------------------------------------------------------------------------

pub fn key_number() -> Style {
    style(Role::KeybarKey)
}

pub fn key_label() -> Style {
    style(Role::KeybarLabel)
}

pub fn command_line() -> Style {
    style(Role::CommandLine)
}

pub fn command_line_selected() -> Style {
    style(Role::CommandLineSelected)
}

// --- dialogs ------------------------------------------------------------------------------------

pub fn dialog(warning: bool) -> Style {
    style(if warning { Role::Warning } else { Role::Dialog })
}

/// The title in a dialog's frame.
pub fn dialog_title() -> Style {
    style(Role::DialogTitle)
}

/// A button: normal, focused, and the colour of its shadow (`look.buttons` = `shadow`).
pub fn button(warning: bool, focused: bool) -> Style {
    style(match (warning, focused) {
        (false, false) => Role::DialogButton,
        (false, true) => Role::DialogButtonFocused,
        (true, false) => Role::WarningButton,
        (true, true) => Role::WarningButtonFocused,
    })
}

pub fn button_shadow(warning: bool) -> Style {
    style(if warning {
        Role::WarningButtonShadow
    } else {
        Role::DialogButtonShadow
    })
}

/// Hotkey letters in dialog text; disabled items.
pub fn dialog_hotkey(warning: bool) -> Style {
    style(if warning {
        Role::WarningHotkey
    } else {
        Role::DialogHotkey
    })
}

pub fn dialog_disabled() -> Style {
    style(Role::DialogDisabled)
}

pub fn button_hotkey(warning: bool, focused: bool) -> Style {
    style(match (warning, focused) {
        (false, false) => Role::DialogButtonHotkey,
        (false, true) => Role::DialogButtonFocusedHotkey,
        (true, false) => Role::WarningButtonHotkey,
        (true, true) => Role::WarningButtonFocusedHotkey,
    })
}

/// An open drop-down list: its lines and their hotkeys, the line under the cursor.
pub fn dialog_list(selected: bool) -> Style {
    style(if selected {
        Role::DialogListSelected
    } else {
        Role::DialogList
    })
}

/// A grey line of an open drop-down list (FAR's `LIF_GRAYED`): shown, never chosen.
pub fn dialog_list_gray(selected: bool) -> Style {
    style(if selected {
        Role::DialogListSelectedGray
    } else {
        Role::DialogListGray
    })
}

/// A list inside a dialog (FAR's `DI_LISTBOX`: Find file's results): its lines, the one under the cursor.
pub fn dialog_listbox(selected: bool) -> Style {
    style(if selected {
        Role::DialogListBoxSelected
    } else {
        Role::DialogListBox
    })
}

pub fn dialog_list_hotkey(selected: bool) -> Style {
    style(if selected {
        Role::DialogListSelectedHotkey
    } else {
        Role::DialogListHotkey
    })
}

pub fn input_field() -> Style {
    style(Role::DialogEdit)
}

/// FAR's `COL_DIALOGEDITDISABLED`: an edit line or a list's field that is disabled.
pub fn input_field_disabled() -> Style {
    style(Role::DialogEditDisabled)
}

pub fn input_field_selected() -> Style {
    style(Role::DialogEditSelected)
}

// --- viewer and editor --------------------------------------------------------------------------

pub fn text_body() -> Style {
    style(Role::Text)
}

pub fn status_bar() -> Style {
    style(Role::TextStatus)
}

/// FAR's `COL_VIEWERSELECTEDTEXT`: what a search found.
pub fn text_selected() -> Style {
    style(Role::TextSelected)
}

mod syntax;
pub use syntax::syntax;

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn colors(fg: Color, bg: Color) -> Style {
        Style::new().fg(fg).bg(bg)
    }

    #[test]
    fn the_far_theme_is_fars_palette() {
        let far = Theme::far();
        assert!(far.problems.is_empty(), "{:?}", far.problems);
        let expected = [
            (Role::Panel, colors(Color::LightCyan, Color::Blue)),
            (Role::PanelCursor, colors(Color::Black, Color::Cyan)),
            (
                Role::PanelCursorSelected,
                colors(Color::LightYellow, Color::Cyan),
            ),
            (Role::PanelSelected, colors(Color::LightYellow, Color::Blue)),
            (Role::KeybarKey, colors(Color::Gray, Color::Black)),
            (Role::Dialog, colors(Color::Black, Color::Gray)),
            (Role::DialogListBox, colors(Color::Black, Color::Gray)),
            (
                Role::DialogListBoxSelected,
                colors(Color::White, Color::Black),
            ),
            (Role::Warning, colors(Color::White, Color::Red)),
            (Role::MenuShadow, colors(Color::DarkGray, Color::Black)),
            (Role::TextStatus, colors(Color::Black, Color::Cyan)),
            (Role::TextSelected, colors(Color::Black, Color::Cyan)),
        ];
        for (role, style) in expected {
            assert_eq!(far.style(role), style, "{}", role.name());
        }
        let given = ThemeFile::parse(FAR_THEME).unwrap();
        for role in Role::ALL {
            assert!(
                given.colors.contains_key(role.name()),
                "far gives {}",
                role.name()
            );
        }
    }

    #[test]
    fn a_theme_gives_only_what_differs_and_roles_fall_back_to_their_parents() {
        let base = ThemeFile::parse(FAR_THEME).unwrap();
        let mine = ThemeFile::parse(
            r#"{"name": "mine", "inherits": "far", "colors": {"menu": "black on green"}}"#,
        )
        .unwrap();
        let theme = Theme::from_files("mine", &[base, mine]);
        assert_eq!(theme.style(Role::Menu), colors(Color::Black, Color::Green));
        assert_eq!(theme.style(Role::Panel), Theme::far().style(Role::Panel));

        let partial =
            ThemeFile::parse(r#"{"name": "p", "colors": {"menu": "white on blue"}}"#).unwrap();
        let theme = Theme::from_files("p", &[partial]);
        assert_eq!(
            theme.style(Role::MenuSelectedHotkey),
            colors(Color::White, Color::Blue),
            "menu.selected.hotkey -> menu.selected -> menu"
        );
        assert_eq!(theme.style(Role::Clock), Style::new());
    }

    #[test]
    fn themes_are_files_or_built_in_over_what_they_inherit() {
        let files = |name: &str| match name {
            "mine" => {
                Some(r#"{"inherits": "dn", "colors": {"clock": "red on white"}}"#.to_string())
            }
            "loop" => Some(r#"{"inherits": "loop"}"#.to_string()),
            "broken" => Some("{".to_string()),
            _ => None,
        };
        for name in built_in_names() {
            let theme = resolve(&name, &files);
            assert!(theme.problems.is_empty(), "{name}: {:?}", theme.problems);
        }
        let mine = resolve("mine", &files);
        assert!(mine.problems.is_empty(), "{:?}", mine.problems);
        assert_eq!(mine.style(Role::Clock), colors(Color::Red, Color::White));
        let dn = resolve("dn", &files);
        assert_eq!(mine.style(Role::Panel), dn.style(Role::Panel), "from dn");
        assert_eq!(
            mine.style(Role::MenuShadow),
            Theme::far().style(Role::MenuShadow)
        );
        assert_ne!(dn.style(Role::Panel), Theme::far().style(Role::Panel));

        for bad in ["nowhere", "loop", "broken"] {
            let theme = resolve(bad, &files);
            assert_eq!(theme.problems.len(), 1, "{bad}: {:?}", theme.problems);
            assert_eq!(theme.style(Role::Panel), Theme::far().style(Role::Panel));
        }
    }

    #[test]
    fn the_theme_in_use_can_be_changed() {
        let file =
            ThemeFile::parse(r#"{"name": "x", "colors": {"clock": "red on white"}}"#).unwrap();
        set_current(Theme::from_files("x", &[file]));
        assert_eq!(clock(), colors(Color::Red, Color::White));
        set_current(Theme::far());
        assert_eq!(clock(), colors(Color::Black, Color::Cyan));
    }
}
