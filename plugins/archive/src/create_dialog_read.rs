//! The "Create archive" dialog's answers as arclite's `read_controls` with `new_arc` takes them (ui.cpp):
//! the format the archiver's buttons choose, the level and method, the password, the self-extracting
//! archive's module checked (`MSG_SFX_OPTIONS_DLG_WRONG_MODULE`, `MSG_UPDATE_DLG_SFX_NO_PPMD`,
//! `MSG_UPDATE_DLG_SFX_NO_ENCRYPT`), the volumes' size; the default extension (`get_default_ext`) and the
//! path made whole (`eval_arc_path`).

use std::path::PathBuf;

use nc_plugin_sdk::Closed;

use crate::create_dialog::{
    ADVANCED, APPEND_EXT, ARC_PATH, CANCEL, CREATE_SFX, CreateDialog, ENABLE_VOLUMES, ENCRYPT,
    ENCRYPT_HEADER, IGNORE_ERRORS, LEVEL, METHOD, MOVE_FILES, OPEN_SHARED, OTHER, OTHER_LIST,
    PASSWORD, PASSWORD_VERIFY, PASSWORD_VISIBLE, SEVEN, SHOW_PASSWORD, SOLID, VOLUME_SIZE, ZIP,
    parse_size,
};
use crate::create_options::{NewArchive, ProfileOptions};
use crate::opening::{expand_env, full_path};
use crate::sfx::SFX_EXT;
use crate::update_dialog::read_password;
use crate::update_options::LEVELS;
use crate::update_props::{COPY, fit_level, methods_of};
use crate::update_volumes::VOLUME_EXT;

/// arclite's `c_min_volume_size`.
const MIN_VOLUME_SIZE: u64 = 16 * 1024;

impl CreateDialog {
    /// arclite's `get_default_ext`: the format's extension, `.exe` after it for a self-extracting 7z,
    /// else `.001` for volumes.
    pub(crate) fn default_ext(&self, format: &str, (sfx, volumes): (bool, bool)) -> String {
        let mut ext = self.archivers.extension(format);
        if sfx && format == "7z" {
            ext.push_str(SFX_EXT);
        } else if volumes {
            ext.push_str(VOLUME_EXT);
        }
        ext
    }

    /// arclite's checks of a self-extracting archive's module: there, and with every codec when PPMD
    /// or encryption is asked.
    fn sfx_checked(&self, method: &str, encrypt: bool) -> Result<(), &'static str> {
        let at = (self.modules.by_name(&self.sfx.name)).ok_or(crate::sfx::WRONG_MODULE)?;
        if method.eq_ignore_ascii_case("PPMD") && !self.modules.all_codecs(at) {
            return Err("Selected SFX module does not support PPMD compression");
        }
        if encrypt && !self.modules.all_codecs(at) {
            return Err("Selected SFX module does not support encryption");
        }
        Ok(())
    }

    /// The format the archiver's buttons choose (arclite's `read_controls` with `new_arc`).
    pub(crate) fn format_of(&self, state: &dyn Fn(usize) -> i32) -> Option<String> {
        if state(SEVEN) == 1 {
            return self
                .archivers
                .seven
                .as_ref()
                .map(|seven| seven.name.clone());
        }
        if state(ZIP) == 1 {
            return Some(self.archivers.zip.name.clone());
        }
        let at = usize::try_from(state(OTHER_LIST)).unwrap_or(usize::MAX);
        (state(OTHER) == 1)
            .then(|| {
                self.archivers
                    .others
                    .get(at)
                    .map(|other| other.name.clone())
            })
            .flatten()
    }

    /// The answers as the dialog holds them (arclite's `read_controls` with `new_arc`), or why they
    /// cannot be taken.
    pub(crate) fn read(
        &self,
        value: &dyn Fn(usize) -> (i32, String),
    ) -> Result<NewArchive, &'static str> {
        let state = |item: usize| value(item).0;
        let format = self.format_of(&state).ok_or("Select archive format")?;
        let (is_7z, is_zip) = (format == "7z", format.eq_ignore_ascii_case("zip"));
        let at = |item: usize| usize::try_from(value(item).0).unwrap_or(usize::MAX);
        let level =
            (LEVELS.get(at(LEVEL)).map(|(_, level)| *level)).ok_or("Select compression level")?;
        let level = fit_level(&format, level);
        let start = &self.start.profile;
        let method = if level == 0 {
            COPY.to_string()
        } else if is_7z || is_zip {
            (methods_of(&format)
                .get(at(METHOD))
                .map(|method| (*method).to_string()))
            .ok_or("Select compression method")?
        } else {
            start.method.clone()
        };
        let encrypt = (is_7z || is_zip) && value(ENCRYPT).0 == 1;
        let show_password = value(SHOW_PASSWORD).0 == 1;
        let password = read_password(
            encrypt,
            show_password,
            &|item| value(item).1,
            (PASSWORD, PASSWORD_VERIFY, PASSWORD_VISIBLE),
        )?;
        let encrypt_header = match value(ENCRYPT_HEADER).0 {
            _ if !(is_7z && encrypt) => start.encrypt_header,
            0 => Some(false),
            1 => Some(true),
            _ => None,
        };
        let create_sfx = is_7z && value(CREATE_SFX).0 == 1;
        if create_sfx {
            self.sfx_checked(&method, encrypt)?;
        }
        // arclite leaves "Split into volumes" as checked, disabled; its `create` would split the 7z.
        let enable_volumes = value(ENABLE_VOLUMES).0 == 1 && !create_sfx;
        let volume_size = value(VOLUME_SIZE).1;
        if enable_volumes && parse_size(&volume_size) < MIN_VOLUME_SIZE {
            return Err("Invalid volume size");
        }
        let solid = if is_7z && level != 0 {
            value(SOLID).0 == 1
        } else {
            start.solid
        };
        Ok(NewArchive {
            path: value(ARC_PATH).1,
            append_ext: value(APPEND_EXT).0 == 1,
            profile: ProfileOptions {
                format,
                level,
                method,
                solid,
                advanced: value(ADVANCED).1,
                encrypt,
                encrypt_header,
                password,
                create_sfx,
                sfx: self.sfx.clone(),
                enable_volumes,
                volume_size,
                move_files: value(MOVE_FILES).0 == 1,
                ignore_errors: value(IGNORE_ERRORS).0 == 1,
            },
            levels: self.levels.clone(),
            show_password,
            open_shared: value(OPEN_SHARED).0 == 1,
        })
    }

    /// arclite's `eval_arc_path` with the starting format's extension.
    pub fn eval_path(&self, typed: &str, append_ext: bool) -> PathBuf {
        let profile = &self.start.profile;
        let ext = self.default_ext(
            &profile.format,
            (profile.create_sfx, profile.enable_volumes),
        );
        self.eval_path_with(typed, append_ext, &ext)
    }

    /// arclite's `eval_arc_path`: the path typed, quotes gone and the environment's names expanded; a
    /// folder's (or nothing) given the default name and the extension `ext`, it added when asked and not
    /// there whatever its case; whole from the panel's folder.
    pub fn eval_path_with(&self, typed: &str, append_ext: bool, ext: &str) -> PathBuf {
        let mut path = expand_env(&typed.trim().replace('"', ""));
        if path.is_empty() || path.ends_with('\\') {
            path.push_str(&self.default_name);
            path.push_str(ext);
        }
        if append_ext && !path.to_lowercase().ends_with(&ext.to_lowercase()) {
            path.push_str(ext);
        }
        let whole = full_path(&path, Some(&self.folder));
        // FAR's `ConvertNameToFull`: `.` and `..` resolved (the system's full path).
        std::path::absolute(&whole).unwrap_or(whole)
    }

    /// The new archive and its whole path the dialog closed with by OK; none for Cancel or Esc.
    pub fn answers(&self, closed: &Closed) -> Option<(NewArchive, PathBuf)> {
        if closed.button.is_none_or(|button| button == CANCEL) {
            return None;
        }
        let value = |item: usize| {
            (closed.values.get(item)).map_or((0, String::new()), |value| {
                (value.state, value.text.clone())
            })
        };
        let answers = self.read(&value).ok()?;
        let profile = &answers.profile;
        let ext = self.default_ext(
            &profile.format,
            (profile.create_sfx, profile.enable_volumes),
        );
        let path = self.eval_path_with(&answers.path, answers.append_ext, &ext);
        Some((answers, path))
    }
}
