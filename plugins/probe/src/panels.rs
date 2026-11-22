//! The probe's commands on the program's panels (API 3): what the tests see of `Host::panel` and its
//! changes, written to `panels-said.txt` in the probe's folder.

use std::fmt::Write as _;

use nc_plugin_sdk::Which;

use super::Probe;

impl Probe {
    /// FAR's SameFolder: the passive panel to the active one's folder, its cursor on the same name; from
    /// the drive menu (`from_disk`) the menu's panel goes to the other one's (FAR's `OPEN_LEFTDISKMENU` /
    /// `OPEN_RIGHTDISKMENU` against `PFLAGS_PANELLEFT`).
    pub(super) fn same_folder(&self, from_disk: bool) -> Result<Option<String>, String> {
        let (from, to) = if from_disk {
            let left = self.host.panel(Which::Left).ok_or("no panel")?;
            if left.call_target {
                (Which::Right, Which::Left)
            } else {
                (Which::Left, Which::Right)
            }
        } else {
            (Which::Active, Which::Passive)
        };
        let source = self.host.panel(from).ok_or("no panel")?;
        let items = self.host.panel_items(from);
        let current = items.get(source.current).map(|item| item.name.clone());
        let cursor = current.filter(|name| name != "..");
        if !self.host.set_folder(to, &source.folder, cursor.as_deref()) {
            return Err(format!("{} could not be read", source.folder));
        }
        Ok(None)
    }

    /// In a panel's call (its folder listed), with the marker `panel-in-list`: what the probe reads of the
    /// panels there (as the program last saw them; no items, no changes), in its log.
    pub(super) fn panels_in_list(&self) {
        if !self.asked("panel-in-list") {
            return;
        }
        let state = self.host.panel(Which::Active);
        let items = self.host.panel_items(Which::Active).len();
        let line = self.host.set_command_line("from the list");
        let folder = state.map(|state| state.folder);
        self.host.log(&format!(
            "in the list: {folder:?} items {items} changed {line}"
        ));
    }

    /// The lines of `script.txt` done one by one, each one's answer a line of `panels-said.txt`.
    pub(super) fn panel_script(&self) -> Result<Option<String>, String> {
        let script = self.told("script.txt").unwrap_or_default();
        let mut said = String::new();
        for line in script.lines() {
            let answer = self.script_line(line);
            let _ = writeln!(said, "{line} => {answer}");
        }
        std::fs::write(self.folder.join("panels-said.txt"), said)
            .map_err(|error| error.to_string())?;
        Ok(None)
    }

    fn script_line(&self, line: &str) -> String {
        let words: Vec<&str> = line.split('|').collect();
        let which = match words.get(1).copied() {
            Some("passive") => Which::Passive,
            Some("left") => Which::Left,
            Some("right") => Which::Right,
            _ => Which::Active,
        };
        let arg = |at: usize| words.get(at).copied().unwrap_or("");
        let number = |at: usize| arg(at).parse::<usize>().unwrap_or(0);
        let host = &self.host;
        let done = match words[0] {
            "state" => {
                let state = host.panel(which);
                return state.map_or_else(
                    || "none".to_string(),
                    |state| {
                        format!(
                            "{} {} sort {} reverse {}",
                            state.folder, state.items, state.sort, state.reverse_sort
                        )
                    },
                );
            }
            "names" => {
                let names: Vec<String> = host
                    .panel_items(which)
                    .into_iter()
                    .map(|item| item.name)
                    .collect();
                return format!("{names:?}");
            }
            "selected" => {
                let names: Vec<String> = host
                    .selected_items(which)
                    .into_iter()
                    .map(|item| item.name)
                    .collect();
                return format!("{names:?}");
            }
            "folder" => {
                host.set_folder(which, arg(2), Some(arg(3)).filter(|name| !name.is_empty()))
            }
            "reread" => host.reread(which, arg(2) == "keep"),
            "select" => host.select(which, &[arg(2).to_string()], arg(3) != "off"),
            "selectall" => host.select_all(which, arg(2) != "off"),
            "sort" => host.set_sort(
                which,
                u32::try_from(number(2)).unwrap_or(0),
                arg(3) == "reverse",
            ),
            "close" => host.close_panel(which, Some(arg(2)).filter(|folder| !folder.is_empty())),
            "view" => host.view(std::path::Path::new(arg(2)), arg(3) == "delete"),
            "edit" => host.edit(
                std::path::Path::new(arg(2)),
                number(3),
                number(4),
                arg(5) == "delete",
            ),
            "line" => return host.command_line(),
            other => return format!("no such line {other}"),
        };
        if done { "ok" } else { "failed" }.to_string()
    }

    /// What the panels are, written down; then the command line set, an item selected and the cursor
    /// moved on the active panel.
    pub(super) fn panel_report(&self) -> Result<Option<String>, String> {
        let mut said = String::new();
        for which in [Which::Active, Which::Passive] {
            let state = self.host.panel(which).ok_or("no panel")?;
            let names: Vec<String> = (self.host.panel_items(which).into_iter())
                .map(|item| item.name)
                .collect();
            let selected: Vec<String> = (self.host.selected_items(which).into_iter())
                .map(|item| item.name)
                .collect();
            let _ = writeln!(
                said,
                "{which:?} left={} active={} target={} kind={:?} folder={} items={} names={names:?} selected={selected:?}",
                state.left, state.active, state.call_target, state.kind, state.folder, state.items
            );
        }
        let _ = writeln!(said, "command line={}", self.host.command_line());
        std::fs::write(self.folder.join("panels-said.txt"), said)
            .map_err(|error| error.to_string())?;
        self.host.set_command_line("probe was here");
        self.host
            .select(Which::Active, &["b.txt".to_string()], true);
        self.host.set_cursor_on(Which::Active, "c.txt");
        Ok(None)
    }
}
