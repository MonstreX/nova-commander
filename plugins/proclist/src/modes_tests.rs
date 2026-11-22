use std::collections::HashMap;

use super::*;

fn counter(index: usize, per_second: bool, thousand: bool, float: bool) -> Source {
    Source::Counter {
        index,
        per_second,
        thousand,
        float,
    }
}

/// FAR's `TranslateMode`: each `X` the next `C<n>`, the other types as they are.
#[test]
fn custom_columns_are_numbered_in_their_order() {
    let translated = translate("N,XI,XB,XP,X0S,X6F").unwrap();
    assert_eq!(translated.columns, "N,C0,C1,C2,C3,C4");
    assert_eq!(
        translated.custom,
        vec![
            (Source::Pid, 1),
            (Source::Bits, 2),
            (Source::Priority, 3),
            (counter(0, true, false, false), 4),
            (counter(6, false, false, true), 5),
        ]
    );
    assert_eq!(translated.described, None);
    let startup = translate("N,XI,XB,XC,D,T").unwrap();
    assert_eq!(startup.columns, "N,C0,C1,C2,D,T");
    assert_eq!(startup.custom[2], (Source::Parent, 3));
}

/// FAR's "Names only" (`N,N`) as the plugin's (`XN,XN`): one column twice, so two stripes of names.
#[test]
fn a_source_again_shares_its_column() {
    let translated = translate("XN,XN").unwrap();
    assert_eq!(translated.columns, "C0,C0");
    assert_eq!(translated.custom, vec![(Source::Friendly, 0)]);
    let three = translate("XN,XI,XN,XI").unwrap();
    assert_eq!(three.columns, "C0,C1,C0,C1");
    let sized = translate_sized("XI,XN,XI", "6,0,10").unwrap();
    assert_eq!(
        sized.columns, "C0,C1,C2",
        "another width: a column of its own"
    );
    let same = translate_sized("XN,XI,XN,XI", "0,6,0,6").unwrap();
    assert_eq!(same.columns, "C0,C1,C0,C1");
}

/// `S` is per second only for a time or a count (FAR's `can_be_per_sec`): a size stays as it is.
#[test]
fn per_second_is_for_times_and_counts_only() {
    let translated = translate("X14FS,X18S,X11s,X3T").unwrap();
    assert_eq!(
        translated
            .custom
            .iter()
            .map(|(source, _)| *source)
            .collect::<Vec<_>>(),
        vec![
            counter(14, false, false, true),
            counter(18, true, false, false),
            counter(11, true, false, false),
            counter(3, false, true, false),
        ]
    );
    assert_eq!(
        translated.titles,
        vec![
            Some("IO Bytes".to_string()),
            Some("IO/s".to_string()),
            Some("PgFt/s".to_string()),
            Some("Handles".to_string()),
        ]
    );
}

#[test]
fn the_description_shows_what_its_letter_says() {
    for (columns, kind, title) in [
        ("N,ZD", Described::FileDescription, "Description"),
        ("N,ZP", Described::Path, "Full path"),
        ("N,ZW", Described::Window, "Window title"),
        ("N,zc", Described::CommandLine, "Command Line"),
    ] {
        let translated = translate(columns).unwrap();
        assert_eq!(translated.columns, "N,Z", "{columns}");
        assert_eq!(translated.described, Some(kind));
        assert_eq!(translated.titles[1].as_deref(), Some(title));
    }
}

/// The modes FAR refuses: its mode then stays empty, the program's showing.
#[test]
fn modes_far_refuses_are_none() {
    for columns in [
        "N,ZD,ZP", "N,ZX", "N,ZDX", "N,Z", "N,X", "N,XQ", "N,X23", "N,X99S",
    ] {
        assert_eq!(translate(columns), None, "{columns}");
    }
    assert!(translate("N,X22").is_some(), "the last counter");
}

/// The titles (`generate_titles`): `N` is the module, `X` its own; the program's types the program's.
#[test]
fn titles_are_fars() {
    let translated = translate("N,XI,O,XG,XU,XT,XC,XP,XB,D,S").unwrap();
    let titles: Vec<Option<&str>> = translated.titles.iter().map(Option::as_deref).collect();
    assert_eq!(
        titles,
        vec![
            Some("Module"),
            Some("PID"),
            Some("Owner"),
            Some("GDI"),
            Some("USER"),
            Some("Threads"),
            Some("PtID"),
            Some("Priority"),
            Some("Bt"),
            None,
            None,
        ]
    );
    assert_eq!(translated.columns, "N,C0,C1,C2,C3,C4,C5,C6,C7,D,S");
    assert_eq!(
        translated.custom[1],
        (Source::Owner, 2),
        "the owner is ours"
    );
}

#[test]
fn every_default_mode_translates() {
    let modes = defaults();
    assert_eq!(modes.len(), MODES);
    for mode in &modes {
        let translated = translate(&mode.columns).unwrap();
        assert_eq!(
            translated.columns.split(',').count(),
            mode.widths.split(',').count(),
            "{}",
            mode.columns
        );
    }
    assert!(modes[5].full_screen);
    assert_eq!(modes.iter().filter(|mode| mode.full_screen).count(), 1);
}

#[test]
fn modes_are_read_and_written_under_fars_names() {
    let mut saved = HashMap::new();
    let mut changed = defaults();
    changed[3].columns = "N,XI".to_string();
    changed[3].widths = "0,8".to_string();
    changed[3].full_screen = true;
    save(&changed, |key, value| {
        saved.insert(key.to_string(), value.to_string());
    });
    assert_eq!(saved.len(), MODES * 3);
    assert_eq!(saved["Mode3.ColumnsLocal"], "N,XI");
    assert_eq!(saved["Mode3.WidthsLocal"], "0,8");
    assert_eq!(saved["Mode3.FullScreenLocal"], "1");
    assert_eq!(saved["Mode5.FullScreenLocal"], "1");
    assert_eq!(saved["Mode0.FullScreenLocal"], "0");
    let loaded = load(|key| saved.get(key).cloned());
    assert_eq!(loaded, changed);
    assert_eq!(load(|_| None), defaults(), "none saved: the defaults");
}
