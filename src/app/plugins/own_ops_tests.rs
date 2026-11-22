use crate::app::fileops::tests::app_in;

/// A disk's panel: neither F8 nor F7 goes to a plugin, the program's own questions stay.
#[test]
fn a_disks_panel_keeps_the_programs_questions() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    let mut app = app_in(temp.path());
    assert!(!app.delete_by_plugin(false));
    assert!(!app.make_folder_by_plugin());
    assert!(app.plugins.key_wait.is_none(), "nothing waited for");
}
