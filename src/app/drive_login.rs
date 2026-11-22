//! Ins on a remembered network drive whose saved name or password the server refused: FAR's own
//! dialog (`GetNameAndPassword`, `stddlg.cpp`) asks for them, its title the share, the user name
//! line with its history (`NetworkUser`, the last one given in it), the password shown as `*`; OK
//! connects with them and asks again while they are refused. The password is not kept for the next
//! time (FAR keeps the last one in memory).

use super::{DriveAction, MenuPlace};
use crate::app::App;
use crate::app::dialog::{Dialog, FormAction};
use crate::app::form::{Control, EditHistory, Form, Item};
use crate::app::line_input::LineInput;

/// A name and password typed for a network drive; a debug print never shows the password.
#[derive(Clone, PartialEq, Eq)]
pub struct Login {
    pub user: String,
    pub password: String,
}

impl std::fmt::Debug for Login {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Login")
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

const USER: usize = 1;
const PASSWORD: usize = 3;
const OK: usize = 5;

/// FAR's dialog of 76 by 10: the two lines, a separator, OK and Cancel.
fn login_form(share: &str) -> Form {
    let edit = |y: u16| {
        let mut item = Item::new(5, y, Control::Edit(LineInput::default()));
        item.width = 66;
        item
    };
    let button = |label: &str, default| {
        Item::new(
            0,
            7,
            Control::Button {
                label: label.to_string(),
                default,
            },
        )
    };
    let items = vec![
        Item::new(5, 2, Control::Label("User name".to_string())),
        edit(3).with_history(EditHistory::new("NetworkUser").use_last()),
        Item::new(5, 4, Control::Label("User password".to_string())),
        edit(5).password(),
        Item::new(0, 6, Control::Separator),
        button("OK", true),
        button("Cancel", false),
    ];
    Form::new(share, 76, 10, items)
}

impl App {
    /// The server refused the saved name or password of `root`'s connection to `share`.
    pub(in crate::app) fn ask_network_login(
        &mut self,
        root: String,
        share: String,
        place: MenuPlace,
    ) {
        let form = login_form(&share);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::NetworkLogin { root, share, place },
        });
    }

    /// OK connects with what was typed; Cancel brings the menu back.
    pub(in crate::app) fn finish_network_login(
        &mut self,
        form: &Form,
        button: Option<usize>,
        (root, share, place): (String, String, MenuPlace),
    ) {
        if button != Some(OK) {
            return self.back_to_drive_menu(place);
        }
        let login = Login {
            user: form.edit_text(USER).to_string(),
            password: form.edit_text(PASSWORD).to_string(),
        };
        let action = DriveAction::Connect {
            root,
            share,
            login: Some(login),
        };
        self.run_drive_action(action, place, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_is_fars_and_a_debug_print_hides_the_password() {
        let form = login_form(r"\\server\share");
        assert_eq!(form.title, r"\\server\share");
        assert!(form.items[PASSWORD].password && !form.items[USER].password);
        assert_eq!(
            form.history(USER).map(|history| history.name.as_str()),
            Some("NetworkUser")
        );
        let login = Login {
            user: "me".to_string(),
            password: "secret".to_string(),
        };
        let shown = format!("{login:?}");
        assert!(shown.contains("me") && !shown.contains("secret"), "{shown}");
    }

    #[cfg(windows)]
    #[test]
    fn a_wrong_name_or_password_is_told_from_other_failures() {
        use crate::platform::volumes::credentials_refused;
        use std::io::Error;
        // Access denied, a wrong password, a logon failure (FAR's loop asks again on these).
        for code in [5, 86, 1326] {
            assert!(
                credentials_refused(&Error::from_raw_os_error(code)),
                "{code}"
            );
        }
        assert!(
            !credentials_refused(&Error::from_raw_os_error(67)),
            "no such share"
        );
        assert!(!credentials_refused(&Error::other("x")));
    }
}
