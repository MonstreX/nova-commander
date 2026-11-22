//! A plugin's panel's own keys in the program (ADR-0016 §14, FAR's `ProcessPanelInput` and `KeyBar`): a key
//! the panel names goes to its plugin's thread before the program's own, with the entry under the cursor,
//! the selection and the other panel's folder; the screen never waits for it, and its answer (read a panel
//! again, a panel to a folder or a file, another of the plugin's panels, the command line) is applied when
//! it comes; a key the plugin passes back is then done as the program's own. The keys pressed meanwhile
//! wait for that answer and come after it in order, as FAR's synchronous `ProcessPanelInput` leaves them
//! in the console's queue; a window the plugin opens in that call takes them at once. Esc pressed once
//! the plugin has kept the key past [`QUICK`](super::super::held::QUICK) stops waiting (deviation D14, as D3 for a listing): the
//! key and those held after it are dropped, the plugin's late answer is let be.
//! The key bar shows the labels the panel gives its keys for the modifiers held. The keys held, the window
//! and Esc are [`Held`]'s, as disk work's (R54).

use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::dialog::Dialog;
use super::super::held::{Held, Press};
use super::super::{App, MainView, Screen, Side};
use super::menu::parse;
use super::opening::opens_over;
use crate::platform::input::Modifiers;
use crate::plugin_host::{KeyAnswer, KeyGo, KeyPress};
use crate::vfs::VfsPath;

impl super::Plugins {
    /// A panel key waiting for its plugin, as if one had just been sent (tests).
    #[cfg(test)]
    pub(in crate::app) fn hold_panel_key_for_test(&mut self) {
        self.key_wait = Some(KeyWait {
            serial: 0,
            plugin: "probe".to_string(),
            side: Side::Left,
            key: KeyEvent::from(KeyCode::F(5)),
            path: VfsPath::in_session("probe", "one", "/"),
            cursor: None,
            held: Held::new(Instant::now(), None),
            told: None,
            step: 0,
            kind: WaitKind::Key,
            editor: None,
        });
    }
}

impl App {
    /// Whether a call of `plugin` is waited for with the keys held (its answer not taken yet).
    pub(super) fn plugin_key_waits_for(&self, plugin: &str) -> bool {
        (self.plugins.key_wait.as_ref()).is_some_and(|wait| wait.plugin == plugin)
    }
}

/// What a plugin's answer waited for is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WaitKind {
    /// A key of its panel's own.
    Key,
    /// A command line of its prefix (`plugins::opening`).
    Command,
    /// A file of its masks, Enter or (`alternative`) Ctrl+PgDn on it, `name` its entry's; `next` the
    /// plugins to ask after it (`plugins::opening`); `temp` the program's folder of a plugin's file's copy.
    File {
        path: std::path::PathBuf,
        name: String,
        alternative: bool,
        next: Vec<usize>,
        temp: Option<std::path::PathBuf>,
    },
    /// Alt+F7's found files; `next` the plugins to ask after it (`plugins::opening`).
    FindList {
        found: Vec<crate::vfs::Entry>,
        next: Vec<usize>,
    },
    /// A command of F11's, a drive menu item or a configuration (FAR's synchronous `OpenW`): ends with its
    /// panel, nothing or its failure (ADR-0019, review R13).
    Run,
    /// Up from a share's root (`plugins::share_up`): not taken, the drive menu.
    ShareUp,
    /// Files written out by the plugin (`plugins::getting`): for the viewer or the editor, else F5 / F6.
    Get(super::getting::Getting),
    /// Shift+F1's files of `folder` for a new panel of the plugin's; `next` the plugins to ask after it
    /// (`plugins::to_new`).
    ToNew {
        sources: Vec<std::path::PathBuf>,
        folder: std::path::PathBuf,
        next: Vec<usize>,
    },
    /// Go to on what Alt+F7 found in an archive (`plugins::opening`): the archive's entry `name` opened
    /// over its panel, then its folder `folder` with the cursor on `cursor`.
    Found {
        name: String,
        folder: String,
        cursor: Option<String>,
    },
    /// Shift+F2 / Shift+F3 on a disk's files (`disk_files`): the run, the plugins to ask after this one.
    DiskFile {
        run: super::own_ops::disk_files::DiskRun,
        next: Vec<usize>,
    },
}

/// The window's lines under the plugin's title while a panel key waits for it (the owner's rule: long
/// work shows a window and Esc stops it; FAR's synchronous `ProcessPanelInput` shows none).
pub(super) const PLUGIN_WORKING: &str = "The plugin is working\u{2026}";

/// The notice after Esc stopped waiting for a plugin (ADR-0019 §3).
pub(super) const PLUGIN_STOPPED: [&str; 2] = [
    "The plugin was not waited for.",
    "It may still finish by itself.",
];

/// A panel key (a command, a file) sent to its plugin, not answered yet: the press, where it was, the keys
/// after it.
#[derive(Debug)]
pub(super) struct KeyWait {
    pub(super) serial: u64,
    pub(super) plugin: String,
    pub(super) side: Side,
    key: KeyEvent,
    pub(super) path: VfsPath,
    cursor: Option<String>,
    /// The keys after it, its window, Esc.
    pub(super) held: Held,
    /// The progress and the busy bar's step its window showed last.
    pub(super) told: Option<crate::plugin_host::CallProgress>,
    pub(super) step: u128,
    pub(super) kind: WaitKind,
    /// The editor's window a call of the editor's was made in.
    pub(super) editor: Option<u64>,
}

impl App {
    /// The panel the plugin's call running is for (the drive menu's, the command's): the active one
    /// when none of its calls is waited for.
    pub(super) fn call_side(&self, plugin: &str) -> Side {
        let own = |wait: &&KeyWait| wait.plugin.eq_ignore_ascii_case(plugin);
        (self.plugins.key_wait.iter().chain(&self.plugins.key_late))
            .find(own)
            .map_or(self.active, |wait| wait.side)
    }

    /// A key pressed while a panel key waits for its plugin: kept for after the answer (true); Esc once
    /// the plugin is slow stops the wait, the key and those held dropped (true too).
    pub(in crate::app) fn hold_plugin_key(&mut self, key: KeyEvent) -> bool {
        // A window up over the panel meanwhile takes the keys from now on; gone, an editor's call
        // holds them again.
        self.expire_plugin_key();
        self.resume_plugin_key();
        if self.restart_hanging(key) {
            return true;
        }
        let Some(wait) = &mut self.plugins.key_wait else {
            return false;
        };
        // Esc asks the call itself to stop (the plugin's `cancelled()`, ADR-0019 §5).
        let host = &self.plugins.host;
        wait.held.arm(|| host.stopper(&wait.plugin, wait.serial));
        if wait.held.press(key) == Press::Abandon {
            // A configuration let go brings no menu back over what the user does meanwhile.
            let stopped = self.plugins.key_wait.take();
            // A copy it was for goes: its late answer names no folder.
            stopped.as_ref().map(super::opening::let_wait_go);
            if let Some(stopped) = stopped.filter(|wait| wait.kind == WaitKind::Run) {
                (self.plugins.configuring)
                    .take_if(|(name, ..)| name.eq_ignore_ascii_case(&stopped.plugin));
            }
            let lines = PLUGIN_STOPPED.iter().map(ToString::to_string).collect();
            self.show_dialog(Dialog::message("Stopped", lines));
        }
        true
    }

    /// A window is up over the panel (a plugin's menu or message in its key's call, taking the keys as
    /// FAR's do there), or disk work holds the keys: the wait ends, the keys after it go on. True when
    /// that happened.
    pub(super) fn expire_plugin_key(&mut self) -> bool {
        let expired =
            self.plugins.key_wait.is_some() && (self.overlay.is_some() || self.is_waiting());
        if !expired {
            return false;
        }
        if let Some(mut wait) = self.plugins.key_wait.take() {
            let held = std::mem::take(&mut wait.held.keys);
            self.plugins.key_late.push(wait);
            self.release_plugin_keys(held);
        }
        true
    }

    /// An editor plugin's call whose window has closed while it still runs: the keys held again until
    /// its end, as FAR's synchronous call holds them (review D2: they went on to the editor among the
    /// plugin's changes, and its key loop read none). True when that happened.
    pub(super) fn resume_plugin_key(&mut self) -> bool {
        let free = self.plugins.key_wait.is_none() && self.overlay.is_none() && !self.is_waiting();
        if !free || !matches!(self.screen, Screen::Editor(_)) {
            return false;
        }
        let front = Some(self.front_window());
        let late = &mut self.plugins.key_late;
        let Some(at) = late.iter().rposition(|wait| wait.editor == front) else {
            return false;
        };
        let mut wait = late.remove(at);
        wait.held = Held::new(Instant::now(), None);
        self.plugins.key_wait = Some(wait);
        true
    }

    /// The keys held for a panel key, in order; those after one that waits again are held for that one.
    pub(super) fn release_plugin_keys(&mut self, keys: Vec<KeyEvent>) {
        let mut keys = keys.into_iter();
        while self.plugins.key_wait.is_none()
            && let Some(key) = keys.next()
        {
            self.dispatch_key(key);
        }
        if let Some(wait) = &mut self.plugins.key_wait {
            wait.held.keys.splice(0..0, keys);
        }
    }

    /// A key the active panel's plugin names for its panel: sent to it instead of the program's (FAR asks
    /// `ProcessPanelInput` first); true when it was.
    pub(in crate::app) fn plugin_panel_key(&mut self, key: KeyEvent) -> bool {
        // A key its plugin left to the program is the program's.
        if self.plugins.passing {
            return false;
        }
        let side = self.active;
        let panel = &self.panels[side.index()];
        if !panel.visible
            || !self.shows_files(side)
            || !self.plugins.host.is_plugin_panel(&panel.path)
        {
            return false;
        }
        // FAR: Enter goes to the plugin only with an empty command line.
        let plain_enter =
            key.code == KeyCode::Enter && (key.modifiers - KeyModifiers::SHIFT).is_empty();
        // Del (no modifiers) edits a command line with text: the panel's only with it empty.
        let plain_del = key.code == KeyCode::Delete && key.modifiers.is_empty();
        if (plain_enter || plain_del) && !self.command_line.is_empty() {
            return false;
        }
        let Some(named) = (panel.look.keys.iter())
            .find(|named| parse(&named.key).is_some_and(|wanted| wanted.matches(key)))
        else {
            return false;
        };
        let passive = &self.panels[side.other().index()].path;
        let working = passive
            .as_local()
            .map(|_| passive)
            .or(panel.before_plugins().map(|entered| &entered.from));
        let press = KeyPress {
            key: named.key.clone(),
            current: (panel.current())
                .filter(|entry| !entry.is_parent)
                .map(|entry| entry.name.clone())
                .unwrap_or_default(),
            selected: panel.selected.iter().cloned().collect(),
            passive: (working.and_then(VfsPath::as_local))
                .map(|folder| folder.display().to_string())
                .unwrap_or_default(),
            folder: (panel.before_plugins())
                .and_then(|entered| entered.from.as_local())
                .map(|folder| folder.display().to_string())
                .unwrap_or_default(),
        };
        let path = panel.path.clone();
        let plugin = path.provider().unwrap_or_default().to_string();
        let serial = self.plugins.key_serial + 1;
        if !(self.plugins.host).panel_key(&path, press, side.index(), serial) {
            // Sent nowhere: the program's own key.
            return false;
        }
        self.wait_for_plugin(WaitKind::Key, &plugin, side, key);
        true
    }

    /// The press `key` on the panel `side` sent to `plugin` (a key, a command, a file): its answer waited
    /// for with the keys after it held; the press's number.
    pub(super) fn wait_for_plugin(
        &mut self,
        kind: WaitKind,
        plugin: &str,
        side: Side,
        key: KeyEvent,
    ) -> u64 {
        let panel = &self.panels[side.index()];
        self.plugins.key_serial += 1;
        let serial = self.plugins.key_serial;
        self.plugins.key_wait = Some(KeyWait {
            serial,
            plugin: plugin.to_string(),
            side,
            key,
            path: panel.path.clone(),
            cursor: panel.current().map(|entry| entry.name.clone()),
            held: Held::new(Instant::now(), None),
            told: None,
            step: 0,
            kind,
            editor: None,
        });
        serial
    }

    /// The call just waited for made in the editor in front: its keys held again after its windows.
    pub(super) fn wait_in_editor(&mut self) {
        let front = self.front_window();
        if let Some(wait) = &mut self.plugins.key_wait {
            wait.editor = Some(front);
        }
    }

    /// The wait for the call `serial`, ended: its own (the keys held after it still held), or one a window
    /// of its call let go; none when nothing waits for it any more (Esc stopped the waiting).
    fn plugin_run_ended(&mut self, plugin: &str, serial: u64) -> Option<KeyWait> {
        let ours = |wait: &KeyWait| {
            wait.kind == WaitKind::Run
                && wait.serial == serial
                && wait.plugin.eq_ignore_ascii_case(plugin)
        };
        if let Some(wait) = self.plugins.key_wait.take_if(|wait| ours(wait)) {
            return Some(wait);
        }
        let at = self.plugins.key_late.iter().position(ours)?;
        Some(self.plugins.key_late.remove(at))
    }

    /// The call `serial` not sent after all: nothing waits for it.
    pub(super) fn drop_plugin_wait(&mut self, serial: u64) {
        let wait = self.plugins.key_wait.take_if(|wait| wait.serial == serial);
        wait.as_ref().map(super::opening::let_wait_go);
    }

    /// A command's or drive menu item's panel (review R13): opened when its call was waited for and its
    /// panel still shows what it showed then; else closed as never shown (a late answer let be, D14:
    /// Esc stopped the waiting, or the user went elsewhere while a window of the call let the keys go).
    pub(super) fn plugin_run_panel(
        &mut self,
        plugin: &str,
        target: usize,
        path: &VfsPath,
        serial: u64,
    ) {
        let side = if target == Side::Right.index() {
            Side::Right
        } else {
            Side::Left
        };
        let wait = self.plugin_run_ended(plugin, serial);
        match &wait {
            Some(wait) if self.run_target_unchanged(wait) => {
                self.open_plugin_panel(plugin, target, path)
            }
            _ => self.plugins.opening.push((path.clone(), side)),
        }
        if let Some(wait) = wait {
            self.release_plugin_keys(wait.held.keys);
        }
    }

    /// A command or drive menu item that opened no panel ended: the keys after it go on.
    pub(super) fn plugin_run_ran(&mut self, plugin: &str, serial: u64) {
        let wait = self.plugin_run_ended(plugin, serial);
        // The next plugin an editor key is told to first: the keys held go on waiting for it.
        self.tell_next_editor_key(serial);
        if let Some(wait) = wait {
            self.release_plugin_keys(wait.held.keys);
        }
    }

    /// A configuration ended: its items and panels as it left them (its menu back unless Esc let it go).
    pub(super) fn plugin_run_configured(
        &mut self,
        plugin: &str,
        result: Result<Option<crate::plugin_host::PluginItems>, String>,
        serial: u64,
    ) {
        let wait = self.plugin_run_ended(plugin, serial);
        self.plugin_configured(plugin, result, Some(serial));
        if let Some(wait) = wait {
            self.release_plugin_keys(wait.held.keys);
        }
    }

    /// The panel a call was made for shows what it showed then, on the panels' screen.
    fn run_target_unchanged(&self, wait: &KeyWait) -> bool {
        self.panels[wait.side.index()].path == wait.path
            && matches!(self.screen, Screen::Panels)
            && self.main_view == MainView::Panels
    }

    /// The plugin's answer to the press `serial` of the panel `side`, then the keys held after it.
    pub(super) fn plugin_key_done(
        &mut self,
        plugin: &str,
        side: usize,
        serial: u64,
        answer: KeyAnswer,
    ) {
        // Waited for, or let go by a window of its call (its answer applied all the same).
        let wait = self.take_key_wait(serial);
        let side = if side == Side::Right.index() {
            Side::Right
        } else {
            Side::Left
        };
        match wait {
            Some(wait) if wait.kind != WaitKind::Key => self.opening_done(plugin, wait, answer),
            Some(wait) => {
                self.apply_key_answer(plugin, side, Some(&wait), answer);
                self.release_plugin_keys(wait.held.keys);
            }
            // Not waited for (Esc stopped the wait, or a window took the keys): only the panels read
            // again; where it goes and a key passed back are let be.
            None => {
                // A panel it opened that no panel will show: closed as one never shown is.
                if let Some(KeyGo::Open(instance)) = &answer.go {
                    let path = VfsPath::in_session(plugin, instance, "/");
                    self.plugins.opening.push((path, side));
                }
                let rereads = KeyAnswer {
                    reread: answer.reread,
                    reread_passive: answer.reread_passive,
                    reread_quick_view: answer.reread_quick_view,
                    ..KeyAnswer::default()
                };
                self.apply_key_answer(plugin, side, None, rereads);
            }
        }
    }

    /// A panel key waits for its plugin's answer.
    pub(in crate::app) fn plugin_key_waits(&self) -> bool {
        self.plugins.key_wait.is_some()
    }

    /// A key passed back done as the program's own (only its own press, still waited for); else panels
    /// read again, then where it goes; nothing when the panel no longer shows that plugin.
    fn apply_key_answer(
        &mut self,
        plugin: &str,
        side: Side,
        wait: Option<&KeyWait>,
        answer: KeyAnswer,
    ) {
        let shown = self.panels[side.index()].path.provider();
        if !shown.is_some_and(|provider| provider.eq_ignore_ascii_case(plugin)) {
            return;
        }
        if answer.pass {
            if let Some(wait) = wait {
                self.pass_plugin_key(wait);
            }
            return;
        }
        let other = side.other();
        if answer.reread {
            self.panels[side.index()].reload();
        }
        if answer.reread_passive {
            self.panels[other.index()].reload();
        }
        // TmpPanel's F7: a quick view beside it shows its file again (`PTYPE_QVIEWPANEL`).
        if answer.reread_quick_view && self.quick_view_side() == Some(other) {
            self.reread_quick_view();
        }
        let went = match (
            answer.go,
            wait.and_then(|wait| opens_over(wait.key, &wait.cursor)),
        ) {
            (Some(KeyGo::Open(instance)), Some(name)) => {
                Some(self.open_over(plugin, side, &instance, &name))
            }
            (Some(go), _) => Some(self.go_after_key(side, go)),
            (None, _) => None,
        };
        if let Some(Err(error)) = went {
            self.show_error("Change directory", &error);
        }
    }

    /// The key the plugin left to the program (FAR's `ProcessPanelInput` returning FALSE): the press
    /// itself done as the program's own, never sent to the plugin again; only where it was pressed - its
    /// panel active on the panels' screen with nothing over it, the same folder and entry (else let be).
    fn pass_plugin_key(&mut self, wait: &KeyWait) {
        if !self.still_where(wait) {
            return;
        }
        self.plugins.passing = true;
        // Past the key-release tracking: the key was pressed once, already counted.
        self.dispatch_key(wait.key);
        self.plugins.passing = false;
    }

    /// Whether the panels are as they were at the press: its panel active on the panels' screen with
    /// nothing over it, the same folder and entry.
    pub(super) fn still_where(&self, wait: &KeyWait) -> bool {
        let panel = &self.panels[wait.side.index()];
        self.active == wait.side
            && self.overlay.is_none()
            && matches!(self.screen, Screen::Panels)
            && self.main_view == MainView::Panels
            && panel.path == wait.path
            && panel.current().map(|entry| entry.name.clone()) == wait.cursor
    }

    /// The labels the active panel's plugin gives its function keys for the modifiers `held` (FAR's
    /// `KeyBar`): each key's number and label.
    pub fn plugin_key_labels(&self, held: Modifiers) -> Vec<(u8, String)> {
        let side = self.active;
        let panel = &self.panels[side.index()];
        if !panel.visible
            || !self.shows_files(side)
            || !self.plugins.host.is_plugin_panel(&panel.path)
        {
            return Vec::new();
        }
        (panel.look.keys.iter())
            .filter(|named| !named.label.is_empty())
            .filter_map(|named| {
                let (number, modifiers) = parse(&named.key)?.function_key()?;
                let wanted = Modifiers {
                    shift: modifiers.contains(KeyModifiers::SHIFT),
                    ctrl: modifiers.contains(KeyModifiers::CONTROL),
                    alt: modifiers.contains(KeyModifiers::ALT),
                };
                (wanted == held).then(|| (number, named.label.clone()))
            })
            .collect()
    }
}

#[path = "key_go.rs"]
mod key_go;

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "stop_tests.rs"]
mod stop_tests;
