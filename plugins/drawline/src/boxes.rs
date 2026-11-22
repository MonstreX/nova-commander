//! FAR's DrawLine tables: the box characters and the line each has to its four sides (0 none, 1 single,
//! 2 double), and how a drawn line joins those around it (`GetEnvType`, `ProcessShiftKey`).

/// FAR's `BoxChar`.
const BOX: [char; 40] = [
    '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', '└', '┴', '┬', '├', '─', '┼',
    '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪',
    '┘', '┌',
];
/// FAR's `BoxLeft`, `BoxUp`, `BoxRight`, `BoxDown`.
const LEFT: [u8; 40] = [
    0, 1, 2, 1, 1, 2, 2, 0, 2, 2, 1, 2, 1, 0, 1, 1, 0, 1, 1, 0, 0, 0, 0, 2, 2, 0, 2, 2, 2, 1, 2, 1,
    0, 0, 0, 0, 1, 2, 1, 0,
];
const UP: [u8; 40] = [
    1, 1, 1, 2, 0, 0, 2, 2, 0, 2, 2, 1, 0, 1, 1, 0, 1, 0, 1, 1, 2, 2, 0, 2, 0, 2, 0, 2, 1, 2, 0, 0,
    2, 1, 0, 0, 2, 1, 1, 0,
];
const RIGHT: [u8; 40] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 2, 1, 2, 2, 2, 2, 2, 2, 2, 2, 1, 2, 1,
    1, 2, 2, 1, 1, 2, 0, 1,
];
const DOWN: [u8; 40] = [
    1, 1, 1, 2, 2, 1, 2, 2, 2, 0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 2, 0, 2, 0, 2, 2, 0, 2, 0, 0, 1, 2,
    0, 0, 1, 2, 2, 1, 0, 1,
];

/// The line drawn: FAR's `LineWidth` 1 or 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    Single,
    Double,
}

impl Width {
    /// F2's.
    pub fn other(self) -> Self {
        match self {
            Self::Single => Self::Double,
            Self::Double => Self::Single,
        }
    }

    fn lines(self) -> u8 {
        match self {
            Self::Single => 1,
            Self::Double => 2,
        }
    }
}

/// The lines a character joins on each side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sides {
    pub left: u8,
    pub up: u8,
    pub right: u8,
    pub down: u8,
}

impl Sides {
    /// FAR's `GetEnvType`: the lines reaching the character at `pos` of `text` from the characters left
    /// and right of it, `above` and `below` it.
    pub fn around(text: &[char], pos: usize, above: Option<char>, below: Option<char>) -> Self {
        let left = pos.checked_sub(1).and_then(|at| text.get(at)).copied();
        let right = text.get(pos + 1).copied();
        let mut sides = Self::default();
        for at in 0..BOX.len() {
            let ch = BOX[at];
            if left == Some(ch) {
                sides.left = RIGHT[at];
            }
            if above == Some(ch) {
                sides.up = DOWN[at];
            }
            if right == Some(ch) {
                sides.right = LEFT[at];
            }
            if below == Some(ch) {
                sides.down = UP[at];
            }
        }
        sides
    }

    /// FAR's `ProcessShiftKey`: the side `key` draws to of `width`, through to the other side when
    /// nothing crosses; sides of two widths that meet made the drawn one's.
    pub fn drawn(&mut self, key: &str, width: Width) {
        let width = width.lines();
        match key {
            "Up" => {
                self.up = width;
                if self.left == 0 && self.right == 0 {
                    self.down = width;
                }
            }
            "Down" => {
                self.down = width;
                if self.left == 0 && self.right == 0 {
                    self.up = width;
                }
            }
            "Left" => {
                self.left = width;
                if self.up == 0 && self.down == 0 {
                    self.right = width;
                }
            }
            "Right" => {
                self.right = width;
                if self.up == 0 && self.down == 0 {
                    self.left = width;
                }
            }
            _ => {}
        }
        if self.left != 0 && self.right != 0 && self.left != self.right {
            (self.left, self.right) = (width, width);
        }
        if self.up != 0 && self.down != 0 && self.up != self.down {
            (self.up, self.down) = (width, width);
        }
    }
}

/// The box character with these sides; none for a set no character has.
pub fn box_char(sides: Sides) -> Option<char> {
    (0..BOX.len())
        .find(|&at| {
            (LEFT[at], UP[at], RIGHT[at], DOWN[at])
                == (sides.left, sides.up, sides.right, sides.down)
        })
        .map(|at| BOX[at])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_alone_goes_through_and_joins_what_it_meets() {
        let mut sides = Sides::around(&[' '], 0, None, None);
        sides.drawn("Right", Width::Single);
        assert_eq!(box_char(sides), Some('─'));
        let mut sides = Sides::around(&[' '], 0, None, None);
        sides.drawn("Down", Width::Double);
        assert_eq!(box_char(sides), Some('║'));
        // A single line from the left meets one going down: a corner.
        let text = ['─', ' '];
        let mut sides = Sides::around(&text, 1, None, Some('│'));
        assert_eq!((sides.left, sides.down), (1, 1));
        sides.drawn("Down", Width::Single);
        assert_eq!(
            box_char(sides),
            Some('┐'),
            "joined, not through: a side is there"
        );
        // Single left, double right: both made the drawn width.
        let text = ['─', ' ', '═'];
        let mut sides = Sides::around(&text, 1, None, None);
        sides.drawn("Up", Width::Double);
        assert_eq!(box_char(sides), Some('╩'));
    }

    #[test]
    fn every_box_character_is_found_by_its_sides() {
        for at in 0..BOX.len() {
            let sides = Sides {
                left: LEFT[at],
                up: UP[at],
                right: RIGHT[at],
                down: DOWN[at],
            };
            assert_eq!(box_char(sides), Some(BOX[at]));
        }
        assert_eq!(box_char(Sides::default()), None);
    }
}
