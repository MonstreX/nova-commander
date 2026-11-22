//! The plugins' settings (ADR-0004 §7): a plugin is loaded unless `"plugins.<name>.enabled": false` turns
//! it off. The keys are the settings file's own, kept with the others the program does not name
//! (`Settings::other`).

/// Whether the plugin `name` is loaded: `"plugins.<name>.enabled": false` turns it off (ADR-0004 §7).
pub fn plugin_enabled(settings: &super::Settings, name: &str) -> bool {
    let key = format!("plugins.{name}.enabled");
    settings
        .other
        .get(&key)
        .and_then(serde_json::Value::as_bool)
        != Some(false)
}

/// The plugins' logs beside the settings file: `settings\Logs` (ADR-0016 §8).
pub fn plugin_logs_dir(settings_path: &std::path::Path) -> std::path::PathBuf {
    settings_path.with_file_name("Logs")
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;

    #[test]
    fn the_logs_are_beside_the_settings_file() {
        let settings = std::path::Path::new("C:/vc/settings/settings.json");
        assert_eq!(
            super::plugin_logs_dir(settings),
            std::path::Path::new("C:/vc/settings/Logs")
        );
    }

    #[test]
    fn plugins_are_turned_off_by_their_own_key_and_other_keys_are_kept() {
        let text = r#"{"plugins.probe.enabled": false, "plugins.other.enabled": true, "x.y": 1}"#;
        let read: Settings = serde_json::from_str(text).unwrap();
        assert!(!super::plugin_enabled(&read, "probe"));
        assert!(super::plugin_enabled(&read, "other"));
        assert!(
            super::plugin_enabled(&read, "unnamed"),
            "on unless turned off"
        );
        let written = serde_json::to_value(&read).unwrap();
        assert_eq!(written["x.y"], 1, "kept when written again");
        assert_eq!(written["infoPanel.power"], true);
    }

    #[test]
    fn only_false_turns_a_plugin_off() {
        let text = r#"{"plugins.a.enabled": "false", "plugins.b.enabled": 0, "plugins.c.enabled": null,
            "plugins.d": false, "d.enabled": false}"#;
        let read: Settings = serde_json::from_str(text).unwrap();
        for name in ["a", "b", "c", "d"] {
            assert!(super::plugin_enabled(&read, name), "{name}");
        }
    }

    #[test]
    fn the_keys_the_program_names_are_not_kept_twice() {
        let mut written = serde_json::to_value(Settings::default()).unwrap();
        written["infoPanel.power"] = false.into();
        written["panels.left.visible"] = false.into();
        written["plugins.probe.enabled"] = false.into();
        written["x.y"] = serde_json::json!({"nested": [1, 2]});
        let read: Settings = serde_json::from_value(written.clone()).unwrap();
        let kept: Vec<&str> = read.other.keys().map(String::as_str).collect();
        assert_eq!(kept, ["plugins.probe.enabled", "x.y"], "only the unnamed");
        assert!(
            !read.info_panel.power,
            "the info panel's own key read by it"
        );
        assert!(!read.left_visible);
        assert_eq!(
            serde_json::to_value(&read).unwrap(),
            written,
            "written back as read"
        );
    }
}
