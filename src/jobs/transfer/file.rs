//! One file: a new copy, a safe overwrite through a temporary sibling (in place only after the
//! low-space warning), append that undoes itself on failure, and the source removed after a move.

use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};

use super::engine::{Action, Flow, Run, Stopped};
use super::sides::{self, disks};
use super::{Answer, Question, Reply};
use crate::jobs::elevation::Verb;
use crate::platform;
use crate::vfs::local::elevated::Request as Elevated;
use crate::vfs::{Entry, Removal, VfsPath, WriteMode, provider_for};

/// The fallback copy's buffer.
const BUFFER: usize = 1024 * 1024;

/// What became of a source file.
enum Done {
    /// A copy exists; a move deletes the source now.
    Copied,
    /// The file itself went to the target (a rename).
    Moved,
    Skipped,
}

impl Run<'_> {
    pub(super) fn file(&mut self, source: &VfsPath, destination: &VfsPath, entry: &Entry) -> Flow {
        let size = entry.size;
        let action = match sides::stat(destination) {
            Err(_) => Action::New(destination.clone()),
            Ok(existing) if sides::is_real_folder(&existing) => {
                self.fail(destination, "a folder with this name exists");
                return Ok(());
            }
            // Inside a merged folder a junction or a hard link can lead back to the source: replacing
            // or appending would destroy what it reads.
            Ok(_) if sides::is_same(source, destination) => {
                self.fail(source, "cannot be copied or moved onto itself");
                return Ok(());
            }
            Ok(existing) => {
                let read_only = existing.attributes.is_read_only();
                self.decide(source, destination, read_only)?
            }
        };
        let base = self.start_file(source, destination, size);
        // Appended to, the file stays what it was.
        let written = match &action {
            Action::New(path) | Action::Replace(path) => Some(path.clone()),
            Action::Append(_) | Action::Skip => None,
        };
        let done = match action {
            Action::Skip => Done::Skipped,
            Action::New(path) => self.copy_new(source, &path, size)?,
            Action::Replace(path) => self.replace(source, &path, size)?,
            Action::Append(path) => self.append(source, &path)?,
        };
        self.incomplete |= matches!(done, Done::Skipped);
        match (&done, &written) {
            (Done::Copied, Some(written)) => {
                self.keep_file_metadata(source, written, entry);
            }
            (Done::Moved, Some(written)) => self.after_rename(written),
            _ => {}
        }
        if matches!(done, Done::Copied) && self.moves() {
            self.attempt(source, |run| run.remove_file(source))?;
        }
        self.host.progress(&mut |progress| {
            progress.files_done += 1;
            progress.bytes_done = base + size;
            progress.file_done = size;
        });
        Ok(())
    }

    /// Shows the file in the progress dialog; the bytes done before it.
    fn start_file(&mut self, source: &VfsPath, destination: &VfsPath, size: u64) -> u64 {
        let mut base = 0;
        self.host.progress(&mut |progress| {
            progress.source = source.clone();
            progress.target = destination.clone();
            progress.file_size = size;
            progress.file_done = 0;
            base = progress.bytes_done;
        });
        base
    }

    /// A copy under a name that is free; the free space is looked at first.
    fn copy_new(&mut self, source: &VfsPath, path: &VfsPath, size: u64) -> Result<Done, Stopped> {
        loop {
            let Some(free) = self.free_space_for(path).filter(|&free| free < size) else {
                break;
            };
            let message = format!("not enough free space: {size} bytes needed, {free} free");
            if !self.ask_after_error(path, &message, true)? {
                return Ok(Done::Skipped);
            }
        }
        self.copy_contents_or_skip(source, path)
    }

    fn copy_contents_or_skip(&mut self, source: &VfsPath, path: &VfsPath) -> Result<Done, Stopped> {
        let copied = self.attempt(source, |run| run.copy_contents(source, path))?;
        Ok(copied.map_or(Done::Skipped, |()| Done::Copied))
    }

    /// The copy under `path`; with Verify, read back. One that differs does not stay.
    fn copy_contents(&mut self, source: &VfsPath, path: &VfsPath) -> io::Result<()> {
        self.write_contents(source, path)?;
        if !self.request.keep.verify {
            return Ok(());
        }
        let checked = self.verify(source, path, 0);
        if let Err(error) = &checked
            && let Err(kept) = self.remove_made(path)
            && kept.kind() != io::ErrorKind::NotFound
        {
            let what = format!("the copy could not be removed: {kept}");
            return Err(self.left_behind(path, error, &what));
        }
        checked
    }

    /// The kernel's copy with progress between disks, chunks otherwise; a cancel inside it stops the
    /// operation.
    fn write_contents(&mut self, source: &VfsPath, path: &VfsPath) -> io::Result<()> {
        let Some((from, to)) = disks(source, path) else {
            return self.stream(source, path);
        };
        let base = self.bytes_before_file();
        let Run { host, stopped, .. } = self;
        let result = platform::files::copy_file(&from, &to, &mut |done, total| {
            host.progress(&mut |progress| {
                progress.file_done = done;
                progress.file_size = total;
                progress.bytes_done = base + done;
            });
            *stopped = host.cancelled();
            !*stopped
        });
        if !self.wants_helper(source, Verb::Copy, &result) {
            return result;
        }
        let request = Elevated::CopyFile { from, to };
        self.through_helper(result, &request, Some(base))
    }

    /// Removes a file, through the helper if the rights were missing.
    fn remove_file(&mut self, path: &VfsPath) -> io::Result<()> {
        self.elevating(
            path,
            Verb::Delete,
            || sides::elevated_remove_file(path),
            || sides::remove_file(path),
        )
    }

    /// Through the providers, a new file in chunks. What did not finish never stays under the name:
    /// the file was made here (`New` fails on anything that existed).
    fn stream(&mut self, source: &VfsPath, path: &VfsPath) -> io::Result<()> {
        let mut input = provider_for(source)?.open_read(source)?;
        let target = provider_for(path)?;
        let mut output = target.open_write(path, WriteMode::New)?;
        let result = self.pump(&mut input, &mut output);
        drop(output);
        // Under a stop of its own: the task's stop that came must not break the cleanup.
        if let Err(error) = &result
            && let Err(kept) = crate::vfs::stop::unstopped(|| target.remove(path, Removal::File))
            && kept.kind() != io::ErrorKind::NotFound
        {
            let what = format!("the unfinished file {path} could not be removed: {kept}");
            return Err(self.left_behind(path, error, &what));
        }
        result
    }

    /// Input to output with progress; a cancel between chunks stops the operation.
    fn pump(&mut self, input: &mut dyn Read, output: &mut dyn Write) -> io::Result<()> {
        let base = self.bytes_before_file();
        let Run { host, stopped, .. } = self;
        copy_stream(input, output, &mut |done| {
            host.progress(&mut |progress| {
                progress.file_done = done;
                progress.bytes_done = base + done;
            });
            *stopped = host.cancelled();
            !*stopped
        })
    }

    /// The free space in the folder that holds `path`, when it is on a disk and the file system says.
    fn free_space_for(&mut self, path: &VfsPath) -> Option<u64> {
        self.host.free_space(path.parent()?.as_local()?)
    }

    fn bytes_before_file(&mut self) -> u64 {
        let mut base = 0;
        self.host
            .progress(&mut |progress| base = progress.bytes_done - progress.file_done);
        base
    }

    /// Overwrite: a move on one volume replaces the target with the source itself; otherwise the
    /// copy goes to a temporary sibling that takes the target's place once complete. Without room for
    /// both, the low-space warning offers in place.
    fn replace(&mut self, source: &VfsPath, target: &VfsPath, size: u64) -> Result<Done, Stopped> {
        if self.moves() {
            let moved = self.attempt_on(source, Some(target), |run| {
                match run.writable(target, |run| run.rename(source, target, true)) {
                    Ok(()) => Ok(true),
                    Err(error) if error.kind() == io::ErrorKind::CrossesDevices => Ok(false),
                    Err(error) => Err(error),
                }
            })?;
            match moved {
                Some(true) => return Ok(Done::Moved),
                None => return Ok(Done::Skipped),
                Some(false) => {}
            }
        }
        let old_size = sides::facts(target).size;
        match self.free_space_for(target) {
            Some(free) if free < size && free.saturating_add(old_size) >= size => {
                self.in_place(source, target, size, old_size, free)
            }
            Some(free) if free < size => {
                let message = format!(
                    "not enough free space: {size} bytes needed, {free} free and {old_size} in the \
                     file it would replace"
                );
                self.ask_after_error(target, &message, false)?;
                Ok(Done::Skipped)
            }
            _ => self.replace_safely(source, target),
        }
    }

    /// The copy under a temporary name takes the target's place once complete. A provider that renames
    /// nothing (a plugin's panel that only writes, P6.0d) gets it otherwise: the old file removed, the
    /// complete copy written under its name, then removed (FAR hands such a plugin the file to overwrite
    /// itself). From then on a source from elsewhere is written in place at once: only a source in the
    /// same panel might be the target under another name (a plugin that ignores case).
    fn replace_safely(&mut self, source: &VfsPath, target: &VfsPath) -> Result<Done, Stopped> {
        if self.no_rename && !source.same_mount(target) {
            return self.replace_in_place(source, target);
        }
        let temp = sides::temp_sibling(target);
        let copied = self.attempt(source, |run| run.copy_contents(source, &temp));
        if !matches!(copied, Ok(Some(()))) {
            // A failed or cancelled copy removed itself; the old file was never touched.
            return Ok(copied?.map_or(Done::Skipped, |()| Done::Copied));
        }
        let replaced = if self.no_rename {
            Ok(Some(false))
        } else {
            self.rename_over(&temp, target)
        };
        match replaced {
            Ok(Some(true)) => Ok(Done::Copied),
            Ok(Some(false)) => {
                self.no_rename = true;
                let done = self.replace_in_place(&temp, target);
                // Not in its place with the old file gone: the whole copy may be all there is (a source
                // that was the target under another name).
                if matches!(done, Ok(Done::Copied)) || sides::stat(target).is_ok() {
                    self.discard(&temp, false);
                } else {
                    self.fail(&temp, "the old file is gone: the whole copy stays here");
                }
                done
            }
            other => {
                self.discard(&temp, false);
                other?;
                Ok(Done::Skipped)
            }
        }
    }

    /// The complete copy `temp` put in `target`'s place: false when its provider renames nothing.
    fn rename_over(&mut self, temp: &VfsPath, target: &VfsPath) -> Result<Option<bool>, Stopped> {
        let behind_provider = sides::disk(target).is_none();
        self.attempt(target, |run| {
            run.writable(target, |run| match run.rename(temp, target, true) {
                Ok(()) => Ok(true),
                // `rename` tells a provider's refusal so.
                Err(error) if behind_provider && error.kind() == io::ErrorKind::CrossesDevices => {
                    Ok(false)
                }
                Err(error) => Err(error),
            })
        })
    }

    /// The old file removed, the copy written under its name.
    fn replace_in_place(&mut self, source: &VfsPath, target: &VfsPath) -> Result<Done, Stopped> {
        if self
            .attempt(target, |run| run.remove_file(target))?
            .is_none()
        {
            return Ok(Done::Skipped);
        }
        self.copy_contents_or_skip(source, target)
    }

    /// The warning, then the old file goes and the copy is written under its name.
    fn in_place(
        &mut self,
        source: &VfsPath,
        target: &VfsPath,
        size: u64,
        old_size: u64,
        free: u64,
    ) -> Result<Done, Stopped> {
        if !self.in_place_all {
            let reply: Reply = self.host.ask(Question::LowSpace {
                target: target.clone(),
                new_size: size,
                old_size,
                free,
            });
            match reply.answer {
                Answer::InPlace => self.in_place_all = reply.for_all,
                Answer::Cancel => return self.stop(),
                _ => return Ok(Done::Skipped),
            }
        }
        self.replace_in_place(source, target)
    }

    /// Append (FAR): in place, so a failure or cancel cuts the file back to its old length.
    fn append(&mut self, source: &VfsPath, target: &VfsPath) -> Result<Done, Stopped> {
        let appended = self.attempt_on(source, Some(target), |run| {
            run.writable(target, |run| run.append_contents(source, target))
        })?;
        Ok(appended.map_or(Done::Skipped, |()| Done::Copied))
    }

    fn append_contents(&mut self, source: &VfsPath, target: &VfsPath) -> io::Result<()> {
        let mut input = provider_for(source)?.open_read(source)?;
        match sides::disk(target) {
            Some(on_disk) => self.append_on_disk(&mut input, source, target, &on_disk),
            None => self.append_through_provider(&mut input, source, target),
        }
    }

    fn append_on_disk(
        &mut self,
        input: &mut dyn Read,
        source: &VfsPath,
        target: &VfsPath,
        on_disk: &std::path::Path,
    ) -> io::Result<()> {
        // Write access (not append only): a failure must be able to cut the file back.
        let mut output = OpenOptions::new().write(true).open(on_disk)?;
        let old_length = output.seek(SeekFrom::End(0))?;
        let result = self.pump_and_check(input, &mut output, source, target, old_length);
        if let Err(error) = &result
            && let Err(cut) = output.set_len(old_length)
        {
            let what = format!("the appended part could not be removed: {cut}");
            return Err(self.left_behind(target, error, &what));
        }
        result
    }

    /// What is appended, with Verify read back after what the file held.
    fn pump_and_check(
        &mut self,
        input: &mut dyn Read,
        output: &mut dyn Write,
        source: &VfsPath,
        target: &VfsPath,
        old_length: u64,
    ) -> io::Result<()> {
        self.pump(input, output)?;
        if self.request.keep.verify {
            self.verify(source, target, old_length)?;
        }
        Ok(())
    }

    /// A provider cannot cut a file back: what was appended before a failure stays, so Retry is not
    /// offered (it would repeat that part).
    fn append_through_provider(
        &mut self,
        input: &mut dyn Read,
        source: &VfsPath,
        target: &VfsPath,
    ) -> io::Result<()> {
        let old_length = sides::stat(target)?.size;
        let mut output = provider_for(target)?.open_write(target, WriteMode::Append)?;
        let result = self.pump_and_check(input, &mut output, source, target, old_length);
        if let Err(error) = &result {
            return Err(self.left_behind(
                target,
                error,
                "the part appended so far stays in the file",
            ));
        }
        result
    }

    /// Part of a failed write stayed and cannot be undone: Retry would repeat it or run into it, and
    /// a cancel, which asks nothing, still has to say so.
    fn left_behind(&mut self, path: &VfsPath, error: &io::Error, what: &str) -> io::Error {
        self.no_retry = true;
        if self.stopped {
            self.fail(path, what);
        }
        io::Error::other(format!("{error}; {what}"))
    }
}

/// The fallback copy (any file system): 1 MiB at a time; `report(done)` false stops it.
pub(super) fn copy_stream(
    input: &mut dyn Read,
    output: &mut dyn Write,
    report: &mut dyn FnMut(u64) -> bool,
) -> io::Result<()> {
    let mut buffer = vec![0u8; BUFFER];
    let mut done = 0u64;
    loop {
        let read = match input.read(&mut buffer) {
            Ok(0) => return output.flush(),
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        output.write_all(&buffer[..read])?;
        done += read as u64;
        if !report(done) {
            return Err(io::Error::other("copy cancelled"));
        }
    }
}
