//! What is left of writing anew around the library: the temporary name, the new file put in place, a
//! source opened, the sources moved deleted.

use super::*;
use crate::extract::{Choice, FileInfo};
use crate::extract_options::Overwrite;
use crate::update::{UpdateAsks, UpdateAt};

/// Answers Cancel to failures, never stops.
#[derive(Default)]
struct Quiet;

impl UpdateAsks for Quiet {
    fn overwrite(
        &mut self,
        _path: &Path,
        _source: &FileInfo,
        _destination: &FileInfo,
    ) -> Option<(Overwrite, bool)> {
        None
    }

    fn fault(&mut self, _fault: &Fault, _can_retry: bool) -> Choice {
        Choice::Cancel
    }

    fn progress(&mut self, _at: &UpdateAt<'_>) {}

    fn stopped(&mut self) -> bool {
        false
    }
}

/// arclite's `get_temp_file_name`: `{GUID}.tmp` in the archive's folder, a new one each time.
#[test]
fn temporary_names_are_new_guids_beside_the_archive() {
    let archive = Path::new(r"C:\a\b.7z");
    let one = temp_name(archive);
    let other = temp_name(archive);
    assert_ne!(one, other);
    assert_eq!(one.parent(), Some(Path::new(r"C:\a")));
    let name = one.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with('{') && name.ends_with("}.tmp"), "{name}");
    assert_eq!(name.len(), 38 + 4);
}

/// The new file in the old one's place, the old one's creation time kept; a failure leaves the old one
/// whole and the new one where it was.
#[test]
fn the_new_file_takes_the_old_ones_place() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("a.7z");
    std::fs::write(&archive, "old").unwrap();
    let created = std::fs::metadata(&archive).unwrap().created().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let new = temp_name(&archive);
    std::fs::write(&new, "new").unwrap();
    replace(&new, &archive).unwrap();
    assert_eq!(std::fs::read(&archive).unwrap(), b"new");
    assert!(!new.exists());
    assert_eq!(
        std::fs::metadata(&archive).unwrap().created().unwrap(),
        created
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let again = temp_name(&archive);
        std::fs::write(&again, "again").unwrap();
        let held = OpenOptions::new()
            .read(true)
            .share_mode(0x1)
            .open(&archive)
            .unwrap();
        assert!(replace(&again, &archive).is_err());
        drop(held);
        assert_eq!(
            std::fs::read(&archive).unwrap(),
            b"new",
            "the old one whole"
        );
        assert!(again.exists(), "the new one left for the caller");
    }
}

/// A source opened shared lets others write to it; not shared, a file held for writing is refused.
#[cfg(windows)]
#[test]
fn a_source_opens_as_arclite_opens_it() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("busy.txt");
    std::fs::write(&path, "busy").unwrap();
    let _writing = OpenOptions::new().write(true).open(&path).unwrap();
    assert!(open_source(&path, true).is_ok());
    assert!(open_source(&path, false).is_err());
    let free = temp.path().join("free.txt");
    std::fs::write(&free, "free").unwrap();
    let mut read = String::new();
    io::Read::read_to_string(&mut open_source(&free, false).unwrap(), &mut read).unwrap();
    assert_eq!(read, "free");
}

/// Moving: the sources go, a folder with what is in it, a read-only file too.
#[test]
fn moved_sources_are_deleted() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path();
    std::fs::write(src.join("top.txt"), "top").unwrap();
    std::fs::create_dir(src.join("new")).unwrap();
    let read_only = src.join("new").join("n.txt");
    std::fs::write(&read_only, "n").unwrap();
    let mut permissions = std::fs::metadata(&read_only).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&read_only, permissions).unwrap();
    let mut asks = Quiet;
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    delete_sources(&[src.join("top.txt"), src.join("new")], &mut faults).unwrap();
    assert!(!src.join("top.txt").exists() && !src.join("new").exists());
    assert!(faults.log.is_empty());
}
