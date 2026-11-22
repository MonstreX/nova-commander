//! arclite's `set_properties` and `load_update_props`'s reading of a method, without the library.

use super::*;
use crate::extract_options::Overwrite;
use crate::update_options::Kind;

fn options(format: &str, level: u32, method: &str, advanced: &str) -> UpdateOptions {
    UpdateOptions {
        level,
        method: method.to_string(),
        advanced: advanced.to_string(),
        encrypt: false,
        show_password: false,
        password: String::new(),
        overwrite: Overwrite::Ask,
        move_files: false,
        ignore_errors: false,
        open_shared: false,
        kind: Kind {
            format: format.to_string(),
            ..Kind::default()
        },
    }
}

fn text(value: &str) -> Prop {
    Prop::Text(value.to_string())
}

fn named(list: &[(&str, Prop)]) -> Vec<(String, Prop)> {
    (list.iter())
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect()
}

/// A 7z: its method, level and solid; the header's encryption only when encrypting and asked either
/// way; Store gives no solid.
#[test]
fn a_7zs_parameters() {
    let mut seven = options("7z", 5, "LZMA2", "");
    seven.kind.solid = true;
    assert_eq!(
        properties(&seven, false),
        named(&[
            ("0", text("LZMA2")),
            ("x", Prop::U32(5)),
            ("s", Prop::Bool(true))
        ])
    );
    seven.encrypt = true;
    seven.kind.encrypt_header = Some(true);
    assert!(properties(&seven, false).contains(&("he".to_string(), Prop::Bool(true))));
    seven.encrypt = false;
    assert!(
        !properties(&seven, false)
            .iter()
            .any(|(name, _)| name == "he")
    );
    let stored = options("7z", 0, "Copy", "");
    assert_eq!(
        properties(&stored, false),
        named(&[("0", text("Copy")), ("x", Prop::U32(0))])
    );
    assert!(properties(&seven, true).contains(&("qs".to_string(), text("on"))));
}

/// Another format: its level only; a bzip2 stored gives none; a zip its method too.
#[test]
fn other_formats_parameters() {
    assert_eq!(
        properties(&options("gzip", 9, "LZMA", ""), false),
        named(&[("x", Prop::U32(9))])
    );
    assert!(properties(&options("bzip2", 0, "Copy", ""), false).is_empty());
    assert_eq!(
        properties(&options("zip", 7, "Deflate", ""), false),
        named(&[("0", text("Deflate")), ("x", Prop::U32(7))])
    );
}

/// The advanced parameters: shorthands written out, an `x=N` taking the level's place (within 1..9),
/// a number given as one, another name after the named ones, a method given again in its place,
/// `--` dropping the method.
#[test]
fn advanced_parameters_are_folded_in() {
    let typed = options("7z", 5, "LZMA", "x12 s- mt4 d=64m 0=LZMA:d=32m");
    assert_eq!(
        properties(&typed, false),
        named(&[
            ("0", text("LZMA:d=32m")),
            ("x", Prop::U32(9)),
            ("s", text("off")),
            ("mt", Prop::U32(4)),
            ("d", text("64m")),
        ])
    );
    let dropped = options("7z", 0, "Copy", "--x3");
    assert_eq!(
        properties(&dropped, false),
        named(&[("x", Prop::U32(3)), ("s", Prop::Bool(false))])
    );
}

/// Digit names past the methods are moved past them and numbered from 0 with no gaps; one whose value
/// starts as the method's takes the method's place.
#[test]
fn digit_names_are_renumbered() {
    let again = options("7z", 5, "LZMA", "1=BCJ2 3=LZMA:d=1m");
    let named_again = properties(&again, false);
    assert_eq!(named_again[0], ("0".to_string(), text("LZMA:d=1m")));
    assert_eq!(named_again.len(), 4);
    let typed = options("7z", 5, "LZMA", "1=BCJ2 3=PPMD");
    let names: Vec<String> = properties(&typed, false)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["0", "x", "s", "1", "2"]);
}

/// A "Method" text: the first known word names the method; Copy is level 0; none: the format's.
#[test]
fn a_method_text_is_read() {
    assert_eq!(
        method_of(Some("LZMA2:24 BCJ"), false),
        (7, "LZMA2".to_string())
    );
    assert_eq!(
        method_of(Some("BCJ2 PPMD:o6"), false),
        (7, "PPMD".to_string())
    );
    assert_eq!(method_of(Some("Copy"), false), (0, "LZMA".to_string()));
    assert_eq!(method_of(None, true), (7, "Deflate".to_string()));
    assert_eq!(method_of(None, false), (7, "LZMA".to_string()));
    assert_eq!(method_of(Some("BZip2"), false), (7, "LZMA".to_string()));
}

/// The advanced parameters of a gzip (no method, no solid), by name: the shorthands typed.
fn advanced_of(typed: &str) -> Vec<(String, Prop)> {
    let mut named = properties(&options("gzip", 5, "LZMA", typed), false);
    named.retain(|(name, _)| name != "x");
    named
}

/// arclite's shorthands written out: `s`, `s1`, `s+` on; `s0`, `s-` off; `qs` likewise; `mtN` and
/// `yxN` with N from 1; other texts as they are.
#[test]
fn shorthands_are_written_out() {
    for on in ["s", "s1", "s+"] {
        assert_eq!(advanced_of(on), named(&[("s", text("on"))]), "{on}");
    }
    for off in ["s0", "s-"] {
        assert_eq!(advanced_of(off), named(&[("s", text("off"))]), "{off}");
    }
    for on in ["qs", "qs1", "qs+", "qs=on"] {
        assert_eq!(advanced_of(on), named(&[("qs", text("on"))]), "{on}");
    }
    for off in ["qs0", "qs-", "qs=off"] {
        assert_eq!(advanced_of(off), named(&[("qs", text("off"))]), "{off}");
    }
    assert_eq!(advanced_of("yx7"), named(&[("yx", Prop::U32(7))]));
    assert_eq!(advanced_of("mt0"), named(&[("mt0", text(""))]), "N from 1");
    assert_eq!(advanced_of("yx"), named(&[("yx", text(""))]));
    assert_eq!(advanced_of("mt12"), named(&[("mt", Prop::U32(12))]));
}

/// Empty words and a parameter of no name are passed over; a value of no digits is text.
#[test]
fn empty_parameters_are_passed_over() {
    assert_eq!(advanced_of("  =5  f=BCJ "), named(&[("f", text("BCJ"))]));
}

/// An `x=N` typed: 0 stays 0 (stored), 1..9 as they are, past 9 is 9; one of no digits leaves the level
/// chosen.
#[test]
fn a_typed_level_is_brought_into_range() {
    let level = |typed: &str| {
        let named = properties(&options("gzip", 5, "LZMA", typed), false);
        named
            .into_iter()
            .find(|(name, _)| name == "x")
            .map(|(_, value)| value)
    };
    assert_eq!(level("x0"), Some(Prop::U32(0)));
    assert_eq!(level("x=1"), Some(Prop::U32(1)));
    assert_eq!(level("x9"), Some(Prop::U32(9)));
    assert_eq!(level("x=10"), Some(Prop::U32(9)));
    assert_eq!(level("x=fast"), Some(Prop::U32(5)));
    assert_eq!(level("X=3"), Some(Prop::U32(3)), "whatever its case");
}

/// Numbers given as numbers: 32 bits when they fit, else 64; a negative one as its bits; a number typed
/// over a named parameter takes its place as one.
#[test]
fn numbers_are_given_as_numbers() {
    assert_eq!(
        advanced_of("d=4294967295 e=4294967296 f=-1"),
        named(&[
            ("d", Prop::U32(u32::MAX)),
            ("e", Prop::U64(1 << 32)),
            ("f", Prop::U64(u64::MAX)),
        ])
    );
    let mut seven = options("7z", 5, "LZMA", "s=4294967296");
    seven.kind.solid = true;
    let named_over = properties(&seven, false);
    assert!(named_over.contains(&("s".to_string(), Prop::U64(1 << 32))));
    seven.advanced = "s=1".to_string();
    let small = properties(&seven, false);
    assert!(
        small.contains(&("s".to_string(), Prop::U32(1))),
        "{small:?}"
    );
    seven.advanced = "s=".to_string();
    let empty = properties(&seven, false);
    assert!(empty.contains(&("s".to_string(), Prop::Empty)), "{empty:?}");
}

/// Several methods typed: renumbered from 0 in their order with no gaps, after the format's own.
#[test]
fn digit_names_close_their_gaps() {
    let typed = options("gzip", 5, "LZMA", "5=BCJ 2=LZMA");
    let names: Vec<String> = properties(&typed, false)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["x", "1", "0"]);
}

/// The levels each format takes: a bzip2 not stored, a wim and a tar stored only.
#[test]
fn the_levels_each_format_takes() {
    assert!(!level_allowed("bzip2", 0) && level_allowed("bzip2", 1));
    assert!(level_allowed("wim", 0) && !level_allowed("wim", 5));
    assert!(level_allowed("tar", 0) && !level_allowed("tar", 9));
    assert!(level_allowed("7z", 0) && level_allowed("7z", 9));
    assert_eq!(fit_level("wim", 9), 0);
    assert_eq!(fit_level("gzip", 3), 3);
}
