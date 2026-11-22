use std::sync::mpsc;

use super::*;

/// A call that answers within the wait gives its answer; `late` hears nothing.
#[test]
fn an_answer_in_time_is_the_callers() {
    let (heard, late) = mpsc::channel();
    let answer = waited(
        Duration::from_secs(30),
        || 7,
        move |value| {
            let _ = heard.send(value);
        },
    );
    assert_eq!(answer, Waited::Answered(7));
    assert!(late.try_recv().is_err(), "nothing late");
}

/// A call held past the wait: late for the caller, its answer to `late` once it comes; the library kept.
#[test]
fn an_answer_past_the_wait_is_the_late_ones() {
    let (release, held) = mpsc::channel::<()>();
    let (heard, late) = mpsc::channel();
    let answer = waited(
        Duration::ZERO,
        move || {
            held.recv().ok();
            "late"
        },
        move |value| {
            let _ = heard.send(value);
        },
    );
    assert_eq!(answer, Waited::Late);
    release.send(()).unwrap();
    assert_eq!(late.recv_timeout(Duration::from_secs(30)), Ok("late"));
    assert!(keep_library_loaded());
}

/// A call that breaks (a panic) fails at once, not after the whole wait; `late` hears nothing.
#[test]
fn a_broken_call_is_none_at_once() {
    let (heard, late) = mpsc::channel::<u8>();
    let answer = waited(
        Duration::from_secs(600),
        || -> u8 { panic!("broken on purpose") },
        move |value| {
            // The test's channel: a send to a dropped receiver is the test's failure anyway.
            let _ = heard.send(value);
        },
    );
    assert_eq!(answer, Waited::Failed);
    assert!(late.try_recv().is_err());
}
