//! A plugin's dialog in the program (ADR-0016 §11): laid out as FAR's (or at the plugin's own
//! coordinates and place), its items' changes told to the plugin's handler as they happen (the values
//! compared after each key and click, and each tick), the focus moving and the dialog drawn anew told too,
//! the handler's answers applied when they come, the program never waiting for them but on closing (a
//! button or Esc), when an answer may keep the dialog open, and for a key or click the handler hears first
//! (`dialog_input`). While a window of the same plugin is over it (a message its handler shows) the dialog
//! waits aside. The dialog gone another way is Esc for the plugin. Its lines' histories are kept as it
//! closes, a button closing it adding their texts (FAR's `CloseDialog`).

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use super::{Pending, PluginForm};
use crate::app::dialog::{Dialog, FormAction};
use crate::app::form::{Control, Form, ListLine};
use crate::app::{App, Overlay};
use crate::plugin_host::windows::{
    Change, DialogEventKind, DialogMessage, DialogRequest, Handled, ItemKind, ItemValue,
};

#[path = "dialog_input.rs"]
mod input;
#[path = "dialog_layout.rs"]
mod layout;

/// A plugin's dialog while it is up.
#[derive(Debug)]
pub struct DialogSession {
    pub plugin: String,
    talk: Sender<DialogMessage>,
    kinds: Vec<ItemKind>,
    /// The form's item of each of the plugin's.
    items: Vec<usize>,
    no_close: Vec<bool>,
    /// The values as the plugin last heard them.
    known: Vec<ItemValue>,
    /// A handler hears the events.
    handled: bool,
    /// The answers awaited, in the order the events went.
    pending: VecDeque<(DialogEventKind, Receiver<Handled>)>,
    /// Closing, waiting for the handler's word: the dialog as the handler was told it (what the user does
    /// meanwhile is not what it approved).
    closing: Option<Box<Form>>,
    /// The dialog while another window of its plugin is over it.
    pub hidden: Option<Box<Dialog>>,
    /// The handler hears the keys and clicks first (FAR's `DN_CONTROLINPUT`).
    keys: bool,
    /// The form's item with the focus as the plugin last heard.
    focus: usize,
    /// The screen the plugin last heard the dialog drawn on (none: not since it came in front).
    drawn: Option<(u16, u16)>,
    /// FAR's `X1`, `Y1` the plugin asked for (none: centred).
    at: (Option<u16>, Option<u16>),
    /// A key or click the handler has not answered yet, the keys after it held.
    wait: Option<input::InputWait>,
    /// A key or click the handler let go, done as the dialog's own.
    passing: bool,
}

impl DialogSession {
    /// The plugin's item a form's item shows.
    fn item_of(&self, form_item: usize) -> Option<usize> {
        self.items.iter().position(|&shown| shown == form_item)
    }

    /// Every item's value in `form`.
    fn values(&self, form: &Form) -> Vec<ItemValue> {
        self.items.iter().map(|&item| value(form, item)).collect()
    }

    /// An event for the handler with the values in `form`; none when no handler hears. False when the
    /// plugin's thread is no longer in the dialog.
    fn tell(&mut self, kind: DialogEventKind, form: &Form) -> bool {
        if !self.handled {
            return true;
        }
        let (reply, answer) = channel();
        let values = self.values(form);
        let message = DialogMessage::Event {
            kind: kind.clone(),
            values,
            reply,
        };
        if self.talk.send(message).is_err() {
            return false;
        }
        self.pending.push_back((kind, answer));
        true
    }

    /// The items changed since the plugin last heard, told one by one, then the focus moved (FAR's
    /// `DN_KILLFOCUS`, `DN_GOTFOCUS`).
    fn tell_changes(&mut self, form: &Form) {
        let now = self.values(form);
        for (index, value) in now.iter().enumerate() {
            if self.known.get(index) != Some(value) {
                // A thread gone is seen when an answer is awaited.
                let _ = self.tell(DialogEventKind::Changed(index), form);
            }
        }
        self.known = now;
        if form.focus != self.focus {
            let (left, came) = (self.item_of(self.focus), self.item_of(form.focus));
            self.focus = form.focus;
            if let Some(left) = left {
                let _ = self.tell(DialogEventKind::Unfocus(left), form);
            }
            if let Some(came) = came {
                let _ = self.tell(DialogEventKind::Focus(came), form);
            }
        }
    }

    /// What the plugin knows is `form` as it is now.
    fn know(&mut self, form: &Form) {
        self.known = self.values(form);
        self.focus = form.focus;
    }

    /// The handler's changes made in `form`.
    fn apply(&self, form: &mut Form, changes: Vec<Change>) {
        for change in changes {
            let item = change.item();
            let (Some(&at), Some(&kind)) = (self.items.get(item), self.kinds.get(item)) else {
                continue;
            };
            apply_one(form, at, kind, change);
        }
    }
}

/// The value of form item `item`: a state and a text.
fn value(form: &Form, item: usize) -> ItemValue {
    let (state, text) = match form.control(item) {
        Some(Control::Edit(input)) => (0, input.text().to_string()),
        Some(Control::Masked(input)) => (0, input.text()),
        Some(Control::ListBox(list)) => {
            let cursor = i32::try_from(list.list.cursor).unwrap_or(0);
            (cursor, list.current().to_string())
        }
        Some(Control::Check { label, checked }) => (i32::from(*checked), label.clone()),
        Some(Control::Tristate { label, state, .. }) => {
            let state = match state {
                Some(true) => 1,
                Some(false) => 0,
                None => 2,
            };
            (state, label.clone())
        }
        Some(Control::Radio { label, checked, .. }) => (i32::from(*checked), label.clone()),
        Some(Control::DropDown { lines, chosen, .. }) => {
            let text = match lines.get(*chosen) {
                Some(ListLine::Choice(text)) => text.clone(),
                _ => String::new(),
            };
            (i32::try_from(*chosen).unwrap_or(0), text)
        }
        Some(Control::Text(text) | Control::Label(text) | Control::Button { label: text, .. }) => {
            (0, text.clone())
        }
        _ => (0, String::new()),
    };
    ItemValue { state, text }
}

/// One change of the handler's in form item `at`, of the plugin's kind `kind`.
fn apply_one(form: &mut Form, at: usize, kind: ItemKind, change: Change) {
    match change {
        Change::Text(_, text) => match form.control_mut(at) {
            Some(Control::Edit(_)) => form.set_edit_text(at, &text),
            Some(Control::Masked(input)) => input.set_text(&text),
            Some(Control::ListBox(list)) => list.title = text,
            Some(
                Control::Text(label)
                | Control::Check { label, .. }
                | Control::Tristate { label, .. }
                | Control::Radio { label, .. }
                | Control::Button { label, .. },
            ) => *label = text,
            _ => {}
        },
        Change::State(_, state) => set_state(form, at, kind, state),
        Change::Enabled(_, on) => {
            if let Some(item) = form.items.get_mut(at) {
                item.disabled = !on;
            }
        }
        Change::Visible(_, on) => {
            if let Some(item) = form.items.get_mut(at) {
                item.hidden = !on;
            }
        }
        Change::Focus(_) => form.focus_item(at),
        Change::Lines(_, lines) => set_lines(form, at, lines),
    }
}

/// A list's lines replaced, its choice kept where it still is.
fn set_lines(form: &mut Form, at: usize, lines: Vec<String>) {
    let lines: Vec<ListLine> = lines.into_iter().map(ListLine::Choice).collect();
    match form.control_mut(at) {
        Some(Control::DropDown {
            lines: shown,
            chosen,
            open,
            ..
        }) => {
            *chosen = (*chosen).min(lines.len().saturating_sub(1));
            *open = None;
            *shown = lines;
        }
        Some(Control::ListBox(list)) => list.set_lines(lines),
        _ => {}
    }
}

fn set_state(form: &mut Form, at: usize, kind: ItemKind, state: i32) {
    match (kind, form.control_mut(at)) {
        (ItemKind::Check, Some(Control::Check { checked, .. })) => *checked = state == 1,
        (ItemKind::Check, Some(Control::Tristate { .. })) => {
            let to = match state {
                1 => Some(true),
                2 => None,
                _ => Some(false),
            };
            form.set_tristate(at, to);
        }
        (ItemKind::Radio, Some(Control::Radio { checked, .. })) => {
            if state == 1 {
                form.select_radio(at);
            } else {
                *checked = false;
            }
        }
        (ItemKind::List, Some(Control::DropDown { lines, chosen, .. })) => {
            let last = lines.len().saturating_sub(1);
            *chosen = usize::try_from(state).unwrap_or(0).min(last);
        }
        (ItemKind::ListBox, Some(Control::ListBox(list))) => {
            list.set_cursor(usize::try_from(state).unwrap_or(0));
        }
        _ => {}
    }
}

impl App {
    /// A plugin's dialog shown; its handler hears Init. One its dialog's handler asks for comes over that
    /// dialog, which waits aside until it is gone (FAR's nested modal dialogs).
    pub(super) fn open_plugin_dialog(
        &mut self,
        plugin: String,
        request: DialogRequest,
        talk: Sender<DialogMessage>,
    ) {
        if let Some(outer) = self.plugins.dialog.take() {
            if outer.plugin != plugin || outer.hidden.is_none() {
                // Another plugin's, or its dialog not in front (only a handler, its thread being in that
                // dialog, asks while it is): the plugin hears Esc.
                self.plugins.dialog = Some(outer);
                return;
            }
            self.plugins.outer_dialogs.push(outer);
        }
        let laid = layout::lay_out(&request, self.width);
        let mut session = DialogSession {
            plugin,
            talk,
            kinds: request.items.iter().map(|item| item.kind).collect(),
            items: laid.items,
            no_close: request.items.iter().map(|item| item.no_close).collect(),
            known: Vec::new(),
            handled: request.handled,
            pending: VecDeque::new(),
            closing: None,
            hidden: None,
            keys: request.keys,
            focus: 0,
            drawn: None,
            at: (request.x, request.y),
            wait: None,
            passing: false,
        };
        let mut form = laid.form;
        // The lines' histories first: an empty one taking its last line is what the plugin hears.
        self.give_histories(&mut form);
        session.know(&form);
        if !session.tell(DialogEventKind::Init, &form) {
            // The plugin's thread is no longer there: nothing to show.
            self.end_plugin_dialog();
            return;
        }
        self.plugins.dialog = Some(session);
        // `show_dialog` without its histories, given above.
        self.forget_dialog_place();
        self.overlay = Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Plugin(PluginForm::Dialog),
        }));
        self.place_plugin_dialog();
    }

    /// The plugin's dialog up in front.
    fn plugin_dialog_shown(&self) -> bool {
        matches!(
            &self.overlay,
            Some(Overlay::Dialog(Dialog::Form {
                action: FormAction::Plugin(PluginForm::Dialog),
                ..
            }))
        )
    }

    /// The dialog's form taken out to work on, and whether it was aside (else in front).
    fn take_plugin_dialog(&mut self) -> Option<(Form, bool)> {
        let session = self.plugins.dialog.as_mut()?;
        let (dialog, aside) = match session.hidden.take() {
            Some(dialog) => (*dialog, true),
            None if self.plugin_dialog_shown() => match self.overlay.take() {
                Some(Overlay::Dialog(dialog)) => (dialog, false),
                _ => return None,
            },
            None => return None,
        };
        match dialog {
            Dialog::Form { form, .. } => Some((form, aside)),
            _ => None,
        }
    }

    /// The dialog's form put back where it was.
    fn put_plugin_dialog(&mut self, form: Form, aside: bool) {
        let dialog = Dialog::Form {
            form,
            action: FormAction::Plugin(PluginForm::Dialog),
        };
        match self.plugins.dialog.as_mut() {
            Some(session) if aside => session.hidden = Some(Box::new(dialog)),
            _ => self.overlay = Some(Overlay::Dialog(dialog)),
        }
    }

    /// The dialog's tick: gone another way (Esc for the plugin), back from aside, its changes told (not while
    /// it is closing), the handler's answers applied, closed when the handler lets it. True when the screen
    /// changed.
    pub(super) fn pump_plugin_dialog(&mut self) -> bool {
        let Some(session) = self.plugins.dialog.as_ref() else {
            return false;
        };
        if session.hidden.is_none() && !self.plugin_dialog_shown() {
            // Dropping the session drops its line to the plugin: Esc.
            self.end_plugin_dialog();
            return false;
        }
        let back = self.restore_plugin_dialog();
        self.tell_plugin_dialog();
        self.tell_plugin_dialog_drawn();
        back | self.take_dialog_answers()
    }

    /// The dialog in front: what changed since the plugin last heard told now (after each key and click),
    /// not while it closes or a key waits for the handler.
    pub(in crate::app) fn tell_plugin_dialog(&mut self) {
        if !self.plugin_dialog_shown() {
            return;
        }
        if let Some((form, aside)) = self.take_plugin_dialog() {
            if let Some(session) = self.plugins.dialog.as_mut()
                && session.closing.is_none()
                && session.wait.is_none()
            {
                session.tell_changes(&form);
            }
            self.put_plugin_dialog(form, aside);
        }
    }

    /// The dialog back in front from aside once the window over it is gone; true when it came back.
    pub(super) fn restore_plugin_dialog(&mut self) -> bool {
        let aside = (self.plugins.dialog.as_ref()).is_some_and(|session| session.hidden.is_some());
        if !aside || self.overlay.is_some() || self.plugins.reply.is_some() {
            return false;
        }
        let Some((form, _)) = self.take_plugin_dialog() else {
            return false;
        };
        self.put_plugin_dialog(form, false);
        self.place_plugin_dialog();
        true
    }

    /// The handler's answers come so far, in order.
    fn take_dialog_answers(&mut self) -> bool {
        let mut changed = false;
        loop {
            let Some(session) = self.plugins.dialog.as_mut() else {
                return changed;
            };
            let Some((kind, answer)) = session.pending.front() else {
                return changed;
            };
            let kind = kind.clone();
            let handled = match answer.try_recv() {
                Ok(handled) => handled,
                Err(TryRecvError::Empty) => return changed,
                Err(TryRecvError::Disconnected) => {
                    // The plugin's thread is gone: so is its dialog, and the keys held for it (typed for
                    // the dialog, never for what is under it: as Esc letting a handler go drops them).
                    session.wait = None;
                    self.drop_plugin_dialog();
                    return true;
                }
            };
            session.pending.pop_front();
            changed = true;
            let input = matches!(
                kind,
                DialogEventKind::Key { .. } | DialogEventKind::Mouse { .. }
            );
            let wait = if input { session.wait.take() } else { None };
            let mut keep_known = false;
            if let DialogEventKind::Close(button) = kind {
                let told = session.closing.take();
                if !handled.keep_open {
                    self.close_plugin_dialog(button, told.map(|form| *form), handled.changes);
                    return true;
                }
                // Kept open: the handler knows the dialog as it was told with its changes; what the user
                // did meanwhile is told as changes on the next tick.
                if let Some(mut told) = told {
                    session.apply(&mut told, handled.changes.clone());
                    session.known = session.values(&told);
                    keep_known = true;
                }
            }
            self.apply_dialog_answer(handled.changes, keep_known);
            if let Some(wait) = wait {
                self.answered_input(wait, handled.taken);
            }
        }
    }

    /// The handler's changes made; what the plugin knows is then the dialog as it is unless `keep_known`.
    fn apply_dialog_answer(&mut self, changes: Vec<Change>, keep_known: bool) {
        let Some((mut form, aside)) = self.take_plugin_dialog() else {
            return;
        };
        if let Some(session) = self.plugins.dialog.as_mut() {
            session.apply(&mut form, changes);
            if !keep_known {
                // What the handler set is what the plugin knows; a focus it moved is told as moved
                // (FAR's `DM_SETFOCUS` sends `DN_KILLFOCUS`, `DN_GOTFOCUS`).
                session.known = session.values(&form);
            }
        }
        self.put_plugin_dialog(form, aside);
    }

    /// The dialog closed by `button` (none: Esc): the plugin hears the values of `told` (the dialog as the
    /// handler was told it; none: as it is) with the handler's last `changes` made.
    fn close_plugin_dialog(
        &mut self,
        button: Option<usize>,
        told: Option<Form>,
        changes: Vec<Change>,
    ) {
        let shown = self.take_plugin_dialog().map(|(form, _)| form);
        let Some(session) = self.plugins.dialog.take() else {
            return;
        };
        if let Some(mut form) = told.or(shown) {
            session.apply(&mut form, changes);
            // FAR's `CloseDialog`: an exit code (a button) adds the lines' texts to their histories.
            self.keep_form_histories(&mut form, button.is_some());
            let values = session.values(&form);
            let _ = session.talk.send(DialogMessage::Closed { button, values });
        }
        self.end_plugin_dialog();
    }

    /// The dialogs of plugin `name` gone, its plugin let go (P6.0e): the one up, those aside under it.
    pub(super) fn drop_dialogs_of(&mut self, name: &str) {
        let ours = |session: &DialogSession| session.plugin.eq_ignore_ascii_case(name);
        (self.plugins.outer_dialogs).retain(|session| !ours(session));
        if self.plugins.dialog.as_ref().is_some_and(ours) {
            let _ = self.take_plugin_dialog();
            self.plugins.dialog = self.plugins.outer_dialogs.pop();
            self.restore_plugin_dialog();
        }
    }

    fn drop_plugin_dialog(&mut self) {
        // What its histories' lists changed is kept, nothing added (no button closed it).
        if let Some((mut form, _)) = self.take_plugin_dialog() {
            self.keep_form_histories(&mut form, false);
        }
        self.end_plugin_dialog();
    }

    /// The dialog up is over: the one under it (whose handler opened it), aside, is the plugin's dialog
    /// again and comes back in front once nothing is over it.
    fn end_plugin_dialog(&mut self) {
        self.plugins.dialog = self.plugins.outer_dialogs.pop();
        // At once: no other window may take the screen in between.
        self.restore_plugin_dialog();
    }

    /// A button pressed (its form item) or Esc in the dialog, the form as it was: back in front, the
    /// changes told first; a button that does not close is heard, any other closes as the handler lets.
    /// Esc again while the handler has not answered closing takes the dialog away (a plugin not answering
    /// never keeps the user in it); a plugin's thread no longer in the dialog does too.
    pub(super) fn finish_plugin_dialog(&mut self, form: &Form, item: Option<usize>) {
        // Back where it stood, as it was (no history given again).
        self.overlay = Some(Overlay::Dialog(Dialog::Form {
            form: form.clone(),
            action: FormAction::Plugin(PluginForm::Dialog),
        }));
        let Some(mut session) = self.plugins.dialog.take() else {
            self.overlay = None;
            return;
        };
        if session.closing.is_some() {
            self.plugins.dialog = Some(session);
            if item.is_none() {
                self.drop_plugin_dialog();
            }
            return;
        }
        session.tell_changes(form);
        let button = item.and_then(|item| session.item_of(item));
        let handled = session.handled;
        let no_close = button.filter(|&button| session.no_close[button]);
        let told = match no_close {
            Some(button) => session.tell(DialogEventKind::Button(button), form),
            None if handled => {
                session.closing = Some(Box::new(form.clone()));
                session.tell(DialogEventKind::Close(button), form)
            }
            None => true,
        };
        self.plugins.dialog = Some(session);
        if !told {
            self.drop_plugin_dialog();
        } else if !handled && no_close.is_none() {
            self.close_plugin_dialog(button, None, Vec::new());
        }
    }

    /// A window the plugin of the dialog asks for (its handler's message), taken from those waiting: the
    /// dialog goes aside under it.
    pub(super) fn plugin_window_over_dialog(&mut self) -> Option<Pending> {
        let plugin = self.plugins.dialog.as_ref()?.plugin.clone();
        if !self.plugin_dialog_shown() || self.plugins.reply.is_some() {
            return None;
        }
        let at = self
            .plugins
            .waiting
            .iter()
            .position(|pending| pending.asks_for(&plugin))?;
        let pending = self.plugins.waiting.remove(at)?;
        if let Some(Overlay::Dialog(dialog)) = self.overlay.take()
            && let Some(session) = self.plugins.dialog.as_mut()
        {
            session.hidden = Some(Box::new(dialog));
            // Drawn anew when it is back in front.
            session.drawn = None;
        }
        Some(pending)
    }
}

#[cfg(test)]
#[path = "dialog_tests.rs"]
mod tests;
