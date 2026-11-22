//! FarColorer's menu in the editor (S2d.7e; `FarEditorSet::showMenu`, `chooseType`, `ChooseTypeMenu`,
//! `colorere.lng`): "Colorer" among the editor's F11 commands opens it. With the highlighting on and a
//! type taken its full list; with no type "List types", "Reload schema library", "Configure"; with the
//! highlighting off only "Configure". "List types" is FarColorer's "Select syntax": "Automatic
//! detection", "Disable", the favourites, then each group's types (`&K name: description` with a
//! hotkey); Ins makes a type a favourite, Del takes one back, F4 asks its hotkey ("Define key").
//! "Update highlighting" parses the document again, "Reload schema library" reads the catalog and the
//! schemes anew, "Configure" is the program's "Editor settings" (FarColorer's own dialog's colour
//! settings are not taken, ADR-0008). The pairs' and the regions' items are `app::colorer_actions`, the
//! outline's `app::colorer_outline`.

use ratatui::crossterm::event::KeyEvent;

use super::dialog::{Dialog, FormAction};
use super::editor_syntax::{self, Choice};
use super::form::{Control, Form, Item};
use super::masked_input::MaskedInput;
use super::plugins::{MenuOutcome, PluginMenu};
use super::{App, Overlay, Screen};
use crate::editor::syntax::highlight::TypeEntry;
use crate::plugin_host::windows::{MenuLine, MenuRequest};
use crate::settings::SyntaxTypeSettings;

/// The editor's F11 item (FarColorer's `mName`, the program's name for it).
pub(super) const ITEM: &str = "Colorer";
/// Its place among F11's commands: no plugin's.
pub(super) const COLORER: usize = usize::MAX;

/// FarColorer's menu actions (`MENU_ACTION`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ListTypes,
    MatchPair,
    SelectBlock,
    SelectPair,
    Outliner,
    Errors,
    SelectRegion,
    RegionInfo,
    FindFunction,
    Update,
    Reload,
    Configure,
}

/// FarColorer's items (`mListTypes`...), none for its separator.
const FULL: [(Option<Action>, &str); 13] = [
    (Some(Action::ListTypes), "&1 List types"),
    (Some(Action::MatchPair), "&2 Match pair"),
    (Some(Action::SelectBlock), "&3 Select block"),
    (Some(Action::SelectPair), "&4 Select pair"),
    (Some(Action::Outliner), "&5 Outliner list"),
    (Some(Action::Errors), "&6 Errors list"),
    (Some(Action::SelectRegion), "&7 Select region"),
    (Some(Action::RegionInfo), "&A Region info"),
    (Some(Action::FindFunction), "&8 Find Function"),
    (None, ""),
    (Some(Action::Update), "&9 Update highlighting"),
    (Some(Action::Reload), "&R Reload schema library"),
    (Some(Action::Configure), "&C Configure"),
];

/// What a line of the type menu stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeLine {
    Auto,
    Off,
    Separator,
    Favorite(String),
    Type(String),
}

/// The menu in front: FarColorer's, its type menu or its outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorerMenu {
    Main(Vec<Option<Action>>),
    Types(Vec<TypeLine>),
    Outline(Box<super::colorer_outline::OutlineMenu>),
}

/// The main menu's lines for the highlighting `enabled` and a type taken (`on`).
fn main_lines(enabled: bool, on: bool) -> Vec<(Option<Action>, &'static str)> {
    match (enabled, on) {
        (true, true) => FULL.to_vec(),
        (true, false) => vec![
            (Some(Action::ListTypes), "&1 List types"),
            (None, ""),
            (Some(Action::Reload), "&R Reload schema library"),
            (Some(Action::Configure), "&C Configure"),
        ],
        (false, _) => vec![(Some(Action::Configure), "&C Configure")],
    }
}

/// `FillTypeMenu` with `ChooseTypeMenu`'s layout: the lines and what each stands for.
pub(super) fn type_lines(
    types: &[TypeEntry],
    settings: &SyntaxTypeSettings,
) -> (Vec<MenuLine>, Vec<TypeLine>) {
    let text = |entry: &TypeEntry| {
        let key = settings
            .hotkeys
            .get(&entry.name)
            .filter(|key| !key.is_empty());
        let mark = key.map_or_else(|| " ".to_string(), |key| format!("&{key}"));
        format!("{mark} {}: {}", entry.name, entry.description)
    };
    let mut out = vec![
        (plain("&A Automatic detection"), TypeLine::Auto),
        (plain("&D Disable"), TypeLine::Off),
        (separator("Favorites"), TypeLine::Separator),
    ];
    let favorite = |entry: &&TypeEntry| settings.favorites.contains(&entry.name);
    for entry in types.iter().filter(favorite) {
        out.push((plain(&text(entry)), TypeLine::Favorite(entry.name.clone())));
    }
    let mut group: Option<&str> = None;
    for entry in types.iter().filter(|entry| !favorite(entry)) {
        if group != Some(entry.group.as_str()) {
            group = Some(entry.group.as_str());
            out.push((separator(&entry.group), TypeLine::Separator));
        }
        out.push((plain(&text(entry)), TypeLine::Type(entry.name.clone())));
    }
    out.into_iter().unzip()
}

fn plain(text: &str) -> MenuLine {
    MenuLine {
        text: text.to_string(),
        ..MenuLine::default()
    }
}

fn separator(text: &str) -> MenuLine {
    MenuLine {
        text: text.to_string(),
        separator: true,
        ..MenuLine::default()
    }
}

/// The line of `name`, or of the choice when it has none.
fn line_of(entries: &[TypeLine], name: Option<&str>) -> usize {
    let named = |line: &TypeLine| match line {
        TypeLine::Favorite(of) | TypeLine::Type(of) => Some(of.as_str()) == name,
        _ => false,
    };
    // FarColorer: no type is "Disable"'s line.
    entries.iter().position(named).unwrap_or(1)
}

/// The line at `at`, else the next to choose (FarColorer's `GetNext` after Del).
fn choosable_from(entries: &[TypeLine], at: usize) -> usize {
    (at..entries.len())
        .find(|&line| entries[line] != TypeLine::Separator)
        .unwrap_or(0)
}

impl App {
    /// "Colorer" chosen in the editor's F11 (FarColorer's `openMenu`).
    pub(super) fn open_colorer_menu(&mut self) {
        let Screen::Editor(editor) = &self.screen else {
            return;
        };
        let syntax = editor.text.syntax();
        let lines = main_lines(!syntax.is_disabled(), syntax.is_on());
        let menu_lines = (lines.iter())
            .map(|(action, text)| MenuLine {
                text: text.to_string(),
                separator: action.is_none(),
                ..MenuLine::default()
            })
            .collect();
        let request = MenuRequest {
            title: ITEM.to_string(),
            lines: menu_lines,
            wrap: true,
            ..MenuRequest::default()
        };
        let mut menu = PluginMenu::new(request);
        menu.colorer = Some(ColorerMenu::Main(lines.iter().map(|(a, _)| *a).collect()));
        self.overlay = Some(Overlay::PluginMenu(Box::new(menu)));
    }

    /// A key in FarColorer's menu or its type menu.
    pub(super) fn colorer_menu_key(&mut self, mut menu: Box<PluginMenu>, key: KeyEvent) {
        let outcome = menu.handle_key(key);
        let Some(kind) = menu.colorer.clone() else {
            return;
        };
        let (item, pressed) = match outcome {
            MenuOutcome::Stay => {
                self.overlay = Some(Overlay::PluginMenu(menu));
                return;
            }
            MenuOutcome::Esc => {
                if let ColorerMenu::Outline(outline) = &kind {
                    self.outline_closed(outline);
                }
                return;
            }
            MenuOutcome::Chosen { item, key } => (item, key),
        };
        match kind {
            ColorerMenu::Outline(outline) => self.outline_key(*outline, item, pressed),
            ColorerMenu::Main(actions) => {
                if let Some(Some(action)) = item.and_then(|item| actions.get(item)) {
                    self.colorer_action(*action);
                }
            }
            ColorerMenu::Types(entries) => {
                let Some(item) = item else {
                    return;
                };
                self.type_menu_choice(&entries, item, pressed);
            }
        }
    }

    /// FarColorer's `execMenuAction`.
    pub(super) fn colorer_action(&mut self, action: Action) {
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        let syntax = editor.text.syntax_mut();
        match action {
            Action::ListTypes => {
                let taken = syntax.taken().map(str::to_string);
                self.show_type_menu(taken.as_deref());
            }
            Action::Update => syntax.restart(),
            // Every viewer's and editor's type chosen again from the schemes read anew (review B13:
            // the others kept the old ones until reopened).
            Action::Reload => {
                editor_syntax::reload();
                self.syntax_again_everywhere();
            }
            Action::Configure => self.ask_options(super::options::OptionsPage::Editor),
            Action::MatchPair
            | Action::SelectBlock
            | Action::SelectPair
            | Action::SelectRegion
            | Action::RegionInfo => self.colorer_parse_action(action),
            Action::Outliner | Action::Errors | Action::FindFunction => {
                self.colorer_outline(action)
            }
        }
    }

    /// FarColorer's "Select syntax", the cursor on the type `name` (none: "Disable").
    pub(super) fn show_type_menu(&mut self, name: Option<&str>) {
        let types = editor_syntax::types();
        let (lines, entries) = type_lines(&types, &self.syntax_types);
        let request = MenuRequest {
            title: "Select syntax".to_string(),
            bottom: format!("Total {} types", types.len()),
            selected: line_of(&entries, name),
            lines,
            break_keys: ["Ins", "Del", "F4"].map(String::from).to_vec(),
            auto_hotkeys: true,
            wrap: true,
            max_height: Some(usize::from(self.height).saturating_sub(4)),
            ..MenuRequest::default()
        };
        let mut menu = PluginMenu::new(request);
        menu.colorer = Some(ColorerMenu::Types(entries));
        self.overlay = Some(Overlay::PluginMenu(Box::new(menu)));
    }

    /// Enter, Ins, Del or F4 on the line `item` of the type menu (FarColorer's `chooseType`).
    fn type_menu_choice(&mut self, entries: &[TypeLine], item: usize, pressed: Option<usize>) {
        let Some(line) = entries.get(item) else {
            return;
        };
        match (pressed, line) {
            (None, TypeLine::Auto) => self.choose_syntax(Choice::Auto),
            (None, TypeLine::Off) => self.choose_syntax(Choice::Off),
            (None, TypeLine::Favorite(name) | TypeLine::Type(name)) => {
                self.choose_syntax(Choice::Named(name.clone()));
            }
            (Some(0), TypeLine::Type(name)) => {
                self.syntax_types.favorites.push(name.clone());
                let name = name.clone();
                self.show_type_menu(Some(&name));
            }
            (Some(1), TypeLine::Favorite(name)) => {
                (self.syntax_types.favorites).retain(|favorite| favorite != name);
                let types = editor_syntax::types();
                let (_, again) = type_lines(&types, &self.syntax_types);
                let at = choosable_from(&again, item);
                let name = match again.get(at) {
                    Some(TypeLine::Favorite(name) | TypeLine::Type(name)) => Some(name.clone()),
                    _ => None,
                };
                self.show_type_menu(name.as_deref());
            }
            (Some(2), TypeLine::Favorite(name) | TypeLine::Type(name)) => {
                self.ask_type_hotkey(name);
            }
            (_, TypeLine::Favorite(name) | TypeLine::Type(name)) => {
                let name = name.clone();
                self.show_type_menu(Some(&name));
            }
            _ => self.show_type_menu(None),
        }
    }

    fn choose_syntax(&mut self, choice: Choice) {
        if let Screen::Editor(editor) = &mut self.screen {
            editor.text.syntax_mut().choose_type(choice);
        }
    }

    /// FarColorer's "Define key" for the type `name`: a letter or a digit.
    fn ask_type_hotkey(&mut self, name: &str) {
        let current = self.syntax_types.hotkeys.get(name).cloned();
        let items = vec![
            Item::new(6, 2, Control::Text("Press the desired key".to_string())),
            Item::new(
                5,
                3,
                Control::Masked(MaskedInput::new("X", &current.unwrap_or_default())),
            )
            .closes(),
        ];
        let form = Form::new("Define key", 34, 6, items);
        let action = FormAction::SyntaxHotkey(name.to_string());
        self.show_dialog(Dialog::Form { form, action });
    }

    /// The hotkey's dialog closed: Enter keeps the key typed (in capitals; none: no hotkey); the type
    /// menu comes back on the type.
    pub(super) fn finish_type_hotkey(&mut self, name: &str, form: &Form, button: Option<usize>) {
        if button.is_some() {
            let typed = form.masked_text(1).trim().to_uppercase();
            let hotkeys = &mut self.syntax_types.hotkeys;
            match typed.chars().next().filter(char::is_ascii_alphanumeric) {
                Some(key) => hotkeys.insert(name.to_string(), key.to_string()),
                None => hotkeys.remove(name),
            };
        }
        self.show_type_menu(Some(name));
    }
}

#[cfg(test)]
#[path = "colorer_menu_tests.rs"]
mod tests;
