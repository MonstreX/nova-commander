//! The "Find file" dialog itself (FAR's `FindAskDlg` and `MainDlgProc`): its items at FAR's places,
//! what it is given back, the code page list (`find::code_pages`) with its title following the marks, and
//! Text / Hex: the line shown and the switches of a text, the one turned into the other through the code
//! page chosen (FAR's `ConvertHexString`: ANSI for the automatic and "All").

use ratatui::crossterm::event::{KeyEvent, KeyModifiers};

use super::super::dialog::{Dialog, FormAction};
use super::super::form::{Control, EditHistory, Form, Item, ListLine};
use super::super::line_input::LineInput;
use super::super::masked_input::MaskedInput;
use super::code_pages::{self, CodePageChoice};
use super::{ALL_FILES, Area, FIND_TITLE, FindState};
use crate::jobs::find::text::{decode_bytes, encode_text};

const WIDTH: u16 = 80;
const HEIGHT: u16 = 21;
/// FAR's edit lines from column 5 to `DlgWidth-4-2`.
const EDIT_WIDTH: u16 = WIDTH - 4 - 2 - 5 + 1;
/// The area's list from column 5 to 36.
const AREA_WIDTH: u16 = 32;
/// FAR's `HexMask(23)`: as many bytes as the line shows without scrolling.
const HEX_BYTES: usize = 23;

// The items, as FAR's `FAD_*`.
pub(in crate::app) const MASK: usize = 1;
pub(in crate::app) const TEXT: usize = 4;
pub(in crate::app) const HEX: usize = 5;
pub(in crate::app) const RADIO_TEXT: usize = 6;
pub(in crate::app) const RADIO_HEX: usize = 7;
const CODE_PAGE_TEXT: usize = 8;
pub(in crate::app) const CODE_PAGE: usize = 9;
const CASE: usize = 11;
const WHOLE_WORDS: usize = 12;
pub(in crate::app) const FUZZY: usize = 13;
const NOT_CONTAINING: usize = 14;
pub(in crate::app) const ARCHIVES: usize = 15;
pub(in crate::app) const FOLDERS: usize = 16;
const LINKS: usize = 17;
pub(in crate::app) const STREAMS: usize = 18;
pub(in crate::app) const AREA: usize = 22;
pub(in crate::app) const USE_FILTER: usize = 23;
pub(in crate::app) const FIND: usize = 25;
pub(in crate::app) const DRIVE: usize = 26;
pub(in crate::app) const FILTER: usize = 27;
pub(in crate::app) const ADVANCED: usize = 28;

impl FindState {
    /// FAR's `FindAskDlg`; `root` the line of [`Area::Root`], `pages` the code page menu's favourites
    /// and names.
    pub(super) fn form(&self, root: &str, pages: &crate::settings::CodePageSettings) -> Form {
        let areas = Area::ALL.map(|area| ListLine::Choice(area.label(root)));
        let chosen = Area::ALL.iter().position(|&area| area == self.area());
        let (code_pages, code_page) = code_pages::lines(self.code_page, &self.marked, pages);
        let mut items = vec![
            text(5, 2, "A file &mask or several file masks:"),
            edit(3, &self.mask).with_history(EditHistory::new("Masks").use_last()),
            Item::new(0, 4, Control::Separator),
            text(5, 5, "Containing"),
            edit(6, &self.text).with_history(EditHistory::new("SearchText")),
            Item::new(
                5,
                6,
                Control::Masked(MaskedInput::new(&hex_mask(), &self.hex)),
            ),
            radio(17, "&Text", !self.hex_mode, true),
            radio(27, "&Hex", self.hex_mode, false),
            text(5, 7, "Using code pa&ge:"),
            drop_down(8, code_pages, code_page)
                .wide(EDIT_WIDTH)
                .with_bottom_title("Space Ins"),
            Item::new(0, 9, Control::Separator),
            check(5, 10, "&Case sensitive", self.case_sensitive),
            check(5, 11, "&Whole words", self.whole_words),
            check(5, 12, "Fu&zzy search", self.fuzzy),
            check(5, 13, "&Not containing", self.not_containing),
            check(41, 10, "Search in arch&ives", self.archives),
            check(41, 11, "Search for f&olders", self.folders),
            check(41, 12, "Search in symbolic lin&ks", self.links),
            check(41, 13, "Search in a&lternate streams", self.streams),
            Item::new(0, 14, Control::Separator),
            Item::new(39, 9, Control::VRule(6)),
            text(5, 15, "Select search &area:"),
            drop_down(16, areas.to_vec(), chosen.unwrap_or(0)).wide(AREA_WIDTH),
            check(41, 16, "&Use filter", self.use_filter),
            Item::new(0, 17, Control::Separator),
        ];
        items.extend([
            button("Find", true),
            button("Dri&ve", false),
            button("Filt&er", false).disabled(!self.use_filter),
            button("Advance&d", false),
            button("Cancel", false),
        ]);
        let mut form = Form::new(FIND_TITLE, WIDTH, HEIGHT, items);
        show_hex(&mut form, self.hex_mode);
        form
    }

    /// FAR's dialog for a plugin's panel (`PluginMode`): the drives' areas grey (one of them chosen gives
    /// way to the root), the links' and streams' boxes off and disabled; off a plugin's panel as the
    /// state says.
    /// Nothing changes while the mode stays (what the user checked stays). "Search in archives" is off
    /// and disabled on a plugin's panel without real names (FAR's `OPIF_REALNAMES`), as it was given
    /// elsewhere.
    pub(super) fn set_plugin_mode(&self, form: &mut Form, plugin: bool, real_names: bool) {
        let no_archives = plugin && !real_names;
        if form.items[ARCHIVES].disabled != no_archives {
            if let Some(Control::Check { checked, .. }) = form.control_mut(ARCHIVES) {
                *checked = self.archives && !no_archives;
            }
            form.items[ARCHIVES].disabled = no_archives;
        }
        if form.items[LINKS].disabled == plugin {
            return;
        }
        if let Some(Control::DropDown { lines, chosen, .. }) = form.control_mut(AREA) {
            for (line, area) in lines.iter_mut().zip(Area::ALL) {
                if !matches!(area, Area::AllDrives | Area::LocalDrives) {
                    continue;
                }
                let label = line.label().unwrap_or_default().to_string();
                *line = match plugin {
                    true => ListLine::Grayed(label),
                    false => ListLine::Choice(label),
                };
            }
            if plugin
                && matches!(
                    Area::ALL.get(*chosen),
                    Some(Area::AllDrives | Area::LocalDrives)
                )
            {
                *chosen = Area::ALL
                    .iter()
                    .position(|&area| area == Area::Root)
                    .unwrap_or(0);
            }
        }
        for (index, kept) in [(LINKS, self.links), (STREAMS, self.streams)] {
            if let Some(Control::Check { checked, .. }) = form.control_mut(index) {
                *checked = kept && !plugin;
            }
            form.items[index].disabled = plugin;
        }
    }

    /// What the dialog was given, kept for the next one; the links' and streams' boxes only where they
    /// could be changed (FAR's `if (!PluginMode)`).
    pub(super) fn read(&mut self, form: &Form) {
        self.mask = match form.edit_text(MASK) {
            "" => ALL_FILES.to_string(),
            mask => mask.to_string(),
        };
        self.text = form.edit_text(TEXT).to_string();
        self.hex = form.masked_text(HEX).trim_end().to_string();
        self.hex_mode = form.is_checked(RADIO_HEX);
        self.case_sensitive = form.is_checked(CASE);
        self.whole_words = form.is_checked(WHOLE_WORDS);
        self.fuzzy = form.is_checked(FUZZY);
        self.not_containing = form.is_checked(NOT_CONTAINING);
        // FAR reads it even disabled (off there): the next search starts without it.
        self.archives = form.is_checked(ARCHIVES);
        self.folders = form.is_checked(FOLDERS);
        if !form.items[LINKS].disabled {
            self.links = form.is_checked(LINKS);
            self.streams = form.is_checked(STREAMS);
        }
        self.use_filter = form.is_checked(USE_FILTER);
        self.area = Area::ALL.get(form.chosen(AREA)).copied();
        let lines = code_page_lines(form);
        self.code_page = code_pages::choice_of(lines, form.chosen(CODE_PAGE)).unwrap_or_default();
    }

    /// The code pages marked in the dialog's list.
    pub(super) fn read_marks(&mut self, form: &Form) {
        self.marked = code_pages::marked_of(code_page_lines(form));
    }
}

fn code_page_lines(form: &Form) -> &[ListLine] {
    match form.control(CODE_PAGE) {
        Some(Control::DropDown { lines, .. }) => lines,
        _ => &[],
    }
}

/// The code page chosen in the dialog's list.
fn chosen_page(form: &Form) -> CodePageChoice {
    code_pages::choice_of(code_page_lines(form), form.chosen(CODE_PAGE)).unwrap_or_default()
}

/// After a key: the title of "All" as the marks are (FAR's `SetAllCpTitle`); FAR's `DN_BTNCLICK` of Text
/// and Hex: the other line shown with the same thing in it, and after the radio's hotkey the focus on it
/// (`m_IsTextOrHexHotkeyUsed`).
pub(in crate::app) fn follow(dialog: &mut Dialog, key: KeyEvent) {
    let Dialog::Form {
        form,
        action: FormAction::FindFile,
    } = dialog
    else {
        return;
    };
    if let Some(Control::DropDown { lines, .. }) = form.control_mut(CODE_PAGE) {
        code_pages::retitle(lines);
    }
    // FAR's `SetAutomation(FAD_CHECKBOX_FILTER, FAD_BUTTON_FILTER)`.
    form.items[FILTER].disabled = !form.is_checked(USE_FILTER);
    let hex = form.is_checked(RADIO_HEX);
    if hex != form.items[TEXT].hidden {
        let page = chosen_page(form).conversion_page();
        if hex {
            let bytes = encode_text(page, form.edit_text(TEXT));
            form.set_masked_text(HEX, &hex_text(&bytes, " "));
        } else {
            let bytes = parse_hex(&form.masked_text(HEX));
            form.set_edit_text(TEXT, &decode_bytes(page, &bytes));
        }
        show_hex(form, hex);
    }
    // Also when the mode was that one already (FAR's hotkey flag is let go after any click).
    if key.modifiers.contains(KeyModifiers::ALT) && matches!(form.focus, RADIO_TEXT | RADIO_HEX) {
        form.focus_item(if hex { HEX } else { TEXT });
    }
}

/// The hex line or the text line, and the switches only a text has (FAR's `DM_SHOWITEM`, `DM_ENABLE`).
fn show_hex(form: &mut Form, hex: bool) {
    form.items[TEXT].hidden = hex;
    form.items[HEX].hidden = !hex;
    for index in [CODE_PAGE_TEXT, CODE_PAGE, CASE, WHOLE_WORDS, FUZZY] {
        form.items[index].disabled = hex;
    }
}

/// FAR's `HexMask`: `HH HH ... HH`.
fn hex_mask() -> String {
    vec!["HH"; HEX_BYTES].join(" ")
}

/// FAR's `BlobToHexString`: upper-case pairs with `separator` between them.
pub(in crate::app) fn hex_text(bytes: &[u8], separator: &str) -> String {
    let pairs: Vec<String> = bytes.iter().map(|byte| format!("{byte:02X}")).collect();
    pairs.join(separator)
}

/// FAR's `ExtractHexString` and `HexStringToBlob`: the digits without the spaces, an odd last one
/// taken as `0x` (`12 34 5` is `12 34 05`).
pub(in crate::app) fn parse_hex(text: &str) -> Vec<u8> {
    let mut digits: Vec<u8> = (text.trim_end().chars())
        .filter_map(|ch| ch.to_digit(16))
        .filter_map(|digit| u8::try_from(digit).ok())
        .collect();
    if digits.len() % 2 == 1 {
        digits.insert(digits.len() - 1, 0);
    }
    digits
        .chunks(2)
        .map(|pair| pair[0] << 4 | pair[1])
        .collect()
}

fn text(x: u16, y: u16, label: &str) -> Item {
    Item::new(x, y, Control::Text(label.to_string()))
}

/// An edit line with its text taken whole by the first key (FAR's `DM_EDITUNCHANGEDFLAG`).
fn edit(y: u16, text: &str) -> Item {
    let mut input = LineInput::with_text(text);
    input.select_all();
    Item::new(5, y, Control::Edit(input)).wide(EDIT_WIDTH)
}

fn radio(x: u16, label: &str, checked: bool, group: bool) -> Item {
    let control = Control::Radio {
        label: label.to_string(),
        checked,
        group,
    };
    Item::new(x, 5, control)
}

fn drop_down(y: u16, lines: Vec<ListLine>, chosen: usize) -> Item {
    let control = Control::DropDown {
        lines,
        chosen,
        open: None,
        caption: None,
        bottom_title: None,
    };
    Item::new(5, y, control)
}

fn check(x: u16, y: u16, label: &str, checked: bool) -> Item {
    let control = Control::Check {
        label: label.to_string(),
        checked,
    };
    Item::new(x, y, control)
}

fn button(label: &str, default: bool) -> Item {
    let control = Control::Button {
        label: label.to_string(),
        default,
    };
    Item::new(0, 18, control)
}
