//! What the console's own 16 colours are in a theme (ADR-0008 §8). A colour name in a theme is only a
//! number until the console says what it looks like, so a theme carries its reference: FAR's classic
//! palette, or the VGA colours Norton Commander and DOS Navigator were seen in.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::style::Color;

use super::spec::{ThemeFile, parse_color};

/// FAR's names in the console's order (the colours `0`-`15` of `OSC 4` and of SGR).
const NAMES: [&str; 16] = [
    "black",
    "red",
    "green",
    "brown",
    "blue",
    "magenta",
    "cyan",
    "lightgray",
    "darkgray",
    "lightred",
    "lightgreen",
    "yellow",
    "lightblue",
    "lightmagenta",
    "lightcyan",
    "white",
];

/// `OSC 104`: the colours back to the terminal's own. Windows Terminal knows it; the classic console
/// ignores it, so its colours are also put back one by one (`restore_sequence`).
const RESET: &str = "\x1b]104\x07";

/// The console's own 16 colours, in the order of `NAMES`.
pub type ConsoleTable = [(u8, u8, u8); 16];

fn set_colour(index: usize, (r, g, b): (u8, u8, u8)) -> String {
    format!("\x1b]4;{index};rgb:{r:02x}/{g:02x}/{b:02x}\x07")
}

/// What gives the console its colours back: `own`, what it had at the start, if that could be read.
pub fn restore_sequence(own: Option<&ConsoleTable>) -> String {
    own.into_iter()
        .flatten()
        .enumerate()
        .map(|(index, &colour)| set_colour(index, colour))
        .chain(std::iter::once(RESET.to_string()))
        .collect()
}

/// The colours a theme gives; those not given stay the console's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConsolePalette([Option<(u8, u8, u8)>; 16]);

impl ConsolePalette {
    /// `file` laid over these colours; what cannot be read is reported and left out.
    pub fn overlay(mut self, file: &ThemeFile, problems: &mut Vec<String>) -> Self {
        let mut given: Vec<(&String, &String)> = file.console_palette.iter().collect();
        given.sort();
        for (name, value) in given {
            let slot = NAMES
                .iter()
                .position(|known| known.eq_ignore_ascii_case(name.trim()));
            match (slot, parse_color(value, &HashMap::new())) {
                (Some(slot), Ok(Color::Rgb(r, g, b))) => self.0[slot] = Some((r, g, b)),
                (None, _) => problems.push(format!(
                    "{}: consolePalette: unknown colour \"{name}\"",
                    file.name
                )),
                _ => problems.push(format!(
                    "{}: consolePalette: {name}: \"{value}\" is not #rrggbb",
                    file.name
                )),
            }
        }
        self
    }

    /// `color` as the theme gives it exactly, when it is one of the console's 16 and the theme gives it.
    fn exact(&self, color: Color) -> Color {
        let slot = match color {
            Color::Black => 0,
            Color::Red => 1,
            Color::Green => 2,
            Color::Yellow => 3,
            Color::Blue => 4,
            Color::Magenta => 5,
            Color::Cyan => 6,
            Color::Gray => 7,
            Color::DarkGray => 8,
            Color::LightRed => 9,
            Color::LightGreen => 10,
            Color::LightYellow => 11,
            Color::LightBlue => 12,
            Color::LightMagenta => 13,
            Color::LightCyan => 14,
            Color::White => 15,
            Color::Indexed(index @ 0..=15) => usize::from(index),
            other => return other,
        };
        self.0[slot].map_or(color, |(r, g, b)| Color::Rgb(r, g, b))
    }

    /// Paints the frame in the theme's exact colours. The console's table alone is not enough: a Windows
    /// Terminal tab goes back to its own scheme whenever the keyboard layout changes.
    pub fn paint_exact(&self, buffer: &mut Buffer) {
        for cell in &mut buffer.content {
            cell.fg = self.exact(cell.fg);
            cell.bg = self.exact(cell.bg);
        }
    }

    /// What to send to the console: its own colours first, so that a colour the previous theme gave
    /// and this one does not goes back; then an `OSC 4` per colour.
    pub fn sequence(&self, own: Option<&ConsoleTable>) -> String {
        let colours = self
            .0
            .iter()
            .enumerate()
            .filter_map(|(index, colour)| colour.map(|colour| set_colour(index, colour)));
        std::iter::once(restore_sequence(own))
            .chain(colours)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Theme, resolve};
    use super::*;
    use ratatui::layout::Rect;

    fn osc(index: u8, rgb: &str) -> String {
        format!("\x1b]4;{index};rgb:{rgb}\x07")
    }

    fn sequence(theme: &str) -> String {
        resolve(theme, &|_| None).console.sequence(None)
    }

    /// FAR's `DefaultPalette`, red before blue as escape sequences count.
    const CLASSIC: [&str; 16] = [
        "00/00/00", "80/00/00", "00/80/00", "80/80/00", "00/00/80", "80/00/80", "00/80/80",
        "c0/c0/c0", "80/80/80", "ff/00/00", "00/ff/00", "ff/ff/00", "00/00/ff", "ff/00/ff",
        "00/ff/ff", "ff/ff/ff",
    ];

    /// The VGA colours of DOS.
    const VGA: [&str; 16] = [
        "00/00/00", "aa/00/00", "00/aa/00", "aa/55/00", "00/00/aa", "aa/00/aa", "00/aa/aa",
        "aa/aa/aa", "55/55/55", "ff/55/55", "55/ff/55", "ff/ff/55", "55/55/ff", "ff/55/ff",
        "55/ff/ff", "ff/ff/ff",
    ];

    fn expected(colours: [&str; 16]) -> String {
        let sets = (0u8..).zip(colours).map(|(index, rgb)| osc(index, rgb));
        std::iter::once(RESET.to_string()).chain(sets).collect()
    }

    #[test]
    fn a_frame_is_painted_in_the_themes_exact_colours() {
        let palette = Theme::far().console;
        let mut buffer = Buffer::empty(Rect::new(0, 0, 3, 1));
        buffer[(0, 0)].set_fg(Color::Yellow).set_bg(Color::Blue);
        buffer[(1, 0)]
            .set_fg(Color::Indexed(1))
            .set_bg(Color::Indexed(200));
        buffer[(2, 0)]
            .set_fg(Color::Rgb(1, 2, 3))
            .set_bg(Color::Reset);

        palette.paint_exact(&mut buffer);

        assert_eq!(buffer[(0, 0)].fg, Color::Rgb(0x80, 0x80, 0));
        assert_eq!(buffer[(0, 0)].bg, Color::Rgb(0, 0, 0x80), "blue is not red");
        assert_eq!(buffer[(1, 0)].fg, Color::Rgb(0x80, 0, 0));
        assert_eq!(buffer[(1, 0)].bg, Color::Indexed(200), "not one of the 16");
        assert_eq!(buffer[(2, 0)].fg, Color::Rgb(1, 2, 3));
        assert_eq!(
            buffer[(2, 0)].bg,
            Color::Reset,
            "the console's own background"
        );
    }

    #[test]
    fn a_colour_the_theme_does_not_give_stays_the_consoles() {
        let palette = ConsolePalette::default();
        assert_eq!(palette.exact(Color::Blue), Color::Blue);
    }

    #[test]
    fn far_has_its_classic_palette_in_the_consoles_order() {
        assert_eq!(Theme::far().console.sequence(None), expected(CLASSIC));
    }

    #[test]
    fn the_dos_themes_have_the_vga_colours() {
        for theme in ["nc5", "dn"] {
            assert_eq!(sequence(theme), expected(VGA), "{theme}");
        }
    }

    #[test]
    fn an_inheriting_theme_takes_its_parents_colours() {
        assert_eq!(sequence("mono"), sequence("far"));
    }

    fn file(json: &str) -> ThemeFile {
        ThemeFile::parse(json).unwrap()
    }

    #[test]
    fn a_theme_changes_only_the_colours_it_gives() {
        let mut problems = Vec::new();
        let base = ConsolePalette::default().overlay(
            &file(r##"{"consolePalette": {"blue": "#000001", "red": "#000002"}}"##),
            &mut problems,
        );
        let top = base.overlay(
            &file(r##"{"consolePalette": {"BLUE": "#0000ff"}}"##),
            &mut problems,
        );
        assert_eq!(
            top.sequence(None),
            format!("{RESET}{}{}", osc(1, "00/00/02"), osc(4, "00/00/ff"))
        );
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn what_cannot_be_read_is_reported_and_left_out() {
        let mut problems = Vec::new();
        let palette = ConsolePalette::default().overlay(
            &file(r##"{"name": "x", "consolePalette": {"pink": "#000000", "blue": "red", "red": "1"}}"##),
            &mut problems,
        );
        assert_eq!(palette, ConsolePalette::default());
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems.iter().all(|problem| problem.starts_with("x: ")));
    }

    #[test]
    fn the_consoles_own_colours_go_back_one_by_one_before_the_reset() {
        let mut own = [(0, 0, 0); 16];
        own[4] = (0, 0x37, 0xda);
        let text = restore_sequence(Some(&own));
        assert_eq!(text.matches("\x1b]4;").count(), 16);
        assert!(text.contains(&osc(4, "00/37/da")), "{text:?}");
        assert!(text.ends_with(RESET), "{text:?}");
    }

    #[test]
    fn a_theme_starts_from_the_consoles_own_colours() {
        let mut own = [(1, 1, 1); 16];
        own[9] = (9, 9, 9);
        let text = Theme::far().console.sequence(Some(&own));
        let restored = text.find(&osc(9, "09/09/09")).unwrap();
        let set = text.find(&osc(9, "ff/00/00")).unwrap();
        assert!(restored < set, "{text:?}");
    }

    #[test]
    fn a_theme_without_colours_only_gives_the_console_its_own_back() {
        assert_eq!(ConsolePalette::default().sequence(None), RESET);
    }
}
