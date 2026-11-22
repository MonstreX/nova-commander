//! Where the dialog in front is drawn (`app::dialog_move`): in the middle of the screen moved by the
//! place the app keeps, inside the screen whatever that place says; the place it really took goes back
//! to the app, so that a key moving it past an edge does not leave it further off than it shows. While
//! the keys move it, FAR's marks stand in its corners (`DN_DRAWDIALOGDONE`'s `\` and `/`). A plugin's
//! dialog asked at its own column and row (FAR's `X1`, `Y1`) stands there, inside the room the dialogs
//! have, the shift it took going back as any other's.

use std::cell::Cell;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::theme;

thread_local! {
    /// The shift asked for the frame being drawn, and the one the dialog took.
    static ASKED: Cell<(i16, i16)> = const { Cell::new((0, 0)) };
    static TAKEN: Cell<Option<(i16, i16)>> = const { Cell::new(None) };
    /// The column and row the next dialog placed is asked at, from the room's corner.
    static AT: Cell<(Option<u16>, Option<u16>)> = const { Cell::new((None, None)) };
}

/// The frame about to be drawn asks for `offset`.
pub fn begin(offset: (i16, i16)) {
    begin_at(offset, None);
}

/// The frame about to be drawn asks for `offset`, or (`at`) a column and row of its own.
pub fn begin_at(offset: (i16, i16), at: Option<(Option<u16>, Option<u16>)>) {
    ASKED.with(|asked| asked.set(offset));
    TAKEN.with(|taken| taken.set(None));
    AT.with(|asked_at| asked_at.set(at.unwrap_or((None, None))));
}

/// The shift the last dialog drawn took, if one was.
pub fn taken() -> Option<(i16, i16)> {
    TAKEN.with(Cell::get)
}

/// The shift a dialog took, as it was before another was drawn in front of it.
pub fn set_taken(shift: Option<(i16, i16)>) {
    TAKEN.with(|taken| taken.set(shift));
}

/// A dialog `width` x `height` in `area`: in its middle, moved as asked, inside it.
pub fn placed(area: Rect, width: u16, height: u16) -> Rect {
    let (dx, dy) = ASKED.with(Cell::get);
    // A column and row of its own for the first dialog placed only.
    let (at_x, at_y) = AT.with(|at| at.replace((None, None)));
    let along = |start: u16, room: u16, size: u16, shift: i16, own: Option<u16>| {
        let middle = i32::from((room - size) / 2);
        let shift = own.map_or(i32::from(shift), |own| i32::from(own) - middle);
        let at = (middle + shift).clamp(0, i32::from(room - size));
        (
            start + u16::try_from(at).unwrap_or(0),
            i16::try_from(at - middle).unwrap_or(0),
        )
    };
    let (x, taken_x) = along(area.x, area.width, width, dx, at_x);
    let (y, taken_y) = along(area.y, area.height, height, dy, at_y);
    TAKEN.with(|taken| taken.set(Some((taken_x, taken_y))));
    Rect::new(x, y, width, height)
}

/// FAR's marks in the corners of a dialog the keys move.
pub fn draw_marks(buffer: &mut Buffer, rect: Rect) {
    if rect.width < 2 || rect.height < 2 {
        return;
    }
    let style = theme::dialog_drag();
    let (right, bottom) = (rect.right() - 1, rect.bottom() - 1);
    for (x, y, mark) in [
        (rect.x, rect.y, "\\"),
        (rect.x, bottom, "/"),
        (right, rect.y, "/"),
        (right, bottom, "\\"),
    ] {
        buffer.set_string(x, y, mark, style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialog_moves_from_the_middle_and_stays_inside_the_screen() {
        let screen = Rect::new(0, 1, 80, 24);
        begin((0, 0));
        assert_eq!(placed(screen, 40, 10), Rect::new(20, 8, 40, 10));
        begin((-3, 2));
        assert_eq!(placed(screen, 40, 10), Rect::new(17, 10, 40, 10));
        assert_eq!(taken(), Some((-3, 2)));
        begin((-1000, 1000));
        assert_eq!(placed(screen, 40, 10), Rect::new(0, 15, 40, 10));
        assert_eq!(taken(), Some((-20, 7)), "what it took goes back");
    }

    #[test]
    fn a_plugins_dialog_stands_at_its_own_column_and_row_of_the_room_once() {
        let room = Rect::new(0, 1, 80, 24);
        begin_at((5, 5), Some((Some(2), None)));
        assert_eq!(
            placed(room, 40, 10),
            Rect::new(2, 13, 40, 10),
            "its X1, the row moved"
        );
        assert_eq!(taken(), Some((-18, 5)), "the shift it took goes back");
        assert_eq!(
            placed(room, 40, 10),
            Rect::new(25, 13, 40, 10),
            "one dialog only"
        );
        begin_at((0, 0), Some((Some(70), Some(0))));
        assert_eq!(
            placed(room, 40, 10),
            Rect::new(40, 1, 40, 10),
            "inside the room"
        );
    }
}
