//! The list under the command line while typing (FAR's `EditControl::AutoComplete`, its defaults: shown,
//! not modal, nothing appended): commands from the history, then files, variables and programs. It does
//! not take the keys: typing goes on in the line and the list follows. Moving in the list puts its line
//! on the command line. The disk is read on a thread of its own.

use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, line_edit};
use crate::platform;
use crate::platform::files::{fold_case, paths};
use crate::terminal::Environment;

/// A line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// The first line, empty: what was typed.
    Typed,
    Text {
        text: String,
        from_history: bool,
    },
    /// A separator with a title, between the sources.
    Group(&'static str),
}

/// What the thread found for a line.
#[derive(Debug, PartialEq, Eq)]
struct Group {
    title: &'static str,
    items: Vec<String>,
}

struct Job {
    typed: String,
    found: Receiver<Vec<Group>>,
}

pub struct Completion {
    pub items: Vec<Item>,
    pub selected: usize,
    pub title: &'static str,
    typed: String,
    job: Option<Job>,
    /// The first line shown (FAR's `TopPos`), kept by the renderer.
    pub top: std::cell::Cell<usize>,
}

const HISTORY: &str = "History";
const FILES: &str = "Files";
const ENVIRONMENT: &str = "Environment";
/// A folder of tens of thousands of files is no list to choose from.
const MOST_FILES: usize = 1000;

impl Completion {
    /// FAR's condition: more than one line, or one that is not what is typed already.
    pub fn is_shown(&self) -> bool {
        let mut texts = self.items.iter().filter_map(|item| match item {
            Item::Text { text, .. } => Some(text),
            _ => None,
        });
        match (texts.next(), texts.next()) {
            (Some(_), Some(_)) => true,
            (Some(only), None) => fold_case(only) != fold_case(&self.typed),
            _ => false,
        }
    }

    fn add_group(&mut self, title: &'static str, texts: impl IntoIterator<Item = String>) {
        let mut texts = texts.into_iter().peekable();
        if texts.peek().is_none() {
            return;
        }
        // FAR's `AddSeparatorOrSetTitle`: the first source is the title, one separator after it.
        if self.items.len() == 1 {
            self.title = title;
        } else if !self.items.iter().any(|item| matches!(item, Item::Group(_))) {
            self.items.push(Item::Group(title));
        }
        let from_history = title == HISTORY;
        self.items
            .extend(texts.map(|text| Item::Text { text, from_history }));
    }

    /// `step` lines on, stopping at the ends (the page keys and Home, End); a titled line is passed over.
    fn paged(&self, step: isize) -> usize {
        let last = isize::try_from(self.items.len()).unwrap_or(1) - 1;
        let start = isize::try_from(self.selected).unwrap_or(0);
        let target = usize::try_from((start + step).clamp(0, last)).unwrap_or(0);
        if matches!(self.items[target], Item::Group(_)) {
            target + 1
        } else {
            target
        }
    }

    /// The next line that can be chosen, `step` lines on (the list goes round).
    fn moved(&self, step: isize) -> usize {
        let count = isize::try_from(self.items.len()).unwrap_or(1);
        let mut position = isize::try_from(self.selected).unwrap_or(0);
        loop {
            position = (position + step).rem_euclid(count);
            let index = usize::try_from(position).unwrap_or(0);
            if !matches!(self.items[index], Item::Group(_)) {
                return index;
            }
        }
    }
}

/// What divides words here: FAR's `WordDiv` and the blanks, without `":\\/%.?-`.
const WORD_DIVIDERS: &str = " \t~!^&*()+|{}<>`=[];',";

/// FAR's `ParseStringWithQuotes`: what stands before the last word, the word without quotes, and
/// whether it opened with a quote. `None` when there is no word.
fn last_word(line: &str) -> Option<(String, String, bool)> {
    let chars: Vec<char> = line.chars().collect();
    let mut start = 0;
    if chars.iter().filter(|&&ch| ch == '"').count() % 2 == 1 {
        start = chars.iter().rposition(|&ch| ch == '"')?;
    } else {
        // Backwards to a divider that is not inside quotes.
        let mut at = chars.len();
        while at > 0 {
            at -= 1;
            if chars[at] == '"' {
                while at > 0 {
                    at -= 1;
                    if chars[at] == '"' {
                        break;
                    }
                }
            } else if WORD_DIVIDERS.contains(chars[at]) {
                start = at + 1;
                break;
            }
        }
    }
    let quoted = chars.get(start) == Some(&'"');
    let word_at = start + usize::from(quoted);
    let word: String = chars[word_at..].iter().filter(|&&ch| ch != '"').collect();
    let prefix: String = chars[..word_at].iter().collect();
    (!word.is_empty()).then_some((prefix, word, quoted))
}

/// `name` as it goes on the line: closing what the word opened, or quoted when it holds what would
/// mean something else to the command interpreter (a file `a&b.txt` must not run `b.txt`).
fn on_the_line(prefix: &str, name: &str, quoted: bool) -> String {
    if quoted {
        format!("{prefix}{name}\"")
    } else {
        format!(
            "{prefix}{}",
            super::cmdline_insert::quote_keeping_variables(name)
        )
    }
}

/// The folder part of a word, as typed, and what the name begins with.
fn split_word(word: &str) -> (&str, &str) {
    match word.rfind(paths::ends_folder_part) {
        Some(end) => word.split_at(end + 1),
        None => ("", word),
    }
}

/// Whether `text` starts with `start` as the system compares names (case aside on Windows).
fn starts_with_ignoring_case(text: &str, start: &str) -> bool {
    fold_case(text).starts_with(fold_case(start).as_ref())
}

/// The folder a word's folder part stands for: a bare `C:` means where the panel is on that drive when
/// the panel is on it. `None` for `C:sub\`, relative to a folder of that drive nobody here knows.
fn folder_to_read(dir: &Path, folder: &str) -> Option<PathBuf> {
    let drive: String = folder.chars().take(2).collect();
    let Some(root) = paths::bare_drive_root(&drive) else {
        return Some(dir.join(folder));
    };
    let rest = &folder[drive.len()..];
    if rest.starts_with(std::path::is_separator) {
        return Some(PathBuf::from(folder));
    }
    if !rest.is_empty() {
        return None;
    }
    let same_drive = starts_with_ignoring_case(&dir.to_string_lossy(), &root.to_string_lossy());
    Some(if same_drive { dir.to_path_buf() } else { root })
}

/// FAR's `EnumFiles`: what lies in the word's folder (variables expanded) and begins as the word does.
fn files_for(word: &str, dir: &Path, environment: &Environment) -> Vec<String> {
    let (typed_folder, start) = split_word(word);
    let expanded = environment.expand(typed_folder);
    let Some(entries) = folder_to_read(dir, &expanded).and_then(|folder| read_dir(folder).ok())
    else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| starts_with_ignoring_case(name, start))
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.truncate(MOST_FILES);
    names
        .into_iter()
        .map(|name| format!("{typed_folder}{name}"))
        .collect()
}

/// The sources that need the disk.
fn find(line: &str, dir: &Path, environment: &Environment) -> Vec<Group> {
    let Some((prefix, word, quoted)) = last_word(line) else {
        return Vec::new();
    };
    let shown = |names: Vec<String>| -> Vec<String> {
        names
            .iter()
            .map(|name| on_the_line(&prefix, name, quoted))
            .collect()
    };
    let mut groups = vec![
        Group {
            title: FILES,
            items: shown(files_for(&word, dir, environment)),
        },
        Group {
            title: ENVIRONMENT,
            items: shown(platform::process::completion_variables(
                &word,
                &environment.names(),
            )),
        },
    ];
    // FAR: programs only for a line with no folder in it.
    if !line.contains(std::path::is_separator) {
        groups.push(Group {
            title: FILES,
            items: shown(platform::process::completion_programs(
                &word,
                environment.get("PATH").unwrap_or_default().as_ref(),
            )),
        });
    }
    groups
}

impl App {
    /// The command line has changed: the list follows it.
    pub(super) fn update_completion(&mut self) {
        let typed = self.command_line.text().to_string();
        if typed.is_empty() {
            self.completion = None;
            return;
        }
        let mut completion = Completion {
            items: vec![Item::Typed],
            selected: 0,
            title: "",
            typed: typed.clone(),
            job: self.completion.take().and_then(|old| old.job),
            top: std::cell::Cell::new(0),
        };
        let similar = (self.commands().rev())
            .filter(|known| starts_with_ignoring_case(known, &typed))
            .map(str::to_string);
        completion.add_group(HISTORY, similar);
        self.completion = Some(completion);
        self.start_completion_job();
    }

    /// Looks on the disk for what is typed, unless that is under way: a slow disk gets one look at a time.
    fn start_completion_job(&mut self) {
        let dir = self.active_folder();
        let environment = self.environment.clone();
        let Some(completion) = &mut self.completion else {
            return;
        };
        if completion.job.is_some() {
            return;
        }
        let typed = completion.typed.clone();
        let line = typed.clone();
        let (send, found) = mpsc::channel();
        let started = std::thread::Builder::new()
            .name("completion".to_string())
            .spawn(move || {
                // Nobody left to tell: the list was closed.
                let _ = send.send(find(&line, &dir, &environment));
            });
        // Without a thread nothing is looked for (review R24: no panic).
        if started.is_ok() {
            completion.job = Some(Job { typed, found });
        }
    }

    /// What the thread found joins the list; when the line has changed meanwhile it looks again.
    pub(super) fn pump_completion(&mut self) -> bool {
        let Some(completion) = &mut self.completion else {
            return false;
        };
        let Some(job) = &completion.job else {
            return false;
        };
        let groups = match job.found.try_recv() {
            Ok(groups) => groups,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Vec::new(),
        };
        let current = job.typed == completion.typed;
        completion.job = None;
        if !current {
            self.start_completion_job();
            return false;
        }
        // The line has been run, cleared or replaced meanwhile.
        if completion.selected == 0 && self.command_line.text() != completion.typed {
            self.completion = None;
            return true;
        }
        for group in groups {
            completion.add_group(group.title, group.items);
        }
        true
    }

    fn select_completion(&mut self, step: isize) {
        if let Some(completion) = &mut self.completion {
            completion.selected = completion.moved(step);
        }
        self.put_selected_on_the_line();
    }

    fn page_completion(&mut self, step: isize) {
        if let Some(completion) = &mut self.completion {
            completion.selected = completion.paged(step);
        }
        self.put_selected_on_the_line();
    }

    pub(super) fn put_selected_on_the_line(&mut self) {
        let Some(completion) = &self.completion else {
            return;
        };
        let text = match &completion.items[completion.selected] {
            Item::Text { text, .. } => text.clone(),
            _ => completion.typed.clone(),
        };
        self.command_line.set(&text);
    }

    /// Shift+Del on a line from the history: it leaves the history and the list (FAR's `DeleteIfUnlocked`).
    fn forget_completion_line(&mut self) {
        let Some(completion) = &mut self.completion else {
            return;
        };
        let Some(Item::Text {
            text,
            from_history: true,
        }) = completion.items.get(completion.selected).cloned()
        else {
            return;
        };
        completion.items.remove(completion.selected);
        // The next line takes its place (FAR); after the last history line, the titled line goes too.
        if !completion.items.iter().any(|item| {
            matches!(
                item,
                Item::Text {
                    from_history: true,
                    ..
                }
            )
        }) && completion.title == HISTORY
        {
            completion.title = "";
            if let Some(Item::Group(title)) = completion.items.get(1).cloned() {
                completion.title = title;
                completion.items.remove(1);
            }
        }
        completion.selected = completion.selected.min(completion.items.len() - 1);
        if matches!(completion.items[completion.selected], Item::Group(_)) {
            completion.selected -= 1;
        }
        self.forget_history_line(&text);
        self.put_selected_on_the_line();
        if !self.completion_shown() {
            self.completion = None;
        }
    }

    /// A key while the list is up: true when it has done all there is to do. The rest goes on as usual,
    /// typing back to the line (which brings the list up to date).
    pub(super) fn handle_completion_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        if ctrl && key.code == KeyCode::Char(' ') && !self.completion_shown() {
            self.update_completion();
            return true;
        }
        if !self.completion_shown() {
            // Not up yet: typing goes on; anything else ends the look for this line.
            if !is_editing_key(key) {
                self.completion = None;
            }
            return false;
        }
        let plain = key.modifiers.is_empty();
        match key.code {
            KeyCode::Up if plain => self.select_completion(-1),
            KeyCode::Down if plain => self.select_completion(1),
            KeyCode::PageUp if plain => self.page_completion(-PAGE),
            KeyCode::PageDown if plain => self.page_completion(PAGE),
            KeyCode::Home if plain => self.page_completion(isize::MIN / 2),
            KeyCode::End if plain => self.page_completion(isize::MAX / 2),
            KeyCode::Char(' ') | KeyCode::End if ctrl => self.select_completion(1),
            KeyCode::Delete if key.modifiers == KeyModifiers::SHIFT => {
                self.forget_completion_line();
            }
            KeyCode::Esc | KeyCode::F(10) if plain => self.completion = None,
            // Moving along the line: here, or a panel with stripes would take Left and Right.
            KeyCode::Left | KeyCode::Right if plain || ctrl => self.move_on_the_line(key),
            KeyCode::Home if ctrl => self.move_on_the_line(key),
            // Typing: the list stays and follows.
            _ if is_editing_key(key) => return false,
            // All the rest closes the list and goes to the owner (FAR); Enter runs the line.
            _ => {
                self.completion = None;
                return false;
            }
        }
        true
    }

    fn move_on_the_line(&mut self, key: KeyEvent) {
        if let Err(error) =
            line_edit::handle_key(&mut self.command_line, key, self.clipboard.as_mut())
        {
            self.show_error("Clipboard", &error);
        }
    }

    /// Whether the list is up, and there is room for it above the command line.
    pub(super) fn completion_shown(&self) -> bool {
        self.completion.as_ref().is_some_and(Completion::is_shown)
            && self.height >= self.bottom_rows() + LEAST_ROOM + u16::from(self.status_row_shown())
    }
}

/// A key that edits the line.
fn is_editing_key(key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char(_) => !key.modifiers.intersects(CONTROL_OR_ALT),
        KeyCode::Backspace | KeyCode::Delete => key.modifiers.is_empty(),
        _ => false,
    }
}

/// What a page key moves.
const PAGE: isize = 8;
/// Rows the list needs above the command line: its frame and one line.
const LEAST_ROOM: u16 = 3;

const CONTROL_OR_ALT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::ALT);

#[cfg(test)]
#[path = "completion_tests.rs"]
mod tests;
