//! A plugin's settings as its thread calls the host: the values it started with, one set read back at
//! once and sent to the program, none for a key never set.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::mpsc::{Receiver, channel};

use super::*;
use crate::plugin_host::ffi::HostContext;
use crate::plugin_host::log::PluginLog;

/// A plugin's context with `values`, and where its events come.
fn context(values: &[(&str, &str)]) -> (HostContext, Receiver<Event>) {
    let (sender, events) = channel();
    // Under the test program's file: a folder that cannot be made, so nothing is ever written.
    let log = PluginLog::new(&std::env::current_exe().unwrap().join("Logs"), "probe");
    let values: BTreeMap<String, String> = (values.iter())
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    let context = HostContext::new("probe", sender, Path::new(r"C:\plugins\probe"), log)
        .with_settings(values);
    (context, events)
}

fn raw(context: &HostContext) -> *mut c_void {
    std::ptr::from_ref(context).cast_mut().cast()
}

/// The value of `key` as the plugin reads it.
fn read(context: &HostContext, key: &str) -> Option<String> {
    let mut value = NcStr::EMPTY;
    // SAFETY: the context and the key live through the call, made on this thread; the answer is copied
    // before the next call.
    let found = unsafe { setting(raw(context), NcStr::new(key), &raw mut value) };
    (found == 0).then(|| text(value))
}

fn write(context: &HostContext, key: &str, value: &str) {
    // SAFETY: as in `read`.
    unsafe { set_setting(raw(context), NcStr::new(key), NcStr::new(value)) }
}

#[test]
fn the_values_the_plugin_started_with_are_read_and_others_are_none() {
    let (context, _events) = context(&[("Mask", "*.temp"), ("Empty", "")]);
    assert_eq!(read(&context, "Mask").as_deref(), Some("*.temp"));
    assert_eq!(
        read(&context, "Empty").as_deref(),
        Some(""),
        "empty is a value"
    );
    assert_eq!(read(&context, "mask"), None, "keys as they are written");
    assert_eq!(read(&context, "Prefix"), None);
    // SAFETY: a null answer is allowed: only whether the value is there.
    let found = unsafe { setting(raw(&context), NcStr::new("Mask"), std::ptr::null_mut()) };
    assert_eq!(found, 0);
}

#[test]
fn a_value_set_is_read_back_at_once_and_sent_to_the_program() {
    let (context, events) = context(&[("Mask", "*.temp")]);
    write(&context, "Mask", "*.lst");
    write(&context, "Prefix", "tmp");
    assert_eq!(read(&context, "Mask").as_deref(), Some("*.lst"));
    assert_eq!(read(&context, "Prefix").as_deref(), Some("tmp"));
    let sent: Vec<(String, String, String)> = events
        .try_iter()
        .map(|event| match event {
            Event::Setting { plugin, key, value } => (plugin, key, value),
            other => panic!("a setting: {other:?}"),
        })
        .collect();
    assert_eq!(
        sent,
        [
            ("probe".into(), "Mask".into(), "*.lst".into()),
            ("probe".into(), "Prefix".into(), "tmp".into())
        ]
    );
}

#[test]
fn a_value_set_with_nobody_listening_is_still_the_plugins() {
    let (context, events) = context(&[]);
    drop(events);
    write(&context, "Mode", "1");
    assert_eq!(read(&context, "Mode").as_deref(), Some("1"));
}

#[test]
fn another_thread_of_the_plugins_gets_the_failure_and_touches_nothing() {
    // Review R45: the context's cells are the plugin thread's alone.
    let (context, events) = context(&[("Mask", "*.temp")]);
    let _ = context.thread.set(std::thread::current().id());
    let address = raw(&context) as usize;
    let other = std::thread::spawn(move || {
        let raw = address as *mut c_void;
        let mut value = NcStr::EMPTY;
        // SAFETY: the context outlives this thread (joined below); the call must not touch it.
        let found = unsafe { setting(raw, NcStr::new("Mask"), &raw mut value) };
        // SAFETY: as above.
        unsafe { set_setting(raw, NcStr::new("Mask"), NcStr::new("other")) };
        found
    });
    assert_eq!(other.join().unwrap(), -1);
    assert_eq!(
        read(&context, "Mask").as_deref(),
        Some("*.temp"),
        "unchanged"
    );
    assert!(events.try_recv().is_err(), "nothing sent");
}
