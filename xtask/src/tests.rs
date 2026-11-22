//! The packaging task on a made-up workspace and build (no cargo run): what is laid out, what goes, what
//! stays, and what stops it.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// A folder of its own under the system's temporary folder, removed with this.
struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            "xtask-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn library(name: &str) -> String {
    format!(
        "{}{name}{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    )
}

fn exe() -> String {
    format!("nova-commander{}", std::env::consts::EXE_SUFFIX)
}

const REAL: &str = r#"{"name": "real", "version": "1", "api": 2, "library": "real_lib"}"#;
const PROBE: &str =
    r#"{"name": "probe", "version": "1", "api": 2, "library": "probe", "test": true}"#;

/// A workspace with a plugin `real` and the test plugin `probe`, both built; and a folder packaged
/// before: a file left in `real`, the probe, and a plugin from elsewhere.
fn workspace() -> (Temp, PathBuf, PathBuf, PathBuf) {
    let temp = Temp::new();
    let root = temp.0.join("root");
    let built = temp.0.join("built");
    let folder = temp.0.join("portable");
    write(&root.join("plugins").join("real").join("plugin.json"), REAL);
    write(
        &root.join("plugins").join("probe").join("plugin.json"),
        PROBE,
    );
    write(&root.join("plugins").join("notes.txt"), "not a plugin");
    write(&root.join("LICENSE"), "the licence");
    write(&root.join("THIRD-PARTY-NOTICES.md"), "the notices");
    let schemes = root.join("assets").join("colorer").join("hrc");
    write(&schemes.join("catalog.xml"), "the catalog");
    write(&schemes.join("base").join("c.hrc"), "c's scheme");
    write(&folder.join("hrc").join("base").join("c.hrc"), "an old c");
    write(&folder.join("hrc").join("mine.hrc"), "the user's");
    std::fs::create_dir_all(root.join("plugins").join("unfinished")).unwrap();
    write(&built.join(exe()), "the program");
    write(&built.join(library("real_lib")), "real's code");
    write(&built.join(library("probe")), "probe's code");
    let packaged = folder.join("plugins");
    write(&packaged.join("real").join("stale.txt"), "old");
    write(&packaged.join("probe").join("plugin.json"), PROBE);
    write(&packaged.join("foreign").join("keep.txt"), "theirs");
    (temp, root, built, folder)
}

#[test]
fn the_program_and_its_plugins_are_laid_out_without_the_test_ones() {
    let (_temp, root, built, folder) = workspace();
    package(&root, &built, &folder, false).unwrap();
    assert_eq!(read(&folder.join(exe())), "the program");
    assert_eq!(
        read(&folder.join("LICENSE")),
        "the licence",
        "MIT asks it to go along"
    );
    assert_eq!(
        read(&folder.join("THIRD-PARTY-NOTICES.md")),
        "the notices",
        "Far Manager's BSD licence asks it"
    );
    let plugins = folder.join("plugins");
    assert_eq!(read(&plugins.join("real").join("plugin.json")), REAL);
    assert_eq!(
        read(&plugins.join("real").join(library("real_lib"))),
        "real's code",
        "the manifest's library, by its name"
    );
    assert!(
        !plugins.join("real").join("stale.txt").exists(),
        "laid out anew"
    );
    assert!(
        !plugins.join("probe").exists(),
        "an earlier test plugin goes"
    );
    assert_eq!(read(&plugins.join("foreign").join("keep.txt")), "theirs");
    assert!(
        !plugins.join("unfinished").exists(),
        "no manifest, no plugin"
    );
    assert!(!plugins.join("notes.txt").exists());
    let schemes = folder.join("hrc");
    assert_eq!(
        read(&schemes.join("catalog.xml")),
        "the catalog",
        "Colorer's schemes beside it"
    );
    assert_eq!(
        read(&schemes.join("base").join("c.hrc")),
        "c's scheme",
        "written over"
    );
    assert_eq!(
        read(&schemes.join("mine.hrc")),
        "the user's",
        "the user's own kept"
    );
}

#[test]
fn the_test_plugins_come_when_asked() {
    let (_temp, root, built, folder) = workspace();
    package(&root, &built, &folder, true).unwrap();
    let probe = folder.join("plugins").join("probe");
    assert_eq!(read(&probe.join("plugin.json")), PROBE);
    assert_eq!(read(&probe.join(library("probe"))), "probe's code");
    assert_eq!(
        read(
            &folder
                .join("plugins")
                .join("real")
                .join(library("real_lib"))
        ),
        "real's code"
    );
}

#[test]
fn the_builds_own_folder_gets_its_plugins_and_keeps_its_program() {
    let (_temp, root, built, _folder) = workspace();
    write(
        &built.join("plugins").join("real").join("plugin.json"),
        r#"{"name": "real", "api": 1, "library": "real_lib"}"#,
    );
    package(&root, &built, &built, false).unwrap();
    assert_eq!(
        read(&built.join(exe())),
        "the program",
        "not copied onto itself"
    );
    let real = built.join("plugins").join("real");
    assert_eq!(
        read(&real.join("plugin.json")),
        REAL,
        "an old manifest replaced"
    );
    assert_eq!(read(&real.join(library("real_lib"))), "real's code");
}

#[test]
fn a_new_folder_is_made() {
    let (_temp, root, built, folder) = workspace();
    let fresh = folder.join("deeper").join("still");
    package(&root, &built, &fresh, false).unwrap();
    assert!(fresh.join(exe()).is_file());
    assert!(
        fresh
            .join("plugins")
            .join("real")
            .join("plugin.json")
            .is_file()
    );
}

#[test]
fn what_is_not_built_stops_it_saying_which() {
    let (_temp, root, built, folder) = workspace();
    std::fs::remove_file(built.join(library("real_lib"))).unwrap();
    let error = package(&root, &built, &folder, false).unwrap_err();
    assert!(error.contains(&library("real_lib")), "{error}");
    std::fs::remove_file(built.join(exe())).unwrap();
    let error = package(&root, &built, &folder, false).unwrap_err();
    assert!(error.contains(&exe()), "{error}");
}

#[test]
fn a_library_not_named_plainly_stops_it() {
    for manifest in [
        r#"{"name": "real", "library": "..\\evil"}"#,
        r#"{"name": "real", "library": "a/b"}"#,
        r#"{"name": "real", "library": ""}"#,
        r#"{"name": "real", "library": 5}"#,
        r#"{"name": "real"}"#,
    ] {
        let (temp, root, built, folder) = workspace();
        write(
            &root.join("plugins").join("real").join("plugin.json"),
            manifest,
        );
        let error = package(&root, &built, &folder, false).unwrap_err();
        assert!(
            error.ends_with("no library named plainly"),
            "{manifest}: {error}"
        );
        assert!(
            !temp.0.join("evil").exists() && !folder.join("evil").exists(),
            "nothing outside"
        );
    }
}

#[test]
fn a_broken_manifest_stops_it() {
    let (_temp, root, built, folder) = workspace();
    write(&root.join("plugins").join("real").join("plugin.json"), "{");
    let error = package(&root, &built, &folder, false).unwrap_err();
    assert!(error.contains("plugin.json"), "{error}");
}

#[test]
fn a_task_is_named_with_its_folder() {
    let args = |args: &[&str]| run(&args.iter().map(ToString::to_string).collect::<Vec<_>>());
    let usage = "usage: package <folder> [--release] [--test-plugins]";
    assert_eq!(args(&[]).unwrap_err(), usage);
    assert_eq!(args(&["package"]).unwrap_err(), usage);
    assert_eq!(
        args(&["build", "out"]).unwrap_err(),
        r#"unknown task "build""#
    );
}
