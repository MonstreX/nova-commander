//! FarColorer's "Outliner list", "Errors list" and "Find Function" (S2d.7e3; `FarEditor::showOutliner`,
//! `locateFunction`): the document's outline (or its errors) as a menu, `Outliner [filter]` -
//! letters, digits, Space and `;:-_~` narrow it, BS takes one back, Tab completes what the items shown
//! have in common, Ctrl+Left / Ctrl+Right show fewer or more levels of the tree, Ctrl+Up / Ctrl+Down
//! move the editor to the item before or after, Enter goes to the item, Ctrl+Enter types its text at
//! the cursor; Esc puts the cursor back where it was. "Find Function" goes to the outline's item with
//! the word at the cursor.

use super::colorer_menu::{Action, ColorerMenu};
use super::dialog::Dialog;
use super::editing::EditorAction;
use super::plugins::PluginMenu;
use super::{App, Overlay, Screen};
use crate::editor::syntax::highlight::Wait;
use crate::editor::syntax::outline::{OutlineItem, tree_level};
use crate::editor::view::edit_view::{EditView, Place, Step};
use crate::editor::view::{LineSource, Missing};
use crate::plugin_host::windows::{MenuLine, MenuRequest};

/// FarColorer's `FILTER_SIZE`.
const FILTER_SIZE: usize = 40;
/// The longest item text (FarColorer's 110).
const LABEL: usize = 110;
/// FarColorer's break keys, in its order (`breakKeys`): BS, Enter, `;`, `-`, Tab, Ctrl+Up, Ctrl+Down,
/// Ctrl+Left, Ctrl+Right, Ctrl+Enter, `:`, `_`, `~`, then the digits, the letters and Space.
const KEYS: [&str; 13] = [
    "BS",
    "Enter",
    ";",
    "-",
    "Tab",
    "Ctrl+Up",
    "Ctrl+Down",
    "Ctrl+Left",
    "Ctrl+Right",
    "Ctrl+Enter",
    ":",
    "_",
    "~",
];

/// The list's state between its showings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineMenu {
    items: Vec<OutlineItem>,
    filter: String,
    /// FarColorer's `visibleLevel`.
    level: usize,
    /// The key that closed it last (FarColorer's `code`; none at first and after BS).
    typed: bool,
    /// The editor's cursor's line number (the item at or above it is the menu's cursor).
    cursor_line: u64,
    /// Each line shown: its item.
    shown: Vec<usize>,
    /// Where the editor was, put back by Esc.
    back: Place,
}

/// All the break keys' names: [`KEYS`], the digits, the letters, Space.
fn break_keys() -> Vec<String> {
    let mut keys: Vec<String> = KEYS.iter().map(|key| (*key).to_string()).collect();
    keys.extend(('0'..='9').chain('a'..='z').map(String::from));
    keys.push("Space".to_string());
    keys
}

/// `indexOfIgnoreCase`.
fn find_in(text: &str, part: &str) -> Option<usize> {
    let (text, part) = (text.to_lowercase(), part.to_lowercase());
    text.find(&part).map(|at| text[..at].chars().count())
}

impl OutlineMenu {
    /// The lines shown for the filter and the levels (FarColorer's loop), the line of the cursor's
    /// item and the deepest level seen.
    fn lines(&mut self) -> (Vec<String>, usize, usize) {
        let (mut texts, mut selected, mut deepest) = (Vec::new(), 0, 0);
        let mut stack = Vec::new();
        self.shown.clear();
        for (index, item) in self.items.iter().enumerate() {
            if !self.filter.is_empty() && find_in(&item.token, &self.filter).is_none() {
                continue;
            }
            let tree = tree_level(&mut stack, item.level);
            deepest = deepest.max(tree);
            if tree > self.level {
                continue;
            }
            let class = item
                .region
                .split(':')
                .nth(1)
                .and_then(|name| name.chars().next());
            let class = class.map_or(' ', |ch| ch.to_lowercase().next().unwrap_or(ch));
            let token: String = item.token.chars().take(LABEL).collect();
            let indent = "  ".repeat(tree);
            texts.push(format!("{:4} {indent}{class} {token}", item.line + 1));
            if self.cursor_line >= item.line {
                selected = texts.len() - 1;
            }
            self.shown.push(index);
        }
        (texts, selected, deepest)
    }

    /// FarColorer's "same function prefix": the filter as long as every line shown has it.
    fn completed(&self, texts: &[String]) -> String {
        let mut auto = self.filter.clone();
        let Some(first) = texts.first().filter(|_| self.typed && texts.len() > 1) else {
            return auto;
        };
        while auto.chars().count() < FILTER_SIZE {
            let Some(at) = find_in(first, &auto) else {
                break;
            };
            let longer: String = first
                .chars()
                .skip(at)
                .take(auto.chars().count() + 1)
                .collect();
            if longer.chars().count() <= auto.chars().count()
                || !texts.iter().all(|text| find_in(text, &longer).is_some())
            {
                break;
            }
            auto = longer;
        }
        auto
    }
}

/// Where an item is gone to: its line, its position, the screen with it half way down.
fn item_place(view: &EditView, item: &OutlineItem, height: usize) -> Result<Place, Missing> {
    let mut top = item.start;
    for _ in 0..height / 2 {
        match view.line_above(top)? {
            Some(above) => top = above,
            None => break,
        }
    }
    Ok(Place {
        line: Some(item.start),
        pos: Some(item.pos),
        tab_pos: None,
        top: Some(top),
        left: None,
        overtype: None,
    })
}

/// The word at `pos` of `line` as FarColorer's `locateFunction` takes it (its loop's bounds too).
fn word_at(line: &[char], pos: usize) -> Option<String> {
    let word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let &ch = line.get(pos)?;
    if !(ch.is_alphanumeric() || ch != '_') {
        return None;
    }
    let (mut start, mut end) = (pos, pos);
    while word(line[end]) && end + 1 < line.len() {
        end += 1;
    }
    while word(line[start]) && start > 0 {
        start -= 1;
    }
    Some(
        line.get(start + 1..end)
            .unwrap_or_default()
            .iter()
            .collect(),
    )
}

impl App {
    /// "Outliner list" or "Errors list" (`listFunctions`, `listErrors`), or "Find Function".
    pub(super) fn colorer_outline(&mut self, action: Action) {
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        let height = usize::from(editor.text.room.get().1);
        let read = editor.text.colorer(|syntax, view| {
            let Some(outlines) = syntax.outlines(view)? else {
                return Ok::<_, Wait>(None);
            };
            let (line, pos) = view.cursor_char()?;
            let number = view.number_of(line)?;
            if action != Action::FindFunction {
                return Ok(Some(Err((outlines, number, view.screen_place()))));
            }
            let text = view.line_at(line)?.chars;
            let found = word_at(&text, pos).and_then(|word| {
                let matching = |item: &&OutlineItem| find_in(&item.token, &word).is_some();
                let items = outlines.functions.iter().filter(matching);
                let (here, other): (Vec<_>, Vec<_>) = items.partition(|item| item.line == number);
                other.last().or(here.last()).map(|item| (*item).clone())
            });
            let place = found
                .map(|item| item_place(view, &item, height))
                .transpose()?;
            Ok(Some(Ok(place)))
        });
        match read {
            None => editor.pending = Some(EditorAction::Colorer(action)),
            Some(None) => {}
            Some(Some(Ok(Some(place)))) => editor.text.press(Step::Place(place)),
            Some(Some(Ok(None))) => {
                self.show_dialog(Dialog::message("Nothing found", Vec::new()));
            }
            Some(Some(Err((outlines, cursor_line, back)))) => {
                let items = match action {
                    Action::Errors => outlines.errors,
                    _ => outlines.functions,
                };
                if items.is_empty() {
                    return;
                }
                let menu = OutlineMenu {
                    items,
                    filter: String::new(),
                    level: 100,
                    typed: false,
                    cursor_line,
                    shown: Vec::new(),
                    back,
                };
                self.show_outline(menu);
            }
        }
    }

    /// The list shown again for its state (an empty filter's last character taken back).
    fn show_outline(&mut self, mut menu: OutlineMenu) {
        let (texts, selected) = loop {
            let (texts, selected, deepest) = menu.lines();
            if texts.is_empty() && menu.filter.pop().is_some() {
                continue;
            }
            let _ = deepest;
            break (texts, selected);
        };
        let auto = menu.completed(&texts);
        let mut caption = menu.filter.clone();
        if auto.chars().count() > menu.filter.chars().count() {
            caption.push('?');
            caption.extend(auto.chars().skip(menu.filter.chars().count()));
        }
        let request = MenuRequest {
            title: format!("Outliner [{caption}]"),
            bottom: "[Ctrl]Enter, Tab, Ctrl-Cursor, [0-9a-z;:-_]".to_string(),
            lines: texts.iter().map(|text| plain(text)).collect(),
            selected,
            break_keys: break_keys(),
            wrap: true,
            show_ampersand: true,
            max_height: Some(usize::from(self.height).saturating_sub(4)),
            ..MenuRequest::default()
        };
        let mut shown = PluginMenu::new(request);
        shown.colorer = Some(ColorerMenu::Outline(Box::new(menu)));
        self.overlay = Some(Overlay::PluginMenu(Box::new(shown)));
    }

    /// A key of the list (`showOutliner`'s `code`): `key` the break key's index, none for Enter.
    pub(super) fn outline_key(
        &mut self,
        mut menu: OutlineMenu,
        item: Option<usize>,
        key: Option<usize>,
    ) {
        let chosen = item.and_then(|line| menu.shown.get(line)).copied();
        let height = self.editor_height();
        menu.typed = key != Some(0);
        match key {
            None | Some(1) => self.outline_go(&menu, chosen, height, None),
            Some(0) => {
                menu.filter.pop();
                self.show_outline(menu);
            }
            Some(4) => {
                let (texts, _, _) = menu.lines();
                menu.filter = menu.completed(&texts);
                self.show_outline(menu);
            }
            Some(code @ (5 | 6)) => self.outline_step(menu, item, code == 5, height),
            Some(7) => {
                let (_, _, deepest) = menu.lines();
                menu.level = match menu.level > deepest {
                    true => deepest.saturating_sub(1),
                    false => menu.level.saturating_sub(1),
                };
                self.show_outline(menu);
            }
            Some(8) => {
                menu.level += 1;
                self.show_outline(menu);
            }
            Some(9) => {
                if let Some(index) = chosen {
                    let text = menu.items[index].token.clone();
                    self.outline_go(&menu, None, height, Some(text));
                }
            }
            Some(code) => {
                let names = break_keys();
                let typed = match names.get(code).map(String::as_str) {
                    Some("Space") => Some(' '),
                    Some(name) => name.chars().next(),
                    None => None,
                };
                if let Some(ch) = typed.filter(|_| menu.filter.chars().count() < FILTER_SIZE) {
                    menu.filter.push(ch);
                }
                self.show_outline(menu);
            }
        }
    }

    /// Esc: the cursor put back where it was.
    pub(super) fn outline_closed(&mut self, menu: &OutlineMenu) {
        if let Screen::Editor(editor) = &mut self.screen {
            editor.text.press(Step::Place(menu.back));
        }
    }

    fn editor_height(&self) -> usize {
        match &self.screen {
            Screen::Editor(editor) => usize::from(editor.text.room.get().1),
            _ => 0,
        }
    }

    /// Enter on an item (or Ctrl+Enter's `text` typed at the cursor): the list closes.
    fn outline_go(
        &mut self,
        menu: &OutlineMenu,
        chosen: Option<usize>,
        height: usize,
        text: Option<String>,
    ) {
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        // Ctrl+Enter: typed where the editor is (FarColorer's `ECTL_INSERTTEXT`).
        if let Some(text) = text {
            editor.text.press(Step::PluginText(text));
            return;
        }
        let Some(item) = chosen.map(|index| &menu.items[index]) else {
            return;
        };
        let place = editor
            .text
            .colorer(|_, view| item_place(view, item, height));
        let place = place.unwrap_or(Place {
            line: Some(item.start),
            pos: Some(item.pos),
            tab_pos: None,
            top: None,
            left: None,
            overtype: None,
        });
        editor.text.press(Step::Place(place));
    }

    /// Ctrl+Up / Ctrl+Down: the editor at the item before or after (round the ends), the list stays.
    fn outline_step(
        &mut self,
        mut menu: OutlineMenu,
        line: Option<usize>,
        up: bool,
        height: usize,
    ) {
        let count = menu.shown.len();
        if let (Some(line), true) = (line, count > 0) {
            let next = match up {
                true => (line + count - 1) % count,
                false => (line + 1) % count,
            };
            let item = menu.items[menu.shown[next]].clone();
            menu.cursor_line = item.line;
            if let Screen::Editor(editor) = &mut self.screen
                && let Some(place) = editor
                    .text
                    .colorer(|_, view| item_place(view, &item, height))
            {
                editor.text.press(Step::Place(place));
            }
        }
        self.show_outline(menu);
    }
}

fn plain(text: &str) -> MenuLine {
    MenuLine {
        text: text.to_string(),
        ..MenuLine::default()
    }
}

#[cfg(test)]
#[path = "colorer_outline_tests.rs"]
mod tests;
