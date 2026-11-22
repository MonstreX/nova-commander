//! Conversion between the application state and the portable settings file.

use std::path::Path;

use super::command::Side;
use super::{App, Panel, ViewMode};
use crate::settings::Settings;
use crate::terminal::{self, Shell};
use crate::vfs::{SortMode, SortOrder, VfsPath};

impl App {
    /// Snapshot of what should survive a restart.
    pub fn to_settings(&self) -> Settings {
        let mut settings = self.saved_fields();
        self.prefs.write_to(&mut settings);
        self.filters.write_to(&mut settings);
        self.find.write_to(&mut settings);
        settings.code_pages.clone_from(&self.code_pages);
        settings.syntax_types.clone_from(&self.syntax_types);
        settings.viewer.clone_from(&self.viewer_settings);
        settings.editor.clone_from(&self.editor_settings);
        settings.highlight = self.panels[0].options.highlight;
        settings.other = self.plugins.kept_settings.clone();
        settings.select_folders = self.panels[0].options.select_folders;
        settings.allow_reverse_sort = self.panels[0].options.allow_reverse;
        settings.right_click_select = self.panels[0].options.right_click_select;
        settings.panel_switches.left_type = self.panel_type(Side::Left).to_string();
        settings.panel_switches.right_type = self.panel_type(Side::Right).to_string();
        let sections = self
            .info_panel
            .as_ref()
            .map_or(self.remembered.info_sections, |info| info.sections);
        settings.info_panel = crate::settings::InfoPanelSettings {
            disk: sections.disk,
            memory: sections.memory,
            power: sections.power,
            description: sections.description,
            plugin: sections.plugin,
            description_files: self.config.description_files.clone(),
            show_cd_info: self.config.info_options.show_cd_info,
            computer_name_format: self.config.info_options.names.computer.id().to_string(),
            user_name_format: self.config.info_options.names.user.id().to_string(),
        };
        settings
    }

    /// What a side shows, as FAR keeps it (`Panel.*.Type`): a plugin's panel is files.
    fn panel_type(&self, side: Side) -> &'static str {
        if self.tree_side() == Some(side) {
            "tree"
        } else if self.info_side() == Some(side) {
            "info"
        } else if self.quick_view_side() == Some(side) {
            "quickView"
        } else {
            "files"
        }
    }

    /// The sides as the settings left them (FAR's `CreatePanel` of `Panel.*.Type` at the start); called
    /// before their visibility is set, so a hidden one stays hidden.
    pub(super) fn restore_panel_types(&mut self, settings: &Settings) {
        for (side, kind) in [
            (Side::Left, settings.panel_switches.left_type.as_str()),
            (Side::Right, settings.panel_switches.right_type.as_str()),
        ] {
            match kind {
                "tree" => self.show_tree_panel_on(side),
                "info" => self.show_info_panel_on(side),
                "quickView" => self.show_quick_view_on(side),
                _ => {}
            }
        }
    }

    fn saved_fields(&self) -> Settings {
        Settings {
            shell: self.shell.id().to_string(),
            user_shells: user_shell_settings(&self.user_shells),
            associations: self.config.associations.clone(),
            folder_shortcuts: self.shortcuts.lists.clone(),
            mask_groups: self.config.mask_groups.clone(),
            highlighting: Some(self.highlighting.kept.clone()),
            panel_modes: Some(self.panel_modes.kept().clone()),
            height_decrement: self.height_decrement,
            width_decrement: self.width_decrement,
            show_key_bar: self.show_key_bar,
            theme: self.themes.chosen.clone(),
            console_palette: self.console_palette,
            mouse: self.mouse.enabled,
            left_visible: self.panels[0].visible,
            right_visible: self.panels[1].visible,
            show_hidden: self.show_hidden,
            auto_update_limit: self.config.auto_update_limit,
            auto_update_remote_drives: self.config.auto_update_remote_drives,
            keep_finished_tasks: self.tasks.keep_done.as_secs(),
            delete_to_recycle_bin: self.config.delete_settings.to_recycle_bin,
            confirm_delete: self.config.delete_settings.confirm,
            confirm_delete_folder: self.config.delete_settings.confirm_folders,
            confirm_read_only: self.config.delete_settings.confirm_read_only,
            delete_show_total: self.config.delete_settings.show_total,
            delete_show_selected: self.config.delete_settings.show_selected,
            drive_menu: super::drive_menu::column_ids(&self.config.drive_columns),
            active_panel: match self.active {
                Side::Left => "left",
                Side::Right => "right",
            }
            .to_string(),
            left_path: saved_path(&self.panels[0]),
            left_sort: saved_sort(&self.panels[0]).mode.id().to_string(),
            left_sort_reverse: Some(saved_sort(&self.panels[0]).reverse),
            left_directories_first: saved_sort(&self.panels[0]).directories_first,
            left_selected_first: saved_sort(&self.panels[0]).selected_first,
            left_sort_groups: saved_sort(&self.panels[0]).sort_groups,
            left_view: saved_view(&self.panels[0]),
            right_path: saved_path(&self.panels[1]),
            right_sort: saved_sort(&self.panels[1]).mode.id().to_string(),
            right_sort_reverse: Some(saved_sort(&self.panels[1]).reverse),
            right_directories_first: saved_sort(&self.panels[1]).directories_first,
            right_selected_first: saved_sort(&self.panels[1]).selected_first,
            right_sort_groups: saved_sort(&self.panels[1]).sort_groups,
            right_view: saved_view(&self.panels[1]),
            ..Settings::default()
        }
    }
}

/// The highlighting groups as saved (none: FAR's), to be made with the groups of file masks.
pub fn highlighting(settings: &Settings) -> super::Highlighting {
    let kept = (settings.highlighting.clone())
        .unwrap_or_else(crate::settings::HighlightGroups::far_defaults);
    super::Highlighting::make(&kept, &|_| None)
}

/// The file panel modes as saved (none: FAR's).
pub fn panel_modes(settings: &Settings) -> super::PanelModes {
    super::PanelModes::make(settings.panel_modes.clone().unwrap_or_default())
}

/// The info panel's sections as saved.
pub fn info_sections(settings: &Settings) -> super::InfoSections {
    super::InfoSections {
        disk: settings.info_panel.disk,
        memory: settings.info_panel.memory,
        description: settings.info_panel.description,
        plugin: settings.info_panel.plugin,
        power: settings.info_panel.power,
    }
}

/// The info panel's options as saved.
pub fn info_options(settings: &Settings) -> crate::vfs::local::InfoOptions {
    use crate::platform::system::{ComputerNameFormat, NameFormats, UserNameFormat};
    let info = &settings.info_panel;
    crate::vfs::local::InfoOptions {
        names: NameFormats {
            computer: ComputerNameFormat::from_id(&info.computer_name_format),
            user: UserNameFormat::from_id(&info.user_name_format),
        },
        show_cd_info: info.show_cd_info,
    }
}

/// The left and right panels' saved sort orders.
pub fn panel_sorts(settings: &Settings) -> [SortOrder; 2] {
    [
        sort_order(
            &settings.left_sort,
            settings.left_sort_reverse,
            settings.left_directories_first,
            settings.left_selected_first,
            settings.left_sort_groups,
        ),
        sort_order(
            &settings.right_sort,
            settings.right_sort_reverse,
            settings.right_directories_first,
            settings.right_selected_first,
            settings.right_sort_groups,
        ),
    ]
}

/// Both panels as the settings left them.
pub fn restored_panels(settings: &Settings, start_dir: &Path) -> [Panel; 2] {
    let [left_sort, right_sort] = panel_sorts(settings);
    let saved = [settings.left_path.as_str(), settings.right_path.as_str()];
    let [left, right] = panel_paths(saved, start_dir, START_LOOK, Path::is_dir);
    [
        restored_panel(settings, left, left_sort, &settings.left_view),
        restored_panel(settings, right, right_sort, &settings.right_view),
    ]
}

/// A panel as the settings left it.
pub fn restored_panel(
    settings: &Settings,
    path: VfsPath,
    sort: crate::vfs::SortOrder,
    view: &str,
) -> Panel {
    let mut panel = Panel::with_settings(
        path,
        settings.show_hidden,
        settings.auto_update_remote_drives,
    );
    panel.options = super::panel_options::PanelOptions {
        highlight: settings.highlight,
        select_folders: settings.select_folders,
        allow_reverse: settings.allow_reverse_sort,
        right_click_select: settings.right_click_select,
    };
    panel.set_sort_order(sort);
    // A user's mode that is not there any more is Medium.
    let custom = settings
        .panel_modes
        .as_ref()
        .map_or(0, |modes| modes.custom.len());
    let mode = match ViewMode::from_id(view) {
        ViewMode::Custom(at) if at >= custom => ViewMode::Medium,
        mode => mode,
    };
    panel.set_view_mode(mode);
    panel
}

/// The folder a panel is saved at: a provider's (a plugin's panel, gone when the program ends) as the
/// folder the panel entered it from.
fn saved_path(panel: &super::Panel) -> String {
    match panel.before_plugins() {
        Some(entered) => entered.from.to_string(),
        None => panel.path.to_string(),
    }
}

/// The view mode a panel is saved with: on a provider's panel the one it had before (as it comes back
/// when the panel leaves it).
fn saved_view(panel: &super::Panel) -> String {
    panel.return_view().unwrap_or(panel.view_mode).id()
}

/// The sort a panel is saved with: on a plugin's panel the one it had before the first. FAR saves none of
/// a plugin's panel's (`config.cpp` keeps the values last stored); ours, the disk's the panel comes back to.
fn saved_sort(panel: &super::Panel) -> crate::vfs::SortOrder {
    panel
        .before_plugins()
        .map_or(panel.sort, |entered| entered.sort)
}

/// How long the start waits for the saved folders' disks (ADR-0019, review R20): one that does not
/// answer (a share gone, ~21 s) starts its panel in the start directory instead of holding the first
/// frame.
pub(in crate::app) const START_LOOK: std::time::Duration = std::time::Duration::from_secs(2);

/// Each `saved` folder if it still exists, otherwise the start directory, their disks asked at once by
/// `is_dir`, all within `limit` (no answer: the start directory; a look left behind ends with its disk's
/// answer). A provider's folder is not reopened on start yet.
pub(in crate::app) fn panel_paths<const N: usize>(
    saved: [&str; N],
    start_dir: &Path,
    limit: std::time::Duration,
    is_dir: fn(&Path) -> bool,
) -> [VfsPath; N] {
    let deadline = std::time::Instant::now() + limit;
    let looks = saved.map(|saved| {
        let folder = VfsPath::parse(saved).as_local()?.to_path_buf();
        if folder.as_os_str().is_empty() {
            return None;
        }
        let (answer, answered) = std::sync::mpsc::channel();
        let asked = folder.clone();
        // Without a thread the folder is not looked at: the start folder (review R24).
        std::thread::Builder::new()
            .name("start folder".to_string())
            .spawn(move || answer.send(is_dir(&asked)))
            .ok()?;
        Some((folder, answered))
    });
    looks.map(|look| match look {
        Some((folder, answered))
            if answered
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap_or(false) =>
        {
            VfsPath::local(folder)
        }
        _ => VfsPath::local(start_dir),
    })
}

/// The saved shell if it is known and installed, otherwise the system's default.
pub fn shell_from_id(id: &str, user: &[Shell]) -> Shell {
    terminal::available_shells(user)
        .into_iter()
        .find(|shell| shell.id() == id)
        .unwrap_or_else(Shell::default_shell)
}

/// The user's own shells (Options > Command line settings > Shells).
pub fn user_shells(settings: &Settings) -> Vec<Shell> {
    (settings.user_shells.iter())
        .map(|shell| {
            let dialect = terminal::Dialect::from_id(&shell.kind);
            Shell::user(&shell.name, &shell.program, &shell.arguments, dialect)
        })
        .collect()
}

fn user_shell_settings(shells: &[Shell]) -> Vec<crate::settings::UserShell> {
    (shells.iter())
        .map(|shell| crate::settings::UserShell {
            name: shell.title().to_string(),
            program: shell.program().display().to_string(),
            arguments: shell.arguments().to_string(),
            kind: shell.dialect().id().to_string(),
        })
        .collect()
}

pub fn side_from_id(id: &str) -> Side {
    if id == "right" {
        Side::Right
    } else {
        Side::Left
    }
}

/// A panel's saved sort order.
pub fn sort_order(
    mode: &str,
    reverse: Option<bool>,
    directories_first: bool,
    selected_first: bool,
    sort_groups: bool,
) -> SortOrder {
    let mode = SortMode::from_id(mode);
    SortOrder {
        mode,
        reverse: reverse.unwrap_or_else(|| mode.descending_by_default()),
        directories_first,
        selected_first,
        sort_groups,
        names_only: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_console_palette_choice_survives_a_restart() {
        let temp = tempfile::tempdir().unwrap();
        let settings = Settings {
            console_palette: false,
            ..Settings::default()
        };
        let app = App::new(temp.path(), 80, 25, &settings);
        assert!(!app.to_settings().console_palette);
        assert!(App::new(temp.path(), 80, 25, &Settings::default()).console_palette);
    }

    #[test]
    fn what_each_side_shows_survives_a_restart() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::new(temp.path(), 80, 25, &Settings::default());
        assert_eq!(
            (
                app.to_settings().panel_switches.left_type.as_str(),
                app.to_settings().panel_switches.right_type.as_str()
            ),
            ("files", "files")
        );
        app.show_tree_panel_on(Side::Left);
        app.show_quick_view_on(Side::Right);
        let saved = app.to_settings();
        assert_eq!(
            (
                saved.panel_switches.left_type.as_str(),
                saved.panel_switches.right_type.as_str()
            ),
            ("tree", "quickView")
        );
        let back = App::new(temp.path(), 80, 25, &saved);
        assert_eq!(
            (back.tree_side(), back.quick_view_side()),
            (Some(Side::Left), Some(Side::Right))
        );
        let info = Settings {
            panel_switches: crate::settings::PanelSwitches {
                right_type: "info".to_string(),
                ..Default::default()
            },
            right_visible: false,
            ..Settings::default()
        };
        let back = App::new(temp.path(), 80, 25, &info);
        assert_eq!(back.info_side(), Some(Side::Right));
        assert!(!back.panels[1].visible, "a hidden side stays hidden");
    }

    #[test]
    fn screen_layout_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let saved = Settings {
            width_decrement: -3,
            show_key_bar: false,
            left_visible: false,
            ..Settings::default()
        };
        let restored = App::new(temp.path(), 100, 40, &saved).to_settings();
        assert_eq!(restored.width_decrement, -3);
        assert!(!restored.show_key_bar);
        assert!(!restored.left_visible);
        assert!(restored.right_visible);
    }

    #[test]
    fn settings_round_trip_through_the_app() {
        let temp = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let saved = Settings {
            height_decrement: 4,
            active_panel: "right".to_string(),
            right_path: other.path().display().to_string(),
            right_sort: "size".to_string(),
            right_sort_reverse: Some(true),
            right_selected_first: true,
            right_sort_groups: true,
            left_view: "brief".to_string(),
            right_view: "detailed".to_string(),
            ..Settings::default()
        };

        let app = App::new(temp.path(), 100, 40, &saved);
        let restored = app.to_settings();

        assert_eq!(restored.height_decrement, 4);
        assert_eq!(restored.left_sort, "name");
        assert_eq!(restored.active_panel, "right");
        assert_eq!(restored.left_path, temp.path().display().to_string());
        assert_eq!(restored.right_path, saved.right_path);
        assert_eq!(restored.right_sort, "size");
        assert_eq!(restored.right_sort_reverse, Some(true));
        assert!(restored.right_selected_first);
        assert!(restored.right_sort_groups && !restored.left_sort_groups);
        assert!(restored.left_directories_first);
        assert_eq!(restored.left_view, "brief");
        assert_eq!(restored.right_view, "detailed");
        assert_eq!(restored.shell, "cmd");
    }

    /// The start's look at one saved folder.
    fn panel_path(saved: &str, start_dir: &Path) -> VfsPath {
        let [path] = panel_paths([saved], start_dir, START_LOOK, Path::is_dir);
        path
    }

    #[test]
    fn vanished_directory_falls_back_to_start_dir() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("gone");
        assert_eq!(
            panel_path(&missing.display().to_string(), temp.path()),
            temp.path()
        );
        assert_eq!(panel_path("", temp.path()), temp.path());
    }

    #[test]
    fn saved_folders_whose_disks_do_not_answer_are_not_waited_for() {
        // Review R20: a disk that never answers; both panels' within one limit, the other's kept.
        fn never(_: &Path) -> bool {
            loop {
                std::thread::park();
            }
        }
        fn answers(folder: &Path) -> bool {
            !folder.ends_with("dead")
        }
        let temp = tempfile::tempdir().unwrap();
        let limit = std::time::Duration::from_millis(50);
        let dead = temp.path().join("dead").display().to_string();
        let alive = temp.path().join("alive").display().to_string();
        let start = VfsPath::local(temp.path());
        let both = panel_paths([&dead, &dead], temp.path(), limit, never);
        assert_eq!(both, [start.clone(), start.clone()]);
        let [left, right] = panel_paths([&dead, &alive], temp.path(), START_LOOK, answers);
        assert_eq!(left, start, "no folder: the start directory");
        assert_eq!(right, VfsPath::local(temp.path().join("alive")));
    }

    #[test]
    fn unknown_ids_fall_back_to_defaults() {
        assert_eq!(shell_from_id("bash-on-mars", &[]), Shell::default_shell());
        assert_eq!(
            sort_order("random", Some(false), true, false, false).mode,
            SortMode::Name
        );
        // Files saved before the direction existed: size keeps its default (largest first).
        assert!(sort_order("size", None, true, false, false).reverse);
        assert_eq!(side_from_id(""), Side::Left);
    }
}
