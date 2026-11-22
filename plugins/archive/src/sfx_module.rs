//! A self-extracting archive's module made and an archive turned into one, as FAR's arclite does it
//! (sfx.cpp `create_sfx_module`, `attach_sfx_module` with its `AttachSfxModuleProgress`): the module
//! copied, its icon and version replaced, the install configuration put after it; a pure 7z (7-Zip's
//! 7z at the file's start) copied after the module into `name.exe`, its times kept, the 7z deleted.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::extract::Fault;
use crate::layout::fit;
use crate::progress::{WIDTH, bar, percent};
use crate::sfx::{SFX_EXT, SfxOptions, install_config_text};

/// arclite's `MSG_PROGRESS_SFX_CONVERT`.
pub const CONVERTING: &str = "Converting into SFX...";

/// arclite's `MSG_ERROR_SFX_CONVERT`.
pub const CANNOT_CONVERT: &str = "Archive cannot be converted into SFX archive";

/// 7-Zip's 7z signature.
const SEVEN_Z: [u8; 6] = [b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C];

/// The piece copied at a time (arclite's 1 MB buffer).
const PIECE: usize = 1024 * 1024;

/// arclite's `create_sfx_module`: `module` copied to `path` (made anew), its icon and version replaced
/// as `options` ask, the install configuration put after it.
pub fn create_module(path: &Path, module: &Path, options: &SfxOptions) -> io::Result<()> {
    let bytes = std::fs::read(module)?;
    std::fs::write(path, bytes)?;
    if options.replace_icon {
        crate::sfx_rsrc::replace_icon(path, Path::new(&options.icon_path))?;
    }
    if options.replace_version {
        crate::sfx_rsrc::replace_version(path, &options.ver_info)?;
    }
    if options.append_install_config {
        let mut file = OpenOptions::new().append(true).open(path)?;
        file.write_all(&install_config_text(&options.install_config))?;
    }
    Ok(())
}

/// The window as arclite's `AttachSfxModuleProgress` writes it: the title with its percent, the file,
/// the bar.
pub fn text(file: &str, done: u64, total: u64) -> (String, Vec<String>) {
    let percent = percent(done, total);
    (
        format!("{{{percent}%}} {CONVERTING}"),
        vec![fit(file, WIDTH), bar(percent)],
    )
}

/// What a conversion tells and asks as it goes: its progress, Esc.
pub trait ConvertAsks {
    fn progress(&mut self, file: &str, done: u64, total: u64, elapsed: Duration);
    fn stopped(&mut self) -> bool;
}

/// Why a conversion did not end: stopped by Esc, or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unconverted {
    Stopped,
    Failed(Fault),
}

/// arclite's `is_pure_7z` of the file: 7-Zip's 7z at its start (its signature there: no stub before
/// it, no password asked to look).
pub fn is_pure_7z(path: &Path) -> bool {
    let mut start = [0u8; 6];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut start))
        .is_ok_and(|()| start == SEVEN_Z)
}

/// The path of `path` made self-extracting (`name.7z.exe`).
pub fn sfx_path(path: &Path) -> PathBuf {
    let mut text = path.as_os_str().to_os_string();
    text.push(SFX_EXT);
    PathBuf::from(text)
}

/// arclite's `attach_sfx_module`: a pure 7z at `path` copied after the module into `name.exe` (its times
/// the 7z's), the 7z deleted; anything else refused; the new file deleted when it stops or fails.
pub fn attach(
    path: &Path,
    module: &Path,
    options: &SfxOptions,
    asks: &mut dyn ConvertAsks,
) -> Result<PathBuf, Unconverted> {
    if !is_pure_7z(path) {
        return Err(Unconverted::Failed(Fault::message(vec![
            CANNOT_CONVERT.to_string(),
        ])));
    }
    let target = sfx_path(path);
    let copied = copy_after(path, &target, module, options, asks);
    if copied.is_err() {
        let _ = std::fs::remove_file(&target);
    }
    copied?;
    std::fs::remove_file(path).map_err(|error| Unconverted::Failed(Fault::system(&error, path)))?;
    Ok(target)
}

fn copy_after(
    path: &Path,
    target: &Path,
    module: &Path,
    options: &SfxOptions,
    asks: &mut dyn ConvertAsks,
) -> Result<(), Unconverted> {
    let failed = |error: io::Error, at: &Path| Unconverted::Failed(Fault::system(&error, at));
    let mut source = File::open(path).map_err(|error| failed(error, path))?;
    let metadata = source.metadata().map_err(|error| failed(error, path))?;
    create_module(target, module, options).map_err(|error| failed(error, target))?;
    let mut out =
        (OpenOptions::new().append(true).open(target)).map_err(|error| failed(error, target))?;
    let started = std::time::Instant::now();
    let shown = path.display().to_string();
    let (mut done, total) = (0u64, metadata.len());
    let mut piece = vec![0u8; PIECE];
    loop {
        if asks.stopped() {
            return Err(Unconverted::Stopped);
        }
        let read = source
            .read(&mut piece)
            .map_err(|error| failed(error, path))?;
        if read == 0 {
            break;
        }
        out.write_all(&piece[..read])
            .map_err(|error| failed(error, target))?;
        done += read as u64;
        asks.progress(&shown, done, total, started.elapsed());
    }
    drop(out);
    let times = nc_plugin_sdk::system::SetTimes {
        created: metadata.created().ok(),
        accessed: metadata.accessed().ok(),
        modified: metadata.modified().ok(),
    };
    nc_plugin_sdk::system::set_times(target, times).map_err(|error| failed(error, target))
}

#[cfg(test)]
#[path = "sfx_module_tests.rs"]
mod tests;
