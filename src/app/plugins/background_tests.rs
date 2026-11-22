use super::*;

fn transfer(upload: bool, names: &[&str]) -> TransferRequest {
    TransferRequest {
        instance: "sftp-1".to_string(),
        folder: "/home/u".to_string(),
        names: names.iter().map(|name| name.to_string()).collect(),
        disk: r"C:\work".to_string(),
        upload,
        moving: false,
    }
}

/// Down: the panel's entries into the disk's folder; up: the disk's entries into the panel's folder;
/// "." and ".." and empty names are no sources.
#[test]
fn a_plugins_copy_becomes_the_programs_request() {
    let down = request_of("netway", &transfer(false, &["a.txt", "..", "", "dir"])).unwrap();
    assert_eq!(
        down.sources,
        [
            VfsPath::in_session("netway", "sftp-1", "/home/u/a.txt"),
            VfsPath::in_session("netway", "sftp-1", "/home/u/dir"),
        ]
    );
    assert_eq!(down.target, VfsPath::local(r"C:\work"));
    assert!(!down.move_);
    assert_eq!(down.existing, Existing::Ask);
    let up = request_of(
        "netway",
        &TransferRequest {
            moving: true,
            ..transfer(true, &["b.txt"])
        },
    )
    .unwrap();
    assert_eq!(up.sources, [VfsPath::local(r"C:\work\b.txt")]);
    assert_eq!(
        up.target,
        VfsPath::in_session("netway", "sftp-1", "/home/u")
    );
    assert!(up.move_);
}

/// Nothing to copy, or no disk's folder: no task.
#[test]
fn nothing_named_is_no_task() {
    assert!(request_of("netway", &transfer(false, &[".", ""])).is_none());
    let no_disk = TransferRequest {
        disk: String::new(),
        ..transfer(false, &["a.txt"])
    };
    assert!(request_of("netway", &no_disk).is_none());
}

/// R7 review A4: a name that is not one entry's of its folder, a relative disk folder or a panel name
/// with / or \ is no task - not one source left out, the whole copy refused.
#[test]
fn names_outside_the_folder_a_relative_disk_or_a_bad_panel_are_no_task() {
    for name in [r"x\..\..\y", "a/b", r"C:\Windows"] {
        let up = transfer(true, &["b.txt", name]);
        assert!(request_of("netway", &up).is_none(), "{name}");
    }
    let down = transfer(false, &["a.txt", "../etc"]);
    assert!(
        request_of("netway", &down).is_none(),
        "up past the panel's folder"
    );
    let relative = TransferRequest {
        disk: r"work\in".to_string(),
        ..transfer(false, &["a.txt"])
    };
    assert!(request_of("netway", &relative).is_none());
    let bad_panel = TransferRequest {
        instance: r"..\sftp".to_string(),
        ..transfer(false, &["a.txt"])
    };
    assert!(request_of("netway", &bad_panel).is_none());
}
