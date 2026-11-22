//! The Windows implementation of the OS layer. The subsystem modules of `platform` take their functions
//! from here by the same names as from the stubs for other systems (`platform/other.rs`).

mod cdrom;
mod clipboard;
mod code_pages;
mod com;
mod console;
mod console_input;
mod copy;
mod device;
mod devices;
mod dir;
mod drive;
mod elevation;
mod environment_block;
mod exact;
mod execute;
mod file_type;
mod git_bash;
mod holders;
mod hotplug;
mod input;
mod interpreter;
mod library;
mod link;
mod locale;
mod make_link;
mod metadata;
mod owner;
mod program;
mod properties;
mod recycle_bin;
mod recycle_probe;
mod registry;
mod reparse;
mod replace;
mod shell_properties;
mod storage;
mod streams;
mod system;
mod tasks;
mod text;
mod thread_io;
mod trash;
mod vhd;
mod watch;

use std::ffi::OsStr;
use std::fs::Metadata;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, Prefix};

use super::files::{Attributes, paths};

pub use cdrom::cd_type;
pub use clipboard::{
    clipboard_is_column, clipboard_text, set_clipboard_column_text, set_clipboard_files,
    set_clipboard_text,
};
pub use code_pages::{
    ansi_code_page, decode_page, encode_page, installed_code_pages, oem_code_page,
};
pub use console::{
    console_code_pages, console_colors, console_cursor_column, console_cursor_size, console_title,
    console_window, flash_console_window, forget_variables, pass_ctrl_c_to_children,
    reopen_in_plain_console, set_console_code_pages, set_console_cursor_size, survive_ctrl_break,
    without_console,
};
pub use console_input::{
    READS_CONSOLE, begin_console_input, end_console_input, read_console, set_console_mouse,
};
pub use copy::{
    copy_file, file_identity, free_space, move_file, open_file_identity, open_past_cache,
    set_read_only, volume_id, volume_of,
};
pub use device::{
    connect_network_drive, credentials_refused, disconnect_network_drive, eject_media,
    is_ejectable_media, is_persistent_network_drive, load_media, remove_subst, set_volume_label,
};
pub use devices::{take_device_changes, take_environment_change};
pub use dir::read_directory;
pub use drive::{
    cluster_size, device_name, disk_size, drive_kind, network_name, on_local_disk, shell_name,
    subst_target, unmounted_volumes, volume_facts, volume_roots,
};
pub use elevation::{
    HelperProcess, PipeListener, PipeStream, can_elevate, connect_pipe, decode_path, encode_path,
    needs_rights, new_pipe_name, start_helper,
};
pub use environment_block::system_environment;
pub use exact::{exact_path, first_file_like, is_plain_name, reparse_tag};
pub use execute::execute;
pub use file_type::file_type_name;
pub use git_bash::git_bash;
pub use holders::{file_holders, may_be_held};
pub use hotplug::{hotplug_device, remove_hotplug_device};
pub use input::{
    double_click_time, is_altgr_text, key_for_char, keyboard_repeat, universal_name,
    wheel_scroll_chars, wheel_scroll_lines,
};
pub use interpreter::{
    CD_TAKES_DRIVE_SWITCH, INTERPRETER_FOLDER_REFUSAL, VARIABLE_NAMES_DIFFER_BY_CASE,
    completion_programs, completion_variables, expand_variables,
};
pub use library::Library;
pub use link::{copy_link, link_target};
pub use locale::{
    compare_names, compare_wide_names, date_format, decode_oem, digit_groups, from_local_time,
    local_time, month_name, number_format, take_locale_change, two_digit_year_max,
};
pub use make_link::make_link;
pub use metadata::{
    add_marks, apply_kept, change_time, copy_folder_attributes, read_security, reset_security,
    set_file_times,
};
pub use owner::set_owner;
pub use program::{DEFAULT_SHELL, open_with_association, program_start, shells, system_folder};
pub use properties::{
    file_properties, remove_reparse_point, set_attributes, set_compressed, set_encrypted,
    set_sparse,
};
pub use recycle_bin::{has_recycle_bin, recycle_bin_capacity};
pub use recycle_probe::can_recycle;
pub use reparse::modify_link;
use reparse::reparse_text;
pub use replace::{name_count, replace_file};
pub use shell_properties::show_properties;
pub use storage::storage_device;
pub use streams::alternate_streams;
pub use system::{beep, system_facts};
pub use tasks::{process_image, switch_to_window, task_windows, terminate_process};
pub use text::fuzzy_fold;
pub use thread_io::break_thread_io;
pub use trash::move_to_trash;
pub use vhd::{detach_virtual_disk, virtual_disk_file};
pub use watch::watch_dir;

/// Names that differ only by case are one name (NTFS and FAT as Windows uses them).
pub const NAMES_DIFFER_BY_CASE: bool = false;

/// An entry's attributes: Windows keeps them as bits, "hidden" among them.
pub fn attributes(_path: &Path, metadata: &Metadata) -> Attributes {
    Attributes::from_windows_bits(metadata.file_attributes())
}

/// The letter of a drive root such as `C:\`; other roots (a volume with no letter) are refused.
fn drive_letter(root: &str) -> io::Result<char> {
    paths::volume_letter(Path::new(root)).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{root} is not a drive letter"),
        )
    })
}

/// A NUL-terminated UTF-16 string for the system.
fn wide(text: &str) -> Vec<u16> {
    wide_os(OsStr::new(text))
}

/// A path or name as it is, NUL-terminated (for calls that want the user's own form of a path). One with
/// a NUL inside is no name: an empty one, which every call refuses, never the part before the NUL (R7
/// review A2: a call acted on the folder before it).
fn wide_os(text: &OsStr) -> Vec<u16> {
    let units: Vec<u16> = text.encode_wide().collect();
    if units.contains(&0) {
        return vec![0];
    }
    units.into_iter().chain([0]).collect()
}

/// The old Win32 calls take a path without the `\\?\` form while it is shorter than this, its NUL
/// counted (`MAX_PATH` less room for a file name in a folder; `std`'s `get_long_path` counts the same).
const LEGACY_MAX_PATH: usize = 248;

/// A path for a raw Win32 file call, NUL-terminated. A long absolute path gets the `\\?\` form these
/// calls need (`std` adds it itself, they do not): `\\?\C:\...`, `\\?\UNC\server\share\...`, made
/// absolute and normalized first, since the form turns off the system's own normalizing.
fn wide_path(path: &Path) -> Vec<u16> {
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    // As `wide_os`: a NUL inside makes no name, not the part before it.
    if units.contains(&0) {
        return vec![0];
    }
    if units.len() + 1 < LEGACY_MAX_PATH || !path.is_absolute() {
        return units.into_iter().chain([0]).collect();
    }
    let Ok(full) = std::path::absolute(path) else {
        return units.into_iter().chain([0]).collect();
    };
    let full_units: Vec<u16> = full.as_os_str().encode_wide().collect();
    let encode = |text: &str| -> Vec<u16> { text.encode_utf16().collect() };
    let long = match full.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_) => [encode(r"\\?\"), full_units].concat(),
            // `\\server\share` becomes `\\?\UNC\server\share`: one of the two backslashes stays.
            Prefix::UNC(..) => [encode(r"\\?\UNC"), full_units[1..].to_vec()].concat(),
            _ => full_units,
        },
        _ => full_units,
    };
    long.into_iter().chain([0]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(units: &[u16]) -> String {
        String::from_utf16_lossy(units.strip_suffix(&[0]).unwrap())
    }

    /// R7 review B6: a share's path is no local disk's, told at once by its form (the system would wait
    /// for a share that does not answer); the temporary folder's disk is one.
    #[test]
    fn a_share_is_no_local_disk_told_at_once() {
        let temp = tempfile::tempdir().unwrap();
        assert!(drive::on_local_disk(&temp.path().join("f.txt")));
        let started = std::time::Instant::now();
        assert!(!drive::on_local_disk(Path::new(r"\\192.0.2.1\share\f.txt")));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    /// R7 review A2: a NUL inside makes an empty name, never the part before it; the bin refuses it.
    #[test]
    fn a_name_with_a_nul_inside_is_no_name() {
        let named = r"C:\Users\me\Docs\0a.txt".replace('0', "\0");
        assert_eq!(wide_path(Path::new(&named)), [0]);
        assert_eq!(wide_os(OsStr::new(&named)), [0]);
        let refused = trash::move_to_trash(Path::new(&named)).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn long_paths_get_the_verbatim_form_and_short_ones_stay() {
        assert_eq!(
            text(&wide_path(Path::new(r"C:\short\name.txt"))),
            r"C:\short\name.txt"
        );
        assert_eq!(text(&wide_path(Path::new("relative"))), "relative");
        let deep = "d".repeat(300);
        assert_eq!(
            text(&wide_path(&Path::new(r"C:\x\..\a").join(&deep))),
            format!(r"\\?\C:\a\{deep}")
        );
        assert_eq!(
            text(&wide_path(&Path::new(r"\\server\share").join(&deep))),
            format!(r"\\?\UNC\server\share\{deep}")
        );
        let verbatim = format!(r"\\?\C:\{deep}");
        // The NUL counts: 246 characters stay as they are, 247 get the form, as in `std`.
        let at = |length: usize| format!(r"C:\{}", "e".repeat(length - 3));
        assert_eq!(text(&wide_path(Path::new(&at(246)))), at(246));
        assert_eq!(
            text(&wide_path(Path::new(&at(247)))),
            format!(r"\\?\{}", at(247))
        );
        assert_eq!(text(&wide_path(Path::new(&verbatim))), verbatim);
    }

    #[test]
    fn a_drive_root_has_a_letter() {
        assert_eq!(drive_letter(r"c:\").unwrap(), 'C');
        assert!(drive_letter(r"\\?\Volume{0}\").is_err());
    }
}
