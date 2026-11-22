//! Keeps the console's own 16 colours those of the theme on screen (ADR-0008 §8, FAR's `SetPalette`)
//! and gives the console its colours back when the program ends, a panic included.

use std::io::{self, Stdout, Write};

use super::{ConsolePalette, ConsoleTable, console_palette, restore_sequence};
use crate::platform;

pub struct ConsoleColors<W: Write = Stdout> {
    out: W,
    enabled: bool,
    /// The colours the console had before this program.
    own: Option<ConsoleTable>,
    /// What the console has been told last.
    applied: Option<ConsolePalette>,
}

impl ConsoleColors {
    /// `enabled`: FAR's "Classic palette"; off, the console is left as it is.
    pub fn new(enabled: bool) -> Self {
        Self::with_output(enabled, platform::process::console_colors(), io::stdout())
    }
}

impl<W: Write> ConsoleColors<W> {
    fn with_output(enabled: bool, own: Option<ConsoleTable>, out: W) -> Self {
        Self {
            out,
            enabled,
            own,
            applied: None,
        }
    }

    /// Called after a draw: tells the console when the theme on screen changed its colours.
    pub fn update(&mut self) -> io::Result<()> {
        let wanted = console_palette();
        if !self.enabled || self.applied == Some(wanted) {
            return Ok(());
        }
        self.out
            .write_all(wanted.sequence(self.own.as_ref()).as_bytes())?;
        self.out.flush()?;
        self.applied = Some(wanted);
        Ok(())
    }
}

impl<W: Write> Drop for ConsoleColors<W> {
    fn drop(&mut self) {
        if self.applied.is_some() {
            // Nothing can be done about a console that no longer listens.
            let _ = self
                .out
                .write_all(restore_sequence(self.own.as_ref()).as_bytes());
            let _ = self.out.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Theme, resolve, set_current};
    use super::*;

    fn written(buffer: &[u8]) -> String {
        String::from_utf8_lossy(buffer).into_owned()
    }

    #[test]
    fn the_colours_are_sent_once_and_taken_back_at_the_end() {
        set_current(Theme::far());
        let mut buffer = Vec::new();
        {
            let mut colors = ConsoleColors::with_output(true, None, &mut buffer);
            colors.update().unwrap();
            colors.update().unwrap();
        }
        let text = written(&buffer);
        assert_eq!(text.matches("\x1b]4;4;").count(), 1, "{text:?}");
        assert!(text.ends_with(&restore_sequence(None)), "{text:?}");
    }

    #[test]
    fn the_console_gets_back_the_colours_it_had() {
        set_current(Theme::far());
        let own = [(0x12, 0x34, 0x56); 16];
        let mut buffer = Vec::new();
        {
            let mut colors = ConsoleColors::with_output(true, Some(own), &mut buffer);
            colors.update().unwrap();
        }
        let text = written(&buffer);
        assert!(text.ends_with(&restore_sequence(Some(&own))), "{text:?}");
        assert_eq!(
            text.matches("rgb:12/34/56").count(),
            32,
            "at the start and at the end"
        );
    }

    #[test]
    fn a_panic_gives_the_console_its_colours_back_too() {
        set_current(Theme::far());
        let mut buffer = Vec::new();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut colors = ConsoleColors::with_output(true, None, &mut buffer);
            colors.update().unwrap();
            panic!("a crash while the program runs");
        }));
        assert!(unwound.is_err());
        assert!(written(&buffer).ends_with(&restore_sequence(None)));
    }

    #[test]
    fn another_theme_sends_its_colours() {
        set_current(Theme::far());
        let mut buffer = Vec::new();
        {
            let mut colors = ConsoleColors::with_output(true, None, &mut buffer);
            colors.update().unwrap();
            set_current(resolve("nc5", &|_| None));
            colors.update().unwrap();
        }
        set_current(Theme::far());
        let text = written(&buffer);
        assert!(text.contains("\x1b]4;4;rgb:00/00/80"), "{text:?}");
        assert!(text.contains("\x1b]4;4;rgb:00/00/aa"), "{text:?}");
    }

    #[test]
    fn switched_off_the_console_is_left_alone() {
        set_current(Theme::far());
        let mut buffer = Vec::new();
        {
            let mut colors = ConsoleColors::with_output(false, None, &mut buffer);
            colors.update().unwrap();
        }
        assert!(buffer.is_empty());
    }
}
