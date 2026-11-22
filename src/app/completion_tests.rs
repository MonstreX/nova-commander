//! FAR's completion list on the command line.

use super::*;
use crate::app::user_screen_link::tests::tick_until;

fn app_in(dir: &Path) -> App {
    App::new(dir, 100, 30, &crate::settings::Settings::default())
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        press(app, KeyCode::Char(ch));
    }
}

fn texts(app: &App) -> Vec<String> {
    app.completion
        .iter()
        .flat_map(|completion| &completion.items)
        .filter_map(|item| match item {
            Item::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Until the thread has answered for what is typed.
fn settled(app: &mut App) {
    assert!(tick_until(app, |app| {
        app.completion
            .as_ref()
            .is_none_or(|completion| completion.job.is_none())
    }));
}

#[test]
fn the_last_word_with_and_without_quotes() {
    let of = |line| last_word(line);
    let parts =
        |prefix: &str, word: &str, quoted| Some((prefix.to_string(), word.to_string(), quoted));
    assert_eq!(of("notepad rea"), parts("notepad ", "rea", false));
    assert_eq!(of("rea"), parts("", "rea", false));
    assert_eq!(of(r#"copy "a b"#), parts("copy \"", "a b", true));
    assert_eq!(of("copy "), None);
}

#[test]
fn a_closed_quote_is_one_word_and_dividers_end_a_word() {
    let parts =
        |prefix: &str, word: &str, quoted| Some((prefix.to_string(), word.to_string(), quoted));
    assert_eq!(last_word(r#""a b""#), parts("\"", "a b", true));
    assert_eq!(
        last_word(r#"copy "C:\Program Files\x""#),
        parts("copy \"", r"C:\Program Files\x", true)
    );
    assert_eq!(last_word("set A=%PA"), parts("set A=", "%PA", false));
    assert_eq!(last_word("a>fi"), parts("a>", "fi", false));
    assert_eq!(last_word("x&&no"), parts("x&&", "no", false));
    assert_eq!(last_word(r"..\sub"), parts("", r"..\sub", false));
}

#[test]
fn a_name_with_a_commands_meaning_is_quoted() {
    assert_eq!(on_the_line("copy ", "a&b.txt", false), "copy \"a&b.txt\"");
    assert_eq!(on_the_line("", "a(1).txt", false), "\"a(1).txt\"");
    assert_eq!(on_the_line("echo ", "%PATH%", false), "echo %PATH%");
}

#[test]
fn a_variable_is_expanded_for_its_folder_but_a_bare_one_offers_nothing() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("zz.txt"), "").unwrap();
    let mut environment = Environment::default();
    environment.set("NC_FOLDER", &temp.path().display().to_string());
    assert_eq!(
        files_for(r"%NC_FOLDER%\z", Path::new(r"C:\"), &environment),
        [r"%NC_FOLDER%\zz.txt"]
    );
    let parent = temp.path().parent().unwrap();
    assert!(files_for("%NC_FOLDER%", parent, &environment).is_empty());
}

#[cfg(windows)]
#[test]
fn a_bare_drive_means_the_panels_folder_on_that_drive() {
    let dir = Path::new(r"C:\work\dir");
    assert_eq!(folder_to_read(dir, "c:"), Some(dir.to_path_buf()));
    assert_eq!(folder_to_read(dir, "D:"), Some(PathBuf::from(r"D:\")));
    assert_eq!(folder_to_read(dir, r"D:\x\"), Some(PathBuf::from(r"D:\x\")));
    assert_eq!(folder_to_read(dir, r"C:sub\"), None);
    assert_eq!(folder_to_read(dir, r"sub\"), Some(dir.join("sub")));
}

#[test]
fn a_look_under_way_is_dropped_when_the_line_is_run() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("zz1"), "").unwrap();
    std::fs::write(temp.path().join("zz2"), "").unwrap();
    let mut app = app_in(temp.path());
    type_text(&mut app, "echo zz");

    press(&mut app, KeyCode::Enter);

    assert!(app.completion.is_none(), "not left to come up later");
}

#[test]
fn a_result_for_a_line_no_longer_there_brings_up_nothing() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("zz1"), "").unwrap();
    std::fs::write(temp.path().join("zz2"), "").unwrap();
    let mut app = app_in(temp.path());
    type_text(&mut app, "zz");
    app.command_line.set("something else");

    assert!(tick_until(&mut app, |app| app.completion.is_none()));
}

#[test]
fn left_and_right_move_in_the_line_with_a_panel_of_stripes_beside_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.active_panel_mut().set_view_metrics(20, 2);
    let panel_cursor = app.active_panel().cursor;
    app.push_history("zzone");
    app.push_history("zztwo");
    type_text(&mut app, "zz");
    settled(&mut app);
    let before = app.command_line.cursor();

    press(&mut app, KeyCode::Left);

    assert_eq!(app.command_line.cursor(), before - 1);
    assert_eq!(app.active_panel().cursor, panel_cursor);
    assert!(app.completion.is_some(), "the list stays");
}

#[test]
fn page_keys_stop_at_the_ends_and_home_end_go_to_them() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.push_history("zzone");
    app.push_history("zztwo");
    type_text(&mut app, "zz");
    settled(&mut app);

    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.command_line.text(), "zzone", "the last: no going round");
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.command_line.text(), "zzone");
    press(&mut app, KeyCode::Home);
    assert_eq!(app.command_line.text(), "zz");
    press(&mut app, KeyCode::End);
    assert_eq!(app.command_line.text(), "zzone");
}

#[test]
fn f10_closes_the_list_and_asks_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.push_history("zzone");
    app.push_history("zztwo");
    type_text(&mut app, "zz");
    settled(&mut app);

    press(&mut app, KeyCode::F(10));

    assert!(app.completion.is_none());
    assert!(app.overlay.is_none());
}

#[test]
fn with_no_room_for_the_list_the_keys_stay_with_the_panels() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::new(temp.path(), 100, 4, &crate::settings::Settings::default());
    app.push_history("zzone");
    app.push_history("zztwo");
    type_text(&mut app, "zz");
    settled(&mut app);

    assert!(!app.completion_shown());
}

#[test]
fn variables_of_set_and_its_path_are_offered() {
    let temp = tempfile::tempdir().unwrap();
    let mut environment = Environment::default();
    environment.set("NC_ZZ_MADE", "x");
    let groups = find("echo %NC_ZZ", temp.path(), &environment);
    let variables = groups
        .iter()
        .find(|group| group.title == ENVIRONMENT)
        .unwrap();
    assert_eq!(variables.items, ["echo %NC_ZZ_MADE%"]);
}

#[test]
fn a_cap_keeps_the_start_of_the_alphabet() {
    let temp = tempfile::tempdir().unwrap();
    for index in (0..MOST_FILES + 5).rev() {
        std::fs::write(temp.path().join(format!("zz{index:05}")), "").unwrap();
    }
    let names = files_for("zz", temp.path(), &Environment::default());
    assert_eq!(names.len(), MOST_FILES);
    assert_eq!(names[0], "zz00000");
}

#[test]
fn a_name_is_put_on_the_line_as_cmd_needs_it() {
    assert_eq!(on_the_line("copy ", "a.txt", false), "copy a.txt");
    assert_eq!(on_the_line("copy ", "a b.txt", false), "copy \"a b.txt\"");
    assert_eq!(on_the_line("copy \"", "a b.txt", true), "copy \"a b.txt\"");
}

#[test]
fn typing_brings_up_the_commands_given_newest_first() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    for command in ["zzone 1", "other", "zzone 2"] {
        app.push_history(command);
    }

    type_text(&mut app, "ZZ");
    settled(&mut app);

    assert!(app.completion_shown());
    let completion = app.completion.as_ref().unwrap();
    assert_eq!(completion.title, "History");
    assert_eq!(completion.items[0], Item::Typed);
    assert_eq!(texts(&app)[..2], ["zzone 2", "zzone 1"]);
}

#[test]
fn typing_goes_on_in_the_line_and_the_list_follows() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.push_history("zzone");
    app.push_history("zztwo");
    type_text(&mut app, "zz");
    settled(&mut app);
    assert_eq!(texts(&app).len(), 2);

    type_text(&mut app, "o");
    settled(&mut app);

    assert_eq!(app.command_line.text(), "zzo");
    assert_eq!(texts(&app), ["zzone"]);
    press(&mut app, KeyCode::Backspace);
    settled(&mut app);
    assert_eq!(texts(&app).len(), 2, "back to both");
}

#[test]
fn moving_in_the_list_puts_its_line_on_the_command_line() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.push_history("zzone");
    app.push_history("zztwo");
    type_text(&mut app, "zz");
    settled(&mut app);

    press(&mut app, KeyCode::Down);
    assert_eq!(app.command_line.text(), "zztwo");
    press(&mut app, KeyCode::Down);
    assert_eq!(app.command_line.text(), "zzone");
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Up);
    assert_eq!(
        app.command_line.text(),
        "zz",
        "the empty first line: what was typed"
    );
    press(&mut app, KeyCode::Up);
    assert_eq!(app.command_line.text(), "zzone", "the list goes round");
}

#[test]
fn esc_closes_it_and_keeps_the_line() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.push_history("zzone");
    app.push_history("zztwo");
    type_text(&mut app, "zz");
    settled(&mut app);
    press(&mut app, KeyCode::Down);

    press(&mut app, KeyCode::Esc);

    assert!(app.completion.is_none());
    assert_eq!(app.command_line.text(), "zztwo");
}

#[test]
fn enter_closes_it_and_runs_the_line() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.push_history("echo zzone");
    app.push_history("echo zztwo");
    type_text(&mut app, "echo z");
    settled(&mut app);

    press(&mut app, KeyCode::Enter);

    assert!(app.completion.is_none());
    assert_eq!(app.main_view, crate::app::MainView::CommandOutput);
}

#[test]
fn a_line_that_is_all_there_is_brings_up_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.push_history("zzone");
    type_text(&mut app, "zzone");
    settled(&mut app);
    assert!(!app.completion_shown());
}

#[test]
fn an_emptied_line_closes_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.push_history("zzone");
    type_text(&mut app, "z");
    press(&mut app, KeyCode::Backspace);
    assert!(app.completion.is_none());
}

#[test]
fn files_of_the_panels_folder_join_the_list_as_cmd_needs_them() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("zzfile.txt"), "").unwrap();
    std::fs::write(temp.path().join("zz with space.txt"), "").unwrap();
    let mut app = app_in(temp.path());

    type_text(&mut app, "copy zz");
    settled(&mut app);

    let completion = app.completion.as_ref().unwrap();
    assert_eq!(completion.title, "Files");
    assert_eq!(
        texts(&app),
        ["copy \"zz with space.txt\"", "copy zzfile.txt"]
    );
}

#[test]
fn a_word_with_a_folder_keeps_it_as_typed() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    std::fs::write(temp.path().join("sub").join("zz.txt"), "").unwrap();
    let mut app = app_in(temp.path());

    type_text(&mut app, r"sub\z");
    settled(&mut app);

    assert_eq!(texts(&app), [r"sub\zz.txt"]);
}

#[test]
fn sources_are_set_apart_by_titled_lines() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("zzfile"), "").unwrap();
    let mut app = app_in(temp.path());
    app.push_history("zzold");

    type_text(&mut app, "zz");
    settled(&mut app);

    let completion = app.completion.as_ref().unwrap();
    assert_eq!(completion.title, "History");
    assert!(completion.items.contains(&Item::Group("Files")));
    // The titled line is passed over.
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.command_line.text(), "zzfile");
}

#[test]
fn shift_del_takes_a_line_out_of_the_history() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.push_history("zzone");
    app.push_history("zztwo");
    type_text(&mut app, "zz");
    settled(&mut app);
    press(&mut app, KeyCode::Down);

    app.handle_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::SHIFT));

    assert_eq!(app.commands().collect::<Vec<_>>(), ["zzone"]);
    assert_eq!(texts(&app), ["zzone"]);
    assert_eq!(
        app.command_line.text(),
        "zzone",
        "the next line takes its place"
    );
}

#[test]
fn ctrl_space_brings_it_up_for_what_is_on_the_line() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.push_history("zzone");
    app.push_history("zztwo");
    app.command_line.set("zz");

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL));
    settled(&mut app);

    assert!(app.completion_shown());
    assert_eq!(app.command_line.text(), "zz");
}

#[test]
fn another_key_closes_it_and_does_its_work() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.push_history("zzone");
    app.push_history("zztwo");
    type_text(&mut app, "zz");
    settled(&mut app);

    press(&mut app, KeyCode::F(12));

    assert!(app.completion.is_none());
    assert!(app.overlay.is_some(), "F12 did its work");
}
