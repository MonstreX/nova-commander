//! The editor's calls as the SDK makes them (API 10): what each sends and how the answers come back.

use std::cell::RefCell;
use std::ffi::c_void;

use crate::api::API_VERSION;
use crate::api::{self, NcEditorAction, NcEditorAnswer, NcStr};
use crate::{Block, EditorLine, Host, Position, Selection};

/// An action the fake editor got: the command, line, pos, tab pos, width, number, number2, text.
type Sent = (u32, i64, i64, i64, i64, i64, i64, String);

thread_local! {
    static SENT: RefCell<Vec<Sent>> = const { RefCell::new(Vec::new()) };
}

/// The line text and break the fake editor answers with, alive while the tests run.
static LINE: &str = "two words";
static BREAK: &str = "\r\n";
static NAME: &str = "C:\\e.txt";

/// The tests' editor: it keeps what it gets and answers the reads.
pub(crate) unsafe extern "C" fn fake_editor(
    _context: *mut c_void,
    action: *const NcEditorAction,
    answer: *mut NcEditorAnswer,
) -> i32 {
    // SAFETY: the SDK hands an action and an answer alive through the call.
    let (action, answer) = unsafe { (&*action, &mut *answer) };
    // SAFETY: the SDK's text, alive through the call.
    let text = unsafe { action.text.as_str() }.to_string();
    SENT.with_borrow_mut(|sent| {
        sent.push((
            action.command,
            action.line,
            action.pos,
            action.tab_pos,
            action.width,
            action.number,
            action.number2,
            text,
        ));
    });
    match action.command {
        api::EDITOR_INFO => {
            answer.info.line = 3;
            answer.info.lines = 10;
            answer.info.block = api::BLOCK_COLUMN;
        }
        api::EDITOR_GET_LINE if action.line > 5 => return -1,
        api::EDITOR_GET_LINE => {
            answer.text = NcStr::new(LINE);
            answer.text2 = NcStr::new(BREAK);
            (answer.number, answer.number2) = (4, -1);
        }
        api::EDITOR_REAL_TO_TAB => answer.number = action.pos * 2,
        api::EDITOR_FILE_NAME => answer.text = NcStr::new(NAME),
        _ => {}
    }
    0
}

fn sent() -> Vec<Sent> {
    SENT.with_borrow_mut(std::mem::take)
}

#[test]
fn the_editor_s_reads_come_back_as_the_sdk_s_types() {
    let table = crate::tests::host(API_VERSION);
    let host = Host {
        table: &raw const table,
    };
    sent();
    let info = host.editor_info().unwrap();
    assert_eq!((info.line, info.lines, info.block), (3, 10, Block::Column));
    let line = host.editor_line(Some(2)).unwrap();
    assert_eq!(
        line,
        EditorLine {
            text: LINE.to_string(),
            eol: BREAK.to_string(),
            block: Some((4, None)),
        }
    );
    assert!(host.editor_line(Some(9)).is_none(), "past the last line");
    assert_eq!(host.editor_real_to_tab(None, 3), Some(6));
    assert_eq!(host.editor_file_name().as_deref(), Some(NAME));
    let lines: Vec<i64> = sent().iter().map(|sent| sent.1).collect();
    assert_eq!(lines, [-1, 2, 9, -1, -1], "None the cursor's line: -1");
}

#[test]
fn the_editor_s_changes_send_what_they_name() {
    let table = crate::tests::host(API_VERSION);
    let host = Host {
        table: &raw const table,
    };
    sent();
    assert!(host.set_editor_line(None, "TWO"));
    assert!(host.editor_insert_text("a\nb"));
    assert!(host.editor_insert_line(true));
    let sent = sent();
    assert_eq!(sent[0].0, api::EDITOR_SET_LINE);
    assert_eq!(sent[0].7, "TWO");
    assert_eq!(
        (sent[1].0, sent[1].7.as_str()),
        (api::EDITOR_INSERT_TEXT, "a\nb")
    );
    assert_eq!((sent[2].0, sent[2].5), (api::EDITOR_INSERT_LINE, 1));
}

#[test]
fn the_editor_s_position_block_and_undo_send_what_they_name() {
    let table = crate::tests::host(API_VERSION);
    let host = Host {
        table: &raw const table,
    };
    sent();
    let position = Position {
        line: Some(4),
        pos: Some(2),
        overtype: Some(true),
        ..Position::default()
    };
    assert!(host.set_editor_position(&position));
    let selection = Selection {
        block: Block::Stream,
        line: 1,
        pos: 0,
        width: 3,
        height: 2,
    };
    assert!(host.editor_select(Some(&selection)));
    assert!(host.editor_select(None));
    assert!(host.editor_undo(api::UNDO_BEGIN));
    let sent = sent();
    assert_eq!(
        (sent[0].1, sent[0].2, sent[0].3, sent[0].5),
        (4, 2, -1, 1),
        "the line, the position, no tab position, overtype"
    );
    assert_eq!(
        (sent[1].5, sent[1].1, sent[1].2, sent[1].4, sent[1].6),
        (api::BLOCK_STREAM, 1, 0, 3, 2)
    );
    assert_eq!(sent[2].5, api::BLOCK_NONE, "no block");
    assert_eq!(
        (sent[3].0, sent[3].5),
        (api::EDITOR_UNDO_REDO, api::UNDO_BEGIN)
    );
}
