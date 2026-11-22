//! A file saved over an existing one keeps what that one carried besides its content (review R2): its named
//! streams, creation time and attributes come along, and every name of a file with several gets the new
//! content.

use super::*;

/// A file with `old` in it, in a folder of its own (the tests of what Windows keeps).
#[cfg(windows)]
fn saved_file(old: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("notes.txt");
    fs::write(&path, old).unwrap();
    (temp, path)
}

#[cfg(windows)]
#[test]
fn a_named_stream_stays_with_the_saved_file() {
    let (_temp, path) = saved_file("old");
    let stream = PathBuf::from(format!("{}:extra", path.display()));
    fs::write(&stream, "kept").unwrap();
    write_atomic(&path, b"new").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(fs::read_to_string(&stream).unwrap(), "kept", "the stream");
}

#[cfg(windows)]
#[test]
fn every_name_of_a_file_gets_the_new_content() {
    let (temp, path) = saved_file("old");
    let other = temp.path().join("other name.txt");
    fs::hard_link(&path, &other).unwrap();
    write_atomic(&path, b"new").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(fs::read_to_string(&other).unwrap(), "new", "the other name");
}

#[cfg(windows)]
#[test]
fn the_creation_time_stays() {
    let (_temp, path) = saved_file("old");
    let created = fs::metadata(&path).unwrap().created().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));
    write_atomic(&path, b"new").unwrap();
    assert_eq!(fs::metadata(&path).unwrap().created().unwrap(), created);
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
}

#[cfg(windows)]
#[test]
fn a_hidden_file_stays_hidden() {
    use std::os::windows::fs::MetadataExt;
    let (_temp, path) = saved_file("old");
    let status = std::process::Command::new("attrib")
        .arg("+h")
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    write_atomic(&path, b"new").unwrap();
    assert_ne!(
        fs::metadata(&path).unwrap().file_attributes() & 0x2,
        0,
        "hidden"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
}

#[test]
fn a_new_file_that_cannot_be_put_in_place_leaves_no_temporary_one() {
    // The target's name is a folder: the rename fails; the temporary file goes (review of step A).
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("taken");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("inside.txt"), "x").unwrap();
    assert!(write_atomic(&path, b"new").is_err());
    let names: Vec<_> = fs::read_dir(temp.path()).unwrap().flatten().collect();
    assert_eq!(names.len(), 1, "only the folder: {names:?}");
}

#[test]
fn a_new_file_is_made_and_no_temporary_one_is_left() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("fresh.txt");
    write_atomic(&path, b"first").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "first");
    write_atomic(&path, b"second").unwrap();
    let names: Vec<_> = fs::read_dir(temp.path()).unwrap().flatten().collect();
    assert_eq!(names.len(), 1, "only the file itself");
}

/// The editor's save stopped or failed while it streamed: the file as it was, no temporary file left,
/// `written` never heard.
#[test]
fn a_streamed_save_that_fails_leaves_the_file_as_it_was() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("kept.txt");
    fs::write(&path, "old").unwrap();
    let mut heard = false;
    let failed = save_streamed(
        &path,
        &mut |out| {
            out.write_all(b"half")?;
            Err(io::Error::new(io::ErrorKind::Interrupted, "Stopped"))
        },
        &mut || heard = true,
    );
    assert_eq!(failed.unwrap_err().kind(), io::ErrorKind::Interrupted);
    assert!(!heard);
    assert_eq!(fs::read_to_string(&path).unwrap(), "old");
    let names: Vec<_> = fs::read_dir(temp.path()).unwrap().flatten().collect();
    assert_eq!(names.len(), 1, "only the file itself: {names:?}");
}

/// `written` runs once, the content on the disk and the old file still in its place (the editor reads
/// its undo's bytes from it then).
#[test]
fn written_is_heard_once_with_the_old_file_still_there() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("notes.txt");
    fs::write(&path, "old content").unwrap();
    let mut seen = Vec::new();
    save_streamed(&path, &mut |out| out.write_all(b"new"), &mut || {
        seen.push(fs::read_to_string(&path).unwrap());
    })
    .unwrap();
    assert_eq!(seen, ["old content"]);
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
}

/// A file with several names gets a shorter content whole (not over the old one's tail) through every
/// name, `written` heard, the temporary file gone.
#[cfg(windows)]
#[test]
fn every_name_gets_a_shorter_content_whole_and_no_temporary_file_stays() {
    let (temp, path) = saved_file("a much longer old content");
    let other = temp.path().join("other name.txt");
    fs::hard_link(&path, &other).unwrap();
    let mut heard = 0;
    save_streamed(&path, &mut |out| out.write_all(b"new"), &mut || heard += 1).unwrap();
    assert_eq!(heard, 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(fs::read_to_string(&other).unwrap(), "new", "the other name");
    let names: Vec<_> = fs::read_dir(temp.path()).unwrap().flatten().collect();
    assert_eq!(names.len(), 2, "the two names only: {names:?}");
}

/// R7 review C10: a name of 250 characters is saved (its temporary name not made past 255 with it).
#[test]
fn a_file_with_a_long_name_is_saved() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(format!("{}.txt", "n".repeat(246)));
    fs::write(&path, "old").unwrap();
    write_atomic(&path, b"new").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(
        fs::read_dir(temp.path()).unwrap().count(),
        1,
        "no temporary file"
    );
}

/// R7 review C3: written over in part, the file's temporary copy holds the only whole content - kept and
/// named in the error; failing before anything was written over, it goes.
#[test]
fn a_copy_over_the_file_failing_in_part_keeps_the_temporary_file() {
    let temp = tempfile::tempdir().unwrap();
    let saved = temp.path().join("notes.txt.tmp");
    fs::write(&saved, "whole new content").unwrap();
    let full = || io::Error::other("disk full");
    let error = in_place_end(Err(InPlace::Cut(full())), &saved).unwrap_err();
    assert!(saved.exists(), "kept");
    assert!(
        error.to_string().contains(&saved.display().to_string()),
        "{error}"
    );
    assert!(in_place_end(Err(InPlace::Untouched(full())), &saved).is_err());
    assert!(!saved.exists(), "nothing written over: gone");
}
