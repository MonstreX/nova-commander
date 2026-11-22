//! arclite's dialogs laid out as its `Far::Dialog` does (`farutils.cpp`): items from the left in rows from
//! column 5 and row 2, a label as wide as its text (its `&` not counted), a check box or a radio button
//! four more, an edit line to the given width or the row's end; `spacer` and `pad` move along the row; a
//! group of buttons centred in its row as FAR's `DIF_CENTERGROUP` puts them, one column between them. The
//! dialog's width is its widest row's (at least the width asked), its height its rows'.

use nc_plugin_sdk::{Dialog, DialogItem};

/// arclite's `c_x_frame`, `c_y_frame`: the first column and row inside the frame.
const X_FRAME: i32 = 5;
const Y_FRAME: i32 = 2;

/// The width of `text` on the screen: its hotkey's `&` not shown, `&&` one `&` (FAR's `HiStrlen`).
pub fn label_len(text: &str) -> i32 {
    let mut count: i32 = 0;
    let mut chars = text.chars().peekable();
    while let Some(char) = chars.next() {
        if char != '&' || chars.next_if_eq(&'&').is_some() {
            count = count.saturating_add(1);
        }
    }
    count
}

/// A dialog being laid out: its items, where the next goes, how wide it is.
pub struct Layout {
    title: String,
    items: Vec<DialogItem>,
    /// The column and row of the next item.
    x: i32,
    y: i32,
    /// The widest row inside the frame (arclite's `client_xs`).
    width: i32,
    /// The buttons of each row centred, by their indexes.
    groups: Vec<Vec<usize>>,
}

impl Layout {
    pub fn new(title: &str, width: i32) -> Self {
        Self {
            title: title.to_string(),
            items: Vec::new(),
            x: X_FRAME,
            y: Y_FRAME,
            width: width.max(label_len(title)),
            groups: Vec::new(),
        }
    }

    /// The column inside the frame the next item starts at.
    fn column(&self) -> i32 {
        self.x - X_FRAME
    }

    fn advance(&mut self, by: i32) {
        self.x += by;
        self.width = self.width.max(self.column());
    }

    fn push(&mut self, item: DialogItem) -> usize {
        self.items.push(item);
        self.items.len() - 1
    }

    pub fn new_line(&mut self) {
        self.x = X_FRAME;
        self.y += 1;
    }

    pub fn spacer(&mut self, size: i32) {
        self.advance(size);
    }

    /// On to column `at` inside the frame, when not there yet.
    pub fn pad(&mut self, at: i32) {
        if at > self.column() {
            self.spacer(at - self.column());
        }
    }

    pub fn separator(&mut self) -> usize {
        let y = self.y;
        self.push(DialogItem::separator().at(-1, y, -1, y))
    }

    /// A label `text` (`&` its hotkey, unless `ampersand` shows it as it is).
    pub fn label(&mut self, text: &str) -> usize {
        let (x, y) = (self.x, self.y);
        self.advance(label_len(text));
        self.push(DialogItem::text(text).at(x, y, self.x - 1, y))
    }

    /// An edit line `width` columns wide, or (0) to the row's end.
    pub fn edit(&mut self, item: DialogItem, width: i32) -> usize {
        let (x, y) = (self.x, self.y);
        if width == 0 {
            self.x = X_FRAME + self.width;
        } else {
            self.advance(width);
        }
        // A history's arrow takes the line's last column (arclite's `DIF_HISTORY`).
        let x2 = self.x - 1 - i32::from(!item.history.is_empty());
        self.push(item.at(x, y, x2, y))
    }

    /// A drop-down list as wide as its longest line and five columns more, its arrow in the last but
    /// one (arclite's `combo_box` with `AUTO_SIZE` and `DIF_DROPDOWNLIST`).
    pub fn combo(&mut self, lines: &[&str], chosen: usize) -> usize {
        let (x, y) = (self.x, self.y);
        let longest = (lines.iter().map(|line| line.chars().count()))
            .max()
            .unwrap_or(0)
            .max(1);
        self.advance(i32::try_from(longest).unwrap_or(0) + 5);
        let x2 = self.x - 2;
        self.push(DialogItem::list(lines, chosen).at(x, y, x2, y))
    }

    /// A drop-down list `width` columns wide, its arrow in the last but one (arclite's `combo_box`
    /// with a size).
    pub fn combo_sized(&mut self, lines: &[&str], chosen: usize, width: i32) -> usize {
        let (x, y) = (self.x, self.y);
        self.advance(width);
        let x2 = self.x - 2;
        self.push(DialogItem::list(lines, chosen).at(x, y, x2, y))
    }

    /// Back to the row's first column (arclite's `reset_line`): the next item over one there.
    pub fn reset_line(&mut self) {
        self.x = X_FRAME;
    }

    /// A check box, a radio button or a button: its label and four columns more.
    pub fn boxed(&mut self, item: DialogItem) -> usize {
        let (x, y) = (self.x, self.y);
        self.advance(label_len(&item.text) + 4);
        let x2 = self.x - 1;
        self.push(item.at(x, y, x2, y))
    }

    /// Buttons centred in their row (FAR's `DIF_CENTERGROUP`).
    pub fn buttons(&mut self, buttons: Vec<DialogItem>) -> Vec<usize> {
        let group: Vec<usize> = buttons.into_iter().map(|item| self.boxed(item)).collect();
        self.groups.push(group.clone());
        group
    }

    /// The dialog: the rows done, the groups centred, its size with the frame's margins.
    pub fn dialog(mut self) -> Dialog {
        let rows = self.y - Y_FRAME;
        for group in std::mem::take(&mut self.groups) {
            let widths: Vec<i32> = (group.iter())
                .map(|&index| label_len(&self.items[index].text) + 4)
                .collect();
            let gaps = i32::try_from(widths.len().saturating_sub(1)).unwrap_or(0);
            let total: i32 = widths.iter().sum::<i32>() + gaps;
            let mut x = X_FRAME + (self.width - total) / 2;
            for (&index, width) in group.iter().zip(widths) {
                let item = &mut self.items[index];
                let y = item.place.1;
                item.place = (x, y, x + width - 1, y);
                x += width + 1;
            }
        }
        let width = u32::try_from(self.width + 2 * X_FRAME).unwrap_or(0);
        let height = u32::try_from(rows + 2 * Y_FRAME).unwrap_or(0);
        Dialog::new(&self.title, self.items).size(width, height)
    }
}

/// arclite's `fit_str`: `text` cut to `size` characters with `...` in its middle.
pub fn fit(text: &str, size: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= size {
        return text.to_string();
    }
    if size <= 3 {
        return chars[..size].iter().collect();
    }
    let keep = size - 3;
    let head = keep / 2;
    let tail = keep - head;
    let mut fitted: String = chars[..head].iter().collect();
    fitted.push_str("...");
    fitted.extend(&chars[chars.len() - tail..]);
    fitted
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
