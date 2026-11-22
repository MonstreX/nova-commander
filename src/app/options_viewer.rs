//! The viewer's pages of the settings (S2d.6): FAR's "Viewer settings" and Alt+Shift+F9's local copy.

use super::super::App;
use super::{Field, check};

/// FAR's "Viewer settings" (`Options::ViewerConfig`): what the viewer does so far (S2d.6a), and the
/// text mode's syntax highlighting, the program's own (ADR-0024).
pub(super) fn viewer() -> Vec<Field> {
    [external(), layout(), kept(), code_pages()]
        .into_iter()
        .flatten()
        .collect()
}

/// The viewer settings' F3's external viewer.
fn external() -> Vec<Field> {
    vec![
        check(
            "Use external viewer for F&3 instead of Alt+F3",
            |app| app.viewer_settings.use_external,
            |app, on| app.viewer_settings.use_external = on,
        ),
        Field::Text {
            label: "&Viewer command:",
            width: 64,
            get: |app| app.viewer_settings.external_command.clone(),
            set: |app, command| app.viewer_settings.external_command = command,
        },
    ]
}

/// The viewer settings' the text's layout and what is shown.
fn layout() -> Vec<Field> {
    vec![
        check(
            "&Persistent selection",
            |app| app.viewer_settings.persistent_selection,
            |app, on| app.viewer_settings.persistent_selection = on,
        ),
        Field::Number {
            label: "&Tab size",
            width: 3,
            get: |app| app.viewer_settings.tab_size,
            set: |app, size| app.viewer_settings.tab_size = size,
        },
        Field::Number {
            label: "&Maximum line width",
            width: 6,
            get: |app| app.viewer_settings.max_line_width,
            set: |app, width| app.viewer_settings.max_line_width = width,
        },
        check(
            "Show scrolling arro&ws",
            |app| app.viewer_settings.show_arrows,
            |app, on| app.viewer_settings.show_arrows = on,
        ),
        check(
            "Visible '\\&0'",
            |app| app.viewer_settings.visible_nul,
            |app, on| app.viewer_settings.visible_nul = on,
        ),
        check(
            "Show scro&llbar",
            |app| app.viewer_settings.show_scrollbar,
            |app, on| app.viewer_settings.show_scrollbar = on,
        ),
    ]
}

/// The viewer settings' what is kept of a file between runs.
fn kept() -> Vec<Field> {
    vec![
        check(
            "&Save file position",
            |app| app.viewer_settings.save_position,
            |app, on| app.viewer_settings.save_position = on,
        ),
        check(
            "Save file &code page",
            |app| app.viewer_settings.save_code_page,
            |app, on| app.viewer_settings.save_code_page = on,
        ),
        check(
            "Save &bookmarks",
            |app| app.viewer_settings.save_bookmarks,
            |app, on| app.viewer_settings.save_bookmarks = on,
        ),
        check(
            "Save vi&ew mode",
            |app| app.viewer_settings.save_view_mode,
            |app, on| app.viewer_settings.save_view_mode = on,
        ),
        check(
            "Save w&rap mode",
            |app| app.viewer_settings.save_wrap_mode,
            |app, on| app.viewer_settings.save_wrap_mode = on,
        ),
    ]
}

/// The viewer settings' the dump and the code pages, the highlighting.
fn code_pages() -> Vec<Field> {
    vec![
        check(
            "Detect d&ump view mode",
            |app| app.viewer_settings.detect_dump,
            |app, on| app.viewer_settings.detect_dump = on,
        ),
        check(
            "&Autodetect code page",
            |app| app.viewer_settings.auto_detect,
            |app, on| app.viewer_settings.auto_detect = on,
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
                (DEFAULT_PAGES.iter())
                    .position(|(_, page)| {
                        *page == default_page_number(app.viewer_settings.default_page)
                    })
                    .unwrap_or(0)
            },
            set: |app, at| {
                let page = match DEFAULT_PAGES.get(at).map_or(0, |(_, page)| *page) {
                    u32::MAX => crate::platform::locale::oem_code_page(),
                    page => page,
                };
                app.viewer_settings.default_page = page;
            },
        },
        check(
            "Syntax &highlighting",
            |app| app.viewer_settings.highlighting,
            |app, on| app.viewer_settings.highlighting = on,
        ),
    ]
}

/// The default code page's choices (0: ANSI, FAR's `CP_DEFAULT`; the OEM page told at run time).
pub(super) const DEFAULT_PAGES: [(&str, u32); 5] = [
    ("ANSI", 0),
    ("OEM", u32::MAX),
    ("UTF-8", 65001),
    ("UTF-16 (Little endian)", 1200),
    ("UTF-16 (Big endian)", 1201),
];

/// A setting's page as the choices name it: the OEM page as OEM.
pub(super) fn default_page_number(page: u32) -> u32 {
    if page != 0 && page == crate::platform::locale::oem_code_page() {
        u32::MAX
    } else {
        page
    }
}

/// The open viewer's settings changed and laid out again.
fn local(app: &mut App, change: impl FnOnce(&mut crate::settings::ViewerSettings)) {
    if let super::super::Screen::Viewer(viewer) = &mut app.screen {
        let mut settings = viewer.settings.clone();
        change(&mut settings);
        viewer.apply_settings(&settings);
    }
}

/// The open viewer's settings, read.
fn local_get<T>(app: &App, get: impl FnOnce(&crate::settings::ViewerSettings) -> T, none: T) -> T {
    match &app.screen {
        super::super::Screen::Viewer(viewer) => get(&viewer.settings),
        _ => none,
    }
}

/// FAR's local "Viewer" settings (Alt+Shift+F9, `ViewerConfig` with `Local`): the open viewer's own.
pub(super) fn viewer_local() -> Vec<Field> {
    vec![
        Field::Number {
            label: "&Tab size",
            width: 3,
            get: |app| local_get(app, |s| s.tab_size, 8),
            set: |app, size| local(app, |s| s.tab_size = size),
        },
        check(
            "Show scrolling arro&ws",
            |app| local_get(app, |s| s.show_arrows, true),
            |app, on| local(app, |s| s.show_arrows = on),
        ),
        check(
            "Visible '\\&0'",
            |app| local_get(app, |s| s.visible_nul, false),
            |app, on| local(app, |s| s.visible_nul = on),
        ),
        check(
            "Show scro&llbar",
            |app| local_get(app, |s| s.show_scrollbar, false),
            |app, on| local(app, |s| s.show_scrollbar = on),
        ),
        check(
            "Syntax &highlighting",
            |app| local_get(app, |s| s.highlighting, true),
            |app, on| local(app, |s| s.highlighting = on),
        ),
    ]
}
