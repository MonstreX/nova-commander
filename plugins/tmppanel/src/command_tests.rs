//! `tmp:`'s switches as FAR's `OpenW` reads them: on and off, compared as written, a common panel by its
//! digit, the rest trimmed.

use super::*;

fn read(line: &str) -> (Options, usize, String) {
    let mut options = Options::default();
    let mut current = 0;
    let rest = switches(line, &mut options, &mut current).to_string();
    (options, current, rest)
}

#[test]
fn switches_turn_options_on_and_off() {
    let (options, current, rest) = read("  +safe +any -replace +menu +full list.temp ");
    assert!(options.safe_mode_panel && options.any_in_panel);
    assert!(!options.replace, "-replace: off");
    assert!(options.menu_for_filelist && options.full_screen_panel);
    assert_eq!((current, rest.as_str()), (0, "list.temp"));
    let (options, _, rest) = read("-menu+any");
    assert!(!options.menu_for_filelist, "a switch runs to a space or <");
    assert!(!options.any_in_panel, "\"menu+any\" is no switch");
    assert_eq!(rest, "");
}

#[test]
fn switches_are_compared_as_written_and_unknown_ones_ignored() {
    let (options, _, rest) = read("+SAFE +ansi +safer name");
    assert_eq!(options, Options::default(), "FAR's `Cmd == Name`");
    assert_eq!(rest, "name");
}

#[test]
fn a_digit_names_the_common_panel_and_a_minus_digit_does_nothing() {
    assert_eq!(read("+3").1, 3);
    assert_eq!(read("+3 +7 x").1, 7);
    assert_eq!(read("-3").1, 0);
    assert_eq!(read("+12").1, 0, "one digit only");
}

#[test]
fn a_command_after_the_switches_starts_at_its_angle_bracket() {
    assert_eq!(read("+menu<dir /b").2, "<dir /b");
    assert_eq!(read("<dir /b").2, "<dir /b");
    assert_eq!(read("+").2, "+", "one character left: no switch");
    assert_eq!(read("").2, "");
}

#[test]
fn quotes_are_dropped_from_a_lists_name() {
    assert_eq!(unquote(r#""C:\my lists\a.temp""#), r"C:\my lists\a.temp");
}

#[test]
fn only_a_file_with_bytes_is_taken_as_a_list() {
    let temp = tempfile::tempdir().unwrap();
    let empty = temp.path().join("empty.temp");
    std::fs::write(&empty, "").unwrap();
    let full = temp.path().join("full.temp");
    std::fs::write(&full, "a").unwrap();
    assert!(!takes(&empty), "FAR's AnalyseW: BufferSize != 0");
    assert!(takes(&full));
    assert!(!takes(&temp.path().join("gone.temp")));
}
