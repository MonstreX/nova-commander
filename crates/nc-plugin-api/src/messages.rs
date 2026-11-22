//! The flags of [`NcHost::message`](crate::NcHost::message) and [`NcHost::input`](crate::NcHost::input)
//! (FAR's `FMSG_*` and `FIB_*`).

/// A message in the warning colours (FAR's `FMSG_WARNING`).
pub const MESSAGE_WARNING: u32 = 1;
/// A message's lines from the left (FAR's `FMSG_LEFTALIGN`), not centred.
pub const MESSAGE_LEFT_ALIGN: u32 = 2;
/// The system's text of the plugin thread's last error added under a message's lines (FAR's
/// `FMSG_ERRORTYPE`: `GetLastError` as the call comes; nothing when it is 0).
pub const MESSAGE_ERROR: u32 = 4;
/// The bits of a message's button set (FAR's `FMSG_MB_*`); 0: the plugin's own buttons.
pub const MESSAGE_BUTTONS: u32 = 0xF0;
/// "OK".
pub const MESSAGE_BUTTONS_OK: u32 = 0x10;
/// "OK", "Cancel".
pub const MESSAGE_BUTTONS_OK_CANCEL: u32 = 0x20;
/// "Abort", "Retry", "Ignore".
pub const MESSAGE_BUTTONS_ABORT_RETRY_IGNORE: u32 = 0x30;
/// "Yes", "No".
pub const MESSAGE_BUTTONS_YES_NO: u32 = 0x40;
/// "Yes", "No", "Cancel".
pub const MESSAGE_BUTTONS_YES_NO_CANCEL: u32 = 0x50;
/// "Retry", "Cancel".
pub const MESSAGE_BUTTONS_RETRY_CANCEL: u32 = 0x60;

/// An input line's empty answer taken (FAR's `FIB_ENABLEEMPTY`); without it an empty one is Esc.
pub const INPUT_ENABLE_EMPTY: u32 = 0x1;
/// The text shown as stars (`FIB_PASSWORD`).
pub const INPUT_PASSWORD: u32 = 0x2;
/// `%NAME%` in the answer replaced by the variable's value (`FIB_EXPANDENV`).
pub const INPUT_EXPAND_ENV: u32 = 0x4;
/// With a history, an empty line does not start with its last one (`FIB_NOUSELASTHISTORY`).
pub const INPUT_NO_LAST_HISTORY: u32 = 0x8;
/// "OK" and "Cancel" under the line (`FIB_BUTTONS`).
pub const INPUT_BUTTONS: u32 = 0x10;
/// `&` in the prompt marks a hotkey rather than being shown (`FIB_NOAMPERSAND`).
pub const INPUT_NO_AMPERSAND: u32 = 0x20;
/// The line is a path, completed as the dialogs' lines are (`FIB_EDITPATH`).
pub const INPUT_EDIT_PATH: u32 = 0x40;
