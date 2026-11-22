//! Brackets (FAR's brackets, `far-source/plugins/brackets`): "Brackets matching" in the editor's F11 menu
//! finds the pair of the bracket under the cursor (or just before it), nested pairs counted, over the
//! lines: "Search" puts the cursor on it, "Select" marks the stream block from the bracket through its
//! pair (the cursor on the pair with "Go to bracket when selecting"). A quotation mark that is its own
//! pair asks the way ("Forward" / "Backward"). FAR's menus and its "Configure" dialog (`BrackEng.lng`),
//! its settings by FAR's names; the cursor back where it was when there is no pair.

use nc_plugin_sdk::{
    Block, Dialog, DialogItem, Host, Info, Menu, MenuItem, Plugin, Position, Selection,
    export_plugin,
};

pub mod matching;

use matching::{Kind, Options, analyse, search, selection};

/// Its item in the editor's plugin commands and in "Plugins configuration" (FAR's `MTitle`).
const ITEM: &str = "Brackets matching";

pub struct Brackets {
    host: Host,
}

impl Brackets {
    fn options(&self) -> Options {
        let host = &self.host;
        let default = Options::default();
        let text =
            |key: &str, default: &str| host.setting(key).unwrap_or_else(|| default.to_string());
        Options {
            ignore_quotes: host.bool_setting("IgnoreQuotes", default.ignore_quotes),
            ignore_after: host.bool_setting("IgnoreAfter", default.ignore_after),
            bracket_prior: host.bool_setting("BracketPrior", default.bracket_prior),
            jump_to_pair: host.bool_setting("JumpToPair", default.jump_to_pair),
            beep: host.bool_setting("Beep", default.beep),
            quotes: text("QuotesType", &default.quotes),
            brackets1: text("Brackets1", &default.brackets1),
            brackets2: text("Brackets2", &default.brackets2),
        }
    }

    /// FAR's `Config`: the rules and the definitions; true when saved.
    fn configure_dialog(&self) -> bool {
        let options = self.options();
        let check = |label: &str, on: bool, y: i32| DialogItem::check(label, on).at(5, y, 5, y);
        let field = |text: &str, width: i32, y: i32| DialogItem::edit(text).at(5, y, 5 + width, y);
        let items = vec![
            DialogItem::single_box("Rules").at(3, 1, 46, 7),
            check(
                "&Ignore brackets in quotation marks",
                options.ignore_quotes,
                2,
            ),
            check("Do not jump if &after bracket", options.ignore_after, 3),
            check("&Priority of double brackets", options.bracket_prior, 4),
            check("&Go to bracket when selecting", options.jump_to_pair, 5),
            check("&Beep if matched", options.beep, 6),
            DialogItem::single_box("Definitions").at(3, 8, 46, 15),
            DialogItem::text("&Quotation marks:").at(5, 9, 22, 9),
            field(&options.quotes, 20, 10),
            DialogItem::text("&Single brackets:").at(5, 11, 22, 11),
            field(&options.brackets1, 20, 12),
            DialogItem::text("&Double brackets:").at(5, 13, 22, 13),
            field(&options.brackets2, 40, 14),
            DialogItem::button("Save")
                .default_button()
                .at(14, 16, 21, 16),
            DialogItem::button("Cancel").at(24, 16, 33, 16),
        ];
        let dialog = Dialog::new("Brackets matching - Configure", items).size(50, 19);
        let closed = self.host.dialog(&dialog);
        if closed.button != Some(13) {
            return false;
        }
        let value = |item: usize| closed.values.get(item).cloned().unwrap_or_default();
        let host = &self.host;
        for (key, item) in [
            ("IgnoreQuotes", 1),
            ("IgnoreAfter", 2),
            ("BracketPrior", 3),
            ("JumpToPair", 4),
            ("Beep", 5),
        ] {
            host.set_bool(key, value(item).checked());
        }
        for (key, item) in [("QuotesType", 8), ("Brackets1", 10), ("Brackets2", 12)] {
            host.set_setting(key, &value(item).text);
        }
        true
    }

    /// FAR's `ShowMenu`: two items and "Configure" (which opens the dialog and the menu again).
    fn ask(&self, first: &str, second: &str) -> Option<usize> {
        loop {
            let items = vec![
                MenuItem::new(first),
                MenuItem::new(second),
                MenuItem::separator(),
                MenuItem::new("&Configure"),
            ];
            let mut menu = Menu::new(ITEM, items);
            menu.wrap = true;
            match self.host.menu(&menu)?.item? {
                3 => {
                    self.configure_dialog();
                }
                item => return Some(item),
            }
        }
    }

    /// FAR's `OpenW`.
    fn matching(&self) {
        let Some(select) = self.ask("S&earch", "&Select").map(|item| item == 1) else {
            return;
        };
        let host = &self.host;
        let (Some(info), Some(line)) = (host.editor_info(), host.editor_line(None)) else {
            return;
        };
        let options = self.options();
        let text: Vec<char> = line.text.chars().collect();
        let Some(bracket) = analyse(&text, info.pos, &options) else {
            return;
        };
        let direction = match bracket.kind {
            Kind::OneMath => match self.ask("&Forward", "&Backward") {
                Some(1) => -1,
                Some(_) => 1,
                None => return,
            },
            _ => bracket.direction,
        };
        let mut read = |at: usize| {
            host.editor_line(Some(at))
                .map(|line| line.text.chars().collect())
        };
        let origin = (info.line, info.pos);
        let Some(found) = search(
            &mut read,
            origin,
            info.lines,
            (&bracket, direction),
            &options,
        ) else {
            return;
        };
        if !select || options.jump_to_pair {
            let shown = found.0 >= info.top && found.0 < info.top + info.height;
            host.set_editor_position(&Position {
                line: Some(found.0),
                pos: Some(found.1),
                top: (!shown).then(|| found.0.saturating_sub(info.height / 2)),
                ..Position::default()
            });
        }
        // Brackets.cpp: `if (Opt.Beep) MessageBeep(0)` once the pair is found (review D9: unused).
        if options.beep {
            host.editor_beep();
        }
        if select {
            let (line, pos, width, height) = selection(origin, found, (bracket.kind, direction));
            host.editor_select(Some(&Selection {
                block: Block::Stream,
                line,
                pos,
                width,
                height,
            }));
        }
    }
}

impl Plugin for Brackets {
    fn new(host: Host) -> Self {
        Self { host }
    }

    fn info(&self) -> Info {
        Info {
            title: "Brackets".to_string(),
            description: "The pair of the bracket under the cursor in the editor, found or marked"
                .to_string(),
            author: "FAR Group's plugin, ported".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn editor_items(&self) -> Vec<String> {
        vec![ITEM.to_string()]
    }

    fn run_editor_item(&mut self, _index: usize) -> Result<(), String> {
        self.matching();
        Ok(())
    }

    fn config_items(&self) -> Vec<String> {
        vec![ITEM.to_string()]
    }

    fn configure(&mut self, _index: usize) -> bool {
        self.configure_dialog()
    }
}

export_plugin!(Brackets);
