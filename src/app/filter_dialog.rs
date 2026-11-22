//! FAR's "Filter" dialog (`far/filefilterparams.cpp`, `FileFilterConfig` without the highlighting's
//! colours): the name, the mask, the size from and to, the date of a kind (write, creation, access,
//! change) between two points or, relative, between two spans before now, the attributes (their own
//! dialog, each set, clear or either) and "Has more than one hardlink"; each check box enables its
//! fields (FAR's `SetAutomation`). Current puts now in both dates and Blank clears them, UTC turns the
//! dates shown into UTC and back, Reset gives FAR's defaults; OK checks the sizes (FAR's
//! "File size field is incorrectly filled") and the mask (the caller, with the groups).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::form::{Control, EditHistory, Form, Item, ListLine, Place};
use super::line_input::LineInput;
use super::masked_input::MaskedInput;
use crate::jobs::attributes::times::{self, TimeFormat};
use crate::settings::Filter;
use crate::vfs::filter::DIRECTORY;

pub const FILTER_TITLE: &str = "Filter";
/// FAR's message of a size it cannot read.
pub const BAD_SIZE: &str = "File size field is incorrectly filled";
const WIDTH: u16 = 80;
const HEIGHT: u16 = 15;
const SECONDS_A_DAY: u64 = 86_400;

// The items, as FAR's `ID_FF_*`.
pub(in crate::app) const NAME: usize = 1;
pub(in crate::app) const MATCH_MASK: usize = 3;
pub(in crate::app) const MASK: usize = 4;
pub(in crate::app) const MATCH_SIZE: usize = 6;
pub(in crate::app) const SIZE_FROM: usize = 8;
pub(in crate::app) const SIZE_TO: usize = 10;
pub(in crate::app) const MATCH_DATE: usize = 11;
pub(in crate::app) const DATE_TYPE: usize = 12;
pub(in crate::app) const RELATIVE: usize = 13;
pub(in crate::app) const DATE_BEFORE: usize = 15;
pub(in crate::app) const DAYS_BEFORE: usize = 16;
pub(in crate::app) const TIME_BEFORE: usize = 17;
pub(in crate::app) const DATE_AFTER: usize = 19;
pub(in crate::app) const DAYS_AFTER: usize = 20;
pub(in crate::app) const TIME_AFTER: usize = 21;
pub(in crate::app) const CURRENT: usize = 22;
pub(in crate::app) const BLANK: usize = 23;
pub(in crate::app) const UTC: usize = 24;
pub(in crate::app) const MATCH_ATTRIBUTES: usize = 27;
pub(in crate::app) const ATTRIBUTES: usize = 28;
pub(in crate::app) const HARD_LINKS: usize = 29;
pub(in crate::app) const OK: usize = 31;
pub(in crate::app) const RESET: usize = 32;

/// The kinds of date: as kept (`Filter::date_type`) and FAR's lines.
const DATE_TYPES: [(&str, &str); 4] = [
    ("modified", "&write"),
    ("created", "&creation"),
    ("accessed", "&access"),
    ("changed", "c&hange"),
];

/// FAR's `AttributeMapping`: the check boxes of the filter's "Attributes" dialog, Windows' bits.
pub const FILTER_ATTRIBUTES: [(u32, &str); 21] = [
    (0x0000_0001, "&Read only"),
    (0x0000_0020, "&Archive"),
    (0x0000_0002, "&Hidden"),
    (0x0000_0004, "&System"),
    (0x0000_0800, "&Compressed"),
    (0x0000_4000, "&Encrypted"),
    (0x0000_2000, "Not &indexed"),
    (DIRECTORY, "&Directory"),
    (0x0000_0200, "S&parse"),
    (0x0000_0100, "Temporar&y"),
    (0x0000_1000, "O&ffline"),
    (0x0000_0400, "Reparse poin&t"),
    (0x0001_0000, "&Virtual"),
    (0x0000_8000, "Inte&grity stream"),
    (0x0002_0000, "No scru&b data"),
    (0x0008_0000, "Pi&nned"),
    (0x0010_0000, "&Unpinned"),
    (0x0004_0000, "Recal&l on open"),
    (0x0040_0000, "Recall &on data access"),
    (0x2000_0000, "Strictly se&quential"),
    (0x0000_0040, "Device"),
];

/// Each attribute of [`FILTER_ATTRIBUTES`]: set, clear, or either (`None`).
pub fn attribute_states(filter: &Filter) -> Vec<Option<bool>> {
    FILTER_ATTRIBUTES
        .iter()
        .map(|&(bit, _)| {
            if filter.attributes_set & bit != 0 {
                Some(true)
            } else if filter.attributes_clear & bit != 0 {
                Some(false)
            } else {
                None
            }
        })
        .collect()
}

/// FAR's Reset of the attributes: either for each, a folder not (a filter is for files).
pub fn reset_states() -> Vec<Option<bool>> {
    (FILTER_ATTRIBUTES.iter())
        .map(|&(bit, _)| (bit == DIRECTORY).then_some(false))
        .collect()
}

/// The bits that must be set and that must be clear.
fn attribute_bits(states: &[Option<bool>]) -> (u32, u32) {
    (FILTER_ATTRIBUTES.iter().zip(states)).fold((0, 0), |(set, clear), (&(bit, _), state)| {
        match state {
            Some(true) => (set | bit, clear),
            Some(false) => (set, clear | bit),
            None => (set, clear),
        }
    })
}

/// FAR's dialog of the attributes (`AttributesDialog`): a three-state box for each, OK and Cancel.
pub fn attributes_form(states: &[Option<bool>]) -> Form {
    let mut items: Vec<Item> = (FILTER_ATTRIBUTES.iter().zip(states).enumerate())
        .map(|(row, (&(_, label), &state))| {
            let control = Control::Tristate {
                label: label.to_string(),
                state,
                three: true,
            };
            Item::new(5, 2 + u16::try_from(row).unwrap_or(0), control)
        })
        .collect();
    let rows = u16::try_from(FILTER_ATTRIBUTES.len()).unwrap_or(0);
    items.push(Item::new(0, 2 + rows, Control::Separator));
    items.push(button(3 + rows, "OK", true));
    items.push(button(3 + rows, "Cancel", false));
    // FAR's `DialogBuilder`: the widest box ("[ ] Recall on data access") and its margins.
    let widest = FILTER_ATTRIBUTES
        .iter()
        .map(|(_, label)| label.replace('&', "").chars().count())
        .max()
        .unwrap_or(0);
    let width = u16::try_from(widest + 4 + 10).unwrap_or(40);
    Form::new("Attributes", width, 6 + rows, items)
}

/// What the attributes dialog was given.
pub fn read_attributes(form: &Form) -> Vec<Option<bool>> {
    (0..FILTER_ATTRIBUTES.len())
        .map(|at| form.tristate(at))
        .collect()
}

/// FAR's `FileFilterConfig` for `filter`, the dates in local time.
pub fn filter_form(filter: &Filter, format: TimeFormat) -> Form {
    let mut items = vec![
        text(5, 2, "&Name:"),
        edit(11, 2, 64, &filter.title).with_history(EditHistory::new("FilterName")),
        Item::new(0, 3, Control::Separator),
        check(5, 4, "&Mask:", filter.use_mask),
        edit(15, 4, 60, &filter.mask).with_history(EditHistory::new("FilterMasks")),
        Item::new(0, 5, Control::Separator),
        check(5, 6, "Si&ze:", filter.use_size),
        text(7, 7, ">&="),
        edit(10, 7, 11, &filter.size_above),
        text(7, 8, "&<="),
        edit(10, 8, 11, &filter.size_below),
        check(24, 6, "Date&/Time:", filter.use_date),
        date_types(&filter.date_type),
        check(24, 8, "Relat&ive", filter.date_relative),
    ];
    items.extend(date_fields(filter, format));
    items.extend([
        button_at(46, 6, "C&urrent"),
        button_at(58, 6, "B&lank"),
        check(68, 6, "UTC", false),
        Item::new(0, 9, Control::Separator),
        Item::new(22, 5, Control::VRule(5)),
        check(5, 10, "Attri&butes", filter.use_attributes),
        button_at(20, 10, "&..."),
        check(41, 10, "Has more than one hardlin&k", filter.use_hard_links),
        Item::new(0, 11, Control::Separator),
        button(12, "OK", true),
        button(12, "&Reset", false),
        button(12, "Cancel", false),
    ]);
    let mut form = Form::new(FILTER_TITLE, WIDTH, HEIGHT, items);
    form.focus = NAME;
    // The fields of the kind of dates kept are the ones shown: nothing is cleared at the start.
    form.items[DAYS_BEFORE].hidden = !filter.date_relative;
    follow(&mut form);
    form
}

/// The six date fields, both kinds: the absolute ones in local time, the relative ones as FAR's
/// `duration_to_string`.
fn date_fields(filter: &Filter, format: TimeFormat) -> Vec<Item> {
    let mut items = Vec::new();
    for (y, sign, seconds) in [(7, "<=", filter.date_before), (8, ">=", filter.date_after)] {
        let (date, time, days) = if filter.date_relative {
            let (days, time) = span_text(seconds, format);
            (String::new(), time, days)
        } else {
            let (date, time) = point_text(seconds, false, format);
            (date, time, String::new())
        };
        items.push(text(43, y, sign));
        items.push(masked(47, y, &format.date_mask(), &date));
        // FAR's field is 11 wide (`{47,y}-{57,y}`); its mask takes four digits.
        items.push(masked(47, y, "9999", &days).wide(11));
        items.push(masked(59, y, &format.time_mask(), &time));
    }
    items
}

/// FAR's date and time of a point kept as seconds since 1970; blank for none (0).
fn point_text(seconds: u64, utc: bool, format: TimeFormat) -> (String, String) {
    if seconds == 0 {
        return (String::new(), String::new());
    }
    times::show(Some(UNIX_EPOCH + Duration::from_secs(seconds)), utc, format)
}

/// FAR's `duration_to_string`: the days, and the rest as a time to a ten-millionth.
fn span_text(seconds: u64, format: TimeFormat) -> (String, String) {
    let (t, d) = (format.time_separator, format.decimal_separator);
    let rest = seconds % SECONDS_A_DAY;
    let time = format!(
        "{:02}{t}{:02}{t}{:02}{d}0000000",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    );
    ((seconds / SECONDS_A_DAY).to_string(), time)
}

/// FAR's `CheckFileSizeStringFormat`: digits, then at most one of `BbKkMmGgTtPpEe`, as typed.
fn size_format(text: &str) -> bool {
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    let mut rest = text.chars().skip(digits);
    match (digits, rest.next(), rest.next()) {
        (0, _, _) | (_, Some(_), Some(_)) => false,
        (_, None, _) => true,
        (_, Some(suffix), None) => "BbKkMmGgTtPpEe".contains(suffix),
    }
}

/// FAR's `ParseDuration`: blank parts are 0 (the ten-millionths are let go: seconds are kept).
fn parse_span(days: &str, time: &str) -> u64 {
    let part = |text: &str, from: usize, length: usize| -> u64 {
        let part: String = text.chars().skip(from).take(length).collect();
        part.trim().parse().unwrap_or(0)
    };
    let days: u64 = days.trim().parse().unwrap_or(0);
    days * SECONDS_A_DAY + part(time, 0, 2) * 3600 + part(time, 3, 2) * 60 + part(time, 6, 2)
}

/// A point the fields give, as seconds since 1970; 0 (no bound) for blank fields or no point in time.
fn parse_point(date: &str, time: &str, utc: bool, format: TimeFormat) -> u64 {
    times::parse((date, time), None, utc, format)
        .and_then(|point| point.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs())
}

/// The filter the dialog gives, over `base` (its marks stay); `Err` with FAR's message for a size it
/// cannot read.
pub fn read_filter(
    form: &Form,
    base: &Filter,
    states: &[Option<bool>],
    format: TimeFormat,
) -> Result<Filter, &'static str> {
    let use_size = form.is_checked(MATCH_SIZE);
    let size = |index| form.edit_text(index).to_string();
    let (size_above, size_below) = (size(SIZE_FROM), size(SIZE_TO));
    let readable = |size: &str| size.is_empty() || size_format(size);
    if use_size && !(readable(&size_above) && readable(&size_below)) {
        return Err(BAD_SIZE);
    }
    let relative = form.is_checked(RELATIVE);
    let utc = form.is_checked(UTC);
    let bound = |date: usize, days: usize, time: usize| {
        let time = form.masked_text(time);
        if relative {
            parse_span(&form.masked_text(days), &time)
        } else {
            parse_point(&form.masked_text(date), &time, utc, format)
        }
    };
    let (attributes_set, attributes_clear) = attribute_bits(states);
    Ok(Filter {
        title: form.edit_text(NAME).to_string(),
        use_mask: form.is_checked(MATCH_MASK),
        mask: form.edit_text(MASK).to_string(),
        use_size,
        size_above,
        size_below,
        use_date: form.is_checked(MATCH_DATE),
        date_type: DATE_TYPES[form.chosen(DATE_TYPE).min(3)].0.to_string(),
        date_relative: relative,
        date_before: bound(DATE_BEFORE, DAYS_BEFORE, TIME_BEFORE),
        date_after: bound(DATE_AFTER, DAYS_AFTER, TIME_AFTER),
        use_attributes: form.is_checked(MATCH_ATTRIBUTES),
        attributes_set,
        attributes_clear,
        use_hard_links: form.is_checked(HARD_LINKS),
        ..base.clone()
    })
}

/// FAR's `SetAutomation`: each check box enables its fields; Relative shows the days in place of the
/// dates (and hides Current and UTC), the fields cleared when it changes (FAR's
/// `FilterDlgRelativeDateItemsUpdate`).
pub fn follow(form: &mut Form) {
    let enable = |form: &mut Form, items: &[usize], on: bool| {
        for &index in items {
            form.items[index].disabled = !on;
        }
    };
    let (mask, size, date) = (
        form.is_checked(MATCH_MASK),
        form.is_checked(MATCH_SIZE),
        form.is_checked(MATCH_DATE),
    );
    let attributes = form.is_checked(MATCH_ATTRIBUTES);
    enable(form, &[MASK], mask);
    enable(form, &(7..=SIZE_TO).collect::<Vec<_>>(), size);
    enable(form, &(DATE_TYPE..=BLANK).collect::<Vec<_>>(), date);
    enable(form, &[ATTRIBUTES], attributes);
    let relative = form.is_checked(RELATIVE);
    let switched = form.items[DAYS_BEFORE].hidden == relative;
    for index in [DATE_BEFORE, DATE_AFTER, CURRENT, UTC] {
        form.items[index].hidden = relative;
    }
    for index in [DAYS_BEFORE, DAYS_AFTER] {
        form.items[index].hidden = !relative;
    }
    if switched {
        clear_dates(form);
    }
}

/// Every date field blank.
fn clear_dates(form: &mut Form) {
    for index in [
        DATE_BEFORE,
        DAYS_BEFORE,
        TIME_BEFORE,
        DATE_AFTER,
        DAYS_AFTER,
        TIME_AFTER,
    ] {
        form.set_masked_text(index, "");
    }
}

/// Current (now, local or UTC as the box says) or Blank in both dates; the focus on the first.
pub fn set_dates(form: &mut Form, now: Option<SystemTime>, format: TimeFormat) {
    let utc = form.is_checked(UTC);
    let (date, time) = now.map_or_else(Default::default, |now| times::show(Some(now), utc, format));
    let relative = form.is_checked(RELATIVE);
    let (before, after) = if relative {
        (DAYS_BEFORE, DAYS_AFTER)
    } else {
        (DATE_BEFORE, DATE_AFTER)
    };
    for (field, time_field) in [(after, TIME_AFTER), (before, TIME_BEFORE)] {
        form.set_masked_text(field, &date);
        form.set_masked_text(time_field, &time);
    }
    form.focus_item(before);
    if let Some(input) = form.masked_mut(before) {
        input.set_cursor(0);
    }
}

/// UTC turned on or off: the absolute dates shown in the other time (FAR's `ID_FF_UTC`); those that
/// make no point in time stay.
pub fn turn_utc(form: &mut Form, to_utc: bool, format: TimeFormat) {
    if form.is_checked(RELATIVE) {
        return;
    }
    for (date, time) in [(DATE_BEFORE, TIME_BEFORE), (DATE_AFTER, TIME_AFTER)] {
        let shown = (form.masked_text(date), form.masked_text(time));
        let Some(point) = times::parse((&shown.0, &shown.1), None, !to_utc, format) else {
            continue;
        };
        let (new_date, new_time) = times::show(Some(point), to_utc, format);
        form.set_masked_text(date, &new_date);
        form.set_masked_text(time, &new_time);
    }
}

/// FAR's Reset: the mask `*` used, no size, the first kind of date and no date, no hard links, the
/// attributes used with only the folders left out.
pub fn reset(form: &mut Form) {
    form.set_edit_text(MASK, "*");
    form.set_edit_text(SIZE_FROM, "");
    form.set_edit_text(SIZE_TO, "");
    if let Some(Control::DropDown { chosen, .. }) = form.control_mut(DATE_TYPE) {
        *chosen = 0;
    }
    for (index, on) in [
        (MATCH_MASK, true),
        (MATCH_SIZE, false),
        (HARD_LINKS, false),
        (MATCH_DATE, false),
        (RELATIVE, false),
        (MATCH_ATTRIBUTES, true),
    ] {
        if let Some(Control::Check { checked, .. }) = form.control_mut(index) {
            *checked = on;
        }
    }
    follow(form);
    clear_dates(form);
}

fn date_types(kept: &str) -> Item {
    let chosen = DATE_TYPES.iter().position(|(id, _)| *id == kept);
    let control = Control::DropDown {
        lines: (DATE_TYPES.iter())
            .map(|(_, label)| ListLine::Choice((*label).to_string()))
            .collect(),
        chosen: chosen.unwrap_or(0),
        open: None,
        caption: None,
        bottom_title: None,
    };
    Item::new(24, 7, control).wide(15)
}

fn text(x: u16, y: u16, label: &str) -> Item {
    Item::new(x, y, Control::Text(label.to_string()))
}

fn edit(x: u16, y: u16, width: u16, text: &str) -> Item {
    Item::new(x, y, Control::Edit(LineInput::with_text(text))).wide(width)
}

fn masked(x: u16, y: u16, mask: &str, text: &str) -> Item {
    Item::new(x, y, Control::Masked(MaskedInput::new(mask, text)))
}

fn check(x: u16, y: u16, label: &str, checked: bool) -> Item {
    let control = Control::Check {
        label: label.to_string(),
        checked,
    };
    Item::new(x, y, control)
}

fn button(y: u16, label: &str, default: bool) -> Item {
    let control = Control::Button {
        label: label.to_string(),
        default,
    };
    Item::new(0, y, control)
}

/// A button at its own column, not in the row's centred group.
fn button_at(x: u16, y: u16, label: &str) -> Item {
    let mut item = button(y, label, false).placed(Place::At);
    item.x = x;
    item
}

#[cfg(test)]
#[path = "filter_dialog_tests.rs"]
mod tests;
