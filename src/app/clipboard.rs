//! The clipboard the application copies to and pastes from: the system's, or one in memory in tests
//! (tests must not touch the user's clipboard).

use std::io;
use std::path::PathBuf;

pub trait Clipboard {
    /// The text on the clipboard, `None` when it holds none.
    fn text(&mut self) -> io::Result<Option<String>>;
    fn set_text(&mut self, text: &str) -> io::Result<()>;
    /// The editor's vertical block as text marked as one (FAR's `SetClipboardVText`).
    fn set_column_text(&mut self, text: &str) -> io::Result<()>;
    /// Whether the clipboard's text is a vertical block (FAR's `GetClipboardVText`).
    fn is_column(&mut self) -> io::Result<bool>;
    /// Files for Explorer's Paste: copied, or moved when `cut`.
    fn set_files(&mut self, paths: &[PathBuf], cut: bool) -> io::Result<()>;
}

/// The clipboard of a new application: the system's, in tests one in memory.
pub fn for_app() -> Box<dyn Clipboard> {
    if cfg!(test) {
        Box::new(Memory::default())
    } else {
        Box::new(System)
    }
}

struct System;

impl Clipboard for System {
    fn text(&mut self) -> io::Result<Option<String>> {
        crate::platform::clipboard::clipboard_text()
    }

    fn set_text(&mut self, text: &str) -> io::Result<()> {
        crate::platform::clipboard::set_clipboard_text(text)
    }

    fn set_column_text(&mut self, text: &str) -> io::Result<()> {
        crate::platform::clipboard::set_clipboard_column_text(text)
    }

    fn is_column(&mut self) -> io::Result<bool> {
        crate::platform::clipboard::clipboard_is_column()
    }

    fn set_files(&mut self, paths: &[PathBuf], cut: bool) -> io::Result<()> {
        crate::platform::clipboard::set_clipboard_files(paths, cut)
    }
}

/// A clipboard in memory, for tests; clones share their contents, so a test keeps one to look into.
#[derive(Debug, Default, Clone)]
pub struct Memory(std::rc::Rc<std::cell::RefCell<Contents>>);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Contents {
    pub text: Option<String>,
    /// The text is a vertical block.
    pub column: bool,
    pub files: Option<(Vec<PathBuf>, bool)>,
}

#[cfg(test)]
impl Memory {
    pub fn contents(&self) -> Contents {
        self.0.borrow().clone()
    }

    pub fn put_text(&self, text: &str) {
        self.0.borrow_mut().text = Some(text.to_string());
    }
}

impl Memory {
    fn put(&self, text: &str, column: bool) {
        *self.0.borrow_mut() = Contents {
            text: Some(text.to_string()),
            column,
            files: None,
        };
    }
}

impl Clipboard for Memory {
    fn text(&mut self) -> io::Result<Option<String>> {
        Ok(self.0.borrow().text.clone())
    }

    fn set_text(&mut self, text: &str) -> io::Result<()> {
        self.put(text, false);
        Ok(())
    }

    fn set_column_text(&mut self, text: &str) -> io::Result<()> {
        self.put(text, true);
        Ok(())
    }

    fn is_column(&mut self) -> io::Result<bool> {
        Ok(self.0.borrow().column)
    }

    fn set_files(&mut self, paths: &[PathBuf], cut: bool) -> io::Result<()> {
        *self.0.borrow_mut() = Contents {
            text: None,
            column: false,
            files: Some((paths.to_vec(), cut)),
        };
        Ok(())
    }
}
