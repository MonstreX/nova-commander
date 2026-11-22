//! F7 on a plugin's panel (5c-2, P6.0d): the folder made through its plugin, a nested `a\b` as FAR's
//! `ShellMakeDir` makes it.

use super::*;

#[test]
fn f7_on_a_plugins_panel_makes_the_folder_there_through_its_plugin() {
    let (temp, mut app) = app_with("panelsmkdir", &[("disks.txt", "Panel mk")]);
    app.open_disk_for_test(0, 0, Side::Left.index());
    let root = VfsPath::in_session("panelsmkdir", "mk", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &root));
    app.active = Side::Left;
    app.make_dir_in("  fresh ", None);
    tick_until(&mut app, |app| {
        app.panels[0]
            .entries
            .iter()
            .any(|entry| entry.name == "fresh")
    });
    let panel = &app.panels[0];
    assert_eq!(
        panel.entries[panel.cursor].name, "fresh",
        "the cursor on it"
    );
    assert!(log(&temp, "panelsmkdir").contains("made folder mk:/fresh"));
    app.make_dir_in("   ", None);
    for _ in 0..5 {
        app.tick();
    }
    assert!(
        app.overlay.is_none(),
        "no name: nothing asked of the plugin"
    );
    app.make_dir_in("a/..", None);
    let (lines, error) = message(&app);
    assert!(
        error && lines.iter().any(|line| line.contains("..")),
        "{lines:?}"
    );
    app.overlay = None;
    app.make_dir_in("fresh", None);
    tick_until(&mut app, |app| app.overlay.is_some());
    let (lines, _) = message(&app);
    assert!(
        lines.iter().any(|line| line.contains("already")),
        "{lines:?}"
    );
}

#[test]
fn f7_makes_a_nested_folder_taking_the_folders_on_the_way_that_are_there() {
    let (temp, mut app) = app_with("mkdirnested", &[("disks.txt", "Panel nest")]);
    app.open_disk_for_test(0, 0, Side::Left.index());
    let root = VfsPath::in_session("mkdirnested", "nest", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &root));
    app.active = Side::Left;
    // `docs` is there: taken; `x` and `y` made, the cursor on `docs`.
    app.make_dir_in(r"docs\x/y", None);
    tick_until(&mut app, |_| {
        log(&temp, "mkdirnested").contains("made folder nest:/docs/x/y")
    });
    settle(&mut app);
    let told = log(&temp, "mkdirnested");
    let made =
        |path: &str| (told.lines()).any(|line| line.ends_with(&format!("made folder {path}")));
    assert!(made("nest:/docs/x"), "{told}");
    assert!(!made("nest:/docs"), "taken, not made: {told}");
    let panel = &app.panels[0];
    assert_eq!(panel.entries[panel.cursor].name, "docs", "the first one");
    // The last one is there: the error, as FAR's.
    app.make_dir_in(r"docs\x", None);
    tick_until(&mut app, |app| app.overlay.is_some());
    let (lines, error) = message(&app);
    assert!(
        error && lines.iter().any(|line| line.contains("already")),
        "{lines:?}"
    );
    app.overlay = None;
    // A leading separator: from the panel's root, whatever folder it shows.
    app.make_dir_in(r"\top", None);
    tick_until(&mut app, |_| {
        log(&temp, "mkdirnested").contains("made folder nest:/top")
    });
}
