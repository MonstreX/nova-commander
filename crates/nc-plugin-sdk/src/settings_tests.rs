//! A plugin's settings through a host faked in this process (its values in a map), and its configuration
//! items and call through the SDK's table (`PLACE_CONFIG`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::c_void;

use crate::api::{self, API_VERSION, NcHost, NcStr, PLACE_CONFIG};
use crate::tests::{host, text};
use crate::{Host, Info, Plugin, entry};

/// The fake host's values, and the last text it answered with (kept as the real host keeps it).
#[derive(Default)]
struct Values {
    map: RefCell<BTreeMap<String, String>>,
    answer: RefCell<String>,
}

/// The fake behind the host's pointer.
///
/// # Safety
/// `context` is a live `Values`.
unsafe fn values<'a>(context: *mut c_void) -> &'a Values {
    // SAFETY: as the caller promises.
    unsafe { &*context.cast::<Values>() }
}

unsafe extern "C" fn setting(context: *mut c_void, key: NcStr, value: *mut NcStr) -> i32 {
    // SAFETY: the tests' context is a live `Values`; the key lives through the call; the answer stays in
    // `answer` until the next call.
    unsafe {
        let values = values(context);
        let Some(found) = values.map.borrow().get(key.as_str()).cloned() else {
            return -1;
        };
        *values.answer.borrow_mut() = found;
        *value = NcStr::new(&values.answer.borrow());
        0
    }
}

unsafe extern "C" fn set_setting(context: *mut c_void, key: NcStr, value: NcStr) {
    // SAFETY: as in `setting`.
    unsafe {
        let values = values(context);
        (values.map.borrow_mut()).insert(key.as_str().to_string(), value.as_str().to_string());
    }
}

/// The host's table over `values`.
fn table(values: &Values) -> NcHost {
    NcHost {
        context: std::ptr::from_ref(values).cast_mut().cast(),
        setting,
        set_setting,
        ..host(API_VERSION)
    }
}

#[test]
fn a_setting_is_read_back_as_it_was_set_and_none_is_none() {
    let values = Values::default();
    let table = table(&values);
    let host = Host {
        table: &raw const table,
    };
    assert_eq!(host.setting("Mask"), None);
    host.set_setting("Mask", "*.temp,*.lst");
    assert_eq!(host.setting("Mask").as_deref(), Some("*.temp,*.lst"));
    host.set_setting("Mask", "");
    assert_eq!(
        host.setting("Mask").as_deref(),
        Some(""),
        "an empty value is one"
    );
    assert_eq!(values.map.borrow().len(), 1);
}

#[test]
fn flags_and_numbers_are_text_with_their_defaults() {
    let values = Values::default();
    let table = table(&values);
    let host = Host {
        table: &raw const table,
    };
    assert!(
        host.bool_setting("AddToDisksMenu", true),
        "none: the default"
    );
    assert!(!host.bool_setting("SafeModePanel", false));
    host.set_bool("AddToDisksMenu", false);
    host.set_bool("SafeModePanel", true);
    assert_eq!(values.map.borrow()["AddToDisksMenu"], "0");
    assert_eq!(values.map.borrow()["SafeModePanel"], "1");
    assert!(!host.bool_setting("AddToDisksMenu", true));
    assert!(host.bool_setting("SafeModePanel", false));
    host.set_setting("SafeModePanel", "yes");
    assert!(
        !host.bool_setting("SafeModePanel", false),
        "neither 1 nor 0: the default"
    );
    assert_eq!(host.int_setting("CopyContents", 0), 0);
    host.set_int("CopyContents", 2);
    assert_eq!(values.map.borrow()["CopyContents"], "2");
    assert_eq!(host.int_setting("CopyContents", 0), 2);
    host.set_int("Offset", -15);
    assert_eq!(host.int_setting("Offset", 0), -15);
    host.set_setting("CopyContents", "two");
    assert_eq!(host.int_setting("CopyContents", 1), 1, "not a number");
}

/// A plugin with two configuration items: the first counts in its settings and says it changed them, the
/// second changes nothing, a third panics.
struct Configured {
    host: Host,
}

impl Plugin for Configured {
    fn new(host: Host) -> Self {
        Self { host }
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn config_items(&self) -> Vec<String> {
        vec!["Count".to_string(), "Look".to_string()]
    }

    fn configure(&mut self, index: usize) -> bool {
        match index {
            0 => {
                let count = self.host.int_setting("count", 0);
                self.host.set_int("count", count + 1);
                true
            }
            1 => false,
            _ => panic!("no such configuration"),
        }
    }
}

#[test]
fn the_configuration_items_and_their_call_go_through_the_table() {
    let values = Values::default();
    let host = table(&values);
    // SAFETY: the host's table and its values live until the plugin is released at the end; one call
    // at a time with the table's own state, none after `release`.
    unsafe {
        let table = &*entry::<Configured>(&raw const host);
        let state = table.state;
        assert_eq!((table.item_count)(state, PLACE_CONFIG), 2);
        assert_eq!(text((table.item)(state, PLACE_CONFIG, 1)), "Look");
        assert_eq!(
            (table.item_count)(state, api::PLACE_COMMANDS),
            0,
            "the places keep their own items"
        );
        let mut answer = NcStr::EMPTY;
        assert_eq!((table.run_item)(state, PLACE_CONFIG, 0, &raw mut answer), 0);
        assert!(!text(answer).is_empty(), "changed: a non-empty answer");
        assert_eq!(values.map.borrow()["count"], "1");
        let mut answer = NcStr::EMPTY;
        assert_eq!((table.run_item)(state, PLACE_CONFIG, 1, &raw mut answer), 0);
        assert_eq!(text(answer), "", "nothing changed: no answer");
        let mut answer = NcStr::EMPTY;
        assert_eq!((table.run_item)(state, PLACE_CONFIG, 2, &raw mut answer), 1);
        assert!(text((table.last_error)(state)).starts_with("The plugin failed: "));
        (table.release)(state);
    }
}

/// A plugin with nothing of its own.
struct Plain;

impl Plugin for Plain {
    fn new(_host: Host) -> Self {
        Self
    }

    fn info(&self) -> Info {
        Info::default()
    }
}

#[test]
fn a_plugin_without_configuration_has_no_items_and_changes_nothing() {
    let host = host(API_VERSION);
    // SAFETY: as above.
    unsafe {
        let table = &*entry::<Plain>(&raw const host);
        let state = table.state;
        assert_eq!((table.item_count)(state, PLACE_CONFIG), 0);
        let mut answer = NcStr::EMPTY;
        assert_eq!((table.run_item)(state, PLACE_CONFIG, 0, &raw mut answer), 0);
        assert_eq!(text(answer), "");
        (table.release)(state);
    }
}
