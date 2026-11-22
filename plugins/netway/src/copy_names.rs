//! WinSCP's names of a transfer (`CopyParam.cpp` `ChangeFileName`, `FileMasks.cpp` `MaskFileName`,
//! `Common.cpp` `ValidLocalFileName`, `RestoreChars`, `IsReservedName`): the copy dialog's file mask on
//! the first level's names; the characters a Windows name cannot hold written `%XX` coming down and read
//! back going up, with WinSCP's trailing space or dot and reserved names (NetBox leaves those two out,
//! which loses such files on Windows).

/// WinSCP's `LocalInvalidChars`.
const LOCAL_INVALID: &str = "/\\:*?\"<>|";
/// WinSCP's `TokenPrefix`: the invalid characters and itself are its tokens.
const TOKEN: char = '%';
/// WinSCP's `AnyMask`.
const ANY_MASK: &str = "*.*";
/// Windows' reserved names (WinSCP's `IsReservedName`).
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// WinSCP's `IsEffectiveFileNameMask`: a mask that changes names (none, `*` and `*.*` do not).
pub fn is_effective(mask: &str) -> bool {
    !mask.is_empty() && mask != "*" && mask != ANY_MASK
}

/// WinSCP's `MaskFileName`: `mask` (the copy dialog's text after its folder) applied to `name`; the parts
/// after the last dots are the extensions (a leading dot of a name is no extension's).
pub fn mask_file_name(name: &str, mask: &str) -> String {
    if !is_effective(mask) {
        return name.to_string();
    }
    let Some(dot) = mask.rfind('.') else {
        return mask_part(name, mask);
    };
    let (base, extension) = match name.rfind('.') {
        Some(at) if at > 0 => (&name[..at], &name[at + 1..]),
        _ => (name, ""),
    };
    let extension = mask_part(extension, &mask[dot + 1..]);
    let base = mask_part(base, &mask[..dot]);
    match extension.is_empty() {
        true => base,
        false => format!("{base}.{extension}"),
    }
}

/// WinSCP's `MaskFilePart`: `*` takes the rest of `part`, `?` its next character, any other character of
/// the mask is written and passes one of the part's; `\` makes the next mask character a plain one.
fn mask_part(part: &str, mask: &str) -> String {
    let chars: Vec<char> = part.chars().collect();
    let mut result = String::new();
    let mut rest = 0;
    let mut escaped = false;
    for char in mask.chars() {
        match (char, escaped) {
            ('\\', false) => escaped = true,
            ('*', false) => {
                result.extend(chars.iter().skip(rest));
                rest = chars.len();
            }
            ('?', false) => {
                if let Some(one) = chars.get(rest) {
                    result.push(*one);
                    rest += 1;
                }
            }
            _ => {
                result.push(char);
                rest += 1;
                escaped = false;
            }
        }
    }
    result
}

/// WinSCP's `IsReservedName`: `CON`, `NUL`, `COM1` ... with or without an extension, in any case.
pub fn is_reserved(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or_default();
    RESERVED.iter().any(|one| one.eq_ignore_ascii_case(base))
}

/// A token's character: `%` and two hex digits.
fn token_at(chars: &[char], at: usize) -> Option<char> {
    let hex: String = chars.get(at + 1..at + 3)?.iter().collect();
    let byte = u8::from_str_radix(&hex, 16).ok()?;
    Some(char::from(byte))
}

fn is_token_char(char: char) -> bool {
    LOCAL_INVALID.contains(char) || char == TOKEN
}

fn token(char: char) -> String {
    format!("{TOKEN}{:02X}", u32::from(char))
}

/// WinSCP's `ValidLocalFileName` with its token replacement: a remote name as a Windows file's, each
/// character Windows refuses as `%XX`, a `%` that would read as a token as `%25`, a trailing space or dot
/// as its token, a reserved name with `%00` before its extension.
pub fn valid_local_file_name(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut result = String::new();
    for (at, char) in chars.iter().enumerate() {
        let tokened = match *char {
            TOKEN => token_at(&chars, at).is_some_and(|one| one != '\0' && is_token_char(one)),
            other => LOCAL_INVALID.contains(other),
        };
        let last = at + 1 == chars.len();
        if tokened || (last && matches!(char, ' ' | '.')) {
            result.push_str(&token(*char));
        } else {
            result.push(*char);
        }
    }
    if is_reserved(&result) {
        let at = result.find('.').unwrap_or(result.len());
        result.insert_str(at, "%00");
    }
    result
}

/// A remote name that, written as it is, would not be one item of its folder on the disk: "." or "..",
/// a drive's or a folder's separator in it.
pub fn walks_out(name: &str) -> bool {
    matches!(name.trim_end_matches([' ', '.']), "") || name.contains(['\\', '/', ':', '\0'])
}

/// WinSCP's `RestoreChars`: a Windows file's name as the remote one, its tokens read back (a token of `/`
/// stays, no remote name holds one); `%00` of a reserved name dropped.
pub fn restore_chars(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut result = String::new();
    let mut at = 0;
    while at < chars.len() {
        let char = chars[at];
        let Some(decoded) = (char == TOKEN).then(|| token_at(&chars, at)).flatten() else {
            result.push(char);
            at += 1;
            continue;
        };
        let at_end = at + 3 == chars.len();
        let readable =
            is_token_char(decoded) || (at_end && (decoded == ' ' || (decoded == '.' && at > 0)));
        if decoded != '\0' && decoded != '/' && readable {
            result.push(decoded);
            at += 3;
        } else if decoded == '\0'
            && (at_end || chars.get(at + 3) == Some(&'.'))
            && reserved_without(&chars, at)
        {
            at += 3;
        } else {
            result.push(char);
            at += 1;
        }
    }
    result
}

/// The name without the token at `at` is a reserved one.
fn reserved_without(chars: &[char], at: usize) -> bool {
    let without: String = chars[..at].iter().chain(&chars[at + 3..]).collect();
    is_reserved(&without)
}

/// A target typed in the copy dialog split as WinSCP's `ExtractFilePath` (`remote`: `UnixExtractFilePath`)
/// and the name after it, the file mask: `C:\out\*.bak` as `C:\out\` and `*.bak`.
pub fn split_target(text: &str, remote: bool) -> (String, String) {
    let separators: &[char] = if remote { &['/'] } else { &['\\', '/', ':'] };
    match text.rfind(separators) {
        Some(at) => (text[..=at].to_string(), text[at + 1..].to_string()),
        None => (String::new(), text.to_string()),
    }
}

#[cfg(test)]
#[path = "copy_names_tests.rs"]
mod tests;
