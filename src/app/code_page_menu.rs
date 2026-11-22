//! The code page menu (Shift+F8 in the viewer; FAR's `codepages::SelectCodePage` with the viewer's
//! flags, `far/codepage_selection.cpp`): "Automatic detection", under "System" the ANSI and OEM code
//! pages, under "Unicode" UTF-8 and UTF-16, under "Favorites" the user's favourites, under "Other" the
//! rest the system has that the viewer can show (a character of at most two bytes), each by its number
//! (FAR's `FormatCodePageString`: `1251 │ ANSI - Cyrillic`, a `*` for a name of the user's). Ins makes
//! a code page of "Other" a favourite, Del takes one back, Ctrl+H hides "Other" (FAR's `CPMenuMode`,
//! the title then ends with ` *`), F4 renames a code page that is not a standard one.

use crate::app::form::{Control, EditHistory, Form, Item};
use crate::app::line_input::LineInput;
use crate::jobs::find::text::{UTF8, UTF16_BE, UTF16_LE};
use crate::platform::locale::{ansi_code_page, installed_code_pages, oem_code_page};
use crate::plugin_host::windows::{MenuLine, MenuRequest};
use crate::settings::CodePageSettings;

/// FAR's `MGetCodePageTitle`.
pub(super) const TITLE: &str = "Code pages";
/// The menu's break keys (FAR's `KeysToLocalizedText(KEY_CTRLH, KEY_INS, KEY_DEL, KEY_F4)`).
pub(super) const KEYS: [&str; 4] = ["Ctrl+H", "Ins", "Del", "F4"];
pub(super) const CTRL_H: usize = 0;
pub(super) const INS: usize = 1;
pub(super) const DEL: usize = 2;
pub(super) const F4: usize = 3;
/// The rename dialog's edit and buttons (FAR's `EditCodePagesDialogControls`).
pub(super) const NAME_EDIT: usize = 0;
pub(super) const NAME_OK: usize = 2;
pub(super) const NAME_RESET: usize = 4;

/// Where a line of the menu stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// "Automatic detection" (FAR's `CP_DEFAULT`).
    Auto,
    Standard,
    Favorite,
    /// "Other" (FAR's normal code pages).
    Normal,
    Separator,
}

/// A line of the menu: its place and its code page (none for the automatic and the separators).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Entry {
    pub kind: Kind,
    pub page: Option<u32>,
}

/// FAR's `GetSystemCodepageName`: "ANSI - Cyrillic", or the system's name when it starts with `kind`.
pub(in crate::app) fn system_name(id: u32, kind: &str) -> String {
    match installed_code_pages().iter().find(|page| page.id == id) {
        None => kind.to_string(),
        Some(page) if page.name.starts_with(kind) => page.name.clone(),
        Some(page) => format!("{kind} - {}", page.name),
    }
}

/// FAR's `FormatCodePageString`: the number in five columns, `│`, a space or `*` (a user's name), the
/// name.
pub(super) fn page_text(page: u32, name: &str, custom: bool) -> String {
    let mark = if custom { '*' } else { ' ' };
    format!("{page:<5}│{mark}{name}")
}

/// A code page's name: the user's when it is given and differs (FAR's `GetCodePageCustomName`).
pub(super) fn name_of(settings: &CodePageSettings, page: u32, system: &str) -> (String, bool) {
    match settings.names.get(&page.to_string()) {
        Some(name) if name != system => (name.clone(), true),
        _ => (system.to_string(), false),
    }
}

/// The menu's lines and their entries.
pub(super) fn lines(settings: &CodePageSettings) -> (Vec<MenuLine>, Vec<Entry>) {
    let mut out: Vec<(MenuLine, Entry)> = Vec::new();
    let separator = |label: &str| {
        let line = MenuLine {
            text: label.to_string(),
            separator: true,
            ..MenuLine::default()
        };
        let entry = Entry {
            kind: Kind::Separator,
            page: None,
        };
        (line, entry)
    };
    let line = |text: String, kind: Kind, page: Option<u32>| {
        let line = MenuLine {
            text,
            ..MenuLine::default()
        };
        (line, Entry { kind, page })
    };
    out.push(line("Automatic detection".to_string(), Kind::Auto, None));
    out.push(separator("System"));
    let (ansi, oem) = (ansi_code_page(), oem_code_page());
    for (page, kind) in [(ansi, "ANSI"), (oem, "OEM")] {
        let text = page_text(page, &system_name(page, kind), false);
        out.push(line(text, Kind::Standard, Some(page)));
    }
    out.push(separator("Unicode"));
    for (page, name) in [
        (UTF8, "UTF-8"),
        (UTF16_LE, "UTF-16 (Little endian)"),
        (UTF16_BE, "UTF-16 (Big endian)"),
    ] {
        out.push(line(
            page_text(page, name, false),
            Kind::Standard,
            Some(page),
        ));
    }
    let standard = [ansi, oem, UTF8, UTF16_LE, UTF16_BE];
    let shown = (installed_code_pages().iter())
        .filter(|info| !standard.contains(&info.id))
        .filter(|info| (1..=2).contains(&info.max_char_size));
    let (mut favorites, mut normal) = (Vec::new(), Vec::new());
    for info in shown {
        let (name, custom) = name_of(settings, info.id, &info.name);
        let text = page_text(info.id, &name, custom);
        match settings.favorites.contains(&info.id) {
            true => favorites.push(line(text, Kind::Favorite, Some(info.id))),
            false if !settings.menu_mode => normal.push(line(text, Kind::Normal, Some(info.id))),
            false => {}
        }
    }
    if !favorites.is_empty() {
        out.push(separator("Favorites"));
        out.extend(favorites);
    }
    if !normal.is_empty() {
        out.push(separator("Other"));
        out.extend(normal);
    }
    out.into_iter().unzip()
}

/// The menu (FAR's `VMENU_WRAPMODE | VMENU_AUTOHIGHLIGHT`, at most the screen's rows less four), the
/// cursor on `selected`.
pub(super) fn request(
    settings: &CodePageSettings,
    selected: Option<u32>,
    rows: u16,
) -> MenuRequest {
    let (lines, entries) = lines(settings);
    let at = (entries.iter()).position(|entry| entry.page.is_some() && entry.page == selected);
    let title = match settings.menu_mode {
        true => format!("{TITLE} *"),
        false => TITLE.to_string(),
    };
    // FAR's bottom title without Ins while "Other" is hidden.
    let keys: Vec<&str> = (KEYS.iter().copied())
        .filter(|key| !(settings.menu_mode && *key == "Ins"))
        .collect();
    MenuRequest {
        title,
        bottom: keys.join(" "),
        lines,
        selected: at.unwrap_or(0),
        break_keys: KEYS.iter().map(|key| (*key).to_string()).collect(),
        auto_hotkeys: true,
        wrap: true,
        max_height: Some(usize::from(rows).saturating_sub(4)),
        ..MenuRequest::default()
    }
}

/// Ins on `page` of "Other": a favourite (none while "Other" is hidden); the cursor's code page after
/// it - the next of "Other", else the one before, else the page among the favourites.
pub(super) fn add_favorite(settings: &mut CodePageSettings, page: u32) -> Option<u32> {
    if settings.menu_mode {
        return None;
    }
    let (_, entries) = lines(settings);
    let normal: Vec<u32> = (entries.iter())
        .filter(|entry| entry.kind == Kind::Normal)
        .filter_map(|entry| entry.page)
        .collect();
    let at = normal.iter().position(|it| *it == page)?;
    settings.favorites.push(page);
    settings.favorites.sort_unstable();
    let next = normal
        .get(at + 1)
        .or(at.checked_sub(1).and_then(|before| normal.get(before)));
    Some(next.copied().unwrap_or(page))
}

/// Del on `page` among the favourites: back to "Other"; the cursor's code page after it - the next
/// favourite, else the one before, else the page itself.
pub(super) fn remove_favorite(settings: &mut CodePageSettings, page: u32) -> Option<u32> {
    let at = settings.favorites.iter().position(|it| *it == page)?;
    let (_, entries) = lines(settings);
    let shown: Vec<u32> = (entries.iter())
        .filter(|entry| entry.kind == Kind::Favorite)
        .filter_map(|entry| entry.page)
        .collect();
    settings.favorites.remove(at);
    let place = shown.iter().position(|it| *it == page).unwrap_or(0);
    let next = shown
        .get(place + 1)
        .or(place.checked_sub(1).and_then(|before| shown.get(before)));
    Some(next.copied().unwrap_or(page))
}

/// FAR's `EditCodePageName` dialog: "Rename the code page" with the name shown (without the number),
/// its history `CodePageName`, OK, Cancel and Reset.
pub(super) fn rename_form(name: &str) -> Form {
    let mut edit = Item::new(5, 2, Control::Edit(LineInput::with_text(name)));
    edit.width = 44;
    let button = |label: &str, default: bool| {
        let label = label.to_string();
        Item::new(0, 4, Control::Button { label, default })
    };
    let items = vec![
        edit.with_history(EditHistory::new("CodePageName")),
        Item::new(0, 3, Control::Separator),
        button("OK", true),
        button("Cancel", false),
        button("&Reset", false),
    ];
    Form::new("Rename the code page", 54, 7, items)
}

/// The name a line of the menu shows: past `│` and its mark.
pub(super) fn shown_name(text: &str) -> &str {
    text.split_once('│')
        .map_or("", |(_, rest)| rest.get(1..).unwrap_or_default())
}

/// OK (a name given; an empty one taken back) or Reset (taken back) of the rename dialog.
pub(super) fn renamed(settings: &mut CodePageSettings, page: u32, name: Option<&str>) {
    match name.filter(|name| !name.is_empty()) {
        Some(name) => {
            settings.names.insert(page.to_string(), name.to_string());
        }
        None => {
            settings.names.remove(&page.to_string());
        }
    }
}

impl super::App {
    /// Shift+F8 in the viewer: the menu, the cursor on `selected`.
    pub(super) fn show_code_page_menu(&mut self, selected: Option<u32>) {
        let request = request(&self.code_pages, selected, self.height);
        let mut menu = super::PluginMenu::new(request);
        menu.code_pages = true;
        self.overlay = Some(super::Overlay::PluginMenu(Box::new(menu)));
    }

    /// A key in the code page menu (FAR's `SelectCodePage`'s handler).
    pub(super) fn code_page_menu_key(
        &mut self,
        mut menu: Box<super::PluginMenu>,
        key: ratatui::crossterm::event::KeyEvent,
    ) {
        use super::plugins::MenuOutcome;
        let (item, pressed) = match menu.handle_key(key) {
            MenuOutcome::Stay => {
                self.overlay = Some(super::Overlay::PluginMenu(menu));
                return;
            }
            MenuOutcome::Esc => return,
            MenuOutcome::Chosen { item, key } => (item, key),
        };
        let (lines, entries) = lines(&self.code_pages);
        let Some(entry) = item.and_then(|item| entries.get(item)).copied() else {
            return;
        };
        let shown = match &self.screen {
            super::Screen::Viewer(viewer) => viewer.text().page_number(),
            super::Screen::Editor(editor) => editor.text.page_number(),
            super::Screen::Panels => return,
        };
        let again = match (pressed, entry.kind, entry.page) {
            (None, Kind::Auto, _) => return self.page_chosen(None),
            (None, _, Some(page)) => return self.page_chosen(Some(page)),
            (Some(CTRL_H), kind, page) => {
                self.code_pages.menu_mode = !self.code_pages.menu_mode;
                // FAR's `FillCodePagesVMenu`: the cursor stays unless its line goes.
                if kind == Kind::Normal { shown } else { page }
            }
            (Some(INS), Kind::Normal, Some(page)) => add_favorite(&mut self.code_pages, page),
            (Some(DEL), Kind::Favorite, Some(page)) => remove_favorite(&mut self.code_pages, page),
            (Some(F4), Kind::Favorite | Kind::Normal, Some(page)) => {
                let name = item
                    .and_then(|item| lines.get(item))
                    .map_or("", |line| shown_name(&line.text));
                self.set_menu_behind(page);
                let form = rename_form(name);
                let action = super::dialog::FormAction::CodePageName(page);
                return self.show_dialog(super::Dialog::Form { form, action });
            }
            (_, _, page) => page,
        };
        self.show_code_page_menu(again.or(entry.page));
    }

    /// The rename dialog closed: OK names the code page (an empty name takes it back), Reset takes it
    /// back; the menu comes back after it ([`resume_code_page_menu`](Self::resume_code_page_menu)).
    pub(super) fn finish_code_page_name(&mut self, form: &Form, button: Option<usize>, page: u32) {
        match button {
            Some(NAME_OK) => renamed(&mut self.code_pages, page, Some(form.edit_text(NAME_EDIT))),
            Some(NAME_RESET) => renamed(&mut self.code_pages, page, None),
            _ => {}
        }
    }

    /// The code page menu back in front once the rename dialog over it has gone.
    pub(super) fn resume_code_page_menu(&mut self) -> bool {
        if self.overlay.is_some() || self.waiting.is_some() {
            return false;
        }
        let behind = match &mut self.screen {
            super::Screen::Viewer(viewer) => viewer.menu_behind.take(),
            super::Screen::Editor(editor) => editor.menu_behind.take(),
            super::Screen::Panels => None,
        };
        let Some(page) = behind else {
            return false;
        };
        self.show_code_page_menu(Some(page));
        true
    }

    /// The menu's choice for the viewer or the editor in front (none: "Automatic detection").
    fn page_chosen(&mut self, page: Option<u32>) {
        match &mut self.screen {
            super::Screen::Viewer(viewer) => match page {
                Some(page) => viewer.set_code_page(page),
                None => viewer.detect_again(),
            },
            super::Screen::Editor(editor) => {
                if !editor.text.set_page(page) {
                    self.editor_page_refused();
                }
            }
            super::Screen::Panels => {}
        }
    }

    /// The menu stands behind the rename dialog: the viewer's or the editor's to bring back.
    fn set_menu_behind(&mut self, page: u32) {
        match &mut self.screen {
            super::Screen::Viewer(viewer) => viewer.menu_behind = Some(page),
            super::Screen::Editor(editor) => editor.menu_behind = Some(page),
            super::Screen::Panels => {}
        }
    }

    /// An edited document keeps its code page until it is saved (its edits are in it).
    pub(super) fn editor_page_refused(&mut self) {
        let lines = vec![
            "The file has been modified.".to_string(),
            "Save it before another code page.".to_string(),
        ];
        self.show_dialog(super::Dialog::message("Editor", lines));
    }
}

#[cfg(test)]
#[path = "code_page_menu_tests.rs"]
mod tests;
