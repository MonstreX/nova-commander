//! EditCase (FAR's editcase, `far-source/plugins/editcase`): "Change Case" in the editor's F11 menu
//! converts the block's text - or, with no block, the word at the cursor or the nearest one - to lower
//! case, Title Case, UPPER CASE, tOGGLE cASE, or cyclically (lower, Title, UPPER as Microsoft Word's
//! Shift+F3) by its first word. FAR's menu (`ECaseEng.lng`), its line by line work through the editor
//! (`ECTL_GETINFO`, `ECTL_SETPOSITION`, `ECTL_GETSTRING`, `ECTL_SETSTRING`), the cursor and the screen put
//! back after a block. Its extra word dividers are its setting `AddWordDiv` (`#`).

use nc_plugin_sdk::{Block, Host, Info, Menu, MenuItem, Plugin, Position, export_plugin};

pub mod case;

use case::{ADD_WORD_DIV, Case, Dividers, change_case, find_bounds, next_case};

/// Its item in the editor's plugin commands and its menu's title (FAR's `MCaseConversion`).
const ITEM: &str = "Change Case";
/// FAR's menu items (`MCaseLower` ... `MCaseCyclic`).
const CASES: [&str; 5] = [
    "&L. lower case",
    "&T. Title Case",
    "&U. UPPER CASE",
    "&G. tOGGLE cASE",
    "&C. Cyclic change",
];

pub struct EditCase {
    host: Host,
}

impl Plugin for EditCase {
    fn new(host: Host) -> Self {
        Self { host }
    }

    fn info(&self) -> Info {
        Info {
            title: "EditCase".to_string(),
            description:
                "Case conversion of the selected text or the word under the cursor in the \
                          editor"
                    .to_string(),
            author: "FAR Group's plugin, ported".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn editor_items(&self) -> Vec<String> {
        vec![ITEM.to_string()]
    }

    fn run_editor_item(&mut self, _index: usize) -> Result<(), String> {
        let mut menu = Menu::new(ITEM, CASES.iter().map(|text| MenuItem::new(text)).collect());
        menu.auto_hotkeys = true;
        menu.wrap = true;
        let Some(item) = self.host.menu(&menu).and_then(|chosen| chosen.item) else {
            return Ok(());
        };
        let add = self.host.setting("AddWordDiv");
        let dividers = Dividers::new(add.as_deref().unwrap_or(ADD_WORD_DIV));
        convert(&self.host, Case::ALL[item], &dividers);
        Ok(())
    }
}

/// FAR's `OpenW` after the menu: the block's lines one by one (the cursor put on each), or the word at
/// the cursor; the cursor and the screen back after a block.
fn convert(host: &Host, mut case: Case, dividers: &Dividers) {
    let Some(info) = host.editor_info() else {
        return;
    };
    let block = info.block != Block::None;
    let mut line = info.block_line;
    loop {
        if block {
            if line >= info.lines {
                break;
            }
            let at = Position {
                line: Some(line),
                ..Position::default()
            };
            host.set_editor_position(&at);
            line += 1;
        }
        let Some(text) = host.editor_line(None) else {
            break;
        };
        let mut chars: Vec<char> = text.text.chars().collect();
        if (block && text.block.is_none()) || (!block && chars.is_empty()) {
            break;
        }
        if chars.is_empty() {
            continue;
        }
        let bounds = match (block, text.block) {
            (true, Some((start, past))) => {
                let end = past.unwrap_or(chars.len()).min(chars.len());
                Some((start, end.max(start)))
            }
            _ => find_bounds(&chars, info.pos, (0, chars.len()), dividers),
        };
        if let Some(bounds) = bounds {
            if case == Case::Cyclic {
                case = next_case(&chars, bounds, dividers);
            }
            if case != Case::Cyclic {
                change_case(&mut chars, bounds, case, dividers);
                host.set_editor_line(None, &chars.iter().collect::<String>());
            }
        }
        if !block {
            break;
        }
    }
    if block {
        host.set_editor_position(&Position {
            line: Some(info.line),
            pos: Some(info.pos),
            top: Some(info.top),
            left: Some(info.left),
            overtype: Some(info.overtype),
            ..Position::default()
        });
    }
}

export_plugin!(EditCase);
