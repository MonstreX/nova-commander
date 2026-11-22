//! The walk of an update further: an answer for every file remembered or not, Skip given, what the
//! question shows, names whatever their case, a folder and a file of one name, a folder the archive has,
//! the archive's folder written with slashes about it; failures retried, ignored once or for all; the
//! sources' folder when they share none or one of them is the folder given.

use super::*;

/// Answers as told and keeps what each question showed.
#[derive(Default)]
struct Shown {
    answers: Vec<(Overwrite, bool)>,
    asked: Vec<(PathBuf, FileInfo, FileInfo)>,
    faults: usize,
    choice: Option<Choice>,
}

impl UpdateAsks for Shown {
    fn overwrite(
        &mut self,
        path: &Path,
        source: &FileInfo,
        destination: &FileInfo,
    ) -> Option<(Overwrite, bool)> {
        self.asked.push((path.to_path_buf(), *source, *destination));
        (!self.answers.is_empty()).then(|| self.answers.remove(0))
    }

    fn fault(&mut self, _fault: &Fault, _can_retry: bool) -> Choice {
        self.faults += 1;
        self.choice.unwrap_or(Choice::Cancel)
    }

    fn progress(&mut self, _at: &UpdateAt<'_>) {}

    fn stopped(&mut self) -> bool {
        false
    }
}

fn faults_of(asks: &mut dyn UpdateAsks) -> Faults<'_> {
    Faults {
        asks,
        ignore_errors: false,
        log: Vec::new(),
    }
}

fn asked_paths(shown: &Shown) -> Vec<PathBuf> {
    shown.asked.iter().map(|(path, ..)| path.clone()).collect()
}

/// "Remember choice" holds for every file after it; without it the next file is asked too.
#[test]
fn an_answer_for_all_is_remembered_and_one_for_one_is_not() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let src = sources(temp.path());
    let names = [PathBuf::from("top.txt"), PathBuf::from("docs")];
    let mut shown = Shown {
        answers: vec![(Overwrite::Overwrite, true)],
        ..Shown::default()
    };
    let plan = prepare(
        &opened,
        "/",
        (&src, &names),
        Overwrite::Ask,
        &mut faults_of(&mut shown),
    )
    .unwrap();
    assert_eq!(asked_paths(&shown), [PathBuf::from("top.txt")], "once");
    assert_eq!(
        paths(&plan),
        ["top.txt", "docs", "docs/a.txt", "docs/b.txt"]
    );
    assert_eq!(plan.replaced.get(&1), Some(&0), "top.txt in its place");
    assert_eq!(plan.replaced.get(&0), Some(&2), "docs/a.txt in its place");
    assert!(!plan.skipped);
    let mut each = Shown {
        answers: vec![(Overwrite::Overwrite, false), (Overwrite::Overwrite, false)],
        ..Shown::default()
    };
    let plan = prepare(
        &opened,
        "/",
        (&src, &names),
        Overwrite::Ask,
        &mut faults_of(&mut each),
    )
    .unwrap();
    assert_eq!(
        asked_paths(&each),
        [PathBuf::from("top.txt"), PathBuf::from(r"docs\a.txt")],
        "each file asked about"
    );
    assert_eq!(plan.replaced.len(), 2);
}

/// Skip given: none asked, none replaced, the new file added, the walk marked as skipping.
#[test]
fn skip_given_asks_none_and_takes_no_place() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let src = sources(temp.path());
    let names = [PathBuf::from("top.txt"), PathBuf::from("new.txt")];
    let mut shown = Shown::default();
    let plan = prepare(
        &opened,
        "/",
        (&src, &names),
        Overwrite::Skip,
        &mut faults_of(&mut shown),
    )
    .unwrap();
    assert!(shown.asked.is_empty());
    assert_eq!(paths(&plan), ["new.txt"]);
    assert!(plan.replaced.is_empty() && plan.skipped);
}

/// The question shows the disk's file as the source and the archive's as the destination.
#[test]
fn the_question_shows_the_new_file_against_the_archives() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let src = temp.path().join("longer");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("top.txt"), "a longer new top").unwrap();
    let mut shown = Shown {
        answers: vec![(Overwrite::Skip, false)],
        ..Shown::default()
    };
    let names = [PathBuf::from("top.txt")];
    let plan = prepare(
        &opened,
        "/",
        (&src, &names),
        Overwrite::Ask,
        &mut faults_of(&mut shown),
    )
    .unwrap();
    let (_, source, destination) = shown.asked[0];
    assert_eq!((source.size, destination.size), (16, 7));
    assert!(!source.is_dir && !destination.is_dir);
    assert_eq!(destination.modified, opened.items[1].modified);
    assert!(source.modified.is_some());
    assert!(plan.added.is_empty() && plan.skipped, "Skip: nothing taken");
}

/// A file whose name differs only in case takes the archive's file's place, under its own name.
#[test]
fn a_name_in_another_case_is_the_same_file() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let src = temp.path().join("upper");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("TOP.TXT"), "upper").unwrap();
    let names = [PathBuf::from("TOP.TXT")];
    let mut shown = Shown::default();
    let plan = prepare(
        &opened,
        "/",
        (&src, &names),
        Overwrite::Overwrite,
        &mut faults_of(&mut shown),
    )
    .unwrap();
    assert_eq!(paths(&plan), ["TOP.TXT"]);
    assert_eq!(plan.replaced.get(&1), Some(&0));
}

/// A disk's folder named as a file of the archive takes no file's place: written with what is in it.
#[test]
fn a_folder_does_not_take_a_files_place() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let src = temp.path().join("folders");
    std::fs::create_dir_all(src.join("top.txt")).unwrap();
    std::fs::write(src.join("top.txt").join("x.txt"), "x").unwrap();
    let names = [PathBuf::from("top.txt")];
    let mut shown = Shown::default();
    let plan = prepare(
        &opened,
        "/",
        (&src, &names),
        Overwrite::Ask,
        &mut faults_of(&mut shown),
    )
    .unwrap();
    assert_eq!(paths(&plan), ["top.txt", "top.txt/x.txt"]);
    assert!(plan.added[0].is_dir);
    assert!(plan.replaced.is_empty() && shown.asked.is_empty());
}

/// A folder the archive has as an item is gone into, not written again; its file there replaced.
#[test]
fn a_folder_the_archive_has_is_gone_into() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("folders.zip");
    crate::test_archives::zip(&path, &[("docs", None), ("docs/a.txt", Some(b"old a"))]);
    let opened = crate::test_archives::opened(&path);
    let src = sources(temp.path());
    let names = [PathBuf::from("docs")];
    let mut shown = Shown::default();
    let plan = prepare(
        &opened,
        "/",
        (&src, &names),
        Overwrite::Overwrite,
        &mut faults_of(&mut shown),
    )
    .unwrap();
    assert_eq!(paths(&plan), ["docs/a.txt", "docs/b.txt"]);
    assert_eq!(plan.replaced.get(&1), Some(&0), "docs/a.txt in its place");
    assert!(!plan.replaced.contains_key(&0), "the folder's item kept");
}

/// The archive's folder with slashes about it is the same folder.
#[test]
fn the_archives_folder_is_read_whatever_its_slashes() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let src = temp.path().join("one");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("a.txt"), "new a").unwrap();
    let names = [PathBuf::from("a.txt")];
    let mut shown = Shown::default();
    let plan = prepare(
        &opened,
        "//docs/",
        (&src, &names),
        Overwrite::Overwrite,
        &mut faults_of(&mut shown),
    )
    .unwrap();
    assert_eq!(paths(&plan), ["docs/a.txt"]);
    assert_eq!(plan.replaced.get(&0), Some(&0));
}

/// Retry tries again, and what is then done is taken; nothing logged.
#[test]
fn retry_tries_again() {
    let mut shown = Shown {
        choice: Some(Choice::Retry),
        ..Shown::default()
    };
    let mut faults = faults_of(&mut shown);
    let mut tries = 0;
    let done = faults.attempt(|| {
        tries += 1;
        if tries == 1 {
            Err(Fault::default())
        } else {
            Ok(tries)
        }
    });
    assert_eq!(done, Ok(Some(2)));
    assert!(faults.log.is_empty() && !faults.ignore_errors);
    assert_eq!(shown.faults, 1);
}

/// Ignore lets this failure be (logged) and asks about the next; Ignore All asks no more.
#[test]
fn ignore_is_for_one_failure_and_ignore_all_for_every_one() {
    let failing = || Err::<(), _>(Fault::default());
    let mut once = Shown {
        choice: Some(Choice::Ignore),
        ..Shown::default()
    };
    let mut faults = faults_of(&mut once);
    assert_eq!(faults.attempt(failing), Ok(None));
    assert!(!faults.ignore_errors);
    assert_eq!(faults.attempt(failing), Ok(None));
    assert_eq!(faults.log.len(), 2);
    assert_eq!(once.faults, 2, "each one asked about");
    let mut all = Shown {
        choice: Some(Choice::IgnoreAll),
        ..Shown::default()
    };
    let mut faults = faults_of(&mut all);
    assert_eq!(faults.attempt(failing), Ok(None));
    assert!(faults.ignore_errors);
    assert_eq!(faults.attempt(failing), Ok(None));
    assert_eq!(faults.log.len(), 2);
    assert_eq!(all.faults, 1, "asked once");
    let mut cancel = Shown::default();
    let mut faults = faults_of(&mut cancel);
    assert_eq!(faults.attempt(failing), Err(Abort));
    assert!(faults.log.is_empty(), "Cancel logs none");
}

/// A source that is the folder given is not in it: the folder above is taken. Sources sharing no folder
/// keep their whole paths; a path shorter than the folder is not under it.
#[test]
fn the_sources_folder_at_its_edges() {
    let given = Path::new(r"C:\Data");
    let itself = [PathBuf::from(r"C:\Data"), PathBuf::from(r"C:\Data\a.txt")];
    let (base, names) = under_folder(&itself, Some(given));
    assert_eq!(base, Path::new(r"C:\"));
    assert_eq!(names, [PathBuf::from("Data"), PathBuf::from(r"Data\a.txt")]);
    let drives = [PathBuf::from(r"C:\a.txt"), PathBuf::from(r"D:\b.txt")];
    let (base, names) = under_folder(&drives, None);
    assert_eq!(base, PathBuf::new());
    assert_eq!(names, drives);
    let one = [PathBuf::from(r"C:\x\y\a.txt")];
    let (base, names) = under_folder(&one, None);
    assert_eq!(base, Path::new(r"C:\x\y"));
    assert_eq!(names, [PathBuf::from("a.txt")]);
    assert_eq!(relative(Path::new(r"C:\x"), Path::new(r"C:\x\y")), None);
    assert_eq!(relative(Path::new(r"C:\xy\a"), Path::new(r"C:\x")), None);
}
