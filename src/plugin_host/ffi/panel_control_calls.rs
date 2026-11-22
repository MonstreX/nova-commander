//! The host's panel functions (API 3, FAR's `PanelControl`): `NcHost::panel_state`, `panel_items` and
//! `panel_do`, on the plugin's thread. Each request goes to the program and its answer is waited for;
//! the texts handed back stay in the context until the next such answer. In a panel's call (the program
//! may be waiting for it) nothing is asked: the state is read from the panels the program last published,
//! the items and the changes fail.

use std::ffi::c_void;
use std::sync::mpsc::channel;

use nc_plugin_api::{
    DO_ALL, DO_CLOSE, DO_DELETE_ON_CLOSE, DO_EDIT, DO_GET_COMMAND_LINE, DO_INSERT_COMMAND_LINE,
    DO_KEEP_SELECTION, DO_OFF, DO_REREAD, DO_REVERSE, DO_RUN_COMMAND, DO_SELECT,
    DO_SET_COMMAND_LINE, DO_SET_CURSOR, DO_SET_FOLDER, DO_SET_SORT, DO_VIEW, ITEM_FOLDER,
    ITEM_PARENT, ITEM_SELECTED, ITEMS_SELECTED, KIND_FILES, KIND_INFO, KIND_QUICK_VIEW, KIND_TREE,
    NcPanelAction, NcPanelItem, NcPanelState, NcStr, STATE_ACTIVE, STATE_CALL_TARGET, STATE_LEFT,
    STATE_PLUGIN, STATE_READING, STATE_REAL_NAMES, STATE_REVERSE_SORT, STATE_SELECTED_FIRST,
    STATE_SHOW_HIDDEN, STATE_VISIBLE, WHICH_ACTIVE, WHICH_LEFT, WHICH_PASSIVE, WHICH_RIGHT,
};

use super::calls::context;
use super::panel_calls::millis;
use super::{HostContext, text};
use crate::plugin_host::Event;
use crate::plugin_host::panel_requests::{
    PanelAction, PanelAnswer, PanelItem, PanelKind, PanelRequest, PanelState, Which,
};

/// The most names of a selection read.
const MOST_NAMES: usize = 10_000_000;

/// What the context keeps of the panels' answers: their texts live here until the next one.
#[derive(Default)]
pub struct PanelAnswers {
    state: PanelState,
    /// The items of a run of `panel_items` calls: of which panel, with which flags.
    items: Option<(u32, u32, Vec<PanelItem>)>,
    text: String,
}

impl PanelAnswers {
    /// The items kept for the reads after a count are let go.
    pub fn forget_items(&mut self) {
        self.items = None;
    }
}

/// `request` sent and its answer waited for; none when the program is not listening or in a panel's
/// call.
pub(super) fn ask(context: &HostContext, request: PanelRequest) -> Option<PanelAnswer> {
    if !context.started() || context.in_quiet_call() {
        return None;
    }
    let (reply, answer) = channel();
    let event = Event::PanelAsk {
        plugin: context.plugin.clone(),
        request,
        reply,
    };
    context.events.send(event).ok()?;
    answer.recv().ok()
}

pub(super) fn which(raw: u32) -> Option<Which> {
    Some(match raw {
        WHICH_ACTIVE => Which::Active,
        WHICH_PASSIVE => Which::Passive,
        WHICH_LEFT => Which::Left,
        WHICH_RIGHT => Which::Right,
        _ => return None,
    })
}

/// `NcHost::panel_state`.
pub unsafe extern "C" fn panel_state(
    context: *mut c_void,
    which_raw: u32,
    out: *mut NcPanelState,
) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        return -1;
    };
    let Some(which) = which(which_raw) else {
        return -1;
    };
    let state = if context.in_quiet_call() {
        seen(context, which)
    } else {
        match ask(context, PanelRequest::State(which)) {
            Some(PanelAnswer::State(state)) => *state,
            _ => return -1,
        }
    };
    let mut answers = context.panel_answers.borrow_mut();
    answers.items = None;
    answers.state = state;
    if !out.is_null() {
        // SAFETY: the plugin hands a writable state or null; its texts stay in the context.
        unsafe { out.write(state_of(&answers.state)) };
    }
    0
}

/// The panel as the program last published it (left, right): the active one is the one it says is.
fn seen(context: &HostContext, which: Which) -> PanelState {
    let panels = context
        .panels_seen
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let active = usize::from(panels[1].active);
    let index = match which {
        Which::Left => 0,
        Which::Right => 1,
        Which::Active => active,
        Which::Passive => 1 - active,
    };
    panels[index].clone()
}

fn state_of(state: &PanelState) -> NcPanelState {
    let flag = |on: bool, bit: u32| if on { bit } else { 0 };
    NcPanelState {
        flags: flag(state.left, STATE_LEFT)
            | flag(state.active, STATE_ACTIVE)
            | flag(state.visible, STATE_VISIBLE)
            | flag(state.plugin.is_some(), STATE_PLUGIN)
            | flag(state.real_names, STATE_REAL_NAMES)
            | flag(state.show_hidden, STATE_SHOW_HIDDEN)
            | flag(state.reverse_sort, STATE_REVERSE_SORT)
            | flag(state.selected_first, STATE_SELECTED_FIRST)
            | flag(state.call_target, STATE_CALL_TARGET)
            | flag(state.reading, STATE_READING),
        kind: match state.kind {
            PanelKind::Files => KIND_FILES,
            PanelKind::Tree => KIND_TREE,
            PanelKind::QuickView => KIND_QUICK_VIEW,
            PanelKind::Info => KIND_INFO,
        },
        folder: NcStr::new(&state.folder),
        plugin: NcStr::new(state.plugin.as_deref().unwrap_or("")),
        disk_folder: NcStr::new(&state.disk_folder),
        items: state.items,
        selected: state.selected,
        current: state.current,
        top: state.top,
        sort: state.sort,
        view_mode: state.view_mode,
        columns: NcStr::new(&state.columns),
        widths: NcStr::new(&state.widths),
    }
}

/// `NcHost::panel_items`.
pub unsafe extern "C" fn panel_items(
    context: *mut c_void,
    which_raw: u32,
    flags: u32,
    from: usize,
    out: *mut NcPanelItem,
    count: usize,
) -> isize {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        return -1;
    };
    let Some(which) = which(which_raw) else {
        return -1;
    };
    let mut answers = context.panel_answers.borrow_mut();
    // A count asked (no room handed) reads them anew; the reads after it take what it read.
    let kept = !out.is_null()
        && (answers.items.as_ref()).is_some_and(|(of, with, _)| *of == which_raw && *with == flags);
    if !kept {
        drop(answers);
        let request = PanelRequest::Items {
            which,
            selected: flags & ITEMS_SELECTED != 0,
        };
        let Some(PanelAnswer::Items(items)) = ask(context, request) else {
            return -1;
        };
        answers = context.panel_answers.borrow_mut();
        answers.items = Some((which_raw, flags, items));
    }
    let Some((_, _, items)) = &answers.items else {
        return -1;
    };
    if !out.is_null() {
        for (at, item) in items.iter().skip(from).take(count).enumerate() {
            // SAFETY: the plugin hands room for `count` items; the texts stay in the context.
            unsafe { out.add(at).write(item_of(item)) };
        }
    }
    isize::try_from(items.len()).unwrap_or(isize::MAX)
}

fn item_of(item: &PanelItem) -> NcPanelItem {
    let flag = |on: bool, bit: u32| if on { bit } else { 0 };
    NcPanelItem {
        name: NcStr::new(&item.name),
        flags: flag(item.selected, ITEM_SELECTED)
            | flag(item.folder, ITEM_FOLDER)
            | flag(item.parent, ITEM_PARENT),
        attributes: item.attributes,
        size: item.size,
        allocated: item.allocated,
        modified: millis(item.modified),
        created: millis(item.created),
        accessed: millis(item.accessed),
        changed: millis(item.changed),
        links: item.links,
        path: NcStr::new(item.path.as_deref().unwrap_or("")),
        description: NcStr::new(&item.description),
        owner: NcStr::new(&item.owner),
    }
}

/// `NcHost::panel_do`.
pub unsafe extern "C" fn panel_do(
    context: *mut c_void,
    which_raw: u32,
    action: *const NcPanelAction,
    answer: *mut NcStr,
) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        return -1;
    };
    if action.is_null() {
        return -1;
    }
    // SAFETY: the plugin hands an action that lives through the call.
    let Some(action) = (unsafe { action_of(&*action) }) else {
        return -1;
    };
    // The command line, the viewer and the editor are no panel's: any `which` does for them.
    let no_panels = matches!(
        action,
        PanelAction::GetCommandLine
            | PanelAction::SetCommandLine(_)
            | PanelAction::InsertCommandLine(_)
            | PanelAction::View { .. }
            | PanelAction::Edit { .. }
    );
    let Some(which) = self::which(which_raw).or(no_panels.then_some(Which::Active)) else {
        return -1;
    };
    let answered = ask(context, PanelRequest::Do(which, action));
    let mut answers = context.panel_answers.borrow_mut();
    answers.items = None;
    match answered {
        Some(PanelAnswer::Text(text)) => {
            answers.text = text;
            if !answer.is_null() {
                // SAFETY: the plugin hands a writable text or null; the text stays in the context.
                unsafe { answer.write(NcStr::new(&answers.text)) };
            }
            0
        }
        Some(PanelAnswer::Done(true)) => 0,
        _ => -1,
    }
}

/// The action as the program reads it; none for one it does not know.
///
/// # Safety
/// `action`'s texts live through the call; `names` points to `name_count` texts or is null.
unsafe fn action_of(action: &NcPanelAction) -> Option<PanelAction> {
    let flags = action.flags;
    // SAFETY: as the caller promises.
    let names = unsafe { names_of(action) };
    let first = names.first().cloned();
    let typed = text(action.text);
    Some(match action.action {
        DO_SET_FOLDER => PanelAction::SetFolder {
            folder: typed,
            cursor: first,
        },
        DO_SET_CURSOR => PanelAction::SetCursor {
            index: action.number,
            name: (!typed.is_empty()).then_some(typed),
            top: (action.number2 != usize::MAX).then_some(action.number2),
        },
        DO_REREAD => PanelAction::Reread {
            keep_selection: flags & DO_KEEP_SELECTION != 0,
        },
        DO_SELECT => PanelAction::Select {
            names,
            on: flags & DO_OFF == 0,
            all: flags & DO_ALL != 0,
        },
        DO_SET_SORT => PanelAction::SetSort {
            sort: u32::try_from(action.number).ok()?,
            reverse: flags & DO_REVERSE != 0,
        },
        DO_CLOSE => PanelAction::Close {
            folder: (!typed.is_empty()).then_some(typed),
        },
        DO_GET_COMMAND_LINE => PanelAction::GetCommandLine,
        DO_SET_COMMAND_LINE => PanelAction::SetCommandLine(typed),
        DO_INSERT_COMMAND_LINE => PanelAction::InsertCommandLine(typed),
        DO_RUN_COMMAND => PanelAction::RunCommand(typed),
        DO_VIEW => PanelAction::View {
            path: typed,
            title: first,
            delete_on_close: flags & DO_DELETE_ON_CLOSE != 0,
        },
        DO_EDIT => PanelAction::Edit {
            path: typed,
            line: action.number,
            column: action.number2,
            delete_on_close: flags & DO_DELETE_ON_CLOSE != 0,
        },
        _ => return None,
    })
}

/// # Safety
/// As [`action_of`].
unsafe fn names_of(action: &NcPanelAction) -> Vec<String> {
    if action.names.is_null() {
        return Vec::new();
    }
    let count = action.name_count.min(MOST_NAMES);
    // SAFETY: as the caller promises: `count` texts from `names`.
    let names = unsafe { std::slice::from_raw_parts(action.names, count) };
    names.iter().map(|name| text(*name)).collect()
}

#[cfg(test)]
#[path = "panel_control_calls_tests.rs"]
mod tests;
