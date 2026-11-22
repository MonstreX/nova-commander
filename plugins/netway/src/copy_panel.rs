//! F5 / F6, F3 / F4 of a connected session's panel (NetBox's `GetFilesEx` / `GetFilesRemote`,
//! `PutFilesEx` / `UploadFiles`, `OperationProgress`, `CancelConfiguration`): the panel gives its files
//! and takes a disk's paths. F5 / F6 to a disk ask NetBox's copy dialog; a disk's files into the panel
//! ask it after the program's; the default transfer settings are the configuration's (S6.5c). The
//! viewer's, the editor's and Find file's copies come with no questions and NetBox's settings for them
//! (`EditViewCopyParam`), the copy dialog only when the configuration asks ("Display download options
//! dialog"); the editor's copy goes back with the download's settings (`FLastEditCopyParam`), at each
//! save with "Upload after every save" (API 8), asked only with "Display upload options dialog" and no
//! upload at each save (NetBox's `EditorDownloadDefaultMode`, `EditorUploadSameOptions`,
//! `EditorUploadOnSave`). The work runs on the runtime while the
//! plugin's thread shows its questions and NetBox's progress window; Esc asks "Cancel current
//! operation?" and the work ends at its next piece (a second Esc after that drops it at once).

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nc_plugin_sdk::{Error, ErrorKind, GetRequest, Host, Message, PutRequest};

use crate::NetWay;
use crate::ask::{Asker, Question, Reply};
use crate::copy_dialog::{self, Copying};
use crate::copy_params::{NameCase, Resume};
use crate::copy_progress::{self, Shown};
use crate::copy_work::{CopyParams, Halted, Job, Progress};
use crate::options;
use crate::sftp::child;

/// NetBox's update of the window: twice a second.
const EVERY: Duration = Duration::from_millis(500);

/// NetBox's `NB_CANCEL_OPERATION`.
pub const CANCEL_QUESTION: &str = "Cancel current operation?";

fn cancelled() -> Error {
    Error::new(ErrorKind::Cancelled, "Cancelled")
}

/// NetBox's `EditViewCopyParam`: the viewer's and the editor's copies keep their names as they are, never
/// read only, never resumed.
pub fn edit_view_params(params: &mut CopyParams) {
    params.name_case = NameCase::NoChange;
    params.preserve_read_only = false;
    params.resume = Resume::Off;
    params.replace_invalid = true;
    params.file_mask.clear();
}

/// The server's folder typed in the copy dialog: from the panel's folder `current` when relative, without
/// its trailing `/` (the root's kept).
pub fn remote_folder(typed: &str, current: &str) -> String {
    let typed = typed.trim();
    let folder = match typed.starts_with('/') {
        true => typed.to_string(),
        false => child(current, typed),
    };
    match folder.trim_end_matches('/') {
        "" => "/".to_string(),
        trimmed => trimmed.to_string(),
    }
}

/// What a transfer or an operation on the server does.
pub(crate) enum Work<'a> {
    /// The disk's folder watched, its changes reflected on the server until stopped.
    KeepUpToDate {
        watch: &'a crate::keep_up_to_date::Watch,
        names: &'a [String],
    },
    /// A synchronization's checked items done, a file there overwritten without asking.
    Synchronize {
        items: &'a [crate::sync_collect::Item],
        local: &'a Path,
        remote: &'a str,
        timestamps: bool,
    },
    Download {
        folder: &'a str,
        names: &'a [String],
        target: &'a Path,
    },
    Upload {
        sources: &'a [PathBuf],
        target: &'a str,
    },
    Delete {
        folder: &'a str,
        names: &'a [String],
    },
    Folder {
        path: &'a str,
        rights: Option<u16>,
    },
    /// Moved (renamed) or duplicated into the target's folder with the mask.
    Move {
        folder: &'a str,
        names: &'a [String],
        target: (&'a str, &'a str),
        duplicate: bool,
    },
    Properties {
        folder: &'a str,
        names: &'a [String],
        properties: crate::remote_ops::Properties,
    },
    Link {
        path: &'a str,
        point_to: &'a str,
        symbolic: bool,
    },
}

impl Work<'_> {
    /// The work on the job.
    async fn run(&self, job: &Job<'_>) -> Result<(), Halted> {
        match self {
            Self::KeepUpToDate { watch, names } => {
                let quiet = Job {
                    fs: job.fs,
                    asker: job.asker,
                    progress: job.progress,
                    params: job.params.clone(),
                    moving: false,
                    temporary: false,
                    overwrite_all: true,
                };
                quiet.keep_up_to_date(watch, names).await
            }
            Self::Synchronize {
                items,
                local,
                remote,
                timestamps,
            } => {
                let quiet = Job {
                    fs: job.fs,
                    asker: job.asker,
                    progress: job.progress,
                    params: job.params.clone(),
                    moving: false,
                    temporary: false,
                    overwrite_all: true,
                };
                quiet.synchronize(items, (local, remote), *timestamps).await
            }
            Self::Download {
                folder,
                names,
                target,
            } => job.download(folder, names, target).await,
            Self::Upload { sources, target } => job.upload(sources, target).await,
            Self::Delete { folder, names } => job.delete(folder, names).await,
            Self::Folder { path, rights } => job.make_folder(path, *rights).await,
            Self::Move {
                folder,
                names,
                target,
                duplicate: true,
            } => job.duplicate(folder, names, *target).await,
            Self::Move {
                folder,
                names,
                target,
                ..
            } => job.move_files(folder, names, *target).await,
            Self::Properties {
                folder,
                names,
                properties,
            } => job.set_properties(folder, names, *properties).await,
            Self::Link {
                path,
                point_to,
                symbolic,
            } => job.link(path, point_to, *symbolic).await,
        }
    }
}

/// The progress window: when it shows next, the time questions took, a stop asked for.
struct Window<'a> {
    host: Host,
    shown: Shown,
    progress: &'a Progress,
    started: Instant,
    next: Cell<Instant>,
    paused: Cell<Duration>,
    stopping: Cell<bool>,
}

impl Window<'_> {
    /// Another window was over it from `since`: that time not counted, the window back at once.
    fn shown_again(&self, since: Instant) {
        self.paused.set(self.paused.get() + since.elapsed());
        self.next.set(Instant::now());
    }

    /// Asked every 50 ms while the work runs: Esc's question, the window when due; true drops the work.
    fn tick(&self) -> bool {
        if self.host.cancelled() {
            if self.stopping.get() {
                return true;
            }
            let since = Instant::now();
            let message =
                Message::new("Confirmation", &[CANCEL_QUESTION]).buttons(&["&Ok", "&Cancel"]);
            if self.host.show(&message) == Some(0) {
                self.progress.stop();
                self.stopping.set(true);
            }
            self.host.resume();
            self.shown_again(since);
        }
        let now = Instant::now();
        if now >= self.next.get() {
            self.next.set(now + EVERY);
            let worked = (now - self.started).saturating_sub(self.paused.get());
            let (title, lines) =
                copy_progress::window(self.shown, &self.progress.snapshot(), worked);
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            let _ = self.host.show(&Message::new(&title, &lines));
        }
        false
    }

    /// A transfer's question shown.
    fn answer(&self, question: Question) -> Reply {
        let since = Instant::now();
        let reply = match question {
            Question::Transfer(question) => {
                Reply::Transfer(crate::copy_questions::show(&self.host, &question))
            }
            _ => Reply::Nothing,
        };
        self.shown_again(since);
        reply
    }
}

impl NetWay {
    /// The entries `request` names in the server's folder `path` of the panel `instance` written to a
    /// disk.
    pub(crate) fn get_remote(
        &mut self,
        instance: &str,
        path: &str,
        request: &GetRequest,
    ) -> Result<(), Error> {
        if !self.reopen(instance, path) {
            return Err(cancelled());
        }
        let names: Vec<String> = (request.names.iter())
            .filter(|name| name.as_str() != "..")
            .cloned()
            .collect();
        if names.is_empty() {
            return Ok(());
        }
        let mode = request.mode;
        let temporary = mode.view || mode.edit || mode.quick_view || mode.find;
        let mut params = options::copy_params(&self.host);
        if temporary {
            edit_view_params(&mut params);
        }
        let mut target = request.destination.clone();
        // NetBox's `GetFilesRemote`: the viewer's and the editor's copies with the dialog only when the
        // configuration asks for it ("Display download options dialog").
        let asked =
            (mode.view || mode.edit) && !options::editor_options(&self.host).download_default;
        if mode.with_dialog() || asked {
            let copying = Copying {
                upload: false,
                moving: request.moving,
                names: &names,
                temporary,
            };
            let folder = request.destination.display().to_string();
            let base = request.folder.as_deref();
            let confirmed = copy_dialog::ask(&self.host, &copying, &folder, &params, base)
                .ok_or_else(cancelled)?;
            if !temporary {
                target = copy_dialog::local_folder(&confirmed.folder, base);
            }
            params = confirmed.params;
            if params.queue && !temporary {
                let disk = target.display().to_string();
                return self.handed_over(instance, (path, &disk), &names, (false, request.moving));
            }
        }
        if mode.edit {
            self.last_edit = Some(params.clone());
            // NetBox's edit history (`FEditHistories`): the files edited in the session.
            if let Some(remote) = self.remotes.get_mut(instance) {
                for name in &names {
                    remote.edited(path, name);
                }
            }
        }
        let shown = Shown {
            moving: request.moving,
            temporary,
            remote_source: true,
            operation: None,
        };
        let work = Work::Download {
            folder: path,
            names: &names,
            target: &target,
        };
        self.transfer(instance, shown, params, work)
    }

    /// A disk's files and folders `request` gives taken into the server's folder `path` of the panel
    /// `instance`.
    pub(crate) fn put_remote(
        &mut self,
        instance: &str,
        path: &str,
        request: &PutRequest,
    ) -> Result<(), Error> {
        if request.sources.is_empty() {
            return Ok(());
        }
        if !self.reopen(instance, path) {
            return Err(cancelled());
        }
        let edit = request.mode.edit;
        let mut target = path.to_string();
        // NetBox's `UploadFiles`: the editor's copy with the download's settings (`FLastEditCopyParam`),
        // asked only when the configuration says so and the copy goes back at the editor's leaving.
        let editor = options::editor_options(&self.host);
        let (mut params, asked) = match edit {
            true => {
                let mut last = options::copy_params(&self.host);
                edit_view_params(&mut last);
                let last = self.last_edit.clone().unwrap_or(last);
                (last, !editor.upload_same && !editor.upload_on_save)
            }
            false => (options::copy_params(&self.host), !request.mode.silent),
        };
        if asked {
            let names: Vec<String> = (request.sources.iter())
                .map(|source| source.display().to_string())
                .collect();
            let copying = Copying {
                upload: true,
                moving: request.moving,
                names: &names,
                temporary: edit,
            };
            let confirmed = copy_dialog::ask(&self.host, &copying, path, &params, None)
                .ok_or_else(cancelled)?;
            if !edit {
                target = remote_folder(&confirmed.folder, path);
            }
            params = confirmed.params;
            if params.queue && !edit {
                return self.handed_up(instance, &target, request);
            }
        }
        let shown = Shown {
            moving: request.moving,
            temporary: edit,
            remote_source: false,
            operation: None,
        };
        let work = Work::Upload {
            sources: &request.sources,
            target: &target,
        };
        self.transfer(instance, shown, params, work)
    }

    /// `work` on the panel `instance`'s connection, its questions and progress shown meanwhile.
    pub(crate) fn transfer(
        &self,
        instance: &str,
        shown: Shown,
        params: CopyParams,
        work: Work,
    ) -> Result<(), Error> {
        let (Some(remote), Some(runtime)) = (self.remotes.get(instance), self.runtime.as_ref())
        else {
            let text = format!("{instance}: the session is no longer connected");
            return Err(Error::new(ErrorKind::NotFound, text));
        };
        let (asker, mut questions) = Asker::new();
        let progress = Progress::default();
        let job = Job {
            fs: &remote.fs,
            asker: &asker,
            progress: &progress,
            params,
            moving: shown.moving,
            temporary: shown.temporary,
            overwrite_all: false,
        };
        let running = work.run(&job);
        let now = Instant::now();
        let window = Window {
            host: self.host,
            shown,
            progress: &progress,
            started: now,
            next: Cell::new(now),
            paused: Cell::new(Duration::ZERO),
            stopping: Cell::new(false),
        };
        let mut answering = |question| window.answer(question);
        match runtime.wait_answering(running, &|| window.tick(), &mut questions, &mut answering) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(Halted)) | Err(_) => Err(cancelled()),
        }
    }
}

#[cfg(test)]
#[path = "copy_panel_tests.rs"]
mod tests;
