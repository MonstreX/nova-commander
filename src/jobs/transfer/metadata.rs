//! What a copy keeps besides the contents (ADR-0009 §9): all the timestamps when asked for, access
//! rights, a new folder's attributes. Through a provider (a plugin's panel, P6.0d) the times and the
//! read-only, hidden and system marks go in one call (`Provider::set_times`, FAR's `PutFiles` items).
//! What a target cannot keep goes into the report, item by item; the copy is there all the same.

use std::fmt::Display;

use super::engine::Run;
use super::sides::{self, disks};
use super::{Rights, VfsPath};
use crate::platform::files::{self, Attributes, FileTimes};
use crate::vfs::{Entry, provider_for};

const RIGHTS: &str = "access rights";
const TIMES: &str = "timestamps";
const ATTRIBUTES: &str = "attributes";
/// Why access rights are not kept behind a provider.
const ONLY_ON_DISK: &str = "only a disk has them";

/// The times of an object as a copy may take them over.
pub(super) fn times_of(entry: &Entry) -> FileTimes {
    FileTimes {
        created: entry.created,
        accessed: entry.accessed,
        modified: entry.modified,
        changed: entry.changed,
    }
}

impl Run<'_> {
    fn not_kept(&mut self, path: &VfsPath, what: &str, why: impl Display) {
        (self.report.not_kept).push(format!("{path}: {what} not kept: {why}"));
    }

    /// The copy `written` of `entry` is complete and `source` still exists. Between disks the kernel's
    /// copy took the last write time and the attributes; through a provider they have to be given.
    pub(super) fn keep_file_metadata(
        &mut self,
        source: &VfsPath,
        written: &VfsPath,
        entry: &Entry,
    ) {
        let keep = self.request.keep;
        let times = &times_of(entry);
        if disks(source, written).is_some() {
            let times = keep.timestamps.then_some(times);
            self.apply_on_disk(source, written, times);
            return;
        }
        if keep.rights == Rights::Copy {
            self.not_kept(written, RIGHTS, ONLY_ON_DISK);
        }
        let wanted = if keep.timestamps {
            *times
        } else {
            FileTimes {
                modified: times.modified,
                ..FileTimes::default()
            }
        };
        self.give_times(written, &wanted, Some(entry.attributes));
    }

    /// `path` given its times and marks through its provider (a disk takes the marks by themselves:
    /// each failure under its own name); a failure reported, but for a provider that keeps none when
    /// nobody asked for the times (no news on every file).
    fn give_times(&mut self, path: &VfsPath, times: &FileTimes, attributes: Option<Attributes>) {
        let on_disk = sides::disk(path);
        let (through_provider, by_themselves) = match on_disk {
            Some(_) => (None, attributes),
            None => (attributes, None),
        };
        let given = provider_for(path)
            .and_then(|provider| provider.set_times(path, times, through_provider));
        match given {
            Ok(()) => {}
            Err(error)
                if error.kind() == std::io::ErrorKind::Unsupported
                    && !self.request.keep.timestamps => {}
            Err(error) => self.not_kept(path, TIMES, error),
        }
        if let (Some(disk), Some(marks)) = (on_disk, by_themselves)
            && let Err(error) = files::add_marks(&disk, marks)
        {
            self.not_kept(path, ATTRIBUTES, error);
        }
    }

    /// Both ends on a disk: the source's rights when they are to be copied, then the times.
    fn apply_on_disk(&mut self, source: &VfsPath, written: &VfsPath, times: Option<&FileTimes>) {
        let Some((from, to)) = disks(source, written) else {
            return;
        };
        let security = if self.request.keep.rights == Rights::Copy {
            match files::read_security(&from) {
                Ok(security) => Some(security),
                Err(error) => {
                    self.not_kept(written, RIGHTS, error);
                    None
                }
            }
        } else {
            None
        };
        let kept = files::apply_kept(&to, security.as_ref(), times);
        if let Err(error) = kept.rights {
            self.not_kept(written, RIGHTS, error);
        }
        if let Err(error) = kept.times {
            self.not_kept(written, TIMES, error);
        }
    }

    /// An object that went by a rename keeps all it had; only Inherit changes that (FAR).
    pub(super) fn after_rename(&mut self, destination: &VfsPath) {
        if self.request.keep.rights != Rights::Inherit {
            return;
        }
        let Some(path) = sides::disk(destination) else {
            self.not_kept(destination, RIGHTS, ONLY_ON_DISK);
            return;
        };
        if let Err(error) = files::reset_security(&path) {
            self.not_kept(destination, RIGHTS, error);
        }
    }

    /// A folder this copy created. Its times wait for the end: what goes into it changes them; through a
    /// provider its marks wait with them (a read-only folder of a plugin's might take nothing in).
    pub(super) fn keep_new_folder(&mut self, source: &VfsPath, created: &VfsPath, entry: &Entry) {
        let keep = self.request.keep;
        let times = if keep.timestamps {
            times_of(entry)
        } else {
            FileTimes::default()
        };
        if let Some((from, to)) = disks(source, created) {
            if keep.timestamps {
                self.new_folders.push((created.clone(), times, None));
            }
            if let Err(error) = files::copy_folder_attributes(&from, &to) {
                self.not_kept(created, ATTRIBUTES, error);
            }
            self.apply_on_disk(source, created, None);
            return;
        }
        let marked = (entry.attributes.is_read_only() || entry.attributes.is_hidden())
            || entry.attributes.is_system();
        if keep.timestamps || marked {
            (self.new_folders).push((created.clone(), times, Some(entry.attributes)));
        }
        if keep.rights == Rights::Copy {
            self.not_kept(created, RIGHTS, ONLY_ON_DISK);
        }
    }

    /// FAR's last step: the folders made get the times of their sources.
    pub(super) fn keep_folder_times(&mut self) {
        for (folder, times, attributes) in std::mem::take(&mut self.new_folders) {
            self.give_times(&folder, &times, attributes);
        }
    }
}
