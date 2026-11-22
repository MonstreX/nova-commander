//! Copying a file's contents with the kernel's own copy (ADR-0009 §4): `CopyFileExW` keeps alternate data
//! streams, attributes, sparse and encrypted files, clones blocks on ReFS and copies on the server on SMB;
//! free space as the user may use it; replacing a file in one step.

use std::ffi::c_void;
use std::io;
use std::path::Path;

use super::{wide_os, wide_path};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, COPY_FILE_FAIL_IF_EXISTS, COPY_FILE_NO_BUFFERING, CopyFileExW,
    FILE_ATTRIBUTE_READONLY, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_NO_BUFFERING,
    GetDiskFreeSpaceExW, GetFileInformationByHandle, GetVolumePathNameW, MOVEFILE_REPLACE_EXISTING,
    MOVEFILE_WRITE_THROUGH, MoveFileExW, PROGRESS_CANCEL, PROGRESS_CONTINUE, SetFileAttributesW,
};

/// From this size on the copy bypasses the file cache, so one big copy does not push everything else
/// out of it.
const NO_BUFFERING_FROM: u64 = 256 * 1024 * 1024;
/// `ERROR_REQUEST_ABORTED`: the progress routine cancelled the copy.
const ERROR_REQUEST_ABORTED: i32 = 1235;

/// What the progress routine needs: the caller's callback and whether the target was created yet.
struct Progress<'a> {
    report: &'a mut dyn FnMut(u64, u64) -> bool,
    created: bool,
    /// The callback panicked: the copy is cancelled and the panic goes on once the system's call has
    /// returned (review R52: a panic must not cross the system's frames, which aborts the program).
    panicked: Option<Box<dyn std::any::Any + Send>>,
}

/// Copies the file `source` to the new name `destination`, never over an existing file
/// (`AlreadyExists`). `report(done, total)` is called as the copy goes; returning false stops it
/// (`Interrupted`). A copy that did not finish leaves nothing under `destination`.
pub fn copy_file(
    source: &Path,
    destination: &Path,
    report: &mut dyn FnMut(u64, u64) -> bool,
) -> io::Result<()> {
    let size = std::fs::metadata(source)?.len();
    let mut flags = COPY_FILE_FAIL_IF_EXISTS;
    if size >= NO_BUFFERING_FROM {
        flags |= COPY_FILE_NO_BUFFERING;
    }
    let mut progress = Progress {
        report,
        created: false,
        panicked: None,
    };
    let (from, to) = (wide_path(source), wide_path(destination));
    // SAFETY: both names are NUL-terminated and outlive the call; `progress` outlives the call and the
    // routine reads it only during the call, on this thread; the cancel flag pointer may be null.
    let copied = unsafe {
        CopyFileExW(
            from.as_ptr(),
            to.as_ptr(),
            Some(on_progress),
            (&raw mut progress).cast::<c_void>().cast_const(),
            std::ptr::null_mut(),
            flags,
        )
    };
    if let Some(panic) = progress.panicked.take() {
        if progress.created && destination.exists() {
            let _ = set_read_only(destination, false);
            let _ = std::fs::remove_file(destination);
        }
        std::panic::resume_unwind(panic);
    }
    if copied != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    // The kernel removes a cancelled copy itself; anything else this call created is removed here. A
    // target that was there before the call (`AlreadyExists`) was never created by it.
    // The copy took the source's attributes: a read-only one is made writable to be removed.
    if progress.created && destination.exists() {
        let _ = set_read_only(destination, false);
        let _ = std::fs::remove_file(destination);
    }
    if error.raw_os_error() == Some(ERROR_REQUEST_ABORTED) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "copy cancelled"));
    }
    Err(error)
}

/// `LPPROGRESS_ROUTINE`: passes the bytes done to the caller's callback.
unsafe extern "system" fn on_progress(
    total: i64,
    done: i64,
    _stream_size: i64,
    _stream_done: i64,
    _stream: u32,
    _reason: u32,
    _source: HANDLE,
    _destination: HANDLE,
    data: *const c_void,
) -> u32 {
    // SAFETY: `data` is the `Progress` that `copy_file` passed, alive and not otherwise borrowed while
    // `CopyFileExW` runs; the routine is called on the copying thread.
    let progress = unsafe { &mut *data.cast_mut().cast::<Progress>() };
    progress.created = true;
    let bytes = |value: i64| u64::try_from(value).unwrap_or(0);
    let report = &mut progress.report;
    let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        report(bytes(done), bytes(total))
    }));
    match answer {
        Ok(true) => PROGRESS_CONTINUE,
        Ok(false) => PROGRESS_CANCEL,
        Err(panic) => {
            progress.panicked = Some(panic);
            PROGRESS_CANCEL
        }
    }
}

/// Opens a file to read it past the file cache: what comes is what the disk holds. The buffers must be
/// aligned to the sector and a whole number of sectors long.
pub fn open_past_cache(path: &Path) -> io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_NO_BUFFERING)
        .open(path)
}

/// The bytes the user may still write in the folder `dir` (quotas counted); `None` when the file
/// system does not say.
pub fn free_space(dir: &Path) -> Option<u64> {
    let name = wide_os(dir.as_os_str());
    let mut available = 0u64;
    // SAFETY: `name` is NUL-terminated; `available` is a valid out pointer; the other two may be null.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            name.as_ptr(),
            &raw mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(available)
}

/// Renames `source` to `target` within one volume (`CrossesDevices` across volumes). With `replace` an
/// existing file `target` is replaced in one step (until it succeeds it is as it was; a read-only one
/// is refused); without it an existing `target` is never touched (`AlreadyExists`), unlike
/// `std::fs::rename`.
pub fn move_file(source: &Path, target: &Path, replace: bool) -> io::Result<()> {
    let (from, to) = (wide_path(source), wide_path(target));
    let flags = if replace {
        MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH
    } else {
        MOVEFILE_WRITE_THROUGH
    };
    // SAFETY: both names are NUL-terminated and outlive the call.
    let moved = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) };
    if moved == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Sets or clears the read-only attribute and keeps the others.
pub fn set_read_only(path: &Path, read_only: bool) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    let attributes = std::fs::symlink_metadata(path)?.file_attributes();
    let attributes = if read_only {
        attributes | FILE_ATTRIBUTE_READONLY
    } else {
        attributes & !FILE_ATTRIBUTE_READONLY
    };
    let name = wide_path(path);
    // SAFETY: `name` is NUL-terminated and outlives the call.
    if unsafe { SetFileAttributesW(name.as_ptr(), attributes) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The volume serial number and file index of what `path` leads to (links followed): two names with
/// the same identity are one file (hard links, junctions).
pub fn file_identity(path: &Path) -> io::Result<(u32, u64)> {
    use std::os::windows::fs::OpenOptionsExt;
    // No data access: opens files other programs hold, and folders (backup semantics).
    let file = std::fs::OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    open_file_identity(&file)
}

/// The volume serial number and file index of a file open: the very file read, whatever is at its
/// path now.
pub fn open_file_identity(file: &std::fs::File) -> io::Result<(u32, u64)> {
    use std::os::windows::io::AsRawHandle;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the handle is open for the call; `info` is a valid out pointer.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    Ok((info.dwVolumeSerialNumber, index))
}

/// The volume of `path`: the serial number of the nearest folder that exists (a target not made yet
/// is on its parent's volume), or the volume's root where it cannot be read or is 0 (some network file
/// systems give none). The serial has 32 bits: two volumes with one number (a cloned disk) share a
/// queue, which only makes their tasks wait for each other.
pub fn volume_id(path: &Path) -> crate::platform::volumes::VolumeId {
    use crate::platform::volumes::VolumeId;
    let mut serial = None;
    for place in path.ancestors() {
        match file_identity(place) {
            Ok((number, _)) => {
                serial = Some(number).filter(|number| *number != 0);
                break;
            }
            // Only a place not made yet sends the look up: an unreachable share is not asked again.
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => break,
        }
    }
    match serial {
        Some(serial) => VolumeId::Number(u64::from(serial)),
        None => VolumeId::Root(volume_of(path)),
    }
}

/// The root of the volume that holds `path` (`C:\`, a mount point folder, `\server\share\`), in
/// upper case so two spellings compare equal; the path's own root when Windows cannot say (a path that
/// does not exist yet still has its volume).
pub fn volume_of(path: &Path) -> std::path::PathBuf {
    let name = wide_os(path.as_os_str());
    let mut buffer = vec![0u16; name.len().max(261) + 1];
    let length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: `name` is NUL-terminated; `buffer` holds `length` characters and outlives the call.
    let ok = unsafe { GetVolumePathNameW(name.as_ptr(), buffer.as_mut_ptr(), length) };
    let root = if ok != 0 {
        let end = buffer
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    } else {
        path.components()
            .take(2)
            .collect::<std::path::PathBuf>()
            .display()
            .to_string()
    };
    std::path::PathBuf::from(root.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_and_a_place_not_made_yet_in_it_are_on_one_volume() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        let id = volume_id(temp.path());
        assert!(
            matches!(id, crate::platform::volumes::VolumeId::Number(_)),
            "{id:?}"
        );
        assert_eq!(volume_id(&folder), id);
        assert_eq!(volume_id(&folder.join("not").join("yet")), id);
        let upper = std::path::PathBuf::from(temp.path().to_string_lossy().to_uppercase());
        assert_eq!(volume_id(&upper), id, "case aside");
    }

    #[test]
    fn a_panic_in_the_progress_goes_on_after_the_copy_and_leaves_nothing() {
        // Review R52: unwinding through `CopyFileExW` would abort the program.
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("a.bin");
        std::fs::write(&source, vec![7u8; 300_000]).unwrap();
        let target = temp.path().join("b.bin");
        let copied = std::panic::catch_unwind(|| {
            copy_file(&source, &target, &mut |_, _| {
                panic!("progress panics on purpose")
            })
        });
        assert!(copied.is_err(), "the panic came back to the caller");
        assert!(!target.exists(), "no half copy");
    }

    #[test]
    fn a_copy_reports_progress_and_never_replaces_a_file() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("a.bin");
        std::fs::write(&source, vec![7u8; 300_000]).unwrap();
        let target = temp.path().join("b.bin");
        let mut last = (0, 0);
        copy_file(&source, &target, &mut |done, total| {
            last = (done, total);
            true
        })
        .unwrap();
        assert_eq!(last, (300_000, 300_000));
        assert_eq!(std::fs::read(&target).unwrap().len(), 300_000);

        std::fs::write(&target, "kept").unwrap();
        let error = copy_file(&source, &target, &mut |_, _| true).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "kept");
    }

    #[test]
    fn a_hard_link_has_the_identity_of_its_file() {
        let temp = tempfile::tempdir().unwrap();
        let (file, link, other) = (
            temp.path().join("a"),
            temp.path().join("b"),
            temp.path().join("c"),
        );
        std::fs::write(&file, "x").unwrap();
        std::fs::write(&other, "x").unwrap();
        std::fs::hard_link(&file, &link).unwrap();
        assert_eq!(file_identity(&file).unwrap(), file_identity(&link).unwrap());
        assert_ne!(
            file_identity(&file).unwrap(),
            file_identity(&other).unwrap()
        );
        assert!(file_identity(temp.path()).is_ok(), "folders too");
    }

    #[test]
    fn the_volume_of_a_path_is_its_root_also_before_it_exists() {
        let temp = tempfile::tempdir().unwrap();
        let volume = volume_of(temp.path());
        assert_eq!(volume_of(&temp.path().join("not-yet").join("x")), volume);
        let drive = temp.path().components().next().unwrap();
        let drive = drive.as_os_str().to_string_lossy().to_uppercase();
        assert!(volume.to_string_lossy().starts_with(&drive), "{volume:?}");
    }

    #[test]
    fn a_cancelled_copy_leaves_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("a.bin");
        std::fs::write(&source, vec![1u8; 3_000_000]).unwrap();
        let target = temp.path().join("b.bin");
        let error = copy_file(&source, &target, &mut |done, _| done == 0).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(!target.exists());
    }

    #[test]
    fn free_space_is_known_and_a_move_replaces_only_when_asked() {
        let temp = tempfile::tempdir().unwrap();
        assert!(free_space(temp.path()).is_some_and(|free| free > 0));
        let (new, old) = (temp.path().join("new"), temp.path().join("old"));
        std::fs::write(&new, "new").unwrap();
        std::fs::write(&old, "old").unwrap();
        let error = move_file(&new, &old, false).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "old");
        move_file(&new, &old, true).unwrap();
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "new");
        assert!(!new.exists());
    }
}
