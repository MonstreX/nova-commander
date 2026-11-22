//! The probe's editor items (API 10, FAR's `PF_EDITOR` commands): "Upper line" sets the cursor's line in
//! capitals; "Mark start" marks the first three characters of the cursor's line as a stream block;
//! "Two lines" types `a`, Enter, `b` at the cursor as one undo step; "Info" logs the editor's state and
//! the file's name; "Ask, then type" shows a message, then types `z` at the cursor.

use nc_plugin_sdk::{Block, Host, Selection};

pub const ITEMS: [&str; 5] = [
    "Upper line",
    "Mark start",
    "Two lines",
    "Info",
    "Ask, then type",
];

pub fn run(host: &Host, index: usize) -> Result<(), String> {
    let failed = || "The editor did not answer".to_string();
    match ITEMS.get(index).copied() {
        Some("Upper line") => {
            let line = host.editor_line(None).ok_or_else(failed)?;
            host.set_editor_line(None, &line.text.to_uppercase())
                .then_some(())
                .ok_or_else(failed)
        }
        Some("Mark start") => {
            let info = host.editor_info().ok_or_else(failed)?;
            let selection = Selection {
                block: Block::Stream,
                line: info.line,
                pos: 0,
                width: 3,
                height: 1,
            };
            host.editor_select(Some(&selection))
                .then_some(())
                .ok_or_else(failed)
        }
        Some("Two lines") => {
            host.editor_undo(nc_plugin_sdk::api::UNDO_BEGIN);
            let typed = host.editor_insert_text("a\nb");
            host.editor_undo(nc_plugin_sdk::api::UNDO_END);
            typed.then_some(()).ok_or_else(failed)
        }
        Some("Info") => {
            let info = host.editor_info().ok_or_else(failed)?;
            let name = host.editor_file_name().unwrap_or_default();
            host.log(&format!(
                "editor: line {} of {}, pos {}, tab {}, block {:?}, file {name}",
                info.line, info.lines, info.pos, info.tab_pos, info.block
            ));
            Ok(())
        }
        Some("Ask, then type") => {
            host.message("Probe", &["Go on?"]);
            host.editor_insert_text("z")
                .then_some(())
                .ok_or_else(failed)
        }
        _ => Err(format!("No editor item {index}")),
    }
}
