//! Explorer's context menu for files and folders and This PC's items (FAR's emenu: `OleThread.cpp`,
//! `Plugin.cpp`), as a safe wrapper (owner, ADR-0016 addendum P6.0e). The shell's objects live on a thread
//! of their own (single-threaded COM with OLE, its messages pumped, as FAR's OLE thread), asked through a
//! channel. Every call waits with the plugin's stop (Esc, ADR-0019): a GUI menu is cancelled, a call that
//! does not come back within a second is let go - the thread keeps the plugin's module loaded until it
//! ends (`FreeLibraryAndExitThread`), so an unloaded plugin never runs away under it.

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    FreeLibraryAndExitThread, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GetModuleHandleExW,
};
use windows_sys::Win32::System::Ole::OleInitialize;
use windows_sys::Win32::System::Threading::{
    CreateMutexW, GetCurrentThreadId, INFINITE, ReleaseMutex, WaitForSingleObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU, VK_SHIFT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    TranslateMessage, WM_APP, WM_QUIT, WM_USER,
};

#[path = "system_shell_com.rs"]
mod com;
#[path = "system_shell_gui.rs"]
mod gui;
#[path = "system_shell_session.rs"]
mod session;

pub use gui::mouse_cell;

/// One item of a context menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellItem {
    /// The command's id to run it ([`Shell::invoke`]); 0 for a separator or a submenu.
    pub id: u32,
    /// Its text, `&` before its hotkey; empty for one its object draws.
    pub text: String,
    pub separator: bool,
    pub submenu: bool,
    pub checked: bool,
    /// Checked as one of a group (a bullet).
    pub radio: bool,
    pub disabled: bool,
    /// Drawn by its object (FAR's "-- owner drawn --").
    pub owner_drawn: bool,
    /// The command's verb ("open", "rename", "delete"), its help text.
    pub verb: Option<String>,
    pub help: Option<String>,
}

/// Which of a command's strings [`Shell::items`] asks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Strings {
    pub verb: bool,
    pub help: bool,
}

/// Where a GUI menu shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiAt {
    Mouse,
    /// The centre of the console window (its `HWND`, [`crate::Screen::window`]).
    Centre(usize),
}

/// The keys held now: Shift, Alt.
pub fn keys_held() -> (bool, bool) {
    // SAFETY: a key's state asked; no memory given.
    let held = |key| unsafe { GetAsyncKeyState(i32::from(key)) } < 0;
    (held(VK_SHIFT), held(VK_MENU))
}

/// What the thread is asked.
enum Ask {
    Computer,
    Paths(Vec<PathBuf>, bool),
    ComputerItem(usize, bool),
    Items(Vec<usize>, Strings),
    /// The thread busy for a while (the tests' call that does not answer).
    #[cfg(test)]
    Sleep(Duration),
    Verb(u32),
    Invoke(u32),
    Track(GuiAt),
}

/// Its answers.
enum Answer {
    Computer(String, Vec<String>),
    Done,
    Items(Vec<ShellItem>),
    Text(Option<String>),
    Chosen(Option<u32>),
}

struct Call {
    ask: Ask,
    reply: Sender<io::Result<Answer>>,
}

/// How the thread ended: whether it ended, whether it was let go before.
#[derive(Debug, Default)]
struct Ending {
    finished: bool,
    abandoned: bool,
}

/// The message that tells the thread to read its channel.
const ASK: u32 = WM_APP + 0x10;

/// The shell's thread and what it holds.
#[derive(Debug)]
pub struct Shell {
    asks: Sender<Call>,
    thread: u32,
    worker: Option<JoinHandle<()>>,
    ending: Arc<Mutex<Ending>>,
    shown: Arc<AtomicIsize>,
    abandoned: bool,
}

fn interrupted() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "stopped")
}

fn gone() -> io::Error {
    io::Error::other("the shell's thread ended")
}

/// The plugin's module held loaded for the thread (none when it cannot be).
fn pin_module() -> Option<HMODULE> {
    let mut module: HMODULE = std::ptr::null_mut();
    let address = pin_module as *const u16;
    // SAFETY: an address in this module; one reference taken, given back as the thread ends.
    let pinned = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
            address,
            &raw mut module,
        )
    } != 0;
    pinned.then_some(module)
}

/// The thread's messages until it is told to end; each ask answered as it comes.
/// The process's one lock on the shell's objects (FAR has one OLE thread a process): some Explorer
/// extensions fail when two threads use them at once (Revo Uninstaller's called a null pointer in the
/// tests). Named for the process, so every plugin's copy of this code shares it.
struct ShellLock(Option<OwnedHandle>);

impl ShellLock {
    fn new() -> Self {
        let name = format!(r"Local\vc-shell-{}", std::process::id());
        let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: no attributes, not owned at first, a NUL-terminated name; the handle checked and owned.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        // SAFETY: a handle just made, owned from here on.
        Self((!handle.is_null()).then(|| unsafe { OwnedHandle::from_raw_handle(handle) }))
    }

    /// `work` done holding the lock (one left by a thread that ended is taken over).
    fn hold<T>(&self, work: impl FnOnce() -> T) -> T {
        let Some(handle) = &self.0 else {
            return work();
        };
        // SAFETY: the mutex is live; owned on return (`WAIT_OBJECT_0` or `WAIT_ABANDONED`).
        unsafe { WaitForSingleObject(handle.as_raw_handle(), INFINITE) };
        let done = work();
        // SAFETY: owned by this thread since the wait.
        unsafe { ReleaseMutex(handle.as_raw_handle()) };
        done
    }
}

fn pump(asks: &Receiver<Call>, shown: &AtomicIsize) {
    let lock = ShellLock::new();
    let mut session = session::Session::default();
    loop {
        let mut message = MSG::default();
        // SAFETY: the message's place lives through it; any window of the thread.
        let got = unsafe { GetMessageW(&raw mut message, std::ptr::null_mut(), 0, 0) };
        if got == 0 || got == -1 {
            // The objects released under the lock too.
            lock.hold(|| drop(session));
            return;
        }
        if message.hwnd.is_null() && message.message == ASK {
            while let Ok(call) = asks.try_recv() {
                let answered = lock.hold(|| answer(&mut session, call.ask, shown));
                let _ = call.reply.send(answered);
            }
            continue;
        }
        // SAFETY: a message just taken from the thread's queue.
        unsafe {
            TranslateMessage(&raw const message);
            DispatchMessageW(&raw const message);
        }
    }
}

fn answer(session: &mut session::Session, ask: Ask, shown: &AtomicIsize) -> io::Result<Answer> {
    Ok(match ask {
        Ask::Computer => {
            let (title, names) = session.computer()?;
            Answer::Computer(title, names)
        }
        Ask::Paths(paths, extended) => {
            session.for_paths(&paths, extended).map(|()| Answer::Done)?
        }
        Ask::ComputerItem(index, extended) => session
            .for_computer(index, extended)
            .map(|()| Answer::Done)?,
        Ask::Items(at, strings) => Answer::Items(session.items(&at, strings)?),
        #[cfg(test)]
        Ask::Sleep(time) => {
            std::thread::sleep(time);
            Answer::Done
        }
        Ask::Verb(id) => Answer::Text(session.verb(id)?),
        Ask::Invoke(id) => session.invoke(id).map(|()| Answer::Done)?,
        Ask::Track(at) => {
            let (menu, popup) = session.open()?;
            Answer::Chosen(gui::track(menu, popup, at, shown)?)
        }
    })
}

/// The thread: OLE for it, its messages pumped, the module given back as it ends (with the thread's own
/// end when it was let go: nothing waits for it then).
fn run(
    asks: &Receiver<Call>,
    ready: &Sender<io::Result<u32>>,
    shown: &AtomicIsize,
    ending: &Mutex<Ending>,
) {
    let Some(module) = pin_module() else {
        let _ = ready.send(Err(io::Error::other(
            "the plugin's module could not be held",
        )));
        return;
    };
    // SAFETY: once on this thread, before any of its objects. Never undone (FAR's OLE thread neither):
    // `OleUninitialize` unloads the shell extensions' libraries the thread loaded, under the threads
    // some of them keep running - the process crashed so in the tests.
    let initialized = unsafe { OleInitialize(std::ptr::null()) } >= 0;
    let mut message = MSG::default();
    // SAFETY: the message's place lives through it; asked only so that the thread has its queue.
    unsafe {
        PeekMessageW(
            &raw mut message,
            std::ptr::null_mut(),
            WM_USER,
            WM_USER,
            PM_NOREMOVE,
        )
    };
    // SAFETY: no arguments.
    let thread = unsafe { GetCurrentThreadId() };
    let _ = ready.send(match initialized {
        true => Ok(thread),
        false => Err(io::Error::other("OLE could not start")),
    });
    if initialized {
        pump(asks, shown);
    }
    let abandoned = {
        let mut ending = ending.lock().unwrap_or_else(PoisonError::into_inner);
        ending.finished = true;
        ending.abandoned
    };
    match abandoned {
        // SAFETY: the reference `pin_module` took; nothing waits for this thread, which ends here.
        true => unsafe { FreeLibraryAndExitThread(module, 0) },
        // SAFETY: the reference `pin_module` took; the plugin waits for this thread, its module loaded.
        false => unsafe {
            FreeLibrary(module);
        },
    }
}

impl Shell {
    /// The shell's thread started.
    pub fn start() -> io::Result<Self> {
        let (asks, receiver) = mpsc::channel::<Call>();
        let (ready, started) = mpsc::channel();
        let ending = Arc::new(Mutex::new(Ending::default()));
        let shown = Arc::new(AtomicIsize::new(0));
        let (its_ending, its_shown) = (Arc::clone(&ending), Arc::clone(&shown));
        let worker = std::thread::Builder::new()
            .name("shell".to_string())
            .spawn(move || run(&receiver, &ready, &its_shown, &its_ending))?;
        // A thread that could not start ends at once: waited for, it never runs past its module.
        let thread = match started.recv() {
            Ok(Ok(thread)) => thread,
            Ok(Err(error)) => {
                let _ = worker.join();
                return Err(error);
            }
            Err(_) => {
                let _ = worker.join();
                return Err(gone());
            }
        };
        Ok(Self {
            asks,
            thread,
            worker: Some(worker),
            ending,
            shown,
            abandoned: false,
        })
    }

    /// Whether its thread was let go (a new `Shell` is needed).
    pub fn abandoned(&self) -> bool {
        self.abandoned
    }

    /// The thread let go while it does not answer (it ends by itself later).
    fn abandon(&mut self) {
        let mut ending = self.ending.lock().unwrap_or_else(PoisonError::into_inner);
        if !ending.finished {
            ending.abandoned = true;
            self.abandoned = true;
        }
    }

    fn call(&mut self, ask: Ask, stop: &dyn Fn() -> bool) -> io::Result<Answer> {
        if self.abandoned {
            return Err(interrupted());
        }
        let (reply, answer) = mpsc::channel();
        self.asks.send(Call { ask, reply }).map_err(|_| gone())?;
        // SAFETY: a message to the thread's queue, made before `start` returned.
        if unsafe { PostThreadMessageW(self.thread, ASK, 0, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut stopped: Option<Instant> = None;
        loop {
            match answer.recv_timeout(Duration::from_millis(50)) {
                Ok(answer) => return answer,
                Err(RecvTimeoutError::Disconnected) => return Err(gone()),
                Err(RecvTimeoutError::Timeout) => {}
            }
            if stopped.is_none() && stop() {
                gui::cancel(&self.shown);
                stopped = Some(Instant::now());
            }
            if stopped.is_some_and(|at| at.elapsed() > Duration::from_secs(1)) {
                self.abandon();
                return Err(interrupted());
            }
        }
    }

    /// This PC's name and its items' names, the file system's first (FAR's "My Computer" menu).
    pub fn computer(&mut self, stop: &dyn Fn() -> bool) -> io::Result<(String, Vec<String>)> {
        match self.call(Ask::Computer, stop)? {
            Answer::Computer(title, names) => Ok((title, names)),
            _ => Err(gone()),
        }
    }

    /// The context menu of `paths` (whole, all in one folder; a drive's root as `C:\`) made the open one,
    /// with the verbs Shift adds when it is held.
    pub fn menu_for_paths(&mut self, paths: &[PathBuf], stop: &dyn Fn() -> bool) -> io::Result<()> {
        let extended = keys_held().0;
        self.call(Ask::Paths(paths.to_vec(), extended), stop)
            .map(|_| ())
    }

    /// The context menu of This PC's item `index` (of [`Shell::computer`]'s) made the open one.
    pub fn menu_for_computer(&mut self, index: usize, stop: &dyn Fn() -> bool) -> io::Result<()> {
        let extended = keys_held().0;
        self.call(Ask::ComputerItem(index, extended), stop)
            .map(|_| ())
    }

    /// The open menu's items at `at` (the positions of the submenus from the top; none: the top), with
    /// the commands' verbs and help texts asked as `strings` says (an item its object draws: both; FAR asks
    /// only for what it shows, some handlers fail there).
    pub fn items(
        &mut self,
        at: &[usize],
        strings: Strings,
        stop: &dyn Fn() -> bool,
    ) -> io::Result<Vec<ShellItem>> {
        match self.call(Ask::Items(at.to_vec(), strings), stop)? {
            Answer::Items(items) => Ok(items),
            _ => Err(gone()),
        }
    }

    /// The verb of the open menu's command `id`.
    pub fn verb(&mut self, id: u32, stop: &dyn Fn() -> bool) -> io::Result<Option<String>> {
        match self.call(Ask::Verb(id), stop)? {
            Answer::Text(verb) => Ok(verb),
            _ => Err(gone()),
        }
    }

    /// The open menu's command `id` run.
    pub fn invoke(&mut self, id: u32, stop: &dyn Fn() -> bool) -> io::Result<()> {
        self.call(Ask::Invoke(id), stop).map(|_| ())
    }

    /// The open menu shown as Windows shows it, at `at`: the command chosen, none when it was closed.
    pub fn track(&mut self, at: GuiAt, stop: &dyn Fn() -> bool) -> io::Result<Option<u32>> {
        self.shown.store(0, Ordering::SeqCst);
        match self.call(Ask::Track(at), stop)? {
            Answer::Chosen(chosen) => Ok(chosen),
            _ => Err(gone()),
        }
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        // SAFETY: a message to the thread's queue; it ends its pump.
        unsafe { PostThreadMessageW(self.thread, WM_QUIT, 0, 0) };
        if let Some(worker) = self.worker.take()
            && !self.abandoned
        {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
#[path = "system_shell_tests.rs"]
mod tests;
