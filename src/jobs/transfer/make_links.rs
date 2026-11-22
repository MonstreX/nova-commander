//! Making links (FAR's Alt+F6, `ShellCopy` with `FCOPY_LINK`): a junction or a symbolic link for each
//! selected item; hard links as a copy would go, folders made and each file linked, an existing file
//! replaced only by the user's choice and through a temporary sibling.

use std::io;

use super::engine::{Action, Flow, Run, Stopped};
use super::sides::{self, disks};
use crate::platform::files::{NewLink, make_link};
use crate::vfs::{Entry, VfsPath};

impl Run<'_> {
    /// One object of a link operation; behind a provider it is refused at once (FAR: Alt+F6 does
    /// nothing on a plugin panel), nothing is made and nothing is asked.
    pub(super) fn linked(
        &mut self,
        source: &VfsPath,
        destination: &VfsPath,
        entry: &Entry,
        kind: NewLink,
    ) -> Flow {
        if disks(source, destination).is_none() {
            self.fail(source, "links are made only on a disk");
            self.host.progress(&mut |progress| progress.files_done += 1);
            return Ok(());
        }
        match kind {
            NewLink::Hard if sides::is_real_folder(entry) => self.dir(source, destination, entry),
            // FAR's walk makes a folder link again as a link (`CreateDirectoryEx` with it as the
            // template): a folder cannot have another name.
            NewLink::Hard if entry.is_dir => self.link(source, destination),
            NewLink::Hard => self.hard_link(source, destination, entry),
            _ => self.new_link(kind, source, destination),
        }
    }

    /// A junction or a symbolic link to a selected item; nothing that exists is replaced.
    fn new_link(&mut self, kind: NewLink, source: &VfsPath, destination: &VfsPath) -> Flow {
        self.show_link(source, destination);
        let made = self.attempt(source, |run| {
            run.make_link_elevating(kind, source, destination)
        })?;
        self.incomplete |= made.is_none();
        self.host.progress(&mut |progress| progress.files_done += 1);
        Ok(())
    }

    /// Another name of a file in the target, as a copy would put it there.
    fn hard_link(&mut self, source: &VfsPath, destination: &VfsPath, entry: &Entry) -> Flow {
        let action = match sides::stat(destination) {
            Err(_) => Action::New(destination.clone()),
            Ok(existing) if sides::is_real_folder(&existing) => {
                self.fail(destination, "a folder with this name exists");
                return Ok(());
            }
            Ok(_) if sides::is_same(source, destination) => {
                self.fail(source, "is already this file");
                return Ok(());
            }
            Ok(existing) => {
                let read_only = existing.attributes.is_read_only();
                self.decide(source, destination, read_only)?
            }
        };
        self.show_link(source, destination);
        let done = match action {
            Action::New(path) => self
                .attempt(source, |run| {
                    run.make_link_elevating(NewLink::Hard, source, &path)
                })?
                .is_some(),
            Action::Replace(path) => self.replace_with_hard_link(source, &path)?,
            Action::Skip | Action::Append(_) => false,
        };
        self.incomplete |= !done;
        let size = entry.size;
        self.host.progress(&mut |progress| {
            progress.files_done += 1;
            progress.bytes_done += size;
        });
        Ok(())
    }

    /// The new link goes next to the old file first, then takes its place in one step.
    fn replace_with_hard_link(
        &mut self,
        source: &VfsPath,
        destination: &VfsPath,
    ) -> Result<bool, Stopped> {
        let temp = sides::temp_sibling(destination);
        let made = self.attempt(source, |run| {
            run.make_link_elevating(NewLink::Hard, source, &temp)
        })?;
        if made.is_none() {
            return Ok(false);
        }
        let replaced = self.attempt(destination, |run| {
            run.writable(destination, |run| run.rename(&temp, destination, true))
        });
        if !matches!(replaced, Ok(Some(()))) {
            self.discard(&temp, false);
        }
        Ok(replaced?.is_some())
    }

    fn show_link(&mut self, source: &VfsPath, destination: &VfsPath) {
        self.host.progress(&mut |progress| {
            progress.source = source.clone();
            progress.target = destination.clone();
            progress.file_size = 0;
            progress.file_done = 0;
        });
    }
}

/// A link at `link` to `target`; only a disk holds links.
pub(super) fn make(kind: NewLink, target: &VfsPath, link: &VfsPath) -> io::Result<()> {
    match disks(target, link) {
        Some((target, link)) => make_link(kind, &target, &link),
        None => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "links are made only on a disk",
        )),
    }
}
