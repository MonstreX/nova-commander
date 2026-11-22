use super::*;
use crate::session::Protocol;

fn store() -> (tempfile::TempDir, Store) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::new(temp.path().join("Sessions"));
    (temp, store)
}

fn session(host: &str) -> Session {
    Session {
        host: host.to_string(),
        user: "me".to_string(),
        ..Session::default()
    }
}

#[test]
fn names_join_and_split_by_slashes() {
    assert_eq!(joined("", "db"), "db");
    assert_eq!(joined("/", "db"), "db");
    assert_eq!(joined("/Work/Prod", "db"), "Work/Prod/db");
    assert_eq!(split("Work/Prod/db"), ("Work/Prod", "db"));
    assert_eq!(split("db"), ("", "db"));
}

/// A store not written yet is empty at its root; a folder of it that is not there is not found.
#[test]
fn a_new_store_is_empty_and_its_folders_are_not_there() {
    let (_temp, store) = store();
    assert_eq!(store.items("/").unwrap(), []);
    assert!(store.has_none());
    let missing = store.items("/Work").unwrap_err();
    assert_eq!(missing.kind(), io::ErrorKind::NotFound);
}

/// A session saved is a file `<name>.netway` in its folders, listed as a session; folders as folders;
/// other files are not the store's.
#[test]
fn sessions_are_files_in_folders() {
    let (_temp, store) = store();
    store
        .save("Work/Prod/db", &session("db.example.com"))
        .unwrap();
    store.save("home", &session("home.example.com")).unwrap();
    store.make_folder("Empty").unwrap();
    std::fs::write(store.folder_path("").join("notes.txt"), "x").unwrap();
    assert!(store.file("Work/Prod/db").ends_with("Work/Prod/db.netway"));
    assert!(store.file("Work/Prod/db").is_file());
    assert_eq!(
        store.items("").unwrap(),
        [
            Item::Folder("Empty".to_string()),
            Item::Folder("Work".to_string()),
            Item::Session("home".to_string()),
        ]
    );
    assert_eq!(
        store.items("/Work/Prod").unwrap(),
        [Item::Session("db".to_string())]
    );
    assert!(!store.has_none());
    assert_eq!(
        store.exists("work/prod/DB"),
        cfg!(windows),
        "the disk's case rules"
    );
    assert_eq!(
        store.item_named("Work", "Prod"),
        Some(Item::Folder("Prod".to_string()))
    );
    assert_eq!(
        store.item_named("", "home"),
        Some(Item::Session("home".to_string()))
    );
    assert_eq!(store.item_named("", "nothing"), None);
}

#[test]
fn only_folders_hold_no_session() {
    let (_temp, store) = store();
    store.make_folder("A/B").unwrap();
    assert!(store.has_none(), "folders alone");
    store.save("A/B/c", &session("c")).unwrap();
    assert!(!store.has_none(), "found deep down");
}

/// A session reads back as saved; its password is kept protected (Windows), never in the clear.
#[test]
fn a_session_reads_back_its_password_protected() {
    let (_temp, store) = store();
    let mut saved = session("example.com");
    saved.protocol = Protocol::Scp;
    saved.password = "pa55word".to_string();
    let kept = store.save("s", &saved).unwrap();
    let text = std::fs::read_to_string(store.file("s")).unwrap();
    assert!(!text.contains("pa55word"), "{text}");
    let loaded = store.load("s").unwrap();
    if cfg!(windows) {
        assert!(kept);
        assert_eq!(loaded.session, saved);
    } else {
        assert!(!kept, "no secret kept off Windows yet");
        assert_eq!(loaded.session.password, "");
    }
    assert!(!loaded.password_lost);
}

/// A password another user kept (or broken) is lost: the session comes without it, and says so.
#[test]
fn a_password_not_read_back_is_lost() {
    let (_temp, store) = store();
    store.save("s", &session("h")).unwrap();
    let file = store.file("s");
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, format!("{text}Password=00ff\n")).unwrap();
    let loaded = store.load("s").unwrap();
    assert!(loaded.password_lost);
    assert_eq!(loaded.session.password, "");
    assert_eq!(loaded.session.host, "h");
}

/// A session without a password writes none; saving again replaces the file whole.
#[test]
fn saving_again_replaces_the_session() {
    let (_temp, store) = store();
    assert!(!store.save("s", &session("one")).unwrap(), "no password");
    assert!(
        !std::fs::read_to_string(store.file("s"))
            .unwrap()
            .contains("Password")
    );
    store.save("s", &session("two")).unwrap();
    assert_eq!(store.load("s").unwrap().session.host, "two");
    let names: Vec<_> = std::fs::read_dir(store.folder_path(""))
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(names, ["s.netway"], "no temporary file left");
}

/// Rename and copy never go over another session; a folder named in the new name is made.
#[test]
fn rename_and_copy_keep_other_sessions() {
    let (_temp, store) = store();
    store.save("a", &session("a")).unwrap();
    store.save("b", &session("b")).unwrap();
    let taken = store.rename("a", "b").unwrap_err();
    assert_eq!(taken.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(taken.to_string(), "Session named 'b' already exists.");
    assert_eq!(
        store.copy("a", "b").unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    store.copy("a", "Work/a2").unwrap();
    assert_eq!(store.load("Work/a2").unwrap().session.host, "a");
    assert!(store.exists("a"), "a copy keeps its source");
    store.rename("b", "Work/b").unwrap();
    assert!(!store.exists("b") && store.exists("Work/b"));
    let gone = store.rename("nothing", "x").unwrap_err();
    assert_eq!(gone.kind(), io::ErrorKind::NotFound);
}

/// Another case of a name: on Windows the same session (renamed by it, never copied over itself); where the
/// disk tells cases apart, another session, never overwritten.
#[test]
fn another_case_is_the_same_session_only_where_the_disk_says_so() {
    let (_temp, store) = store();
    store.save("a", &session("small")).unwrap();
    if cfg!(windows) {
        assert!(store.same("a", "A"));
        let itself = store.copy("a", "A").unwrap_err();
        assert_eq!(itself.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(store.load("a").unwrap().session.host, "small", "whole");
        store.rename("a", "A").unwrap();
        assert_eq!(store.items("").unwrap(), [Item::Session("A".to_string())]);
    } else {
        store.save("A", &session("capital")).unwrap();
        assert!(!store.same("a", "A"));
        let taken = store.rename("a", "A").unwrap_err();
        assert_eq!(taken.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(store.load("A").unwrap().session.host, "capital");
    }
    assert!(!store.same("a", "nothing"), "a name not there");
}

/// A copy keeps the kept password as it was.
#[test]
fn a_copy_keeps_the_password() {
    let (_temp, store) = store();
    let mut saved = session("h");
    saved.password = "pw".to_string();
    store.save("a", &saved).unwrap();
    store.copy("a", "b").unwrap();
    assert_eq!(
        store.load("b").unwrap().session.password,
        store.load("a").unwrap().session.password
    );
}

/// A session removed is gone; a folder removed goes with all its sessions.
#[test]
fn a_folder_is_removed_with_its_sessions() {
    let (_temp, store) = store();
    store.save("Work/Prod/db", &session("db")).unwrap();
    store.save("Work/web", &session("web")).unwrap();
    store.save("home", &session("home")).unwrap();
    store
        .remove("", &Item::Session("home".to_string()))
        .unwrap();
    assert!(!store.exists("home"));
    store
        .remove("/", &Item::Folder("Work".to_string()))
        .unwrap();
    assert_eq!(store.items("").unwrap(), []);
    assert!(store.has_none());
}

/// A folder's sessions with those of its folders, its folders' first (the panel's order): the export of a
/// folder, the command line's stored sessions.
#[test]
fn the_sessions_under_a_folder_by_their_whole_names() {
    let (_temp, store) = store();
    assert_eq!(store.sessions_under(""), Vec::<String>::new());
    for name in ["home", "Work/web", "Work/Prod/db", "Work/Prod/cache"] {
        store.save(name, &session(name)).unwrap();
    }
    assert_eq!(
        store.sessions_under(""),
        ["Work/Prod/cache", "Work/Prod/db", "Work/web", "home"]
    );
    assert_eq!(
        store.sessions_under("/Work/Prod"),
        ["Work/Prod/cache", "Work/Prod/db"]
    );
    assert_eq!(store.sessions_under("Gone"), Vec::<String>::new());
}

/// A session's file read and written as it is (an import): the keys not known and a kept password with
/// it, its folders made, no temporary file left.
#[test]
fn a_sessions_text_goes_in_and_out_as_it_is() {
    let (_temp, store) = store();
    let text = "Protocol=ftp\nHost=h\nPassword=00ff\nLater=kept\n";
    store.write_text("Work/imported", text).unwrap();
    assert_eq!(store.text("Work/imported").unwrap(), text);
    assert_eq!(
        store.items("Work").unwrap(),
        [Item::Session("imported".to_string())]
    );
    let loaded = store.load("Work/imported").unwrap();
    assert_eq!(loaded.session.protocol, Protocol::Ftp);
    assert!(loaded.password_lost, "not a password kept here");
    store.write_text("Work/imported", "Protocol=scp\n").unwrap();
    assert_eq!(store.text("Work/imported").unwrap(), "Protocol=scp\n");
    assert!(store.text("Work/none").is_err());
}
