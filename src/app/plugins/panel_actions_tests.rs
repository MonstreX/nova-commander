//! What a plugin does to the panels (API 3) on the probe's script, with the panels read in the background
//! (no wait), so that a request for a panel being read is answered only once it is read.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::super::super::fileops::tests::app_in;
use super::super::super::{App, Screen, Side};
use crate::plugin_host::tests::{install_probe, load as load_plugins};
use crate::vfs::VfsPath;

struct Probe {
    temp: tempfile::TempDir,
    place: PathBuf,
    app: App,
}

/// The probe as `name` (F11: "Run script"; the drive menu: its panel "one"), the left panel in `left`
/// (a.txt, b.txt, c.txt, active, the cursor on b.txt), the right one in `right` (x.txt); both panels read
/// in the background.
fn probe(name: &str) -> Probe {
    let temp = tempfile::tempdir().unwrap();
    let manifest =
        format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#);
    let place = install_probe(temp.path(), name, &manifest);
    std::fs::write(place.join("commands.txt"), "Run script").unwrap();
    std::fs::write(place.join("disks.txt"), "Panel one").unwrap();
    let (left, right) = (temp.path().join("left"), temp.path().join("right"));
    for (folder, files) in [
        (&left, &["a.txt", "b.txt", "c.txt"][..]),
        (&right, &["x.txt"][..]),
    ] {
        std::fs::create_dir_all(folder).unwrap();
        for file in files {
            std::fs::write(folder.join(file), file).unwrap();
        }
    }
    let mut app = app_in(&left);
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    app.panels[1]
        .change_dir(&VfsPath::local(&right), None)
        .unwrap();
    tick_until(&mut app, |app| !app.panels[1].is_reading());
    for panel in &mut app.panels {
        panel.set_read_wait(Duration::ZERO);
    }
    app.active = Side::Left;
    app.panels[0].go_to_name("b.txt");
    Probe { temp, place, app }
}

fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

impl Probe {
    fn folder(&self, name: &str) -> String {
        self.temp.path().join(name).display().to_string()
    }

    /// The script run (F11), its answers once it ended.
    fn run(&mut self, script: &[String]) -> Vec<String> {
        let said = self.place.join("panels-said.txt");
        let _ = std::fs::remove_file(&said);
        std::fs::write(self.place.join("script.txt"), script.join("\n")).unwrap();
        self.app.run_plugin_command(&[(0, 0)], 0);
        tick_until(&mut self.app, |_| said.exists());
        // The command's end comes after its file.
        tick_until(&mut self.app, |app| app.plugins.key_wait.is_none());
        let text = std::fs::read_to_string(&said).unwrap();
        text.lines().map(str::to_string).collect()
    }
}

fn answer(line: &str) -> &str {
    line.split_once(" => ").map_or("", |(_, answer)| answer)
}

#[test]
fn a_folder_set_is_answered_once_read_and_a_missing_one_fails() {
    let mut probe = probe("actfolder");
    let (left, right) = (probe.folder("left"), probe.folder("right"));
    let missing = probe.folder("missing");
    let said = probe.run(&[
        format!("folder|passive|{left}|c.txt"),
        "state|passive".to_string(),
        "names|passive".to_string(),
        format!("folder|passive|{missing}"),
        "folder|passive|..\\right".to_string(),
        "state|passive".to_string(),
    ]);
    assert_eq!(answer(&said[0]), "ok");
    assert_eq!(
        answer(&said[1]),
        format!("{left} 4 sort 1 reverse false"),
        "read when asked"
    );
    assert_eq!(answer(&said[2]), r#"["..", "a.txt", "b.txt", "c.txt"]"#);
    assert_eq!(answer(&said[3]), "failed");
    assert_eq!(answer(&said[4]), "ok", "relative to the panel's folder");
    assert_eq!(answer(&said[5]), format!("{right} 2 sort 1 reverse false"));
}

#[test]
fn reading_again_keeps_or_clears_the_selection_and_select_sorts_and_closes_act() {
    let mut probe = probe("actreread");
    let said = probe.run(&[
        "select|active|a.txt|on".to_string(),
        "selectall|active|on".to_string(),
        "selected|active".to_string(),
        "select|active|b.txt|off".to_string(),
        "reread|active|keep".to_string(),
        "selected|active".to_string(),
        "reread|active|clear".to_string(),
        "selected|active".to_string(),
        "sort|active|6|reverse".to_string(),
        "state|active".to_string(),
        "close|active".to_string(),
    ]);
    let left = probe.folder("left");
    assert_eq!(answer(&said[2]), r#"["a.txt", "b.txt", "c.txt"]"#);
    assert_eq!(answer(&said[5]), r#"["a.txt", "c.txt"]"#, "kept as it was");
    assert_eq!(
        answer(&said[7]),
        r#"["b.txt"]"#,
        "none selected: the current one"
    );
    assert_eq!(answer(&said[9]), format!("{left} 4 sort 6 reverse true"));
    assert_eq!(answer(&said[10]), "ok", "a disk's panel has none to close");
}

#[test]
fn the_viewer_deletes_its_file_once_closed_and_the_editor_opens_at_its_line() {
    let mut probe = probe("actview");
    let file = probe.temp.path().join("shown.txt");
    std::fs::write(&file, "one\ntwo\nthree").unwrap();
    let shown = file.display().to_string();
    let said = probe.run(&[format!("view|active|{shown}|delete")]);
    assert_eq!(answer(&said[0]), "ok");
    tick_until(&mut probe.app, |app| {
        matches!(app.screen, Screen::Viewer(_))
    });
    assert!(file.exists(), "kept while shown");
    probe.app.screen = Screen::Panels;
    tick_until(&mut probe.app, |_| !file.exists());

    std::fs::write(&file, "one\ntwo\nthree").unwrap();
    let said = probe.run(&[format!("edit|active|{shown}|2|3")]);
    assert_eq!(answer(&said[0]), "ok");
    tick_until(&mut probe.app, |app| {
        matches!(app.screen, Screen::Editor(_))
    });
    // Line 2, column 3 (FAR's 1-based start line and character).
    tick_until(&mut probe.app, |app| match &app.screen {
        Screen::Editor(screen) => {
            screen.text.room.set((40, 5));
            screen
                .text
                .place()
                .is_some_and(|place| (place.0, place.2) == (2, 3))
        }
        _ => false,
    });
    // With the editor up a plugin opens no viewer over it.
    let said = probe.run(&[format!("view|active|{shown}")]);
    assert_eq!(answer(&said[0]), "failed");
    assert!(
        matches!(probe.app.screen, Screen::Editor(_)),
        "the user's editor stays"
    );
}

#[test]
fn on_its_own_panel_a_plugin_is_answered_at_once() {
    let mut probe = probe("actown");
    probe.app.open_disk_for_test(0, 0, Side::Left.index());
    let root = VfsPath::in_session("actown", "one", "/");
    tick_until(&mut probe.app, |app| {
        app.panels[0].path == root && !app.panels[0].is_reading()
    });
    // The screen's usual wait for a folder (the tests' 30 s): a wait for its own reading would show.
    probe.app.panels[0].set_read_wait(Duration::from_secs(30));
    let started = Instant::now();
    let said = probe.run(&[
        "reread|active|keep".to_string(),
        "folder|active|/docs".to_string(),
        "state|active".to_string(),
    ]);
    assert_eq!(answer(&said[0]), "ok");
    assert_eq!(answer(&said[1]), "ok");
    let docs = VfsPath::in_session("actown", "one", "/docs");
    tick_until(&mut probe.app, |app| app.panels[0].path == docs);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "no wait for itself"
    );
}

#[test]
fn in_its_listing_a_plugin_reads_the_panels_as_last_seen_and_changes_nothing() {
    let mut probe = probe("actlist");
    std::fs::write(probe.place.join("panel-in-list"), "").unwrap();
    probe.app.tick();
    probe.app.open_disk_for_test(0, 0, Side::Left.index());
    let root = VfsPath::in_session("actlist", "one", "/");
    tick_until(&mut probe.app, |app| {
        app.panels[0].path == root && !app.panels[0].is_reading()
    });
    let log = std::fs::read_to_string(log_of(probe.temp.path(), "actlist")).unwrap();
    let left = probe.folder("left");
    assert!(
        log.contains(&format!(
            "in the list: Some({left:?}) items 0 changed false"
        )),
        "{log}"
    );
    assert_eq!(probe.app.command_line.text(), "", "nothing changed");
}

fn log_of(folder: &Path, name: &str) -> PathBuf {
    folder.join("Logs").join(format!("{name}.log"))
}
