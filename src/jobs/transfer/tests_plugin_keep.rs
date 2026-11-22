//! What a copy into or out of a plugin's panel keeps (P6.0d): the times and the hidden, system and
//! read-only marks through the plugin's `set_times` (FAR's `PutFiles` items), a folder's once what went
//! into it is there; a plugin that renames nothing overwritten in place. The engine against the probe.
#![cfg(windows)]

use std::fs;
use std::os::windows::fs::MetadataExt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::tests::Script;
use super::tests_plugin::{log, probe, read};
use super::tests_vfs::{clean, copy_into, over};
use super::*;
use crate::platform::files::{self, Attributes, FileTimes};
use crate::vfs::{Entry, provider_for};

const HIDDEN: u32 = 0x2;
const SYSTEM: u32 = 0x4;
const ARCHIVE: u32 = 0x20;

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn stat(path: &VfsPath) -> Entry {
    provider_for(path).unwrap().stat(path).unwrap()
}

/// A file and a folder on a disk with old times, both hidden and system.
fn marked_sources(disk: &std::path::Path) -> (VfsPath, VfsPath) {
    let file = disk.join("old.txt");
    fs::write(&file, "old").unwrap();
    let folder = disk.join("marked");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("in.txt"), "in").unwrap();
    let times = FileTimes {
        created: Some(at(1_000_000_000)),
        modified: Some(at(1_200_000_000)),
        ..FileTimes::default()
    };
    for path in [&file, &folder] {
        files::apply_kept(path, None, Some(&times)).times.unwrap();
        let marks = Attributes::from_windows_bits(HIDDEN | SYSTEM | ARCHIVE);
        files::set_attributes(path, marks).unwrap();
    }
    (VfsPath::local(file), VfsPath::local(folder))
}

#[test]
fn a_file_and_a_folder_put_into_a_plugins_panel_keep_their_times_and_marks() {
    let (temp, _host) = probe("keepone");
    let disk = tempfile::tempdir().unwrap();
    let (file, folder) = marked_sources(disk.path());
    let panel = VfsPath::in_session("keepone", "one", "/");
    let mut request = copy_into(&[&file, &folder], &panel);
    request.keep.timestamps = true;
    clean(&run(&request, &mut Script::default()));
    for name in ["old.txt", "marked"] {
        let entry = stat(&panel.join(name));
        assert_eq!(entry.modified, Some(at(1_200_000_000)), "{name}");
        assert_eq!(entry.created, Some(at(1_000_000_000)), "{name}");
        let marks = entry.attributes;
        assert!(marks.is_hidden() && marks.is_system(), "{name}");
        assert!(!marks.is_read_only(), "{name}");
    }
    let told = log(&temp, "keepone");
    let folder_kept = told.find("kept times one:/marked\n").unwrap();
    let inner_made = told.find("made file one:/marked/in.txt").unwrap();
    assert!(inner_made < folder_kept, "the folder's last: {told}");
}

#[test]
fn without_all_timestamps_a_file_keeps_its_last_write_time_and_its_marks() {
    let (temp, _host) = probe("keeptwo");
    let disk = tempfile::tempdir().unwrap();
    let (file, folder) = marked_sources(disk.path());
    fs::create_dir(disk.path().join("bare")).unwrap();
    let bare = VfsPath::local(disk.path().join("bare"));
    let panel = VfsPath::in_session("keeptwo", "one", "/");
    clean(&run(
        &copy_into(&[&file, &folder, &bare], &panel),
        &mut Script::default(),
    ));
    let entry = stat(&panel.join("old.txt"));
    assert_eq!(entry.modified, Some(at(1_200_000_000)));
    assert_eq!(entry.created, None, "not asked for");
    assert!(entry.attributes.is_hidden());
    let marked = stat(&panel.join("marked"));
    assert!(marked.attributes.is_system() && marked.modified.is_none());
    // A folder with nothing to keep: the plugin is not asked.
    assert!(!log(&temp, "keeptwo").contains("kept times one:/bare"));
}

#[test]
fn a_file_copied_out_of_a_plugins_panel_keeps_its_marks_on_the_disk() {
    let (_temp, _host) = probe("keepthree");
    let panel = VfsPath::in_session("keepthree", "one", "/");
    let out = tempfile::tempdir().unwrap();
    let request = copy_into(&[&panel.join(".hidden")], &VfsPath::local(out.path()));
    clean(&run(&request, &mut Script::default()));
    let bits = fs::metadata(out.path().join(".hidden"))
        .unwrap()
        .file_attributes();
    assert_ne!(bits & HIDDEN, 0, "{bits:#x}");
}

#[test]
fn a_plugin_that_renames_nothing_is_overwritten_in_place_after_its_first_refusal() {
    let (temp, _host) = probe("keepfour");
    let disk = tempfile::tempdir().unwrap();
    fs::write(disk.path().join("readme.txt"), "one").unwrap();
    fs::write(disk.path().join(".hidden"), "two").unwrap();
    let sources = [
        VfsPath::local(disk.path().join("readme.txt")),
        VfsPath::local(disk.path().join(".hidden")),
    ];
    let panel = VfsPath::in_session("keepfour", "norename", "/");
    let request = over(
        &[&sources[0], &sources[1]],
        &panel,
        false,
        Existing::Overwrite,
    );
    clean(&run(&request, &mut Script::default()));
    assert_eq!(read(&panel.join("readme.txt")), b"one");
    assert_eq!(read(&panel.join(".hidden")), b"two");
    let mut names = Vec::new();
    let stop = std::sync::atomic::AtomicBool::new(false);
    (provider_for(&panel).unwrap())
        .list(&panel, &stop, &mut |entry| names.push(entry.name))
        .unwrap();
    assert_eq!(names.len(), 4, "no temporary copy left: {names:?}");
    let told = log(&temp, "keepfour");
    // The first under a temporary name, refused, then each in place.
    assert_eq!(told.matches("made file").count(), 3, "{told}");
    assert_eq!(told.matches("removed norename:").count(), 3, "{told}");
}

#[test]
fn within_a_panel_that_renames_nothing_each_file_replaces_its_target_through_a_whole_copy() {
    let (temp, _host) = probe("keepfive");
    let panel = VfsPath::in_session("keepfive", "norename", "/");
    let docs = panel.join("docs");
    let sources = [panel.join("readme.txt"), panel.join(".hidden")];
    let both = [&sources[0], &sources[1]];
    clean(&run(&copy_into(&both, &docs), &mut Script::default()));
    let before = log(&temp, "keepfive").matches("made file").count();
    clean(&run(
        &over(&both, &docs, false, Existing::Overwrite),
        &mut Script::default(),
    ));
    assert_eq!(read(&docs.join("readme.txt")), b"Hello probe!");
    let told = log(&temp, "keepfive");
    // A source in the same panel might be its target under another name: never removed before a
    // whole copy of it is there (each under a temporary name, then written in its place).
    assert_eq!(told.matches("made file").count() - before, 4, "{told}");
    // Its end told (API 9) is no making nor removing: counted on its own.
    let ended = (told.lines())
        .filter(|line| line.contains("ended file") && line.contains(".nc-tmp"))
        .count();
    assert_eq!(
        told.matches(".nc-tmp").count() - ended,
        4,
        "made and removed twice: {told}"
    );
    assert_eq!(ended, 2, "each temporary file's end told: {told}");
    assert!(
        read(&panel.join("readme.txt")) == b"Hello probe!",
        "the sources stay"
    );
}
