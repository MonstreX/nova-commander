use super::*;

/// WinSCP's `MaskFileName`: no mask, `*` and `*.*` keep the name; a plain name renames; `*` and `?`
/// take from the name; the extension's part masks the extension.
#[test]
fn the_mask_renames_as_winscps() {
    assert_eq!(mask_file_name("report.txt", ""), "report.txt");
    assert_eq!(mask_file_name("report.txt", "*"), "report.txt");
    assert_eq!(mask_file_name("report.txt", "*.*"), "report.txt");
    assert_eq!(mask_file_name("report.txt", "new.doc"), "new.doc");
    assert_eq!(mask_file_name("report.txt", "*.bak"), "report.bak");
    // WinSCP's plain mask characters pass the name's: `old_` passes `repo`.
    assert_eq!(mask_file_name("report.txt", "old_*.*"), "old_rt.txt");
    assert_eq!(mask_file_name("report.txt", "??.*"), "re.txt");
    assert_eq!(
        mask_file_name("report", "*.bak"),
        "report.bak",
        "no extension"
    );
    assert_eq!(
        mask_file_name(".profile", "*.bak"),
        ".profile.bak",
        "a leading dot"
    );
    assert_eq!(
        mask_file_name("a.tar.gz", "*.zip"),
        "a.tar.zip",
        "the last dot"
    );
    assert_eq!(
        mask_file_name("report.txt", "name"),
        "name",
        "no dot in the mask"
    );
}

/// A plain mask character passes one of the name's; `\` makes `*` and `?` plain.
#[test]
fn plain_characters_pass_one_of_the_names() {
    assert_eq!(mask_file_name("abcdef", "x*"), "xbcdef");
    assert_eq!(mask_file_name("abc", "\\*?"), "*b");
    assert_eq!(mask_file_name("abc", "\\\\*"), "\\bc");
    assert_eq!(mask_file_name("ab", "????"), "ab", "nothing left");
}

/// Coming down: Windows' invalid characters as `%XX`, a `%` kept unless it reads as a token, a
/// trailing space or dot as its token, a reserved name with `%00`.
#[test]
fn a_remote_name_comes_down_as_a_windows_one() {
    assert_eq!(valid_local_file_name("plain.txt"), "plain.txt");
    assert_eq!(valid_local_file_name("a:b*c?.txt"), "a%3Ab%2Ac%3F.txt");
    assert_eq!(valid_local_file_name("back\\slash"), "back%5Cslash");
    assert_eq!(valid_local_file_name("<\"|>"), "%3C%22%7C%3E");
    assert_eq!(valid_local_file_name("100%"), "100%");
    assert_eq!(valid_local_file_name("50%off"), "50%off", "%of is no token");
    assert_eq!(
        valid_local_file_name("a%3Ab"),
        "a%253Ab",
        "a token's look kept"
    );
    assert_eq!(valid_local_file_name("name."), "name%2E");
    assert_eq!(valid_local_file_name("name "), "name%20");
    assert_eq!(valid_local_file_name("con"), "con%00");
    assert_eq!(valid_local_file_name("NUL.txt"), "NUL%00.txt");
    assert_eq!(valid_local_file_name("console"), "console");
}

/// Going up: the tokens read back, a `/` kept as its token, `%00` of a reserved name dropped.
#[test]
fn a_windows_name_goes_up_as_the_remote_one() {
    for name in [
        "a:b*c?.txt",
        "back\\slash",
        "<\"|>",
        "100%",
        "a%3Ab",
        "name.",
        "name ",
        "con",
        "NUL.txt",
        "plain.txt",
    ] {
        assert_eq!(restore_chars(&valid_local_file_name(name)), name, "{name}");
    }
    assert_eq!(restore_chars("a%2Fb"), "a%2Fb", "no remote name holds /");
    assert_eq!(restore_chars("x%20y"), "x%20y", "a space only at the end");
    assert_eq!(restore_chars("%2E"), "%2E", "a lone dot stays");
    assert_eq!(restore_chars("file%00"), "file%00", "not a reserved name");
    assert_eq!(restore_chars("50%"), "50%");
}

/// A server's name never leaves the disk's target folder, "Replace invalid characters" off or on; the
/// listings drop what is no item of a folder.
#[test]
fn a_remote_name_stays_in_its_folder() {
    let params = crate::copy_params::CopyParams {
        replace_invalid: false,
        ..crate::copy_params::CopyParams::default()
    };
    for name in ["..", ".", "..\\..\\evil.exe", "C:evil", "a/b", " .. "] {
        let changed = params.change_file_name(name, true, false);
        assert!(!walks_out(&changed), "{name} -> {changed}");
        assert_eq!(
            std::path::Path::new(&changed).components().count(),
            1,
            "{changed}"
        );
    }
    assert_eq!(
        params.change_file_name("a*b", true, false),
        "a*b",
        "the setting kept"
    );
    for name in ["", ".", "..", "a/b", "nul\0"] {
        assert!(!crate::fs::valid_name(name), "{name:?}");
    }
    assert!(crate::fs::valid_name("..x"));
    assert!(crate::fs::valid_name("back\\slash"));
}

/// What walks out of a folder on the disk: nothing but dots and spaces (Windows drops them at the end), a
/// drive's or a folder's separator, a NUL; dots or spaces with a name are a name.
#[test]
fn only_names_that_leave_their_folder_walk_out() {
    for name in [
        "", ".", "..", "...", " ", " . ", ". .", "a\\b", "a/b", "C:x", "x\0", "\\",
    ] {
        assert!(walks_out(name), "{name:?}");
    }
    for name in [
        "a", ".a", "a.", "..a", "a..", " a ", "a b", "a*b", "a?b", "a%3Ab",
    ] {
        assert!(!walks_out(name), "{name:?}");
    }
    let params = crate::copy_params::CopyParams {
        replace_invalid: false,
        ..crate::copy_params::CopyParams::default()
    };
    assert_eq!(params.change_file_name("a.", true, false), "a.");
    assert_ne!(params.change_file_name("...", true, false), "...");
}

/// The listings' rule (WinSCP's `IsValidFile`): only an empty name, "." and "..", or one with "/" or a NUL
/// is no item; dots, spaces and a disk's separators are the server's own names.
#[test]
fn a_listing_keeps_every_name_of_one_item() {
    for name in ["", ".", "..", "/", "a/", "/a", "a\0b", "\0"] {
        assert!(!crate::fs::valid_name(name), "{name:?}");
    }
    for name in ["...", " ", ". ", " ..", ".a", "a:b", "a\\b", "%2F"] {
        assert!(crate::fs::valid_name(name), "{name:?}");
    }
}

/// The dialog's target: its folder with its separator, then the mask.
#[test]
fn the_target_splits_into_the_folder_and_the_mask() {
    let split = |text: &str, remote| {
        let (folder, mask) = split_target(text, remote);
        (folder, mask)
    };
    assert_eq!(split(r"C:\out\", false), (r"C:\out\".into(), String::new()));
    assert_eq!(
        split(r"C:\out\*.bak", false),
        (r"C:\out\".into(), "*.bak".into())
    );
    assert_eq!(split("C:new.txt", false), ("C:".into(), "new.txt".into()));
    assert_eq!(split("name.txt", false), (String::new(), "name.txt".into()));
    assert_eq!(
        split("/home/user/", true),
        ("/home/user/".into(), String::new())
    );
    assert_eq!(split("/home/a:b", true), ("/home/".into(), "a:b".into()));
}
