use super::*;

/// `kMultiSignature`: each signature after its length; a length past the end gives none.
#[test]
fn several_signatures_are_split_by_their_lengths() {
    assert_eq!(
        split_signatures(&[2, b'P', b'K', 1, b'x']),
        [b"PK".to_vec(), b"x".to_vec()]
    );
    assert_eq!(split_signatures(&[]), Vec::<Vec<u8>>::new());
    assert_eq!(split_signatures(&[3, b'a']), Vec::<Vec<u8>>::new());
    assert_eq!(split_signatures(&[0, 1, b'y']), [Vec::new(), b"y".to_vec()]);
}

/// A list of extensions split at its spaces.
#[test]
fn words_are_split_at_spaces() {
    assert_eq!(words("tar  tgz"), ["tar", "tgz"]);
    assert!(words("").is_empty());
}

/// With no library in the plugin's folder, 7-Zip's installed one (the development machine has it):
/// its formats each once, in their class ids' order, with their names, extensions and signatures.
#[test]
fn the_installed_library_is_found_and_its_formats_read() {
    let empty = tempfile::tempdir().unwrap();
    let libraries = find(empty.path());
    assert!(!libraries.loaded.is_empty(), "7-Zip is installed here");
    let folder = libraries.folder.clone().unwrap();
    assert!(folder.join("7z.dll").is_file(), "{folder:?}");
    let named = |name: &str| {
        (libraries.formats.iter())
            .find(|found| found.format.name == name)
            .unwrap_or_else(|| panic!("no {name}"))
    };
    let seven = &named("7z").format;
    assert_eq!(seven.extensions, ["7z"]);
    assert!(seven.updatable);
    assert_eq!(seven.signatures, [b"7z\xBC\xAF\x27\x1C".to_vec()]);
    let gzip = &named("gzip").format;
    assert!(gzip.extensions.iter().any(|ext| ext == "tgz"));
    assert!(gzip.add_extensions.iter().any(|ext| ext == ".tar"));
    assert!(!named("Rar").format.updatable);
    let ids: Vec<&Vec<u8>> = libraries
        .formats
        .iter()
        .map(|found| &found.format.class_id)
        .collect();
    assert!(
        ids.windows(2).all(|pair| pair[0] < pair[1]),
        "sorted and once each"
    );
    assert!(
        libraries
            .formats
            .iter()
            .all(|found| found.format.library == 0)
    );
    assert_eq!(modules(&libraries)[0].path, folder.join("7z.dll"));
    assert!(modules(&libraries)[0].version >> 48 >= 9);
}

/// A library in the plugin's folder is taken before the installed one; a file there that is no 7-Zip
/// library is let go.
#[test]
fn the_plugin_folder_comes_first() {
    let installed = find(tempfile::tempdir().unwrap().path())
        .folder
        .clone()
        .unwrap();
    let own = tempfile::tempdir().unwrap();
    std::fs::copy(installed.join("7z.dll"), own.path().join("7z.dll")).unwrap();
    let system = std::env::var("SystemRoot").unwrap();
    let other = PathBuf::from(system).join("System32").join("version.dll");
    std::fs::copy(other, own.path().join("version.DLL")).unwrap();
    let libraries = find(own.path());
    assert_eq!(libraries.loaded.len(), 1);
    assert_eq!(libraries.loaded[0].path, own.path().join("7z.dll"));
    assert_eq!(libraries.folder.as_deref(), Some(own.path()));
    drop(libraries);
}

/// The SFX modules: the plugin's folder's programs among its `*.sfx` (a file not starting `MZ` passed
/// over), else those beside the installed library.
#[test]
fn sfx_modules_are_found_as_arclite_finds_them() {
    let own = tempfile::tempdir().unwrap();
    std::fs::write(own.path().join("mine.sfx"), b"MZ program").unwrap();
    std::fs::write(own.path().join("text.sfx"), b"not a program").unwrap();
    std::fs::write(own.path().join("short.sfx"), b"M").unwrap();
    let libraries = find(own.path());
    assert_eq!(libraries.sfx, [own.path().join("mine.sfx")]);
    let folder = libraries.folder.clone().unwrap();
    assert_ne!(folder, own.path(), "the library is the installed one");
    drop(libraries);
    let empty = tempfile::tempdir().unwrap();
    let libraries = find(empty.path());
    let installed = libraries.folder.clone().unwrap();
    assert_eq!(libraries.sfx, sfx_in(&installed));
    assert!(
        (libraries.sfx.iter()).any(|path| path.ends_with("7z.sfx")),
        "{:?}",
        libraries.sfx
    );
}

/// A library of the plugin's folder with no SFX module there: the modules beside the installed library;
/// with one there: that one only, the library still the folder's.
#[test]
fn a_plugin_folders_library_takes_the_installed_modules_when_it_has_none() {
    let installed = find(tempfile::tempdir().unwrap().path())
        .folder
        .clone()
        .unwrap();
    let own = tempfile::tempdir().unwrap();
    std::fs::copy(installed.join("7z.dll"), own.path().join("7z.dll")).unwrap();
    let libraries = find(own.path());
    assert_eq!(libraries.folder.as_deref(), Some(own.path()));
    assert!(!libraries.sfx.is_empty(), "the installed modules");
    assert_eq!(libraries.sfx, sfx_in(&installed));
    drop(libraries);
    std::fs::write(own.path().join("mine.sfx"), b"MZ program").unwrap();
    let libraries = find(own.path());
    assert_eq!(libraries.sfx, [own.path().join("mine.sfx")]);
    assert_eq!(libraries.loaded[0].path, own.path().join("7z.dll"));
}

/// Only files of the extension asked for, whatever its case; a folder of that name is passed over.
#[test]
fn files_of_an_extension_are_listed() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("a.FORMAT"), "").unwrap();
    std::fs::write(folder.path().join("b.dll"), "").unwrap();
    std::fs::create_dir(folder.path().join("c.format")).unwrap();
    assert_eq!(
        files_in(folder.path(), "format"),
        [folder.path().join("a.FORMAT")]
    );
    assert!(files_in(&folder.path().join("none"), "dll").is_empty());
}

/// A library before 9.31 (no `kFlags`): its flags from its single answers.
#[test]
fn old_flags_come_from_single_answers() {
    assert_eq!(old_flags(&|_| false), 0);
    let all = super::super::flags::KEEP_NAME
        | super::super::flags::ALT_STREAMS
        | super::super::flags::NT_SECURE;
    assert_eq!(old_flags(&|_| true), all);
    let only = |wanted: u32| move |id: u32| id == wanted;
    assert_eq!(old_flags(&only(handler::KEEP_NAME)), 1);
    assert_eq!(old_flags(&only(handler::ALT_STREAMS)), 2);
    assert_eq!(old_flags(&only(handler::NT_SECURE)), 4);
    assert_eq!(old_flags(&only(handler::UPDATE)), 0);
}

/// Two libraries giving the same formats: each format once, the newer library's; the first one's when
/// both are of one version.
#[test]
fn a_format_of_two_libraries_is_the_newer_ones() {
    let installed = find(tempfile::tempdir().unwrap().path())
        .folder
        .clone()
        .unwrap();
    let own = tempfile::tempdir().unwrap();
    let (first, second) = (own.path().join("first.dll"), own.path().join("second.dll"));
    std::fs::copy(installed.join("7z.dll"), &first).unwrap();
    std::fs::copy(installed.join("7z.dll"), &second).unwrap();
    let mut loaded = load_all(&[first, second]);
    assert_eq!(loaded.len(), 2);
    let one = formats(&loaded[..1]).len();
    let libraries = |loaded: &[Loaded]| {
        let found = formats(loaded);
        assert_eq!(found.len(), one, "each format once");
        let mut used: Vec<usize> = found.iter().map(|it| it.format.library).collect();
        used.dedup();
        used
    };
    assert_eq!(libraries(&loaded), [0], "one version: the first's");
    loaded[1].version += 1;
    assert_eq!(libraries(&loaded), [1], "the newer");
    loaded[0].version = loaded[1].version + 1;
    assert_eq!(libraries(&loaded), [0], "the newer");
}

/// A file that is no library is not loaded; the search path finds a system library.
#[test]
fn what_is_no_library_is_not_loaded() {
    let folder = tempfile::tempdir().unwrap();
    let fake = folder.path().join("fake.dll");
    std::fs::write(&fake, "not a library").unwrap();
    assert!(load(&fake).is_none());
    assert!(search_path("kernel32.dll").is_some());
    assert!(search_path("no-such-library-here.dll").is_none());
}
