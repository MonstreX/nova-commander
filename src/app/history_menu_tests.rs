use super::*;
use crate::app::console_input::{Mouse, MouseAction};
use crate::app::{Cells, Command, MenuArea};
use crate::platform::input::MouseButtons;
use crate::settings::Settings;

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let app = App::new(temp.path(), 100, 30, &Settings::default());
    (temp, app)
}

/// A key, and the disk work it started done (the menu looks for folders and files beside the UI).
fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    crate::app::waiting::finish_waiting(app);
}

fn press(app: &mut App, code: KeyCode) {
    key(app, code, KeyModifiers::NONE);
}

fn menu(app: &App) -> &HistoryMenu {
    let Some(Overlay::History(menu)) = &app.overlay else {
        panic!("no history menu: {:?}", app.overlay.is_some());
    };
    menu
}

/// The entries' texts, a `√` before the locked ones.
fn entries(app: &App) -> Vec<String> {
    (menu(app).lines.iter())
        .filter_map(|line| match line {
            HistoryLine::Entry { text, locked, .. } => {
                Some(format!("{}{text}", if *locked { "√" } else { "" }))
            }
            HistoryLine::Date(_) => None,
        })
        .collect()
}

fn selected_text(app: &App) -> String {
    match &menu(app).lines[menu(app).selected] {
        HistoryLine::Entry { text, .. } => text.clone(),
        HistoryLine::Date(date) => panic!("a date is selected: {date}"),
    }
}

fn commands(app: &mut App, lines: &[&str]) {
    for line in lines {
        app.push_history(line);
    }
}

#[test]
fn alt_f8_shows_the_commands_under_their_date_the_newest_selected() {
    let (_temp, mut app) = app();
    commands(&mut app, &["one", "two", "three"]);
    key(&mut app, KeyCode::F(8), KeyModifiers::ALT);
    assert_eq!(menu(&app).list, List::Commands);
    assert!(matches!(menu(&app).lines[0], HistoryLine::Date(_)));
    assert_eq!(entries(&app), ["one", "two", "three"]);
    assert_eq!(selected_text(&app), "three");
    press(&mut app, KeyCode::Down);
    assert_eq!(selected_text(&app), "one", "round the end, past the date");
    press(&mut app, KeyCode::Up);
    assert_eq!(selected_text(&app), "three");
    press(&mut app, KeyCode::Home);
    assert_eq!(selected_text(&app), "one");
    press(&mut app, KeyCode::Esc);
    assert!(app.overlay.is_none());
}

#[test]
fn ctrl_enter_puts_the_command_on_the_line_only() {
    let (_temp, mut app) = app();
    commands(&mut app, &["dir /b", "ver"]);
    app.execute(Command::History(List::Commands));
    press(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
    assert!(app.overlay.is_none());
    assert_eq!(app.command_line.text(), "dir /b");
    assert_eq!(app.commands().count(), 2, "nothing was run");
}

#[test]
fn ins_locks_and_del_leaves_the_locked_after_the_question() {
    let (_temp, mut app) = app();
    commands(&mut app, &["keep", "go"]);
    app.execute(Command::History(List::Commands));
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Insert);
    assert_eq!(entries(&app), ["√keep", "go"]);
    press(&mut app, KeyCode::Delete);
    let Some(Overlay::Dialog(Dialog::Confirm { lines, buttons, .. })) = &app.overlay else {
        panic!("the question");
    };
    assert_eq!(
        lines,
        &["All records in the history will be deleted. Continue?"]
    );
    assert_eq!(buttons, &["&Clear history", "Cancel"]);
    press(&mut app, KeyCode::Enter);
    assert_eq!(entries(&app), ["√keep"], "the menu is back");
    assert_eq!(app.commands().collect::<Vec<_>>(), ["keep"]);
}

#[test]
fn cancel_of_the_question_keeps_everything_and_the_menu() {
    let (_temp, mut app) = app();
    commands(&mut app, &["one"]);
    app.execute(Command::History(List::Commands));
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Esc);
    assert_eq!(entries(&app), ["one"]);
}

#[test]
fn with_its_question_off_del_clears_at_once() {
    let (_temp, mut app) = app();
    app.prefs.confirm_history_clear = false;
    commands(&mut app, &["one", "two"]);
    app.execute(Command::History(List::Commands));
    press(&mut app, KeyCode::Delete);
    assert!(entries(&app).is_empty());
}

#[test]
fn shift_del_takes_out_the_line_unless_it_is_locked() {
    let (_temp, mut app) = app();
    commands(&mut app, &["one", "two"]);
    app.execute(Command::History(List::Commands));
    key(&mut app, KeyCode::Delete, KeyModifiers::SHIFT);
    assert_eq!(entries(&app), ["one"]);
    assert_eq!(selected_text(&app), "one");
    press(&mut app, KeyCode::Insert);
    key(&mut app, KeyCode::Delete, KeyModifiers::SHIFT);
    assert_eq!(entries(&app), ["√one"]);
}

#[test]
fn ctrl_c_copies_the_line() {
    let (_temp, mut app) = app();
    commands(&mut app, &["echo x"]);
    app.execute(Command::History(List::Commands));
    key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(app.clipboard.text().unwrap().as_deref(), Some("echo x"));
    assert!(app.overlay.is_some(), "the menu stays");
}

#[test]
fn f3_tells_the_folder_a_command_was_given_in() {
    let (temp, mut app) = app();
    commands(&mut app, &["dir"]);
    app.execute(Command::History(List::Commands));
    press(&mut app, KeyCode::F(3));
    let Some(Overlay::Dialog(Dialog::Message { title, lines, .. })) = &app.overlay else {
        panic!("no information");
    };
    assert_eq!(title, "Information");
    assert_eq!(lines[1], temp.path().display().to_string());
    press(&mut app, KeyCode::Enter);
    assert_eq!(entries(&app), ["dir"]);
}

/// The folders history with these folders, the newest selected.
fn folders_menu(app: &mut App, folders: &[&std::path::Path]) {
    for folder in folders {
        app.add_to_history(
            List::Folders,
            Entry::new(&folder.display().to_string(), Kind::Plain),
        );
    }
    key(app, KeyCode::F(12), KeyModifiers::ALT);
}

#[test]
fn enter_goes_to_the_folder_and_ctrl_shift_enter_in_the_passive_panel() {
    let (temp, mut app) = app();
    let sub = temp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    folders_menu(&mut app, &[&sub]);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.panels[0].path, sub);

    folders_menu(&mut app, &[temp.path()]);
    key(
        &mut app,
        KeyCode::Enter,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(app.panels[1].path, temp.path().to_path_buf());
    assert_eq!(app.panels[0].path, sub, "the active one stays");
}

#[test]
fn shift_enter_goes_to_the_folder_unnoted() {
    let (temp, mut app) = app();
    let sub = temp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    app.remember_folders();
    folders_menu(&mut app, &[&sub]);
    key(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    app.remember_folders();
    assert_eq!(app.panels[0].path, sub);
    assert_eq!(app.history.lists.folders.len(), 1, "only the one given");
}

#[test]
fn a_folder_no_longer_there_is_said_and_the_menu_comes_back() {
    let (temp, mut app) = app();
    let gone = temp.path().join("gone");
    folders_menu(&mut app, &[&gone]);
    press(&mut app, KeyCode::Enter);
    let Some(Overlay::Dialog(Dialog::Message { title, lines, .. })) = &app.overlay else {
        panic!("no error");
    };
    assert_eq!(title, "Folders history");
    assert_eq!(lines.last(), Some(&gone.display().to_string()));
    press(&mut app, KeyCode::Enter);
    assert_eq!(entries(&app), [gone.display().to_string()]);
    assert_eq!(app.panels[0].path, temp.path().to_path_buf());
}

#[test]
fn ctrl_r_takes_out_the_folders_gone_but_not_the_locked() {
    let (temp, mut app) = app();
    let (here, gone, locked) = (temp.path(), temp.path().join("a"), temp.path().join("b"));
    folders_menu(&mut app, &[&locked, &gone, here]);
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Insert);
    key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    let locked = format!("√{}", locked.display());
    assert_eq!(entries(&app), [locked, here.display().to_string()]);
}

#[test]
fn the_view_history_opens_as_each_line_was_opened_and_f3_f4_choose() {
    let (temp, mut app) = app();
    let file = temp.path().join("a.txt");
    std::fs::write(&file, "text").unwrap();
    app.remember_file(&file, Kind::Edit);
    key(&mut app, KeyCode::F(11), KeyModifiers::ALT);
    assert_eq!(entries(&app), [format!("Edit: {}", file.display())]);
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.screen, Screen::Editor(_)));

    app.screen = Screen::Panels;
    key(&mut app, KeyCode::F(11), KeyModifiers::ALT);
    press(&mut app, KeyCode::F(3));
    assert!(matches!(app.screen, Screen::Viewer(_)));
    assert_eq!(
        app.history.lists.views.len(),
        2,
        "a view of it is a line of its own"
    );

    app.screen = Screen::Panels;
    key(&mut app, KeyCode::F(11), KeyModifiers::ALT);
    key(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
    assert_eq!(app.command_line.text(), file.display().to_string());
}

#[test]
fn f3_and_f4_on_the_panel_note_the_file() {
    let (temp, mut app) = app();
    std::fs::write(temp.path().join("a.txt"), "text").unwrap();
    app.panels[0].reload();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !app.panels[0]
        .entries
        .iter()
        .any(|entry| entry.name == "a.txt")
    {
        assert!(std::time::Instant::now() < deadline, "the file never came");
        app.tick();
    }
    app.panels[0].go_to_name("a.txt");
    press(&mut app, KeyCode::F(4));
    app.screen = Screen::Panels;
    press(&mut app, KeyCode::F(3));
    let kinds: Vec<Kind> = app.history.lists.views.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [Kind::Edit, Kind::View]);
}

#[test]
fn the_commands_menu_has_the_three_histories() {
    let labels: Vec<&str> = (crate::app::menu::MENUS[2].items.iter())
        .filter_map(|item| match item {
            crate::app::menu::MenuItem::Item { label, key, .. } => {
                Some(*label).filter(|_| key.starts_with("Alt+F"))
            }
            crate::app::menu::MenuItem::Separator => None,
        })
        .collect();
    assert_eq!(
        labels,
        [
            "&Find file",
            "&History",
            "Fi&nd folder",
            "File vie&w history",
            "F&olders history"
        ]
    );
}

#[test]
fn pgup_and_pgdn_move_by_a_page_of_what_is_shown() {
    let (_temp, mut app) = app();
    let lines: Vec<String> = (0..30).map(|index| format!("c{index}")).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    commands(&mut app, &lines);
    app.execute(Command::History(List::Commands));
    menu(&app).rows.set(10);
    press(&mut app, KeyCode::PageUp);
    assert_eq!(selected_text(&app), "c20");
    press(&mut app, KeyCode::PageDown);
    assert_eq!(selected_text(&app), "c29");
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::PageDown);
    assert_eq!(selected_text(&app), "c9");
    press(&mut app, KeyCode::PageUp);
    assert_eq!(selected_text(&app), "c0", "not onto the date");
}

/// The box the tests put the menu in: its lines from row 6.
const FRAME: Cells = Cells {
    x: 10,
    y: 5,
    width: 50,
    height: 8,
};

fn mouse(row: u16, buttons: MouseButtons, action: MouseAction) -> Mouse {
    Mouse {
        column: 20,
        row,
        buttons,
        pressed: buttons,
        action,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn a_click_on_a_line_takes_it_and_the_wheel_moves_over_the_dates() {
    let (temp, mut app) = app();
    let sub = temp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    folders_menu(&mut app, &[&sub, temp.path()]);
    let area = MenuArea {
        frame: FRAME,
        outer: FRAME,
        first: 0,
    };
    app.mouse.menu = Some(area);
    app.handle_mouse(mouse(0, MouseButtons::default(), MouseAction::Wheel(1)));
    assert_eq!(selected_text(&app), sub.display().to_string());
    app.handle_mouse(mouse(0, MouseButtons::default(), MouseAction::Wheel(1)));
    assert_eq!(
        selected_text(&app),
        sub.display().to_string(),
        "the date is skipped"
    );

    app.handle_mouse(mouse(
        FRAME.y + 3,
        MouseButtons::default(),
        MouseAction::Moved,
    ));
    assert_eq!(selected_text(&app), temp.path().display().to_string());
    let left = MouseButtons {
        left: true,
        ..MouseButtons::default()
    };
    app.mouse.menu = Some(area);
    app.handle_mouse(mouse(FRAME.y + 2, left, MouseAction::Button));
    app.handle_mouse(mouse(
        FRAME.y + 2,
        MouseButtons::default(),
        MouseAction::Button,
    ));
    crate::app::waiting::finish_waiting(&mut app);
    assert!(app.overlay.is_none());
    assert_eq!(app.panels[0].path, sub);
}

#[test]
fn ctrl_r_takes_out_the_files_gone() {
    let (temp, mut app) = app();
    let here = temp.path().join("here.txt");
    std::fs::write(&here, "x").unwrap();
    app.remember_file(&temp.path().join("gone.txt"), Kind::View);
    app.remember_file(&here, Kind::Edit);
    key(&mut app, KeyCode::F(11), KeyModifiers::ALT);
    key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert_eq!(entries(&app), [format!("Edit: {}", here.display())]);
}

#[test]
fn what_a_history_has_no_use_for_closes_the_menu_only() {
    let (temp, mut app) = app();
    let file = temp.path().join("a.txt");
    std::fs::write(&file, "text").unwrap();
    app.remember_file(&file, Kind::View);
    key(&mut app, KeyCode::F(11), KeyModifiers::ALT);
    key(
        &mut app,
        KeyCode::Enter,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert!(app.overlay.is_none());
    assert!(matches!(app.screen, Screen::Panels), "nothing opened");

    commands(&mut app, &["dir"]);
    app.execute(Command::History(List::Commands));
    key(
        &mut app,
        KeyCode::Enter,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert!(app.overlay.is_none());
    assert!(
        app.command_line.is_empty(),
        "FAR's Alt+F8 does nothing with it"
    );
}

#[cfg(windows)]
#[test]
fn shift_enter_runs_the_command_in_a_new_window() {
    let (temp, mut app) = app();
    // A folder cmd cannot stand in: the start is refused before any window opens.
    let verbatim = format!(r"\\?\{}", temp.path().display());
    app.panels[0].path = VfsPath::local(verbatim);
    commands(&mut app, &["echo x"]);
    app.execute(Command::History(List::Commands));
    key(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    let Some(Overlay::Dialog(Dialog::Message { lines, .. })) = &app.overlay else {
        panic!("not started in a new window");
    };
    assert_eq!(
        lines,
        &[crate::platform::process::INTERPRETER_FOLDER_REFUSAL.to_string()]
    );
}

#[test]
fn the_menu_behind_a_dialog_is_dropped_when_the_panels_are_gone() {
    let (_temp, mut app) = app();
    commands(&mut app, &["one"]);
    app.execute(Command::History(List::Commands));
    let Some(Overlay::History(open)) = app.overlay.take() else {
        panic!("no menu");
    };
    app.history.behind = Some(open);
    app.main_view = MainView::CommandOutput;
    app.resume_history_menu();
    assert!(app.overlay.is_none() && app.history.behind.is_none());
}

#[test]
fn the_open_menu_follows_a_folder_noted_meanwhile() {
    let (temp, mut app) = app();
    let (a, b) = (temp.path().join("a"), temp.path().join("b"));
    folders_menu(&mut app, &[&a, &b]);
    press(&mut app, KeyCode::Up);
    assert_eq!(selected_text(&app), a.display().to_string());
    // A slow folder read ends while the menu is up.
    app.add_to_history(
        List::Folders,
        Entry::new(&a.display().to_string(), Kind::Plain),
    );
    assert_eq!(
        entries(&app),
        [b.display().to_string(), a.display().to_string()]
    );
    assert_eq!(
        selected_text(&app),
        a.display().to_string(),
        "the cursor stays on its line"
    );
    key(&mut app, KeyCode::Delete, KeyModifiers::SHIFT);
    assert_eq!(entries(&app), [b.display().to_string()]);
}

#[test]
fn a_quiet_visit_that_does_not_happen_leaves_nothing_behind() {
    let (temp, mut app) = app();
    app.remember_folders();
    folders_menu(&mut app, &[temp.path()]);
    key(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    assert_eq!(app.history.untold[0], None, "the panel is there already");

    let gone = temp.path().join("gone");
    app.history.untold[0] = Some(VfsPath::local(&gone));
    app.remember_folders();
    assert_eq!(app.history.untold[0], None, "the panel never got there");
}

#[test]
fn ctrl_alt_f_filters_the_lines_and_the_keys_act_on_those_left() {
    let (_temp, mut app) = app();
    commands(&mut app, &["dir /b", "git status", "Git log", "ver"]);
    key(&mut app, KeyCode::F(8), KeyModifiers::ALT);
    let ctrl_alt = KeyModifiers::CONTROL | KeyModifiers::ALT;
    key(&mut app, KeyCode::Char('f'), ctrl_alt);
    for ch in "GIT".chars() {
        key(&mut app, KeyCode::Char(ch), KeyModifiers::SHIFT);
    }
    assert_eq!(entries(&app), ["git status", "Git log"], "in any case");
    assert!(
        matches!(menu(&app).lines[0], HistoryLine::Date(_)),
        "the day's date over them"
    );
    assert_eq!(menu(&app).filter.as_ref().unwrap().text, "GIT");
    // Locked, the keys are the menu's again: Ins locks the line under the cursor.
    key(&mut app, KeyCode::Char('l'), ctrl_alt);
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Insert);
    assert_eq!(entries(&app), ["√git status", "Git log"]);
    key(&mut app, KeyCode::Char('l'), ctrl_alt);
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    assert_eq!(entries(&app).len(), 4, "every line back");
    key(&mut app, KeyCode::Char('f'), ctrl_alt);
    assert_eq!(menu(&app).filter, None);
}

#[test]
fn with_no_line_left_a_character_is_not_added_and_another_key_closes_the_menu() {
    let (_temp, mut app) = app();
    commands(&mut app, &["one", "two"]);
    key(&mut app, KeyCode::F(8), KeyModifiers::ALT);
    key(
        &mut app,
        KeyCode::Char('f'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Char('y'));
    assert!(entries(&app).is_empty());
    assert_eq!(
        menu(&app).filter.as_ref().unwrap().text,
        "x",
        "nothing after the last line went"
    );
    press(&mut app, KeyCode::Enter);
    assert!(app.overlay.is_none());
}
