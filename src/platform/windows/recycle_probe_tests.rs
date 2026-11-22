//! The shell asked whether it would recycle an object: on a fixed drive with a Recycle Bin it would, and the
//! probe deletes nothing (it aborts the deletion it started); a path that is not there cannot be asked.

use super::*;

#[test]
fn a_file_on_a_fixed_drive_can_be_recycled_and_stays() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("probe.txt");
    std::fs::write(&file, "keep me").unwrap();
    let root = crate::platform::volumes::volume_of(&file);
    let answer = can_recycle(&file);
    assert!(file.exists(), "the probe deletes nothing");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep me");
    if crate::platform::volumes::has_recycle_bin(&root.to_string_lossy()) {
        assert_eq!(answer, Some(true), "a temporary file on a drive with a bin");
    }
}

#[test]
fn a_folder_is_asked_without_its_contents_and_stays() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("sub");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("inside.txt"), "x").unwrap();
    let answer = can_recycle(&folder);
    assert!(folder.join("inside.txt").exists());
    assert!(answer.is_some(), "the shell answered");
}

#[test]
fn an_object_that_is_not_there_cannot_be_asked() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(can_recycle(&temp.path().join("none.txt")), None);
}
