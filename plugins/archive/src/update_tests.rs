use super::*;

/// Answers as told, keeps what it was asked.
#[derive(Default)]
pub struct Told {
    pub answers: Vec<Option<(Overwrite, bool)>>,
    pub asked: Vec<PathBuf>,
    pub faults: Vec<Fault>,
    pub choice: Option<Choice>,
}

impl UpdateAsks for Told {
    fn overwrite(
        &mut self,
        path: &Path,
        _source: &FileInfo,
        _destination: &FileInfo,
    ) -> Option<(Overwrite, bool)> {
        self.asked.push(path.to_path_buf());
        if self.answers.is_empty() {
            None
        } else {
            self.answers.remove(0)
        }
    }

    fn fault(&mut self, fault: &Fault, _can_retry: bool) -> Choice {
        self.faults.push(fault.clone());
        self.choice.unwrap_or(Choice::Cancel)
    }

    fn progress(&mut self, _at: &UpdateAt<'_>) {}

    fn stopped(&mut self) -> bool {
        false
    }
}

/// An archive with `docs/a.txt`, `top.txt` and the folder `docs` only implied.
fn archive(temp: &Path) -> Opened {
    let path = temp.join("a.zip");
    crate::test_archives::zip(
        &path,
        &[
            ("docs/a.txt", Some(b"old a")),
            ("top.txt", Some(b"old top")),
        ],
    );
    crate::test_archives::opened(&path)
}

/// A disk's folder `src` with `top.txt`, `new.txt` and `docs/a.txt`, `docs/b.txt`.
fn sources(temp: &Path) -> PathBuf {
    let src = temp.join("src");
    std::fs::create_dir_all(src.join("docs")).unwrap();
    for name in ["top.txt", "new.txt", "docs/a.txt", "docs/b.txt"] {
        std::fs::write(src.join(name), name).unwrap();
    }
    src
}

fn paths(plan: &Plan) -> Vec<String> {
    (plan.added.iter())
        .map(|added| added.path.join("/"))
        .collect()
}

/// Ask: each file there asked about (Overwrite once, then Skip for all), a new one added; the folder
/// only implied written, its files under it.
#[test]
fn files_there_are_asked_about() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let src = sources(temp.path());
    let names = [
        PathBuf::from("top.txt"),
        PathBuf::from("new.txt"),
        PathBuf::from("docs"),
    ];
    let mut told = Told {
        answers: vec![
            Some((Overwrite::Overwrite, false)),
            Some((Overwrite::Skip, true)),
        ],
        ..Told::default()
    };
    let mut faults = Faults {
        asks: &mut told,
        ignore_errors: false,
        log: Vec::new(),
    };
    let plan = prepare(&opened, "/", (&src, &names), Overwrite::Ask, &mut faults).unwrap();
    assert_eq!(paths(&plan), ["top.txt", "new.txt", "docs", "docs/b.txt"]);
    assert_eq!(plan.replaced.get(&1), Some(&0), "top.txt in its place");
    assert!(plan.skipped, "docs/a.txt skipped");
    assert_eq!(
        told.asked,
        [PathBuf::from("top.txt"), PathBuf::from(r"docs\a.txt")]
    );
}

/// Overwrite: none asked, every file there replaced; into a folder below the root.
#[test]
fn overwrite_replaces_without_asking() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let src = sources(temp.path());
    let names = [PathBuf::from("docs/a.txt"), PathBuf::from("top.txt")];
    let mut told = Told::default();
    let mut faults = Faults {
        asks: &mut told,
        ignore_errors: false,
        log: Vec::new(),
    };
    let plan = prepare(
        &opened,
        "/docs",
        (&src, &names[1..]),
        Overwrite::Overwrite,
        &mut faults,
    )
    .unwrap();
    assert_eq!(paths(&plan), ["docs/top.txt"], "a new file there");
    assert!(plan.replaced.is_empty());
    let plan = prepare(
        &opened,
        "/",
        (&src, &names),
        Overwrite::Overwrite,
        &mut faults,
    )
    .unwrap();
    assert_eq!(
        paths(&plan),
        ["docs/a.txt", "top.txt"],
        "kept under its folder"
    );
    assert_eq!(plan.replaced.len(), 2);
    assert!(told.asked.is_empty() && !plan.skipped);
}

/// Cancel at the question stops it all; a source not there is a failure, Cancel stops, ignored it is
/// logged.
#[test]
fn cancel_and_failures() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let src = sources(temp.path());
    let mut told = Told::default();
    let mut faults = Faults {
        asks: &mut told,
        ignore_errors: false,
        log: Vec::new(),
    };
    let names = [PathBuf::from("top.txt")];
    assert_eq!(
        prepare(&opened, "/", (&src, &names), Overwrite::Ask, &mut faults),
        Err(Abort)
    );
    let gone = [PathBuf::from("gone.txt"), PathBuf::from("new.txt")];
    assert_eq!(
        prepare(&opened, "/", (&src, &gone), Overwrite::Ask, &mut faults),
        Err(Abort)
    );
    faults.ignore_errors = true;
    let plan = prepare(&opened, "/", (&src, &gone), Overwrite::Ask, &mut faults).unwrap();
    assert_eq!(paths(&plan), ["new.txt"]);
    assert_eq!(faults.log.len(), 1);
    assert_eq!(told.faults.len(), 1, "asked once, then let be");
}

/// The sources' folder: the one given when they are all in it (whatever its case), else the folder they
/// share.
#[test]
fn the_sources_folder_is_found() {
    let given = Path::new(r"C:\Data");
    let sources = [
        PathBuf::from(r"C:\data\a.txt"),
        PathBuf::from(r"C:\DATA\sub"),
    ];
    let (base, names) = under_folder(&sources, Some(given));
    assert_eq!(base, given);
    assert_eq!(names, [PathBuf::from("a.txt"), PathBuf::from("sub")]);
    let apart = [
        PathBuf::from(r"C:\x\one\a.txt"),
        PathBuf::from(r"C:\x\two\b.txt"),
    ];
    let (base, names) = under_folder(&apart, Some(given));
    assert_eq!(base, Path::new(r"C:\x"));
    assert_eq!(
        names,
        [PathBuf::from(r"one\a.txt"), PathBuf::from(r"two\b.txt")]
    );
    let (base, _) = under_folder(&apart, None);
    assert_eq!(base, Path::new(r"C:\x"));
    assert_eq!(relative(given, given), None, "never itself");
}

#[path = "update_walk_tests.rs"]
mod walk;
