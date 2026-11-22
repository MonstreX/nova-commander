//! A job object that ends its processes with its last handle: what a process of the program starts and
//! waits for does not outlive it when it is killed (the runner killed with its user screen left the
//! command it waited for, a `cmd` asking for a line, running on a console no one closed).

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    DeleteProcThreadAttributeList, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_JOB_LIST, UpdateProcThreadAttribute,
};

/// The job of one process and everything it starts.
#[derive(Debug)]
pub(super) struct KillJob(HANDLE);

// SAFETY: a kernel handle may be used from any thread; one holder uses it at a time.
unsafe impl Send for KillJob {}

impl KillJob {
    /// A new job that kills its processes, and what they start, when its handle closes: when this
    /// value drops or the program dies.
    pub(super) fn new() -> Option<Self> {
        // SAFETY: no attributes (the handle is not inherited) and no name.
        let job = unsafe { CreateJobObjectW(null(), null()) };
        if job.is_null() {
            return None;
        }
        let job = Self(job);
        job.set_kill_on_close(true).then_some(job)
    }

    /// Puts a running `process` into a new job. What it started before is not in it: a process this
    /// program creates is better born in the job ([`Self::birth`]). `None` when the system does not
    /// allow it: then the process is not tied to the program.
    pub(super) fn around(process: HANDLE) -> Option<Self> {
        let job = Self::new()?;
        // SAFETY: both handles are live.
        (unsafe { AssignProcessToJobObject(job.0, process) } != 0).then_some(job)
    }

    /// What starts a process inside this job (`PROC_THREAD_ATTRIBUTE_JOB_LIST`): no moment when the
    /// program could die and leave it untied. `None` when the list cannot be made.
    pub(super) fn birth(&self) -> Option<JobBirth> {
        JobBirth::new(self.0)
    }

    /// Unties the processes from the program and closes the job: what the process started to go on
    /// running (`start notepad`, a server started in the background) outlives the one waited for.
    pub(super) fn release(self) {
        self.set_kill_on_close(false);
    }

    fn set_kill_on_close(&self, on: bool) -> bool {
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        if on {
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        }
        let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).unwrap_or(0);
        // SAFETY: the job's handle is live; `limits` is of the size given.
        unsafe {
            SetInformationJobObject(
                self.0,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size,
            ) != 0
        }
    }
}

impl Drop for KillJob {
    fn drop(&mut self) {
        // SAFETY: the handle is owned here and closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// A process attribute list naming one job, for `STARTUPINFOEXW`; it must outlive the job's use in
/// `CreateProcessW` and not outlive the job.
pub(super) struct JobBirth {
    /// The list's memory, of the size the system asked for (`usize`s keep it aligned).
    list: Vec<usize>,
    /// The job's handle, where the list points to it.
    jobs: Box<[HANDLE; 1]>,
}

impl JobBirth {
    fn new(job: HANDLE) -> Option<Self> {
        let mut size = 0usize;
        // SAFETY: a null list asks for the size only; the call fails with that size written.
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &raw mut size) };
        let attribute = usize::try_from(PROC_THREAD_ATTRIBUTE_JOB_LIST).ok()?;
        let mut list = vec![0usize; size.div_ceil(size_of::<usize>())];
        // SAFETY: `list` has the `size` bytes the system asked for.
        let made = unsafe {
            InitializeProcThreadAttributeList(list.as_mut_ptr().cast(), 1, 0, &raw mut size)
        };
        if made == 0 {
            return None;
        }
        // Made: from here `Drop` deletes it.
        let mut birth = Self {
            list,
            jobs: Box::new([job]),
        };
        // SAFETY: the list is made; `jobs` is boxed, so the value it points to stays where it is
        // while `birth` lives, which the caller keeps until the process is created.
        let set = unsafe {
            UpdateProcThreadAttribute(
                birth.list(),
                0,
                attribute,
                birth.jobs.as_ptr().cast(),
                size_of::<HANDLE>(),
                null_mut(),
                null(),
            )
        };
        (set != 0).then_some(birth)
    }

    /// For `STARTUPINFOEXW::lpAttributeList`.
    pub(super) fn list(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.list.as_mut_ptr().cast()
    }
}

impl Drop for JobBirth {
    fn drop(&mut self) {
        // SAFETY: a value exists only for a list made in `new`; it is deleted once.
        unsafe { DeleteProcThreadAttributeList(self.list()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    /// A `cmd` that starts a program and waits for it, some 5 s: a process and what it started.
    fn tree() -> std::process::Child {
        Command::new("cmd")
            .args(["/D", "/C", "ping -n 6 127.0.0.1 >NUL"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
    }

    /// Well before the tree would end by itself.
    fn ends_soon(child: &mut std::process::Child) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            if child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn a_process_born_in_the_job_is_in_it_from_the_start_and_ends_with_it() {
        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::System::JobObjects::IsProcessInJob;
        use windows_sys::Win32::System::Threading::WaitForSingleObject;
        let job = KillJob::new().unwrap();
        let mut birth = job.birth().expect("job lists (Windows 10)");
        let mut line: Vec<u16> = "cmd /D /C ping -n 6 127.0.0.1 >NUL\0"
            .encode_utf16()
            .collect();
        let folder: Vec<u16> = "C:\\\0".encode_utf16().collect();
        let process =
            super::super::execute::create_process(&mut line, &folder, 0, Some(&mut birth)).unwrap();
        drop(birth);
        let mut inside = 0;
        // SAFETY: both handles are live; `inside` is a live local.
        unsafe { IsProcessInJob(process, job.0, &raw mut inside) };
        assert_ne!(inside, 0, "born in the job");
        drop(job);
        // SAFETY: the handle is live; it is closed once below.
        let ended = unsafe { WaitForSingleObject(process, 2000) };
        // SAFETY: owned here, closed once.
        unsafe { CloseHandle(process) };
        assert_eq!(
            ended, WAIT_OBJECT_0,
            "ended with the job, long before ping would"
        );
    }

    #[test]
    fn closing_the_job_ends_its_processes_and_a_released_one_does_not() {
        let mut killed = tree();
        let job = KillJob::around(killed.as_raw_handle()).expect("jobs inside jobs (Windows 8)");
        drop(job);
        assert!(
            ends_soon(&mut killed),
            "the job closed with its process in it"
        );

        let mut released = tree();
        let job = KillJob::around(released.as_raw_handle()).unwrap();
        job.release();
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            released.try_wait().unwrap().is_none(),
            "released, it runs on"
        );
        released.kill().unwrap();
        let _ = released.wait();
    }
}
