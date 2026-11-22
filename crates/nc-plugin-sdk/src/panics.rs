//! The library's panics: kept for the call's failure, never printed over the host's screen.

use std::cell::RefCell;

thread_local! {
    /// What the last panic on this thread said (the library's own hook keeps it off the host's screen).
    static PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The library's panics are kept for the call's failure, never printed over the host's screen. The hook
/// is the library's own (a plugin links its own standard library); [`export_plugin!`](crate::export_plugin)
/// installs it.
#[doc(hidden)]
pub fn keep_panics() {
    std::panic::set_hook(Box::new(|info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(ToString::to_string)
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic".to_string());
        PANIC.with(|panic| *panic.borrow_mut() = Some(message));
    }));
}

/// What a panic said, for the call's failure.
pub(crate) fn panic_text() -> String {
    let message = PANIC.with(|panic| panic.borrow_mut().take());
    format!(
        "The plugin failed: {}",
        message.unwrap_or_else(|| "a panic".to_string())
    )
}
