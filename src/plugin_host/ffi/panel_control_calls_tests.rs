use super::*;

#[test]
fn a_state_and_an_item_carry_their_flags_and_texts() {
    let state = PanelState {
        left: true,
        active: true,
        plugin: Some("tmppanel".to_string()),
        call_target: true,
        reading: true,
        kind: PanelKind::Info,
        folder: "/one".to_string(),
        disk_folder: r"C:\work".to_string(),
        items: 3,
        current: 2,
        ..PanelState::default()
    };
    let raw = state_of(&state);
    assert_eq!(
        raw.flags,
        STATE_LEFT | STATE_ACTIVE | STATE_PLUGIN | STATE_CALL_TARGET | STATE_READING
    );
    assert_eq!(raw.kind, KIND_INFO);
    // SAFETY: the texts point into `state`, alive here.
    unsafe {
        assert_eq!(raw.folder.as_str(), "/one");
        assert_eq!(raw.plugin.as_str(), "tmppanel");
        assert_eq!(raw.disk_folder.as_str(), r"C:\work");
    }
    assert_eq!((raw.items, raw.current), (3, 2));

    let item = PanelItem {
        name: "a.txt".to_string(),
        selected: true,
        folder: true,
        path: Some(r"C:\work\a.txt".to_string()),
        modified: Some(std::time::UNIX_EPOCH + std::time::Duration::from_millis(1500)),
        ..PanelItem::default()
    };
    let raw = item_of(&item);
    assert_eq!(raw.flags, ITEM_SELECTED | ITEM_FOLDER);
    assert_eq!(raw.modified, 1500);
    assert_eq!(raw.created, nc_plugin_api::TIME_NONE);
    // SAFETY: as above.
    unsafe { assert_eq!(raw.path.as_str(), r"C:\work\a.txt") };
}

#[test]
fn an_action_is_read_with_its_arguments() {
    let names = [NcStr::new("a.txt"), NcStr::new("b.txt")];
    let mut raw = NcPanelAction::new(DO_SELECT);
    raw.flags = DO_OFF;
    raw.names = names.as_ptr();
    raw.name_count = names.len();
    // SAFETY: the names live through the call.
    let action = unsafe { action_of(&raw) };
    let names = vec!["a.txt".to_string(), "b.txt".to_string()];
    let wanted = PanelAction::Select {
        names,
        on: false,
        all: false,
    };
    assert_eq!(action, Some(wanted));

    let mut raw = NcPanelAction::new(DO_SET_CURSOR);
    raw.number = 4;
    raw.number2 = usize::MAX;
    // SAFETY: no texts.
    let action = unsafe { action_of(&raw) };
    let wanted = PanelAction::SetCursor {
        index: 4,
        name: None,
        top: None,
    };
    assert_eq!(action, Some(wanted));

    let folder = NcStr::new(r"C:\work");
    let mut raw = NcPanelAction::new(DO_SET_FOLDER);
    raw.text = folder;
    // SAFETY: the text lives through the call.
    let action = unsafe { action_of(&raw) };
    let wanted = PanelAction::SetFolder {
        folder: r"C:\work".to_string(),
        cursor: None,
    };
    assert_eq!(action, Some(wanted));
    // SAFETY: no texts.
    let unknown = unsafe { action_of(&NcPanelAction::new(99)) };
    assert_eq!(unknown, None, "not known");
}

#[test]
fn a_panel_is_named_by_its_place_or_its_focus() {
    assert_eq!(which(WHICH_ACTIVE), Some(Which::Active));
    assert_eq!(which(WHICH_PASSIVE), Some(Which::Passive));
    assert_eq!(which(WHICH_LEFT), Some(Which::Left));
    assert_eq!(which(WHICH_RIGHT), Some(Which::Right));
    assert_eq!(which(7), None);
}

/// A plugin's context, started, and where its events come.
fn started_context() -> (
    tempfile::TempDir,
    HostContext,
    std::sync::mpsc::Receiver<Event>,
) {
    let logs = tempfile::tempdir().unwrap();
    let (sender, events) = std::sync::mpsc::channel();
    let log = crate::plugin_host::log::PluginLog::new(logs.path(), "probe");
    let context = HostContext::new("probe", sender, std::path::Path::new(r"C:\probe"), log);
    context
        .started
        .store(true, std::sync::atomic::Ordering::Release);
    (logs, context, events)
}

fn raw(context: &HostContext) -> *mut c_void {
    std::ptr::from_ref(context).cast_mut().cast()
}

/// The program: each request answered with the next of `answers`, the requests handed back.
fn program(
    events: std::sync::mpsc::Receiver<Event>,
    answers: Vec<PanelAnswer>,
) -> std::thread::JoinHandle<Vec<PanelRequest>> {
    std::thread::spawn(move || {
        let mut heard = Vec::new();
        for answer in answers {
            let event = events
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("a request");
            let Event::PanelAsk { request, reply, .. } = event else {
                panic!("a panel's request: {event:?}");
            };
            heard.push(request);
            reply.send(answer).unwrap();
        }
        heard
    })
}

fn named(names: &[&str]) -> PanelAnswer {
    let item = |name: &&str| PanelItem {
        name: (*name).to_string(),
        ..PanelItem::default()
    };
    PanelAnswer::Items(names.iter().map(item).collect())
}

#[test]
fn a_count_reads_the_items_anew_and_the_reads_after_it_take_them() {
    let (_logs, context, events) = started_context();
    let answers = vec![named(&["..", "a"]), named(&["..", "a", "b"])];
    let program = program(events, answers);
    let raw = raw(&context);
    let mut items = [NcPanelItem::EMPTY; 3];
    // SAFETY: the test's own context; room for the items asked.
    unsafe {
        assert_eq!(
            panel_items(raw, WHICH_ACTIVE, 0, 0, std::ptr::null_mut(), 0),
            2
        );
        assert_eq!(
            panel_items(raw, WHICH_ACTIVE, 0, 0, items.as_mut_ptr(), 2),
            2
        );
        assert_eq!(items[1].name.as_str(), "a");
        // The plugin's next call (the user changed the panel meanwhile): read again.
        assert_eq!(
            panel_items(raw, WHICH_ACTIVE, 0, 0, std::ptr::null_mut(), 0),
            3
        );
        assert_eq!(
            panel_items(raw, WHICH_ACTIVE, 0, 1, items.as_mut_ptr(), 3),
            3
        );
        assert_eq!(items[1].name.as_str(), "b", "from the second item");
    }
    assert_eq!(
        program.join().unwrap().len(),
        2,
        "asked twice, read four times"
    );
}

#[test]
fn a_panel_action_for_no_panel_fails_without_asking() {
    let (_logs, context, events) = started_context();
    let raw = raw(&context);
    let action = NcPanelAction::new(DO_REREAD);
    // SAFETY: the test's own context; the action lives through the call.
    let answer = unsafe { panel_do(raw, 9, &raw const action, std::ptr::null_mut()) };
    assert_eq!(answer, -1);
    assert!(events.try_recv().is_err(), "nothing asked");
}

#[test]
fn in_a_panels_call_the_state_is_the_one_last_published_and_nothing_else_answers() {
    let (_logs, context, events) = started_context();
    let left = PanelState {
        left: true,
        folder: r"C:\left".to_string(),
        ..PanelState::default()
    };
    let right = PanelState {
        active: true,
        folder: r"C:\right".to_string(),
        ..PanelState::default()
    };
    *context.panels_seen.lock().unwrap() = [left, right];
    context.quiet.set(true);
    let raw = raw(&context);
    let mut state = NcPanelState::EMPTY;
    let action = NcPanelAction::new(DO_GET_COMMAND_LINE);
    // SAFETY: the test's own context; the outputs live through the calls.
    unsafe {
        assert_eq!(panel_state(raw, WHICH_ACTIVE, &raw mut state), 0);
        assert_eq!(state.folder.as_str(), r"C:\right", "the active one");
        assert_eq!(panel_state(raw, WHICH_PASSIVE, &raw mut state), 0);
        assert_eq!(state.folder.as_str(), r"C:\left");
        assert_eq!(
            panel_items(raw, WHICH_ACTIVE, 0, 0, std::ptr::null_mut(), 0),
            -1
        );
        assert_eq!(
            panel_do(raw, WHICH_ACTIVE, &raw const action, std::ptr::null_mut()),
            -1
        );
    }
    assert!(events.try_recv().is_err(), "nothing asked of the program");
}
