//! FAR's `GetPluginPrefixPath` on command lines: a prefix before the first `:`, never a drive's letter,
//! an empty one or a path from the root.

use super::command_prefix;

#[test]
fn the_text_before_the_first_colon_is_the_prefix() {
    assert_eq!(command_prefix("tmp:+menu list.temp"), Some("tmp"));
    assert_eq!(command_prefix("TMP:"), Some("TMP"));
    assert_eq!(
        command_prefix("tmp:<dir /b:a"),
        Some("tmp"),
        "the first colon"
    );
    assert_eq!(command_prefix("my plugin:x"), Some("my plugin"));
    assert_eq!(command_prefix("1:x"), Some("1"), "one character, no letter");
}

#[test]
fn a_drive_an_empty_prefix_a_root_path_or_no_colon_is_none() {
    for line in [
        "c:",
        "C:\\work",
        "d:dir",
        ":tmp",
        "\\\\server\\share:x",
        "/tmp:x",
        "dir /b",
        "",
    ] {
        assert_eq!(command_prefix(line), None, "{line}");
    }
}

#[cfg(windows)]
#[path = "create_tests.rs"]
mod create;
