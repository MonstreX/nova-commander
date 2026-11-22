//! A process's text where FAR's `GetFiles` puts it: read by the viewer and the quick view (F3, Ctrl+Q:
//! the entry is the text, made again at most each second), written by F5 as `<name>.txt` into the other
//! panel's folder at once (FAR shows no dialog for it) in FAR's UTF-16, shown by Shift+F3 after its
//! options (FAR's "View with Options...") from a temporary file the viewer deletes. The viewer's text is
//! UTF-8: the program's viewer reads no other until stage 2d. Writing many tells the progress and stops
//! on Esc.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nc_plugin_sdk::{Error, ErrorKind, FileEntry, KeyAnswer, KeyPress, Message, Which};

use crate::ProcList;
use crate::entries::{View, entry};
use crate::facts::gather;
use crate::info::{bytes, file_name, text};
use crate::modes::Translated;
use crate::options::Options;

/// How long a text made is read again before it is made anew (the quick view asks each second).
const FRESH: Duration = Duration::from_secs(1);
/// Texts older than this are dropped as the next is made.
const KEPT: Duration = Duration::from_secs(30);

fn not_there(key: &str) -> Error {
    Error::new(ErrorKind::NotFound, format!("{key} is not there"))
}

impl ProcList {
    /// The text of the entry `key` with `options`; none for a process no longer listed.
    fn made(&mut self, key: &str, options: &Options) -> Option<String> {
        let table = self.refresher().table().clone();
        let process = table.by_key(key)?;
        let host = self.host;
        let stop = move || host.cancelled();
        let facts = gather(process, &table, options, &stop);
        Some(text(&facts, options))
    }

    /// The entry's text as the viewer reads it. `anew`: made again unless made within a second (a
    /// stat: the viewer's size, the quick view's look each second); else the one there is, whatever its
    /// age, so that the reads after a stat get the text it told the size of.
    fn current_text(&mut self, key: &str, anew: bool) -> Result<&[u8], Error> {
        let now = Instant::now();
        self.texts.retain(|_, (made, _)| !stale(*made, now, KEPT));
        let there = self.texts.get(key).map(|(made, _)| *made);
        if there.is_none_or(|made| anew && stale(made, now, FRESH)) {
            let options = self.options;
            // UTF-8 for the viewer of now, which reads no other (UTF-16 with stage 2d's).
            let made = self.made(key, &options).ok_or_else(|| not_there(key))?;
            // Stamped once made: gathering may take long.
            self.texts
                .insert(key.to_string(), (Instant::now(), made.into_bytes()));
        }
        (self.texts.get(key))
            .map(|(_, text)| text.as_slice())
            .ok_or_else(|| not_there(key))
    }

    /// The entry `path` as the viewer sees it: its text's size.
    pub(crate) fn text_entry(&mut self, path: &str) -> Result<FileEntry, Error> {
        let key = path.trim_start_matches('/');
        if key.is_empty() {
            return Ok(FileEntry::folder(""));
        }
        let size = u64::try_from(self.current_text(key, true)?.len()).unwrap_or(u64::MAX);
        let table = self.refresher().table().clone();
        let process = table.by_key(key).ok_or_else(|| not_there(key))?;
        let mode = Translated::default();
        let view = View {
            mode: &mode,
            widths: &[],
            decimal: '.',
        };
        Ok(FileEntry {
            size,
            ..entry(process, &view, None, None)
        })
    }

    /// Up to `buffer.len()` bytes of the entry's text from `offset`.
    pub(crate) fn read_text(
        &mut self,
        path: &str,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize, Error> {
        let key = path.trim_start_matches('/');
        let text = self.current_text(key, false)?;
        Ok(slice(text, offset, buffer))
    }

    /// The folder F5 writes into: the other panel's (FAR's `AnotherPanel->GetCurDir()`).
    fn target_folder(&self, press: &KeyPress) -> Option<PathBuf> {
        press.passive.clone().or_else(|| {
            // A tree, a quick view or the info panel beside: its folder; a plugin's panel: none.
            let passive = self.host.panel(Which::Passive)?;
            let folder = passive.disk_folder;
            (passive.plugin.is_none() && !folder.is_empty()).then(|| PathBuf::from(folder))
        })
    }

    /// F5: each process's text written as `<name>.txt` into the other panel's folder, the progress told,
    /// Esc stopping it; one that cannot be written told and the rest left.
    pub(crate) fn write_texts(&mut self, press: &KeyPress) -> KeyAnswer {
        let Some(folder) = self.target_folder(press) else {
            return KeyAnswer::default();
        };
        let keys = press.targets();
        let total = u64::try_from(keys.len()).unwrap_or(0);
        let options = self.options;
        let mut written = 0;
        for (done, key) in keys.iter().enumerate() {
            if self.host.cancelled() {
                break;
            }
            self.host
                .progress(u64::try_from(done).unwrap_or(0), total, key);
            let Some(made) = self.made(key, &options) else {
                continue;
            };
            // Esc while it was made: its handles are cut short, it is not written.
            if self.host.cancelled() {
                break;
            }
            let path = folder.join(file_name(key));
            if let Err(error) = std::fs::write(&path, bytes(&made)) {
                self.cannot_write(&path, &error);
                break;
            }
            written += 1;
        }
        // FAR's `PluginGetFiles` clears the selection once all is done.
        if written == keys.len() {
            self.host.select_all(Which::Active, false);
        }
        KeyAnswer {
            reread_passive: true,
            ..KeyAnswer::default()
        }
    }

    fn cannot_write(&self, path: &Path, error: &std::io::Error) {
        let lines = [
            format!("Cannot write {}", path.display()),
            crate::keys::system_text(error),
        ];
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let message = Message::new("Error", &lines).warning().buttons(&["Ok"]);
        let _ = self.host.show(&message);
    }

    /// Shift+F3 (FAR's "View with Options..."): the text's parts asked for this view, then the viewer on
    /// a temporary file it deletes as it closes.
    pub(crate) fn view_with_options(&mut self, press: &KeyPress) -> KeyAnswer {
        let Some(key) = press.current.clone() else {
            return KeyAnswer::default();
        };
        let closed = self.host.dialog(&self.options.view_dialog());
        let Some(options) = self.options.from_view_dialog(&closed) else {
            return KeyAnswer::default();
        };
        let Some(made) = self.made(&key, &options) else {
            return KeyAnswer::default();
        };
        // A folder of its own each time (FAR's `MkTemp`): a second view never writes over the first's
        // file; the viewer deletes the file, the empty folders go as the next view starts.
        self.viewed_folders
            .retain(|folder| std::fs::remove_dir(folder).is_err());
        self.viewed += 1;
        let name = format!("vc-proclist-{}-{}", std::process::id(), self.viewed);
        let folder = std::env::temp_dir().join(name);
        let path = folder.join(file_name(&key));
        let written = std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(&path, made));
        self.viewed_folders.push(folder.clone());
        match written {
            Ok(()) if self.host.view(&path, true) => {}
            Ok(()) => {
                // Not viewed: nothing deletes it but this.
                let _ = std::fs::remove_file(&path);
            }
            Err(error) => self.cannot_write(&path, &error),
        }
        KeyAnswer::default()
    }
}

/// Whether a text made at `made` is older than `age` at `now`.
pub fn stale(made: Instant, now: Instant, age: Duration) -> bool {
    now.saturating_duration_since(made) >= age
}

/// Up to `buffer.len()` bytes of `text` from `offset` copied into `buffer`: how many (none past its end).
pub fn slice(text: &[u8], offset: u64, buffer: &mut [u8]) -> usize {
    let start = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(text.len());
    let count = buffer.len().min(text.len() - start);
    buffer[..count].copy_from_slice(&text[start..start + count]);
    count
}

#[cfg(test)]
#[path = "texts_tests.rs"]
mod tests;
