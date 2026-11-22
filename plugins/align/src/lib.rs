//! Align (FAR's align, `far-source/plugins/align`): "Align block" in the editor's F11 menu asks for the
//! right margin and the ways (FAR's dialog, `AlignEng.lng`) and reformats the stream block's lines - joined,
//! broken again at the margin under their least indent, short words not left at the lines' ends when
//! asked, each line but the last spread to the margin with "Justify right border" - or, without
//! "Reformat block", only spreads its lines. Its settings by FAR's names. Differs: the work is one undo
//! step (FAR's is a step per line it changes).

use nc_plugin_sdk::{Block, Dialog, DialogItem, Host, Info, Plugin, Position, export_plugin};

pub mod format;

/// Its item in the editor's plugin commands and its dialog's title (FAR's `MAlign`).
const ITEM: &str = "Align block";
/// The dialog's items read back.
const MARGIN: usize = 0;
const REFORMAT: usize = 2;
const SMART: usize = 3;
const JUSTIFY: usize = 4;
const OK: usize = 6;

/// What the dialog sets (FAR's settings `RightMargin`, `Reformat`, `SmartMode`, `Justify`).
struct Options {
    margin: usize,
    reformat: bool,
    smart: bool,
    justify: bool,
}

pub struct Align {
    host: Host,
}

impl Align {
    /// FAR's dialog; the options it was left with, none when not OK.
    fn ask(&self) -> Option<Options> {
        let host = &self.host;
        let margin = host.int_setting("RightMargin", 75);
        let check = |label: &str, key: &str, on: bool, y: i32| {
            DialogItem::check(label, host.bool_setting(key, on)).at(5, y, 5, y)
        };
        let items = vec![
            DialogItem::edit(&margin.to_string()).at(5, 2, 7, 2),
            DialogItem::text("Right &margin").at(9, 2, 21, 2),
            check("&Reformat block", "Reformat", true, 3),
            check(
                "Do not leave &short words at line ends",
                "SmartMode",
                false,
                4,
            ),
            check("&Justify right border", "Justify", false, 5),
            DialogItem::separator().at(-1, 6, -1, 6),
            DialogItem::button("Ok").default_button().at(16, 7, 21, 7),
            DialogItem::button("Cancel").at(24, 7, 33, 7),
        ];
        let closed = host.dialog(&Dialog::new(ITEM, items).size(54, 10));
        if closed.button != Some(OK) {
            return None;
        }
        let value = |item: usize| closed.values.get(item).cloned().unwrap_or_default();
        let options = Options {
            margin: value(MARGIN).text.trim().parse().unwrap_or(0),
            reformat: value(REFORMAT).checked(),
            smart: value(SMART).checked(),
            justify: value(JUSTIFY).checked(),
        };
        host.set_bool("Reformat", options.reformat);
        host.set_int("RightMargin", i64::try_from(options.margin).unwrap_or(0));
        host.set_bool("SmartMode", options.smart);
        host.set_bool("Justify", options.justify);
        Some(options)
    }

    /// The block's lines from its first one, their tabs as spaces (FAR's loop over `ECTL_GETSTRING`
    /// with `ECTL_EXPANDTABS`): to a line the block is not on, or ends at the start of.
    fn block_lines(&self, first: usize) -> Vec<String> {
        let host = &self.host;
        let mut lines = Vec::new();
        for number in first.. {
            let Some(line) = host.editor_line(Some(number)) else {
                break;
            };
            if line.block.is_none_or(|(start, past)| past == Some(start)) {
                break;
            }
            host.editor_expand_tabs(Some(number));
            lines.push(
                host.editor_line(Some(number))
                    .map(|line| line.text)
                    .unwrap_or_default(),
            );
        }
        lines
    }

    /// FAR's `ReformatBlock`: the block's lines out, the reformatted ones in their place, the cursor
    /// back.
    fn reformat(&self, options: &Options) {
        let host = &self.host;
        let Some(info) = host
            .editor_info()
            .filter(|info| info.block == Block::Stream)
        else {
            return;
        };
        if options.margin < 1 {
            return;
        }
        let lines = self.block_lines(info.block_line);
        let first = Position {
            line: Some(info.block_line),
            pos: Some(0),
            ..Position::default()
        };
        host.set_editor_position(&first);
        for _ in &lines {
            host.editor_delete_line();
        }
        let formatted = format::reformat(&lines, options.margin, options.smart, options.justify);
        host.set_editor_position(&first);
        host.editor_insert_text(&format!("{}\n", formatted.join("\n")));
        host.set_editor_position(&Position {
            line: Some(info.line),
            pos: Some(info.pos),
            ..Position::default()
        });
    }

    /// FAR's `JustifyBlock`: each line of the block shorter than the margin spread to it.
    fn justify(&self, margin: usize) {
        let host = &self.host;
        let Some(info) = host
            .editor_info()
            .filter(|info| info.block == Block::Stream)
        else {
            return;
        };
        let lines = self.block_lines(info.block_line);
        for (number, line) in (info.block_line..).zip(&lines) {
            let chars: Vec<char> = line.chars().collect();
            if chars.len() < margin
                && let Some(spread) = format::justified(&chars, margin)
            {
                host.set_editor_line(Some(number), &spread.iter().collect::<String>());
            }
        }
    }
}

impl Plugin for Align {
    fn new(host: Host) -> Self {
        Self { host }
    }

    fn info(&self) -> Info {
        Info {
            title: "Align".to_string(),
            description: "The editor's block reformatted at a right margin, or justified"
                .to_string(),
            author: "FAR Group's plugin, ported".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn editor_items(&self) -> Vec<String> {
        vec![ITEM.to_string()]
    }

    fn run_editor_item(&mut self, _index: usize) -> Result<(), String> {
        let Some(options) = self.ask() else {
            return Ok(());
        };
        self.host.editor_undo(nc_plugin_sdk::api::UNDO_BEGIN);
        if options.reformat {
            self.reformat(&options);
        } else if options.justify {
            self.justify(options.margin);
        }
        self.host.editor_undo(nc_plugin_sdk::api::UNDO_END);
        Ok(())
    }
}

export_plugin!(Align);
