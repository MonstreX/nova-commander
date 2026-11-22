//! FAR's drive menu (`far/diskmenu.cpp`, `ChangeDiskMenu`): "C: │ fixed │ 465 G │ 120 G", each column
//! as wide as its widest text, five columns into the panel it changes, centred vertically, scrolled when
//! the drives do not fit. The plugins' items under a separator: "1  Temporary panel".

use ratatui::Frame;
use ratatui::layout::Rect;

use super::size_format::{SizeStyle, format_size, format_size_from_unit};
use super::vmenu::{self, Line};
use crate::app::{App, DiskItem, DriveMenu, Side};
use crate::platform;
use crate::platform::locale::NumberFormat;
use crate::platform::volumes::CdType;
use crate::vfs::local::{Drive, DriveColumns, DriveType};

const TITLE: &str = "Change drive";
const KEYS: &str = "Del Shift+Del F3 F4 F9";
/// FAR's `MChangeDriveLabelAbsent`.
const NO_LABEL: &str = "not available";

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    menu: &DriveMenu,
) -> Option<crate::app::MenuArea> {
    let texts = lines(menu, platform::locale::number_format());
    let separator = (!menu.plugins.is_empty()).then_some(menu.drives.len());
    let all: Vec<Line> = (texts.iter().enumerate())
        .map(|(at, text)| Line {
            label: text,
            key: "",
            check: None,
            separator: Some(at) == separator,
            disabled: false,
        })
        .collect();
    let panel = panel_rect(area, app, menu.side);
    // FAR's `MaxHeight`: `ScrY - PanelRect.top - 3`.
    let max_rows = usize::from(area.height).saturating_sub(4);
    if all.is_empty() {
        return None;
    }
    let rows = all.len().min(max_rows.max(1));
    let top = crate::app::keep_in_view(&menu.top, menu.selected, all.len(), rows);
    let shown = &all[top.min(all.len())..(top + rows).min(all.len())];
    let vmenu = vmenu::Menu {
        lines: shown,
        selected: menu.selected - top,
        title: TITLE,
        bottom_title: KEYS,
        scroll: Some((top, all.len())),
    };
    let rect = place(area, panel, menu.side, vmenu.natural_width(), rows);
    // `draw_in_margin` draws nothing smaller.
    if rect.width < 7 || rect.height < 5 {
        return None;
    }
    vmenu::draw_in_margin(frame.buffer_mut(), rect, &vmenu, area);
    let inner = Rect::new(
        rect.x + 2,
        rect.y + 1,
        rect.width.saturating_sub(4),
        rect.height.saturating_sub(2),
    );
    Some(super::menu_area(inner, rect, top))
}

/// The panel the menu belongs to, also while it is hidden.
fn panel_rect(area: Rect, app: &App, side: Side) -> Rect {
    let (left_width, right_width) = crate::app::panel_widths(area.width, app.width_decrement);
    match side {
        Side::Left => Rect::new(area.x, area.y, left_width, area.height),
        Side::Right => Rect::new(area.x + left_width, area.y, right_width, area.height),
    }
}

/// FAR's `ChDisk->SetPosition({ X, -1, 0, 0 })` and `VMenu2::Resize`: the box five columns into the
/// panel (its margin two before), the whole kept on screen, centred vertically. Returns the rectangle
/// with the margin.
fn place(area: Rect, panel: Rect, side: Side, box_width: usize, rows: usize) -> Rect {
    let screen_x = i32::from(area.width) - 1;
    let screen_y = i32::from(area.height) - 1;
    let full_screen_right = side == Side::Right && panel.width == area.width && area.width > 40;
    let box_x = if full_screen_right {
        i32::from(area.width) / 2 + 5
    } else {
        i32::from(panel.x - area.x) + 5
    };
    let width = (i32::try_from(box_width).unwrap_or(i32::MAX) + 4).min(screen_x + 1);
    let height = i32::try_from(rows).unwrap_or(i32::MAX) + 4;
    let mut x = if box_x > 1 { box_x - 2 } else { box_x };
    if x > 0 && x + width > screen_x {
        x -= x + width - screen_x;
    }
    let y = (screen_y + 1 - height) / 2;
    let clamp = |value: i32| u16::try_from(value.max(0)).unwrap_or(u16::MAX);
    Rect::new(
        area.x + clamp(x),
        area.y + clamp(y),
        clamp(width).min(area.width),
        clamp(height).min(area.height),
    )
}

/// The menu's lines, `&` before each drive letter and item hotkey; the separator's empty.
fn lines(menu: &DriveMenu, numbers: NumberFormat) -> Vec<String> {
    let mut lines = drive_lines(menu, numbers);
    if !menu.plugins.is_empty() {
        lines.push(String::new());
    }
    lines.extend(menu.plugins.iter().map(item_line));
    lines
}

/// A plugin's item: "&1  Temporary panel", without a hotkey its label where the others' are.
fn item_line(item: &DiskItem) -> String {
    crate::app::with_hotkey(&vmenu::escape_ampersands(&item.label), item.hotkey)
}

/// The drives' lines.
fn drive_lines(menu: &DriveMenu, numbers: NumberFormat) -> Vec<String> {
    let columns = &menu.columns;
    let cells: Vec<Cells> = menu
        .drives
        .iter()
        .map(|drive| Cells::of(drive, columns, numbers))
        .collect();
    let width = |cell: fn(&Cells) -> &str| {
        cells
            .iter()
            .map(|cells| cell(cells).chars().count())
            .max()
            .unwrap_or(0)
    };
    let widths = [
        width(|c| &c.kind),
        width(|c| &c.label),
        width(|c| &c.file_system),
        width(|c| &c.total),
        width(|c| &c.free),
        width(|c| &c.path),
    ];
    menu.drives
        .iter()
        .zip(&cells)
        .map(|(drive, cells)| line(drive, cells, columns, widths))
        .collect()
}

/// One drive's column texts.
struct Cells {
    kind: String,
    label: String,
    file_system: String,
    total: String,
    free: String,
    path: String,
}

impl Cells {
    fn of(drive: &Drive, columns: &DriveColumns, numbers: NumberFormat) -> Self {
        let label = match &drive.label {
            Some(label) => label.clone(),
            None if columns.label && drive.volume_read => NO_LABEL.to_string(),
            None => String::new(),
        };
        let (total, free) = drive.size.map_or_else(Default::default, |(total, free)| {
            (
                size_text(total, columns, numbers),
                size_text(free, columns, numbers),
            )
        });
        Self {
            kind: drive.drive_type.map_or("", type_name).to_string(),
            label,
            file_system: drive.file_system.clone().unwrap_or_default(),
            total,
            free,
            path: drive.associated_path.clone().unwrap_or_default(),
        }
    }
}

fn line(drive: &Drive, cells: &Cells, columns: &DriveColumns, widths: [usize; 6]) -> String {
    let name = drive
        .letter
        .map_or_else(|| "  ".to_string(), |letter| format!("{letter}:"));
    let mut text = format!("&{name}");
    let mut first = true;
    let mut add = |cell: &str, width: usize, right: bool| {
        text.push_str(if first { " " } else { " │ " });
        first = false;
        if right {
            text.push_str(&format!("{cell:>width$}"));
        } else {
            text.push_str(&format!("{cell:<width$}"));
        }
    };
    if columns.kind {
        add(&cells.kind, widths[0], false);
    }
    if columns.label {
        add(&cells.label, widths[1], false);
    }
    if columns.file_system {
        add(&cells.file_system, widths[2], false);
    }
    if columns.shows_size() {
        add(&cells.total, widths[3], true);
        add(&cells.free, widths[4], true);
    }
    if columns.associated_path && widths[5] > 0 {
        add(&cells.path, 0, false);
    }
    text
}

fn type_name(kind: DriveType) -> &'static str {
    match kind {
        DriveType::Removable => "removable",
        DriveType::Fixed => "fixed",
        DriveType::Network => "network",
        DriveType::Disconnected => "disconnected",
        DriveType::Cd(kind) => cd_name(kind),
        DriveType::RamDisk => "RAM disk",
        DriveType::Subst => "subst",
        DriveType::Virtual => "virtual",
    }
}

/// FAR's `MChangeDriveCDROM` ... `MChangeDriveHDDVDRAM`.
pub(super) fn cd_name(kind: CdType) -> &'static str {
    match kind {
        CdType::CdRom => "CD-ROM",
        CdType::CdRw => "CD-RW",
        CdType::CdRwDvd => "CD-RW/DVD",
        CdType::DvdRom => "DVD-ROM",
        CdType::DvdRw => "DVD-RW",
        CdType::DvdRam => "DVD-RAM",
        CdType::BdRom => "BD-ROM",
        CdType::BdR => "BD-R",
        CdType::BdRe => "BD-RE",
        CdType::HdDvdRom => "HDDVD-ROM",
        CdType::HdDvdR => "HDDVD-R",
        CdType::HdDvdRw => "HDDVD-RW",
        CdType::HdDvdRam => "HDDVD-RAM",
    }
}

/// FAR's `FileSizeToStr(Size, 9, ...)` trimmed: megabytes with grouped digits, or "1.50 G".
fn size_text(size: u64, columns: &DriveColumns, numbers: NumberFormat) -> String {
    let text = if columns.size {
        let style = SizeStyle {
            group_digits: true,
            ..SizeStyle::default()
        };
        format_size_from_unit(size, 9, false, style, numbers, 2)
    } else {
        let style = SizeStyle {
            float: true,
            show_unit: true,
            ..SizeStyle::default()
        };
        format_size(size, 9, false, style, numbers)
    };
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::volumes::DriveKind;

    const NUMBERS: NumberFormat = NumberFormat {
        thousands_separator: ',',
        decimal_separator: '.',
    };
    const GIB: u64 = 1 << 30;

    #[test]
    fn the_mouse_finds_each_drive_where_it_is_drawn_also_scrolled() {
        let temp = tempfile::tempdir().unwrap();
        let app = App::new(temp.path(), 80, 12, &crate::settings::Settings::default());
        let drives = "CDEFGHIJKLMN"
            .chars()
            .map(|letter| drive(letter, DriveType::Fixed, None))
            .collect();
        let mut menu = menu(drives, DriveColumns::default());
        menu.selected = 11;
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 12)).unwrap();
        let mut reported = None;
        terminal
            .draw(|frame| reported = draw(frame, frame.area(), &app, &menu))
            .unwrap();
        let area = reported.unwrap();
        assert!(area.first > 0, "scrolled to the last drive");
        assert_eq!(area.outer.width, area.frame.width + 4, "the margin");
        let buffer = terminal.backend().buffer();
        let frame = area.frame;
        for (offset, y) in (frame.y + 1..frame.y + frame.height - 1).enumerate() {
            let text: String = (frame.x + 1..frame.x + frame.width - 1)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            let letter = menu.drives[area.first + offset].letter.unwrap();
            assert!(text.contains(&format!("{letter}:")), "row {y}: {text}");
        }
    }

    fn drive(letter: char, drive_type: DriveType, size: Option<(u64, u64)>) -> Drive {
        Drive {
            root: format!("{letter}:\\"),
            letter: Some(letter),
            kind: DriveKind::Fixed,
            disconnected: false,
            subst: None,
            image: None,
            drive_type: Some(drive_type),
            label: None,
            file_system: None,
            size,
            associated_path: None,
            volume_read: true,
        }
    }

    fn menu(drives: Vec<Drive>, columns: DriveColumns) -> DriveMenu {
        DriveMenu {
            side: Side::Left,
            drives,
            plugins: Vec::new(),
            selected: 0,
            columns,
            top: std::cell::Cell::new(0),
        }
    }

    #[test]
    fn columns_are_as_wide_as_their_widest_text() {
        let mut subst = drive('S', DriveType::Subst, None);
        subst.associated_path = Some(r"C:\work".to_string());
        let drives = vec![
            drive('C', DriveType::Fixed, Some((500 * GIB, 3 * GIB / 2))),
            subst,
        ];
        let far = DriveColumns {
            label: false,
            ..DriveColumns::default()
        };
        let shown = lines(&menu(drives.clone(), far), NUMBERS);
        assert_eq!(shown[0], "&C: fixed │ 500 G │ 1.50 G");
        assert_eq!(shown[1], "&S: subst │       │       ");

        let columns = DriveColumns {
            size: true,
            size_float: false,
            associated_path: true,
            ..far
        };
        let shown = lines(&menu(drives, columns), NUMBERS);
        assert_eq!(shown[0], "&C: fixed │ 512,000 M │ 1,536 M │ ");
        assert_eq!(shown[1], r"&S: subst │           │         │ C:\work");
        let tiny = DriveColumns {
            size: true,
            size_float: false,
            ..far
        };
        assert_eq!(size_text(100 * 1024, &tiny, NUMBERS), "0 M");
    }

    #[test]
    fn an_optical_drive_is_named_by_what_it_can_do_and_an_image_as_virtual() {
        let drives = vec![
            drive('E', DriveType::Cd(CdType::CdRwDvd), None),
            drive('F', DriveType::Cd(CdType::BdRe), None),
            drive('V', DriveType::Virtual, None),
        ];
        let columns = DriveColumns {
            label: false,
            size_float: false,
            ..DriveColumns::default()
        };
        assert_eq!(
            lines(&menu(drives, columns), NUMBERS),
            ["&E: CD-RW/DVD", "&F: BD-RE    ", "&V: virtual  "]
        );
    }

    #[test]
    fn an_unreadable_label_is_not_available_and_a_volume_without_letter_has_none() {
        let mut volume = drive('X', DriveType::Fixed, None);
        volume.letter = None;
        let columns = DriveColumns {
            label: true,
            kind: false,
            size_float: false,
            ..DriveColumns::default()
        };
        assert_eq!(
            lines(&menu(vec![volume], columns), NUMBERS),
            ["&   not available"]
        );
    }

    fn item(plugin: usize, label: &str, hotkey: Option<char>) -> DiskItem {
        DiskItem {
            plugin,
            index: 0,
            label: label.to_string(),
            hotkey,
        }
    }

    #[test]
    fn the_plugins_items_come_under_a_separator_with_their_hotkeys() {
        let columns = DriveColumns {
            kind: false,
            label: false,
            size_float: false,
            cd: false,
            ..DriveColumns::default()
        };
        let mut menu = menu(vec![drive('C', DriveType::Fixed, None)], columns);
        menu.plugins = vec![item(0, "Temp & more", Some('1')), item(1, "Eleventh", None)];
        assert_eq!(
            lines(&menu, NUMBERS),
            ["&C:", "", "&1  Temp && more", "   Eleventh"]
        );
        menu.selected = 2;
        let temp = tempfile::tempdir().unwrap();
        let app = App::new(temp.path(), 60, 16, &crate::settings::Settings::default());
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 16)).unwrap();
        let mut reported = None;
        terminal
            .draw(|frame| reported = draw(frame, frame.area(), &app, &menu))
            .unwrap();
        let frame = reported.unwrap().frame;
        let buffer = terminal.backend().buffer();
        let row = |line: u16| -> String {
            (frame.x..frame.x + frame.width)
                .map(|x| buffer[(x, frame.y + 1 + line)].symbol())
                .collect()
        };
        assert!(row(0).contains("C:"), "{}", row(0));
        assert!(
            row(1).contains('─') && !row(1).contains(' '),
            "a separator: {}",
            row(1)
        );
        assert!(row(2).contains("1  Temp & more"), "{}", row(2));
        assert!(row(3).contains("   Eleventh"), "{}", row(3));
        // FAR's `AddHotkey`: the hotkey `&` written `&&&`, shown as one marked `&`.
        menu.plugins = vec![item(0, "Amp", Some('&'))];
        assert_eq!(lines(&menu, NUMBERS)[2], "&&&  Amp");
        assert_eq!(
            crate::app::shown_chars(&lines(&menu, NUMBERS)[2])[0],
            ('&', true)
        );
    }

    #[test]
    fn the_menu_opens_five_columns_into_its_panel() {
        let area = Rect::new(0, 0, 100, 30);
        let left = Rect::new(0, 0, 50, 30);
        let right = Rect::new(50, 0, 50, 30);
        // The box at column 5, its margin from column 3; 4 lines centred.
        assert_eq!(
            place(area, left, Side::Left, 30, 4),
            Rect::new(3, 11, 34, 8)
        );
        assert_eq!(place(area, right, Side::Right, 30, 4).x, 53);
        // Too wide for the right: moved left to end a column before the edge.
        assert_eq!(
            place(area, right, Side::Right, 60, 4),
            Rect::new(35, 11, 64, 8)
        );
        // The right panel over the whole screen: from the middle.
        assert_eq!(place(area, area, Side::Right, 30, 4).x, 53);
    }
}
