//! A plugin's copy done as one of the program's own background tasks (API 9's `NcHost::transfer`, S6.8a;
//! NetBox's "Transfer on background"): the entries of its panel's folder to a disk's folder, or a disk's
//! entries into its panel's folder, copied (or moved) by the program's engine through the plugin's file
//! calls - the progress dialog in front (Background sends it behind), its questions, F12's list.

use crate::app::App;
use crate::jobs::transfer::{Existing, Keep, Request};
use crate::plugin_host::panel_requests::TransferRequest;
use crate::vfs::VfsPath;

/// The program's task of `plugin`'s `transfer`: its sources and its target.
/// None for a panel name that is no path's part, a disk folder not absolute, or a name that is not
/// one entry's of its folder (R7 review A4: `x\..\..\y` read and moved from outside it).
pub(super) fn request_of(plugin: &str, transfer: &TransferRequest) -> Option<Request> {
    let absolute = std::path::Path::new(&transfer.disk).is_absolute();
    if crate::plugin_host::bad_instance(&transfer.instance).is_some() || !absolute {
        return None;
    }
    let panel = VfsPath::in_session(plugin, &transfer.instance, &transfer.folder);
    let disk = VfsPath::local(&transfer.disk);
    let (from, target) = match transfer.upload {
        true => (&disk, panel.clone()),
        false => (&panel, disk.clone()),
    };
    let names = (transfer.names.iter()).filter(|name| !matches!(name.as_str(), "" | "." | ".."));
    let sources = names
        .map(|name| from.accepts_name(name).then(|| from.join(name)))
        .collect::<Option<Vec<VfsPath>>>()?;
    if sources.is_empty() {
        return None;
    }
    Some(Request {
        sources,
        target,
        move_: transfer.moving,
        existing: Existing::Ask,
        ask_read_only: true,
        keep: Keep::default(),
        retrying: Vec::new(),
        link: None,
        filter: None,
    })
}

impl App {
    /// `plugin`'s copy started as a task in front; false when it names nothing to copy.
    pub(super) fn plugin_transfer(&mut self, plugin: &str, transfer: &TransferRequest) -> bool {
        let Some(request) = request_of(plugin, transfer) else {
            return false;
        };
        self.start_in_front(request);
        true
    }
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
