//! The status row of the background tasks (ADR-0010): while tasks exist, the top row (where FAR's
//! "Always show menu bar" puts the menu bar) shows each one: `Copy 45% [####......]`, `Move queued`,
//! `Copy question!`, each search in the background (`Find 37 [..###.....]`, `Find 37 done`), each
//! command, and F12 for the Screens list. The panels sit one row lower meanwhile. While a task's
//! total is counted it says `Copy …` and the bar's block goes to and fro.

use crate::app::text_cut::truncate_right;
use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Rect;

use super::theme;
use super::transfer::busy_position;
use crate::app::{App, FindResults, TaskStatus, search_label, task_label, task_percent};

/// The mini bar's width.
const BAR: usize = 10;
/// The block going to and fro while a task's total is counted.
const BUSY: usize = 3;
/// Between two tasks.
const SEPARATOR: &str = "  │  ";

fn mini_bar(percent: Option<u64>, moment: Duration) -> String {
    let Some(percent) = percent else {
        let at = busy_position(BAR - BUSY, moment);
        return format!(
            " [{}{}{}]",
            ".".repeat(at),
            "#".repeat(BUSY),
            ".".repeat(BAR - BUSY - at)
        );
    };
    let done = usize::try_from(percent.min(100)).unwrap_or(0) * BAR / 100;
    format!(" [{}{}]", "#".repeat(done), ".".repeat(BAR - done))
}

/// All the parts in `room`: each is cut as little as it takes, short ones leaving their room to long ones.
fn fit_parts(parts: &[String], room: usize) -> String {
    let gaps = SEPARATOR.chars().count() * parts.len().saturating_sub(1);
    let mut left = room.saturating_sub(gaps);
    let mut lengths: Vec<usize> = parts.iter().map(|part| part.chars().count()).collect();
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by_key(|&index| lengths[index]);
    for (done, &index) in order.iter().enumerate() {
        let share = left / (parts.len() - done);
        lengths[index] = lengths[index].min(share);
        left -= lengths[index];
    }
    let cut_parts: Vec<String> = parts
        .iter()
        .zip(&lengths)
        .map(|(part, &length)| truncate_right(part, length))
        .collect();
    // Too narrow even for the separators: what there is room for.
    cut_parts.join(SEPARATOR).chars().take(room).collect()
}

/// What a search in the background says: its block moves while it goes on.
fn search_part(results: &FindResults) -> String {
    let mut part = search_label(results);
    if !results.finalized {
        part.push_str(&mini_bar(None, results.elapsed()));
    }
    part
}

/// What a command in the background says.
fn command_part(line: &str, ended: bool) -> String {
    if ended {
        format!("[done] {line}")
    } else {
        line.to_string()
    }
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let buffer = frame.buffer_mut();
    let style = theme::menu_bar();
    let width = usize::from(area.width);
    buffer.set_string(area.x, area.y, " ".repeat(width), style);
    let mut parts = Vec::new();
    for task in app.tasks.iter() {
        let mut part = task_label(task);
        if task.status() == TaskStatus::Running {
            part.push_str(&mini_bar(task_percent(task), task.elapsed()));
        }
        parts.push(part);
    }
    parts.extend(app.background_searches().iter().map(search_part));
    parts.extend(
        app.detached
            .iter()
            .map(|command| command_part(&command.line, command.ended)),
    );
    let hint = "F12 Screens ";
    let room = width.saturating_sub(hint.len() + 2);
    buffer.set_string(area.x + 1, area.y, fit_parts(&parts, room), style);
    let hint_x = area
        .right()
        .saturating_sub(u16::try_from(hint.len()).unwrap_or(0));
    buffer.set_string(hint_x, area.y, hint, theme::menu_bar_hotkey());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(items: &[&str]) -> Vec<String> {
        items.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn what_fits_is_shown_whole() {
        assert_eq!(
            fit_parts(&parts(&["Copy 45%", "ping"]), 40),
            "Copy 45%  │  ping"
        );
    }

    #[test]
    fn a_short_part_leaves_its_room_to_a_long_one() {
        let long = "ping -n 30 127.0.0.1 and much more text";
        let shown = fit_parts(&parts(&["Copy", long]), 30);
        assert_eq!(shown.chars().count(), 30);
        assert!(shown.starts_with("Copy  │  ping -n 30"), "{shown}");
        assert!(shown.ends_with('…'), "{shown}");
    }

    #[test]
    fn every_part_keeps_a_share_of_the_room() {
        let long = "x".repeat(50);
        let shown = fit_parts(&parts(&[&long, &long, &long]), 40);
        assert_eq!(shown.chars().count(), 40);
        assert_eq!(shown.matches('│').count(), 2);
        assert_eq!(shown.matches('…').count(), 3);
    }

    #[test]
    fn no_room_shows_nothing_and_breaks_nothing() {
        let shown = fit_parts(&parts(&["Copy", "ping"]), 0);
        assert_eq!(shown, "");
        assert_eq!(fit_parts(&[], 10), "");
    }

    #[test]
    fn a_search_has_its_moving_block_while_it_goes_on() {
        let going = search_part(&FindResults::sample(false));
        assert!(
            going.starts_with("Find 0 [") && going.ends_with(']'),
            "{going}"
        );
        assert_eq!(search_part(&FindResults::sample(true)), "Find 0 done");
    }

    #[test]
    fn a_command_that_ended_says_so_first() {
        assert_eq!(command_part("ping x", false), "ping x");
        assert_eq!(command_part("ping x", true), "[done] ping x");
    }

    #[test]
    fn the_mini_bar_fills_with_a_percent_and_moves_without_one() {
        assert_eq!(mini_bar(Some(50), Duration::ZERO), " [#####.....]");
        assert_eq!(mini_bar(None, Duration::ZERO), " [###.......]");
        assert_eq!(mini_bar(None, Duration::from_millis(100)), " [..###.....]");
    }
}
