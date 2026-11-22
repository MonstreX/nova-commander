//! A transfer at work (WinSCP's `TTerminal::CopyToLocal` / `CopyToRemote` over SFTP with NetBox's
//! defaults): the items walked first for their total size (`CalculateSize`), then each folder made and
//! each file sent in pieces, its time kept (`PreserveTime`), a moved source removed after it; a target
//! there asked over, or decided by an answer for all, by "New and updated file(s) only", or for the viewer
//! and the editor overwritten unasked (`cpNoConfirmation`); a failure asked about (abort, retry, skip,
//! skip all). It runs on the runtime: its questions go to the plugin's thread through the asker, its
//! progress through [`Progress`], and the user's stop ends it at the next piece, the file at hand removed
//! when it was being written anew. Downloads: `download`; uploads: `upload`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::fs::Fs;

use crate::ask::{Asker, Question, Reply};
use crate::copy_questions::{
    AppendOrResume, Failed, Overwrite, Resume, Side, TransferQuestion, TransferReply, failed_text,
    lines, overwrite_text,
};
use crate::file_masks::{FileMasks, Params};
use crate::text_mode::Throttle;

/// The size of a piece sent at once (SFTP's usual packet's data).
pub const PIECE: usize = 32 * 1024;

pub use crate::copy_params::CopyParams;

/// Where a transfer is, for its window; read on the plugin's thread while the work writes it.
#[derive(Debug, Default)]
pub struct Progress {
    total: AtomicU64,
    done: AtomicU64,
    file_size: AtomicU64,
    file_done: AtomicU64,
    file: Mutex<String>,
    folder: Mutex<String>,
    calculating: AtomicBool,
    stop: AtomicBool,
    /// The total size was not calculated ("Calculate total size" off): the files counted instead.
    uncounted: AtomicBool,
    files: AtomicU64,
    files_done: AtomicU64,
    started: Mutex<String>,
}

/// The progress as read at once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub total: u64,
    pub done: u64,
    pub file_size: u64,
    pub file_done: u64,
    /// The source at hand, by its full path.
    pub file: String,
    /// Its target's folder.
    pub folder: String,
    /// The items still being walked for their size.
    pub calculating: bool,
    /// NetBox's `TotalSizeSet`: false shows the start time and the files done.
    pub total_known: bool,
    pub files: u64,
    pub files_done: u64,
    /// The local time the transfer started.
    pub started: String,
}

impl Progress {
    pub fn snapshot(&self) -> Snapshot {
        let text = |kept: &Mutex<String>| kept.lock().map(|text| text.clone()).unwrap_or_default();
        Snapshot {
            total: self.total.load(Ordering::Relaxed),
            done: self.done.load(Ordering::Relaxed),
            file_size: self.file_size.load(Ordering::Relaxed),
            file_done: self.file_done.load(Ordering::Relaxed),
            file: text(&self.file),
            folder: text(&self.folder),
            calculating: self.calculating.load(Ordering::Relaxed),
            total_known: !self.uncounted.load(Ordering::Relaxed),
            files: self.files.load(Ordering::Relaxed),
            files_done: self.files_done.load(Ordering::Relaxed),
            started: text(&self.started),
        }
    }

    /// The user's stop: the work ends at its next piece.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn set(kept: &Mutex<String>, text: &str) {
        if let Ok(mut kept) = kept.lock() {
            text.clone_into(&mut kept);
        }
    }

    /// The walk at `folder`.
    pub(crate) fn walking(&self, folder: &str) {
        self.calculating.store(true, Ordering::Relaxed);
        Self::set(&self.file, folder);
    }

    /// The walk done: `total` bytes to send in `files` files; with `counted` off the total is not shown
    /// (WinSCP's `CalculateSize` off), the files done are.
    pub(crate) fn walked(&self, total: u64, files: u64, counted: bool) {
        self.calculating.store(false, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
        self.files.store(files, Ordering::Relaxed);
        self.uncounted.store(!counted, Ordering::Relaxed);
        let now = nc_plugin_sdk::system::local_time(nc_plugin_sdk::system::now_ticks());
        Self::set(&self.started, &now.map(|now| now.time).unwrap_or_default());
    }

    /// A file done (sent, skipped or failed and passed).
    pub(crate) fn file_done(&self) {
        self.files_done.fetch_add(1, Ordering::Relaxed);
    }

    /// The file `file` of `size` bytes into `folder` at hand.
    pub(crate) fn file(&self, file: &str, folder: &str, size: u64) {
        Self::set(&self.file, file);
        Self::set(&self.folder, folder);
        self.file_size.store(size, Ordering::Relaxed);
        self.file_done.store(0, Ordering::Relaxed);
    }

    /// The file at hand sent again from its start (a retry): what was counted of it taken back.
    pub(crate) fn restart_file(&self) {
        let counted = self.file_done.swap(0, Ordering::Relaxed);
        let _ = (self.done).try_update(Ordering::Relaxed, Ordering::Relaxed, |done| {
            Some(done.saturating_sub(counted))
        });
    }

    /// `bytes` more of the file at hand sent (or passed over).
    pub(crate) fn add(&self, bytes: u64) {
        self.file_done.fetch_add(bytes, Ordering::Relaxed);
        self.done.fetch_add(bytes, Ordering::Relaxed);
    }
}

/// The work ended before its end: the user stopped or cancelled it, or aborted on a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Halted;

/// A step's failure: its text, or the user's stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    Failed(String),
    Stopped,
}

/// A failure's text as a step's fault.
pub fn fault(error: impl std::fmt::Display) -> Fault {
    Fault::Failed(error.to_string())
}

/// WinSCP's batch overwrite modes an answer for all sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Batch {
    /// Yes to All.
    All,
    /// No to All.
    None,
    /// Newer only: a newer source over an older target.
    Older,
}

/// How a file is written where one is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Write {
    /// Left as it is.
    Skip,
    /// Written anew.
    Overwrite,
    /// The whole source after the target's `n` bytes.
    Append(u64),
    /// The source from the target's size on, after it.
    Resume(u64),
    /// Sent under another name in the same folder, anew.
    Renamed(String),
}

/// The answers given for all of a transfer's files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answers {
    pub overwrite: Option<Batch>,
    pub resume_all: bool,
    pub skip_all: bool,
}

/// WinSCP's `boOlder`: the source newer than the target, both times known.
pub fn is_newer(new: Side, existing: Side) -> bool {
    matches!((new.modified, existing.modified), (Some(new), Some(old)) if new > old)
}

/// What a batch mode does with a target there.
pub fn batch_write(batch: Batch, new: Side, existing: Side) -> Write {
    match batch {
        Batch::All => Write::Overwrite,
        Batch::None => Write::Skip,
        Batch::Older if is_newer(new, existing) => Write::Overwrite,
        Batch::Older => Write::Skip,
    }
}

/// A transfer's settings and where it reports.
pub struct Job<'a> {
    pub fs: &'a Fs,
    pub asker: &'a Asker,
    pub progress: &'a Progress,
    pub params: CopyParams,
    pub moving: bool,
    /// For the viewer or the editor: no questions, a file there overwritten.
    pub temporary: bool,
    /// A file there overwritten without asking (WinSCP's `spNoConfirmation` of synchronizing).
    pub overwrite_all: bool,
}

/// A target there: `name` (on the disk when `local`), the source `new` and the target `existing`.
pub struct There<'a> {
    pub name: &'a str,
    pub source: &'a str,
    pub local: bool,
    pub new: Side,
    pub existing: Side,
    /// Sent in the text mode: appended, never resumed (WinSCP's `CanAlternateResume`).
    pub text: bool,
}

impl Job<'_> {
    async fn ask(&self, question: TransferQuestion) -> Option<TransferReply> {
        match self.asker.ask(Question::Transfer(question)).await {
            Reply::Transfer(reply) => Some(reply),
            _ => None,
        }
    }

    /// How a file is written over the target there (WinSCP's `ConfirmFileOverwrite`).
    pub async fn decide(&self, answers: &mut Answers, there: &There<'_>) -> Result<Write, Halted> {
        let (new, existing) = (there.new, there.existing);
        if self.temporary || self.overwrite_all {
            return Ok(Write::Overwrite);
        }
        if self.params.newer_only {
            return Ok(batch_write(Batch::Older, new, existing));
        }
        if let Some(batch) = answers.overwrite {
            return Ok(batch_write(batch, new, existing));
        }
        let text = overwrite_text(there.name, there.local, new, existing);
        let question = TransferQuestion::Overwrite {
            lines: lines(&text),
            // SCP writes no file from an offset: no Append (WinSCP's SCP).
            append: self.fs.can(crate::fs::Capability::Resume),
        };
        let answer = match self.ask(question).await {
            Some(TransferReply::Overwrite(answer)) => answer,
            _ => Overwrite::Cancel,
        };
        let batch = match answer {
            Overwrite::Yes => return Ok(Write::Overwrite),
            Overwrite::No => return Ok(Write::Skip),
            Overwrite::Cancel => return Err(Halted),
            Overwrite::Append => return self.append(answers, there).await,
            Overwrite::NewName => return self.new_name(there.name).await,
            Overwrite::YesToAll => Batch::All,
            Overwrite::NoToAll => Batch::None,
            Overwrite::NewerOnly => Batch::Older,
        };
        answers.overwrite = Some(batch);
        Ok(batch_write(batch, new, existing))
    }

    /// Append: a shorter target asks whether to append or resume (WinSCP's `SFTPConfirmOverwrite`).
    async fn append(&self, answers: &mut Answers, there: &There<'_>) -> Result<Write, Halted> {
        let size = there.existing.size;
        if size >= there.new.size || there.text {
            return Ok(Write::Append(size));
        }
        if answers.resume_all {
            return Ok(Write::Resume(size));
        }
        let question = TransferQuestion::AppendOrResume(there.source.to_string());
        match self.ask(question).await {
            Some(TransferReply::AppendOrResume(AppendOrResume::Append)) => Ok(Write::Append(size)),
            Some(TransferReply::AppendOrResume(AppendOrResume::Resume)) => Ok(Write::Resume(size)),
            Some(TransferReply::AppendOrResume(AppendOrResume::ResumeAll)) => {
                answers.resume_all = true;
                Ok(Write::Resume(size))
            }
            _ => Err(Halted),
        }
    }

    /// WinSCP's `SFTPConfirmResume` of a partial file of `partial` bytes found for the target `name` of a
    /// source of `size`: the offset to resume from, or none (it is deleted and the file sent anew). The
    /// viewer's and the editor's transfers resume unasked unless it is bigger.
    pub async fn resume_partial(
        &self,
        name: &str,
        partial: u64,
        size: u64,
    ) -> Result<Option<u64>, Halted> {
        let bigger = partial > size;
        if self.temporary {
            return Ok((!bigger).then_some(partial));
        }
        let question = TransferQuestion::Resume {
            name: name.to_string(),
            bigger,
        };
        match self.ask(question).await {
            Some(TransferReply::Resume(Resume::Yes)) => Ok(Some(partial)),
            Some(TransferReply::Resume(Resume::No)) => Ok(None),
            _ => Err(Halted),
        }
    }

    /// New name: the name typed, Esc cancelling.
    async fn new_name(&self, name: &str) -> Result<Write, Halted> {
        match self.ask(TransferQuestion::NewName(name.to_string())).await {
            Some(TransferReply::NewName(Some(typed))) if !typed.trim().is_empty() => {
                Ok(Write::Renamed(typed.trim().to_string()))
            }
            _ => Err(Halted),
        }
    }

    /// A step on `file` failed with `fault`: retried (true), skipped (false) or the work halted.
    pub async fn failed(
        &self,
        answers: &mut Answers,
        file: &str,
        fault: Fault,
    ) -> Result<bool, Halted> {
        let headline = format!("Error transferring file '{file}'.");
        self.failed_about(answers, &headline, fault).await
    }

    /// A step failed with `fault`, WinSCP's `headline` first ("Error deleting file '...'."): retried
    /// (true), skipped (false) or the work halted.
    pub async fn failed_about(
        &self,
        answers: &mut Answers,
        headline: &str,
        fault: Fault,
    ) -> Result<bool, Halted> {
        let Fault::Failed(error) = fault else {
            return Err(Halted);
        };
        if answers.skip_all {
            return Ok(false);
        }
        let question = TransferQuestion::Failed(lines(&failed_text(headline, &error)));
        match self.ask(question).await {
            Some(TransferReply::Failed(Failed::Retry)) => Ok(true),
            Some(TransferReply::Failed(Failed::Skip)) => Ok(false),
            Some(TransferReply::Failed(Failed::SkipAll)) => {
                answers.skip_all = true;
                Ok(false)
            }
            _ => Err(Halted),
        }
    }

    /// The user's stop between pieces.
    pub fn check_stop(&self) -> Result<(), Fault> {
        match self.progress.stopped() {
            true => Err(Fault::Stopped),
            false => Ok(()),
        }
    }

    /// `work` on `file` until it is done (its result), skipped after a failure (none) or the work halted
    /// (WinSCP's `FILE_OPERATION_LOOP`).
    pub async fn retrying<T>(
        &self,
        answers: &mut Answers,
        file: &str,
        work: impl AsyncFnMut() -> Result<T, Fault>,
    ) -> Result<Option<T>, Halted> {
        let headline = format!("Error transferring file '{file}'.");
        self.retrying_about(answers, &headline, work).await
    }

    /// [`Job::retrying`] with WinSCP's `headline` of the step's failure.
    pub async fn retrying_about<T>(
        &self,
        answers: &mut Answers,
        headline: &str,
        mut work: impl AsyncFnMut() -> Result<T, Fault>,
    ) -> Result<Option<T>, Halted> {
        loop {
            match work().await {
                Ok(done) => return Ok(Some(done)),
                Err(fault) => {
                    if !self.failed_about(answers, headline, fault).await? {
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// The user's stop while walking.
    pub fn halt_if_stopped(&self) -> Result<(), Halted> {
        match self.progress.stopped() {
            true => Err(Halted),
            false => Ok(()),
        }
    }

    /// WinSCP's `AllowTransfer`: the transfer settings' file mask takes the item `path` (a disk's when
    /// `local`), a folder or a file of `size` and `modified`.
    pub fn allowed(&self, path: &str, local: bool, directory: bool, side: Side) -> bool {
        let mask = &self.params.include_mask;
        if mask.trim().is_empty() {
            return true;
        }
        let params = mask_params(side);
        FileMasks::parse(mask)
            .is_ok_and(|masks| masks.matches(path, local, directory, Some(params)))
    }

    /// WinSCP's `UseAsciiTransfer` of the file `path`.
    pub fn text_for(&self, path: &str, local: bool, side: Side) -> bool {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        self.params.use_text(name, local, mask_params(side))
    }

    /// The walk's steps told to the progress: their bytes and files.
    pub fn walked<S, T>(&self, steps: &[Step<S, T>]) {
        let total = steps.iter().map(Step::size).sum();
        let files = steps
            .iter()
            .filter(|step| matches!(step, Step::File { .. }))
            .count();
        let files = u64::try_from(files).unwrap_or(u64::MAX);
        (self.progress).walked(total, files, self.params.calculate_size);
    }

    /// The pause after a piece of `bytes` the speed limit asks for.
    pub async fn throttle(&self, throttle: &mut Throttle, bytes: u64) {
        let wait = throttle.after(bytes);
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

/// What a mask's size and time bounds read of a file.
fn mask_params(side: Side) -> Params {
    let modified = side
        .modified
        .map_or(0, |seconds| i64::try_from(seconds).unwrap_or(i64::MAX));
    Params {
        size: side.size,
        modified,
    }
}

/// One step of a transfer, `S` its source's path and `T` its target's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step<S, T> {
    /// A folder made.
    Folder { source: S, target: T },
    /// A file sent.
    File {
        source: S,
        target: T,
        size: u64,
        /// Seconds since 1970, when known.
        modified: Option<u64>,
        /// A server's file's rights, when known.
        rights: Option<u32>,
        /// Inside a link to a folder: copied, never removed by a move (FAR's `InsideReparsePoint`; R7
        /// review C2: a move deleted what the link led to).
        behind: bool,
    },
    /// A folder's items done: a moved folder removed - a link to a folder as a link (`link`), nothing
    /// inside one (`behind`).
    Leave { source: S, behind: bool, link: bool },
}

impl<S, T> Step<S, T> {
    /// The bytes it sends.
    pub fn size(&self) -> u64 {
        match self {
            Self::File { size, .. } => *size,
            _ => 0,
        }
    }
}

/// A disk's time of `seconds` since 1970.
pub fn system_time(seconds: u64) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds)
}

/// Seconds since 1970 of a disk's time.
pub fn unix_seconds(time: std::time::SystemTime) -> Option<u64> {
    let since = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(since.as_secs())
}

#[cfg(test)]
#[path = "copy_work_tests.rs"]
mod tests;
