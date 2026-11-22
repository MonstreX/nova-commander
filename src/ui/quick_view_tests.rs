//! The quick view drawn as FAR's `QuickView::DisplayObject`.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::app::{FileText, Side};
// Tests only: the count itself stands where the test says.
use crate::jobs::folder_info::{FolderInfo, InfoCount};
use crate::vfs::VfsPath;

fn rows(view: &QuickView, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), view, true))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

fn numbers() -> NumberFormat {
    NumberFormat {
        thousands_separator: ' ',
        decimal_separator: '.',
    }
}

fn counted(over: bool) -> Counted {
    Counted {
        info: FolderInfo {
            folders: 3,
            files: 12,
            bytes: 10_000,
            allocated: 12_288,
            cluster: Some(4096),
            slack: 2288,
            mft_overhead: 0,
        },
        link: None,
        over,
    }
}

fn folder(path: &str, counted: Counted) -> QuickView {
    let shown = Shown {
        path: VfsPath::local(path),
        what: Showing::Folder {
            count: InfoCount::at(counted.clone()),
            seen: counted,
        },
    };
    QuickView::showing(Side::Right, Some(shown))
}

#[test]
fn a_folder_s_rows_stand_where_far_puts_them() {
    let view = folder(r"C:\work\sub", counted(true));
    let shown = rows(&view, 40, 20);
    assert!(shown[0].contains(" Quick view "), "{shown:#?}");
    assert_eq!(shown[2].trim_matches(['║', ' ']), r#"Folder "C:\work\sub""#);
    let row = |y: usize| shown[y].trim_matches(['║', ' ']).to_string();
    assert_eq!(row(4), "Folders:        3");
    assert_eq!(row(5), "Files:          12");
    assert!(row(6).starts_with("Files size:     "), "{}", row(6));
    assert!(
        row(7).ends_with("(122%)"),
        "allocated of the bytes: {}",
        row(7)
    );
    assert_eq!(row(8), "", "a blank row before the disk's");
    let separator = crate::platform::locale::number_format().thousands_separator;
    assert_eq!(
        row(9),
        format!("Cluster size:   {}", group_digits(4096, separator))
    );
    assert!(
        row(10).ends_with("(18%)"),
        "slack of the allocated: {}",
        row(10)
    );
    assert!(row(11).ends_with("(0%)"));
    assert!(shown[17].starts_with("╟─"), "the separator: {shown:#?}");
    assert_eq!(row(18), "sub", "the name on the last row");
}

#[test]
fn a_count_not_ended_is_marked() {
    let view = folder(r"C:\work\sub", counted(false));
    let lines = folder_lines(view.shown.as_ref().unwrap(), &counted(false), numbers(), 38);
    let folders = lines.iter().find(|line| line.row == 4).unwrap();
    assert_eq!(folders.value, "~3");
    assert!(folders.unfinished);
}

#[test]
fn a_folder_off_the_disks_has_no_disk_rows_and_a_link_its_row() {
    let mut inside = counted(true);
    inside.info.cluster = None;
    inside.link = Some(FolderLink {
        kind: LinkKind::Junction,
        target: Some(r"D:\data".to_string()),
    });
    let view = folder(r"C:\link", inside.clone());
    let lines = folder_lines(view.shown.as_ref().unwrap(), &inside, numbers(), 38);
    let rows: Vec<u16> = lines.iter().map(|line| line.row).collect();
    assert_eq!(rows, [2, 3, 4, 5, 6, 7]);
    assert_eq!(lines[1].label, r#"Junction "D:\data""#);
}

#[test]
fn links_are_named_as_far_names_them() {
    let link = |kind, target: &str| FolderLink {
        kind,
        target: Some(target.to_string()),
    };
    assert_eq!(
        link_line(&link(LinkKind::Symlink, r"C:\x"), 40),
        r#"Symlink "C:\x""#
    );
    let volume = r"\\?\Volume{0a1b}\";
    assert_eq!(
        link_line(&link(LinkKind::Junction, volume), 40),
        format!("Volume \"{volume}\"")
    );
    let cloud = LinkKind::from_reparse_tag(0x9000_001A);
    assert_eq!(link_line(&link(cloud, ""), 40), "Reparse point CLOUD");
    let unknown = LinkKind::from_reparse_tag(0x1234);
    assert_eq!(link_line(&link(unknown, ""), 40), "Unknown reparse point");
}

#[test]
fn a_file_s_type_sits_on_the_separator() {
    // A file that is not there: its type is drawn whatever its contents.
    let temp = tempfile::tempdir().unwrap();
    let missing = VfsPath::local(temp.path().join("run.bat"));
    let shown = Shown {
        path: missing.clone(),
        what: Showing::File {
            type_name: Some("Windows Batch File".to_string()),
            text: Box::new(FileText::open(missing)),
        },
    };
    let view = QuickView::showing(Side::Right, Some(shown));
    let rows = rows(&view, 40, 12);
    assert!(rows[9].contains(" Windows Batch File "), "{rows:#?}");
    assert!(rows[10].contains("run.bat"));
}

#[test]
fn a_long_path_keeps_its_root_and_its_end() {
    assert_eq!(truncate_path(r"C:\a\b", 10), r"C:\a\b");
    assert_eq!(truncate_path(r"C:\one\two\three", 12), r"C:\…wo\three");
    assert_eq!(
        truncate_path(r"C:\one\two\three", 4),
        r"C:\…",
        "FAR: the root before the end"
    );
    assert_eq!(truncate_path(r"C:\one\two\three", 2), "C…");
    assert_eq!(truncate_path(r"C:\one", 0), "");
}

#[test]
fn percents_round_down_as_far_s() {
    assert_eq!(to_percent(0, 10), 0);
    assert_eq!(to_percent(5, 0), 0);
    assert_eq!(to_percent(10, 10), 100);
    assert_eq!(to_percent(2, 3), 66);
    assert_eq!(to_percent(u64::MAX, u64::MAX / 2), 200);
}

#[test]
fn a_long_link_target_keeps_its_root_and_its_end() {
    let link = FolderLink {
        kind: LinkKind::Junction,
        target: Some(r"D:\a\very\long\folder\name".to_string()),
    };
    // 20 inside: "Junction" and 5 more leave 7: the ellipsis, the root, three of the end.
    assert_eq!(link_line(&link, 20), r#"Junction "D:\…ame""#);
}

#[test]
fn a_file_s_contents_fill_the_box_above_the_separator() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("notes.txt");
    std::fs::write(&path, "hello\n\tworld\n").unwrap();
    let text = Box::new(FileText::open(VfsPath::local(&path)));
    let shown = Shown {
        path: VfsPath::local(&path),
        what: Showing::File {
            type_name: None,
            text,
        },
    };
    let mut view = QuickView::showing(Side::Right, Some(shown));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let shown = loop {
        let shown = rows(&view, 30, 10);
        if shown[2].contains("world") {
            break shown;
        }
        let Some(Shown {
            what: Showing::File { text, .. },
            ..
        }) = view.shown.as_mut()
        else {
            unreachable!()
        };
        text.pump();
        assert!(std::time::Instant::now() < deadline, "{shown:#?}");
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert_eq!(shown[1], "║hello                       ║");
    assert_eq!(
        shown[2], "║        world               ║",
        "a tab to its stop"
    );
    assert_eq!(
        shown[6], "║                            ║",
        "the last row of text"
    );
    assert!(shown[7].starts_with("╟─"), "the separator");
    assert_eq!(shown[8].trim_matches(['║', ' ']), "notes.txt");
}
