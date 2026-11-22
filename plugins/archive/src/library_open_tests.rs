//! Through 7-Zip's installed library and the archives of `testdata` (see the SDK's: `a.7z`, `a.tar.gz`,
//! `locked.7z` with the password "secret", `nested.tar` holding `a.7z`).

use super::*;

#[derive(Default)]
struct Asker {
    password: Option<String>,
}

impl Opening for Asker {
    fn password(&mut self) -> Option<String> {
        self.password.clone()
    }
}

fn with_file(bytes: &[u8], name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    (folder, path)
}

/// The chains `path` opens as, by their names.
fn names(path: &Path, every: bool, asker: &mut Asker) -> io::Result<Vec<String>> {
    let library = Library::find(path.parent().unwrap());
    let formats = library.formats();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let request = Request {
        library: &library,
        formats: &formats,
        enabled: &enabled,
        every,
    };
    let chains = open(&request, path, asker)?;
    Ok(chains.iter().map(|chain| chain.name(&formats)).collect())
}

/// A 7z opens as 7z whatever its name says: the extension's format is tried first and fails.
#[test]
fn a_7z_opens_whatever_its_name() {
    let (_folder, path) = with_file(include_bytes!("../testdata/a.7z"), "a.7z");
    assert_eq!(names(&path, false, &mut Asker::default()).unwrap(), ["7z"]);
    let (_folder, misnamed) = with_file(include_bytes!("../testdata/a.7z"), "a.zip");
    assert_eq!(
        names(&misnamed, false, &mut Asker::default()).unwrap(),
        ["7z"]
    );
}

/// After a stub (an SFX's): found at its signature's place.
#[test]
fn an_archive_after_a_stub_is_found() {
    let mut bytes = b"a stub of a self-extracting archive".to_vec();
    bytes.extend_from_slice(include_bytes!("../testdata/a.7z"));
    let (_folder, path) = with_file(&bytes, "setup.bin");
    let library = Library::find(path.parent().unwrap());
    let formats = library.formats();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let request = Request {
        library: &library,
        formats: &formats,
        enabled: &enabled,
        every: false,
    };
    let chains = open(&request, &path, &mut Asker::default()).unwrap();
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].parts[0].offset, 35);
    assert_eq!(chains[0].archive().count(), 3);
    assert!(chains[0].outer.is_empty());
}

/// A tar.gz is gzip's (its one item the tar); a file of no format none.
#[test]
fn a_tar_gz_is_gzips_and_text_is_none() {
    let (_folder, path) = with_file(include_bytes!("../testdata/a.tar.gz"), "a.tar.gz");
    assert_eq!(
        names(&path, false, &mut Asker::default()).unwrap(),
        ["gzip"]
    );
    let (_folder, text) = with_file(b"just some words, no archive at all", "notes.txt");
    assert!(
        names(&text, false, &mut Asker::default())
            .unwrap()
            .is_empty()
    );
}

/// The format menu's every way: a 7z among them.
#[test]
fn every_way_is_listed_for_the_menu() {
    let (_folder, path) = with_file(include_bytes!("../testdata/nested.tar"), "nested.tar");
    let every = names(&path, true, &mut Asker::default()).unwrap();
    assert_eq!(every[0], "tar", "{every:?}");
}

/// Encrypted headers: the password asked opens it; none stops the opening (not "no archive").
#[test]
fn encrypted_headers_ask_for_the_password() {
    let (_folder, path) = with_file(include_bytes!("../testdata/locked.7z"), "locked.7z");
    let mut asker = Asker {
        password: Some("secret".to_string()),
    };
    assert_eq!(names(&path, false, &mut asker).unwrap(), ["7z"]);
    let refused = names(&path, false, &mut Asker::default()).unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::Interrupted);
}

/// A chain's name joins its formats by arrows.
#[test]
fn a_chains_name_joins_its_formats() {
    let (_folder, path) = with_file(include_bytes!("../testdata/a.7z"), "a.7z");
    let library = Library::find(path.parent().unwrap());
    let formats = library.formats();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let request = Request {
        library: &library,
        formats: &formats,
        enabled: &enabled,
        every: false,
    };
    let mut chain = open(&request, &path, &mut Asker::default())
        .unwrap()
        .remove(0);
    let gzip = formats
        .iter()
        .position(|format| format.name == "gzip")
        .unwrap();
    chain.parts.insert(
        0,
        Entry {
            format: gzip,
            offset: 0,
        },
    );
    assert_eq!(chain.name(&formats), "gzip→7z");
    assert_eq!(chain.format(), chain.parts.last().map(|part| part.format));
}

/// An archive the library reads, as a panel holds it: its items and tree, never written by the zip
/// crate (`changeable` refuses it: its items would be lost), its files read through the library.
#[test]
fn a_library_archive_is_neither_written_nor_read_by_the_crate() {
    let (_folder, path) = with_file(include_bytes!("../testdata/a.7z"), "a.7z");
    let library = Library::find(path.parent().unwrap());
    let formats = library.formats();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let request = Request {
        library: &library,
        formats: &formats,
        enabled: &enabled,
        every: false,
    };
    let chain = open(&request, &path, &mut Asker::default())
        .unwrap()
        .remove(0);
    let format = ("7z".to_string(), "a");
    let mut opened = crate::opened::Opened::from_library(&path, chain, format, None).unwrap();
    assert_eq!(opened.items.len(), 3);
    let refused = crate::updating::changeable(&opened).unwrap_err();
    assert_eq!(
        refused.message,
        crate::updating::NOT_WRITTEN_YET,
        "a format not marked written (writes not set by the test)"
    );
    let top = opened
        .tree
        .find("/top.txt")
        .and_then(|node| opened.tree.nodes[node].item);
    // Its files read through the library (S6.3b): from the start and from inside, kept while read.
    let mut buffer = [0u8; 8];
    assert_eq!(opened.read(top.unwrap(), 0, &mut buffer).unwrap(), 3);
    assert_eq!(&buffer[..3], b"top");
    assert_eq!(opened.read(top.unwrap(), 1, &mut buffer[..1]).unwrap(), 1);
    assert_eq!(&buffer[..1], b"o");
    assert_eq!(opened.read(top.unwrap(), 9, &mut buffer).unwrap(), 0);
    let lines = opened.library().unwrap().1;
    assert_eq!(lines[0].data, path.display().to_string());
}

/// The chains `bytes` (a file named `name`) opens as: their names and their parts' offsets.
fn chains_of(bytes: &[u8], name: &str, every: bool) -> Vec<(String, Vec<u64>)> {
    let (_folder, path) = with_file(bytes, name);
    let library = Library::find(path.parent().unwrap());
    let formats = library.formats();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let request = Request {
        library: &library,
        formats: &formats,
        enabled: &enabled,
        every,
    };
    let chains = open(&request, &path, &mut Asker::default()).unwrap();
    (chains.iter())
        .map(|chain| {
            let offsets = chain.parts.iter().map(|part| part.offset).collect();
            (chain.name(&formats), offsets)
        })
        .collect()
}

fn named(name: &str, offsets: &[u64]) -> (String, Vec<u64>) {
    (name.to_string(), offsets.to_vec())
}

const SEVEN: &[u8] = include_bytes!("../testdata/a.7z");
const TAR_GZ: &[u8] = include_bytes!("../testdata/a.tar.gz");
const NESTED: &[u8] = include_bytes!("../testdata/nested.tar");

/// 7-Zip's SFX module with a 7z after it: the program (a `kPreArc` format) opens only with its tail
/// allowed, the 7z after it is tried past the program's size and is the one shown; the format menu has
/// both, whatever the file's name.
#[test]
fn a_program_with_an_archive_after_it_opens_as_the_archive() {
    let folder = Library::find(tempfile::tempdir().unwrap().path())
        .folder()
        .unwrap();
    let module = std::fs::read(folder.join("7z.sfx")).unwrap();
    let start = module.len() as u64;
    let bytes = [module, SEVEN.to_vec()].concat();
    for name in ["setup.exe", "setup.bin"] {
        assert_eq!(
            chains_of(&bytes, name, false),
            [named("7z", &[start])],
            "{name}"
        );
        assert_eq!(
            chains_of(&bytes, name, true),
            [named("PE", &[0]), named("7z", &[start])],
            "{name}"
        );
    }
}

/// An archive's place found: what is after it is tried from where it ends (the gzip after a 7z after a
/// stub); a signature inside it is not (the 7z inside a tar after a stub longer than the tar).
#[test]
fn what_an_archive_holds_is_not_tried_again() {
    let stub = b"a stub of a self-extracting archive";
    let both = [stub.as_slice(), SEVEN, TAR_GZ].concat();
    let after = stub.len() as u64;
    let end = after + SEVEN.len() as u64;
    assert_eq!(
        chains_of(&both, "x.7z", true),
        [named("7z", &[after]), named("gzip", &[end])]
    );
    assert_eq!(chains_of(&both, "x.7z", false), [named("7z", &[after])]);
    let tar = [vec![0; 2000], NESTED.to_vec()].concat();
    assert_eq!(chains_of(&tar, "stub.bin", true), [named("tar", &[2000])]);
}

/// A file whose extension's format is first and does not open at its start: that format's archive
/// found further in (here past the buffer searched) measured, and the signatures inside that size passed
/// over (arclite's `get_skip_header`).
#[test]
fn the_extensions_format_measured_passes_over_what_it_covers() {
    let mut bytes = vec![0u8; MAX_CHECK + 4096];
    bytes[100..100 + TAR_GZ.len()].copy_from_slice(TAR_GZ);
    bytes[400..400 + TAR_GZ.len()].copy_from_slice(TAR_GZ);
    let at = MAX_CHECK - 2;
    bytes[at..at + SEVEN.len()].copy_from_slice(SEVEN);
    assert_eq!(chains_of(&bytes, "far.7z", false), [named("gzip", &[400])]);
    assert_eq!(chains_of(&bytes, "far.bin", false), [named("gzip", &[100])]);
    assert_eq!(
        chains_of(&bytes, "far.bin", true),
        [named("gzip", &[100]), named("gzip", &[400])]
    );
}

/// A disk image naming its main file: the archive in it opened in turn (a chain), the deepest shown,
/// the image kept around it; the format menu has the image, the chain and (by its signature) the 7z.
#[test]
fn an_images_main_file_opens_as_a_chain() {
    let image = vhd(&[SEVEN.to_vec(), vec![0; 2 << 20]].concat());
    assert_eq!(
        chains_of(&image, "a.vhd", false),
        [named("VHD→7z", &[0, 0])]
    );
    assert_eq!(
        chains_of(&image, "a.vhd", true),
        [
            named("VHD", &[0]),
            named("VHD→7z", &[0, 0]),
            named("7z", &[0])
        ]
    );
    assert_eq!(
        chains_of(&image, "a.bin", true),
        [
            named("7z", &[0]),
            named("VHD", &[0]),
            named("VHD→7z", &[0, 0])
        ]
    );
    let (_folder, path) = with_file(&image, "a.vhd");
    let library = Library::find(path.parent().unwrap());
    let formats = library.formats();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let request = Request {
        library: &library,
        formats: &formats,
        enabled: &enabled,
        every: false,
    };
    let chain = open(&request, &path, &mut Asker::default())
        .unwrap()
        .remove(0);
    assert_eq!(chain.archive().count(), 3);
    assert_eq!(chain.outer.len(), 1);
    assert_eq!(chain.outer[0].count(), 1, "the image's one disk");
    let seven = formats.iter().position(|format| format.name == "7z");
    assert_eq!(chain.format(), seven);
}

/// A file longer than the start searched: a signature at the buffer's end (gzip's three bytes), its
/// quick look asking for more, is tried there.
#[test]
fn a_signature_at_the_searched_starts_end_is_tried() {
    let at = MAX_CHECK - 3;
    let bytes = [vec![0u8; at], TAR_GZ.to_vec()].concat();
    assert_eq!(
        chains_of(&bytes, "x.bin", false),
        [named("gzip", &[at as u64])]
    );
}

/// A program (`kPreArc`) is opened with its tail allowed only when tried first: after the extension's
/// format failed at the start it is not, and the archive after it is tried from the start on.
#[test]
fn a_program_not_tried_first_is_not_opened_with_its_tail() {
    let folder = Library::find(tempfile::tempdir().unwrap().path())
        .folder()
        .unwrap();
    let module = std::fs::read(folder.join("7z.sfx")).unwrap();
    let start = module.len() as u64;
    let bytes = [module, SEVEN.to_vec()].concat();
    assert_eq!(
        chains_of(&bytes, "setup.zip", true),
        [named("7z", &[start])]
    );
}

/// A fixed VHD of `data` (at least 4 KB): the data, then its footer.
fn vhd(data: &[u8]) -> Vec<u8> {
    let size = data.len().div_ceil(512).max(8) * 512;
    let mut bytes = data.to_vec();
    bytes.resize(size, 0);
    let mut footer = [0u8; 512];
    footer[..8].copy_from_slice(b"conectix");
    footer[8..12].copy_from_slice(&2u32.to_be_bytes());
    footer[12..16].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    footer[16..24].copy_from_slice(&u64::MAX.to_be_bytes());
    footer[40..48].copy_from_slice(&(size as u64).to_be_bytes());
    footer[48..56].copy_from_slice(&(size as u64).to_be_bytes());
    footer[60..64].copy_from_slice(&2u32.to_be_bytes());
    let sum: u32 = footer.iter().map(|&byte| u32::from(byte)).sum();
    footer[64..68].copy_from_slice(&(!sum).to_be_bytes());
    bytes.extend_from_slice(&footer);
    bytes
}
