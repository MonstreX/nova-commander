use nc_plugin_sdk::{Closed, ItemKind, Value};

use super::*;
use crate::create_options::{FORMAT, Profile, ProfileOptions};

fn start() -> NewArchive {
    NewArchive {
        path: "work".to_string(),
        append_ext: false,
        profile: ProfileOptions {
            format: FORMAT.to_string(),
            level: 7,
            method: "PPMD".to_string(),
            ..ProfileOptions::default()
        },
        levels: String::new(),
        show_password: false,
        open_shared: true,
    }
}

fn shown(profiles: Vec<Profile>) -> CreateDialog {
    CreateDialog {
        start: start(),
        profiles: Profiles(profiles),
        default_name: "work".to_string(),
        folder: PathBuf::from(r"E:\data"),
        levels: String::new(),
        archivers: Archivers::zip_only(),
        old_ext: ".zip".to_string(),
        modules: crate::sfx::Modules::default(),
        sfx: crate::sfx::SfxOptions::default(),
    }
}

fn closed(dialog: &Dialog, button: usize, changed: &[(usize, i32, &str)]) -> Closed {
    let mut values: Vec<Value> = (dialog.items.iter())
        .map(|item| Value {
            state: item.state,
            text: item.text.clone(),
        })
        .collect();
    for &(item, state, text) in changed {
        values[item].state = state;
        if !text.is_empty() {
            values[item].text = text.to_string();
        }
    }
    Closed {
        button: Some(button),
        values,
    }
}

/// arclite's `UpdateDialog::show` with `new_arc`: its rows, words and their order.
#[test]
fn the_dialog_is_arclites() {
    let dialog = shown(Vec::new()).dialog();
    assert_eq!(dialog.title, TITLE);
    let text = |item: usize| dialog.items[item].text.as_str();
    assert_eq!(text(PROFILE - 1), "&Profile:");
    assert_eq!(dialog.items[PROFILE].kind, ItemKind::List);
    assert_eq!(text(SAVE_PROFILE), "Save");
    assert_eq!(text(DELETE_PROFILE), "Delete");
    assert!(dialog.items[DELETE_PROFILE].disabled, "no profile chosen");
    assert_eq!(text(EVALUATE), "Eval&uate");
    assert_eq!(text(APPEND_EXT), "&Append extension");
    assert_eq!(text(ARC_PATH - 1), "Arc&hive path:");
    assert_eq!(text(ARC_PATH), "work.zip");
    assert_eq!(dialog.items[ARC_PATH].history, "arclite.arc_path");
}

/// The rows from the archiver on: zip chosen, the 7z items disabled, the volumes enabled.
#[test]
fn the_dialogs_lower_rows_are_arclites() {
    let dialog = shown(Vec::new()).dialog();
    let text = |item: usize| dialog.items[item].text.as_str();
    assert_eq!(text(ZIP), "&zip");
    assert_eq!(dialog.items[ZIP].state, 1);
    assert_eq!(dialog.items[LEVEL].state, 4, "Maximum");
    assert_eq!(dialog.items[METHOD].state, 2, "PPMD");
    assert!(dialog.items[SOLID].disabled, "7z's");
    assert_eq!(text(CREATE_SFX), "Create self-e&xtracting archive");
    assert!(dialog.items[CREATE_SFX].disabled && dialog.items[SFX_OPTIONS].disabled);
    assert_eq!(text(ENABLE_VOLUMES), "Split into &volumes");
    assert!(!dialog.items[ENABLE_VOLUMES].disabled, "a zip split too");
    assert_eq!(text(MOVE_FILES), "&Delete files after archiving");
    assert_eq!(
        [text(OK), text(CANCEL), text(SAVE)],
        ["OK", "Cancel", "Save settings"]
    );
}

/// The profile the options match is chosen; Delete enabled then.
#[test]
fn the_matching_profile_is_chosen() {
    let mine = Profile {
        name: "mine".to_string(),
        options: start().profile,
    };
    let other = Profile {
        name: "fast".to_string(),
        options: ProfileOptions {
            level: 1,
            ..start().profile
        },
    };
    let dialog = shown(vec![other, mine]).dialog();
    assert_eq!(dialog.items[PROFILE].state, 1);
    assert!(!dialog.items[DELETE_PROFILE].disabled);
}

/// OK: the answers as arclite's `read_controls` takes them, the path made whole.
#[test]
fn ok_takes_the_answers() {
    let shown = shown(Vec::new());
    let dialog = shown.dialog();
    let changed = [
        (ARC_PATH, 0, r"..\out\new"),
        (APPEND_EXT, 1, ""),
        (LEVEL, 0, ""),
        (MOVE_FILES, 1, ""),
    ];
    let (answers, path) = shown.answers(&closed(&dialog, OK, &changed)).unwrap();
    assert_eq!(answers.profile.level, 0);
    assert_eq!(answers.profile.method, "Copy");
    assert!(answers.profile.move_files && answers.append_ext);
    assert_eq!(path, PathBuf::from(r"E:\out\new.zip"), "made whole");
    assert!(shown.answers(&closed(&dialog, CANCEL, &[])).is_none());
    let no_format = [(ZIP, 0, "")];
    assert!(shown.answers(&closed(&dialog, OK, &no_format)).is_none());
}

/// arclite's `eval_arc_path`: quotes gone, a folder given the default name, the extension added when
/// asked and not there.
#[test]
fn the_path_is_made_whole_as_arclite_does() {
    let shown = shown(Vec::new());
    assert_eq!(
        shown.eval_path("\"a b.zip\"", true),
        PathBuf::from(r"E:\data\a b.zip")
    );
    assert_eq!(
        shown.eval_path("a.ZIP", true),
        PathBuf::from(r"E:\data\a.ZIP")
    );
    assert_eq!(shown.eval_path("a", false), PathBuf::from(r"E:\data\a"));
    assert_eq!(
        shown.eval_path("", false),
        PathBuf::from(r"E:\data\work.zip")
    );
    assert_eq!(
        shown.eval_path(r"sub\", false),
        PathBuf::from(r"E:\data\sub\work.zip")
    );
    assert_eq!(
        shown.eval_path(r"F:\x.zip", false),
        PathBuf::from(r"F:\x.zip")
    );
}

/// A dialog for a 7z with 7z.sfx and the small installer among the modules.
fn shown_7z(module: &str) -> CreateDialog {
    let mut shown = shown(Vec::new());
    shown.archivers.seven = Some(crate::create_formats::Archiver {
        name: "7z".to_string(),
        extension: ".7z".to_string(),
    });
    shown.start.profile.format = "7z".to_string();
    shown.start.profile.method = "LZMA".to_string();
    shown.old_ext = ".7z".to_string();
    shown.modules = crate::sfx::Modules(vec![
        PathBuf::from(r"C:\7\7z.sfx"),
        PathBuf::from(r"C:\7\7zS2.sfx"),
    ]);
    shown.sfx.name = module.to_string();
    shown
}

/// "Create self-extracting archive" for a 7z: the path given `.7z.exe`, the volumes not split, the
/// module and options in the answers; a module not there, or one without every codec for PPMD or
/// encryption, refused in arclite's words.
#[test]
fn a_self_extracting_7z_is_read_as_arclite_reads_it() {
    let shown = shown_7z("7z.sfx");
    let dialog = shown.dialog();
    let sfx = [
        (SEVEN, 1, ""),
        (CREATE_SFX, 1, ""),
        (ENABLE_VOLUMES, 1, ""),
        (VOLUME_SIZE, 0, "1M"),
        (ARC_PATH, 0, "work"),
        (APPEND_EXT, 1, ""),
    ];
    let (answers, path) = shown.answers(&closed(&dialog, OK, &sfx)).unwrap();
    assert!(answers.profile.create_sfx && !answers.profile.enable_volumes);
    assert_eq!(answers.profile.sfx.name, "7z.sfx");
    assert_eq!(path, PathBuf::from(r"E:\data\work.7z.exe"));
    assert_eq!(
        shown.eval_path("", false),
        PathBuf::from(r"E:\data\work.7z")
    );
    let made = answers.update_options(&shown.modules).kind.sfx.unwrap();
    assert_eq!(made.module, PathBuf::from(r"C:\7\7z.sfx"));
    let values: Vec<(i32, String)> = (closed(&dialog, OK, &sfx).values.iter())
        .map(|value| (value.state, value.text.clone()))
        .collect();
    let value = |item: usize| values[item].clone();
    let missing = shown_7z("7zSD.sfx");
    assert_eq!(missing.read(&value).err(), Some(crate::sfx::WRONG_MODULE));
    let small = shown_7z("7zS2.sfx");
    let ppmd = |item: usize| {
        if item == METHOD {
            (2, String::new())
        } else {
            value(item)
        }
    };
    assert_eq!(
        small.read(&ppmd).err(),
        Some("Selected SFX module does not support PPMD compression")
    );
    let encrypted = |item: usize| match item {
        ENCRYPT => (1, String::new()),
        PASSWORD | PASSWORD_VERIFY => (0, "pw".to_string()),
        _ => value(item),
    };
    assert_eq!(
        small.read(&encrypted).err(),
        Some("Selected SFX module does not support encryption")
    );
    assert!(small.read(&value).is_ok(), "LZMA, not encrypted");
    let zip = |item: usize| match item {
        SEVEN => (0, String::new()),
        ZIP => (1, String::new()),
        _ => value(item),
    };
    let read = missing.read(&zip).unwrap();
    assert!(
        !read.profile.create_sfx && read.profile.enable_volumes,
        "a 7z's only"
    );
}

/// The extension: `.exe` after a 7z's made self-extracting, else `.001` for volumes.
#[test]
fn the_default_extension_follows_sfx_and_volumes() {
    let shown = shown_7z("7z.sfx");
    assert_eq!(shown.default_ext("7z", (true, true)), ".7z.exe");
    assert_eq!(shown.default_ext("7z", (false, true)), ".7z.001");
    assert_eq!(shown.default_ext("zip", (true, true)), ".zip.001");
    assert_eq!(shown.default_ext("zip", (true, false)), ".zip");
}

/// The answers of `shown` read with a 7z at Normal, LZMA, nothing encrypted, then `changed` over them.
fn read_7z(
    shown: &CreateDialog,
    changed: &[(usize, i32, &str)],
) -> Result<NewArchive, &'static str> {
    let dialog = shown.dialog();
    let mut all = vec![
        (SEVEN, 1, ""),
        (ZIP, 0, ""),
        (OTHER, 0, ""),
        (LEVEL, 3, ""),
        (METHOD, 0, ""),
        (ENCRYPT, 0, ""),
        (SHOW_PASSWORD, 0, ""),
        (CREATE_SFX, 0, ""),
        (ENABLE_VOLUMES, 0, ""),
    ];
    all.extend_from_slice(changed);
    let values = closed(&dialog, OK, &all).values;
    shown.read(&|item| (values[item].state, values[item].text.clone()))
}

/// "Split into volumes": a size under arclite's least (16 KB) refused, the least taken; not split, any
/// size is let be.
#[test]
fn a_volumes_size_is_checked() {
    let shown = shown_7z("7z.sfx");
    let split = |size: &'static str| [(ENABLE_VOLUMES, 1, ""), (VOLUME_SIZE, 0, size)];
    assert_eq!(
        read_7z(&shown, &split("15K")).err(),
        Some("Invalid volume size")
    );
    let read = read_7z(&shown, &split("16K")).unwrap();
    assert!(read.profile.enable_volumes);
    assert_eq!(read.profile.volume_size, "16K");
    let unsplit = read_7z(&shown, &[(VOLUME_SIZE, 0, "1")]).unwrap();
    assert!(!unsplit.profile.enable_volumes);
}

/// A 7z encrypted: "Encrypt header"'s three states; not encrypted, or a zip, the profile's.
#[test]
fn the_headers_encryption_is_a_7zs() {
    let mut shown = shown_7z("7z.sfx");
    shown.start.profile.encrypt_header = Some(true);
    let encrypted = |header: i32| {
        [
            (ENCRYPT, 1, ""),
            (PASSWORD, 0, "pw"),
            (PASSWORD_VERIFY, 0, "pw"),
            (ENCRYPT_HEADER, header, ""),
        ]
    };
    let header = |changed: &[(usize, i32, &str)]| read_7z(&shown, changed).unwrap().profile;
    assert_eq!(header(&encrypted(0)).encrypt_header, Some(false));
    assert_eq!(header(&encrypted(1)).encrypt_header, Some(true));
    assert_eq!(header(&encrypted(2)).encrypt_header, None);
    let read = header(&encrypted(0));
    assert!(read.encrypt && read.password == "pw");
    let plain = [(ENCRYPT, 0, ""), (ENCRYPT_HEADER, 0, "")];
    assert_eq!(header(&plain).encrypt_header, Some(true), "the profile's");
    let mut zip = encrypted(0).to_vec();
    zip.extend([(SEVEN, 0, ""), (ZIP, 1, "")]);
    let read = header(&zip);
    assert!(read.encrypt, "a zip encrypted");
    assert_eq!(read.encrypt_header, Some(true), "the profile's");
}

/// The password: the two hidden lines must match; shown, its one line taken.
#[test]
fn the_password_is_read_from_its_lines() {
    let shown = shown_7z("7z.sfx");
    let typed = [
        (ENCRYPT, 1, ""),
        (PASSWORD, 0, "one"),
        (PASSWORD_VERIFY, 0, "two"),
        (PASSWORD_VISIBLE, 0, "seen"),
    ];
    assert_eq!(read_7z(&shown, &typed).err(), Some("Passwords don't match"));
    let mut visible = typed.to_vec();
    visible.push((SHOW_PASSWORD, 1, ""));
    let read = read_7z(&shown, &visible).unwrap();
    assert_eq!(read.profile.password, "seen");
    assert!(read.show_password);
}

/// The level and method by their lists' places: one past the lists refused; Store gives Copy; solid read
/// for a 7z not stored, else the profile's.
#[test]
fn the_level_method_and_solid_are_read() {
    let mut shown = shown_7z("7z.sfx");
    shown.start.profile.solid = true;
    let level = |state: i32| read_7z(&shown, &[(LEVEL, state, "")]);
    assert_eq!(level(-1).err(), Some("Select compression level"));
    assert_eq!(level(6).err(), Some("Select compression level"));
    assert_eq!(level(5).unwrap().profile.level, 9);
    let method = |state: i32| read_7z(&shown, &[(METHOD, state, "")]);
    assert_eq!(method(1).unwrap().profile.method, "LZMA2");
    assert_eq!(method(5).err(), Some("Select compression method"));
    let solid = |level: i32, state: i32| {
        let read = read_7z(&shown, &[(LEVEL, level, ""), (SOLID, state, "")]);
        read.unwrap().profile.solid
    };
    assert!(!solid(3, 0) && solid(3, 1));
    assert!(solid(0, 0), "stored: the profile's");
    let stored = level(0).unwrap().profile;
    assert_eq!((stored.level, stored.method.as_str()), (0, "Copy"));
}

/// Another format by the list's place: its level fitted, the profile's method and solid, never
/// encrypted, never self-extracting; none chosen, or past the list, refused.
#[test]
fn another_format_is_read_from_its_list() {
    let mut shown = shown_7z("7z.sfx");
    shown.start.profile.solid = false;
    shown.archivers.others = ["tar", "gzip"]
        .map(|name| crate::create_formats::Archiver {
            name: name.to_string(),
            extension: format!(".{name}"),
        })
        .to_vec();
    let other = |at: i32| {
        vec![
            (SEVEN, 0, ""),
            (OTHER, 1, ""),
            (OTHER_LIST, at, ""),
            (ENCRYPT, 1, ""),
            (CREATE_SFX, 1, ""),
            (SOLID, 1, ""),
        ]
    };
    let gzip = read_7z(&shown, &other(1)).unwrap().profile;
    assert_eq!(gzip.format, "gzip");
    assert_eq!((gzip.level, gzip.method.as_str()), (5, "LZMA"));
    assert!(!gzip.encrypt && !gzip.create_sfx && !gzip.solid);
    let tar = read_7z(&shown, &other(0)).unwrap().profile;
    assert_eq!(
        (tar.level, tar.method.as_str()),
        (0, "Copy"),
        "a tar stored"
    );
    assert_eq!(
        read_7z(&shown, &other(2)).err(),
        Some("Select archive format")
    );
    let mut unchosen = other(1);
    unchosen.push((OTHER, 0, ""));
    assert_eq!(
        read_7z(&shown, &unchosen).err(),
        Some("Select archive format")
    );
}

/// The check boxes and texts taken as they are.
#[test]
fn the_other_answers_are_taken() {
    let shown = shown_7z("7z.sfx");
    let changed = [
        (ADVANCED, 0, "mt4"),
        (MOVE_FILES, 1, ""),
        (IGNORE_ERRORS, 1, ""),
        (OPEN_SHARED, 0, ""),
        (APPEND_EXT, 1, ""),
        (ARC_PATH, 0, "out"),
    ];
    let read = read_7z(&shown, &changed).unwrap();
    assert_eq!(read.profile.advanced, "mt4");
    assert!(read.profile.move_files && read.profile.ignore_errors);
    assert!(!read.open_shared && read.append_ext);
    assert_eq!(read.path, "out");
    let flipped = [
        (MOVE_FILES, 0, ""),
        (IGNORE_ERRORS, 0, ""),
        (OPEN_SHARED, 1, ""),
    ];
    let read = read_7z(&shown, &flipped).unwrap();
    assert!(!read.profile.move_files && !read.profile.ignore_errors && read.open_shared);
}

/// arclite's `parse_size_string`: K, M, G, T of 1024.
#[test]
fn sizes_are_read_as_arclite_reads_them() {
    assert_eq!(parse_size("100"), 100);
    assert_eq!(parse_size("16K"), 16 * 1024);
    assert_eq!(parse_size("2 m"), 2 << 20);
    assert_eq!(parse_size("1G"), 1 << 30);
    assert_eq!(parse_size("x"), 0);
}
