//! A self-extracting archive's options and modules as FAR's arclite has them (common.hpp `SfxOptions`,
//! `SfxVersionInfo`, `SfxInstallConfig`; archive.cpp `SfxModules`, `c_known_sfx_modules`; options.cpp
//! `load_sfx_options`, `save_sfx_options` under `sfx.*`; sfx.cpp `generate_install_config`).

use std::path::{Path, PathBuf};

use nc_plugin_sdk::Host;

/// arclite's `c_sfx_ext`.
pub const SFX_EXT: &str = ".exe";

/// The module `arc:c -sfx` takes when it names none (arclite's cmdline.cpp).
pub const DEFAULT_MODULE: &str = "7z.sfx";

/// arclite's `MSG_SFX_OPTIONS_DLG_WRONG_MODULE`.
pub const WRONG_MODULE: &str = "Select SFX module";

/// A new 7z's module and options (arclite's `create_sfx` with `sfx_options`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sfx {
    pub module: PathBuf,
    pub options: SfxOptions,
}

/// arclite's `SfxVersionInfo`: the version resource put into the module.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionInfo {
    pub version: String,
    pub comments: String,
    pub company_name: String,
    pub file_description: String,
    pub legal_copyright: String,
    pub product_name: String,
}

/// arclite's `SfxInstallConfig`: the installer's configuration after the module.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallConfig {
    pub title: String,
    pub begin_prompt: String,
    /// `yes`, `no`, or empty (the check box's third state).
    pub progress: String,
    pub run_program: String,
    pub directory: String,
    pub execute_file: String,
    pub execute_parameters: String,
}

/// arclite's `SfxOptions`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SfxOptions {
    /// The module's file name (`7z.sfx`).
    pub name: String,
    pub replace_icon: bool,
    pub icon_path: String,
    pub replace_version: bool,
    pub ver_info: VersionInfo,
    pub append_install_config: bool,
    pub install_config: InstallConfig,
}

/// The texts kept under `sfx.*`, by their names, as getters and setters of [`SfxOptions`].
type Text = (
    &'static str,
    fn(&SfxOptions) -> &String,
    fn(&mut SfxOptions) -> &mut String,
);

const TEXTS: [Text; 15] = [
    ("name", |o| &o.name, |o| &mut o.name),
    ("icon_path", |o| &o.icon_path, |o| &mut o.icon_path),
    (
        "ver_info.version",
        |o| &o.ver_info.version,
        |o| &mut o.ver_info.version,
    ),
    (
        "ver_info.comments",
        |o| &o.ver_info.comments,
        |o| &mut o.ver_info.comments,
    ),
    (
        "ver_info.company_name",
        |o| &o.ver_info.company_name,
        |o| &mut o.ver_info.company_name,
    ),
    (
        "ver_info.file_description",
        |o| &o.ver_info.file_description,
        |o| &mut o.ver_info.file_description,
    ),
    (
        "ver_info.legal_copyright",
        |o| &o.ver_info.legal_copyright,
        |o| &mut o.ver_info.legal_copyright,
    ),
    (
        "ver_info.product_name",
        |o| &o.ver_info.product_name,
        |o| &mut o.ver_info.product_name,
    ),
    (
        "install_config.title",
        |o| &o.install_config.title,
        |o| &mut o.install_config.title,
    ),
    (
        "install_config.begin_prompt",
        |o| &o.install_config.begin_prompt,
        |o| &mut o.install_config.begin_prompt,
    ),
    (
        "install_config.progress",
        |o| &o.install_config.progress,
        |o| &mut o.install_config.progress,
    ),
    (
        "install_config.run_program",
        |o| &o.install_config.run_program,
        |o| &mut o.install_config.run_program,
    ),
    (
        "install_config.directory",
        |o| &o.install_config.directory,
        |o| &mut o.install_config.directory,
    ),
    (
        "install_config.execute_file",
        |o| &o.install_config.execute_file,
        |o| &mut o.install_config.execute_file,
    ),
    (
        "install_config.execute_parameters",
        |o| &o.install_config.execute_parameters,
        |o| &mut o.install_config.execute_parameters,
    ),
];

/// The check boxes kept under `sfx.*`.
const FLAGS: [&str; 3] = ["replace_icon", "replace_version", "append_install_config"];

impl SfxOptions {
    /// The options kept under `prefix` (`""` for arclite's `update_sfx_options`, a profile's key for
    /// its own).
    pub fn load(host: &Host, prefix: &str) -> Self {
        Self::read(
            &|name| host.setting(&format!("{prefix}sfx.{name}")),
            &|name| host.bool_setting(&format!("{prefix}sfx.{name}"), false),
        )
    }

    /// The options from their values by name (`text`, `flag`); a text not there the default's.
    pub fn read(text: &dyn Fn(&str) -> Option<String>, flag: &dyn Fn(&str) -> bool) -> Self {
        let mut options = Self::default();
        for (name, _, set) in TEXTS {
            if let Some(text) = text(name) {
                *set(&mut options) = text;
            }
        }
        options.replace_icon = flag(FLAGS[0]);
        options.replace_version = flag(FLAGS[1]);
        options.append_install_config = flag(FLAGS[2]);
        options
    }

    /// The options kept under `prefix`.
    pub fn save(&self, host: &Host, prefix: &str) {
        self.write(
            &mut |name, text| host.set_setting(&format!("{prefix}sfx.{name}"), text),
            &mut |name, value| host.set_bool(&format!("{prefix}sfx.{name}"), value),
        );
    }

    /// Each value given by name to `text` or `flag`.
    pub fn write(&self, text: &mut dyn FnMut(&str, &str), flag: &mut dyn FnMut(&str, bool)) {
        for (name, get, _) in TEXTS {
            text(name, get(self));
        }
        let flags = [
            self.replace_icon,
            self.replace_version,
            self.append_install_config,
        ];
        for (name, value) in FLAGS.iter().zip(flags) {
            flag(name, value);
        }
    }

    /// The values under `prefix` emptied (a profile deleted keeps nothing behind).
    pub fn clear(host: &Host, prefix: &str) {
        for (name, _, _) in TEXTS {
            host.set_setting(&format!("{prefix}sfx.{name}"), "");
        }
        for name in FLAGS {
            host.set_setting(&format!("{prefix}sfx.{name}"), "");
        }
    }
}

/// arclite's `c_known_sfx_modules`: each module's name, description, whether it has every codec, and
/// whether it takes an install configuration.
const KNOWN: [(&str, &str, bool, bool); 6] = [
    ("7z.sfx", "Standard GUI", true, false),
    ("7zCon.sfx", "Standard console", true, false),
    ("7zS.sfx", "Installer GUI (static)", false, true),
    ("7zSD.sfx", "Installer GUI", false, true),
    ("7zS2.sfx", "Small installer GUI", false, false),
    ("7zS2con.sfx", "Small installer console", false, false),
];

/// A path's file name, as text.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn known(path: &Path) -> Option<&'static (&'static str, &'static str, bool, bool)> {
    let name = file_name(path);
    KNOWN
        .iter()
        .find(|known| known.0.eq_ignore_ascii_case(&name))
}

/// The SFX modules found with the library (arclite's `SfxModules`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Modules(pub Vec<PathBuf>);

impl Modules {
    /// The module of file name `name` whatever its case (arclite's `find_by_name`).
    pub fn by_name(&self, name: &str) -> Option<usize> {
        (self.0.iter()).position(|path| file_name(path).eq_ignore_ascii_case(name))
    }

    /// arclite's `SfxModule::description`: a known module's words, else "Unknown [name]".
    pub fn description(&self, at: usize) -> String {
        let path = &self.0[at];
        known(path).map_or_else(
            || format!("Unknown [{}]", file_name(path)),
            |known| known.1.to_string(),
        )
    }

    /// arclite's `all_codecs`: an unknown module is taken to have them.
    pub fn all_codecs(&self, at: usize) -> bool {
        known(&self.0[at]).is_none_or(|known| known.2)
    }

    /// arclite's `install_config`: an unknown module is taken to read one.
    pub fn install_config(&self, at: usize) -> bool {
        known(&self.0[at]).is_none_or(|known| known.3)
    }
}

/// arclite's `generate_install_config`: the lines given, between the markers, in UTF-8.
pub fn install_config_text(config: &InstallConfig) -> Vec<u8> {
    let mut text = String::from(";!@Install@!UTF-8!\n");
    for (name, value) in [
        ("Title", &config.title),
        ("BeginPrompt", &config.begin_prompt),
        ("Progress", &config.progress),
        ("RunProgram", &config.run_program),
        ("Directory", &config.directory),
        ("ExecuteFile", &config.execute_file),
        ("ExecuteParameters", &config.execute_parameters),
    ] {
        if !value.is_empty() {
            text.push_str(&format!("{name}=\"{value}\"\n"));
        }
    }
    text.push_str(";!@InstallEnd@!\n");
    text.into_bytes()
}

#[cfg(test)]
#[path = "sfx_tests.rs"]
mod tests;
