//! A copy's or move's way to the elevated helper (ADR-0007): each step that failed for want of rights
//! is asked about, then done again by the helper.

use std::fs;
use std::io;

use super::engine::Run;
use super::sides::{self, disks};
use super::{Answer, Question};
use crate::jobs::elevation::{Decision, Verb};
use crate::platform;
use crate::platform::files::NewLink;
use crate::vfs::VfsPath;
use crate::vfs::local::elevated::Request as Elevated;
use crate::vfs::local::elevated_session::is_not_started;

impl Run<'_> {
    /// `op`; when the rights were missing and the user agrees, `request` through the helper instead.
    /// No: `op`'s failure, for the usual question.
    pub(super) fn elevating(
        &mut self,
        path: &VfsPath,
        verb: Verb,
        request: impl FnOnce() -> Option<Elevated>,
        op: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        let result = op();
        if !self.wants_helper(path, verb, &result) {
            return result;
        }
        match request() {
            Some(request) => self.through_helper(result, &request, None),
            None => result,
        }
    }

    /// Whether `result` failed for want of rights and the user wants the helper: FAR's question.
    /// Esc stops the operation.
    pub(super) fn wants_helper(
        &mut self,
        path: &VfsPath,
        verb: Verb,
        result: &io::Result<()>,
    ) -> bool {
        let Err(error) = result else {
            return false;
        };
        if !platform::elevation::needs_rights(error) || !self.host.elevate_available() {
            return false;
        }
        let host = &mut self.host;
        let decision = self.approval.decide(|| {
            let reply = host.ask(Question::Elevate {
                path: path.clone(),
                verb,
            });
            let decision = match reply.answer {
                Answer::Elevate => Decision::Elevate,
                Answer::Skip => Decision::Refuse,
                _ => Decision::Cancel,
            };
            (decision, reply.for_all)
        });
        self.stopped |= decision == Decision::Cancel;
        decision == Decision::Elevate
    }

    /// Has the helper do `request` in place of the failed `original`. A helper that could not be
    /// started (a refused prompt) is not tried again in this operation: `original` stands.
    pub(super) fn through_helper(
        &mut self,
        original: io::Result<()>,
        request: &Elevated,
        base: Option<u64>,
    ) -> io::Result<()> {
        match self.host.run_elevated(request, base) {
            Err(error) if is_not_started(&error) => {
                self.approval.give_up();
                original
            }
            result => {
                self.helper_used = true;
                self.stopped |= result
                    .as_ref()
                    .is_err_and(|error| error.kind() == io::ErrorKind::Interrupted);
                result
            }
        }
    }

    /// A link made like `source` at `path`.
    pub(super) fn copy_link_elevating(
        &mut self,
        source: &VfsPath,
        path: &VfsPath,
    ) -> io::Result<()> {
        self.elevating(
            source,
            Verb::SymbolicLink,
            || {
                disks(source, path).map(|(source, destination)| Elevated::CopyLink {
                    source,
                    destination,
                })
            },
            || super::engine::copy_link(source, path),
        )
    }

    /// Removes a link itself.
    pub(super) fn remove_link(&mut self, source: &VfsPath) -> io::Result<()> {
        let request = sides::elevated_remove_link(source);
        self.elevating(
            source,
            Verb::Delete,
            || request,
            || sides::remove_link(source),
        )
    }

    /// A new link at `link`.
    pub(super) fn make_link_elevating(
        &mut self,
        kind: NewLink,
        target: &VfsPath,
        link: &VfsPath,
    ) -> io::Result<()> {
        let verb = if kind == NewLink::Hard {
            Verb::HardLink
        } else {
            Verb::SymbolicLink
        };
        self.elevating(
            link,
            verb,
            || disks(target, link).map(|(target, link)| Elevated::MakeLink { kind, target, link }),
            || super::make_links::make(kind, target, link),
        )
    }

    /// Runs `write` on a file the user agreed to write into with read-only cleared; a step that fails
    /// gives the file its read-only back.
    pub(super) fn writable<T>(
        &mut self,
        path: &VfsPath,
        write: impl FnOnce(&mut Self) -> io::Result<T>,
    ) -> io::Result<T> {
        // Behind a provider the attribute cannot be touched yet (ADR-0011): the provider says what it
        // refuses.
        let Some(on_disk) = sides::disk(path) else {
            return write(self);
        };
        let read_only =
            fs::symlink_metadata(&on_disk).is_ok_and(|metadata| metadata.permissions().readonly());
        let request = |read_only| Elevated::SetReadOnly {
            path: on_disk.clone(),
            read_only,
        };
        if read_only {
            self.elevating(
                path,
                Verb::SetAttributes,
                || Some(request(false)),
                || platform::files::set_read_only(&on_disk, false),
            )?;
        }
        let result = write(self);
        if read_only && result.is_err() && fs::symlink_metadata(&on_disk).is_ok() {
            let back = platform::files::set_read_only(&on_disk, true);
            // Nothing more to do about a file that stays writable; the failure is what is reported.
            let _ = self.with_helper_used(back, &request(true));
        }
        result
    }

    /// `result`, or `request` by the helper if it failed for want of rights and the helper has been
    /// used here already: cleaning up after it asks nothing.
    fn with_helper_used(&mut self, result: io::Result<()>, request: &Elevated) -> io::Result<()> {
        let denied = result
            .as_ref()
            .is_err_and(platform::elevation::needs_rights);
        if denied && self.helper_used {
            return self.host.run_elevated(request, None);
        }
        result
    }

    /// Removes a file this operation made.
    pub(super) fn remove_made(&mut self, path: &VfsPath) -> io::Result<()> {
        match sides::elevated_remove_file(path) {
            Some(request) => self.with_helper_used(sides::remove_file(path), &request),
            None => sides::remove_file(path),
        }
    }

    /// Removes the temporary file that did not take its target's place; one that stays is reported.
    pub(super) fn discard(&mut self, temp: &VfsPath, link: bool) {
        // Under a stop of its own: the task's stop must not break the cleanup.
        let result = crate::vfs::stop::unstopped(|| {
            if link {
                match sides::elevated_remove_link(temp) {
                    Some(request) => self.with_helper_used(sides::remove_link(temp), &request),
                    None => sides::remove_link(temp),
                }
            } else {
                self.remove_made(temp)
            }
        });
        if let Err(error) = result
            && error.kind() != io::ErrorKind::NotFound
        {
            self.fail(
                temp,
                &format!("the temporary file could not be removed: {error}"),
            );
        }
    }
}
