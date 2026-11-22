//! The program's code pages as a plugin's thread asks for them: ANSI, OEM, UTF-8 and the pages the system
//! has decoded, others refused; the answer kept until the next.

use std::path::Path;
use std::sync::mpsc::channel;

use super::*;
use crate::plugin_host::ffi::HostContext;
use crate::plugin_host::log::PluginLog;

fn context() -> HostContext {
    let (sender, _events) = channel();
    // Under the test program's file: a folder that cannot be made, so nothing is ever written.
    let log = PluginLog::new(&std::env::current_exe().unwrap().join("Logs"), "probe");
    HostContext::new("probe", sender, Path::new(r"C:\plugins\probe"), log)
}

fn decoded(context: &HostContext, page: u32, bytes: &[u8]) -> Option<String> {
    let mut text = NcStr::EMPTY;
    let raw = std::ptr::from_ref(context).cast_mut().cast();
    // SAFETY: the context and the bytes live through the call, made on this thread; the answer is
    // copied before the next call.
    let done = unsafe { decode(raw, page, bytes.as_ptr(), bytes.len(), &raw mut text) };
    (done == 0).then(|| super::super::text(text))
}

#[test]
fn ascii_is_the_same_in_every_page_the_system_has() {
    let context = context();
    for page in [CODE_PAGE_ANSI, CODE_PAGE_OEM, UTF8] {
        assert_eq!(
            decoded(&context, page, b"C:\\a.txt").as_deref(),
            Some("C:\\a.txt")
        );
    }
    assert_eq!(decoded(&context, CODE_PAGE_OEM, b"").as_deref(), Some(""));
}

#[test]
fn utf8_is_decoded_and_a_page_the_system_has_not_is_refused() {
    let context = context();
    assert_eq!(
        decoded(&context, UTF8, "п".as_bytes()).as_deref(),
        Some("п")
    );
    assert_eq!(decoded(&context, 3, b"a"), None, "no page 3");
}

#[cfg(windows)]
#[test]
fn an_installed_page_by_its_number_is_decoded() {
    let context = context();
    // 866: the Cyrillic OEM page, installed with every Windows.
    assert_eq!(decoded(&context, 866, &[0xAF]).as_deref(), Some("п"));
    assert_eq!(decoded(&context, 1251, &[0xEF]).as_deref(), Some("п"));
}

#[cfg(windows)]
#[test]
fn a_path_utf8_cannot_hold_is_never_handed_over_as_another() {
    // Review R49: an unpaired surrogate was handed as U+FFFD, another name.
    use std::os::windows::ffi::OsStringExt;
    let odd = std::path::PathBuf::from(std::ffi::OsString::from_wide(&[
        u16::from(b'C'),
        u16::from(b':'),
        u16::from(b'\\'),
        0xD800,
    ]));
    assert!(super::exact(&odd).is_err());
    assert_eq!(
        super::folder_text(Some(&odd)),
        "",
        "no folder rather than another"
    );
    assert_eq!(
        super::exact(std::path::Path::new(r"C:\Привет")).as_deref(),
        Ok(r"C:\Привет")
    );
}
