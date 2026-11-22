//! The system clipboard: text (a vertical block's too), and files to paste in a file manager (Ctrl+C /
//! Ctrl+X in Explorer).

pub use super::os::{
    clipboard_is_column, clipboard_text, set_clipboard_column_text, set_clipboard_files,
    set_clipboard_text,
};
