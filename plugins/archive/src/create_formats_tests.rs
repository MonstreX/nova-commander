//! The formats offered, from formats made up here.

use super::*;

fn format(name: &str, ext: &str, updatable: bool, library: usize) -> Format {
    Format {
        name: name.to_string(),
        extensions: vec![ext.to_string()],
        updatable,
        library,
        ..Format::default()
    }
}

fn formats() -> Vec<Format> {
    vec![
        format("zip", "zip", true, 0),
        format("7z", "7z", true, 0),
        format("xz", "xz", true, 0),
        format("Rar", "rar", false, 0),
        format("tar", "tar", true, 0),
        format("gzip", "gz", true, 0),
        format("WIM", "wim", true, 0),
        format("Extra", "ext", true, 1),
        format("SWFc", "swf", true, 0),
    ]
}

fn names(archivers: &Archivers) -> Vec<&str> {
    (archivers.others.iter())
        .map(|archiver| archiver.name.as_str())
        .collect()
}

/// One file: 7z, zip and every other format written, by library then name; SWFc only for an
/// uncompressed Flash file; several files: no format of one file.
#[test]
fn the_formats_are_offered_as_arclites() {
    let folder = tempfile::tempdir().unwrap();
    let one = folder.path().join("a.txt");
    std::fs::write(&one, "text").unwrap();
    let offered = Archivers::of(&formats(), std::slice::from_ref(&one));
    assert_eq!(offered.seven.as_ref().unwrap().extension, ".7z");
    assert_eq!(names(&offered), ["gzip", "tar", "WIM", "xz", "Extra"]);
    let flash = folder.path().join("a.swf");
    std::fs::write(&flash, b"FWS\x0a\0\0\0\0").unwrap();
    let flashy = Archivers::of(&formats(), std::slice::from_ref(&flash));
    assert!(names(&flashy).contains(&"SWFc"));
    let several = Archivers::of(&formats(), &[one.clone(), flash]);
    assert_eq!(names(&several), ["tar", "WIM", "Extra"]);
    let folder_only = Archivers::of(&formats(), &[folder.path().to_path_buf()]);
    assert!(!names(&folder_only).contains(&"gzip"));
    assert_eq!(offered.extension("GZIP"), ".gz");
    assert_eq!(offered.extension("zip"), ".zip");
    assert_eq!(offered.extension("Rar"), "");
}

/// No library: zip only.
#[test]
fn without_a_library_zip_only() {
    let offered = Archivers::of(&[], &[]);
    assert_eq!(offered, Archivers::zip_only());
    assert!(offered.seven.is_none() && offered.others.is_empty());
    assert_eq!(offered.find("zip").unwrap().extension, ".zip");
}
