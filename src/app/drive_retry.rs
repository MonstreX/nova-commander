//! A drive chosen in the drive menu that cannot be read: FAR's "Cannot read the disk in drive X:" with
//! Retry and Cancel (`ChooseDisk`'s `FarChDir` loop, `diskmenu.cpp`). The panel stays where it was;
//! Retry tries the drive again, or the one whose letter was typed over it.

use std::io;

use super::App;
use super::command::Side;
use super::dialog::{Dialog, FormAction};
use super::form::{Control, Form, Item};
use super::line_input::LineInput;
use crate::platform;
use crate::vfs::VfsPath;

/// FAR's `MChangeDriveCannotReadDisk`.
const CANNOT_READ: &str = "Cannot read the disk in drive";
/// The dialog's width and the width of its error lines.
const WIDTH: u16 = 56;
const TEXT_WIDTH: usize = 46;
/// The letter's edit line (a drive with a letter).
const LETTER: usize = 1;

/// The folder a drive is entered at, as FAR's `FarChDir("D:")` then `FarChDir("D:\")` find it: `target`
/// when it can be opened, else the drive's root; the error of the root when neither can. Not on the UI
/// thread: a drive may take its time to answer.
pub(super) fn reachable(target: VfsPath, root: &str) -> io::Result<VfsPath> {
    // FAR's `SetCurrentDirectory`: the folder is there and can be gone into (listing it may be denied).
    let opens = |path: &VfsPath| match path.as_local() {
        Some(dir) => std::fs::metadata(dir).and_then(|found| match found.is_dir() {
            true => Ok(()),
            false => Err(std::io::Error::from(std::io::ErrorKind::NotADirectory)),
        }),
        None => Ok(()),
    };
    if opens(&target).is_ok() {
        return Ok(target);
    }
    let root = VfsPath::local(root);
    opens(&root).map(|()| root)
}

impl App {
    /// FAR's dialog: the drive's letter (to type another over it) or its root, the error, Retry and
    /// Cancel.
    pub(super) fn ask_drive_retry(&mut self, side: Side, root: String, error: &io::Error) {
        let letter = letter_of(&root);
        let mut items = match letter {
            Some(letter) => {
                let label = CANNOT_READ.chars().count();
                let x = u16::try_from(label).unwrap_or(0) + 6;
                vec![
                    Item::new(5, 2, Control::Text(CANNOT_READ.to_string())),
                    Item::new(
                        x,
                        2,
                        Control::Edit(LineInput::with_text(&letter.to_string())),
                    )
                    .wide(1),
                    Item::new(x + 1, 2, Control::Text(":".to_string())),
                ]
            }
            None => vec![Item::new(
                5,
                2,
                Control::Text(format!("{CANNOT_READ} {root}")),
            )],
        };
        items.push(Item::new(0, 3, Control::Separator));
        let lines = super::plugins::message::wrapped(&error.to_string(), TEXT_WIDTH);
        let mut y = 4;
        for line in lines {
            items.push(Item::new(5, y, Control::Text(line)));
            y += 1;
        }
        items.push(Item::new(0, y, Control::Separator));
        for (label, default) in [("&Retry", true), ("&Cancel", false)] {
            let button = Control::Button {
                label: label.to_string(),
                default,
            };
            items.push(Item::new(0, y + 1, button));
        }
        self.show_dialog(Dialog::Form {
            form: Form::new("Error", WIDTH, y + 3, items).warning(),
            action: FormAction::DriveRetry { side, root },
        });
    }

    /// Retry: the drive again, or the one of the letter typed; Cancel leaves the panel as it is.
    pub(super) fn finish_drive_retry(
        &mut self,
        form: &Form,
        button: Option<usize>,
        (side, root): (Side, String),
    ) {
        let retry = (form.items.iter().enumerate())
            .find(|(_, item)| matches!(&item.control, Control::Button { default: true, .. }))
            .map(|(at, _)| at);
        if button.is_none() || button != retry {
            return;
        }
        let root = match letter_of(&root) {
            Some(_) => match form.edit_text(LETTER).chars().next() {
                Some(letter) if letter.is_ascii_alphabetic() => {
                    format!("{}:\\", letter.to_ascii_uppercase())
                }
                _ => root,
            },
            None => root,
        };
        let kind = platform::volumes::drive_kind(&root);
        let letter = letter_of(&root).is_some();
        self.go_to_drive(side, root, letter, kind);
    }
}

/// The letter of a drive's root (`D:\`).
fn letter_of(root: &str) -> Option<char> {
    let mut chars = root.chars();
    let letter = chars.next().filter(char::is_ascii_alphabetic)?;
    (chars.next() == Some(':')).then_some(letter)
}

#[cfg(test)]
#[path = "drive_retry_tests.rs"]
mod tests;
