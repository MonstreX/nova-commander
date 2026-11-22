//! FAR's own specification of `CmpName` (`processname.cpp`, `Masks` and the `CmpName` test case), case for
//! case: the port is exact. FAR's table is Windows', names compared without case.

use super::*;

/// FAR's `CmpName(Mask, Src)` with its defaults (legacy mode), the mask folded as the masks are.
fn far(mask: &str, name: &str) -> bool {
    let pattern: Vec<char> = mask.chars().map(fold).collect();
    cmp_name(&pattern, name, MASKS)
}

/// FAR's `Masks` and, for each, its `Tests` group.
const TABLE: &[(&str, &[(&str, bool)])] = &[
    ("", &[("", true), ("non-empty", false)]),
    ("*", &[("", true), (".", true), ("whatever", true)]),
    (
        "A?Z*",
        &[
            ("", false),
            ("1", false),
            ("AZ", false),
            ("ALZ", true),
            ("ALZA1", true),
        ],
    ),
    (
        "*.txt",
        &[
            ("", false),
            ("foo.bar", false),
            ("foo.txt", true),
            (".txt", true),
            ("foo.txt1", false),
        ],
    ),
    (
        "*?.bak",
        &[
            ("", false),
            ("foo.bar", false),
            ("1.bak", true),
            ("foo.bak", true),
            ("foo.bak1", false),
        ],
    ),
    (
        "?????.?????",
        &[
            ("", false),
            ("12345.1234", false),
            ("12345.12345", true),
            ("1.234.123.4", true),
            ("...........", true),
            ("123456.12345", false),
        ],
    ),
    (
        "*_NEW.*",
        &[
            ("", false),
            ("1", false),
            ("_NEW", true),
            ("1_NEW", true),
            ("1_NEW.", true),
            ("1_NEW.2", true),
            ("1_NEW2", false),
        ],
    ),
    (
        "?x.????999.*rForTheCourse",
        &[
            ("", false),
            ("1", false),
            ("Rx.1234999.rForTheCourse", true),
            ("Rx.1234999.QrForTheCourse", true),
            ("Rx.999.rForTheCourse", false),
        ],
    ),
    (
        "*.*.2",
        &[
            ("", false),
            (".bar.2", true),
            ("..2", true),
            ("foo..2", true),
            ("foo.bar.2", true),
            ("foo.bar.", false),
        ],
    ),
    (
        "test.*",
        &[
            ("...txt", false),
            ("..txt", false),
            (".txt", false),
            ("a.txt", false),
            ("t.txt", false),
            ("test", true),
            ("test.", true),
            ("test..", true),
            ("test.b.txt", true),
            ("test.foo.bar.txt", true),
            ("test.md", true),
            ("test.txt", true),
        ],
    ),
    ("t*?.", NO_EXTENSION),
    ("t?*.", NO_EXTENSION),
    (
        "t*?.*",
        &[
            ("...txt", false),
            ("..txt", false),
            (".txt", false),
            ("a.txt", false),
            ("t.txt", true),
            ("test", true),
            ("test.", true),
            ("test..", true),
            ("test.b.txt", true),
            ("test.foo.bar.txt", true),
            ("test.md", true),
            ("test.txt", true),
        ],
    ),
    (
        "*.*.*.txt",
        &[
            ("...txt", true),
            ("..txt", false),
            (".txt", false),
            ("a.txt", false),
            ("t.txt", false),
            ("test", false),
            ("test.", false),
            ("test..", false),
            ("test.b.txt", false),
            ("TEST.FOO.BAR.TXT", true),
            ("test.md", false),
            ("test.txt", false),
        ],
    ),
    (
        "[a-cf]*.txt",
        &[
            ("a.txt", true),
            ("bc.txt", true),
            ("CDE.TXT", true),
            ("e.txt", false),
            ("f.txt", true),
            ("g.txt", false),
        ],
    ),
    (
        "*[a-cf].t[]x[]t",
        &[
            ("a.txt", true),
            ("BC.TXT", true),
            ("cde.txt", false),
            ("e.txt", false),
            ("f.txt", true),
            ("g.txt", false),
        ],
    ),
    ("t[est.txt", &[("test.txt", false), ("t[est.txt", true)]),
    (
        "*a*a*a*a*a*a*a*a*b",
        &[
            ("aaaaaaaaaaaaaaaaaab", true),
            ("aaaaaaaaaaaaaaaaaaaac", false),
        ],
    ),
    (
        "[t-]*",
        &[
            ("aaa.txt", false),
            ("t.txt", true),
            ("test", true),
            ("-.txt", true),
            ("t-test.txt", true),
            (".txt", false),
        ],
    ),
    (
        "*?**??*???*.??*?",
        &[
            ("12345.txt", false),
            ("123456.txt", true),
            ("12345678.txt", true),
            ("FOO.TEST.BAR.TXT", true),
            ("1.1.1.1.md", false),
            ("1.1.1.1.text", true),
        ],
    ),
];

/// The group FAR gives both `t*?.` and `t?*.`.
const NO_EXTENSION: &[(&str, bool)] = &[
    ("...txt", false),
    ("..txt", false),
    (".txt", false),
    ("a.txt", false),
    ("t.txt", false),
    ("test", true),
    ("test.", true),
    ("test..", true),
    ("test.b.txt", false),
    ("test.foo.bar.txt", false),
    ("test.md", false),
    ("test.txt", false),
];

#[test]
fn far_s_own_cmp_name_table_holds() {
    if NAMES_DIFFER_BY_CASE {
        // FAR's table compares without case (Windows); elsewhere names keep their case.
        return;
    }
    assert_eq!(TABLE.len(), 20, "FAR's 20 masks");
    for (mask, group) in TABLE {
        for (name, expected) in *group {
            assert_eq!(far(mask, name), *expected, "{mask:?} on {name:?}");
        }
    }
}

#[test]
fn a_mask_and_a_name_written_alike_match_whatever_their_letters() {
    // A letter whose lower case is two (İ) and a final sigma: the mask is folded as the name is.
    let mask = |text: &str| FileMask::parse(text).unwrap();
    assert!(mask("İ*").matches("İstanbul.txt"));
    assert!(mask("ΟΔΟΣ.*").matches("ΟΔΟΣ.txt"));
    assert!(mask("οδος.*").matches("ΟΔΟΣ.txt") != NAMES_DIFFER_BY_CASE);
}

#[test]
fn the_quick_search_is_far_s_without_the_legacy_endings_on_the_name_alone() {
    // `CmpName(..., true, false)`: no legacy mode, the path skipped, case ignored everywhere.
    let path = ["dev", "report.txt"].join(std::path::MAIN_SEPARATOR_STR);
    assert!(matches_prefix("abc", "ABCD.txt"));
    assert!(
        !matches_prefix("abc.", "abc"),
        "`.` at the end is no ending here"
    );
    assert!(matches_prefix("*.", "a.txt"));
    assert!(
        !matches_prefix("*.", "abc"),
        "`*.` takes names with a dot only"
    );
    assert!(matches_prefix("rep", &path), "the name after the path");
    assert!(!matches_prefix("dev", &path));
}
