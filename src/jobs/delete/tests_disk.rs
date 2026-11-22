//! The deletion engine on a real disk: temporary folders, junctions, read-only files, the job thread.

use super::*;
use std::fs;
use std::path::Path;

fn local_item(path: &Path) -> Item {
    let mut entry = None;
    let parent = VfsPath::local(path.parent().unwrap());
    let provider = crate::vfs::provider_for(&parent).unwrap();
    provider
        .list(
            &parent,
            &std::sync::atomic::AtomicBool::new(false),
            &mut |found| {
                if Some(found.name.as_str()) == path.file_name().and_then(|name| name.to_str()) {
                    entry = Some(found);
                }
            },
        )
        .unwrap();
    Item {
        path: VfsPath::local(path),
        entry: entry.unwrap(),
    }
}

fn junction(link: &Path, target: &Path) {
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap()
        .status;
    assert!(status.success());
}

#[test]
fn a_real_tree_with_a_junction_and_a_read_only_file_goes_and_the_target_stays() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("precious.txt"), "keep").unwrap();
    let doomed = temp.path().join("doomed");
    fs::create_dir_all(doomed.join("sub")).unwrap();
    fs::write(doomed.join("sub").join("file.txt"), "x").unwrap();
    let read_only = doomed.join("ro.txt");
    fs::write(&read_only, "x").unwrap();
    crate::platform::files::set_read_only(&read_only, true).unwrap();
    junction(&doomed.join("link"), &target);
    let request = Request {
        items: vec![local_item(&doomed)],
        mode: Mode::Remove,
        ask_folders: false,
        ask_read_only: true,
        count_totals: false,
    };
    let mut host = Script::answering(&[once(Answer::Delete)]);

    let report = run(&request, &mut host);

    assert_eq!(report, Report::default());
    assert!(matches!(&host.asked[..], [Question::ReadOnly { .. }]));
    assert!(!doomed.exists());
    assert_eq!(
        fs::read_to_string(target.join("precious.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn the_job_runs_on_its_own_thread_and_asks_through_the_ui() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("folder");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("inside.txt"), "x").unwrap();
    let items = vec![local_item(&folder)];
    // The count the question started goes on in the deletion.
    let counter = super::Counter::start(items.clone());
    let mut job = Job::start(
        Request {
            items,
            mode: Mode::Remove,
            ask_folders: true,
            ask_read_only: true,
            count_totals: true,
        },
        Some(counter),
        None,
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while job.question().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(matches!(job.question(), Some(Question::NotEmpty { .. })));
    job.answer(once(Answer::Delete));
    let report = loop {
        if let Some(report) = job.finished() {
            break report;
        }
        assert!(std::time::Instant::now() < deadline, "the job did not end");
        std::thread::sleep(std::time::Duration::from_millis(5));
    };

    assert_eq!(report, Report::default());
    assert!(!folder.exists());
    assert_eq!(job.progress().done, 2);
    // The count runs beside the deletion; it ends on its own.
    while !job.progress().totals_known {
        assert!(
            std::time::Instant::now() < deadline,
            "the count did not end"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        job.progress().total <= 2,
        "what was deleted first is not counted"
    );
}
