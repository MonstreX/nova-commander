//! FAR's file associations (`filetype.cpp`, `ProcessLocalFileTypes`): Enter, Ctrl+PgDn, F3, Alt+F3, F4 and
//! Alt+F4 on a file first look for the associations whose masks take its name and that have a command
//! for that key. None: the key does what it does without them. One: its command runs, its special symbols
//! speaking of the file; more: FAR's "Select association" menu, each line the association's description
//! (or its command). The command runs as the user menu's lines do; Enter's goes into the command history.

use super::App;
use super::dialog::{Dialog, ListAction};
use super::metasymbols::expand_label;
use super::user_menu::without_marks;
use crate::settings::{Association, AssociationCommand};
use crate::vfs::FileMask;

/// The key a file was opened with (FAR's `FILETYPE_MODE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssocMode {
    Execute,
    AltExecute,
    View,
    AltView,
    Edit,
    AltEdit,
}

impl AssocMode {
    pub const ALL: [Self; 6] = [
        Self::Execute,
        Self::AltExecute,
        Self::View,
        Self::AltView,
        Self::Edit,
        Self::AltEdit,
    ];

    /// FAR's labels in the association's dialog.
    pub fn label(self) -> &'static str {
        match self {
            Self::Execute => "E&xecute command (used for Enter):",
            Self::AltExecute => "Exec&ute command (used for Ctrl+PgDn):",
            Self::View => "&View command (used for F3):",
            Self::AltView => "V&iew command (used for Alt+F3):",
            Self::Edit => "&Edit command (used for F4):",
            Self::AltEdit => "Edit comm&and (used for Alt+F4):",
        }
    }
}

/// The file an association's command speaks of: the name its masks are matched against, the file its
/// symbols name, and the folder it runs in when that is not the panel's (FAR's `ProcessLocalFileTypes`'
/// `Name` and `CurrentDirectory`: Find file's results run it in the found file's folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssocTarget {
    pub name: String,
    pub current: String,
    pub dir: Option<std::path::PathBuf>,
}

/// The command of `association` for `mode`.
pub fn command_of(association: &Association, mode: AssocMode) -> &AssociationCommand {
    match mode {
        AssocMode::Execute => &association.execute,
        AssocMode::AltExecute => &association.alt_execute,
        AssocMode::View => &association.view,
        AssocMode::AltView => &association.alt_view,
        AssocMode::Edit => &association.edit,
        AssocMode::AltEdit => &association.alt_edit,
    }
}

pub fn command_of_mut(association: &mut Association, mode: AssocMode) -> &mut AssociationCommand {
    match mode {
        AssocMode::Execute => &mut association.execute,
        AssocMode::AltExecute => &mut association.alt_execute,
        AssocMode::View => &mut association.view,
        AssocMode::AltView => &mut association.alt_view,
        AssocMode::Edit => &mut association.edit,
        AssocMode::AltEdit => &mut association.alt_edit,
    }
}

/// The associations for `name` and `mode`, in their order: the line each shows and its command;
/// `mask` reads their masks.
pub fn matching(
    associations: &[Association],
    name: &str,
    mode: AssocMode,
    mask: &dyn Fn(&str) -> Option<FileMask>,
) -> Vec<(String, String)> {
    (associations.iter())
        .filter(|association| mask(&association.masks).is_some_and(|mask| mask.matches(name)))
        .filter_map(|association| {
            let command = command_of(association, mode);
            (command.on && !command.command.trim().is_empty())
                .then(|| (association.description.clone(), command.command.clone()))
        })
        .collect()
}

impl App {
    /// The file under the cursor (on the quick view, the file it shows) opened with `mode` by its
    /// associations; false when it has none.
    pub(super) fn open_by_association(&mut self, mode: AssocMode) -> bool {
        let Some(name) = self.quick_view_file().or_else(|| {
            let entry = self
                .active_panel()
                .current()
                .filter(|entry| !entry.is_dir)?;
            // A plugin's entry of a real file is matched by that file's name, as FAR's (P2.4).
            let real = (entry.real_path.as_deref())
                .and_then(std::path::Path::file_name)
                .map(|name| name.to_string_lossy().into_owned());
            Some(real.unwrap_or_else(|| entry.name.clone()))
        }) else {
            return false;
        };
        let target = AssocTarget {
            name,
            current: self.menu_facts().active.current,
            dir: None,
        };
        self.open_target_by_association(target, mode)
    }

    /// `target` opened with `mode` by its associations; false when it has none.
    pub(super) fn open_target_by_association(
        &mut self,
        target: AssocTarget,
        mode: AssocMode,
    ) -> bool {
        let found = matching(&self.config.associations, &target.name, mode, &|text| {
            self.file_mask(text)
        });
        if found.is_empty() {
            return false;
        }
        let mut facts = self.menu_facts();
        facts.active.current.clone_from(&target.current);
        let commands: Vec<String> = found.iter().map(|(_, command)| command.clone()).collect();
        if let [command] = commands.as_slice() {
            self.run_association(command, mode, target);
            return true;
        }
        let items = (found.iter())
            .map(|(description, command)| {
                let shown = if description.is_empty() {
                    command
                } else {
                    description
                };
                without_marks(&expand_label(shown, &facts))
            })
            .collect();
        self.show_dialog(Dialog::List {
            title: "Select association".to_string(),
            items,
            cursor: 0,
            top: Default::default(),
            action: ListAction::Association(commands, mode, Box::new(target)),
        });
        true
    }

    /// The association's command for `target`; Enter's goes into the history as it runs, its symbols
    /// replaced and its questions answered. Its questions have no title, as FAR's.
    pub(super) fn run_association(&mut self, command: &str, mode: AssocMode, target: AssocTarget) {
        let line = super::user_menu_run::RunLine {
            text: command.to_string(),
            current: target.current,
            dir: target.dir,
            unselect: false,
            note: mode == AssocMode::Execute,
            title: String::new(),
            group: 0,
        };
        self.queue_run("", vec![line]);
    }
}

#[cfg(test)]
#[path = "associations_tests.rs"]
mod tests;
