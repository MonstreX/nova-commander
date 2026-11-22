//! A plugin's thread as the host sees it (ADR-0016): the jobs it is sent, one at a time, and its start and
//! end.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

use super::calls::{Call, CallState, Calls};
use super::ffi::{self, HostContext};
use super::run;
use crate::vfs::Entry;

/// How long the plugins may take to start (side by side, review R17), and one to end when the program does.
pub(super) const START: Duration = Duration::from_secs(10);
pub(super) const END: Duration = Duration::from_secs(2);

/// What a plugin's thread is asked to do.
pub(super) enum Job {
    /// An item of a place (`PLACE_*`) chosen for the panel `target`.
    Run {
        place: u32,
        index: usize,
        target: usize,
        /// The program's number for the call: its end says it (ADR-0019).
        serial: u64,
    },
    /// A folder of a panel (its provider waits for the answer).
    List {
        instance: String,
        path: String,
        reply: Sender<io::Result<Vec<Entry>>>,
    },
    /// How a panel looks at a folder: its title and modes.
    Look {
        instance: String,
        path: String,
        reply: Sender<crate::vfs::PanelLook>,
    },
    /// A panel no longer shown.
    Close(String),
    /// A key of a panel's own (nobody waits: its end comes as [`Event::Key`]).
    Key {
        instance: String,
        path: String,
        press: ffi::KeyPress,
        side: usize,
        serial: u64,
    },
    /// `prefix:command` from the command line for the panel `side` (its answer comes as
    /// [`Event::Key`] for the press `serial`).
    Command {
        prefix: String,
        command: String,
        folder: Option<PathBuf>,
        side: usize,
        serial: u64,
    },
    /// A file of the plugin's masks, Enter or (`alternative`) Ctrl+PgDn on it in the panel `side` (its
    /// answer comes as [`Event::Key`]).
    OpenFile {
        path: PathBuf,
        alternative: bool,
        folder: Option<PathBuf>,
        side: usize,
        serial: u64,
    },
    /// Alt+F7's found files for the panel `side` (its answer comes as [`Event::Key`]).
    FindList {
        found: Vec<crate::vfs::Entry>,
        side: usize,
        serial: u64,
    },
    /// Paths put into a panel's folder (nobody waits: its end comes as [`Event::Put`]).
    Put {
        instance: String,
        path: String,
        files: ffi::PutFiles,
    },
    /// Files of a panel's folder written out by the plugin for the press `serial` (FAR's `GetFilesW`; its
    /// end comes as [`Event::Got`]).
    Get {
        instance: String,
        path: String,
        files: ffi::GetFiles,
        serial: u64,
    },
    /// Entries of a panel's folder deleted by the plugin for the press `serial`, its own question first
    /// (FAR's `DeleteFilesW`; its end comes as [`Event::Got`]).
    Delete {
        instance: String,
        path: String,
        names: Vec<String>,
        serial: u64,
    },
    /// A folder made by the plugin in a panel's folder for the press `serial`, the name asked by it
    /// (FAR's `MakeDirectoryW`; its end comes as [`Event::Made`]).
    MakeFolder {
        instance: String,
        path: String,
        serial: u64,
    },
    /// Entries of a panel's folder handed to the plugin's commands on the file the panel was opened
    /// from, for the press `serial` (FAR's `ProcessHostFileW`, Shift+F3; its end comes as
    /// [`Event::Processed`]).
    HostFile {
        instance: String,
        path: String,
        names: Vec<String>,
        serial: u64,
    },
    /// A disk's file offered for Shift+F2 / Shift+F3 for the press `serial` on the panel `side`
    /// (`plugin_host::disk_files`; its end comes as [`Event::DiskFile`], not taken as [`Event::Key`]
    /// passing it on).
    DiskFile {
        path: PathBuf,
        purpose: super::disk_files::DiskUse,
        folder: Option<PathBuf>,
        side: usize,
        serial: u64,
    },
    /// Shift+F1's files of the panel `side` put into a new panel of the plugin's for the press `serial`
    /// (FAR's `OpenFilePlugin` with `OFP_CREATE`, then `PutFilesW` and `ClosePanel`): its end comes as
    /// [`Event::Created`], a plugin that makes no such panel as [`Event::Key`] passing it on.
    Create {
        files: ffi::PutFiles,
        side: usize,
        serial: u64,
    },
    /// What it asked for with `NcHost::synchro` (FAR's `ProcessSynchroEventW`).
    Synchro(u64),
    /// A key the editor has done, by its name, for a plugin that takes them (API 10); its end comes
    /// as [`Event::Ran`] for the press `serial`.
    EditorKey { key: String, serial: u64 },
    /// Its items in the program's menus told again (FAR's `GetPluginInfo` as a menu opens), quietly.
    Items { reply: Sender<super::PluginItems> },
    /// A call on one file of a panel (its provider waits for the answer).
    File {
        instance: String,
        path: String,
        call: ffi::FileCall,
        reply: Sender<io::Result<ffi::FileAnswer>>,
    },
    /// A disk's file opened for Alt+F7's search in archives (`OPEN_SEARCH`, `plugin_host::search_open`):
    /// the instance of the panel opened, none when not taken (the search waits for the answer).
    SearchOpen {
        path: PathBuf,
        reply: Sender<Option<String>>,
    },
}

/// A plugin's thread: it loads the library, then runs the jobs sent to it until the host drops.
#[derive(Debug)]
pub(super) struct Worker {
    /// Shared with the plugin's provider; closed when the plugin ends, which ends its thread.
    pub(super) jobs: Arc<Calls>,
    ended: Receiver<()>,
    /// Whether its end is waited for as it is dropped (not for one that never started in time).
    wait_end: bool,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Run { place, index, .. } => write!(formatter, "Run({place}, {index})"),
            Self::List { path, .. } => write!(formatter, "List({path})"),
            Self::Look { path, .. } => write!(formatter, "Look({path})"),
            Self::Close(instance) => write!(formatter, "Close({instance})"),
            Self::Put { path, files, .. } => write!(formatter, "Put({path}, {:?})", files.sources),
            Self::Get { path, files, .. } => write!(formatter, "Get({path}, {:?})", files.names),
            Self::Delete { path, names, .. } => write!(formatter, "Delete({path}, {names:?})"),
            Self::MakeFolder { path, .. } => write!(formatter, "MakeFolder({path})"),
            Self::HostFile { path, names, .. } => write!(formatter, "HostFile({path}, {names:?})"),
            Self::DiskFile { path, purpose, .. } => {
                write!(formatter, "DiskFile({}, {purpose:?})", path.display())
            }
            Self::Create { files, .. } => write!(formatter, "Create({:?})", files.sources),
            Self::Key { path, press, .. } => write!(formatter, "Key({path}, {})", press.key),
            Self::Command {
                prefix, command, ..
            } => write!(formatter, "Command({prefix}:{command})"),
            Self::OpenFile { path, .. } => write!(formatter, "OpenFile({})", path.display()),
            Self::FindList { found, .. } => write!(formatter, "FindList({})", found.len()),
            Self::File { path, call, .. } => write!(formatter, "File({path}, {call:?})"),
            Self::Items { .. } => write!(formatter, "Items"),
            Self::Synchro(param) => write!(formatter, "Synchro({param})"),
            Self::EditorKey { key, .. } => write!(formatter, "EditorKey({key})"),
            Self::SearchOpen { path, .. } => write!(formatter, "SearchOpen({})", path.display()),
        }
    }
}

/// A plugin's thread started, its start not told yet.
pub(super) struct Starting {
    worker: Worker,
    hello: Receiver<run::Started>,
}

/// A start looked at without waiting: told, or still to come.
pub(super) enum Poll {
    Told(Box<Result<(Worker, run::Told), String>>),
    Waiting(Starting),
}

impl Starting {
    /// What the plugin told as it started, waited for until `deadline` (the plugins start side by side:
    /// one deadline for all, review R17).
    pub(super) fn told(self, deadline: std::time::Instant) -> Result<(Worker, run::Told), String> {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match self.hello.recv_timeout(left) {
            Ok(Ok(told)) => Ok((self.worker, told)),
            Ok(Err(reason)) => Err(reason),
            Err(RecvTimeoutError::Timeout) => {
                // Hung in its start: let go at once, the others' starts not held behind it (R17).
                let mut worker = self.worker;
                worker.wait_end = false;
                drop(worker);
                Err("The plugin did not start in time".to_string())
            }
            Err(RecvTimeoutError::Disconnected) => Err("The plugin's thread ended".to_string()),
        }
    }

    /// What the plugin told as it started, if it has; past `deadline` it is let go (a plugin loaded while
    /// the program runs: the screen never waits for its start).
    pub(super) fn poll(self, deadline: std::time::Instant) -> Poll {
        match self.hello.try_recv() {
            Ok(Ok(told)) => Poll::Told(Box::new(Ok((self.worker, told)))),
            Ok(Err(reason)) => Poll::Told(Box::new(Err(reason))),
            Err(std::sync::mpsc::TryRecvError::Empty) if std::time::Instant::now() < deadline => {
                Poll::Waiting(self)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.worker.let_go();
                Poll::Told(Box::new(
                    Err("The plugin did not start in time".to_string()),
                ))
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Poll::Told(Box::new(Err("The plugin's thread ended".to_string())))
            }
        }
    }
}

impl Worker {
    /// The plugin's thread started: it loads the library and starts the plugin on its own; a `copied`
    /// library (a copy of the plugin's, P6.0e) is removed as the thread ends, its plugin released.
    pub(super) fn spawn(
        library: PathBuf,
        copied: bool,
        context: HostContext,
    ) -> Result<Starting, String> {
        let (jobs, received) = channel::<Call>();
        let state = Arc::new(CallState::default());
        let context = context.with_calls(Arc::clone(&state));
        let (started, hello) = channel::<run::Started>();
        let (end, ended) = channel();
        let events = context.events.clone();
        std::thread::Builder::new()
            .name(format!("plugin {}", context.plugin))
            .spawn(move || {
                run::run_plugin(&library, context, &received, &started, &events);
                if copied {
                    let _ = std::fs::remove_file(&library);
                }
                let _ = end.send(());
            })
            .map_err(|error| error.to_string())?;
        let worker = Self {
            jobs: Arc::new(Calls::new(jobs, state)),
            ended,
            wait_end: true,
        };
        Ok(Starting { worker, hello })
    }

    /// The job sent to the plugin's thread: its call's number; none when the plugin is gone.
    pub(super) fn send(&self, job: Job) -> Option<u64> {
        self.jobs.send(job)
    }

    /// The plugin let go while the program runs (P6.0e): its call asked to stop, nothing it asks heard;
    /// its thread releases it and unloads its library once that call ends, the screen never waiting. A
    /// call that never ends keeps its thread and library to the program's end.
    pub(super) fn let_go(mut self) {
        self.jobs.state.let_go();
        self.wait_end = false;
    }
}

impl Drop for Worker {
    /// The thread is told to end (its channel closes: the provider's line goes too) and given a moment;
    /// one that does not end is left to the program's end.
    fn drop(&mut self) {
        self.jobs.close();
        if self.wait_end {
            let _ = self.ended.recv_timeout(END);
        }
    }
}
