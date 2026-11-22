//! The panel's keys for the favorites (FAR's Network `GetResourceParent` with `GetFavoritesParent`,
//! `RemoveItems`, `EditFavorites`): ".." (Enter on it, Ctrl+PgUp) on a computer kept in the favorites goes
//! up to the Favorites folder with "Browse up to favorites if possible", the computer under the cursor;
//! F8 in the Favorites folder removes its computers after FAR's question; Shift+F4 on one there tells
//! what it is (FAR's message), elsewhere does nothing.

use nc_plugin_sdk::system::network::NetResource;
use nc_plugin_sdk::{KeyAnswer, KeyPress, Message, Which};

use crate::NetworkPlugin;
use crate::browse::split;
use crate::favorites::{self, FOLDER, Favorites};
use crate::options::Options;

/// The Favorites folder's path.
pub fn folder_path() -> String {
    format!("/{FOLDER}")
}

/// Whether `path` is the Favorites folder.
pub fn in_folder(path: &str) -> bool {
    path.eq_ignore_ascii_case(&folder_path())
}

/// FAR's `RemoveItems` question for `names`.
pub fn remove_question(names: &[String]) -> String {
    match names {
        [one] => format!("Remove \"{one}\" from Favorites"),
        _ => format!("Remove {} items from Favorites", names.len()),
    }
}

/// FAR's `GetResourceParent` with `GetFavoritesParent` for ".." at `path`, the folder of `resource`:
/// the Favorites folder and the name under its cursor when it goes up there (not at the top, not from
/// the Favorites folder's own entries, whose ".." is the folder already); none: the program's "..".
pub fn favorites_parent(
    path: &str,
    resource: &NetResource,
    options: &Options,
    favorites: &Favorites,
) -> Option<(String, String)> {
    if path == "/" || path.is_empty() {
        return None;
    }
    let (parent, name) = split(path);
    if in_folder(parent) || !crate::start::up_to_favorites(resource, options, favorites) {
        return None;
    }
    Some((folder_path(), name.to_string()))
}

impl NetworkPlugin {
    /// ".." on the folder at `path`: the Favorites folder when it goes up there (`favorites_parent`),
    /// the name under the cursor; none: the program's "..".
    pub(crate) fn up_to_favorites(&mut self, path: &str) -> Option<KeyAnswer> {
        let folder = (self.browser)
            .resolve(
                &favorites::favored(&self.net, &self.favorites.borrow()),
                &self.options,
                path,
            )
            .ok()
            .flatten()?;
        let (up, name) = favorites_parent(path, &folder, &self.options, &self.favorites.borrow())?;
        // Refused, the panel stays where it is and the program tells why: nothing more to do here.
        let _ = (self.host).set_folder(Which::Active, &up, Some(&name));
        Some(KeyAnswer::default())
    }

    /// F8 in the Favorites folder (FAR's `RemoveItems`): its computers removed after the question; the
    /// panel read again.
    pub(crate) fn remove_favorites(&mut self, names: &[String]) -> KeyAnswer {
        if names.is_empty() {
            return KeyAnswer::default();
        }
        let question = remove_question(names);
        let message = Message::new("Remove from Favorites", &[&question])
            .warning()
            .buttons(&["&Ok", "&Cancel"]);
        if self.host.show(&message) != Some(0) {
            return KeyAnswer::default();
        }
        let mut kept = self.favorites.borrow_mut();
        for name in names {
            kept.remove(name);
        }
        self.host.set_setting(favorites::SETTING, &kept.text());
        KeyAnswer::reread()
    }

    /// Shift+F4 (FAR's `EditFavorites`): on a computer of the Favorites folder FAR's message.
    pub(crate) fn tell_favorite(&self, path: &str, press: &KeyPress) -> KeyAnswer {
        if in_folder(path) && press.current.is_some() {
            let message = Message::new("Info", &["This is a SERVER"]).buttons(&["&Ok"]);
            self.host.show(&message);
        }
        KeyAnswer::default()
    }
}

#[cfg(test)]
#[path = "favorite_keys_tests.rs"]
mod tests;
