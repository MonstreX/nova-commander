use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use super::*;

fn over(count: &InfoCount) -> FolderInfo {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let now = count.now();
        if now.over {
            return now.info;
        }
        assert!(Instant::now() < deadline, "the count did not end");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn file(name: &str, size: u64, allocated: Option<u64>) -> Entry {
    Entry {
        name: name.to_string(),
        size,
        allocated,
        ..Entry::default()
    }
}

#[test]
fn space_past_a_file_s_end_is_slack_from_a_cluster_on_and_overhead_below_it() {
    let mut info = FolderInfo {
        cluster: Some(4096),
        ..FolderInfo::default()
    };
    info.add_file(&file("big", 10_000, Some(12_288)));
    info.add_file(&file("small", 5, Some(8)));
    info.add_file(&file("sparse", 100_000, Some(4096)));
    assert_eq!(info.files, 3);
    assert_eq!(info.bytes, 110_005);
    assert_eq!(info.allocated, 12_288 + 8 + 4096);
    assert_eq!(info.slack, 2288, "FAR's FilesSlack");
    assert_eq!(info.mft_overhead, 3, "FAR's MFTOverhead");
}

#[test]
fn an_item_without_its_space_takes_its_size_as_a_plugin_s() {
    let mut info = FolderInfo::default();
    info.add_file(&file("in.zip", 700, None));
    assert_eq!((info.allocated, info.slack, info.mft_overhead), (700, 0, 0));
}

#[test]
fn a_folder_s_folders_files_and_bytes_with_its_disk_s_cluster() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("sub").join("deeper")).unwrap();
    fs::write(temp.path().join("a.txt"), "hello").unwrap();
    fs::write(temp.path().join("sub").join("b.bin"), vec![0u8; 10_000]).unwrap();
    let info = over(&InfoCount::start(VfsPath::local(temp.path())));
    assert_eq!((info.folders, info.files, info.bytes), (2, 2, 10_005));
    let cluster = info
        .cluster
        .expect("the temp folder's disk tells its cluster");
    assert!(cluster >= 512, "{cluster}");
    assert!(info.allocated >= 10_000, "{info:?}");
}

#[test]
fn a_link_back_up_is_walked_once() {
    let temp = tempfile::tempdir().unwrap();
    let sub = temp.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("a.txt"), "x").unwrap();
    junction(&sub.join("up"), temp.path());
    let info = over(&InfoCount::start(VfsPath::local(temp.path())));
    let up = InfoCount::start(VfsPath::local(sub.join("up")));
    over(&up);
    let link = up.now().link.expect("a junction");
    assert_eq!(link.kind, LinkKind::Junction);
    assert_eq!(link.target, Some(temp.path().display().to_string()));
    // sub and the link are folders; the link leads where the count began: not walked again.
    assert_eq!((info.folders, info.files, info.bytes), (2, 1, 1));
}

#[test]
fn dropping_the_count_stops_it() {
    let temp = tempfile::tempdir().unwrap();
    let count = InfoCount::start(VfsPath::local(temp.path()));
    let shared = Arc::clone(&count.0);
    drop(count);
    assert!(shared.stop.load(Ordering::Relaxed));
}

fn junction(link: &Path, target: &Path) {
    let done = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(done.status.success());
}
