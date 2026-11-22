//! The names' case as FAR's filecase changes it (`FileMix.cpp`, `ProcessName.cpp`): a name split at its
//! last dot into the name and the extension, each in its own case; a part in mixed case left as it is
//! when asked; `.` and `..` never. A letter whose other case is more than one letter (`ß`) stays as it is,
//! as Windows' `CharUpper` keeps it.

/// A part's case (FAR's `MODE_*`, in the dialog's order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Lower,
    Upper,
    /// The first letter a capital, the rest small (`MODE_N_WORD`).
    FirstCapital,
    /// Each word's first letter a capital, words split at the delimiters (`MODE_LN_WORD`).
    Title,
    None,
}

impl Mode {
    /// The dialog's radio button `index` (FAR's setting's number).
    pub fn of(index: i64) -> Self {
        match index {
            1 => Self::Upper,
            2 => Self::FirstCapital,
            3 => Self::Title,
            4 => Self::None,
            _ => Self::Lower,
        }
    }

    pub fn index(self) -> i64 {
        match self {
            Self::Lower => 0,
            Self::Upper => 1,
            Self::FirstCapital => 2,
            Self::Title => 3,
            Self::None => 4,
        }
    }
}

/// What a conversion does (FAR's `Options`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    pub name: Mode,
    pub extension: Mode,
    /// A part in mixed case is left (`SkipMixedCase`).
    pub skip_mixed: bool,
    /// Folders' names too (`ProcessDir`).
    pub folder_names: bool,
    /// What splits words for the title case (`WordDiv`).
    pub delimiters: String,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            name: Mode::Lower,
            extension: Mode::Lower,
            skip_mixed: true,
            folder_names: false,
            delimiters: DELIMITERS.to_string(),
        }
    }
}

/// FAR's word delimiters (the dialog's "Reset"): a space and an underscore.
pub const DELIMITERS: &str = " _";

fn upper(letter: char) -> char {
    single(letter.to_uppercase()).unwrap_or(letter)
}

fn lower(letter: char) -> char {
    single(letter.to_lowercase()).unwrap_or(letter)
}

fn single(mut letters: impl Iterator<Item = char>) -> Option<char> {
    match (letters.next(), letters.next()) {
        (Some(letter), None) => Some(letter),
        _ => None,
    }
}

/// Whether `part` has letters of both cases (FAR's `IsCaseMixed`: each letter against the first one).
pub fn is_mixed(part: &str) -> bool {
    let mut letters = part.chars().filter(|letter| letter.is_alphabetic());
    let Some(first) = letters.next() else {
        return false;
    };
    letters.any(|letter| letter.is_lowercase() != first.is_lowercase())
}

/// `part` in the case `mode` (FAR's `CaseWord`).
pub fn in_case(part: &str, mode: Mode, delimiters: &str) -> String {
    match mode {
        Mode::Lower => part.chars().map(lower).collect(),
        Mode::Upper => part.chars().map(upper).collect(),
        Mode::FirstCapital => (part.chars().enumerate())
            .map(|(at, letter)| {
                if at == 0 {
                    upper(letter)
                } else {
                    lower(letter)
                }
            })
            .collect(),
        Mode::Title => {
            let mut done = String::with_capacity(part.len());
            let mut before: Option<char> = None;
            for letter in part.chars() {
                let starts = before.is_none_or(|before| delimiters.contains(before));
                let letter = if starts { upper(letter) } else { lower(letter) };
                done.push(letter);
                before = Some(letter);
            }
            done
        }
        Mode::None => part.to_string(),
    }
}

/// The new name of the file or folder (`folder`) named `name`; none when it stays as it is.
pub fn new_name(name: &str, folder: bool, rules: &Rules) -> Option<String> {
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    if extension.is_empty() && matches!(stem, "" | "." | "..") {
        return None;
    }
    let takes =
        |mode: Mode, part: &str| mode != Mode::None && !(rules.skip_mixed && is_mixed(part));
    let (stem_takes, extension_takes) =
        (takes(rules.name, stem), takes(rules.extension, extension));
    if !(stem_takes || extension_takes) || (folder && !rules.folder_names) {
        return None;
    }
    let part = |takes: bool, part: &str, mode: Mode| match takes {
        true => in_case(part, mode, &rules.delimiters),
        false => part.to_string(),
    };
    let stem = part(stem_takes, stem, rules.name);
    let extension = part(extension_takes, extension, rules.extension);
    let new = match extension.is_empty() {
        true => stem,
        false => format!("{stem}.{extension}"),
    };
    (new != name).then_some(new)
}

#[cfg(test)]
#[path = "case_tests.rs"]
mod tests;
