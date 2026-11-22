//! The configuration's own windows (arclite's ui.cpp): `MultiSelectDialog` - a list's items as check
//! boxes in columns shaped to the screen, the ones chosen checked, OK giving them back joined by commas
//! (read-only: all checked and disabled, no buttons) - and `FormatLibraryInfoDialog` - each library's
//! path, its version, its formats and masks in a read-only list; "7z library not found" when none is.

use nc_plugin_sdk::sevenzip::{Format, Module};
use nc_plugin_sdk::{Dialog, DialogItem, Host};

/// arclite's `MSG_7Z_LIB_NOT_FOUND`.
pub const NOT_FOUND: &str = "7z library not found";

/// The list split at commas, sorted whatever the case (arclite's `ItemCompare`), no empty item.
fn items_of(text: &str) -> Vec<String> {
    let mut items: Vec<String> = (text.split(','))
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect();
    items.sort_by_key(|item| item.to_uppercase());
    items
}

/// arclite's `estimate_column_widths`: the columns (their widths) whose shape is nearest the screen's
/// (`ratio` its width to its height), more columns tried while they come nearer.
pub fn column_widths(items: &[String], ratio: f64) -> Vec<usize> {
    let mut best = f64::MAX;
    let mut chosen = Vec::new();
    for columns in 1..=items.len() {
        let mut widths = vec![0usize; columns];
        for (at, item) in items.iter().enumerate() {
            let column = at % columns;
            widths[column] = widths[column].max(item.chars().count());
        }
        let width = widths.iter().sum::<usize>() + columns * 4 + (columns - 1);
        let height = items.len().div_ceil(columns);
        let difference = (width as f64 / height as f64 - ratio).abs();
        if difference > best {
            break;
        }
        best = difference;
        chosen = widths;
    }
    chosen
}

/// The screen's width to its height (arclite's 80 / 25 integer division, 3, when not known).
fn screen_ratio(host: &Host) -> f64 {
    host.screen().map_or(3.0, |screen| {
        f64::from(screen.columns) / f64::from(screen.rows.max(1))
    })
}

/// The window: the items as check boxes (`chosen` checked), column after column a row at a time;
/// read-only all checked and disabled with no buttons; else OK and Cancel. The OK button's index.
pub fn multi_select_dialog(
    title: &str,
    items: &[String],
    chosen: &[String],
    widths: &[usize],
    read_only: bool,
) -> (Dialog, usize) {
    let columns = widths.len().max(1);
    let mut dialog_items = Vec::new();
    let (mut x, mut y) = (5i32, 2i32);
    for (at, item) in items.iter().enumerate() {
        let column = at % columns;
        let on = read_only || chosen.iter().any(|it| it.eq_ignore_ascii_case(item));
        let length = i32::try_from(item.chars().count()).unwrap_or(0);
        let check = DialogItem::check(item, on).disabled(read_only);
        dialog_items.push(check.at(x, y, x + length + 3, y));
        if column + 1 == columns {
            x = 5;
            y += 1;
        } else {
            let width = i32::try_from(widths.get(column).copied().unwrap_or(0)).unwrap_or(0);
            x += width + 5;
        }
    }
    if !items.len().is_multiple_of(columns) {
        y += 1;
    }
    let width = widths.iter().sum::<usize>() + columns * 4 + (columns - 1);
    let width = i32::try_from(width)
        .unwrap_or(60)
        .max(i32::try_from(title.len()).unwrap_or(0) + 4);
    let mut ok = items.len();
    if !read_only {
        dialog_items.push(DialogItem::separator().at(-1, y, -1, y));
        y += 1;
        ok = dialog_items.len();
        // "[ OK ]" and "[ Cancel ]" centred (FAR's `DIF_CENTERGROUP`).
        let start = (width + 10 - 17) / 2;
        let ok_button = DialogItem::button("OK").default_button();
        dialog_items.push(ok_button.at(start, y, start + 5, y));
        dialog_items.push(DialogItem::button("Cancel").at(start + 7, y, start + 16, y));
        y += 1;
    }
    let size = (width.unsigned_abs() + 10, (y + 2).unsigned_abs());
    (Dialog::new(title, dialog_items).size(size.0, size.1), ok)
}

/// arclite's `MultiSelectDialog`: `items` (joined by commas) offered, `chosen` checked; OK gives the
/// ones checked joined by commas, none for Cancel or when there is nothing to offer.
pub fn multi_select(host: &Host, title: &str, items: &str, chosen: &str) -> Option<String> {
    let items = items_of(items);
    if items.is_empty() {
        return None;
    }
    let chosen = items_of(chosen);
    let widths = column_widths(&items, screen_ratio(host));
    let (dialog, ok) = multi_select_dialog(title, &items, &chosen, &widths, false);
    let closed = host.dialog(&dialog);
    if closed.button != Some(ok) {
        return None;
    }
    let picked: Vec<&str> = (items.iter().enumerate())
        .filter(|(at, _)| closed.values.get(*at).is_some_and(|value| value.checked()))
        .map(|(_, item)| item.as_str())
        .collect();
    Some(picked.join(","))
}

/// The list read only (the library info's Formats and Masks).
fn show_list(host: &Host, title: &str, items: &str) {
    let items = items_of(items);
    if items.is_empty() {
        return;
    }
    let widths = column_widths(&items, screen_ratio(host));
    let (dialog, _) = multi_select_dialog(title, &items, &[], &widths, true);
    let _ = host.dialog(&dialog);
}

/// The names of the formats of library `library`, joined by commas.
pub fn formats_of(formats: &[Format], library: usize) -> String {
    (formats.iter())
        .filter(|format| format.library == library)
        .map(|format| format.name.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// The masks of the extensions of library `library`'s formats (`*.ext`), joined by commas; all the
/// libraries' with none (arclite's "Generate masks").
pub fn masks_of(formats: &[Format], library: Option<usize>) -> String {
    (formats.iter())
        .filter(|format| library.is_none_or(|library| format.library == library))
        .flat_map(|format| format.extensions.iter().map(|ext| format!("*.{ext}")))
        .collect::<Vec<_>>()
        .join(",")
}

/// A library's version as arclite writes it: four numbers of 16 bits from the highest.
pub fn version_text(version: u64) -> String {
    let part = |shift: u32| (version >> shift) & 0xFFFF;
    format!("{}.{}.{}.{}", part(48), part(32), part(16), part(0))
}

/// arclite's `FormatLibraryInfoDialog`: each library's path (read only, at most the screen's width),
/// "Version:", its Formats and Masks buttons (each its list); "7z library not found" when none. The
/// buttons' indexes by library.
pub fn library_info_dialog(
    modules: &[Module],
    screen_width: usize,
) -> (Dialog, Vec<(usize, usize)>) {
    let mut items = Vec::new();
    let mut buttons = Vec::new();
    let mut y = 2;
    if modules.is_empty() {
        items.push(DialogItem::text(NOT_FOUND).at(5, y, 24, y));
        y += 1;
    }
    let longest = (modules.iter())
        .map(|module| module.path.display().to_string().chars().count())
        .max()
        .unwrap_or(0);
    let width = (longest + 1).min(screen_width).max(30);
    let right = 4 + i32::try_from(width).unwrap_or(30);
    for module in modules {
        let path = module.path.display().to_string();
        items.push(DialogItem::edit(&path).read_only().at(5, y, right, y));
        y += 1;
        let version = format!("Version: {}", version_text(module.version));
        let after = 6 + i32::try_from(version.len()).unwrap_or(0);
        items.push(DialogItem::text(&version).at(5, y, after - 2, y));
        let formats = items.len();
        items.push(
            DialogItem::button("Formats")
                .no_close()
                .at(after, y, after + 10, y),
        );
        items.push(
            DialogItem::button("Masks")
                .no_close()
                .at(after + 12, y, after + 20, y),
        );
        buttons.push((formats, formats + 1));
        y += 1;
    }
    let size = (
        u32::try_from(width).unwrap_or(30) + 10,
        (y + 2).unsigned_abs(),
    );
    (
        Dialog::new("7z library info", items).size(size.0, size.1),
        buttons,
    )
}

/// The library info window: its Formats and Masks buttons show their lists.
pub fn library_info(host: &Host, modules: &[Module], formats: &[Format]) {
    let width = host.screen().map_or(60, |screen| {
        usize::try_from(screen.columns)
            .unwrap_or(80)
            .saturating_sub(20)
    });
    let (dialog, buttons) = library_info_dialog(modules, width);
    let _ = host.dialog_with(&dialog, |event, _controls| {
        if let nc_plugin_sdk::DialogEvent::Button(pressed) = event {
            for (library, (formats_button, masks_button)) in buttons.iter().enumerate() {
                if pressed == *formats_button {
                    show_list(host, "Formats", &formats_of(formats, library));
                } else if pressed == *masks_button {
                    show_list(host, "Masks", &masks_of(formats, Some(library)));
                }
            }
        }
        true
    });
}

#[cfg(test)]
#[path = "options_dialogs_tests.rs"]
mod tests;
