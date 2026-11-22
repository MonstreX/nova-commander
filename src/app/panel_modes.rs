//! FAR's file panel modes made from the settings (`far/panelmix.cpp` `DeserialiseViewSettings`): each
//! mode's columns read from FAR's types and widths, its flags, and the mode a panel's view shows - a
//! provider's own over the program's (FAR's `PrepareViewSettings` of a plugin's panel).

use std::borrow::Cow;

use super::ViewMode;
use crate::{settings, vfs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Name,
    Size,
    /// Space on disk (`P`).
    Allocated,
    Date,
    Time,
    WriteTime,
    CreationTime,
    AccessTime,
    ChangeTime,
    Attributes,
    /// The extension alone (`X`).
    Extension,
    /// A provider's text of an entry (`C0`..`C99`, FAR's `CustomColumnData`).
    Custom(u8),
    /// An entry's description (`Z`): a provider's (file descriptions come later).
    Description,
}

impl ColumnKind {
    /// FAR's `ColumnInfo` width; 0 means "share the remaining space".
    fn base_width(self) -> usize {
        match self {
            Self::Name | Self::Extension | Self::Custom(_) | Self::Description => 0,
            Self::Size | Self::Allocated | Self::Attributes => 6,
            Self::Date => 8,
            Self::Time => 5,
            Self::WriteTime | Self::CreationTime | Self::AccessTime | Self::ChangeTime => 14,
        }
    }

    /// The columns of a date and a time together (`DM`, `DC`, `DA`, `DE`).
    pub fn is_date_and_time(self) -> bool {
        matches!(
            self,
            Self::WriteTime | Self::CreationTime | Self::AccessTime | Self::ChangeTime
        )
    }

    /// Column title (`MColumn*` in `far/farlang.templ.m4`).
    pub fn title(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Size => "Size",
            Self::Allocated => "Allocated",
            Self::Date => "Date",
            Self::Time => "Time",
            Self::WriteTime => "Write",
            Self::CreationTime => "Created",
            Self::AccessTime => "Accessed",
            Self::ChangeTime => "Change",
            Self::Attributes => "Attr",
            Self::Extension => "Extension",
            Self::Custom(_) => "",
            Self::Description => "Description",
        }
    }
}

/// FAR's `COLFLAGS_*` a column type can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColumnFlags {
    /// Long names keep their end (`R`).
    pub right_align: bool,
    /// Names always to the right (`F`).
    pub right_align_force: bool,
    /// "√ " in front of selected names (`M`).
    pub mark: bool,
    /// The mark takes room only on selected names (`D`).
    pub mark_dynamic: bool,
    /// Files without their extension (`N`).
    pub no_extension: bool,
    /// A folder's last name only (`O`): a file panel's names are that already.
    pub name_only: bool,
    /// Thousands separators in sizes (`C`).
    pub group_digits: bool,
    /// No space before the unit (`E`).
    pub economic: bool,
    /// "1.21 M" (`F`).
    pub float: bool,
    /// Units of 1000 (`T`).
    pub thousand: bool,
    /// A short date, the year for old ones instead of the time (`B`).
    pub brief: bool,
    /// The month's name (`M`).
    pub month: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnDef {
    pub kind: ColumnKind,
    pub flags: ColumnFlags,
    /// 0 = the default width of the kind.
    pub width: usize,
    /// The width is a part of the panel in percent (`40%`).
    pub percent: bool,
    /// FAR's negative width (`EmptyColumns`): the column is kept in the stripes but not shown and takes
    /// no room, its divider neither.
    pub empty: bool,
}

impl ColumnDef {
    /// FAR's `GetDefaultWidth`.
    pub fn default_width(&self) -> usize {
        let width = self.kind.base_width();
        if !self.kind.is_date_and_time() {
            return width;
        }
        let width = if self.flags.brief { width - 3 } else { width };
        width + usize::from(self.flags.month)
    }
}

/// One mode made ready (FAR's `PanelViewSettings`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeSpec {
    pub name: String,
    /// The panel columns' types as the mode gives them ("N,S,D"; FAR's `FCTL_GETCOLUMNTYPES`).
    pub types: String,
    pub panel: Vec<ColumnDef>,
    /// The panel columns' own titles (a provider's mode), by their place; `None` or none: the kind's, an
    /// empty one shown so (FAR's `ColumnTitles`).
    pub titles: Vec<Option<String>>,
    pub status: Vec<ColumnDef>,
    pub full_screen: bool,
    pub align_extensions: bool,
    pub folder_align_extensions: bool,
    pub folder_upper_case: bool,
    pub files_lower_case: bool,
    pub upper_to_lower_case: bool,
}

impl ModeSpec {
    pub fn make(mode: &settings::PanelMode) -> Self {
        Self {
            name: mode.name.clone(),
            types: mode.columns.clone(),
            panel: columns(&mode.columns, &mode.widths),
            titles: Vec::new(),
            status: columns(&mode.status_columns, &mode.status_widths),
            full_screen: mode.full_screen,
            align_extensions: mode.align_extensions,
            folder_align_extensions: mode.folder_align_extensions,
            folder_upper_case: mode.folder_upper_case,
            files_lower_case: mode.files_lower_case,
            upper_to_lower_case: mode.upper_to_lower_case,
        }
    }

    /// Whether the view shows names without their folders: its first name column's `O` (FAR's
    /// `IgnorePaths`, `filelist.cpp` `SortFileList`), which the sort follows.
    pub fn names_only(&self) -> bool {
        (self.panel.iter())
            .find(|column| column.kind == ColumnKind::Name)
            .is_some_and(|column| column.flags.name_only)
    }

    /// The program's mode on a provider's panel with its flags (FAR's `PrepareViewSettings` without a
    /// mode of the plugin's, `filelist.cpp`): names only and kept to the right on name columns, names'
    /// cases as given.
    pub fn shown_by(&self, look: &vfs::PanelLook) -> Self {
        let mut shown = self.clone();
        for column in (shown.panel.iter_mut()).filter(|column| column.kind == ColumnKind::Name) {
            column.flags.name_only |= look.names_only;
            column.flags.right_align |= look.right_align_names;
        }
        if look.preserve_case {
            shown.folder_upper_case = false;
            shown.files_lower_case = false;
            shown.upper_to_lower_case = false;
        }
        shown
    }

    /// A provider's mode over the program's `base` at its key (FAR's `PrepareViewSettings`): its columns
    /// and titles, its status line (none: the name, or the name, size, date and time), full screen and
    /// aligned extensions as it says; the program's cases only with its `case_conversion`.
    pub fn of_provider(own: &vfs::PanelMode, base: &Self) -> Self {
        let (panel, titles) = titled_columns(&own.columns, &own.widths, &own.titles);
        let status = if !own.status_columns.is_empty() && !own.status_widths.is_empty() {
            columns(&own.status_columns, &own.status_widths)
        } else if own.detailed_status {
            // FAR's `PMFLAGS_DETAILEDSTATUS` (`filelist.cpp`): the size without digit groups.
            columns("NR,S,D,T", "0,8,0,5")
        } else {
            columns("NR", "0")
        };
        let cases = own.case_conversion;
        Self {
            name: base.name.clone(),
            types: own.columns.clone(),
            panel,
            titles,
            status,
            full_screen: own.full_screen,
            align_extensions: own.align_extensions,
            folder_align_extensions: base.folder_align_extensions,
            folder_upper_case: cases && base.folder_upper_case,
            files_lower_case: cases && base.files_lower_case,
            upper_to_lower_case: cases && base.upper_to_lower_case,
        }
    }
}

/// What a type gives: a column, one FAR knows that is not drawn here yet (owners, links, streams, a
/// plugin's `<name>`: it still takes its width), or nothing (FAR leaves an unknown type out).
enum Parsed {
    Column(ColumnKind, ColumnFlags),
    NotDrawn,
    Unknown,
}

fn name_flags(modifiers: &str) -> ColumnFlags {
    let mut flags = ColumnFlags::default();
    for letter in modifiers.chars() {
        match letter {
            'M' => flags.mark = true,
            'D' => flags.mark_dynamic = true,
            'O' => flags.name_only = true,
            'R' => flags.right_align = true,
            'F' => flags.right_align_force = true,
            'N' => flags.no_extension = true,
            _ => {}
        }
    }
    flags
}

fn size_flags(modifiers: &str) -> ColumnFlags {
    let mut flags = ColumnFlags::default();
    for letter in modifiers.chars() {
        match letter {
            'C' => flags.group_digits = true,
            'E' => flags.economic = true,
            'F' => flags.float = true,
            'T' => flags.thousand = true,
            _ => {}
        }
    }
    flags
}

fn date_flags(modifiers: &str) -> ColumnFlags {
    ColumnFlags {
        brief: modifiers.contains('B'),
        month: modifiers.contains('M'),
        ..ColumnFlags::default()
    }
}

/// One of FAR's column types.
fn parse_type(text: &str) -> Parsed {
    let plain = |kind| Parsed::Column(kind, ColumnFlags::default());
    if let Some(modifiers) = text.strip_prefix('N') {
        return Parsed::Column(ColumnKind::Name, name_flags(modifiers));
    }
    if let Some(modifiers) = text.strip_prefix('S') {
        return Parsed::Column(ColumnKind::Size, size_flags(modifiers));
    }
    if let Some(modifiers) = text.strip_prefix('P') {
        return Parsed::Column(ColumnKind::Allocated, size_flags(modifiers));
    }
    let dates = [
        ("DM", ColumnKind::WriteTime),
        ("DC", ColumnKind::CreationTime),
        ("DA", ColumnKind::AccessTime),
        ("DE", ColumnKind::ChangeTime),
    ];
    if let Some((modifiers, kind)) = (dates.iter())
        .find_map(|(prefix, kind)| text.strip_prefix(prefix).map(|rest| (rest, *kind)))
    {
        return Parsed::Column(kind, date_flags(modifiers));
    }
    if let Some(modifiers) = text.strip_prefix('X') {
        let flags = ColumnFlags {
            right_align: modifiers.starts_with('R'),
            ..ColumnFlags::default()
        };
        return Parsed::Column(ColumnKind::Extension, flags);
    }
    let custom = text.len() > 2 && text.starts_with('<') && text.ends_with('>');
    if (2..=3).contains(&text.len())
        && let Some(index) = text.strip_prefix('C')
    {
        // FAR's `C0`..`C99` (its `from_string` reads the digits it starts with); a `C` without a
        // number is its default column, a name.
        let digits: String = index.chars().take_while(char::is_ascii_digit).collect();
        return match digits.parse::<u8>() {
            Ok(number) => plain(ColumnKind::Custom(number)),
            Err(_) => plain(ColumnKind::Name),
        };
    }
    match text {
        "D" => plain(ColumnKind::Date),
        "T" => plain(ColumnKind::Time),
        "A" => plain(ColumnKind::Attributes),
        "Z" => plain(ColumnKind::Description),
        _ if text.starts_with('G') || text.starts_with('O') => Parsed::NotDrawn,
        "LN" | "F" => Parsed::NotDrawn,
        _ if custom => Parsed::NotDrawn,
        _ => Parsed::Unknown,
    }
}

/// FAR's width (`from_string` to an `int`): its leading digits, a `-` before them allowed (none: the
/// default), `%` after them a part of the panel; a negative one is an empty column. Returns (width,
/// percent, empty).
fn parse_width(text: &str) -> (usize, bool, bool) {
    let (negative, rest) = text
        .strip_prefix('-')
        .map_or((false, text), |rest| (true, rest));
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    // FAR reads an `int`: one out of its range is the default.
    let width = (digits.parse::<i32>().ok())
        .and_then(|width| usize::try_from(width).ok())
        .unwrap_or(0);
    let percent = text.len() > 1 && text.ends_with('%');
    if negative && width > 0 {
        return (0, percent, true);
    }
    (width, percent, false)
}

/// FAR's `DeserialiseViewSettings`: the types with the widths in their order (missing ones the
/// default); none at all is one name column.
pub fn columns(types: &str, widths: &str) -> Vec<ColumnDef> {
    titled_columns(types, widths, &[]).0
}

/// [`columns`] with their titles, `titles` by FAR's columns (those it knows, drawn here or not); a
/// missing title is `None`: the kind's own.
fn titled_columns(
    types: &str,
    widths: &str,
    titles: &[Option<String>],
) -> (Vec<ColumnDef>, Vec<Option<String>>) {
    let parsed: Vec<Parsed> = (types.split(','))
        .filter(|text| !text.is_empty())
        .map(parse_type)
        .filter(|parsed| !matches!(parsed, Parsed::Unknown))
        .collect();
    let mut widths = widths.split(',');
    let mut made: Vec<ColumnDef> = Vec::new();
    let mut made_titles = Vec::new();
    for (at, parsed) in parsed.into_iter().enumerate() {
        let (width, percent, empty) = widths.next().map_or((0, false, false), parse_width);
        if let Parsed::Column(kind, flags) = parsed {
            made.push(ColumnDef {
                kind,
                flags,
                width,
                percent,
                empty,
            });
            made_titles.push(titles.get(at).cloned().flatten());
        }
    }
    if made.is_empty() {
        made.push(ColumnDef {
            kind: ColumnKind::Name,
            flags: ColumnFlags::default(),
            width: 0,
            percent: false,
            empty: false,
        });
    }
    (made, made_titles)
}

/// The modes as the settings give them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelModes {
    kept: settings::PanelModes,
    made: [ModeSpec; 6],
    custom: Vec<ModeSpec>,
}

/// The place of a predefined mode among the six.
fn slot(mode: ViewMode) -> Option<usize> {
    match mode {
        ViewMode::Brief => Some(0),
        ViewMode::Medium => Some(1),
        ViewMode::Full => Some(2),
        ViewMode::Wide => Some(3),
        ViewMode::Detailed => Some(4),
        ViewMode::AlternativeFull => Some(5),
        ViewMode::Custom(_) | ViewMode::Provider(_) => None,
    }
}

fn slot_of(modes: &settings::PanelModes, at: usize) -> &settings::PanelMode {
    [
        &modes.brief,
        &modes.medium,
        &modes.full,
        &modes.wide,
        &modes.detailed,
        &modes.alternative,
    ][at]
}

/// FAR's `ResetViewModes` of a mode: its `InitialModes` (a user's mode: an empty one).
pub fn far_default(mode: ViewMode) -> settings::PanelMode {
    let far = settings::PanelModes::far_defaults();
    slot(mode).map_or_else(settings::PanelMode::default, |at| slot_of(&far, at).clone())
}

impl Default for PanelModes {
    fn default() -> Self {
        Self::make(settings::PanelModes::far_defaults())
    }
}

impl PanelModes {
    pub fn make(kept: settings::PanelModes) -> Self {
        let made = [0, 1, 2, 3, 4, 5].map(|at| ModeSpec::make(slot_of(&kept, at)));
        let custom = kept.custom.iter().map(ModeSpec::make).collect();
        Self { kept, made, custom }
    }

    /// The mode as the settings keep it; `None` for a user's mode that is not there.
    pub fn kept_mode(&self, mode: ViewMode) -> Option<&settings::PanelMode> {
        match (slot(mode), mode) {
            (Some(at), _) => Some(slot_of(&self.kept, at)),
            (None, ViewMode::Custom(at)) => self.kept.custom.get(at),
            (None, _) => None,
        }
    }

    /// The user's own modes.
    pub fn custom_count(&self) -> usize {
        self.kept.custom.len()
    }

    /// FAR's `SetViewSettings`.
    pub fn set(&mut self, mode: ViewMode, value: settings::PanelMode) {
        let kept = &mut self.kept;
        let slot = match mode {
            ViewMode::Brief => &mut kept.brief,
            ViewMode::Medium => &mut kept.medium,
            ViewMode::Full => &mut kept.full,
            ViewMode::Wide => &mut kept.wide,
            ViewMode::Detailed => &mut kept.detailed,
            ViewMode::AlternativeFull => &mut kept.alternative,
            ViewMode::Custom(at) => match kept.custom.get_mut(at) {
                Some(slot) => slot,
                None => return,
            },
            ViewMode::Provider(_) => return,
        };
        *slot = value;
        *self = Self::make(self.kept.clone());
    }

    /// FAR's `AddViewSettings`: a user's mode at `at` among them (past them: the last).
    pub fn add(&mut self, at: usize, value: settings::PanelMode) -> ViewMode {
        let at = at.min(self.kept.custom.len());
        self.kept.custom.insert(at, value);
        *self = Self::make(self.kept.clone());
        ViewMode::Custom(at)
    }

    /// FAR's `DeleteViewSettings` of a user's mode.
    pub fn remove(&mut self, at: usize) {
        if at < self.kept.custom.len() {
            self.kept.custom.remove(at);
            *self = Self::make(self.kept.clone());
        }
    }

    /// The modes as the settings keep them.
    pub fn kept(&self) -> &settings::PanelModes {
        &self.kept
    }

    /// The mode a view shows on a panel that looks `look`: a provider's own at its key over the
    /// program's (ADR-0016 §13).
    pub fn spec_in(&self, mode: ViewMode, look: &vfs::PanelLook) -> Cow<'_, ModeSpec> {
        let base = self.spec(mode);
        match mode.number().and_then(|number| look.mode(number)) {
            Some(own) => Cow::Owned(ModeSpec::of_provider(own, base)),
            None if look.names_only || look.right_align_names || look.preserve_case => {
                Cow::Owned(base.shown_by(look))
            }
            None => Cow::Borrowed(base),
        }
    }

    /// The mode a view shows; a user's mode that is not there, or a provider's mode not there, shows
    /// Medium.
    pub fn spec(&self, mode: ViewMode) -> &ModeSpec {
        match (slot(mode), mode) {
            (Some(at), _) => &self.made[at],
            (None, ViewMode::Custom(at)) => self.custom.get(at).unwrap_or(&self.made[1]),
            (None, _) => &self.made[1],
        }
    }
}

#[cfg(test)]
#[path = "panel_modes_tests.rs"]
mod tests;
