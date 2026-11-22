//! The filters as kept and as a panel's area reads them.

use super::*;
use crate::app::fileops::tests::app_in;

fn entry(name: &str, is_dir: bool) -> Entry {
    Entry {
        name: name.to_string(),
        is_dir,
        ..Entry::default()
    }
}

#[test]
fn a_files_type_is_its_extension_and_folders_have_none() {
    assert_eq!(type_mask(&entry("a.TXT", false)).as_deref(), Some("*.TXT"));
    assert_eq!(type_mask(&entry("README", false)).as_deref(), Some("*."));
    assert_eq!(
        type_mask(&entry(".gitignore", false)).as_deref(),
        Some("*.gitignore"),
        "FAR's name_ext"
    );
    assert_eq!(type_mask(&entry("src", true)), None);
}

#[test]
fn a_filters_settings_become_its_conditions() {
    let filter = Filter {
        use_size: true,
        size_above: "1K".to_string(),
        use_date: true,
        date_type: "created".to_string(),
        date_relative: true,
        date_after: 60,
        use_attributes: true,
        attributes_set: 0x2,
        ..Filter::default()
    };
    let read = conditions(&filter, &|text| FileMask::parse_with(text, &|_| None, ""));
    assert!(read.mask.is_some_and(|mask| mask.matches("anything")));
    assert_eq!(read.size, Some((Some(1024), None)));
    assert_eq!(
        read.date,
        Some((
            DateKind::Created,
            Dates::Relative {
                after: Duration::from_secs(60),
                before: Duration::ZERO
            }
        ))
    );
    assert_eq!(read.attributes, Some((0x2, 0)));
}

#[test]
fn a_type_with_a_comma_is_one_mask() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let mut marks = FilterMarks::default();
    set_mark(&mut marks, Area::LeftPanel, Mark::Exclude);
    app.filters.types.push(TypeFilter {
        mask: "*.a,b".to_string(),
        marks,
    });
    let area = app.area_filter(Area::LeftPanel);
    let now = SystemTime::now();
    assert!(!area.takes(&entry("x.a,b", false), now));
    assert!(area.takes(&entry("x.a", false), now));
    assert!(
        app.area_filter(Area::RightPanel)
            .takes(&entry("x.a,b", false), now)
    );
}

#[test]
fn a_type_with_a_group_takes_the_groups_masks() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.config.mask_groups.push(crate::settings::MaskGroup {
        name: "olds".to_string(),
        masks: "*.old,*.bak".to_string(),
    });
    let mut marks = FilterMarks::default();
    set_mark(&mut marks, Area::LeftPanel, Mark::Exclude);
    app.filters.types.push(TypeFilter {
        mask: "<olds>".to_string(),
        marks,
    });
    let area = app.area_filter(Area::LeftPanel);
    let now = SystemTime::now();
    assert!(!area.takes(&entry("x.bak", false), now), "FAR expands it");
    assert!(area.takes(&entry("x.txt", false), now));
}
