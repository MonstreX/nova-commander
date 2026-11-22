//! A plugin's panels through the host, on the probe: the item that opens one named for the panel it was
//! run for, a name that cannot be a path's part, the panel's folders and title read through its provider
//! (the plugin's entries copied as they are, its failures by kind), its end told once to its plugin, a
//! plugin dropped or loaded again. Each test names its probe alone: the providers' list is the program's.
#![cfg(windows)]

use std::sync::atomic::AtomicBool;
use std::time::{Duration, UNIX_EPOCH};

use super::*;
use crate::plugin_host::run::opened;

/// The probe installed as plugin `name`, its drive menu items `disks`, loaded; its logs in `Logs`.
fn probe_named(name: &str, disks: &str) -> (tempfile::TempDir, PluginHost) {
    let temp = tempfile::tempdir().unwrap();
    let manifest =
        format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#);
    let place = install_probe(temp.path(), name, &manifest);
    std::fs::write(place.join("disks.txt"), disks).unwrap();
    let host = load(temp.path(), &|_| true);
    assert!(host.failures.is_empty(), "{:?}", host.failures);
    (temp, host)
}

/// The folder `path` as its provider lists it.
fn listed(path: &VfsPath) -> io::Result<Vec<Entry>> {
    let provider = vfs::provider_for(path)?;
    let mut entries = Vec::new();
    provider.list(path, &AtomicBool::new(false), &mut |entry| {
        entries.push(entry)
    })?;
    Ok(entries)
}

fn names(path: &VfsPath) -> Vec<String> {
    let entries = listed(path).unwrap();
    entries.into_iter().map(|entry| entry.name).collect()
}

fn title(path: &VfsPath) -> Option<String> {
    vfs::provider_for(path)
        .ok()?
        .look(path, &AtomicBool::new(false))
        .title
}

/// Every job sent to the probe so far handled: its "Fail" command run after them, its failure waited for.
fn settle(host: &PluginHost) {
    host.run_command(0, 1, 0, 0);
    let Event::Failed { error, .. } = next_event(host) else {
        panic!("the command's failure");
    };
    assert_eq!(error, "The probe fails on purpose");
}

fn log(temp: &tempfile::TempDir, name: &str) -> String {
    let path = temp.path().join("Logs").join(format!("{name}.log"));
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn a_panels_name_with_a_slash_or_a_backslash_is_the_items_failure() {
    let Event::Panel {
        plugin,
        target,
        path,
        ..
    } = opened("Tmp_Panel", "one two", 1, 0)
    else {
        panic!("a panel");
    };
    assert_eq!((plugin.as_str(), target), ("Tmp_Panel", 1));
    assert_eq!(path, VfsPath::in_session("tmp_panel", "one two", "/"));
    for name in ["a/b", "a\\b", "/", "\\"] {
        let Event::Failed { plugin, error, .. } = opened("Tmp_Panel", name, 0, 0) else {
            panic!("{name}: a failure");
        };
        assert_eq!(plugin, "Tmp_Panel");
        assert_eq!(
            error,
            format!("The plugin named its panel \"{name}\": a name with / or \\ cannot be opened")
        );
    }
}

#[test]
fn an_item_that_opens_a_panel_names_it_for_the_panel_it_was_run_for() {
    let (_temp, host) = probe_named("HostOpens", "Panel one\nPanel a\\b\nTemp");
    host.open_disk(0, 0, 1, 0);
    let Event::Panel {
        plugin,
        target,
        path,
        ..
    } = next_event(&host)
    else {
        panic!("a panel");
    };
    assert_eq!((plugin.as_str(), target), ("HostOpens", 1));
    assert_eq!(path.to_string(), "hostopens://one/");
    host.open_disk(0, 0, 0, 0);
    let Event::Panel { target, .. } = next_event(&host) else {
        panic!("a panel");
    };
    assert_eq!(target, 0, "the panel asked for each time");
    host.open_disk(0, 1, 0, 0);
    let Event::Failed { error, .. } = next_event(&host) else {
        panic!("a name with a backslash");
    };
    assert!(error.contains("\"a\\b\""), "{error}");
}

#[test]
fn a_panels_folders_and_title_come_from_its_plugin_as_it_gave_them() {
    let (_temp, _host) = probe_named("hostlists", "");
    let root = VfsPath::in_session("hostlists", "one", "/");
    let entries = listed(&root).unwrap();
    let shape: Vec<(&str, bool, u64, usize)> = (entries.iter())
        .map(|entry| {
            (
                entry.name.as_str(),
                entry.is_dir,
                entry.size,
                entry.position,
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            ("docs", true, 0, 0),
            ("locked", true, 0, 1),
            ("readme.txt", false, 12, 2),
            (".hidden", false, 0, 3)
        ],
        "in the plugin's order, no \"..\" of the provider's own"
    );
    let readme = &entries[2];
    assert_eq!(readme.modified, Some(UNIX_EPOCH + Duration::from_secs(1)));
    assert_eq!((readme.created, readme.accessed), (None, None));
    assert!(!readme.attributes.is_hidden());
    assert!(entries[3].attributes.is_hidden());
    assert_eq!(title(&root).as_deref(), Some("Probe one:/"));
    let docs = VfsPath::in_session("hostlists", "two", "/docs");
    assert_eq!(names(&docs), ["a.txt", "deep"], "the path asked");
    assert_eq!(title(&docs).as_deref(), Some("Probe two:/docs"));
    let untitled = VfsPath::in_session("hostlists", "untitled", "/docs");
    assert_eq!(title(&untitled), None, "no title: the path stands");
}

#[test]
fn a_folder_the_plugin_refuses_fails_with_its_kind_and_its_reason() {
    let (_temp, _host) = probe_named("hostrefuses", "");
    let at = |instance: &str, path: &str| {
        let error = listed(&VfsPath::in_session("hostrefuses", instance, path)).unwrap_err();
        (error.kind(), error.to_string())
    };
    assert_eq!(
        at("one", "/locked"),
        (
            io::ErrorKind::PermissionDenied,
            "The probe keeps it locked".to_string()
        )
    );
    assert_eq!(
        at("one", "/nowhere"),
        (
            io::ErrorKind::NotFound,
            "The probe has no folder /nowhere".to_string()
        )
    );
    assert_eq!(
        at("refused", "/"),
        (
            io::ErrorKind::PermissionDenied,
            "The probe refuses this panel".to_string()
        ),
        "the instance asked"
    );
}

#[test]
fn a_panel_no_longer_shown_is_told_to_its_plugin_by_its_instance() {
    let (temp, host) = probe_named("HostCloses", "");
    let docs = VfsPath::in_session("HostCloses", "one", "/docs");
    assert!(host.is_plugin_panel(&docs), "its name in any case");
    assert!(host.is_plugin_panel(&VfsPath::parse("hostcloses://two/")));
    let container = VfsPath::parse(r"hostcloses:C:\a.zip!/");
    assert_eq!(container.provider(), Some("hostcloses"));
    assert!(
        !host.is_plugin_panel(&container),
        "a file's inside is no panel"
    );
    assert!(!host.is_plugin_panel(&VfsPath::in_session("nosuch", "one", "/")));
    assert!(!host.is_plugin_panel(&VfsPath::local(r"C:\")));
    host.close_panel(&docs);
    host.close_panel(&container);
    host.close_panel(&VfsPath::local(r"C:\"));
    host.close_panel(&VfsPath::in_session("nosuch", "two", "/"));
    settle(&host);
    let told = log(&temp, "HostCloses");
    assert_eq!(told.matches("closed").count(), 1, "{told}");
    assert!(told.contains("closed one"), "{told}");
}

#[test]
fn a_plugin_dropped_answers_that_it_is_gone_and_one_loaded_again_takes_its_paths() {
    let (_one, first) = probe_named("hostagain", "");
    let (_two, second) = probe_named("hostagain", "");
    let root = VfsPath::in_session("hostagain", "one", "/");
    drop(first);
    assert_eq!(listed(&root).unwrap().len(), 4, "the plugin loaded last");
    drop(second);
    let error = listed(&root).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotConnected, "{error}");
    assert_eq!(title(&root), None);
}

#[test]
fn a_panels_modes_and_its_entries_texts_come_from_its_plugin() {
    let (_temp, _host) = probe_named("hostmodes", "");
    let root = VfsPath::in_session("hostmodes", "one", "/");
    let look = vfs::provider_for(&root)
        .unwrap()
        .look(&root, &AtomicBool::new(false));
    assert_eq!(look.title.as_deref(), Some("Probe one:/"));
    assert_eq!(look.modes.len(), 7);
    let kinds = look.mode(3).unwrap();
    assert_eq!(
        (kinds.columns.as_str(), kinds.widths.as_str()),
        ("N,C0,Z", "0,6,0")
    );
    assert_eq!(
        kinds.titles,
        [
            Some("Item".to_string()),
            Some("Kind".to_string()),
            Some("About".to_string())
        ]
    );
    assert!(kinds.detailed_status && !kinds.full_screen);
    let letters = look.mode(6).unwrap();
    assert_eq!(letters.titles, [None, Some("Letters".to_string())]);
    assert_eq!(
        (
            letters.status_columns.as_str(),
            letters.status_widths.as_str()
        ),
        ("C0,NR", "6,0")
    );
    assert!(letters.full_screen && !letters.detailed_status);
    assert!(look.mode(2).is_none() && look.mode(7).is_none());
    let entries = listed(&root).unwrap();
    let texts = entries[2].texts.as_deref().unwrap();
    assert_eq!(entries[2].name, "readme.txt");
    assert_eq!(texts.columns, ["file", "10"]);
    assert_eq!(texts.description, "About readme.txt");
    assert_eq!(entries[0].texts.as_deref().unwrap().columns[0], "folder");
    let untitled = VfsPath::in_session("hostmodes", "untitled", "/");
    let provider = vfs::provider_for(&untitled).unwrap();
    assert!(
        (provider.look(&untitled, &AtomicBool::new(false)).modes).is_empty(),
        "no modes: the program's"
    );
}

#[test]
fn the_temporary_panel_takes_paths_put_and_answers_for_its_items() {
    let temp = tempfile::tempdir().unwrap();
    super::tests::install_shipped(temp.path(), "tmppanel", "hosttmp");
    let host = load(temp.path(), &|_| true);
    let files = tempfile::tempdir().unwrap();
    let a = files.path().join("a.txt");
    std::fs::write(&a, "hello").unwrap();
    // 5d-3b: the common panel is one instance showing the current list (FAR's one global current), not
    // one instance a list ("0".."9").
    let root = VfsPath::in_session("hosttmp", "common", "/");
    host.put(&root, vec![a.clone()], (false, 0), Some(files.path()));
    let Event::Busy { message, .. } = next_event(&host) else {
        panic!("FAR's \"Sending files to temporary panel\" first");
    };
    assert_eq!(message.lines, ["Sending files to temporary panel"]);
    let Event::Put { plugin, done } = next_event(&host) else {
        panic!("the put's end");
    };
    assert_eq!((plugin.as_str(), done), ("hosttmp", true));
    assert!(
        matches!(next_event(&host), Event::Done { .. }),
        "its message goes with the call"
    );
    let name = a.display().to_string();
    assert_eq!(names(&root), std::slice::from_ref(&name));
    let provider = vfs::provider_for(&root).unwrap();
    let found = provider.stat(&root.join(&name)).unwrap();
    assert_eq!((found.size, found.real_path), (5, Some(a.clone())));
    let missing = files.path().join("b.txt").display().to_string();
    let error = provider.stat(&root.join(&missing)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound, "not on its list");
    let inside = listed(&root.join("x")).unwrap_err();
    assert_eq!(
        inside.kind(),
        io::ErrorKind::NotFound,
        "no folders of its own"
    );
    let other = VfsPath::in_session("hosttmp", "42", "/");
    assert_eq!(listed(&other).unwrap_err().kind(), io::ErrorKind::NotFound);
    host.put(&other, vec![a.clone()], (false, 0), None);
    let Event::Failed { error, .. } = next_event(&host) else {
        panic!("a panel it has not");
    };
    assert_eq!(error, "No temporary panel 42");
    provider
        .remove(&root.join(&name), vfs::Removal::File)
        .unwrap();
    assert!(names(&root).is_empty(), "off the list");
    assert!(a.is_file(), "the file stays");
}

#[test]
fn the_temporary_panel_looks_as_far_s_tmppanel() {
    let temp = tempfile::tempdir().unwrap();
    super::tests::install_shipped(temp.path(), "tmppanel", "hosttmplook");
    let _host = load(temp.path(), &|_| true);
    // 5d-3b: the common panel's instance (FAR's one global current).
    let root = VfsPath::in_session("hosttmplook", "common", "/");
    let look = vfs::provider_for(&root)
        .unwrap()
        .look(&root, &AtomicBool::new(false));
    assert_eq!(look.title.as_deref(), Some("Temporary panel [0]"));
    assert_eq!(
        look.format.as_deref(),
        Some("Temporary panel"),
        "its Format"
    );
    assert!(
        look.names_only && look.takes_paths,
        "OPIF_SHOWNAMESONLY, PutFiles"
    );
    assert!(
        look.mode(4).unwrap().case_conversion,
        "PMFLAGS_CASECONVERSION"
    );
    assert_eq!(look.start_mode, Some(4));
    let labelled: Vec<(&str, &str)> = (look.keys.iter())
        .filter(|key| !key.label.is_empty())
        .map(|key| (key.key.as_str(), key.label.as_str()))
        .collect();
    assert_eq!(
        labelled,
        [
            ("F7", "Remove"),
            ("Alt+Shift+F2", "SavLst"),
            ("Alt+Shift+F3", "Goto"),
            ("Alt+Shift+F12", "Switch")
        ],
        "TmpPanel's key bar"
    );
    // 5d-3b: and the 34 keys it checks the entry for (F3..F8 but F7 with any modifiers but Ctrl alone;
    // Alt+Shift+F3 is Goto), unlabelled (`TmpClass.cpp:575-579`).
    assert_eq!(look.keys.len(), 14 + 34, "and Alt+Shift+0..9");
}

/// The folder the sources came from not handed over exactly (its name not Unicode): nothing put, the
/// failure told as the plugin's (review R49, as for a source).
#[test]
fn a_put_from_a_folder_not_unicode_puts_nothing() {
    use std::os::windows::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    super::tests::install_shipped(temp.path(), "tmppanel", "hosttmpodd");
    let host = load(temp.path(), &|_| true);
    let files = tempfile::tempdir().unwrap();
    let a = files.path().join("a.txt");
    std::fs::write(&a, "hello").unwrap();
    let odd = PathBuf::from(std::ffi::OsString::from_wide(&[0xD800, u16::from(b'x')]));
    let root = VfsPath::in_session("hosttmpodd", "common", "/");
    host.put(&root, vec![a], (false, 0), Some(&odd));
    let Event::Failed { plugin, error, .. } = next_event(&host) else {
        panic!("refused before the plugin");
    };
    assert_eq!(plugin, "hosttmpodd");
    assert!(error.contains("not Unicode"), "{error}");
    assert!(names(&root).is_empty(), "nothing put");
}
