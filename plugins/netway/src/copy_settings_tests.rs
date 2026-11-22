//! The transfer settings at work against the tests' server (S6.5c).

use super::*;
use crate::copy_params::{NameCase, TransferMode};
use crate::test_server::rights_of;

fn with(params: CopyParams) -> Setup {
    Setup {
        params,
        ..Setup::default()
    }
}

/// The text mode: LF as CRLF down, CRLF as LF up; the automatic mode by the masks.
#[test]
fn the_text_mode_turns_the_lines_both_ways() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    fs::write(srv.join("a.txt"), "x\ny\r\nz\n").unwrap();
    fs::write(srv.join("b.bin"), "1\n2\n").unwrap();
    let up = folders.out().join("up.txt");
    fs::write(&up, "p\r\nq\r\n").unwrap();
    let session = folders.session();
    let text = CopyParams {
        transfer_mode: TransferMode::Text,
        ..CopyParams::default()
    };
    let down = Work::Down("/srv", names(&["a.txt"]), folders.out());
    assert_eq!(
        run(&session, with(text.clone()), down, Vec::new()).0,
        Ok(())
    );
    let got = fs::read_to_string(folders.out().join("a.txt")).unwrap();
    assert_eq!(got, "x\r\ny\r\nz\r\n");
    let work = Work::Up(vec![up], "/srv");
    assert_eq!(run(&session, with(text), work, Vec::new()).0, Ok(()));
    assert_eq!(fs::read_to_string(srv.join("up.txt")).unwrap(), "p\nq\n");
    let automatic = CopyParams {
        transfer_mode: TransferMode::Automatic,
        ..CopyParams::default()
    };
    let down = Work::Down("/srv", names(&["b.bin"]), folders.out());
    assert_eq!(run(&session, with(automatic), down, Vec::new()).0, Ok(()));
    let binary = fs::read_to_string(folders.out().join("b.bin")).unwrap();
    assert_eq!(binary, "1\n2\n", "not a text file's mask: as it is");
}

/// The file mask takes the files it matches, a folder excluded with all it holds.
#[test]
fn the_file_mask_chooses_what_goes() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    fs::write(srv.join("docs/a.txt"), "a").unwrap();
    fs::write(srv.join("docs/b.log"), "b").unwrap();
    fs::write(srv.join("docs/deep/c.txt"), "c").unwrap();
    let session = folders.session();
    let only_text = CopyParams {
        include_mask: "*.txt".to_string(),
        ..CopyParams::default()
    };
    let down = || Work::Down("/srv", names(&["docs"]), folders.out());
    assert_eq!(run(&session, with(only_text), down(), Vec::new()).0, Ok(()));
    let out = folders.out().join("docs");
    assert!(out.join("a.txt").exists() && out.join("deep/c.txt").exists());
    assert!(!out.join("b.log").exists(), "not matched");
    fs::remove_dir_all(&out).unwrap();
    let no_deep = CopyParams {
        include_mask: "| deep/".to_string(),
        ..CopyParams::default()
    };
    assert_eq!(run(&session, with(no_deep), down(), Vec::new()).0, Ok(()));
    assert!(out.join("b.log").exists() && !out.join("deep").exists());
}

/// The names' case changes on both levels; the first level's masked first.
#[test]
fn names_change_their_case() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    fs::write(srv.join("docs/Mixed.Txt"), "m").unwrap();
    let session = folders.session();
    let upper = CopyParams {
        name_case: NameCase::Upper,
        ..CopyParams::default()
    };
    let down = Work::Down("/srv", names(&["docs"]), folders.out());
    assert_eq!(run(&session, with(upper), down, Vec::new()).0, Ok(()));
    let listed: Vec<String> = fs::read_dir(folders.out().join("DOCS"))
        .unwrap()
        .map(|item| item.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(listed.contains(&"MIXED.TXT".to_string()), "{listed:?}");
}

/// "Set permissions": a file's rights, a folder's with X where it reads; a refusal asked about, or
/// passed by with "Ignore permission errors".
#[test]
fn uploads_set_their_rights_and_ignore_refusals_when_asked() {
    let folders = Folders::new();
    let up = folders.out().join("up");
    fs::create_dir_all(&up).unwrap();
    fs::write(up.join("a.txt"), "a").unwrap();
    let locked = folders.out().join("locked.txt");
    fs::write(&locked, "l").unwrap();
    let session = folders.session();
    let rights = CopyParams {
        preserve_rights: true,
        rights: 0o640,
        ..CopyParams::default()
    };
    let work = Work::Up(vec![up], "/srv");
    assert_eq!(
        run(&session, with(rights.clone()), work, Vec::new()).0,
        Ok(())
    );
    let srv = folders.root().join("srv");
    assert_eq!(rights_of(&srv.join("up/a.txt")), Some(0o640));
    assert_eq!(rights_of(&srv.join("up")), Some(0o750), "X where it reads");
    let skip = vec![TransferReply::Failed(Failed::Skip)];
    let work = Work::Up(vec![locked.clone()], "/srv");
    let (done, seen) = run(&session, with(rights.clone()), work, skip);
    assert_eq!(done, Ok(()));
    let [TransferQuestion::Failed(lines)] = &seen[..] else {
        panic!("{seen:?}");
    };
    assert!(
        lines
            .join(" ")
            .contains("error occurred while setting the permissions")
    );
    let ignoring = CopyParams {
        ignore_perm_errors: true,
        ..rights
    };
    let work = Work::Up(vec![locked], "/srv");
    let yes = vec![TransferReply::Overwrite(Overwrite::Yes)];
    let (done, seen) = run(&session, with(ignoring), work, yes);
    assert_eq!(done, Ok(()));
    assert!(
        matches!(&seen[..], [TransferQuestion::Overwrite { .. }]),
        "only the file there asked over, the refusal passed by: {seen:?}"
    );
}

/// "Preserve read-only": a server's file without the owner's write right comes down read only.
#[test]
fn a_file_without_the_owners_write_comes_down_read_only() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    fs::write(srv.join("ro.txt"), "r").unwrap();
    fs::write(srv.join("rw.txt"), "w").unwrap();
    let session = folders.session();
    let attrs = russh_sftp::protocol::FileAttributes {
        permissions: Some(0o444),
        ..russh_sftp::protocol::FileAttributes::empty()
    };
    (session.runtime)
        .wait(session.sftp.set_metadata("/srv/ro.txt", attrs), &|| false)
        .unwrap()
        .unwrap();
    let keep = CopyParams {
        preserve_read_only: true,
        ..CopyParams::default()
    };
    let down = Work::Down("/srv", names(&["ro.txt", "rw.txt"]), folders.out());
    assert_eq!(run(&session, with(keep), down, Vec::new()).0, Ok(()));
    let ro = folders.out().join("ro.txt");
    assert!(fs::metadata(&ro).unwrap().permissions().readonly());
    assert!(
        !fs::metadata(folders.out().join("rw.txt"))
            .unwrap()
            .permissions()
            .readonly()
    );
    // The temporary folder must go: the read-only bit off again.
    nc_plugin_sdk::system::set_attributes(&ro, 0).unwrap();
}

/// "Clear 'Archive' attribute": a source sent and not moved loses it.
#[cfg(windows)]
#[test]
fn an_uploads_source_loses_its_archive_attribute() {
    use std::os::windows::fs::MetadataExt;
    let folders = Folders::new();
    let source = folders.out().join("a.txt");
    fs::write(&source, "a").unwrap();
    nc_plugin_sdk::system::set_attributes(&source, 0x20).unwrap();
    let session = folders.session();
    let clear = CopyParams {
        clear_archive: true,
        ..CopyParams::default()
    };
    let work = Work::Up(vec![source.clone()], "/srv");
    assert_eq!(run(&session, with(clear), work, Vec::new()).0, Ok(()));
    assert_eq!(fs::metadata(&source).unwrap().file_attributes() & 0x20, 0);
}

/// The speed limit: a file goes no faster than it.
#[test]
fn the_speed_limit_holds_a_file_back() {
    let folders = Folders::new();
    fs::write(folders.root().join("srv/big.bin"), vec![7u8; 64 * 1024]).unwrap();
    let session = folders.session();
    let slow = CopyParams {
        speed_limit: 128 * 1024,
        ..CopyParams::default()
    };
    let started = std::time::Instant::now();
    let down = Work::Down("/srv", names(&["big.bin"]), folders.out());
    assert_eq!(run(&session, with(slow), down, Vec::new()).0, Ok(()));
    let took = started.elapsed();
    assert!(took >= std::time::Duration::from_millis(400), "{took:?}");
    assert_eq!(
        fs::read(folders.out().join("big.bin")).unwrap().len(),
        64 * 1024
    );
}
