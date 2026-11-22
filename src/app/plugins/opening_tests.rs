//! The command line and a file opened by plugins: tested with TmpPanel's library as it is shipped in
//! `tmp_panel_lists_tests` (`tmp:`, Enter and Ctrl+PgDn on a list file, a menu from a list) and the waits in
//! `plugin_keys_tests` (answers after a window, a file passed on where the panel moved). Here: which
//! plugins a file's name is offered to.
#![cfg(windows)]

use std::time::{Duration, Instant};

use crate::app::fileops::tests::app_in;
use crate::plugin_host::tests::{install_shipped, load};

#[test]
fn a_name_is_offered_only_to_the_plugins_whose_masks_take_it() {
    let plugins = tempfile::tempdir().unwrap();
    install_shipped(plugins.path(), "tmppanel", "openmasks");
    let mut app = app_in(plugins.path());
    app.set_plugins(load(plugins.path(), &|_| true));
    let deadline = Instant::now() + Duration::from_secs(10);
    while (app.plugins.host.plugins.iter()).all(|plugin| plugin.file_masks.is_empty()) {
        assert!(Instant::now() < deadline, "no masks");
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(app.plugins_for("list.temp"), [0]);
    assert!(app.plugins_for("a.txt").is_empty(), "not its masks'");
    let with_masks = app.plugins_with_masks();
    assert_eq!(with_masks.len(), 1);
    assert!(with_masks[0].1.iter().any(|mask| mask.matches("x.temp")));
}
