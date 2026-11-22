//! FAR's `GetHiColor` over the groups of the settings, and the colours as the settings write them.

use std::time::SystemTime;

use super::*;
use crate::platform::files::Attributes;
use crate::settings::Filter;
use crate::vfs::FileMask;

fn plain_masks(text: &str) -> Option<FileMask> {
    let text = text
        .replace("<exec>", "*.exe,*.cmd")
        .replace("<arc>", "*.zip,*.r[0-9][0-9]")
        .replace("<temp>", "*.bak,*.tmp");
    FileMask::parse(&text)
}

fn far() -> Highlighting {
    Highlighting::make(&HighlightGroups::far_defaults(), &plain_masks)
}

fn file(name: &str, bits: u32) -> Entry {
    Entry {
        name: name.to_string(),
        is_dir: bits & 0x10 != 0,
        attributes: Attributes::from_windows_bits(bits),
        ..Entry::default()
    }
}

fn normal(element: Option<HighlightElement>) -> Option<u8> {
    element.and_then(|element| element.name(HighlightState::Normal).fg)
}

fn index(name: &str) -> Option<u8> {
    Paint::parse(name).fg
}

#[test]
fn fars_default_groups_take_in_their_order() {
    let far = far();
    let now = SystemTime::now();
    let colour = |name: &str, bits: u32| normal(far.element(&file(name, bits), now));
    assert_eq!(colour("cache", 0x12), index("cyan"), "hidden before folder");
    assert_eq!(colour("boot.ini", 0x4), index("cyan"), "system");
    assert_eq!(
        colour("src.zip", 0x10),
        index("white"),
        "a folder before <arc>"
    );
    assert_eq!(colour("setup.EXE", 0), index("lightgreen"));
    assert_eq!(colour("backup.zip", 0), index("lightmagenta"));
    assert_eq!(colour("part.r01", 0), index("lightmagenta"));
    assert_eq!(colour("notes.bak", 0), index("brown"));
    assert_eq!(colour("readme.md", 0), None);
    let hidden = far.element(&file("x", 0x2), now).unwrap();
    assert_eq!(hidden.name(HighlightState::Cursor).fg, index("darkgray"));
    assert_eq!(
        hidden.name(HighlightState::Selected),
        Paint::default(),
        "transparent"
    );
}

fn group(mask: &str, normal: &str, mark: &str, continue_processing: bool) -> HighlightGroup {
    HighlightGroup {
        filter: Filter {
            mask: mask.to_string(),
            ..Filter::default()
        },
        colors: HighlightColors {
            normal: normal.to_string(),
            ..HighlightColors::default()
        },
        mark: mark.to_string(),
        mark_inherit: mark.is_empty(),
        continue_processing,
    }
}

#[test]
fn continue_processing_lays_the_next_groups_over_and_the_lists_go_in_fars_order() {
    let groups = HighlightGroups {
        first: vec![group("*.rs", "on blue", "", true)],
        upper: vec![group("*.rs", "yellow", "*", false)],
        lower: vec![group("*", "red", "-", false)],
        last: vec![],
    };
    let made = Highlighting::make(&groups, &plain_masks);
    let element = made
        .element(&file("main.rs", 0), SystemTime::now())
        .unwrap();
    let paint = element.name(HighlightState::Normal);
    assert_eq!(
        (paint.fg, paint.bg),
        (index("yellow"), Paint::parse("on blue").bg)
    );
    assert_eq!(
        element.mark, "*",
        "the upper group stops before the lower one"
    );
    let other = made.element(&file("a.txt", 0), SystemTime::now()).unwrap();
    assert_eq!(
        (normal(Some(other.clone())), other.mark.as_str()),
        (index("red"), "-")
    );
}

#[test]
fn an_inherited_mark_keeps_the_one_before() {
    let groups = HighlightGroups {
        first: vec![group("*", "", "#", true), group("*", "green", "", false)],
        ..HighlightGroups::default()
    };
    let made = Highlighting::make(&groups, &plain_masks);
    let element = made.element(&file("a", 0), SystemTime::now()).unwrap();
    assert_eq!(element.mark, "#");
    assert_eq!(normal(Some(element)), index("green"));
}

#[test]
fn colours_read_as_the_theme_files_write_them() {
    assert_eq!(
        Paint::parse("lightgreen on blue"),
        Paint {
            fg: Some(10),
            bg: Some(1)
        }
    );
    assert_eq!(
        Paint::parse("on Blue"),
        Paint {
            fg: None,
            bg: Some(1)
        }
    );
    assert_eq!(
        Paint::parse("brown"),
        Paint {
            fg: Some(6),
            bg: None
        }
    );
    assert_eq!(Paint::parse(""), Paint::default(), "transparent");
    assert_eq!(Paint::parse("nonsense"), Paint::default());
    let under = Paint::parse("white on black").under(Paint::parse("on red"));
    assert_eq!(under, Paint::parse("white on red"));
}

#[test]
fn the_groups_are_saved_and_fars_come_when_none_are() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    assert_eq!(app.highlighting.kept, HighlightGroups::far_defaults());
    app.highlighting.kept.first.truncate(1);
    let saved = app.to_settings().highlighting.unwrap();
    assert_eq!(saved.first.len(), 1);
}

#[test]
fn a_mask_that_cannot_be_made_takes_nothing() {
    let groups = HighlightGroups {
        first: vec![
            group("<nothing>", "red", "", false),
            group("*", "green", "", false),
        ],
        ..HighlightGroups::default()
    };
    let made = Highlighting::make(&groups, &|text| FileMask::parse_with(text, &|_| None, ""));
    let element = made.element(&file("a.txt", 0), SystemTime::now());
    assert_eq!(normal(element), index("green"), "the next group takes it");
}

#[test]
fn the_groups_of_file_masks_make_the_highlighting_again_when_they_change() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    let now = SystemTime::now();
    let colour = |app: &App, name: &str| normal(app.highlighting.element(&file(name, 0), now));
    assert_eq!(
        colour(&app, "run.exe"),
        index("lightgreen"),
        "the real <exec>"
    );
    assert_eq!(colour(&app, "x.bak"), index("brown"));
    let at = (app.config.mask_groups.iter())
        .position(|group| group.name == "exec")
        .unwrap();
    app.remove_mask_group(at);
    assert_eq!(
        colour(&app, "run.exe"),
        None,
        "no <exec>: its group takes nothing"
    );
    assert_eq!(colour(&app, "notes.txt"), None);
    assert_eq!(
        colour(&app, "x.bak"),
        index("brown"),
        "the groups after it still look"
    );
    let at = (app.config.mask_groups.iter())
        .position(|group| group.name == "temp")
        .unwrap();
    app.config.mask_groups[at].masks = "*.old".to_string();
    app.make_highlighting();
    assert_eq!(colour(&app, "x.old"), index("brown"));
    assert_eq!(colour(&app, "x.bak"), None);
    app.restore_mask_groups();
    assert_eq!(colour(&app, "run.exe"), index("lightgreen"), "restored");
}

#[test]
fn a_new_group_inherits_its_mark() {
    assert!(
        HighlightGroup::default().mark_inherit,
        "FAR's `Mark.Inherit`"
    );
}

#[test]
fn sort_groups_give_the_upper_ones_place_none_between_and_the_lower_ones_after() {
    let groups = HighlightGroups {
        first: vec![group("*", "red", "", false)],
        upper: vec![group("*.md", "", "", false), group("*.rs", "", "", false)],
        lower: vec![
            group("*.rs,*.txt", "", "", false),
            group("*.log", "", "", false),
        ],
        last: vec![group("*", "", "", false)],
    };
    let sort = SortGroups::make(&groups, &plain_masks);
    let now = SystemTime::now();
    let of = |name: &str| sort.group(&file(name, 0), now);
    assert_eq!(of("a.md"), 0);
    assert_eq!(
        of("main.rs"),
        1,
        "the first that takes it, an upper one before a lower one"
    );
    assert_eq!(
        of("readme"),
        10_000,
        "the first and last groups are not sort groups"
    );
    assert_eq!(of("a.txt"), 10_001);
    assert_eq!(of("x.log"), 10_002);
    assert_eq!(SortGroups::default().group(&file("a.md", 0), now), 10_000);
}

#[test]
fn a_sort_groups_relative_date_is_measured_from_the_time_of_the_sort() {
    let recent = HighlightGroup {
        filter: Filter {
            use_date: true,
            date_relative: true,
            date_after: 86_400,
            ..Filter::default()
        },
        ..HighlightGroup::default()
    };
    let groups = HighlightGroups {
        upper: vec![recent],
        ..HighlightGroups::default()
    };
    let sort = SortGroups::make(&groups, &plain_masks);
    let written = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let entry = Entry {
        modified: Some(written),
        ..file("a.txt", 0)
    };
    let hour = std::time::Duration::from_secs(3_600);
    assert_eq!(
        sort.group(&entry, written + hour),
        0,
        "written within the day"
    );
    assert_eq!(
        sort.group(&entry, written + 48 * hour),
        10_000,
        "two days later: no longer"
    );
}
