//! Items placed in a dialog by their columns and rows (FAR's `X1`, `Y1`, `X2`, `Y2`), as NetBox's
//! dialogs place theirs.

use nc_plugin_sdk::DialogItem;

/// `item` at column `x`, row `y`.
pub fn at(item: DialogItem, x: i32, y: i32) -> DialogItem {
    item.at(x, y, x, y)
}

/// `item` from column `x`, `width` columns wide, on row `y`.
pub fn line(item: DialogItem, x: i32, y: i32, width: i32) -> DialogItem {
    item.at(x, y, x + width - 1, y)
}

/// A group's line with its `title` (NetBox's `TFarSeparator` with a caption).
pub fn heading(title: &str, x: i32, y: i32, width: i32) -> DialogItem {
    line(DialogItem::single_box(title), x, y, width)
}
