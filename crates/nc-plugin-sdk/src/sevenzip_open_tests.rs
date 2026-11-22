use super::*;

/// What the plugin answers, and what it was asked.
#[derive(Default)]
struct Asker {
    totals: Vec<(Option<u64>, Option<u64>)>,
    done: Vec<(Option<u64>, Option<u64>)>,
    password: Option<String>,
    stop: bool,
}

impl Opening for Asker {
    fn set_total(&mut self, files: Option<u64>, bytes: Option<u64>) -> bool {
        self.totals.push((files, bytes));
        !self.stop
    }

    fn set_completed(&mut self, files: Option<u64>, bytes: Option<u64>) -> bool {
        self.done.push((files, bytes));
        !self.stop
    }

    fn password(&mut self) -> Option<String> {
        self.password.clone()
    }
}

fn interface(opener: &Opener, iid: &GUID) -> Com {
    opener.0.query(iid).unwrap()
}

/// The library's progress reaches the plugin during the open call only; a stop is `E_ABORT`.
#[test]
fn progress_reaches_the_plugin_during_the_call() {
    let opener = Opener::new(None);
    let open = interface(&opener, &IID_OPEN_CALLBACK);
    // SAFETY: the open callback's table, as the library calls it.
    let table = unsafe { open.table::<OpenTable>() };
    let files = 3u64;
    let mut asker = Asker::default();
    let results = opener.with(&mut asker, || {
        // SAFETY: as the library calls it: a count or null.
        unsafe {
            [
                (table.set_total)(open.raw(), &raw const files, std::ptr::null()),
                (table.set_completed)(open.raw(), std::ptr::null(), &raw const files),
            ]
        }
    });
    assert_eq!(results, [S_OK, S_OK]);
    assert_eq!(asker.totals, [(Some(3), None)]);
    assert_eq!(asker.done, [(None, Some(3))]);
    asker.stop = true;
    let stopped = opener.with(&mut asker, || {
        // SAFETY: as above.
        unsafe { (table.set_total)(open.raw(), std::ptr::null(), std::ptr::null()) }
    });
    assert_eq!(stopped, E_ABORT);
    // SAFETY: as above, after the call: no plugin to reach.
    let after = unsafe { (table.set_total)(open.raw(), std::ptr::null(), std::ptr::null()) };
    assert_eq!(after, E_ABORT);
}

/// The password the plugin gives goes to the library as a `BSTR`; none is `E_ABORT`.
#[test]
fn the_password_is_asked_of_the_plugin() {
    let opener = Opener::new(None);
    let password = interface(&opener, &IID_CRYPTO_GET_TEXT_PASSWORD);
    // SAFETY: the password's table.
    let table = unsafe { password.table::<PasswordTable>() };
    let mut asker = Asker {
        password: Some("secret".to_string()),
        ..Asker::default()
    };
    let mut text: BSTR = std::ptr::null_mut();
    let result = opener.with(&mut asker, || {
        // SAFETY: as the library calls it, its place for the `BSTR`.
        unsafe { (table.password)(password.raw(), &raw mut text) }
    });
    assert_eq!(result, S_OK);
    assert_eq!(
        super::super::com::take_bstr(text),
        Some("secret".to_string())
    );
    asker.password = None;
    let refused = opener.with(&mut asker, || {
        // SAFETY: as above.
        unsafe { (table.password)(password.raw(), &raw mut text) }
    });
    assert_eq!(refused, E_ABORT);
}

/// Every interface reaches the others and the same object; another is refused.
#[test]
fn the_interfaces_reach_one_another() {
    let opener = Opener::new(None);
    let volume = interface(&opener, &IID_OPEN_VOLUME_CALLBACK);
    let back = volume.query(&IID_UNKNOWN).unwrap();
    assert_eq!(back.raw(), opener.raw());
    let password = back.query(&IID_CRYPTO_GET_TEXT_PASSWORD).unwrap();
    assert_eq!(
        password.query(&IID_OPEN_VOLUME_CALLBACK).unwrap().raw(),
        volume.raw()
    );
    assert!(volume.query(&super::super::com::IID_IN_ARCHIVE).is_none());
}

/// The volume callback tells the first file and opens a sibling by its name, which becomes the file it
/// tells; a name not there or a folder is `S_FALSE`.
#[test]
fn volumes_are_the_archives_siblings() {
    let folder = tempfile::tempdir().unwrap();
    let first = folder.path().join("a.7z.001");
    std::fs::write(&first, b"one").unwrap();
    std::fs::write(folder.path().join("a.7z.002"), b"second").unwrap();
    std::fs::create_dir(folder.path().join("sub")).unwrap();
    let opener = Opener::new(Some(&first));
    let volume = interface(&opener, &IID_OPEN_VOLUME_CALLBACK);
    // SAFETY: the volume callback's table.
    let table = unsafe { volume.table::<VolumeTable>() };
    let property = |id: u32| {
        let mut value = PropVariant::empty();
        // SAFETY: as the library calls it.
        unsafe { (table.property)(volume.raw(), id, &raw mut value) };
        value.read(false)
    };
    assert_eq!(property(prop::NAME), Prop::Text("a.7z.001".to_string()));
    assert_eq!(property(prop::SIZE), Prop::U64(3));
    assert_eq!(property(prop::IS_DIR), Prop::Bool(false));
    assert!(matches!(property(prop::MODIFIED), Prop::Time(time) if time > 0));
    assert_eq!(property(prop::PATH), Prop::Empty);
    let open = |name: &str| {
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        let mut out: Raw = std::ptr::null_mut();
        // SAFETY: as the library calls it.
        let result = unsafe { (table.stream)(volume.raw(), wide.as_ptr(), &raw mut out) };
        (result, Com::take(out).map(InStream))
    };
    let (result, stream) = open("a.7z.002");
    assert_eq!(result, S_OK);
    let mut buffer = [0u8; 6];
    stream.unwrap().read(&mut buffer).unwrap();
    assert_eq!(&buffer, b"second");
    assert_eq!(property(prop::SIZE), Prop::U64(6));
    assert_eq!(open("a.7z.003").0, S_FALSE);
    assert_eq!(open("sub").0, S_FALSE);
    assert_eq!(opener.volumes(), ["a.7z.002"]);
}

/// An archive inside another has no volumes.
#[test]
fn an_inner_archive_has_no_volumes() {
    let opener = Opener::new(None);
    let volume = interface(&opener, &IID_OPEN_VOLUME_CALLBACK);
    // SAFETY: the volume callback's table.
    let table = unsafe { volume.table::<VolumeTable>() };
    let wide: Vec<u16> = "x".encode_utf16().chain([0]).collect();
    let mut out: Raw = std::ptr::null_mut();
    // SAFETY: as the library calls it.
    let result = unsafe { (table.stream)(volume.raw(), wide.as_ptr(), &raw mut out) };
    assert_eq!(result, S_FALSE);
    assert!(out.is_null());
}

/// A call that unwinds leaves the plugin unreachable all the same: a later call is refused.
#[test]
fn an_unwinding_call_leaves_no_plugin_reachable() {
    let opener = Opener::new(None);
    let open = interface(&opener, &IID_OPEN_CALLBACK);
    // SAFETY: the open callback's table.
    let table = unsafe { open.table::<OpenTable>() };
    let mut asker = Asker::default();
    let unwound = catch_unwind(AssertUnwindSafe(|| {
        opener.with(&mut asker, || std::panic::resume_unwind(Box::new("broken")))
    }));
    assert!(unwound.is_err());
    // SAFETY: as the library calls it.
    let after = unsafe { (table.set_total)(open.raw(), std::ptr::null(), std::ptr::null()) };
    assert_eq!(after, E_ABORT);
}
