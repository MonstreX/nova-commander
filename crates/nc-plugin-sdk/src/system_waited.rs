//! A call the system may hold for minutes (the network's browsing) waited for a while on a thread of its
//! own: past the wait the thread is let go to finish by itself, its answer handed to `late`. The plugin's
//! library is then kept loaded until the program ends (`GET_MODULE_HANDLE_EX_FLAG_PIN`): a thread left
//! running must never come back into a library let go (ADR-0019: no thread is ended from outside).

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_PIN, GetModuleHandleExW,
};

/// The library this code is in kept loaded until the process ends; false when the system refused.
pub fn keep_library_loaded() -> bool {
    let mut module: HMODULE = std::ptr::null_mut();
    let inside = keep_library_loaded as *const ();
    // SAFETY: the address is this function's, inside the library; the handle goes to a local and is
    // not freed (a pinned library is never let go).
    let done = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
            inside.cast(),
            &raw mut module,
        )
    };
    done != 0
}

/// Where the thread's answer goes: still waited for, given, the call broken (a panic), or the waiting
/// over.
enum Slot<T> {
    Waiting,
    Answered(T),
    Broken,
    LetGo,
}

/// What became of a call waited for.
#[derive(Debug, PartialEq, Eq)]
pub enum Waited<T> {
    /// Its answer, within the wait.
    Answered(T),
    /// The wait over: the thread finishes by itself and gives its answer to `late`.
    Late,
    /// No answer will come (`late` never hears): no thread could start, or the call broke (a panic).
    Failed,
}

/// `call` run on a thread of its own and waited for `wait`. Whether the answer is the caller's or
/// `late`'s is decided once, under the slot's lock.
pub fn waited<T: Send + 'static>(
    wait: Duration,
    call: impl FnOnce() -> T + Send + 'static,
    late: impl FnOnce(T) + Send + 'static,
) -> Waited<T> {
    let shared = Arc::new((Mutex::new(Slot::Waiting), Condvar::new()));
    let theirs = Arc::clone(&shared);
    let started = std::thread::Builder::new()
        .name("vc-waited".to_string())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(call));
            let (slot, answered) = &*theirs;
            let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
            match result {
                Ok(result) if matches!(*slot, Slot::LetGo) => {
                    drop(slot);
                    late(result);
                }
                Ok(result) => *slot = Slot::Answered(result),
                Err(_) if matches!(*slot, Slot::LetGo) => {}
                Err(_) => *slot = Slot::Broken,
            }
            answered.notify_all();
        });
    let Ok(thread) = started else {
        return Waited::Failed;
    };
    let (slot, answered) = &*shared;
    let guard = slot.lock().unwrap_or_else(PoisonError::into_inner);
    let (mut guard, _) = answered
        .wait_timeout_while(guard, wait, |slot| matches!(slot, Slot::Waiting))
        .unwrap_or_else(PoisonError::into_inner);
    let slot = std::mem::replace(&mut *guard, Slot::LetGo);
    drop(guard);
    match slot {
        // The thread has given its answer and only ends now: waited for, so that no code of the
        // library runs on it once the caller goes on (the library may be let go after).
        Slot::Answered(result) => {
            let ended = thread.join();
            debug_assert!(ended.is_ok(), "the call's panic was caught on the thread");
            Waited::Answered(result)
        }
        Slot::Broken => {
            let ended = thread.join();
            debug_assert!(ended.is_ok(), "the call's panic was caught on the thread");
            Waited::Failed
        }
        Slot::Waiting | Slot::LetGo => {
            keep_library_loaded();
            Waited::Late
        }
    }
}

#[cfg(test)]
#[path = "system_waited_tests.rs"]
mod tests;
