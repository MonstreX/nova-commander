//! NetBox's "Location Profiles" (`TLocationProfilesDialog`): the session's and the shared bookmarks of a
//! local and a remote folder by their names; "Add" names the panels' folders now, "Remove" asks first
//! ("Do you wish to remove bookmark '...'"), "Rename" asks the new name, "Open" takes both panels to the
//! bookmark's folders; OK keeps the changes.

use nc_plugin_sdk::{Controls, Dialog, DialogEvent, DialogItem, Host, KeyAnswer, Message, Which};

use crate::NetWay;
use crate::bookmarks::{self, Mark};
use crate::placed::{at, line};

/// NetBox's `NB_LOCATION_PROFILES_*`, `NB_LOCATION_BOOKMARK_*`.
pub const TITLE: &str = "Location Profiles";
const NAME_PROMPT: &str = "Bookmark name:";
const RENAME_PROMPT: &str = "New name:";

/// NetBox's `NB_LOCATION_PROFILES_REMOVE_CONFIRM`.
pub fn remove_question(name: &str) -> String {
    format!("Do you wish to remove bookmark '{name}'")
}

// The items, in their order.
const SESSION: usize = 0;
const SHARED: usize = 1;
const LIST: usize = 2;
const LOCAL: usize = 4;
const REMOTE: usize = 6;
const ADD: usize = 7;
const REMOVE: usize = 8;
const RENAME: usize = 9;
const OPEN: usize = 10;
const OK: usize = 12;

/// The two lists of bookmarks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profiles {
    pub session: Vec<Mark>,
    pub shared: Vec<Mark>,
}

impl Profiles {
    /// The list shown (`shared` or the session's).
    pub fn list(&mut self, shared: bool) -> &mut Vec<Mark> {
        match shared {
            true => &mut self.shared,
            false => &mut self.session,
        }
    }

    /// A bookmark added under its name (NetBox's: none when the name is taken or empty): whether it was.
    pub fn add(&mut self, shared: bool, mark: Mark) -> bool {
        let list = self.list(shared);
        let added = !mark.name.is_empty() && !list.iter().any(|old| old.name == mark.name);
        if added {
            list.push(mark);
        }
        added
    }
}

/// The names of a list's bookmarks.
fn names(marks: &[Mark]) -> Vec<String> {
    marks.iter().map(|mark| mark.name.clone()).collect()
}

/// The dialog's items with the session's list shown.
pub fn items(profiles: &Profiles) -> Vec<DialogItem> {
    let shown = names(&profiles.session);
    let shown: Vec<&str> = shown.iter().map(String::as_str).collect();
    vec![
        at(DialogItem::radio("&Session profiles", true).group(), 5, 2),
        at(DialogItem::radio("S&hared profiles", false), 30, 2),
        DialogItem::list_box("", &shown, 0).at(5, 3, 64, 12),
        at(DialogItem::text("&Local directory:"), 5, 13),
        line(DialogItem::edit("").read_only(), 5, 14, 60),
        at(DialogItem::text("&Remote directory:"), 5, 15),
        line(DialogItem::edit("").read_only(), 5, 16, 60),
        at(DialogItem::button("&Add").no_close(), 5, 18),
        at(DialogItem::button("Re&move").no_close(), 14, 18),
        at(DialogItem::button("Re&name").no_close(), 26, 18),
        at(DialogItem::button("&Open"), 38, 18),
        DialogItem::separator().at(-1, 19, -1, 19),
        at(DialogItem::button("OK").default_button(), 26, 20),
        at(DialogItem::button("Cancel"), 33, 20),
    ]
}

fn shown_list(controls: &Controls) -> bool {
    controls.value(SHARED).checked()
}

fn cursor(controls: &Controls) -> usize {
    usize::try_from(controls.value(LIST).state).unwrap_or(0)
}

/// The list and the folders' lines as `profiles` has them (NetBox's `LoadBookmarks`, `UpdateControls`).
fn refresh(controls: &mut Controls, profiles: &mut Profiles, at: Option<usize>) {
    let marks = profiles.list(shown_list(controls)).clone();
    let lines = names(&marks);
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    controls.set_lines(LIST, &lines);
    if let Some(at) = at {
        controls.set_state(LIST, i32::try_from(at).unwrap_or(0));
    }
    let chosen = marks.get(cursor(controls));
    controls.set_text(LOCAL, chosen.map_or("", |mark| mark.local.as_str()));
    controls.set_text(REMOTE, chosen.map_or("", |mark| mark.remote.as_str()));
    for item in [REMOVE, RENAME, OPEN] {
        controls.enable(item, chosen.is_some());
    }
}

/// A button of the dialog done on `profiles`; `here`: the panels' folders now (local, remote).
fn pressed(
    host: &Host,
    controls: &mut Controls,
    profiles: &mut Profiles,
    (button, here): (usize, &Mark),
) {
    let shared = shown_list(controls);
    let at = cursor(controls);
    let current = profiles.list(shared).get(at).cloned();
    match (button, current) {
        (ADD, _) => {
            if let Some(name) = host.input(TITLE, NAME_PROMPT, "") {
                let mark = Mark {
                    name: name.trim().to_string(),
                    ..here.clone()
                };
                profiles.add(shared, mark);
            }
        }
        (REMOVE, Some(mark)) => {
            let question = remove_question(&mark.name);
            let message = Message::new(TITLE, &[&question]).buttons(&["&OK", "&Cancel"]);
            if host.show(&message) == Some(0) {
                profiles.list(shared).remove(at);
            }
        }
        (RENAME, Some(mark)) => {
            let taken = |name: &str| {
                profiles
                    .session
                    .iter()
                    .chain(&profiles.shared)
                    .any(|old| old.name == name)
            };
            if let Some(name) = host.input(TITLE, RENAME_PROMPT, &mark.name)
                && !name.trim().is_empty()
                && !taken(name.trim())
                && let Some(renamed) = profiles.list(shared).get_mut(at)
            {
                renamed.name = name.trim().to_string();
            }
        }
        _ => {}
    }
    refresh(controls, profiles, None);
    controls.focus(LIST);
}

impl NetWay {
    /// NetBox's "Location Profiles" of the connected panel `instance`.
    pub(crate) fn location_profiles(&mut self, instance: &str) -> KeyAnswer {
        let Some(remote) = self.remotes.get(instance) else {
            return KeyAnswer::default();
        };
        let file = self.host.data_folder().join(bookmarks::FILE);
        let key = remote.name.clone();
        let other = self.host.panel(Which::Passive);
        let local = match crate::browsing::regular(other.as_ref()) {
            true => other.map(|state| state.folder).unwrap_or_default(),
            false => String::new(),
        };
        let here = Mark {
            name: String::new(),
            remote: remote.folder.clone(),
            local,
        };
        let mut profiles = Profiles {
            session: bookmarks::load_marks(&file, &key),
            shared: bookmarks::load_marks(&file, ""),
        };
        let host = self.host;
        let dialog = Dialog::new(TITLE, items(&profiles)).size(70, 23);
        let closed = host.dialog_with(&dialog, |event, controls| {
            match event {
                DialogEvent::Init => refresh(controls, &mut profiles, Some(0)),
                DialogEvent::Changed(SESSION | SHARED) => refresh(controls, &mut profiles, Some(0)),
                DialogEvent::Changed(LIST) => refresh(controls, &mut profiles, None),
                DialogEvent::Button(button) if [ADD, REMOVE, RENAME].contains(&button) => {
                    pressed(&host, controls, &mut profiles, (button, &here));
                }
                _ => {}
            }
            true
        });
        if !matches!(closed.button, Some(OK | OPEN)) {
            return KeyAnswer::default();
        }
        let _ = bookmarks::save_marks(&file, &key, &profiles.session);
        let _ = bookmarks::save_marks(&file, "", &profiles.shared);
        if closed.button == Some(OPEN) {
            let shared = closed
                .values
                .get(SHARED)
                .is_some_and(|value| value.checked());
            let at = (closed.values.get(LIST))
                .map_or(0, |value| usize::try_from(value.state).unwrap_or(0));
            if let Some(mark) = profiles.list(shared).get(at).cloned() {
                if !mark.local.is_empty() {
                    let _ = host.set_folder(Which::Passive, &mark.local, None);
                }
                if !mark.remote.is_empty() {
                    // Both panels set by the profile: the local one does not follow again.
                    if let Some(remote) = self.remotes.get(instance) {
                        remote.unfollowed_to(&mark.remote);
                    }
                    let _ = host.set_folder(Which::Active, &mark.remote, None);
                }
            }
        }
        KeyAnswer::default()
    }
}

#[cfg(test)]
#[path = "location_profiles_tests.rs"]
mod tests;
