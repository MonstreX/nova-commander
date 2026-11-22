//! What a plugin writes out of its panel and takes into it (FAR's `GetFilesInfo` and `PutFilesInfo`, API 5):
//! the requests of [`NcPlugin::get`](crate::NcPlugin::get) and [`NcPlugin::put`](crate::NcPlugin::put).

use crate::NcStr;

/// What [`NcPlugin::get`](crate::NcPlugin::get) writes out and where (FAR's `GetFilesInfo`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcGet {
    /// `count` names of entries of the panel's folder.
    pub names: *const NcStr,
    pub count: usize,
    /// The folder on a disk they go to (FAR's `DestPath`; the plugin's dialog may change it).
    pub destination: NcStr,
    /// Non-zero: moved (F6), the plugin removing what it wrote out.
    pub moving: u32,
    /// `GET_*`; 0: F5 / F6, the plugin's dialog first.
    pub mode: u32,
    /// The program's folder for names given without one (FAR's current folder); empty: none.
    pub folder: NcStr,
}

/// What [`NcPlugin::put`](crate::NcPlugin::put) takes in (FAR's `PutFilesInfo`, API 5).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcPut {
    /// `count` files and folders on a disk by their absolute paths.
    pub sources: *const NcStr,
    pub count: usize,
    /// Non-zero: moved (F6), the plugin removing the sources it took.
    pub moving: u32,
    /// `GET_*` (FAR's `OPM_*`): [`GET_EDIT`](crate::GET_EDIT) the editor's copy put back after F4; 0:
    /// F5 / F6.
    pub mode: u32,
    /// The folder the sources were taken from (FAR's `SrcPath`); empty: none.
    pub folder: NcStr,
}
