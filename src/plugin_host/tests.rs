//! The plugin host on the probe plugin (`plugins/probe`, built with the workspace as a dev-dependency):
//! loading, its commands, a message answered, a failure and a panic reported, a plugin disabled or broken,
//! a plugin misbehaving as a marker file in its folder asks.

#[cfg(windows)]
use std::time::{Duration, Instant};

use super::*;
// What the tests here and under them use of the host's parts (the plugins load on Windows only).
#[cfg(windows)]
use super::worker::END;
#[cfg(windows)]
use crate::vfs::Entry;
#[cfg(windows)]
use std::io;

/// The plugins in `plugins` that `enabled` lets load, their logs in `plugins\Logs`.
pub(crate) fn load(plugins: &Path, enabled: &dyn Fn(&str) -> bool) -> PluginHost {
    PluginHost::load(
        plugins,
        &plugins.join("Logs"),
        &serde_json::Map::new(),
        enabled,
    )
}

/// The probe's library as the build left it: beside the test program or one folder up.
fn probe_library() -> PathBuf {
    built_library("probe")
}

/// The library `library` of this workspace's plugins as the build left it: in the nearest folder up from
/// the test program that has it (`target/debug/deps` held the test programs; since Rust 1.100 they are
/// under `target/debug/build/<package>/<hash>/out`, the libraries still in `target/debug`).
fn built_library(library: &str) -> PathBuf {
    let name = platform::libraries::file_name(library);
    let exe = std::env::current_exe().expect("the test program");
    (exe.ancestors().skip(1))
        .map(|folder| folder.join(&name))
        .find(|path| path.is_file())
        .expect("the probe is built with the workspace")
}

/// The probe's own manifest, as JSON.
fn probe_manifest() -> serde_json::Value {
    serde_json::from_str(include_str!("../../plugins/probe/plugin.json"))
        .expect("the probe's manifest")
}

/// The probe installed in `plugins` as `plugins\<folder>`, its manifest `manifest`.
pub(crate) fn install_probe(plugins: &Path, folder: &str, manifest: &str) -> PathBuf {
    let place = plugins.join(folder);
    std::fs::create_dir_all(&place).unwrap();
    std::fs::write(place.join("plugin.json"), manifest).unwrap();
    let library = platform::libraries::file_name("probe");
    std::fs::copy(probe_library(), place.join(library)).unwrap();
    place
}

/// The shipped plugin of library `library` (`plugins/<library>`, the temporary panel's `tmppanel`)
/// installed in `plugins` as the plugin `name` (each test its own: the providers' list is the program's).
#[cfg(windows)]
pub(crate) fn install_shipped(plugins: &Path, library: &str, name: &str) -> PathBuf {
    let place = plugins.join(name);
    std::fs::create_dir_all(&place).unwrap();
    let manifest =
        format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "{library}"}}"#);
    std::fs::write(place.join("plugin.json"), manifest).unwrap();
    let file = platform::libraries::file_name(library);
    std::fs::copy(built_library(library), place.join(file)).unwrap();
    place
}

/// A `plugins` folder with the probe installed as `plugins\probe`, its manifest changed by `edit`.
pub(crate) fn plugins_with_probe(edit: impl Fn(&mut serde_json::Value)) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let mut manifest = probe_manifest();
    edit(&mut manifest);
    install_probe(temp.path(), "probe", &manifest.to_string());
    temp
}

/// The probe as it is, with marker files in its folder.
#[cfg(windows)]
pub(crate) fn probe_with_markers(markers: &[&str]) -> tempfile::TempDir {
    let temp = plugins_with_probe(|_| {});
    for marker in markers {
        std::fs::write(temp.path().join("probe").join(marker), "").unwrap();
    }
    temp
}

#[cfg(windows)]
pub(crate) fn next_event(host: &PluginHost) -> Event {
    // A call's end without a panel (`Event::Ran`, ADR-0019) is the program's to wait on; the tests here
    // look at what comes besides it.
    loop {
        match next_event_or_end(host) {
            Event::Ran { .. } => {}
            event => return event,
        }
    }
}

/// The next event, a call's end without a panel included.
#[cfg(windows)]
pub(crate) fn next_event_or_end(host: &PluginHost) -> Event {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(event) = host.take_event() {
            return event;
        }
        assert!(Instant::now() < deadline, "no event from the plugin");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The next event as a message the plugin waits on: its plugin, the message and the reply.
#[cfg(windows)]
fn next_message(host: &PluginHost) -> (String, MessageRequest, Sender<Answer>) {
    match next_event(host) {
        Event::Ask {
            plugin,
            request: Request::Message(message),
            reply,
        } => (plugin, message, reply),
        other => panic!("a message: {other:?}"),
    }
}

#[cfg(windows)]
#[test]
fn the_probe_loads_and_tells_its_commands() {
    let folder = plugins_with_probe(|_| {});
    let host = load(folder.path(), &|_| true);
    assert!(host.failures.is_empty(), "{:?}", host.failures);
    let [probe] = host.plugins.as_slice() else {
        panic!("one plugin");
    };
    assert_eq!(probe.name, "probe");
    assert_eq!(probe.info.title, "Probe");
    assert_eq!(probe.info.author, "nova-commander");
    assert_eq!(probe.commands, ["Say hello", "Fail", "Panic"]);
    assert!(probe.disks.is_empty(), "no drive menu items unless asked");
}

#[cfg(windows)]
#[test]
fn the_drive_menus_items_are_told_at_the_start_and_run_on_the_plugins_thread() {
    let folder = plugins_with_probe(|_| {});
    std::fs::write(
        folder.path().join("probe").join("disks.txt"),
        "Temp
Fail
",
    )
    .unwrap();
    let host = load(folder.path(), &|_| true);
    let probe = &host.plugins[0];
    assert_eq!(probe.disks, ["Temp", "Fail"]);
    assert_eq!(
        probe.commands,
        ["Say hello", "Fail", "Panic"],
        "the commands their own"
    );
    host.open_disk(0, 0, 0, 0);
    let (plugin, message, reply) = next_message(&host);
    assert_eq!((plugin.as_str(), message.title.as_str()), ("probe", "Disk"));
    assert_eq!(message.lines, ["0 Temp"]);
    reply.send(Answer::Button(0)).unwrap();
    host.open_disk(0, 1, 0, 0);
    let Event::Failed { error, .. } = next_event(&host) else {
        panic!("the item fails");
    };
    assert_eq!(
        error, "The probe's disk fails on purpose",
        "nothing came between"
    );
    host.open_disk(0, 5, 0, 0);
    let Event::Failed { error, .. } = next_event(&host) else {
        panic!("no such item");
    };
    assert_eq!(error, "The probe has no disk 5");
    host.open_disk(3, 0, 0, 0);
    host.run_command(0, 1, 0, 0);
    let Event::Failed { error, .. } = next_event(&host) else {
        panic!("the command fails");
    };
    assert_eq!(
        error, "The probe fails on purpose",
        "a plugin not there ran nothing"
    );
}

#[cfg(windows)]
#[test]
fn a_message_waits_for_its_answer() {
    let folder = plugins_with_probe(|_| {});
    let host = load(folder.path(), &|_| true);
    host.run_command(0, 0, 0, 0);
    let (plugin, message, reply) = next_message(&host);
    assert_eq!(
        (plugin.as_str(), message.title.as_str()),
        ("probe", "Probe")
    );
    assert_eq!(message.lines, ["Hello from the probe", "second line"]);
    assert_eq!(message.buttons, ["OK"]);
    assert!(!message.warning && !message.left_align);
    reply.send(Answer::Esc).unwrap();
    let Event::Failed { error, .. } = next_event(&host) else {
        panic!("Esc fails the probe's command");
    };
    assert_eq!(error, "closed with Esc");
}

#[cfg(windows)]
#[test]
fn a_failure_and_a_panic_are_reported_and_the_plugin_goes_on() {
    let folder = plugins_with_probe(|_| {});
    let host = load(folder.path(), &|_| true);
    host.run_command(0, 1, 0, 0);
    let Event::Failed { error, .. } = next_event(&host) else {
        panic!("a failure");
    };
    assert_eq!(error, "The probe fails on purpose");
    host.run_command(0, 2, 0, 0);
    let Event::Failed { error, .. } = next_event(&host) else {
        panic!("a panic");
    };
    assert_eq!(error, "The plugin failed: the probe panics on purpose");
    host.run_command(0, 0, 0, 0);
    let _still_there = next_message(&host);
}

#[cfg(windows)]
#[test]
fn the_program_ends_while_a_plugin_waits_for_an_answer() {
    let folder = plugins_with_probe(|_| {});
    let host = load(folder.path(), &|_| true);
    host.run_command(0, 0, 0, 0);
    let event = next_event(&host);
    let started = Instant::now();
    drop(event);
    drop(host);
    assert!(started.elapsed() < END, "the plugin was answered and ended");
}

#[cfg(windows)]
#[test]
fn a_panic_starting_is_a_plugin_that_did_not_start() {
    let folder = probe_with_markers(&["panic-in-new"]);
    let host = load(folder.path(), &|_| true);
    assert!(host.plugins.is_empty());
    assert_eq!(host.failures[0].reason, "The plugin did not start");
}

#[cfg(windows)]
#[test]
fn a_message_while_starting_is_answered_at_once() {
    for marker in ["message-in-new", "message-in-info"] {
        message_while_starting(marker);
    }
}

#[cfg(windows)]
fn message_while_starting(marker: &str) {
    let folder = probe_with_markers(&[marker]);
    let started = Instant::now();
    let host = load(folder.path(), &|_| true);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{marker}: no wait"
    );
    assert_eq!(host.plugins.len(), 1, "{:?}", host.failures);
    assert!(host.take_event().is_none(), "nothing left to show");
    let answered = std::fs::read_to_string(folder.path().join("probe").join("answered")).unwrap();
    assert_eq!(answered, "false", "Esc");
}

#[cfg(windows)]
#[test]
fn a_panic_telling_or_listing_leaves_the_plugin_loaded_with_less() {
    let folder = probe_with_markers(&["panic-in-info", "panic-in-commands"]);
    let host = load(folder.path(), &|_| true);
    let [probe] = host.plugins.as_slice() else {
        panic!("loaded: {:?}", host.failures);
    };
    assert_eq!(probe.info, PluginInfo::default());
    assert!(probe.commands.is_empty());
}

#[cfg(windows)]
#[test]
fn a_library_without_the_entry_is_not_a_plugin() {
    let temp = tempfile::tempdir().unwrap();
    let place = temp.path().join("other");
    std::fs::create_dir(&place).unwrap();
    let manifest = r#"{"name": "other", "version": "1", "api": 11, "library": "other"}"#;
    std::fs::write(place.join("plugin.json"), manifest).unwrap();
    let system = platform::process::system_folder().join("System32");
    std::fs::copy(system.join("version.dll"), place.join("other.dll")).unwrap();
    let host = load(temp.path(), &|_| true);
    assert_eq!(
        host.failures[0].reason,
        "The library exports no nc_plugin_entry"
    );
}

#[cfg(windows)]
#[test]
fn a_second_plugin_of_the_same_name_is_not_loaded() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = probe_manifest().to_string();
    install_probe(temp.path(), "a", &manifest);
    install_probe(temp.path(), "b", &manifest);
    let host = load(temp.path(), &|_| true);
    assert_eq!(host.plugins.len(), 1);
    assert_eq!(host.failures[0].path, temp.path().join("b"));
    assert_eq!(host.failures[0].reason, "Another plugin is named probe");
}

#[cfg(windows)]
#[test]
fn a_plugin_named_as_another_but_for_case_is_not_loaded() {
    // Review R48: its panels' paths name it in lower case, so the calls would go to either.
    let temp = tempfile::tempdir().unwrap();
    let mut upper = probe_manifest();
    upper["name"] = "Probe".into();
    install_probe(temp.path(), "a", &probe_manifest().to_string());
    install_probe(temp.path(), "b", &upper.to_string());
    let host = load(temp.path(), &|_| true);
    assert_eq!(host.plugins.len(), 1);
    assert_eq!(host.failures[0].reason, "Another plugin is named Probe");
}

#[test]
fn a_disabled_plugin_is_not_loaded() {
    let folder = plugins_with_probe(|_| {});
    let host = load(folder.path(), &|name| name != "probe");
    assert!(host.plugins.is_empty());
    assert!(host.failures.is_empty());
}

#[test]
fn a_plugin_that_cannot_load_says_why() {
    let other_api = plugins_with_probe(|manifest| manifest["api"] = 99.into());
    let host = load(other_api.path(), &|_| true);
    assert_eq!(
        host.failures[0].reason,
        format!("The plugin needs API version 99; this program has {API_VERSION}")
    );
    let no_library = plugins_with_probe(|manifest| manifest["library"] = "missing".into());
    let host = load(no_library.path(), &|_| true);
    assert_eq!(host.failures.len(), 1, "{:?}", host.failures);
    assert!(host.plugins.is_empty());
    let broken = tempfile::tempdir().unwrap();
    install_probe(broken.path(), "probe", "{");
    let host = load(broken.path(), &|_| true);
    assert!(host.failures[0].reason.starts_with("plugin.json:"));
    assert_eq!(host.failures[0].path, broken.path().join("probe"));
}

#[test]
fn a_folder_without_plugins_loads_none() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("not a plugin")).unwrap();
    let host = load(temp.path(), &|_| true);
    assert!(host.plugins.is_empty() && host.failures.is_empty());
    let missing = load(&temp.path().join("none"), &|_| true);
    assert!(missing.plugins.is_empty());
}

/// The probe installed as `plugins\<folder>`, named `name` in its manifest.
#[cfg(windows)]
fn install_named(plugins: &Path, folder: &str, name: &str) -> PathBuf {
    let mut manifest = probe_manifest();
    manifest["name"] = name.into();
    install_probe(plugins, folder, &manifest.to_string())
}

/// The next event, a failure: its plugin and its reason.
#[cfg(windows)]
fn next_failure(host: &PluginHost) -> (String, String) {
    match next_event(host) {
        Event::Failed { plugin, error, .. } => (plugin, error),
        other => panic!("a failure: {other:?}"),
    }
}

#[cfg(windows)]
#[test]
fn a_library_is_a_dll() {
    assert_eq!(platform::libraries::file_name("probe"), "probe.dll");
}

#[cfg(windows)]
#[test]
fn the_probe_tells_all_it_is() {
    let folder = plugins_with_probe(|_| {});
    let host = load(folder.path(), &|_| true);
    let expected = PluginInfo {
        title: "Probe".to_string(),
        description: "The tests' plugin".to_string(),
        author: "nova-commander".to_string(),
        version: "0.1.0".to_string(),
    };
    assert_eq!(host.plugins[0].info, expected, "each field its own");
}

#[cfg(windows)]
#[test]
fn the_library_is_the_manifests_and_the_plugin_goes_by_its_name() {
    let temp = tempfile::tempdir().unwrap();
    let place = temp.path().join("folder-x");
    std::fs::create_dir(&place).unwrap();
    let manifest = r#"{"name": "named", "version": "1", "api": 11, "library": "custom"}"#;
    std::fs::write(place.join("plugin.json"), manifest).unwrap();
    std::fs::copy(probe_library(), place.join("custom.dll")).unwrap();
    let host = load(temp.path(), &|_| true);
    assert!(host.failures.is_empty(), "{:?}", host.failures);
    assert_eq!(host.plugins[0].name, "named");
    host.run_command(0, 1, 0, 0);
    assert_eq!(
        next_failure(&host).0,
        "named",
        "not its folder's or library's"
    );
}

#[cfg(windows)]
#[test]
fn plugins_load_in_their_folders_order_as_enabled_by_their_names() {
    let temp = tempfile::tempdir().unwrap();
    install_named(temp.path(), "a", "zulu");
    install_named(temp.path(), "b", "alpha");
    install_named(temp.path(), "c", "mike");
    let asked = std::cell::RefCell::new(Vec::new());
    let enabled = |name: &str| {
        asked.borrow_mut().push(name.to_string());
        name != "alpha"
    };
    let host = load(temp.path(), &enabled);
    assert_eq!(asked.into_inner(), ["zulu", "alpha", "mike"]);
    let names: Vec<&str> = host
        .plugins
        .iter()
        .map(|plugin| plugin.name.as_str())
        .collect();
    assert_eq!(names, ["zulu", "mike"], "by folder, not by name; alpha off");
    assert!(host.failures.is_empty(), "turned off is no failure");
    host.run_command(1, 1, 0, 0);
    assert_eq!(
        next_failure(&host).0,
        "mike",
        "a command goes to its plugin"
    );
}

#[cfg(windows)]
#[test]
fn commands_run_in_their_order_and_an_unknown_plugin_runs_none() {
    let folder = plugins_with_probe(|_| {});
    let host = load(folder.path(), &|_| true);
    host.run_command(1, 1, 0, 0);
    host.run_command(0, 1, 0, 0);
    host.run_command(0, 2, 0, 0);
    host.run_command(0, 7, 0, 0);
    let failures: Vec<(String, String)> = (0..3).map(|_| next_failure(&host)).collect();
    let probe = |error: &str| ("probe".to_string(), error.to_string());
    assert_eq!(
        failures,
        [
            probe("The probe fails on purpose"),
            probe("The plugin failed: the probe panics on purpose"),
            probe("The plugin failed: the probe has no command 7"),
        ],
        "a formatted panic says what it said"
    );
}

#[cfg(windows)]
#[test]
fn ok_to_a_message_lets_its_command_succeed() {
    let folder = plugins_with_probe(|_| {});
    let host = load(folder.path(), &|_| true);
    host.run_command(0, 0, 0, 0);
    let (_, _, reply) = next_message(&host);
    reply.send(Answer::Button(0)).unwrap();
    host.run_command(0, 1, 0, 0);
    assert_eq!(
        next_failure(&host).1,
        "The probe fails on purpose",
        "the next command's failure, none from OK"
    );
}

#[cfg(windows)]
#[test]
fn a_command_that_opens_no_panel_tells_its_end() {
    // ADR-0019 (review R13): the program waits on a command's end; one with no panel says so.
    let folder = plugins_with_probe(|_| {});
    let host = load(folder.path(), &|_| true);
    host.run_command(0, 0, 0, 0);
    let Event::Ask { reply, .. } = next_event_or_end(&host) else {
        panic!("its message first");
    };
    reply.send(Answer::Button(0)).unwrap();
    let end = next_event_or_end(&host);
    assert!(
        matches!(&end, Event::Ran { plugin, .. } if plugin == "probe"),
        "{end:?}"
    );
}

#[cfg(windows)]
#[test]
fn a_message_comes_as_the_plugin_wrote_it() {
    let folder = plugins_with_probe(|_| {});
    let text = "Its title\none\r\ntwo\n\nfour\n";
    std::fs::write(folder.path().join("probe").join("message.txt"), text).unwrap();
    let host = load(folder.path(), &|_| true);
    host.run_command(0, 0, 0, 0);
    let (_, message, reply) = next_message(&host);
    assert_eq!(message.title, "Its title");
    assert_eq!(
        message.lines,
        ["one", "two", "", "four", ""],
        "an empty line at the end stays"
    );
    reply.send(Answer::Button(0)).unwrap();
}

#[cfg(windows)]
#[test]
fn a_long_menu_is_cut_at_the_most_commands() {
    let folder = plugins_with_probe(|_| {});
    let items: Vec<String> = (0..=1000).map(|index| format!("Item {index}")).collect();
    let commands = folder.path().join("probe").join("commands.txt");
    std::fs::write(commands, items.join("\n")).unwrap();
    let host = load(folder.path(), &|_| true);
    let commands = &host.plugins[0].commands;
    assert_eq!(commands.len(), 1000, "1001 told, 1000 asked for");
    assert_eq!(commands[..], items[..1000]);
}

#[test]
fn a_plugin_turned_off_is_not_checked_further() {
    let folder = plugins_with_probe(|manifest| manifest["api"] = 99.into());
    let host = load(folder.path(), &|name| name != "probe");
    assert!(host.plugins.is_empty());
    assert!(host.failures.is_empty(), "{:?}", host.failures);
}

#[test]
fn an_earlier_api_version_is_another_version() {
    let folder = plugins_with_probe(|manifest| manifest["api"] = 0.into());
    let host = load(folder.path(), &|_| true);
    assert!(host.plugins.is_empty());
    assert_eq!(
        host.failures,
        [LoadFailure {
            path: folder.path().join("probe"),
            reason: format!("The plugin needs API version 0; this program has {API_VERSION}"),
        }]
    );
}

#[test]
fn a_folder_named_plugin_json_is_no_manifest() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("odd").join("plugin.json")).unwrap();
    std::fs::write(temp.path().join("plugin.json"), "{").unwrap();
    let host = load(temp.path(), &|_| true);
    assert!(host.plugins.is_empty());
    assert!(host.failures.is_empty(), "{:?}", host.failures);
}

#[test]
fn a_host_without_plugins_runs_nothing() {
    let host = PluginHost::default();
    host.run_command(0, 0, 0, 0);
    assert!(host.take_event().is_none());
}

#[cfg(not(windows))]
#[test]
fn other_systems_say_why_no_plugin_loads() {
    let folder = plugins_with_probe(|_| {});
    let host = load(folder.path(), &|_| true);
    assert!(host.plugins.is_empty());
    assert_eq!(
        host.failures[0].reason,
        "plugins are not loaded on this system yet"
    );
}

#[path = "panels_tests.rs"]
mod panels;

#[path = "files_tests.rs"]
mod files;
