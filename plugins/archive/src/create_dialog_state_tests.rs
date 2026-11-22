//! The dialog's state without a host: the extension changed with the format, the items' state.

use super::*;

/// arclite's `change_extension`: the format's extension replaced, an `.exe` or `.001` after it counted
/// with it, whatever its case; a path not ending so kept.
#[test]
fn the_extension_follows_the_format() {
    assert_eq!(
        with_extension("work.7z", ".7z", ".zip").as_deref(),
        Some("work.zip")
    );
    assert_eq!(
        with_extension(r"C:\a.b\work.7Z", ".7z", ".tar").as_deref(),
        Some(r"C:\a.b\work.tar")
    );
    assert_eq!(
        with_extension("work.7z", ".7z", ".7z.001").as_deref(),
        Some("work.7z.001")
    );
    assert_eq!(
        with_extension("work.7z.001", ".7z.001", ".7z").as_deref(),
        Some("work.7z")
    );
    assert_eq!(with_extension("work.rar", ".7z", ".zip"), None);
    assert_eq!(with_extension(".7z", ".7z", ".zip"), None);
}

/// The items by the format: a 7z's method, solid and the header's encryption; a tar's none of them
/// and no encryption; a zip's no volumes; the other formats' list with its button.
#[test]
fn the_items_follow_the_format() {
    let state = |item: usize| match item {
        LEVEL => 4,
        ENCRYPT | ENABLE_VOLUMES | OTHER => 1,
        _ => 0,
    };
    let of = |format: &str| -> std::collections::HashMap<usize, (bool, bool)> {
        control_state(&state, format, false).into_iter().collect()
    };
    let seven = of("7z");
    assert_eq!(seven[&SOLID], (true, true));
    assert_eq!(seven[&ENCRYPT_HEADER], (true, true));
    assert_eq!(seven[&VOLUME_SIZE], (true, true));
    assert_eq!(seven[&OTHER_LIST], (true, true));
    let tar = of("tar");
    assert_eq!(tar[&METHOD], (false, true));
    assert_eq!(tar[&ENCRYPT], (false, true));
    assert_eq!(tar[&PASSWORD], (false, true));
    assert_eq!(tar[&ENABLE_VOLUMES], (true, true));
    let zip = of("zip");
    assert_eq!(zip[&METHOD], (true, true));
    assert_eq!(zip[&SOLID], (false, true));
    assert_eq!(zip[&ENABLE_VOLUMES], (true, true), "a zip split too");
    assert_eq!(zip[&VOLUME_SIZE], (true, true));
    assert_eq!(seven[&CREATE_SFX], (false, true), "split: no SFX");
    assert_eq!(zip[&CREATE_SFX], (false, true), "a 7z's only");
}

/// "Create self-extracting archive" checked for a 7z: its options enabled, the volumes not; unchecked
/// and not split: it enabled, its options not; for a zip the volumes whatever it says.
#[test]
fn a_self_extracting_7z_is_not_split() {
    let of = |format: &str, sfx: i32| -> std::collections::HashMap<usize, (bool, bool)> {
        let state = |item: usize| match item {
            LEVEL => 4,
            ENABLE_VOLUMES | SEVEN => 1,
            CREATE_SFX => sfx,
            _ => 0,
        };
        control_state(&state, format, false).into_iter().collect()
    };
    let sfx = of("7z", 1);
    assert_eq!(sfx[&CREATE_SFX], (true, true));
    assert_eq!(sfx[&SFX_OPTIONS], (true, true));
    assert_eq!(sfx[&ENABLE_VOLUMES], (false, true));
    assert_eq!(sfx[&VOLUME_SIZE], (false, true));
    let split = of("7z", 0);
    assert_eq!(split[&CREATE_SFX], (false, true));
    assert_eq!(split[&SFX_OPTIONS], (false, true));
    assert_eq!(split[&VOLUME_SIZE], (true, true));
    let zip = of("zip", 1);
    assert_eq!(zip[&ENABLE_VOLUMES], (true, true));
    assert_eq!(
        zip[&VOLUME_SIZE_LABEL],
        (false, true),
        "arclite's local unsplit"
    );
    assert_eq!(zip[&SFX_OPTIONS], (false, true));
}

/// The levels' and methods' lines.
#[test]
fn lines_are_found() {
    assert_eq!(level_at(9), 5);
    assert_eq!(level_at(4), 3, "Normal for one not listed");
    assert_eq!(method_line(&["LZMA", "PPMD"], "ppmd"), 1);
    assert_eq!(method_line(&["LZMA", "PPMD"], "Deflate"), 0);
    assert_eq!(wrapped("abcdef", 4), ["abcd", "ef"]);
}
