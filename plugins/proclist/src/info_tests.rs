use super::*;
use crate::counters::{COUNT, PAGE_FILE};
use crate::table::TOTAL;

fn local(day: u16, date: &str, time: &str) -> LocalTime {
    LocalTime {
        year: 2026,
        month: 11,
        day,
        date: date.to_string(),
        time: time.to_string(),
    }
}

fn process() -> Process {
    let mut counters = [0u64; COUNT];
    counters[0] = 36_000_000_000;
    counters[3] = 1234;
    counters[PAGE_FILE] = 2048;
    counters[11] = 5000;
    let mut results = [0u64; COUNT];
    results[0] = 12;
    results[3] = 1234;
    results[PAGE_FILE] = 2048;
    results[11] = 50;
    Process {
        pid: 1234,
        parent: 4,
        name: "a.exe".to_string(),
        threads: 9,
        priority: 8,
        session: 1,
        created: 116_444_736_000_000_000 + 1_234_567,
        counters,
        results,
        path: Some(r"C:\a.exe".to_string()),
        command_line: Some("a.exe /x".to_string()),
        bits: Some(64),
        gdi: 3,
        user: 0,
        ..Process::default()
    }
}

fn facts() -> Facts {
    let started = (
        local(15, "15.11.2026", "10:00:00"),
        local(15, "15.11.2026", "12:00:00"),
        72_000_000_000,
    );
    Facts {
        process: process(),
        parent_name: Some("System".to_string()),
        version: Some(FileVersion {
            version: Some("1.2.3".to_string()),
            description: Some("Test app".to_string()),
        }),
        owner: Some(Owner {
            user: "me".to_string(),
            domain: "HOME".to_string(),
            sid: "S-1-5-21-1".to_string(),
        }),
        started: Some(started),
        memory: Some(ProcessMemory {
            current_folder: Some(r"C:\work\".to_string()),
            environment: vec!["A=1".to_string(), "B=2".to_string()],
        }),
        decimal: '.',
        ..Facts::default()
    }
}

fn quiet() -> Options {
    Options {
        export_performance: false,
        ..Options::default()
    }
}

fn text_of(options: &Options) -> String {
    text(&facts(), options)
}

#[test]
fn a_title_is_padded_to_fars_column() {
    assert_eq!(title("PID"), format!("PID:{} ", " ".repeat(27)));
    assert_eq!(title("PID").len(), 32);
    let long = "x".repeat(35);
    assert_eq!(title(&long), format!("{long}: "));
}

#[test]
fn the_head_and_the_numbers_are_fars() {
    let text = text_of(&quiet());
    let lines: Vec<&str> = text.lines().collect();
    let expected = [
        format!("{}a.exe, 64-bit", title("Module")),
        format!(r"{}C:\a.exe", title("Full path")),
        format!("{}1.2.3", title("File version")),
        format!("{}Test app", title("Description")),
        format!("{}1234", title("PID")),
        format!("{}4 (System)", title("Parent PID")),
        format!("{}8", title("Priority")),
        format!("{}9", title("Threads")),
        format!(r"{}HOME\me (S-1-5-21-1)", title("Owner")),
        format!("{}1", title("Session")),
    ];
    for (at, wanted) in expected.iter().enumerate() {
        assert_eq!(lines[at], wanted, "line {at}");
    }
}

#[test]
fn the_start_and_the_uptime_under_its_end() {
    let text = text_of(&quiet());
    let started = format!("\n\n{}10:00:00.1234567\n", title("Started at"));
    assert!(text.contains(&started), "{text}");
    let uptime = format!("{}02:00:00.0000000\n", title("Uptime"));
    assert!(text.contains(&uptime), "{text}");
    let mut other = facts();
    other.started = Some((
        local(14, "14.11.2026", "10:00:00"),
        local(15, "15.11.2026", "09:00:00"),
        0,
    ));
    let dated = format!("{}14.11.2026 10:00:00.1234567\n", title("Started at"));
    assert!(
        super::text(&other, &quiet()).contains(&dated),
        "another day: its date"
    );
    let under = format!(
        "{}{}00:00:00.0000000
",
        title("Uptime"),
        " ".repeat(11)
    );
    assert!(
        super::text(&other, &quiet()).contains(&under),
        "right under the date's end"
    );
}

#[test]
fn the_parameters_folder_environment_and_objects() {
    let text = text_of(&quiet());
    let parameters =
        "\nCommand Line:\na.exe /x\n\nCurrent Directory:\nC:\\work\\\n\nEnvironment:\nA=1\nB=2\n";
    assert!(text.contains(parameters), "{text}");
    assert!(text.contains(&format!("\n{}3\n", title("GDI Objects"))));
    assert!(!text.contains("USER Objects"), "none: not written");
    let without = Options {
        export_environment: false,
        ..quiet()
    };
    assert!(!text_of(&without).contains("Environment"));
}

/// FAR's `DumpNTCounters`: a time with its percent, a count with its rate, a size as it is.
#[test]
fn the_counters_are_dumped_as_far_does() {
    let text = text_of(&Options::default());
    let lines = [
        format!(
            "{}{:>27} {:7}%\n",
            title("Processor Time"),
            "01:00:00.0000000",
            12
        ),
        format!("{}{:>27}\n", title("Handle Count"), "1.23 k"),
        format!("{}{:>27}\n", title("Page File Bytes"), "2.00 K"),
        format!("{}{:>27} {:>6}/s\n", title("Page Faults"), "5.00 k", "50"),
    ];
    for line in &lines {
        assert!(text.contains(line.as_str()), "{line:?} in {text}");
    }
    assert!(
        !text_of(&quiet()).contains("Processor Time"),
        "not asked for"
    );
}

#[test]
fn the_window_and_its_styles() {
    let mut facts = facts();
    facts.window = Some(WindowFacts {
        title: "A window".to_string(),
        handle: 0x1A2B,
        style: 0x1000_0000 | 0x0080_0000 | 0x8,
        extended: 0x0000_0100,
    });
    let text = text(&facts, &quiet());
    let lines = [
        format!("\n{}A window\n", title("Window title")),
        format!("{}0x1a2b\n", title("HWND")),
        format!(
            "{}10800008  WS_VISIBLE WS_BORDER 0x00000008\n",
            title("Window style")
        ),
        format!("{}00000100  WS_EX_WINDOWEDGE\n", title("Extended style")),
    ];
    for line in &lines {
        assert!(text.contains(line.as_str()), "{line:?} in {text}");
    }
}

fn with_module() -> Facts {
    let mut facts = facts();
    let module = Module {
        base: 0x7FF6_0000_0000,
        size: 0x1_2000,
        path: r"C:\a.exe".to_string(),
    };
    let version = FileVersion {
        version: Some("1.0".to_string()),
        description: Some("App".to_string()),
    };
    facts.modules = Some(vec![(module, Some(version))]);
    facts
}

#[test]
fn the_modules_without_their_versions() {
    let plain = text(&with_module(), &quiet());
    let head = "\nModules:\nBase             Size     Path (version info is not displayed)\n";
    assert!(plain.contains(head), "{plain}");
    assert!(
        plain.contains("00007FF600000000    12000 C:\\a.exe\n"),
        "{plain}"
    );
}

/// FAR's `PrintModuleVersion`: a 34-character entry, tabs to 40, 48 and 56, the version, spaces to 72.
#[test]
fn the_modules_with_their_versions() {
    let versions = Options {
        export_module_version: true,
        ..quiet()
    };
    let text = text(&with_module(), &versions);
    assert!(text.contains("Path, version, description"));
    let entry = "00007FF600000000    12000 C:\\a.exe";
    let expected = format!("{entry}\t\t\t1.0{}App\n", " ".repeat(14));
    assert!(text.contains(&expected), "{text:?}");
}

fn with_handles() -> Facts {
    let mut facts = facts();
    facts.handles = Some(vec![
        HandleLine {
            value: 0x4,
            access: 0x1F_0003,
            type_name: "Event".to_string(),
            name: String::new(),
        },
        HandleLine {
            value: 0x1A0,
            access: 0x12_019F,
            type_name: "File".to_string(),
            name: r"\Device\HarddiskVolume3\a.txt".to_string(),
        },
    ]);
    facts
}

#[test]
fn the_handles_named_or_with_the_unnamed() {
    let text = text(&with_handles(), &quiet());
    let head = "\nHandles:\nHandle Access   Type            Name\n";
    assert!(text.contains(head), "{text}");
    let file = format!(
        "   1A0 0012019F {:15} \\Device\\HarddiskVolume3\\a.txt\n",
        "File"
    );
    assert!(text.contains(&file), "{text}");
    assert!(!text.contains("Event"), "unnamed: left out");
    let all = Options {
        export_handles_unnamed: true,
        ..quiet()
    };
    let unnamed = format!("     4 001F0003 {:15} \n", "Event");
    assert!(super::text(&with_handles(), &all).contains(&unnamed));
}

#[test]
fn total_has_its_threads_and_counters_only() {
    let facts = Facts {
        process: Process {
            pid: TOTAL,
            name: "_Total".to_string(),
            bits: None,
            path: None,
            command_line: None,
            created: 0,
            ..process()
        },
        decimal: '.',
        ..Facts::default()
    };
    let text = text(&facts, &Options::default());
    let head = format!("{}_Total\n{}9\n\n", title("Module"), title("Threads"));
    assert!(text.starts_with(&head), "{text}");
    assert!(!text.contains("PID") && !text.contains("Session"));
    assert!(text.contains("Processor Time"));
}

#[test]
fn the_file_is_utf16_with_its_mark_and_fars_name() {
    assert_eq!(bytes("Ab"), [0xFF, 0xFE, b'A', 0, b'b', 0]);
    assert_eq!(file_name("svchost.exe 1234"), "svchost.exe 1234.txt");
    assert_eq!(
        file_name("a:b*c?d/e\\f\"g<h>i;j|k\u{1}"),
        "a_b_c_d_e_f_g_h_i_j_k_.txt"
    );
}

#[test]
fn registry_keys_are_shortened_as_far_does() {
    let sid = "S-1-5-21-9";
    let cases = [
        (r"\REGISTRY\MACHINE\SOFTWARE\CLASSES\.txt", r"HKCR\.txt"),
        (r"\REGISTRY\MACHINE\SYSTEM", r"HKLM\SYSTEM"),
        (r"\REGISTRY\USER\S-1-5-21-9\Software", r"HKCU\Software"),
        (
            r"\REGISTRY\USER\S-1-5-21-9_CLASSES\CLSID",
            r"HKCU\Classes\CLSID",
        ),
        (
            r"\REGISTRY\USER\S-1-5-18\Software",
            r"HKU\S-1-5-18\Software",
        ),
        (r"\REGISTRY\USER", "HKU"),
        (r"\Device\Null", r"\Device\Null"),
    ];
    for (name, short) in cases {
        assert_eq!(registry_name(name, sid), short, "{name}");
    }
}

/// FAR's `StyleStr` starts each name with a space: none leaves a space after the bits.
#[test]
fn a_style_of_no_known_bit_keeps_fars_space() {
    assert_eq!(style_text(0, &STYLES), "00000000 ");
    assert_eq!(style_text(0x8000_0000, &STYLES), "80000000  WS_POPUP");
}

/// A process not opened still has FAR's headers over nothing.
#[test]
fn the_headers_stand_over_no_modules_or_handles() {
    let mut facts = facts();
    facts.modules = Some(Vec::new());
    facts.handles = Some(Vec::new());
    let text = text(&facts, &quiet());
    assert!(text.contains("\nModules:\nBase "), "{text}");
    let handles = "\nHandles:\nHandle Access   Type            Name\n";
    assert!(text.ends_with(handles), "{text}");
}
