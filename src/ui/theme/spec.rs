//! Theme files (ADR-0008): JSON with a palette of named colours, colours by role (`"<fg> on <bg>"`)
//! the look of elements (`look`) and `inherits`. A colour is one of FAR's 16 names, `0`-`255` or `#rrggbb`. Theme files are data:
//! what cannot be read is reported and left out, never a failure.

use std::collections::HashMap;

use ratatui::style::{Color, Style};
use serde::Deserialize;

use super::look::LookFile;
use super::role::Role;

/// A theme file as written.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ThemeFile {
    pub name: String,
    pub inherits: Option<String>,
    pub palette: HashMap<String, String>,
    pub colors: HashMap<String, String>,
    /// What the console's 16 colours really are in this theme: FAR's name -> `#rrggbb`.
    #[serde(rename = "consolePalette")]
    pub console_palette: HashMap<String, String>,
    pub look: LookFile,
}

impl ThemeFile {
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| error.to_string())
    }
}

/// Colours by role, each given or not; `problems` lists what could not be read.
#[derive(Clone, Debug)]
pub struct Colors {
    pub given: Vec<Option<Style>>,
    pub problems: Vec<String>,
}

impl Colors {
    pub fn empty() -> Self {
        Self {
            given: vec![None; Role::ALL.len()],
            problems: Vec::new(),
        }
    }

    /// `file` laid over these colours: its roles replace theirs.
    pub fn overlay(mut self, file: &ThemeFile) -> Self {
        let mut roles: Vec<(&String, &String)> = file.colors.iter().collect();
        roles.sort();
        for (name, spec) in roles {
            let Some(role) = Role::from_name(name) else {
                self.problems
                    .push(format!("{}: unknown role \"{name}\"", file.name));
                continue;
            };
            match parse_spec(spec, &file.palette) {
                Ok(style) => self.given[role.index()] = Some(style),
                Err(error) => self
                    .problems
                    .push(format!("{}: {name}: {error}", file.name)),
            }
        }
        self
    }
}

/// `"<fg> on <bg>"`, `"<fg>"` or `"on <bg>"`.
pub fn parse_spec(spec: &str, palette: &HashMap<String, String>) -> Result<Style, String> {
    let spec = spec.trim();
    let (fg, bg) = match spec.strip_prefix("on ") {
        Some(bg) => ("", bg),
        None => spec.split_once(" on ").unwrap_or((spec, "")),
    };
    let mut style = Style::new();
    if !fg.trim().is_empty() {
        style = style.fg(parse_color(fg, palette)?);
    }
    if !bg.trim().is_empty() {
        style = style.bg(parse_color(bg, palette)?);
    }
    Ok(style)
}

/// A palette name, one of FAR's 16 colour names, `0`-`255` or `#rrggbb`.
pub fn parse_color(text: &str, palette: &HashMap<String, String>) -> Result<Color, String> {
    let text = text.trim();
    let text = palette.get(text).map_or(text, |value| value.trim());
    if let Some(hex) = text.strip_prefix('#') {
        return parse_hex(hex).ok_or_else(|| format!("\"{text}\" is not #rrggbb"));
    }
    if let Ok(index) = text.parse::<u8>() {
        return Ok(Color::Indexed(index));
    }
    named(text).ok_or_else(|| format!("unknown colour \"{text}\""))
}

fn parse_hex(hex: &str) -> Option<Color> {
    if hex.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    let [_, r, g, b] = value.to_be_bytes();
    Some(Color::Rgb(r, g, b))
}

/// FAR's names (`palette.hpp`): its `BROWN` is the dark yellow, `YELLOW` the bright one,
/// `LIGHTGRAY` the normal console text colour.
fn named(name: &str) -> Option<Color> {
    let color = match name.to_ascii_lowercase().as_str() {
        "black" => Color::Black,
        "blue" => Color::Blue,
        "green" => Color::Green,
        "cyan" => Color::Cyan,
        "red" => Color::Red,
        "magenta" => Color::Magenta,
        "brown" => Color::Yellow,
        "lightgray" => Color::Gray,
        "darkgray" => Color::DarkGray,
        "lightblue" => Color::LightBlue,
        "lightgreen" => Color::LightGreen,
        "lightcyan" => Color::LightCyan,
        "lightred" => Color::LightRed,
        "lightmagenta" => Color::LightMagenta,
        "yellow" => Color::LightYellow,
        "white" => Color::White,
        _ => return None,
    };
    Some(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_palette() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn colours_are_far_names_indexes_or_rgb() {
        let palette = HashMap::from([("accent".to_string(), "#00aaaa".to_string())]);
        assert_eq!(parse_color("yellow", &palette), Ok(Color::LightYellow));
        assert_eq!(parse_color("Brown", &palette), Ok(Color::Yellow));
        assert_eq!(parse_color("208", &palette), Ok(Color::Indexed(208)));
        assert_eq!(
            parse_color("#ff8000", &palette),
            Ok(Color::Rgb(255, 128, 0))
        );
        assert_eq!(parse_color("accent", &palette), Ok(Color::Rgb(0, 170, 170)));
        assert!(parse_color("#ff80", &palette).is_err());
        assert!(parse_color("pink", &palette).is_err());
    }

    #[test]
    fn a_spec_gives_text_background_or_both() {
        let both = parse_spec("black on cyan", &no_palette()).unwrap();
        assert_eq!((both.fg, both.bg), (Some(Color::Black), Some(Color::Cyan)));
        let fg = parse_spec("yellow", &no_palette()).unwrap();
        assert_eq!((fg.fg, fg.bg), (Some(Color::LightYellow), None));
        let bg = parse_spec("on blue", &no_palette()).unwrap();
        assert_eq!((bg.fg, bg.bg), (None, Some(Color::Blue)));
    }

    #[test]
    fn what_cannot_be_read_is_reported_and_left_out() {
        let file = ThemeFile::parse(
            r#"{"name": "t", "colors": {"panel": "white on blue", "pannel": "red", "clock": "pink"}}"#,
        )
        .unwrap();
        let colors = Colors::empty().overlay(&file);
        assert!(colors.given[Role::Panel.index()].is_some());
        assert!(colors.given[Role::Clock.index()].is_none());
        assert_eq!(colors.problems.len(), 2, "{:?}", colors.problems);
    }
}
