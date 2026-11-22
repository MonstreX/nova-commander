//! A plugin's requests beyond the panels as the program answers them (the plugin's thread played by the
//! test): keys posted and done as typed after the events, the screen, a panel's filter, the clipboard,
//! no help yet, a command run as the command line runs it, a synchro sent back to the plugin's thread.

use std::sync::mpsc::{Receiver, channel};

use super::*;
use crate::plugin_host::Event;
use crate::plugin_host::panel_requests::{PanelAction, Which};
use crate::vfs::Entry;

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    (temp, app)
}

/// `request` asked as plugin "probe" asks; the receiver of the answer, the program's pump run.
fn asked(app: &mut App, request: PanelRequest) -> Receiver<PanelAnswer> {
    let (reply, answer) = channel();
    app.plugins.host.send_event(Event::PanelAsk {
        plugin: "probe".to_string(),
        request,
        reply,
    });
    app.pump_plugins();
    answer
}

fn answer(app: &mut App, request: PanelRequest) -> PanelAnswer {
    asked(app, request).try_recv().unwrap()
}

#[test]
fn keys_posted_are_done_as_typed_after_the_events_and_a_name_unknown_posts_none() {
    let (_temp, mut app) = app();
    let posted = PanelRequest::PostKeys("a Shift+b".to_string());
    assert_eq!(answer(&mut app, posted), PanelAnswer::Done(true));
    assert_eq!(app.command_line.text(), "aB", "typed into the command line");
    let wrong = PanelRequest::PostKeys("c Nonsense".to_string());
    assert_eq!(answer(&mut app, wrong), PanelAnswer::Done(false));
    assert_eq!(app.command_line.text(), "aB", "none of them done");
}

#[test]
fn the_screen_the_filter_the_clipboard_and_help_are_answered() {
    let (_temp, mut app) = app();
    let screen = PanelAnswer::Screen {
        columns: 100,
        rows: 30,
    };
    assert_eq!(answer(&mut app, PanelRequest::Screen), screen);
    let entry = |name: &str| Entry {
        name: name.to_string(),
        ..Entry::default()
    };
    let filter = PanelRequest::Filter {
        which: Which::Passive,
        entries: vec![entry("a.txt"), entry("b.txt")],
    };
    assert_eq!(
        answer(&mut app, filter),
        PanelAnswer::Matches(vec![true, true]),
        "no filter on: every one"
    );
    let put = PanelRequest::Clipboard("copied".to_string());
    assert_eq!(answer(&mut app, put), PanelAnswer::Done(true));
    assert_eq!(app.clipboard.text().unwrap().as_deref(), Some("copied"));
    let help = PanelRequest::Help("Contents".to_string());
    assert_eq!(
        answer(&mut app, help),
        PanelAnswer::Done(false),
        "no help yet"
    );
}

/// FAR's names of the confirmations, any case, answered by the user's Confirmations; another name by
/// none.
#[test]
fn a_confirmation_is_the_users_setting() {
    let (_temp, mut app) = app();
    app.prefs.confirm_connection = false;
    let asked =
        |app: &mut App, name: &str| answer(app, PanelRequest::Confirmation(name.to_string()));
    assert_eq!(
        asked(&mut app, "RemoveConnection"),
        PanelAnswer::Confirms(Some(false))
    );
    app.prefs.confirm_connection = true;
    assert_eq!(
        asked(&mut app, "removeconnection"),
        PanelAnswer::Confirms(Some(true))
    );
    app.config.delete_settings.confirm_folders = false;
    assert_eq!(
        asked(&mut app, "DeleteFolder"),
        PanelAnswer::Confirms(Some(false))
    );
    assert_eq!(asked(&mut app, "Copy"), PanelAnswer::Confirms(None));
}

/// FAR's `ProcessName(PN_CMPNAMELIST)`: the program's masks, its groups and exclusions; masks with errors
/// none.
#[test]
fn masks_are_matched_as_the_programs() {
    let (_temp, mut app) = app();
    let asked = |app: &mut App, masks: &str, name: &str| {
        let request = PanelRequest::MatchMasks {
            masks: masks.to_string(),
            name: name.to_string(),
        };
        answer(app, request)
    };
    let yes = PanelAnswer::Confirms(Some(true));
    let no = PanelAnswer::Confirms(Some(false));
    assert_eq!(asked(&mut app, "*.zip,*.[7bgxl]z", "a.ZIP"), yes);
    assert_eq!(asked(&mut app, "*.zip,*.[7bgxl]z", "a.7z"), yes);
    assert_eq!(asked(&mut app, "*.zip,*.[7bgxl]z", "a.rar"), no);
    assert_eq!(asked(&mut app, "*|b.*", "b.zip"), no);
    assert_eq!(asked(&mut app, "<arc>", "a.zip"), yes, "the groups");
    assert_eq!(asked(&mut app, "/(/", "a.zip"), PanelAnswer::Confirms(None));
}

#[test]
fn a_command_runs_as_the_command_line_runs_it_leaving_the_line_typed() {
    let (temp, mut app) = app();
    app.command_line.set("typed");
    let folder = temp.path().join("made");
    std::fs::create_dir(&folder).unwrap();
    let line = format!("cd \"{}\"", folder.display());
    let run = PanelRequest::Do(Which::Active, PanelAction::RunCommand(line));
    assert_eq!(answer(&mut app, run), PanelAnswer::Done(true));
    assert_eq!(app.command_line.text(), "typed");
    assert_eq!(
        app.active_panel().path,
        crate::vfs::VfsPath::local(&folder),
        "done"
    );
    let empty = PanelRequest::Do(Which::Active, PanelAction::RunCommand("  ".to_string()));
    assert_eq!(answer(&mut app, empty), PanelAnswer::Done(false));
}

#[test]
#[cfg(windows)]
fn a_synchro_asked_by_a_plugins_own_thread_comes_back_on_the_plugins_thread() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = r#"{"name": "synchroone", "version": "1", "api": 11, "library": "probe"}"#;
    let place = crate::plugin_host::tests::install_probe(temp.path(), "synchroone", manifest);
    std::fs::write(place.join("synchro"), "").unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.set_plugins(crate::plugin_host::tests::load(temp.path(), &|_| true));
    let log = temp.path().join("Logs").join("synchroone.log");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !std::fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("synchro 7")
    {
        assert!(std::time::Instant::now() < deadline, "no synchro");
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn keys_posted_while_the_plugins_call_is_waited_for_come_after_its_answer() {
    let (_temp, mut app) = app();
    app.plugins.hold_panel_key_for_test();
    let posted = PanelRequest::PostKeys("x".to_string());
    assert_eq!(answer(&mut app, posted), PanelAnswer::Done(true));
    assert_eq!(
        app.command_line.text(),
        "",
        "kept while its call is waited for"
    );
    app.plugins.key_wait = None;
    app.pump_plugins();
    assert_eq!(app.command_line.text(), "x", "done once it is answered");
}

/// API 9's transfer: a copy with something to copy is the program's task in front (its progress dialog
/// shown); one naming nothing is refused.
#[test]
fn a_plugins_copy_handed_over_is_a_task_in_front() {
    let (temp, mut app) = app();
    std::fs::write(temp.path().join("up.txt"), "x").unwrap();
    let transfer = |names: &[&str]| {
        PanelRequest::Transfer(Box::new(
            crate::plugin_host::panel_requests::TransferRequest {
                instance: "one".to_string(),
                folder: "/".to_string(),
                names: names.iter().map(|name| name.to_string()).collect(),
                disk: temp.path().display().to_string(),
                upload: true,
                moving: false,
            },
        ))
    };
    assert_eq!(
        answer(&mut app, transfer(&[".."])),
        PanelAnswer::Done(false)
    );
    assert!(app.shown_task.is_none() && !app.is_waiting());
    assert_eq!(
        answer(&mut app, transfer(&["up.txt"])),
        PanelAnswer::Done(true)
    );
    crate::app::waiting::finish_waiting(&mut app);
    let shown = app.shown_task.clone().expect("its progress in front");
    assert_eq!(app.tasks.len(), 1);
    let task = app.tasks.get(shown.id).expect("the task");
    assert_eq!(task.kind, crate::jobs::tasks::Kind::Copy);
    let target = task.target.display().to_string();
    assert!(
        target.contains("probe") && target.contains("one"),
        "{target}"
    );
}

#[test]
fn a_panel_showing_no_files_has_no_columns() {
    let (_temp, mut app) = app();
    app.panels[0].set_drawn_columns("N,S", &[20, 10]);
    let state = app.panel_state(crate::app::Side::Left, "");
    assert_eq!(
        (state.columns.as_str(), state.widths.as_str()),
        ("N,S", "20,10")
    );
    app.panels[0].visible = false;
    let state = app.panel_state(crate::app::Side::Left, "");
    assert_eq!((state.columns.as_str(), state.widths.as_str()), ("", ""));
}
