//! A plugin's requests of the editor (API 10, FAR's `EditorControl`) answered on the program's thread:
//! what it reads waits until the editor's steps asked before are done and its blocks are read; what it
//! changes goes in as the editor's steps, in order. Without an editor, or in lock mode for a change,
//! the request fails as FAR's `EditorControl` returning FALSE.

use std::collections::VecDeque;
use std::sync::mpsc::Sender;

use ratatui::crossterm::event::{KeyEvent, KeyModifiers};

use super::super::{App, Screen};
use crate::app::editing::{EditorScreen, PluginBars};
use crate::editor::view::edit_view::{Place, Range, Step};
use crate::platform::input::Modifiers;
use crate::plugin_host::editor_requests::{
    self as requests, EditorAnswer, EditorInfo, EditorRequest,
};

/// A request not answered yet and where its answer goes.
pub(in crate::app) type EditorAsk = (EditorRequest, Sender<EditorAnswer>);

/// An editor key's plugins not told it yet: the key, its name, the call told it last, the plugins
/// after that one.
#[derive(Debug)]
pub(in crate::app) struct EditorKeyRest {
    key: KeyEvent,
    name: String,
    pub(super) serial: u64,
    takers: VecDeque<(usize, String)>,
}

impl App {
    /// A key the editor has done, for the plugins that take its keys (FAR's `ProcessEditorInputW`,
    /// API 10): each hears it in turn, as FAR's `PluginManager::ProcessEditorInput` calls them one
    /// after another (review D11: each call's wait took the one before's place), the keys after it
    /// held until the last one's end (as a panel key's, ADR-0019). Not a key that brought up a window.
    pub(in crate::app) fn editor_key_to_plugins(&mut self, key: KeyEvent) {
        if self.overlay.is_some() || !matches!(self.screen, Screen::Editor(_)) {
            return;
        }
        let Some(name) = super::menu::key_name(key) else {
            return;
        };
        let takers: VecDeque<(usize, String)> = (self.plugins.host.plugins.iter().enumerate())
            .filter(|(_, plugin)| plugin.editor_keys)
            .map(|(index, plugin)| (index, plugin.name.clone()))
            .collect();
        if takers.is_empty() {
            return;
        }
        self.plugins.editor_window = Some(self.front_window());
        self.plugins.editor_key_rest = Some(EditorKeyRest {
            key,
            name,
            serial: 0,
            takers,
        });
        self.tell_editor_key_on();
    }

    /// The call `ended` over: the next plugin its editor key is for told it, when it was one.
    pub(super) fn tell_next_editor_key(&mut self, ended: u64) {
        if (self.plugins.editor_key_rest.as_ref()).is_some_and(|rest| rest.serial == ended) {
            self.tell_editor_key_on();
        }
    }

    /// The editor key told to the next plugin, its end waited for; none left, or the key's editor no
    /// longer in front with nothing over it: the rest not told.
    fn tell_editor_key_on(&mut self) {
        loop {
            let in_front = self.overlay.is_none()
                && matches!(self.screen, Screen::Editor(_))
                && self.plugins.editor_window == Some(self.front_window());
            let Some(rest) = self.plugins.editor_key_rest.as_mut() else {
                return;
            };
            let Some((index, plugin)) = rest.takers.pop_front().filter(|_| in_front) else {
                self.plugins.editor_key_rest = None;
                return;
            };
            let (key, name) = (rest.key, rest.name.clone());
            let serial =
                self.wait_for_plugin(super::keys::WaitKind::Run, &plugin, self.active, key);
            if self.plugins.host.send_editor_key(index, &name, serial) {
                self.wait_in_editor();
                if let Some(rest) = self.plugins.editor_key_rest.as_mut() {
                    rest.serial = serial;
                }
                return;
            }
            self.drop_plugin_wait(serial);
        }
    }

    /// Enter in the editor's "Plugin commands": the command runs on its plugin's thread with the editor
    /// up, waited for (ADR-0019); what it asks of the editor is answered meanwhile (API 10).
    pub(in crate::app) fn run_editor_plugin_command(&mut self, plugin: usize, index: usize) {
        self.plugins.editor_window = Some(self.front_window());
        let side = self.command_side();
        self.run_plugin_item(plugin, side, |host, serial| {
            host.run_editor_command(plugin, index, serial)
        });
        self.wait_in_editor();
    }

    /// `Event::EditorAsk`: kept in order, answered as soon as it can be.
    pub(super) fn plugin_editor_ask(
        &mut self,
        request: EditorRequest,
        reply: Sender<EditorAnswer>,
    ) {
        self.plugins.editor_asks.push_back((request, reply));
        self.answer_editor_asks();
    }

    /// The requests that can be answered now, in order; true when the editor may have changed.
    pub(super) fn answer_editor_asks(&mut self) -> bool {
        let mut changed = false;
        // While work runs (a save), the document stays as it is: a change made meanwhile would be lost
        // when the saved file takes its place (R7 review B8).
        if self.waiting.is_some() {
            return changed;
        }
        while let Some(request) = (self.plugins.editor_asks.front()).map(|(asked, _)| asked.clone())
        {
            // Another window in front than the editor the call began in (F12, Ctrl+Tab meanwhile):
            // no editor for it, as FAR's `EditorControl` of a window gone (review D2) - told before
            // anything of the request is done (R7 review A6: its key was pressed in that window).
            let moved = (self.plugins.editor_window).is_some_and(|id| id != self.front_window());
            let answer = if moved {
                EditorAnswer::Done(false)
            } else {
                match (self.input_answer(&request), &mut self.screen) {
                    (Some(Some(answer)), _) => answer,
                    (Some(None), _) => return changed,
                    (None, Screen::Editor(editor)) => match answer(editor, &request) {
                        Some(answer) => answer,
                        None => return changed,
                    },
                    (None, _) => EditorAnswer::Done(false),
                }
            };
            if let Some((_, reply)) = self.plugins.editor_asks.pop_front() {
                let _ = reply.send(answer);
            }
            changed = true;
        }
        changed
    }

    /// The answer to a request of the plugin's key loop (FAR's `ECTL_READINPUT`, `PROCESSINPUT`,
    /// `SETTITLE`, `SETKEYBAR`): `Some(None)` until it can be given; none for another request or
    /// without an editor.
    fn input_answer(&mut self, request: &EditorRequest) -> Option<Option<EditorAnswer>> {
        let Screen::Editor(editor) = &mut self.screen else {
            return None;
        };
        let done = |done: bool| Some(Some(EditorAnswer::Done(done)));
        match request {
            EditorRequest::ReadInput => Some(self.held_input_key().map(EditorAnswer::Text)),
            EditorRequest::ProcessInput(name) => done(self.process_input(name)),
            EditorRequest::SetTitle(title) => {
                editor.plugin_bars.title.clone_from(title);
                done(true)
            }
            EditorRequest::SetKeyBar(labels) => {
                set_labels(&mut editor.plugin_bars, labels.as_deref());
                done(true)
            }
            EditorRequest::Beep => {
                crate::platform::system::beep();
                done(true)
            }
            _ => None,
        }
    }

    /// Whether a plugin's key loop waits for the next key in the editor (nothing over it).
    pub(in crate::app) fn plugin_reads_input(&self) -> bool {
        let reading = matches!(
            self.plugins.editor_asks.front(),
            Some((EditorRequest::ReadInput, _))
        );
        reading && self.overlay.is_none() && matches!(self.screen, Screen::Editor(_))
    }

    /// The first key held while the plugin worked, for its key loop; none yet.
    fn held_input_key(&mut self) -> Option<String> {
        if self.overlay.is_some() {
            return None;
        }
        let wait = self.plugins.key_wait.as_mut()?;
        while !wait.held.keys.is_empty() {
            let key = wait.held.keys.remove(0);
            if let Some(name) = super::menu::key_name(key) {
                // It works again from this key: the time for its window starts anew.
                wait.held.restart();
                return Some(name);
            }
        }
        None
    }

    /// A key pressed while a plugin's key loop waits for one: its answer (true).
    pub(in crate::app) fn editor_input_key(&mut self, key: KeyEvent) -> bool {
        if !self.plugin_reads_input() {
            return false;
        }
        let Some(name) = super::menu::key_name(key) else {
            return true;
        };
        if let Some(wait) = &mut self.plugins.key_wait {
            wait.held.restart();
        }
        if let Some((_, reply)) = self.plugins.editor_asks.pop_front() {
            let _ = reply.send(EditorAnswer::Text(name));
        }
        self.answer_editor_asks();
        true
    }

    /// The key named `name` done by the editor as if pressed (FAR's `ECTL_PROCESSINPUT`); false for a
    /// name not known.
    fn process_input(&mut self, name: &str) -> bool {
        let Some(key) = super::menu::parse(name).map(super::menu::BreakKey::event) else {
            return false;
        };
        if self.text_screen_key(key) {
            return true;
        }
        if let Screen::Editor(editor) = &mut self.screen {
            let action = editor.handle_key(key);
            self.handle_editor_action(action);
        }
        true
    }
}

/// FAR's `KeyBar::Change`: the labels of the keys named, the others kept; none: the editor's own.
fn set_labels(bars: &mut PluginBars, labels: Option<&[(String, String)]>) {
    let Some(labels) = labels else {
        bars.labels.clear();
        return;
    };
    for (name, label) in labels {
        let Some((number, modifiers)) = super::menu::parse(name).and_then(|key| key.function_key())
        else {
            continue;
        };
        let held = Modifiers {
            shift: modifiers.contains(KeyModifiers::SHIFT),
            ctrl: modifiers.contains(KeyModifiers::CONTROL),
            alt: modifiers.contains(KeyModifiers::ALT),
        };
        bars.labels
            .retain(|(other, mods, _)| (*other, *mods) != (number, held));
        bars.labels.push((number, held, label.clone()));
    }
}

/// The answer to `request`; none until the editor can give it.
fn answer(editor: &mut EditorScreen, request: &EditorRequest) -> Option<EditorAnswer> {
    let edits = !matches!(
        request,
        EditorRequest::Info
            | EditorRequest::GetLine(_)
            | EditorRequest::SetPosition { .. }
            | EditorRequest::Select { .. }
            | EditorRequest::Redraw
            | EditorRequest::RealToTab(..)
            | EditorRequest::TabToReal(..)
            | EditorRequest::FileName
            | EditorRequest::UndoRedo(0 | 1)
    );
    if edits && editor.text.is_locked() {
        return Some(EditorAnswer::Done(false));
    }
    match request {
        EditorRequest::Info => info(editor).map(EditorAnswer::Info),
        EditorRequest::GetLine(line) => {
            let read = editor.text.query(|view| match view.line_start_of(*line)? {
                Some(start) => view.plugin_line(start).map(Some),
                None => Ok(None),
            })?;
            Some(
                read.map_or(EditorAnswer::Done(false), |line| EditorAnswer::Line {
                    text: line.text,
                    eol: line.eol,
                    block: line.block,
                }),
            )
        }
        EditorRequest::FileName => Some(EditorAnswer::Text(editor.path.display().to_string())),
        EditorRequest::Redraw => Some(EditorAnswer::Done(true)),
        EditorRequest::RealToTab(line, pos) => {
            number(editor, *line, |view, start| view.real_to_tab(start, *pos))
        }
        EditorRequest::TabToReal(line, tab_pos) => number(editor, *line, |view, start| {
            view.tab_to_real(start, *tab_pos)
        }),
        _ => change(editor, request),
    }
}

/// A position of the line `line` as `convert` reads it.
fn number(
    editor: &mut EditorScreen,
    line: Option<u64>,
    convert: impl FnOnce(
        &mut crate::editor::view::edit_view::EditView,
        u64,
    ) -> Result<usize, crate::editor::view::Missing>,
) -> Option<EditorAnswer> {
    let read = editor.text.query(|view| match view.line_start_of(line)? {
        Some(start) => convert(view, start).map(Some),
        None => Ok(None),
    })?;
    Some(read.map_or(EditorAnswer::Done(false), |number| {
        EditorAnswer::Number(number as u64)
    }))
}

/// A change: the lines it names found, then its step pressed.
fn change(editor: &mut EditorScreen, request: &EditorRequest) -> Option<EditorAnswer> {
    let step = match request {
        EditorRequest::SetLine { line, text, eol } => {
            let start = start_of(editor, *line)?;
            start.map(|start| Step::SetLineText(start, text.clone(), eol.clone()))
        }
        EditorRequest::ExpandTabs(line) => start_of(editor, *line)?.map(Step::ExpandTabs),
        EditorRequest::InsertLine(indent) => Some(Step::Break(*indent)),
        EditorRequest::DeleteLine => Some(Step::DeleteLine),
        EditorRequest::InsertText(text) => Some(Step::PluginText(text.clone())),
        EditorRequest::DeleteChar => Some(Step::DeleteChar),
        EditorRequest::UndoRedo(command) => undo_step(*command),
        EditorRequest::SetPosition {
            line,
            pos,
            tab_pos,
            top,
            left,
            overtype,
        } => {
            let (line, top) = editor.text.query(|view| {
                let line = match line {
                    Some(line) => view.line_start_of(Some(*line))?,
                    None => None,
                };
                let top = match top {
                    Some(top) => view.line_start_of(Some(*top))?,
                    None => None,
                };
                Ok((line, top))
            })?;
            Some(Step::Place(Place {
                line,
                pos: *pos,
                tab_pos: *tab_pos,
                top,
                left: *left,
                overtype: *overtype,
            }))
        }
        EditorRequest::Select {
            kind,
            line,
            pos,
            width,
            height,
        } => select_step(editor, (*kind, *line), (*pos, *width, *height))?,
        _ => None,
    };
    Some(match step {
        Some(step) => {
            editor.text.press(step);
            EditorAnswer::Done(true)
        }
        None => EditorAnswer::Done(false),
    })
}

/// The first byte of the line `line` (none: the cursor's); `Some(None)` past the last line.
fn start_of(editor: &mut EditorScreen, line: Option<u64>) -> Option<Option<u64>> {
    editor.text.query(|view| view.line_start_of(line))
}

/// FAR's `EUR_*` as the editor's steps.
fn undo_step(command: i64) -> Option<Step> {
    Some(match command {
        0 => Step::Group(true),
        1 => Step::Group(false),
        2 => Step::Undo,
        3 => Step::Redo,
        _ => return None,
    })
}

/// FAR's `ECTL_SELECT` as a step: no block for none, else from its first line found.
fn select_step(
    editor: &mut EditorScreen,
    (kind, line): (i64, Option<u64>),
    (pos, width, height): (i64, i64, i64),
) -> Option<Option<Step>> {
    if !matches!(kind, 1 | 2) || pos < 0 {
        return Some(Some(Step::Unselect));
    }
    if height < 1 {
        return Some(None);
    }
    let start = start_of(editor, line)?;
    Some(start.map(|line| {
        Step::SelectRange(Range {
            kind,
            line,
            pos,
            width,
            height,
        })
    }))
}

/// FAR's `EditorInfo`.
fn info(editor: &mut EditorScreen) -> Option<EditorInfo> {
    let read = editor.text.query(|view| view.plugin_info())?;
    let (width, height) = editor.text.room.get();
    let options = editor.text.options();
    let flags = [
        (options.expand_tabs == 2, requests::OPTION_EXPAND_ALL_TABS),
        (options.expand_tabs == 1, requests::OPTION_EXPAND_NEW_TABS),
        (
            options.persistent_blocks,
            requests::OPTION_PERSISTENT_BLOCKS,
        ),
        (
            options.del_removes_blocks,
            requests::OPTION_DEL_REMOVES_BLOCKS,
        ),
        (options.auto_indent, requests::OPTION_AUTO_INDENT),
        (
            options.cursor_beyond_eol,
            requests::OPTION_CURSOR_BEYOND_EOL,
        ),
        (editor.text.white, requests::OPTION_SHOW_WHITE_SPACE),
        (editor.text.scrollbar, requests::OPTION_SHOW_SCROLLBAR),
        (editor.text.numbers, requests::OPTION_SHOW_LINE_NUMBERS),
    ];
    let options = (flags.iter())
        .filter(|(on, _)| *on)
        .fold(0, |all, (_, flag)| all | flag);
    let state = [
        (!editor.text.is_changed(), requests::STATE_SAVED),
        (editor.text.is_locked(), requests::STATE_LOCKED),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .fold(0, |all, (_, flag)| all | flag);
    Some(EditorInfo {
        width: u64::from(width),
        height: u64::from(height),
        lines: read.lines,
        line: read.line,
        pos: read.pos as u64,
        tab_pos: read.tab_pos as u64,
        top: read.top,
        left: read.left as u64,
        overtype: read.overtype,
        block: read.block,
        block_line: read.block_line,
        options,
        state,
        tab_size: editor.text.tab_size() as u64,
        code_page: editor.text.page_number().unwrap_or(0),
    })
}

#[cfg(test)]
#[path = "editor_control_tests.rs"]
mod tests;
