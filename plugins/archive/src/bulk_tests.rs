use super::*;

fn options(separate_dir: Tri, delete_archive: bool) -> ExtractOptions {
    ExtractOptions {
        dst_dir: PathBuf::new(),
        ignore_errors: false,
        overwrite: crate::extract_options::Overwrite::Ask,
        move_files: Tri::Undef,
        password: String::new(),
        separate_dir,
        delete_archive,
        disable_delete_archive: false,
        open_dir: Tri::True,
    }
}

/// arclite's `extract(arc_list)`: a folder of the archive's name when asked, or asked neither way for a
/// root of more than one entry; the folder given itself otherwise.
#[test]
fn an_archives_root_goes_into_its_own_folder_as_arclite_decides() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path();
    let archive = Path::new(r"C:\a\data.zip");
    let own = base.join("data");
    let goes =
        |separate: Tri, entries: usize| target(&options(separate, false), base, archive, entries);
    assert_eq!(goes(Tri::Undef, 2), own, "several entries");
    assert_eq!(goes(Tri::Undef, 1), base, "one entry");
    assert_eq!(goes(Tri::Undef, 0), base, "none");
    assert_eq!(goes(Tri::True, 1), own, "asked: one entry too");
    assert_eq!(goes(Tri::False, 5), base, "asked not to: several too");
    std::fs::create_dir(&own).unwrap();
    assert_eq!(
        goes(Tri::True, 1),
        base.join("data.0"),
        "a name taken: a new one"
    );
}

/// The archive is deleted only when asked and its extraction added no failure.
#[test]
fn an_archive_is_deleted_only_when_asked_and_nothing_failed() {
    assert!(deletes_archive(&options(Tri::Undef, true), false));
    assert!(
        !deletes_archive(&options(Tri::Undef, true), true),
        "a failure"
    );
    assert!(
        !deletes_archive(&options(Tri::Undef, false), false),
        "not asked"
    );
}

/// "Go to target directory": the folder given for several archives, the lone archive's own folder,
/// onto its root's one entry when it has one.
#[test]
fn the_panel_goes_where_arclite_takes_it() {
    let base = PathBuf::from(r"D:\out");
    let last = PathBuf::from(r"D:\out\data");
    let one = Some("only.txt".to_string());
    assert_eq!(
        gone_to(2, one.clone(), base.clone(), last.clone()),
        (base.clone(), None)
    );
    assert_eq!(
        gone_to(1, None, base.clone(), last.clone()),
        (last.clone(), None)
    );
    assert_eq!(gone_to(1, one.clone(), base, last.clone()), (last, one));
}

/// arclite's `is_real_file_panel`: a disk's panel by its folder, a plugin's of real names by the disk's
/// folder under it; a plugin's of its own names, and a panel that shows no files, none.
#[test]
fn only_a_panel_of_real_files_has_a_disks_folder() {
    let disk = PanelState {
        folder: r"C:\disk".to_string(),
        disk_folder: r"C:\under".to_string(),
        ..PanelState::default()
    };
    assert_eq!(real_folder(&disk), Some(PathBuf::from(r"C:\disk")));
    let own_names = PanelState {
        plugin: Some("archive".to_string()),
        folder: "/docs".to_string(),
        ..disk.clone()
    };
    assert_eq!(real_folder(&own_names), None, "a plugin's own names");
    let real_names = PanelState {
        real_names: true,
        ..own_names.clone()
    };
    assert_eq!(
        real_folder(&real_names),
        Some(PathBuf::from(r"C:\under")),
        "the disk's folder under it, not the plugin's path"
    );
    for kind in [PanelKind::Tree, PanelKind::QuickView, PanelKind::Info] {
        let shown = |state: &PanelState| {
            real_folder(&PanelState {
                kind,
                ..state.clone()
            })
        };
        assert_eq!(shown(&disk), None, "{kind:?} of a disk");
        assert_eq!(shown(&real_names), None, "{kind:?} of real names");
    }
}

/// A file that could not be read: the system's words and its path, the path named first in a test's log.
#[test]
fn a_file_not_opened_is_logged_with_its_path() {
    let error = Error::new(ErrorKind::Denied, "Access is denied.");
    let archive = Path::new(r"C:\a.zip");
    let extracted = not_opened(archive, &error, false);
    assert_eq!(extracted.system.as_deref(), Some("Access is denied."));
    assert_eq!(extracted.messages, [r"C:\a.zip"]);
    assert!(extracted.objects.is_empty());
    let tested = not_opened(archive, &error, true);
    assert_eq!(tested.objects, [r"C:\a.zip"]);
    assert!(tested.messages.is_empty());
}
