//! EMenu (FAR's emenu, `far-source/plugins/emenu`): Explorer's context menu for the panel's selected files
//! and folders (the one under the cursor; on `..` the folder itself), or for one of This PC's items, as a
//! text menu in the program or as Windows shows it. From the plugin commands (F11, its main menu), the
//! command line (`rclk:`, `rclk_txt:`, `rclk_gui:`, `rclk_cmd:<verb>`, `rclk_item:<text>`, files after it
//! or the panel's) and Apps on a panel or a drive (`rclk_apps:`, FAR's built-in call of EMenu: the text
//! menu unless the GUI one is configured). The shell's objects through the SDK's `system::Shell`.

#[cfg(windows)]
mod menu;
mod options;
mod params;

#[cfg(windows)]
pub use menu::{line, panel_targets};
pub use options::{Extra, Gui, Options, SAVE, TITLE};
pub use params::{Target, expand, names, one_folder, title};

use nc_plugin_sdk::{Error, Host, Info, KeyAnswer, Message, Plugin, Which, export_plugin};

/// How a menu ended (FAR's `EDoMenu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Cancelled,
    Back,
    DifferentFolders,
    CannotShow,
    CannotInvoke,
    /// Esc while the shell did not answer.
    Stopped,
}

/// A command looked for by its verb or by its item's text (`rclk_cmd:`, `rclk_item:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auto {
    Verb,
    Item,
}

/// The command line's prefixes: FAR's five, the program's Apps key and its right click (`X Y` of the
/// screen's cell first).
pub const PREFIXES: [&str; 7] = [
    "rclk",
    "rclk_txt",
    "rclk_gui",
    "rclk_cmd",
    "rclk_item",
    "rclk_apps",
    "rclk_mouse",
];

/// FAR's messages for each end (`OpenPlugin`): none for a cancel or a command done.
pub fn message_for(outcome: Outcome) -> Option<&'static str> {
    Some(match outcome {
        Outcome::DifferentFolders => {
            "Can't show the menu because files come from different folders."
        }
        Outcome::CannotShow => "Can't show the menu.",
        Outcome::CannotInvoke => "Can't invoke menu command.",
        Outcome::Stopped => "Stopped: the shell did not answer.",
        Outcome::Done | Outcome::Cancelled | Outcome::Back => return None,
    })
}

/// The configuration dialog: true when saved.
pub fn configure(host: &Host, options: &mut Options) -> bool {
    let closed = host.dialog_with(&options.dialog(), |event, controls| {
        Options::handle(&event, controls)
    });
    let Some(new) = Options::from_dialog(&closed) else {
        return false;
    };
    *options = new;
    new.save(host);
    true
}

/// The menu's kind and how a command line names its command, for `prefix` (FAR's `DoMenu`: `rclk_txt`,
/// `rclk_cmd` and `rclk_item` the text menu, `rclk_gui` the GUI one; Apps the text menu where FAR's
/// `[?]` would ask, a right click too).
pub fn kind_for(prefix: &str, configured: Gui) -> Option<(Gui, Option<Auto>)> {
    Some(match prefix.to_ascii_lowercase().as_str() {
        "rclk" => (configured, None),
        "rclk_txt" => (Gui::Text, None),
        "rclk_gui" => (Gui::Gui, None),
        "rclk_cmd" => (Gui::Text, Some(Auto::Verb)),
        "rclk_item" => (Gui::Text, Some(Auto::Item)),
        "rclk_apps" | "rclk_mouse" if configured == Gui::Ask => (Gui::Text, None),
        "rclk_apps" | "rclk_mouse" => (configured, None),
        _ => return None,
    })
}

/// A right click's `X Y` at the start of `text` (the program's `rclk_mouse:`), and the rest.
pub fn split_click(text: &str) -> (Option<(u16, u16)>, &str) {
    let mut words = text.trim_start().splitn(3, ' ');
    let number = |word: Option<&str>| word.and_then(|word| word.parse::<u16>().ok());
    match (number(words.next()), number(words.next())) {
        (Some(x), Some(y)) => (Some((x, y)), words.next().unwrap_or_default()),
        _ => (None, text),
    }
}

pub struct EMenu {
    host: Host,
    options: Options,
    /// The shell's thread, kept for the plugin's life as FAR's OLE thread (a command's objects may work on
    /// after it returns); a new one only after one was let go.
    #[cfg(windows)]
    shell: Option<nc_plugin_sdk::system::Shell>,
}

impl EMenu {
    /// One call: the shell's thread for it, the menus, then FAR's end.
    #[cfg(windows)]
    fn call(
        &mut self,
        (gui, click): (Gui, Option<(u16, u16)>),
        what: impl FnOnce(&mut menu::Run) -> Outcome,
    ) {
        use nc_plugin_sdk::system::Shell;
        if self.shell.as_ref().is_none_or(Shell::abandoned) {
            self.shell = Shell::start().ok();
        }
        let outcome = match self.shell.as_mut() {
            Some(shell) => {
                let mut run = menu::Run {
                    host: &self.host,
                    options: &mut self.options,
                    gui,
                    click,
                    shell,
                };
                what(&mut run)
            }
            None => Outcome::CannotShow,
        };
        self.finish(outcome);
    }

    /// FAR's end of a call: its message unless silent; after a command "Press OK to continue" when
    /// configured and the panel read again without its selection.
    fn finish(&self, outcome: Outcome) {
        let say = |lines: &[&str], warning: bool| {
            if self.options.silent {
                return;
            }
            let message = Message::new(TITLE, lines).buttons(&["OK"]);
            let message = if warning { message.warning() } else { message };
            let _ = self.host.show(&message);
        };
        if let Some(text) = message_for(outcome) {
            say(&[text], true);
        }
        if outcome != Outcome::Done {
            return;
        }
        if self.options.wait {
            say(&["Press \"OK\" to continue..."], false);
        }
        if self.options.clear_selection {
            let _ = self.host.reread(Which::Active, false);
        }
    }
}

impl Plugin for EMenu {
    fn new(host: Host) -> Self {
        let options = Options::load(&host);
        Self {
            host,
            options,
            #[cfg(windows)]
            shell: None,
        }
    }

    fn info(&self) -> Info {
        Info {
            title: TITLE.to_string(),
            description: "Explorer's context menu for the files and folders of the panel"
                .to_string(),
            author: "nova-commander".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<String> {
        vec![TITLE.to_string()]
    }

    fn run_command(&mut self, _index: usize) -> Result<Option<String>, String> {
        #[cfg(windows)]
        self.call((self.options.gui, None), |run| run.main_menu());
        #[cfg(not(windows))]
        self.finish(Outcome::CannotShow);
        Ok(None)
    }

    fn config_items(&self) -> Vec<String> {
        vec![TITLE.to_string()]
    }

    fn configure(&mut self, _index: usize) -> bool {
        configure(&self.host, &mut self.options)
    }

    fn prefixes(&self) -> Vec<String> {
        PREFIXES.iter().map(ToString::to_string).collect()
    }

    fn open_command(
        &mut self,
        prefix: &str,
        command: &str,
        _folder: Option<&std::path::Path>,
    ) -> Result<KeyAnswer, Error> {
        if let Some((gui, auto)) = kind_for(prefix, self.options.gui) {
            let (click, command) = match prefix.eq_ignore_ascii_case("rclk_mouse") {
                true => split_click(command),
                false => (None, command),
            };
            let text = expand(command);
            #[cfg(windows)]
            self.call((gui, click), |run| run.for_files(Some((&text, auto))));
            #[cfg(not(windows))]
            {
                let _ = (gui, auto, text, click);
                self.finish(Outcome::CannotShow);
            }
        }
        Ok(KeyAnswer::default())
    }
}

export_plugin!(EMenu);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_prefix_has_its_menu_kind() {
        assert_eq!(kind_for("rclk", Gui::Ask), Some((Gui::Ask, None)));
        assert_eq!(kind_for("RCLK_TXT", Gui::Gui), Some((Gui::Text, None)));
        assert_eq!(kind_for("rclk_gui", Gui::Text), Some((Gui::Gui, None)));
        assert_eq!(
            kind_for("rclk_cmd", Gui::Gui),
            Some((Gui::Text, Some(Auto::Verb)))
        );
        assert_eq!(
            kind_for("rclk_item", Gui::Ask),
            Some((Gui::Text, Some(Auto::Item)))
        );
        assert_eq!(
            kind_for("rclk_apps", Gui::Ask),
            Some((Gui::Text, None)),
            "Apps: text"
        );
        assert_eq!(kind_for("rclk_apps", Gui::Gui), Some((Gui::Gui, None)));
        assert_eq!(kind_for("rclk_mouse", Gui::Ask), Some((Gui::Text, None)));
        assert_eq!(kind_for("other", Gui::Ask), None);
    }

    #[test]
    fn a_right_clicks_cell_comes_first() {
        assert_eq!(split_click("12 5"), (Some((12, 5)), ""));
        assert_eq!(split_click(r"3 4 C:\"), (Some((3, 4)), r"C:\"));
        assert_eq!(split_click("a.txt"), (None, "a.txt"));
        assert_eq!(split_click("7"), (None, "7"));
    }

    #[test]
    fn fars_messages_for_each_end() {
        assert_eq!(
            message_for(Outcome::CannotShow),
            Some("Can't show the menu.")
        );
        assert_eq!(
            message_for(Outcome::CannotInvoke),
            Some("Can't invoke menu command.")
        );
        assert!(
            message_for(Outcome::DifferentFolders).is_some_and(|text| text.contains("different"))
        );
        assert!(message_for(Outcome::Stopped).is_some());
        for quiet in [Outcome::Done, Outcome::Cancelled, Outcome::Back] {
            assert_eq!(message_for(quiet), None);
        }
    }
}
