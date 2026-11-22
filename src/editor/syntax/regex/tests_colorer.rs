//! Colorer's own matches (`tests/unit/test_cregexp.cpp`'s `require_match` / `require_no_match`, its line
//! numbers kept): the pattern, the text, the whole match expected (none: no match), moving, from where.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use super::{Matches, Parse, Regex};

type Case = (
    u32,
    &'static str,
    &'static str,
    Option<(i32, i32)>,
    bool,
    usize,
);

const CASES: &[Case] = &[
    (86, "//", "", Some((0, 0)), false, 0),
    (87, "//", "abc", Some((0, 0)), false, 0),
    (210, "/abc/", "xabc", None, false, 0),
    (215, "/abc/", "xabc", Some((1, 4)), true, 0),
    (248, "/abc/i", "ABC", Some((0, 3)), false, 0),
    (249, "/abc/i", "AbC", Some((0, 3)), false, 0),
    (250, "/abc/", "ABC", None, false, 0),
    (251, "/[x]/i", "X", Some((0, 1)), false, 0),
    (252, "/[x]/i", "x", Some((0, 1)), false, 0),
    (253, "/[aX]/i", "x", Some((0, 1)), false, 0),
    (254, "/[aX]/i", "A", Some((0, 1)), false, 0),
    (255, "/[x]/", "X", None, false, 0),
    (260, "/a.c/", "a\nc", None, false, 0),
    (261, "/a.c/s", "a\nc", Some((0, 3)), false, 0),
    (266, "/a b c/x", "abc", Some((0, 3)), false, 0),
    (272, "/\".*\"/", "\"a\"b\"", Some((0, 5)), false, 0),
    (273, "/\".*?\"/", "\"a\"b\"", Some((0, 3)), false, 0),
    (274, "/a+?/", "aaa", Some((0, 1)), false, 0),
    (275, "/a*?b/", "aaab", Some((0, 4)), false, 0),
    (276, "/a??b/", "b", Some((0, 1)), false, 0),
    (277, "/a??b/", "ab", Some((0, 2)), false, 0),
    (389, "/(a)\\1/", "aa", Some((0, 2)), false, 0),
    (390, "/(a)\\1/", "ab", None, false, 0),
    (416, "/(?{n}a)\\p{n}/", "aa", Some((0, 2)), false, 0),
    (417, "/(?{n}a)\\p{n}/", "ab", None, false, 0),
    (454, "/abc(def)?=/", "abcdef", Some((0, 3)), false, 0),
    (455, "/abc(def)?=/", "abcxyz", None, false, 0),
    (460, "/abc(x)?!/", "abc", Some((0, 3)), false, 0),
    (461, "/abc(x)?!/", "abcx", None, false, 0),
    (466, "/(abc)?#3xyz/", "abcxyz", Some((3, 6)), false, 3),
    (467, "/(abc)?#3xyz/", "abcxyz", None, false, 0),
    (472, "/(z)?~1abc/", "xabc", Some((1, 4)), false, 1),
    (473, "/(x)?~1abc/", "xabc", None, false, 1),
    (479, "/\\babc\\b/", "abc", Some((0, 3)), false, 0),
    (480, "/\\babc\\b/", "abcd", None, false, 0),
    (481, "/\\babc\\b/", "xabc", None, false, 0),
    (482, "/\\babc\\b/", " abc ", Some((1, 4)), true, 0),
    (483, "/\\Babc/", "xabc", Some((1, 4)), true, 0),
    (488, "/[{Nd}]+/", "123", Some((0, 3)), false, 0),
    (489, "/[{L}]+/", "Hello", Some((0, 5)), false, 0),
    (490, "/[{Nd}]+/", "abc", None, false, 0),
    (576, "/a\\mz|c/", "abc", Some((2, 3)), true, 0),
    (581, "/a\\Mz|c/", "abc", Some((2, 3)), true, 0),
    (732, "/^abc/", "abcabc", Some((0, 3)), false, 0),
    (733, "/^abc/", "abcabc", None, false, 3),
    (735, "/^abc/", "xabc", None, true, 0),
    (736, "/^abc/", "abcx", Some((0, 3)), true, 0),
    (741, "/(^\\s*)(#)/", "  #x", Some((0, 3)), false, 0),
    (742, "/(^\\s*)(#)/", "x #", None, true, 1),
    (747, "/^abc/m", "x\nabc", Some((2, 5)), false, 2),
    (748, "/^abc/m", "x\nabc", Some((2, 5)), true, 1),
    (753, "/^abc|xyz/", "..xyz", Some((2, 5)), false, 2),
    (774, "/abc$/", "xxabc", Some((2, 5)), false, 2),
    (775, "/abc$/", "xxabc", None, false, 0),
    (776, "/[^\\\\\"]$/", "hello", Some((4, 5)), false, 4),
    (777, "/[^\\\\\"]$/", "hello", None, false, 0),
    (778, "/(abc)$/", "xabc", Some((1, 4)), false, 1),
    (779, "/(abc)$/", "xabc", None, false, 0),
    (785, "/$/m", "a\nb", Some((2, 2)), true, 0),
    (786, "/$/m", "a\nb", Some((2, 2)), false, 2),
    (791, "/.*$/", "abcd", Some((0, 4)), true, 0),
    (792, "/a.*$/", "a___", Some((0, 4)), true, 0),
    (797, "/abc$/", "xxabc", Some((2, 5)), true, 0),
    (798, "/x$/", "abcx", Some((3, 4)), true, 0),
    (799, "/[^\\\\\"]$/", "hello", Some((4, 5)), true, 0),
    (860, "/(^|[^\\\\]?#1)$/", "", Some((0, 0)), false, 0),
    (865, "/abc$|xyz/", "..xyz", Some((2, 5)), false, 2),
    (952, "/\\w+\\@\\w+/", "mail me at host", None, false, 0),
    (953, "/\\w+\\@\\w+/", "me@host", Some((0, 7)), false, 0),
    (961, "/(a|ab)/", "ab", Some((0, 1)), false, 0),
    (966, "/(a|)/", "a", Some((0, 1)), false, 0),
    (967, "/(a|)/", "b", Some((0, 0)), false, 0),
    (968, "/(a|)/", "", Some((0, 0)), false, 0),
    (969, "/(|a)/", "a", Some((0, 0)), false, 0),
    (974, "/(abc|xyz)/i", "ABC", Some((0, 3)), false, 0),
    (975, "/(abc|xyz)/i", "XyZ", Some((0, 3)), false, 0),
    (976, "/(abc|xyz)/i", "qrs", None, false, 0),
    (981, "/(a|b)/", "\u{e9}", None, false, 0),
    (982, "/(a|b)/", "\u{e9}a", Some((1, 2)), true, 0),
    (983, "/(\u{e9}|b)/", "\u{e9}", Some((0, 1)), false, 0),
    (984, "/(\u{e9}|b)/", "b", Some((0, 1)), false, 0),
    (989, "/(\\b|x)/", "a", Some((0, 0)), false, 0),
    (990, "/(\\bfoo|bar)/", "foo", Some((0, 3)), false, 0),
    (991, "/(\\bfoo|bar)/", "bar", Some((0, 3)), false, 0),
    (992, "/(x?=|a)/", "x", Some((0, 0)), false, 0),
    (993, "/(x?=|a)/", "a", Some((0, 1)), false, 0),
    (1010, "/((a|b)|c)/", "a", Some((0, 1)), false, 0),
    (1011, "/((a|b)|c)/", "b", Some((0, 1)), false, 0),
    (1012, "/((a|b)|c)/", "c", Some((0, 1)), false, 0),
    (1013, "/((a|b)|c)/", "d", None, false, 0),
];

#[test]
fn colorer_s_matches_come_out_as_in_colorer() {
    for &(line, pattern, text, expected, moves, pos) in CASES {
        let regex = Regex::new(pattern).unwrap_or_else(|e| panic!("line {line}: {pattern}: {e:?}"));
        let text: Vec<char> = text.chars().collect();
        let mut found = Matches::default();
        let how = Parse {
            pos,
            moves,
            ..Parse::default()
        };
        let matched = regex.parse(&text, &mut found, how);
        let got = matched.then_some((found.s[0], found.e[0]));
        assert_eq!(got, expected, "line {line}: {pattern} on {text:?}");
    }
}
