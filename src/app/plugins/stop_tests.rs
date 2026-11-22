//! A plugin's call asked to stop (ADR-0019 §5, API 2), on the probe: past the quick wait its window shows
//! the progress the plugin tells; Esc reaches the plugin, which ends with what it did; one that does not
//! look is asked about after a while and abandoned. Each test names its probe alone.
#![cfg(windows)]

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::super::fileops::tests::app_in;
use super::super::super::held::{NOT_ANSWERING, STOPPING};
use super::super::super::{App, Dialog, Overlay};
use crate::plugin_host::tests::{install_probe, load as load_plugins};

/// The probe as plugin `name` with the commands "Count" and "Stubborn", loaded into an app.
fn app_with(name: &str) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let manifest = format!(
        r#"{{"name": "{name}", "version": "1", "api": {}, "library": "probe"}}"#,
        nc_plugin_api::API_VERSION
    );
    let place = install_probe(temp.path(), name, &manifest);
    std::fs::write(place.join("commands.txt"), "Count\nStubborn").unwrap();
    let mut app = app_in(temp.path());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    (temp, app)
}

/// The event loop's periodic work until `done` holds, at most 20 s.
fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(app) {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn log(temp: &tempfile::TempDir, name: &str) -> String {
    let path = temp.path().join("Logs").join(format!("{name}.log"));
    std::fs::read_to_string(path).unwrap_or_default()
}

fn esc(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
}

fn last_line(app: &App) -> Option<String> {
    crate::app::waiting::held_text(app)?.last().cloned()
}

#[test]
fn esc_asks_a_counting_plugin_to_stop_and_its_window_shows_how_far_it_is() {
    let (temp, mut app) = app_with("stopcount");
    app.run_plugin_command(&[(0, 0)], 0);
    tick_until(&mut app, |app| {
        (crate::app::waiting::held_text(app))
            .is_some_and(|lines| lines.iter().any(|line| line.contains(" of 200 (")))
    });
    let lines = crate::app::waiting::held_text(&app).unwrap();
    assert!(lines[1].starts_with("item "), "the item at hand: {lines:?}");

    esc(&mut app);
    assert_eq!(
        last_line(&app).as_deref(),
        Some(STOPPING),
        "asked, not let go"
    );
    tick_until(&mut app, |app| !app.plugin_key_waits());
    assert!(
        log(&temp, "stopcount").contains("stopped at"),
        "the plugin heard it: {}",
        log(&temp, "stopcount")
    );
    assert!(
        app.overlay.is_none(),
        "its own end: no notice of letting go"
    );
}

#[test]
fn a_plugin_that_does_not_stop_is_asked_about_and_abandoned() {
    let (temp, mut app) = app_with("stopstubborn");
    app.run_plugin_command(&[(0, 1)], 0);
    tick_until(&mut app, |app| app.held_window().is_some());
    esc(&mut app);
    tick_until(&mut app, |app| {
        last_line(app).as_deref() == Some(NOT_ANSWERING[1])
    });
    esc(&mut app);
    assert!(!app.plugin_key_waits(), "abandoned");
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Message { title, .. })) if title == "Stopped")
    );
    std::fs::write(temp.path().join("stopstubborn").join("release"), "").unwrap();
}
