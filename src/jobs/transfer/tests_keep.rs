//! What a copy keeps besides the contents (ADR-0009 §9). ACLs are set only on files and folders these
//! tests make themselves in a temporary folder.

#![cfg(windows)]

use std::fs::{self, File, FileTimes};
use std::os::windows::fs::{FileTimesExt, MetadataExt};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::tests::{Script, request};
use super::*;
use crate::platform::files::read_security;
use crate::vfs::test_tree as tree;

const HIDDEN: u32 = 0x2;

/// The well-known SID of "Everyone": the name is translated, the SID is not.
const EVERYONE: &str = "*S-1-1-0";

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

const CREATED: u64 = 1_000_000_000;
const ACCESSED: u64 = 1_100_000_000;
const MODIFIED: u64 = 1_200_000_000;
const CHANGED: u64 = 1_300_000_000;

fn set_old_times(path: &Path) {
    let file = fs::OpenOptions::new()
        .write(true)
        .custom_flags_for_folders()
        .open(path)
        .unwrap();
    let times = FileTimes::new()
        .set_created(at(CREATED))
        .set_accessed(at(ACCESSED))
        .set_modified(at(MODIFIED));
    File::set_times(&file, times).unwrap();
    drop(file);
    let changed = crate::platform::files::FileTimes {
        changed: Some(at(CHANGED)),
        ..crate::platform::files::FileTimes::default()
    };
    crate::platform::files::apply_kept(path, None, Some(&changed))
        .times
        .unwrap();
}

fn change_time(path: &Path) -> Option<SystemTime> {
    crate::platform::files::change_time(path, false)
}

trait ForFolders {
    fn custom_flags_for_folders(&mut self) -> &mut Self;
}

impl ForFolders for fs::OpenOptions {
    /// `FILE_FLAG_BACKUP_SEMANTICS`: folders open too.
    fn custom_flags_for_folders(&mut self) -> &mut Self {
        std::os::windows::fs::OpenOptionsExt::custom_flags(self, 0x0200_0000)
    }
}

fn icacls(path: &Path, arguments: &[&str]) {
    let done = Command::new("icacls")
        .arg(path)
        .args(arguments)
        .output()
        .unwrap();
    assert!(done.status.success(), "{done:?}");
}

/// An access rule of its own that a new file would not get.
fn grant_everyone(path: &Path) {
    icacls(path, &["/grant", &format!("{EVERYONE}:R")]);
}

fn keeping(mut request: Request, keep: Keep) -> Request {
    request.keep = keep;
    request
}

fn timestamps() -> Keep {
    Keep {
        timestamps: true,
        ..Keep::default()
    }
}

fn rights(rights: Rights) -> Keep {
    Keep {
        rights,
        ..Keep::default()
    }
}

fn copied(temp: &Path, name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let source = temp.join(name);
    let target = temp.join("target");
    fs::create_dir_all(&target).unwrap();
    (source, target)
}

fn file_with_old_times(temp: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let (source, target) = copied(temp, "a.txt");
    fs::write(&source, "text").unwrap();
    set_old_times(&source);
    (source, target)
}

fn clean(report: &Report) {
    assert_eq!(report, &Report::default());
}

#[test]
fn all_timestamps_are_kept_when_asked() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = file_with_old_times(temp.path());
    let plan = keeping(
        request(&[source], &target, false, Existing::Ask),
        timestamps(),
    );

    clean(&run(&plan, &mut Script::default()));

    let copy = fs::metadata(target.join("a.txt")).unwrap();
    assert_eq!(copy.created().unwrap(), at(CREATED));
    assert_eq!(copy.modified().unwrap(), at(MODIFIED));
    assert_eq!(copy.accessed().unwrap(), at(ACCESSED));
    assert_eq!(change_time(&target.join("a.txt")), Some(at(CHANGED)));
}

#[test]
fn without_the_box_only_the_last_write_time_comes_along() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = file_with_old_times(temp.path());

    clean(&run(
        &request(&[source], &target, false, Existing::Ask),
        &mut Script::default(),
    ));

    let copy = fs::metadata(target.join("a.txt")).unwrap();
    assert_eq!(copy.modified().unwrap(), at(MODIFIED));
    assert_ne!(copy.created().unwrap(), at(CREATED));
}

#[test]
fn a_read_only_copy_gets_its_times_too() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = file_with_old_times(temp.path());
    let mut read_only = fs::metadata(&source).unwrap().permissions();
    read_only.set_readonly(true);
    fs::set_permissions(&source, read_only).unwrap();
    let plan = keeping(
        request(&[source], &target, false, Existing::Ask),
        timestamps(),
    );

    clean(&run(&plan, &mut Script::default()));

    assert_eq!(
        fs::metadata(target.join("a.txt"))
            .unwrap()
            .created()
            .unwrap(),
        at(CREATED)
    );
}

#[test]
fn a_moved_file_across_volumes_keeps_its_times_and_goes() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = file_with_old_times(temp.path());
    let plan = keeping(
        request(std::slice::from_ref(&source), &target, true, Existing::Ask),
        timestamps(),
    );
    let mut host = Script {
        cross_volume: true,
        ..Script::default()
    };

    clean(&run(&plan, &mut host));

    assert!(!source.exists());
    assert_eq!(
        fs::metadata(target.join("a.txt"))
            .unwrap()
            .created()
            .unwrap(),
        at(CREATED)
    );
}

#[test]
fn a_replaced_file_gets_the_times_of_the_new_one() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = file_with_old_times(temp.path());
    fs::write(target.join("a.txt"), "old").unwrap();
    let plan = keeping(
        request(&[source], &target, false, Existing::Overwrite),
        timestamps(),
    );

    clean(&run(&plan, &mut Script::default()));

    let copy = fs::metadata(target.join("a.txt")).unwrap();
    assert_eq!(copy.created().unwrap(), at(CREATED));
    assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "text");
}

#[test]
fn a_folder_gets_its_times_after_what_went_into_it() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = copied(temp.path(), "d");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("f"), "x").unwrap();
    set_old_times(&source);
    let plan = keeping(
        request(&[source], &target, false, Existing::Ask),
        timestamps(),
    );

    clean(&run(&plan, &mut Script::default()));

    let folder = fs::metadata(target.join("d")).unwrap();
    assert_eq!(folder.modified().unwrap(), at(MODIFIED));
    assert_eq!(folder.created().unwrap(), at(CREATED));
}

#[test]
fn without_the_box_a_folder_is_new() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = copied(temp.path(), "d");
    fs::create_dir(&source).unwrap();
    set_old_times(&source);

    clean(&run(
        &request(&[source], &target, false, Existing::Ask),
        &mut Script::default(),
    ));

    assert_ne!(
        fs::metadata(target.join("d")).unwrap().modified().unwrap(),
        at(MODIFIED)
    );
}

#[test]
fn a_new_folder_takes_the_attributes_of_its_source() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = copied(temp.path(), "d");
    fs::create_dir(&source).unwrap();
    let hidden = Command::new("attrib")
        .arg("+h")
        .arg(&source)
        .status()
        .unwrap();
    assert!(hidden.success());

    clean(&run(
        &request(&[source], &target, false, Existing::Ask),
        &mut Script::default(),
    ));

    let attributes = fs::metadata(target.join("d")).unwrap().file_attributes();
    assert_ne!(attributes & HIDDEN, 0);
}

#[test]
fn a_folder_merged_into_keeps_its_own_times() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = copied(temp.path(), "d");
    fs::create_dir(&source).unwrap();
    set_old_times(&source);
    fs::create_dir(target.join("d")).unwrap();
    let plan = keeping(
        request(&[source], &target, false, Existing::Ask),
        timestamps(),
    );

    clean(&run(&plan, &mut Script::default()));

    assert_ne!(
        fs::metadata(target.join("d")).unwrap().created().unwrap(),
        at(CREATED)
    );
}

#[test]
fn rights_are_copied_when_asked_and_not_otherwise() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = copied(temp.path(), "a.txt");
    fs::write(&source, "text").unwrap();
    grant_everyone(&source);
    let (copy_to, default_to) = (target.join("copy"), target.join("default"));
    fs::create_dir(&copy_to).unwrap();
    fs::create_dir(&default_to).unwrap();
    let wanted = read_security(&source).unwrap();

    let plan = keeping(
        request(
            std::slice::from_ref(&source),
            &copy_to,
            false,
            Existing::Ask,
        ),
        rights(Rights::Copy),
    );
    clean(&run(&plan, &mut Script::default()));
    let plan = request(&[source], &default_to, false, Existing::Ask);
    clean(&run(&plan, &mut Script::default()));

    let rules = |path: &Path| read_security(path).unwrap().rules().to_vec();
    assert_eq!(rules(&copy_to.join("a.txt")), wanted.rules());
    assert_ne!(rules(&default_to.join("a.txt")), wanted.rules());
}

#[test]
fn a_followed_link_gets_the_rights_of_what_it_leads_to() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = copied(temp.path(), "real");
    fs::create_dir(&real).unwrap();
    fs::write(real.join("f"), "x").unwrap();
    grant_everyone(&real);
    let link = temp.path().join("link");
    let made = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&real)
        .output()
        .unwrap();
    assert!(made.status.success(), "{made:?}");
    let wanted = read_security(&real).unwrap();
    let plan = keeping(
        request(&[link], &target, false, Existing::Ask),
        Keep {
            rights: Rights::Copy,
            link_contents: true,
            ..Keep::default()
        },
    );

    clean(&run(&plan, &mut Script::default()));

    let copy = target.join("link");
    assert!(!fs::symlink_metadata(&copy).unwrap().is_symlink());
    assert_eq!(read_security(&copy).unwrap().rules(), wanted.rules());
}

#[test]
fn a_folder_gets_the_rights_of_its_source() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = copied(temp.path(), "d");
    fs::create_dir(&source).unwrap();
    grant_everyone(&source);
    let wanted = read_security(&source).unwrap();
    let plan = keeping(
        request(&[source], &target, false, Existing::Ask),
        rights(Rights::Copy),
    );

    clean(&run(&plan, &mut Script::default()));

    assert_eq!(
        read_security(&target.join("d")).unwrap().rules(),
        wanted.rules()
    );
}

#[test]
fn rights_that_shut_the_user_out_still_leave_the_times() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = file_with_old_times(temp.path());
    icacls(
        &source,
        &["/inheritance:r", "/grant:r", &format!("{EVERYONE}:R")],
    );
    let plan = keeping(
        request(std::slice::from_ref(&source), &target, false, Existing::Ask),
        Keep {
            timestamps: true,
            rights: Rights::Copy,
            ..Keep::default()
        },
    );

    let report = run(&plan, &mut Script::default());

    let copy = target.join("a.txt");
    let created = fs::metadata(&copy).unwrap().created().unwrap();
    icacls(&source, &["/reset"]);
    icacls(&copy, &["/reset"]);
    clean(&report);
    assert_eq!(created, at(CREATED));
}

/// `root/from` passes a rule of its own on to what it holds, `root/to` does not. The test's own root
/// takes its rights by inheritance first, as a user's folder does: a temporary folder whose rights are
/// not marked inherited (a CI runner's) gives new files rules without the inherited marks.
fn folders_with_rules(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    icacls(root, &["/reset"]);
    let (from, to) = (root.join("from"), root.join("to"));
    fs::create_dir(&from).unwrap();
    fs::create_dir(&to).unwrap();
    icacls(&from, &["/grant", &format!("{EVERYONE}:(OI)(CI)R")]);
    fs::write(from.join("a.txt"), "text").unwrap();
    (from, to)
}

#[test]
fn a_rename_keeps_its_rights_by_default_and_takes_the_folders_with_inherit() {
    let temp = tempfile::tempdir().unwrap();
    let (from, to) = folders_with_rules(temp.path());
    fs::write(to.join("fresh.txt"), "x").unwrap();
    let fresh = read_security(&to.join("fresh.txt")).unwrap();
    let carried = read_security(&from.join("a.txt")).unwrap();
    assert_ne!(fresh, carried);
    fs::copy(from.join("a.txt"), from.join("b.txt")).unwrap();

    let source = from.join("a.txt");
    clean(&run(
        &request(&[source], &to, true, Existing::Ask),
        &mut Script::default(),
    ));
    assert_eq!(read_security(&to.join("a.txt")).unwrap(), carried);

    let plan = keeping(
        request(&[from.join("b.txt")], &to, true, Existing::Ask),
        rights(Rights::Inherit),
    );
    clean(&run(&plan, &mut Script::default()));
    assert_eq!(read_security(&to.join("b.txt")).unwrap(), fresh);
}

#[test]
fn a_renamed_folder_takes_the_rights_of_where_it_went_with_inherit() {
    let temp = tempfile::tempdir().unwrap();
    let (from, to) = folders_with_rules(temp.path());
    fs::create_dir(to.join("fresh")).unwrap();
    let fresh = read_security(&to.join("fresh")).unwrap();
    let inner = from.join("inner");
    fs::create_dir(&inner).unwrap();
    assert_ne!(read_security(&inner).unwrap(), fresh);
    let plan = keeping(
        request(&[inner], &to, true, Existing::Ask),
        rights(Rights::Inherit),
    );

    clean(&run(&plan, &mut Script::default()));

    assert_eq!(read_security(&to.join("inner")).unwrap(), fresh);
}

#[test]
fn what_a_provider_cannot_keep_is_told_per_item_and_the_copy_stands() {
    let root = tree::root("keep-report");
    let target = tree::dir(&root.join("target"));
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "text").unwrap();
    let mut plan = request(
        std::slice::from_ref(&source),
        Path::new("x"),
        false,
        Existing::Ask,
    );
    plan.target = target.clone();
    plan.keep = Keep {
        timestamps: true,
        rights: Rights::Copy,
        ..Keep::default()
    };

    let report = run(&plan, &mut Script::default());

    assert_eq!(tree::read(&target.join("a.txt")), b"text");
    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(report.not_kept.len(), 2, "{report:?}");
    assert!(
        report.not_kept[0].contains("access rights not kept"),
        "{report:?}"
    );
    assert!(
        report.not_kept[1].contains("timestamps not kept"),
        "{report:?}"
    );
}

#[test]
fn a_provider_without_times_is_no_news_when_nobody_asked() {
    let root = tree::root("keep-quiet");
    let target = tree::dir(&root.join("target"));
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "text").unwrap();
    let mut plan = request(&[source], Path::new("x"), false, Existing::Ask);
    plan.target = target;

    clean(&run(&plan, &mut Script::default()));
}
