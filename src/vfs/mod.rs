//! Virtual file system (ADR-0013): where a panel is (`VfsPath`), who reads it (`Provider`), what it holds
//! (`Entry`), and how entries are matched and sorted. No UI dependencies.

mod entry;
pub mod filter;
pub mod local;
mod look;
mod mask;
mod opener;
mod path;
mod provider;
mod sort;
pub mod stop;
#[cfg(test)]
pub mod test_tree;
#[cfg(test)]
pub mod testing;
pub mod text;

pub use entry::{Entry, EntryTexts, FolderSize};
pub use look::{InfoLine, PanelKey, PanelLook, PanelMode};
pub use mask::{FileMask, matches_prefix};
pub use opener::{FileOpener, OpenedFile};
pub use path::VfsPath;
pub use provider::{
    Capabilities, Provider, RandomRead, Removal, Watch, WriteMode, provider_for, register,
    unregister,
};
#[cfg(test)]
pub use sort::sort_entries;
pub use sort::{SortMode, SortOrder, sort_entries_grouped, split_extension};
