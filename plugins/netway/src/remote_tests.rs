use super::*;

fn item(name: &str) -> RemoteEntry {
    RemoteEntry {
        name: name.to_string(),
        size: 42,
        modified: Some(86_400),
        permissions: Some(0o100644),
        owner: Some("root".to_string()),
        group: Some("staff".to_string()),
        ..RemoteEntry::default()
    }
}

#[test]
fn rights_as_winscp_writes_them() {
    assert_eq!(rights(0o100644), "rw-r--r--");
    assert_eq!(rights(0o40755), "rwxr-xr-x");
    assert_eq!(rights(0o4755), "rwsr-xr-x");
    assert_eq!(rights(0o2644), "rw-r-Sr--");
    assert_eq!(rights(0o1777), "rwxrwxrwt");
    assert_eq!(rights(0o1776), "rwxrwxrwT");
    assert_eq!(octal(0o100644), "644");
    assert_eq!(octal(0o4755), "4755");
    assert_eq!(octal(0o7), "007");
}

/// A file: its size, time, owner, group and rights in the columns; nothing marked.
#[test]
fn a_file_carries_its_columns() {
    let entry = entry(&item("a.txt"));
    assert_eq!(
        (entry.name.as_str(), entry.size, entry.is_dir),
        ("a.txt", 42, false)
    );
    assert_eq!(
        entry.modified,
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(86_400))
    );
    assert_eq!(entry.columns[GROUP], "staff");
    assert_eq!(entry.columns[RIGHTS], "rw-r--r--");
    assert_eq!(entry.columns[OCTAL], "644");
    assert_eq!(entry.columns[OWNER], "root");
    assert!(!entry.hidden && !entry.read_only);
    assert_eq!(entry.attributes, 0);
}

/// A dot file is hidden, a file nobody may write read-only, a folder has no size, a link its marks.
#[test]
fn marks_as_netboxs() {
    assert!(entry(&item(".profile")).hidden);
    let mut locked = item("locked");
    locked.permissions = Some(0o100444);
    assert!(entry(&locked).read_only);
    let mut folder = item("docs");
    folder.is_dir = true;
    assert_eq!(entry(&folder).size, 0);
    let mut link = item(".link");
    link.is_link = true;
    link.is_dir = true;
    assert_eq!(entry(&link).attributes, REPARSE_POINT | DIRECTORY | HIDDEN);
    let mut unknown = item("x");
    unknown.permissions = None;
    unknown.owner = None;
    let entry = entry(&unknown);
    assert_eq!(
        (
            entry.columns[RIGHTS].as_str(),
            entry.columns[OWNER].as_str()
        ),
        ("", "")
    );
    assert!(!entry.read_only);
}

/// A panel of a WebDAV session that never connects (nothing is asked of it).
fn panel() -> Remote {
    let link = crate::link_options::Link::default();
    let connect = crate::webdav::Connect {
        host: "127.0.0.1",
        port: 9,
        link: &link,
        tls: None,
        user: "",
        password: "",
        compression: false,
    };
    let options = crate::session_options::Options::default();
    let dav = crate::webdav::Dav::new(&connect).unwrap();
    Remote {
        connection: None,
        fs: crate::fs::Fs::webdav(dav, &options),
        host: "127.0.0.1".to_string(),
        name: "test".to_string(),
        number: 1,
        start: None,
        url: String::new(),
        plan: crate::login::Plan {
            host: "127.0.0.1".to_string(),
            port: 9,
            user: String::new(),
            password: String::new(),
            key: None,
            known_hosts: std::path::PathBuf::new(),
            folder: String::new(),
            protocol: crate::session::Protocol::default(),
            encryption: crate::session::Encryption::default(),
            scp_fallback: false,
            options,
        },
        retries: 0,
        stored: None,
        folder: "/srv".to_string(),
        history: Vec::new(),
        edits: Vec::new(),
        synchronized: true,
        seen: std::cell::RefCell::new(None),
        unfollowed: std::cell::RefCell::new(None),
        background: None,
    }
}

/// A command's move is not followed once: its folder matched with or without a trailing "/" (the root
/// too), taken by the first move asked about whatever it is; another folder is followed.
#[test]
fn a_commands_move_is_unfollowed_once() {
    let mut remote = panel();
    assert!(!remote.take_unfollowed("/srv"), "nothing told");
    remote.unfollowed_to("/srv/docs/");
    assert!(remote.take_unfollowed("/srv/docs"));
    assert!(!remote.take_unfollowed("/srv/docs"), "taken once");
    remote.unfollowed_to("/srv/docs");
    assert!(remote.take_unfollowed("/srv/docs//"));
    remote.unfollowed_to("/");
    assert!(remote.take_unfollowed("/"), "the root");
    remote.unfollowed_to("/srv/docs");
    assert!(!remote.take_unfollowed("/srv/other"), "another folder");
    assert!(!remote.take_unfollowed("/srv/docs"), "taken by that move");
    remote.unfollowed_to("/srv/docs");
    assert!(!remote.take_unfollowed("/srv/doc"));
    assert!(!remote.take_unfollowed("/srv"));
}

#[test]
fn the_detailed_mode_is_netboxs() {
    let modes = modes();
    assert_eq!(modes.len(), 10);
    let detailed = modes[DETAILED].as_ref().unwrap();
    assert_eq!(detailed.columns, "N,S,DM,C4,C0,C1");
    assert_eq!(detailed.widths, "0,8,14,0,0,9");
    assert!(detailed.full_screen);
    assert_eq!(detailed.titles[3].as_deref(), Some("Owner"));
    assert!(
        modes
            .iter()
            .enumerate()
            .all(|(n, mode)| mode.is_some() == (n == DETAILED))
    );
}
