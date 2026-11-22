use super::*;

fn mask(text: &str) -> FileMask {
    FileMask::parse(text).unwrap()
}

#[test]
fn far_star_dot_star_matches_everything() {
    let all = mask("*.*");
    assert!(all.matches("readme"));
    assert!(all.matches("Main.RS"));
    assert!(all.matches(".gitignore"));
}

#[test]
fn extension_masks_lists_and_exclusions() {
    let rust = mask("*.rs, *.toml");
    assert!(rust.matches("main.rs"));
    assert!(rust.matches("Cargo.TOML"));
    assert!(!rust.matches("readme.md"));
    let without = mask("*.*|*.bak;*.tmp");
    assert!(without.matches("a.txt"));
    assert!(!without.matches("a.bak"));
    assert!(!without.matches("b.TMP"));
}

#[test]
fn wildcards_sets_and_special_endings() {
    assert!(mask("file?.txt").matches("file1.txt"));
    assert!(!mask("file?.txt").matches("file10.txt"));
    assert!(mask("[a-c]*").matches("beta"));
    assert!(!mask("[a-c]*").matches("delta"));
    assert!(mask("*.").matches("Makefile"));
    assert!(!mask("*.").matches("make.txt"));
    assert!(mask("make.*").matches("make"));
    assert!(mask("\"my file.txt\"").matches("My File.txt"));
    assert_eq!(FileMask::parse(" , "), None);
}

#[test]
fn bracket_sets_can_hold_brackets_and_unclosed_ones_are_literal() {
    assert!(mask("a[[]1[]]").matches("a[1]"));
    assert!(!mask("a[[]1[]]").matches("a1"));
    assert!(mask("x[*").matches("x[y"));
    assert!(!mask("x[*").matches("xy"));
}

#[test]
fn groups_stand_for_their_masks_once_and_pathext_for_the_programs() {
    let group = |name: &str| match name {
        "arc" => Some("*.zip,*.7z".to_string()),
        "loop" => Some("*.a,<loop>".to_string()),
        "exec" => Some("*.bat,%PathExt%".to_string()),
        _ => None,
    };
    let parse = |text: &str| FileMask::parse_with(text, &group, ".COM;.EXE").unwrap();
    assert!(parse("<arc>").matches("x.7z"));
    assert!(parse("*.txt,<arc>|<arc>").matches("a.txt"));
    assert!(!parse("*.*|<arc>").matches("a.zip"));
    assert!(
        parse("<loop>").matches("x.a"),
        "a group in itself stands for nothing"
    );
    assert!(parse("<exec>").matches("run.exe"));
    assert!(parse("<exec>").matches("run.bat"));
    assert!(!parse("<exec>").matches("run.txt"));
    assert_eq!(FileMask::parse_with("<none>", &group, ""), None);
    assert!(!mask("*.x,<arc>").matches("a.zip"), "no groups known");
    let but = parse("|<arc>");
    assert!(
        but.matches("a.txt") && !but.matches("a.zip"),
        "all but the exclusions"
    );
}

#[test]
fn far_cmp_name_reads_the_name_in_place() {
    // FAR's `CmpName`: each character compared as the system compares names, no copy of the name.
    assert_eq!(mask("файл*.TXT").matches("ФАЙЛ-1.txt"), cfg!(windows));
    assert!(mask("файл*.txt").matches("файл-1.txt"));
    assert!(mask("*a*b*c*.rs").matches("xxaYYbZZc_main.rs"));
    assert!(!mask("*a*b*c*.rs").matches("xxaYYcZZb_main.rs"));
    assert!(
        mask("?ф?.*").matches("aфb"),
        "`.*` at the end: no extension too"
    );
    assert!(mask("[а-я]*").matches("жук") && !mask("[а-я]*").matches("zebra"));
    assert!(
        !mask("[a-c]?").matches("b"),
        "a set takes one character, `?` one more"
    );
}

#[test]
fn quick_search_is_a_prefix_with_literal_brackets() {
    assert!(matches_prefix("rea", "README.md"));
    assert!(matches_prefix("r*me", "readme"));
    assert!(!matches_prefix("me", "readme"));
    assert!(matches_prefix("[1]", "[1] notes.txt"));
    assert!(!matches_prefix("[1]", "1 notes.txt"));
}

#[test]
fn separators_inside_a_set_belong_to_it() {
    let set = mask("[,;]*.txt,b*");
    assert!(set.matches(",a.txt"));
    assert!(set.matches(";a.txt"));
    assert!(set.matches("b.md"));
    assert!(!set.matches("a.txt"));
}

#[test]
fn regular_expressions_are_searched_for_in_the_name() {
    let re = mask(r"/\.(rs|md)$/");
    assert!(re.matches("main.rs"));
    assert!(re.matches("README.md"));
    assert!(!re.matches("main.RS"), "case counts without i");
    assert!(mask(r"/\.RS$/i").matches("main.rs"));
    assert!(
        mask(r"/^a\/b$/").matches("a/b"),
        "a slash after a backslash is the expression's"
    );
    assert!(mask("/x/o").matches("box"), "o changes nothing");
}

#[test]
fn expressions_and_masks_mix_with_exclusions() {
    let mixed = mask(r"*.txt /^read/i | /old/ ,*.bak");
    assert!(mixed.matches("a.txt"));
    assert!(mixed.matches("README"));
    assert!(!mixed.matches("old.txt"), "excluded by the expression");
    assert!(!mixed.matches("read.bak"), "excluded by the mask");
    assert!(mask("|/^a/").matches("b"), "exclusions alone: the rest");
    assert!(!mask("|/^a/").matches("abc"));
}

#[test]
fn an_expression_with_errors_fails_the_whole_list() {
    assert!(FileMask::parse("*.txt,/[/").is_none());
    assert!(FileMask::parse("/abc").is_none(), "no closing slash");
    assert!(FileMask::parse("/abc/q").is_none(), "an unknown option");
    assert!(FileMask::parse("/(/").is_none());
}

#[test]
fn two_expressions_with_other_options_differ() {
    assert_ne!(mask("/a/"), mask("/a/i"));
    assert_eq!(mask("/a/i"), mask("/a/i"));
}

#[test]
fn quotes_come_before_sets() {
    // FAR's quoted type `"*.a[,b"`: the `[` inside quotes opens no set, the quote is no character.
    let quoted = mask("\"*.a[,b\"");
    assert!(quoted.matches("x.a[,b"));
}
