//! A plugin's menu (ADR-0016 5b-1, FAR's `Menu` / `VMenu2` in the middle of the screen): its lines with
//! separators, check marks, grey lines the cursor passes (`MIF_DISABLE`) or stops on without Enter taking
//! them (`MIF_GRAYED`), hidden ones left out with the others keeping their indexes, `&` hotkeys (automatic
//! ones when the plugin asks, from the last line up if it says; `&` shown as it is with
//! `FMENU_SHOWAMPERSAND`), the keys FAR's menus share (round the ends only with `FMENU_WRAPMODE`), and the
//! plugin's break keys that close it as Enter does. Esc and F10 close it unchosen.

use std::cell::Cell;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::vmenu_keys;
use crate::plugin_host::windows::{MenuLine, MenuRequest};

/// FAR's `ScrY - 6` for menus in the middle of the screen.
pub const PLUGIN_MENU_ROWS_LEFT: usize = 7;

#[derive(Debug)]
pub struct PluginMenu {
    pub title: String,
    pub bottom: String,
    pub lines: Vec<MenuLine>,
    pub selected: usize,
    pub top: Cell<usize>,
    pub rows: Cell<usize>,
    /// The plugin's break keys, in its order (a name not read: `None`, never pressed).
    break_keys: Vec<Option<BreakKey>>,
    /// F11's "Plugin commands" (the program's own menu): each line's command and its text.
    pub commands: Option<Vec<((usize, usize), String)>>,
    /// Options > Plugins configuration (the program's own menu): `commands` are the configurations.
    pub config: bool,
    /// The editor's F11 "Plugin commands": `commands` are the plugins' editor items (API 10).
    pub editor: bool,
    /// Ctrl+W's task list (the program's own menu): its windows.
    pub tasks: Option<crate::app::task_windows::TaskWindows>,
    /// The viewer's Shift+F4 "View mode" (the program's own menu): its line is the mode chosen.
    pub viewer_mode: bool,
    /// The viewer's Shift+F8 "Code pages" (the program's own menu, `app::code_page_menu`).
    pub code_pages: bool,
    /// FarColorer's menu or its type menu in the editor (`app::colorer_menu`).
    pub colorer: Option<crate::app::colorer_menu::ColorerMenu>,
    /// The editor's F7 "All" list (the program's own menu, `app::editor_found`).
    pub found_all: Option<Box<crate::app::editor_found::FoundAll>>,
    /// The editor's Ctrl+Space words in the code mode (`app::editor_code`): the characters typed.
    pub completion: Option<usize>,
    /// Each line's characters drawn marked (FAR's item annotations), none: plain.
    pub marks: Vec<Option<(usize, usize)>>,
    /// Each line's index among the plugin's items (hidden ones left out).
    items: Vec<usize>,
    /// FAR's `FMENU_WRAPMODE`: Up and Down go round the ends.
    wrap: bool,
    /// FAR's `X`, `Y`: where the plugin puts it (none: centred).
    pub x: Option<u16>,
    pub y: Option<u16>,
    /// FAR's `MaxHeight`: the most lines it shows.
    pub max_height: Option<usize>,
}

/// What a key did to a plugin's menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOutcome {
    Stay,
    Esc,
    /// Closed on this line (none in a menu without one to choose), by Enter (`key` none) or a break key.
    Chosen {
        item: Option<usize>,
        key: Option<usize>,
    },
}

impl PluginMenu {
    pub fn new(request: MenuRequest) -> Self {
        let mut all = request.lines;
        if request.show_ampersand {
            for line in &mut all {
                line.text = vmenu_keys::escape(&line.text);
            }
        }
        // FAR's `apiMenuFn`: either flag turns the automatic hotkeys on; `AssignHighlights` goes over every
        // item, hidden ones too, from the last up with `VMENU_REVERSEHIGHLIGHT`.
        if request.auto_hotkeys || request.reverse_auto_hotkeys {
            let mut labels: Vec<String> = all.iter().map(|line| line.text.clone()).collect();
            if request.reverse_auto_hotkeys {
                labels.reverse();
            }
            vmenu_keys::auto_hotkeys(&mut labels);
            if request.reverse_auto_hotkeys {
                labels.reverse();
            }
            // A separator takes its letter too, but shows its title as it is (FAR draws it plain).
            for (line, label) in all.iter_mut().zip(labels) {
                if !line.separator {
                    line.text = label;
                }
            }
        }
        let wanted = request.selected;
        let (items, lines): (Vec<usize>, Vec<MenuLine>) = (all.into_iter().enumerate())
            .filter(|(_, line)| !line.hidden)
            .unzip();
        let skip: Vec<bool> = lines.iter().map(|line| !choosable(line)).collect();
        // FAR's `UpdateItemFlags`: an item that cannot take the cursor is not selected; then the first
        // from the top that can (`SetSelectPos(0, 1)`).
        let selected = match items.iter().position(|index| *index == wanted) {
            Some(line) if !skip[line] => line,
            _ => vmenu_keys::first_from(&skip, 0),
        };
        Self {
            title: request.title,
            bottom: request.bottom,
            lines,
            selected,
            top: Cell::new(0),
            rows: Cell::new(1),
            break_keys: request.break_keys.iter().map(|name| parse(name)).collect(),
            commands: None,
            config: false,
            editor: false,
            tasks: None,
            viewer_mode: false,
            code_pages: false,
            colorer: None,
            found_all: None,
            completion: None,
            marks: Vec::new(),
            items,
            wrap: request.wrap,
            x: request.x,
            y: request.y,
            max_height: request.max_height,
        }
    }

    /// Lines the cursor passes over.
    fn skip(&self) -> Vec<bool> {
        self.lines.iter().map(|line| !choosable(line)).collect()
    }

    /// Whether line `index` can be chosen (the mouse's too).
    pub fn choosable(&self, index: usize) -> bool {
        self.lines.get(index).is_some_and(choosable)
    }

    /// The line under the cursor, when it can be chosen.
    fn current(&self) -> Option<usize> {
        self.choosable(self.selected).then_some(self.selected)
    }

    /// Whether Enter takes line `index` (FAR's `item_can_be_entered`: not grey with `MIF_GRAYED`).
    fn enterable(&self, index: usize) -> bool {
        self.choosable(index) && self.lines.get(index).is_some_and(|line| !line.grayed)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> MenuOutcome {
        if let Some(pressed) = (self.break_keys.iter())
            .position(|wanted| wanted.is_some_and(|wanted| wanted.matches(key)))
        {
            return MenuOutcome::Chosen {
                item: self
                    .current()
                    .and_then(|line| self.items.get(line).copied()),
                key: Some(pressed),
            };
        }
        let plain = key.modifiers.difference(KeyModifiers::SHIFT).is_empty();
        match key.code {
            KeyCode::Esc | KeyCode::F(10) => MenuOutcome::Esc,
            KeyCode::Enter => self.choose(Some(self.selected)),
            KeyCode::Char(ch) if plain || key.modifiers == KeyModifiers::ALT => {
                let hotkey = ch.to_lowercase().next().unwrap_or(ch);
                let line = (self.lines.iter().enumerate()).position(|(index, line)| {
                    self.choosable(index) && vmenu_keys::marked_hotkey(&line.text) == Some(hotkey)
                });
                match line {
                    // FAR: a hotkey chooses its line at once (a grey one only gets the cursor).
                    Some(line) => {
                        self.selected = line;
                        self.choose(Some(line))
                    }
                    None => MenuOutcome::Stay,
                }
            }
            code => {
                self.selected = vmenu_keys::step_in(
                    &self.skip(),
                    self.selected,
                    code,
                    (self.rows.get(), self.wrap),
                );
                MenuOutcome::Stay
            }
        }
    }

    /// Enter on line `line`: nothing on a line Enter does not take or a menu without one; the plugin
    /// hears its item's index.
    fn choose(&self, line: Option<usize>) -> MenuOutcome {
        match line.filter(|line| self.enterable(*line)) {
            Some(line) => MenuOutcome::Chosen {
                item: self.items.get(line).copied(),
                key: None,
            },
            None => MenuOutcome::Stay,
        }
    }
}

fn choosable(line: &MenuLine) -> bool {
    !line.separator && !line.disabled
}

/// A key a plugin names: `Ctrl+`, `Alt+`, `Shift+` before the key ("F4", "Del", "Ins", "Enter", "Tab",
/// "Space", "BS", "Home", "End", "PgUp", "PgDn", "Up", "Down", "Left", "Right", "Apps", a letter or another
/// character).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakKey {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl BreakKey {
    /// The key as typed: a letter with Shift alone is its capital.
    pub(super) fn event(self) -> KeyEvent {
        match self.code {
            KeyCode::Char(ch) if self.modifiers == KeyModifiers::SHIFT && ch.is_alphabetic() => {
                let capital = ch.to_uppercase().next().unwrap_or(ch);
                KeyEvent::new(KeyCode::Char(capital), self.modifiers)
            }
            code => KeyEvent::new(code, self.modifiers),
        }
    }

    pub(super) fn matches(self, key: KeyEvent) -> bool {
        let modifiers =
            key.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        // Alt+Shift+digit by the key, whatever character Shift makes of it (FAR's VK '0'..'9').
        let alt_shift = KeyModifiers::ALT | KeyModifiers::SHIFT;
        if let KeyCode::Char(digit @ '0'..='9') = self.code
            && self.modifiers == alt_shift
        {
            let pressed = crate::app::console_input::alt_shift_digit(&key);
            return pressed
                .and_then(|pressed| u32::try_from(pressed).ok())
                .and_then(|pressed| char::from_digit(pressed, 10))
                == Some(digit);
        }
        normal(self.code, self.modifiers) == normal(key.code, modifiers)
    }

    /// The function key's number and whether `held` are its modifiers (for a key bar's label).
    pub(super) fn function_key(self) -> Option<(u8, KeyModifiers)> {
        match self.code {
            KeyCode::F(number) => Some((number, self.modifiers)),
            _ => None,
        }
    }
}

/// A key as compared: a letter in lower case with Shift for its capital, other characters without Shift
/// (it is in the character already).
fn normal(code: KeyCode, modifiers: KeyModifiers) -> (KeyCode, KeyModifiers) {
    let KeyCode::Char(ch) = code else {
        return (code, modifiers);
    };
    let lower = ch.to_lowercase().next().unwrap_or(ch);
    if !ch.is_alphabetic() {
        return (code, modifiers.difference(KeyModifiers::SHIFT));
    }
    let shift = if lower == ch {
        KeyModifiers::NONE
    } else {
        KeyModifiers::SHIFT
    };
    (KeyCode::Char(lower), modifiers | shift)
}

fn lower(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

/// The key a plugin names; `None` for a name not known.
pub fn parse(name: &str) -> Option<BreakKey> {
    let mut modifiers = KeyModifiers::NONE;
    let mut rest = name.trim();
    while let Some((head, tail)) = rest.split_once('+').filter(|(head, _)| !head.is_empty()) {
        modifiers |= match head.to_ascii_lowercase().as_str() {
            "ctrl" => KeyModifiers::CONTROL,
            "alt" => KeyModifiers::ALT,
            "shift" => KeyModifiers::SHIFT,
            _ => return None,
        };
        rest = tail;
    }
    let code = match key_code(rest)? {
        // With Ctrl or Alt a letter is the key, its case only how it is written ("Ctrl+A").
        KeyCode::Char(ch)
            if ch.is_alphabetic()
                && modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            KeyCode::Char(lower(ch))
        }
        // The console tells Shift+Tab as its own key.
        KeyCode::Tab if modifiers.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
        code => code,
    };
    Some(BreakKey { code, modifiers })
}

/// A key's name as the plugins name keys ("F4", "Ctrl+Enter", "Alt+Shift+F2", "a"), the inverse of
/// [`parse`]: Ctrl, Alt and Shift in that order, a letter with Ctrl or Alt in capitals, a typed character
/// as it is (its Shift in it). `None` for a key without a name.
pub fn key_name(key: KeyEvent) -> Option<String> {
    let held = |modifier: KeyModifiers| key.modifiers.contains(modifier);
    let (ctrl, alt) = (held(KeyModifiers::CONTROL), held(KeyModifiers::ALT));
    let mut shift = held(KeyModifiers::SHIFT);
    let code = match key.code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(ch) if ctrl || alt => ch.to_uppercase().collect(),
        KeyCode::Char(ch) => {
            shift = false;
            ch.to_string()
        }
        KeyCode::F(number) => format!("F{number}"),
        KeyCode::BackTab => {
            shift = true;
            "Tab".to_string()
        }
        code => named_code(code)?.to_string(),
    };
    let mut name = String::new();
    for (on, prefix) in [(ctrl, "Ctrl+"), (alt, "Alt+"), (shift, "Shift+")] {
        if on {
            name.push_str(prefix);
        }
    }
    name.push_str(&code);
    Some(name)
}

fn named_code(code: KeyCode) -> Option<&'static str> {
    Some(match code {
        KeyCode::Enter => "Enter",
        KeyCode::Esc => "Esc",
        KeyCode::Tab => "Tab",
        KeyCode::Backspace => "BS",
        KeyCode::Insert => "Ins",
        KeyCode::Delete => "Del",
        KeyCode::Home => "Home",
        KeyCode::End => "End",
        KeyCode::PageUp => "PgUp",
        KeyCode::PageDown => "PgDn",
        KeyCode::Up => "Up",
        KeyCode::Down => "Down",
        KeyCode::Left => "Left",
        KeyCode::Right => "Right",
        KeyCode::Menu => "Apps",
        _ => return None,
    })
}

fn key_code(name: &str) -> Option<KeyCode> {
    let mut chars = name.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        return Some(KeyCode::Char(ch));
    }
    let lower = name.to_ascii_lowercase();
    if let Some(number) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=24).contains(&number).then_some(KeyCode::F(number));
    }
    Some(match lower.as_str() {
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Char(' '),
        "bs" | "backspace" => KeyCode::Backspace,
        "ins" | "insert" => KeyCode::Insert,
        "del" | "delete" => KeyCode::Delete,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pgup" => KeyCode::PageUp,
        "pgdn" => KeyCode::PageDown,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "apps" => KeyCode::Menu,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
