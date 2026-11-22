//! Text of one panel cell, as FAR's `FileList::ShowList` and `FormatStr_*` (`far/panelmix.cpp`)
//! produce it.

use std::time::SystemTime;

use super::columns::{Column, ColumnKind};
use super::format;
use super::panel_names::{
    cased, extension_cell, last_name, link_status_name, name_cell, split_extension,
};
use super::size_format::{SizeStyle, format_size};
use crate::platform;
use crate::platform::files::LinkKind;
use crate::vfs::{Entry, FolderSize};

/// Labels in the size column of folders and links (`MListUp`, `MListFolder`, `MListJunction`,
/// `MListSymlink`, `MListUnknownReparsePoint`; `MListBrackets` around them when they fit).
const UP_LABEL: &str = "Up";
const FOLDER_LABEL: &str = "Folder";
const JUNCTION_LABEL: &str = "Junction";
const SYMLINK_LABEL: &str = "Symlink";
const UNKNOWN_LINK_LABEL: &str = "?Symlink?";
/// After a link's name (not in FAR, which can only put a mark before the name): ` →`.
pub(super) const LINK_ARROW: &str = " →";
const LABEL_WIDTH: usize = 6;

/// A rendered cell; `too_long` asks the caller to draw FAR's `}` marker after a cut name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub too_long: bool,
}

impl Cell {
    pub(super) fn plain(text: String) -> Self {
        Self {
            text,
            too_long: false,
        }
    }
}

/// What a panel's mode adds to its cells (FAR's `PVS_*` flags) and the time a brief date is recent by.
#[derive(Debug, Clone, Copy)]
pub struct CellLook {
    pub align_extensions: bool,
    pub folder_align_extensions: bool,
    pub folder_upper_case: bool,
    pub files_lower_case: bool,
    pub upper_to_lower_case: bool,
    pub now: SystemTime,
}

impl CellLook {
    pub fn of(mode: &crate::app::ModeSpec, now: SystemTime) -> Self {
        Self {
            align_extensions: mode.align_extensions,
            folder_align_extensions: mode.folder_align_extensions,
            folder_upper_case: mode.folder_upper_case,
            files_lower_case: mode.files_lower_case,
            upper_to_lower_case: mode.upper_to_lower_case,
            now,
        }
    }

    fn plain(align_extensions: bool) -> Self {
        Self {
            align_extensions,
            folder_align_extensions: false,
            folder_upper_case: false,
            files_lower_case: false,
            upper_to_lower_case: false,
            now: SystemTime::now(),
        }
    }
}

/// The cell of `entry` in `column`. `status` is the status line (no extension alignment, long names keep
/// their end when the column asks for it).
pub fn cell(
    entry: &Entry,
    column: &Column,
    selected: bool,
    align_extensions: bool,
    status: bool,
) -> Cell {
    cell_in(
        entry,
        column,
        selected,
        &CellLook::plain(align_extensions),
        status,
    )
}

/// FAR's name of an entry in a name column: without its extension with `N`, its extension aligned
/// and its cases as the mode says (not in the status line).
fn name_of(entry: &Entry, column: &Column, selected: bool, look: &CellLook, status: bool) -> Cell {
    // `O`: the last name of a path a provider gives (FAR's `PointToFolderNameIfFolder`); a disk's names
    // are that already.
    let full = match column.flags.name_only {
        true => last_name(&entry.name),
        false => &entry.name,
    };
    let name = if column.flags.no_extension && !entry.is_dir {
        split_extension(full).0
    } else {
        full
    };
    if status {
        return match &entry.link_target {
            Some(target) => Cell::plain(link_status_name(name, target, column.width)),
            None => name_cell(name, "", column, selected, false),
        };
    }
    let arrow = if entry.points_elsewhere() {
        LINK_ARROW
    } else {
        ""
    };
    let align = if entry.is_dir {
        look.folder_align_extensions
    } else {
        look.align_extensions
    };
    let shown = name_cell(name, arrow, column, selected, align);
    Cell {
        text: cased(shown.text, name, entry.is_dir, look),
        ..shown
    }
}

/// [`cell`] in a mode's look.
pub fn cell_in(
    entry: &Entry,
    column: &Column,
    selected: bool,
    look: &CellLook,
    status: bool,
) -> Cell {
    let width = column.width;
    match column.kind {
        ColumnKind::Name => name_of(entry, column, selected, look, status),
        ColumnKind::Extension => extension_cell(entry, column),
        ColumnKind::Size => Cell::plain(match entry.folder_size {
            FolderSize::Unknown => size_text(entry, entry.size, column),
            known => counted_text(entry.size, known, column),
        }),
        ColumnKind::Allocated => Cell::plain(match entry.allocated {
            Some(allocated) => size_text(entry, allocated, column),
            None if entry.is_dir => size_text(entry, 0, column),
            None => " ".repeat(column.width),
        }),
        ColumnKind::Date => {
            let date = time_part(entry.modified, width > 9, 5).0;
            Cell::plain(crate::app::text_cut::fit_to_right(&date, width))
        }
        ColumnKind::Time => {
            let time = time_part(entry.modified, false, width).1;
            Cell::plain(crate::app::text_cut::fit_to_right(&time, width))
        }
        ColumnKind::WriteTime => Cell::plain(date_and_time(entry.modified, column, look.now)),
        ColumnKind::CreationTime => Cell::plain(date_and_time(entry.created, column, look.now)),
        ColumnKind::AccessTime => Cell::plain(date_and_time(entry.accessed, column, look.now)),
        ColumnKind::ChangeTime => Cell::plain(date_and_time(entry.changed, column, look.now)),
        ColumnKind::Attributes => Cell::plain(format::fit(&entry.attributes.letters(width), width)),
        ColumnKind::Custom(number) => {
            let texts = entry.texts.as_deref();
            let text = texts.and_then(|texts| texts.columns.get(usize::from(number)));
            Cell::plain(clipped(text.map_or("", String::as_str), width))
        }
        ColumnKind::Description => {
            let text = (entry.texts.as_deref()).map_or("", |texts| texts.description.as_str());
            Cell::plain(clipped(text, width))
        }
    }
}

/// A provider's text cut to the column with no mark (FAR's `fmt::Clip`), its control characters spaces.
fn clipped(text: &str, width: usize) -> String {
    let cut: String = (text.chars())
        .map(|letter| if letter.is_control() { ' ' } else { letter })
        .take(width)
        .collect();
    format!("{cut:<width$}")
}

/// The status line of a link: its size column grows to the whole label (`<Junction>`; FAR would cut it to
/// the column's six characters), and when `name → target` still does not fit, the date and time give
/// their room to it.
pub fn fit_status_columns(entry: &Entry, columns: &mut Vec<Column>) {
    if !entry.points_elsewhere() {
        return;
    }
    let Some(name) = columns
        .iter()
        .position(|column| column.kind == ColumnKind::Name)
    else {
        return;
    };
    if let Some(size) = columns
        .iter()
        .position(|column| column.kind == ColumnKind::Size)
        && let Some(kind) = entry.link
    {
        let label = link_label(kind).chars().count() + 2;
        let extra = label.saturating_sub(columns[size].width);
        if columns[name].width >= extra {
            columns[name].width -= extra;
            columns[size].width += extra;
        }
    }
    // Only a target that could be read is worth the date's room.
    let Some(target) = &entry.link_target else {
        return;
    };
    let wanted = format!("{}{LINK_ARROW} {target}", entry.name)
        .chars()
        .count();
    if columns[name].width >= wanted {
        return;
    }
    let is_time = |column: &Column| matches!(column.kind, ColumnKind::Date | ColumnKind::Time);
    // Each dropped column frees its width and the space before it.
    let freed: usize = columns
        .iter()
        .filter(|column| is_time(column))
        .map(|column| column.width + 1)
        .sum();
    columns.retain(|column| !is_time(column));
    if let Some(name) = columns
        .iter_mut()
        .find(|column| column.kind == ColumnKind::Name)
    {
        name.width += freed;
    }
}

/// FAR's size modifiers of a column: `C`, `E`, `F`, `T`.
fn size_style(column: &Column) -> SizeStyle {
    let flags = column.flags;
    SizeStyle {
        group_digits: flags.group_digits,
        float: flags.float,
        compact: flags.economic,
        thousand: flags.thousand,
        ..SizeStyle::default()
    }
}

/// A folder's size from F3: `~` when it was counted before the last reading (FAR), `…` while it is
/// being counted (owner: a total still growing).
fn counted_text(size: u64, state: FolderSize, column: &Column) -> String {
    let (before, after) = match state {
        FolderSize::Outdated => ("~", ""),
        FolderSize::Counting => ("", "…"),
        FolderSize::Unknown | FolderSize::Precise => ("", ""),
    };
    let width = column
        .width
        .saturating_sub(before.len() + after.chars().count());
    let text = format_size(
        size,
        width,
        false,
        size_style(column),
        platform::locale::number_format(),
    );
    format!("{before}{text}{after}")
}

/// FAR's `FormatStr_Size`: `<Folder>` / `<  Up  >` for directories (without brackets when narrow),
/// otherwise the size fitted to the column.
fn size_text(entry: &Entry, size: u64, column: &Column) -> String {
    let width = column.width;
    // A file that is a link has no size of its own to show (a deduplicated file still has one).
    let has_size = !entry.is_dir && (entry.link.is_none_or(LinkKind::has_own_size) || size > 0);
    if has_size {
        return format_size(
            size,
            width,
            false,
            size_style(column),
            platform::locale::number_format(),
        );
    }
    let label = if entry.is_parent {
        format::center(UP_LABEL, LABEL_WIDTH)
    } else if let Some(kind) = entry.link {
        link_label(kind).to_string()
    } else {
        format::center(FOLDER_LABEL, LABEL_WIDTH)
    };
    let label = if label.chars().count() + 2 <= width {
        format!("<{label}>")
    } else {
        label
    };
    right(&label, width)
}

/// FAR's label of a link in the size column (`MListJunction`, `MListSymlink`, the tag's name,
/// `MListUnknownReparsePoint`).
fn link_label(kind: LinkKind) -> &'static str {
    match kind {
        LinkKind::Junction => JUNCTION_LABEL,
        LinkKind::Symlink => SYMLINK_LABEL,
        LinkKind::Reparse(_) => kind.name().unwrap_or(UNKNOWN_LINK_LABEL),
    }
}

/// FAR's `FormatStr_DateTime` of `DM`, `DC`, `DA`, `DE`: "date time", a four-digit year for some widths
/// (not a brief one), the column's `B` and `M`.
fn date_and_time(time: Option<SystemTime>, column: &Column, now: SystemTime) -> String {
    let (width, flags) = (column.width, column.flags);
    let compared = width.saturating_sub(usize::from(flags.month));
    let style = platform::locale::DateStyle {
        full_year: !flags.brief && (matches!(compared, 15 | 16 | 18 | 19) || compared > 21),
        time_length: width.saturating_sub(9),
        brief: flags.brief,
        month: flags.month,
    };
    let (date, time) = time
        .map(|time| platform::locale::styled_date_time(time, style, now))
        .unwrap_or_default();
    if date.is_empty() {
        return " ".repeat(width);
    }
    crate::app::text_cut::fit_to_right(&format!("{date} {time}"), width)
}

fn time_part(
    time: Option<std::time::SystemTime>,
    full_year: bool,
    time_length: usize,
) -> (String, String) {
    time.map(|time| crate::platform::locale::date_time(time, full_year, time_length))
        .unwrap_or_default()
}

/// Right-aligned in `width`, cut on the left when longer.
fn right(text: &str, width: usize) -> String {
    let length = text.chars().count();
    if length >= width {
        return text.chars().skip(length - width).collect();
    }
    format!("{text:>width$}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::columns::ColumnFlags;
    use crate::ui::panel_names::aligned_name;

    fn column(kind: ColumnKind, width: usize) -> Column {
        Column {
            kind,
            flags: ColumnFlags::default(),
            width,
        }
    }

    #[test]
    fn a_counted_folder_shows_its_size_old_with_a_tilde_growing_with_an_ellipsis() {
        let wide = column(ColumnKind::Size, 8);
        let shown = |state| {
            let folder = Entry {
                name: "d".to_string(),
                is_dir: true,
                size: 1234,
                folder_size: state,
                ..Entry::default()
            };
            cell(&folder, &wide, false, true, false).text
        };
        assert_eq!(shown(FolderSize::Precise), "    1234");
        assert_eq!(shown(FolderSize::Outdated), "~   1234");
        assert_eq!(shown(FolderSize::Counting), "   1234…");
        assert_eq!(shown(FolderSize::Unknown), "<Folder>");
    }

    fn file(name: &str, size: u64) -> Entry {
        Entry {
            name: name.to_string(),
            size,
            ..Entry::default()
        }
    }

    #[test]
    fn extensions_align_three_from_the_right() {
        assert_eq!(aligned_name("readme.md", 12).unwrap(), "readme   md ");
        assert_eq!(aligned_name("main.rs", 10).unwrap(), "main   rs ");
        assert_eq!(aligned_name("a.json", 8).unwrap(), "a   json");
        assert_eq!(aligned_name("Makefile", 10), None);
        assert_eq!(aligned_name(".gitignore", 12), None);
        assert_eq!(aligned_name("toolong.txt", 5), None);
    }

    #[test]
    fn names_are_cut_and_flagged() {
        let name = column(ColumnKind::Name, 5);
        let cell = cell(&file("notes.txt", 1), &name, false, false, false);
        assert_eq!(cell.text, "notes");
        assert!(cell.too_long);
        let status = Column {
            flags: ColumnFlags {
                right_align: true,
                ..ColumnFlags::default()
            },
            ..name
        };
        let cell = super::cell(&file("notes.txt", 1), &status, false, false, true);
        assert_eq!(cell.text, "s.txt");
    }

    #[test]
    fn mark_column_shows_selection() {
        let marked = Column {
            flags: ColumnFlags {
                mark: true,
                ..ColumnFlags::default()
            },
            ..column(ColumnKind::Name, 8)
        };
        assert_eq!(
            cell(&file("a", 0), &marked, true, false, false).text,
            "√ a     "
        );
        assert_eq!(
            cell(&file("a", 0), &marked, false, false, false).text,
            "  a     "
        );
    }

    #[test]
    fn directories_show_far_labels() {
        let parent = Entry {
            name: "..".to_string(),
            is_dir: true,
            is_parent: true,
            ..Entry::default()
        };
        let folder = Entry {
            is_dir: true,
            ..file("src", 0)
        };
        let narrow = column(ColumnKind::Size, 6);
        let wide = column(ColumnKind::Size, 10);
        assert_eq!(size_text(&folder, 0, &narrow), "Folder");
        assert_eq!(size_text(&parent, 0, &narrow), "  Up  ");
        assert_eq!(size_text(&folder, 0, &wide), "  <Folder>");
        assert_eq!(size_text(&parent, 0, &wide), "  <  Up  >");
        assert_eq!(size_text(&file("a", 123_456), 123_456, &narrow), "123456");
        assert_eq!(
            size_text(&file("a", 1_234_567), 1_234_567, &narrow),
            "1206 K"
        );
    }

    #[test]
    fn links_have_an_arrow_after_the_name_and_the_status_line_names_the_target() {
        let link = Entry {
            is_dir: true,
            link: Some(LinkKind::Symlink),
            link_target: Some("E:\\dev\\docs".to_string()),
            ..file("docs", 0)
        };
        let name = column(ColumnKind::Name, 10);
        assert_eq!(cell(&link, &name, false, true, false).text, "docs →    ");
        let long = Entry {
            name: "documentation".to_string(),
            ..link.clone()
        };
        let cut = cell(&long, &name, false, true, false);
        assert_eq!((cut.text.as_str(), cut.too_long), ("document →", true));
        let status = column(ColumnKind::Name, 24);
        assert_eq!(
            cell(&link, &status, false, true, true).text,
            "docs → E:\\dev\\docs      "
        );
        // Too long: the name stays, the target keeps its end.
        let narrow = column(ColumnKind::Name, 14);
        assert_eq!(
            cell(&link, &narrow, false, true, true).text,
            "docs → …v\\docs"
        );
        // Even the name does not fit: its end, as for other long names.
        let tiny = column(ColumnKind::Name, 6);
        assert_eq!(cell(&link, &tiny, false, true, true).text, "v\\docs");
        // The size column grows to the whole label.
        let mut status_columns = vec![column(ColumnKind::Name, 30), column(ColumnKind::Size, 6)];
        fit_status_columns(&link, &mut status_columns);
        assert_eq!((status_columns[0].width, status_columns[1].width), (27, 9));
        assert_eq!(size_text(&link, 0, &status_columns[1]), "<Symlink>");
        // Short of room, the date and time make way for the name and the target.
        let mut narrow_status = vec![
            column(ColumnKind::Name, 10),
            column(ColumnKind::Size, 6),
            column(ColumnKind::Date, 8),
            column(ColumnKind::Time, 5),
        ];
        fit_status_columns(&link, &mut narrow_status);
        let kinds: Vec<(ColumnKind, usize)> = narrow_status
            .iter()
            .map(|column| (column.kind, column.width))
            .collect();
        assert_eq!(kinds, vec![(ColumnKind::Name, 22), (ColumnKind::Size, 9)]);
        // A target that cannot be read (a WSL link): the plain name, the date stays.
        let unread = Entry {
            link_target: None,
            ..link.clone()
        };
        assert_eq!(
            cell(&unread, &narrow, false, true, true).text,
            "docs          "
        );
        let mut with_date = vec![column(ColumnKind::Name, 4), column(ColumnKind::Date, 8)];
        fit_status_columns(&unread, &mut with_date);
        assert_eq!(with_date.len(), 2);
        // A file kept in the cloud is no link: no arrow.
        let cloud = Entry {
            link: Some(LinkKind::Reparse(0x9000_101A)),
            ..file("a.txt", 1)
        };
        assert_eq!(cell(&cloud, &name, false, false, false).text, "a.txt     ");
    }

    #[test]
    fn links_show_far_labels() {
        let link = |is_dir, kind| Entry {
            is_dir,
            link: Some(kind),
            ..file("link", 0)
        };
        let wide = column(ColumnKind::Size, 10);
        let narrow = column(ColumnKind::Size, 8);
        let junction = link(true, LinkKind::Junction);
        assert_eq!(size_text(&junction, 0, &wide), "<Junction>");
        assert_eq!(size_text(&junction, 0, &narrow), "Junction");
        let symlink = link(true, LinkKind::Symlink);
        assert_eq!(size_text(&symlink, 0, &wide), " <Symlink>");
        // A link to a file too: it has no size of its own.
        assert_eq!(
            size_text(&link(false, LinkKind::Symlink), 0, &wide),
            " <Symlink>"
        );
        assert_eq!(
            size_text(&link(true, LinkKind::Reparse(0x9000_101A)), 0, &wide),
            " <CLOUD_1>"
        );
        assert_eq!(
            size_text(&link(true, LinkKind::Reparse(0x1234)), 0, &wide),
            " ?Symlink?"
        );
        // Files behind other reparse points (OneDrive, deduplication) keep their size.
        assert_eq!(
            size_text(&link(false, LinkKind::Reparse(0x9000_101A)), 7, &wide),
            "         7"
        );
        assert_eq!(
            size_text(
                &link(false, LinkKind::from_reparse_tag(0x8000_0013)),
                0,
                &wide
            ),
            "         0"
        );
    }

    #[test]
    fn missing_times_leave_blank_cells() {
        let entry = file("a", 0);
        assert_eq!(
            cell(&entry, &column(ColumnKind::Date, 8), false, false, false).text,
            "        "
        );
        let now = SystemTime::now();
        assert_eq!(
            date_and_time(None, &column(ColumnKind::WriteTime, 14), now),
            " ".repeat(14)
        );
        let allocated = column(ColumnKind::Allocated, 6);
        assert_eq!(cell(&entry, &allocated, false, false, false).text, "      ");
        let on_disk = Entry {
            allocated: Some(4096),
            ..file("a", 1)
        };
        assert_eq!(
            cell(&on_disk, &allocated, false, false, false).text,
            "  4096"
        );
        assert_eq!(right("abcdef", 3), "def");
    }
}
