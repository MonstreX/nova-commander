//! The probe's items asked again: its `commands.txt` read anew, a busy plugin not waited for past the
//! menu's wait and its answer taken as it comes.

use std::time::Duration;
#[cfg(windows)]
use std::time::Instant;

use super::super::tests::{load, plugins_with_probe};

#[test]
fn the_items_are_asked_again_and_a_change_is_told() {
    let plugins = plugins_with_probe(|_| {});
    let commands = plugins.path().join("probe").join("commands.txt");
    std::fs::write(&commands, "One").unwrap();
    let mut host = load(plugins.path(), &|_| true);
    assert_eq!(host.plugins[0].commands, ["One"]);
    assert!(!host.refresh_items(Duration::from_secs(5)), "the same");
    std::fs::write(&commands, "One\nTwo").unwrap();
    assert!(host.refresh_items(Duration::from_secs(5)));
    assert_eq!(host.plugins[0].commands, ["One", "Two"]);
}

#[test]
#[cfg(windows)]
fn a_busy_plugin_is_waited_for_a_while_only_and_its_answer_taken_later() {
    let plugins = plugins_with_probe(|_| {});
    let place = plugins.path().join("probe");
    let commands = place.join("commands.txt");
    std::fs::write(&commands, "Stubborn").unwrap();
    let mut host = load(plugins.path(), &|_| true);
    // "Stubborn" holds the plugin's thread until `release` is in its folder.
    assert!(host.run_command(0, 0, 0, 1));
    std::fs::write(&commands, "Stubborn\nLater").unwrap();
    let started = Instant::now();
    assert!(
        !host.refresh_items(Duration::from_millis(50)),
        "not in time"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the wait is short"
    );
    let again = Instant::now();
    assert!(
        !host.refresh_items(Duration::from_secs(5)),
        "still asked: not waited for"
    );
    assert!(again.elapsed() < Duration::from_secs(2));
    std::fs::write(place.join("release"), "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !host.pump_items() {
        assert!(Instant::now() < deadline, "the late answer");
        let _ = host.take_event();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(host.plugins[0].commands, ["Stubborn", "Later"]);
}

#[test]
#[cfg(windows)]
fn a_busy_plugin_takes_none_of_the_others_wait() {
    let temp = tempfile::tempdir().unwrap();
    for (folder, name, commands) in [("a", "one", "Stubborn"), ("b", "two", "Old")] {
        let manifest =
            format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#);
        let place = super::super::tests::install_probe(temp.path(), folder, &manifest);
        std::fs::write(place.join("commands.txt"), commands).unwrap();
    }
    let mut host = load(temp.path(), &|_| true);
    assert_eq!(host.plugins[0].name, "one", "the busy one first");
    assert!(host.run_command(0, 0, 0, 1));
    std::fs::write(temp.path().join("b").join("commands.txt"), "New").unwrap();
    // Plugin one never answers in time; two is asked with it, not after its wait (asked after it, two
    // would get none of the time whatever it is: room enough for a loaded machine).
    assert!(host.refresh_items(Duration::from_secs(2)));
    assert_eq!(host.plugins[1].commands, ["New"]);
    std::fs::write(temp.path().join("a").join("release"), "").unwrap();
}
