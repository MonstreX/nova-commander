//! Shift+F4's "Editor" (R7 review D1; FAR's `dlgOpenEditor` and `FileList::ProcessKey`'s `KEY_SHIFTF4`):
//! "Open/create file" with its history `NewEdit` (the last name given), "Code page" ("Default",
//! "Automatic detection", then the code page menu's); a name is the active panel's folder's unless it
//! is absolute, a file not there is opened new and made by its first save (its folder too, after FAR's
//! question), an empty name a new file named by the time (FAR's `MNewFileName`). On a plugin's panel a
//! new file is edited as a copy that goes to the panel once saved, as F4's. The editor's Shift+F4 asks
//! the same (FAR hands the key to the active panel), not in a modal editor.

use std::path::{Path, PathBuf};

use super::dialog::{ConfirmAction, Dialog, FormAction};
use super::form::{Control, EditHistory, Form, Item, ListLine};
use super::line_input::LineInput;
use super::{App, Screen};
use crate::settings::CodePageSettings;
use crate::settings::history::Kind;

/// The name's edit line.
const NAME: usize = 1;
/// The code page's list.
const CODE_PAGE: usize = 4;
/// OK.
const OK: usize = 6;

/// The code page a line of the list chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    /// FAR's `CP_DEFAULT`: as the editor's settings say.
    Default,
    /// FAR's `CP_REDETECT`: told from the file whatever the settings say.
    Detect,
    /// This code page.
    Number(u32),
}

/// The list's lines and the code page of each (none: a section's line).
fn pages(settings: &CodePageSettings) -> (Vec<ListLine>, Vec<Option<Page>>) {
    let (menu, entries) = super::code_page_menu::lines(settings);
    let mut lines = vec![ListLine::Choice("Default".to_string())];
    let mut chosen = vec![Some(Page::Default)];
    for (line, entry) in menu.into_iter().zip(entries) {
        use super::code_page_menu::Kind as Line;
        let (line, page) = match (entry.kind, entry.page) {
            (Line::Separator, _) => (ListLine::Section(line.text), None),
            (Line::Auto, _) => (ListLine::Choice(line.text), Some(Page::Detect)),
            (_, page) => (ListLine::Choice(line.text), page.map(Page::Number)),
        };
        lines.push(line);
        chosen.push(page);
    }
    (lines, chosen)
}

/// FAR's dialog: its box 3..72 by 1..8, OK the default button.
fn form(settings: &CodePageSettings) -> Form {
    let mut name = Item::new(5, 3, Control::Edit(LineInput::with_text("")));
    name.width = 66;
    let (lines, _) = pages(settings);
    let mut list = Item::new(
        25,
        5,
        Control::DropDown {
            lines,
            chosen: 0,
            open: None,
            caption: None,
            bottom_title: None,
        },
    );
    list.width = 46;
    let button = |label: &str, default: bool| {
        let label = label.to_string();
        Item::new(0, 7, Control::Button { label, default })
    };
    let items = vec![
        Item::new(5, 2, Control::Text("&Open/create file:".to_string())),
        name.with_history(EditHistory::new("NewEdit").use_last()),
        Item::new(0, 4, Control::Separator),
        Item::new(5, 5, Control::Text("&Code page:".to_string())),
        list,
        Item::new(0, 6, Control::Separator),
        button("OK", true),
        button("Cancel", false),
    ];
    let mut form = Form::new("Editor", 76, 10, items);
    form.focus = NAME;
    form
}

/// FAR's `MNewFileName`: `New_File_<date>_<time>.txt` in `folder`.
fn new_file_name(folder: &Path) -> PathBuf {
    let now = std::time::SystemTime::now();
    let millis = now
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.subsec_millis());
    let name = match crate::platform::locale::local_time(now) {
        Some(at) => format!(
            "New_File_{:04}-{:02}-{:02}_{:02}.{:02}.{:02}.{millis:03}.txt",
            at.year, at.month, at.day, at.hour, at.minute, at.second
        ),
        None => "New_File.txt".to_string(),
    };
    folder.join(name)
}

/// A name typed in quotes, without them (FAR's `unquote`).
fn unquoted(text: &str) -> String {
    text.trim().replace('"', "")
}

impl App {
    /// Shift+F4 on the panels, or in an editor that may give its place to another window.
    pub(super) fn ask_edit_new(&mut self) {
        if let Screen::Editor(editor) = &self.screen
            && (self.user_menu.reread.is_some() || self.edits_extracted(&editor.path))
        {
            return;
        }
        let form = form(&self.code_pages);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::EditNew,
        });
    }

    /// The dialog closed: OK opens the file.
    pub(super) fn finish_edit_new(&mut self, form: &Form, button: Option<usize>) {
        if button != Some(OK) {
            return;
        }
        let name = unquoted(form.edit_text(NAME));
        let (_, chosen) = pages(&self.code_pages);
        let page = chosen
            .get(form.chosen(CODE_PAGE))
            .copied()
            .flatten()
            .unwrap_or(Page::Default);
        let panel = self.active_panel();
        let absolute = Path::new(&name).is_absolute();
        if !absolute && !name.is_empty() && panel.path.as_local().is_none() {
            if let Err(error) = self.edit_new_copy(&name) {
                self.show_error("Editor", &error);
            }
            return self.apply_page(page);
        }
        let Some(folder) = panel
            .path
            .as_local()
            .map(Path::to_path_buf)
            .or_else(|| absolute.then(PathBuf::new))
        else {
            return;
        };
        let path = match name.is_empty() {
            true => new_file_name(&folder),
            false => folder.join(&name),
        };
        let parent_missing = path.parent().is_some_and(|parent| !parent.exists());
        if !path.exists() && parent_missing {
            return self.show_dialog(Dialog::Confirm {
                title: "Warning".to_string(),
                lines: vec![
                    "The path to the edited file does not exist,".to_string(),
                    "but will be created when the file is saved.".to_string(),
                    "Continue?".to_string(),
                ],
                buttons: vec!["&Yes", "&No"],
                focused: 0,
                danger: true,
                action: ConfirmAction::EditNewPath(path, page_number(page)),
            });
        }
        self.edit_named(&path, page);
    }

    /// FAR's question about the folder answered Yes: the new file opened.
    pub(super) fn edit_new_in_missing_folder(&mut self, path: &Path, page: Option<u32>) {
        let page = page.map_or(Page::Default, Page::Number);
        self.edit_named(path, page);
    }

    /// The file at `path` edited, a new one when it is not there, in the code page chosen.
    fn edit_named(&mut self, path: &Path, page: Page) {
        if path.exists() {
            self.edit_file(path, true);
        } else {
            self.open_new_file(path);
            self.remember_file(path, Kind::Edit);
        }
        if matches!(&self.screen, Screen::Editor(editor) if editor.path == path) {
            self.apply_page(page);
        }
    }

    /// The editor just opened read in the code page chosen (a document not read yet only).
    fn apply_page(&mut self, page: Page) {
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        if editor.text.view().is_some() {
            return;
        }
        match page {
            Page::Default => {}
            Page::Detect => {
                let mut settings = editor.settings.clone();
                settings.auto_detect = true;
                editor.text.apply_settings(&settings);
            }
            Page::Number(page) => editor.text.kept_page = Some(page),
        }
    }
}

/// The question's code page: a number, or none for the settings' (detection asked is the settings').
fn page_number(page: Page) -> Option<u32> {
    match page {
        Page::Number(page) => Some(page),
        Page::Default | Page::Detect => None,
    }
}

#[cfg(test)]
#[path = "edit_new_tests.rs"]
mod tests;
