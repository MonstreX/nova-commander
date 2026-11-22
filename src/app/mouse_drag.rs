//! Dragging files between the panels as FAR's `Panel::ProcessMouseDrag`: the left button goes down
//! on a file (Shift: to move), past 15 columns or over the other panel a note follows the mouse
//! (`Copy "name"`, `Move 3 files`), the right button swaps copy and move, let go over the other panel
//! the F5 or F6 dialog comes up, for the folder under the mouse when there is one (FAR's `ToSubdir`).

use ratatui::crossterm::event::KeyModifiers;

use super::cmdline_insert::quote_for_shell;
use super::console_input::Mouse;
use super::mouse::{ListArea, PanelArea, is_button_event};
use super::{App, Side};

/// FAR: the mouse has gone this far from where the drag began.
const DRAG_COLUMNS: u16 = 15;

/// A drag under way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drag {
    source: Side,
    /// Where the button went down.
    column: u16,
    moving: bool,
    /// The first of what is dragged.
    name: String,
    /// A tree's folder dragged, by its whole path.
    folder: Option<std::path::PathBuf>,
}

/// The note following the mouse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragText {
    pub column: u16,
    pub row: u16,
    pub text: String,
}

impl App {
    /// The left button went down over the entries of a panel narrower than the screen.
    pub(super) fn start_drag(
        &mut self,
        side: Side,
        area: &PanelArea,
        list: &ListArea,
        mouse: &Mouse,
    ) {
        let press = mouse.buttons.left && is_button_event(mouse.action);
        if !press || area.width >= self.width {
            return;
        }
        self.move_to_mouse(side, list, mouse);
        let Some(name) = self.panels[side.index()].first_target() else {
            return;
        };
        self.mouse.drag = Some(Drag {
            source: side,
            column: mouse.column,
            moving: mouse.modifiers.contains(KeyModifiers::SHIFT),
            name,
            folder: None,
        });
    }

    /// A record during a drag: `None` when there is no drag or the record goes on to the panel under
    /// the mouse (within 15 columns of the start the cursor just follows it). What the drag takes
    /// ends a button held over a panel's edge: in FAR that is a loop no drag runs in.
    pub(super) fn drag_mouse(&mut self, mouse: &Mouse) -> Option<bool> {
        let taken = self.drag_record(mouse)?;
        self.end_hold();
        Some(taken)
    }

    /// The left button went down on a folder of the tree (its root is not dragged).
    pub(super) fn start_tree_drag(&mut self, side: Side, area: &PanelArea, mouse: &Mouse) {
        if area.width >= self.width {
            return;
        }
        let Some(folder) = self.tree_drag_folder(side) else {
            return;
        };
        let name = (folder.file_name())
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        self.mouse.drag = Some(Drag {
            source: side,
            column: mouse.column,
            moving: mouse.modifiers.contains(KeyModifiers::SHIFT),
            name,
            folder: Some(folder),
        });
    }

    fn drag_record(&mut self, mouse: &Mouse) -> Option<bool> {
        let drag = self.mouse.drag.clone()?;
        // FAR's info panel knows nothing of drags; a tree only of those from it.
        let files = self
            .panel_under(mouse)
            .filter(|(side, _)| self.shows_files(*side) || *side == drag.source);
        let Some((side, area)) = files else {
            if mouse.buttons.any() {
                return Some(false);
            }
            self.end_drag();
            return Some(true);
        };
        if !mouse.buttons.any() {
            self.end_drag();
            if is_button_event(mouse.action) && drag.source != side {
                self.drop_on(&drag, side, &area, mouse);
            }
            return Some(true);
        }
        let other_shown = self.panels[drag.source.other().index()].visible;
        if mouse.row <= area.y || mouse.row >= area.bottom() || !other_shown {
            self.end_drag();
            return Some(true);
        }
        if mouse.pressed.right
            && is_button_event(mouse.action)
            && let Some(drag) = &mut self.mouse.drag
        {
            drag.moving = !drag.moving;
        }
        if !mouse.buttons.left {
            return None;
        }
        if mouse.column.abs_diff(drag.column) > DRAG_COLUMNS || drag.source != side {
            self.show_drag(mouse);
            return Some(true);
        }
        self.mouse.drag_text = None;
        None
    }

    /// FAR's `DragMessage`: what the panel would give F5 now, at the mouse; when one thing is dragged
    /// and the note first shows, the cursor goes back to it (`GoToFile`). Nothing to give, no note.
    fn show_drag(&mut self, mouse: &Mouse) {
        let Some(drag) = self.mouse.drag.clone() else {
            return;
        };
        let verb = if drag.moving { "Move" } else { "Copy" };
        let screen_right = self.width.saturating_sub(1);
        if let Some(folder) = drag
            .folder
            .as_ref()
            .filter(|_| self.tree_side() == Some(drag.source))
        {
            // The tree's cursor back on the folder dragged (FAR's `GoToFile(strDragName)`); gone, no note.
            let back = self.mouse.drag_text.is_some() || self.tree_back_to(folder);
            self.mouse.drag_text = back.then(|| {
                let what = quote_for_shell(&drag.name);
                place_note(format!("{verb} {what}"), mouse, screen_right)
            });
            return;
        }
        let panel = &mut self.panels[drag.source.index()];
        if panel.targets().len() == 1 && self.mouse.drag_text.is_none() {
            panel.go_to_name(&drag.name);
        }
        let count = panel.targets().len();
        let Some(name) = panel.first_target().filter(|_| count > 0) else {
            self.mouse.drag_text = None;
            return;
        };
        let what = if count == 1 {
            quote_for_shell(&name)
        } else {
            format!("{count} files")
        };
        self.mouse.drag_text = Some(place_note(format!("{verb} {what}"), mouse, screen_right));
    }

    /// FAR: the cursor of the panel dropped on goes where the mouse is, then the source's F5 or F6.
    fn drop_on(&mut self, drag: &Drag, side: Side, area: &PanelArea, mouse: &Mouse) {
        let Some(list) = area.list.as_ref().filter(|_| self.shows_files(side)) else {
            return;
        };
        self.move_to_mouse(side, list, mouse);
        self.active = drag.source;
        self.ask_drop(drag.moving);
    }

    pub(super) fn end_drag(&mut self) {
        self.mouse.drag = None;
        self.mouse.drag_text = None;
    }
}

/// FAR's `DragMessage`: at the mouse, moved left to stay on screen, cut when longer than it.
fn place_note(mut text: String, mouse: &Mouse, screen_right: u16) -> DragText {
    let length = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
    let mut column = mouse.column;
    if length.saturating_add(column) > screen_right {
        column = screen_right.saturating_sub(length);
        if length > screen_right {
            text = text.chars().take(usize::from(screen_right)).collect();
        }
    }
    DragText {
        column,
        row: mouse.row,
        text,
    }
}
