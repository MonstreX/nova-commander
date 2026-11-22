//! Colorer's regular expression tests (`tests/unit/test_cregexp.cpp`) beyond its plain matches: the
//! compile errors, the brackets and their names, `\m` / `\M`, `~`, `\y`, the prefilters, the step limit.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use super::*;

fn text(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| panic!("{pattern}: {e:?}"))
}

/// The match of `pattern` over `s` as `how` says.
fn found(pattern: &str, s: &str, how: Parse) -> Option<Matches> {
    let mut found = Matches::default();
    regex(pattern)
        .parse(&text(s), &mut found, how)
        .then_some(found)
}

fn at(pos: usize, moves: bool) -> Parse<'static> {
    Parse {
        pos,
        moves,
        ..Parse::default()
    }
}

fn whole(m: &Matches) -> (i32, i32) {
    (m.s[0], m.e[0])
}

#[test]
fn bad_patterns_say_why() {
    let cases = [
        ("", Error::Failed),
        ("abc", Error::Syntax),
        ("/abc", Error::Syntax),
        ("abc/", Error::Syntax),
        ("/(abc/", Error::Brackets),
        ("/abc)/", Error::Brackets),
        ("/[abc/", Error::Enum),
        ("/a{/", Error::Brackets),
        ("/+/", Error::Op),
        ("/{2}/", Error::Op),
        ("/a{5,2}/", Error::Op),
        ("/{5,2}/", Error::Op),
        ("/*/", Error::Op),
        ("/\\/", Error::Syntax),
        ("/\\y/", Error::Syntax),
        ("/\\p/", Error::Syntax),
        ("   ", Error::Syntax),
        ("/\\y{n}/", Error::Failed),
    ];
    for (pattern, error) in cases {
        assert_eq!(Regex::new(pattern).err(), Some(error), "{pattern:?}");
    }
}

/// A pattern, a text and the whole match expected (none: no match).
type Plain = (&'static str, &'static str, Option<(i32, i32)>);

#[test]
fn plain_matches_from_the_start() {
    let cases: &[Plain] = &[
        ("/abc/", "abc", Some((0, 3))),
        ("/abc/", "ab", None),
        ("/abc/", "abcd", Some((0, 3))),
        ("/abc/", "xabc", None),
        ("/^abc/", "abc", Some((0, 3))),
        ("/^abc/", "xabc", None),
        ("/abc$/", "abc", Some((0, 3))),
        ("/abc$/", "abcd", None),
        ("/^abc$/", "abc", Some((0, 3))),
        ("/a.c/", "axc", Some((0, 3))),
        ("/\\d+/", "42", Some((0, 2))),
        ("/\\D+/", "ab", Some((0, 2))),
        ("/\\w+/", "ab_1", Some((0, 4))),
        ("/\\s+/", " \t", Some((0, 2))),
        ("/[abc]/", "b", Some((0, 1))),
        ("/[^a]/", "b", Some((0, 1))),
        ("/[^a]/", "a", None),
        ("/a*/", "", Some((0, 0))),
        ("/a*/", "b", Some((0, 0))),
        ("/a+/", "", None),
        ("/a+/", "aaa", Some((0, 3))),
        ("/a?/", "", Some((0, 0))),
        ("/a?/", "a", Some((0, 1))),
        ("/a|b/", "a", Some((0, 1))),
        ("/a|b/", "b", Some((0, 1))),
        ("/ab|cd/", "cd", Some((0, 2))),
        ("/a{2}/", "aa", Some((0, 2))),
        ("/a{2}/", "a", None),
        ("/a{2,4}/", "aaaaa", Some((0, 4))),
        ("/a{2,}/", "aaa", Some((0, 3))),
        ("/\\t/", "\t", Some((0, 1))),
        ("/\\n/", "\n", Some((0, 1))),
        ("/\\x{41}/", "A", Some((0, 1))),
        ("/\\x41/", "A", Some((0, 1))),
        ("/a\\/b/", "a/b", Some((0, 3))),
        ("//", "abc", Some((0, 0))),
        ("/[a-c]+/", "abcd", Some((0, 3))),
        ("/[{L}-[a]]+/", "bca", Some((0, 2))),
        ("/[{L}&&[a-b]]+/", "abc", Some((0, 2))),
    ];
    for &(pattern, s, expected) in cases {
        let got = found(pattern, s, Parse::default()).map(|m| whole(&m));
        assert_eq!(got, expected, "{pattern} on {s:?}");
    }
}

#[test]
fn the_range_and_the_moves_bound_the_search() {
    let rx = regex("/abc/");
    let s = text("xxabcxx");
    let mut m = Matches::default();
    let how = |pos, eol| Parse {
        pos,
        eol: Some(eol),
        ..Parse::default()
    };
    assert!(rx.parse(&s, &mut m, how(2, 5)));
    assert_eq!(whole(&m), (2, 5));
    assert!(!rx.parse(&s, &mut m, how(2, 4)));
    assert_eq!(
        found("/abc/", "xxabc", at(0, true)).map(|m| whole(&m)),
        Some((2, 5))
    );
}

#[test]
fn brackets_numbered_and_named() {
    let m = found("/(ab)c/", "abc", Parse::default()).unwrap();
    assert_eq!((m.count, m.s[1], m.e[1]), (2, 0, 2));
    let m = found("/(a)(b)/", "ab", Parse::default()).unwrap();
    assert_eq!((m.count, m.s[2], m.e[2]), (3, 1, 2));
    let m = found("/(a)x|b/", "ab", at(0, true)).unwrap();
    assert_eq!(
        (whole(&m), m.s[1], m.e[1]),
        ((1, 2), -1, -1),
        "a failed offset keeps none"
    );
    let m = found("/(?:ab)c/", "abc", Parse::default()).unwrap();
    assert_eq!(m.count, 1);
    // Past the slots a bracket takes nothing but still matches.
    let full = format!("/a{}z/", "(x)".repeat(MATCHES));
    let s = format!("a{}z", "x".repeat(MATCHES));
    let m = found(&full, &s, Parse::default()).unwrap();
    assert_eq!((m.count, whole(&m)), (MATCHES, (0, s.len() as i32)));
    let named = |n: usize| {
        let body: String = (0..n).map(|i| format!("(?{{n{i}}}x)")).collect();
        regex(&format!("/{body}/"))
    };
    let over = named(NAMED + 1);
    assert_eq!(over.bracket_no(&text(&format!("n{NAMED}"))), None);
    assert_eq!(over.bracket_no(&text("n0")), Some(0));
    let rx = regex("/(?{n}ab)c/");
    assert_eq!(rx.bracket_no(&text("n")), Some(0));
    assert_eq!(rx.bracket_name(0).as_deref(), Some("n"));
    assert_eq!(rx.bracket_name(1), None);
    let m = found("/(?{n}ab)c/", "abc", Parse::default()).unwrap();
    assert_eq!((m.named, m.ns[0], m.ne[0]), (1, 0, 2));
    assert_eq!(
        found("/(?{}ab)c/", "abc", Parse::default()).unwrap().named,
        0
    );
    assert_eq!(regex("/(?{Foo}x)/").bracket_no(&text("foo")), Some(0));
    assert_eq!(regex("/(?{n}x)/").bracket_no(&text("missing")), None);
}

#[test]
fn colorer_s_start_end_and_scheme_start() {
    let m = found("/x\\mabc\\M/", "xabc", Parse::default()).unwrap();
    assert_eq!(whole(&m), (1, 4));
    let rx = regex("/~abc/");
    let s = text("xxabc");
    let mut m = Matches::default();
    let scheme = |pos| Parse {
        pos,
        scheme_start: 2,
        ..Parse::default()
    };
    assert!(rx.parse(&s, &mut m, scheme(2)));
    assert_eq!(whole(&m), (2, 5));
    assert!(!rx.parse(&s, &mut m, scheme(0)));
    // \m and \M do not stay from one parse or one offset to the next.
}

#[test]
fn start_and_end_do_not_stay_from_one_parse_to_the_next() {
    let mut m = Matches::default();
    let cases = [
        (
            "/foo\\mbar|xyz/",
            ("foobar", (3, 6)),
            ("xyz", (0, 3)),
            false,
        ),
        ("/abc\\Mx|xyz/", ("abcx", (0, 3)), ("xyz", (0, 3)), false),
        (
            "/foo\\mbar|end/",
            ("aaafoobar", (6, 9)),
            ("xxxend", (3, 6)),
            true,
        ),
    ];
    for (pattern, first, second, moves) in cases {
        let rx = regex(pattern);
        for (s, expected) in [first, second] {
            assert!(rx.parse(&text(s), &mut m, at(0, moves)), "{pattern} on {s}");
            assert_eq!(whole(&m), expected, "{pattern} on {s}");
        }
    }
}

#[test]
fn a_failed_branch_s_end_and_the_scheme_start() {
    let mut m = Matches::default();
    let failed = found("/\\M\\s+|;/", ";", Parse::default()).unwrap();
    assert_eq!(
        whole(&failed),
        (0, 0),
        "\\M of a failed branch still bounds it"
    );
    let rx = regex("/~\\s*#/");
    let s = text("x  #  #");
    let from = |pos, moves| Parse {
        pos,
        moves,
        scheme_start: 1,
        ..Parse::default()
    };
    assert!(rx.parse(&s, &mut m, from(1, false)));
    assert_eq!(m.e[0], 4);
    assert!(!rx.parse(&s, &mut m, from(4, false)));
    assert!(!rx.parse(&s, &mut m, from(2, true)));
}

/// `start` over `start_text` from its position, then `end` with its `\y` on it over `end_text`.
fn back(
    (start, start_text, from): (&str, &str, usize),
    (end, end_text): (&str, &str),
    how: Parse,
) -> Option<(i32, i32)> {
    let start = regex(start);
    let start_text = text(start_text);
    let mut trace = Matches::default();
    assert!(start.parse(&start_text, &mut trace, at(from, false)));
    let end = Regex::with_back(end, Some(&start)).unwrap();
    let mut m = Matches::default();
    let how = Parse {
        back: Some((&start_text, &trace)),
        ..how
    };
    end.parse(&text(end_text), &mut m, how).then_some(whole(&m))
}

#[test]
fn y_refers_to_another_expression_s_brackets() {
    let plain = Parse::default();
    let cases = [
        (
            ("/(foo)/", "foobar", 0),
            ("/\\y1bar/", "foobar"),
            plain,
            (0, 6),
        ),
        (
            ("/(?{n}foo)/", "foo", 0),
            ("/\\y{n}bar/", "foobar"),
            plain,
            (0, 6),
        ),
        (("/(foo)/", "foo", 0), ("/\\yF/", "foo"), plain, (0, 0)),
        (
            ("/(x)(?{n}Foo)/", "xFoo", 0),
            ("/\\Y{n}/", "foo"),
            plain,
            (0, 3),
        ),
        (
            ("/(abc)/", "xxabc", 2),
            ("/\\y1$/", "xxabc"),
            at(0, true),
            (2, 5),
        ),
    ];
    for (start, end, how, expected) in cases {
        assert_eq!(back(start, end, how), Some(expected), "{end:?}");
    }
    assert!(!regex("/foo/").has_back_trace());
    assert!(regex("/\\y1/").has_back_trace());
    let start = regex("/(?{n}foo)/");
    assert!(
        Regex::with_back("/\\Y{n}/", Some(&start))
            .unwrap()
            .has_back_trace()
    );
}

#[test]
fn the_first_characters_tell_where_a_match_may_start() {
    let rx = regex("/abc/");
    assert!(rx.can_start_with('a') && !rx.can_start_with('b') && !rx.can_start_with('x'));
    let rx = regex("/abc/i");
    assert!(rx.can_start_with('a') && rx.can_start_with('A'));
    let rx = regex("/a|x/");
    assert!(rx.can_start_with('a') && rx.can_start_with('x') && !rx.can_start_with('b'));
    let rx = regex("/[xyz]/");
    assert!(rx.can_start_with('y') && !rx.can_start_with('a'));
}

#[test]
fn may_match_and_the_end_anchor() {
    let rx = regex("/a$/");
    let s = text("xaZ");
    let line = ascii_chars(&s);
    let how = |pos, eol| Parse {
        pos,
        eol: Some(eol),
        ..Parse::default()
    };
    assert!(!rx.may_match(s.len(), &how(0, 2), &line));
    assert!(rx.may_match(s.len(), &how(1, 2), &line));
    let mut m = Matches::default();
    assert!(!rx.parse(&s, &mut m, how(0, 2)));
    assert!(rx.parse(&s, &mut m, how(1, 2)));
    assert_eq!(whole(&m), (1, 2));
    assert!(!rx.parse(&s, &mut m, how(1, 3)));
    let rx = regex("/(^|[^\\\\]?#1)$/");
    let s = text("abc");
    let line = ascii_chars(&s);
    assert!(rx.may_match(3, &how(3, 3), &line));
    assert!(!rx.may_match(3, &how(0, 3), &line));
    assert!(rx.parse(&s, &mut m, at(3, false)));
    assert_eq!(whole(&m), (3, 3));
    assert!(!rx.parse(&s, &mut m, at(0, false)));
    assert!(rx.parse(&s, &mut m, at(0, true)));
    assert_eq!(whole(&m), (3, 3));
    assert!(!rx.parse(&text("ab\\"), &mut m, at(3, false)));
}

/// A moving search with the line's characters given.
fn with_line(pattern: &str, s: &str, pos: usize) -> bool {
    let s = text(s);
    let line = ascii_chars(&s);
    let how = Parse {
        pos,
        moves: true,
        line_chars: Some(&line),
        ..Parse::default()
    };
    regex(pattern).parse(&s, &mut Matches::default(), how)
}

#[test]
fn the_characters_a_match_needs_filter_lines() {
    let line = ascii_chars(&text("a@\u{e9}"));
    assert!(line[1] & (1 << (b'a' - 64)) != 0 && line[1] & (1 << (b'@' - 64)) != 0);
    assert!(line[1] & (1 << (b'b' - 64)) == 0 && line[0] == 0);
    let yes = [
        ("/\\w+\\@\\w+/", "mail me@host"),
        ("/x/", "x"),
        ("/(x\\@)?y/", "y"),
        ("/(\\@)*y/", "y"),
        ("/(\\@){0,3}y/", "y"),
        ("/\\@?y/", "y"),
        ("/(a\\@|b\\#)/", "b#"),
        ("/a\\@|b\\#/", "a@"),
        ("/(x|)\\%/", "%"),
        ("/[\\@]?y/", "y"),
        ("/\\d?y/", "y"),
        ("/y(\\@)?!/", "y"),
        ("/(\\@)?~1y/", "y"),
        ("/(y)\\1/", "yy"),
        ("/y(\\@)?=/", "y@"),
        ("/k/i", "K"),
        ("/a1/i", "A1"),
    ];
    for (pattern, s) in yes {
        assert!(with_line(pattern, s, 0), "{pattern} on {s:?}");
    }
    let no = [
        ("/\\w+\\@\\w+/", "mail me at host"),
        ("/(a\\@|b\\#)/", "ab"),
        ("/y(\\@)?=/", "y"),
        ("/a1/i", "A2"),
    ];
    for (pattern, s) in no {
        assert!(!with_line(pattern, s, 0), "{pattern} on {s:?}");
    }
    // The line's characters cover what lies before the position.
    let rx = regex("/\\@/");
    let s = text("@ x");
    let line = ascii_chars(&s);
    let how = |pos| Parse {
        pos,
        line_chars: Some(&line),
        ..Parse::default()
    };
    assert!(!rx.parse(&s, &mut Matches::default(), how(1)));
    assert!(rx.parse(&s, &mut Matches::default(), how(0)));
}

#[test]
fn the_step_limit_stops_runaway_backtracking() {
    let mut rx = regex("/(a+)+b/");
    rx.set_step_limit(10_000);
    let s = text(&format!("{}c", "a".repeat(24)));
    assert_eq!(
        rx.parse_limited(&s, &mut Matches::default(), Parse::default()),
        (false, true)
    );
    let rx = regex("/abc/");
    let mut m = Matches::default();
    assert_eq!(
        rx.parse_limited(&text("abc"), &mut m, Parse::default()),
        (true, false)
    );
    let rx = regex("/xyz/");
    assert_eq!(
        rx.parse_limited(&text("aaa"), &mut m, Parse::default()),
        (false, false)
    );
    // The thread's frames serve the next parse as well.
    let rx = regex("/(a|b)+c/");
    assert!(rx.parse(&text("aaabbc"), &mut m, Parse::default()));
    assert!(rx.parse(&text("aaabbc"), &mut m, Parse::default()));
    assert_eq!(whole(&m), (0, 6));
}
