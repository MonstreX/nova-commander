//! The host's functions beyond the panels as a plugin calls them, without a library: what reaches the
//! program and what the plugin reads back; `synchro` from another thread.

use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use nc_plugin_api::{NcEntry, WHICH_PASSIVE};

use super::*;
use crate::plugin_host::log::PluginLog;
use crate::plugin_host::panel_requests::Which;

fn host_context() -> (super::super::HostContext, Receiver<Event>) {
    let (sender, events) = channel();
    let logs = std::env::current_exe().unwrap().join("Logs");
    let log = PluginLog::new(&logs, "probe");
    let context = super::super::HostContext::new("probe", sender, Path::new(r"C:\p"), log);
    context
        .started
        .store(true, std::sync::atomic::Ordering::Release);
    (context, events)
}

fn raw(context: &super::super::HostContext) -> *mut c_void {
    std::ptr::from_ref(context).cast_mut().cast()
}

/// The program: the next request taken and `answer` sent; the request handed back.
fn program(events: Receiver<Event>, answer: PanelAnswer) -> JoinHandle<PanelRequest> {
    std::thread::spawn(move || {
        let event = events.recv_timeout(Duration::from_secs(10)).unwrap();
        let Event::PanelAsk { request, reply, .. } = event else {
            panic!("a request: {event:?}");
        };
        reply.send(answer).unwrap();
        request
    })
}

/// A confirmation asked by its name: 1 or 0 as the program answers, -1 for a name it does not know.
#[test]
fn a_confirmation_is_answered_by_its_name() {
    for (answer, code) in [(Some(true), 1), (Some(false), 0), (None, -1)] {
        let (context, events) = host_context();
        let asked = program(events, PanelAnswer::Confirms(answer));
        // SAFETY: the context is alive; the text lives through the call.
        let got = unsafe { confirmation(raw(&context), NcStr::new("RemoveConnection")) };
        assert_eq!(got, code);
        assert_eq!(
            asked.join().unwrap(),
            PanelRequest::Confirmation("RemoveConnection".to_string())
        );
    }
}

/// Masks matched by the program: 1 or 0 as it answers, -1 for masks with errors.
#[test]
fn masks_are_matched_by_the_program() {
    for (answer, code) in [(Some(true), 1), (Some(false), 0), (None, -1)] {
        let (context, events) = host_context();
        let asked = program(events, PanelAnswer::Confirms(answer));
        // SAFETY: the context is alive; the texts live through the call.
        let got = unsafe { match_masks(raw(&context), NcStr::new("*.zip"), NcStr::new("a.zip")) };
        assert_eq!(got, code);
        let wanted = PanelRequest::MatchMasks {
            masks: "*.zip".to_string(),
            name: "a.zip".to_string(),
        };
        assert_eq!(asked.join().unwrap(), wanted);
    }
}

#[test]
fn keys_the_clipboard_and_help_reach_the_program_and_its_word_comes_back() {
    let (context, events) = host_context();
    let asked = program(events, PanelAnswer::Done(true));
    // SAFETY: the context is alive; the text lives through the call.
    let code = unsafe { post_keys(raw(&context), NcStr::new("Shift+F6")) };
    assert_eq!(code, 0);
    let wanted = PanelRequest::PostKeys("Shift+F6".to_string());
    assert_eq!(asked.join().unwrap(), wanted);
    let (context, events) = host_context();
    let asked = program(events, PanelAnswer::Done(false));
    // SAFETY: as above.
    let code = unsafe { show_help(raw(&context), NcStr::new("Contents")) };
    assert_eq!(code, -1, "the program shows none");
    assert_eq!(
        asked.join().unwrap(),
        PanelRequest::Help("Contents".to_string())
    );
    let (context, events) = host_context();
    let asked = program(events, PanelAnswer::Done(true));
    // SAFETY: as above.
    assert_eq!(unsafe { clipboard(raw(&context), NcStr::new("x")) }, 0);
    assert_eq!(
        asked.join().unwrap(),
        PanelRequest::Clipboard("x".to_string())
    );
}

#[test]
fn the_screen_and_the_filter_are_answered_into_the_plugins_places() {
    let (context, events) = host_context();
    let screen_answer = PanelAnswer::Screen {
        columns: 100,
        rows: 30,
    };
    let asked = program(events, screen_answer);
    let mut screen_out = NcScreen {
        columns: 0,
        rows: 0,
        window: 0,
    };
    // SAFETY: the context is alive; the place lives through the call.
    assert_eq!(unsafe { screen(raw(&context), &raw mut screen_out) }, 0);
    assert_eq!((screen_out.columns, screen_out.rows), (100, 30));
    assert_eq!(asked.join().unwrap(), PanelRequest::Screen);

    let (context, events) = host_context();
    let asked = program(events, PanelAnswer::Matches(vec![false, true]));
    let entries = [
        NcEntry {
            name: NcStr::new("a"),
            attributes: 0x22,
            ..NcEntry::EMPTY
        },
        NcEntry {
            name: NcStr::new("b"),
            ..NcEntry::EMPTY
        },
    ];
    let mut results = [9u8; 2];
    // SAFETY: the entries and the results live through the call.
    let code = unsafe {
        filter(
            raw(&context),
            WHICH_PASSIVE,
            entries.as_ptr(),
            2,
            results.as_mut_ptr(),
        )
    };
    assert_eq!((code, results), (0, [0, 1]));
    let PanelRequest::Filter { which, entries } = asked.join().unwrap() else {
        panic!("a filter's request");
    };
    assert_eq!(which, Which::Passive);
    assert_eq!(entries[0].name, "a");
    assert!(entries[0].attributes.is_hidden(), "its own bits");
}

/// `filter` of `count` entries at `entries` into `results`, the program's answer `matches` (none asked:
/// none sent).
fn filtered(
    entries: *const NcEntry,
    count: usize,
    results: *mut u8,
    matches: Option<Vec<bool>>,
) -> i32 {
    let (context, events) = host_context();
    let program = matches.map(|matches| program(events, PanelAnswer::Matches(matches)));
    // SAFETY: the context is alive; the places are the caller's, checked by the host.
    let code = unsafe { filter(raw(&context), WHICH_PASSIVE, entries, count, results) };
    if let Some(program) = program {
        program.join().unwrap();
    }
    code
}

#[test]
fn a_filter_refuses_missing_places_a_batch_past_the_most_and_an_answer_of_another_length() {
    let mut results = [9u8; 3];
    let null_entries = filtered(std::ptr::null(), 3, results.as_mut_ptr(), None);
    assert_eq!(null_entries, -1, "no entries read from null");
    let entries = [NcEntry::EMPTY; 2];
    let null_results = filtered(entries.as_ptr(), 2, std::ptr::null_mut(), None);
    assert_eq!(null_results, -1);
    let too_many = super::super::calls::MOST_VALUES + 1;
    assert_eq!(
        filtered(entries.as_ptr(), too_many, results.as_mut_ptr(), None),
        -1,
        "past the most read at once: asked in batches"
    );
    let short = filtered(entries.as_ptr(), 2, results.as_mut_ptr(), Some(vec![true]));
    assert_eq!(short, -1, "an answer of another length");
    assert_eq!(results, [9; 3], "nothing written");
    assert_eq!(
        filtered(std::ptr::null(), 0, std::ptr::null_mut(), Some(Vec::new())),
        0
    );
}

#[test]
fn a_synchro_asked_from_another_thread_reaches_the_program() {
    let (context, events) = host_context();
    let address = raw(&context) as usize;
    std::thread::spawn(move || {
        // SAFETY: the context outlives the thread (joined below); `synchro` is any thread's.
        unsafe { synchro(address as *mut c_void, 42) };
    })
    .join()
    .unwrap();
    let event = events.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(
        matches!(&event, Event::Synchro { plugin, param: 42 } if plugin == "probe"),
        "{event:?}"
    );
}

/// R7 review A5: a transfer with no names but a count is refused, nothing asked of the program; its
/// names otherwise reach it.
#[test]
fn a_transfer_with_null_names_and_a_count_is_refused() {
    let (context, events) = host_context();
    let none = NcStr::new("");
    // SAFETY: the context is alive; a null `names` is what is checked.
    let code = unsafe { transfer(raw(&context), none, none, std::ptr::null(), 3, none, 0) };
    assert_eq!(code, -1);
    assert!(events.try_recv().is_err(), "nothing asked");
    let asked = program(events, PanelAnswer::Done(true));
    let names = [NcStr::new("a.txt")];
    // SAFETY: one name, alive through the call.
    let code = unsafe { transfer(raw(&context), none, none, names.as_ptr(), 1, none, 0) };
    assert_eq!(code, 0);
    let PanelRequest::Transfer(request) = asked.join().unwrap() else {
        panic!("a transfer");
    };
    assert_eq!(request.names, ["a.txt"]);
}
