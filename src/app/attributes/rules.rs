//! The dialog's own rules after each key (FAR's `SetAttrDlgProc`): `*` gives a time line to the others,
//! UTC turns the times over, compressed and encrypted never stay both on, "More"'s reparse point may only
//! be taken off, "Process subfolders" turns every box to three states and blanks a folder's times and
//! owner. What the user changed by hand (FAR's `ChangedManually`) is left alone by these rules.

use std::time::SystemTime;

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::layout::{self, FIRST_FLAG, ORIGINAL, OWNER, SUBFOLDERS, UTC};
use super::{COMPRESSED, ENCRYPTED, MAIN_FLAGS, Mode, Session, times};
use crate::app::dialog::{Dialog, FormAction};
use crate::app::form::Form;
use crate::app::masked_input::MaskedInput;

/// What the user changed by hand (FAR's `ChangedManually`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Touched {
    /// The boxes, "More" last.
    pub flags: [bool; 20],
    /// The time lines (a date or a time typed, Current, Blank, `*`).
    pub times: [bool; 4],
    pub owner: bool,
}

/// The fields as the last key left them, to tell what the next key changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    pub times: [(String, String); 4],
    pub owner: String,
}

/// Original: the times as they were (no longer changed by hand); Current: now; Blank: nothing, which
/// leaves them as they are (both changes by hand, as FAR's text set into the fields).
pub fn set_times(form: &mut Form, session: &mut Session, button: usize) {
    let now = SystemTime::now();
    for row in 0..4 {
        let time = match button {
            ORIGINAL => session.times[row],
            layout::CURRENT => Some(now),
            _ => None,
        };
        let (date, time) = times::show(time, session.utc, session.format);
        form.set_masked_text(layout::date(row), &date);
        form.set_masked_text(layout::time(row), &time);
    }
    session.touched.times = [button != ORIGINAL; 4];
    remember(form, session);
}

/// FAR's `DN_CONTROLINPUT`: `*` in a date or a time gives its line to the other three. Returns whether
/// the key was taken.
pub fn intercept(dialog: &mut Dialog, key: KeyEvent) -> bool {
    let Dialog::Form {
        form,
        action: FormAction::Attributes(_),
    } = dialog
    else {
        return false;
    };
    let Some(row) = layout::row_of(form.focus) else {
        return false;
    };
    if key.code != KeyCode::Char('*') {
        return false;
    }
    let date = form.masked_text(layout::date(row));
    let time = form.masked_text(layout::time(row));
    for other in (0..4).filter(|other| *other != row) {
        form.set_masked_text(layout::date(other), &date);
        form.set_masked_text(layout::time(other), &time);
    }
    true
}

/// FAR's `DN_CONTROLINPUT` of a time's label: a click takes the focus to its date, a double click puts
/// now into its line (in UTC when the box says so). Returns whether the click was taken.
pub fn text_click(dialog: &mut Dialog, index: usize, double: bool) -> bool {
    let Dialog::Form {
        form,
        action: FormAction::Attributes(session),
    } = dialog
    else {
        return false;
    };
    let Some(row) = layout::row_of_label(index) else {
        return false;
    };
    if double {
        let (date, time) = times::show(Some(SystemTime::now()), session.utc, session.format);
        form.set_masked_text(layout::date(row), &date);
        form.set_masked_text(layout::time(row), &time);
    } else {
        form.focus_item(layout::date(row));
    }
    true
}

/// FAR's `DN_BTNCLICK` and `DN_EDITCHANGE` after a key.
pub fn follow(dialog: &mut Dialog) {
    match dialog {
        Dialog::Form {
            form,
            action: FormAction::Attributes(session),
        } => {
            notice(form, session);
            if form.is_checked(UTC) != session.utc {
                turn_times(form, session);
            }
            exclude(form, session);
            if form.is_checked(SUBFOLDERS) != session.subfolders {
                subfolders(form, session);
            }
            remember(form, session);
            if form.focus != session.focus {
                got_focus(form, session);
            }
        }
        Dialog::Form {
            form,
            action: FormAction::AttributesMore { session, .. },
        } => {
            let before = session.more_reparse;
            if form.tristate(0) == Some(true) && before != Some(true) {
                form.set_tristate(0, before);
            }
            session.more_reparse = form.tristate(0);
        }
        _ => {}
    }
}

/// What the key changed by hand: a box, a time line, the owner.
fn notice(form: &Form, session: &mut Session) {
    for at in 0..MAIN_FLAGS {
        session.touched.flags[at] |= form.tristate(FIRST_FLAG + at) != session.shown[at];
    }
    for (row, seen) in session.seen.times.iter().enumerate() {
        let now = (
            form.masked_text(layout::date(row)),
            form.masked_text(layout::time(row)),
        );
        session.touched.times[row] |= now != *seen;
    }
    session.touched.owner |= form.edit_text(OWNER) != session.seen.owner;
}

/// Keeps the fields as they are now, to tell what the next key changes.
pub fn remember(form: &Form, session: &mut Session) {
    for (row, seen) in session.seen.times.iter_mut().enumerate() {
        *seen = (
            form.masked_text(layout::date(row)),
            form.masked_text(layout::time(row)),
        );
    }
    session.seen.owner = form.edit_text(OWNER).to_string();
    for (at, shown) in session.shown.iter_mut().enumerate() {
        *shown = form.tristate(FIRST_FLAG + at);
    }
}

/// FAR's "Process subfolders" turned on: every box goes round through `[?]` and one not changed by
/// hand shows `[?]`; a folder's times and owner not changed by hand go blank. Turned off: what was not
/// changed by hand is as it started, and a folder's boxes have two states again.
fn subfolders(form: &mut Form, session: &mut Session) {
    let on = form.is_checked(SUBFOLDERS);
    session.subfolders = on;
    let folder = session.mode == Mode::Folder;
    for at in 0..20 {
        if on {
            session.three[at] = true;
        } else if folder {
            session.three[at] = false;
        }
        let state = if on { None } else { session.flags[at] };
        let touched = session.touched.flags[at];
        match at.checked_sub(MAIN_FLAGS) {
            None => {
                form.set_three_state(FIRST_FLAG + at, session.three[at]);
                if !touched {
                    form.set_tristate(FIRST_FLAG + at, state);
                }
            }
            Some(more) if !touched => session.advanced[more] = state,
            Some(_) => {}
        }
    }
    for row in (0..4).filter(|row| folder && !session.touched.times[*row]) {
        let time = if on { None } else { session.times[row] };
        let (date, time) = times::show(time, session.utc, session.format);
        form.set_masked_text(layout::date(row), &date);
        form.set_masked_text(layout::time(row), &time);
    }
    if !session.touched.owner {
        let owner = if on { "" } else { session.owner.as_str() };
        form.set_edit_text(OWNER, owner);
    }
}

/// FAR's `DN_GOTFOCUS`: a year-first date whose first place is blank takes the cursor one place in, so
/// four digits type a year.
pub fn got_focus(form: &mut Form, session: &mut Session) {
    session.focus = form.focus;
    if session.format.order != crate::platform::locale::DateOrder::YearMonthDay
        || !(0..4).any(|row| layout::date(row) == form.focus)
    {
        return;
    }
    if let Some(input) = form.masked_mut(form.focus)
        && input.text().starts_with(' ')
        && input.cursor() == 0
    {
        input.set_cursor(1);
    }
}

/// Compressed on takes encrypted off and the other way round (FAR: they are mutually exclusive).
fn exclude(form: &mut Form, session: &Session) {
    let state = |form: &Form, at: usize| form.tristate(FIRST_FLAG + at);
    for (at, other) in [(COMPRESSED, ENCRYPTED), (ENCRYPTED, COMPRESSED)] {
        let now = state(form, at);
        if now == Some(true) && session.shown[at] != Some(true) && state(form, other) != Some(false)
        {
            form.set_tristate(FIRST_FLAG + other, Some(false));
        }
    }
}

/// UTC on or off: a line the user left as it was shows the original in the new zone; one they typed is
/// read in the old zone and shown in the new; a blank one, or one that is no time, stays as it is.
fn turn_times(form: &mut Form, session: &mut Session) {
    let to_utc = !session.utc;
    let format = session.format;
    for row in 0..4 {
        let fields = (
            form.masked_text(layout::date(row)),
            form.masked_text(layout::time(row)),
        );
        let (blank_date, blank_time) = times::show(None, session.utc, format);
        if fields.0 == masked(&format.date_mask(), &blank_date)
            && fields.1 == masked(&format.time_mask(), &blank_time)
        {
            continue;
        }
        let (date, time) = times::show(session.times[row], session.utc, format);
        let untouched = fields.0 == masked(&format.date_mask(), &date)
            && fields.1 == masked(&format.time_mask(), &time);
        let time = if untouched {
            session.times[row]
        } else {
            match times::parse(
                (&fields.0, &fields.1),
                session.times[row],
                session.utc,
                format,
            ) {
                Some(time) => Some(time),
                None => continue,
            }
        };
        let (date, time) = times::show(time, to_utc, format);
        form.set_masked_text(layout::date(row), &date);
        form.set_masked_text(layout::time(row), &time);
    }
    session.utc = to_utc;
}

/// A text as a field with `mask` shows it.
fn masked(mask: &str, text: &str) -> String {
    MaskedInput::new(mask, text).text()
}
