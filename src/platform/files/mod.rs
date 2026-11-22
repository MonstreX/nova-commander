//! Files: the attribute model, link kinds, what a directory listing adds to `std`, the order of names
//! and whether their case matters, the path rules, copying and moving with the system's own calls, the
//! Recycle Bin.

mod attributes;
mod link_kind;
pub mod paths;
mod properties;
mod setting;
mod watch;

use std::borrow::Cow;

pub use attributes::{Attributes, Flag};
pub use link_kind::LinkKind;
pub use properties::{FileProperties, LinkFacts};
pub use setting::Setting;
pub(in crate::platform) use watch::WatchFlags;
pub use watch::{DirWatch, watch_dir};

pub use super::os::{
    add_marks, alternate_streams, apply_kept, attributes, can_recycle, change_time, compare_names,
    compare_wide_names, copy_file, copy_folder_attributes, copy_link, exact_path, file_holders,
    file_identity, file_properties, file_type_name, first_file_like, is_plain_name, link_target,
    make_link, may_be_held, modify_link, move_file, move_to_trash, name_count, open_file_identity,
    open_past_cache, read_directory, read_security, remove_reparse_point, reparse_tag,
    replace_file, reset_security, set_attributes, set_compressed, set_encrypted, set_file_times,
    set_owner, set_read_only, set_sparse, show_properties,
};

/// How putting a saved file in place of the old one (`replace_file`) went wrong.
#[derive(Debug)]
pub enum Replaced {
    /// Nothing changed: the old file and the new one are both where they were.
    Untouched,
    /// The old file is gone and the new one is still under its own name (Windows'
    /// `ERROR_UNABLE_TO_MOVE_REPLACEMENT` without a backup name): the caller must put it in place.
    Moved,
}

/// Who holds a file open (FAR's `get_locking_processes`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Holders {
    /// At most five, each as `name (PID 7, application, [service])`.
    pub programs: Vec<String>,
    /// How many more the system knows of.
    pub more: usize,
    pub notes: Vec<HoldNote>,
}

/// Why a holder cannot simply be asked to let go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldNote {
    OtherSession,
    CriticalProcess,
    CriticalService,
    ThisProcess,
}

/// The links FAR makes (Alt+F6's "Link type", `ReparsePointTypes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewLink {
    /// Another name of the same file.
    Hard,
    /// A folder link to a full path.
    Junction,
    /// A symbolic link, to a folder when the target is one.
    Symbolic,
    SymbolicFile,
    SymbolicFolder,
}

/// A file's times; one that is `None` is left as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileTimes {
    pub created: Option<std::time::SystemTime>,
    pub accessed: Option<std::time::SystemTime>,
    pub modified: Option<std::time::SystemTime>,
    pub changed: Option<std::time::SystemTime>,
}

/// The access rights of a file, kept to be given to another one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityDescriptor(pub(in crate::platform) Vec<u64>);

#[cfg(test)]
impl SecurityDescriptor {
    /// The rules without the header: writing a descriptor does not carry all its control bits over.
    pub fn rules(&self) -> &[u64] {
        &self.0[1..]
    }
}

/// How giving a copy its access rights and its times went.
#[derive(Debug)]
pub struct Kept {
    pub rights: std::io::Result<()>,
    pub times: std::io::Result<()>,
}

/// One entry of a directory as one pass over it gives it (`read_directory`, FAR's `enum_files`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirectoryRecord {
    pub name: std::ffi::OsString,
    pub attributes: Attributes,
    pub size: u64,
    /// Space on disk (FAR's "Allocated"); `None` where the system does not say.
    pub allocated: Option<u64>,
    pub created: Option<std::time::SystemTime>,
    pub accessed: Option<std::time::SystemTime>,
    pub modified: Option<std::time::SystemTime>,
    /// Last change of the entry's data or attributes (FAR's "Change time").
    pub changed: Option<std::time::SystemTime>,
    /// The kind of a link or other special entry; `None` for everything else and when the system
    /// could not say.
    pub link: Option<LinkKind>,
}

/// Whether two names that differ only by case are two names (Unix) or one (Windows).
pub const NAMES_DIFFER_BY_CASE: bool = super::os::NAMES_DIFFER_BY_CASE;

/// `name` as names are compared on this system: lower case where case does not matter, as it is
/// where it does (masks, quick comparisons).
pub fn fold_case(name: &str) -> Cow<'_, str> {
    if NAMES_DIFFER_BY_CASE {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(name.to_lowercase())
    }
}

/// Whether two paths, as text, name the same place: case aside where it does not matter, and a
/// separator at the end aside. Plain ASCII paths make no new strings.
pub fn same_path(a: &str, b: &str) -> bool {
    let a = a.trim_end_matches(std::path::is_separator);
    let b = b.trim_end_matches(std::path::is_separator);
    if NAMES_DIFFER_BY_CASE || a == b {
        return a == b;
    }
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(b);
    }
    a.to_lowercase() == b.to_lowercase()
}

/// [`same_path`] of two OS paths.
pub fn same_place(a: &std::path::Path, b: &std::path::Path) -> bool {
    same_path(&a.to_string_lossy(), &b.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_follows_the_system() {
        if NAMES_DIFFER_BY_CASE {
            assert_eq!(fold_case("ReadMe"), "ReadMe");
            assert!(!same_path("/home/A", "/home/a"));
        } else {
            assert_eq!(fold_case("ReadMe"), "readme");
            assert!(same_path(r"C:\Work\", r"c:\work"));
            assert!(same_path("C:\\Пример", "c:\\пРИМЕР"));
            assert!(same_place(
                std::path::Path::new(r"D:\A"),
                std::path::Path::new(r"d:\a\")
            ));
        }
        assert!(!same_path("a", "b"));
    }
}
