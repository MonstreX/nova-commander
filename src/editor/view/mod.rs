//! The view core (ADR-0015): a file of any size shown without reading it whole. Positions are byte
//! offsets, only the blocks the screen needs are read (by the caller, who hands them in), and only the
//! rows on screen are laid out. Quick view (Ctrl+Q) shows files with it; the viewer (F3, stage 2d)
//! grows from it.

mod blocks;
mod code_page;
pub mod edit_view;
pub mod hex_cursor;
mod line_count;
mod lines;
pub mod pieces;
pub mod search;
mod text_view;

pub use blocks::{BLOCK, KEPT_WHOLE, Missing};
pub use code_page::{ByteDecoder, Detected, Encoding, detect};
pub use lines::{Layout, Line, LineSource};
pub use text_view::{Mode, Move, Screen, TextView};
