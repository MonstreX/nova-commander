//! The plugins' opener for the search against stand-ins for the plugins' threads: the masks, a plugin
//! that passes and the next one asked, the panel closed once the search is done with it, a stop while a
//! plugin is busy, a plugin gone.

use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use super::*;
use crate::plugin_host::calls::{Call, CallState};

/// A stand-in plugin `name` taking `mask`: its taker, the jobs it is sent, its calls' state.
fn taker(name: &str, mask: &str) -> (Taker, Receiver<Call>, Arc<CallState>) {
    let (sender, calls) = channel::<Call>();
    let state = Arc::new(CallState::default());
    let jobs = Arc::new(Calls::new(sender, Arc::clone(&state)));
    let masks = vec![FileMask::parse(mask).expect("a mask")];
    let taker = Taker {
        name: name.to_string(),
        masks,
        jobs,
    };
    (taker, calls, state)
}

/// Answers each search's open with `instance` (none: passes), and tells the jobs it heard.
fn answering(
    calls: Receiver<Call>,
    instance: Option<&str>,
) -> std::thread::JoinHandle<Vec<String>> {
    let instance = instance.map(str::to_string);
    std::thread::spawn(move || {
        let mut heard = Vec::new();
        while let Ok(call) = calls.recv() {
            heard.push(format!("{:?}", call.job));
            if let Job::SearchOpen { reply, .. } = call.job {
                let _ = reply.send(instance.clone());
            }
        }
        heard
    })
}

#[test]
fn the_first_plugin_whose_masks_take_the_file_opens_it_and_closes_it_after() {
    let (passing, passing_calls, _) = taker("Lister", "*.zip");
    let (other, other_calls, _) = taker("Other", "*.txt");
    let (archive, archive_calls, _) = taker("Archive", "*.zip,*.7z");
    let opener = PluginOpener {
        takers: vec![passing, other, archive],
    };
    let passing_heard = answering(passing_calls, None);
    let other_heard = answering(other_calls, Some("9"));
    let archive_heard = answering(archive_calls, Some("4"));
    let stop = AtomicBool::new(false);
    let opened = opener
        .open(Path::new(r"D:\x\a.zip"), &stop)
        .expect("opened");
    assert_eq!(opened.root(), &VfsPath::in_session("archive", "4", "/"));
    assert_eq!(opened.provider(), "archive");
    assert!(
        opener.open(Path::new(r"D:\x\notes.doc"), &stop).is_none(),
        "no plugin's masks take it"
    );
    drop(opened);
    drop(opener);
    let open = r"SearchOpen(D:\x\a.zip)".to_string();
    assert_eq!(
        passing_heard.join().expect("lister"),
        std::slice::from_ref(&open)
    );
    assert_eq!(
        other_heard.join().expect("other"),
        Vec::<String>::new(),
        "its masks do not take the file"
    );
    assert_eq!(
        archive_heard.join().expect("archive"),
        [open, "Close(4)".to_string()],
        "closed once the search is done with it"
    );
}

#[test]
fn a_stop_ends_the_wait_and_the_plugin_hears_it() {
    let (busy, calls, state) = taker("Archive", "*");
    let opener = PluginOpener { takers: vec![busy] };
    // The plugin takes the call and does not answer while the search is stopped.
    let held = std::thread::spawn(move || calls.recv().map(|call| (call.id, call.job)));
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = Arc::clone(&stop);
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        stopping.store(true, Ordering::Relaxed);
    });
    let since = Instant::now();
    assert!(opener.open(Path::new(r"D:\a.zip"), &stop).is_none());
    assert!(since.elapsed() < Duration::from_secs(5), "not waited for");
    stopper.join().expect("stopper");
    let (id, job) = held.join().expect("plugin").expect("the call came");
    state.begin(id);
    assert!(state.cancelled(), "the plugin hears the stop");
    drop(job);
}

#[test]
fn a_panel_opened_after_the_search_stopped_waiting_is_closed() {
    let (reply, answer) = channel();
    drop(answer);
    let mut closed = Vec::new();
    hand_over(Some("7".to_string()), &reply, |instance| {
        closed.push(instance.to_string());
    });
    assert_eq!(closed, ["7"], "nobody took it: closed");
    let (reply, answer) = channel();
    hand_over(Some("8".to_string()), &reply, |_| panic!("taken"));
    assert_eq!(answer.recv().unwrap().as_deref(), Some("8"));
    drop(answer);
    hand_over(None, &reply, |_| panic!("nothing opened"));
}

#[test]
fn a_plugin_gone_gives_way_to_the_next() {
    let (gone, calls, _) = taker("Gone", "*.zip");
    drop(calls);
    let (archive, archive_calls, _) = taker("Archive", "*.zip");
    let heard = answering(archive_calls, Some("1"));
    let opener = PluginOpener {
        takers: vec![gone, archive],
    };
    let stop = AtomicBool::new(false);
    let opened = opener.open(Path::new(r"D:\a.zip"), &stop);
    assert_eq!(
        opened.as_ref().map(OpenedFile::root),
        Some(&VfsPath::in_session("archive", "1", "/"))
    );
    drop(opened);
    drop(opener);
    assert_eq!(heard.join().expect("archive").len(), 2, "opened, closed");
}

/// R7 review A4: a panel named with a `\` is not opened as a path (R47's check of the commands), and
/// the plugin closes it.
#[test]
fn a_panel_named_with_a_slash_is_refused_and_closed() {
    let (bad, calls, _) = taker("Archive", "*.zip");
    let opener = PluginOpener { takers: vec![bad] };
    let heard = answering(calls, Some(r"..\4"));
    let stop = AtomicBool::new(false);
    assert!(opener.open(Path::new(r"D:\x\a.zip"), &stop).is_none());
    drop(opener);
    let heard = heard.join().expect("archive");
    assert_eq!(
        heard.last().map(String::as_str),
        Some(r"Close(..\4)"),
        "{heard:?}"
    );
}

#[path = "search_open_stop_tests.rs"]
mod stop;
