//! The filter's dialog as FAR's `FileFilterConfig` fills it and reads it back.

use std::time::{Duration, UNIX_EPOCH};

use super::*;
use crate::platform::locale::DateOrder;

/// `YYYYY.MM.DD`, `hh:mm:ss.fffffff`.
fn format() -> TimeFormat {
    TimeFormat {
        order: DateOrder::YearMonthDay,
        date_separator: '.',
        time_separator: ':',
        decimal_separator: '.',
    }
}

fn check(form: &mut Form, index: usize, on: bool) {
    if let Some(Control::Check { checked, .. }) = form.control_mut(index) {
        *checked = on;
    }
    follow(form);
}

#[test]
fn a_new_filter_shows_its_mask_and_leaves_the_rest_disabled() {
    let form = filter_form(&Filter::default(), format());
    assert_eq!(
        (form.title.as_str(), form.width, form.height),
        ("Filter", 80, 15)
    );
    assert_eq!(form.focus, NAME);
    assert_eq!(form.edit_text(MASK), "*");
    assert!(form.is_checked(MATCH_MASK) && !form.items[MASK].disabled);
    assert!(form.items[SIZE_FROM].disabled && form.items[SIZE_TO].disabled);
    assert!((DATE_TYPE..=BLANK).all(|index| form.items[index].disabled));
    assert!(
        !form.items[UTC].disabled,
        "FAR leaves UTC out of the date's automation"
    );
    assert!(form.items[ATTRIBUTES].disabled);
    assert!(form.items[DAYS_BEFORE].hidden && !form.items[DATE_BEFORE].hidden);
}

#[test]
fn the_boxes_enable_their_fields() {
    let mut form = filter_form(&Filter::default(), format());
    check(&mut form, MATCH_SIZE, true);
    check(&mut form, MATCH_DATE, true);
    check(&mut form, MATCH_ATTRIBUTES, true);
    check(&mut form, MATCH_MASK, false);
    assert!(!form.items[SIZE_FROM].disabled && !form.items[TIME_AFTER].disabled);
    assert!(!form.items[ATTRIBUTES].disabled && !form.items[CURRENT].disabled);
    assert!(form.items[MASK].disabled);
}

#[test]
fn relative_shows_the_days_and_clears_the_dates() {
    let filter = Filter {
        use_date: true,
        date_before: 1_794_392_430,
        ..Filter::default()
    };
    let mut form = filter_form(&filter, format());
    assert!(
        !form.masked_text(DATE_BEFORE).trim().is_empty(),
        "the date kept is shown"
    );
    check(&mut form, RELATIVE, true);
    assert!(form.items[DATE_BEFORE].hidden && !form.items[DAYS_BEFORE].hidden);
    assert!(form.items[CURRENT].hidden && form.items[UTC].hidden);
    assert_eq!(
        form.masked_text(TIME_BEFORE).trim_end(),
        "  :  :  .",
        "{:?}",
        form.masked_text(TIME_BEFORE)
    );
    follow(&mut form);
    form.set_masked_text(DAYS_BEFORE, "3");
    follow(&mut form);
    assert_eq!(
        form.masked_text(DAYS_BEFORE).trim(),
        "3",
        "only a change clears"
    );
}

#[test]
fn a_relative_span_reads_back_in_seconds() {
    let filter = Filter {
        use_date: true,
        date_relative: true,
        date_after: 3 * 86_400 + 3_723,
        ..Filter::default()
    };
    let form = filter_form(&filter, format());
    assert_eq!(form.masked_text(DAYS_AFTER).trim(), "3");
    assert_eq!(form.masked_text(TIME_AFTER), "01:02:03.0000000");
    assert_eq!(
        form.masked_text(DAYS_BEFORE).trim(),
        "0",
        "FAR's 0 for no bound"
    );
    let read = read_filter(&form, &filter, &[], format()).unwrap();
    assert_eq!((read.date_after, read.date_before), (filter.date_after, 0));
}

#[test]
fn a_point_in_utc_reads_back_as_seconds_since_1970() {
    let mut form = filter_form(&Filter::default(), format());
    check(&mut form, MATCH_DATE, true);
    check(&mut form, UTC, true);
    form.set_masked_text(DATE_AFTER, " 2026.11.11");
    form.set_masked_text(TIME_AFTER, "10:20:30.0000000");
    let read = read_filter(&form, &Filter::default(), &[], format()).unwrap();
    assert_eq!(read.date_after, 1_794_392_430);
    assert_eq!(read.date_before, 0, "a blank date is no bound");
    assert!(read.use_date && !read.date_relative);
}

#[test]
fn utc_turns_the_dates_and_back() {
    let filter = Filter {
        date_before: 1_794_392_430,
        ..Filter::default()
    };
    let mut form = filter_form(&filter, format());
    let local = (form.masked_text(DATE_BEFORE), form.masked_text(TIME_BEFORE));
    turn_utc(&mut form, true, format());
    assert_eq!(form.masked_text(TIME_BEFORE), "10:20:30.0000000");
    turn_utc(&mut form, false, format());
    assert_eq!(
        (form.masked_text(DATE_BEFORE), form.masked_text(TIME_BEFORE)),
        local
    );
}

#[test]
fn current_fills_both_dates_and_blank_clears_them() {
    let mut form = filter_form(&Filter::default(), format());
    check(&mut form, MATCH_DATE, true);
    check(&mut form, UTC, true);
    set_dates(
        &mut form,
        Some(UNIX_EPOCH + Duration::from_secs(1_794_392_430)),
        format(),
    );
    assert_eq!(form.masked_text(DATE_AFTER).trim(), "2026.11.11");
    assert_eq!(form.masked_text(TIME_BEFORE), "10:20:30.0000000");
    assert_eq!(form.focus, DATE_BEFORE);
    set_dates(&mut form, None, format());
    assert!(
        form.masked_text(DATE_BEFORE)
            .trim()
            .replace(['.', ' '], "")
            .is_empty()
    );
}

#[test]
fn a_size_that_cannot_be_read_is_fars_warning() {
    let mut form = filter_form(&Filter::default(), format());
    check(&mut form, MATCH_SIZE, true);
    form.set_edit_text(SIZE_FROM, "10K");
    form.set_edit_text(SIZE_TO, "1x");
    assert_eq!(
        read_filter(&form, &Filter::default(), &[], format()),
        Err(BAD_SIZE)
    );
    form.set_edit_text(SIZE_TO, "2m");
    let read = read_filter(&form, &Filter::default(), &[], format()).unwrap();
    assert_eq!(
        (read.size_above.as_str(), read.size_below.as_str()),
        ("10K", "2m")
    );
    check(&mut form, MATCH_SIZE, false);
    form.set_edit_text(SIZE_TO, "1x");
    assert!(
        read_filter(&form, &Filter::default(), &[], format()).is_ok(),
        "not used, not read"
    );
}

#[test]
fn the_name_mask_and_switches_read_back_over_the_marks() {
    let mut base = Filter::default();
    base.marks.left_panel = "+".to_string();
    let mut form = filter_form(&base, format());
    form.set_edit_text(NAME, "Sources");
    form.set_edit_text(MASK, "*.rs");
    check(&mut form, HARD_LINKS, true);
    check(&mut form, MATCH_ATTRIBUTES, true);
    if let Some(Control::DropDown { chosen, .. }) = form.control_mut(DATE_TYPE) {
        *chosen = 3;
    }
    let mut states = vec![None; FILTER_ATTRIBUTES.len()];
    states[0] = Some(true);
    states[7] = Some(false);
    let read = read_filter(&form, &base, &states, format()).unwrap();
    assert_eq!(
        (read.title.as_str(), read.mask.as_str()),
        ("Sources", "*.rs")
    );
    assert!(read.use_hard_links && read.use_attributes);
    assert_eq!(read.date_type, "changed");
    assert_eq!(
        (read.attributes_set, read.attributes_clear),
        (0x1, DIRECTORY)
    );
    assert_eq!(read.marks, base.marks, "the marks stay");
}

#[test]
fn reset_gives_fars_defaults() {
    let filter = Filter {
        use_mask: false,
        mask: "x".to_string(),
        use_size: true,
        size_above: "1".to_string(),
        use_date: true,
        date_relative: true,
        date_type: "accessed".to_string(),
        use_hard_links: true,
        ..Filter::default()
    };
    let mut form = filter_form(&filter, format());
    reset(&mut form);
    assert_eq!(form.edit_text(MASK), "*");
    assert!(form.is_checked(MATCH_MASK) && form.is_checked(MATCH_ATTRIBUTES));
    assert!(!form.is_checked(MATCH_SIZE) && !form.is_checked(MATCH_DATE));
    assert!(!form.is_checked(RELATIVE) && !form.is_checked(HARD_LINKS));
    assert_eq!((form.chosen(DATE_TYPE), form.edit_text(SIZE_FROM)), (0, ""));
    assert!(!form.items[DATE_BEFORE].hidden);
    let states = reset_states();
    assert_eq!(states.iter().filter(|state| state.is_some()).count(), 1);
    assert_eq!(states[7], Some(false), "folders left out");
}

#[test]
fn the_attributes_dialog_has_a_three_state_box_for_each() {
    let filter = Filter {
        attributes_set: 0x2,
        attributes_clear: DIRECTORY,
        ..Filter::default()
    };
    let states = attribute_states(&filter);
    assert_eq!(
        (states[2], states[7], states[0]),
        (Some(true), Some(false), None)
    );
    let mut form = attributes_form(&states);
    assert_eq!(form.title, "Attributes");
    assert_eq!(read_attributes(&form), states);
    form.set_tristate(0, Some(false));
    assert_eq!(read_attributes(&form)[0], Some(false));
    let Some(Control::Button {
        label,
        default: true,
    }) = form.control(FILTER_ATTRIBUTES.len() + 1)
    else {
        panic!("no OK");
    };
    assert_eq!(label, "OK");
}

#[test]
fn sizes_are_checked_as_typed_as_far_does() {
    for good in ["10", "10K", "2m", "1b", "99999999999999999999"] {
        assert!(size_format(good), "{good}");
    }
    for bad in [" 10", "10 ", "K", "10KB", "10x", "1.5M"] {
        assert!(!size_format(bad), "{bad}");
    }
}

#[test]
fn the_attributes_dialog_keeps_its_buttons_inside_the_box() {
    let form = attributes_form(&reset_states());
    let ok = &form.items[FILTER_ATTRIBUTES.len() + 1];
    assert!(
        ok.y < form.height - 2,
        "the box's bottom line is at height - 2"
    );
    assert_eq!((form.width, form.height), (35, 27), "FAR's DialogBuilder");
}

#[test]
fn spans_are_days_and_a_time() {
    assert_eq!(
        span_text(0, format()),
        ("0".to_string(), "00:00:00.0000000".to_string())
    );
    assert_eq!(parse_span("2", "10:00:05.1234567"), 2 * 86_400 + 36_005);
    assert_eq!(parse_span("", "  :  :  .       "), 0);
}
