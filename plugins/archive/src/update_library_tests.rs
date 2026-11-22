//! Writing through 7-Zip's installed library: new archives of the disk's files, then changed (a file
//! added, one replaced, one deleted), read back as the panel reads them.

use std::path::Path;

use nc_plugin_sdk::sevenzip::{Library, Opening};

use super::*;
use crate::extract::{Choice, FileInfo};
use crate::extract_options::Overwrite;
use crate::library_open::{Request, open};
use crate::tree::Tree;
use crate::update::{Existing, UpdateAsks, prepare, prepare_in, under_folder};
use crate::update_options::Kind;

/// Overwrites, stops when told (after so many looks), keeps what it saw.
#[derive(Default)]
struct Asks {
    stop_after: Option<usize>,
    progress: Vec<(u64, u64)>,
    faults: Vec<Fault>,
}

impl UpdateAsks for Asks {
    fn overwrite(
        &mut self,
        _path: &Path,
        _source: &FileInfo,
        _destination: &FileInfo,
    ) -> Option<(Overwrite, bool)> {
        Some((Overwrite::Overwrite, true))
    }

    fn fault(&mut self, fault: &Fault, _can_retry: bool) -> Choice {
        self.faults.push(fault.clone());
        Choice::Ignore
    }

    fn progress(&mut self, at: &UpdateAt<'_>) {
        self.progress.push((at.done, at.total));
    }

    fn stopped(&mut self) -> bool {
        match self.stop_after.as_mut() {
            Some(0) => true,
            Some(left) => {
                *left -= 1;
                false
            }
            None => false,
        }
    }
}

struct NoAsk;
impl Opening for NoAsk {}

fn options(format: &str) -> UpdateOptions {
    UpdateOptions {
        level: 5,
        method: "LZMA2".to_string(),
        advanced: String::new(),
        encrypt: false,
        show_password: false,
        password: String::new(),
        overwrite: Overwrite::Overwrite,
        move_files: false,
        ignore_errors: false,
        open_shared: true,
        kind: Kind {
            format: format.to_string(),
            ..Kind::default()
        },
    }
}

/// `path` read as the panel reads it.
fn opened(library: &Library, path: &Path) -> Opened {
    let formats = library.formats();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let request = Request {
        library,
        formats: &formats,
        enabled: &enabled,
        every: false,
    };
    let chain = open(&request, path, &mut NoAsk).unwrap().remove(0);
    let format = chain.name(&formats);
    Opened::from_library(path, chain, (format, "a"), None).unwrap()
}

/// Each file's path (`\` between parts) and data.
fn contents(opened: &mut Opened) -> Vec<(String, Vec<u8>)> {
    let mut read = Vec::new();
    for item in 0..opened.items.len() {
        if opened.items[item].is_dir {
            continue;
        }
        let mut buffer = vec![0; 4096];
        let count = opened.read(item, 0, &mut buffer).unwrap();
        buffer.truncate(count);
        read.push((opened.items[item].path.join("\\"), buffer));
    }
    read.sort();
    read
}

/// The plan of a new archive of `names` in `folder`.
fn new_plan(folder: &Path, names: &[&str], faults: &mut Faults<'_>) -> Plan {
    let sources: Vec<std::path::PathBuf> = names.iter().map(|name| folder.join(name)).collect();
    let (src_dir, names) = under_folder(&sources, Some(folder));
    let tree = Tree::new(&[]);
    let existing = Existing {
        tree: &tree,
        items: &[],
    };
    prepare_in(
        existing,
        "",
        (&src_dir, &names),
        Overwrite::Overwrite,
        faults,
    )
    .unwrap()
}

/// A folder of `docs\readme.txt` "hello" and `top.txt` "top".
fn sources() -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir(folder.path().join("docs")).unwrap();
    std::fs::write(folder.path().join("docs").join("readme.txt"), "hello").unwrap();
    std::fs::write(folder.path().join("top.txt"), "top").unwrap();
    folder
}

/// A new 7z of a folder and a file, written beside its place: its items read back, the library's
/// progress told.
#[test]
fn a_new_7z_is_written() {
    let folder = sources();
    let library = Library::find(folder.path());
    let mut asks = Asks::default();
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let plan = new_plan(folder.path(), &["docs", "top.txt"], &mut faults);
    let archive = folder.path().join("new.7z");
    let made = write_new(&library, &archive, &plan, &options("7z"), &mut faults).unwrap();
    let Made::Beside(temp) = made else {
        panic!("written beside")
    };
    std::fs::rename(&temp, &archive).unwrap();
    let mut read = opened(&library, &archive);
    assert_eq!(read.format, "7z");
    assert_eq!(
        contents(&mut read),
        [
            (r"docs\readme.txt".to_string(), b"hello".to_vec()),
            ("top.txt".to_string(), b"top".to_vec())
        ]
    );
    assert_eq!(asks.progress.last(), Some(&(8, 8)));
}

/// An archive there changed: a file of the name replaced, a new one added, one deleted; the other kept.
#[test]
fn a_7z_is_changed() {
    let folder = sources();
    let library = Library::find(folder.path());
    let mut asks = Asks::default();
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let plan = new_plan(folder.path(), &["docs", "top.txt"], &mut faults);
    let archive = folder.path().join("a.7z");
    let Made::Beside(temp) =
        write_new(&library, &archive, &plan, &options("7z"), &mut faults).unwrap()
    else {
        panic!("written beside")
    };
    std::fs::rename(&temp, &archive).unwrap();
    let more = tempfile::tempdir().unwrap();
    std::fs::write(more.path().join("top.txt"), "TOP").unwrap();
    std::fs::write(more.path().join("new.txt"), "new").unwrap();
    let base = opened(&library, &archive);
    let sources = [more.path().join("top.txt"), more.path().join("new.txt")];
    let (src_dir, names) = under_folder(&sources, Some(more.path()));
    let mut plan = prepare(
        &base,
        "",
        (&src_dir, &names),
        Overwrite::Overwrite,
        &mut faults,
    )
    .unwrap();
    let readme = (base.items.iter())
        .position(|item| item.path == ["docs", "readme.txt"])
        .unwrap();
    plan.removed.insert(readme);
    let temp = write(&base, &plan, &options("7z"), &mut faults).unwrap();
    drop(base);
    std::fs::rename(&temp, &archive).unwrap();
    let mut read = opened(&library, &archive);
    assert_eq!(
        contents(&mut read),
        [
            ("new.txt".to_string(), b"new".to_vec()),
            ("top.txt".to_string(), b"TOP".to_vec())
        ]
    );
}

/// Split into volumes of 1 KiB: written at their place, the first opening the whole (7-Zip's split
/// format), the data whole.
#[test]
fn a_new_archive_is_split_into_volumes() {
    let folder = tempfile::tempdir().unwrap();
    let data: Vec<u8> = (0..5000u32).map(|at| (at * 7919 % 251) as u8).collect();
    std::fs::write(folder.path().join("big.bin"), &data).unwrap();
    let library = Library::find(folder.path());
    let mut asks = Asks::default();
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let plan = new_plan(folder.path(), &["big.bin"], &mut faults);
    let archive = folder.path().join("v.tar.001");
    let mut split = options("tar");
    split.level = 0;
    split.kind.volume_size = Some(1024);
    let made = write_new(&library, &archive, &plan, &split, &mut faults).unwrap();
    assert_eq!(made, Made::InVolumes);
    assert!(folder.path().join("v.tar.005").exists());
    assert_eq!(std::fs::metadata(&archive).unwrap().len(), 1024);
    let mut read = opened(&library, &archive);
    assert_eq!(read.format, "Split→tar");
    let mut buffer = vec![0; 8000];
    let count = read.read(0, 0, &mut buffer).unwrap();
    assert_eq!(&buffer[..count], data.as_slice());
}

/// A self-extracting 7z: the installed 7-Zip's `7z.sfx` first, the 7z after it opened as one; a module
/// that cannot be made (its icon not there) leaves nothing.
#[test]
fn a_new_7z_is_written_after_its_module() {
    let folder = sources();
    let library = Library::find(folder.path());
    let modules = library.sfx_modules();
    let module = (modules.iter())
        .find(|path| path.ends_with("7z.sfx"))
        .expect("7-Zip's 7z.sfx installed");
    let mut asks = Asks::default();
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let plan = new_plan(folder.path(), &["top.txt"], &mut faults);
    let archive = folder.path().join("new.7z.exe");
    let mut sfx = options("7z");
    sfx.kind.sfx = Some(crate::sfx::Sfx {
        module: module.clone(),
        options: crate::sfx::SfxOptions::default(),
    });
    let Made::Beside(temp) = write_new(&library, &archive, &plan, &sfx, &mut faults).unwrap()
    else {
        panic!("written beside")
    };
    std::fs::rename(&temp, &archive).unwrap();
    let bytes = std::fs::read(&archive).unwrap();
    let stub = std::fs::read(module).unwrap();
    assert!(bytes.starts_with(&stub));
    assert_eq!(&bytes[stub.len()..stub.len() + 2], b"7z");
    let mut read = opened(&library, &archive);
    assert_eq!(
        contents(&mut read),
        [("top.txt".to_string(), b"top".to_vec())]
    );
    drop(read);
    let mut broken = sfx.clone();
    if let Some(sfx) = &mut broken.kind.sfx {
        sfx.options.replace_icon = true;
        sfx.options.icon_path = folder.path().join("none.ico").display().to_string();
    }
    let failed = write_new(
        &library,
        &folder.path().join("b.exe"),
        &plan,
        &broken,
        &mut faults,
    );
    assert!(matches!(failed, Err(Stop::Failed(_))), "{failed:?}");
    let left: Vec<_> = (std::fs::read_dir(folder.path()).unwrap())
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "tmp"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

/// A stop leaves nothing of the new archive; a source that cannot be opened, let be, is left out.
#[test]
fn a_stop_leaves_nothing_and_a_source_let_be_is_left_out() {
    let folder = sources();
    let library = Library::find(folder.path());
    let mut asks = Asks {
        stop_after: Some(0),
        ..Asks::default()
    };
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let plan = new_plan(folder.path(), &["top.txt"], &mut faults);
    let archive = folder.path().join("stopped.7z");
    let stopped = write_new(&library, &archive, &plan, &options("7z"), &mut faults);
    assert_eq!(stopped, Err(Stop::Abort));
    let left: Vec<_> = (std::fs::read_dir(folder.path()).unwrap())
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "tmp"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    let mut asks = Asks::default();
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let mut plan = new_plan(folder.path(), &["top.txt", "docs"], &mut faults);
    std::fs::remove_file(folder.path().join("top.txt")).unwrap();
    plan.skipped = false;
    let Made::Beside(temp) =
        write_new(&library, &archive, &plan, &options("7z"), &mut faults).unwrap()
    else {
        panic!("written beside")
    };
    std::fs::rename(&temp, &archive).unwrap();
    assert_eq!(faults.log.len(), 1);
    let mut read = opened(&library, &archive);
    assert_eq!(
        contents(&mut read),
        [(r"docs\readme.txt".to_string(), b"hello".to_vec())]
    );
}

/// The plan's items as 7-Zip numbers them.
#[test]
fn entries_keep_replace_delete_and_add() {
    let added = |name: &str| Added {
        path: vec![name.to_string()],
        source: name.into(),
        is_dir: false,
        size: 0,
        modified: None,
        created: None,
        accessed: None,
        attributes: 0x20,
    };
    let mut plan = Plan {
        added: vec![added("a"), added("b"), added("c")],
        ..Plan::default()
    };
    plan.replaced.insert(1, 1);
    plan.removed.insert(2);
    assert_eq!(
        entries(4, &plan),
        [
            Entry::Old(0),
            Entry::Replaced(1, 1),
            Entry::Old(3),
            Entry::New(0),
            Entry::New(2)
        ]
    );
    let one = added("x");
    assert_eq!(added_property(&one, prop::PATH), Prop::Text("x".into()));
    assert_eq!(added_property(&one, prop::ATTRIBUTES), Prop::U32(0x20));
    assert_eq!(added_property(&one, prop::MODIFIED), Prop::Empty);
}

#[path = "update_library_more_tests.rs"]
mod more;
