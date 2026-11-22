//! The temporary panel from the command line and from list files (ADR-0016 §14, 5d-4; FAR's TmpPanel
//! `OpenW`), its library as it is shipped: `tmp:` with a list file named from the active folder, Replace
//! and `-replace`, the common panel by `+n`, a command's output, Enter and Ctrl+PgDn on a list file opening
//! the panel over the file's (".." back on the file, the selection kept), and "Menu from file list".

use super::*;

fn run_line(app: &mut App, line: &str) {
    app.command_line.set(line);
    app.run_command_line();
}

/// The files' folder's `name` holding `lines`.
fn list(files: &tempfile::TempDir, name: &str, lines: &str) -> std::path::PathBuf {
    let path = files.path().join(name);
    std::fs::write(&path, lines).unwrap();
    path
}

fn whole(files: &tempfile::TempDir, name: &str) -> String {
    files.path().join(name).display().to_string()
}

fn title(app: &App, side: Side) -> Option<String> {
    app.panels[side.index()].look.title.clone()
}

#[test]
fn tmp_with_a_list_file_opens_the_panel_with_its_paths() {
    let (_temp, files, mut app) = opened("tmplistone");
    list(&files, "list.temp", "a.txt\r\n\r\nsub\\b.txt\nnot there\n");
    run_line(&mut app, "tmp:list.temp");
    let wanted = vec![whole(&files, "a.txt"), whole(&files, r"sub\b.txt")];
    tick_until(&mut app, |app| names(app, Side::Right) == wanted);
    assert_eq!(
        title(&app, Side::Right).as_deref(),
        Some("Temporary panel [0]")
    );
    assert!(app.command_line.is_empty(), "the line went");
    app.active = Side::Right;
    assert_eq!(
        app.prompt_folder(),
        "",
        "FAR's prompt: TmpPanel's `CurDir`, empty"
    );
    app.active = Side::Left;
    assert_eq!(app.prompt_folder(), "", "the left one too");
}

#[test]
fn a_list_replaces_the_panels_paths_and_minus_replace_adds_them() {
    let (_temp, files, mut app) = opened("tmplisttwo");
    list(&files, "one.temp", "a.txt\n");
    list(&files, "two.temp", "sub\n");
    run_line(&mut app, "tmp:one.temp");
    let a = whole(&files, "a.txt");
    tick_until(&mut app, |app| names(app, Side::Right) == [a.clone()]);
    let back = VfsPath::local(files.path());
    app.panels[1].change_dir(&back, None).unwrap();
    tick_until(&mut app, |app| shows(app, Side::Right, &back));
    run_line(&mut app, "tmp:two.temp");
    let sub = whole(&files, "sub");
    // FAR's Replace: the list in place of what was there
    tick_until(&mut app, |app| names(app, Side::Right) == [sub.clone()]);
    app.panels[1].change_dir(&back, None).unwrap();
    tick_until(&mut app, |app| shows(app, Side::Right, &back));
    run_line(&mut app, "tmp:-replace one.temp");
    tick_until(&mut app, |app| {
        // Folders first, as the panel sorts them.
        names(app, Side::Right) == [sub.clone(), a.clone()]
    });
}

#[test]
fn plus_a_digit_opens_that_common_panel_and_a_list_not_found_does_nothing() {
    let (_temp, files, mut app) = opened("tmplistthree");
    run_line(&mut app, "tmp:+3");
    tick_until(&mut app, |app| {
        title(app, Side::Right).as_deref() == Some("Temporary panel [3]")
    });
    let back = VfsPath::local(files.path());
    app.panels[1].change_dir(&back, None).unwrap();
    tick_until(&mut app, |app| shows(app, Side::Right, &back));
    run_line(&mut app, "tmp:nowhere.temp");
    for _ in 0..50 {
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(shows(&app, Side::Right, &back), "FAR: no panel, no message");
    assert!(app.overlay.is_none());
}

#[cfg(windows)]
#[test]
fn a_commands_output_is_the_list() {
    let (_temp, files, mut app) = opened("tmplistfour");
    run_line(&mut app, "tmp:<dir /b /s *.txt");
    let wanted = vec![whole(&files, "a.txt"), whole(&files, r"sub\b.txt")];
    tick_until(&mut app, |app| {
        names(app, Side::Right) == wanted && app.overlay.is_none()
    });
}

#[test]
fn enter_on_a_list_file_opens_the_panel_over_it_and_dotdot_comes_back_to_it() {
    let (_temp, files, mut app) = opened("tmplistfive");
    list(&files, "list.temp", "a.txt\n");
    let back = VfsPath::local(files.path());
    app.panels[1].reload();
    tick_until(&mut app, |app| {
        names(app, Side::Right).contains(&"list.temp".to_string())
    });
    point(&mut app, Side::Right, "a.txt");
    app.handle_key(key(KeyCode::Insert));
    point(&mut app, Side::Right, "list.temp");
    app.handle_key(key(KeyCode::Enter));
    let a = whole(&files, "a.txt");
    tick_until(&mut app, |app| names(app, Side::Right) == [a.clone()]);
    // ".." at its root: back to the files, the cursor on the list file, a.txt still selected.
    app.panels[1].cursor = 0;
    app.handle_key(key(KeyCode::Enter));
    tick_until(&mut app, |app| shows(app, Side::Right, &back));
    let panel = &app.panels[1];
    assert_eq!(
        panel.current().map(|entry| entry.name.as_str()),
        Some("list.temp")
    );
    assert!(panel.selected.contains("a.txt"), "FAR's PopPrevData");
    // Ctrl+PgDn opens it too (FAR's `OFP_ALTERNATIVE`).
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL));
    tick_until(&mut app, |app| names(app, Side::Right) == [a.clone()]);
}

/// "Menu from file list" for `tmp:+menu list.temp`: the menu up with its items' texts.
fn menu_of(app: &mut App) -> Vec<String> {
    run_line(app, "tmp:+menu list.temp");
    tick_until(app, |app| {
        matches!(app.overlay, Some(Overlay::PluginMenu(_)))
    });
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        unreachable!();
    };
    (menu.lines.iter()).map(|line| line.text.clone()).collect()
}

#[test]
fn a_menu_from_a_list_takes_the_panel_to_a_folder() {
    let (_temp, files, mut app) = opened("tmplistsix");
    list(
        &files,
        "list.temp",
        "|&Folder|sub\n|-|\n|Run it|notepad readme\n",
    );
    assert_eq!(menu_of(&mut app), ["&Folder", "", "Run it"]);
    app.handle_key(key(KeyCode::Enter));
    let sub = VfsPath::local(files.path().join("sub"));
    tick_until(&mut app, |app| shows(app, Side::Right, &sub));
}

#[test]
fn a_menu_from_a_list_puts_what_is_no_path_in_the_command_line() {
    let (_temp, files, mut app) = opened("tmplistseven");
    list(
        &files,
        "list.temp",
        "|&Folder|sub\n|Run it|notepad readme\n",
    );
    menu_of(&mut app);
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    tick_until(&mut app, |app| app.command_line.text() == "notepad readme");
    let back = VfsPath::local(files.path());
    assert!(shows(&app, Side::Right, &back), "no panel opened");
}

#[test]
fn a_list_file_on_the_temporary_panel_opens_another_over_it() {
    // Panels of their own (CommonPanel off): the left one holds the list file, by its whole path.
    let (_temp, files, mut app) = opened_with("tmplisteight", &[("CommonPanel", "0")], "local");
    // By its whole path: a relative name is FAR's current folder's, not the list's (`GetFullPath`).
    let list_file = list(
        &files,
        "list.temp",
        &format!("{}\n", whole(&files, "a.txt")),
    );
    app.panels[1].reload();
    tick_until(&mut app, |app| {
        names(app, Side::Right).contains(&"list.temp".to_string())
    });
    point(&mut app, Side::Right, "list.temp");
    app.handle_key(key(KeyCode::Insert));
    copy(&mut app, KeyCode::F(5));
    let whole_list = list_file.display().to_string();
    tick_until(&mut app, |app| {
        names(app, Side::Left) == [whole_list.clone()] && app.plugins.putting.is_none()
    });
    let under = app.panels[0].path.clone();
    app.active = Side::Left;
    point(&mut app, Side::Left, &whole_list);
    app.handle_key(key(KeyCode::Enter));
    let a = whole(&files, "a.txt");
    // FAR's `PushPrev`: over the temporary panel, even the same plugin's.
    tick_until(&mut app, |app| {
        names(app, Side::Left) == [a.clone()] && app.panels[0].path != under
    });
    assert_eq!(
        app.panels[0].under(1),
        Some(&under),
        "the first one under it"
    );
    app.panels[0].cursor = 0;
    app.handle_key(key(KeyCode::Enter));
    tick_until(&mut app, |app| {
        app.panels[0].path == under && app.panels[0].reading().is_none()
    });
    assert_eq!(
        app.panels[0].current().map(|entry| entry.name.clone()),
        Some(whole_list),
        "back on the list file, named by its whole path"
    );
}

#[test]
fn found_files_go_to_the_current_common_panel_in_place_of_the_active_one() {
    let (_temp, files, mut app) = opened("tmplistnine");
    let a = files.path().join("a.txt");
    let sub = files.path().join("sub");
    app.find_list_to_plugins(found(&[a.clone(), sub.clone()]));
    let wanted = vec![sub.display().to_string(), a.display().to_string()];
    tick_until(&mut app, |app| names(app, Side::Right) == wanted);
    assert_eq!(
        title(&app, Side::Right).as_deref(),
        Some("Temporary panel [0]"),
        "without the option: the current one"
    );
    // FAR's `SetFindList` empties it first, whatever Replace says.
    app.find_list_to_plugins(found(std::slice::from_ref(&a)));
    let only_a = vec![a.display().to_string()];
    tick_until(&mut app, |app| names(app, Side::Right) == only_a);
}

#[test]
fn with_a_new_panel_for_search_results_the_first_empty_common_panel_takes_them() {
    let values = [("NewPanelForSearchResults", "1")];
    let (_temp, files, mut app) = opened_with("tmplistten", &values, "common");
    // Panel 0 holds something: the results go to panel 1, now the current one.
    list(&files, "list.temp", "a.txt\n");
    run_line(&mut app, "tmp:list.temp");
    tick_until(&mut app, |app| !names(app, Side::Right).is_empty());
    let back = VfsPath::local(files.path());
    app.panels[1].change_dir(&back, None).unwrap();
    tick_until(&mut app, |app| shows(app, Side::Right, &back));
    app.find_list_to_plugins(found(&[files.path().join("sub")]));
    tick_until(&mut app, |app| {
        title(app, Side::Right).as_deref() == Some("Temporary panel [1]")
    });
    assert_eq!(names(&app, Side::Right), [whole(&files, "sub")]);
}

/// Alt+F7's found objects as the search hands them: each named by its whole path, which is its real
/// name, a folder told.
fn found(paths: &[std::path::PathBuf]) -> Vec<crate::vfs::Entry> {
    (paths.iter())
        .map(|path| crate::vfs::Entry {
            name: path.display().to_string(),
            is_dir: path.is_dir(),
            real_path: Some(path.clone()),
            ..crate::vfs::Entry::default()
        })
        .collect()
}
