//! A selected folder that cannot be read again, and one whose listing the task's cancel stopped: neither
//! walked, the first a failure, the second the cancel (review R5, R15); a folder inside made a link
//! meanwhile, not walked (R7 review C8).

use super::*;

#[test]
fn a_folder_that_cannot_be_read_again_is_not_walked() {
    let root = tree::root("stat-fails");
    let folder = root.join("folder");
    tree::dir(&folder);
    let inside = file(&folder.join("inside.txt"));
    tree::fail_call("stat", &folder, io::ErrorKind::PermissionDenied, 1);
    let mut host = Script::answering(&[]);

    let report = run(&request(&[&folder], Mode::Remove), &mut host);

    assert!(exists(&inside), "nothing inside deleted");
    assert!(exists(&folder));
    assert_eq!(report.failures.len(), 1, "{report:?}");
}

#[test]
fn a_listing_the_cancel_stopped_ends_the_task_as_cancelled_with_no_question() {
    // Review of 19-2 (R15): the task's cancel stops the listing; that is the cancel, not a failure.
    let root = tree::root("listing-cancelled");
    let folder = root.join("folder");
    tree::dir(&folder);
    file(&folder.join("inside.txt"));
    tree::fail(&folder, io::ErrorKind::Interrupted, 1);
    let mut host = Script::answering(&[]);
    host.stop = Some(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
        true,
    )));

    let report = run(&request(&[&folder], Mode::Remove), &mut host);

    assert!(report.cancelled, "{report:?}");
    assert!(report.failures.is_empty(), "{report:?}");
    assert!(host.asked.is_empty());
    assert!(exists(&folder.join("inside.txt")));
}

/// R7 review C8: a folder inside, made a link after its folder was listed, is looked at again before it
/// is walked: removed as the link, never listed (what it leads to is not deleted).
#[test]
fn an_inner_folder_made_a_link_meanwhile_is_removed_as_the_link() {
    let root = tree::root("inner-link");
    let top = dir(&root.join("top"));
    // A sibling makes the question come; `sub` empty (the tree keeps a link's former items).
    let sub = dir(&top.join("sub"));
    file(&top.join("sibling.txt"));
    let mut request = request(&[&top], Mode::Remove);
    request.ask_folders = true;
    let mut host = Script::answering(&[Reply {
        answer: Answer::Delete,
        for_all: true,
    }]);
    let made_link = sub.clone();
    host.on_ask = Some(Box::new(move || {
        link(&made_link);
    }));

    run(&request, &mut host);

    let log = tree::log("inner-link");
    assert!(
        !log.iter()
            .any(|line| line.starts_with("list ") && line.ends_with("/top/sub")),
        "never walked into: {log:?}"
    );
    assert!(!exists(&top));
}
