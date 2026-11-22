//! A name from an archive as a file's name on a disk (arclite's `correct_filename` with its default mode
//! `0x12`: characters Windows refuses in names replaced by look-alikes, an empty name `_`), and arclite's
//! `auto_rename` (`name.0`, `name.1`, ... for one that exists).

use std::path::{Path, PathBuf};

/// The look-alikes of the control characters 1 to 31 (arclite's `control_chars`, the OEM glyphs).
const CONTROLS: [char; 31] = [
    '\u{263A}', '\u{263B}', '\u{2665}', '\u{2666}', '\u{2663}', '\u{2660}', '\u{2022}', '\u{25D8}',
    '\u{25CB}', '\u{25D9}', '\u{2642}', '\u{2640}', '\u{266A}', '\u{266B}', '\u{263C}', '\u{25BA}',
    '\u{25C4}', '\u{2195}', '\u{203C}', '\u{00B6}', '\u{00A7}', '\u{25AC}', '\u{21A8}', '\u{2191}',
    '\u{2193}', '\u{2192}', '\u{2190}', '\u{221F}', '\u{2194}', '\u{25B2}', '\u{25BC}',
];

/// `name` as a name on a disk (arclite's mode 2): `<` `>` as `«` `»`, `:` as `…`, `*` as `¤`, `?` as
/// `¿`, `|` as `¦`, `"` as `“` and `”` in turns, `/` as `⁄`, `\` as `¬`, a control character as its
/// glyph; an empty name `_` (mode `0x10`, the file's own name only: `empty`).
pub fn corrected(name: &str, empty: bool) -> String {
    let mut quote = false;
    let mut out: String = (name.chars())
        .map(|char| match char {
            '<' => '\u{00AB}',
            '>' => '\u{00BB}',
            ':' => '\u{2026}',
            '*' => '\u{00A4}',
            '?' => '\u{00BF}',
            '|' => '\u{00A6}',
            '"' => {
                quote = !quote;
                if quote { '\u{201C}' } else { '\u{201D}' }
            }
            '/' => '\u{2044}',
            '\\' => '\u{00AC}',
            '\u{1}'..='\u{1f}' => CONTROLS[usize::from(u8::try_from(char).unwrap_or(1)) - 1],
            other => other,
        })
        .collect();
    if out.is_empty() && empty {
        out.push('_');
    }
    out
}

/// arclite's `auto_rename`: `path` when nothing is there, else `path.0`, `path.1`, ... the first free.
pub fn auto_rename(path: &Path) -> PathBuf {
    if std::fs::symlink_metadata(path).is_err() {
        return path.to_path_buf();
    }
    let numbered = |number: u32| {
        let mut text = path.as_os_str().to_os_string();
        text.push(format!(".{number}"));
        PathBuf::from(text)
    };
    (0..)
        .map(numbered)
        .find(|candidate| std::fs::symlink_metadata(candidate).is_err())
        .unwrap_or_else(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_windows_refuses_get_look_alikes() {
        assert_eq!(corrected("a<b>c", true), "a\u{ab}b\u{bb}c");
        assert_eq!(corrected("x:y*z?|", true), "x\u{2026}y\u{a4}z\u{bf}\u{a6}");
        assert_eq!(corrected("\"a\"\"", true), "\u{201c}a\u{201d}\u{201c}");
        assert_eq!(corrected("a\u{1}b\u{1f}", true), "a\u{263a}b\u{25bc}");
        assert_eq!(corrected("", true), "_");
        assert_eq!(corrected("", false), "", "a folder on the way: as it is");
        assert_eq!(corrected("plain.txt", true), "plain.txt");
    }

    #[test]
    fn a_name_taken_gets_a_number() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("a.txt");
        assert_eq!(auto_rename(&path), path, "free");
        std::fs::write(&path, "x").unwrap();
        std::fs::write(temp.path().join("a.txt.0"), "x").unwrap();
        assert_eq!(auto_rename(&path), temp.path().join("a.txt.1"));
    }

    /// The slashes as their look-alikes, each control character its own glyph; a space, `~` and
    /// characters past ASCII kept; a name of only refused characters not empty.
    #[test]
    fn every_refused_character_has_its_look_alike() {
        assert_eq!(corrected("a/b\\c", true), "a\u{2044}b\u{ac}c");
        assert_eq!(corrected("\u{2}\u{1e}", true), "\u{263b}\u{25b2}");
        assert_eq!(corrected("\u{a}", true), "\u{25d9}");
        assert_eq!(corrected(" ~\u{7f}\u{e9}", true), " ~\u{7f}\u{e9}");
        assert_eq!(corrected("\u{0}", true), "\u{0}", "arclite's range from 1");
        assert_eq!(
            corrected("\"\"\"\"", false),
            "\u{201c}\u{201d}\u{201c}\u{201d}"
        );
        assert_eq!(corrected("?", false), "\u{bf}");
    }

    /// The first number tried is 0; a folder takes a name as a file does.
    #[test]
    fn numbers_start_at_zero_and_a_folder_takes_a_name() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("data");
        std::fs::create_dir(&folder).unwrap();
        assert_eq!(auto_rename(&folder), temp.path().join("data.0"));
        std::fs::create_dir(temp.path().join("data.0")).unwrap();
        std::fs::create_dir(temp.path().join("data.1")).unwrap();
        assert_eq!(auto_rename(&folder), temp.path().join("data.2"));
    }
}
