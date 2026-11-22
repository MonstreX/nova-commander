//! The syntax highlighting of the editor (S2d.5d) and of the viewer's text (S2d.5e; ADR-0024):
//! Colorer's schemes read once on a worker from `hrc\catalog.xml` beside the program, the file's type
//! chosen by its name and first line, its schemes read on a worker, then the highlighter's states
//! filled on a little at each tick (the documents the line count takes) and the screen painted from
//! them.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::editor::syntax::highlight::{
    Chosen, CursorRegion, Highlighter, Paint, PairMatch, Syntax, TypeEntry, Wait,
};
use crate::editor::syntax::hrc::SchemeId;
use crate::editor::view::LineSource;

/// The time a tick gives the states' filling.
const FILL_BUDGET: Duration = Duration::from_millis(3);

/// The schemes read, and the reading they came from (none: not asked yet).
static SHARED: Mutex<(Option<Arc<Syntax>>, Option<u64>)> = Mutex::new((None, None));
/// The reading asked last: a reading started before a reload is let go.
static READING: AtomicU64 = AtomicU64::new(0);

/// Where the schemes are: `hrc\catalog.xml` beside the program.
#[cfg(not(test))]
fn catalog() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent()
        .map(|dir| dir.join("hrc").join("catalog.xml"))
        .unwrap_or_default()
}

impl super::App {
    /// The type of the file under the active panel's cursor read ahead while the panels are in front
    /// and the highlighting is on (`read_ahead`).
    pub(super) fn read_syntax_ahead(&mut self) -> bool {
        let on = self.viewer_settings.highlighting || self.editor_settings.highlighting;
        if !on || !self.panels_on_screen() {
            return false;
        }
        let panel = self.active_panel();
        // Not a file a cloud or a storage brings back as it is opened (R7 review B6: resting the cursor
        // on one downloaded it).
        let file = panel
            .current()
            .is_some_and(|entry| !entry.is_dir && !brought_back(entry));
        let path = panel.current_path().filter(|_| file);
        read_ahead(path.as_deref());
        false
    }
}

/// Whether opening `entry` would bring it back from elsewhere (offline, a cloud's placeholder).
fn brought_back(entry: &crate::vfs::Entry) -> bool {
    use crate::platform::files::Flag;
    let attributes = entry.attributes;
    [Flag::Offline, Flag::RecallOnOpen, Flag::RecallOnDataAccess]
        .into_iter()
        .any(|flag| attributes.has(flag))
}

/// The tests' schemes: the repository's (no program's folder beside a test).
#[cfg(test)]
fn catalog() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/colorer/hrc/catalog.xml")
}

/// The schemes, once read; their reading started on a worker at the first ask.
fn shared() -> Option<Arc<Syntax>> {
    let mut shared = SHARED.lock().ok()?;
    if let Some(syntax) = &shared.0 {
        return Some(Arc::clone(syntax));
    }
    let reading = READING.load(Ordering::SeqCst);
    if shared.1 != Some(reading) {
        // No thread to be had (R7 review B9: `spawn` panicked, under this lock): asked again later.
        let started =
            (std::thread::Builder::new().name("colorer schemes".to_string())).spawn(move || {
                let syntax = Arc::new(Syntax::open(&catalog()));
                if let Ok(mut shared) = SHARED.lock()
                    && READING.load(Ordering::SeqCst) == reading
                {
                    shared.0 = Some(syntax);
                }
            });
        if started.is_ok() {
            shared.1 = Some(reading);
        }
    }
    None
}

/// The worker reading types ahead, and the last file it was given.
static AHEAD: Mutex<(Option<std::sync::mpsc::Sender<PathBuf>>, Option<PathBuf>)> =
    Mutex::new((None, None));

/// The longest wait of the worker for the catalog.
const CATALOG_WAIT: Duration = Duration::from_secs(60);

/// The schemes read ahead (ours; owner 2026-11-21: FAR colours a file the moment it opens): the
/// catalog at once, then the type of the file at `path` (the panel's cursor on it) on a worker, so that
/// F3 / F4 find its schemes read. The last file asked for is the one read; a type read once stays.
pub fn read_ahead(path: Option<&std::path::Path>) {
    let _ = shared();
    let Ok(mut ahead) = AHEAD.lock() else {
        return;
    };
    let Some(path) = path.filter(|path| ahead.1.as_deref() != Some(*path)) else {
        return;
    };
    ahead.1 = Some(path.to_path_buf());
    // Looked at once per file: only one on a local fixed disk is read ahead.
    if !crate::platform::volumes::on_local_disk(path) {
        return;
    }
    if ahead.0.is_none() {
        let (sender, receiver) = channel();
        let worker = std::thread::Builder::new().name("colorer read-ahead".to_string());
        // No thread to be had: no reading ahead now (R7 review B9).
        if worker.spawn(move || read_types_ahead(&receiver)).is_err() {
            return;
        }
        ahead.0 = Some(sender);
    }
    if let Some(sender) = &ahead.0 {
        let _ = sender.send(path.to_path_buf());
    }
}

/// The worker: the newest file's type chosen by its name and first line, and its schemes read.
fn read_types_ahead(files: &Receiver<PathBuf>) {
    while let Ok(mut path) = files.recv() {
        while let Ok(newer) = files.try_recv() {
            path = newer;
        }
        let deadline = std::time::Instant::now() + CATALOG_WAIT;
        let syntax = loop {
            match shared() {
                Some(syntax) => break Some(syntax),
                None if std::time::Instant::now() > deadline => break None,
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        let Some(syntax) = syntax else {
            continue;
        };
        let name =
            (path.file_name()).map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        if let Some(id) = syntax.choose_by_start(&name, &first_lines(&path)) {
            syntax.load(id);
        }
    }
}

/// The file's first lines as the type is chosen by (its first bytes read as UTF-8, a byte order mark
/// let go; Colorer's chooser takes some of them, `Library::choose_by_start`).
fn first_lines(path: &std::path::Path) -> Vec<String> {
    use std::io::Read;
    let mut head = Vec::new();
    if let Ok(file) = std::fs::File::open(path) {
        let _ = file.take(16 << 10).read_to_end(&mut head);
    }
    let text = String::from_utf8_lossy(&head);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    text.lines().take(64).map(str::to_string).collect()
}

/// FarColorer's "Reload schema library": the catalog and the schemes read anew at the next ask.
pub fn reload() {
    READING.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut shared) = SHARED.lock() {
        *shared = (None, None);
    }
}

/// The types the schemes know, in their order (none until they are read).
pub fn types() -> Vec<TypeEntry> {
    shared().map(|syntax| syntax.types()).unwrap_or_default()
}

/// How a document's type is had (FarColorer's type menu).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Choice {
    /// By the file's name and first line ("Automatic detection").
    #[default]
    Auto,
    /// None ("Disable").
    Off,
    /// The type of this name.
    Named(String),
}

/// Where the highlighting of a document is.
#[derive(Debug)]
enum Stage {
    /// The schemes or the document not ready yet.
    Waiting,
    /// The type's schemes being read.
    Loading(Arc<Syntax>, Receiver<Option<SchemeId>>),
    On,
    /// No type for the file, or no schemes.
    Off,
    /// Turned off by the settings.
    Disabled,
}

/// A document's highlighting as the screen and the ticks ask.
#[derive(Debug)]
pub struct TextSyntax {
    name: String,
    stage: Stage,
    highlighter: RefCell<Option<Highlighter>>,
    /// The document is read anew (another code page): its states from its first line again.
    restart: bool,
    choice: Choice,
    /// The type taken, by its name.
    taken: Option<String>,
    /// When the document was opened (`holds_paint`).
    born: std::time::Instant,
}

/// How long a document just opened keeps the screen from showing it uncoloured (`holds_paint`).
const FIRST_PAINT_HOLD: Duration = Duration::from_millis(1500);

impl TextSyntax {
    /// For a file named `name`.
    pub fn new(name: &str) -> TextSyntax {
        TextSyntax {
            name: name.to_string(),
            stage: Stage::Waiting,
            highlighter: RefCell::new(None),
            restart: false,
            choice: Choice::Auto,
            taken: None,
            born: std::time::Instant::now(),
        }
    }

    /// Whether a document just opened is still without its text or its type's schemes: the screen
    /// waits for them (at most `FIRST_PAINT_HOLD`) rather than show it plain, then coloured (owner
    /// 2026-11-21: FAR colours a file the moment it shows it, no blink).
    pub fn holds_paint(&self) -> bool {
        matches!(self.stage, Stage::Waiting | Stage::Loading(..))
            && self.born.elapsed() < FIRST_PAINT_HOLD
    }

    /// Whether the document is highlighted.
    pub fn is_on(&self) -> bool {
        matches!(self.stage, Stage::On)
    }

    /// Highlighting turned on or off (the settings' switch).
    pub fn set_enabled(&mut self, on: bool) {
        match (on, &self.stage) {
            (false, _) => {
                self.stage = Stage::Disabled;
                *self.highlighter.borrow_mut() = None;
            }
            (true, Stage::Disabled) => self.stage = Stage::Waiting,
            _ => {}
        }
    }

    /// The document read anew: its states go.
    pub fn restart(&mut self) {
        self.restart = true;
    }

    /// The type taken as `choice` says (FarColorer's type menu), its states from the start.
    pub fn choose_type(&mut self, choice: Choice) {
        self.choice = choice;
        self.again();
    }

    /// The type chosen anew (the schemes read again, or another choice): the highlighting waits.
    pub fn again(&mut self) {
        if !matches!(self.stage, Stage::Disabled) {
            self.stage = Stage::Waiting;
        }
        *self.highlighter.borrow_mut() = None;
        self.taken = None;
    }

    /// The type taken, by its name; none while there is none.
    pub fn taken(&self) -> Option<&str> {
        self.taken.as_deref()
    }

    /// FarColorer's `cursorRegion` (none without highlighting).
    pub fn cursor_region(
        &self,
        view: &dyn LineSource,
        line: u64,
        pos: usize,
    ) -> Result<Option<CursorRegion>, Wait> {
        match self.highlighter.borrow().as_ref() {
            Some(highlighter) => highlighter.cursor_region(view, line, pos),
            None => Ok(None),
        }
    }

    /// The document's outlines (none without highlighting).
    pub fn outlines(
        &self,
        view: &dyn LineSource,
    ) -> Result<Option<crate::editor::syntax::outline::Outlines>, Wait> {
        match self.highlighter.borrow_mut().as_mut() {
            Some(highlighter) => highlighter.outlines(view).map(Some),
            None => Ok(None),
        }
    }

    /// Colorer's `searchGlobalPair` (none without highlighting).
    pub fn search_pair(
        &self,
        view: &dyn LineSource,
        line: u64,
        pos: usize,
    ) -> Result<Option<PairMatch>, Wait> {
        match self.highlighter.borrow().as_ref() {
            Some(highlighter) => highlighter.search_pair(view, line, pos),
            None => Ok(None),
        }
    }

    /// Whether the settings turned it off.
    pub fn is_disabled(&self) -> bool {
        matches!(self.stage, Stage::Disabled)
    }

    /// A tick over the document (its first line at `floor`, changed from `changed`): the type chosen
    /// and its schemes asked for, then the states filled on; true when the screen is to be drawn again
    /// (the highlighting just came).
    pub fn tick(&mut self, view: &dyn LineSource, floor: u64, changed: Option<u64>) -> bool {
        match &self.stage {
            Stage::Waiting => self.choose(view, floor),
            Stage::Loading(syntax, done) => {
                let syntax = Arc::clone(syntax);
                match done.try_recv() {
                    Ok(Some(base)) => {
                        let highlighter = Highlighter::new(syntax, base, floor);
                        *self.highlighter.borrow_mut() = Some(highlighter);
                        self.stage = Stage::On;
                        self.restart = false;
                        return true;
                    }
                    Ok(None) | Err(TryRecvError::Disconnected) => self.stage = Stage::Off,
                    Err(TryRecvError::Empty) => {}
                }
                false
            }
            Stage::On => {
                let mut highlighter = self.highlighter.borrow_mut();
                let Some(highlighter) = highlighter.as_mut() else {
                    return false;
                };
                let restarted = std::mem::take(&mut self.restart);
                if restarted {
                    highlighter.restart(floor);
                } else if let Some(pos) = changed {
                    highlighter.changed(pos);
                }
                highlighter.fill(view, FILL_BUDGET);
                restarted | highlighter.take_missed()
            }
            Stage::Off | Stage::Disabled => false,
        }
    }

    /// The type chosen by the name and the first line; its schemes read on a worker.
    fn choose(&mut self, view: &dyn LineSource, floor: u64) -> bool {
        let Some(syntax) = shared() else {
            return false;
        };
        // The document's lines from its first, as Colorer's chooser reads its start (review B5).
        let mut starts = vec![floor];
        let mut line = |index: usize| -> Result<Option<String>, ()> {
            while starts.len() <= index {
                let last = view.line_at(starts[starts.len() - 1]).map_err(|_| ())?;
                match view.line_after(&last).map_err(|_| ())? {
                    Some(next) => starts.push(next),
                    None => return Ok(None),
                }
            }
            let text = view.line_at(starts[index]).map_err(|_| ())?;
            Ok(Some(text.chars.iter().collect()))
        };
        let chosen = match &self.choice {
            Choice::Auto => syntax.try_choose(&self.name, &mut line, None),
            Choice::Off => Chosen::Broken,
            Choice::Named(name) => syntax.try_choose(&self.name, &mut line, Some(name)),
        };
        let id = match chosen {
            Chosen::Type(id, name) => {
                self.taken = Some(name);
                id
            }
            // Another document's schemes being read: the next tick asks again (the UI never waits).
            Chosen::Busy => return false,
            Chosen::Broken => {
                self.stage = Stage::Off;
                self.taken = None;
                return false;
            }
        };
        // Its schemes read already (read ahead from the panel, or by another document): highlighted
        // in this very tick, before the screen first shows the text (FAR colours as it opens).
        if let Some(base) = syntax.loaded_base(id) {
            *self.highlighter.borrow_mut() = Some(Highlighter::new(syntax, base, floor));
            self.stage = Stage::On;
            self.restart = false;
            return true;
        }
        let (sender, receiver) = channel();
        let worker = Arc::clone(&syntax);
        let thread = std::thread::Builder::new().name("colorer type".to_string());
        // No thread to be had: tried again at the next look (R7 review B9).
        let load = move || {
            let _ = sender.send(worker.load(id));
        };
        if thread.spawn(load).is_err() {
            return false;
        }
        self.stage = Stage::Loading(syntax, receiver);
        false
    }

    /// The paints of the rows (each one's line and first cell) `width` cells wide.
    pub fn paints(
        &self,
        view: &dyn LineSource,
        rows: &[(u64, usize)],
        width: usize,
    ) -> Vec<Vec<Paint>> {
        let mut highlighter = self.highlighter.borrow_mut();
        highlighter
            .as_mut()
            .map(|highlighter| highlighter.paint(view, rows, width))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R7 review B6: a file a cloud or a storage brings back on open is not read ahead; one on a local
    /// fixed disk is.
    #[test]
    fn only_files_at_hand_on_a_local_disk_are_read_ahead() {
        let entry = |bits: u32| crate::vfs::Entry {
            attributes: crate::platform::files::Attributes::from_windows_bits(bits),
            ..crate::vfs::Entry::default()
        };
        assert!(!brought_back(&entry(0x20)), "a plain file");
        for bits in [0x1000, 0x4_0000, 0x40_0000] {
            assert!(brought_back(&entry(bits)), "{bits:#x}");
        }
    }
}
