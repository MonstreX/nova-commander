use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::{Command, OptionsPage};

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let app = app_in(temp.path());
    (temp, app)
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn form_mut(app: &mut App) -> &mut Form {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("no dialog"),
    }
}

fn title(app: &App) -> String {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form.title.clone(),
        Some(Overlay::Dialog(Dialog::List { title, .. })) => title.clone(),
        Some(Overlay::Dialog(Dialog::Message { title, .. })) => title.clone(),
        Some(Overlay::Dialog(Dialog::Confirm { title, .. })) => title.clone(),
        _ => String::new(),
    }
}

fn list_items(app: &App) -> Vec<String> {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::List { items, .. })) => items.clone(),
        _ => panic!("no list"),
    }
}

/// Item `index` of the dialog in front, focused, and Enter on it.
fn press_item(app: &mut App, index: usize) {
    form_mut(app).focus_item(index);
    press(app, KeyCode::Enter);
}

fn button_index(form: &Form, label: &str) -> usize {
    form.items
        .iter()
        .position(
            |item| matches!(&item.control, Control::Button { label: found, .. } if found == label),
        )
        .unwrap()
}

/// The shell's dialog filled in.
fn fill(app: &mut App, name: &str, program: &str, arguments: &str, dialect: Dialect) {
    let form = form_mut(app);
    form.set_edit_text(NAME, name);
    form.set_edit_text(PROGRAM, program);
    form.set_edit_text(ARGUMENTS, arguments);
    if let Control::DropDown { chosen, .. } = &mut form.items[DIALECT].control {
        *chosen = Dialect::ALL
            .iter()
            .position(|known| *known == dialect)
            .unwrap();
    }
}

/// Command line settings, then its "Shells..." button.
fn open_list(app: &mut App) {
    app.execute(Command::Options(OptionsPage::CommandLine));
    let shells = button_index(form_mut(app), "S&hells...");
    press_item(app, shells);
    assert_eq!(title(app), TITLE);
}

fn add(app: &mut App, name: &str, program: &str, arguments: &str, dialect: Dialect) {
    press(app, KeyCode::Insert);
    assert_eq!(title(app), "Shell");
    fill(app, name, program, arguments, dialect);
    press_item(app, OK);
}

#[test]
fn shells_opens_the_list_and_esc_brings_back_command_line_settings() {
    let (_temp, mut app) = app();
    open_list(&mut app);
    assert_eq!(list_items(&app), [NO_SHELLS]);
    press(&mut app, KeyCode::Esc);
    assert_eq!(title(&app), "Command line settings");
}

#[test]
fn a_shell_added_is_in_the_list_and_among_the_shells_to_choose() {
    let (_temp, mut app) = app();
    open_list(&mut app);
    add(
        &mut app,
        "My bash",
        r"C:\msys64\usr\bin\bash.exe",
        r#"--login -c "exec bash""#,
        Dialect::Msys,
    );
    assert_eq!(list_items(&app), ["My bash"]);
    let mine = &app.user_shells[0];
    assert_eq!(mine.args(), ["--login", "-c", "exec bash"]);
    assert_eq!(mine.dialect(), Dialect::Msys);
    let titles: Vec<String> = crate::terminal::available_shells(&app.user_shells)
        .iter()
        .map(|shell| shell.title().to_string())
        .collect();
    assert_eq!(titles.last().map(String::as_str), Some("My bash"));
}

#[test]
fn a_shell_needs_a_name_of_its_own_and_a_program() {
    let (_temp, mut app) = app();
    open_list(&mut app);
    add(&mut app, "one", "a.exe", "", Dialect::Cmd);
    add(&mut app, "ONE", "b.exe", "", Dialect::Cmd);
    let Some(Overlay::Dialog(Dialog::Message { lines, .. })) = &app.overlay else {
        panic!("no error");
    };
    assert_eq!(lines, &["There is a shell of this name already."]);
    press(&mut app, KeyCode::Enter);
    assert_eq!(title(&app), "Shell", "the dialog is back as it was");
    assert_eq!(form_mut(&mut app).edit_text(PROGRAM), "b.exe");
    form_mut(&mut app).set_edit_text(NAME, " ");
    press_item(&mut app, OK);
    press(&mut app, KeyCode::Enter);
    form_mut(&mut app).set_edit_text(NAME, "two");
    press_item(&mut app, OK);
    assert_eq!(list_items(&app), ["one", "two"]);
}

#[test]
fn f4_changes_a_shell_and_the_guest_follows_it() {
    let (_temp, mut app) = app();
    open_list(&mut app);
    add(&mut app, "one", "a.exe", "", Dialect::Cmd);
    app.shell = app.user_shells[0].clone();
    press(&mut app, KeyCode::F(4));
    assert_eq!(form_mut(&mut app).edit_text(PROGRAM), "a.exe");
    fill(&mut app, "uno", "b.exe", "-x", Dialect::PowerShell);
    press_item(&mut app, OK);
    assert_eq!(list_items(&app), ["uno"]);
    assert_eq!(app.shell.id(), "user:uno");
    assert_eq!(app.shell.program(), std::path::Path::new("b.exe"));
}

#[test]
fn cancel_leaves_the_shell_as_it_was() {
    let (_temp, mut app) = app();
    open_list(&mut app);
    add(&mut app, "one", "a.exe", "", Dialect::Cmd);
    press(&mut app, KeyCode::Enter);
    form_mut(&mut app).set_edit_text(NAME, "changed");
    press(&mut app, KeyCode::Esc);
    assert_eq!(list_items(&app), ["one"]);
}

#[test]
fn del_asks_and_takes_the_shell_out_and_the_guest_goes_back_to_the_default() {
    let (_temp, mut app) = app();
    open_list(&mut app);
    add(&mut app, "one", "a.exe", "", Dialect::Cmd);
    add(&mut app, "two", "b.exe", "", Dialect::Cmd);
    app.shell = app.user_shells[0].clone();
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Delete);
    assert_eq!(title(&app), "Delete shell");
    press(&mut app, KeyCode::Esc);
    assert_eq!(list_items(&app), ["one", "two"], "Cancel keeps it");
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Enter);
    assert_eq!(list_items(&app), ["two"]);
    assert_eq!(app.shell, crate::terminal::Shell::default_shell());
}

#[test]
fn the_shells_and_the_one_chosen_outlive_the_program() {
    let (temp, mut app) = app();
    app.user_shells.push(Shell::user(
        "Git",
        r"E:\Git\bin\bash.exe",
        "--login -i",
        Dialect::Msys,
    ));
    app.shell = app.user_shells[0].clone();
    let settings = app.to_settings();
    let text = serde_json::to_string(&settings).unwrap();
    assert!(text.contains(r#""type":"msys""#), "{text}");
    let back: crate::settings::Settings = serde_json::from_str(&text).unwrap();
    let restored = App::new(temp.path(), 100, 40, &back);
    assert_eq!(restored.user_shells, app.user_shells);
    assert_eq!(restored.shell, app.shell);
}

fn message(app: &App) -> Vec<String> {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Message { lines, .. })) => lines.clone(),
        _ => panic!("no message"),
    }
}

#[test]
fn a_shell_without_a_name_or_a_program_is_not_kept() {
    let (_temp, mut app) = app();
    open_list(&mut app);
    add(&mut app, " ", "a.exe", "", Dialect::Cmd);
    assert_eq!(message(&app), ["The shell needs a name and a program."]);
    press(&mut app, KeyCode::Enter);
    fill(&mut app, "one", "", "", Dialect::Cmd);
    press_item(&mut app, OK);
    assert_eq!(message(&app), ["The shell needs a name and a program."]);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Esc);
    assert_eq!(list_items(&app), [NO_SHELLS]);
}

#[test]
fn a_program_in_quotes_is_the_path_inside_them() {
    let (_temp, mut app) = app();
    open_list(&mut app);
    add(
        &mut app,
        "one",
        r#""C:\Program Files\x\bash.exe""#,
        "",
        Dialect::Msys,
    );
    assert_eq!(
        app.user_shells[0].program(),
        std::path::Path::new(r"C:\Program Files\x\bash.exe")
    );
}

#[test]
fn an_open_list_asks_for_no_drawing_of_its_own() {
    let (_temp, mut app) = app();
    open_list(&mut app);
    add(&mut app, "one", "a.exe", "", Dialect::Cmd);
    app.tick();
    assert!(!app.tick(), "nothing changed");
}

#[test]
fn command_line_settings_come_back_as_they_were_left_with_the_new_shell() {
    let (_temp, mut app) = app();
    app.execute(Command::Options(OptionsPage::CommandLine));
    // The default shell changed in the dialog, not yet taken.
    if let Control::DropDown { chosen, lines, .. } = &mut form_mut(&mut app).items[1].control {
        *chosen = lines.len() - 1;
    }
    let shells = button_index(form_mut(&mut app), "S&hells...");
    press_item(&mut app, shells);
    add(&mut app, "zz mine", "a.exe", "", Dialect::Cmd);
    press(&mut app, KeyCode::Esc);
    assert_eq!(title(&app), "Command line settings");
    let form = form_mut(&mut app);
    let Control::DropDown { lines, chosen, .. } = &form.items[1].control else {
        panic!("no list of shells");
    };
    assert_eq!(lines.last(), Some(&ListLine::Choice("zz mine".to_string())));
    assert_eq!(*chosen, lines.len() - 2, "the choice made stays");
    assert_eq!(form.focus, shells, "on the button it was left at");
}

#[test]
fn the_buttons_letter_reaches_it() {
    let (_temp, mut app) = app();
    app.execute(Command::Options(OptionsPage::CommandLine));
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::ALT));
    assert_eq!(title(&app), TITLE);
}

#[test]
fn a_rename_of_the_shell_chosen_in_the_settings_keeps_it_chosen() {
    let (_temp, mut app) = app();
    app.user_shells
        .push(Shell::user("one", "a.exe", "", Dialect::Cmd));
    app.shell = app.user_shells[0].clone();
    open_list(&mut app);
    press(&mut app, KeyCode::F(4));
    form_mut(&mut app).set_edit_text(NAME, "uno");
    press_item(&mut app, OK);
    press(&mut app, KeyCode::Esc);
    assert_eq!(title(&app), "Command line settings");
    let ok = button_index(form_mut(&mut app), "OK");
    press_item(&mut app, ok);
    assert_eq!(app.shell.id(), "user:uno", "not cmd");
}

#[test]
fn a_name_of_the_systems_is_taken() {
    let (_temp, mut app) = app();
    open_list(&mut app);
    add(&mut app, "Command Prompt (cmd)", "a.exe", "", Dialect::Cmd);
    assert_eq!(message(&app), ["There is a shell of this name already."]);
}
