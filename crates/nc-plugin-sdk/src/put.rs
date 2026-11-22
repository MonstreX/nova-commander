//! Files and folders of a disk taken in by a panel that takes paths (FAR's `PutFilesW`, API 5's request):
//! their paths, moving or not, why the program asks ([`GetMode`], FAR's `OPM_*`) and the folder they were
//! taken from.

use std::ffi::c_void;
use std::path::PathBuf;

use crate::api::{NcPut, NcStr};
use crate::files::{code, refuse};
use crate::panel::host_text;
use crate::{GetMode, Plugin, state};

/// What the program asks [`Plugin::put`] to take in (FAR's `PutFilesInfo`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PutRequest {
    /// The files and folders on a disk, by their absolute paths.
    pub sources: Vec<PathBuf>,
    /// Moved (F6): the plugin removes the sources it took.
    pub moving: bool,
    /// [`GetMode::edit`]: the editor's copy put back after F4 (FAR's `OPM_EDIT`); none: F5 / F6.
    pub mode: GetMode,
    /// The folder they were taken from (FAR's `SrcPath`).
    pub folder: Option<PathBuf>,
}

impl PutRequest {
    /// The host's request as the trait's.
    ///
    /// # Safety
    /// `raw`'s texts are the host's, alive through the call.
    unsafe fn of(raw: &NcPut) -> Self {
        let sources = if raw.sources.is_null() || raw.count == 0 {
            Vec::new()
        } else {
            // SAFETY: the caller promises `count` texts at `sources`.
            let texts = unsafe { std::slice::from_raw_parts(raw.sources, raw.count) };
            // SAFETY: each is the host's text, alive through the call.
            (texts.iter().map(|text| unsafe { host_text(*text) }))
                .map(PathBuf::from)
                .collect()
        };
        // SAFETY: the caller promises the host's text.
        let folder = unsafe { host_text(raw.folder) };
        Self {
            sources,
            moving: raw.moving != 0,
            mode: GetMode::of(raw.mode),
            folder: (!folder.is_empty()).then(|| PathBuf::from(folder)),
        }
    }
}

pub(crate) unsafe extern "C" fn put<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    put: *const NcPut,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    if put.is_null() {
        return refuse(state, "Nothing to take in".to_string());
    }
    // SAFETY: the host's texts and request live through the call.
    let (instance, path, request) =
        unsafe { (host_text(instance), host_text(path), PutRequest::of(&*put)) };
    code(
        state,
        |plugin| plugin.put(instance, path, &request),
        |_, ()| {},
    )
}

#[cfg(test)]
#[path = "put_tests.rs"]
mod tests;
