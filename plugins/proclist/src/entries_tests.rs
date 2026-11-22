use std::time::{Duration, UNIX_EPOCH};

use super::*;
use crate::counters::{COUNT, PAGE_FILE, WORKING_SET};
use crate::modes::translate;
use crate::table::TOTAL;

fn process() -> Process {
    let mut counters = [0u64; COUNT];
    counters[0] = 36_000_000_000 + 5;
    counters[3] = 123;
    counters[WORKING_SET] = 5 * 1024 * 1024;
    counters[PAGE_FILE] = 1024;
    counters[11] = 12_345_678;
    let mut results = [0u64; COUNT];
    results[0] = 7;
    results[11] = 42;
    Process {
        pid: 1234,
        parent: 4,
        name: "svchost.exe".to_string(),
        threads: 9,
        priority: 8,
        created: 116_444_736_000_000_000 + 20_000_000,
        counters,
        results,
        path: Some("C:\\Windows\\svchost.exe".to_string()),
        command_line: Some("svchost.exe -k x".to_string()),
        bits: Some(DEFAULT_BITS),
        gdi: 3,
        user: 4,
        owner: Some("NT AUTHORITY\\SYSTEM:0".to_string()),
        ..Process::default()
    }
}

#[test]
fn an_entry_is_named_by_its_key_with_fars_size_and_times() {
    let mode = translate("N,XI,XP").unwrap();
    let view = View {
        mode: &mode,
        widths: &[20, 6, 2],
        decimal: '.',
    };
    let entry = entry(&process(), &view, Some("text".to_string()), None);
    assert_eq!(entry.name, "svchost.exe 1234");
    assert_eq!(entry.size, 5 * 1024 * 1024 + 1024);
    let started = Some(UNIX_EPOCH + Duration::from_secs(2));
    assert_eq!(entry.created, started);
    assert_eq!(
        (entry.modified, entry.accessed, entry.changed),
        (started, started, started)
    );
    assert_eq!(entry.columns, ["  1234", " 8"]);
    assert_eq!(entry.description, "text");
    assert!(!entry.hidden);
    assert_eq!(entry.attributes, 0);
}

#[test]
fn a_32_bit_process_is_virtual_and_total_hidden() {
    let mode = translate("N").unwrap();
    let view = View {
        mode: &mode,
        widths: &[10],
        decimal: '.',
    };
    let old = Process {
        bits: Some(32),
        ..process()
    };
    assert_eq!(entry(&old, &view, None, None).attributes, 0x1_0000);
    let total = Process {
        pid: TOTAL,
        name: "_Total".to_string(),
        bits: None,
        ..process()
    };
    let shown = entry(&total, &view, None, None);
    assert!(shown.hidden);
    assert_eq!(shown.name, "_Total");
}

/// FAR's texts: a time as a duration or per second as a percent, a size fitted to its width or in the
/// float style, a count in units of 1000.
#[test]
fn counters_are_written_as_far_writes_them() {
    let process = process();
    let mode = translate("X0,X0S,X6,X6F,X11,X11S,X3").unwrap();
    let texts: Vec<String> = (mode.custom.iter())
        .map(|(source, _)| column(&process, *source, 8, '.'))
        .collect();
    assert_eq!(
        texts,
        [
            "01:00:00.0000005",
            "       7",
            " 5242880",
            "  5.00 M",
            "12345678",
            "      42",
            "     123",
        ]
    );
    let narrow: Vec<String> = (mode.custom.iter())
        .map(|(source, _)| column(&process, *source, 5, '.'))
        .collect();
    assert_eq!(narrow[2], "  5 M", "a size too wide takes a unit");
    assert_eq!(narrow[4], " 12 m", "a count by 1000");
}

#[test]
fn the_numbers_and_the_owner() {
    let process = process();
    let mode = translate("XI,XC,XT,XB,XG,XU,XP,O").unwrap();
    let texts: Vec<String> = (mode.custom.iter())
        .map(|(source, _)| column(&process, *source, 4, '.'))
        .collect();
    assert_eq!(
        texts,
        [
            "1234",
            "   4",
            "   9",
            "  64",
            "   3",
            "   4",
            "   8",
            "NT AUTHORITY\\SYSTEM:0"
        ]
    );
}

/// `_Total` has its threads and its counters, no other number.
#[test]
fn total_shows_threads_and_counters_only() {
    let total = Process {
        pid: TOTAL,
        bits: None,
        owner: None,
        ..process()
    };
    let mode = translate("XI,XT,XB,X3,O").unwrap();
    let texts: Vec<String> = (mode.custom.iter())
        .map(|(source, _)| column(&total, *source, 4, '.'))
        .collect();
    assert_eq!(texts, ["    ", "   9", "    ", " 123", ""]);
}

/// A column not drawn stays empty, the next keeping their places.
#[test]
fn a_column_not_drawn_is_empty() {
    let mode = translate("N,XI,XT").unwrap();
    let view = View {
        mode: &mode,
        widths: &widths("20,0,3"),
        decimal: '.',
    };
    assert_eq!(entry(&process(), &view, None, None).columns, ["", "  9"]);
    assert_eq!(widths("5, x,7"), [5, 0, 7]);
}

#[test]
fn the_description_of_the_table() {
    let process = process();
    assert_eq!(
        described(&process, Described::Path).as_deref(),
        Some("C:\\Windows\\svchost.exe")
    );
    assert_eq!(
        described(&process, Described::CommandLine).as_deref(),
        Some("svchost.exe -k x")
    );
    assert_eq!(described(&process, Described::Window), None);
}

/// `XN`: the friendly name as given, left as it is; none given: the image's name.
#[test]
fn the_friendly_name_is_its_column() {
    let mode = translate("XN,XI").unwrap();
    let view = View {
        mode: &mode,
        widths: &[20, 6],
        decimal: '.',
    };
    let named = entry(&process(), &view, None, Some("+ Service Host: X"));
    assert_eq!(named.columns, ["+ Service Host: X", "  1234"]);
    assert_eq!(
        named.name, "svchost.exe 1234",
        "the entry keeps its own name"
    );
    let plain = entry(&process(), &view, None, None);
    assert_eq!(plain.columns[0], "svchost.exe");
}
