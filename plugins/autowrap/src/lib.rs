//! AutoWrap (FAR's autowrap, `far-source/plugins/autowrap`): with "Enable auto wrap" a line typed past the
//! right margin is broken at its last blank before the margin, the rest going on the next line (with the
//! editor's auto indent), the cursor at its end; again while it is still too long. Only in files of its
//! masks and not of its excluded ones. "Auto wrap" in the editor's F11 menu is FAR's dialog
//! (`WrapEng.lng`); its settings by FAR's names (`Wrap`, `RightMargin`, `FileMasks`, `ExcludeFileMasks`).
//! It takes the editor's keys (FAR's `ProcessEditorInputW`): the program has done each one first, as
//! AutoWrap's own `ECTL_PROCESSINPUT` does; a key that changed the cursor's line's length on that line
//! is a typed one (FAR compares the line before and after the key: here the line the last key left in
//! the same file; with none known there, the first key in an editor, a key that types a character is
//! one - review D10: a line left in another editor counted, the first character typed never wrapped).

use nc_plugin_sdk::{Dialog, DialogItem, Host, Info, Plugin, Position, export_plugin};

/// Its item in the editor's plugin commands and its dialog's title (FAR's `MAutoWrap`).
const ITEM: &str = "Auto wrap";
/// The dialog's items read back.
const ENABLE: usize = 0;
const MARGIN: usize = 1;
const MASKS: usize = 5;
const EXCLUDED: usize = 7;
const OK: usize = 8;

/// FAR's options.
struct Options {
    wrap: bool,
    margin: usize,
    masks: String,
    excluded: String,
}

pub struct AutoWrap {
    host: Host,
    /// The edited file, the cursor's line and its length as the last key left them.
    last: Option<(String, usize, usize)>,
}

/// Whether the key named `key` types a character (a letter's name is the letter).
fn types(key: &str) -> bool {
    key.chars().count() == 1 || matches!(key, "Space" | "Shift+Space" | "Tab")
}

impl AutoWrap {
    fn options(&self) -> Options {
        let host = &self.host;
        Options {
            wrap: host.bool_setting("Wrap", false),
            margin: usize::try_from(host.int_setting("RightMargin", 75)).unwrap_or(75),
            masks: host
                .setting("FileMasks")
                .unwrap_or_else(|| "*.*".to_string()),
            excluded: host.setting("ExcludeFileMasks").unwrap_or_default(),
        }
    }

    /// FAR's dialog; the settings kept when OK.
    fn ask_options(&self) {
        let options = self.options();
        let host = &self.host;
        let items = vec![
            DialogItem::check("Enable auto &wrap", options.wrap).at(5, 2, 5, 2),
            DialogItem::edit(&options.margin.to_string()).at(5, 3, 7, 3),
            DialogItem::text("Right &margin").at(9, 3, 21, 3),
            DialogItem::separator().at(-1, 4, -1, 4),
            DialogItem::text("File masks delimited with commas, where wrap should be &enabled")
                .at(5, 5, 70, 5),
            DialogItem::edit(&options.masks).at(5, 6, 70, 6),
            DialogItem::text("File masks delimited with commas, where wrap should be &disabled")
                .at(5, 7, 70, 7),
            DialogItem::edit(&options.excluded).at(5, 8, 70, 8),
            DialogItem::button("Ok").default_button().at(28, 10, 33, 10),
            DialogItem::button("Cancel").at(36, 10, 45, 10),
        ];
        let closed = host.dialog(&Dialog::new(ITEM, items).size(76, 13));
        if closed.button != Some(OK) {
            return;
        }
        let value = |item: usize| closed.values.get(item).cloned().unwrap_or_default();
        host.set_bool("Wrap", value(ENABLE).checked());
        let margin = value(MARGIN).text.trim().parse().unwrap_or(options.margin);
        host.set_int("RightMargin", i64::try_from(margin).unwrap_or(75));
        host.set_setting("FileMasks", &value(MASKS).text);
        host.set_setting("ExcludeFileMasks", &value(EXCLUDED).text);
    }

    /// Whether the edited file is one of the masks and none of the excluded ones (FAR's
    /// `ProcessName` with `PN_SKIPPATH`).
    fn file_taken(&self, options: &Options, file: &str) -> bool {
        if options.masks.is_empty() {
            return true;
        }
        let name = std::path::Path::new(file)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let matches =
            |masks: &str| !masks.is_empty() && self.host.match_masks(masks, &name).unwrap_or(false);
        matches(&options.masks) && !matches(&options.excluded)
    }

    /// FAR's `ProcessEditorInputW` after the key `key`: the line broken while it is past the margin.
    fn wrap(&mut self, options: &Options, key: &str) {
        let host = &self.host;
        let (Some(info), Some(line)) = (host.editor_info(), host.editor_line(None)) else {
            return;
        };
        let file = host.editor_file_name().unwrap_or_default();
        let length = line.text.chars().count();
        let typed = match &self.last {
            Some((last_file, last_line, last_length)) if *last_file == file => {
                *last_line == info.line && *last_length != length
            }
            _ => types(key),
        };
        self.last = Some((file.clone(), info.line, length));
        if !typed || !self.file_taken(options, &file) {
            return;
        }
        while self.break_line(options) {}
        if let (Some(info), Some(line)) = (host.editor_info(), host.editor_line(None)) {
            self.last = Some((file, info.line, line.text.chars().count()));
        }
    }

    /// One of FAR's passes: the cursor's line broken at its last blank before the margin, the cursor
    /// at the next line's end; false when there is nothing (more) to break.
    fn break_line(&self, options: &Options) -> bool {
        let host = &self.host;
        let (Some(info), Some(line)) = (host.editor_info(), host.editor_line(None)) else {
            return false;
        };
        let text: Vec<char> = line.text.chars().collect();
        let tabs = text.contains(&'\t');
        let shown = |pos: usize| match tabs {
            true => host.editor_real_to_tab(None, pos).unwrap_or(pos),
            false => pos,
        };
        if shown(text.len()) < options.margin || info.pos < text.len() {
            return false;
        }
        let blank = |ch: char| ch == ' ' || ch == '\t';
        let mut space = None;
        for at in (1..text.len()).rev() {
            if blank(text[at]) {
                space = Some(at);
                if shown(at) < options.margin {
                    break;
                }
            }
        }
        let Some(space) = space else {
            return false;
        };
        if text[..space].iter().all(|ch| blank(*ch)) {
            return false;
        }
        host.set_editor_position(&Position {
            pos: Some(space + 1),
            ..Position::default()
        });
        if !host.editor_insert_line(true) {
            return false;
        }
        let next = host
            .editor_line(Some(info.line + 1))
            .map_or(0, |line| line.text.chars().count());
        host.set_editor_position(&Position {
            line: Some(info.line + 1),
            pos: Some(next),
            ..Position::default()
        });
        host.editor_redraw();
        true
    }
}

impl Plugin for AutoWrap {
    fn new(host: Host) -> Self {
        Self { host, last: None }
    }

    fn info(&self) -> Info {
        Info {
            title: "AutoWrap".to_string(),
            description: "Lines typed past the right margin wrapped in the editor".to_string(),
            author: "FAR Group's plugin, ported".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn editor_items(&self) -> Vec<String> {
        vec![ITEM.to_string()]
    }

    fn run_editor_item(&mut self, _index: usize) -> Result<(), String> {
        self.ask_options();
        Ok(())
    }

    fn takes_editor_keys(&self) -> bool {
        true
    }

    fn editor_key(&mut self, key: &str) {
        let options = self.options();
        if !options.wrap || key == "F1" {
            return;
        }
        self.wrap(&options, key);
    }
}

export_plugin!(AutoWrap);
