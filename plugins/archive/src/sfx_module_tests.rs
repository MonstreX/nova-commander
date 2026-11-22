//! A module made from the installed 7-Zip's `7z.sfx`; a 7z turned into a self-extracting archive.

use super::*;
use crate::test_archives::{make, opened};

/// The installed 7-Zip's `7z.sfx`.
fn module() -> PathBuf {
    let empty = tempfile::tempdir().unwrap();
    let modules = nc_plugin_sdk::sevenzip::Library::find(empty.path()).sfx_modules();
    (modules.into_iter())
        .find(|path| path.ends_with("7z.sfx"))
        .expect("7-Zip's 7z.sfx installed")
}

/// What a conversion was told; Esc after `stop_after` looks.
#[derive(Default)]
struct Told {
    progress: Vec<(String, u64, u64)>,
    stop_after: Option<usize>,
}

impl ConvertAsks for Told {
    fn progress(&mut self, file: &str, done: u64, total: u64, _elapsed: Duration) {
        self.progress.push((file.to_string(), done, total));
    }

    fn stopped(&mut self) -> bool {
        match self.stop_after {
            Some(0) => true,
            Some(left) => {
                self.stop_after = Some(left - 1);
                false
            }
            None => false,
        }
    }
}

/// The module copied as it is; with the install configuration asked, the configuration after it.
#[test]
fn a_module_is_the_sfx_with_its_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let plain = temp.path().join("plain.exe");
    create_module(&plain, &module(), &SfxOptions::default()).unwrap();
    assert_eq!(
        std::fs::read(&plain).unwrap(),
        std::fs::read(module()).unwrap()
    );
    let options = SfxOptions {
        append_install_config: true,
        install_config: crate::sfx::InstallConfig {
            title: "T".to_string(),
            ..crate::sfx::InstallConfig::default()
        },
        ..SfxOptions::default()
    };
    let configured = temp.path().join("configured.exe");
    create_module(&configured, &module(), &options).unwrap();
    let bytes = std::fs::read(&configured).unwrap();
    assert!(bytes.ends_with(&install_config_text(&options.install_config)));
    assert!(bytes.starts_with(b"MZ"));
    let options = SfxOptions {
        replace_icon: true,
        icon_path: temp.path().join("none.ico").display().to_string(),
        ..SfxOptions::default()
    };
    assert!(create_module(&temp.path().join("x.exe"), &module(), &options).is_err());
}

/// A 7z turned into `name.7z.exe`: the module then the 7z, its time kept, the 7z gone, the progress
/// counting its bytes; the new file opens as the 7z after its stub.
#[test]
fn a_pure_7z_becomes_self_extracting() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.7z");
    make(&path, "7z", &[("a.txt", Some(b"alpha"))], (false, None));
    let modified = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000);
    let times = nc_plugin_sdk::system::SetTimes {
        created: None,
        accessed: None,
        modified: Some(modified),
    };
    nc_plugin_sdk::system::set_times(&path, times).unwrap();
    let archive = std::fs::read(&path).unwrap();
    let mut told = Told::default();
    let made = attach(&path, &module(), &SfxOptions::default(), &mut told).unwrap();
    assert_eq!(made, temp.path().join("a.7z.exe"));
    assert!(!path.exists(), "the 7z deleted");
    let bytes = std::fs::read(&made).unwrap();
    let stub = std::fs::read(module()).unwrap();
    assert!(bytes.starts_with(&stub) && bytes.ends_with(&archive));
    assert_eq!(bytes.len(), stub.len() + archive.len());
    assert_eq!(
        std::fs::metadata(&made).unwrap().modified().unwrap(),
        modified
    );
    let size = archive.len() as u64;
    assert_eq!(told.progress, [(path.display().to_string(), size, size)]);
    let sfx = opened(&made);
    assert!(sfx.tree.find("/a.txt").is_some());
}

/// Not a pure 7z (a zip, or a 7z after a stub): refused in arclite's words, nothing made; Esc: the new
/// file deleted, the 7z kept.
#[test]
fn only_a_pure_7z_is_converted() {
    let temp = tempfile::tempdir().unwrap();
    let zip = temp.path().join("a.zip");
    make(&zip, "zip", &[("a.txt", Some(b"alpha"))], (false, None));
    let refused = attach(
        &zip,
        &module(),
        &SfxOptions::default(),
        &mut Told::default(),
    );
    let words = Fault::message(vec![CANNOT_CONVERT.to_string()]);
    assert_eq!(refused, Err(Unconverted::Failed(words)));
    assert!(zip.exists() && !sfx_path(&zip).exists());
    let path = temp.path().join("b.7z");
    make(&path, "7z", &[("b.txt", Some(b"beta"))], (false, None));
    let mut stopping = Told {
        stop_after: Some(0),
        ..Told::default()
    };
    let stopped = attach(&path, &module(), &SfxOptions::default(), &mut stopping);
    assert_eq!(stopped, Err(Unconverted::Stopped));
    assert!(path.exists() && !sfx_path(&path).exists());
    assert!(!is_pure_7z(&temp.path().join("none.7z")));
}

/// The window: the percent in the title, the file, the bar.
#[test]
fn the_window_is_arclites() {
    let (title, lines) = text(r"C:\a.7z", 1, 4);
    assert_eq!(title, "{25%} Converting into SFX...");
    assert_eq!(lines[0], r"C:\a.7z");
    assert_eq!(lines[1].chars().count(), WIDTH);
}
