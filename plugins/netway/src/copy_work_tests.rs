use super::*;

/// WinSCP's batch modes: all, none, and a newer source only.
#[test]
fn the_batch_answers_decide_as_winscps() {
    let old = Side {
        size: 1,
        modified: Some(10),
    };
    let new = Side {
        size: 1,
        modified: Some(20),
    };
    assert_eq!(batch_write(Batch::All, old, new), Write::Overwrite);
    assert_eq!(batch_write(Batch::None, new, old), Write::Skip);
    assert_eq!(batch_write(Batch::Older, new, old), Write::Overwrite);
    assert_eq!(batch_write(Batch::Older, old, new), Write::Skip);
    let unknown = Side {
        size: 1,
        modified: None,
    };
    assert!(!is_newer(new, unknown), "times not known: not newer");
}

/// A retried file's bytes are taken back from the whole's; the files done counted.
#[test]
fn a_restarted_file_is_counted_once() {
    let progress = Progress::default();
    progress.walked(100, 2, true);
    progress.file("a", "b", 50);
    progress.add(30);
    progress.restart_file();
    progress.add(50);
    progress.file_done();
    let seen = progress.snapshot();
    assert_eq!((seen.done, seen.file_done, seen.total), (50, 50, 100));
    assert_eq!((seen.files, seen.files_done), (2, 1));
    assert!(seen.total_known && !seen.started.is_empty());
    progress.walked(0, 2, false);
    assert!(!progress.snapshot().total_known, "not calculated");
}
