//! A plugin's start: the plugins start side by side (review R17), one slow to start holding the others no
//! longer than itself; each is given its data folder (API 7).

use std::path::Path;

use super::PluginHost;

/// A plugin's data folder (API 7) is `settings\Plugins\<name>`, beside the logs' `settings\Logs`.
#[test]
fn a_plugins_data_folder_is_beside_the_logs() {
    let settings = Path::new("vc").join("settings");
    let host = PluginHost {
        logs: settings.join("Logs"),
        ..PluginHost::default()
    };
    assert_eq!(
        host.data_folder("netway"),
        settings.join("Plugins").join("netway")
    );
}

#[cfg(windows)]
#[test]
fn plugins_slow_to_start_start_side_by_side() {
    use super::tests::{install_probe, load};
    use std::time::{Duration, Instant};

    let temp = tempfile::tempdir().unwrap();
    let names = ["slowa", "slowb", "slowc"];
    for name in names {
        let manifest = format!(
            r#"{{"name": "{name}", "version": "1", "api": {}, "library": "probe"}}"#,
            nc_plugin_api::API_VERSION
        );
        let place = install_probe(temp.path(), name, &manifest);
        std::fs::write(place.join("slow-start"), "").unwrap();
    }
    let started = Instant::now();
    let host = load(temp.path(), &|_| true);
    let took = started.elapsed();
    assert_eq!(host.plugins.len(), names.len(), "{:?}", host.failures);
    assert!(
        took < Duration::from_secs(5),
        "three starts of 2 s each took {took:?}: one after another"
    );
}
