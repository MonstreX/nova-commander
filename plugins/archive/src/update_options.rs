//! What adding to an archive does (arclite's `UpdateOptions`) and what of it is kept (its `update_*`
//! options under their names): the compression level and method, encryption, the existing file's answer,
//! moving, ignoring errors, files open for writing.

use nc_plugin_sdk::Host;

use crate::extract_options::Overwrite;

/// arclite's `c_levels`: each level's name and number.
pub const LEVELS: [(&str, u32); 6] = [
    ("Store", 0),
    ("Fastest", 1),
    ("Fast", 3),
    ("Normal", 5),
    ("Maximum", 7),
    ("Ultra", 9),
];

/// The method a zip gets when none fits (arclite's default for a zip, `c_method_deflate`).
pub const DEFLATE: &str = "Deflate";

/// arclite's `update_*` options, kept by "Save settings".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSettings {
    pub level: u32,
    pub method: String,
    pub advanced: String,
    pub encrypt: bool,
    pub show_password: bool,
    pub move_files: bool,
    pub ignore_errors: bool,
    pub overwrite: Overwrite,
}

impl Default for UpdateSettings {
    /// arclite's defaults (`options.cpp`).
    fn default() -> Self {
        Self {
            level: 5,
            method: String::new(),
            advanced: String::new(),
            encrypt: false,
            show_password: false,
            move_files: false,
            ignore_errors: false,
            overwrite: Overwrite::Ask,
        }
    }
}

impl UpdateSettings {
    pub fn load(host: &Host) -> Self {
        let defaults = Self::default();
        let level = host.int_setting("update_level", i64::from(defaults.level));
        Self {
            level: u32::try_from(level).unwrap_or(defaults.level),
            method: host.setting("update_method").unwrap_or_default(),
            advanced: host.setting("update_advanced").unwrap_or_default(),
            encrypt: host.bool_setting("update_encrypt", defaults.encrypt),
            show_password: host.bool_setting("update_show_password", defaults.show_password),
            move_files: host.bool_setting("update_move_files", defaults.move_files),
            ignore_errors: host.bool_setting("update_ignore_errors", defaults.ignore_errors),
            overwrite: Overwrite::of(host.int_setting("update_overwrite", 0)),
        }
    }

    /// "Save settings" of an existing archive's dialog: the answer for an existing file kept, not the
    /// move (arclite keeps that for a new archive only).
    pub fn save(&self, host: &Host) {
        host.set_int("update_level", i64::from(self.level));
        host.set_setting("update_method", &self.method);
        host.set_setting("update_advanced", &self.advanced);
        host.set_bool("update_encrypt", self.encrypt);
        host.set_bool("update_show_password", self.show_password);
        host.set_bool("update_ignore_errors", self.ignore_errors);
        host.set_int("update_overwrite", self.overwrite as i64);
    }
}

/// One update of an archive (arclite's `UpdateOptions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateOptions {
    /// One of [`LEVELS`]' numbers; 0 stores.
    pub level: u32,
    /// 7-Zip's name of the method (one of arclite's `c_methods`, or `Copy`).
    pub method: String,
    /// 7-Zip's parameters typed in (`em=` the cipher; the others not used).
    pub advanced: String,
    pub encrypt: bool,
    pub show_password: bool,
    pub password: String,
    pub overwrite: Overwrite,
    pub move_files: bool,
    pub ignore_errors: bool,
    /// Files open for writing read all the same (FAR's "Copy files opened for writing").
    pub open_shared: bool,
    /// The format and what only 7-Zip's library writes (arclite's `arc_type`, `solid`,
    /// `encrypt_header`, the volumes).
    pub kind: Kind,
}

/// What an archive is written as besides a zip's options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kind {
    /// 7-Zip's name of the format (`zip`, `7z`, `tar`, ...).
    pub format: String,
    pub solid: bool,
    /// arclite's `encrypt_header` (a 7z's; none: the format's default).
    pub encrypt_header: Option<bool>,
    /// Each volume's size when the new archive is split (arclite's `enable_volumes`, `volume_size`).
    pub volume_size: Option<u64>,
    /// The module a new 7z is written after (arclite's `create_sfx` with `sfx_options`).
    pub sfx: Option<crate::sfx::Sfx>,
}

impl Default for Kind {
    fn default() -> Self {
        Self {
            format: "zip".to_string(),
            solid: false,
            encrypt_header: None,
            volume_size: None,
            sfx: None,
        }
    }
}

impl Kind {
    /// Whether it is a zip.
    pub fn is_zip(&self) -> bool {
        self.format.eq_ignore_ascii_case("zip")
    }
}

impl UpdateOptions {
    /// arclite's `put_files` for an existing archive: the method given (Deflate when it is none arclite
    /// offers), encryption as given, the kept level and answers; F4's copy put back always overwrites.
    pub fn for_archive(
        settings: &UpdateSettings,
        (method, encrypted): (&str, bool),
        edit: bool,
        open_shared: bool,
    ) -> Self {
        let method = (crate::update_props::LIBRARY_METHODS.iter())
            .find(|known| known.eq_ignore_ascii_case(method))
            .map_or(DEFLATE, |known| known);
        Self {
            level: settings.level,
            method: method.to_string(),
            advanced: settings.advanced.clone(),
            encrypt: encrypted,
            show_password: settings.show_password,
            password: String::new(),
            overwrite: if edit {
                Overwrite::Overwrite
            } else {
                settings.overwrite
            },
            move_files: false,
            ignore_errors: settings.ignore_errors,
            open_shared,
            kind: Kind::default(),
        }
    }

    /// arclite's `put_files` for an archive 7-Zip's library writes (`load_update_props`): its format,
    /// method, solid and encryption, the kept level (a stored archive's 0 kept: arclite's `m_level` is
    /// not taken, the kept level is) and answers; F4's copy put back always overwrites.
    pub fn for_library(
        settings: &UpdateSettings,
        (format, props): (&str, &crate::update_props::ArchiveProps),
        edit: bool,
        open_shared: bool,
    ) -> Self {
        let mut options =
            Self::for_archive(settings, ("Deflate", props.encrypted), edit, open_shared);
        options.method.clone_from(&props.method);
        options.kind = Kind {
            format: format.to_string(),
            solid: props.solid,
            encrypt_header: None,
            volume_size: None,
            sfx: None,
        };
        options
    }

    /// The AES strength 7-Zip's `em=` asks for (`AES128`, `AES192`, `AES256`); 256 by default (the
    /// crate writes no ZipCrypto).
    pub fn aes_bits(&self) -> u16 {
        let asked = (self.advanced.split_whitespace())
            .filter_map(|param| param.split_once('='))
            .find(|(name, _)| name.eq_ignore_ascii_case("em"))
            .map(|(_, value)| value.to_ascii_uppercase());
        match asked.as_deref() {
            Some("AES128") => 128,
            Some("AES192") => 192,
            _ => 256,
        }
    }
}

#[cfg(test)]
#[path = "update_options_tests.rs"]
mod tests;
