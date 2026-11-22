//! The editor's document written into a file (S2d.4b, R7 review C1, C3): streamed beside it and put in
//! its place (`vfs::local::ops::save_streamed`), the bytes it did not change from what was read or from
//! the file it was read from, kept open; its undo steps moved onto the saved file.

use std::path::Path;

use super::editor_actions::Empty;
use crate::editor::view::pieces::PieceTable;
use crate::vfs::RandomRead;

/// The bytes of the old file a document's undo may read in over a save (more: its steps let go).
const UNDO_KEPT: u64 = 64 << 20;

/// Where a save takes the bytes the document did not change: the file it was read from, kept open
/// (`held`), and whether another program rewrote it in place since.
pub(super) struct Original<'a> {
    pub path: &'a Path,
    pub held: Option<crate::jobs::file_reader::HeldFile>,
    pub in_place: bool,
}

/// Why a document rewritten under it is not saved.
const REWRITTEN: &str = "The file was rewritten by another program before the editor read all of \
                         it: saved, the text would mix with the new file's";

/// The document written to `path` beside it and put in its place (`vfs::local::ops::save_streamed`:
/// the exact name, the old file's attributes, ACL and streams kept, every name of a file with several
/// written, no link replaced), the bytes it did not change taken as `from` says (what was read, or the
/// file it was opened from); the document moved onto the saved file with its steps (none when they
/// could not be kept).
pub(super) fn save_to(
    from: Original<'_>,
    (path, convert): (&Path, Option<super::editor_convert::Convert>),
    doc: &PieceTable,
    written: &std::sync::atomic::AtomicU64,
) -> std::io::Result<Option<PieceTable>> {
    let Original {
        path: source,
        held,
        in_place,
    } = from;
    // The bytes not changed are taken from what was read (kept whole, `KEPT_WHOLE`); rewritten under
    // it, a document not read whole would mix with the new file's bytes: not saved.
    if in_place && !doc.original_held() {
        return Err(std::io::Error::other(REWRITTEN));
    }
    // FAR's `CreatePath`: Shift+F4's file in a folder not there yet (R7 review D1).
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let place = crate::vfs::VfsPath::local(source);
    // The bytes not edited come from the file the document was read from, kept open (review C2: never
    // from what another editor's save put at its path since).
    let opened =
        || crate::vfs::provider_for(&place).and_then(|provider| provider.open_random(&place));
    let original: Box<dyn RandomRead> = match held {
        Some(held) => Box::new(crate::jobs::file_reader::HeldRead(held)),
        None => match opened() {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Box::new(Empty),
            Err(error) => return Err(error),
        },
    };
    let stop = crate::vfs::stop::current();
    let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
    let original = std::cell::RefCell::new(Some(original));
    let mut moved = None;
    crate::vfs::local::ops::save_streamed(
        path,
        &mut |out| {
            let mut progress = |done| {
                written.store(done, std::sync::atomic::Ordering::Relaxed);
                !stopped()
            };
            let mut borrowed = original.borrow_mut();
            let Some(file) = borrowed.as_mut() else {
                return Ok(());
            };
            // Shift+F2's conversion as the bytes go (R7 review D5).
            let Some(convert) = convert else {
                return doc.write_to(file.as_mut(), out, &mut progress);
            };
            let mut converter = super::editor_convert::Converter::new(out, convert)?;
            doc.write_to(file.as_mut(), &mut converter, &mut progress)?;
            converter.finish()
        },
        // Read while the old file is there: the steps' bytes the saved one has no more; then it is
        // let go, so that the saved file can take its place.
        // Rewritten in place, the file has no step's bytes: no steps kept.
        &mut || {
            // Converted, the saved bytes are no steps' bytes: none kept (it is read again).
            if let Some(mut file) = original.borrow_mut().take()
                && !in_place
                && convert.is_none()
            {
                moved = doc.rebased(file.as_mut(), UNDO_KEPT).unwrap_or(None);
            }
        },
    )?;
    Ok(moved)
}
