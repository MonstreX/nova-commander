use super::*;

fn zip_profile() -> ProfileOptions {
    ProfileOptions {
        format: FORMAT.to_string(),
        method: "Deflate".to_string(),
        ..ProfileOptions::default()
    }
}

/// arclite's `operator==`: the method only when compressing, the password only when encrypting, the
/// volumes' size only when split; the format, level, move and errors always.
#[test]
fn profiles_are_compared_as_arclite_does() {
    let one = zip_profile();
    let same = |change: &dyn Fn(&mut ProfileOptions)| {
        let mut other = one.clone();
        change(&mut other);
        one.same(&other)
    };
    assert!(same(&|_| {}));
    assert!(!same(&|other| other.level = 9));
    assert!(!same(&|other| other.method = "BZip2".to_string()));
    assert!(!same(&|other| other.format = "7z".to_string()));
    assert!(
        same(&|other| other.password = "x".to_string()),
        "not encrypting"
    );
    assert!(
        same(&|other| other.volume_size = "10M".to_string()),
        "not split"
    );
    assert!(!same(&|other| other.move_files = true));
    assert!(!same(&|other| other.ignore_errors = true));
    assert!(!same(&|other| other.advanced = "em=AES128".to_string()));
    assert!(same(&|other| other.encrypt_header = Some(true)), "a zip's");
    let stored = ProfileOptions {
        level: 0,
        ..zip_profile()
    };
    let other_method = ProfileOptions {
        method: "PPMD".to_string(),
        ..stored.clone()
    };
    assert!(stored.same(&other_method), "storing: no method");
    let locked = ProfileOptions {
        encrypt: true,
        password: "a".to_string(),
        ..zip_profile()
    };
    let other_password = ProfileOptions {
        password: "b".to_string(),
        ..locked.clone()
    };
    assert!(!locked.same(&other_password));
    let split = ProfileOptions {
        enable_volumes: true,
        volume_size: "1M".to_string(),
        ..zip_profile()
    };
    let other_size = ProfileOptions {
        volume_size: "2M".to_string(),
        ..split.clone()
    };
    assert!(!split.same(&other_size));
}

/// A self-extracting 7z: its SFX options compared (not when it is not one), the volumes not; its module
/// found by name for the writing (none for a zip, or a module not there).
#[test]
fn a_self_extracting_7z_is_compared_with_its_options() {
    let sfx = ProfileOptions {
        format: "7z".to_string(),
        create_sfx: true,
        sfx: crate::sfx::SfxOptions {
            name: "7z.sfx".to_string(),
            ..crate::sfx::SfxOptions::default()
        },
        ..ProfileOptions::default()
    };
    let mut other = sfx.clone();
    other.sfx.name = "7zCon.sfx".to_string();
    assert!(!sfx.same(&other));
    let plain = ProfileOptions {
        create_sfx: false,
        ..sfx.clone()
    };
    let plain_other = ProfileOptions {
        create_sfx: false,
        ..other.clone()
    };
    assert!(
        plain.same(&plain_other),
        "not an SFX: its options not compared"
    );
    let split = ProfileOptions {
        enable_volumes: true,
        ..sfx.clone()
    };
    assert!(sfx.same(&split), "an SFX is not split");
    let modules = crate::sfx::Modules(vec![std::path::PathBuf::from(r"C:\7\7Z.SFX")]);
    let made = sfx.sfx_of(&modules).unwrap();
    assert_eq!(made.module, std::path::PathBuf::from(r"C:\7\7Z.SFX"));
    assert_eq!(made.options, sfx.sfx);
    assert!(other.sfx_of(&modules).is_none(), "its module not there");
    assert!(plain.sfx_of(&modules).is_none());
    let zip = ProfileOptions {
        format: FORMAT.to_string(),
        ..sfx.clone()
    };
    assert!(zip.sfx_of(&modules).is_none(), "a 7z's only");
}

fn named(name: &str, level: u32) -> Profile {
    Profile {
        name: name.to_string(),
        options: ProfileOptions {
            level,
            ..zip_profile()
        },
    }
}

/// arclite's `UpdateProfiles::update`, its lists sorted by name whatever the case.
#[test]
fn a_profile_is_updated_as_arclite_does() {
    let mut profiles = Profiles(vec![named("b", 1), named("d", 3)]);
    profiles.update("A", &named("", 7).options);
    let names = |profiles: &Profiles| -> Vec<String> {
        profiles
            .0
            .iter()
            .map(|profile| profile.name.clone())
            .collect()
    };
    assert_eq!(names(&profiles), ["A", "b", "d"]);
    // The options of "d" under the name "b": "b" takes them, "d" goes.
    profiles.update("B", &named("", 3).options);
    assert_eq!(names(&profiles), ["A", "B"]);
    assert_eq!(profiles.0[1].options.level, 3);
    // A new name for options there: that profile renamed.
    profiles.update("c", &named("", 7).options);
    assert_eq!(names(&profiles), ["B", "c"]);
    assert_eq!(profiles.by_name("C"), Some(1));
    assert_eq!(profiles.by_options(&named("", 3).options), Some(0));
    assert_eq!(profiles.by_options(&named("", 9).options), None);
}

/// arclite's `arc_levels`: the format's level first, its old one gone; read back by the format.
#[test]
fn each_format_keeps_its_level() {
    assert_eq!(with_level("", "zip", 5), "zip=5");
    assert_eq!(with_level("7z=9;zip=3", "zip", 1), "zip=1;7z=9");
    assert_eq!(with_level("zip=3;7z=9", "zip", 7), "zip=7;7z=9");
    assert_eq!(level_in("7z=9;zip=3", "zip"), Some(3));
    assert_eq!(level_in("7z=9", "zip"), None);
    assert_eq!(level_in("zip=", "zip"), Some(0));
}

/// arclite's name: the one source's, else its folder's; the first source's at a disk's root.
#[test]
fn the_archive_is_named_as_arclite_names_it() {
    let sources = |names: &[&str]| {
        names
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(default_name(r"E:\work", &sources(&["a.txt"])), "a.txt");
    assert_eq!(default_name(r"E:\work", &sources(&["a", "b"])), "work");
    assert_eq!(default_name(r"E:\work\", &sources(&["a", "b"])), "work");
    assert_eq!(default_name(r"E:\", &sources(&["a", "b"])), "a");
    assert_eq!(default_name("E:", &sources(&["a", "b"])), "a");
}

fn settings() -> (UpdateSettings, NewSettings) {
    let update = UpdateSettings {
        level: 7,
        method: "ppmd".to_string(),
        advanced: "em=AES128".to_string(),
        encrypt: true,
        show_password: true,
        move_files: true,
        ignore_errors: true,
        overwrite: Overwrite::Skip,
    };
    let new = NewSettings {
        password: "kept".to_string(),
        create_sfx: true,
        enable_volumes: true,
        volume_size: "5M".to_string(),
        append_ext: true,
        levels: "zip=7".to_string(),
        ..NewSettings::default()
    };
    (update, new)
}

/// The menu's `create_archive` neither encrypts, moves, splits nor takes the advanced parameters.
#[test]
fn the_menus_options_start_as_arclites() {
    let (update, new) = settings();
    let menu = NewArchive::starting(
        ("a".to_string(), FORMAT),
        (&update, &new),
        Asked::Menu,
        true,
    );
    assert_eq!(menu.profile.format, FORMAT);
    assert_eq!(menu.profile.level, 7);
    assert_eq!(menu.profile.method, "PPMD");
    assert!(!menu.profile.encrypt);
    assert!(menu.profile.password.is_empty());
    assert!(!menu.profile.move_files && !menu.profile.create_sfx && !menu.profile.enable_volumes);
    assert!(menu.profile.advanced.is_empty());
    assert!(menu.profile.ignore_errors && menu.append_ext && menu.show_password);
    assert_eq!(menu.levels, "zip=7");
}

/// `put_files` takes all that is kept (the volumes too); a method arclite does not offer starts as
/// Deflate.
#[test]
fn the_puts_options_start_as_arclites() {
    let (update, new) = settings();
    let put = NewArchive::starting(
        ("a".to_string(), FORMAT),
        (&update, &new),
        Asked::Put,
        false,
    );
    assert!(put.profile.encrypt);
    assert_eq!(put.profile.password, "kept");
    assert!(put.profile.move_files && put.profile.create_sfx);
    assert!(put.profile.enable_volumes, "a zip split too");
    assert_eq!(put.profile.advanced, "em=AES128");
    assert!(!put.open_shared);
    let unknown = UpdateSettings {
        method: "LZMA".to_string(),
        ..update
    };
    let deflate = NewArchive::starting(
        ("a".to_string(), FORMAT),
        (&unknown, &new),
        Asked::Menu,
        true,
    );
    assert_eq!(deflate.profile.method, "Deflate");
    // Another format: arclite's method kept (LZMA for none), the volumes kept when put.
    let seven = NewArchive::starting(("a".to_string(), "7z"), (&unknown, &new), Asked::Put, true);
    assert_eq!(seven.profile.method, "LZMA");
    assert_eq!(seven.profile.format, "7z");
    let split = NewSettings {
        enable_volumes: true,
        volume_size: "1M".to_string(),
        ..new.clone()
    };
    let put = NewArchive::starting(
        ("a".to_string(), "7z"),
        (&unknown, &split),
        Asked::Put,
        true,
    );
    assert!(put.profile.enable_volumes);
    assert_eq!(
        put.update_options(&crate::sfx::Modules::default())
            .kind
            .volume_size,
        Some(1 << 20)
    );
    let menu = NewArchive::starting(
        ("a".to_string(), "7z"),
        (&unknown, &split),
        Asked::Menu,
        true,
    );
    assert!(!menu.profile.enable_volumes, "the menu's splits none");
    assert_eq!(
        menu.update_options(&crate::sfx::Modules::default())
            .kind
            .format,
        "7z"
    );
}

/// Every file new: overwritten, the password only when encrypting.
#[test]
fn the_writing_takes_the_profile() {
    let (update, new) = settings();
    let put = NewArchive::starting(("a".to_string(), FORMAT), (&update, &new), Asked::Put, true);
    let options = put.update_options(&crate::sfx::Modules::default());
    assert_eq!(options.overwrite, Overwrite::Overwrite);
    assert_eq!(options.password, "kept");
    assert!(options.encrypt && options.move_files && options.open_shared);
    let mut plain = put.clone();
    plain.profile.encrypt = false;
    assert!(
        plain
            .update_options(&crate::sfx::Modules::default())
            .password
            .is_empty()
    );
}
