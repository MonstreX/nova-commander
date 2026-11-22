use super::*;

fn rules(name: Mode, extension: Mode) -> Rules {
    Rules {
        name,
        extension,
        skip_mixed: false,
        ..Rules::default()
    }
}

#[test]
fn each_part_takes_its_own_case() {
    let both = rules(Mode::Upper, Mode::Lower);
    assert_eq!(
        new_name("read me.TXT", false, &both).as_deref(),
        Some("READ ME.txt")
    );
    let first = rules(Mode::FirstCapital, Mode::None);
    assert_eq!(
        new_name("hELLO wORLD.Md", false, &first).as_deref(),
        Some("Hello world.Md")
    );
    let title = rules(Mode::Title, Mode::Upper);
    assert_eq!(
        new_name("my_old file.doc", false, &title).as_deref(),
        Some("My_Old File.DOC")
    );
}

#[test]
fn the_name_splits_at_its_last_dot_and_a_dot_file_is_all_extension() {
    let upper = rules(Mode::Upper, Mode::Lower);
    assert_eq!(
        new_name("a.b.TAR", false, &upper).as_deref(),
        Some("A.B.tar")
    );
    assert_eq!(
        new_name(".GitIgnore", false, &upper).as_deref(),
        Some(".gitignore")
    );
    assert_eq!(
        new_name("makefile", false, &upper).as_deref(),
        Some("MAKEFILE")
    );
}

#[test]
fn dot_names_unchanged_names_and_folders_are_left() {
    let upper = rules(Mode::Upper, Mode::Upper);
    assert_eq!(new_name(".", true, &upper), None);
    assert_eq!(new_name("..", true, &upper), None);
    assert_eq!(new_name("DONE.TXT", false, &upper), None, "as it is");
    assert_eq!(new_name("folder", true, &upper), None, "no folders' names");
    let folders = Rules {
        folder_names: true,
        ..upper
    };
    assert_eq!(
        new_name("folder", true, &folders).as_deref(),
        Some("FOLDER")
    );
    let none = rules(Mode::None, Mode::None);
    assert_eq!(new_name("any.Name", false, &none), None);
}

#[test]
fn a_part_in_mixed_case_is_left_when_asked() {
    let skip = Rules {
        skip_mixed: true,
        ..rules(Mode::Lower, Mode::Lower)
    };
    assert_eq!(
        new_name("McDonald.TXT", false, &skip).as_deref(),
        Some("McDonald.txt")
    );
    assert_eq!(
        new_name("UPPER.Txt", false, &skip).as_deref(),
        Some("upper.Txt")
    );
    assert!(is_mixed("aB") && is_mixed("1_Ab") && !is_mixed("ABC") && !is_mixed("123"));
    assert!(
        !is_mixed("__abc"),
        "the first letter counts, not the first character"
    );
}

#[test]
fn title_case_starts_a_word_after_each_delimiter() {
    assert_eq!(
        in_case("one two_three-four", Mode::Title, " _"),
        "One Two_Three-four"
    );
    assert_eq!(in_case("one-two", Mode::Title, "-"), "One-Two");
    assert_eq!(in_case("", Mode::Title, " _"), "");
}

#[test]
fn letters_beyond_ascii_change_case_one_for_one() {
    assert_eq!(in_case("привет", Mode::Upper, ""), "ПРИВЕТ");
    assert_eq!(in_case("ÉCOLE", Mode::FirstCapital, ""), "École");
    assert_eq!(
        in_case("straße", Mode::Upper, ""),
        "STRAßE",
        "ß stays one letter"
    );
}

#[test]
fn the_modes_are_the_dialogs_radio_buttons() {
    for index in 0..5 {
        assert_eq!(Mode::of(index).index(), index);
    }
    assert_eq!(Mode::of(9), Mode::Lower, "a setting out of range");
}
