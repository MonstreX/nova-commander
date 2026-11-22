//! A stop while a plugin is busy with the search's file: no plugin after it is asked.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::{PluginOpener, answering, taker};
use crate::vfs::FileOpener;
use std::path::Path;

#[test]
fn a_stop_while_a_plugin_is_busy_asks_no_plugin_after_it() {
    let (busy, calls, _) = taker("Busy", "*.zip");
    let (next, next_calls, _) = taker("Next", "*.zip");
    let heard = answering(next_calls, Some("1"));
    let opener = PluginOpener {
        takers: vec![busy, next],
    };
    // The busy plugin takes the call and keeps it unanswered until the end.
    let held = std::thread::spawn(move || calls.recv().map(|call| (call.id, call.job)));
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = Arc::clone(&stop);
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        stopping.store(true, Ordering::Relaxed);
    });
    assert!(opener.open(Path::new(r"D:\a.zip"), &stop).is_none());
    stopper.join().expect("stopper");
    let kept = held.join().expect("plugin");
    drop(opener);
    assert!(
        heard.join().expect("next").is_empty(),
        "stopped: the next plugin is not asked"
    );
    drop(kept);
}
