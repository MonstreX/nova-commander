//! A new archive's options as arclite has them (`UpdateOptions` for `create_archive` and for `put_files`
//! into a panel of no archive yet; `ProfileOptions`, `UpdateProfiles`, options.cpp): the archive's path,
//! its format, what the "Add files to archive" dialog has, a self-extracting archive and volumes; the
//! settings kept under arclite's `update_*` names, the named profiles under `profiles`.

use nc_plugin_sdk::Host;

use crate::extract_options::Overwrite;
use crate::sfx::{Modules, Sfx, SfxOptions};
use crate::update_options::{DEFLATE, UpdateOptions, UpdateSettings};

/// The format the plugin writes, by 7-Zip's name (arclite's radio button "&zip").
pub const FORMAT: &str = "zip";

/// Its archives' extension (7-Zip's `default_extension`).
pub const EXTENSION: &str = ".zip";

/// What a named profile keeps (arclite's `ProfileOptions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileOptions {
    /// The format by 7-Zip's name.
    pub format: String,
    pub level: u32,
    pub method: String,
    pub solid: bool,
    pub advanced: String,
    pub encrypt: bool,
    /// 7z's "Encrypt header": on, off, or its third state.
    pub encrypt_header: Option<bool>,
    pub password: String,
    pub create_sfx: bool,
    /// arclite's `sfx_options`.
    pub sfx: SfxOptions,
    pub enable_volumes: bool,
    pub volume_size: String,
    pub move_files: bool,
    pub ignore_errors: bool,
}

impl Default for ProfileOptions {
    /// arclite's `ProfileOptions()`.
    fn default() -> Self {
        Self {
            format: "7z".to_string(),
            level: 5,
            method: String::new(),
            solid: true,
            advanced: String::new(),
            encrypt: false,
            encrypt_header: None,
            password: String::new(),
            create_sfx: false,
            sfx: SfxOptions::default(),
            enable_volumes: false,
            volume_size: String::new(),
            move_files: false,
            ignore_errors: false,
        }
    }
}

impl ProfileOptions {
    /// arclite's `operator==` of two profiles: what the format does not use not compared (the method
    /// when storing, the password when not encrypting, the volumes' size when not split).
    pub fn same(&self, other: &Self) -> bool {
        if self.format != other.format || self.level != other.level {
            return false;
        }
        let is_7z = self.format == "7z";
        let is_zip = self.format == FORMAT;
        if (is_7z || is_zip)
            && ((self.level != 0 && self.method != other.method) || self.solid != other.solid)
        {
            return false;
        }
        if self.advanced != other.advanced {
            return false;
        }
        if (is_7z || is_zip)
            && (self.encrypt != other.encrypt
                || (self.encrypt && self.password != other.password)
                || (self.encrypt && is_7z && self.encrypt_header != other.encrypt_header))
        {
            return false;
        }
        if is_7z
            && (self.create_sfx != other.create_sfx || (self.create_sfx && self.sfx != other.sfx))
        {
            return false;
        }
        if (!is_7z || !self.create_sfx)
            && (self.enable_volumes != other.enable_volumes
                || (self.enable_volumes && self.volume_size != other.volume_size))
        {
            return false;
        }
        self.move_files == other.move_files && self.ignore_errors == other.ignore_errors
    }
}

impl ProfileOptions {
    /// The module and options a new 7z is written after (arclite's `create_sfx` of a 7z), its module
    /// one of `modules`; none when it is not made self-extracting or the module is not there.
    pub fn sfx_of(&self, modules: &Modules) -> Option<Sfx> {
        let wanted = self.create_sfx && self.format == "7z";
        let at = wanted.then(|| modules.by_name(&self.sfx.name)).flatten()?;
        Some(Sfx {
            module: modules.0[at].clone(),
            options: self.sfx.clone(),
        })
    }
}

/// One named profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub options: ProfileOptions,
}

/// The profiles' list, in the order of their names whatever their case (arclite's `UpdateProfiles`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profiles(pub Vec<Profile>);

/// The values a profile keeps, by their settings' names (arclite's).
const VALUES: [&str; 13] = [
    "arc_type",
    "level",
    "method",
    "solid",
    "advanced",
    "encrypt",
    "encrypt_header",
    "password",
    "create_sfx",
    "enable_volumes",
    "volume_size",
    "move_files",
    "ignore_errors",
];

/// The settings' key of a profile's value.
fn profile_key(name: &str, value: &str) -> String {
    format!("profiles\\{name}\\{value}")
}

/// The third state of a check box kept as arclite's `triUndef`.
const UNDEFINED: i64 = 2;

fn tri_of(value: i64) -> Option<bool> {
    match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn tri_value(tri: Option<bool>) -> i64 {
    tri.map_or(UNDEFINED, i64::from)
}

impl Profiles {
    /// The profiles kept (their names under `profiles`, one a line).
    pub fn load(host: &Host) -> Self {
        let names = host.setting("profiles").unwrap_or_default();
        let defaults = ProfileOptions::default();
        let mut profiles: Vec<Profile> = (names.lines())
            .filter(|name| !name.is_empty())
            .map(|name| {
                let text = |value: &str, default: &str| {
                    (host.setting(&profile_key(name, value))).unwrap_or_else(|| default.to_string())
                };
                let flag = |value: &str, default: bool| {
                    host.bool_setting(&profile_key(name, value), default)
                };
                let level = host.int_setting(&profile_key(name, "level"), 5);
                let header = host.int_setting(&profile_key(name, "encrypt_header"), UNDEFINED);
                Profile {
                    name: name.to_string(),
                    options: ProfileOptions {
                        format: text("arc_type", &defaults.format),
                        level: u32::try_from(level).unwrap_or(defaults.level),
                        method: text("method", ""),
                        solid: flag("solid", defaults.solid),
                        advanced: text("advanced", ""),
                        encrypt: flag("encrypt", false),
                        encrypt_header: tri_of(header),
                        password: text("password", ""),
                        create_sfx: flag("create_sfx", false),
                        sfx: SfxOptions::load(host, &profile_key(name, "")),
                        enable_volumes: flag("enable_volumes", false),
                        volume_size: text("volume_size", ""),
                        move_files: flag("move_files", false),
                        ignore_errors: flag("ignore_errors", false),
                    },
                }
            })
            .collect();
        profiles.sort_by_key(|profile| profile.name.to_lowercase());
        Self(profiles)
    }

    /// Whether `name` can name a profile: not empty, one line, no `\` (arclite's are the settings'
    /// keys under `profiles`).
    pub fn fits(name: &str) -> bool {
        !name.trim().is_empty() && !name.contains(['\n', '\r', '\\'])
    }

    /// The profiles kept as they are now; the values of one gone emptied (arclite deletes every
    /// profile's key first: no password stays behind).
    pub fn save(&self, host: &Host) {
        let names: Vec<&str> = self.0.iter().map(|profile| profile.name.as_str()).collect();
        let before = host.setting("profiles").unwrap_or_default();
        for gone in before.lines().filter(|name| !names.contains(name)) {
            for value in VALUES {
                host.set_setting(&profile_key(gone, value), "");
            }
            SfxOptions::clear(host, &profile_key(gone, ""));
        }
        host.set_setting("profiles", &names.join("\n"));
        for Profile { name, options } in &self.0 {
            let key = |value: &str| profile_key(name, value);
            host.set_setting(&key("arc_type"), &options.format);
            host.set_int(&key("level"), i64::from(options.level));
            host.set_setting(&key("method"), &options.method);
            host.set_bool(&key("solid"), options.solid);
            host.set_setting(&key("advanced"), &options.advanced);
            host.set_bool(&key("encrypt"), options.encrypt);
            host.set_int(&key("encrypt_header"), tri_value(options.encrypt_header));
            host.set_setting(&key("password"), &options.password);
            host.set_bool(&key("create_sfx"), options.create_sfx);
            options.sfx.save(host, &key(""));
            host.set_bool(&key("enable_volumes"), options.enable_volumes);
            host.set_setting(&key("volume_size"), &options.volume_size);
            host.set_bool(&key("move_files"), options.move_files);
            host.set_bool(&key("ignore_errors"), options.ignore_errors);
        }
    }

    /// The profile named `name` whatever its case (arclite's `find_by_name`).
    pub fn by_name(&self, name: &str) -> Option<usize> {
        let name = name.to_lowercase();
        (self.0.iter()).position(|profile| profile.name.to_lowercase() == name)
    }

    /// The first profile with these options (arclite's `find_by_options`).
    pub fn by_options(&self, options: &ProfileOptions) -> Option<usize> {
        (self.0.iter()).position(|profile| profile.options.same(options))
    }

    /// arclite's `UpdateProfiles::update`: the profile of that name takes the options (one with the
    /// same options gone), else the one with the same options takes the name, else a new one; the list
    /// sorted again.
    pub fn update(&mut self, name: &str, options: &ProfileOptions) {
        let profile = Profile {
            name: name.to_string(),
            options: options.clone(),
        };
        match (self.by_name(name), self.by_options(options)) {
            (Some(named), Some(same)) => {
                self.0[named] = profile;
                if same != named {
                    self.0.remove(same);
                }
            }
            (Some(at), None) | (None, Some(at)) => self.0[at] = profile,
            (None, None) => self.0.push(profile),
        }
        self.0.sort_by_key(|profile| profile.name.to_lowercase());
    }
}

/// What arclite keeps for a new archive beside an update's (`update_arc_format_name`,
/// `update_levels`, `update_solid`, `update_encrypt_header`, `update_password`, `update_create_sfx`,
/// `update_enable_volumes`, `update_volume_size`, `update_append_ext`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSettings {
    pub format: String,
    pub levels: String,
    pub solid: bool,
    pub encrypt_header: Option<bool>,
    pub password: String,
    pub create_sfx: bool,
    /// arclite's `update_sfx_options` (its `sfx.*` values).
    pub sfx: SfxOptions,
    pub enable_volumes: bool,
    pub volume_size: String,
    pub append_ext: bool,
}

impl Default for NewSettings {
    /// arclite's defaults (`Options()`).
    fn default() -> Self {
        Self {
            format: "7z".to_string(),
            levels: String::new(),
            solid: true,
            encrypt_header: None,
            password: String::new(),
            create_sfx: false,
            sfx: SfxOptions::default(),
            enable_volumes: false,
            volume_size: String::new(),
            append_ext: false,
        }
    }
}

impl NewSettings {
    pub fn load(host: &Host) -> Self {
        let defaults = Self::default();
        let text = |key: &str, default: &str| host.setting(key).unwrap_or_else(|| default.into());
        Self {
            format: text("update_arc_format_name", &defaults.format),
            levels: text("update_levels", ""),
            solid: host.bool_setting("update_solid", defaults.solid),
            encrypt_header: tri_of(host.int_setting("update_encrypt_header", UNDEFINED)),
            password: text("update_password", ""),
            create_sfx: host.bool_setting("update_create_sfx", false),
            sfx: SfxOptions::load(host, ""),
            enable_volumes: host.bool_setting("update_enable_volumes", false),
            volume_size: text("update_volume_size", ""),
            append_ext: host.bool_setting("update_append_ext", false),
        }
    }
}

/// How the new archive was asked for: the Plugins menu's "Create archive" (arclite's
/// `create_archive`) or files put into a panel of no archive yet (its `put_files`, Shift+F1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    Menu,
    Put,
}

/// One new archive (arclite's `UpdateOptions` with `arc_path`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewArchive {
    /// As typed: relative to the panel's folder, its extension added by the dialog.
    pub path: String,
    pub append_ext: bool,
    pub profile: ProfileOptions,
    /// Each format's level (arclite's `levels`, `zip=5;7z=9`).
    pub levels: String,
    pub show_password: bool,
    pub open_shared: bool,
}

/// arclite's default name for the archive: the one source's name, else its folder's (the first
/// source's at a disk's root).
pub fn default_name(folder: &str, sources: &[String]) -> String {
    let first = sources.first().cloned().unwrap_or_default();
    let trimmed = folder.trim_end_matches(['\\', '/']);
    // arclite's `is_root_path`: a disk's root, or a share's (`\\server\share`).
    let share_root =
        (trimmed.strip_prefix("\\\\")).is_some_and(|share| share.split(['\\', '/']).count() <= 2);
    let at_root = trimmed.is_empty() || trimmed.ends_with(':') || share_root;
    if sources.len() == 1 || at_root {
        return first;
    }
    trimmed
        .rsplit(['\\', '/'])
        .next()
        .map_or(first, str::to_string)
}

/// The method a new zip starts with: the kept one when arclite offers it, else Deflate (arclite's
/// switch from 7z's LZMA).
pub(crate) fn method_of(kept: &str) -> String {
    if kept.eq_ignore_ascii_case("LZMA") || kept.is_empty() {
        return DEFLATE.to_string();
    }
    (crate::update_props::LIBRARY_METHODS.iter())
        .find(|method| method.eq_ignore_ascii_case(kept))
        .map_or(DEFLATE, |method| method)
        .to_string()
}

/// The method a new archive of `format` starts with: a zip's as [`method_of`]; another's the kept one
/// when arclite offers it, else LZMA (arclite's empty method is 7z's LZMA).
pub(crate) fn method_for(format: &str, kept: &str) -> String {
    if format.eq_ignore_ascii_case(FORMAT) {
        return method_of(kept);
    }
    (crate::update_props::LIBRARY_METHODS.iter())
        .find(|method| method.eq_ignore_ascii_case(kept))
        .map_or("LZMA", |method| method)
        .to_string()
}

impl NewArchive {
    /// arclite's options before its dialog: the menu's `create_archive` neither encrypts nor moves
    /// (nor takes the advanced parameters); `put_files` takes them as kept.
    pub fn starting(
        (name, format): (String, &str),
        (update, new): (&UpdateSettings, &NewSettings),
        asked: Asked,
        open_shared: bool,
    ) -> Self {
        let put = asked == Asked::Put;
        let encrypt = put && update.encrypt;
        Self {
            path: name,
            append_ext: new.append_ext,
            profile: ProfileOptions {
                format: format.to_string(),
                level: update.level,
                method: method_for(format, &update.method),
                solid: new.solid,
                advanced: if put {
                    update.advanced.clone()
                } else {
                    String::new()
                },
                encrypt,
                encrypt_header: new.encrypt_header,
                password: if encrypt {
                    new.password.clone()
                } else {
                    String::new()
                },
                create_sfx: put && new.create_sfx,
                sfx: new.sfx.clone(),
                // The menu's `create_archive` splits none.
                enable_volumes: put && new.enable_volumes,
                volume_size: new.volume_size.clone(),
                move_files: put && update.move_files,
                ignore_errors: update.ignore_errors,
            },
            levels: new.levels.clone(),
            show_password: update.show_password,
            open_shared,
        }
    }

    /// What the writing takes of it: every file new (arclite's `PrepareUpdate` with `oaOverwrite`); a 7z
    /// made self-extracting by its module among `modules` (none there: not made so).
    pub fn update_options(&self, modules: &Modules) -> UpdateOptions {
        let profile = &self.profile;
        UpdateOptions {
            level: profile.level,
            method: profile.method.clone(),
            advanced: profile.advanced.clone(),
            encrypt: profile.encrypt,
            show_password: self.show_password,
            password: if profile.encrypt {
                profile.password.clone()
            } else {
                String::new()
            },
            overwrite: Overwrite::Overwrite,
            move_files: profile.move_files,
            ignore_errors: profile.ignore_errors,
            open_shared: self.open_shared,
            kind: crate::update_options::Kind {
                format: profile.format.clone(),
                solid: profile.solid,
                encrypt_header: profile.encrypt_header,
                volume_size: profile
                    .enable_volumes
                    .then(|| crate::create_dialog::parse_size(&profile.volume_size)),
                sfx: profile.sfx_of(modules),
            },
        }
    }

    /// "Save settings" of the "Create archive" dialog (arclite's `save_params_ctrl_id` with `new_arc`).
    pub fn save(&self, host: &Host) {
        let profile = &self.profile;
        host.set_setting("update_arc_format_name", &profile.format);
        host.set_bool("update_create_sfx", profile.create_sfx);
        profile.sfx.save(host, "");
        host.set_bool("update_enable_volumes", profile.enable_volumes);
        host.set_setting("update_volume_size", &profile.volume_size);
        host.set_int("update_encrypt_header", tri_value(profile.encrypt_header));
        host.set_setting("update_password", &profile.password);
        host.set_bool("update_append_ext", self.append_ext);
        host.set_bool("update_move_files", profile.move_files);
        host.set_int("update_level", i64::from(profile.level));
        host.set_setting("update_levels", &self.levels);
        host.set_setting("update_method", &profile.method);
        host.set_bool("update_solid", profile.solid);
        host.set_setting("update_advanced", &profile.advanced);
        host.set_bool("update_encrypt", profile.encrypt);
        host.set_bool("update_show_password", self.show_password);
        host.set_bool("update_ignore_errors", profile.ignore_errors);
    }
}

/// arclite's `arc_levels('s')`: `levels` with the format's level first, its old one gone.
pub fn with_level(levels: &str, format: &str, level: u32) -> String {
    let own = format!("{format}=");
    let rest: Vec<&str> = (levels.split(';'))
        .filter(|part| !part.is_empty() && !part.starts_with(&own))
        .collect();
    let mut parts = vec![format!("{own}{level}")];
    parts.extend(rest.iter().map(|part| (*part).to_string()));
    parts.join(";")
}

/// arclite's `arc_levels('g')`: the format's level in `levels`, when it has one.
pub fn level_in(levels: &str, format: &str) -> Option<u32> {
    let own = format!("{format}=");
    (levels.split(';'))
        .find_map(|part| part.strip_prefix(&own))
        .map(|level| {
            let digits: String = level.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap_or(0)
        })
}

#[cfg(test)]
#[path = "create_options_tests.rs"]
mod tests;
