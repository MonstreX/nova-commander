//! The host's editor function (API 10, FAR's `EditorControl`): `NcHost::editor`, on the plugin's thread.
//! The request goes to the program, which answers it on its own thread with the editor up; the texts
//! handed back stay in the context until the next answer. In a panel's call nothing is asked.

use std::ffi::c_void;
use std::sync::mpsc::channel;

use nc_plugin_api::{
    EDITOR_BEEP, EDITOR_DELETE_CHAR, EDITOR_DELETE_LINE, EDITOR_EXPAND_TABS, EDITOR_FILE_NAME,
    EDITOR_GET_LINE, EDITOR_INFO, EDITOR_INSERT_LINE, EDITOR_INSERT_TEXT, EDITOR_PROCESS_INPUT,
    EDITOR_READ_INPUT, EDITOR_REAL_TO_TAB, EDITOR_REDRAW, EDITOR_SELECT, EDITOR_SET_KEY_BAR,
    EDITOR_SET_LINE, EDITOR_SET_POSITION, EDITOR_SET_TITLE, EDITOR_TAB_TO_REAL, EDITOR_UNDO_REDO,
    NcEditorAction, NcEditorAnswer, NcEditorInfo, NcStr,
};

use super::calls::context;
use super::{HostContext, Loaded, text};
use crate::plugin_host::Event;
use crate::plugin_host::editor_requests::{EditorAnswer, EditorInfo, EditorRequest};

/// The texts of the editor's last answer, valid until the next one.
#[derive(Default)]
pub struct EditorTexts {
    text: String,
    text2: String,
}

/// `request` sent and its answer waited for; none when the program is not listening or in a panel's
/// call.
fn ask(context: &HostContext, request: EditorRequest) -> Option<EditorAnswer> {
    if !context.started() || context.in_quiet_call() {
        return None;
    }
    let (reply, answer) = channel();
    let event = Event::EditorAsk {
        plugin: context.plugin.clone(),
        request,
        reply,
    };
    context.events.send(event).ok()?;
    answer.recv().ok()
}

/// The most a column, a width or a count a plugin gives is taken as (review D6: its sums overflowed,
/// a huge column padded billions of spaces).
const MOST: i64 = 1 << 20;

/// A number given (not negative).
fn given(number: i64) -> Option<u64> {
    u64::try_from(number).ok()
}

/// A column given (not negative), at most [`MOST`].
fn given_usize(number: i64) -> Option<usize> {
    usize::try_from(number.min(MOST)).ok()
}

/// A signed amount given, within [`MOST`] either way.
fn bounded(number: i64) -> i64 {
    number.clamp(-MOST, MOST)
}

/// A text given; none when it is longer than a text is taken (review D8: read as empty, it erased the
/// line it was to be).
fn given_text(raw: NcStr) -> Option<String> {
    (raw.len <= super::MOST_TEXT).then(|| text(raw))
}

/// The request as the program reads it; none for a command it does not know.
fn request_of(action: &NcEditorAction) -> Option<EditorRequest> {
    let line = given(action.line);
    Some(match action.command {
        EDITOR_INFO => EditorRequest::Info,
        EDITOR_GET_LINE => EditorRequest::GetLine(line),
        EDITOR_SET_LINE => EditorRequest::SetLine {
            line,
            text: given_text(action.text)?,
            eol: Some(given_text(action.text2)?).filter(|eol| !eol.is_empty()),
        },
        EDITOR_INSERT_LINE => EditorRequest::InsertLine(action.number == 1),
        EDITOR_DELETE_LINE => EditorRequest::DeleteLine,
        EDITOR_INSERT_TEXT => EditorRequest::InsertText(given_text(action.text)?),
        EDITOR_DELETE_CHAR => EditorRequest::DeleteChar,
        EDITOR_SET_POSITION => EditorRequest::SetPosition {
            line,
            pos: given_usize(action.pos),
            tab_pos: given_usize(action.tab_pos),
            top: given(action.top),
            left: given_usize(action.left),
            overtype: given(action.number).map(|on| on != 0),
        },
        // FAR's -1: the cursor's line (review D7: it was the first).
        EDITOR_SELECT => EditorRequest::Select {
            kind: action.number,
            line,
            pos: bounded(action.pos),
            width: bounded(action.width),
            height: bounded(action.number2),
        },
        EDITOR_UNDO_REDO => EditorRequest::UndoRedo(action.number),
        EDITOR_REDRAW => EditorRequest::Redraw,
        EDITOR_EXPAND_TABS => EditorRequest::ExpandTabs(line),
        EDITOR_REAL_TO_TAB => EditorRequest::RealToTab(line, given_usize(action.pos)?),
        EDITOR_TAB_TO_REAL => EditorRequest::TabToReal(line, given_usize(action.tab_pos)?),
        EDITOR_FILE_NAME => EditorRequest::FileName,
        EDITOR_READ_INPUT => EditorRequest::ReadInput,
        EDITOR_PROCESS_INPUT => EditorRequest::ProcessInput(text(action.text)),
        EDITOR_SET_TITLE => {
            EditorRequest::SetTitle(Some(text(action.text)).filter(|t| !t.is_empty()))
        }
        EDITOR_SET_KEY_BAR => EditorRequest::SetKeyBar(key_bar(&text(action.text))),
        EDITOR_BEEP => EditorRequest::Beep,
        _ => return None,
    })
}

/// [`EDITOR_SET_KEY_BAR`]'s lines as keys' names and labels; none for an empty text.
fn key_bar(text: &str) -> Option<Vec<(String, String)>> {
    if text.is_empty() {
        return None;
    }
    let label = |line: &str| match line.split_once('\t') {
        Some((key, label)) => (key.to_string(), label.to_string()),
        None => (line.to_string(), String::new()),
    };
    Some(
        text.lines()
            .filter(|line| !line.is_empty())
            .map(label)
            .collect(),
    )
}

/// FAR's `EditorInfo` as the ABI has it.
fn info_of(info: &EditorInfo) -> NcEditorInfo {
    let signed = |number: u64| i64::try_from(number).unwrap_or(i64::MAX);
    NcEditorInfo {
        width: signed(info.width),
        height: signed(info.height),
        lines: signed(info.lines),
        line: signed(info.line),
        pos: signed(info.pos),
        tab_pos: signed(info.tab_pos),
        top: signed(info.top),
        left: signed(info.left),
        overtype: i64::from(info.overtype),
        block: info.block,
        block_line: signed(info.block_line),
        options: info.options,
        tab_size: signed(info.tab_size),
        state: info.state,
        code_page: info.code_page,
    }
}

/// `NcHost::editor`.
pub unsafe extern "C" fn editor(
    context: *mut c_void,
    action: *const NcEditorAction,
    answer: *mut NcEditorAnswer,
) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        return -1;
    };
    if action.is_null() {
        return -1;
    }
    // SAFETY: the plugin hands an action that lives through the call.
    let Some(request) = request_of(unsafe { &*action }) else {
        return -1;
    };
    let Some(answered) = ask(context, request) else {
        return -1;
    };
    let mut texts = context.editor_texts.borrow_mut();
    let mut out = NcEditorAnswer::default();
    match answered {
        EditorAnswer::Done(false) => return -1,
        EditorAnswer::Done(true) => {}
        EditorAnswer::Info(info) => out.info = info_of(&info),
        EditorAnswer::Line { text, eol, block } => {
            (texts.text, texts.text2) = (text, eol);
            (out.number, out.number2) = block;
        }
        EditorAnswer::Number(number) => out.number = i64::try_from(number).unwrap_or(i64::MAX),
        EditorAnswer::Text(text) => texts.text = text,
    }
    out.text = NcStr::new(&texts.text);
    out.text2 = NcStr::new(&texts.text2);
    if !answer.is_null() {
        // SAFETY: the plugin hands a writable answer or null; its texts stay in the context.
        unsafe { answer.write(out) };
    }
    0
}

impl Loaded {
    /// `NcPlugin::editor_key`: the key the editor has done, by its name; why it failed.
    pub fn editor_key(&self, key: &str) -> Result<(), String> {
        let table = self.table();
        // SAFETY: as in `info`; the key's text lives through the call.
        match unsafe { (table.editor_key)(table.state, NcStr::new(key)) } {
            0 => Ok(()),
            _ => Err(self.last_error()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review D6, D7, D8 (and D12's missing host tests): the requests as the program reads them.
    #[test]
    fn a_plugins_numbers_and_texts_are_taken_within_bounds() {
        let mut select = NcEditorAction::new(EDITOR_SELECT);
        (select.number, select.pos, select.width, select.number2) = (1, i64::MAX, i64::MIN, 3);
        match request_of(&select) {
            Some(EditorRequest::Select {
                line,
                pos,
                width,
                height,
                ..
            }) => {
                assert_eq!(line, None, "-1: the cursor's line");
                assert_eq!((pos, width, height), (MOST, -MOST, 3));
            }
            other => panic!("{other:?}"),
        }
        let mut place = NcEditorAction::new(EDITOR_SET_POSITION);
        place.tab_pos = i64::MAX;
        assert!(matches!(
            request_of(&place),
            Some(EditorRequest::SetPosition { tab_pos: Some(tab), .. }) if tab == MOST as usize
        ));
        let mut set = NcEditorAction::new(EDITOR_SET_LINE);
        set.text = NcStr {
            ptr: b"x".as_ptr(),
            len: super::super::MOST_TEXT + 1,
        };
        assert!(
            request_of(&set).is_none(),
            "too long: refused, not an empty line"
        );
        set.text = NcStr::new("new");
        assert!(matches!(
            request_of(&set),
            Some(EditorRequest::SetLine { text, eol: None, .. }) if text == "new"
        ));
        // Review D4: `text2` is the break a last line without one gets (FAR's StringEOL).
        set.text2 = NcStr::new("\n");
        assert!(matches!(
            request_of(&set),
            Some(EditorRequest::SetLine { eol: Some(eol), .. }) if eol == "\n"
        ));
    }

    /// Review D8: a text or a break too long to be taken refuses the request (never read as empty);
    /// numbers within bounds pass as they are, a negative line is the cursor's.
    #[test]
    fn a_too_long_text_refuses_the_request_and_numbers_within_bounds_pass() {
        let too_long = NcStr {
            ptr: b"x".as_ptr(),
            len: super::super::MOST_TEXT + 1,
        };
        let mut insert = NcEditorAction::new(EDITOR_INSERT_TEXT);
        insert.text = too_long;
        assert!(request_of(&insert).is_none());
        insert.text = NcStr::new("ab");
        assert!(matches!(
            request_of(&insert),
            Some(EditorRequest::InsertText(text)) if text == "ab"
        ));
        let mut set = NcEditorAction::new(EDITOR_SET_LINE);
        (set.text, set.text2, set.line) = (NcStr::new("new"), too_long, 4);
        assert!(request_of(&set).is_none(), "its break too long");
        let mut select = NcEditorAction::new(EDITOR_SELECT);
        (select.number, select.line, select.pos) = (2, 7, -5);
        (select.width, select.number2) = (MOST - 1, -MOST - 9);
        assert!(matches!(
            request_of(&select),
            Some(EditorRequest::Select { kind: 2, line: Some(7), pos: -5, width, height })
                if width == MOST - 1 && height == -MOST
        ));
        let mut place = NcEditorAction::new(EDITOR_SET_POSITION);
        (place.tab_pos, place.left, place.pos) = (MOST - 1, -1, 3);
        assert!(matches!(
            request_of(&place),
            Some(EditorRequest::SetPosition { tab_pos: Some(tab), left: None, pos: Some(3), .. })
                if tab == (MOST - 1) as usize
        ));
    }

    /// Review D9: Brackets' "Beep if matched" through the program.
    #[test]
    fn the_beep_and_an_unknown_command_are_read() {
        assert!(matches!(
            request_of(&NcEditorAction::new(EDITOR_BEEP)),
            Some(EditorRequest::Beep)
        ));
        assert!(request_of(&NcEditorAction::new(9999)).is_none(), "unknown");
    }
}
