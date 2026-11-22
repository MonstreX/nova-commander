use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;

use super::*;

/// A system whose one process has as many threads as it was read times; refusing while `refuse` is on.
struct Counting {
    reads: Arc<AtomicU32>,
    refuse: Arc<AtomicBool>,
}

impl Source for Counting {
    fn records(&self) -> io::Result<Vec<ProcessRecord>> {
        if self.refuse.load(Ordering::SeqCst) {
            return Err(io::Error::other("refused"));
        }
        let reads = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(vec![ProcessRecord {
            pid: 100,
            name: "a.exe".to_string(),
            threads: reads,
            ..ProcessRecord::default()
        }])
    }

    fn processors(&self) -> u64 {
        1
    }

    fn details(&self, _pid: u32) -> Option<ProcessDetails> {
        Some(ProcessDetails {
            path: Some(r"C:\a.exe".to_string()),
            ..ProcessDetails::default()
        })
    }

    fn description(&self, path: &str) -> Option<String> {
        Some(format!("described {path}"))
    }
}

fn threads(refresher: &Refresher) -> u32 {
    refresher
        .table()
        .process(100)
        .map_or(0, |process| process.threads)
}

fn counting() -> (Counting, Arc<AtomicU32>, Arc<AtomicBool>) {
    let (reads, refuse) = (
        Arc::new(AtomicU32::new(0)),
        Arc::new(AtomicBool::new(false)),
    );
    let source = Counting {
        reads: Arc::clone(&reads),
        refuse: Arc::clone(&refuse),
    };
    (source, reads, refuse)
}

#[test]
fn the_table_is_read_at_once_then_by_the_thread_and_on_demand() {
    let (source, reads, _) = counting();
    let (sender, changes) = mpsc::channel();
    let refresher = Refresher::start(Box::new(source), move || {
        let _ = sender.send(());
    });
    assert!(refresher.not_started.is_none());
    assert_eq!(threads(&refresher), 1, "read as it starts");
    refresher.read_now().unwrap();
    assert_eq!(threads(&refresher), 2, "read on demand");
    // Its first turn tells of the descriptions, the next of its read.
    for _ in 0..2 {
        changes
            .recv_timeout(EVERY * 5)
            .expect("the thread's turn tells of itself");
    }
    assert!(threads(&refresher) >= 3);
    drop(refresher);
    let after = reads.load(Ordering::SeqCst);
    while changes.try_recv().is_ok() {}
    assert_eq!(
        changes.try_recv(),
        Err(mpsc::TryRecvError::Disconnected),
        "the thread ended with the refresher, its callback gone"
    );
    assert_eq!(reads.load(Ordering::SeqCst), after, "no read after the end");
}

#[test]
fn a_refused_read_keeps_the_table() {
    let (source, _, refuse) = counting();
    let refresher = Refresher::start(Box::new(source), || {});
    assert_eq!(threads(&refresher), 1);
    refuse.store(true, Ordering::SeqCst);
    assert!(refresher.read_now().is_err());
    assert_eq!(threads(&refresher), 1, "the last read kept");
    assert!(refresher.table().read_at.is_some());
}

/// The files' descriptions come from the thread, never from the first read: none, then the file's.
#[test]
fn descriptions_are_read_by_the_thread() {
    let (source, _, _) = counting();
    let (sender, changes) = mpsc::channel();
    let refresher = Refresher::start(Box::new(source), move || {
        let _ = sender.send(());
    });
    let path = r"C:\a.exe";
    changes
        .recv_timeout(EVERY * 5)
        .expect("the thread's first turn");
    assert_eq!(
        refresher.description(path),
        Some(Some(format!("described {path}")))
    );
    assert_eq!(
        refresher.description(r"C:\b.exe"),
        None,
        "not a listed file"
    );
}
