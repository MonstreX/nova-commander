//! Console input read by ourselves (ADR-0012 3e step 1): what a console record becomes.
//!
//! Given a translator and the records that came before, when one more record is translated, then
//! the whole list of inputs it yields is what the numbered rule says: nothing more, nothing less.

use crate::app::console_input::{Input, Translator};
use crate::platform::input::{ConsoleRecord, ControlKeys, KeyRecord, Modifiers};
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyEventState, KeyModifiers};

/// The control keys down in a record, in the order of the fields of `ControlKeys`.
const fn control(s: bool, l_ctrl: bool, r_ctrl: bool, l_alt: bool, r_alt: bool) -> ControlKeys {
    ControlKeys {
        shift: s,
        left_ctrl: l_ctrl,
        right_ctrl: r_ctrl,
        left_alt: l_alt,
        right_alt: r_alt,
    }
}

const NO_KEYS: ControlKeys = control(false, false, false, false, false);
const SHIFT: ControlKeys = control(true, false, false, false, false);
const LEFT_CTRL: ControlKeys = control(false, true, false, false, false);
const RIGHT_CTRL: ControlKeys = control(false, false, true, false, false);
const LEFT_ALT: ControlKeys = control(false, false, false, true, false);
const RIGHT_ALT: ControlKeys = control(false, false, false, false, true);
const CTRL_SHIFT: ControlKeys = control(true, true, false, false, false);
const LEFT_CTRL_ALT: ControlKeys = control(false, true, false, true, false);
const ALTGR: ControlKeys = control(false, true, false, false, true);
const ALTGR_SHIFT: ControlKeys = control(true, true, false, false, true);

/// What `Held` says, in the order of the fields of `Modifiers`.
const fn holding(shift: bool, ctrl: bool, alt: bool) -> Modifiers {
    Modifiers { shift, ctrl, alt }
}

const H_NONE: Modifiers = holding(false, false, false);
const H_SHIFT: Modifiers = holding(true, false, false);
const H_CTRL: Modifiers = holding(false, true, false);
const H_ALT: Modifiers = holding(false, false, true);
const H_CTRL_SHIFT: Modifiers = holding(true, true, false);
const H_CTRL_ALT: Modifiers = holding(false, true, true);
const H_ALL: Modifiers = holding(true, true, true);

const M_NONE: KeyModifiers = KeyModifiers::NONE;
const M_SHIFT: KeyModifiers = KeyModifiers::SHIFT;
const M_CTRL: KeyModifiers = KeyModifiers::CONTROL;
const M_ALT: KeyModifiers = KeyModifiers::ALT;
const M_CTRL_ALT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::ALT);
const M_CTRL_SHIFT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::SHIFT);

/// An `Input` compared field by field. `KeyEvent`'s own equality normalises case first (it holds
/// `Char('f')` with SHIFT equal to `Char('F')` with SHIFT), which would hide rules 6 and 14.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    Key(KeyCode, KeyModifiers, KeyEventKind, KeyEventState),
    Held(Modifiers),
    Resize(u16, u16),
    Focus(bool),
    Mouse(crate::app::console_input::Mouse),
}

fn exact(input: Input) -> Seen {
    match input {
        Input::Key(k) => Seen::Key(k.code, k.modifiers, k.kind, k.state),
        Input::Held(modifiers) => Seen::Held(modifiers),
        Input::Resize(width, height) => Seen::Resize(width, height),
        Input::Focus(gained) => Seen::Focus(gained),
        Input::Mouse(mouse) => Seen::Mouse(mouse),
    }
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> Seen {
    Seen::Key(code, modifiers, KeyEventKind::Press, KeyEventState::NONE)
}

fn key_up(code: KeyCode, modifiers: KeyModifiers) -> Seen {
    Seen::Key(code, modifiers, KeyEventKind::Release, KeyEventState::NONE)
}

fn ch(character: char, modifiers: KeyModifiers) -> Seen {
    key(KeyCode::Char(character), modifiers)
}

fn keypad(character: char, modifiers: KeyModifiers) -> Seen {
    let code = KeyCode::Char(character);
    Seen::Key(code, modifiers, KeyEventKind::Press, KeyEventState::KEYPAD)
}

fn nothing() -> Vec<Seen> {
    Vec::new()
}

fn held(modifiers: Modifiers) -> Vec<Seen> {
    vec![Seen::Held(modifiers)]
}

/// `Held(modifiers)` and then what the same record gave.
fn after(modifiers: Modifiers, seen: Seen) -> Vec<Seen> {
    vec![Seen::Held(modifiers), seen]
}

fn record(down: bool, virtual_key: u16, unit: u16, control: ControlKeys) -> ConsoleRecord {
    ConsoleRecord::Key(KeyRecord {
        down,
        virtual_key,
        unit,
        control,
    })
}

fn press(virtual_key: u16, unit: u16, keys: ControlKeys) -> ConsoleRecord {
    record(true, virtual_key, unit, keys)
}

fn release(virtual_key: u16, unit: u16, keys: ControlKeys) -> ConsoleRecord {
    record(false, virtual_key, unit, keys)
}

fn feed(t: &mut Translator, record: ConsoleRecord) -> Vec<Seen> {
    t.translate(&record).into_iter().map(exact).collect()
}

/// One more record on a translator that has seen some already.
#[track_caller]
fn then(t: &mut Translator, record: ConsoleRecord, expected: Vec<Seen>) {
    assert_eq!(feed(t, record), expected);
}

/// The first record a translator sees.
#[track_caller]
fn yields(record: ConsoleRecord, expected: Vec<Seen>) {
    then(&mut Translator::default(), record, expected);
}

/// A translator that has already been told that `control` is held.
fn holding_down(virtual_key: u16, keys: ControlKeys) -> Translator {
    let mut t = Translator::default();
    feed(&mut t, press(virtual_key, 0, keys));
    t
}

// Rule 1: what is held.

#[test]
fn a_modifier_alone_only_changes_what_is_held() {
    let cases = [
        (0x10, SHIFT, H_SHIFT),     // VK_SHIFT
        (0x11, LEFT_CTRL, H_CTRL),  // VK_CONTROL, either of the two
        (0x11, RIGHT_CTRL, H_CTRL), //
        (0x12, LEFT_ALT, H_ALT),    // VK_MENU, either of the two
        (0x12, RIGHT_ALT, H_ALT),   //
    ];
    for (virtual_key, keys, modifiers) in cases {
        yields(press(virtual_key, 0, keys), held(modifiers));
    }
}

#[test]
fn letting_go_of_a_modifier_says_that_nothing_is_held() {
    let mut t = holding_down(0x11, LEFT_CTRL); // VK_CONTROL
    then(&mut t, release(0x11, 0, NO_KEYS), held(H_NONE));
}

#[test]
fn held_comes_before_the_key_of_the_same_record() {
    let f5 = key(KeyCode::F(5), M_CTRL);
    yields(press(0x74, 0, LEFT_CTRL), after(H_CTRL, f5)); // VK_F5
}

#[test]
fn an_unchanged_state_is_not_reported_again() {
    let mut t = holding_down(0x11, LEFT_CTRL); // VK_CONTROL
    then(&mut t, press(0x11, 0, LEFT_CTRL), nothing()); // its auto-repeat
    let f5 = key(KeyCode::F(5), M_CTRL);
    then(&mut t, press(0x74, 0, LEFT_CTRL), vec![f5]); // VK_F5
}

#[test]
fn going_from_one_ctrl_to_the_other_changes_nothing_held() {
    let mut t = holding_down(0x11, LEFT_CTRL); // VK_CONTROL
    then(&mut t, press(0x11, 0, RIGHT_CTRL), nothing());
}

#[test]
fn a_key_with_nothing_held_after_a_modifier_says_so_first() {
    let mut t = holding_down(0x11, LEFT_CTRL); // VK_CONTROL
    let typed = after(H_NONE, ch('a', M_NONE));
    then(&mut t, press(0x41, 'a' as u16, NO_KEYS), typed); // VK_A
}

#[test]
fn the_state_on_a_release_is_reported_too() {
    yields(release(0x41, 'A' as u16, SHIFT), held(H_SHIFT)); // VK_A, the release is not
}

// Rule 2: focus and the records with no input of their own.

#[test]
fn losing_focus_with_nothing_held_says_only_that() {
    yields(ConsoleRecord::Focus(false), vec![Seen::Focus(false)]);
}

#[test]
fn losing_focus_lets_go_of_what_was_held_first() {
    let mut t = holding_down(0x10, SHIFT); // VK_SHIFT
    let lost = vec![Seen::Held(H_NONE), Seen::Focus(false)];
    then(&mut t, ConsoleRecord::Focus(false), lost);
    let again = vec![Seen::Focus(false)]; // it has already been let go of
    then(&mut t, ConsoleRecord::Focus(false), again);
}

#[test]
fn what_is_still_held_after_focus_returns_is_reported_again() {
    let mut t = holding_down(0x10, SHIFT); // VK_SHIFT
    feed(&mut t, ConsoleRecord::Focus(false));
    then(&mut t, press(0x10, 0, SHIFT), held(H_SHIFT));
}

#[test]
fn gaining_focus_says_only_that() {
    yields(ConsoleRecord::Focus(true), vec![Seen::Focus(true)]);
    let mut t = holding_down(0x10, SHIFT); // VK_SHIFT
    then(&mut t, ConsoleRecord::Focus(true), vec![Seen::Focus(true)]);
}

#[test]
fn a_resize_and_other_records_yield_nothing_from_the_translator() {
    yields(ConsoleRecord::Resized, nothing());
    yields(ConsoleRecord::Other, nothing());
    let mut t = holding_down(0x10, SHIFT); // VK_SHIFT
    then(&mut t, ConsoleRecord::Resized, nothing());
    then(&mut t, ConsoleRecord::Other, nothing());
}

// Rule 3: plain characters.

#[test]
fn a_plain_character_is_a_press_with_no_modifiers() {
    yields(press(0x41, 'a' as u16, NO_KEYS), vec![ch('a', M_NONE)]); // VK_A
}

#[test]
fn a_character_of_another_layout_is_typed_as_it_is() {
    yields(press(0x5A, 'я' as u16, NO_KEYS), vec![ch('я', M_NONE)]); // VK_Z
}

#[test]
fn a_held_down_character_repeats() {
    let mut t = Translator::default();
    let a = press(0x41, 'a' as u16, NO_KEYS); // VK_A
    then(&mut t, a, vec![ch('a', M_NONE)]);
    then(&mut t, a, vec![ch('a', M_NONE)]);
}

#[test]
fn the_release_of_a_character_is_not_reported() {
    yields(release(0x41, 'a' as u16, NO_KEYS), nothing()); // VK_A
}

// Rule 4: keys that are not characters.

const NAMED_KEYS: &[(u16, u16, KeyCode)] = &[
    (0x08, 0x08, KeyCode::Backspace),
    (0x09, 0x09, KeyCode::Tab),
    (0x0D, 0x0D, KeyCode::Enter),
    (0x1B, 0x1B, KeyCode::Esc),
    (0x21, 0, KeyCode::PageUp),
    (0x22, 0, KeyCode::PageDown),
    (0x23, 0, KeyCode::End),
    (0x24, 0, KeyCode::Home),
    (0x25, 0, KeyCode::Left),
    (0x26, 0, KeyCode::Up),
    (0x27, 0, KeyCode::Right),
    (0x28, 0, KeyCode::Down),
    (0x2D, 0, KeyCode::Insert),
    (0x2E, 0, KeyCode::Delete),
    (0x5D, 0, KeyCode::Menu),
];

#[test]
fn keys_that_are_not_characters_are_named_by_the_virtual_key() {
    for &(virtual_key, unit, code) in NAMED_KEYS {
        yields(press(virtual_key, unit, NO_KEYS), vec![key(code, M_NONE)]);
    }
}

#[test]
fn keys_that_are_not_characters_report_their_release() {
    for &(virtual_key, unit, code) in NAMED_KEYS {
        yields(
            release(virtual_key, unit, NO_KEYS),
            vec![key_up(code, M_NONE)],
        );
    }
}

#[test]
fn all_the_function_keys_are_named_and_report_their_release() {
    for virtual_key in 0x70..=0x87_u16 {
        // VK_F1..=VK_F24
        let code = KeyCode::F(u8::try_from(virtual_key - 0x6F).unwrap());
        yields(press(virtual_key, 0, NO_KEYS), vec![key(code, M_NONE)]);
        yields(release(virtual_key, 0, NO_KEYS), vec![key_up(code, M_NONE)]);
    }
}

#[test]
fn the_key_after_f24_is_no_function_key() {
    yields(press(0x88, 0, NO_KEYS), nothing()); // unassigned
}

#[test]
fn tab_with_shift_is_backtab_and_keeps_the_shift() {
    let mut t = Translator::default();
    let tab = after(H_SHIFT, key(KeyCode::BackTab, M_SHIFT));
    then(&mut t, press(0x09, 0x09, SHIFT), tab); // VK_TAB
    let tab_up = vec![key_up(KeyCode::BackTab, M_SHIFT)];
    then(&mut t, release(0x09, 0x09, SHIFT), tab_up);
}

#[test]
fn keys_that_are_not_characters_carry_their_modifiers() {
    let left = after(H_ALT, key(KeyCode::Left, M_ALT));
    yields(press(0x25, 0, RIGHT_ALT), left); // VK_LEFT
    let end = after(H_SHIFT, key(KeyCode::End, M_SHIFT));
    yields(press(0x23, 0, SHIFT), end); // VK_END
}

#[test]
fn the_unit_under_ctrl_does_not_rename_a_key_with_a_name() {
    let enter = after(H_CTRL, key(KeyCode::Enter, M_CTRL));
    yields(press(0x0D, 0x0A, LEFT_CTRL), enter); // VK_RETURN, the unit is a line feed
    let backspace = after(H_CTRL, key(KeyCode::Backspace, M_CTRL));
    yields(press(0x08, 0x7F, LEFT_CTRL), backspace); // VK_BACK, the unit is DEL
}

// Rule 5: the gray keys of the numeric keypad.

const GRAY_KEYS: [(u16, char); 3] = [
    (0x6B, '+'), // VK_ADD
    (0x6D, '-'), // VK_SUBTRACT
    (0x6A, '*'), // VK_MULTIPLY
];

#[test]
fn the_gray_keys_say_they_come_from_the_keypad() {
    for (virtual_key, symbol) in GRAY_KEYS {
        yields(
            press(virtual_key, symbol as u16, NO_KEYS),
            vec![keypad(symbol, M_NONE)],
        );
    }
}

#[test]
fn the_same_characters_from_the_main_keyboard_do_not() {
    yields(press(0xBB, '=' as u16, NO_KEYS), vec![ch('=', M_NONE)]); // VK_OEM_PLUS
    yields(press(0xBD, '-' as u16, NO_KEYS), vec![ch('-', M_NONE)]); // VK_OEM_MINUS
    let plus = after(H_SHIFT, ch('+', M_SHIFT));
    yields(press(0xBB, '+' as u16, SHIFT), plus);
}

#[test]
fn a_gray_key_under_ctrl_has_no_unit_and_is_still_named() {
    for (virtual_key, symbol) in GRAY_KEYS {
        for keys in [LEFT_CTRL, RIGHT_CTRL] {
            yields(
                press(virtual_key, 0, keys),
                after(H_CTRL, keypad(symbol, M_CTRL)),
            );
        }
    }
}

#[test]
fn the_release_of_a_gray_character_is_not_reported() {
    yields(release(0x6B, '+' as u16, NO_KEYS), nothing()); // VK_ADD
}

// Rule 6: keys under Ctrl or Alt whose unit says nothing.

#[test]
fn ctrl_and_a_letter_is_named_by_the_us_layout() {
    for keys in [LEFT_CTRL, RIGHT_CTRL] {
        yields(press(0x46, 0x06, keys), after(H_CTRL, ch('f', M_CTRL))); // VK_F, unit ^F
    }
}

#[test]
fn ctrl_shift_and_a_letter_stays_lower_case() {
    let lower = after(H_CTRL_SHIFT, ch('f', M_CTRL_SHIFT));
    yields(press(0x46, 0x06, CTRL_SHIFT), lower); // VK_F
}

#[test]
fn ctrl_and_a_bracket_is_not_mistaken_for_esc() {
    let bracket = after(H_CTRL, ch('[', M_CTRL));
    yields(press(0xDB, 0x1B, LEFT_CTRL), bracket); // VK_OEM_4, the `[` key, `х` in Russian
}

#[test]
fn ctrl_shift_and_a_key_with_no_unit_is_named_by_the_us_layout() {
    let bracket = after(H_CTRL_SHIFT, ch(']', M_CTRL_SHIFT));
    yields(press(0xDD, 0, CTRL_SHIFT), bracket); // VK_OEM_6, the `]` key
}

#[test]
fn alt_and_a_key_with_no_unit_is_named_by_the_us_layout() {
    yields(press(0x46, 0, LEFT_ALT), after(H_ALT, ch('f', M_ALT))); // VK_F
}

#[test]
fn a_key_the_us_layout_has_no_name_for_gives_no_key_under_ctrl() {
    yields(press(0x5B, 0, LEFT_CTRL), held(H_CTRL)); // VK_LWIN
}

// Rule 7: keys that type nothing.

#[test]
fn a_dead_key_types_nothing() {
    yields(press(0xDE, 0, NO_KEYS), nothing()); // VK_OEM_7
    yields(press(0xDE, 0, SHIFT), held(H_SHIFT));
}

#[test]
fn a_key_with_no_character_at_all_yields_nothing() {
    yields(press(0x5B, 0, NO_KEYS), nothing()); // VK_LWIN
    yields(release(0x5B, 0, NO_KEYS), nothing());
}

// Rule 8: AltGr is left Ctrl with right Alt.

#[test]
fn altgr_with_a_printable_unit_is_text() {
    let at = after(H_CTRL_ALT, ch('@', M_NONE));
    yields(press(0x51, '@' as u16, ALTGR), at); // VK_Q, the German layout
}

#[test]
fn altgr_text_keeps_the_shift() {
    let letter = after(H_ALL, ch('Ą', M_SHIFT));
    yields(press(0x41, 'Ą' as u16, ALTGR_SHIFT), letter); // VK_A, the Polish layout
}

#[test]
fn altgr_text_starts_at_the_space_and_below_it_is_a_shortcut() {
    let space = after(H_CTRL_ALT, ch(' ', M_NONE));
    yields(press(0x20, 0x20, ALTGR), space); // VK_SPACE
    let shortcut = after(H_CTRL_ALT, ch('a', M_CTRL_ALT));
    yields(press(0x41, 0x1F, ALTGR), shortcut); // VK_A
}

#[test]
fn altgr_with_a_control_code_stays_a_shortcut() {
    for unit in [0x00, 0x0A, 0x0D] {
        let enter = after(H_CTRL_ALT, key(KeyCode::Enter, M_CTRL_ALT));
        yields(press(0x0D, unit, ALTGR), enter); // VK_RETURN
    }
}

// Rule 9: left Ctrl with left Alt is decided later, by `keyboard::normalize`.

#[test]
fn left_ctrl_and_left_alt_keep_both_modifiers_and_the_character() {
    let at = after(H_CTRL_ALT, ch('@', M_CTRL_ALT));
    yields(press(0x51, '@' as u16, LEFT_CTRL_ALT), at); // VK_Q
}

// Rule 10: Alt and a character.

#[test]
fn alt_and_a_character_keeps_the_character_for_quick_search() {
    yields(
        press(0x5A, 'я' as u16, LEFT_ALT),
        after(H_ALT, ch('я', M_ALT)),
    ); // VK_Z
}

// Rule 11: Alt codes arrive on the release of Alt.

#[test]
fn an_alt_code_is_a_press_of_its_character() {
    yields(release(0x12, '&' as u16, NO_KEYS), vec![ch('&', M_NONE)]); // VK_MENU
}

#[test]
fn an_alt_code_typed_on_the_keypad_gives_its_character_after_alt_is_let_go() {
    let mut t = Translator::default();
    then(&mut t, press(0x12, 0, LEFT_ALT), held(H_ALT)); // VK_MENU
    then(&mut t, press(0x63, 0, LEFT_ALT), nothing()); // VK_NUMPAD3
    then(&mut t, release(0x63, 0, LEFT_ALT), nothing());
    let typed = after(H_NONE, ch('&', M_NONE));
    then(&mut t, release(0x12, '&' as u16, NO_KEYS), typed);
}

#[test]
fn the_release_of_alt_itself_is_only_a_change_of_what_is_held() {
    let mut t = holding_down(0x12, LEFT_ALT); // VK_MENU
    then(&mut t, release(0x12, 0, NO_KEYS), held(H_NONE));
    then(&mut t, release(0x12, 0, NO_KEYS), nothing());
}

// Rule 12: the keypad digits compose an Alt code.

#[test]
fn keypad_digits_under_alt_alone_yield_no_key() {
    for virtual_key in 0x60..=0x69_u16 {
        // VK_NUMPAD0..=VK_NUMPAD9
        for unit in [0, 0x30 + virtual_key - 0x60] {
            let mut t = holding_down(0x12, LEFT_ALT); // VK_MENU
            then(&mut t, press(virtual_key, unit, LEFT_ALT), nothing());
            then(&mut t, release(virtual_key, unit, LEFT_ALT), nothing());
        }
    }
}

#[test]
fn a_keypad_digit_without_alt_is_typed() {
    let seen = feed(&mut Translator::default(), press(0x61, '1' as u16, NO_KEYS)); // VK_NUMPAD1
    let typed = matches!(seen.as_slice(), [Seen::Key(KeyCode::Char('1'), ..)]);
    assert!(typed, "got {seen:?}");
}

#[test]
fn a_digit_of_the_main_keyboard_under_alt_is_a_key() {
    yields(
        press(0x31, '1' as u16, LEFT_ALT),
        after(H_ALT, ch('1', M_ALT)),
    ); // VK_1
}

// Rule 13: surrogate pairs. U+1F600 is 0xD83D, 0xDE00; VK_PACKET is what typed text comes with.

const VK_PACKET: u16 = 0xE7;
const VK_MENU: u16 = 0x12;
const HIGH: u16 = 0xD83D;
const LOW: u16 = 0xDE00;

fn grinning() -> Vec<Seen> {
    vec![ch('\u{1F600}', M_NONE)]
}

#[test]
fn a_surrogate_pair_is_one_character() {
    let mut t = Translator::default();
    then(&mut t, press(VK_PACKET, HIGH, NO_KEYS), nothing());
    then(&mut t, press(VK_PACKET, LOW, NO_KEYS), grinning());
}

#[test]
fn releases_between_the_halves_do_not_break_a_pair() {
    let mut t = Translator::default();
    then(&mut t, press(VK_PACKET, HIGH, NO_KEYS), nothing());
    then(&mut t, release(VK_PACKET, HIGH, NO_KEYS), nothing());
    then(&mut t, press(VK_PACKET, LOW, NO_KEYS), grinning());
    then(&mut t, release(VK_PACKET, LOW, NO_KEYS), nothing());
}

#[test]
fn a_low_surrogate_with_no_high_before_it_yields_nothing() {
    yields(press(VK_PACKET, LOW, NO_KEYS), nothing());
    yields(release(VK_MENU, LOW, NO_KEYS), nothing());
}

#[test]
fn a_high_surrogate_makes_only_one_pair() {
    let mut t = Translator::default();
    feed(&mut t, press(VK_PACKET, HIGH, NO_KEYS));
    feed(&mut t, press(VK_PACKET, LOW, NO_KEYS));
    then(&mut t, press(VK_PACKET, LOW, NO_KEYS), nothing());
}

#[test]
fn a_surrogate_pair_through_alt_codes_is_one_character() {
    let mut t = Translator::default();
    then(&mut t, release(VK_MENU, HIGH, NO_KEYS), nothing());
    then(&mut t, release(VK_MENU, LOW, NO_KEYS), grinning());
}

// Rule 14: Shift and a capital.

#[test]
fn shift_and_a_capital_keeps_both_and_says_so_once() {
    let mut t = Translator::default();
    let first = after(H_SHIFT, ch('A', M_SHIFT));
    then(&mut t, press(0x41, 'A' as u16, SHIFT), first); // VK_A
    then(
        &mut t,
        press(0x42, 'B' as u16, SHIFT),
        vec![ch('B', M_SHIFT)],
    ); // VK_B
}

#[test]
fn num5_without_numlock_is_the_keypad_s_middle_key() {
    let middle = |modifiers| key(KeyCode::KeypadBegin, modifiers);
    yields(press(0x0C, 0, NO_KEYS), vec![middle(M_NONE)]); // VK_CLEAR
    yields(press(0x0C, 0, SHIFT), after(H_SHIFT, middle(M_SHIFT)));
}
