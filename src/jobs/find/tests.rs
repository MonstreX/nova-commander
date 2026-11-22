//! The walk of FAR's `ScanTree` and what `IsFileIncluded` takes, on a tree in memory.

use super::*;
use crate::vfs::filter::{Conditions, Mark, Marked};
use crate::vfs::test_tree as tree;
use std::time::Instant;

pub(super) fn request(root: &VfsPath, mask: &str) -> Request {
    Request {
        roots: vec![root.clone()],
        drives: None,
        recurse: true,
        mask: FileMask::parse(mask).unwrap(),
        folders: true,
        links: true,
        pattern: None,
        not_containing: false,
        first: 0,
        streams: false,
        filter: None,
        archives: None,
    }
}

/// Everything found, as `folder|name`, once the search is over.
fn run(request: Request) -> Vec<String> {
    let search = Search::start(request);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut found = Vec::new();
    loop {
        let over = search.over();
        found.extend(search.take_found());
        if over {
            break;
        }
        assert!(Instant::now() < deadline, "the search did not end");
        std::thread::sleep(Duration::from_millis(2));
    }
    let root = tree::root("x").to_string();
    (found.iter())
        .map(|item| {
            let folder = item.folder.to_string();
            let folder = folder.strip_prefix(&root).unwrap_or(&folder).to_string();
            format!("{folder}|{}", item.entry.name)
        })
        .collect()
}

fn sample(session: &str) -> VfsPath {
    let root = tree::root(session);
    tree::write(&root.join("b.txt"), b"hello world");
    tree::dir(&root.join("a"));
    tree::write(&root.join("a").join("c.txt"), b"nothing here");
    tree::dir(&root.join("a").join("deep"));
    tree::write(&root.join("a").join("deep").join("d.rs"), b"fn hello()");
    tree::dir(&root.join("z"));
    tree::write(&root.join("z").join("e.txt"), b"HELLO");
    root
}

pub(super) fn names(found: &[String]) -> Vec<&str> {
    found
        .iter()
        .map(|line| line.rsplit('|').next().unwrap())
        .collect()
}

#[test]
fn files_come_first_then_each_folder_with_what_is_in_it() {
    let root = sample("find-order");
    let found = run(request(&root, "*"));
    assert_eq!(
        names(&found),
        ["b.txt", "a", "c.txt", "deep", "d.rs", "z", "e.txt"]
    );
}

#[test]
fn the_mask_takes_names_and_folders_only_when_asked() {
    let root = sample("find-mask");
    let found = run(request(&root, "*.txt"));
    assert_eq!(names(&found), ["b.txt", "c.txt", "e.txt"]);
    let no_folders = Request {
        folders: false,
        ..request(&root, "*")
    };
    assert!(!names(&run(no_folders)).contains(&"a"));
}

#[test]
fn the_current_folder_only_is_not_walked_into() {
    let root = sample("find-flat");
    let flat = Request {
        recurse: false,
        ..request(&root, "*")
    };
    assert_eq!(names(&run(flat)), ["b.txt", "a", "z"]);
}

#[test]
fn a_text_takes_the_files_with_it_and_never_folders() {
    let root = sample("find-text");
    let with_text = Request {
        pattern: Some(text::Pattern::Text(text::Query {
            text: "hello".to_string(),
            case_sensitive: false,
            whole_words: false,
            fuzzy: false,
            code_pages: text::CodePages::Auto,
        })),
        ..request(&root, "*")
    };
    assert_eq!(names(&run(with_text.clone())), ["b.txt", "d.rs", "e.txt"]);
    let without = Request {
        not_containing: true,
        ..with_text
    };
    assert_eq!(names(&run(without)), ["c.txt"]);
}

#[test]
fn links_to_folders_are_walked_into_only_when_asked() {
    let root = tree::root("find-links");
    tree::link(&root.join("elsewhere"));
    tree::write(&root.join("f.txt"), b"");
    let no_links = Request {
        links: false,
        ..request(&root, "*")
    };
    assert_eq!(names(&run(no_links)), ["f.txt", "elsewhere"]);
    let listed = |session| {
        tree::log(session)
            .iter()
            .any(|line| line.contains("elsewhere"))
    };
    assert!(!listed("find-links"), "{:?}", tree::log("find-links"));
    run(request(&root, "*"));
    assert!(listed("find-links"), "{:?}", tree::log("find-links"));
}

fn junction(link: &std::path::Path, target: &std::path::Path) {
    let done = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(done.status.success());
}

#[test]
fn a_link_back_up_is_not_walked_into_and_one_elsewhere_is() {
    let temp = tempfile::tempdir().unwrap();
    let top = temp.path().join("top");
    let other = temp.path().join("other");
    std::fs::create_dir_all(&top).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(top.join("a.txt"), "a").unwrap();
    std::fs::write(other.join("b.txt"), "b").unwrap();
    junction(&top.join("back"), &top);
    junction(&top.join("elsewhere"), &other);
    junction(&other.join("up"), &top);
    let found = run(Request {
        folders: false,
        ..request(&VfsPath::local(&top), "*")
    });
    let mut names = names(&found);
    names.sort_unstable();
    assert_eq!(names, ["a.txt", "b.txt"], "{found:?}");
}

#[test]
fn every_root_is_walked_in_turn() {
    let root = sample("find-roots");
    let roots = Request {
        roots: vec![root.join("z"), root.join("a").join("deep")],
        ..request(&root, "*")
    };
    assert_eq!(names(&run(roots)), ["e.txt", "d.rs"]);
}

#[test]
fn a_stopped_search_is_over() {
    let search = Search::start(Request {
        roots: vec![crate::vfs::testing::endless()],
        ..request(&tree::root("find-stop"), "*")
    });
    search.stop();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !search.over() {
        assert!(Instant::now() < deadline, "the search did not stop");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_folders_stream_is_a_file() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("box");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(format!("{}:notes", folder.display()), "n").unwrap();
    let search = Search::start(Request {
        streams: true,
        recurse: false,
        ..request(&VfsPath::local(temp.path()), "*")
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !search.over() {
        assert!(Instant::now() < deadline, "the search did not end");
        std::thread::sleep(Duration::from_millis(2));
    }
    let found = search.take_found();
    let stream = found.iter().find(|item| item.entry.name == "box:notes");
    let stream = stream.unwrap_or_else(|| panic!("{found:?}"));
    assert!(!stream.entry.is_dir && !stream.entry.attributes.is_directory());
    assert_eq!(stream.entry.size, 1);
}

/// A filter of the user's with this mark taking the names `mask` takes.
fn marked(mark: Mark, mask: &str) -> Marked {
    Marked {
        mark,
        conditions: Conditions {
            mask: FileMask::parse(mask),
            ..Conditions::default()
        },
    }
}

fn filtered(root: &VfsPath, filters: Vec<Marked>) -> Vec<String> {
    let filter = AreaFilter {
        filters,
        ..AreaFilter::default()
    };
    run(Request {
        filter: Some((filter, SystemTime::now())),
        ..request(root, "*")
    })
}

#[test]
fn a_folder_the_filter_leaves_out_is_not_walked_into() {
    let root = sample("find-filter-exclude");
    let found = filtered(&root, vec![marked(Mark::Exclude, "a")]);
    assert_eq!(names(&found), ["b.txt", "z", "e.txt"]);
}

#[test]
fn with_an_including_filter_what_none_takes_is_left_out_and_folders_still_walked() {
    let root = sample("find-filter-include");
    let found = filtered(&root, vec![marked(Mark::Include, "*.rs")]);
    assert_eq!(names(&found), ["d.rs"], "folders walked, not listed");
    let found = filtered(&root, vec![marked(Mark::Exclude, "*.txt")]);
    assert_eq!(
        names(&found),
        ["a", "deep", "d.rs", "z"],
        "only excluding: the rest stays"
    );
}

#[test]
fn folders_no_filter_takes_are_walked_but_not_found_while_a_filter_includes_files() {
    let root = sample("find-filter-files");
    // Ins's new filter: files only (FAR's `SetAttr(true, 0, FILE_ATTRIBUTE_DIRECTORY)`).
    let files_only = Marked {
        conditions: Conditions {
            attributes: Some((0, crate::vfs::filter::DIRECTORY)),
            ..marked(Mark::Include, "*.rs").conditions
        },
        ..marked(Mark::Include, "*.rs")
    };
    let filter = AreaFilter {
        filters: vec![files_only],
        ..AreaFilter::default()
    };
    let folder = Entry {
        name: "a".to_string(),
        is_dir: true,
        attributes: crate::platform::files::Attributes::from_windows_bits(0x10),
        ..Entry::default()
    };
    let now = SystemTime::now();
    assert!(filter.takes(&folder, now), "a panel shows the folder");
    assert!(!filter.verdict(&folder, now).lists(), "Find file does not");
    let found = filtered(&root, filter.filters.clone());
    assert_eq!(names(&found), ["d.rs"]);
}

#[test]
fn a_search_whose_thread_broke_is_over() {
    let shared = Arc::new(Shared::default());
    let searching = Arc::clone(&shared);
    let broke = std::thread::spawn(move || {
        let _over = Over(searching);
        panic!("broken on purpose");
    })
    .join();
    assert!(broke.is_err());
    assert!(Search(shared).over(), "not \"searching\" for ever");
}
