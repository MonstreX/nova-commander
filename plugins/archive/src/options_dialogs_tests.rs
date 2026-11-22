use std::path::PathBuf;

use nc_plugin_sdk::ItemKind;

use super::*;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

/// The list split at commas and sorted whatever the case; empty items dropped.
#[test]
fn items_are_split_and_sorted() {
    assert_eq!(items_of("zip,7z,,Rar,arj"), ["7z", "arj", "Rar", "zip"]);
    assert!(items_of("").is_empty());
}

/// Columns are added while the list's shape comes nearer the screen's; a wide screen takes more.
#[test]
fn columns_follow_the_screens_shape() {
    let items: Vec<String> = (0..12).map(|at| format!("format{at:02}")).collect();
    let narrow = column_widths(&items, 1.0);
    let wide = column_widths(&items, 5.0);
    assert!(narrow.len() < wide.len(), "{narrow:?} {wide:?}");
    assert!(wide.iter().all(|width| *width == 8));
    assert_eq!(column_widths(&strings(&["a"]), 3.0), [1]);
    assert!(column_widths(&[], 3.0).is_empty());
}

/// The window: a check box for each item (the chosen ones checked whatever their case), then OK and
/// Cancel; read only: all checked and disabled, no buttons.
#[test]
fn the_window_lists_the_items() {
    let items = strings(&["7z", "Rar", "zip"]);
    let (dialog, ok) = multi_select_dialog("Pick", &items, &strings(&["ZIP"]), &[3, 3, 3], false);
    assert_eq!(ok, 4);
    let checks: Vec<(String, bool)> = (dialog.items[..3].iter())
        .map(|item| (item.text.clone(), item.state == 1))
        .collect();
    assert_eq!(
        checks,
        [
            ("7z".to_string(), false),
            ("Rar".to_string(), false),
            ("zip".to_string(), true)
        ]
    );
    assert_eq!(dialog.items[3].kind, ItemKind::Separator);
    assert_eq!(dialog.items[ok].text, "OK");
    assert_eq!(dialog.items[ok + 1].text, "Cancel");
    let (read_only, _) = multi_select_dialog("Pick", &items, &[], &[3], true);
    assert_eq!(read_only.items.len(), 3);
    assert!(
        read_only
            .items
            .iter()
            .all(|item| item.state == 1 && item.disabled)
    );
}

fn format(name: &str, library: usize, extensions: &[&str]) -> Format {
    Format {
        name: name.to_string(),
        library,
        extensions: strings(extensions),
        ..Format::default()
    }
}

/// A library's formats and masks; every library's masks for "Generate masks".
#[test]
fn formats_and_masks_by_library() {
    let formats = [
        format("7z", 0, &["7z"]),
        format("gzip", 0, &["gz", "tgz"]),
        format("Extra", 1, &["ext"]),
    ];
    assert_eq!(formats_of(&formats, 0), "7z,gzip");
    assert_eq!(masks_of(&formats, Some(1)), "*.ext");
    assert_eq!(masks_of(&formats, None), "*.7z,*.gz,*.tgz,*.ext");
}

/// The version's four parts from the highest; the info window's path, version and buttons, or the
/// library not found.
#[test]
fn the_library_info_window() {
    assert_eq!(version_text(0x001A_0003_0000_0000), "26.3.0.0");
    let modules = [Module {
        path: PathBuf::from(r"C:\Program Files\7-Zip\7z.dll"),
        version: 0x001A_0003_0000_0000,
    }];
    let (dialog, buttons) = library_info_dialog(&modules, 80);
    assert_eq!(dialog.items[0].text, r"C:\Program Files\7-Zip\7z.dll");
    assert_eq!(dialog.items[1].text, "Version: 26.3.0.0");
    assert_eq!(buttons, [(2, 3)]);
    assert_eq!(dialog.items[2].text, "Formats");
    assert_eq!(dialog.items[3].text, "Masks");
    let (none, buttons) = library_info_dialog(&[], 80);
    assert_eq!(none.items[0].text, "7z library not found");
    assert!(buttons.is_empty());
}

/// arclite's `estimate_column_widths` for items of different widths: each column as wide as its widest
/// item (items go across the columns), as many columns as bring the shape nearest the screen's.
#[test]
fn the_columns_are_arclites() {
    let items = strings(&["a", "bbbbbb", "cc", "d", "eeee", "ff", "g"]);
    for (ratio, widths) in [
        (0.5, &[6][..]),
        (3.0, &[6]),
        (4.0, &[4, 6]),
        (6.0, &[4, 6]),
        (6.5, &[1, 6, 2]),
        (10.0, &[1, 6, 2]),
        (12.0, &[4, 6, 2, 1]),
        (20.0, &[2, 6, 2, 1, 4]),
    ] {
        assert_eq!(column_widths(&items, ratio), widths, "{ratio}");
    }
    // A shape as near as the one before: the columns go on (arclite stops only when it comes farther).
    assert_eq!(column_widths(&strings(&["aaaa", "aaaa"]), 10.5), [4, 4]);
}

/// The check boxes stand column after column, a row at a time, each column as wide as its widest item
/// and five apart; a row not full ends; the separator, then OK and Cancel centred under them; the
/// window as wide as the columns or its title.
#[test]
fn the_window_is_laid_out_as_arclites() {
    let items = strings(&["7z", "Rar", "zip"]);
    let (dialog, ok) = multi_select_dialog("Pick", &items, &[], &[3, 3], false);
    let places: Vec<(i32, i32, i32, i32)> = dialog.items.iter().map(|item| item.place).collect();
    assert_eq!(
        places,
        [
            (5, 2, 10, 2),
            (13, 2, 19, 2),
            (5, 3, 11, 3),
            (-1, 4, -1, 4),
            (4, 5, 9, 5),
            (11, 5, 20, 5)
        ]
    );
    assert_eq!(ok, 4);
    assert_eq!(dialog.size, Some((25, 8)));
    let title = "A title wider than the columns";
    let (wide, _) = multi_select_dialog(title, &items, &[], &[3, 3], false);
    let width = u32::try_from(title.len()).unwrap() + 4;
    assert_eq!(wide.size, Some((width + 10, 8)));
    let (read_only, ok) = multi_select_dialog("Pick", &items, &[], &[3], true);
    let rows: Vec<i32> = read_only.items.iter().map(|item| item.place.1).collect();
    assert_eq!(rows, [2, 3, 4]);
    assert_eq!(ok, 3);
    assert_eq!(read_only.size, Some((18, 7)));
}

/// Each library's path as wide as the longest (at least 30, at most the screen's), its version's line
/// with Formats and Masks after it; the buttons' indexes by library.
#[test]
fn the_library_info_window_is_laid_out() {
    let long = PathBuf::from(r"C:\A folder of a long name\and another\7z.dll");
    let length = long.display().to_string().chars().count();
    let modules = [
        Module {
            path: long.clone(),
            version: 0x0001_0002_0003_0004,
        },
        Module {
            path: PathBuf::from(r"C:\7z.dll"),
            version: 0x0019_0001_0000_0000,
        },
    ];
    let (dialog, buttons) = library_info_dialog(&modules, 80);
    assert_eq!(buttons, [(2, 3), (6, 7)]);
    let right = 4 + i32::try_from(length + 1).unwrap();
    assert_eq!(dialog.items[0].place, (5, 2, right, 2));
    assert!(dialog.items[0].read_only);
    assert_eq!(dialog.items[1].text, "Version: 1.2.3.4");
    assert_eq!(dialog.items[1].place, (5, 3, 20, 3));
    assert_eq!(dialog.items[2].place, (22, 3, 32, 3));
    assert_eq!(dialog.items[3].place, (34, 3, 42, 3));
    assert_eq!(dialog.items[4].place.1, 4);
    assert_eq!(dialog.items[5].text, "Version: 25.1.0.0");
    let width = u32::try_from(length + 1).unwrap();
    assert_eq!(dialog.size, Some((width + 10, 8)));
    let (narrow, _) = library_info_dialog(&modules, 35);
    assert_eq!(narrow.items[0].place.2, 39, "the screen's width");
    let (least, _) = library_info_dialog(&modules[1..], 80);
    assert_eq!(least.items[0].place.2, 34, "30 at least");
    assert_eq!(least.size, Some((40, 6)));
}
