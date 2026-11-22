//! FAR's info panel (`far/infolist.cpp`, `InfoList::DisplayObject`): a double box titled "Information",
//! the computer and the user, then the sections Disk, Memory and Power Status, each under a separator
//! with its title in the middle and `[-]` / `[+]` at the left (folded by Ctrl+F12). A line's name is at
//! the left, its value at the right in the info colour. Last, for a folder on a disk, the folder's
//! description: its file's name as the section's title and its contents to the panel's bottom (FAR's
//! `DizView`), or FAR's "Folder description file is absent"; beside a plugin's panel its lines (FAR's
//! "Plugin" section, `InfoLines`).

use crate::app::text_cut::truncate_left;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, BorderType};

use super::drive_menu::cd_name;
use super::size_format::{SizeStyle, format_size};
use super::theme;
use crate::app::{Description, InfoPanel, Section};
use crate::platform;
use crate::platform::locale::NumberFormat;
use crate::platform::system::{
    AcLine, AccessLevel, Amount, Charge, Elevation, MemoryFacts, PowerFacts, SystemFacts,
};
use crate::vfs::local::{DiskInfo, DriveType};

/// One row inside the box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Row {
    /// A name and its value.
    Line(String, String),
    /// A section's separator with its title; `true` when the section is unfolded.
    Title(String, bool),
    /// The description file's contents from here to the panel's bottom.
    Description,
    /// A plugin's separator with its title in the middle (FAR's `IPLFLAGS_SEPARATOR`), no `[-]`.
    Separator(String),
}

pub fn draw(frame: &mut Frame, area: Rect, info: &InfoPanel, focused: bool) {
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(theme::panel_border())
        .style(theme::panel());
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);
    if area.width < 4 || area.height < 3 {
        return;
    }
    let buffer = frame.buffer_mut();
    let title = " Information ";
    let title_width = width_of(title);
    let x = area.x + area.width.saturating_sub(title_width) / 2;
    set_text(buffer, x, area.y, title, theme::panel_title(focused));
    let time_separator = platform::locale::date_format().time_separator;
    let rows = rows(info, platform::locale::number_format(), time_separator);
    // Clicks find the description only where it is drawn this time.
    if let Some(description) = &info.description {
        description.area.set((0, 0, 0, 0));
    }
    for (row, y) in rows.iter().zip(area.y + 1..area.bottom() - 1) {
        if *row == Row::Description
            && let Some(description) = &info.description
        {
            let text = Rect::new(area.x + 1, y, area.width - 2, area.bottom() - 1 - y);
            description
                .area
                .set((text.x, text.y, text.width, text.height));
            super::quick_view::draw_text(buffer, text, &description.text);
            break;
        }
        draw_row(buffer, area, y, row);
    }
}

fn draw_row(buffer: &mut Buffer, area: Rect, y: u16, row: &Row) {
    let inner = usize::from(area.width - 2);
    match row {
        Row::Title(text, shown) => {
            draw_separator(buffer, area, y, text, 0);
            let mark = if *shown { "[-]" } else { "[+]" };
            set_text(buffer, area.x + 1, y, mark, theme::panel());
        }
        // FAR's `ShowPluginDescription`: centred a column to the left of the sections' titles.
        Row::Separator(text) => draw_separator(buffer, area, y, text, 1),
        Row::Description => {}
        Row::Line(name, value) => {
            // FAR's `PrintText` at two columns in, `PrintInfo` ending a column before the frame.
            let name: String = name.chars().take(inner.saturating_sub(1)).collect();
            set_text(buffer, area.x + 2, y, &name, theme::panel());
            let after_name = 2 + name.chars().count();
            let room = inner.saturating_sub(after_name + 1);
            let value = truncate_left(value, room);
            if value.is_empty() {
                return;
            }
            let x = area.right() - 2 - width_of(&value);
            if usize::from(x - area.x) > after_name {
                set_text(buffer, x, y, &value, theme::panel_info());
            }
        }
    }
}

/// FAR's `DrawSeparator` across the box with " `text` " in its middle (none for an empty one), `left`
/// columns to the left of it.
fn draw_separator(buffer: &mut Buffer, area: Rect, y: u16, text: &str, left: u16) {
    let inner = usize::from(area.width - 2);
    let line = format!("╟{}╢", "─".repeat(inner));
    set_text(buffer, area.x, y, &line, theme::panel_border());
    if text.is_empty() {
        return;
    }
    let title = truncate_left(&format!(" {text} "), inner.saturating_sub(2));
    let x = area.x + area.width.saturating_sub(left + width_of(&title)) / 2;
    set_text(buffer, x, y, &title, theme::panel());
}

/// What the panel shows, row by row.
pub(super) fn rows(info: &InfoPanel, numbers: NumberFormat, time_separator: char) -> Vec<Row> {
    let Some(read) = &info.info else {
        return Vec::new();
    };
    let mut rows = system_rows(&read.system);
    let sections = info.sections;
    let disk_shown = sections.shows(Section::Disk);
    let title = if disk_shown {
        disk_title(&read.disk)
    } else {
        "Disk".to_string()
    };
    rows.push(Row::Title(title, disk_shown));
    if disk_shown {
        rows.extend(disk_rows(&read.disk, numbers));
    }
    let memory_shown = sections.shows(Section::Memory);
    rows.push(Row::Title("Memory".to_string(), memory_shown));
    if let (true, Some(memory)) = (memory_shown, &read.system.memory) {
        rows.extend(memory_rows(memory, numbers));
    }
    let power_shown = sections.shows(Section::Power);
    rows.push(Row::Title("Power Status".to_string(), power_shown));
    if let (true, Some(power)) = (power_shown, &read.system.power) {
        rows.extend(power_rows(power, time_separator));
    }
    if info.describes {
        rows.extend(description_rows(info));
    } else if let Some(lines) = &info.plugin_lines {
        rows.extend(plugin_rows(info.sections.shows(Section::Plugin), lines));
    }
    rows
}

/// FAR's "Plugin" section beside a plugin's panel: its `InfoLines`, a separator's title in its middle.
fn plugin_rows(shown: bool, lines: &[crate::vfs::InfoLine]) -> Vec<Row> {
    let mut rows = vec![Row::Title("Plugin".to_string(), shown)];
    if shown {
        rows.extend(lines.iter().map(|line| match line.separator {
            true => Row::Separator(line.text.clone()),
            false => Row::Line(line.text.clone(), line.data.clone()),
        }));
    }
    rows
}

/// FAR's description section: titled by its file's name while shown, its contents or "absent".
fn description_rows(info: &InfoPanel) -> Vec<Row> {
    let shown = info.sections.shows(Section::Description);
    let file: Option<&Description> = info.description.as_ref().filter(|_| shown);
    let title = file
        .and_then(|description| description.path.file_name())
        .map_or_else(
            || "Description".to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
    let mut rows = vec![Row::Title(title, shown)];
    match (shown, file) {
        (false, _) => {}
        (true, Some(_)) => rows.push(Row::Description),
        (true, None) => rows.push(line("Folder description file is absent", "")),
    }
    rows
}

fn line(name: &str, value: impl Into<String>) -> Row {
    Row::Line(name.to_string(), value.into())
}

fn system_rows(system: &SystemFacts) -> Vec<Row> {
    let mut rows = vec![line("Computer name", system.computer_name.clone())];
    if let Some(description) = &system.computer_description {
        rows.push(line("Computer description", description.clone()));
    }
    rows.push(line("User name", system.user_name.clone()));
    if let Some(description) = &system.user_description {
        rows.push(line("User description", description.clone()));
    }
    if let Some(level) = system.access_level {
        let level = match level {
            AccessLevel::Guest => "Guest",
            AccessLevel::User => "User",
            AccessLevel::Administrator => "Administrator",
            AccessLevel::Unknown => "Unknown",
        };
        rows.push(line("Access level", level));
    }
    let elevated = if system.elevated { "Yes" } else { "No" };
    let elevated = match system.elevation {
        Some(Elevation::Limited) => format!("{elevated} (Limited)"),
        Some(Elevation::Full) => format!("{elevated} (Full)"),
        None => elevated.to_string(),
    };
    rows.push(line("Elevated", elevated));
    rows
}

/// FAR's "Fixed disk C:\ (NTFS)", with the SUBST folder, share or image file after it.
fn disk_title(disk: &DiskInfo) -> String {
    let Some(volume) = &disk.volume else {
        return disk.root.clone();
    };
    let kind = disk.kind.map_or("Unknown", kind_name);
    let mut title = format!("{kind} disk {} ({})", disk.root, volume.file_system);
    if let Some(associated) = &disk.associated {
        title.push(' ');
        title.push_str(associated);
    }
    title
}

/// FAR's `MInfo*` names of the kinds.
fn kind_name(kind: DriveType) -> &'static str {
    match kind {
        DriveType::Removable => "Removable",
        DriveType::Fixed => "Fixed",
        DriveType::Network | DriveType::Disconnected => "Network",
        DriveType::Cd(cd) => cd_name(cd),
        DriveType::RamDisk => "RAM",
        DriveType::Subst => "Subst",
        DriveType::Virtual => "Virtual",
    }
}

fn disk_rows(disk: &DiskInfo, numbers: NumberFormat) -> Vec<Row> {
    let mut rows = Vec::new();
    if let Some((total, available)) = disk.size {
        rows.extend(metric_rows("Space", Amount { total, available }, numbers));
    }
    let volume = disk.volume.as_ref();
    let label = volume
        .map(|volume| volume.label.clone())
        .unwrap_or_default();
    rows.push(line("Volume label", label));
    let serial = volume
        .and_then(|volume| volume.serial)
        .map_or_else(String::new, |serial| {
            format!("{:04X}-{:04X}", serial >> 16, serial & 0xFFFF)
        });
    rows.push(line("Serial number", serial));
    // Ours (owner, 2026-11-09): the device the volume is on.
    if let Some(device) = &disk.device {
        let name = [device.vendor.as_str(), device.product.as_str()]
            .iter()
            .filter(|part| !part.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        let text = if device.bus.is_empty() {
            name
        } else {
            format!("{name} ({})", device.bus)
        };
        rows.push(line("Device", text));
    }
    rows
}

fn memory_rows(memory: &MemoryFacts, numbers: NumberFormat) -> Vec<Row> {
    let mut rows = metric_rows("Committable", memory.committable, numbers);
    rows.extend(metric_rows("Addressable", memory.addressable, numbers));
    rows.extend(metric_rows("Physical", memory.physical, numbers));
    if let Some(installed) = memory.installed {
        rows.push(line("Physical, installed", size_text(installed, numbers)));
    }
    rows
}

/// FAR's `PrintMetric`: total, available and used, the last two with their percentage.
fn metric_rows(kind: &str, amount: Amount, numbers: NumberFormat) -> Vec<Row> {
    let percent = if amount.total == 0 {
        0
    } else {
        u128::from(amount.available) * 100 / u128::from(amount.total)
    };
    let used = amount.total.saturating_sub(amount.available);
    vec![
        line(&format!("{kind}, total"), size_text(amount.total, numbers)),
        line(
            &format!("{kind}, available"),
            format!("{percent}%, {}", size_text(amount.available, numbers)),
        ),
        line(
            &format!("{kind}, used"),
            format!(
                "{}%, {}",
                100u128.saturating_sub(percent),
                size_text(used, numbers)
            ),
        ),
    ]
}

/// FAR's `size2str`: the float style with its unit, "B" after it ("465 GB").
pub(super) fn size_text(size: u64, numbers: NumberFormat) -> String {
    let style = SizeStyle {
        float: true,
        show_unit: true,
        ..SizeStyle::default()
    };
    let mut text = format_size(size, 0, false, style, numbers);
    if !text.ends_with('B') {
        text.push('B');
    }
    text
}

fn power_rows(power: &PowerFacts, separator: char) -> Vec<Row> {
    let ac = match power.ac_line {
        AcLine::Offline => "Offline",
        AcLine::Online => "Online",
        AcLine::BackupPower => "Backup power",
        AcLine::Unknown => "Unknown",
    };
    let battery = power.battery.map_or_else(
        || "Unknown".to_string(),
        |battery| {
            let charge = match battery.charge {
                Charge::High => "High",
                Charge::Medium => "Medium",
                Charge::Low => "Low",
                Charge::Critical => "Critical",
                Charge::NoBattery => "No system battery",
                Charge::Unknown => "Unknown",
            };
            let percent = battery
                .percent
                .map_or_else(|| "unknown".to_string(), |percent| format!("{percent}%"));
            let mut text = format!("{charge} ({percent})");
            if battery.charging {
                text.push_str(", charging");
            }
            text
        },
    );
    let time = |seconds: Option<u32>| match seconds {
        // FAR's `duration_to_string_hms`.
        Some(seconds) => format!(
            "{:02}{separator}{:02}{separator}{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        ),
        None if power.ac_line == AcLine::Online => "-".to_string(),
        None => "Unknown".to_string(),
    };
    vec![
        line("AC power status", ac),
        line("Battery charge status", battery),
        line("Battery life time", time(power.life)),
        line("Battery full time", time(power.full_life)),
    ]
}

fn width_of(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

fn set_text(buffer: &mut Buffer, x: u16, y: u16, text: &str, style: ratatui::style::Style) {
    let area = buffer.area;
    if y >= area.bottom() || x >= area.right() {
        return;
    }
    buffer.set_stringn(x, y, text, usize::from(area.right() - x), style);
}

#[cfg(test)]
#[path = "info_panel_tests.rs"]
mod tests;
