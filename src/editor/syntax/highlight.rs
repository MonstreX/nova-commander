//! The highlighting of an edited document (ADR-0024 §4): the parser's state kept at the start of every
//! `STEP`-th line from the document's start (Colorer's `ParseCache`, as far as it was parsed), a screen
//! coloured from the nearest state above it; a screen past what was parsed, further than a walk from
//! there, is coloured from `BACK` lines above it as from the start (FarColorer's `backparse`, nearer)
//! until the states reach it. An edit drops the states past it (Colorer's `modifyEvent`).

use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use super::classes::class_of;
use super::hrc::{Library, RegionId, SchemeId, TypeId};
use super::parser::{State, parse_line};
use crate::editor::view::{Line, LineSource, Missing};

/// Lines between kept states.
pub const STEP: usize = 64;
/// How far past the kept states a screen's start is walked to from the last one.
const WALK: usize = STEP * 4;
/// Lines above a screen far past the kept states it is coloured from.
pub const BACK: usize = 200;
/// The documents whose states are filled on in the background (the line count's bound).
pub const FILL_LIMIT: u64 = 16 << 20;

/// What [`Syntax::try_choose`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen {
    /// The type and its name.
    Type(TypeId, String),
    /// The schemes are being read: asked again later.
    Busy,
    /// No type for the file (or the schemes cannot be read).
    Broken,
}

/// What an action of the screen's thread waits for before it is done again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    /// The file's block of this index, not read yet.
    Block(u64),
    /// The schemes a worker reads a type's schemes into (R7 review B5).
    Library,
}

impl From<Missing> for Wait {
    fn from(Missing(index): Missing) -> Wait {
        Wait::Block(index)
    }
}

/// The schemes, shared by every highlighted document (Colorer's `ParserFactory`).
#[derive(Debug)]
pub struct Syntax {
    lib: RwLock<Library>,
    /// The types listed by the catalog (reading a type's schemes lists none more): the type menu
    /// has them while a worker reads schemes.
    types: Vec<TypeEntry>,
}

impl Syntax {
    /// The schemes of a catalog (`hrc\catalog.xml`); none read when it is not there.
    pub fn open(catalog: &Path) -> Syntax {
        let lib = Library::from_catalog(catalog);
        let types = (lib.listed_types().iter())
            .map(|&id| {
                let file_type = lib.file_type(id);
                TypeEntry {
                    id,
                    name: file_type.name.clone(),
                    group: file_type.group.clone(),
                    description: file_type.description.clone(),
                }
            })
            .collect();
        Syntax {
            lib: RwLock::new(lib),
            types,
        }
    }

    /// The type a file is taken as by its name and first line, waiting for a reading under way (the
    /// workers').
    #[cfg(test)]
    pub fn choose(&self, name: &str, first_line: &str) -> Option<TypeId> {
        let lib = self.lib.read().ok()?;
        lib.choose_type(name, first_line, 0)
    }

    /// [`Syntax::try_choose`] by the text's start, waiting for a reading under way (the workers').
    pub fn choose_by_start(&self, name: &str, lines: &[String]) -> Option<TypeId> {
        let lib = self.lib.read().ok()?;
        let mut line = |index: usize| Ok(lines.get(index).cloned());
        lib.choose_by_start(name, &mut line).ok().flatten()
    }

    /// [`Syntax::choose`] and the type's name without waiting for schemes being read on a worker (the
    /// UI's tick; review B4): `Busy` while they are, `Broken` when a reading panicked.
    /// By its text's start as Colorer takes it (`Library::choose_by_start`): `line(i)` gives line `i`,
    /// `Err` while it is not read (`Busy` then too).
    pub fn try_choose(
        &self,
        name: &str,
        line: &mut dyn FnMut(usize) -> Result<Option<String>, ()>,
        named: Option<&str>,
    ) -> Chosen {
        let lib = match self.lib.try_read() {
            Ok(lib) => lib,
            Err(std::sync::TryLockError::WouldBlock) => return Chosen::Busy,
            Err(std::sync::TryLockError::Poisoned(_)) => return Chosen::Broken,
        };
        let id = match named {
            Some(named) => lib.type_named(named),
            None => match lib.choose_by_start(name, line) {
                Ok(id) => id,
                Err(()) => return Chosen::Busy,
            },
        };
        match id {
            Some(id) => Chosen::Type(id, lib.file_type(id).name.clone()),
            None => Chosen::Broken,
        }
    }

    /// The base scheme of a type whose schemes are read already (none when they are not, or while
    /// another reading holds them): highlighting at once, with no worker.
    pub fn loaded_base(&self, id: TypeId) -> Option<SchemeId> {
        let lib = self.lib.try_read().ok()?;
        if !lib.is_loaded(id) {
            return None;
        }
        lib.file_type(id).base_scheme
    }

    /// The library for an action of the screen's thread, not waited for: while a worker reads a
    /// type's schemes (it holds the library for all of it) the action waits and is done again (R7
    /// review B5: the outline and the pair keys held the screen as long); none when a worker broke it.
    pub(super) fn read_now(&self) -> Result<Option<std::sync::RwLockReadGuard<'_, Library>>, Wait> {
        match self.lib.try_read() {
            Ok(lib) => Ok(Some(lib)),
            Err(std::sync::TryLockError::WouldBlock) => Err(Wait::Library),
            Err(std::sync::TryLockError::Poisoned(_)) => Ok(None),
        }
    }

    /// The type's schemes read (they may take a while: on a worker), its base scheme.
    pub fn load(&self, id: TypeId) -> Option<SchemeId> {
        {
            let lib = self.lib.read().ok()?;
            if lib.is_loaded(id) {
                return lib.file_type(id).base_scheme;
            }
        }
        let mut lib = self.lib.write().ok()?;
        lib.load_type(id);
        lib.file_type(id).base_scheme
    }

    /// The types a file may be taken as, in the catalog's order (FarColorer's type menu,
    /// `enumerateFileTypes`).
    pub fn types(&self) -> Vec<TypeEntry> {
        self.types.clone()
    }

    /// The type's name and its group, as FarColorer's menu shows them.
    #[cfg(test)]
    pub fn type_name(&self, id: TypeId) -> Option<(String, String)> {
        let lib = self.lib.read().ok()?;
        let file_type = lib.file_type(id);
        Some((file_type.description.clone(), file_type.group.clone()))
    }
}

/// A file type as FarColorer's type menu lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeEntry {
    pub id: TypeId,
    pub name: String,
    pub group: String,
    pub description: String,
}

/// One coloured part of a row: its cells from the first shown, the class it is painted as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paint {
    pub from: usize,
    pub to: usize,
    pub class: &'static str,
}

/// A document's highlighting.
#[derive(Debug)]
pub struct Highlighter {
    syntax: Arc<Syntax>,
    base: SchemeId,
    /// The states kept: a line's first byte and the state at it, every `STEP` lines from the first.
    marks: Vec<(u64, State)>,
    classes: Vec<Option<Option<&'static str>>>,
    /// A screen went unpainted while the schemes were being read: to be drawn again.
    missed: std::cell::Cell<bool>,
    /// The outlines collected so far (`Outliner`) and where they go on from.
    outline: OutlineWalk,
    /// The states reach the document's end: nothing to fill until it changes (review B8: the tail was
    /// parsed again at every tick).
    filled: bool,
}

/// The outlines collected and the line they go on from (its first byte, number, state); none: done.
#[derive(Debug, Default)]
struct OutlineWalk {
    outlines: super::outline::Outlines,
    next: Option<(u64, u64, State)>,
}

impl Highlighter {
    pub fn new(syntax: Arc<Syntax>, base: SchemeId, floor: u64) -> Highlighter {
        Highlighter {
            syntax,
            base,
            marks: vec![(floor, State::default())],
            classes: Vec::new(),
            missed: std::cell::Cell::new(false),
            outline: OutlineWalk {
                outlines: Default::default(),
                next: Some((floor, 0, State::default())),
            },
            filled: false,
        }
    }

    /// Whether a screen went unpainted since the last ask (the schemes were being read).
    pub fn take_missed(&self) -> bool {
        self.missed.take()
    }

    /// The document read anew from `floor`: only the first line's state, the start's.
    pub fn restart(&mut self, floor: u64) {
        self.filled = false;
        self.marks = vec![(floor, State::default())];
        self.outline = OutlineWalk {
            outlines: Default::default(),
            next: Some((floor, 0, State::default())),
        };
    }

    /// The document changed from `pos`: the states past it go.
    pub fn changed(&mut self, pos: u64) {
        self.filled = false;
        let keep = self.marks.partition_point(|(at, _)| *at <= pos).max(1);
        self.marks.truncate(keep);
        // The outlines from the last state kept on (`modifyEvent`), unless they stopped before it.
        let (start, state) = self.marks[keep - 1].clone();
        let behind = (self.outline.next.as_ref()).is_some_and(|(next, _, _)| *next <= start);
        if !behind {
            self.outline.outlines.drop_from(start);
            self.outline.next = Some((start, ((keep - 1) * STEP) as u64, state));
        }
    }

    fn class(&mut self, lib: &Library, region: RegionId) -> Option<&'static str> {
        if self.classes.len() <= region {
            self.classes.resize(region + 1, None);
        }
        *self.classes[region].get_or_insert_with(|| class_of(lib, region))
    }

    /// Where a parse that reaches `top` starts: a line and the state at it; true when it starts at the
    /// last state kept (its lines may add states).
    fn start_for(&self, view: &dyn LineSource, top: u64) -> Result<(u64, State, bool), Missing> {
        let at = self
            .marks
            .partition_point(|(start, _)| *start <= top)
            .max(1)
            - 1;
        let (start, state) = &self.marks[at];
        let last = at + 1 == self.marks.len();
        if !last {
            return Ok((*start, state.clone(), false));
        }
        let mut line_start = *start;
        for _ in 0..WALK {
            if line_start >= top {
                return Ok((*start, state.clone(), true));
            }
            match view.line_after(&view.line_at(line_start)?)? {
                Some(next) => line_start = next,
                None => return Ok((*start, state.clone(), true)),
            }
        }
        let mut back = top;
        for _ in 0..BACK {
            match view.line_above(back)? {
                Some(above) => back = above,
                None => break,
            }
        }
        Ok((back, State::default(), false))
    }

    /// The paints of each row `width` cells wide - its line and its first cell shown, in order (a
    /// wrapped line on several rows) - none for a row not shown yet or while the schemes are read.
    pub fn paint(
        &mut self,
        view: &dyn LineSource,
        rows: &[(u64, usize)],
        width: usize,
    ) -> Vec<Vec<Paint>> {
        let syntax = Arc::clone(&self.syntax);
        let lib = match syntax.lib.try_read() {
            Ok(lib) => lib,
            // Being read: painted again once they are.
            Err(std::sync::TryLockError::WouldBlock) => {
                self.missed.set(true);
                return Vec::new();
            }
            // A reading that panicked: no colours, and no screen drawn again for them at every tick
            // (review B6).
            Err(std::sync::TryLockError::Poisoned(_)) => return Vec::new(),
        };
        let Some(&(top, _)) = rows.first() else {
            return Vec::new();
        };
        let Ok((mut start, mut state, extends)) = self.start_for(view, top) else {
            return Vec::new();
        };
        let mut counted = extends.then_some(0);
        let mut painted = Vec::with_capacity(rows.len());
        while painted.len() < rows.len() {
            let Ok(line) = view.line_at(start) else {
                break;
            };
            let parsed = parse_line(&lib, self.base, &state, &line.chars);
            while let Some(&(_, cell)) = rows.get(painted.len()).filter(|row| row.0 == start) {
                painted.push(self.row(&lib, &line, &parsed.spans, (cell, width)));
            }
            state = parsed.next;
            let Ok(Some(next)) = view.line_after(&line) else {
                break;
            };
            start = next;
            counted = counted.map(|count| count + 1);
            if counted == Some(STEP) {
                self.marks.push((start, state.clone()));
                counted = Some(0);
            }
        }
        painted
    }

    /// A line's spans as paints of its shown cells.
    fn row(
        &mut self,
        lib: &Library,
        line: &Line,
        spans: &[super::parser::Span],
        (left, width): (usize, usize),
    ) -> Vec<Paint> {
        let mut paints = Vec::new();
        for span in spans {
            let Some(class) = span.region.and_then(|region| self.class(lib, region)) else {
                continue;
            };
            let first = line.cell_of(span.start);
            let past = span.end.map_or(line.cells.len(), |end| line.cell_of(end));
            let from = first.saturating_sub(left).min(width);
            let to = past.saturating_sub(left).min(width);
            if to > from {
                paints.push(Paint { from, to, class });
            }
        }
        paints
    }

    /// The states filled on from the last one for at most `budget`; true while more are to come.
    pub fn fill(&mut self, view: &dyn LineSource, budget: Duration) -> bool {
        if view.size() > FILL_LIMIT {
            return false;
        }
        let syntax = Arc::clone(&self.syntax);
        let lib = match syntax.lib.try_read() {
            Ok(lib) => lib,
            Err(std::sync::TryLockError::WouldBlock) => return true,
            Err(std::sync::TryLockError::Poisoned(_)) => return false,
        };
        let started = Instant::now();
        // The states at the end: the outlines collected ahead in the tick's time (review B3).
        if self.filled {
            let until = started + budget;
            return matches!(self.walk_outlines(&lib, view, Some(until)), Ok(true));
        }
        let Some((mut start, mut state)) = self.marks.last().cloned() else {
            return false;
        };
        while started.elapsed() < budget {
            for _ in 0..STEP {
                let Ok(line) = view.line_at(start) else {
                    return true;
                };
                state = parse_line(&lib, self.base, &state, &line.chars).next;
                match view.line_after(&line) {
                    Ok(Some(next)) => start = next,
                    Ok(None) => {
                        self.filled = true;
                        // The outlines next, at the next ticks.
                        return self.outline.next.is_some();
                    }
                    Err(_) => return true,
                }
            }
            self.marks.push((start, state.clone()));
        }
        true
    }
}

#[path = "highlight_pairs.rs"]
mod pairs;
pub use pairs::{CursorRegion, PairMatch};

#[cfg(test)]
#[path = "highlight_tests.rs"]
mod tests;
