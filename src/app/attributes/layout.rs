//! The items of FAR's "Attributes" dialog at `far/setattr.cpp`'s places (`DlgX` 80, `C1` 5, `C2` 32,
//! `TB` 6), and its "More" dialog. With a link everything from the link's line down is one row lower,
//! as FAR moves it; with a drive's device (the drive menu's F4) one more.

use super::{MAIN_FLAGS, Mode, Session};
use crate::app::form::{Control, Form, Item, ListLine, Place};
use crate::app::line_input::LineInput;
use crate::app::masked_input::MaskedInput;
use crate::app::text_cut::truncate_center;
use crate::jobs::attributes::times::{self, TimeFormat};
use crate::platform::files::{LinkFacts, LinkKind};

const WIDTH: u16 = 80;
const HEIGHT: u16 = 21;
const C1: u16 = 5;
const C2: u16 = 32;
const TB: u16 = 6;

#[cfg(test)]
pub const NAME: usize = 1;
#[cfg(test)]
pub const HARD_LINKS: usize = 2;
#[cfg(test)]
pub const LINK_TEXT: usize = 3;
/// The link's target, which a symbolic link's or a junction's "Set" changes.
pub const LINK_EDIT: usize = 4;
#[cfg(test)]
pub const DEVICE_TEXT: usize = 5;
#[cfg(test)]
pub const DEVICE_EDIT: usize = 6;
/// The first of the ten attribute boxes.
pub const FIRST_FLAG: usize = 8;
pub const MORE: usize = 18;
pub const UTC: usize = 22;
/// The label of the first time line; each line is its label, its date and its time.
const FIRST_TIME_LABEL: usize = 23;
pub const ORIGINAL: usize = 35;
pub const CURRENT: usize = 36;
pub const BLANK: usize = 37;
pub const OWNER: usize = 39;
/// "Process subfolders", shown when a folder is among the objects.
pub const SUBFOLDERS: usize = 41;
pub const SET: usize = 43;
pub const SYSTEM: usize = 44;

/// The date field of time row `row` (write, creation, access, change).
pub const fn date(row: usize) -> usize {
    FIRST_TIME_LABEL + 1 + 3 * row
}

pub const fn time(row: usize) -> usize {
    FIRST_TIME_LABEL + 2 + 3 * row
}

/// The time row whose label is item `index`.
pub fn row_of_label(index: usize) -> Option<usize> {
    (0..4).find(|row| index == FIRST_TIME_LABEL + 3 * row)
}

/// The time row a date or time field belongs to.
pub fn row_of(index: usize) -> Option<usize> {
    (0..4).find(|row| index == date(*row) || index == time(*row))
}

/// FAR's labels of the attribute boxes, "More" last (`AttributeMap`).
pub const FLAG_LABELS: [&str; 20] = [
    "&Read only",
    "&Archive",
    "&Hidden",
    "&System",
    "&Compressed",
    "&Encrypted",
    "Not &indexed",
    "&Sparse",
    "&Temporary",
    "&Offline",
    "&Reparse point",
    "&Virtual",
    "&Integrity stream",
    "&No scrub data",
    "&Pinned",
    "&Unpinned",
    "R&ecall on open",
    "Rec&all on data access",
    "Strictly se&quential",
    "Device",
];

const TIME_LABELS: [&str; 4] = ["&write:", "crea&tion:", "&access:", "chan&ge:"];

fn text(x: u16, y: u16, text: &str) -> Item {
    Item::new(x, y, Control::Text(text.to_string()))
}

fn button(y: u16, label: &str, default: bool) -> Item {
    let control = Control::Button {
        label: label.to_string(),
        default,
    };
    Item::new(0, y, control)
}

/// The dialog as FAR first shows it for `session`.
pub fn main_form(session: &Session) -> Form {
    let format = session.format;
    let shift = u16::from(session.link.is_some()) + u16::from(session.device.is_some());
    // FAR's `EnableSubfolders`: two rows more for the box; the vertical line keeps its length.
    let folders =
        (session.targets.iter()).any(|target| target.properties.attributes.is_directory());
    let rule = HEIGHT - 4 - 4 + 1;
    let height = HEIGHT + shift + if folders { 2 } else { 0 };
    let mut items = header(session);
    items.push(Item::new(0, 4 + shift, Control::Separator));
    for (at, label) in FLAG_LABELS[..MAIN_FLAGS].iter().enumerate() {
        let y = 5 + shift + u16::try_from(at).unwrap_or(0);
        let control = Control::Tristate {
            label: (*label).to_string(),
            state: session.flags[at],
            three: session.three[at],
        };
        items.push(Item::new(C1, y, control));
    }
    let mut more = button(16 + shift, "&More", false).placed(Place::At);
    more.x = C1;
    items.push(more);
    items.extend(time_items(session, format, shift));
    items.push(text(C2, 15 + shift, "Owner&:"));
    let owner = Control::Edit(LineInput::with_text(session.owner.as_str()));
    items.push(Item::new(C2, 16 + shift, owner).wide(WIDTH - 6 - C2 + 1));
    items.push(Item::new(0, 17 + shift, Control::Separator).hidden(!folders));
    let subfolders = Control::Check {
        label: "Process sub&folders".to_string(),
        checked: false,
    };
    items.push(
        Item::new(C1, 18 + shift, subfolders)
            .hidden(!folders)
            .disabled(!folders),
    );
    items.push(Item::new(0, height - 4, Control::Separator));
    items.push(button(height - 3, "Set", true));
    // FAR: one object only (`SelCount == 1`).
    let one = session.targets.len() == 1;
    items.push(button(height - 3, "System &properties", false).disabled(!one));
    items.push(button(height - 3, "Cancel", false));
    items.push(Item::new(C2 - 2, 4 + shift, Control::VRule(rule)));
    items.push(Item::new(
        C2 - 2,
        TB + 8 + shift,
        Control::HRule(WIDTH - 4 - (C2 - 2) + 1),
    ));
    let mut form = Form::new("Attributes", WIDTH, height, items);
    form.focus = FIRST_FLAG;
    form
}

/// "Change file attributes for", the name (or the hard links), the link's line.
fn header(session: &Session) -> Vec<Item> {
    let title = "Change file attributes for";
    let centred = |text: &str| (WIDTH - u16::try_from(text.chars().count()).unwrap_or(0)) / 2;
    let name = match session.mode {
        Mode::Multiple => "selected objects".to_string(),
        Mode::File | Mode::Folder => session.shown_name.clone(),
    };
    let (name_x, name) = centred_inside(&name);
    let hard_links = &session.targets[0].properties.names;
    let linked = session.mode == Mode::File && hard_links.len() > 1;
    let lines = hard_links.iter().map(|name| ListLine::Choice(name.clone()));
    let names = Control::DropDown {
        lines: lines.collect(),
        chosen: 0,
        open: None,
        caption: Some(format!("hard links ({})", hard_links.len())),
        bottom_title: None,
    };
    let mut items = vec![
        text(centred(title), 2, title),
        Item::new(name_x, 3, Control::Label(name)).hidden(linked),
        Item::new(C1, 3, names)
            .wide(WIDTH - 6 - C1 + 1)
            .hidden(!linked),
    ];
    items.extend(link_items(session.link.as_ref()));
    items.extend(device_items(session));
    items
}

/// FAR's "Device:" under the link's line: what the kernel calls a drive (the drive menu's F4).
fn device_items(session: &Session) -> [Item; 2] {
    let y = 4 + u16::from(session.link.is_some());
    let device = session.device.as_deref().unwrap_or_default();
    let edit = Control::Edit(LineInput::with_text(device));
    let hidden = session.device.is_none();
    [
        text(C1, y, "Device:").hidden(hidden),
        Item::new(18, y, edit)
            .wide(WIDTH - 6 - 18 + 1)
            .read_only()
            .hidden(hidden),
    ]
}

/// FAR's label of a link it can read (`symlink_type`): a reparse point that is no link of its own kind
/// is a "Symlink" with the tag's mark.
fn link_label(link: &LinkFacts) -> Option<String> {
    if !link.known {
        return None;
    }
    let label = match link.kind {
        _ if link.volume => "&Volume:",
        LinkKind::Junction => "&Junction:",
        LinkKind::Symlink => "S&ymlink:",
        LinkKind::Reparse(_) => {
            let mark = match link.kind.name() {
                Some("APPEXECLINK") => return Some("App Alias:".to_string()),
                Some("DFS") => " DFS",
                Some("NFS") => " NFS",
                Some("LX_SYMLINK") => " LX",
                _ => " (?)",
            };
            return Some(format!("S&ymlink{mark}:"));
        }
    };
    Some(label.to_string())
}

/// FAR's link line: "Junction:", "Symlink:", "Volume:", "App Alias:" or "Symlink LX:" and the target
/// (a symbolic link's or a junction's can be changed); a reparse point it cannot read by its tag's name
/// across the dialog.
fn link_items(link: Option<&LinkFacts>) -> [Item; 2] {
    let edit_width = WIDTH - 6 - 18 + 1;
    let Some(link) = link else {
        return [
            text(C1, 4, "").hidden(true),
            Item::new(18, 4, Control::Edit(LineInput::default())).hidden(true),
        ];
    };
    let target = Control::Edit(LineInput::with_text(link.target.as_str()));
    match link_label(link) {
        Some(label) => {
            let edit = Item::new(18, 4, target).wide(edit_width);
            // FAR's `IsMountPoint || !IsEditableReparsePoint`: read only.
            let edit = if link.editable {
                edit
            } else {
                edit.read_only()
            };
            [text(C1, 4, &label), edit]
        }
        None => {
            let shown = match link.kind.name() {
                Some(name) => format!("Reparse point {name}"),
                None => "Unknown reparse point".to_string(),
            };
            let (x, shown) = centred_inside(&shown);
            [
                Item::new(x, 4, Control::Label(shown)),
                Item::new(18, 4, target).hidden(true),
            ]
        }
    }
}

/// FAR's `DIF_CENTERTEXT` between columns 5 and `DlgX`-6: centred there, a longer text cut in the middle.
fn centred_inside(text: &str) -> (u16, String) {
    let room = WIDTH - 6 - C1 + 1;
    let text = truncate_center(text, usize::from(room));
    let length = u16::try_from(text.chars().count()).unwrap_or(room);
    (C1 + room.saturating_sub(length) / 2, text)
}

/// "Date/Time:", the titles, UTC, the four lines, Original / Current / Blank.
fn time_items(session: &Session, format: TimeFormat, shift: u16) -> Vec<Item> {
    let date_x = WIDTH - 33;
    let time_x = WIDTH - 21;
    let mut items = vec![
        text(C2, TB + shift, "Date/Time:"),
        text(date_x, TB + shift, &format.date_title()),
        text(time_x, TB + shift, &format.time_title()),
        Item::new(
            WIDTH - 12,
            TB + shift,
            Control::Check {
                label: "UTC".to_string(),
                checked: false,
            },
        ),
    ];
    for (row, label) in TIME_LABELS.iter().enumerate() {
        let y = TB + 1 + shift + u16::try_from(row).unwrap_or(0);
        let (date, time) = times::show(session.times[row], false, format);
        items.push(text(C2, y, label));
        let date = MaskedInput::new(&format.date_mask(), &date);
        items.push(Item::new(date_x, y, Control::Masked(date)));
        let time = MaskedInput::new(&format.time_mask(), &time);
        items.push(Item::new(time_x, y, Control::Masked(time)));
    }
    let margins = Place::Between(C2, 6);
    let multiple = session.mode == Mode::Multiple;
    items.push(
        button(TB + 6 + shift, "&Original", false)
            .placed(margins)
            .disabled(multiple),
    );
    items.push(button(TB + 6 + shift, "Curre&nt", false).placed(margins));
    items.push(button(TB + 6 + shift, "&Blank", false).placed(margins));
    items
}

/// FAR's `AdvancedAttributesDialog`: the other ten boxes, OK and Cancel.
pub fn more_form(session: &Session) -> Form {
    let mut items: Vec<Item> = (FLAG_LABELS[MAIN_FLAGS..].iter().enumerate())
        .map(|(at, label)| {
            let control = Control::Tristate {
                label: (*label).to_string(),
                state: session.advanced[at],
                three: session.three[MAIN_FLAGS + at],
            };
            Item::new(5, 2 + u16::try_from(at).unwrap_or(0), control)
        })
        .collect();
    let rows = u16::try_from(items.len()).unwrap_or(0);
    items.push(Item::new(0, 2 + rows, Control::Separator));
    items.push(button(3 + rows, "OK", true));
    items.push(button(3 + rows, "Cancel", false));
    Form::new("Attributes", 36, 6 + rows, items)
}

/// The "More" dialog's OK button.
pub const MORE_OK: usize = 11;
