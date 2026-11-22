//! The places files were viewed and edited at, kept between runs (FAR's `FilePositionCache` with
//! `ViewerPosCache` and `EditorPosCache`; S2d.6b, S2d.6d): for each file by its full path, the
//! viewer's first byte shown and first column, its code page, its mode and wrapping when they were
//! changed, its bookmarks; the editor's cursor, its screen and its code page. FAR's
//! `MaxPositionCache`: the last 512 files of each. Each store is read anew for each save, so two
//! copies of the program keep each other's files.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::platform::files::fold_case;

/// FAR's `MaxPositionCache`.
const KEPT: usize = 512;

/// A file's place in the viewer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewerPosition {
    pub position: u64,
    pub left: usize,
    /// The code page shown (0: none kept).
    pub page: u32,
    /// The mode and the wrapping as the user left them (FAR's `m_mode_changed`; none: not changed).
    pub mode: Option<String>,
    pub wrap: Option<bool>,
    /// Wrapped at the words (FAR's `m_mode_wrap_words`).
    pub words: Option<bool>,
    /// Ctrl+Shift+0..9's bookmarks: a byte and a column each.
    pub bookmarks: Vec<Option<(u64, usize)>>,
}

/// A file's place in the editor (FAR's `EditorPosCache`): the cursor's line by its first byte and
/// its column, the first row's line by its first byte and the first column shown, the code page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorPosition {
    pub line: u64,
    pub col: usize,
    pub top: u64,
    pub left: usize,
    /// The code page (0: none kept).
    pub page: u32,
    /// Ctrl+Shift+0..9's bookmarks (FAR's `bm`): the line's first byte, the column, the first
    /// column shown, the row on the screen.
    pub bookmarks: Vec<Option<(u64, usize, usize, usize)>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Entry<T> {
    file: String,
    #[serde(alias = "viewer")]
    place: T,
}

/// The viewer's positions' file beside the settings file.
pub fn positions_path(settings_path: &Path) -> PathBuf {
    settings_path.with_file_name("positions.json")
}

/// The editor's positions' file beside the viewer's.
pub fn editor_positions_path(viewer_store: &Path) -> PathBuf {
    viewer_store.with_file_name("editor-positions.json")
}

/// The store's entries; none when it is not there. One that cannot be read is an error: never written
/// over (R7 review E5: every place kept was lost to one broken byte).
fn read<T: DeserializeOwned>(path: &Path) -> std::io::Result<Vec<Entry<T>>> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

/// A store that cannot be parsed put aside as `<name>.unreadable` (one there already: kept, this one
/// stays and nothing is kept), so that keeping places goes on in a new one.
fn put_aside(path: &Path) -> std::io::Result<()> {
    let aside = path.with_extension("json.unreadable");
    if aside.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} cannot be read and is left as it is", path.display()),
        ));
    }
    std::fs::rename(path, aside)
}

fn key(file: &Path) -> String {
    fold_case(&file.to_string_lossy()).into_owned()
}

/// The viewer's place kept for `file`.
pub fn viewer_position(path: &Path, file: &Path) -> Option<ViewerPosition> {
    position(path, file)
}

/// `file`'s place in the viewer kept.
pub fn keep_viewer_position(
    path: &Path,
    file: &Path,
    viewer: ViewerPosition,
) -> std::io::Result<()> {
    keep_position(path, file, viewer)
}

/// The place kept for `file` in the store at `path`.
pub fn position<T: DeserializeOwned>(path: &Path, file: &Path) -> Option<T> {
    let key = key(file);
    read(path)
        .ok()?
        .into_iter()
        .rev()
        .find(|entry| entry.file == key)
        .map(|entry| entry.place)
}

/// `file`'s place kept in the store at `path` (the latest last, the oldest past 512 gone).
pub fn keep_position<T: Serialize + DeserializeOwned>(
    path: &Path,
    file: &Path,
    place: T,
) -> std::io::Result<()> {
    let key = key(file);
    let mut entries = match read::<T>(path) {
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
            put_aside(path)?;
            Vec::new()
        }
        read => read?,
    };
    entries.retain(|entry| entry.file != key);
    entries.push(Entry { file: key, place });
    let excess = entries.len().saturating_sub(KEPT);
    entries.drain(..excess);
    let text = serde_json::to_string_pretty(&entries).map_err(std::io::Error::other)?;
    // Whole or not at all, the exact name, as the settings (R7 review E5).
    crate::vfs::local::ops::write_atomic(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_is_kept_by_the_file_s_name_and_the_oldest_go() {
        let temp = tempfile::tempdir().unwrap();
        let store = temp.path().join("positions.json");
        let file = temp.path().join("A.txt");
        assert_eq!(viewer_position(&store, &file), None);
        let place = ViewerPosition {
            position: 42,
            page: 1251,
            ..ViewerPosition::default()
        };
        keep_viewer_position(&store, &file, place.clone()).unwrap();
        let other_case = temp.path().join("a.TXT");
        assert_eq!(
            viewer_position(&store, &other_case),
            Some(place),
            "the name's case aside"
        );
        for n in 0..KEPT {
            let other = temp.path().join(format!("{n}.txt"));
            keep_viewer_position(&store, &other, ViewerPosition::default()).unwrap();
        }
        assert_eq!(viewer_position(&store, &file), None, "512 files after it");
    }

    #[test]
    fn the_editor_s_places_are_kept_apart_and_old_viewer_entries_read() {
        let temp = tempfile::tempdir().unwrap();
        let store = temp.path().join("positions.json");
        let file = temp.path().join("e.txt");
        let old = format!(r#"[{{"file":{:?},"viewer":{{"position":7}}}}]"#, key(&file));
        std::fs::write(&store, old).unwrap();
        assert_eq!(viewer_position(&store, &file).map(|p| p.position), Some(7));
        let editor = editor_positions_path(&store);
        let place = EditorPosition {
            line: 10,
            col: 3,
            top: 5,
            left: 1,
            page: 866,
            bookmarks: vec![None, Some((4, 1, 0, 2))],
        };
        keep_position(&editor, &file, place.clone()).unwrap();
        assert_eq!(position::<EditorPosition>(&editor, &file), Some(place));
        assert_eq!(viewer_position(&store, &file).map(|p| p.position), Some(7));
    }

    /// R7 review E5: a store that cannot be read is put aside whole, never written over; a second one
    /// then stays as it is and nothing is kept.
    #[test]
    fn a_broken_store_is_put_aside_not_written_over() {
        let temp = tempfile::tempdir().unwrap();
        let store = temp.path().join("positions.json");
        let file = temp.path().join("a.txt");
        std::fs::write(&store, "[{\"file\": broken").unwrap();
        assert_eq!(viewer_position(&store, &file), None);
        keep_viewer_position(&store, &file, ViewerPosition::default()).unwrap();
        let aside = temp.path().join("positions.json.unreadable");
        assert_eq!(
            std::fs::read_to_string(&aside).unwrap(),
            "[{\"file\": broken"
        );
        assert!(viewer_position(&store, &file).is_some(), "a new store");
        std::fs::write(&store, "also broken").unwrap();
        assert!(keep_viewer_position(&store, &file, ViewerPosition::default()).is_err());
        assert_eq!(std::fs::read_to_string(&store).unwrap(), "also broken");
        assert_eq!(
            std::fs::read_to_string(&aside).unwrap(),
            "[{\"file\": broken"
        );
    }
}
