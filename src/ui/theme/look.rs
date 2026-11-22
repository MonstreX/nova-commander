//! The look of elements a theme gives besides colours (ADR-0008 step 3): the lines of dialog and menu
//! frames and how buttons are drawn. Panels keep FAR's double frame in every theme: FAR, Norton
//! Commander 5 and DOS Navigator all draw it.

use ratatui::symbols::border;
use serde::Deserialize;

/// The lines of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lines {
    Double,
    Single,
    Rounded,
    Ascii,
}

impl Lines {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "double" => Self::Double,
            "single" => Self::Single,
            "rounded" => Self::Rounded,
            "ascii" => Self::Ascii,
            _ => return None,
        })
    }

    pub fn set(self) -> border::Set<'static> {
        match self {
            Self::Double => border::DOUBLE,
            Self::Single => border::PLAIN,
            Self::Rounded => border::ROUNDED,
            Self::Ascii => border::Set {
                top_left: "+",
                top_right: "+",
                bottom_left: "+",
                bottom_right: "+",
                vertical_left: "|",
                vertical_right: "|",
                horizontal_top: "-",
                horizontal_bottom: "-",
            },
        }
    }

    /// A separator across a frame `width` wide, joined to its sides: `╟───╢` in a double frame.
    pub fn separator(self, width: usize) -> String {
        let (left, line, right) = match self {
            Self::Double => ('╟', '─', '╢'),
            Self::Single | Self::Rounded => ('├', '─', '┤'),
            Self::Ascii => ('+', '-', '+'),
        };
        let inner: String = std::iter::repeat_n(line, width.saturating_sub(2)).collect();
        format!("{left}{inner}{right}")
    }

    /// A double separator across a frame (FAR's `DIF_SEPARATOR2`): `╠═══╣` in a double frame.
    pub fn double_separator(self, width: usize) -> String {
        let (left, line, right) = match self {
            Self::Double => ('╠', '═', '╣'),
            Self::Single | Self::Rounded => ('╞', '═', '╡'),
            Self::Ascii => ('+', '=', '+'),
        };
        let inner: String = std::iter::repeat_n(line, width.saturating_sub(2)).collect();
        format!("{left}{inner}{right}")
    }

    /// A line down inside a frame from one separator to another (FAR's `BS_T_H1V1`, `BS_V1`,
    /// `BS_B_H1V1`).
    pub fn inner_vertical(self) -> (char, char, char) {
        match self {
            Self::Double | Self::Single | Self::Rounded => ('┬', '│', '┴'),
            Self::Ascii => ('+', '|', '+'),
        }
    }

    /// A line across from an inner vertical line to the frame (`BS_L_H1V1`, `BS_H1`, `BS_R_H1V2`).
    pub fn inner_branch(self) -> (char, char, char) {
        match self {
            Self::Double => ('├', '─', '╢'),
            Self::Single | Self::Rounded => ('├', '─', '┤'),
            Self::Ascii => ('+', '-', '+'),
        }
    }
}

/// How dialog buttons are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Buttons {
    /// FAR: `[ OK ]`, the focused one in another colour.
    Brackets,
    /// Turbo Vision and Norton Commander 5: ` OK ` on its own colour with a shadow `▄` / `▀`.
    Shadow,
}

impl Buttons {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "brackets" => Self::Brackets,
            "shadow" => Self::Shadow,
            _ => return None,
        })
    }
}

/// The look of a theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub dialog_frame: Lines,
    pub menu_frame: Lines,
    pub buttons: Buttons,
}

impl Default for Look {
    /// FAR's.
    fn default() -> Self {
        Self {
            dialog_frame: Lines::Double,
            menu_frame: Lines::Double,
            buttons: Buttons::Brackets,
        }
    }
}

/// The `look` of a theme file: what it does not give comes from the theme it inherits.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LookFile {
    pub dialog_frame: Option<String>,
    pub menu_frame: Option<String>,
    pub buttons: Option<String>,
}

impl Look {
    /// `file` laid over this look; what cannot be read goes to `problems`.
    pub fn overlay(mut self, file: &LookFile, problems: &mut Vec<String>) -> Self {
        if let Some(lines) = read(
            &file.dialog_frame,
            "dialogFrame",
            Lines::from_name,
            problems,
        ) {
            self.dialog_frame = lines;
        }
        if let Some(lines) = read(&file.menu_frame, "menuFrame", Lines::from_name, problems) {
            self.menu_frame = lines;
        }
        if let Some(buttons) = read(&file.buttons, "buttons", Buttons::from_name, problems) {
            self.buttons = buttons;
        }
        self
    }
}

/// The value of `key` if the file gives it and it is known; an unknown one goes to `problems`.
fn read<T>(
    given: &Option<String>,
    key: &str,
    from_name: fn(&str) -> Option<T>,
    problems: &mut Vec<String>,
) -> Option<T> {
    let name = given.as_deref()?;
    let value = from_name(name);
    if value.is_none() {
        problems.push(format!("look.{key}: unknown value \"{name}\""));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_file_changes_only_what_it_gives_and_reports_what_it_cannot_read() {
        let file: LookFile =
            serde_json::from_str(r#"{"menuFrame": "single", "buttons": "glass"}"#).unwrap();
        let mut problems = Vec::new();
        let look = Look::default().overlay(&file, &mut problems);
        assert_eq!(look.menu_frame, Lines::Single);
        assert_eq!(look.dialog_frame, Lines::Double);
        assert_eq!(look.buttons, Buttons::Brackets);
        assert_eq!(problems.len(), 1, "{problems:?}");
    }

    #[test]
    fn separators_join_the_frame_they_cross() {
        assert_eq!(Lines::Double.separator(5), "╟───╢");
        assert_eq!(Lines::Single.separator(4), "├──┤");
        assert_eq!(Lines::Ascii.separator(3), "+-+");
        assert_eq!(Lines::Double.inner_vertical(), ('┬', '│', '┴'));
        assert_eq!(Lines::Double.inner_branch().2, '╢');
        assert_eq!(Lines::Single.inner_branch().2, '┤');
    }
}
