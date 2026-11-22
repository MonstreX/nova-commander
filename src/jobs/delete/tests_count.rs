//! The count of what a deletion takes: FAR's `calculate_total`, and the counter beside the question
//! and the deletion.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::super::counter::count;
use super::*;

#[test]
fn counted_totals_are_the_selection_and_everything_inside_but_links() {
    let (folder, _, _) = folder_tree("totals");
    link(&folder.join("link"));
    tree::sized(&folder.join("big.bin"), 5000);
    let request = request(&[&folder], Mode::Remove);
    let mut published = Vec::new();

    count(
        &request.items,
        &AtomicBool::new(false),
        &mut |total, known| {
            published.push((total, known));
        },
    );

    // Folders: folder, inner, the link (a link to a folder looks like one); files: a.txt, big.bin,
    // deep.txt; bytes: big.bin's.
    let (tally, known) = *published.last().unwrap();
    assert_eq!(
        tally,
        Tally {
            files: 3,
            folders: 3,
            bytes: 5000
        }
    );
    assert!(known);
    assert_eq!(tally.objects(), 6);
    assert!(!tree::log("totals").contains(&"list tree://totals/folder/link".to_string()));
}

#[test]
fn a_stopped_count_reads_and_publishes_nothing_more() {
    let root = tree::root("count-stop");
    let folder = dir(&root.join("folder"));
    for index in 0..200 {
        dir(&folder.join(&format!("d{index}")));
    }
    let request = request(&[&folder], Mode::Remove);

    let stopped = AtomicBool::new(true);
    let mut published = Vec::new();
    count(&request.items, &stopped, &mut |tally, known| {
        published.push((tally, known));
    });
    assert!(published.is_empty());
    assert!(tree::log("count-stop").is_empty(), "nothing read");

    // Stopped on the way (at its first publication): no more publications, the end never comes.
    let stop = AtomicBool::new(false);
    let mut published = Vec::new();
    count(&request.items, &stop, &mut |tally, known| {
        published.push((tally, known));
        stop.store(true, Ordering::Relaxed);
    });
    assert_eq!(published.len(), 1);
    assert!(!published[0].1, "never known");
}

#[test]
fn a_counter_counts_on_its_own_thread_and_says_when_it_is_over() {
    let (folder, _, _) = folder_tree("counter");
    let request = request(&[&folder], Mode::Remove);
    let counter = Counter::start(request.items.clone());
    let last = wait_known(&counter);
    assert_eq!(
        last.0,
        Tally {
            files: 2,
            folders: 2,
            bytes: 0
        }
    );
    assert_eq!(counter.now(), last, "nothing changes after the end");
}

fn wait_known(counter: &Counter) -> (Tally, bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last = (Tally::default(), false);
    while !last.1 {
        assert!(Instant::now() < deadline, "the count did not end");
        last = counter.wait(last, Duration::from_millis(50));
    }
    last
}

fn wait_end(job: &mut Job) -> Report {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(report) = job.finished() {
            return report;
        }
        assert!(Instant::now() < deadline, "the job did not end");
        std::thread::yield_now();
    }
}

#[test]
fn the_deletion_goes_on_with_the_count_it_is_given() {
    let (folder, _, _) = folder_tree("counted-once");
    let mut request = request(&[&folder], Mode::Remove);
    request.ask_folders = false;
    request.count_totals = true;
    let counter = Counter::start(request.items.clone());
    wait_known(&counter);

    let mut job = Job::start(request, Some(counter), None);
    assert_eq!(wait_end(&mut job), Report::default());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !job.progress().totals_known {
        assert!(Instant::now() < deadline, "the total did not come");
        std::thread::yield_now();
    }

    assert_eq!(job.progress().total, 4);
    let folder_listed = tree::log("counted-once")
        .iter()
        .filter(|line| *line == "list tree://counted-once/folder")
        .count();
    assert_eq!(folder_listed, 2, "once by the count, once by the deletion");
}

#[test]
fn to_the_recycle_bin_the_count_ends_before_anything_moves() {
    let root = tree::root("count-then-bin");
    let folder = dir(&root.join("folder"));
    for index in 0..3000 {
        dir(&folder.join(&format!("d{index}")));
    }
    let mut request = request(&[&folder], Mode::Trash);
    request.ask_folders = false;
    request.count_totals = true;
    // Still counting when the deletion starts.
    let counter = Counter::start(request.items.clone());

    let mut job = Job::start(request, Some(counter), None);
    assert_eq!(wait_end(&mut job), Report::default());

    let log = tree::log("count-then-bin");
    let moved = log
        .iter()
        .position(|line| line.starts_with("trash"))
        .unwrap();
    assert!(
        !log[moved..].iter().any(|line| line.starts_with("list")),
        "no folder is read once the bin starts: {:?}",
        &log[moved..]
    );
    let progress = job.progress();
    assert_eq!(
        (progress.done, progress.total, progress.totals_known),
        (1, 1, true),
        "the bin takes the selected object in one step"
    );
}
