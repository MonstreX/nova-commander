//! NetWay's configuration kept in the program's settings of the plugin (FAR's `PluginSettings`): the
//! default transfer settings under WinSCP's names of `TCopyParamType::Save` ("CopyParam.TransferMode", ...;
//! what WinSCP's `Load` reads the same), NetBox's editor options (`FarConfiguration`'s
//! `EditorDownloadDefaultMode`, `EditorUploadSameOptions`, `EditorUploadOnSave`), NetBox's presets
//! (`TCopyParamPresetList::AddDefaultPresets`: not edited in NetBox, so fixed here).

use nc_plugin_sdk::Host;

use crate::copy_params::{CopyParams, NameCase, Resume, TransferMode, rights_of_text, rights_text};

/// The prefix of the default transfer settings' keys.
const COPY_PARAM: &str = "CopyParam.";

/// NetBox's editor options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorOptions {
    /// "Use default download options" (off: "Display download options dialog").
    pub download_default: bool,
    /// "Use same options as for download" (off: "Display upload options dialog").
    pub upload_same: bool,
    /// "Upload after every save".
    pub upload_on_save: bool,
}

impl Default for EditorOptions {
    /// NetBox's `TFarConfiguration::Default`.
    fn default() -> Self {
        Self {
            download_default: true,
            upload_same: true,
            upload_on_save: true,
        }
    }
}

const DOWNLOAD_DEFAULT: &str = "EditorDownloadDefaultMode";
const UPLOAD_SAME: &str = "EditorUploadSameOptions";
const UPLOAD_ON_SAVE: &str = "EditorUploadOnSave";

fn flag(on: bool) -> String {
    if on { "1" } else { "0" }.to_string()
}

fn number<T: Into<i64>>(value: T) -> String {
    value.into().to_string()
}

/// WinSCP's `TCopyParamType::Save`: each kept value by its name.
pub fn copy_param_entries(params: &CopyParams) -> Vec<(&'static str, String)> {
    vec![
        ("AddXToDirectories", flag(params.add_x_to_dirs)),
        ("Masks", params.ascii_mask.clone()),
        ("FileNameCase", number(params.name_case as i32)),
        ("PreserveReadOnly", flag(params.preserve_read_only)),
        ("PreserveTime", flag(params.preserve_time)),
        ("PreserveRights", flag(params.preserve_rights)),
        ("IgnorePermErrors", flag(params.ignore_perm_errors)),
        ("Text", rights_text(params.rights)),
        ("TransferMode", number(params.transfer_mode as i32)),
        ("ResumeSupport", number(params.resume as i32)),
        ("ResumeThreshold", params.resume_threshold.to_string()),
        (
            "ReplaceInvalidChars",
            number(i32::from(params.replace_invalid)),
        ),
        ("CalculateSize", flag(params.calculate_size)),
        ("IncludeFileMask", params.include_mask.clone()),
        ("TempPath", params.temp_path.clone()),
        ("ClearArchive", flag(params.clear_archive)),
        ("CPSLimit", params.speed_limit.to_string()),
        ("NewerOnly", flag(params.newer_only)),
        ("Queue", flag(params.queue)),
    ]
}

fn read_flag(value: Option<String>, default: bool) -> bool {
    match value.as_deref().map(str::trim) {
        Some("1") => true,
        Some("0") => false,
        _ => default,
    }
}

fn read_number<T: std::str::FromStr>(value: Option<String>, default: T) -> T {
    value
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn mode_of(number: i32) -> Option<TransferMode> {
    [
        TransferMode::Binary,
        TransferMode::Text,
        TransferMode::Automatic,
    ]
    .get(usize::try_from(number).ok()?)
    .copied()
}

fn case_of(number: i32) -> Option<NameCase> {
    let cases = [
        NameCase::NoChange,
        NameCase::Upper,
        NameCase::Lower,
        NameCase::FirstUpper,
        NameCase::LowerShort,
    ];
    cases.get(usize::try_from(number).ok()?).copied()
}

fn resume_of(number: i32) -> Option<Resume> {
    [Resume::On, Resume::Smart, Resume::Off]
        .get(usize::try_from(number).ok()?)
        .copied()
}

/// WinSCP's `TCopyParamType::Load` over the defaults: `value` gives each name's text, a value not read
/// keeps the default.
pub fn copy_params_from(value: &dyn Fn(&str) -> Option<String>) -> CopyParams {
    let base = CopyParams::default();
    let int = |name: &str| read_number::<i32>(value(name), -1);
    CopyParams {
        add_x_to_dirs: read_flag(value("AddXToDirectories"), base.add_x_to_dirs),
        ascii_mask: value("Masks").unwrap_or(base.ascii_mask),
        name_case: case_of(int("FileNameCase")).unwrap_or(base.name_case),
        preserve_read_only: read_flag(value("PreserveReadOnly"), base.preserve_read_only),
        preserve_time: read_flag(value("PreserveTime"), base.preserve_time),
        preserve_rights: read_flag(value("PreserveRights"), base.preserve_rights),
        ignore_perm_errors: read_flag(value("IgnorePermErrors"), base.ignore_perm_errors),
        rights: (value("Text").as_deref())
            .and_then(rights_of_text)
            .unwrap_or(base.rights),
        transfer_mode: mode_of(int("TransferMode")).unwrap_or(base.transfer_mode),
        resume: resume_of(int("ResumeSupport")).unwrap_or(base.resume),
        resume_threshold: read_number(value("ResumeThreshold"), base.resume_threshold),
        replace_invalid: read_number(value("ReplaceInvalidChars"), 1) != 0,
        calculate_size: read_flag(value("CalculateSize"), base.calculate_size),
        include_mask: value("IncludeFileMask").unwrap_or_default(),
        temp_path: value("TempPath").unwrap_or_default(),
        clear_archive: read_flag(value("ClearArchive"), base.clear_archive),
        speed_limit: read_number(value("CPSLimit"), base.speed_limit),
        newer_only: read_flag(value("NewerOnly"), base.newer_only),
        queue: read_flag(value("Queue"), base.queue),
        file_mask: String::new(),
    }
}

/// The default transfer settings kept (WinSCP's `DefaultCopyParam`).
pub fn copy_params(host: &Host) -> CopyParams {
    copy_params_from(&|name| host.setting(&format!("{COPY_PARAM}{name}")))
}

/// The default transfer settings kept from now on ("Use same settings next time", the configuration's
/// "Transfer settings"); the file mask is a transfer's own.
pub fn save_copy_params(host: &Host, params: &CopyParams) {
    for (name, value) in copy_param_entries(params) {
        host.set_setting(&format!("{COPY_PARAM}{name}"), &value);
    }
}

pub fn editor_options(host: &Host) -> EditorOptions {
    let base = EditorOptions::default();
    EditorOptions {
        download_default: host.bool_setting(DOWNLOAD_DEFAULT, base.download_default),
        upload_same: host.bool_setting(UPLOAD_SAME, base.upload_same),
        upload_on_save: host.bool_setting(UPLOAD_ON_SAVE, base.upload_on_save),
    }
}

pub fn save_editor_options(host: &Host, options: EditorOptions) {
    host.set_bool(DOWNLOAD_DEFAULT, options.download_default);
    host.set_bool(UPLOAD_SAME, options.upload_same);
    host.set_bool(UPLOAD_ON_SAVE, options.upload_on_save);
}

/// NetBox's presets: their names and parameters (`AddDefaultPresets`).
pub fn presets() -> Vec<(&'static str, CopyParams)> {
    let base = CopyParams::default();
    vec![
        ("Default", base.clone()),
        (
            "Text files (ASCII)",
            CopyParams {
                transfer_mode: TransferMode::Text,
                ..base.clone()
            },
        ),
        (
            "Binary all",
            CopyParams {
                transfer_mode: TransferMode::Binary,
                preserve_rights: true,
                ..base.clone()
            },
        ),
        (
            "No preserve time",
            CopyParams {
                preserve_time: false,
                ..base
            },
        ),
    ]
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
