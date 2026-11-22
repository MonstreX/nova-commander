//! Development tasks (ADR-0016). `cargo run -p xtask -- package <folder> [--release] [--test-plugins]`
//! builds the workspace and lays out the portable folder the program is installed as: the program, and
//! `plugins\<name>\` with each plugin's `plugin.json` and library. Plugins marked `"test": true` in their
//! manifest (the probe) come only with `--test-plugins`. The build's own folder may be the folder
//! (`package target\debug`): the program stays, the plugins are laid out beside it for the debug runs.
//! Colorer's syntax schemes go to `hrc\` (ADR-0024).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let [task, folder, flags @ ..] = args else {
        return Err("usage: package <folder> [--release] [--test-plugins]".to_string());
    };
    if task != "package" {
        return Err(format!("unknown task {task:?}"));
    }
    let release = flags.iter().any(|flag| flag == "--release");
    let test_plugins = flags.iter().any(|flag| flag == "--test-plugins");
    let root = workspace_root()?;
    build(&root, release)?;
    let built = target_dir(&root).join(if release { "release" } else { "debug" });
    package(&root, &built, Path::new(folder), test_plugins)
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "no workspace above xtask".to_string())
}

fn target_dir(root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from)
}

fn build(root: &Path, release: bool) -> Result<(), String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    // Not itself: its own program is running.
    command
        .current_dir(root)
        .args(["build", "--workspace", "--exclude", "xtask"]);
    if release {
        command.arg("--release");
    }
    let status = command.status().map_err(|error| error.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("cargo build failed: {status}"))
}

/// The program and its plugins in `folder`.
fn package(root: &Path, built: &Path, folder: &Path, test_plugins: bool) -> Result<(), String> {
    let exe = format!("nova-commander{}", std::env::consts::EXE_SUFFIX);
    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    copy(&built.join(&exe), &folder.join(&exe))?;
    // The licence and Far Manager's notice go with the program, as both licences ask.
    for notice in ["LICENSE", "THIRD-PARTY-NOTICES.md"] {
        copy(&root.join(notice), &folder.join(notice))?;
    }
    // Colorer's syntax schemes as data beside the program (ADR-0024 §2): the repository's files
    // written over, the user's own ones there kept.
    copy_tree(
        &root.join("assets").join("colorer").join("hrc"),
        &folder.join("hrc"),
    )?;
    let plugins = root.join("plugins");
    let entries = std::fs::read_dir(&plugins).map_err(|error| error.to_string())?;
    for entry in entries.flatten() {
        let manifest_path = entry.path().join("plugin.json");
        let Ok(text) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let manifest: serde_json::Value =
            serde_json::from_str(&text).map_err(|error| format!("{manifest_path:?}: {error}"))?;
        // This repository's plugins are laid out anew (a test plugin of an earlier packaging goes); other
        // plugins in the folder, and their files, stay.
        let place = folder.join("plugins").join(entry.file_name());
        if place.exists() {
            std::fs::remove_dir_all(&place).map_err(|error| error.to_string())?;
        }
        if manifest["test"].as_bool() == Some(true) && !test_plugins {
            continue;
        }
        // As the host takes it: letters, digits, `-` and `_`.
        let library = manifest["library"]
            .as_str()
            .filter(|name| {
                !name.is_empty()
                    && name
                        .chars()
                        .all(|ch| ch.is_alphanumeric() || ch == '-' || ch == '_')
            })
            .ok_or_else(|| format!("{manifest_path:?}: no library named plainly"))?;
        let file = format!(
            "{}{library}{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        );
        std::fs::create_dir_all(&place).map_err(|error| error.to_string())?;
        copy(&manifest_path, &place.join("plugin.json"))?;
        copy(&built.join(&file), &place.join(&file))?;
    }
    println!("packaged into {}", folder.display());
    Ok(())
}

/// A folder's files copied into another, its subfolders too.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|error| format!("{}: {error}", to.display()))?;
    let entries =
        std::fs::read_dir(from).map_err(|error| format!("{}: {error}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            copy(&entry.path(), &target)?;
        }
    }
    Ok(())
}

/// `from` copied to `to`; nothing when they are one file (the build's own folder packaged in place, as
/// `package target\debug` lays the plugins out for the debug program).
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let (Ok(source), Ok(target)) = (from.canonicalize(), to.canonicalize())
        && source == target
    {
        return Ok(());
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|error| format!("{} -> {}: {error}", from.display(), to.display()))
}

#[cfg(test)]
mod tests;
