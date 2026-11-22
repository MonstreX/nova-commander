//! The command line.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};

use super::theme;
use crate::app::LineInput;

/// The command line "C:\path>text". Returns where the cursor goes.
pub fn draw_command_line(
    frame: &mut Frame,
    area: Rect,
    prompt: &str,
    input: &LineInput,
) -> (Position, crate::app::CommandLineArea) {
    let buffer = frame.buffer_mut();
    buffer.set_style(area, theme::command_line());
    let width = usize::from(area.width);
    let prompt_chars = prompt.chars().count();
    // Long prompts are cut from the left so the end of the path stays visible.
    let shown_prompt: String = if prompt_chars > width / 2 {
        let skip = prompt_chars - width / 2;
        format!("…{}", prompt.chars().skip(skip + 1).collect::<String>())
    } else {
        prompt.to_string()
    };
    let prompt_width = shown_prompt.chars().count();
    let text_width = width.saturating_sub(prompt_width + 1);
    // The column after the text is the cursor's too.
    let skip = input.shown_from(text_width + 1);
    let text: String = input.text().chars().skip(skip).take(text_width).collect();
    let line = format!("{shown_prompt}{text}");
    buffer.set_stringn(area.x, area.y, line, width, theme::command_line());
    let text_area = Rect::new(
        area.x + u16::try_from(prompt_width).unwrap_or(area.width),
        area.y,
        u16::try_from(text_width).unwrap_or(0),
        1,
    );
    mark_selection(
        buffer,
        text_area,
        input,
        skip,
        theme::command_line_selected(),
    );
    let cursor_x = prompt_width + input.cursor() - skip;
    let text_x = area.x + u16::try_from(prompt_width).unwrap_or(area.width);
    let line = crate::app::CommandLineArea {
        x: text_x,
        row: area.y,
        width: area.right().saturating_sub(text_x),
        first: skip,
    };
    let cursor = Position::new(
        area.x + u16::try_from(cursor_x).unwrap_or(area.width),
        area.y,
    );
    (cursor, line)
}

/// Colours the selected characters of `input` shown in the row `area`, which starts with character
/// `skip`.
pub fn mark_selection(
    buffer: &mut ratatui::buffer::Buffer,
    area: Rect,
    input: &LineInput,
    skip: usize,
    style: ratatui::style::Style,
) {
    let Some(selection) = input.selection() else {
        return;
    };
    let start = selection.start.max(skip) - skip;
    let end = selection
        .end
        .saturating_sub(skip)
        .min(usize::from(area.width));
    if start >= end {
        return;
    }
    let to_x = |at: usize| area.x + u16::try_from(at).unwrap_or(area.width);
    buffer.set_style(
        Rect::new(to_x(start), area.y, to_x(end) - to_x(start), 1),
        style,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn the_selected_part_of_the_line_has_its_own_colour() {
        let mut terminal = Terminal::new(TestBackend::new(20, 1)).unwrap();
        let mut input = LineInput::with_text("copy a.txt");
        input.select_word_left();
        terminal
            .draw(|frame| {
                draw_command_line(frame, frame.area(), r"C:\>", &input);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let styled = |x: u16| buffer[(x, 0)].style().bg;
        let selected = theme::command_line_selected().bg;
        assert_eq!(
            buffer[(11, 0)].symbol(),
            "t",
            "'txt' after the prompt and 'copy a.'"
        );
        assert!((11..14).all(|x| styled(x) == selected));
        assert!((4..11).chain(14..20).all(|x| styled(x) != selected));
    }

    #[test]
    fn a_click_on_a_scrolled_line_leaves_the_text_where_it_is() {
        let mut terminal = Terminal::new(TestBackend::new(30, 1)).unwrap();
        let mut input = LineInput::with_text(&"abcdefghij".repeat(4));
        let mut draw = |input: &LineInput| {
            let mut area = None;
            terminal
                .draw(|frame| area = Some(draw_command_line(frame, frame.area(), r"C:\>", input).1))
                .unwrap();
            area.unwrap()
        };
        let area = draw(&input);
        // 30 columns, a 4-column prompt: 25 characters and the cursor's column.
        assert_eq!((area.x, area.first), (4, 15));
        input.place_cursor(area.first + 2);
        let again = draw(&input);
        assert_eq!(again.first, 15, "FAR's LeftPos stays");
        input.place_cursor(3);
        assert_eq!(draw(&input).first, 3, "the cursor left the view");
    }
}
