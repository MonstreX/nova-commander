//! The plugins in the program (ADR-0016): F11's "Plugin commands" (FAR's `PluginManager::CommandsMenu`:
//! every plugin's items, sorted, Enter runs one on the plugin's thread), the windows a plugin asks for
//! (5b-1: FAR's `Message` with its buttons, a window without buttons kept while it works, `Menu`,
//! `InputBox`; the plugin waits for the answer), a command's failure, and FAR's "Error loading plugin" for
//! one that could not be loaded. One window at a time: what comes while another is up waits for it; a
//! plugin's window without buttons gives way to the next.

use std::collections::VecDeque;
use std::sync::mpsc::Sender;

use ratatui::crossterm::event::KeyEvent;

use super::dialog::{Dialog, FormAction};
use super::{App, Overlay, Side};
use crate::plugin_host::windows::{Answer, MessageRequest, Request};
use crate::plugin_host::{Event, LoadFailure, PluginHost};

mod menu;

pub(super) mod message;

mod dialog;
mod editor_control;

mod getting;
mod hotkeys;
mod keys;
mod own_ops;
mod to_new;
mod wait_window;

mod opening;
pub(in crate::app) use opening::CLICK_PREFIX;

mod info;

pub(in crate::app) use getting::GetView;
pub(in crate::app) use hotkeys::DRIVE_MENU_HOTKEYS;
pub use hotkeys::{HotkeyEdit, HotkeyMenu};

mod manager;
pub use manager::PLUGINS_KEYS;
mod background;
mod panel_actions;
mod panel_control;
mod panels;
mod program_asks;
mod share_up;

mod config;

pub(in crate::app) use menu::MenuOutcome;
pub use menu::{PLUGIN_MENU_ROWS_LEFT, PluginMenu};

/// What waits to be shown.
#[derive(Debug)]
pub(super) enum Pending {
    Event(Event),
    Failure(LoadFailure),
    /// A configuration's menu (and its line) back after it ended, in its turn after the windows the
    /// configuration showed.
    MenuBack(HotkeyMenu, usize),
}

impl Pending {
    /// A window plugin `plugin` waits for: a message, a menu, an input line or a dialog.
    fn asks_for(&self, plugin: &str) -> bool {
        matches!(
            self,
            Self::Event(Event::Ask { plugin: asking, .. } | Event::Dialog { plugin: asking, .. })
                if asking == plugin
        )
    }
}

/// A plugin's message as a form: the index of its first button's item, or none (kept while it works).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginForm {
    Message {
        first_button: usize,
    },
    Busy,
    /// A plugin's dialog (`plugins::dialog`).
    Dialog,
    /// A plugin's input line (`message::input_form`): FAR's `FIB_ENABLEEMPTY`, `FIB_EXPANDENV`.
    Input {
        enable_empty: bool,
        expand_env: bool,
    },
}

#[derive(Debug, Default)]
pub struct Plugins {
    // Before the host: as the program ends, a plugin waiting for an answer hears Esc first, then its
    // thread is ended (fields drop in their order).
    /// The answer a plugin's window waits for while it is up.
    reply: Option<Sender<Answer>>,
    /// The plugin of that window.
    reply_of: Option<String>,
    /// The plugin whose window without buttons is up.
    busy: Option<String>,
    /// The call that showed the busy window: Esc there asks that one to stop.
    busy_call: u64,
    /// A plugin's dialog while it is up (its line to the plugin dropped first too).
    dialog: Option<dialog::DialogSession>,
    /// The dialogs under it, each aside, whose handlers opened the one above (FAR's nested modal dialogs).
    outer_dialogs: Vec<dialog::DialogSession>,
    waiting: VecDeque<Pending>,
    /// The plugins' panels the panels showed at the last tick (one path of each instance).
    panels: Vec<crate::vfs::VfsPath>,
    /// The plugins' panels opened and not shown yet, with the panel reading them.
    opening: Vec<(crate::vfs::VfsPath, super::Side)>,
    /// A put sent and not ended.
    pub(super) putting: Option<panels::Putting>,
    /// A configuration sent and not ended: its plugin, and the menu and line that come back after it.
    configuring: Option<(String, HotkeyMenu, usize)>,
    /// A key a plugin's panel's plugin left to the program is being done: the program's own.
    passing: bool,
    /// A panel key its plugin has not answered yet, the keys after it held (FAR's `ProcessPanelInput` is
    /// synchronous).
    key_wait: Option<keys::KeyWait>,
    /// The presses whose calls opened a window (their keys let go to it): their answers are still
    /// applied in full when they come, as after FAR's synchronous call.
    key_late: Vec<keys::KeyWait>,
    /// The last panel key's number.
    key_serial: u64,
    /// The plugins' requests of the panels not answered yet: their panels are being read (API 3).
    panel_asks: Vec<panel_control::PanelAsk>,
    /// The plugins' requests of the editor not answered yet (API 10), and the editor's window the call
    /// asking them was started in (review D2: answered against that editor only).
    editor_asks: VecDeque<editor_control::EditorAsk>,
    editor_window: Option<u64>,
    /// An editor key's plugins not told it yet, told in turn (review D11).
    editor_key_rest: Option<editor_control::EditorKeyRest>,
    /// The keys plugins posted (FAR's `ACTL_POSTKEYSEQUENCE`) with the plugin each, done as typed once
    /// its call has ended.
    posted_keys: std::collections::VecDeque<(String, ratatui::crossterm::event::KeyEvent)>,
    /// The files a plugin's viewer or editor delete as they close (FAR's `VF_DELETEONCLOSE`).
    delete_on_close: Vec<std::path::PathBuf>,
    /// "Plugins configuration" opened by F11's Alt+Shift+F9: F11's line to come back to after it.
    commands_back: Option<usize>,
    /// The plugins that asked to be unloaded themselves, let go once their calls end (FAR's
    /// `UnloadPluginExternal` marks an active one).
    unload_after: Vec<String>,
    /// The drive menu Apps called EMenu from, with that plugin: back once its call ends, so that the keys it
    /// posts (Shift+F6 for "rename") reach the menu, as FAR's stays under EMenu's.
    pub(in crate::app) drive_menu_under: Option<(String, crate::app::DriveMenu)>,
    /// The plugins let go while the requests are answered (by another plugin's request): theirs taken
    /// out already are not answered.
    gone: Vec<String>,
    pub host: PluginHost,
    /// The settings' keys the program does not name (`plugins.<name>.enabled`), written back as read.
    pub kept_settings: serde_json::Map<String, serde_json::Value>,
}

/// How long a menu of the plugins' items waits for them, asked anew as it opens (FAR's `GetPluginInfo`
/// is waited for; here a plugin busy with other work keeps the items it told last, ADR-0019).
pub(in crate::app) const ITEMS_WAIT: std::time::Duration = std::time::Duration::from_millis(100);

impl Plugins {
    /// None loaded yet; the settings' other keys kept to be written back.
    pub fn keeping(settings: &crate::settings::Settings) -> Self {
        Self {
            kept_settings: settings.other.clone(),
            ..Self::default()
        }
    }
}

impl App {
    /// The plugins loaded at the start; those that could not be are reported (FAR's message).
    pub fn set_plugins(&mut self, mut host: PluginHost) {
        let failures = std::mem::take(&mut host.failures);
        self.plugins
            .waiting
            .extend(failures.into_iter().map(Pending::Failure));
        self.plugins.host = host;
        self.plugins.set_drive_menu_hotkeys();
    }

    /// F11: FAR's "Plugin commands": every plugin's items, sorted by their text (`plugins::hotkeys`).
    pub(super) fn open_plugin_commands(&mut self) {
        self.open_plugin_commands_at(0);
    }

    /// Enter in "Plugin commands": the command runs on its plugin's thread, waited for (ADR-0019).
    pub(super) fn run_plugin_command(&mut self, commands: &[(usize, usize)], index: usize) {
        if let Some(&(plugin, command)) = commands.get(index) {
            let side = self.command_side();
            self.run_plugin_item(plugin, side, |host, serial| {
                host.run_command(plugin, command, side.index(), serial)
            });
        }
    }

    /// The drive menu item `index` of plugin `plugin` chosen for the panel `target`, as the menu does (tests).
    #[cfg(test)]
    pub(in crate::app) fn open_disk_for_test(
        &mut self,
        plugin: usize,
        index: usize,
        target: usize,
    ) {
        self.run_plugin_item(plugin, side_of(target), |host, serial| {
            host.open_disk(plugin, index, target, serial)
        });
    }

    /// F11's command `index` of plugin `plugin` run for the panel `target`, as the menu does (tests;
    /// the plugins load on Windows only).
    #[cfg(all(test, windows))]
    pub(in crate::app) fn run_command_for_test(
        &mut self,
        plugin: usize,
        index: usize,
        target: usize,
    ) {
        self.run_plugin_item(plugin, side_of(target), |host, serial| {
            host.run_command(plugin, index, target, serial)
        });
    }

    /// A command, drive menu item or configuration of the plugin `plugin` sent by `send` with its number:
    /// its end waited for as a panel key's (the keys after it held, a window past 300 ms, Esc stops the
    /// waiting); nothing waited for when it did not reach the plugin's thread.
    pub(super) fn run_plugin_item(
        &mut self,
        plugin: usize,
        side: Side,
        send: impl FnOnce(&PluginHost, u64) -> bool,
    ) {
        let Some(name) = (self.plugins.host.plugins.get(plugin)).map(|loaded| loaded.name.clone())
        else {
            return;
        };
        let enter = KeyEvent::from(ratatui::crossterm::event::KeyCode::Enter);
        let serial = self.wait_for_plugin(keys::WaitKind::Run, &name, side, enter);
        if !send(&self.plugins.host, serial) {
            self.drop_plugin_wait(serial);
        }
    }

    /// What plugins asked or reported, shown one at a time when nothing else is up; true when the screen
    /// is to be drawn again.
    pub(super) fn pump_plugins(&mut self) -> bool {
        // A panel's view or look may have changed: sorted by what it shows.
        self.sync_sort_names();
        self.sync_share_parent();
        let expired = self.expire_plugin_key() | self.resume_plugin_key();
        // A window gone some other way than its own keys: the plugin hears Esc.
        if self.plugins.reply.is_some() && !self.plugin_window_shown() {
            self.answer_plugin(Answer::Esc);
        }
        if self.plugins.busy.is_some() && !self.plugin_busy_shown() {
            self.plugins.busy = None;
        }
        let mut changed = self.pump_plugin_dialog() | expired | self.plugin_wait_due();
        // The panels read since: the requests that waited for them answered.
        changed |= self.answer_panel_asks();
        changed |= self.answer_editor_asks();
        self.delete_closed_files();
        self.publish_panels();
        // A late answer to the items asked as a menu opened, for the next one: not under a menu of
        // them, whose lines name the items by their places.
        if !matches!(
            self.overlay,
            Some(Overlay::PluginMenu(_) | Overlay::DriveMenu(_))
        ) {
            self.plugins.host.pump_items();
        }
        while let Some(event) = self.plugins.host.take_event() {
            match self.take_at_once(event) {
                Some(event) => self.plugins.waiting.push_back(Pending::Event(event)),
                None => changed = true,
            }
        }
        self.close_left_plugin_panels();
        // The handler of the dialog up asks for a window: over the dialog.
        if let Some(over) = self.plugin_window_over_dialog() {
            changed |= self.show_pending(over);
        }
        // A window without buttons gives way; the ends of calls that showed none change nothing. Work the
        // screen waits for (its keys kept for after it) comes first, as other work's windows do.
        while self.plugins.reply.is_none()
            && (!self.screen_busy() || self.plugins.busy.is_some())
            && let Some(next) = self.plugins.waiting.pop_front()
        {
            changed |= self.show_pending(next);
        }
        changed | self.do_posted_keys() | self.pump_plugin_starts()
    }

    /// What is no window of its own is taken at once (any other event is handed back): a panel opens, a
    /// value is kept, a configuration's end changes the lists and panels (its menu back waits its turn).
    fn take_at_once(&mut self, event: Event) -> Option<Event> {
        match event {
            Event::Panel {
                plugin,
                target,
                path,
                serial,
            } => self.plugin_run_panel(&plugin, target, &path, serial),
            Event::Ran { plugin, serial } => self.plugin_run_ran(&plugin, serial),
            Event::PanelAsk {
                plugin,
                request,
                reply,
            } => self.plugin_panel_ask(plugin, request, reply),
            Event::EditorAsk { request, reply, .. } => self.plugin_editor_ask(request, reply),
            Event::Setting { plugin, key, value } => self.keep_plugin_setting(&plugin, &key, value),
            Event::Synchro { plugin, param } => self.plugins.host.synchro(&plugin, param),
            Event::Configured {
                plugin,
                result,
                serial,
            } => self.plugin_run_configured(&plugin, result, serial),
            event => return Some(event),
        }
        None
    }

    /// A plugin's window without buttons, shown while its call `call` works on.
    fn show_busy(
        &mut self,
        plugin: String,
        message: &crate::plugin_host::windows::MessageRequest,
        call: u64,
    ) {
        let form = self.plugin_message_form(&plugin, message);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::Plugin(PluginForm::Busy),
        });
        self.plugins.busy = Some(plugin);
        self.plugins.busy_call = call;
    }

    /// Shows what waited; false when it changed nothing on the screen.
    fn show_pending(&mut self, pending: Pending) -> bool {
        let event = match pending {
            Pending::Event(event) => event,
            Pending::Failure(failure) => {
                let lines = vec![
                    "Error loading plugin".to_string(),
                    failure.path.display().to_string(),
                    failure.reason,
                ];
                self.plugins.busy = None;
                self.show_dialog(Dialog::error("Error", lines));
                return true;
            }
            Pending::MenuBack(menu, selected) => return self.plugin_menu_back(menu, selected),
        };
        match event {
            Event::Done { plugin } => {
                if self.plugins.busy.as_deref() != Some(&plugin) {
                    return false;
                }
                self.plugins.busy = None;
                self.overlay = None;
            }
            Event::Busy {
                plugin,
                message,
                call,
            } => self.show_busy(plugin, &message, call),
            Event::Ask {
                plugin,
                request,
                reply,
            } => {
                self.plugins.busy = None;
                self.plugins.reply = Some(reply);
                self.plugins.reply_of = Some(plugin.clone());
                self.show_request(&plugin, request);
            }
            Event::Dialog {
                plugin,
                request,
                talk,
            } => {
                self.plugins.busy = None;
                self.open_plugin_dialog(plugin, request, talk);
            }
            Event::Failed {
                plugin,
                error,
                serial,
            } => {
                self.plugins.busy = None;
                // An editor key's plugins after a failed one are not told it: an error is up.
                (self.plugins.editor_key_rest).take_if(|rest| Some(rest.serial) == serial);
                self.plugin_key_failed(&plugin, serial);
                // A put that failed takes no selection; an edited copy it was for is kept.
                let mut lines = vec![error];
                lines.extend(self.put_failed(&plugin));
                let title = self.plugin_title(&plugin);
                self.show_dialog(Dialog::error(&title, lines));
            }
            // Taken at once as they come; here only as `take_at_once` hands none of them back.
            event @ (Event::Panel { .. }
            | Event::Ran { .. }
            | Event::PanelAsk { .. }
            | Event::EditorAsk { .. }
            | Event::Setting { .. }
            | Event::Synchro { .. }
            | Event::Configured { .. }) => {
                let _ = self.take_at_once(event);
            }
            Event::Put { plugin, done } => self.put_ended(&plugin, done),
            event @ Event::Got { .. } => self.plugin_got(event),
            event @ (Event::Made { .. } | Event::Processed { .. }) => self.own_ended(event),
            event @ (Event::Created { .. } | Event::DiskFile { .. }) => self.own_ended(event),
            Event::Key {
                plugin,
                side,
                serial,
                answer,
            } => self.plugin_key_done(&plugin, side, serial, answer),
        }
        true
    }

    fn show_request(&mut self, plugin: &str, request: Request) {
        match request {
            Request::Message(message) => {
                let form = self.plugin_message_form(plugin, &message);
                let first_button = form.items.len() - message.buttons.len();
                self.show_dialog(Dialog::Form {
                    form,
                    action: FormAction::Plugin(PluginForm::Message { first_button }),
                });
            }
            Request::Menu(menu) => {
                self.overlay = Some(Overlay::PluginMenu(Box::new(PluginMenu::new(menu))));
            }
            Request::Input(input) => {
                let kind = PluginForm::Input {
                    enable_empty: input.enable_empty,
                    // FAR expands no password line (`dialog.cpp`).
                    expand_env: input.expand_env && !input.password,
                };
                self.show_dialog(Dialog::Form {
                    form: message::input_form(&input),
                    action: FormAction::Plugin(kind),
                });
            }
        }
    }

    /// FAR's message (`plugins::message`); a message without a title is titled by its plugin.
    fn plugin_message_form(&self, plugin: &str, message: &MessageRequest) -> super::form::Form {
        let title = if message.title.is_empty() {
            self.plugin_title(plugin)
        } else {
            message.title.clone()
        };
        // FAR's message keeps the lines the screen has room for: the box, the separator and the buttons
        // (with their shadow) besides them.
        let room = usize::from(self.height.saturating_sub(7)).max(1);
        message::message_form(&title, message, self.width, room)
    }

    /// The plugin's own title, or its name.
    fn plugin_title(&self, name: &str) -> String {
        (self.plugins.host.plugins.iter())
            .find(|plugin| plugin.name == name)
            .map(|plugin| plugin.info.title.clone())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| name.to_string())
    }

    /// A window a plugin waits for is up.
    fn plugin_window_shown(&self) -> bool {
        matches!(
            &self.overlay,
            Some(
                Overlay::PluginMenu(_)
                    | Overlay::Dialog(Dialog::Form {
                        action: FormAction::Plugin(
                            PluginForm::Message { .. } | PluginForm::Input { .. }
                        ),
                        ..
                    })
            )
        )
    }

    /// A plugin waits for the answer of its window, or its busy window is up.
    pub(in crate::app) fn plugin_window_waits(&self) -> bool {
        self.plugins.reply.is_some() || self.plugins.busy.is_some()
    }

    fn plugin_busy_shown(&self) -> bool {
        matches!(
            &self.overlay,
            Some(Overlay::Dialog(Dialog::Form {
                action: FormAction::Plugin(PluginForm::Busy),
                ..
            }))
        )
    }

    /// A plugin's message closed by a button (its item) or Esc; a window without buttons by Esc; a
    /// dialog's button or Esc (`plugins::dialog`).
    pub(super) fn finish_plugin_form(
        &mut self,
        kind: PluginForm,
        form: &super::form::Form,
        item: Option<usize>,
    ) {
        match (kind, item) {
            (PluginForm::Dialog, item) => self.finish_plugin_dialog(form, item),
            (PluginForm::Message { first_button }, Some(item)) => {
                self.answer_plugin(Answer::Button(item.saturating_sub(first_button)));
            }
            (PluginForm::Message { .. }, None) => self.answer_plugin(Answer::Esc),
            // Esc on a plugin's window without buttons reaches the plugin: its call is asked to stop
            // (ADR-0019 §5; FAR's plugins look with `CheckForEsc`).
            (PluginForm::Busy, _) => {
                if let Some(plugin) = self.plugins.busy.take() {
                    self.plugins.host.stop_call(&plugin, self.plugins.busy_call);
                }
            }
            (PluginForm::Input { .. }, None) => self.answer_plugin(Answer::Esc),
            (PluginForm::Input { .. }, Some(item)) if item > message::INPUT_OK => {
                self.answer_plugin(Answer::Esc);
            }
            (
                PluginForm::Input {
                    enable_empty,
                    expand_env,
                },
                Some(_),
            ) => {
                let typed = form.edit_text(message::INPUT_LINE).to_string();
                // FAR's `GetString`: an empty line without `FIB_ENABLEEMPTY` is no answer.
                let answer = if typed.is_empty() && !enable_empty {
                    Answer::Esc
                } else if expand_env {
                    let environment = &self.environment;
                    let expanded = crate::platform::process::expand_variables(&typed, &|name| {
                        environment.get(name)
                    });
                    Answer::Text(expanded)
                } else {
                    Answer::Text(typed)
                };
                self.answer_plugin(answer);
            }
        }
    }

    /// The keys of a plugin's menu.
    pub(super) fn handle_plugin_menu_key(&mut self, mut menu: Box<PluginMenu>, key: KeyEvent) {
        if menu.commands.is_some() {
            return self.plugin_commands_key(menu, key);
        }
        if menu.tasks.is_some() {
            return self.task_list_menu_key(menu, key);
        }
        match menu.handle_key(key) {
            MenuOutcome::Stay => self.overlay = Some(Overlay::PluginMenu(menu)),
            MenuOutcome::Esc => self.answer_plugin(Answer::Esc),
            MenuOutcome::Chosen { item, key } => self.answer_plugin(Answer::Chosen { item, key }),
        }
    }

    /// The user's answer to the window a plugin waits for.
    pub(super) fn answer_plugin(&mut self, answer: Answer) {
        self.plugins.reply_of = None;
        if let Some(reply) = self.plugins.reply.take() {
            let _ = reply.send(answer);
        }
        // A dialog under the window comes back at once: keys typed next are its.
        self.restore_plugin_dialog();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod window_tests;

/// The panel of the plugin host's number (0 the left).
#[cfg(test)]
fn side_of(target: usize) -> Side {
    if target == Side::Right.index() {
        Side::Right
    } else {
        Side::Left
    }
}
