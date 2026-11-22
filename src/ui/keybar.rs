//! FAR's key bar (`far/keybar.cpp`): twelve keys "1Help 2UserMn ... 12Screen", the label set chosen by
//! the modifier keys held (`KBL_MAIN`, `KBL_SHIFT`, `KBL_ALT`, `KBL_CTRL`, ...), labels from
//! `far/farlang.templ.m4` (`MF1`, `MShiftF1`, `MAltF1`, `MCtrlF1`, ...).

use crate::app::text_cut::cut_right;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::theme;
use crate::platform::input::Modifiers;

const KEY_COUNT: usize = 12;
const MIN_LABEL_WIDTH: usize = 6;

pub type Labels = [&'static str; KEY_COUNT];

/// FAR's viewer keys (`MViewF1`...).
const VIEWER: Labels = [
    "Help", "Wrap", "Quit", "Hex", "", "Edit", "Search", "ANSI", "", "Quit", "Plugins", "Screen",
];
const VIEWER_SHIFT: Labels = [
    "", "WWrap", "", "Mode", "", "", "Next", "CodePg", "", "", "", "",
];
const VIEWER_ALT: Labels = [
    "", "", "", "", "", "", "Prev", "Goto", "Video", "", "ViewHs", "",
];
const VIEWER_CTRL: Labels = ["", "", "", "", "", "", "", "", "", "GoFile", "", ""];
const VIEWER_ALT_SHIFT: Labels = ["", "", "", "", "", "", "", "", "Config", "", "", ""];

/// The viewer's labels for the modifiers held (FAR's `Viewer::UpdateViewKeyBar`): F2's and F4's what
/// each would do, F8's the code page it would switch to (`[f2, f4, f8]`), Shift+F2's the wrapping.
pub fn viewer_labels(
    held: Modifiers,
    [f2, f4, f8]: [&'static str; 3],
    shift_f2: &'static str,
) -> Labels {
    match (held.ctrl, held.alt, held.shift) {
        (false, false, false) => {
            let mut labels = VIEWER;
            labels[1] = f2;
            labels[3] = f4;
            labels[7] = f8;
            labels
        }
        (false, false, true) => {
            let mut labels = VIEWER_SHIFT;
            labels[1] = shift_f2;
            labels
        }
        (false, true, false) => VIEWER_ALT,
        (true, false, false) => VIEWER_CTRL,
        (false, true, true) => VIEWER_ALT_SHIFT,
        _ => EMPTY,
    }
}

/// FAR's "Find folder" keys.
pub const FOLDER_TREE: Labels = [
    "Help", "Rescan", "", "", "Zoom", "", "", "", "", "Quit", "", "",
];
/// FAR's editor keys (`MEditF1`...).
const EDITOR: Labels = [
    "Help", "Save", "", "Quit", "", "View", "Search", "ANSI", "", "Quit", "Plugin", "Screen",
];
const EDITOR_SHIFT: Labels = [
    "", "SaveAs", "", "Edit…", "", "", "Next", "CodePg", "", "SaveQ", "", "",
];
const EDITOR_ALT: Labels = [
    "", "", "", "", "", "", "Prev", "Goto", "Video", "", "ViewHs", "",
];
const EDITOR_CTRL: Labels = [
    "", "", "LineNum", "", "", "", "Replac", "", "", "GoFile", "", "",
];

/// The editor's labels for the modifiers held, F8's the code page it would switch to (`page`),
/// Ctrl+F4's the mode it would switch to (`hex`: in the hex mode).
pub fn editor_labels(held: Modifiers, page: &'static str, hex: bool) -> Labels {
    match (held.ctrl, held.alt, held.shift) {
        (false, false, false) => {
            let mut labels = EDITOR;
            labels[7] = page;
            labels
        }
        (false, false, true) => EDITOR_SHIFT,
        (false, true, false) => EDITOR_ALT,
        // Ctrl+F4: the hex mode (ADR-0027 and its addendum) or back to the text.
        (true, false, false) => {
            let mut labels = EDITOR_CTRL;
            labels[3] = if hex { "Text" } else { "Hex" };
            labels
        }
        _ => EMPTY,
    }
}

/// File panels, nothing held.
pub const PANELS: Labels = [
    "Help", "UserMn", "View", "Edit", "Copy", "RenMov", "MkFold", "Delete", "ConfMn", "Quit",
    "Plugin", "Screen",
];
const PANELS_SHIFT: Labels = [
    "Add", "Extrct", "ArcCmd", "Edit…", "Copy", "Rename", "", "Delete", "Save", "Last", "Group",
    "SelUp",
];
const PANELS_ALT: Labels = [
    "Left", "Right", "View…", "Edit…", "Print", "MkLink", "Find", "Histry", "Video", "Tree",
    "ViewHs", "FoldHs",
];
const PANELS_CTRL: Labels = [
    "Left", "Right", "Name", "Extens", "Write", "Size", "Unsort", "Creatn", "Access", "Descr",
    "Owner", "Sort",
];
const PANELS_CTRL_SHIFT: Labels = ["", "", "View", "Edit", "", "", "", "", "", "", "", ""];
const PANELS_ALT_SHIFT: Labels = ["", "", "", "", "", "", "", "", "ConfPl", "", "", ""];
const EMPTY: Labels = [""; KEY_COUNT];

/// FAR's quick view keys (`MQViewF1`...).
const QUICK_VIEW: Labels = [
    "Help", "Wrap", "View", "Hex", "", "", "Search", "ANSI", "ConfMn", "Quit", "Plugin", "Screen",
];
const QUICK_VIEW_SHIFT: Labels = [
    "", "WWrap", "", "", "", "", "Next", "CodePg", "Save", "Last", "", "",
];
const QUICK_VIEW_ALT: Labels = [
    "Left", "Right", "", "", "", "", "Find", "Goto", "Video", "Tree", "ViewHs", "FoldHs",
];
const QUICK_VIEW_CTRL: Labels = ["Left", "Right", "", "", "", "", "", "", "", "", "", ""];

/// The quick view's labels for the modifiers held (FAR's `QuickView::DynamicUpdateKeyBar`): a file's
/// are its viewer's (F2 Unwrap as it is wrapped, F8 the code page it would switch to: `next_page`);
/// with no file F2 is the user menu, Alt+F8 the history, and the viewer's keys have none.
pub fn quick_view_labels(held: Modifiers, next_page: Option<&'static str>) -> Labels {
    let mut labels = match (held.ctrl, held.alt, held.shift) {
        (false, false, false) => QUICK_VIEW,
        (false, false, true) => QUICK_VIEW_SHIFT,
        (false, true, false) => QUICK_VIEW_ALT,
        (true, false, false) => QUICK_VIEW_CTRL,
        _ => EMPTY,
    };
    let modifiers = (held.ctrl, held.alt, held.shift);
    match (next_page, modifiers) {
        (Some(page), (false, false, false)) => {
            labels[1] = "Unwrap";
            labels[7] = page;
        }
        (None, (false, false, false)) => {
            labels[1] = PANELS[1];
            labels[3] = "";
            labels[7] = "";
        }
        (None, (false, false, true)) => {
            labels[1] = "";
            labels[7] = "";
        }
        (None, (false, true, false)) => labels[7] = PANELS_ALT[7],
        _ => {}
    }
    labels
}

/// The info panel's labels (FAR's `MInfoF1`... and `InfoList::DynamicUpdateKeyBar`): the quick view's
/// with F3 "VieDiz" and F4 "EdtDiz" (F3 none without a description file; ours: F4 keeps its label while
/// a file is shown, where FAR's viewer writes "Hex" over it though F4 edits), Ctrl+F12 "Mode".
pub fn info_labels(held: Modifiers, next_page: Option<&'static str>) -> Labels {
    let mut labels = quick_view_labels(held, next_page);
    match (held.ctrl, held.alt, held.shift) {
        (false, false, false) => {
            labels[2] = if next_page.is_some() { "VieDiz" } else { "" };
            labels[3] = "EdtDiz";
        }
        (true, false, false) => labels[11] = "Mode",
        _ => {}
    }
    labels
}

/// The panel labels for the modifiers held (FAR's `Mapping` in `KeyBar::DisplayObject`).
pub fn panel_labels(held: Modifiers) -> &'static Labels {
    match (held.ctrl, held.alt, held.shift) {
        (false, false, false) => &PANELS,
        (false, false, true) => &PANELS_SHIFT,
        (false, true, false) => &PANELS_ALT,
        (true, false, false) => &PANELS_CTRL,
        (true, false, true) => &PANELS_CTRL_SHIFT,
        (false, true, true) => &PANELS_ALT_SHIFT,
        (true, true, _) => &EMPTY,
    }
}

/// Draws the bar: each key is its number (Keybar.Num) and its label (Keybar.Text) padded to the key's
/// width, with one background column between keys. Widths are FAR's: at least six columns per label,
/// extra space spread evenly (Bresenham-style boundaries). Returns where each key ends, from the
/// bar's left (FAR's `m_KeyBoundaries`, for the mouse).
pub fn draw(frame: &mut Frame, area: Rect, labels: &[&str; KEY_COUNT]) -> Vec<u16> {
    let buffer = frame.buffer_mut();
    let total = usize::from(area.width);
    let blank = " ".repeat(total);
    // Whatever is left after the last key is Keybar.Text, as in FAR.
    buffer.set_stringn(area.x, area.y, &blank, total, theme::key_label());
    let mut x = usize::from(area.x);
    let right = usize::from(area.right());
    let widths = key_widths(total);
    let ends = widths
        .iter()
        .scan(0usize, |end, width| {
            *end += width;
            Some(to_u16(*end))
        })
        .collect();
    for (index, width) in widths.into_iter().enumerate() {
        if x >= right {
            break;
        }
        let number = (index + 1).to_string();
        buffer.set_stringn(to_u16(x), area.y, &number, right - x, theme::key_number());
        x += number.len();
        let label_width = width - number.len() - usize::from(index + 1 < KEY_COUNT);
        if x < right {
            let text = format!("{:<label_width$}", cut_right(labels[index], label_width));
            buffer.set_stringn(to_u16(x), area.y, &text, right - x, theme::key_label());
        }
        x += label_width + usize::from(index + 1 < KEY_COUNT);
    }
    ends
}

/// The width of each key, its background column included (the last key has none).
fn key_widths(total: usize) -> Vec<usize> {
    let min_full = (1 + MIN_LABEL_WIDTH + 1) * KEY_COUNT + (KEY_COUNT - 9) - 1;
    let mut extra = total.saturating_sub(min_full);
    let mut widths = Vec::with_capacity(KEY_COUNT);
    let mut previous = 0;
    for index in 0..KEY_COUNT {
        let extra_digit = usize::from(index + 1 > 9);
        let mut boundary = previous + 1 + extra_digit + MIN_LABEL_WIDTH + 1;
        if extra > 0 {
            let even = (index + 1) * total / KEY_COUNT;
            if even > boundary {
                let added = (even - boundary).min(extra);
                extra -= added;
                boundary += added;
            }
        }
        widths.push(boundary - previous);
        previous = boundary;
    }
    widths
}

fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(width: u16, labels: &Labels) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
        terminal
            .draw(|frame| {
                draw(frame, frame.area(), labels);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..width).map(|x| buffer[(x, 0)].symbol()).collect()
    }

    #[test]
    fn the_keys_end_where_the_next_number_begins() {
        let mut terminal = Terminal::new(TestBackend::new(100, 1)).unwrap();
        let mut ends = Vec::new();
        terminal
            .draw(|frame| ends = draw(frame, frame.area(), &PANELS))
            .unwrap();
        assert_eq!(ends.len(), KEY_COUNT);
        let buffer = terminal.backend().buffer();
        for (index, &end) in ends[..KEY_COUNT - 1].iter().enumerate() {
            let number = (index + 2).to_string();
            assert_eq!(buffer[(end, 0)].symbol(), &number[..1], "key {}", index + 2);
        }
        assert!(ends[KEY_COUNT - 1] >= 100, "the last key takes the rest");
    }

    #[test]
    fn modifiers_choose_far_label_sets() {
        let held = |shift, ctrl, alt| Modifiers { shift, ctrl, alt };
        assert_eq!(panel_labels(held(false, false, false))[1], "UserMn");
        assert_eq!(panel_labels(held(true, false, false))[8], "Save");
        assert_eq!(panel_labels(held(false, false, true))[6], "Find");
        assert_eq!(panel_labels(held(false, true, false))[2], "Name");
        assert_eq!(panel_labels(held(true, true, false))[2], "View");
        assert_eq!(panel_labels(held(true, false, true))[8], "ConfPl");
        assert_eq!(panel_labels(held(false, true, true))[0], "");
    }

    #[test]
    fn the_quick_view_s_labels_follow_what_it_shows() {
        let none = Modifiers::default();
        let file = quick_view_labels(none, Some("OEM"));
        assert_eq!((file[1], file[3], file[7]), ("Unwrap", "Hex", "OEM"));
        let folder = quick_view_labels(none, None);
        assert_eq!((folder[1], folder[3], folder[7]), ("UserMn", "", ""));
        let shift = Modifiers {
            shift: true,
            ..none
        };
        assert_eq!(quick_view_labels(shift, Some("ANSI"))[1], "WWrap");
        assert_eq!(quick_view_labels(shift, None)[1], "");
        let alt = Modifiers { alt: true, ..none };
        assert_eq!(quick_view_labels(alt, None)[7], "Histry");
        assert_eq!(quick_view_labels(alt, Some("ANSI"))[7], "Goto");
    }

    #[test]
    fn the_info_panel_s_labels_name_its_description_keys() {
        let none = Modifiers::default();
        let file = info_labels(none, Some("OEM"));
        assert_eq!((file[1], file[2], file[3]), ("Unwrap", "VieDiz", "EdtDiz"));
        let absent = info_labels(none, None);
        assert_eq!((absent[1], absent[2], absent[3]), ("UserMn", "", "EdtDiz"));
        let ctrl = Modifiers { ctrl: true, ..none };
        assert_eq!(info_labels(ctrl, None)[11], "Mode");
    }

    #[test]
    fn minimum_width_bar_matches_far() {
        // FAR's minimum full bar: 12 keys of number + 6 + background, 3 extra digits, no last background.
        let bar = render(98, &PANELS);
        assert!(bar.starts_with("1Help   2UserMn 3View   4Edit   "), "{bar}");
        assert!(bar.ends_with("11Plugin 12Screen"), "{bar}");
    }

    #[test]
    fn wide_bars_spread_space_evenly() {
        let bar = render(120, &PANELS);
        assert_eq!(bar.chars().count(), 120);
        assert_eq!(bar.find("2UserMn"), Some(10), "{bar}");
        assert!(bar.trim_end().ends_with("12Screen"));
        assert_eq!(key_widths(120).iter().sum::<usize>(), 120);
    }

    #[test]
    fn the_editors_ctrl_f4_names_the_mode_it_switches_to() {
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        assert_eq!(editor_labels(ctrl, "OEM", false)[3], "Hex");
        assert_eq!(editor_labels(ctrl, "OEM", true)[3], "Text");
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        assert_eq!(
            editor_labels(alt, "OEM", false)[3],
            "",
            "Alt+F4 is the console's"
        );
    }
}
