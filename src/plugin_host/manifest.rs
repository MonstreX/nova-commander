//! A plugin's `plugin.json` (ADR-0004 §7): its name, version, the API version it was built for and its
//! library's name without the system's prefix and extension (`probe` is `probe.dll` on Windows).

use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub api: u32,
    pub library: String,
}

impl Manifest {
    /// The manifest at `path`; why it cannot be used otherwise.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        let manifest: Self =
            serde_json::from_str(&text).map_err(|error| format!("plugin.json: {error}"))?;
        // Names go into paths, settings' keys and a thread's name: letters, digits, `-` and `_` only.
        let plain = |name: &str| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|ch| ch.is_alphanumeric() || ch == '-' || ch == '_')
        };
        if !plain(&manifest.name) {
            return Err(format!("plugin.json: no plugin named {:?}", manifest.name));
        }
        if !plain(&manifest.library) {
            return Err(format!(
                "plugin.json: no library named {:?}",
                manifest.library
            ));
        }
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Result<Manifest, String> {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("plugin.json");
        std::fs::write(&path, text).unwrap();
        Manifest::read(&path)
    }

    #[test]
    fn a_manifest_names_the_plugin_and_its_library() {
        let manifest = read(
            r#"{"name": "probe", "version": "1.2", "api": 1, "library": "probe", "test": true}"#,
        )
        .unwrap();
        assert_eq!(manifest.name, "probe");
        assert_eq!(manifest.api, 1);
        assert_eq!(manifest.library, "probe");
    }

    #[test]
    fn a_name_is_letters_digits_dashes_and_underscores() {
        let named = |name: &str| {
            read(&format!(
                r#"{{"name": "{name}", "version": "1", "api": 1, "library": "x"}}"#
            ))
        };
        assert!(named("tmp-panel_2").is_ok());
        assert!(named("a\\u0000b").is_err(), "no NUL: a thread's name");
        assert!(named("a.b").is_err(), "no dot: a settings key");
        assert!(named("").is_err());
        let library = read(r#"{"name": "x", "version": "1", "api": 1, "library": "x\u0001"}"#);
        assert!(library.is_err(), "no control characters in the library");
    }

    #[test]
    fn a_broken_manifest_says_why() {
        assert!(read("{").unwrap_err().starts_with("plugin.json:"));
        assert!(
            read(r#"{"name": "x", "version": "1", "api": 1}"#).is_err(),
            "no library"
        );
        let outside = read(r#"{"name": "x", "version": "1", "api": 1, "library": "..\\evil"}"#);
        assert_eq!(
            outside.unwrap_err(),
            r#"plugin.json: no library named "..\\evil""#,
            "a library outside its folder"
        );
    }

    #[test]
    fn a_manifest_is_read_whole() {
        let manifest =
            read(r#"{"name": "a_b-1", "version": "2.0.1", "api": 3, "library": "lib_x"}"#).unwrap();
        let expected = Manifest {
            name: "a_b-1".to_string(),
            version: "2.0.1".to_string(),
            api: 3,
            library: "lib_x".to_string(),
        };
        assert_eq!(manifest, expected);
    }

    #[test]
    fn each_field_has_its_type() {
        let fields = [
            r#"{"name": "x", "version": 1, "api": 1, "library": "x"}"#,
            r#"{"name": "x", "version": "1", "api": "1", "library": "x"}"#,
            r#"{"name": "x", "version": "1", "api": -1, "library": "x"}"#,
            r#"{"name": "x", "version": "1", "api": 4294967296, "library": "x"}"#,
            r#"{"name": 1, "version": "1", "api": 1, "library": "x"}"#,
            r#"{"version": "1", "api": 1, "library": "x"}"#,
            r#"{"name": "x", "api": 1, "library": "x"}"#,
            r#"{"name": "x", "version": "1", "library": "x"}"#,
        ];
        for text in fields {
            let error = read(text).unwrap_err();
            assert!(error.starts_with("plugin.json: "), "{text}: {error}");
        }
    }

    #[test]
    fn a_name_with_a_path_or_a_space_is_refused_saying_which() {
        let named = |name: &str| {
            read(&format!(
                r#"{{"name": "{name}", "version": "1", "api": 1, "library": "x"}}"#
            ))
        };
        for name in ["a b", "a/b", "a\\\\b", "..", "a:b"] {
            assert!(named(name).is_err(), "{name}");
        }
        assert_eq!(
            named("a.b").unwrap_err(),
            r#"plugin.json: no plugin named "a.b""#
        );
        let library = read(r#"{"name": "x", "version": "1", "api": 1, "library": ""}"#);
        assert_eq!(library.unwrap_err(), r#"plugin.json: no library named """#);
    }

    #[test]
    fn a_missing_manifest_says_why() {
        let temp = tempfile::tempdir().unwrap();
        let error = Manifest::read(&temp.path().join("plugin.json")).unwrap_err();
        assert!(!error.is_empty());
        assert!(
            !error.starts_with("plugin.json:"),
            "not a broken one: {error}"
        );
    }
}
