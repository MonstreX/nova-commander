//! Plugins loaded, unloaded and started anew while the program runs, on the probe: its library loaded
//! from a copy (the installed file free to be replaced), its paths gone with it, one hanging in a call
//! left there while a new one answers, the plugins' folder looked at again.
#![cfg(windows)]

use std::time::{Duration, Instant};

use super::*;
use crate::plugin_host::tests::{install_probe, load};
use crate::vfs::VfsPath;

fn manifest(name: &str) -> String {
    format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#)
}

/// The starts told, waited for (at most 10 s) until one is.
fn started(host: &mut PluginHost) -> Started {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(told) = host.pump_starts().pop() {
            return told;
        }
        assert!(Instant::now() < deadline, "no start told");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn log(plugins: &std::path::Path, name: &str) -> String {
    std::fs::read_to_string(plugins.join("Logs").join(format!("{name}.log"))).unwrap_or_default()
}

#[test]
fn a_plugin_unloaded_and_loaded_again_runs_from_a_copy_its_installed_file_free() {
    let temp = tempfile::tempdir().unwrap();
    let place = install_probe(temp.path(), "liveone", &manifest("liveone"));
    let mut host = load(temp.path(), &|_| true);
    assert_eq!(host.plugins.len(), 1);
    let library = place.join(crate::platform::libraries::file_name("probe"));
    // Loaded from a copy: the installed library can be written over while it runs.
    let code = std::fs::read(&library).unwrap();
    std::fs::write(&library, &code).unwrap();
    let path = VfsPath::in_session("liveone", "one", "/");
    assert!(vfs::provider_for(&path).is_ok());
    assert_eq!(host.unload("LiveOne"), Some(place.clone()));
    assert!(host.plugins.is_empty());
    let gone = vfs::provider_for(&path).err().map(|error| error.kind());
    assert_eq!(
        gone,
        Some(std::io::ErrorKind::Unsupported),
        "its paths have none"
    );
    assert_eq!(host.unload("liveone"), None, "not twice");
    assert_eq!(host.not_loaded(), [("liveone".to_string(), place.clone())]);
    host.start_plugin(&place, &serde_json::Map::new()).unwrap();
    assert_eq!(host.starting_names(), ["liveone"]);
    let refused = host.start_plugin(&place, &serde_json::Map::new());
    assert!(refused.is_err(), "one of a name: {refused:?}");
    assert!(matches!(started(&mut host), Started::Loaded(name) if name == "liveone"));
    assert!(host.starting_names().is_empty() && host.not_loaded().is_empty());
    assert!(vfs::provider_for(&path).is_ok(), "its paths back");
}

#[test]
fn a_plugin_hanging_in_a_call_is_started_anew_and_the_new_one_answers() {
    let temp = tempfile::tempdir().unwrap();
    let place = install_probe(temp.path(), "livetwo", &manifest("livetwo"));
    std::fs::write(place.join("commands.txt"), "Stubborn\nPanel live\n").unwrap();
    let mut host = load(temp.path(), &|_| true);
    assert!(host.run_command(0, 0, 0, 1), "stubborn: hangs");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !host.in_call("livetwo") {
        assert!(Instant::now() < deadline, "not in its call");
        std::thread::sleep(Duration::from_millis(5));
    }
    let place_back = host.unload("livetwo").unwrap();
    host.start_plugin(&place_back, &serde_json::Map::new())
        .unwrap();
    assert!(matches!(started(&mut host), Started::Loaded(_)));
    assert!(!host.in_call("livetwo"), "the new one is free");
    // The new one runs a call and ends it; the one left hanging is heard no more.
    assert!(host.run_command(0, 1, 0, 2));
    let deadline = Instant::now() + Duration::from_secs(10);
    let ended = loop {
        match host.take_event() {
            Some(Event::Panel { serial, .. }) => break serial,
            Some(_) => {}
            None => {
                assert!(Instant::now() < deadline, "the new one does not answer");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    };
    assert_eq!(ended, 2);
    std::fs::write(place.join("release"), "").unwrap();
    // Woken, it ends its thread, which removes its library's copy: the new one's alone is left. Its log
    // line is not written: a plugin let go is heard no more.
    let copies = || {
        (std::fs::read_dir(&place).unwrap().flatten())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".probe.vc-")
            })
            .count()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while copies() != 1 {
        assert!(Instant::now() < deadline, "the old one never ended");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!log(temp.path(), "livetwo").contains("stubborn ended"));
    std::thread::sleep(Duration::from_millis(100));
    let late = std::iter::from_fn(|| host.take_event())
        .find(|event| matches!(event, Event::Ran { serial: 1, .. } | Event::Failed { .. }));
    assert!(late.is_none(), "the old one's end is not heard: {late:?}");
}

#[test]
fn a_folder_without_a_good_manifest_is_no_plugin_to_load_and_a_start_that_fails_is_told() {
    let temp = tempfile::tempdir().unwrap();
    let mut host = load(temp.path(), &|_| true);
    std::fs::create_dir(temp.path().join("empty")).unwrap();
    assert!(host.not_loaded().is_empty());
    let broken = temp.path().join("broken");
    std::fs::create_dir(&broken).unwrap();
    std::fs::write(broken.join("plugin.json"), manifest("broken")).unwrap();
    assert_eq!(host.not_loaded().len(), 1, "a manifest: a plugin to load");
    host.start_plugin(&broken, &serde_json::Map::new()).unwrap();
    let Started::Failed(failure) = started(&mut host) else {
        panic!("no library: failed");
    };
    assert_eq!(failure.path, broken);
    assert!(host.plugins.is_empty());
}
