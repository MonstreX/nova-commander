//! The operations on the server's files against the tests' server (S6.5d).

use super::*;
use crate::remote_ops::Properties;
use crate::rights_box::Rights;
use crate::test_server::rights_of;

/// A folder with all its items goes, a file too; one not there is asked about and skipped.
#[test]
fn delete_takes_folders_whole_and_asks_about_failures() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    fs::write(srv.join("docs/deep/a.txt"), "a").unwrap();
    fs::write(srv.join("docs/b.txt"), "b").unwrap();
    fs::write(srv.join("c.txt"), "c").unwrap();
    let session = folders.session();
    let work = Work::Delete("/srv", names(&["docs", "c.txt"]));
    let (done, seen) = run(&session, Setup::default(), work, Vec::new());
    assert_eq!((done, seen), (Ok(()), Vec::new()));
    assert!(!srv.join("docs").exists() && !srv.join("c.txt").exists());
    let skip = vec![TransferReply::Failed(Failed::Skip)];
    let work = Work::Delete("/srv", names(&["none.txt"]));
    let (done, seen) = run(&session, Setup::default(), work, skip);
    assert_eq!(done, Ok(()));
    assert!(
        matches!(&seen[..], [TransferQuestion::Failed(_)]),
        "{seen:?}"
    );
}

/// A folder made with its rights; one there already asked about with WinSCP's headline.
#[test]
fn a_folder_is_made_with_its_rights() {
    let folders = Folders::new();
    let session = folders.session();
    let (done, _) = run(
        &session,
        Setup::default(),
        Work::Folder("/srv/new", Some(0o750)),
        Vec::new(),
    );
    assert_eq!(done, Ok(()));
    let made = folders.root().join("srv/new");
    assert!(made.is_dir());
    assert_eq!(rights_of(&made), Some(0o750));
    let skip = vec![TransferReply::Failed(Failed::Skip)];
    let (_, seen) = run(
        &session,
        Setup::default(),
        Work::Folder("/srv/new", None),
        skip,
    );
    let [TransferQuestion::Failed(lines)] = &seen[..] else {
        panic!("{seen:?}");
    };
    assert!(
        lines[0].starts_with("Error creating folder '/srv/new'."),
        "{lines:?}"
    );
}

/// Moved into another folder under the mask's names; duplicated whole with the times.
#[test]
fn files_move_and_duplicate_on_the_server() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("a.txt"), "alpha", 1_000_000_000);
    write(&srv.join("docs/deep/b.txt"), "beta", 1_100_000_000);
    fs::create_dir_all(srv.join("to")).unwrap();
    let session = folders.session();
    let duplicate = Work::Move("/srv", names(&["docs"]), ("/srv/to", "copy"), true);
    assert_eq!(
        run(&session, Setup::default(), duplicate, Vec::new()).0,
        Ok(())
    );
    let copied = srv.join("to/copy/deep/b.txt");
    assert_eq!(fs::read_to_string(&copied).unwrap(), "beta");
    assert_eq!(modified(&copied), 1_100_000_000);
    assert!(
        srv.join("docs/deep/b.txt").exists(),
        "duplicated, not moved"
    );
    let moving = Work::Move("/srv", names(&["a.txt"]), ("/srv/to", "*.bak"), false);
    assert_eq!(
        run(&session, Setup::default(), moving, Vec::new()).0,
        Ok(())
    );
    assert_eq!(fs::read_to_string(srv.join("to/a.bak")).unwrap(), "alpha");
    assert!(!srv.join("a.txt").exists());
    let skip = vec![TransferReply::Failed(Failed::Skip)];
    let again = Work::Move("/srv/to", names(&["a.bak"]), ("/srv/to", "copy"), false);
    let (_, seen) = run(&session, Setup::default(), again, skip);
    let [TransferQuestion::Failed(lines)] = &seen[..] else {
        panic!("{seen:?}");
    };
    assert!(
        lines[0].starts_with("Error renaming file '/srv/to/a.bak' to '/srv/to/copy'."),
        "{lines:?}"
    );
}

/// The rights set on the items named and, recursively, their items; a folder's X where it reads.
#[test]
fn properties_go_to_the_items_and_recursively() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    fs::write(srv.join("docs/a.txt"), "a").unwrap();
    let session = folders.session();
    let properties = Properties {
        rights: Some(Rights::of(0o640)),
        add_x: true,
        recursive: true,
        ..Properties::default()
    };
    let work = Work::Properties("/srv", names(&["docs"]), properties);
    assert_eq!(run(&session, Setup::default(), work, Vec::new()).0, Ok(()));
    assert_eq!(rights_of(&srv.join("docs")), Some(0o750));
    assert_eq!(rights_of(&srv.join("docs/a.txt")), Some(0o640));
    assert_eq!(properties.rights_for(0o600, false), Some(0o640));
    let kept = Properties {
        rights: Some(Rights {
            set: 0o004,
            unset: 0,
        }),
        ..Properties::default()
    };
    assert_eq!(
        kept.rights_for(0o640, false),
        Some(0o644),
        "only the others' R added"
    );
}

/// A link the server cannot make is asked about with WinSCP's headline.
#[test]
fn a_link_refused_is_asked_about() {
    let folders = Folders::new();
    let session = folders.session();
    let skip = vec![TransferReply::Failed(Failed::Skip)];
    let work = Work::Link("/srv/link", "/srv/docs", true);
    let (done, seen) = run(&session, Setup::default(), work, skip);
    assert_eq!(done, Ok(()));
    let [TransferQuestion::Failed(lines)] = &seen[..] else {
        panic!("{seen:?}");
    };
    assert!(
        lines[0].starts_with("Cannot create link '/srv/link'."),
        "{lines:?}"
    );
}
