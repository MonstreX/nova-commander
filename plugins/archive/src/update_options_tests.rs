use super::*;

fn settings() -> UpdateSettings {
    UpdateSettings {
        level: 9,
        method: "PPMD".to_string(),
        advanced: "em=AES128 x=3".to_string(),
        encrypt: false,
        show_password: true,
        move_files: true,
        ignore_errors: true,
        overwrite: Overwrite::Skip,
    }
}

/// arclite's `put_files` for an existing archive: its own method and encryption, the kept level and
/// answers, no move offered from the kept settings; F4's copy overwrites.
#[test]
fn an_archives_update_starts_as_arclites() {
    let options = UpdateOptions::for_archive(&settings(), ("deflate", true), false, true);
    assert_eq!(options.method, "Deflate", "7-Zip's name for it");
    assert_eq!(options.level, 9);
    assert!(options.encrypt && options.password.is_empty());
    assert_eq!(options.overwrite, Overwrite::Skip);
    assert!(!options.move_files, "arclite: `move != 0`, F5 here");
    assert!(options.ignore_errors && options.open_shared && options.show_password);
    let edited = UpdateOptions::for_archive(&settings(), ("PPMD", false), true, false);
    assert_eq!(edited.overwrite, Overwrite::Overwrite, "OPM_EDIT");
    assert_eq!(edited.method, "PPMD");
    assert!(!edited.encrypt);
}

/// A method arclite does not offer (Store, BZip2, AES's own, none) gives Deflate.
#[test]
fn a_method_not_written_gives_deflate() {
    for method in ["Store", "BZip2", "WzAES", ""] {
        let options = UpdateOptions::for_archive(&settings(), (method, false), false, false);
        assert_eq!(options.method, DEFLATE, "{method}");
    }
}

/// 7-Zip's `em=` picks the AES strength; none, or one not known, is AES-256.
#[test]
fn em_picks_the_aes_strength() {
    let mut options = UpdateOptions::for_archive(&settings(), ("Deflate", true), false, false);
    assert_eq!(options.aes_bits(), 128);
    options.advanced = "EM=aes192".to_string();
    assert_eq!(options.aes_bits(), 192);
    options.advanced = "em=ZipCrypto".to_string();
    assert_eq!(options.aes_bits(), 256, "the crate writes no ZipCrypto");
    options.advanced.clear();
    assert_eq!(options.aes_bits(), 256);
}

/// `em=` among other parameters, in any case; a parameter only ending in `em`, or `em` with no value, is
/// none.
#[test]
fn em_is_found_among_other_parameters() {
    let mut options = UpdateOptions::for_archive(&settings(), ("Deflate", true), false, false);
    for (advanced, bits) in [
        ("x=9 em=aes128 mt=2", 128),
        ("mt=on   EM=AES192", 192),
        ("mem=AES128", 256),
        ("em", 256),
        ("em=AES128x", 256),
    ] {
        options.advanced = advanced.to_string();
        assert_eq!(options.aes_bits(), bits, "{advanced}");
    }
}

/// The archive's method found whatever its case; the kept parameters and the shown password carried.
#[test]
fn the_archives_method_is_found_whatever_its_case() {
    for (method, name) in [
        ("lzma2", "LZMA2"),
        ("ppmd", "PPMD"),
        ("DEFLATE", "Deflate"),
        ("deflate64", "Deflate64"),
    ] {
        let options = UpdateOptions::for_archive(&settings(), (method, false), false, false);
        assert_eq!(options.method, name, "{method}");
        assert_eq!(options.advanced, "em=AES128 x=3");
        assert!(!options.open_shared);
    }
}

/// arclite's defaults: Normal, no method kept, the answer asked.
#[test]
fn the_defaults_are_arclites() {
    let defaults = UpdateSettings::default();
    assert_eq!(defaults.level, 5);
    assert!(defaults.method.is_empty() && !defaults.encrypt && !defaults.move_files);
    assert_eq!(defaults.overwrite, Overwrite::Ask);
    assert_eq!(LEVELS[3], ("Normal", 5));
    assert_eq!(DEFLATE, "Deflate");
}
