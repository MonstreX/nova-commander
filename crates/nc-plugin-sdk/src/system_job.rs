//! A program a plugin starts kept under control (ADR-0019): started suspended in a job of its own (Windows'
//! job object, ended with the job's last handle), then let run, so whatever it starts is in the job too;
//! ended with all of it on [`Job::end`] (Esc), and when the job is dropped.

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
};

/// A program running in a job of its own.
#[derive(Debug)]
pub struct Job {
    job: OwnedHandle,
    child: Child,
}

/// A new job whose programs end with its last handle.
fn new_job() -> io::Result<OwnedHandle> {
    // SAFETY: no attributes, no name.
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a handle just made, owned from here on.
    let job = unsafe { OwnedHandle::from_raw_handle(job) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).unwrap_or(0);
    // SAFETY: the job is live; `limits` is of the size given.
    let set = unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size,
        )
    };
    if set == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(job)
}

/// The threads of the process `id` let run (it was started suspended); an error when none could be.
fn resume(id: u32) -> io::Result<()> {
    // SAFETY: a snapshot of the threads; checked below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a handle just made, owned from here on.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
    let mut entry = THREADENTRY32 {
        dwSize: u32::try_from(size_of::<THREADENTRY32>()).unwrap_or(0),
        ..THREADENTRY32::default()
    };
    // SAFETY: the snapshot is live; `entry` is of the size it says.
    let mut more = unsafe { Thread32First(snapshot.as_raw_handle(), &raw mut entry) } != 0;
    let mut resumed = 0usize;
    while more {
        if entry.th32OwnerProcessID == id {
            // SAFETY: the thread's id from the snapshot; the handle is checked and closed.
            let thread: HANDLE =
                unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if !thread.is_null() {
                // SAFETY: a handle just opened, owned from here on.
                let thread = unsafe { OwnedHandle::from_raw_handle(thread) };
                // SAFETY: the thread is open for resuming.
                let before = unsafe { ResumeThread(thread.as_raw_handle()) };
                resumed += usize::from(before != u32::MAX);
            }
        }
        // SAFETY: as `Thread32First`.
        more = unsafe { Thread32Next(snapshot.as_raw_handle(), &raw mut entry) } != 0;
    }
    if resumed == 0 {
        return Err(io::Error::other("the program could not be let run"));
    }
    Ok(())
}

impl Job {
    /// `command` started with no console window of its own (its standard streams as set), in a job.
    pub fn start(command: &mut Command) -> io::Result<Self> {
        let job = new_job()?;
        let mut child = command
            .creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW)
            .spawn()?;
        // SAFETY: both handles are live.
        let assigned =
            unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) } != 0;
        if !assigned {
            let error = io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        resume(child.id())?;
        Ok(Self { job, child })
    }

    /// How it ended, waiting at most `time`; none while it runs.
    pub fn wait_for(&mut self, time: Duration) -> io::Result<Option<ExitStatus>> {
        let until = Instant::now() + time;
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(Some(status));
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            std::thread::sleep(left.min(Duration::from_millis(20)));
        }
    }

    /// The program ended with all it started.
    pub fn end(&mut self) {
        // SAFETY: the job is live.
        let ended = unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } != 0;
        if !ended {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }

    /// How many of its programs still run (the job's `ActiveProcesses`).
    pub fn running(&self) -> u32 {
        let mut information = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        let size = u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>()).unwrap_or(0);
        // SAFETY: the job is live; the buffer is of the size given; no length asked back.
        let read = unsafe {
            QueryInformationJobObject(
                self.job.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                (&raw mut information).cast(),
                size,
                std::ptr::null_mut(),
            )
        };
        if read == 0 {
            0
        } else {
            information.ActiveProcesses
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_runs_to_its_end_and_one_ended_stops_with_what_it_started() {
        let mut quick = Command::new("cmd");
        quick.args(["/C", "exit 3"]);
        let mut job = Job::start(&mut quick).unwrap();
        let status = job.wait_for(Duration::from_secs(10)).unwrap().unwrap();
        assert_eq!(status.code(), Some(3));
        let mut slow = Command::new("cmd");
        slow.args(["/C", "ping -n 30 127.0.0.1 >nul"]);
        let mut job = Job::start(&mut slow).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            job.running() >= 2,
            "cmd and the ping it started are in the job"
        );
        assert_eq!(
            job.wait_for(Duration::from_millis(200)).unwrap(),
            None,
            "runs"
        );
        job.end();
        assert_eq!(job.running(), 0, "all it started ended too");
        assert!(
            job.wait_for(Duration::from_secs(5)).unwrap().is_some(),
            "ended"
        );
    }
}
