//! Advanced compare's options (FAR's compare `ShowDialog`, `ShowDialogProc`): its dialog at FAR's places
//! (66 x 22), the settings under FAR's names, read as the dialog opens and saved as OK closes it - those
//! of the items not disabled, as FAR saves them. Subfolders and contents are off for a plugin's panel,
//! "selected items only" off when neither panel has a selection.

use nc_plugin_sdk::{Closed, Controls, Dialog, DialogEvent, DialogItem, Host};

/// The plugin's title, its dialog's and its messages'.
pub const TITLE: &str = "Advanced compare";

/// The options of one comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub subfolders: bool,
    pub use_max_depth: bool,
    pub max_depth: u32,
    pub selected_only: bool,
    pub time: bool,
    /// Two seconds' precision.
    pub low_precision: bool,
    /// A difference of a whole number of quarter hours, up to 26 hours, is none.
    pub ignore_time_zone: bool,
    pub size: bool,
    pub contents: bool,
    /// Contents compared ignoring what [`Options::whitespace`] says.
    pub contents_ignore: bool,
    /// Ignoring white space (else the new line characters' differences).
    pub whitespace: bool,
    pub message_when_no_difference: bool,
}

impl Default for Options {
    /// FAR's defaults.
    fn default() -> Self {
        Self {
            subfolders: false,
            use_max_depth: false,
            max_depth: 99,
            selected_only: false,
            time: true,
            low_precision: true,
            ignore_time_zone: true,
            size: true,
            contents: false,
            contents_ignore: false,
            whitespace: false,
            message_when_no_difference: false,
        }
    }
}

/// The dialog's items by index (FAR's `InitItems` less its box).
pub const SUBFOLDERS: usize = 1;
pub const USE_MAX_DEPTH: usize = 2;
pub const MAX_DEPTH: usize = 3;
pub const SELECTED_ONLY: usize = 4;
pub const TIME: usize = 7;
pub const LOW_PRECISION: usize = 8;
pub const TIME_ZONE: usize = 9;
pub const SIZE: usize = 10;
pub const CONTENTS: usize = 11;
pub const CONTENTS_IGNORE: usize = 12;
pub const NEW_LINES: usize = 13;
pub const WHITESPACE: usize = 14;
pub const NO_DIFFERENCE: usize = 16;
pub const OK: usize = 18;

/// Each setting: FAR's name, the dialog's item.
const SETTINGS: [(&str, usize); 13] = [
    ("ProcessSubfolders", SUBFOLDERS),
    ("UseMaxScanDepth", USE_MAX_DEPTH),
    ("MaxScanDepth", MAX_DEPTH),
    ("ProcessSelected", SELECTED_ONLY),
    ("CompareTime", TIME),
    ("LowPrecisionTime", LOW_PRECISION),
    ("IgnorePossibleTimeZoneDifferences", TIME_ZONE),
    ("CompareSize", SIZE),
    ("CompareContents", CONTENTS),
    ("CompareContentsIgnore", CONTENTS_IGNORE),
    ("IgnoreNewLines", NEW_LINES),
    ("IgnoreWhitespace", WHITESPACE),
    ("MessageWhenNoDiff", NO_DIFFERENCE),
];

impl Options {
    pub fn load(host: &Host) -> Self {
        let default = Self::default();
        let on = |key: &str, default: bool| host.bool_setting(key, default);
        let whitespace = on("IgnoreWhitespace", default.whitespace);
        let new_lines = on("IgnoreNewLines", !default.whitespace);
        Self {
            subfolders: on("ProcessSubfolders", default.subfolders),
            use_max_depth: on("UseMaxScanDepth", default.use_max_depth),
            max_depth: u32::try_from(host.int_setting("MaxScanDepth", 99)).unwrap_or(99),
            selected_only: on("ProcessSelected", default.selected_only),
            time: on("CompareTime", default.time),
            low_precision: on("LowPrecisionTime", default.low_precision),
            ignore_time_zone: on(
                "IgnorePossibleTimeZoneDifferences",
                default.ignore_time_zone,
            ),
            size: on("CompareSize", default.size),
            contents: on("CompareContents", default.contents),
            contents_ignore: on("CompareContentsIgnore", default.contents_ignore),
            // FAR: both radio buttons the same - the new lines' one on.
            whitespace: whitespace && !new_lines,
            message_when_no_difference: on("MessageWhenNoDiff", default.message_when_no_difference),
        }
    }

    /// FAR's dialog: `plugins` (a plugin's panel among the two) turns subfolders and contents off,
    /// `selection` (either panel has selected items) lets "selected items only" on.
    pub fn dialog(&self, plugins: bool, selection: bool) -> Dialog {
        let check = |label: &str, on: bool, (x, y): (i32, i32)| {
            let x2 = x + i32::try_from(label.len()).unwrap_or(0) + 4;
            DialogItem::check(label, on).at(x, y, x2, y)
        };
        let radio = |label: &str, on: bool, y: i32| {
            let x2 = 13 + i32::try_from(label.len()).unwrap_or(0) + 4;
            DialogItem::radio(label, on).at(13, y, x2, y)
        };
        let line = |y: i32| DialogItem::separator().at(-1, y, -1, y);
        let subfolders = self.subfolders && !plugins;
        let contents = self.contents && !plugins;
        let ignore = contents && self.contents_ignore;
        let items = vec![
            DialogItem::text("Process:").at(5, 2, 12, 2),
            check("sub&folders", subfolders, (5, 3)).disabled(plugins),
            check("&maximum folder depth:", self.use_max_depth, (9, 4)).disabled(!subfolders),
            DialogItem::fixed("99999", &self.max_depth.to_string())
                .at(35, 4, 39, 4)
                .disabled(!subfolders),
            check(
                "s&elected items only",
                self.selected_only && selection,
                (5, 5),
            )
            .disabled(!selection),
            line(6),
            DialogItem::text("Compare:").at(5, 7, 12, 7),
            check("&time", self.time, (5, 8)),
            check("two seconds &precision", self.low_precision, (9, 9)).disabled(!self.time),
            check(
                "&ignore possible time zone differences",
                self.ignore_time_zone,
                (9, 10),
            )
            .disabled(!self.time),
            check("&size", self.size, (5, 11)),
            check("&contents", contents, (5, 12)).disabled(plugins),
            check("i&gnore:", self.contents_ignore, (9, 13)).disabled(!contents),
            radio("differences in new &line characters", !self.whitespace, 14)
                .group()
                .disabled(!ignore),
            radio("&whitespace", self.whitespace, 15).disabled(!ignore),
            line(16),
            check(
                "Display a message if &no differences were found",
                self.message_when_no_difference,
                (5, 17),
            ),
            line(18),
            DialogItem::button("Ok").default_button().at(24, 19, 29, 19),
            DialogItem::button("Cancel").at(31, 19, 40, 19),
        ];
        let mut dialog = Dialog::new(TITLE, items).size(66, 22);
        // FAR's focus: the first check box not disabled.
        if let Some(first) = (dialog.items.iter_mut())
            .find(|item| item.kind == nc_plugin_sdk::ItemKind::Check && !item.disabled)
        {
            first.focus = true;
        }
        dialog
    }

    /// FAR's `ShowDialogProc`: a box turned on lets the ones under it on (contents' radio buttons only
    /// with "ignore" on), turned off turns them off.
    pub fn handle(event: &DialogEvent, controls: &mut Controls) -> bool {
        let DialogEvent::Changed(item) = *event else {
            return true;
        };
        let on = controls.value(item).checked();
        let under: &[usize] = match item {
            SUBFOLDERS => &[USE_MAX_DEPTH, MAX_DEPTH],
            TIME => &[LOW_PRECISION, TIME_ZONE],
            CONTENTS => &[CONTENTS_IGNORE],
            CONTENTS_IGNORE => &[NEW_LINES, WHITESPACE],
            _ => &[],
        };
        for &below in under {
            controls.enable(below, on);
        }
        if item == CONTENTS {
            let ignore = on && controls.value(CONTENTS_IGNORE).checked();
            controls.enable(NEW_LINES, ignore);
            controls.enable(WHITESPACE, ignore);
        }
        true
    }

    /// The options the dialog closed by OK gives, its items' values as they are (the disabled ones off as
    /// shown), and saves those of the items not disabled as it closed (FAR's `CheckDisabled` after
    /// `DialogRun`); `plugins` and `selection` as the dialog was made with.
    pub fn from_dialog(
        host: &Host,
        closed: &Closed,
        (plugins, selection): (bool, bool),
    ) -> Option<Self> {
        if closed.button != Some(OK) {
            return None;
        }
        for (key, value) in saved(closed, plugins, selection) {
            host.set_setting(key, &value);
        }
        Some(Self::of_values(closed))
    }

    /// The options of the dialog's values.
    pub fn of_values(closed: &Closed) -> Self {
        let on = |item: usize| closed.values.get(item).is_some_and(|value| value.checked());
        let depth = (closed.values.get(MAX_DEPTH))
            .and_then(|value| value.text.trim().parse().ok())
            .unwrap_or(0);
        Self {
            subfolders: on(SUBFOLDERS),
            use_max_depth: on(USE_MAX_DEPTH),
            max_depth: depth,
            selected_only: on(SELECTED_ONLY),
            time: on(TIME),
            low_precision: on(LOW_PRECISION),
            ignore_time_zone: on(TIME_ZONE),
            size: on(SIZE),
            contents: on(CONTENTS),
            contents_ignore: on(CONTENTS_IGNORE),
            whitespace: on(WHITESPACE),
            message_when_no_difference: on(NO_DIFFERENCE),
        }
    }
}

/// Whether the dialog's `item` was enabled as it closed: the rules it opened with and `handle` keeps.
fn enabled(closed: &Closed, item: usize, (plugins, selection): (bool, bool)) -> bool {
    let on = |item: usize| closed.values.get(item).is_some_and(|value| value.checked());
    match item {
        SUBFOLDERS | CONTENTS => !plugins,
        USE_MAX_DEPTH | MAX_DEPTH => !plugins && on(SUBFOLDERS),
        SELECTED_ONLY => selection,
        LOW_PRECISION | TIME_ZONE => on(TIME),
        CONTENTS_IGNORE => !plugins && on(CONTENTS),
        NEW_LINES | WHITESPACE => !plugins && on(CONTENTS) && on(CONTENTS_IGNORE),
        _ => true,
    }
}

/// The settings the dialog closed gives: each enabled item's, as text (FAR's names).
pub fn saved(closed: &Closed, plugins: bool, selection: bool) -> Vec<(&'static str, String)> {
    (SETTINGS.iter())
        .filter(|(_, item)| enabled(closed, *item, (plugins, selection)))
        .filter_map(|&(key, item)| {
            let value = closed.values.get(item)?;
            let text = match item {
                MAX_DEPTH => value.text.trim().parse::<u32>().unwrap_or(0).to_string(),
                _ => String::from(if value.checked() { "1" } else { "0" }),
            };
            Some((key, text))
        })
        .collect()
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
