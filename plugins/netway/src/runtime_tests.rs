use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn work_done_is_its_result() {
    let runtime = Runtime::new().unwrap();
    let done = runtime.wait(async { 6 * 7 }, &|| false);
    assert_eq!(done, Ok(42));
}

/// A stop drops work that never ends, within a few of the polls.
#[test]
fn a_stop_drops_the_work() {
    let runtime = Runtime::new().unwrap();
    let started = std::time::Instant::now();
    let asked = std::cell::Cell::new(0);
    let stopped = || {
        asked.set(asked.get() + 1);
        asked.get() > 2
    };
    let result = runtime.wait(std::future::pending::<()>(), &stopped);
    assert_eq!(result, Err(Stopped));
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// Work spawned goes on between waits, on the runtime's own threads.
#[test]
fn spawned_work_runs_without_a_wait() {
    let runtime = Runtime::new().unwrap();
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let _task = runtime.spawn(async move { flag.store(true, Ordering::SeqCst) });
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !ran.load(Ordering::SeqCst) {
        assert!(
            std::time::Instant::now() < deadline,
            "the spawned work never ran"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
