//! The "Create archive" dialog's items as arclite keeps them (ui.cpp `UpdateDialog`): their enabled and
//! shown state by the format chosen (`set_control_state`), the path's extension changed with the format
//! (`change_extension`), a profile's options written in (`write_controls`), the methods' list of the
//! format, the levels' lines.

use nc_plugin_sdk::Controls;

use crate::create_dialog::{
    ADVANCED, ARC_PATH, CREATE_SFX, DELETE_PROFILE, ENABLE_VOLUMES, ENCRYPT, ENCRYPT_HEADER,
    IGNORE_ERRORS, LEVEL, METHOD, METHOD_LABEL, MOVE_FILES, OTHER, OTHER_LIST, PASSWORD,
    PASSWORD_LABEL, PASSWORD_VERIFY, PASSWORD_VISIBLE, PASSWORD_VISIBLE_LABEL, SEVEN, SFX_OPTIONS,
    SHOW_PASSWORD, SOLID, VOLUME_SIZE, VOLUME_SIZE_LABEL, ZIP,
};
use crate::create_formats::Archivers;
use crate::create_options::ProfileOptions;
use crate::update_options::LEVELS;
use crate::update_props::methods_of;
use crate::update_volumes::VOLUME_EXT;

/// arclite's `c_sfx_ext`.
const SFX_EXT: &str = ".exe";

/// `text` cut into lines of `width` characters (arclite's `word_wrap` of a path: no spaces to break at).
pub fn wrapped(text: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    (chars.chunks(width.max(1)))
        .map(|line| line.iter().collect())
        .collect()
}

/// The line of `level` in the levels' list; Normal for one that is none of them (arclite's
/// `update_level_list`: "normal or first enabled").
pub fn level_at(level: u32) -> usize {
    let normal = (LEVELS.iter()).position(|(_, known)| *known == 5);
    (LEVELS.iter())
        .position(|(_, known)| *known == level)
        .or(normal)
        .unwrap_or(0)
}

/// The line of `method` in `methods`, whatever its case; the first when it is none of them (arclite's
/// LZMA, the crate's PPMD).
pub fn method_line(methods: &[&str], method: &str) -> usize {
    (methods.iter())
        .position(|known| known.eq_ignore_ascii_case(method))
        .unwrap_or(0)
}

/// The methods' list of `format` put in, the method chosen kept when the list has it; else a 7z's LZMA,
/// a zip's Deflate (arclite's switch between 7z and zip).
pub fn method_lines(controls: &mut Controls, format: &str) {
    let methods = methods_of(format);
    let shown = controls.value(METHOD).text;
    let line = (methods.iter())
        .position(|known| known.eq_ignore_ascii_case(&shown))
        .unwrap_or_else(|| {
            let default = if format.eq_ignore_ascii_case("zip") {
                "Deflate"
            } else {
                "LZMA"
            };
            method_line(methods, default)
        });
    controls.set_lines(METHOD, methods);
    controls.set_state(METHOD, i32::try_from(line).unwrap_or(0));
}

/// arclite's `change_extension`: the path's extension, when it is the one it was last given (an `.exe`
/// or `.001` after it looked past), replaced by `ext`; `old` becomes `ext`.
pub fn change_extension(controls: &mut Controls, old: &mut String, ext: &str) {
    if old.is_empty() || ext.is_empty() || old.as_str() == ext {
        return;
    }
    let path = controls.value(ARC_PATH).text;
    let Some(changed) = with_extension(&path, old, ext) else {
        return;
    };
    controls.set_text(ARC_PATH, &changed);
    *old = ext.to_string();
}

/// `path` with its extension `old` (whatever its case; an `.exe` or `.001` after the format's one
/// counted with it) put as `ext`; none when it does not end so.
pub fn with_extension(path: &str, old: &str, ext: &str) -> Option<String> {
    let trimmed = path.trim().trim_matches('"');
    let name_start = trimmed.rfind('\\').map_or(0, |at| at + 1);
    let name = &trimmed[name_start..];
    let dot = name.rfind('.').filter(|dot| *dot > 0)?;
    let mut start = dot;
    let last = &name[dot..];
    if (last.eq_ignore_ascii_case(SFX_EXT) || last.eq_ignore_ascii_case(VOLUME_EXT))
        && let Some(before) = name[..dot].rfind('.').filter(|before| *before > 0)
    {
        start = before;
    }
    if !name[start..].eq_ignore_ascii_case(old) {
        return None;
    }
    let at = path.rfind(&name[start..])?;
    let mut changed = path.to_string();
    changed.replace_range(at..at + old.len(), ext);
    Some(changed)
}

/// arclite's `write_controls`: a profile's options in the items.
pub fn write_controls(controls: &mut Controls, options: &ProfileOptions, archivers: &Archivers) {
    let format = options.format.as_str();
    let is = |name: &str| format.eq_ignore_ascii_case(name);
    controls.set_checked(SEVEN, is("7z") && archivers.seven.is_some());
    controls.set_checked(ZIP, is("zip"));
    if let Some(at) = archivers.others.iter().position(|other| is(&other.name)) {
        controls.set_checked(OTHER, true);
        controls.set_state(OTHER_LIST, i32::try_from(at).unwrap_or(0));
    }
    let level = level_at(options.level);
    controls.set_state(LEVEL, i32::try_from(level).unwrap_or(0));
    let methods = methods_of(format);
    controls.set_lines(METHOD, methods);
    let method = method_line(methods, &options.method);
    controls.set_state(METHOD, i32::try_from(method).unwrap_or(0));
    controls.set_checked(SOLID, options.solid);
    controls.set_text(ADVANCED, &options.advanced);
    controls.set_checked(ENCRYPT, options.encrypt);
    let header = match options.encrypt_header {
        Some(true) => 1,
        Some(false) => 0,
        None => 2,
    };
    controls.set_state(ENCRYPT_HEADER, header);
    for item in [PASSWORD, PASSWORD_VERIFY, PASSWORD_VISIBLE] {
        controls.set_text(item, &options.password);
    }
    controls.set_checked(CREATE_SFX, options.create_sfx);
    controls.set_checked(ENABLE_VOLUMES, options.enable_volumes);
    controls.set_text(VOLUME_SIZE, &options.volume_size);
    controls.set_checked(MOVE_FILES, options.move_files);
    controls.set_checked(IGNORE_ERRORS, options.ignore_errors);
}

/// arclite's `set_control_state` for a new archive of `format`: each item it changes, whether it is
/// enabled and shown, by the level (Store: no method, no solid), the format (a method and encryption
/// for 7z and zip; solid and the header's encryption for 7z), Encrypt, Show password, the volumes, the
/// other formats' button, a profile chosen (Delete), a self-extracting 7z (not split).
pub fn control_state(
    state: &dyn Fn(usize) -> i32,
    format: &str,
    profile_chosen: bool,
) -> Vec<(usize, (bool, bool))> {
    let (is_7z, is_zip) = (format == "7z", format.eq_ignore_ascii_case("zip"));
    let compressed = state(LEVEL) != 0;
    let encrypt = (is_7z || is_zip) && state(ENCRYPT) == 1;
    let shown = state(SHOW_PASSWORD) == 1;
    // A self-extracting archive is not split (arclite's `set_control_state`).
    let sfx = state(CREATE_SFX) == 1;
    let volumes = state(ENABLE_VOLUMES) == 1 && !sfx;
    let sfx_not = !is_7z || !sfx;
    let mut items = vec![
        (DELETE_PROFILE, (profile_chosen, true)),
        (OTHER_LIST, (state(OTHER) == 1, true)),
        (METHOD_LABEL, ((is_7z || is_zip) && compressed, true)),
        (METHOD, ((is_7z || is_zip) && compressed, true)),
        (SOLID, (is_7z && compressed, true)),
        (ENCRYPT, (is_7z || is_zip, true)),
        (SHOW_PASSWORD, (encrypt, true)),
        (ENCRYPT_HEADER, (is_7z && encrypt, true)),
        (CREATE_SFX, (is_7z && !volumes, true)),
        (SFX_OPTIONS, (is_7z && sfx && !volumes, true)),
        (ENABLE_VOLUMES, (sfx_not, true)),
        (VOLUME_SIZE_LABEL, (volumes && sfx_not, true)),
        (VOLUME_SIZE, (volumes && sfx_not, true)),
    ];
    for item in PASSWORD_LABEL..=PASSWORD_VERIFY {
        items.push((item, (encrypt, !shown)));
    }
    for item in PASSWORD_VISIBLE_LABEL..=PASSWORD_VISIBLE {
        items.push((item, (encrypt, shown)));
    }
    items.push((MOVE_FILES, (true, true)));
    items
}

#[cfg(test)]
#[path = "create_dialog_state_tests.rs"]
mod tests;
