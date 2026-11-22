//! Copy and move with an end in a plugin's panel (ADR-0016 §13, 5c-2): the engine against the probe's
//! provider - files in and out in several chunks, a move within its panel one rename, a move across its
//! panels a copy and a removal, an overwrite through a temporary name, a write it refuses mid-file.
//! Each test names its probe alone: the providers' list is the program's.
#![cfg(windows)]

use std::fs;

use super::tests::Script;
use super::tests_vfs::{LONG, clean, copy_into, over};
use super::*;
use crate::plugin_host::PluginHost;
use crate::plugin_host::tests::{install_probe, load};
use crate::vfs::provider_for;

/// The probe loaded as plugin `name`; its log's folder.
pub(super) fn probe(name: &str) -> (tempfile::TempDir, PluginHost) {
    let temp = tempfile::tempdir().unwrap();
    let manifest =
        format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#);
    install_probe(temp.path(), name, &manifest);
    let host = load(temp.path(), &|_| true);
    assert!(host.failures.is_empty(), "{:?}", host.failures);
    (temp, host)
}

pub(super) fn read(path: &VfsPath) -> Vec<u8> {
    let mut data = Vec::new();
    std::io::Read::read_to_end(
        &mut *provider_for(path).unwrap().open_read(path).unwrap(),
        &mut data,
    )
    .unwrap();
    data
}

fn exists(path: &VfsPath) -> bool {
    provider_for(path)
        .and_then(|provider| provider.stat(path))
        .is_ok()
}

pub(super) fn log(temp: &tempfile::TempDir, name: &str) -> String {
    fs::read_to_string(temp.path().join("Logs").join(format!("{name}.log"))).unwrap_or_default()
}

/// `LONG` bytes that are not all alike.
fn long() -> Vec<u8> {
    (0..LONG).map(|at| (at % 251) as u8).collect()
}

#[test]
fn files_go_in_and_out_of_a_plugins_panel_in_several_chunks() {
    let (_temp, _host) = probe("transferone");
    let disk = tempfile::tempdir().unwrap();
    fs::write(disk.path().join("big.bin"), long()).unwrap();
    let panel = VfsPath::in_session("transferone", "one", "/docs");
    let request = copy_into(&[&VfsPath::local(disk.path().join("big.bin"))], &panel);
    clean(&run(&request, &mut Script::default()));
    assert_eq!(read(&panel.join("big.bin")), long(), "in");
    let out = tempfile::tempdir().unwrap();
    let request = copy_into(&[&panel.join("big.bin")], &VfsPath::local(out.path()));
    clean(&run(&request, &mut Script::default()));
    assert_eq!(fs::read(out.path().join("big.bin")).unwrap(), long(), "out");
}

#[test]
fn a_move_within_a_panel_is_one_rename_and_across_panels_a_copy_and_a_removal() {
    let (temp, _host) = probe("transfertwo");
    let one = VfsPath::in_session("transfertwo", "one", "/");
    let request = over(
        &[&one.join("readme.txt")],
        &one.join("docs"),
        true,
        Existing::Ask,
    );
    clean(&run(&request, &mut Script::default()));
    assert!(log(&temp, "transfertwo").contains("renamed one:/readme.txt to /docs/readme.txt"));
    assert!(!log(&temp, "transfertwo").contains("made file"), "no copy");
    let two = VfsPath::in_session("transfertwo", "two", "/");
    let request = over(
        &[&one.join("docs").join("a.txt")],
        &two,
        true,
        Existing::Ask,
    );
    clean(&run(&request, &mut Script::default()));
    assert_eq!(read(&two.join("a.txt")), b"a");
    assert!(
        !exists(&one.join("docs").join("a.txt")),
        "the source removed"
    );
    let told = log(&temp, "transfertwo");
    assert!(told.contains("made file two:/a.txt") && told.contains("removed one:/docs/a.txt"));
}

#[test]
fn an_existing_file_in_a_plugins_panel_is_overwritten_whole() {
    let (_temp, _host) = probe("transferthree");
    let disk = tempfile::tempdir().unwrap();
    fs::write(disk.path().join("readme.txt"), "replaced").unwrap();
    let panel = VfsPath::in_session("transferthree", "one", "/");
    let source = VfsPath::local(disk.path().join("readme.txt"));
    let request = over(&[&source], &panel, false, Existing::Overwrite);
    clean(&run(&request, &mut Script::default()));
    assert_eq!(read(&panel.join("readme.txt")), b"replaced");
    let mut names = Vec::new();
    provider_for(&panel)
        .unwrap()
        .list(
            &panel,
            &std::sync::atomic::AtomicBool::new(false),
            &mut |entry| {
                names.push(entry.name);
            },
        )
        .unwrap();
    assert_eq!(
        names.iter().filter(|name| name.contains("readme")).count(),
        1,
        "{names:?}"
    );
}

#[test]
fn a_write_the_plugin_refuses_mid_file_leaves_no_part_and_keeps_a_moves_source() {
    let (_temp, _host) = probe("transferfour");
    let disk = tempfile::tempdir().unwrap();
    let source = disk.path().join("full.bin");
    fs::write(&source, long()).unwrap();
    let panel = VfsPath::in_session("transferfour", "one", "/");
    let request = over(&[&VfsPath::local(&source)], &panel, true, Existing::Ask);
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);
    let report = run(&request, &mut host);
    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert!(!exists(&panel.join("full.bin")), "the part removed");
    assert_eq!(
        fs::read(&source).unwrap(),
        long(),
        "the move's source stays"
    );
}
