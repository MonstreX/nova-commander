//! The keys FAR's menus share (`VMenu` with `VMENU_WRAPMODE`): Up and Down go round the ends over the
//! separators, Home and End to the first and last item, PgUp and PgDn by the lines shown. Hotkeys are
//! marked with `&`; with `VMENU_AUTOHIGHLIGHT` FAR gives the items without one a hotkey of their own.

use ratatui::crossterm::event::KeyCode;

/// `&` written so that the menu shows it as itself (FAR's `escape_ampersands`).
pub fn escape(text: &str) -> String {
    text.replace('&', "&&")
}

/// The hotkey `label` marks with `&` (`&&` is an `&` shown, `&&&` the hotkey `&`), lower case.
pub fn marked_hotkey(label: &str) -> Option<char> {
    let at = mark_at(label)?;
    label[at + 1..].chars().next().map(lower)
}

/// Where the `&` that marks the hotkey stands: FAR's `HiTextBase` - the first run of `&` of odd length,
/// a character after it (a run of even length shows half as many `&`).
fn mark_at(label: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(found) = label[from..].find('&') {
        let begin = from + found;
        let end = label[begin..]
            .find(|ch| ch != '&')
            .map_or(label.len(), |run| begin + run);
        if (end - begin) % 2 == 1 {
            return (begin + 1 < label.len()).then_some(begin);
        }
        from = end;
    }
    None
}

/// FAR's `unescape`: `&&` an `&`, a lone `&` dropped.
fn unescape(text: &str) -> impl Iterator<Item = char> + '_ {
    let mut last_ampersand = false;
    text.chars().filter(move |ch| {
        let shown = *ch != '&' || last_ampersand;
        last_ampersand = *ch == '&' && !last_ampersand;
        shown
    })
}

/// The characters a menu's `label` shows, each with whether it is the hotkey (FAR's `HiTextBase`): the
/// text before the mark and after the hotkey unescaped; `&&&` marks an `&`, the run's last `&` dropped.
pub fn shown_chars(label: &str) -> Vec<(char, bool)> {
    let Some(at) = mark_at(label) else {
        return unescape(label).map(|ch| (ch, false)).collect();
    };
    let mut shown: Vec<(char, bool)> = unescape(&label[..at]).map(|ch| (ch, false)).collect();
    let mut rest = label[at + 1..].chars();
    if let Some(hotkey) = rest.next() {
        shown.push((hotkey, true));
    }
    let tail = rest.as_str();
    let tail = if label[at..].starts_with("&&&") {
        tail.strip_prefix('&').unwrap_or(tail)
    } else {
        tail
    };
    shown.extend(unescape(tail).map(|ch| (ch, false)));
    shown
}

/// FAR's `AddHotkey`: "&k  text" (`&&&` for the hotkey `&`), or the text where the others' are.
pub fn with_hotkey(label: &str, hotkey: Option<char>) -> String {
    match hotkey {
        Some('&') => format!("&&&  {label}"),
        Some(key) => format!("&{key}  {label}"),
        None => format!("   {label}"),
    }
}

/// A key as the menus compare hotkeys: lower case.
pub fn lower(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

/// FAR's `AssignHighlights`: a hotkey marked again in a later label is taken from it; a label without
/// a hotkey is given its first letter or digit that no other label has as its hotkey yet, marked with
/// `&`.
pub fn auto_hotkeys(labels: &mut [String]) {
    let mut used: Vec<char> = Vec::new();
    for label in labels.iter_mut() {
        let (Some(at), Some(key)) = (mark_at(label), marked_hotkey(label)) else {
            continue;
        };
        if used.contains(&key) {
            label.remove(at);
        } else {
            used.push(key);
        }
    }
    for label in labels.iter_mut() {
        if marked_hotkey(label).is_some() {
            continue;
        }
        let mut chars = label.char_indices().peekable();
        let mut found = None;
        while let Some((at, ch)) = chars.next() {
            if ch == '&' {
                // A run of `&` shown; the character right after it cannot be marked with `&` (the run
                // would grow odd: `&&&` is the hotkey `&`).
                while chars.next_if(|(_, next)| *next == '&').is_some() {}
                chars.next();
                continue;
            }
            if ch.is_alphanumeric() && !used.contains(&lower(ch)) {
                used.push(lower(ch));
                found = Some(at);
                break;
            }
        }
        if let Some(at) = found {
            label.insert(at, '&');
        }
    }
}

/// The item at `at` or the nearest after it, else before it; 0 with none (`skip` marks separators).
pub fn first_from(skip: &[bool], at: usize) -> usize {
    let after = (at..skip.len()).find(|&line| !skip[line]);
    let before = (0..at.min(skip.len())).rev().find(|&line| !skip[line]);
    after.or(before).unwrap_or(0)
}

/// The item at `at` or the nearest before it, else after it.
pub fn last_from(skip: &[bool], at: usize) -> usize {
    let before = (0..=at.min(skip.len().saturating_sub(1)))
        .rev()
        .find(|&line| !skip.get(line).copied().unwrap_or(true));
    before.unwrap_or_else(|| first_from(skip, at))
}

/// Where `key` takes the cursor from `selected`; other keys leave it.
pub fn step(skip: &[bool], selected: usize, key: KeyCode, rows: usize) -> usize {
    step_in(skip, selected, key, (rows, true))
}

/// [`step`] with `wrap` (FAR's `VMENU_WRAPMODE`) or not: without it Up on the first item and Down on the
/// last stay.
pub fn step_in(skip: &[bool], selected: usize, key: KeyCode, (rows, wrap): (usize, bool)) -> usize {
    let count = skip.len();
    if count == 0 {
        return 0;
    }
    let page = rows.max(2) - 1;
    match key {
        KeyCode::Up | KeyCode::Down => {
            for offset in 1..=count {
                let down = key == KeyCode::Down;
                if !wrap
                    && (if down {
                        selected + offset >= count
                    } else {
                        offset > selected
                    })
                {
                    break;
                }
                let at = if down {
                    (selected + offset) % count
                } else {
                    (selected + count * 2 - offset) % count
                };
                if !skip[at] {
                    return at;
                }
            }
            selected
        }
        KeyCode::Home => first_from(skip, 0),
        KeyCode::End => last_from(skip, count - 1),
        KeyCode::PageUp => first_from(skip, selected.saturating_sub(page)),
        KeyCode::PageDown => last_from(skip, (selected + page).min(count - 1)),
        _ => selected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES: [bool; 6] = [true, false, false, true, false, false];

    #[test]
    fn up_and_down_go_round_over_separators() {
        assert_eq!(step(&LINES, 2, KeyCode::Down, 3), 4);
        assert_eq!(step(&LINES, 5, KeyCode::Down, 3), 1);
        assert_eq!(step(&LINES, 1, KeyCode::Up, 3), 5);
        assert_eq!(step(&[], 0, KeyCode::Down, 3), 0);
    }

    #[test]
    fn hotkeys_are_marked_or_given_the_first_letter_no_other_has() {
        assert_eq!(marked_hotkey("Tom && &Jerry"), Some('j'));
        assert_eq!(marked_hotkey("Tom && Jerry"), None);
        let mut twice = ["&Work".to_string(), "&Web".to_string()];
        auto_hotkeys(&mut twice);
        assert_eq!(twice, ["&Work".to_string(), "W&eb".to_string()]);
        let mut labels = [
            r"C:\Users".to_string(),
            r"C:\Windows".to_string(),
            "&Users".to_string(),
            String::new(),
            "&&1".to_string(),
            "&&12".to_string(),
            "&&&&ab".to_string(),
        ];
        auto_hotkeys(&mut labels);
        assert_eq!(
            labels,
            [
                r"&C:\Users".to_string(),
                r"C:\&Windows".to_string(),
                "&Users".to_string(),
                String::new(),
                "&&1".to_string(),
                "&&1&2".to_string(),
                "&&&&a&b".to_string(),
            ]
        );
    }

    /// FAR's own cases (`interf.cpp`'s tests of `HiTextHotkey` and `HiText2Str`).
    #[test]
    fn ampersands_are_read_as_fars() {
        let cases = [
            ("&", "", None),
            ("1&2", "12", Some('2')),
            ("1&", "1", None),
            ("&1&2", "12", Some('1')),
            ("&&", "&", None),
            ("1&&2", "1&2", None),
            ("&&&", "&", Some('&')),
            ("&&&1", "&1", Some('&')),
            ("1&&&2", "1&2", Some('&')),
            ("&1&&&2", "1&2", Some('1')),
            ("&1&2&3&", "123", Some('1')),
            ("&&1&2&", "&12", Some('2')),
            ("123&&", "123&", None),
            ("&&&&&", "&&", Some('&')),
        ];
        for (label, text, hotkey) in cases {
            let shown = shown_chars(label);
            let visible: String = shown.iter().map(|(ch, _)| *ch).collect();
            assert_eq!(visible, text, "{label}");
            assert_eq!(marked_hotkey(label), hotkey, "{label}");
            let marked: Vec<char> = (shown.iter().filter(|(_, marked)| *marked))
                .map(|(ch, _)| *ch)
                .collect();
            assert_eq!(marked, hotkey.into_iter().collect::<Vec<_>>(), "{label}");
        }
    }

    #[test]
    fn home_end_and_pages_stop_on_items() {
        assert_eq!(step(&LINES, 4, KeyCode::Home, 3), 1);
        assert_eq!(step(&LINES, 1, KeyCode::End, 3), 5);
        assert_eq!(step(&LINES, 1, KeyCode::PageDown, 3), 2);
        assert_eq!(step(&LINES, 5, KeyCode::PageUp, 3), 4);
        assert_eq!(
            step(&LINES, 4, KeyCode::PageUp, 4),
            1,
            "not onto the separator"
        );
        assert_eq!(first_from(&[true, true], 0), 0);
    }
}
