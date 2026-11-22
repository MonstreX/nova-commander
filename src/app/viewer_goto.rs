//! The viewer's Alt+F8 (FAR's `Viewer::GoTo` with `GoToRowCol`'s "Go to" dialog and `GetRowCol`): an
//! offset, a percentage with `%`, either relative with `+` / `-`, hex with `0x`, `$`, a trailing `h` or
//! "Hex value" checked (a trailing `m`: decimal); after ` .,;:` the column of lines not wrapped. Ours
//! over FAR (ADR-0015 addendum): a trailing `l` makes it a line's number, found by the line index (its
//! count waited for with Esc while it goes on).

use std::sync::Arc;

use super::Viewer;
use crate::app::form::{Control, EditHistory, Form, Item};
use crate::app::line_input::LineInput;
use crate::editor::view::{Mode, Move};
use crate::jobs::line_index::{LineIndex, NotFound};

/// The dialog's edit, its box and its OK (FAR's `DialogBuilder`: the edit, a separator, the box, the
/// buttons).
const EDIT: usize = 0;
const HEX: usize = 2;
const OK: usize = 4;
/// The waiting window's line while the count goes on.
pub(super) const COUNTING: &str = "Counting lines...";

/// One side of what was typed (FAR's `goto_coord`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Coord {
    pub value: u64,
    /// -1, 0 or +1.
    pub relative: i8,
    pub percent: bool,
    /// Ours: a line's number (a trailing `l`).
    pub line: bool,
    pub exist: bool,
}

/// FAR's `GetRowCol`'s `Parse` of one side.
fn parse_one(mut part: &str, hex: bool) -> Option<Coord> {
    let mut coord = Coord::default();
    if let Some(rest) = part.strip_prefix('-') {
        (part, coord.relative) = (rest, -1);
    } else if let Some(rest) = part.strip_prefix('+') {
        (part, coord.relative) = (rest, 1);
    }
    if let Some(rest) = part.strip_suffix('%') {
        (part, coord.percent) = (rest, true);
    } else if let Some(rest) = part.strip_suffix(['l', 'L']) {
        (part, coord.line) = (rest, true);
    }
    let radix = if let Some(rest) = part.strip_prefix("0x") {
        part = rest;
        16
    } else if let Some(rest) = part.strip_prefix('$') {
        part = rest;
        16
    } else if let Some(rest) = part.strip_suffix('h') {
        part = rest;
        16
    } else if let Some(rest) = part.strip_suffix('m') {
        part = rest;
        10
    } else if hex && !coord.percent && !coord.line {
        16
    } else {
        10
    };
    coord.value = u64::from_str_radix(part, radix).ok()?;
    coord.exist = true;
    Some(coord)
}

/// FAR's `GetRowCol`: the place and the column (after the first of ` .,;:`); none when either side
/// is not a number (FAR's dialog then does nothing).
pub fn parse(text: &str, hex: bool) -> Option<(Coord, Coord)> {
    let text = text.trim();
    let side = |part: &str| match part {
        "" | "+" | "-" => Some(Coord::default()),
        part => parse_one(part, hex),
    };
    match text.find([' ', '.', ',', ';', ':']) {
        None => Some((side(text)?, Coord::default())),
        Some(at) => Some((side(&text[..at])?, side(&text[at + 1..])?)),
    }
}

/// FAR's `FromPercent`.
fn from_percent(percent: u64, base: u64) -> u64 {
    match percent {
        0 => 0,
        100.. => base,
        percent => u64::try_from(u128::from(base) * u128::from(percent) / 100).unwrap_or(base),
    }
}

/// The byte an offset or a percentage names from `position` in a file `len` long (FAR's `GoTo`).
pub fn offset_of(row: Coord, position: u64, len: u64) -> u64 {
    let value = if row.percent {
        from_percent(row.value, len)
    } else {
        row.value
    };
    let moved = match row.relative {
        0 => value,
        sign if sign < 0 => position.saturating_sub(value),
        _ => position.saturating_add(value),
    };
    moved.min(len)
}

/// FAR's "Go to" dialog: the edit with its history (`LineNumber`, the last line in it), "Hex value"
/// (`hex` at first).
pub fn form(hex: bool) -> Form {
    let mut edit = Item::new(5, 2, Control::Edit(LineInput::with_text("")));
    edit.width = 28;
    let button = |label: &str, default: bool| {
        let label = label.to_string();
        Item::new(0, 6, Control::Button { label, default })
    };
    let items = vec![
        edit.with_history(EditHistory::new("LineNumber").use_last()),
        Item::new(0, 3, Control::Separator),
        Item::new(
            5,
            4,
            Control::Check {
                label: "&Hex value".to_string(),
                checked: hex,
            },
        ),
        Item::new(0, 5, Control::Separator),
        button("OK", true),
        button("Cancel", false),
    ];
    Form::new("Go to", 38, 9, items)
}

/// The dialog answered with OK: what it says (none: Cancel, or not a number).
pub fn answer(form: &Form, button: Option<usize>) -> Option<(Coord, Coord)> {
    (button == Some(OK)).then_some(())?;
    parse(form.edit_text(EDIT), form.is_checked(HEX))
}

impl Viewer {
    /// The line index of the code page shown, started when it is told or another is.
    pub(super) fn keep_index(&mut self) {
        let Some((units, floor)) = self.text.line_units() else {
            return;
        };
        if self
            .index
            .as_ref()
            .is_some_and(|(_, kept)| *kept == (units, floor))
        {
            return;
        }
        let index = LineIndex::start(crate::vfs::VfsPath::local(&self.path), units, floor);
        self.index = Some((Arc::new(index), (units, floor)));
    }

    /// FAR's "Go to" dialog: the edit with its history (`LineNumber`, the last line in it), "Hex
    /// value" checked at first in hex mode (FAR's `m_GotoHex`).
    pub fn go_to_form(&mut self) -> Form {
        form(*self.goto_hex.get_or_insert(self.text.mode() == Mode::Hex))
    }

    /// The column after the place (FAR: lines not wrapped in text only; a percentage: column 0).
    fn go_to_column(&mut self, col: Coord) {
        if !col.exist || self.text.mode() != Mode::Text || self.text.wrapped() {
            return;
        }
        let value = if col.percent { 0 } else { col.value };
        let left = usize::try_from(value).unwrap_or(usize::MAX);
        let left = match col.relative {
            0 => left,
            sign if sign < 0 => self
                .text
                .facts()
                .map_or(0, |it| it.left)
                .saturating_sub(left),
            _ => self
                .text
                .facts()
                .map_or(0, |it| it.left)
                .saturating_add(left),
        };
        self.text.press(Move::Column(left));
    }

    /// The byte `pos` shown (and the column), as FAR's `GoTo` ends.
    pub(super) fn go_to(&mut self, pos: u64, col: Coord) {
        self.text.press(Move::To(pos));
        self.go_to_column(col);
    }
}

impl super::super::App {
    /// Alt+F8 in the viewer.
    pub(in crate::app) fn show_viewer_go_to(&mut self) {
        let super::super::Screen::Viewer(viewer) = &mut self.screen else {
            return;
        };
        let form = viewer.go_to_form();
        let action = super::super::dialog::FormAction::ViewerGoTo;
        self.show_dialog(super::super::Dialog::Form { form, action });
    }

    /// "Go to" answered: an offset or a percentage at once, a line once the count reaches it.
    pub(in crate::app) fn finish_viewer_go_to(&mut self, form: &Form, button: Option<usize>) {
        if button != Some(OK) {
            return;
        }
        let hex = form.is_checked(HEX);
        let super::super::Screen::Viewer(viewer) = &mut self.screen else {
            return;
        };
        viewer.goto_hex = Some(hex);
        let Some((row, col)) = parse(form.edit_text(EDIT), hex) else {
            return;
        };
        let Some(facts) = viewer.text.facts() else {
            return;
        };
        if !row.line {
            let pos = if row.exist {
                offset_of(row, facts.position, facts.len)
            } else {
                facts.position
            };
            return viewer.go_to(pos, col);
        }
        if row.relative != 0 {
            return;
        }
        viewer.keep_index();
        let Some((index, _)) = viewer.index.clone() else {
            return;
        };
        let line = row.value.saturating_sub(1);
        self.wait_for_stoppable(vec![COUNTING.to_string()], move || {
            let stop = crate::vfs::stop::current();
            let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
            let found = index.line_start(line, &stopped);
            Box::new(move |app: &mut super::super::App| {
                let super::super::Screen::Viewer(viewer) = &mut app.screen else {
                    return;
                };
                match found {
                    Ok(pos) => viewer.go_to(pos, col),
                    // Past the last line: its end shown (FAR's place past the size: the end).
                    Err(NotFound::Past) => viewer.text.press(Move::End),
                    Err(NotFound::Stopped | NotFound::Failed(_)) => {}
                }
            })
        });
    }
}

#[cfg(test)]
#[path = "viewer_goto_tests.rs"]
mod tests;
