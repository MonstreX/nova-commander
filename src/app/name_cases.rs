//! FAR's `PanelCtrlFRule` (`Panel::CreateFullPathName`): with it on, a full name put into the command line
//! or onto the clipboard (Ctrl+F, Ctrl+Alt+F, Ctrl+Alt+[ / ], Alt+Shift+Ins, Ctrl+Alt+Ins) takes the
//! cases the panel's mode shows its names in. Off by default, as FAR's; the setting has no dialog
//! (`panels.ctrlFRule`, FAR's `Panel.CtrlFRule` in far:config).

use super::App;

/// The cases a mode shows names in (FAR's `PVS_FOLDERUPPERCASE`, `PVS_FILELOWERCASE`,
/// `PVS_FILEUPPERTOLOWERCASE`); all off is a name as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct NameCases {
    pub folder_upper: bool,
    pub files_lower: bool,
    pub upper_to_lower: bool,
}

impl NameCases {
    /// The active panel's mode's cases when the setting asks for them, none otherwise.
    pub(super) fn of_active(app: &App) -> Self {
        Self::of(app, app.active)
    }

    /// The mode's cases of the panel on `side` (FAR's `SrcPanel->CreateFullPathName`).
    pub(super) fn of(app: &App, side: super::Side) -> Self {
        if !app.prefs.ctrl_f_rule {
            return Self::default();
        }
        let panel = &app.panels[side.index()];
        let mode = app.panel_modes.spec_in(panel.view_mode, &panel.look);
        Self {
            folder_upper: mode.folder_upper_case,
            files_lower: mode.files_lower_case,
            upper_to_lower: mode.upper_to_lower_case,
        }
    }

    /// `full` as FAR's `CreateFullPathName` gives it: a folder's whole name in capitals, a file's path
    /// in capitals; a file's name in small letters when the mode lowers files, or when it lowers names
    /// in capitals and the name is not of mixed case.
    pub(super) fn apply(self, full: &str, directory: bool) -> String {
        if self == Self::default() {
            return full.to_string();
        }
        // FAR's `FindLastSlash`: the name starts at the last slash (kept with it).
        let slash = full.rfind(['\\', '/']);
        let (path, name) = match slash {
            Some(at) if !directory => full.split_at(at),
            _ => (full, ""),
        };
        let path = if self.folder_upper {
            path.to_uppercase()
        } else {
            path.to_string()
        };
        let lowered =
            slash.is_some() && (self.files_lower || (self.upper_to_lower && !case_mixed(name)));
        if lowered {
            path + &name.to_lowercase()
        } else {
            path + name
        }
    }
}

/// FAR's `IsCaseMixed`: a letter of the other case than the first letter.
fn case_mixed(text: &str) -> bool {
    let mut letters = text.chars().filter(|ch| ch.is_alphabetic());
    let Some(first) = letters.next() else {
        return false;
    };
    letters.any(|ch| ch.is_lowercase() != first.is_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: NameCases = NameCases {
        folder_upper: true,
        files_lower: false,
        upper_to_lower: true,
    };

    #[test]
    fn a_folder_is_all_capitals_and_a_files_path_only() {
        assert_eq!(ALL.apply(r"C:\Dir\Sub", true), r"C:\DIR\SUB");
        assert_eq!(ALL.apply(r"C:\Dir\README", false), r"C:\DIR\readme");
        assert_eq!(
            ALL.apply(r"C:\Dir\ReadMe.txt", false),
            r"C:\DIR\ReadMe.txt",
            "a name of mixed case is kept"
        );
    }

    #[test]
    fn files_lowered_whatever_their_case_and_none_as_it_is() {
        let lower = NameCases {
            files_lower: true,
            ..NameCases::default()
        };
        assert_eq!(
            lower.apply(r"C:\Dir\ReadMe.TXT", false),
            r"C:\Dir\readme.txt"
        );
        assert_eq!(
            lower.apply(r"C:\Dir\Sub", true),
            r"C:\Dir\Sub",
            "not a folder"
        );
        assert_eq!(
            NameCases::default().apply(r"C:\Dir\README", false),
            r"C:\Dir\README"
        );
    }

    #[test]
    fn case_mixed_goes_by_the_first_letter() {
        assert!(!case_mixed(r"\README.TXT1"));
        assert!(case_mixed(r"\Readme"));
        assert!(!case_mixed(r"\123"));
    }
}
