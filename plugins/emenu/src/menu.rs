//! EMenu's menus (FAR's emenu `DoMenu`, `SelectDrive`, `MenuForPanelOrCmdLine`, `ShowTextMenu`,
//! `CFarMenu::Show`): the main menu (the panel's items or This PC's), the choice of the GUI or the text
//! menu, the text menu with its submenus (Right or Enter into one, Left, BS or Esc back), a command found by
//! its verb or its text, then run: "rename" as Shift+F6, "delete" as F8 when so configured, any other by
//! its object. Every call to the shell waits with Esc (ADR-0019).

use std::io;
use std::path::Path;

use nc_plugin_sdk::system::{GuiAt, Shell, ShellItem, Strings, keys_held, mouse_cell};
use nc_plugin_sdk::{Host, Menu, MenuItem, Which};

use crate::options::{Extra, Gui, Options, TITLE};
use crate::params::{self, Target};
use crate::{Auto, Outcome};

/// How a shown menu closed (FAR's `CFarMenu::Show`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    Item(usize),
    Back,
    Cancel,
}

/// What a text menu gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Picked {
    Command(u32),
    Back,
    Cancel,
}

/// FAR's menus' break keys: into a submenu, back, an item chosen, the configuration.
const KEYS: [&str; 9] = [
    "Right",
    "Left",
    "Ctrl+PgDn",
    "Ctrl+PgUp",
    "BS",
    "Space",
    "Shift+Enter",
    "Shift+Space",
    "Alt+Shift+F9",
];
/// FAR's `CFarMenu::MAX_HEIGHT`.
const MOST_LINES: u32 = 45;
/// The mark after an item with a submenu.
const ARROW: &str = "  \u{25BA}";

fn failed(error: &io::Error) -> Outcome {
    match error.kind() {
        io::ErrorKind::Interrupted => Outcome::Stopped,
        _ => Outcome::CannotShow,
    }
}

/// One call of the plugin's: its options, the menu's kind for it, the shell.
pub struct Run<'a> {
    pub host: &'a Host,
    pub options: &'a mut Options,
    pub gui: Gui,
    /// The screen's cell a right click was at (FAR's `CALL_RIGHTCLICK`): every menu at the mouse.
    pub click: Option<(u16, u16)>,
    pub shell: &'a mut Shell,
}

/// Esc in the program, as a shell call waits.
fn stopper(host: Host) -> impl Fn() -> bool {
    move || host.cancelled()
}

impl Run<'_> {
    /// The menus at the mouse: configured so, or for a right click.
    fn at_mouse(&self) -> bool {
        self.click.is_some() || self.options.at_mouse
    }

    /// Where a text menu shows (FAR's `CFarMenu::Show`): at the mouse's cell, else at the click's; none:
    /// in the middle.
    fn place(&self) -> Option<(u16, u16)> {
        if !self.at_mouse() {
            return None;
        }
        let screen = self.host.screen();
        (screen.and_then(|screen| mouse_cell(screen.window, screen.columns, screen.rows)))
            .or(self.click)
    }

    /// A menu shown as FAR's `CFarMenu::Show`: Right and Ctrl+PgDn only into a submenu, Left, BS and
    /// Ctrl+PgUp back, Space and Shift+Enter as Enter, Alt+Shift+F9 the configuration.
    fn show(&mut self, title: &str, items: &[MenuItem], submenus: &[bool], first: usize) -> Shown {
        let mut selected = first;
        loop {
            let mut menu = Menu::new(title, items.to_vec())
                .selected(selected)
                .break_keys(&KEYS)
                .wrap()
                .max_height(MOST_LINES);
            if let Some((x, y)) = self.place() {
                menu = menu.at(x, y);
            }
            let Some(chosen) = self.host.menu(&menu) else {
                return Shown::Cancel;
            };
            let Some(item) = chosen.item else {
                return Shown::Cancel;
            };
            selected = item;
            match chosen.key {
                None | Some(5..=7) => return Shown::Item(item),
                Some(0 | 2) if submenus.get(item) == Some(&true) => return Shown::Item(item),
                Some(1 | 3 | 4) => return Shown::Back,
                Some(8) => {
                    crate::configure(self.host, self.options);
                }
                _ => {}
            }
        }
    }

    /// FAR's main menu: the context menu of the panel's items or of one of This PC's.
    pub fn main_menu(&mut self) -> Outcome {
        let items = [
            MenuItem::new("&Context menu"),
            MenuItem::new("&My Computer"),
        ];
        let mut selected = 0;
        loop {
            let outcome = match self.show(TITLE, &items, &[false, false], selected) {
                Shown::Item(0) => {
                    selected = 0;
                    self.for_files(None)
                }
                Shown::Item(_) => {
                    selected = 1;
                    self.for_computer()
                }
                Shown::Back | Shown::Cancel => return Outcome::Cancelled,
            };
            if outcome != Outcome::Back {
                return outcome;
            }
        }
    }

    /// This PC's items, then the context menu of the one chosen (FAR's `SelectDrive`).
    fn for_computer(&mut self) -> Outcome {
        let stop = stopper(*self.host);
        let computer = self.shell.computer(&stop);
        let (title, names) = match computer {
            Ok(found) => found,
            Err(error) => return failed(&error),
        };
        let items: Vec<MenuItem> = names.iter().map(|name| MenuItem::new(name)).collect();
        let index = match self.show(&title, &items, &[], 0) {
            Shown::Item(index) => index,
            Shown::Back => return Outcome::Back,
            Shown::Cancel => return Outcome::Cancelled,
        };
        let stop = stopper(*self.host);
        if let Err(error) = self.shell.menu_for_computer(index, &stop) {
            return failed(&error);
        }
        let name = names.get(index).cloned().unwrap_or_default();
        let title = format!("{}\\", name.trim_end_matches('\\'));
        self.menu(&title, None)
    }

    /// The context menu of the files a command line names (`params`, the first the command when
    /// `auto`), else of the panel's (FAR's `MenuForPanelOrCmdLine`).
    pub fn for_files(&mut self, params: Option<(&str, Option<Auto>)>) -> Outcome {
        let mut command = None;
        let mut targets = Vec::new();
        if let Some((text, auto)) = params {
            let mut names = params::names(text);
            if auto.is_some() && !names.is_empty() {
                command = auto.map(|auto| (names.remove(0), auto));
            }
            targets = self.named(&names);
        }
        if targets.is_empty() {
            targets = match panel_targets(self.host) {
                Some(targets) => targets,
                None => return Outcome::CannotShow,
            };
        }
        if !params::one_folder(&targets) {
            return Outcome::DifferentFolders;
        }
        let paths: Vec<_> = targets.iter().map(|target| target.path.clone()).collect();
        let stop = stopper(*self.host);
        if let Err(error) = self.shell.menu_for_paths(&paths, &stop) {
            return failed(&error);
        }
        self.menu(&params::title(&targets), command)
    }

    /// The names of a command line as targets, in the active panel's folder when they have none.
    fn named(&self, names: &[String]) -> Vec<Target> {
        let current = self
            .host
            .panel(Which::Active)
            .map(|panel| match panel.plugin {
                Some(_) => panel.disk_folder,
                None => panel.folder,
            });
        let current = current.unwrap_or_default();
        (names.iter())
            .map(|name| {
                let path = params::whole(name, Path::new(&current));
                let path = params::drive_root(name).map_or(path, Into::into);
                let folder = path.is_dir();
                Target { path, folder }
            })
            .collect()
    }

    /// The open context menu: its command found (`command`) or chosen in the GUI or the text menu, then
    /// run (FAR's `DoMenu` for the files).
    fn menu(&mut self, title: &str, command: Option<(String, Auto)>) -> Outcome {
        let id = match command {
            Some((command, auto)) => match self.find(&command, auto) {
                Ok(Some(id)) => id,
                Ok(None) => return Outcome::CannotInvoke,
                Err(outcome) => return outcome,
            },
            None => match self.choose(title) {
                Ok(id) => id,
                Err(outcome) => return outcome,
            },
        };
        self.run_command(id)
    }

    /// The first top item not disabled whose verb or text is `command` (but for `&` and case).
    fn find(&mut self, command: &str, auto: Auto) -> Result<Option<u32>, Outcome> {
        let stop = stopper(*self.host);
        let strings = Strings {
            verb: auto == Auto::Verb,
            help: false,
        };
        let items = self
            .shell
            .items(&[], strings, &stop)
            .map_err(|error| failed(&error))?;
        let found = (items.iter())
            .filter(|item| !item.disabled && !item.separator)
            .find(|item| {
                let said = match auto {
                    Auto::Verb => item.verb.as_deref(),
                    Auto::Item => Some(item.text.as_str()),
                };
                said.is_some_and(|said| params::same_but_hotkeys(said, command))
            });
        Ok(found.map(|item| item.id).filter(|&id| id != 0))
    }

    /// The command chosen: the GUI or the text menu as configured, or as chosen in FAR's menu of the two.
    fn choose(&mut self, title: &str) -> Result<u32, Outcome> {
        let kinds = [
            MenuItem::new("&1. GUI menu"),
            MenuItem::new("&2. Text menu"),
        ];
        let mut selected = 0;
        loop {
            let gui = match self.gui {
                Gui::Gui => true,
                Gui::Text => false,
                Gui::Ask => match self.show(title, &kinds, &[], selected) {
                    Shown::Item(item) => {
                        selected = item;
                        item == 0
                    }
                    Shown::Back => return Err(Outcome::Back),
                    Shown::Cancel => return Err(Outcome::Cancelled),
                },
            };
            let picked = match gui {
                true => self.gui_menu()?,
                false => self.text_menu(&mut Vec::new(), title)?,
            };
            match picked {
                Picked::Command(id) => return Ok(id),
                Picked::Cancel => return Err(Outcome::Cancelled),
                Picked::Back => {}
            }
        }
    }

    /// The menu as Windows shows it, at the mouse or in the window's centre.
    fn gui_menu(&mut self) -> Result<Picked, Outcome> {
        let at = match self.at_mouse() {
            true => GuiAt::Mouse,
            false => GuiAt::Centre(self.host.screen().map_or(0, |screen| screen.window)),
        };
        let stop = stopper(*self.host);
        match self.shell.track(at, &stop) {
            Ok(Some(id)) => Ok(Picked::Command(id)),
            Ok(None) => Ok(Picked::Cancel),
            Err(error) => Err(failed(&error)),
        }
    }

    /// The text menu of the submenu at `at` (FAR's `ShowTextMenu`): a submenu opened in place; Esc or
    /// back there shows this one again.
    fn text_menu(&mut self, at: &mut Vec<usize>, title: &str) -> Result<Picked, Outcome> {
        let stop = stopper(*self.host);
        let strings = Strings {
            verb: self.options.extra == Extra::Verb,
            help: self.options.extra == Extra::Help,
        };
        let items = self
            .shell
            .items(at, strings, &stop)
            .map_err(|error| failed(&error))?;
        if items.is_empty() {
            return Err(Outcome::CannotShow);
        }
        let lines = with_arrows(
            items.iter().map(|item| line(item, self.options)).collect(),
            &items,
        );
        let submenus: Vec<bool> = items.iter().map(|item| item.submenu).collect();
        let mut selected = 0;
        loop {
            let index = match self.show(title, &lines, &submenus, selected) {
                Shown::Item(index) => index,
                Shown::Back => return Ok(Picked::Back),
                Shown::Cancel => return Ok(Picked::Cancel),
            };
            selected = index;
            let Some(item) = items.get(index) else {
                continue;
            };
            if item.submenu {
                at.push(index);
                let picked = self.text_menu(at, title)?;
                at.pop();
                if let Picked::Command(_) = picked {
                    return Ok(picked);
                }
            } else if item.id != 0 {
                return Ok(Picked::Command(item.id));
            }
        }
    }

    /// The command `id` run: "rename" as Shift+F6, "delete" as F8 (Shift+Del, Alt+Del) when so
    /// configured, done by the program after the call; any other by its object.
    fn run_command(&mut self, id: u32) -> Outcome {
        let stop = stopper(*self.host);
        let verb = match self.shell.verb(id, &stop) {
            Ok(verb) => verb.unwrap_or_default(),
            Err(error) => return failed(&error),
        };
        if verb == "rename" {
            self.host.post_keys("Shift+F6");
            return Outcome::Cancelled;
        }
        if self.options.delete_with_f8 && verb == "delete" {
            let keys = match keys_held() {
                (true, _) => "Shift+Del",
                (_, true) => "Alt+Del",
                _ => "F8",
            };
            self.host.post_keys(keys);
            return Outcome::Cancelled;
        }
        let stop = stopper(*self.host);
        match self.shell.invoke(id, &stop) {
            // FAR does not look at the answer: some commands fail after a normal end (Esc in theirs).
            Ok(()) => Outcome::Done,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Outcome::Stopped,
            Err(_) => Outcome::Done,
        }
    }
}

/// The panel's items the menu is for (FAR's `GetFilesFromPanel`): the selected ones (the one under the
/// cursor first), else the current one; on `..` or in an empty panel its folder itself.
pub fn panel_targets(host: &Host) -> Option<Vec<Target>> {
    let panel = host.panel(Which::Active)?;
    let folder = match panel.plugin {
        Some(_) => panel.disk_folder.clone(),
        None => panel.folder.clone(),
    };
    let selected = host.selected_items(Which::Active);
    if selected.is_empty() {
        return (!folder.is_empty()).then(|| {
            vec![Target {
                path: folder.into(),
                folder: true,
            }]
        });
    }
    let current =
        (host.panel_items(Which::Active).get(panel.current)).map(|item| item.name.clone());
    let mut targets = Vec::new();
    for item in &selected {
        let path = match (&item.path, &panel.plugin) {
            (Some(path), _) => path.into(),
            (None, None) => Path::new(&folder).join(&item.name),
            (None, Some(_)) => return None,
        };
        targets.push(Target {
            path,
            folder: item.folder,
        });
    }
    if let Some(at) = selected
        .iter()
        .position(|item| Some(&item.name) == current.as_ref())
    {
        targets.swap(0, at);
    }
    Some(targets)
}

/// A text menu's line for `item` (FAR's `ShowTextMenu`): an item its object draws by its help or verb in
/// braces, else "-- owner drawn --"; the help or the verb in brackets after it as configured.
pub fn line(item: &ShellItem, options: &Options) -> MenuItem {
    if item.separator {
        return MenuItem::separator();
    }
    let mut text = item.text.clone();
    if item.owner_drawn {
        let said = match options.extra {
            Extra::Help => None,
            // FAR's help text without its closing dots (`GetAdditionalString`).
            _ => (item.help.as_deref())
                .map(|help| help.trim_end_matches('.').to_string())
                .filter(|help| !help.is_empty())
                .or_else(|| item.verb.clone()),
        };
        text = said.map_or_else(
            || "-- owner drawn --".to_string(),
            |said| format!("{{{}}}", undoubled(&said)),
        );
    }
    let extra = match options.extra {
        Extra::Nothing => None,
        Extra::Help => item.help.as_deref().map(|help| help.trim_end_matches('.')),
        Extra::Verb => item.verb.as_deref(),
    };
    if let Some(extra) = extra.filter(|extra| !extra.is_empty())
        && !(options.different_only && params::same_but_hotkeys(&text, extra))
    {
        text = format!("{text} ({extra})");
    }
    MenuItem::new(&text)
        .checked(item.checked)
        .disabled(item.disabled)
}

/// A verb written twice ("AboutA&bout") as once (FAR's "a little AI": the second half when the halves are
/// the same but for `&`).
fn undoubled(text: &str) -> String {
    let letters: Vec<char> = text.chars().collect();
    if letters.len() <= 3 {
        return text.to_string();
    }
    let half = letters.len() / 2;
    let (left, right): (String, String) = (
        letters[..half].iter().collect(),
        letters[half..].iter().collect(),
    );
    match params::same_but_hotkeys(&left, &right) {
        true => right,
        false => text.to_string(),
    }
}

/// The lines with a submenu's mark after them, all at one column (FAR's `AddArrows`).
fn with_arrows(mut lines: Vec<MenuItem>, items: &[ShellItem]) -> Vec<MenuItem> {
    let shown = |text: &str| text.chars().filter(|&letter| letter != '&').count();
    let widest = lines
        .iter()
        .map(|line| shown(&line.text))
        .max()
        .unwrap_or(0);
    for (line, item) in lines.iter_mut().zip(items) {
        if item.submenu {
            let pad = widest - shown(&line.text);
            line.text = format!("{}{}{ARROW}", line.text, " ".repeat(pad));
        }
    }
    lines
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
