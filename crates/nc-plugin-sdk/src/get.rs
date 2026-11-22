//! A panel's files written out by the plugin itself (FAR's `GetFilesW`, API 5): the entries, the folder
//! on a disk, moving or not, and why the program asks ([`GetMode`], FAR's `OPM_*`).

use std::ffi::c_void;
use std::path::PathBuf;

use crate::api::{
    GET_EDIT, GET_FIND, GET_QUICK_VIEW, GET_SILENT, GET_TOP_LEVEL, GET_VIEW, NcGet, NcStr,
};
use crate::files::{code, refuse};
use crate::panel::host_text;
use crate::{Plugin, state};

/// Why the program asks for the files (FAR's `OPM_*` of `GetFilesW`); none set: F5 / F6, the plugin's
/// dialog first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GetMode {
    /// No windows but the errors' (`OPM_SILENT`).
    pub silent: bool,
    /// For Find file's search in the files: no windows at all (`OPM_FIND`).
    pub find: bool,
    /// One file for the viewer, into a folder of the program's (`OPM_VIEW`).
    pub view: bool,
    /// One file for the editor (`OPM_EDIT`).
    pub edit: bool,
    /// Archives of a disk's panel at once (`OPM_TOPLEVEL`).
    pub top_level: bool,
    /// For the quick view: no windows at all (`OPM_QUICKVIEW`).
    pub quick_view: bool,
}

impl GetMode {
    /// The ABI's `GET_*` bits as the mode.
    pub fn of(bits: u32) -> Self {
        let on = |bit: u32| bits & bit != 0;
        Self {
            silent: on(GET_SILENT),
            find: on(GET_FIND),
            view: on(GET_VIEW),
            edit: on(GET_EDIT),
            top_level: on(GET_TOP_LEVEL),
            quick_view: on(GET_QUICK_VIEW),
        }
    }

    /// The plugin shows its dialog (FAR's arclite: none for the viewer, the editor, Find file, the quick
    /// view, nor a silent call).
    pub fn with_dialog(self) -> bool {
        !(self.silent || self.find || self.view || self.edit || self.quick_view)
    }
}

/// What the program asks [`Plugin::get`] to write out (FAR's `GetFilesInfo`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GetRequest {
    /// Entries of the panel's folder by their names.
    pub names: Vec<String>,
    /// The folder on a disk they go to (FAR's `DestPath`).
    pub destination: PathBuf,
    /// Moved (F6): what was written out is removed from the panel.
    pub moving: bool,
    pub mode: GetMode,
    /// The program's folder for names given without one (FAR's current folder).
    pub folder: Option<PathBuf>,
}

impl GetRequest {
    /// The host's request as the trait's.
    ///
    /// # Safety
    /// `raw`'s texts and names are the host's, alive through the call.
    unsafe fn of(raw: &NcGet) -> Self {
        let names = if raw.names.is_null() || raw.count == 0 {
            Vec::new()
        } else {
            // SAFETY: the caller promises `count` texts at `names`.
            let texts = unsafe { std::slice::from_raw_parts(raw.names, raw.count) };
            // SAFETY: each is the host's text, alive through the call.
            (texts.iter().map(|text| unsafe { host_text(*text) }))
                .map(str::to_string)
                .collect()
        };
        // SAFETY: the caller promises the host's texts.
        let (destination, folder) = unsafe { (host_text(raw.destination), host_text(raw.folder)) };
        Self {
            names,
            destination: PathBuf::from(destination),
            moving: raw.moving != 0,
            mode: GetMode::of(raw.mode),
            folder: (!folder.is_empty()).then(|| PathBuf::from(folder)),
        }
    }
}

pub(crate) unsafe extern "C" fn get<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    get: *const NcGet,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    if get.is_null() {
        return refuse(state, "Nothing to write out".to_string());
    }
    // SAFETY: the host's texts and request live through the call.
    let (instance, path, request) =
        unsafe { (host_text(instance), host_text(path), GetRequest::of(&*get)) };
    code(
        state,
        |plugin| plugin.get(instance, path, &request),
        |_, ()| {},
    )
}

#[cfg(test)]
#[path = "get_tests.rs"]
mod tests;
