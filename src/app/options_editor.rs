//! The editor's page of the settings (S2d.6c-d): FAR's "Editor settings" (`Options::EditorConfig`),
//! and its local copy for the open editor (Alt+Shift+F9, `LocalEditorConfig`): the same fields but
//! the external editor, the places kept, the read-only files and the code pages, as FAR's `Local`.

use super::super::{App, Screen};
use super::viewer_pages::{DEFAULT_PAGES, default_page_number};
use super::{Field, check};
use crate::settings::EditorSettings;

/// FAR's "Editor settings" (`Options::EditorConfig`).
pub(super) fn editor() -> Vec<Field> {
    [
        external(),
        editing::<false>(),
        shown::<false>(),
        kept(),
        code_pages(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// FAR's local "Editor" settings (Alt+Shift+F9): the open editor's own.
pub(super) fn editor_local() -> Vec<Field> {
    let mut fields = editing::<true>();
    fields.extend(shown::<true>());
    fields
}

/// The settings read: the open editor's (`local`) or the program's.
fn read<T>(app: &App, get: impl FnOnce(&EditorSettings) -> T, local: bool) -> T {
    match (&app.screen, local) {
        (Screen::Editor(editor), true) => get(&editor.settings),
        _ => get(&app.editor_settings),
    }
}

/// The settings changed: the open editor's (`local`, what it shows with them) or the program's.
fn write(app: &mut App, change: impl FnOnce(&mut EditorSettings), local: bool) {
    match (&mut app.screen, local) {
        (Screen::Editor(editor), true) => {
            change(&mut editor.settings);
            editor.text.apply_settings(&editor.settings);
            editor.text.numbers = editor.settings.show_line_numbers;
        }
        (_, true) => {}
        _ => change(&mut app.editor_settings),
    }
}

/// The editor settings' F4 external editor.
fn external() -> Vec<Field> {
    vec![
        check(
            "Use external editor for F&4 instead of Alt+F4",
            |app| app.editor_settings.use_external,
            |app, on| app.editor_settings.use_external = on,
        ),
        Field::Text {
            label: "&Editor command:",
            width: 64,
            get: |app| app.editor_settings.external_command.clone(),
            set: |app, command| app.editor_settings.external_command = command,
        },
    ]
}

/// The editor settings' way of editing (FAR's first column).
fn editing<const LOCAL: bool>() -> Vec<Field> {
    vec![
        Field::Choice {
            label: "Expand &tabs:",
            lines: |_| EXPAND_TABS.map(String::from).to_vec(),
            get: |app| read(app, |s| usize::from(s.expand_tabs.min(2)), LOCAL),
            set: |app, at| {
                write(
                    app,
                    |s| s.expand_tabs = u8::try_from(at).unwrap_or(0),
                    LOCAL,
                );
            },
        },
        check(
            "&Persistent blocks",
            |app| read(app, |s| s.persistent_blocks, LOCAL),
            |app, on| write(app, |s| s.persistent_blocks = on, LOCAL),
        ),
        check(
            "&Del removes blocks",
            |app| read(app, |s| s.del_removes_blocks, LOCAL),
            |app, on| write(app, |s| s.del_removes_blocks = on, LOCAL),
        ),
        check(
            "Auto &indent",
            |app| read(app, |s| s.auto_indent, LOCAL),
            |app, on| write(app, |s| s.auto_indent = on, LOCAL),
        ),
        Field::Number {
            label: "&Tab size",
            width: 3,
            get: |app| read(app, |s| s.tab_size, LOCAL),
            set: |app, size| write(app, |s| s.tab_size = size, LOCAL),
        },
        check(
            "Sh&ow white space",
            |app| read(app, |s| s.show_white_space, LOCAL),
            |app, on| write(app, |s| s.show_white_space = on, LOCAL),
        ),
        // Ours (owner 2026-11-22): today's code editors' keys in place of FAR's.
        check(
            "Code editing &mode",
            |app| read(app, |s| s.code_editing, LOCAL),
            |app, on| write(app, |s| s.code_editing = on, LOCAL),
        ),
        Field::Number {
            label: "Ruler colum&n (0: none)",
            width: 4,
            get: |app| read(app, |s| s.ruler_column, LOCAL),
            set: |app, column| write(app, |s| s.ruler_column = column, LOCAL),
        },
        check(
            "Clean line ends on sa&ve",
            |app| read(app, |s| s.trim_on_save, LOCAL),
            |app, on| write(app, |s| s.trim_on_save = on, LOCAL),
        ),
    ]
}

/// The editor settings' cursor, search and look (FAR's second column) and the highlighting (ours).
fn shown<const LOCAL: bool>() -> Vec<Field> {
    vec![
        check(
            "&Cursor beyond end of line",
            |app| read(app, |s| s.cursor_beyond_eol, LOCAL),
            |app, on| write(app, |s| s.cursor_beyond_eol = on, LOCAL),
        ),
        check(
            "Se&lect found",
            |app| read(app, |s| s.select_found, LOCAL),
            |app, on| write(app, |s| s.select_found = on, LOCAL),
        ),
        check(
            "Cu&rsor at the end",
            |app| read(app, |s| s.cursor_at_end, LOCAL),
            |app, on| write(app, |s| s.cursor_at_end = on, LOCAL),
        ),
        check(
            "S&how scrollbar",
            |app| read(app, |s| s.show_scrollbar, LOCAL),
            |app, on| write(app, |s| s.show_scrollbar = on, LOCAL),
        ),
        check(
            "&Show line numbers",
            |app| read(app, |s| s.show_line_numbers, LOCAL),
            |app, on| write(app, |s| s.show_line_numbers = on, LOCAL),
        ),
        check(
            "Syntax &highlighting",
            |app| read(app, |s| s.highlighting, LOCAL),
            |app, on| write(app, |s| s.highlighting = on, LOCAL),
        ),
    ]
}

/// The editor settings' places kept and read-only files (the program's only).
fn kept() -> Vec<Field> {
    vec![
        check(
            "&Save file position",
            |app| app.editor_settings.save_position,
            |app, on| app.editor_settings.save_position = on,
        ),
        check(
            "Save &bookmarks",
            |app| app.editor_settings.save_bookmarks,
            |app, on| app.editor_settings.save_bookmarks = on,
        ),
        check(
            "Lock editing of read-only &files",
            |app| app.editor_settings.read_only_lock,
            |app, on| app.editor_settings.read_only_lock = on,
        ),
        check(
            "&Warn when opening read-only files",
            |app| app.editor_settings.read_only_warn,
            |app, on| app.editor_settings.read_only_warn = on,
        ),
    ]
}

/// The editor settings' code pages.
fn code_pages() -> Vec<Field> {
    vec![
        check(
            "&Autodetect code page",
            |app| app.editor_settings.auto_detect,
            |app, on| app.editor_settings.auto_detect = on,
        ),
        Field::Choice {
            label: "&Default code page:",
            lines: |_| {
                DEFAULT_PAGES
                    .iter()
                    .map(|(name, _)| (*name).to_string())
                    .collect()
            },
            get: |app| {
                let page = default_page_number(app.editor_settings.default_page);
                (DEFAULT_PAGES.iter())
                    .position(|(_, choice)| *choice == page)
                    .unwrap_or(0)
            },
            set: |app, at| app.editor_settings.default_page = chosen_page(at),
        },
    ]
}

/// FAR's "Expand tabs" choices (`EXPAND_NOTABS`, `EXPAND_NEWTABS`, `EXPAND_ALLTABS`).
const EXPAND_TABS: [&str; 3] = [
    "Do not expand tabs",
    "Expand newly entered tabs to spaces",
    "Expand all tabs to spaces",
];

/// A default code page chosen: the OEM page by its number.
fn chosen_page(at: usize) -> u32 {
    match DEFAULT_PAGES.get(at).map_or(0, |(_, page)| *page) {
        u32::MAX => crate::platform::locale::oem_code_page(),
        page => page,
    }
}
