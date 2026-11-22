//! The info panel's rows as FAR's `InfoList::DisplayObject` prints them.

use super::*;
use crate::app::{Description, InfoPanel, InfoSections, Side};
use crate::platform::system::{Battery, PowerFacts};
use crate::platform::volumes::{CdType, StorageDevice, VolumeFacts};
use crate::vfs::local::Info;

const NUMBERS: NumberFormat = NumberFormat {
    decimal_separator: '.',
    thousands_separator: ',',
};

const GIB: u64 = 1 << 30;

fn info() -> Info {
    Info {
        system: SystemFacts {
            computer_name: "WORKSTATION".to_string(),
            computer_description: None,
            user_name: "me".to_string(),
            user_description: Some("The owner".to_string()),
            access_level: Some(AccessLevel::Administrator),
            elevated: false,
            elevation: Some(Elevation::Limited),
            memory: Some(MemoryFacts {
                committable: Amount {
                    total: 8 * GIB,
                    available: 2 * GIB,
                },
                addressable: Amount {
                    total: 128 * GIB,
                    available: 128 * GIB,
                },
                physical: Amount {
                    total: 4 * GIB,
                    available: GIB,
                },
                installed: Some(4 * GIB),
            }),
            power: Some(PowerFacts {
                ac_line: AcLine::Online,
                battery: Some(Battery {
                    charge: Charge::NoBattery,
                    percent: None,
                    charging: false,
                }),
                life: None,
                full_life: Some(3725),
            }),
        },
        disk: DiskInfo {
            root: r"C:\".to_string(),
            kind: Some(DriveType::Fixed),
            volume: Some(VolumeFacts {
                label: "SYSTEM".to_string(),
                file_system: "NTFS".to_string(),
                serial: Some(0x1234_ABCD),
            }),
            size: Some((500 * GIB, 125 * GIB)),
            associated: None,
            device: Some(StorageDevice {
                vendor: String::new(),
                product: "Samsung SSD 970".to_string(),
                bus: "NVMe".to_string(),
            }),
        },
    }
}

fn line(name: &str, value: &str) -> Row {
    Row::Line(name.to_string(), value.to_string())
}

#[test]
fn every_section_is_printed_as_far_prints_it() {
    let panel = InfoPanel::showing(Side::Right, InfoSections::default(), Some(info()));
    let rows = rows(&panel, NUMBERS, ':');
    let expected = [
        line("Computer name", "WORKSTATION"),
        line("User name", "me"),
        line("User description", "The owner"),
        line("Access level", "Administrator"),
        line("Elevated", "No (Limited)"),
        Row::Title(r"Fixed disk C:\ (NTFS)".to_string(), true),
        line("Space, total", "500 GB"),
        line("Space, available", "25%, 125 GB"),
        line("Space, used", "75%, 375 GB"),
        line("Volume label", "SYSTEM"),
        line("Serial number", "1234-ABCD"),
        line("Device", "Samsung SSD 970 (NVMe)"),
        Row::Title("Memory".to_string(), true),
        line("Committable, total", "8.00 GB"),
        line("Committable, available", "25%, 2.00 GB"),
        line("Committable, used", "75%, 6.00 GB"),
        line("Addressable, total", "128 GB"),
        line("Addressable, available", "100%, 128 GB"),
        line("Addressable, used", "0%, 0 B"),
        line("Physical, total", "4.00 GB"),
        line("Physical, available", "25%, 1.00 GB"),
        line("Physical, used", "75%, 3.00 GB"),
        line("Physical, installed", "4.00 GB"),
        Row::Title("Power Status".to_string(), true),
        line("AC power status", "Online"),
        line("Battery charge status", "No system battery (unknown)"),
        line("Battery life time", "-"),
        line("Battery full time", "01:02:05"),
    ];
    assert_eq!(rows, expected);
}

#[test]
fn folded_sections_keep_only_their_titles() {
    let sections = InfoSections {
        disk: false,
        memory: false,
        description: false,
        plugin: false,
        power: false,
    };
    let panel = InfoPanel::showing(Side::Right, sections, Some(info()));
    let titles: Vec<Row> = rows(&panel, NUMBERS, ':').into_iter().skip(5).collect();
    assert_eq!(
        titles,
        [
            Row::Title("Disk".to_string(), false),
            Row::Title("Memory".to_string(), false),
            Row::Title("Power Status".to_string(), false),
        ]
    );
}

#[test]
fn beside_a_plugins_panel_its_lines_come_last_under_plugin_and_fold() {
    use crate::vfs::InfoLine;
    let lines: std::sync::Arc<[InfoLine]> = std::sync::Arc::from(vec![
        InfoLine {
            text: "Server".to_string(),
            data: "not shown".to_string(),
            separator: true,
        },
        InfoLine {
            text: "Host".to_string(),
            data: "example".to_string(),
            separator: false,
        },
    ]);
    let mut panel = InfoPanel::showing(Side::Right, InfoSections::default(), Some(info()));
    panel.plugin_lines = Some(lines);
    let shown = rows(&panel, NUMBERS, ':');
    let last: Vec<Row> = shown.iter().rev().take(3).rev().cloned().collect();
    assert_eq!(
        last,
        [
            Row::Title("Plugin".to_string(), true),
            Row::Separator("Server".to_string()),
            line("Host", "example"),
        ]
    );
    panel.sections.plugin = false;
    let folded = rows(&panel, NUMBERS, ':');
    assert_eq!(
        folded.last(),
        Some(&Row::Title("Plugin".to_string(), false))
    );
    assert_eq!(folded.len(), shown.len() - 2);
}

#[test]
fn a_disk_is_titled_by_its_kind_and_what_it_stands_for() {
    let mut read = info();
    read.disk.kind = Some(DriveType::Cd(CdType::DvdRom));
    read.disk.associated = Some(r"C:\images\a.iso".to_string());
    assert_eq!(
        disk_title(&read.disk),
        r"DVD-ROM disk C:\ (NTFS) C:\images\a.iso"
    );
    read.disk.volume = None;
    assert_eq!(disk_title(&read.disk), r"C:\", "unreadable: the root alone");
    read.disk.kind = Some(DriveType::Virtual);
    assert_eq!(kind_name(DriveType::Virtual), "Virtual");
}

#[test]
fn nothing_is_printed_before_the_first_reading() {
    let panel = InfoPanel::showing(Side::Left, InfoSections::default(), None);
    assert!(rows(&panel, NUMBERS, ':').is_empty());
}

#[test]
fn a_long_value_keeps_its_end() {
    assert_eq!(truncate_left("abcdef", 4), "…def");
    assert_eq!(truncate_left("abc", 4), "abc");
    assert_eq!(truncate_left("abc", 0), "");
}

#[test]
fn battery_times_use_the_time_separator() {
    let mut read = info();
    if let Some(power) = &mut read.system.power {
        power.ac_line = AcLine::Offline;
    }
    let power = read.system.power.unwrap();
    let shown = power_rows(&power, '.');
    assert_eq!(
        shown[2],
        line("Battery life time", "Unknown"),
        "unknown on battery"
    );
    assert_eq!(shown[3], line("Battery full time", "01.02.05"));
}

#[test]
fn the_info_panel_is_never_full_screen() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = crate::app::App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    app.panels[1].set_view_mode(crate::app::ViewMode::Detailed);
    app.active = Side::Left;
    app.handle_key(ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Char('l'),
        ratatui::crossterm::event::KeyModifiers::CONTROL,
    ));
    app.active = Side::Right;
    let rects = super::super::panel_rects(Rect::new(0, 0, 100, 28), &app);
    assert!(rects[0].is_some() && rects[1].is_some(), "both halves");
}

#[test]
fn the_description_comes_last_titled_by_its_file() {
    let temp = tempfile::tempdir().unwrap();
    let mut panel = InfoPanel::showing(Side::Right, InfoSections::default(), Some(info()));
    panel.describes = true;
    let tail = |panel: &InfoPanel| {
        rows(panel, NUMBERS, ':').split_off(rows(panel, NUMBERS, ':').len() - 2)
    };
    assert_eq!(
        tail(&panel),
        [
            Row::Title("Description".to_string(), true),
            line("Folder description file is absent", ""),
        ]
    );
    panel.description = Some(Description::open(temp.path().join("File_Id.diz")));
    assert_eq!(
        tail(&panel),
        [
            Row::Title("File_Id.diz".to_string(), true),
            Row::Description
        ]
    );
    panel.sections.description = false;
    let last = rows(&panel, NUMBERS, ':').pop();
    assert_eq!(
        last,
        Some(Row::Title("Description".to_string(), false)),
        "folded"
    );
}
