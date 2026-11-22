//! Writing through 7-Zip's installed library, its edges: what is before an archive kept, a format not
//! there, a stop leaving no volume or module, the password given only when encrypting, a new item's
//! properties.

use std::time::{Duration, SystemTime};

use super::*;

/// The `.tmp` files and volumes left in `folder`.
fn left_over(folder: &Path) -> Vec<std::path::PathBuf> {
    (std::fs::read_dir(folder).unwrap())
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            (path.extension()).is_some_and(|ext| ext == "tmp" || ext == "001" || ext == "exe")
        })
        .collect()
}

/// A 7z after a stub (an SFX's program) changed: the stub copied before the new archive, which opens
/// after it with the file added and the one kept.
#[test]
fn a_change_keeps_what_is_before_the_archive() {
    let folder = sources();
    let library = Library::find(folder.path());
    let mut asks = Asks::default();
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let plan = new_plan(folder.path(), &["top.txt"], &mut faults);
    let plain = folder.path().join("plain.7z");
    let Made::Beside(temp) =
        write_new(&library, &plain, &plan, &options("7z"), &mut faults).unwrap()
    else {
        panic!("written beside")
    };
    let mut stub = b"MZ".to_vec();
    stub.resize(4096, 0x5A);
    let stubbed = folder.path().join("s.exe");
    std::fs::write(
        &stubbed,
        [stub.clone(), std::fs::read(&temp).unwrap()].concat(),
    )
    .unwrap();
    let base = opened(&library, &stubbed);
    assert_eq!(base.library().unwrap().0.parts[0].offset, 4096);
    let more = tempfile::tempdir().unwrap();
    std::fs::write(more.path().join("new.txt"), "new").unwrap();
    let sources = [more.path().join("new.txt")];
    let (src_dir, names) = under_folder(&sources, Some(more.path()));
    let plan = prepare(
        &base,
        "",
        (&src_dir, &names),
        Overwrite::Overwrite,
        &mut faults,
    )
    .unwrap();
    let changed = write(&base, &plan, &options("7z"), &mut faults).unwrap();
    drop(base);
    let bytes = std::fs::read(&changed).unwrap();
    assert!(bytes.starts_with(&stub), "the stub kept first");
    assert_eq!(&bytes[4096..4098], b"7z", "the archive right after it");
    std::fs::rename(&changed, &stubbed).unwrap();
    let mut read = opened(&library, &stubbed);
    assert_eq!(read.library().unwrap().0.parts[0].offset, 4096);
    assert_eq!(
        contents(&mut read),
        [
            ("new.txt".to_string(), b"new".to_vec()),
            ("top.txt".to_string(), b"top".to_vec())
        ]
    );
}

/// A format the library does not have: arclite's "Select archive format", nothing made.
#[test]
fn a_format_not_there_is_refused() {
    let folder = sources();
    let library = Library::find(folder.path());
    let mut asks = Asks::default();
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let plan = new_plan(folder.path(), &["top.txt"], &mut faults);
    let archive = folder.path().join("a.none");
    let refused = write_new(&library, &archive, &plan, &options("none"), &mut faults);
    let words = Fault::message(vec!["Select archive format".to_string()]);
    assert_eq!(refused, Err(Stop::Failed(words)));
    assert!(left_over(folder.path()).is_empty());
}

/// A stop while writing volumes deletes the volumes made; one while writing after a module deletes the
/// file with the module.
#[test]
fn a_stop_leaves_no_volume_and_no_module() {
    let folder = sources();
    let library = Library::find(folder.path());
    let mut asks = Asks {
        stop_after: Some(0),
        ..Asks::default()
    };
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let plan = new_plan(folder.path(), &["top.txt"], &mut faults);
    let mut split = options("7z");
    split.kind.volume_size = Some(1024);
    let archive = folder.path().join("v.7z");
    let stopped = write_new(&library, &archive, &plan, &split, &mut faults);
    assert_eq!(stopped, Err(Stop::Abort));
    assert!(left_over(folder.path()).is_empty(), "no volume left");
    let module = (library.sfx_modules().into_iter())
        .find(|path| path.ends_with("7z.sfx"))
        .expect("7-Zip's 7z.sfx installed");
    let mut sfx = options("7z");
    sfx.kind.sfx = Some(crate::sfx::Sfx {
        module,
        options: crate::sfx::SfxOptions::default(),
    });
    let archive = folder.path().join("s.7z.exe");
    let stopped = write_new(&library, &archive, &plan, &sfx, &mut faults);
    assert_eq!(stopped, Err(Stop::Abort));
    assert!(left_over(folder.path()).is_empty(), "no module left");
}

/// A new 7z of `options` in `folder`, as it reads back: whether an item is encrypted.
fn encrypted_7z(folder: &Path, options: &UpdateOptions) -> bool {
    let library = Library::find(folder);
    let mut asks = Asks::default();
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let plan = new_plan(folder, &["top.txt"], &mut faults);
    let archive = folder.join("e.7z");
    let Made::Beside(temp) = write_new(&library, &archive, &plan, options, &mut faults).unwrap()
    else {
        panic!("written beside")
    };
    std::fs::rename(&temp, &archive).unwrap();
    let read = opened(&library, &archive);
    let (chain, _) = read.library().unwrap();
    crate::update_props::load_update_props(chain.archive(), false).encrypted
}

/// The password goes to the library only when encrypting: its items encrypted then, not otherwise.
#[test]
fn the_password_is_given_only_when_encrypting() {
    let folder = sources();
    let mut encrypted = options("7z");
    encrypted.encrypt = true;
    encrypted.password = "pw".to_string();
    encrypted.kind.encrypt_header = Some(false);
    assert!(encrypted_7z(folder.path(), &encrypted), "encrypted");
    let plain = UpdateOptions {
        encrypt: false,
        ..encrypted
    };
    assert!(
        !encrypted_7z(folder.path(), &plain),
        "a password typed, no encryption"
    );
}

/// A new item's properties as arclite gives them: its path by `\`, its name, a folder's attribute
/// added, its size and times.
#[test]
fn a_new_items_properties_are_arclites() {
    let moment = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let ticks = crate::item::to_ticks(moment).unwrap().cast_unsigned();
    let folder = Added {
        path: vec!["docs".to_string(), "sub".to_string()],
        source: "docs".into(),
        is_dir: true,
        size: 0,
        modified: Some(moment),
        created: None,
        accessed: Some(moment),
        attributes: 0x01,
    };
    assert_eq!(
        added_property(&folder, prop::PATH),
        Prop::Text(r"docs\sub".into())
    );
    assert_eq!(
        added_property(&folder, prop::NAME),
        Prop::Text("sub".into())
    );
    assert_eq!(added_property(&folder, prop::IS_DIR), Prop::Bool(true));
    assert_eq!(added_property(&folder, prop::ATTRIBUTES), Prop::U32(0x11));
    assert_eq!(added_property(&folder, prop::MODIFIED), Prop::Time(ticks));
    assert_eq!(added_property(&folder, prop::ACCESSED), Prop::Time(ticks));
    assert_eq!(added_property(&folder, prop::CREATED), Prop::Empty);
    let file = Added {
        is_dir: false,
        size: 12,
        created: Some(moment),
        ..folder
    };
    assert_eq!(added_property(&file, prop::SIZE), Prop::U64(12));
    assert_eq!(added_property(&file, prop::IS_DIR), Prop::Bool(false));
    assert_eq!(added_property(&file, prop::ATTRIBUTES), Prop::U32(0x01));
    assert_eq!(added_property(&file, prop::CREATED), Prop::Time(ticks));
    assert_eq!(added_property(&file, prop::ENCRYPTED), Prop::Empty);
}
