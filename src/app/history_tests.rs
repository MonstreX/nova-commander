use super::*;

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    (temp, app)
}

fn commands(app: &App) -> Vec<&str> {
    app.commands().collect()
}

fn folders(app: &App) -> Vec<String> {
    (app.history.lists.folders.iter())
        .map(|entry| entry.text.clone())
        .collect()
}

#[test]
fn a_command_given_again_moves_to_the_end_whatever_its_case() {
    let (_temp, mut app) = app();
    app.push_history("one");
    app.push_history("two");
    app.push_history("ONE");
    assert_eq!(commands(&app), ["two", "ONE"]);
}

#[test]
fn a_command_keeps_the_folder_it_was_given_in() {
    let (temp, mut app) = app();
    app.push_history("dir");
    assert_eq!(
        app.history.lists.commands[0].folder,
        temp.path().display().to_string()
    );
}

#[test]
fn the_history_of_an_earlier_run_is_there_after_a_restart() {
    let (temp, mut app) = app();
    let path = temp.path().join("settings").join("history.json");
    app.open_history(path.clone());
    app.push_history("cargo build");
    app.push_history("git status");
    app.remember_file(&temp.path().join("a.txt"), Kind::Edit);
    app.close_history();

    let (_second, mut restarted) = self::app();
    restarted.open_history(path);
    assert_eq!(commands(&restarted), ["cargo build", "git status"]);
    assert_eq!(restarted.history.lists.views[0].kind, Kind::Edit);
    assert_eq!(
        folders(&restarted),
        [temp.path().display().to_string()],
        "the active panel's folder at the end"
    );
    restarted.recall_history(-1);
    assert_eq!(restarted.command_line.text(), "git status");
    restarted.close_history();
}

#[test]
fn a_list_not_saved_is_of_this_run_only() {
    let (temp, mut app) = app();
    let path = temp.path().join("history.json");
    app.prefs.save_view_history = false;
    app.open_history(path.clone());
    app.remember_file(&temp.path().join("a.txt"), Kind::View);
    app.push_history("dir");
    app.close_history();
    assert_eq!(app.history.lists.views.len(), 1, "this run has it");

    let (_second, mut restarted) = self::app();
    restarted.open_history(path);
    assert!(restarted.history.lists.views.is_empty());
    assert_eq!(commands(&restarted), ["dir"]);
    restarted.close_history();
}

#[test]
fn a_broken_history_file_is_reported_and_left_alone() {
    let (temp, mut app) = app();
    let path = temp.path().join("history.json");
    std::fs::write(&path, "broken").unwrap();

    app.open_history(path.clone());
    app.push_history("x");
    app.close_history();

    assert!(app.overlay.is_some(), "it is said");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken");
}

#[test]
fn history_walks_back_and_forward() {
    let (_temp, mut app) = app();
    app.push_history("one");
    app.push_history("two");
    app.push_history("two");

    app.recall_history(-1);
    assert_eq!(app.command_line.text(), "two");
    app.recall_history(-1);
    assert_eq!(app.command_line.text(), "one");
    app.recall_history(-1);
    assert_eq!(app.command_line.text(), "one");
    app.recall_history(1);
    assert_eq!(app.command_line.text(), "two");
    app.recall_history(1);
    assert_eq!(app.command_line.text(), "");
}

#[test]
fn a_forgotten_line_is_gone_after_a_restart() {
    let (temp, mut app) = app();
    let path = temp.path().join("history.json");
    app.open_history(path.clone());
    app.push_history("keep");
    app.push_history("forget");
    app.forget_history_line("forget");
    app.close_history();

    let (_second, mut restarted) = self::app();
    restarted.open_history(path);
    assert_eq!(commands(&restarted), ["keep"]);
    restarted.close_history();
}

#[test]
fn the_folder_left_and_the_one_come_to_are_noted() {
    let (temp, mut app) = app();
    let sub = temp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    app.remember_folders();
    assert!(folders(&app).is_empty(), "nothing at the start");

    app.panels[0]
        .change_dir(&VfsPath::local(&sub), None)
        .unwrap();
    app.remember_folders();
    let start = temp.path().display().to_string();
    assert_eq!(folders(&app), [start.clone(), sub.display().to_string()]);

    app.panels[0]
        .change_dir(&VfsPath::local(temp.path()), None)
        .unwrap();
    app.remember_folders();
    assert_eq!(folders(&app), [sub.display().to_string(), start]);
}

#[test]
fn a_folder_gone_to_quietly_is_not_noted() {
    let (temp, mut app) = app();
    let sub = temp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    app.remember_folders();
    app.history.untold[0] = Some(VfsPath::local(&sub));
    app.panels[0]
        .change_dir(&VfsPath::local(&sub), None)
        .unwrap();
    app.remember_folders();
    assert!(folders(&app).is_empty());
    assert_eq!(app.history.untold[0], None);
}
