//! The arrangement of the main screen as FAR keeps it (`far/filepanels.cpp`): which panels are shown
//! (Ctrl+F1, Ctrl+F2, Ctrl+P), where the panels split (Ctrl+Left / Ctrl+Right, FAR's
//! `WidthDecrement`), whether the key bar is shown (Ctrl+B), swapping the panels (Ctrl+U) and Tab.

use super::{App, Side};

/// The narrowest a panel gets when the split moves (FAR: `ScrX / 2 - 10`).
const MIN_HALF_WIDTH: i32 = 10;

impl App {
    /// Rows under the panels: the command line and, unless Ctrl+B hid it, the key bar.
    pub fn bottom_rows(&self) -> u16 {
        1 + u16::from(self.show_key_bar)
    }

    /// The status row of the background tasks is shown above the panels (ADR-0010): while a task, a
    /// command or a search is in the background and the panels are on screen. The operation in front
    /// (its dialog, its question, its next destinations) has the dialog for that, so starting one does
    /// not move the panels (owner).
    pub fn status_row_shown(&self) -> bool {
        let in_front = |id| {
            self.shown_task
                .as_ref()
                .is_some_and(|shown| shown.id == id || shown.rest.contains(&id))
        };
        let background = !self.detached.is_empty() || !self.background_searches().is_empty();
        (self.tasks.iter().any(|task| !in_front(task.id)) || background)
            && matches!(self.screen, super::Screen::Panels)
            && self.main_view == super::MainView::Panels
    }

    /// Ctrl+F1 / Ctrl+F2: the panel on that side on or off. Hiding the active panel makes the other
    /// one active if it is shown; showing a panel while the other is hidden makes it active.
    pub fn toggle_panel(&mut self, side: Side) {
        let other = side.other();
        let other_visible = self.panels[other.index()].visible;
        let panel = &mut self.panels[side.index()];
        if panel.visible {
            panel.visible = false;
            if other_visible {
                self.active = other;
            }
        } else {
            panel.visible = true;
            if !other_visible {
                self.active = side;
            }
        }
    }

    /// Ctrl+O (FAR's `FilePanels::ProcessKey`): with a panel shown, both are hidden and remembered;
    /// with both hidden, the remembered ones come back (both, if none was).
    pub fn toggle_panels(&mut self) {
        if self.panels_hidden() {
            if !self.remembered.panels_before_hide.contains(&true) {
                self.remembered.panels_before_hide = [true, true];
            }
            for (panel, shown) in self
                .panels
                .iter_mut()
                .zip(self.remembered.panels_before_hide)
            {
                panel.visible = shown;
            }
            if !self.active_panel().visible {
                self.active = self.active.other();
            }
        } else {
            self.remembered.panels_before_hide = self.panels.each_ref().map(|panel| panel.visible);
            for panel in &mut self.panels {
                panel.visible = false;
            }
        }
    }

    /// Ctrl+P: the passive panel on or off, while the active one is shown.
    pub fn toggle_passive_panel(&mut self) {
        if self.active_panel().visible {
            let passive = &mut self.panels[self.active.other().index()];
            passive.visible = !passive.visible;
        }
    }

    /// Ctrl+B.
    pub fn toggle_key_bar(&mut self) {
        self.show_key_bar = !self.show_key_bar;
        self.clamp_height_decrement();
    }

    /// Ctrl+U: the panels change places with their state; the active one stays active (FAR moves it
    /// with the panel). Nothing happens while both are hidden.
    pub fn swap_panels(&mut self) {
        if self.panels.iter().all(|panel| !panel.visible) {
            return;
        }
        self.panels.swap(0, 1);
        self.swap_filter_marks();
        self.active = self.active.other();
        self.swap_info_panel();
        self.swap_tree_panel();
        self.swap_quick_view();
        for side in self.work.task_marks.values_mut() {
            *side = side.other();
        }
    }

    /// Tab: to the other panel when it is shown.
    pub fn switch_panel(&mut self) {
        if self.passive_panel().visible {
            self.active = self.active.other();
        }
    }

    /// Ctrl+Left (+1: the left panel narrower) and Ctrl+Right (-1).
    pub fn change_width_decrement(&mut self, delta: i16) {
        let wanted = i32::from(self.width_decrement) + i32::from(delta);
        self.width_decrement = clamp_width_decrement(self.width, wanted);
    }

    pub(super) fn clamp_height_decrement(&mut self) {
        let max = self
            .height
            .saturating_sub(self.bottom_rows() + super::MIN_PANEL_ROWS);
        self.height_decrement = self.height_decrement.min(max);
    }

    pub(super) fn change_height_decrement(&mut self, delta: i16) {
        self.height_decrement = self.height_decrement.saturating_add_signed(delta);
        self.clamp_height_decrement();
    }

    pub(super) fn clamp_width_decrement(&mut self) {
        self.width_decrement = clamp_width_decrement(self.width, i32::from(self.width_decrement));
    }

    /// Both panels are hidden: the screen shows the shell's output above the command line.
    pub fn panels_hidden(&self) -> bool {
        self.panels.iter().all(|panel| !panel.visible)
    }
}

/// FAR's `SetPanelPositions`: the decrement is kept within `±(ScrX / 2 - 10)`.
fn clamp_width_decrement(width: u16, decrement: i32) -> i16 {
    let scr_x = i32::from(width) - 1;
    let max = (scr_x / 2 - MIN_HALF_WIDTH).max(0);
    i16::try_from(decrement.clamp(-max, max)).unwrap_or(0)
}

/// The widths of the left and right panels on a screen `width` columns wide: FAR's left panel spans
/// columns `0..=ScrX / 2 - WidthDecrement`.
pub fn panel_widths(width: u16, decrement: i16) -> (u16, u16) {
    let scr_x = i32::from(width) - 1;
    let left = (scr_x / 2 + 1 - i32::from(decrement)).clamp(0, i32::from(width));
    let left = u16::try_from(left).unwrap_or(0);
    (left, width - left)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> (tempfile::TempDir, App) {
        let temp = tempfile::tempdir().unwrap();
        let app = App::new(temp.path(), 100, 40, &crate::settings::Settings::default());
        (temp, app)
    }

    fn visible(app: &App) -> [bool; 2] {
        [app.panels[0].visible, app.panels[1].visible]
    }

    #[test]
    fn ctrl_o_brings_back_the_panels_that_were_shown_as_far_does() {
        let (_temp, mut app) = app();
        app.toggle_panel(Side::Right);
        app.toggle_panels();
        assert_eq!(visible(&app), [false, false]);
        app.toggle_panels();
        assert_eq!(visible(&app), [true, false]);
        assert_eq!(app.active, Side::Left);

        // Hidden one by one (Ctrl+F1), the remembered state still decides (FAR's m_StateBeforeHide).
        app.toggle_panel(Side::Left);
        app.toggle_panels();
        assert_eq!(visible(&app), [true, false]);

        // Nothing remembered as shown: both come back.
        app.toggle_panel(Side::Left);
        app.remembered.panels_before_hide = [false, false];
        app.toggle_panels();
        assert_eq!(visible(&app), [true, true]);
    }

    #[test]
    fn ctrl_f1_and_ctrl_f2_hide_and_show_moving_the_focus_as_far_does() {
        let (_temp, mut app) = app();
        app.toggle_panel(Side::Left);
        assert_eq!(visible(&app), [false, true]);
        assert_eq!(app.active, Side::Right, "the shown panel takes the focus");
        app.toggle_panel(Side::Right);
        assert_eq!(visible(&app), [false, false]);
        assert!(app.panels_hidden());
        app.toggle_panel(Side::Left);
        assert_eq!(visible(&app), [true, false]);
        assert_eq!(app.active, Side::Left, "the only shown panel is active");
        app.toggle_panel(Side::Right);
        assert_eq!(app.active, Side::Left, "showing the second keeps the focus");
    }

    #[test]
    fn ctrl_p_toggles_the_passive_panel_only_while_the_active_is_shown() {
        let (_temp, mut app) = app();
        app.toggle_passive_panel();
        assert_eq!(visible(&app), [true, false]);
        app.switch_panel();
        assert_eq!(app.active, Side::Left, "Tab does not go to a hidden panel");
        app.toggle_passive_panel();
        assert_eq!(visible(&app), [true, true]);
        app.toggle_panel(Side::Left);
        assert_eq!(app.active, Side::Right);
        app.panels[1].visible = false;
        app.toggle_passive_panel();
        assert_eq!(visible(&app), [false, false]);
    }

    #[test]
    fn swapping_moves_the_active_panel_with_its_state() {
        let (_temp, mut app) = app();
        app.panels[1].visible = false;
        let left_path = app.panels[0].path.clone();
        app.swap_panels();
        assert_eq!(visible(&app), [false, true]);
        assert_eq!(app.active, Side::Right);
        assert_eq!(app.active_panel().path, left_path);
        app.panels[1].visible = false;
        app.swap_panels();
        assert_eq!(
            app.active,
            Side::Right,
            "nothing to swap while both are hidden"
        );
    }

    #[test]
    fn the_split_moves_within_far_limits() {
        let (_temp, mut app) = app();
        assert_eq!(panel_widths(100, 0), (50, 50));
        assert_eq!(panel_widths(101, 0), (51, 50));
        app.change_width_decrement(1);
        assert_eq!(panel_widths(app.width, app.width_decrement), (49, 51));
        app.change_width_decrement(1000);
        assert_eq!(app.width_decrement, 39, "ScrX / 2 - 10 for 100 columns");
        app.change_width_decrement(-1000);
        assert_eq!(app.width_decrement, -39);
        assert_eq!(panel_widths(app.width, app.width_decrement), (89, 11));
        app.resize(30, 20);
        assert_eq!(app.width_decrement, -4);
    }

    #[test]
    fn hiding_the_key_bar_gives_its_row_to_the_panels() {
        let (_temp, mut app) = app();
        assert_eq!(app.bottom_rows(), 2);
        app.toggle_key_bar();
        assert_eq!(app.bottom_rows(), 1);
    }
}
