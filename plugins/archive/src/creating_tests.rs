use super::*;
use crate::extract::{Choice, FileInfo};
use crate::update::{UpdateAsks, UpdateAt};

/// Answers Cancel to failures, never stops.
struct Quiet;

impl UpdateAsks for Quiet {
    fn overwrite(&mut self, _: &Path, _: &FileInfo, _: &FileInfo) -> Option<(Overwrite, bool)> {
        None
    }

    fn fault(&mut self, _: &Fault, _: bool) -> Choice {
        Choice::Cancel
    }

    fn progress(&mut self, _: &UpdateAt<'_>) {}

    fn stopped(&mut self) -> bool {
        false
    }
}

fn options(move_files: bool) -> UpdateOptions {
    UpdateOptions {
        level: 5,
        method: "Deflate".to_string(),
        advanced: String::new(),
        encrypt: false,
        show_password: false,
        password: String::new(),
        overwrite: Overwrite::Overwrite,
        move_files,
        ignore_errors: false,
        open_shared: false,
        kind: crate::update_options::Kind::default(),
    }
}

/// The new archive put in its place as the plugin does without panels of it: renamed over it.
fn renamed(temp: &Path, archive: &Path) -> Result<(), Error> {
    std::fs::rename(temp, archive).map_err(|error| Error::new(ErrorKind::Other, error.to_string()))
}

/// A disk's `src` with `a.txt` and `docs/b.txt`.
fn sources(temp: &Path) -> PathBuf {
    let src = temp.join("src");
    std::fs::create_dir_all(src.join("docs")).unwrap();
    std::fs::write(src.join("a.txt"), b"alpha").unwrap();
    std::fs::write(src.join("docs").join("b.txt"), b"beta").unwrap();
    src
}

fn names_in(archive: &Path) -> Vec<String> {
    crate::test_archives::marked_names(archive)
}

/// arclite's `Archive::create`: the folder made, every source in it under its name, a file there
/// replaced; nothing left beside it.
#[test]
fn the_archive_is_made_of_the_sources() {
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let archive = temp.path().join("out").join("new.zip");
    let picked = [src.join("a.txt"), src.join("docs")];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Menu,
        library: Some(crate::test_archives::leaked()),
    };
    let mut asks = Quiet;
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    written(
        &archive,
        &request,
        &options(false),
        &mut faults,
        &mut renamed,
    )
    .unwrap();
    assert_eq!(names_in(&archive), ["a.txt", "docs/", "docs/b.txt"]);
    assert_eq!(
        crate::test_archives::data_of(&archive, "docs/b.txt"),
        b"beta"
    );
    assert!(src.join("a.txt").exists(), "not moved");
    let left: Vec<_> = std::fs::read_dir(temp.path().join("out"))
        .unwrap()
        .collect();
    assert_eq!(left.len(), 1, "only the archive");
    // Made again over it: the old one's place taken.
    let only = [src.join("a.txt")];
    let again = Request {
        sources: &only,
        ..request
    };
    written(&archive, &again, &options(false), &mut faults, &mut renamed).unwrap();
    assert_eq!(names_in(&archive), ["a.txt"]);
}

/// Moved: the sources go once the archive is made without failures.
#[test]
fn the_sources_moved_go() {
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let archive = temp.path().join("moved.zip");
    let picked = [src.join("a.txt"), src.join("docs")];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Put,
        library: Some(crate::test_archives::leaked()),
    };
    let mut asks = Quiet;
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    written(
        &archive,
        &request,
        &options(true),
        &mut faults,
        &mut renamed,
    )
    .unwrap();
    assert_eq!(names_in(&archive), ["a.txt", "docs/", "docs/b.txt"]);
    assert!(!src.join("a.txt").exists() && !src.join("docs").exists());
}

fn quiet_faults(asks: &mut Quiet) -> Faults<'_> {
    Faults {
        asks,
        ignore_errors: false,
        log: Vec::new(),
    }
}

/// Moved with the archive one of its own sources (an old archive of that name overwritten): the other
/// sources go, the new archive stays.
#[test]
fn a_moved_source_the_archive_took_the_place_of_stays() {
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let archive = src.join("src.zip");
    std::fs::write(&archive, b"old").unwrap();
    let picked = [src.join("a.txt"), src.join("docs"), archive.clone()];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Put,
        library: Some(crate::test_archives::leaked()),
    };
    let mut asks = Quiet;
    let mut faults = quiet_faults(&mut asks);
    written(
        &archive,
        &request,
        &options(true),
        &mut faults,
        &mut renamed,
    )
    .unwrap();
    assert_eq!(
        names_in(&archive),
        ["a.txt", "docs/", "docs/b.txt", "src.zip"]
    );
    assert!(!src.join("a.txt").exists() && !src.join("docs").exists());
}

/// Moved with the archive inside a source folder: what else is in that folder goes, the folder and
/// the archive stay.
#[test]
fn a_moved_folder_the_archive_is_in_keeps_it() {
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let archive = src.join("docs").join("inner").join("docs.zip");
    let picked = [src.join("docs")];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Put,
        library: Some(crate::test_archives::leaked()),
    };
    let mut asks = Quiet;
    let mut faults = quiet_faults(&mut asks);
    written(
        &archive,
        &request,
        &options(true),
        &mut faults,
        &mut renamed,
    )
    .unwrap();
    assert_eq!(names_in(&archive), ["docs/", "docs/b.txt"]);
    assert!(archive.exists());
    assert!(!src.join("docs").join("b.txt").exists());
    assert!(src.join("a.txt").exists(), "not a source");
}

/// Moved and split into volumes inside a source folder: every volume stays (not only the first), what
/// else is in the folder goes.
#[test]
fn a_moved_folder_keeps_every_volume_in_it() {
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let noise: Vec<u8> = (0..60_000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[3]
        })
        .collect();
    std::fs::write(src.join("docs").join("big.bin"), &noise).unwrap();
    let archive = src.join("docs").join("v.7z.001");
    let picked = [src.join("docs")];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Put,
        library: Some(crate::test_archives::leaked()),
    };
    let mut split = options(true);
    split.level = 0;
    split.kind.format = "7z".to_string();
    split.kind.volume_size = Some(16 * 1024);
    let mut asks = Quiet;
    let mut faults = quiet_faults(&mut asks);
    written(&archive, &request, &split, &mut faults, &mut renamed).unwrap();
    let left = left_in(&src.join("docs"));
    let first = std::fs::metadata(&archive).map(|meta| meta.len());
    assert!(left.len() >= 4, "{left:?} {first:?} {:?}", faults.log);
    assert!(
        left.iter().all(|name| name.starts_with("v.7z.")),
        "every volume kept, the sources gone: {left:?}"
    );
}

/// Esc as the writing asks (`stopped` true at once), failures let be or cancelled as `ignore` says.
struct Stopping;

impl UpdateAsks for Stopping {
    fn overwrite(&mut self, _: &Path, _: &FileInfo, _: &FileInfo) -> Option<(Overwrite, bool)> {
        None
    }

    fn fault(&mut self, _: &Fault, _: bool) -> Choice {
        Choice::Cancel
    }

    fn progress(&mut self, _: &UpdateAt<'_>) {}

    fn stopped(&mut self) -> bool {
        true
    }
}

/// What is in `folder` by name, sorted (nothing when it is not there).
fn left_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = (std::fs::read_dir(folder).into_iter().flatten())
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Esc while writing: nothing of the archive left beside its place (its new file gone), the sources
/// moved all kept, the stop the user's.
#[test]
fn esc_while_writing_leaves_no_new_file_and_keeps_the_sources() {
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let out = temp.path().join("out");
    let archive = out.join("new.zip");
    let picked = [src.join("a.txt"), src.join("docs")];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Put,
        library: Some(crate::test_archives::leaked()),
    };
    let mut asks = Stopping;
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let made = written(
        &archive,
        &request,
        &options(true),
        &mut faults,
        &mut renamed,
    );
    assert_eq!(made.unwrap_err().kind, ErrorKind::Cancelled);
    assert!(left_in(&out).is_empty(), "{:?}", left_in(&out));
    assert!(src.join("a.txt").exists() && src.join("docs").join("b.txt").exists());
}

/// A source the writing cannot read (another holds it whole) and the failure cancelled: no new file
/// left, nothing placed, the sources moved kept.
#[cfg(windows)]
#[test]
fn a_source_that_cannot_be_read_cancelled_leaves_nothing() {
    use std::os::windows::fs::OpenOptionsExt as _;
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(src.join("a.txt"))
        .unwrap();
    let out = temp.path().join("out");
    let archive = out.join("new.zip");
    let picked = [src.join("a.txt"), src.join("docs")];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Put,
        library: Some(crate::test_archives::leaked()),
    };
    let mut placed = 0;
    let mut place = |temp: &Path, archive: &Path| {
        placed += 1;
        renamed(temp, archive)
    };
    let mut asks = Quiet;
    let mut faults = quiet_faults(&mut asks);
    let made = written(&archive, &request, &options(true), &mut faults, &mut place);
    drop(held);
    assert_eq!(made.unwrap_err().kind, ErrorKind::Cancelled);
    assert_eq!(placed, 0, "nothing to place");
    assert!(left_in(&out).is_empty(), "{:?}", left_in(&out));
    assert!(src.join("a.txt").exists() && src.join("docs").join("b.txt").exists());
}

/// Moved with a failure let be (logged): the archive made of the rest, but no source deleted (arclite
/// deletes them only when nothing failed).
#[test]
fn a_moved_run_with_a_logged_failure_keeps_the_sources() {
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let archive = temp.path().join("moved.zip");
    let picked = [src.join("a.txt"), src.join("docs"), src.join("missing.txt")];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Put,
        library: Some(crate::test_archives::leaked()),
    };
    let mut asks = Quiet;
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: true,
        log: Vec::new(),
    };
    written(
        &archive,
        &request,
        &options(true),
        &mut faults,
        &mut renamed,
    )
    .unwrap();
    assert_eq!(faults.log.len(), 1, "the missing one logged");
    assert_eq!(names_in(&archive), ["a.txt", "docs/", "docs/b.txt"]);
    assert!(src.join("a.txt").exists(), "kept: a failure was logged");
    assert!(src.join("docs").join("b.txt").exists());
}

/// The archive not put in its place: its failure the answer, the sources moved kept.
#[test]
fn an_archive_not_placed_keeps_the_sources() {
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let archive = temp.path().join("moved.zip");
    let picked = [src.join("a.txt")];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Put,
        library: Some(crate::test_archives::leaked()),
    };
    let mut written_at = None;
    let mut place = |temp: &Path, _archive: &Path| {
        written_at = Some(temp.to_path_buf());
        let _ = std::fs::remove_file(temp);
        Err(Error::new(ErrorKind::Other, "not placed"))
    };
    let mut asks = Quiet;
    let mut faults = quiet_faults(&mut asks);
    let made = written(&archive, &request, &options(true), &mut faults, &mut place);
    let error = made.unwrap_err();
    assert_eq!(
        (error.kind, error.message.as_str()),
        (ErrorKind::Other, "not placed")
    );
    let written_at = written_at.expect("written before it is placed");
    assert_eq!(
        written_at.parent(),
        archive.parent(),
        "written beside its place"
    );
    assert_ne!(written_at, archive, "not over it");
    assert!(src.join("a.txt").exists());
}

/// Not moved: the sources stay even when nothing failed.
#[test]
fn sources_not_moved_stay() {
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let archive = temp.path().join("kept.zip");
    let picked = [src.join("a.txt"), src.join("docs")];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Put,
        library: Some(crate::test_archives::leaked()),
    };
    let mut asks = Quiet;
    let mut faults = quiet_faults(&mut asks);
    written(
        &archive,
        &request,
        &options(false),
        &mut faults,
        &mut renamed,
    )
    .unwrap();
    assert!(src.join("a.txt").exists() && src.join("docs").join("b.txt").exists());
}

/// Moved with the archive inside a source folder named in another case: still that folder (the
/// system's names are not cased), so the archive stays.
#[test]
fn a_moved_folder_the_archive_is_in_is_found_whatever_the_case() {
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let archive = src.join("DOCS").join("Docs.ZIP");
    let picked = [src.join("docs")];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Put,
        library: Some(crate::test_archives::leaked()),
    };
    let mut asks = Quiet;
    let mut faults = quiet_faults(&mut asks);
    written(
        &archive,
        &request,
        &options(true),
        &mut faults,
        &mut renamed,
    )
    .unwrap();
    assert!(archive.exists(), "the new archive kept");
    assert!(!src.join("docs").join("b.txt").exists(), "the rest gone");
}

/// The walk stopped: no folder made for the archive, nothing written.
#[test]
fn a_failure_in_the_walk_leaves_no_folder() {
    let temp = tempfile::tempdir().unwrap();
    let src = sources(temp.path());
    let archive = temp.path().join("new").join("deep").join("x.zip");
    let picked = [src.join("missing.txt")];
    let request = Request {
        folder: &src,
        sources: &picked,
        asked: Asked::Menu,
        library: Some(crate::test_archives::leaked()),
    };
    let mut asks = Quiet;
    let mut faults = quiet_faults(&mut asks);
    let made = written(
        &archive,
        &request,
        &options(false),
        &mut faults,
        &mut renamed,
    );
    assert_eq!(made.unwrap_err().kind, ErrorKind::Cancelled);
    assert!(!temp.path().join("new").exists());
}
