//! An archive's writing parameters as arclite finds and gives them for 7-Zip's library: what an archive
//! there was written with (`load_update_props`: its method, solid, encrypted) and the parameters set on
//! the library before it writes (`Archive::set_properties`: the method, the level, solid, the header's
//! encryption, the advanced parameters typed, folded as arclite folds them).

use nc_plugin_sdk::sevenzip::{Archive, Prop, prop};

use crate::update_options::UpdateOptions;

/// 7-Zip's method name for no compression (arclite's `c_method_copy`).
pub const COPY: &str = "Copy";

/// The methods arclite offers for 7z and zip (`c_methods`), in its order.
pub const LIBRARY_METHODS: [&str; 5] = ["LZMA", "LZMA2", "PPMD", "Deflate", "Deflate64"];

/// What an archive there was written with (arclite's `m_level`, `m_method`, `m_solid`, `m_encrypted`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveProps {
    pub level: u32,
    pub method: String,
    pub solid: bool,
    pub encrypted: bool,
}

/// arclite's `load_update_props`: an item encrypted, the archive solid, its method (the first word of
/// 7-Zip's "Method" that is a known one; `Copy`: level 0 and LZMA; a zip's default Deflate), its level
/// "maximum" (7) unless stored. arclite's look at the first item's method never runs (it tests
/// `GetNumberOfItems`'s `S_OK`, 0, as true), so only the archive's is read.
pub fn load_update_props(archive: &Archive, is_zip: bool) -> ArchiveProps {
    let encrypted = (0..archive.count())
        .any(|index| archive.property(index, prop::ENCRYPTED) == Prop::Bool(true));
    let solid = archive.archive_property(prop::SOLID) == Prop::Bool(true);
    let methods = archive.archive_property(prop::METHOD);
    let (level, method) = method_of(methods.text(), is_zip);
    ArchiveProps {
        level,
        method,
        solid,
        encrypted,
    }
}

/// The level and method a "Method" text names (arclite's loop over its words).
pub fn method_of(methods: Option<&str>, is_zip: bool) -> (u32, String) {
    let Some(methods) = methods else {
        let method = if is_zip { "Deflate" } else { "LZMA" };
        return (7, method.to_string());
    };
    for word in methods.split(' ') {
        let name = word.split(':').next().unwrap_or_default();
        if name.eq_ignore_ascii_case(COPY) {
            return (0, "LZMA".to_string());
        }
        if let Some(known) = (LIBRARY_METHODS.iter()).find(|known| known.eq_ignore_ascii_case(name))
        {
            return (7, (*known).to_string());
        }
    }
    (7, "LZMA".to_string())
}

/// arclite's `update_level_list`: whether `format` takes the level (a bzip2 not stored, a wim or tar
/// stored only).
pub fn level_allowed(format: &str, level: u32) -> bool {
    !((level == 0 && format == "bzip2") || (level != 0 && (format == "wim" || format == "tar")))
}

/// The level a format writes when `level` is not one it takes: Normal, else the first it takes.
pub fn fit_level(format: &str, level: u32) -> u32 {
    if level_allowed(format, level) {
        return level;
    }
    if level_allowed(format, 5) {
        return 5;
    }
    (crate::update_options::LEVELS.iter())
        .map(|(_, level)| *level)
        .find(|level| level_allowed(format, *level))
        .unwrap_or(level)
}

/// The methods offered for `format` (arclite's `c_methods`, the same for every format; only a 7z's and a
/// zip's are taken).
pub fn methods_of(_format: &str) -> &'static [&'static str] {
    &LIBRARY_METHODS
}

/// arclite's `ExternalCodec` defaults (`defopts`): levels 1..9 as they are, no BCJ of its own.
const MIN_LEVEL: u32 = 1;
const MAX_LEVEL: u32 = 9;

/// A parameter typed after arclite's shorthands are written out (`s`, `s1`, `s+` -> `s=on`; `s0`,
/// `s-` -> `s=off`; `qs...` likewise; `xN` -> `x=N`; `mtN` -> `mt=N`; `yxN` -> `yx=N`).
fn written_out(param: &str) -> String {
    match param {
        "s" | "s1" | "s+" => return "s=on".to_string(),
        "s0" | "s-" => return "s=off".to_string(),
        "qs" | "qs1" | "qs+" | "qs=on" => return "qs=on".to_string(),
        "qs0" | "qs-" | "qs=off" => return "qs=off".to_string(),
        _ => {}
    }
    let bytes = param.as_bytes();
    let digit = |at: usize| bytes.get(at).is_some_and(u8::is_ascii_digit);
    let nonzero = |at: usize| {
        bytes
            .get(at)
            .is_some_and(|byte| (b'1'..=b'9').contains(byte))
    };
    if bytes.first() == Some(&b'x') && digit(1) {
        return format!("x={}", &param[1..]);
    }
    if (param.starts_with("mt") || param.starts_with("yx")) && nonzero(2) {
        return format!("{}={}", &param[..2], &param[2..]);
    }
    param.to_string()
}

/// A single digit from 0 to `up`.
fn is_digit(name: &str, up: char) -> bool {
    let mut chars = name.chars();
    matches!((chars.next(), chars.next()), (Some(first), None) if ('0'..=up).contains(&first))
}

/// arclite's `variant`: a number (32 bits when it fits) or the text; empty for nothing.
fn variant(value: &str) -> Prop {
    if value.is_empty() {
        return Prop::Empty;
    }
    match value.parse::<u64>() {
        Ok(number) => u32::try_from(number).map_or(Prop::U64(number), Prop::U32),
        Err(_) => Prop::Text(value.to_string()),
    }
}

/// The advanced parameters as arclite reads them: a leading `-` drops the method's own ones (none
/// here), a second `-` the method itself; the level an `x=N` among them asks for.
struct Advanced {
    params: Vec<String>,
    ignore_method: bool,
    level: Option<u32>,
    have_digits: bool,
    have_qs: bool,
}

fn advanced(text: &str) -> Advanced {
    let (text, ignore_method) = match text.strip_prefix('-') {
        Some(rest) => match rest.strip_prefix('-') {
            Some(rest) => (rest, true),
            None => (rest, false),
        },
        None => (text, false),
    };
    let mut parsed = Advanced {
        params: Vec::new(),
        ignore_method,
        level: None,
        have_digits: false,
        have_qs: false,
    };
    for param in text.split(' ').map(written_out) {
        let sep = param.find('=');
        if param.is_empty() || sep == Some(0) {
            continue;
        }
        if let Some(sep) = sep {
            let (name, value) = (&param[..sep], &param[sep + 1..]);
            if name.eq_ignore_ascii_case("x") {
                if value.starts_with(|first: char| first.is_ascii_digit()) {
                    let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
                    parsed.level = digits.parse().ok();
                }
                continue;
            }
            parsed.have_digits |= name == "0" || name == "1";
        }
        parsed.have_qs |= param.starts_with("qs=");
        parsed.params.push(param);
    }
    parsed
}

/// The level written: the one chosen, or the advanced `x=N` brought into the method's range (0 a
/// multiple of its `mod0L`, none here).
fn level_of(chosen: u32, asked: Option<u32>) -> u32 {
    match asked {
        None => chosen,
        Some(0) => 0,
        Some(level) if level < MIN_LEVEL => MIN_LEVEL,
        Some(level) => level.min(MAX_LEVEL),
    }
}

/// arclite's `Archive::set_properties`: the parameters given to the library before it writes an archive
/// of `options`' format.
pub fn properties(options: &UpdateOptions, qs_by_default: bool) -> Vec<(String, Prop)> {
    let format = options.kind.format.as_str();
    let (is_7z, is_zip) = (format == "7z", format.eq_ignore_ascii_case("zip"));
    let parsed = advanced(&options.advanced);
    let mut params = parsed.params.clone();
    if !parsed.have_qs && qs_by_default && is_7z {
        params.push("qs=on".to_string());
    }
    let level = level_of(options.level, parsed.level);
    let mut named: Vec<(String, Prop)> = Vec::new();
    let mut methods = 0;
    if (is_7z || is_zip) && !parsed.ignore_method {
        named.push(("0".to_string(), Prop::Text(options.method.clone())));
        methods = 1;
    }
    if is_7z || is_zip || format != "bzip2" || level != 0 {
        named.push(("x".to_string(), Prop::U32(level)));
    }
    if is_7z && level != 0 {
        named.push(("s".to_string(), Prop::Bool(options.kind.solid)));
    }
    if is_7z
        && options.encrypt
        && let Some(header) = options.kind.encrypt_header
    {
        named.push(("he".to_string(), Prop::Bool(header)));
    }
    let shift = if parsed.have_digits { methods } else { 0 };
    for param in &params {
        merge(&mut named, param, (shift, methods));
    }
    normalize_digits(&mut named);
    for (_, value) in &mut named {
        if let Prop::Text(text) = value
            && let Some(number) = integer_of(text)
        {
            *value = number;
        }
    }
    named
}

/// One advanced parameter put among the named ones: in place of one of its name (a digit name of
/// the method's place shifted past the methods arclite named; a method given again by its digit when
/// its value starts the same), else after them.
fn merge(named: &mut Vec<(String, Prop)>, param: &str, (shift, methods): (usize, usize)) {
    let (name, value) = param.split_once('=').unwrap_or((param, ""));
    let mut name = name.to_string();
    if shift > 0 && is_digit(&name, '7') {
        let digit = name.as_bytes()[0] + u8::try_from(shift).unwrap_or(0);
        name = char::from(digit).to_string();
    }
    let upper = value.to_uppercase();
    let same_method = |at: usize, known: &str, known_value: &Prop| {
        at < methods
            && is_digit(known, '9')
            && is_digit(&name, '9')
            && (known_value.text())
                .is_some_and(|text| !text.is_empty() && upper.starts_with(&text.to_uppercase()))
    };
    let found = (named.iter().enumerate()).position(|(at, (known, known_value))| {
        known.eq_ignore_ascii_case(&name) || same_method(at, known, known_value)
    });
    match found {
        Some(at) => named[at].1 = variant(value),
        None => named.push((name, Prop::Text(value.to_string()))),
    }
}

/// arclite's renumbering of `{N}=` parameters from 0 with no gaps (`1 5 3` -> `0 2 1`).
fn normalize_digits(named: &mut [(String, Prop)]) {
    let mut used = [false; 10];
    for (name, _) in named.iter() {
        if is_digit(name, '9') {
            used[usize::from(name.as_bytes()[0] - b'0')] = true;
        }
    }
    let mut gaps = [0u8; 10];
    let mut gap = 0;
    for (digit, slot) in gaps.iter_mut().enumerate() {
        *slot = gap;
        if !used[digit] {
            gap += 1;
        }
    }
    for (name, _) in named.iter_mut() {
        if is_digit(name, '9') {
            let digit = name.as_bytes()[0];
            *name = char::from(digit - gaps[usize::from(digit - b'0')]).to_string();
        }
    }
}

/// arclite's last pass: a text that is a whole number given as one (32 bits when it fits).
fn integer_of(text: &str) -> Option<Prop> {
    if text.is_empty() {
        return None;
    }
    if let Ok(number) = text.parse::<u64>() {
        return Some(u32::try_from(number).map_or(Prop::U64(number), Prop::U32));
    }
    let number = text.parse::<i64>().ok()?;
    let unsigned = number.cast_unsigned();
    Some(u32::try_from(unsigned).map_or(Prop::U64(unsigned), Prop::U32))
}

#[cfg(test)]
#[path = "update_props_tests.rs"]
mod tests;
