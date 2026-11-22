use super::*;

#[test]
fn a_typed_name_takes_backslashes_for_slashes() {
    assert_eq!(typed_name(r" Work\Prod\db "), "Work/Prod/db");
    assert_eq!(typed_name("/db/"), "db");
    assert_eq!(typed_name("db"), "db");
}

fn press(current: Option<&str>) -> KeyPress {
    KeyPress {
        key: "Enter".to_string(),
        current: current.map(str::to_string),
        ..KeyPress::default()
    }
}

/// Enter on a folder or on ".." is the program's; on the hint nothing.
#[test]
fn enter_goes_into_folders_and_up() {
    let folder = Item::Folder("Work".to_string());
    assert!(enter_folder(&press(Some("Work")), Some(&folder)).pass);
    assert!(enter_folder(&press(None), None).pass);
    let hint = enter_folder(&press(Some(sessions::HINT)), None);
    assert_eq!(hint, KeyAnswer::default());
}
