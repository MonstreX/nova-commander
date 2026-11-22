use super::*;

fn at(seconds: u64, hundreds: u64) -> Option<SystemTime> {
    Some(UNIX_EPOCH + Duration::from_secs(seconds) + Duration::from_nanos(hundreds * 100))
}

fn options(low_precision: bool, ignore_time_zone: bool) -> Options {
    Options {
        low_precision,
        ignore_time_zone,
        ..Options::default()
    }
}

#[test]
fn times_are_the_same_within_two_seconds_or_a_whole_number_of_quarter_hours() {
    let exact = options(false, false);
    assert!(same_time(at(100, 0), at(100, 0), &exact));
    assert!(!same_time(at(100, 0), at(100, 1), &exact), "to 100 ns");
    let low = options(true, false);
    assert!(same_time(at(100, 0), at(102, 0), &low));
    assert!(!same_time(at(100, 0), at(102, 1), &low));
    let zones = options(true, true);
    assert!(same_time(at(0, 0), at(3600, 0), &zones), "an hour");
    assert!(
        same_time(at(0, 0), at(5 * 900 + 1, 0), &zones),
        "75 min, a second off"
    );
    assert!(!same_time(at(0, 0), at(5 * 900 + 30, 0), &zones));
    assert!(same_time(at(0, 0), at(26 * 3600, 0), &zones), "26 hours");
    assert!(
        !same_time(at(0, 0), at(27 * 3600, 0), &zones),
        "past 26 hours"
    );
    assert!(!same_time(at(0, 0), at(3600, 1), &options(false, true)));
    assert!(same_time(None, None, &exact));
    assert!(!same_time(None, at(0, 0), &exact));
}

fn item(name: &str, folder: bool, size: u64, seconds: u64) -> Item {
    Item {
        name: name.to_string(),
        folder,
        size,
        modified: at(seconds, 0),
        ..Item::default()
    }
}

fn run_with<T>(options: &Options, work: impl FnOnce(&mut Run) -> T) -> T {
    let mut progress = |_: &Path, _: &Path| {};
    let filter = |_: Side, items: &[Item]| vec![true; items.len()];
    let mut run = Run::new(
        options,
        false,
        contents::LEAST_BLOCK,
        &|| false,
        &mut progress,
        &filter,
    );
    work(&mut run)
}

#[test]
fn items_pair_by_name_and_kind_and_what_differs_or_has_no_pair_is_marked() {
    let a = [
        item("Same.txt", false, 1, 10),
        item("bigger.txt", false, 1, 10),
        item("only-here.txt", false, 1, 10),
        item("both", true, 0, 10),
        item("kind", false, 1, 10),
    ];
    let p = [
        item("same.TXT", false, 1, 10),
        item("bigger.txt", false, 2, 10),
        item("only-there.txt", false, 1, 10),
        item("both", true, 0, 99),
        item("kind", true, 0, 10),
    ];
    let (marks_a, marks_p, same) =
        run_with(&Options::default(), |run| run.compare(&a, &p, true, 0));
    assert_eq!(marks_a, [false, true, true, false, true]);
    assert_eq!(marks_p, [false, true, true, false, true]);
    assert!(!same);
    let (_, _, same) = run_with(&Options::default(), |run| {
        run.compare(&a[..1], &p[..1], true, 0)
    });
    assert!(same, "only the same pair");
}

#[test]
fn time_and_size_count_only_when_asked() {
    let a = [item("f", false, 1, 10)];
    let p = [item("f", false, 2, 500)];
    let neither = Options {
        time: false,
        size: false,
        ..Options::default()
    };
    assert!(run_with(&neither, |run| run.compare(&a, &p, true, 0)).2);
}

fn tree(root: &Path, files: &[(&str, &str)]) {
    for (name, text) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

fn folder_item(root: &Path, name: &str) -> Item {
    Item {
        name: name.to_string(),
        folder: true,
        path: Some(root.join(name)),
        ..Item::default()
    }
}

#[test]
fn folders_are_compared_by_what_is_in_them_down_to_the_depth_given() {
    let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    tree(one.path(), &[("d/x.txt", "x"), ("d/deep/y.txt", "y")]);
    tree(two.path(), &[("d/x.txt", "x"), ("d/deep/y.txt", "yy")]);
    let (a, p) = (
        [folder_item(one.path(), "d")],
        [folder_item(two.path(), "d")],
    );
    let subfolders = Options {
        subfolders: true,
        time: false,
        ..Options::default()
    };
    assert!(
        !run_with(&subfolders, |run| run.compare(&a, &p, true, 0)).2,
        "y differs in size"
    );
    let shallow = Options {
        use_max_depth: true,
        max_depth: 1,
        ..subfolders
    };
    assert!(
        run_with(&shallow, |run| run.compare(&a, &p, true, 0)).2,
        "deep not gone into"
    );
    let without = Options {
        subfolders: false,
        ..subfolders
    };
    assert!(
        run_with(&without, |run| run.compare(&a, &p, true, 0)).2,
        "folders by name"
    );
}

#[test]
fn contents_differ_and_a_file_not_opened_is_told() {
    let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    tree(one.path(), &[("f.txt", "abc")]);
    tree(two.path(), &[("f.txt", "abd")]);
    let file = |root: &Path| Item {
        path: Some(root.join("f.txt")),
        ..item("f.txt", false, 3, 0)
    };
    let contents = Options {
        time: false,
        contents: true,
        ..Options::default()
    };
    let (a, p) = ([file(one.path())], [file(two.path())]);
    assert!(!run_with(&contents, |run| run.compare(&a, &p, true, 0)).2);
    let gone = [Item {
        path: Some(two.path().join("none.txt")),
        ..file(two.path())
    }];
    let failed = run_with(&contents, |run| {
        run.compare(&a, &gone, true, 0);
        run.open_failed
    });
    assert!(failed);
}

#[test]
fn a_stop_ends_it_at_once() {
    let a = [item("f", false, 1, 10)];
    let mut progress = |_: &Path, _: &Path| {};
    let filter = |_: Side, items: &[Item]| vec![true; items.len()];
    let options = Options::default();
    let mut run = Run::new(
        &options,
        false,
        contents::LEAST_BLOCK,
        &|| true,
        &mut progress,
        &filter,
    );
    run.compare(&a, &a, true, 0);
    assert!(run.stopped);
}

#[test]
fn two_names_told_apart_by_case_pair_one_and_leave_the_other() {
    let a = [item("a", false, 1, 10)];
    let p = [item("A", false, 1, 10), item("a", false, 1, 10)];
    let (marks_a, marks_p, same) =
        run_with(&Options::default(), |run| run.compare(&a, &p, true, 0));
    assert_eq!(marks_a, [false]);
    assert_eq!(
        marks_p.iter().filter(|&&mark| mark).count(),
        1,
        "{marks_p:?}"
    );
    assert!(!same, "one without a pair");
}

#[cfg(windows)]
#[test]
fn a_link_back_to_a_folder_compared_above_is_not_gone_into_again() {
    let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for root in [one.path(), two.path()] {
        tree(root, &[("d/x.txt", "x")]);
        nc_plugin_sdk::system::junction(&root.join("d"), &root.join("d").join("back")).unwrap();
    }
    let (a, p) = (
        [folder_item(one.path(), "d")],
        [folder_item(two.path(), "d")],
    );
    let subfolders = Options {
        subfolders: true,
        time: false,
        ..Options::default()
    };
    assert!(
        run_with(&subfolders, |run| run.compare(&a, &p, true, 0)).2,
        "ends, the same"
    );
}
