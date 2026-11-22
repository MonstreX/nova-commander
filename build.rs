//! The program's Windows resources: its icon (`assets/icon/nova-commander.ico`, drawn by
//! `assets/icon/draw_icon.py`) and its version, compiled by the Windows SDK's `rc.exe` (there wherever
//! the MSVC toolchain links; `RC` names another) and given to the linker. Off Windows, or without
//! `rc.exe`, the program is built without them (a warning says so).

use std::path::{Path, PathBuf};
use std::process::Command;

const ICON: &str = "assets/icon/nova-commander.ico";

fn main() {
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RC");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    if !windows || !msvc {
        return;
    }
    if let Err(why) = resources() {
        println!("cargo:warning=built without its icon and version: {why}");
    }
}

fn resources() -> Result<(), String> {
    let out = PathBuf::from(std::env::var("OUT_DIR").map_err(|error| error.to_string())?);
    let root =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|error| error.to_string())?);
    let script = out.join("nova-commander.rc");
    let compiled = out.join("nova-commander.res");
    std::fs::write(&script, script_text(&root.join(ICON))).map_err(|error| error.to_string())?;
    let compiler = rc_exe().ok_or("no rc.exe of the Windows SDK found")?;
    let status = Command::new(&compiler)
        .args(["/nologo", "/fo"])
        .arg(&compiled)
        .arg(&script)
        .status()
        .map_err(|error| format!("{}: {error}", compiler.display()))?;
    if !status.success() {
        return Err(format!("{} failed: {status}", compiler.display()));
    }
    println!("cargo:rustc-link-arg-bins={}", compiled.display());
    Ok(())
}

/// The resource script: the icon first (the one Explorer and the console show) and the version.
fn script_text(icon: &Path) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let numbers: Vec<u16> = (version.split(['.', '-']))
        .take(3)
        .map(|part| part.parse().unwrap_or(0))
        .collect();
    let number = |at: usize| numbers.get(at).copied().unwrap_or(0);
    let binary = format!("{},{},{},0", number(0), number(1), number(2));
    let path = icon.display().to_string().replace('\\', "\\\\");
    format!(
        "1 ICON \"{path}\"\n\
         1 VERSIONINFO\n\
         FILEVERSION {binary}\n\
         PRODUCTVERSION {binary}\n\
         FILEOS 0x40004\n\
         FILETYPE 0x1\n\
         BEGIN\n\
         \x20 BLOCK \"StringFileInfo\"\n\
         \x20 BEGIN\n\
         \x20   BLOCK \"040904B0\"\n\
         \x20   BEGIN\n\
         \x20     VALUE \"CompanyName\", \"Nova Commander\"\n\
         \x20     VALUE \"FileDescription\", \"Nova Commander\"\n\
         \x20     VALUE \"FileVersion\", \"{version}\"\n\
         \x20     VALUE \"InternalName\", \"nova-commander\"\n\
         \x20     VALUE \"LegalCopyright\", \"MIT License\"\n\
         \x20     VALUE \"OriginalFilename\", \"nova-commander.exe\"\n\
         \x20     VALUE \"ProductName\", \"Nova Commander\"\n\
         \x20     VALUE \"ProductVersion\", \"{version}\"\n\
         \x20   END\n\
         \x20 END\n\
         \x20 BLOCK \"VarFileInfo\"\n\
         \x20 BEGIN\n\
         \x20   VALUE \"Translation\", 0x409, 1200\n\
         \x20 END\n\
         END\n"
    )
}

/// `RC` when set; else the newest Windows 10/11 SDK's `rc.exe` for this machine.
fn rc_exe() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("RC") {
        return Some(PathBuf::from(named));
    }
    let programs = std::env::var_os("ProgramFiles(x86)")?;
    let bin = Path::new(&programs)
        .join("Windows Kits")
        .join("10")
        .join("bin");
    let mut versions: Vec<PathBuf> = (std::fs::read_dir(&bin).ok()?)
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("x64").join("rc.exe").is_file())
        .collect();
    versions.sort_by_key(|path| version_of(path));
    versions.last().map(|path| path.join("x64").join("rc.exe"))
}

/// A folder named like `10.0.26100.0` as numbers, to order the SDKs.
fn version_of(path: &Path) -> Vec<u32> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    (name.unwrap_or_default().split('.'))
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}
