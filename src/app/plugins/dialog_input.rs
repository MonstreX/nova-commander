//! A plugin's dialog whose handler hears the keys and clicks first (FAR's `DN_CONTROLINPUT`, asked by
//! `DIALOG_KEYS`): each key or click goes to the handler before the dialog, named as the plugins name
//! keys ("Ctrl+Enter", "MsLClick"), with the values told before it; the dialog does it once the handler
//! lets it go, nothing when the handler took it. The screen never waits meanwhile: the keys after it are
//! held and come after the answer, in order, as FAR's synchronous handler leaves them in the console's
//! queue (the mouse's records too: a button let go after its press); Esc once the handler has kept the key past
//! [`QUICK`](crate::app::held::QUICK) takes the dialog away (Esc for the plugin), as a plugin not
//! answering never keeps the user in it. Also here: the dialog drawn anew told (FAR's `DN_DRAWDIALOG`,
//! `DN_RESIZECONSOLE`), and the dialog put where the plugin asked.

use std::time::Instant;

use ratatui::crossterm::event::KeyEvent;

use super::super::keys::PLUGIN_STOPPED;
use super::super::menu::key_name;
use crate::app::console_input::{Mouse, MouseAction};
use crate::app::dialog::Dialog;
use crate::app::held::{Held, Press};
use crate::app::{App, Overlay};
use crate::plugin_host::windows::DialogEventKind;

/// What the handler hears first.
#[derive(Debug, Clone, Copy)]
enum Input {
    Key(KeyEvent),
    Mouse(Mouse),
}

/// A key or click sent to the handler and not answered yet, the keys after it and the mouse's records
/// (each with the number of keys held before it).
#[derive(Debug)]
pub(super) struct InputWait {
    input: Input,
    held: Held,
    mice: Vec<(usize, Mouse)>,
}

impl InputWait {
    /// What was held after it, in the order it came.
    fn held_input(self) -> Vec<Input> {
        let mut mice = self.mice.into_iter().peekable();
        let mut all = Vec::new();
        for (at, key) in self.held.keys.into_iter().enumerate() {
            while let Some((_, mouse)) = mice.next_if(|(before, _)| *before <= at) {
                all.push(Input::Mouse(mouse));
            }
            all.push(Input::Key(key));
        }
        all.extend(mice.map(|(_, mouse)| Input::Mouse(mouse)));
        all
    }
}

/// FAR's names of a click (`MsLClick`, `MsRClick`, `MsM1Click`), its modifiers before it; a double one
/// `MsLDblClick` (FAR tells it by the record's flag).
fn click_name(mouse: &Mouse) -> String {
    let pressed = mouse.pressed;
    let double = mouse.action == MouseAction::DoubleClick;
    let button = match (pressed.right, pressed.middle, double) {
        (true, _, _) => "MsRClick",
        (false, true, _) => "MsM1Click",
        (false, false, true) => "MsLDblClick",
        (false, false, false) => "MsLClick",
    };
    let modifiers = mouse.modifiers;
    let mut name = String::new();
    for (modifier, prefix) in [
        (ratatui::crossterm::event::KeyModifiers::CONTROL, "Ctrl+"),
        (ratatui::crossterm::event::KeyModifiers::ALT, "Alt+"),
        (ratatui::crossterm::event::KeyModifiers::SHIFT, "Shift+"),
    ] {
        if modifiers.contains(modifier) {
            name.push_str(prefix);
        }
    }
    name.push_str(button);
    name
}

impl App {
    /// Whether the dialog in front has a handler hearing its input first, now (not while the handler
    /// lets one go, nor while a list of it is open: the list takes the keys).
    fn plugin_dialog_hears_input(&self) -> bool {
        let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &self.overlay else {
            return false;
        };
        let hears = (self.plugins.dialog.as_ref())
            .is_some_and(|session| session.keys && !session.passing && session.hidden.is_none());
        hears && self.plugin_dialog_shown() && !crate::app::tree_panel::target_tree::list_open(form)
    }

    /// A key in the dialog whose handler hears the keys: sent to it (true), or held while it answers one
    /// (Esc past `QUICK` taking the dialog away).
    pub(in crate::app) fn plugin_dialog_key(&mut self, key: KeyEvent) -> bool {
        if !self.plugin_dialog_hears_input() {
            return false;
        }
        let Some(session) = self.plugins.dialog.as_mut() else {
            return false;
        };
        if let Some(wait) = &mut session.wait {
            if wait.held.press(key) == Press::Abandon {
                // The notice in front first: a dialog under this one (nested) waits aside under it.
                if let Some((mut form, _)) = self.take_plugin_dialog() {
                    self.keep_form_histories(&mut form, false);
                }
                let lines = PLUGIN_STOPPED.iter().map(ToString::to_string).collect();
                self.show_dialog(Dialog::message("Stopped", lines));
                self.end_plugin_dialog();
            }
            return true;
        }
        if session.closing.is_some() {
            return false;
        }
        let Some(name) = key_name(key) else {
            return false;
        };
        self.send_dialog_input(Input::Key(key), |item| DialogEventKind::Key {
            item,
            key: name,
        })
    }

    /// A click in the dialog whose handler hears the clicks: sent to it (true); any mouse record held
    /// while it answers an input. None: not the handler's.
    pub(in crate::app) fn plugin_dialog_mouse(&mut self, mouse: &Mouse) -> Option<bool> {
        if !self.plugin_dialog_hears_input() {
            return None;
        }
        if let Some(wait) = (self.plugins.dialog.as_mut()).and_then(|session| session.wait.as_mut())
        {
            wait.mice.push((wait.held.keys.len(), *mouse));
            return Some(false);
        }
        let pressed = mouse.pressed;
        let click = matches!(mouse.action, MouseAction::Button | MouseAction::DoubleClick)
            && (pressed.left || pressed.right || pressed.middle);
        let area = self.mouse.dialog.clone()?;
        if !click || !area.frame.contains(mouse.column, mouse.row) {
            return None;
        }
        let session = self.plugins.dialog.as_ref()?;
        if session.closing.is_some() {
            return None;
        }
        let under = (area.controls.iter().rev())
            .find(|control| control.cells.contains(mouse.column, mouse.row))
            .and_then(|control| session.item_of(control.index));
        let key = click_name(mouse);
        let (x, y) = (
            i32::from(mouse.column - area.frame.x),
            i32::from(mouse.row - area.frame.y),
        );
        let sent = self.send_dialog_input(Input::Mouse(*mouse), |_| DialogEventKind::Mouse {
            item: under,
            key,
            x,
            y,
        });
        Some(sent)
    }

    /// `input` told to the handler as `kind` (of the focused item), the values told first; its answer
    /// awaited. True when it went (a plugin gone takes the dialog away).
    fn send_dialog_input(
        &mut self,
        input: Input,
        kind: impl FnOnce(Option<usize>) -> DialogEventKind,
    ) -> bool {
        let Some((form, aside)) = self.take_plugin_dialog() else {
            return false;
        };
        let Some(session) = self.plugins.dialog.as_mut() else {
            self.put_plugin_dialog(form, aside);
            return false;
        };
        session.tell_changes(&form);
        let focused = session.item_of(form.focus);
        let told = session.tell(kind(focused), &form);
        if told {
            session.wait = Some(InputWait {
                input,
                held: Held::new(Instant::now(), None),
                mice: Vec::new(),
            });
        }
        self.put_plugin_dialog(form, aside);
        if !told {
            self.drop_plugin_dialog();
        }
        true
    }

    /// The handler's answer to a key or click: done as the dialog's own unless it took it, then what was
    /// held after it, in order (one waiting for the handler again holds the rest).
    pub(super) fn answered_input(&mut self, wait: InputWait, taken: bool) {
        if !taken {
            let was = self.set_dialog_passing(true);
            self.do_input(wait.input);
            self.set_dialog_passing(was);
        }
        self.release_dialog_input(wait.held_input());
    }

    fn do_input(&mut self, input: Input) {
        match input {
            Input::Key(key) => self.dispatch_key(key),
            Input::Mouse(mouse) => {
                let _ = self.handle_mouse(mouse);
            }
        }
    }

    /// `held` done in order; those after one that waits for the handler again held for it.
    fn release_dialog_input(&mut self, held: Vec<Input>) {
        let mut held = held.into_iter();
        while !self.plugin_dialog_waits()
            && let Some(input) = held.next()
        {
            self.do_input(input);
        }
        let Some(wait) = (self.plugins.dialog.as_mut()).and_then(|session| session.wait.as_mut())
        else {
            return;
        };
        for input in held {
            match input {
                Input::Key(key) => wait.held.keys.push(key),
                Input::Mouse(mouse) => wait.mice.push((wait.held.keys.len(), mouse)),
            }
        }
    }

    /// Whether a key or click goes past the handler now (one it let go, a key a click makes); the
    /// earlier word.
    pub(in crate::app) fn set_dialog_passing(&mut self, passing: bool) -> bool {
        (self.plugins.dialog.as_mut())
            .map(|session| std::mem::replace(&mut session.passing, passing))
            .unwrap_or_default()
    }

    /// A key or click of the dialog waits for its handler.
    pub(in crate::app) fn plugin_dialog_waits(&self) -> bool {
        (self.plugins.dialog.as_ref()).is_some_and(|session| session.wait.is_some())
    }

    /// The dialog in front drawn on a screen the plugin has not heard of (shown, back in front, the
    /// screen resized): told (FAR's `DN_DRAWDIALOG`).
    pub(super) fn tell_plugin_dialog_drawn(&mut self) {
        let screen = (self.width, self.height);
        let fresh = (self.plugins.dialog.as_ref())
            .is_some_and(|session| session.drawn != Some(screen) && session.closing.is_none());
        if !fresh || !self.plugin_dialog_shown() {
            return;
        }
        let Some((form, aside)) = self.take_plugin_dialog() else {
            return;
        };
        if let Some(session) = self.plugins.dialog.as_mut() {
            session.drawn = Some(screen);
            let (columns, rows) = screen;
            let _ = session.tell(DialogEventKind::Draw { columns, rows }, &form);
        }
        self.put_plugin_dialog(form, aside);
    }

    /// The dialog in front put where its plugin asked (FAR's `X1`, `Y1`): the next draw turns the place
    /// into the shift the dialogs are moved by, in the room and the size it draws them with.
    pub(super) fn place_plugin_dialog(&mut self) {
        let Some(session) = self.plugins.dialog.as_ref() else {
            return;
        };
        if session.at != (None, None) {
            self.dialog_place.at = Some(session.at);
        }
    }
}
