//! DrawLine (FAR's drawline, `far-source/plugins/drawline`): "Draw lines" in the editor's F11 menu starts
//! FAR's key loop - Shift with the arrows draws single or double lines of box characters, joining those
//! around (FAR's `GetEnvType`), Ctrl+Shift with them a tab's width at once, F2 switches single and
//! double, Esc or F10 ends it; the other keys go to the editor as they are, F3-F12 to nothing. A second
//! press in the same direction moves on first (FAR's `DRAWLINE_MULTIEDITSTYLE`). The title and the key
//! bar say so while it runs (`DrawEng.lng`); the whole drawing is one undo step, as FAR's.

use nc_plugin_sdk::api::{UNDO_BEGIN, UNDO_END};
use nc_plugin_sdk::{Host, Info, Plugin, Position, export_plugin};

pub mod boxes;

use boxes::{Sides, Width};

/// Its item in the editor's plugin commands (FAR's `MDrawLines`).
const ITEM: &str = "Draw lines";

/// The way the last line was drawn (FAR's `Present_Direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    None,
    Left,
    Right,
    Up,
    Down,
}

/// A key the loop read: its modifiers and the key without them.
struct Pressed<'a> {
    shift: bool,
    ctrl: bool,
    alt: bool,
    key: &'a str,
}

impl<'a> Pressed<'a> {
    fn of(name: &'a str) -> Self {
        let mut pressed = Pressed {
            shift: false,
            ctrl: false,
            alt: false,
            key: name,
        };
        loop {
            if let Some(rest) = pressed.key.strip_prefix("Ctrl+") {
                pressed.ctrl = true;
                pressed.key = rest;
            } else if let Some(rest) = pressed.key.strip_prefix("Alt+") {
                pressed.alt = true;
                pressed.key = rest;
            } else if let Some(rest) = pressed.key.strip_prefix("Shift+") {
                pressed.shift = true;
                pressed.key = rest;
            } else {
                return pressed;
            }
        }
    }

    fn plain(&self) -> bool {
        !self.shift && !self.ctrl && !self.alt
    }
}

pub struct DrawLine {
    host: Host,
    direction: Direction,
}

impl DrawLine {
    /// FAR's `SetTitle`: the key bar's F1, F2 and F10 only, the title for `width`; false without an
    /// editor.
    fn set_title(&self, width: Width) -> bool {
        let groups = [
            "",
            "Ctrl+",
            "Alt+",
            "Shift+",
            "Ctrl+Shift+",
            "Alt+Shift+",
            "Ctrl+Alt+",
        ];
        let keys: Vec<String> = (groups.iter())
            .flat_map(|group| (1..=12).map(move |number| format!("{group}F{number}")))
            .collect();
        let other = match width {
            Width::Single => "╠═╬═╣",
            Width::Double => "├─┼─┤",
        };
        let label = |key: &str| match key {
            "F1" => "Help",
            "F2" => other,
            "F10" => "Quit",
            _ => "",
        };
        let labels: Vec<(&str, &str)> = (keys.iter())
            .map(|key| (key.as_str(), label(key)))
            .collect();
        let title = match width {
            Width::Single => "Shift→←↑↓ draw ┼",
            Width::Double => "Shift→←↑↓ draw ╬",
        };
        self.host.set_editor_key_bar(Some(&labels)) && self.host.set_editor_title(Some(title))
    }

    /// FAR's `OpenW` loop: the keys read until Esc or F10.
    fn draw(&mut self) {
        let mut width = Width::Single;
        if !self.set_title(width) {
            return;
        }
        let host = &self.host;
        host.editor_undo(UNDO_BEGIN);
        while let Some(name) = self.host.editor_read_input() {
            let pressed = Pressed::of(&name);
            match pressed.key {
                "Esc" | "F10" if pressed.plain() => break,
                "Esc" | "F10" => {}
                "F1" => {
                    self.host.show_help("Contents");
                    self.direction = Direction::None;
                }
                "F2" if pressed.plain() => {
                    width = width.other();
                    self.set_title(width);
                    self.direction = Direction::None;
                }
                "F2" => {}
                "PgUp" | "PgDn" | "End" | "Home" | "Left" | "Up" | "Right" | "Down" => {
                    self.moved(&pressed, &name, width);
                }
                key => {
                    self.direction = Direction::None;
                    if !is_kept_function_key(key) {
                        self.host.editor_process_input(&name);
                    }
                }
            }
        }
        let host = &self.host;
        host.set_editor_title(None);
        host.set_editor_key_bar(None);
        host.editor_undo(UNDO_END);
    }

    /// A move key: with Shift a line drawn (with Ctrl a tab's width of it, FAR's
    /// `DRAWLINE_MULTIEDITSTYLE`), else the editor's own move.
    fn moved(&mut self, pressed: &Pressed, name: &str, width: Width) {
        if !pressed.shift {
            self.direction = Direction::None;
            self.host.editor_process_input(name);
            return;
        }
        self.host.editor_undo(UNDO_BEGIN);
        let times = match pressed.ctrl {
            true => self.host.editor_info().map_or(1, |info| info.tab_size),
            false => 1,
        };
        for _ in 0..times {
            self.shift_key(pressed.key, width);
        }
        self.host.editor_undo(UNDO_END);
    }

    /// FAR's `ProcessShiftKey`: the box character at the cursor joined to those around for `key`.
    fn shift_key(&mut self, key: &str, width: Width) {
        let host = &self.host;
        let Some(info) = host.editor_info() else {
            return;
        };
        let mut place = Position {
            line: Some(info.line),
            pos: Some(info.tab_pos),
            ..Position::default()
        };
        if info.line > 0 {
            host.editor_expand_tabs(Some(info.line - 1));
        }
        host.editor_expand_tabs(Some(info.line));
        // FAR: on the last line a line is added under it (its count read here instead of `TotalLines`,
        // which a file too big to count does not have).
        if host.editor_line(Some(info.line + 1)).is_none() {
            let length = host
                .editor_line(Some(info.line))
                .map_or(0, |line| line.text.chars().count());
            let at = |pos: usize| Position {
                line: Some(info.line),
                pos: Some(pos),
                ..Position::default()
            };
            host.set_editor_position(&at(length));
            host.editor_insert_line(false);
            host.set_editor_position(&at(info.tab_pos));
        }
        host.editor_expand_tabs(Some(info.line + 1));
        if self.step_on(key, &mut place) {
            self.host.set_editor_position(&place);
        }
        self.put_box(key, width, &place);
    }

    /// FAR's `DRAWLINE_MULTIEDITSTYLE`: the same direction again moves the cursor first; true when it did.
    fn step_on(&mut self, key: &str, place: &mut Position) -> bool {
        let (line, pos) = (place.line.unwrap_or(0), place.pos.unwrap_or(0));
        let (direction, moved) = match key {
            "Up" => (Direction::Up, (line > 0).then(|| (line - 1, pos))),
            "Down" => (Direction::Down, Some((line + 1, pos))),
            "Left" => (Direction::Left, (pos > 0).then(|| (line, pos - 1))),
            "Right" => (Direction::Right, Some((line, pos + 1))),
            _ => return false,
        };
        let again = self.direction == direction;
        self.direction = direction;
        match moved.filter(|_| again) {
            Some((line, pos)) => {
                (place.line, place.pos) = (Some(line), Some(pos));
                true
            }
            None => false,
        }
    }

    /// The character at the cursor set to the box character of the sides around it with `key`'s.
    fn put_box(&self, key: &str, width: Width, place: &Position) {
        let host = &self.host;
        let (Some(info), Some(line)) = (host.editor_info(), host.editor_line(None)) else {
            return;
        };
        let mut text: Vec<char> = line.text.chars().collect();
        if text.len() <= info.pos {
            text.resize(info.pos + 1, ' ');
        }
        let above = (info.line > 0)
            .then(|| host.editor_line(Some(info.line - 1)))
            .flatten();
        let below = host.editor_line(Some(info.line + 1));
        let at = |line: Option<nc_plugin_sdk::EditorLine>| {
            line.and_then(|line| line.text.chars().nth(info.pos))
        };
        let mut sides = Sides::around(&text, info.pos, at(above), at(below));
        sides.drawn(key, width);
        let Some(ch) = boxes::box_char(sides) else {
            return;
        };
        text[info.pos] = ch;
        let text: String = text.into_iter().collect();
        host.set_editor_line(Some(info.line), &text);
        host.set_editor_position(place);
        host.editor_redraw();
    }
}

/// FAR's `KeyCode < VK_F3 || KeyCode > VK_F12`: F3-F12 go nowhere while it draws.
fn is_kept_function_key(key: &str) -> bool {
    key.strip_prefix('F')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (3..=12).contains(&number))
}

impl Plugin for DrawLine {
    fn new(host: Host) -> Self {
        Self {
            host,
            direction: Direction::None,
        }
    }

    fn info(&self) -> Info {
        Info {
            title: "DrawLine".to_string(),
            description: "Draw lines for the editor".to_string(),
            author: "FAR Group's plugin, ported".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn editor_items(&self) -> Vec<String> {
        vec![ITEM.to_string()]
    }

    fn run_editor_item(&mut self, _index: usize) -> Result<(), String> {
        self.draw();
        Ok(())
    }
}

export_plugin!(DrawLine);
