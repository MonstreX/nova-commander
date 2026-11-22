//! Shift+F2's "Save file as" (R7 review D5; FAR's `dlgSaveFileAs`, `FileEditor::SaveAction`): the name
//! (history `NewEdit`), the code page (the code page menu's lines without "Automatic detection"), "Add
//! signature (BOM)" (open for the Unicode pages only: FAR's `IsUtfCodePage`), "Change line breaks to"
//! (do not change, CR LF, LF, CR); "Save". Another code page, signature or line breaks write the text
//! converted (`editor_convert`), the editor then reading the saved file in its code page.

use std::path::{Path, PathBuf};

use super::dialog::{Dialog, FormAction};
use super::editor_convert::Convert;
use super::form::{Control, EditHistory, Form, Item, ListLine};
use super::line_input::LineInput;
use super::{App, Screen};
use crate::settings::CodePageSettings;

/// The name's edit line.
pub(super) const NAME: usize = 1;
/// The code page's list.
pub(super) const CODE_PAGE: usize = 4;
/// "Add signature (BOM)".
pub(super) const SIGNATURE: usize = 5;
/// The first line break's radio ("Do not change").
pub(super) const KEEP_BREAKS: usize = 8;
/// "Save".
pub(super) const SAVE: usize = 13;

/// The line breaks of the radios after "Do not change".
const BREAKS: [&str; 3] = ["\r\n", "\n", "\r"];

/// FAR's `IsUtfCodePage`.
fn is_utf(page: Option<u32>) -> bool {
    matches!(page, Some(65001 | 1200 | 1201))
}

/// The list's lines and the code page of each (none: a section's line).
fn pages(settings: &CodePageSettings) -> (Vec<ListLine>, Vec<Option<u32>>) {
    use super::code_page_menu::Kind;
    let (menu, entries) = super::code_page_menu::lines(settings);
    let parts = menu.into_iter().zip(entries);
    let parts = parts.filter(|(_, entry)| entry.kind != Kind::Auto);
    parts
        .map(|(line, entry)| match entry.kind {
            Kind::Separator => (ListLine::Section(line.text), None),
            _ => (ListLine::Choice(line.text), entry.page),
        })
        .unzip()
}

/// FAR's dialog: its box 3..72 by 1..15, the name, `page` chosen, the signature as the file has it.
fn form(settings: &CodePageSettings, name: &str, page: Option<u32>, bom: bool) -> Form {
    let mut edit = Item::new(5, 3, Control::Edit(LineInput::with_text(name)));
    edit.width = 66;
    let (lines, chosen_pages) = pages(settings);
    let chosen = chosen_pages.iter().position(|it| *it == page).unwrap_or(0);
    let mut list = Item::new(
        25,
        5,
        Control::DropDown {
            lines,
            chosen,
            open: None,
            caption: None,
            bottom_title: None,
        },
    );
    list.width = 46;
    let mut signature = Item::new(
        5,
        6,
        Control::Check {
            label: "Add &signature (BOM)".to_string(),
            checked: bom,
        },
    );
    signature.disabled = !is_utf(page);
    let radio = |y: u16, label: &str, group: bool| {
        let label = label.to_string();
        let checked = group;
        Item::new(
            5,
            y,
            Control::Radio {
                label,
                checked,
                group,
            },
        )
    };
    let button = |label: &str, default: bool| {
        let label = label.to_string();
        Item::new(0, 14, Control::Button { label, default })
    };
    let items = vec![
        Item::new(5, 2, Control::Text("Save file &as".to_string())),
        edit.with_history(EditHistory::new("NewEdit")),
        Item::new(0, 4, Control::Separator),
        Item::new(5, 5, Control::Text("&Code page:".to_string())),
        list,
        signature,
        Item::new(0, 7, Control::Separator),
        Item::new(5, 8, Control::Text("Change line breaks to:".to_string())),
        radio(9, "Do n&ot change", true),
        radio(10, "&Dos/Windows format (CR LF)", false),
        radio(11, "&Unix format (LF)", false),
        radio(12, "&Mac format (CR)", false),
        Item::new(0, 13, Control::Separator),
        button("Save", true),
        button("Cancel", false),
    ];
    let mut form = Form::new("Editor", 76, 17, items);
    form.focus = NAME;
    form
}

/// After a key: the signature open for a Unicode page only, checked as FAR's `AddUnicodeBOM` (on) when
/// a Unicode page comes after another, unchecked otherwise (FAR's `DN_EDITCHANGE`).
pub(super) fn follow(dialog: &mut Dialog, settings: &CodePageSettings) {
    let Dialog::Form {
        form,
        action: FormAction::EditorSaveAs,
    } = dialog
    else {
        return;
    };
    let (_, chosen_pages) = pages(settings);
    let utf = is_utf(chosen_pages.get(form.chosen(CODE_PAGE)).copied().flatten());
    let was_open = !form.items[SIGNATURE].disabled;
    if utf != was_open
        && let Some(Control::Check { checked, .. }) = form.control_mut(SIGNATURE)
    {
        *checked = utf;
    }
    form.items[SIGNATURE].disabled = !utf;
}

impl App {
    /// Shift+F2 (and F2 of a file not its own yet): FAR's "Save file as".
    pub(super) fn ask_save_as(&mut self) {
        let Screen::Editor(screen) = &self.screen else {
            return;
        };
        let name = screen.path.display().to_string();
        let page = screen.text.page_number();
        let bom = screen.text.view().is_some_and(|view| view.window().2 > 0);
        let form = form(&self.code_pages, &name, page, bom);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::EditorSaveAs,
        });
    }

    /// "Save": the name given saved there, converted when another code page, signature or line breaks
    /// were asked; the editor on it after.
    pub(super) fn finish_save_as(&mut self, form: &Form, button: Option<usize>) {
        let name = form.edit_text(NAME).trim().replace('"', "");
        if button != Some(SAVE) || name.is_empty() {
            return;
        }
        let (_, chosen_pages) = pages(&self.code_pages);
        let chosen = chosen_pages.get(form.chosen(CODE_PAGE)).copied().flatten();
        let Some((source, path)) = self.save_as_paths(&name) else {
            return;
        };
        let Screen::Editor(screen) = &self.screen else {
            return;
        };
        let from = screen.text.page_number().unwrap_or(65001);
        let floor = screen.text.view().map_or(0, |view| view.window().2);
        let page = chosen.unwrap_or(from);
        let bom = form.is_checked(SIGNATURE) && is_utf(Some(page));
        let breaks = (0..BREAKS.len()).find(|at| form.is_checked(KEEP_BREAKS + 1 + at));
        let eol = breaks.map(|at| BREAKS[at]);
        let same = page == from && bom == (floor > 0) && eol.is_none();
        let convert = (!same).then_some(Convert {
            from,
            floor,
            page,
            bom,
            eol,
        });
        self.save_editor_as((Some(source), Some(path)), convert);
    }

    /// The editor's file and the one `name` names (its folder's unless absolute).
    fn save_as_paths(&self, name: &str) -> Option<(PathBuf, PathBuf)> {
        let Screen::Editor(screen) = &self.screen else {
            return None;
        };
        let base = screen.path.parent().map(Path::to_path_buf);
        let path = base.map_or_else(|| PathBuf::from(name), |base| base.join(name));
        Some((screen.path.clone(), path))
    }

    /// "Save" with the name alone, nothing converted (the tests').
    #[cfg(test)]
    pub(super) fn save_as_named(&mut self, name: &str) {
        if let Some((source, path)) = self.save_as_paths(name.trim()) {
            self.save_editor_as((Some(source), Some(path)), None);
        }
    }
}

#[cfg(test)]
#[path = "editor_save_as_tests.rs"]
mod tests;
