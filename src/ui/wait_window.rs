//! A work's window (`app::waiting`; owner 2026-11-21): laid out as the copy dialog is (`transfer`) -
//! FAR's margins round its frame, what the work is centred, a blank row, the bar as wide as the window
//! (FAR's bar with the percent, or a block going to and fro with the time taken), a blank row, Esc's
//! line centred.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Clear};

use super::{theme, vmenu};
use crate::app::{WaitBar, WaitWindow};

/// FAR's `DlgW` of a progress window: its width at least.
const WIDTH: usize = 76;
/// The columns round the items: the margins, the frame and a column inside it on each side.
const SIDES: usize = 10;
/// What the bar leaves after it: the percent, or the time taken.
const TAIL: usize = 5;
/// The busy block's width.
const BUSY_BLOCK: usize = 12;

pub fn draw(frame: &mut Frame, area: Rect, window: &WaitWindow) {
    let texts = window.lines.iter().chain(&window.tail);
    let longest = texts.map(|line| line.chars().count()).max().unwrap_or(0);
    let width = (longest + SIDES).max(WIDTH);
    let bar_rows = if window.bar.is_some() { 3 } else { 0 };
    let height = window.lines.len() + bar_rows + window.tail.len() + 4;
    let width = u16::try_from(width).unwrap_or(u16::MAX).min(area.width);
    let height = u16::try_from(height).unwrap_or(u16::MAX).min(area.height);
    let rect = super::dialog_place::placed(area, width, height);
    let style = theme::dialog(false);
    frame.render_widget(Clear, rect);
    frame.render_widget(Block::new().style(style), rect);
    let inside = Rect::new(
        rect.x + 3,
        rect.y + 1,
        rect.width.saturating_sub(6),
        rect.height.saturating_sub(2),
    );
    let block = Block::bordered()
        .border_set(theme::look().dialog_frame.set())
        .style(style);
    frame.render_widget(block, inside);
    vmenu::draw_shadow(frame.buffer_mut(), rect, area);
    let canvas = Rect::new(
        rect.x + 5,
        rect.y + 2,
        rect.width.saturating_sub(10),
        rect.height.saturating_sub(4),
    );
    write_rows(frame.buffer_mut(), canvas, window, style);
}

/// The lines, the bar between blank rows, Esc's line, from the canvas's top.
fn write_rows(buffer: &mut Buffer, canvas: Rect, window: &WaitWindow, style: Style) {
    let mut y = canvas.y;
    let bottom = canvas.y + canvas.height;
    let centred = |buffer: &mut Buffer, y: u16, line: &str| {
        if y < bottom {
            let shown = line.chars().count().min(usize::from(canvas.width));
            let left = (usize::from(canvas.width) - shown) / 2;
            let x = canvas.x + u16::try_from(left).unwrap_or(0);
            buffer.set_stringn(x, y, line, usize::from(canvas.width), style);
        }
    };
    for line in &window.lines {
        centred(buffer, y, line);
        y += 1;
    }
    if let Some(bar) = window.bar {
        if y + 1 < bottom {
            let text = bar_text(bar, usize::from(canvas.width));
            buffer.set_string(canvas.x, y + 1, text, style);
        }
        y += 3;
    }
    for line in &window.tail {
        centred(buffer, y, line);
        y += 1;
    }
}

/// The bar `width` columns wide: FAR's `make_progressbar` (`█` done, `░` to go, ` 35%`), or the busy
/// block at its place with the time taken (` 1:05`).
fn bar_text(bar: WaitBar, width: usize) -> String {
    let size = width.saturating_sub(TAIL);
    match bar {
        WaitBar::Percent(percent) => {
            let done = usize::try_from(percent.min(100)).unwrap_or(0) * size / 100;
            format!(
                "{}{}{:>4}%",
                "█".repeat(done),
                "░".repeat(size - done),
                percent
            )
        }
        WaitBar::Busy(elapsed) => {
            let block = BUSY_BLOCK.min(size);
            let at = super::transfer::busy_position(size - block, elapsed);
            let seconds = elapsed.as_secs();
            let time = format!("{}:{:02}", seconds / 60, seconds % 60);
            format!(
                "{}{}{} {time:>4}",
                "░".repeat(at),
                "█".repeat(block),
                "░".repeat(size - block - at)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::time::Duration;

    /// The screen's rows as text.
    fn rows(window: &WaitWindow) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        terminal
            .draw(|frame| draw(frame, frame.area(), window))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..20)
            .map(|y| (0..100).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    /// Owner 2026-11-21: laid out as the copy dialog - the text centred, the bar as wide as the window
    /// between blank rows, Esc's line centred.
    #[test]
    fn the_window_is_laid_out_as_the_copy_dialog() {
        let window = WaitWindow {
            lines: vec!["Saving the file big.log".to_string()],
            bar: Some(WaitBar::Percent(50)),
            tail: vec!["Esc: stop waiting".to_string()],
        };
        let screen = rows(&window);
        let row = |text: &str| screen.iter().position(|row| row.contains(text)).unwrap();
        let (text, esc) = (row("Saving the file"), row("Esc: stop"));
        let bar = row("50%");
        assert_eq!((bar - text, esc - bar), (2, 2), "blank rows round the bar");
        assert!(screen[text + 1].trim_matches([' ', '║']).is_empty());
        let middle = |line: &str, text: &str| {
            let at = line.find(text).unwrap();
            let left = line[..at].chars().count();
            let right = line[at + text.len()..].chars().count();
            left.abs_diff(right) <= 1
        };
        assert!(
            middle(&screen[text], "Saving the file big.log"),
            "{}",
            screen[text]
        );
        assert!(middle(&screen[esc], "Esc: stop waiting"), "{}", screen[esc]);
        let bar_cells = screen[bar]
            .chars()
            .filter(|ch| matches!(ch, '█' | '░'))
            .count();
        assert_eq!(bar_cells, WIDTH - SIDES - TAIL, "the bar fills the window");
    }

    #[test]
    fn the_bar_is_as_wide_as_the_window() {
        let full = bar_text(WaitBar::Percent(50), 66);
        assert_eq!(full.chars().count(), 66);
        assert!(
            full.starts_with(&"█".repeat(30)) && full.ends_with("  50%"),
            "{full}"
        );
        let busy = bar_text(WaitBar::Busy(Duration::from_secs(65)), 66);
        assert_eq!(busy.chars().count(), 66);
        assert!(busy.ends_with(" 1:05"), "{busy}");
        assert!(busy.contains(&"█".repeat(BUSY_BLOCK)));
    }
}
