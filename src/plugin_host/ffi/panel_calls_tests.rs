//! A plugin's entries and failures as the program's.

use std::time::{Duration, UNIX_EPOCH};

use super::*;
use crate::vfs::{InfoLine, PanelMode};
use nc_plugin_api::{INFO_SEPARATOR, NcInfoLine};

fn raw(name: &'static str, flags: u32) -> NcEntry {
    NcEntry {
        name: NcStr::new(name),
        flags,
        size: 7,
        modified: 1_500,
        created: -2_000,
        accessed: TIME_NONE,
        ..NcEntry::EMPTY
    }
}

#[test]
fn an_entry_keeps_its_name_kind_size_times_and_flags() {
    let file = entry(&raw("note.txt", ENTRY_HIDDEN | ENTRY_READ_ONLY), 3);
    assert_eq!(file.name, "note.txt");
    assert!(!file.is_dir && !file.is_parent);
    assert_eq!((file.size, file.position), (7, 3));
    assert_eq!(
        file.modified,
        Some(UNIX_EPOCH + Duration::from_millis(1_500))
    );
    assert_eq!(
        file.created,
        Some(UNIX_EPOCH - Duration::from_millis(2_000))
    );
    assert_eq!(file.accessed, None);
    assert!(file.attributes.is_hidden() && !file.attributes.is_system());
    assert_eq!(
        file.attributes,
        Attributes::from_windows_bits(HIDDEN | READ_ONLY)
    );
    let folder = entry(&raw("dir", ENTRY_DIR | ENTRY_SYSTEM), 0);
    assert!(folder.is_dir);
    assert_eq!(
        folder.attributes,
        Attributes::from_windows_bits(DIRECTORY | SYSTEM)
    );
}

#[test]
fn a_name_is_kept_as_the_plugin_wrote_it() {
    for name in ["Read Me.TXT", "Привет.Doc", " spaced ", ""] {
        assert_eq!(entry(&raw(name, 0), 0).name, name);
    }
}

#[test]
fn times_far_from_1970_either_way_are_kept_or_none() {
    assert_eq!(time(TIME_NONE), None);
    assert_eq!(time(0), Some(UNIX_EPOCH));
    assert_eq!(time(-1), UNIX_EPOCH.checked_sub(Duration::from_millis(1)));
    assert_eq!(
        time(i64::MAX),
        UNIX_EPOCH.checked_add(Duration::from_millis(i64::MAX.unsigned_abs()))
    );
    assert_eq!(
        time(i64::MIN + 1),
        UNIX_EPOCH.checked_sub(Duration::from_millis((i64::MIN + 1).unsigned_abs()))
    );
}

#[test]
fn a_failures_code_is_its_kind_and_its_text_stays() {
    let kinds = [
        (ERROR_NOT_FOUND, io::ErrorKind::NotFound),
        (ERROR_DENIED, io::ErrorKind::PermissionDenied),
        (ERROR_EXISTS, io::ErrorKind::AlreadyExists),
        (ERROR_UNSUPPORTED, io::ErrorKind::Unsupported),
        (nc_plugin_api::ERROR_OTHER, io::ErrorKind::Other),
        (99, io::ErrorKind::Other),
        (-1, io::ErrorKind::Other),
    ];
    for (code, kind) in kinds {
        let made = error(code, format!("why {code}"));
        assert_eq!(made.kind(), kind, "{code}");
        assert_eq!(made.to_string(), format!("why {code}"));
    }
}

#[test]
fn an_entrys_real_file_is_kept_only_as_an_absolute_path() {
    let mut with = raw("a.txt", 0);
    with.real_path = NcStr::new(r"C:\files\a.txt");
    assert_eq!(
        entry(&with, 0).real_path,
        Some(std::path::PathBuf::from(r"C:\files\a.txt"))
    );
    with.real_path = NcStr::new(r"files\a.txt");
    assert_eq!(entry(&with, 0).real_path, None, "taken from nowhere");
    assert_eq!(entry(&raw("a.txt", 0), 0).real_path, None);
    // R7 review C1: a plugin's reparse bit makes a link, which a move never walks into.
    let mut linked = raw("link", ENTRY_DIR);
    linked.attributes = DIRECTORY | REPARSE_POINT;
    assert!(entry(&linked, 0).points_elsewhere());
    assert!(!entry(&raw("folder", ENTRY_DIR), 0).points_elsewhere());
    // R7 review A2: a NUL inside would make the folder before it the file.
    with.real_path = NcStr::new("C:\\Docs\\\0a.txt");
    assert_eq!(entry(&with, 0).real_path, None, "a NUL inside");
}

#[test]
fn an_entrys_texts_are_copied_up_to_a_hundred_and_none_given_are_none() {
    let columns = [NcStr::new("kind"), NcStr::EMPTY, NcStr::new("Привет")];
    let with = NcEntry {
        columns: columns.as_ptr(),
        column_count: columns.len(),
        description: NcStr::new("About it"),
        ..raw("a", 0)
    };
    let texts = entry(&with, 0).texts.unwrap();
    assert_eq!(texts.columns, ["kind", "", "Привет"]);
    assert_eq!(texts.description, "About it");
    assert_eq!(entry(&raw("b", 0), 0).texts, None, "none given: none kept");
    let described = NcEntry {
        description: NcStr::new("d"),
        ..raw("c", 0)
    };
    let texts = entry(&described, 0).texts.unwrap();
    assert!(texts.columns.is_empty());
    let many = vec![NcStr::new("x"); 150];
    let crowded = NcEntry {
        columns: many.as_ptr(),
        column_count: many.len(),
        ..raw("d", 0)
    };
    assert_eq!(
        entry(&crowded, 0).texts.unwrap().columns.len(),
        100,
        "C0..C99"
    );
    let null = NcEntry {
        column_count: 5,
        ..raw("e", 0)
    };
    assert_eq!(entry(&null, 0).texts, None, "a count without texts: none");
}

/// FAR's `AllocationSize` (an archive's packed size) both ways; `SIZE_NONE` none.
#[test]
fn an_entrys_allocated_size_goes_both_ways() {
    let packed = NcEntry {
        allocated: 77,
        ..raw("a.txt", 0)
    };
    let read = entry(&packed, 0);
    assert_eq!(read.allocated, Some(77));
    assert_eq!(entry(&raw("b.txt", 0), 0).allocated, None);
    assert_eq!(raw_entry(&read, "a.txt", "").allocated, 77);
    let unknown = Entry::default();
    assert_eq!(
        raw_entry(&unknown, "c", "").allocated,
        nc_plugin_api::SIZE_NONE
    );
}

/// A mode of a name and a text, its titles at `titles`.
fn raw_mode(titles: &[NcStr], flags: u32) -> NcPanelMode {
    NcPanelMode {
        columns: NcStr::new("N,C0"),
        widths: NcStr::new("0,5"),
        titles: titles.as_ptr(),
        title_count: titles.len(),
        status_columns: NcStr::new("C0"),
        status_widths: NcStr::new("6"),
        flags,
    }
}

fn info(modes: &[NcPanelMode]) -> NcPanelInfo {
    NcPanelInfo {
        title: NcStr::EMPTY,
        modes: modes.as_ptr(),
        mode_count: modes.len(),
        flags: 0,
        start_mode: nc_plugin_api::START_MODE_NONE,
        start_sort: nc_plugin_api::SORT_KEEP,
        start_order: nc_plugin_api::ORDER_ASCENDING,
        format: NcStr::EMPTY,
        keys: std::ptr::null(),
        key_count: 0,
        info_lines: std::ptr::null(),
        info_line_count: 0,
    }
}

#[test]
fn a_panels_info_lines_are_read_with_their_separators() {
    let lines = [
        NcInfoLine {
            text: NcStr::new("Host"),
            data: NcStr::new("example"),
            flags: 0,
        },
        NcInfoLine {
            text: NcStr::new("Server"),
            data: NcStr::new("ignored"),
            flags: INFO_SEPARATOR,
        },
    ];
    let made = look(&NcPanelInfo {
        info_lines: lines.as_ptr(),
        info_line_count: lines.len(),
        ..info(&[])
    });
    let wanted = [
        InfoLine {
            text: "Host".to_string(),
            data: "example".to_string(),
            separator: false,
        },
        InfoLine {
            text: "Server".to_string(),
            data: "ignored".to_string(),
            separator: true,
        },
    ];
    assert_eq!(&*made.info_lines, &wanted);
    let null = NcPanelInfo {
        info_lines: std::ptr::null(),
        info_line_count: 2,
        ..info(&[])
    };
    assert!(
        look(&null).info_lines.is_empty(),
        "a count without lines: none"
    );
}

#[test]
fn a_panels_modes_are_read_by_key_with_their_flags_up_to_ten() {
    // FAR's `ColumnTitles`: a null title the program's, an empty one shown empty.
    let titles = [NcStr::EMPTY, NcStr::new("Kind"), NcStr::new("")];
    let raw_modes = [
        NcPanelMode::EMPTY,
        raw_mode(&titles, MODE_FULL_SCREEN | MODE_CASE_CONVERSION),
        raw_mode(&[], MODE_DETAILED_STATUS | MODE_ALIGN_EXTENSIONS),
    ];
    let made = modes(&info(&raw_modes));
    assert_eq!(made.len(), 3, "by key: none at Ctrl+0 keeps its place");
    assert_eq!(made[0], None);
    assert_eq!(
        made[1],
        Some(PanelMode {
            columns: "N,C0".to_string(),
            widths: "0,5".to_string(),
            titles: vec![None, Some("Kind".to_string()), Some(String::new())],
            status_columns: "C0".to_string(),
            status_widths: "6".to_string(),
            full_screen: true,
            case_conversion: true,
            ..PanelMode::default()
        })
    );
    let second = made[2].as_ref().unwrap();
    assert!(second.titles.is_empty());
    assert!(second.detailed_status && second.align_extensions);
    assert!(!second.full_screen && !second.case_conversion);
    let twelve = [raw_mode(&[], 0); 12];
    assert_eq!(modes(&info(&twelve)).len(), 10, "Ctrl+0..9");
    let null = NcPanelInfo {
        modes: std::ptr::null(),
        mode_count: 3,
        ..info(&[])
    };
    assert!(modes(&null).is_empty(), "a count without modes: none");
}

#[test]
fn a_panels_start_mode_is_a_key_or_none_and_its_flag_tells_paths() {
    let look = |flags: u32, start_mode: i32| {
        let made = look(&NcPanelInfo {
            flags,
            start_mode,
            ..info(&[])
        });
        assert_eq!(made.title, None, "an empty title: none");
        (made.start_mode, made.takes_paths)
    };
    assert_eq!(look(PANEL_TAKES_PATHS, 4), (Some(4), true));
    assert_eq!(look(0, nc_plugin_api::START_MODE_NONE), (None, false));
    assert_eq!(look(0, 10), (None, false), "past Ctrl+9");
    assert_eq!(look(0, -7), (None, false));
}

#[test]
fn a_panels_format_and_show_flags_are_read_bit_by_bit() {
    let format = NcStr::new("Temporary panel");
    let look_of = |flags: u32, format: NcStr| {
        look(&NcPanelInfo {
            flags,
            format,
            ..info(&[])
        })
    };
    let shown = look_of(PANEL_NAMES_ONLY, format);
    assert_eq!(shown.format.as_deref(), Some("Temporary panel"));
    assert!(shown.names_only && !shown.right_align_names && !shown.preserve_case);
    let right = look_of(PANEL_RIGHT_ALIGN_NAMES, NcStr::EMPTY);
    assert_eq!(right.format, None, "none given: the dialog's `::`");
    assert!(right.right_align_names && !right.names_only && !right.preserve_case);
    let own = look_of(nc_plugin_api::PANEL_OWN_GET, NcStr::EMPTY);
    assert!(own.own_get && !own.own_delete);
    let own = look_of(nc_plugin_api::PANEL_OWN_DELETE, NcStr::EMPTY);
    assert!(own.own_delete && !own.own_get && !own.preserve_case);
    let cased = look_of(PANEL_PRESERVE_CASE, NcStr::EMPTY);
    assert!(cased.preserve_case && !cased.right_align_names && !cased.takes_paths);
}

#[test]
fn a_found_object_goes_to_the_plugin_as_it_comes_back() {
    // What Alt+F7's "Panel" hands a plugin (`raw_entry`) reads back as the program reads entries: the
    // times after and before 1970 and none, and each flag on its own.
    let after = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1_234_567);
    let before = std::time::UNIX_EPOCH - std::time::Duration::from_millis(7_654_321);
    for (hidden, system, read_only) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let bits = (u32::from(hidden) * 0x2) | (u32::from(system) * 0x4) | u32::from(read_only);
        let attributes = crate::platform::files::Attributes::from_windows_bits(bits);
        let found = Entry {
            name: r"C:\a\b.txt".to_string(),
            size: 42,
            modified: Some(after),
            created: Some(before),
            accessed: None,
            attributes,
            real_path: Some(std::path::PathBuf::from(r"C:\a\b.txt")),
            ..Entry::default()
        };
        let raw = raw_entry(&found, &found.name, r"C:\a\b.txt");
        let back = entry(&raw, 0);
        assert_eq!(back.name, found.name);
        assert_eq!(back.size, 42);
        assert_eq!(
            (back.modified, back.created, back.accessed),
            (Some(after), Some(before), None)
        );
        assert_eq!(back.real_path, found.real_path);
        let told = (
            back.attributes.is_hidden(),
            back.attributes.is_system(),
            back.attributes.is_read_only(),
        );
        assert_eq!(told, (hidden, system, read_only));
        assert!(!back.is_dir);
    }
    let folder = Entry {
        is_dir: true,
        ..Entry::default()
    };
    assert!(entry(&raw_entry(&folder, "", ""), 0).is_dir);
}

#[test]
fn a_panels_start_sort_is_the_programs_sort_or_none() {
    // FAR's `StartSortMode` / `StartSortOrder` (API 2).
    let sort = |start_sort: u32, start_order: u32| {
        look(&NcPanelInfo {
            start_sort,
            start_order,
            ..info(&[])
        })
        .start_sort
    };
    use crate::vfs::SortMode;
    assert_eq!(
        sort(nc_plugin_api::SORT_SIZE, nc_plugin_api::ORDER_DESCENDING),
        Some((SortMode::Size, true))
    );
    assert_eq!(
        sort(nc_plugin_api::SORT_NAME, nc_plugin_api::ORDER_ASCENDING),
        Some((SortMode::Name, false))
    );
    assert_eq!(sort(nc_plugin_api::SORT_KEEP, 1), None, "the panel's own");
    assert_eq!(sort(99, 0), None, "a sort the program has not");
}

#[test]
fn what_an_object_put_in_keeps_is_handed_with_its_marks_only_when_given() {
    let times = FileTimes {
        modified: Some(UNIX_EPOCH + Duration::from_millis(1_500)),
        changed: UNIX_EPOCH.checked_sub(Duration::from_secs(2)),
        ..FileTimes::default()
    };
    // A folder's attributes: its folder bit is not a mark.
    let marks = Attributes::from_windows_bits(SYSTEM | READ_ONLY | DIRECTORY);
    let raw = raw_times(&times, Some(marks));
    assert_eq!(
        (raw.modified, raw.created, raw.accessed, raw.changed),
        (1_500, TIME_NONE, TIME_NONE, -2_000)
    );
    assert_eq!(
        (raw.flags, raw.flags_given),
        (ENTRY_SYSTEM | ENTRY_READ_ONLY, 1)
    );
    let hidden = Attributes::from_windows_bits(HIDDEN);
    assert_eq!(raw_times(&times, Some(hidden)).flags, ENTRY_HIDDEN);
    let none = raw_times(&FileTimes::default(), None);
    assert_eq!(
        (none.modified, none.flags, none.flags_given),
        (TIME_NONE, 0, 0)
    );
}
