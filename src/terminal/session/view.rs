//! What the screen shows of the shell: its rows without the `cd` lines this program typed by itself
//! to keep the shell in the panel's folder. FAR runs each command in the panel's folder and has no
//! shell of its own, so its user screen holds only what the user typed and the commands printed.

use super::Session;

/// Own lines remembered; older ones have long scrolled away.
const OWN_LINES_KEPT: usize = 64;

/// The shell's screen and the rows of it to show.
pub struct ShellView<'a> {
    pub screen: &'a vt100::Screen,
    /// Screen rows to show, top to bottom.
    pub rows: Vec<u16>,
}

impl ShellView<'_> {
    /// The cursor as (place in `rows`, column); `None` while its row is hidden.
    pub fn cursor(&self) -> Option<(u16, u16)> {
        let (row, col) = self.screen.cursor_position();
        let index = self.rows.iter().position(|&shown| shown == row)?;
        Some((u16::try_from(index).ok()?, col))
    }
}

impl Session {
    /// The screen without the program's own `cd` lines (and the blank line cmd prints after a
    /// command). Only finished lines are hidden, never the line being typed.
    pub fn view(&self) -> ShellView<'_> {
        let screen = self.screen();
        let (rows, cols) = screen.size();
        let (cursor_row, _) = screen.cursor_position();
        let texts: Vec<String> = screen.rows(0, cols.get()).collect();
        let width = usize::from(cols.get());
        let mut shown = Vec::new();
        let mut after_own = false;
        let mut row = 0;
        while row < rows.get() {
            let mut end = row;
            while end + 1 < rows.get() && screen.row_wrapped(end) {
                end += 1;
            }
            let line = logical_line(&texts[usize::from(row)..=usize::from(end)], width);
            let finished = end < cursor_row;
            let own = finished && self.is_own_line(&line);
            let blank_after_own = finished && after_own && line.trim().is_empty();
            if !own && !blank_after_own {
                shown.extend(row..=end);
            }
            after_own = own;
            row = end + 1;
        }
        ShellView {
            screen,
            rows: shown,
        }
    }

    pub(super) fn remember_own_line(&mut self, line: String) {
        if self.own_lines.len() == OWN_LINES_KEPT {
            self.own_lines.remove(0);
        }
        self.own_lines.push(line);
    }

    /// A prompt followed by one of the lines this program typed.
    fn is_own_line(&self, line: &str) -> bool {
        let line = line.trim_end();
        self.own_lines.iter().any(|own| {
            line.strip_suffix(own.as_str())
                .is_some_and(|prompt| prompt.trim_end().ends_with('>'))
        })
    }
}

/// Screen rows of one line joined; a wrapped row is full, so what the screen trimmed is put back.
fn logical_line(rows: &[String], width: usize) -> String {
    let mut line = String::new();
    for (index, row) in rows.iter().enumerate() {
        line.push_str(row);
        if index + 1 < rows.len() {
            let padding = width.saturating_sub(row.chars().count());
            line.extend(std::iter::repeat_n(' ', padding));
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use crate::terminal::Shell;
    use crate::terminal::session::Session;
    use crate::terminal::session::tests::{wait_for_text, wait_until_quiet};

    fn shown_text(session: &Session) -> String {
        let view = session.view();
        let (_, cols) = view.screen.size();
        let texts: Vec<String> = view.screen.rows(0, cols.get()).collect();
        let lines: Vec<&str> = view
            .rows
            .iter()
            .map(|&row| texts[usize::from(row)].as_str())
            .collect();
        lines.join("\n")
    }

    #[test]
    fn the_programs_own_cd_lines_are_not_shown() {
        for shell in [
            Shell::default_shell(),
            Shell::system(crate::platform::process::ShellKind::Powershell).unwrap(),
        ] {
            let start = tempfile::tempdir().unwrap();
            let other = tempfile::tempdir().unwrap();
            let mut session = Session::spawn(shell.clone(), start.path(), 30, 200).unwrap();
            assert!(wait_for_text(&mut session, ">"));
            wait_until_quiet(&mut session);

            session
                .enter_dir_and_type(other.path(), b"echo shown-5d2\r")
                .unwrap();
            let other_prompt = format!("{}>", other.path().display());
            let printed = wait_for_text(&mut session, "\nshown-5d2");
            assert!(printed, "{shell:?}:\n{}", session.screen().contents());
            wait_until_quiet(&mut session);

            let all = session.screen().contents();
            assert!(all.trim_end().ends_with(&other_prompt), "{shell:?}:\n{all}");
            let shown = shown_text(&session);
            let own = shell.change_dir(other.path());
            assert!(all.contains(&own), "{shell:?}: the shell got it:\n{all}");
            assert!(!shown.contains(&own), "{shell:?}:\n{shown}");
            assert!(shown.contains("echo shown-5d2"), "{shell:?}:\n{shown}");
            // Above the prompt (below it the screen is empty anyway).
            let above_prompt = shown.trim_end().rsplit_once('\n').unwrap().0;
            assert!(
                !above_prompt.contains("\n\n\n"),
                "{shell:?}: no gap left:\n{shown}"
            );
            let view = session.view();
            let (cursor, _) = view.cursor().unwrap();
            let cursor_row = view.rows[usize::from(cursor)];
            assert_eq!(cursor_row, view.screen.cursor_position().0);
        }
    }
}
