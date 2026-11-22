//! Rendering of the application state with ratatui. Reads `app`; never changes behaviour,
//! only tells panels and text views how many rows they have.

mod bars;
mod centred_menu;
mod columns;
mod completion;
mod dialog;
mod dialog_place;
mod drive_menu;
mod edit_list;
mod filters_menu;
mod find_results;
mod folder_tree;
mod form;
mod format;
mod guest_frame;
mod highlight;
mod history_menu;
mod info_panel;
mod keybar;
mod menu;
mod panel;
mod panel_cells;
mod panel_names;
mod panel_title;
mod plugin_menu;
mod plugins_list;
mod popup;
mod quick_search;
mod quick_view;
mod separator;
mod shortcuts_menu;
pub mod signals;
mod size_format;
mod status_row;
mod terminal;
mod text_view;
pub mod theme;
mod transfer;
mod tree_panel;
mod user_menu;
mod vmenu;
mod wait_window;

use ratatui::Frame;
use std::time::SystemTime;

use ratatui::layout::{Position, Rect};

use crate::app::{App, Command, MainView, Overlay, Screen, panel_widths};
use crate::terminal::UserScreen;

pub fn draw(frame: &mut Frame, app: &mut App) {
    vmenu::show_scrollbars(app.prefs.dialogs.menu_scrollbar);
    // A dialog drawn under something else stays where it was put.
    dialog_place::begin(app.dialog_place.offset);
    let themes = &app.themes;
    let problems = theme::use_named(themes.shown(), &|name| themes.read_file(name));
    app.themes.problems.extend(problems);
    let full = frame.area();
    // Where the panels and bars are, for the mouse: only those drawn now.
    app.mouse.forget_areas();
    // While tasks exist their status row takes the top row and the rest moves down (ADR-0010).
    let area = if app.status_row_shown() && full.height > 1 {
        status_row::draw(frame, Rect::new(full.x, full.y, full.width, 1), app);
        Rect::new(full.x, full.y + 1, full.width, full.height - 1)
    } else {
        full
    };
    let mut cursor = draw_screen(frame, area, app);
    if let Some(shown) = &app.shown_task
        && let Some(task) = app.tasks.get(shown.id)
    {
        let dialog = transfer::draw(frame, area, task, shown.focus);
        app.mouse.dialog = Some(dialog);
        cursor = None;
    }
    // The menu or list in front, for the mouse; the one under the command line unless another.
    let mut menu_area = app.mouse.menu;
    let mut dialog_area = app.mouse.dialog.take();
    fit_overlay(app, full, area);
    match &app.overlay {
        Some(Overlay::Menu(state)) => {
            menu_area = draw_bar_menu(frame, full, app, state);
            // FAR draws the clock over the menu bar too.
            draw_clock(frame, full, app.prefs.clock);
            cursor = None;
        }
        Some(Overlay::DriveMenu(menu)) => {
            menu_area = drive_menu::draw(frame, area, app, menu);
            cursor = None;
        }
        Some(Overlay::FindResults(results)) => {
            dialog_area = Some(find_results::draw(frame, area, results));
            cursor = None;
        }
        Some(Overlay::Dialog(dialog)) => {
            let drawn = draw_dialog(frame, area, dialog, &mut app.dialog_place);
            cursor = drawn.cursor;
            menu_area = drawn.menu.or(menu_area);
            dialog_area = drawn.dialog;
        }
        Some(Overlay::FolderTree(found)) => {
            // The copy dialog waiting under the window.
            if let crate::app::tree_panel::Purpose::Target(dialog) = &found.purpose {
                dialog::draw(frame, area, dialog);
            }
            cursor = folder_tree::draw(frame, full, found);
            menu_area = None;
            dialog_area = None;
        }
        Some(Overlay::QuickSearch(search)) => {
            let panel = active_panel_rect(area, app);
            cursor = Some(quick_search::draw(frame, area, panel, search));
        }
        Some(overlay) => {
            menu_area = draw_list_menu(frame, area, overlay);
            cursor = None;
        }
        None => {}
    }
    // Work the keys wait for, slow to end: its window, Esc to stop waiting (ADR-0019).
    // A panel key's plugin slow to answer too (one mechanism, R54).
    if let Some(window) = app.held_window() {
        wait_window::draw(frame, area, &window);
        // As under any window: no cursor in the command line (the mouse is held by the wait).
        cursor = None;
    }
    app.mouse.menu = menu_area;
    app.mouse.dialog = dialog_area;
    if let Some(position) = cursor {
        frame.set_cursor_position(position);
    }
}

/// Before the overlays are drawn: those that keep their place and size for the mouse and the keys.
fn fit_overlay(app: &mut App, full: Rect, area: Rect) {
    match &mut app.overlay {
        Some(Overlay::FolderTree(found)) => folder_tree::fit(found, full),
        Some(Overlay::FindResults(results)) => results.fit(area.width, area.height),
        _ => {}
    }
}

/// The menu bar's menu, its items checked as FAR's (the view mode, the info panel, the tree).
fn draw_bar_menu(
    frame: &mut Frame,
    full: Rect,
    app: &App,
    state: &crate::app::MenuState,
) -> Option<crate::app::MenuArea> {
    let panels = &app.panels;
    let (info_side, tree_side) = (app.info_side(), app.tree_side());
    let checked = |command: Command| match command {
        Command::SetViewMode(side, mode) => {
            app.shows_files(side) && panels[side.index()].view_mode == mode
        }
        Command::InfoPanel(side) => info_side == Some(side),
        Command::TreePanel(side) => tree_side == Some(side),
        Command::QuickView(side) => app.quick_view_side() == Some(side),
        _ => false,
    };
    menu::draw(frame, full, state, &checked)
}

/// The menus that stand alone over the screen (a pop-up, the histories, the user menu, the lists).
fn draw_list_menu(
    frame: &mut Frame,
    area: Rect,
    overlay: &Overlay,
) -> Option<crate::app::MenuArea> {
    match overlay {
        Overlay::Popup(menu) => popup::draw(frame, area, menu),
        Overlay::History(menu) => history_menu::draw(frame, area, menu),
        Overlay::UserMenu(menu) => user_menu::draw(frame, area, menu),
        Overlay::EditList(list) => edit_list::draw(frame, area, list),
        Overlay::Shortcuts(menu) => shortcuts_menu::draw(frame, area, menu),
        Overlay::Filters(menu) => filters_menu::draw(frame, area, menu),
        Overlay::PluginMenu(menu) => plugin_menu::draw(frame, area, menu),
        Overlay::Menu(_)
        | Overlay::DriveMenu(_)
        | Overlay::Dialog(_)
        | Overlay::QuickSearch(_)
        | Overlay::FindResults(_)
        | Overlay::FolderTree(_) => None,
    }
}

/// A menu's box and its margin, for the mouse.
pub(super) fn menu_area(frame: Rect, outer: Rect, first: usize) -> crate::app::MenuArea {
    let cells = |rect: Rect| crate::app::Cells {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    };
    crate::app::MenuArea {
        frame: cells(frame),
        outer: cells(outer),
        first,
    }
}

/// FAR's main screen. With panels: they fill the screen down to the command line and the key bar,
/// the user screen lies behind them (Ctrl+Up/Ctrl+Down and hidden panels uncover its last lines).
/// While a command runs the user screen fills the whole console; the guest shell (Alt+Shift+T) too.
/// The screen under the windows: the viewer, the editor or the panels; where the cursor stands.
fn draw_screen(frame: &mut Frame, area: Rect, app: &mut App) -> Option<Position> {
    let held = (app.held, app.prefs.clock, app.front_can_lose_focus());
    match &mut app.screen {
        // Ctrl+O in the viewer or the editor: FAR's user screen behind them until a key.
        Screen::Viewer(_) | Screen::Editor(_) if app.backdrop => {
            let view = app.user_screen.as_ref().map(UserScreen::view);
            terminal::draw_screen(frame, area, view.as_ref());
            None
        }
        Screen::Viewer(viewer) => text_view::draw_viewer(frame, area, viewer, held),
        Screen::Editor(editor) => text_view::draw_editor(frame, area, editor, held),
        Screen::Panels => draw_main(frame, area, app),
    }
}

fn draw_main(frame: &mut Frame, area: Rect, app: &mut App) -> Option<Position> {
    let view = app.user_screen.as_ref().map(UserScreen::view);
    match app.main_view {
        MainView::Panels => {}
        MainView::CommandOutput => {
            app.mouse.terminal = Some(form::cells(area));
            return terminal::draw_screen(frame, area, view.as_ref());
        }
        MainView::Guest => {
            app.mouse.terminal = Some(form::cells(guest_frame::shell_area(area)));
            return guest_frame::draw(frame, area, app.guest.as_ref(), app.prefs.clock);
        }
        MainView::Detached(index) => {
            app.mouse.terminal = Some(form::cells(area));
            return terminal::draw_screen(frame, area, app.detached_view(index).as_ref());
        }
    }
    let bottom_rows = app.bottom_rows();
    if area.height < bottom_rows + 1 {
        return None;
    }
    // A dialog that came up by itself covers the drag.
    let drag_text = app
        .mouse
        .drag_text
        .clone()
        .filter(|_| app.overlay.is_none() && app.shown_task.is_none());
    let above_bottom = area.height - bottom_rows;
    let decrement = app.height_decrement.min(above_bottom);
    let panels_height = above_bottom - decrement;
    let behind = Rect::new(area.x, area.y, area.width, above_bottom);
    terminal::draw_above_cursor(frame, behind, view.as_ref());
    if panels_height > 0 {
        draw_panels(
            frame,
            Rect::new(area.x, area.y, area.width, panels_height),
            app,
        );
    }
    let command_area = Rect::new(area.x, area.y + above_bottom, area.width, 1);
    let prompt = format!("{}>", app.prompt_folder());
    let (cursor, line) = bars::draw_command_line(frame, command_area, &prompt, &app.command_line);
    app.mouse.command_line = Some(line);
    app.mouse.menu = completion::draw(
        frame,
        command_area,
        prompt_end(&prompt, area.width),
        area.y,
        app,
    );
    if app.show_key_bar {
        let keys_area = Rect::new(area.x, area.y + above_bottom + 1, area.width, 1);
        let own = app.plugin_key_labels(app.held);
        let ends = keybar::draw(frame, keys_area, &key_labels(app, &own));
        app.mouse.key_bar = Some(crate::app::KeyBarArea {
            x: keys_area.x,
            row: keys_area.y,
            width: keys_area.width,
            ends,
        });
    }
    if let Some(note) = drag_text
        && area.contains(Position::new(note.column, note.row))
    {
        let room = usize::from(area.right() - note.column);
        frame.buffer_mut().set_stringn(
            note.column,
            note.row,
            &note.text,
            room,
            theme::panel_drag(),
        );
    }
    Some(cursor)
}

/// Where the text of the command line begins: a long prompt takes half the width at most.
fn prompt_end(prompt: &str, width: u16) -> u16 {
    let chars = u16::try_from(prompt.chars().count()).unwrap_or(u16::MAX);
    chars.min(width / 2)
}

/// The shown panels side by side, split where Ctrl+Left/Right put it; a panel in a full-screen view
/// mode (Detailed) takes the whole width while it is active. The clock sits in the top-right corner
/// over the frame, as in FAR.
fn draw_panels(frame: &mut Frame, area: Rect, app: &mut App) {
    let clock_x = draw_clock(frame, area, app.prefs.clock);
    let active = app.active.index();
    let info_side = app.info_side().map(crate::app::Side::index);
    let tree_side = app.tree_side().map(crate::app::Side::index);
    let view_side = app.quick_view_side().map(crate::app::Side::index);
    for (index, rect) in panel_rects(area, app).into_iter().enumerate() {
        if let (Some(rect), Some(info)) = (rect, &app.info_panel)
            && info_side == Some(index)
        {
            info_panel::draw(frame, rect, info, index == active);
            app.mouse.areas[index] = Some(panel_area(rect, None));
        } else if let (Some(rect), Some(tree)) = (rect, &mut app.tree_panel)
            && tree_side == Some(index)
        {
            tree_panel::draw(frame, rect, tree, index == active);
            if app.prefs.panel_scrollbar {
                // FAR's tree: from under the top frame, the height less 4 (`treelist.cpp`).
                let length = usize::from(rect.height.saturating_sub(4));
                let place = (rect.right().saturating_sub(1), rect.y + 1);
                panel::draw_bar_at(
                    frame.buffer_mut(),
                    place,
                    length,
                    (tree.top, tree.items.len()),
                );
            }
            app.mouse.areas[index] = Some(panel_area(rect, None));
        } else if let (Some(rect), Some(view)) = (rect, &app.quick_view)
            && view_side == Some(index)
        {
            quick_view::draw(frame, rect, view, index == active);
            app.mouse.areas[index] = Some(panel_area(rect, None));
        } else if let Some(rect) = rect {
            let shown = &app.panels[index];
            let mode = app.panel_modes.spec_in(shown.view_mode, &shown.look);
            let look = panel::Look {
                focused: index == active,
                highlighting: (shown.options.highlight).then_some(&app.highlighting),
                now: std::time::SystemTime::now(),
                mode: &mode,
            };
            let list = panel::draw(frame, rect, &mut app.panels[index], &look, clock_x);
            if app.prefs.panel_scrollbar
                && let Some(list) = &list
            {
                // FAR's files: on the right frame beside the list, by its rows (`ShowFileList`).
                let panel = &app.panels[index];
                let stripes = panel.stripes().max(1);
                let per_stripe = |value: usize| value.div_ceil(stripes);
                let place = (rect.right().saturating_sub(1), list.top);
                let rows = (per_stripe(panel.offset), per_stripe(panel.entries.len()));
                panel::draw_bar_at(frame.buffer_mut(), place, usize::from(list.rows), rows);
            }
            app.mouse.areas[index] = Some(panel_area(rect, list));
        }
    }
    draw_clock(frame, area, app.prefs.clock);
}

/// The key bar's labels: "Find folder"'s own (FAR's `MFolderTreeF1`...), the quick view's, the info
/// panel's or the panels', a plugin's panel's own (FAR's `KeyBar`, `own`) over them.
fn key_labels<'a>(app: &App, own: &'a [(u8, String)]) -> [&'a str; 12] {
    let mut labels = match &app.overlay {
        Some(Overlay::FolderTree(_)) => return keybar::FOLDER_TREE,
        _ if app.quick_view_has_keys() => {
            return keybar::quick_view_labels(app.held, app.quick_view_next_page());
        }
        _ if app.info_has_keys() => {
            return keybar::info_labels(app.held, app.description_next_page());
        }
        _ => *keybar::panel_labels(app.held),
    };
    for (number, label) in own {
        if let Some(slot) = labels.get_mut(usize::from(*number).wrapping_sub(1)) {
            *slot = label;
        }
    }
    labels
}

fn panel_area(rect: Rect, list: Option<crate::app::ListArea>) -> crate::app::PanelArea {
    crate::app::PanelArea {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        list,
    }
}

/// Where each panel is drawn in `area` (the rows above the command line), `None` when hidden.
fn panel_rects(area: Rect, app: &App) -> [Option<Rect>; 2] {
    let active = app.active.index();
    let shown = |index: usize| app.panels[index].visible;
    let files_active = app.shows_files(app.active);
    if shown(active)
        && files_active
        && (app.panel_modes)
            .spec_in(app.panels[active].view_mode, &app.panels[active].look)
            .full_screen
    {
        let mut rects = [None, None];
        rects[active] = Some(area);
        return rects;
    }
    let (left_width, right_width) = panel_widths(area.width, app.width_decrement);
    let left = Rect::new(area.x, area.y, left_width, area.height);
    let right = Rect::new(area.x + left_width, area.y, right_width, area.height);
    [shown(0).then_some(left), shown(1).then_some(right)]
}

/// Where the active panel is drawn (for boxes FAR places relative to it).
fn active_panel_rect(area: Rect, app: &App) -> Rect {
    let height = area
        .height
        .saturating_sub(app.bottom_rows())
        .saturating_sub(app.height_decrement);
    let panels = Rect::new(area.x, area.y, area.width, height);
    panel_rects(panels, app)[app.active.index()].unwrap_or(panels)
}

/// The clock in the top-right corner; returns the x where it starts.
pub(super) fn draw_clock(frame: &mut Frame, area: Rect, shown: bool) -> u16 {
    if !shown {
        return area.right();
    }
    let clock = format::clock(SystemTime::now());
    let clock_width = u16::try_from(clock.chars().count()).unwrap_or(0);
    let clock_x = area.right().saturating_sub(clock_width);
    frame.buffer_mut().set_stringn(
        clock_x,
        area.y,
        &clock,
        usize::from(clock_width),
        theme::clock(),
    );
    clock_x
}

/// The dialog in front where the app put it; the place it took goes back, FAR's marks in its corners
/// while the keys move it.
fn draw_dialog(
    frame: &mut Frame,
    area: Rect,
    dialog: &crate::app::Dialog,
    place: &mut crate::app::DialogPlace,
) -> dialog::Drawn {
    dialog_place::begin_at(place.offset, place.at.take());
    let drawn = dialog::draw(frame, area, dialog);
    if let Some(taken) = dialog_place::taken() {
        place.offset = taken;
    }
    let moved =
        (drawn.dialog.as_ref().map(|dialog| dialog.frame)).or(drawn.menu.map(|menu| menu.outer));
    if let (Some(_), Some(cells)) = (place.keys, moved) {
        let rect = Rect::new(cells.x, cells.y, cells.width, cells.height);
        dialog_place::draw_marks(frame.buffer_mut(), rect);
    }
    drawn
}

#[cfg(test)]
#[path = "scroll_bar_tests.rs"]
mod scroll_bar_tests;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
