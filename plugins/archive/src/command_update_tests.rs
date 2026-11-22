//! The methods and formats `arc:c` / `arc:u` take, without the library.

use super::*;

fn format(name: &str, ext: &str) -> Format {
    Format {
        name: name.to_string(),
        extensions: vec![ext.to_string()],
        updatable: true,
        ..Format::default()
    }
}

/// The methods `-m` names as each format's writer writes them, `copy` stored, one not written refused.
#[test]
fn methods_are_the_writers() {
    assert_eq!(method_named("ppmd").unwrap().as_deref(), Some("PPMD"));
    assert_eq!(method_named("COPY").unwrap(), None);
    assert_eq!(
        method_named("lzma").unwrap().as_deref(),
        Some("LZMA"),
        "a zip's too"
    );
    assert_eq!(method_named("lzma2").unwrap().as_deref(), Some("LZMA2"));
    assert_eq!(
        method_named("Deflate64").unwrap().as_deref(),
        Some("Deflate64")
    );
    assert!(method_named("bzip2").is_err());
}

/// A new archive's format by `-t`, the profile, else its extension (the library's formats' first of
/// it; with none, zip for `.zip` and 7z else); none: the profile's (7z).
#[test]
fn formats_as_arclite_tells_them() {
    let command = UpdateCommand::default();
    let profile = ProfileOptions::default();
    let named = |command: &UpdateCommand, archive: &str, formats: &[Format]| {
        new_format(command, &profile, Path::new(archive), formats)
    };
    assert_eq!(named(&command, r"C:\a.ZIP", &[]), "zip");
    assert_eq!(named(&command, r"C:\a.7z", &[]), "7z");
    assert_eq!(
        named(&command, r"C:\backup", &[]),
        "7z",
        "no extension: arclite's 7z"
    );
    let typed = UpdateCommand {
        format: Some("Zip".to_string()),
        ..UpdateCommand::default()
    };
    assert_eq!(named(&typed, r"C:\a.7z", &[]), "Zip");
    let profiled = UpdateCommand {
        profile: Some("mine".to_string()),
        ..UpdateCommand::default()
    };
    assert_eq!(
        named(&profiled, r"C:\a.zip", &[]),
        "7z",
        "the profile's (arclite's default)"
    );
    let formats = [
        format("zip", "zip"),
        format("tar", "tar"),
        format("gzip", "gz"),
    ];
    assert_eq!(named(&command, r"C:\a.TAR", &formats), "tar");
    assert_eq!(named(&command, r"C:\a.gz", &formats), "gzip");
    assert_eq!(named(&command, r"C:\a.xyz", &formats), "7z");
}

/// `-eh`'s three answers; none given: the profile's.
#[test]
fn the_headers_answer() {
    assert_eq!(header_of(Some(Tri::True), None), Some(true));
    assert_eq!(header_of(Some(Tri::False), Some(true)), Some(false));
    assert_eq!(header_of(Some(Tri::Undef), Some(true)), None);
    assert_eq!(header_of(None, Some(false)), Some(false));
}
