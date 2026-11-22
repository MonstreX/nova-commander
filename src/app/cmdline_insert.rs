//! Putting names and paths into the command line from the panels, as FAR does (`far/filelist.cpp`,
//! `MakePathForUI` in `far/panelmix.cpp`):
//! - Ctrl+Enter / Ctrl+J the name under the cursor, Ctrl+Shift+Enter the one in the passive panel;
//! - Ctrl+F its full name, Ctrl+Alt+F its network (UNC) name;
//! - Ctrl+[ / Ctrl+] the left / right panel's folder, Ctrl+Shift+[ / ] the active / passive one's;
//!   with Ctrl+Alt (left / right) or Alt+Shift (active / passive) the network name of the folder.
//!
//! Ctrl+\ (to the root folder) is here too: FAR handles it next to these keys.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::name_cases::NameCases;
use super::{App, Side};

/// Which panel a folder path comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    Fixed(Side),
    Active,
    Passive,
}

/// What to insert for the entry under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NameKind {
    Name,
    Full,
    Network,
}

/// Handles the key if it is one of the insertion keys (or Ctrl+\); returns whether it was.
pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    let ctrl = KeyModifiers::CONTROL;
    let shift = KeyModifiers::SHIFT;
    let alt = KeyModifiers::ALT;
    match (key.code, key.modifiers) {
        (KeyCode::Enter | KeyCode::Char('j'), m) if m == ctrl => {
            insert_current_name(app, NameKind::Name);
        }
        (KeyCode::Enter, m) if m == ctrl | shift => insert_passive_name(app),
        (KeyCode::Char('f'), m) if m == ctrl => insert_current_name(app, NameKind::Full),
        (KeyCode::Char('f'), m) if m == ctrl | alt => {
            insert_current_name(app, NameKind::Network);
        }
        (KeyCode::Char('\\'), m) if m == ctrl => go_root(app),
        (KeyCode::Char(bracket @ ('[' | ']')), m) => {
            let Some((source, network)) = folder_source(bracket, m) else {
                return false;
            };
            insert_folder(app, source, network);
        }
        _ => return false,
    }
    true
}

/// The panel and the kind of name for a bracket key, `None` for other modifiers.
pub(super) fn folder_source(bracket: char, modifiers: KeyModifiers) -> Option<(Source, bool)> {
    let left = bracket == '[';
    let ctrl = KeyModifiers::CONTROL;
    let shift = KeyModifiers::SHIFT;
    let alt = KeyModifiers::ALT;
    let side = if left { Side::Left } else { Side::Right };
    let pair = if left {
        Source::Active
    } else {
        Source::Passive
    };
    match modifiers {
        m if m == ctrl => Some((Source::Fixed(side), false)),
        m if m == ctrl | shift => Some((pair, false)),
        m if m == ctrl | alt => Some((Source::Fixed(side), true)),
        m if m == alt | shift => Some((pair, true)),
        _ => None,
    }
}

/// The panel a bracket key names.
pub(super) fn side_of(app: &App, source: Source) -> Side {
    match source {
        Source::Fixed(side) => side,
        Source::Active => app.active,
        Source::Passive => app.active.other(),
    }
}

/// Gives `put` the text `make` makes. `network`: `make` asks for network names, which may wait out the
/// network's timeout, so it runs on a worker with the keys waiting (ADR-0012 3c).
pub(super) fn put_text(
    app: &mut App,
    network: bool,
    make: impl FnOnce() -> String + Send + 'static,
    put: fn(&mut App, String),
) {
    if !network {
        let text = make();
        return put(app, text);
    }
    app.wait_for(Vec::new(), move || {
        let text = make();
        Box::new(move |app: &mut App| put(app, text))
    });
    app.label_waiting("Reading the network names…");
}

fn into_command_line(app: &mut App, text: String) {
    app.command_line.insert_str(&text);
}

fn insert_folder(app: &mut App, source: Source, network: bool) {
    let side = side_of(app, source);
    let panel = &app.panels[side.index()];
    let Some(path) = panel.path.as_local() else {
        // A provider's panel: its own path (FAR's plugin `CurDir`), network names or not.
        let text = quote_for_shell(&panel.folder_text());
        return into_command_line(app, text);
    };
    let path = path.to_path_buf();
    // FAR's `MakePath`: the folder's real name (with Ctrl+Alt, Alt+Shift) is made by `CreateFullPathName`.
    let cases = if network {
        NameCases::of(app, side)
    } else {
        NameCases::default()
    };
    let make = move || {
        let path = if network { network_name(&path) } else { path };
        quote_for_shell(&with_end_slash(Path::new(
            &cases.apply(&path.display().to_string(), true),
        )))
    };
    put_text(app, network, make, into_command_line);
}

/// Ctrl+Enter, Ctrl+F, Ctrl+Alt+F. On ".." FAR inserts the panel's own folder (with a slash after it
/// for Ctrl+Enter).
fn insert_current_name(app: &mut App, kind: NameKind) {
    let panel = app.active_panel();
    let Some(entry) = panel.current() else {
        return;
    };
    let Some(dir) = panel.path.as_local() else {
        let text = provider_name(panel, entry, kind);
        return into_command_line(app, format!("{} ", quote_for_shell(&text)));
    };
    let (parent, name, dir) = (entry.is_parent, entry.name.clone(), dir.to_path_buf());
    let directory = entry.is_dir || parent;
    let cases = NameCases::of_active(app);
    let make = move || {
        let text = if parent && kind == NameKind::Name {
            // FAR turns Ctrl+Enter on ".." into Ctrl+F: the folder by `CreateFullPathName`.
            with_end_slash(Path::new(&cases.apply(&dir.display().to_string(), true)))
        } else if kind == NameKind::Name {
            name
        } else {
            let path = if parent { dir } else { dir.join(&name) };
            cases.apply(&full_name(&path, kind).display().to_string(), directory)
        };
        format!("{} ", quote_for_shell(&text))
    };
    put_text(app, kind == NameKind::Network, make, into_command_line);
}

/// Ctrl+Enter, Ctrl+F, Ctrl+Alt+F on a provider's panel (FAR's plugin branch): the name, or the
/// provider's own path and the name; ".." is that path (an entry with a file on a disk, TmpPanel's, is
/// that file).
pub(super) fn provider_name(
    panel: &super::Panel,
    entry: &crate::vfs::Entry,
    kind: NameKind,
) -> String {
    if entry.is_parent {
        return panel.folder_text();
    }
    match (&entry.real_path, kind) {
        (_, NameKind::Name) => entry.name.clone(),
        (Some(real), _) if !panel.look.own_get => real.display().to_string(),
        _ => format!("{}{}", panel.folder_text(), entry.name),
    }
}

pub(super) fn full_name(path: &Path, kind: NameKind) -> PathBuf {
    if kind == NameKind::Network {
        network_name(path)
    } else {
        path.to_path_buf()
    }
}

/// Ctrl+Shift+Enter: the name under the cursor of the passive panel, as it is.
fn insert_passive_name(app: &mut App) {
    let Some(entry) = app.passive_panel().current() else {
        return;
    };
    let text = quote_for_shell(&entry.name);
    app.command_line.insert_str(&format!("{text} "));
}

/// Ctrl+\: the root folder of the active panel's drive or share.
fn go_root(app: &mut App) {
    let root = app.active_panel().path.root();
    if let Err(error) = app.active_panel_mut().change_dir(&root, None) {
        app.show_error("Change directory", &error);
    }
}

fn network_name(path: &Path) -> PathBuf {
    crate::platform::volumes::universal_name(path)
}

/// FAR's `AddEndSlash`.
pub(super) fn with_end_slash(path: &Path) -> String {
    let mut text = path.display().to_string();
    if !text.ends_with(['\\', '/']) {
        text.push(std::path::MAIN_SEPARATOR);
    }
    text
}

/// Characters that change the meaning of a command line in cmd or PowerShell (FAR's `QuotedSymbols`
/// plus PowerShell's own).
const SHELL_SPECIAL: &[char] = &[
    ' ', '&', '|', '<', '>', '^', '(', ')', '[', ']', '{', '}', ';', ',', '=', '!', '\'', '+', '`',
    '~', '%', '$', '@', '#', '\u{A0}',
];

/// FAR's `QuoteSpace`: the whole text in quotes when it holds a special character.
pub(super) fn quote_for_shell(name: &str) -> String {
    if name.contains(SHELL_SPECIAL) {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

/// As [`quote_for_shell`], but `%NAME%` keeps its meaning.
pub(super) fn quote_keeping_variables(name: &str) -> String {
    if name
        .chars()
        .any(|ch| ch != '%' && SHELL_SPECIAL.contains(&ch))
    {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> (tempfile::TempDir, App) {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("sub")).unwrap();
        std::fs::write(temp.path().join("my notes.txt"), "hello").unwrap();
        std::fs::write(temp.path().join("plain.txt"), "x").unwrap();
        let app = App::new(temp.path(), 100, 40, &crate::settings::Settings::default());
        (temp, app)
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_key(KeyEvent::new(code, modifiers));
        crate::app::waiting::finish_waiting(app);
    }

    fn select(app: &mut App, name: &str) {
        let panel = app.active_panel_mut();
        panel.cursor = panel.entries.iter().position(|e| e.name == name).unwrap();
    }

    fn slash(path: &Path) -> String {
        with_end_slash(path)
    }

    #[test]
    fn ctrl_enter_and_ctrl_f_insert_the_name_and_the_full_name() {
        let (temp, mut app) = app();
        select(&mut app, "plain.txt");
        press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
        let full = temp.path().join("plain.txt").display().to_string();
        assert_eq!(app.command_line.text(), format!("plain.txt {full} "));
        app.command_line.take();
        select(&mut app, "my notes.txt");
        press(&mut app, KeyCode::Char('j'), KeyModifiers::CONTROL);
        assert_eq!(app.command_line.text(), "\"my notes.txt\" ");
    }

    #[test]
    fn with_the_ctrl_f_rule_the_full_name_takes_the_modes_cases() {
        let (temp, mut app) = app();
        std::fs::write(temp.path().join("NOTES"), "x").unwrap();
        app.active_panel_mut().reload();
        let mut modes = crate::settings::PanelModes::default();
        modes.medium.folder_upper_case = true;
        modes.medium.upper_to_lower_case = true;
        app.panel_modes = crate::app::PanelModes::make(modes);
        app.active_panel_mut()
            .set_view_mode(crate::app::ViewMode::Medium);
        select(&mut app, "NOTES");
        press(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
        let plain = temp.path().join("NOTES").display().to_string();
        assert_eq!(
            app.command_line.text(),
            format!("{plain} "),
            "off: as it is"
        );
        app.command_line.take();
        app.prefs.ctrl_f_rule = true;
        press(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
        let folder = temp.path().display().to_string().to_uppercase();
        assert_eq!(app.command_line.text(), format!("{folder}\\notes "));
        app.command_line.take();
        // Ctrl+Enter on "..": FAR's Ctrl+F there, the folder with the mode's cases and a slash.
        select(&mut app, "..");
        press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
        let parent = temp.path().display().to_string().to_uppercase();
        assert_eq!(
            app.command_line.text(),
            format!("{} ", slash(Path::new(&parent)))
        );
        app.command_line.take();
        // Ctrl+Alt+] takes the right panel's cases (it is in Medium too here: then none with them off).
        app.panels[1].set_view_mode(crate::app::ViewMode::Brief);
        assert_eq!(NameCases::of(&app, Side::Right), NameCases::default());
        select(&mut app, "sub");
        press(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
        assert_eq!(app.command_line.text(), format!("{folder}\\SUB "));
    }

    #[test]
    fn on_the_parent_entry_the_panel_folder_is_inserted() {
        let (temp, mut app) = app();
        app.active_panel_mut().home();
        press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
        let folder = temp.path().display().to_string();
        assert!(
            !folder.contains(SHELL_SPECIAL),
            "the test folder needs no quotes"
        );
        let expected = format!("{} {folder} ", slash(temp.path()));
        assert_eq!(app.command_line.text(), expected);
    }

    #[test]
    fn bracket_keys_insert_panel_folders() {
        let (temp, mut app) = app();
        let sub = temp.path().join("sub");
        app.panels[1]
            .change_dir(&crate::vfs::VfsPath::local(&sub), None)
            .unwrap();
        let ctrl = KeyModifiers::CONTROL;
        let shift = KeyModifiers::SHIFT;
        press(&mut app, KeyCode::Char('['), ctrl);
        assert_eq!(app.command_line.take(), slash(temp.path()));
        press(&mut app, KeyCode::Char(']'), ctrl);
        assert_eq!(app.command_line.take(), slash(&sub));
        app.active = Side::Right;
        press(&mut app, KeyCode::Char('['), ctrl | shift);
        assert_eq!(app.command_line.take(), slash(&sub), "active");
        press(&mut app, KeyCode::Char(']'), ctrl | shift);
        assert_eq!(app.command_line.take(), slash(temp.path()), "passive");
        press(&mut app, KeyCode::Char(']'), KeyModifiers::ALT | shift);
        assert_eq!(
            app.command_line.take(),
            slash(temp.path()),
            "network, local path"
        );
    }

    #[test]
    fn ctrl_shift_enter_inserts_the_passive_panel_name() {
        let (_temp, mut app) = app();
        app.panels[1].end();
        press(
            &mut app,
            KeyCode::Enter,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        assert_eq!(app.command_line.text(), "plain.txt ");
    }

    #[test]
    fn ctrl_backslash_goes_to_the_root() {
        let (temp, mut app) = app();
        press(&mut app, KeyCode::Char('\\'), KeyModifiers::CONTROL);
        let root = temp.path().ancestors().last().unwrap();
        assert_eq!(app.active_panel().path, root);
    }

    #[test]
    fn names_with_shell_metacharacters_are_quoted() {
        assert_eq!(quote_for_shell("a&calc.txt"), "\"a&calc.txt\"");
        assert_eq!(quote_for_shell("x(1).txt"), "\"x(1).txt\"");
        assert_eq!(quote_for_shell("plain.txt"), "plain.txt");
    }
}
