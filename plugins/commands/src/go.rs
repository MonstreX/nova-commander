//! A path gone to as FAR's farcmds goes (`OpenW` after `goto:` and `whereis:`): the active panel to the
//! path's folder, the cursor on its name (letters' case not counted) made the top line. A path ending in
//! `\` goes to the folder only. FAR finds nothing first: the path is taken as it is.

use nc_plugin_sdk::{Host, Which};

use crate::text;

/// The active panel to `path`'s folder, its cursor on `path`'s name.
pub fn to(host: &Host, path: &str) {
    if path.is_empty() {
        return;
    }
    let at = text::name_start(path);
    let folder = text::unquote(path[..at].trim());
    let name = text::unquote(path[at..].trim());
    if !folder.is_empty() {
        let _ = host.set_folder(Which::Active, &folder, None);
    }
    if name.is_empty() {
        return;
    }
    let items = host.panel_items(Which::Active);
    let found = (items.iter()).position(|item| {
        let own = &item.name[text::name_start(&item.name)..];
        own.to_lowercase() == name.to_lowercase()
    });
    if let Some(at) = found {
        let _ = host.set_cursor(Which::Active, at, Some(at));
    }
}
