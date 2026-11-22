//! The ABI's promises a plugin built apart relies on: both tables open with the API version and their
//! size (so either side can read them from a table of another version), and `NcStr`'s empty text.

use core::mem::{align_of, offset_of, size_of};

use nc_plugin_api::{NcHost, NcPlugin, NcStr};

#[test]
fn both_tables_start_with_the_version_and_their_size() {
    assert_eq!(offset_of!(NcHost, api_version), 0);
    assert_eq!(offset_of!(NcPlugin, api_version), 0);
    // A `u32` and then a `usize`: the size right after the version, at the `usize`'s alignment.
    assert_eq!(offset_of!(NcHost, size), align_of::<usize>());
    assert_eq!(offset_of!(NcPlugin, size), align_of::<usize>());
    assert_eq!(size_of::<u32>(), 4);
}

/// R7 review A3: the structs' fingerprint, pinned. When this fails a struct of the ABI changed: bump
/// `API_VERSION` (and note it in its list), then put the new value here.
#[cfg(all(windows, target_pointer_width = "64"))]
#[test]
fn the_structs_fingerprint_is_pinned() {
    /// The fingerprint of API 11's structs on 64-bit Windows.
    const PINNED: u64 = 0xa9fa_2f29_1cae_e1bd;
    assert_eq!(nc_plugin_api::ABI, PINNED, "{:#x}", nc_plugin_api::ABI);
    assert_eq!(nc_plugin_api::API_VERSION, 11);
}

#[test]
fn text_is_its_bytes_and_empty_is_empty() {
    let text = "héllo, мир";
    let raw = NcStr::new(text);
    assert_eq!(raw.len, text.len(), "bytes, not characters");
    // SAFETY: `text` is a live `&'static str`.
    assert_eq!(unsafe { raw.as_str() }, text);
    // SAFETY: none of these is read: an empty text, and a null pointer `as_str` checks for.
    unsafe {
        assert_eq!(NcStr::EMPTY.as_str(), "");
        assert_eq!(NcStr::new("").as_str(), "");
        let none = NcStr {
            ptr: text.as_ptr(),
            len: 0,
        };
        assert_eq!(none.as_str(), "", "any pointer when the length is 0");
        let null = NcStr {
            ptr: core::ptr::null(),
            len: 5,
        };
        assert_eq!(null.as_str(), "", "a null pointer is no text");
    }
}
