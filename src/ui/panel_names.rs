//! A name in a panel column as FAR's `ShowList` and `ConvertName` draw it (`far/filelist.cpp`), the
//! extension column, and the cases a mode asks for.

//! Widths are the screen's cells (R7 review E14: a wide character two).

use super::columns::Column;
use super::panel_cells::{Cell, LINK_ARROW};
use crate::app::text_cut::{cells, head, left_aligned, right_aligned, tail};
use crate::vfs::Entry;

/// FAR's `name_ext`: the name and its extension from the last dot (a name all extension has none).
pub(super) fn split_extension(name: &str) -> (&str, &str) {
    crate::vfs::split_extension(name)
}

/// FAR's `X` column: a file's extension without its dot, to the left, to the right with `R`.
pub(super) fn extension_cell(entry: &Entry, column: &Column) -> Cell {
    let extension = if entry.is_dir {
        ""
    } else {
        split_extension(&entry.name).1.trim_start_matches('.')
    };
    let width = column.width;
    let text = if column.flags.right_align {
        crate::app::text_cut::fit_to_right(extension, width)
    } else {
        crate::app::text_cut::fit_to_left(extension, width)
    };
    Cell {
        text,
        too_long: cells(extension) > width,
    }
}

/// FAR's `IsCaseMixed`: small and capital letters both after the first letter.
fn case_mixed(name: &str) -> bool {
    let mut letters = name.chars().filter(|c| c.is_alphabetic());
    let Some(first) = letters.next() else {
        return false;
    };
    let lower = first.is_lowercase();
    letters.any(|c| c.is_lowercase() != lower)
}

/// One character for one, as FAR's `CharUpperBuff` changes them (a letter that would become two stays).
fn each_char(text: &str, upper: bool) -> String {
    text.chars()
        .map(|c| {
            let mut changed: Vec<char> = if upper {
                c.to_uppercase().collect()
            } else {
                c.to_lowercase().collect()
            };
            if changed.len() == 1 {
                changed.remove(0)
            } else {
                c
            }
        })
        .collect()
}

/// The cases of a mode (FAR's `PVS_FILEUPPERTOLOWERCASE`, `PVS_FOLDERUPPERCASE`, `PVS_FILELOWERCASE`)
/// over a name's text; `name` is the name as shown (without its extension with `N`, FAR's `NameCopy`).
pub(super) fn cased(
    text: String,
    name: &str,
    is_dir: bool,
    look: &super::panel_cells::CellLook,
) -> String {
    let mut text = text;
    if look.upper_to_lower_case && !is_dir && !case_mixed(name) {
        text = each_char(&text, false);
    }
    if look.folder_upper_case && is_dir {
        text = each_char(&text, true);
    }
    if look.files_lower_case && !is_dir {
        text = each_char(&text, false);
    }
    text
}

/// The status line spells a link out: `name → target`. When that is too long the name stays whole and
/// the target keeps its end (`name → …\links\target`); a name too long for even that keeps its end.
pub(super) fn link_status_name(name: &str, target: &str, width: usize) -> String {
    let full = format!("{name}{LINK_ARROW} {target}");
    if cells(&full) <= width {
        return left_aligned(&full, width);
    }
    let start = format!("{name}{LINK_ARROW} …");
    let room = width.checked_sub(cells(&start)).filter(|room| *room > 0);
    match room {
        Some(room) => format!("{start}{}", tail(target, room)),
        None => tail(&full, width).to_string(),
    }
}

/// FAR's name in a column: the selection's mark (`M`, none on an unselected name with `D`), the name
/// with its link arrow cut to the column, kept to the right when long with `R` and always with `F`,
/// its extension aligned with `align`.
pub(super) fn name_cell(
    name: &str,
    suffix: &str,
    column: &Column,
    selected: bool,
    align: bool,
) -> Cell {
    let mut width = column.width;
    let mut text = String::new();
    if column.flags.mark && width > 2 {
        let mark = match (selected, column.flags.mark_dynamic) {
            (true, _) => "√ ",
            (false, true) => "",
            (false, false) => "  ",
        };
        text.push_str(mark);
        width -= mark.chars().count();
    }
    // A link's arrow stays when its name is cut (and goes only when nothing else would fit).
    let suffix_length = cells(suffix);
    let (name_width, suffix) = if suffix_length < width {
        (width - suffix_length, suffix)
    } else {
        (width, "")
    };
    let length = cells(name);
    let force = column.flags.right_align_force;
    if force && length <= name_width {
        text.push_str(&right_aligned(&format!("{name}{suffix}"), width));
        return Cell::plain(text);
    }
    if (column.flags.right_align || force) && length > name_width {
        let end = tail(name, name_width);
        text.push_str(&" ".repeat(name_width - cells(end)));
        text.push_str(end);
        text.push_str(suffix);
        return Cell {
            text,
            too_long: true,
        };
    }
    let aligned = (align && suffix.is_empty())
        .then(|| aligned_name(name, width))
        .flatten();
    text.push_str(
        &aligned
            .unwrap_or_else(|| left_aligned(&format!("{}{suffix}", head(name, name_width)), width)),
    );
    Cell {
        text,
        too_long: length > name_width,
    }
}

/// FAR's `PVS_ALIGNEXTENSIONS` (`FileList::ConvertName`): the extension (without the dot) starts in a
/// column three characters from the right edge; `None` when the name does not qualify.
pub(super) fn aligned_name(name: &str, width: usize) -> Option<String> {
    let length = cells(name);
    // FAR's `ConvertName` takes `name_ext`: a dot in a folder of a whole path is no extension's.
    let (stem, dotted) = split_extension(name);
    let dot = stem.len();
    let extension = dotted.strip_prefix('.')?;
    let qualifies = length <= width
        && !extension.is_empty()
        && dot > 0
        && !(length <= 2 && name.starts_with('.'))
        && !extension.contains(' ');
    if !qualifies {
        return None;
    }
    // The dot is dropped unless the stem ends with a space.
    let stem = &name[..dot];
    let stem = if stem.ends_with(' ') {
        &name[..=dot]
    } else {
        stem
    };
    let stem_length = cells(stem);
    let extension_length = cells(extension);
    let aligned_extension = extension_length.max(3);
    let mut between = (width).saturating_sub(stem_length + aligned_extension);
    if stem_length + aligned_extension > width {
        between = 0;
    }
    if between == 0 && stem_length + extension_length <= width {
        between = width - stem_length - extension_length;
    }
    let after = width.saturating_sub(stem_length + between + extension_length);
    Some(format!(
        "{stem}{}{extension}{}",
        " ".repeat(between),
        " ".repeat(after)
    ))
}

/// A path's last name, a folder's without its ending separator (FAR's `PointToFolderNameIfFolder`): a
/// name column's `O` on a provider's whole paths.
pub(super) fn last_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches(std::path::is_separator);
    trimmed
        .rsplit(std::path::is_separator)
        .next()
        .unwrap_or(trimmed)
}

#[cfg(test)]
#[path = "panel_names_tests.rs"]
mod tests;
