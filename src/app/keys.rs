//! Keys on the main screen (panels + command line), in the spirit of Norton Commander and FAR.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::App;
use super::ViewMode;
use super::command::{Command, Side};
use super::editing::is_text_input;
use super::sorting;
use super::{cmdline_insert, guest_link, line_edit, panel_clipboard, quick_search, selection};
use crate::settings::history::List;

pub fn handle_panels_key(app: &mut App, key: KeyEvent) {
    if app.handle_completion_key(key) {
        return;
    }
    // The detach key still held after the panels came back.
    if key.kind == KeyEventKind::Repeat && super::detached::is_detach_key(key) {
        return;
    }
    // Shift+movement selection ends when a key arrives without Shift (FAR's `ShiftSelection`).
    // as does Shift+movement selection in the command line (FAR's `FEDITLINE_MARKINGBLOCK`).
    if !key.modifiers.contains(KeyModifiers::SHIFT) {
        app.active_panel_mut().end_shift_selection();
        app.command_line.end_marking();
    }
    if guest_link::is_guest_key(key) {
        app.execute(Command::ToggleGuestShell);
        return;
    }
    // FAR's `FileList::ProcessKey`: a plugin's panel hears its keys first (`SendKeyToPlugin`).
    if app.handle_tree_key(key)
        || info_panel_key(app, key)
        || app.quick_view_key(key)
        || app.description_key(key)
        || app.plugin_panel_key(key)
        || app.handle_shortcut_key(key)
    {
        return;
    }
    // FAR's info panel and tree take the screen's keys and the command line; the file keys do nothing
    // there.
    if !app.active_panel().visible || !app.shows_files(app.active) {
        handle_hidden_panel_key(app, key);
        return;
    }
    if selection::handle_selection_key(app, key)
        || cmdline_insert::handle_key(app, key)
        || quick_search::start(app, key)
    {
        return;
    }
    if let Some(command) = command_for_key(key, app.active) {
        if !(is_delete(command) && key.kind == KeyEventKind::Repeat) {
            app.execute(command);
        }
        return;
    }
    if panel_clipboard::handle_key(app, key) {
        return;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && !is_text_input(key.modifiers) {
        handle_control_key(app, key);
    } else {
        handle_plain_key(app, key);
    }
}

/// Ctrl+L and Ctrl+Q anywhere on the panels; Ctrl+R and Ctrl+F12 on the info panel (FAR's
/// `InfoList::ProcessKey`), Ctrl+R on the quick view.
fn info_panel_key(app: &mut App, key: KeyEvent) -> bool {
    if key.modifiers != KeyModifiers::CONTROL {
        return false;
    }
    match key.code {
        KeyCode::Char('l') => app.toggle_info_panel(),
        KeyCode::Char('q') => app.toggle_quick_view(),
        KeyCode::Char('r') if app.quick_view_is_active() => app.reread_quick_view(),
        KeyCode::Char('r') if app.info_is_active() => app.reread_info_panel(),
        KeyCode::F(12) if app.info_is_active() => app.open_info_sections_menu(),
        _ => return false,
    }
    true
}

/// Keys that map directly to a [`Command`] (function keys and their Alt/Ctrl variants).
fn command_for_key(key: KeyEvent, active: Side) -> Option<Command> {
    let KeyCode::F(number) = key.code else {
        return match (key.code, key.modifiers) {
            (KeyCode::Char('o'), KeyModifiers::CONTROL) => Some(Command::TogglePanels),
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => Some(Command::TogglePassivePanel),
            (KeyCode::Char('b'), KeyModifiers::CONTROL) => Some(Command::ToggleKeyBar),
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => Some(Command::SwapPanels),
            (KeyCode::Char('i'), KeyModifiers::CONTROL) => Some(Command::Filter),
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => Some(Command::Reread(active)),
            (KeyCode::Char(digit), KeyModifiers::CONTROL) => {
                ViewMode::for_key(digit).map(|mode| Command::SetViewMode(active, mode))
            }
            (KeyCode::Up, KeyModifiers::CONTROL) => Some(Command::PanelsShorter),
            (KeyCode::Down, KeyModifiers::CONTROL) => Some(Command::PanelsTaller),
            // FAR's `KEY_APPS`, `KEY_SHIFTAPPS`.
            (KeyCode::Menu, KeyModifiers::NONE | KeyModifiers::SHIFT) => Some(Command::ContextMenu),
            _ => None,
        };
    };
    if key.modifiers.contains(KeyModifiers::ALT) {
        return match number {
            1 => Some(Command::ChangeDrive(Side::Left)),
            2 => Some(Command::ChangeDrive(Side::Right)),
            3 => Some(Command::ViewAlt),
            4 => Some(Command::EditAlt),
            6 => Some(Command::Link),
            7 => Some(Command::FindFile),
            10 => Some(Command::FindFolder),
            8 => Some(Command::History(List::Commands)),
            11 => Some(Command::History(List::Views)),
            12 => Some(Command::History(List::Folders)),
            _ => None,
        };
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match number {
            1 => return Some(Command::TogglePanel(Side::Left)),
            2 => return Some(Command::TogglePanel(Side::Right)),
            12 => return Some(Command::SortMenu(active)),
            _ => {}
        }
        return sorting::mode_for_ctrl_f(number).map(|mode| Command::Sort(active, mode));
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        return match number {
            1 => Some(Command::AddArchive),
            2 => Some(Command::ExtractArchive),
            3 => Some(Command::ArchiveCommands),
            4 => Some(Command::EditNew),
            5 => Some(Command::CopyCurrent),
            6 => Some(Command::MoveCurrent),
            8 => Some(Command::DeleteCurrent),
            9 => Some(Command::SaveSetup),
            10 => Some(Command::OpenLastMenuItem),
            11 => Some(Command::ToggleSortGroups(active)),
            12 => Some(Command::ToggleSelectedFirst(active)),
            _ => None,
        };
    }
    plain_function_key(number)
}

fn plain_function_key(number: u8) -> Option<Command> {
    match number {
        1 => Some(Command::Help),
        2 => Some(Command::UserMenu),
        3 => Some(Command::View),
        4 => Some(Command::Edit),
        5 => Some(Command::Copy),
        6 => Some(Command::Move),
        7 => Some(Command::MakeDir),
        8 => Some(Command::Delete),
        9 => Some(Command::OpenMenu),
        10 => Some(Command::Quit),
        11 => Some(Command::PluginCommands),
        12 => Some(Command::Screens),
        _ => None,
    }
}

/// Ctrl keys of the panels; the rest edit the command line (FAR passes what the panel does not take
/// to the command line's `Edit`: Ctrl+Shift+arrows, Ctrl+Backspace, Ctrl+Y, Ctrl+V, ...).
fn handle_control_key(app: &mut App, key: KeyEvent) {
    if key.modifiers != KeyModifiers::CONTROL {
        edit_command_line(app, key);
        return;
    }
    match key.code {
        KeyCode::Char('c') => copy_block(app),
        KeyCode::Char('e') => app.recall_history(-1),
        KeyCode::Char('x') => app.recall_history(1),
        KeyCode::Char('h') => app.toggle_hidden(),
        KeyCode::Char('m') => app.active_panel_mut().restore_selection(),
        // FAR: file attributes on a shown panel; on a hidden one the line's Select all.
        KeyCode::Char('a') if app.active_panel().visible && app.shows_files(app.active) => {
            app.execute(Command::Attributes);
        }
        KeyCode::Char('g') if app.active_panel().visible && app.shows_files(app.active) => {
            app.execute(Command::Apply);
        }
        KeyCode::Left | KeyCode::Right => ctrl_left_right(app, key.code == KeyCode::Left),
        KeyCode::PageUp => go_parent(app),
        KeyCode::PageDown => enter_or(app, App::open_current_file_alt),
        _ => edit_command_line(app, key),
    }
}

/// Ctrl+C: copies the command line's block as FAR's `Edit` does (a running command gets Ctrl+C
/// itself: keys go to it while it runs).
fn copy_block(app: &mut App) {
    if let Err(error) = line_edit::copy(&app.command_line, app.clipboard.as_mut()) {
        app.show_error("Clipboard", &error);
    }
}

/// FAR: with a typed command, or with both panels hidden, Ctrl+Left/Right move by words; otherwise
/// they move the split between the panels.
fn ctrl_left_right(app: &mut App, left: bool) {
    let split = app.command_line.is_empty() && !app.panels_hidden();
    match (split, left) {
        (false, true) => app.command_line.word_left(),
        (false, false) => app.command_line.word_right(),
        (true, true) => app.execute(Command::SplitLeft),
        (true, false) => app.execute(Command::SplitRight),
    }
}

/// FAR's `FileList::ProcessKey` on a hidden panel: the screen's keys (F9, Ctrl+F1, Ctrl+O, ...) and a
/// few panel keys (insertions, F7, Ctrl+H) work; everything else edits the command line.
fn handle_hidden_panel_key(app: &mut App, key: KeyEvent) {
    // FAR's `InfoList::ProcessKey` takes none of the file keys either: no folder made, no name of the
    // files panel under it put on the line; a view mode (Ctrl+0..9) gives the files back, as FAR's
    // `ChangePanelViewMode`.
    let info = !app.shows_files(app.active);
    if let Some(command) = command_for_key(key, app.active) {
        let file_command = matches!(command, Command::MakeDir | Command::Filter);
        if works_with_hidden_panel(command) && !(info && file_command) {
            app.execute(command);
        }
        return;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let panel_only = matches!(
        key.code,
        KeyCode::Char('\\' | 'j' | 'm') | KeyCode::PageUp | KeyCode::PageDown
    ) || (key.code == KeyCode::Enter
        && key.modifiers.contains(KeyModifiers::SHIFT));
    if ctrl && panel_only {
        return;
    }
    if !info && cmdline_insert::handle_key(app, key) {
        return;
    }
    if ctrl && !is_text_input(key.modifiers) {
        handle_control_key(app, key);
        return;
    }
    match key.code {
        KeyCode::Tab => app.switch_panel(),
        // With the panels hidden there is no file under the cursor to take.
        KeyCode::Enter if is_run_as_key(key) => {
            if !app.command_line.is_empty() {
                app.run_as_administrator();
            }
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => app.run_in_new_window(),
        // FAR: an empty line only shows the prompt on the user screen.
        KeyCode::Enter if key.modifiers.is_empty() => app.run_command_line(),
        // Alt+letter is quick search, which needs a shown panel; it types nothing.
        KeyCode::Char(_) if key.modifiers.contains(KeyModifiers::ALT) => {}
        // FAR's `CommandLine::ProcessKey`: with both panels hidden, Up/Down are Ctrl+E/Ctrl+X.
        KeyCode::Up | KeyCode::Down if app.panels_hidden() && key.modifiers.is_empty() => {
            app.recall_history(if key.code == KeyCode::Up { -1 } else { 1 });
        }
        KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown | KeyCode::Enter => {}
        KeyCode::Insert if key.modifiers.is_empty() => {}
        _ => edit_command_line(app, key),
    }
}

/// FAR ignores a held delete key: one press, one question.
fn is_delete(command: Command) -> bool {
    matches!(
        command,
        Command::Delete | Command::DeleteCurrent | Command::DeletePermanently
    )
}

/// The commands FAR handles at the screen level, which work while the active panel is hidden.
fn works_with_hidden_panel(command: Command) -> bool {
    !matches!(
        command,
        Command::View
            | Command::Edit
            | Command::Copy
            | Command::Move
            | Command::CopyCurrent
            | Command::MoveCurrent
            | Command::Link
            | Command::Attributes
            | Command::Delete
            | Command::DeleteCurrent
            | Command::DeletePermanently
            | Command::Sort(..)
            | Command::SortMenu(_)
            | Command::ToggleSelectedFirst(_)
            | Command::ToggleSortGroups(_)
            | Command::Reread(_)
    )
}

fn handle_plain_key(app: &mut App, key: KeyEvent) {
    let line_empty = app.command_line.is_empty();
    // Shift+Ins pastes, Shift+Left selects in the line, Alt+Shift+Home selects to its start.
    let plain = key.modifiers.is_empty();
    match key.code {
        KeyCode::Tab => app.switch_panel(),
        KeyCode::Up => app.active_panel_mut().move_cursor(-1),
        KeyCode::Down => app.active_panel_mut().move_cursor(1),
        KeyCode::PageUp => app.active_panel_mut().page(-1),
        KeyCode::PageDown => app.active_panel_mut().page(1),
        KeyCode::Insert if plain => app.active_panel_mut().toggle_selection(),
        // FAR deletes with Shift+Del on a shown panel, never cutting from the command line there; Alt+Del
        // (wipe) is deferred. Del deletes too when the line is empty (the owner's addition). A held key
        // deletes once (FAR's `IsRepeatedKey`).
        KeyCode::Delete if key.kind == KeyEventKind::Repeat && (line_empty || !plain) => {}
        KeyCode::Delete if key.modifiers == KeyModifiers::SHIFT => {
            app.execute(Command::DeletePermanently);
        }
        KeyCode::Delete if key.modifiers.contains(KeyModifiers::ALT) => {}
        KeyCode::Delete if plain && line_empty => app.execute(Command::Delete),
        // FAR: Left/Right move by a column when the command line is empty or the view has stripes.
        KeyCode::Left | KeyCode::Right
            if plain && (line_empty || app.active_panel().stripes() > 1) =>
        {
            let direction = if key.code == KeyCode::Left { -1 } else { 1 };
            app.active_panel_mut().move_column(direction);
        }
        KeyCode::Home if plain && line_empty => app.active_panel_mut().home(),
        KeyCode::End if plain && line_empty => app.active_panel_mut().end(),
        KeyCode::Enter if is_run_as_key(key) && line_empty => {
            enter_or(app, App::run_as_administrator);
        }
        KeyCode::Enter if is_run_as_key(key) => app.run_as_administrator(),
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => app.run_in_new_window(),
        KeyCode::Enter if line_empty => enter_or(app, App::open_current_file),
        KeyCode::Enter => app.run_command_line(),
        _ => edit_command_line(app, key),
    }
}

/// The command line's `Edit` keys (FAR's `CommandLine::ProcessKey`); Esc clears the line, and on an
/// empty line hides or shows the panels as Ctrl+O does (Volkov Commander's Esc, not FAR's).
fn edit_command_line(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        if key.modifiers.is_empty() && ask_stop_reading(app) {
            return;
        }
        if app.command_line.is_empty() && key.modifiers.is_empty() {
            app.execute(Command::TogglePanels);
        } else {
            app.command_line.take();
        }
        return;
    }
    let before = app.command_line.text().to_string();
    if let Err(error) = line_edit::handle_key(&mut app.command_line, key, app.clipboard.as_mut()) {
        app.show_error("Clipboard", &error);
    }
    if app.command_line.text() != before && app.prefs.completion_list {
        app.update_completion();
    }
}

/// Whether the active panel is at a disk's root (a drive's or a share's).
fn at_disk_root(app: &App) -> bool {
    let panel = app.active_panel();
    panel.path.parent().is_none() && panel.path.as_local().is_some()
}

/// Ctrl+PgUp (FAR's `KEY_CTRLPGUP`): the parent folder; at a drive's or share's root, with "Use
/// Ctrl+PgUp to change drive", FAR's `ChangeDir("..")` there (`plugins::share_up`: the network plugin
/// from a share, else the drive menu), else nothing.
fn go_parent(app: &mut App) {
    if at_disk_root(app) {
        if app.prefs.ctrl_pg_up_changes_drive != 0 {
            app.up_from_root(app.active);
        }
        return;
    }
    if let Err(error) = app.active_panel_mut().go_parent() {
        app.show_error("Change directory", &error);
    }
}

/// Ctrl+Alt+Enter, FAR's `IsRunAs`.
fn is_run_as_key(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.modifiers.contains(KeyModifiers::ALT)
}

/// Enter on a folder goes in; on a file `open` decides. On ".." at a share's root: FAR's
/// `ChangeDir("..")` there (`plugins::share_up`).
pub(super) fn enter_or(app: &mut App, open: fn(&mut App)) {
    let on_parent = app
        .active_panel()
        .current()
        .is_some_and(|entry| entry.is_parent);
    if on_parent && at_disk_root(app) {
        return app.up_from_root(app.active);
    }
    match app.active_panel_mut().enter() {
        Ok(true) => {}
        Ok(false) => open(app),
        Err(error) => app.show_error("Change directory", &error),
    }
}

/// Esc while a folder is read: FAR's `CheckForEscAndConfirmAbort`. False when none is read, or only an
/// automatic refresh the title does not show yet (Esc then is the panels' own).
fn ask_stop_reading(app: &mut App) -> bool {
    if app.panels.iter().all(|panel| panel.reading().is_none()) {
        return false;
    }
    if !app.prefs.confirm_interrupt {
        app.panels.iter_mut().for_each(super::Panel::stop_reading);
        return true;
    }
    app.ask_interrupt(super::dialog::ConfirmAction::StopReading);
    true
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
