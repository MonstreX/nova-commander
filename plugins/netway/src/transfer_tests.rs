use super::*;

#[test]
fn an_exported_file_is_named_by_the_whole_name() {
    assert_eq!(export_file_name("Work/Prod/db"), "Work-Prod-db.netway");
    assert_eq!(
        export_file_name("root@example.com"),
        "root@example.com.netway"
    );
}

#[test]
fn a_file_comes_in_named_without_its_extension() {
    assert_eq!(
        import_name(&Path::new("x").join("Work-Prod-db.netway")).as_deref(),
        Some("Work-Prod-db")
    );
    assert_eq!(import_name(Path::new("db")).as_deref(), Some("db"));
    // One that cannot be a session's name stays out.
    assert_eq!(import_name(Path::new("db .netway")), None);
}

#[test]
fn the_export_asks_with_the_name_or_how_many() {
    assert_eq!(export_prompt(&["db"]), "Export session \"db\" to:");
    assert_eq!(export_prompt(&["db", "Work"]), "Export 2 session(s) to:");
}

#[test]
fn a_relative_folder_is_the_programs() {
    let base = std::env::temp_dir();
    assert_eq!(typed_folder("out", Some(&base)), base.join("out"));
    assert_eq!(typed_folder(" \"out\" ", None), PathBuf::from("out"));
    let absolute = base.join("abs");
    let typed = absolute.to_string_lossy();
    assert_eq!(typed_folder(&typed, Some(Path::new("elsewhere"))), absolute);
}
