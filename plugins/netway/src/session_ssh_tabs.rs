//! The session dialog's SSH, KEX and Authentication tabs (NetBox's `TSessionDialog`, S6.5i): "Enable
//! compression" and the ciphers' order (a list with Up / Down, NetBox's `CipherButtonClick`); the key
//! exchanges' order and the rekey limits; bypassing authentication, Pageant, keyboard-interactive and
//! answering its first prompt with the password (that one only while keyboard-interactive is on).

use nc_plugin_sdk::{Controls, DialogItem, Value};

use crate::placed::{at, heading, line};
use crate::ssh_options::{CIPHERS, KEXES, Ssh, label};

/// The first item of SSH, KEX and Authentication, and the one after them.
pub const FIRST: [usize; 4] = [108, 115, 125, 130];

const COMPRESSION: usize = 109;
pub const CIPHER_LIST: usize = 112;
pub const CIPHER_UP: usize = 113;
pub const CIPHER_DOWN: usize = 114;
pub const KEX_LIST: usize = 117;
pub const KEX_UP: usize = 118;
pub const KEX_DOWN: usize = 119;
const REKEY_TIME: usize = 122;
const REKEY_DATA: usize = 124;
const BYPASS: usize = 125;
const AGENT: usize = 127;
pub const KEYBOARD: usize = 128;
const KEYBOARD_PASSWORD: usize = 129;

/// Each tab's item the focus goes to.
pub const FOCUS: [usize; 3] = [COMPRESSION, KEX_LIST, BYPASS];

const LEFT: i32 = 5;
const WIDTH: i32 = 59;

fn labels(table: &[(&str, &str)], names: &[String]) -> Vec<String> {
    names.iter().map(|name| label(table, name)).collect()
}

fn list(
    table: &[(&str, &str)],
    names: &[String],
    (x1, y1, x2, y2): (i32, i32, i32, i32),
) -> DialogItem {
    let lines = labels(table, names);
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    DialogItem::list_box("", &lines, 0).at(x1, y1, x2, y2)
}

fn ssh_tab(ssh: &Ssh) -> Vec<DialogItem> {
    vec![
        heading("SSH protocol options", LEFT - 1, 3, WIDTH + 2),
        at(
            DialogItem::check("Enable &compression", ssh.compression),
            LEFT,
            4,
        ),
        heading("Encryption options", LEFT - 1, 5, WIDTH + 2),
        at(
            DialogItem::text("&Encryption cipher selection policy:"),
            LEFT,
            6,
        ),
        list(&CIPHERS, &ssh.ciphers, (LEFT, 7, 40, 16)),
        at(DialogItem::button("&Up").no_close(), 43, 7),
        at(DialogItem::button("&Down").no_close(), 43, 8),
    ]
}

fn kex_tab(ssh: &Ssh) -> Vec<DialogItem> {
    vec![
        heading("Key exchange algorithm options", LEFT - 1, 3, WIDTH + 2),
        at(DialogItem::text("Algorithm selection &policy:"), LEFT, 4),
        list(&KEXES, &ssh.kexes, (LEFT, 5, 46, 13)),
        at(DialogItem::button("&Up").no_close(), 49, 5),
        at(DialogItem::button("&Down").no_close(), 49, 6),
        heading(
            "Options controlling key re-exchange",
            LEFT - 1,
            14,
            WIDTH + 2,
        ),
        at(
            DialogItem::text("Max &minutes before rekey (0 for no limit):"),
            LEFT,
            15,
        ),
        line(
            DialogItem::fixed("####", &ssh.rekey_time.to_string()),
            50,
            15,
            4,
        ),
        at(
            DialogItem::text("Ma&x data before rekey (0 for no limit):"),
            LEFT,
            16,
        ),
        line(DialogItem::edit(&ssh.rekey_data), 50, 16, 8),
    ]
}

fn authentication_tab(ssh: &Ssh) -> Vec<DialogItem> {
    vec![
        at(
            DialogItem::check("&Bypass authentication entirely (SSH-2)", ssh.no_user_auth),
            LEFT,
            3,
        ),
        heading("Authentication options", LEFT - 1, 4, WIDTH + 2),
        at(
            DialogItem::check("Attempt authentication using &Pageant", ssh.try_agent),
            LEFT,
            5,
        ),
        at(
            DialogItem::check(
                "Attempt 'keyboard-&interactive' authentication (SSH-2)",
                ssh.keyboard_interactive,
            ),
            LEFT,
            6,
        ),
        at(
            DialogItem::check(
                "Respond with pass&word to the first prompt",
                ssh.ki_password,
            ),
            LEFT + 4,
            7,
        ),
    ]
}

/// The three tabs' items, hidden.
pub fn items(ssh: &Ssh) -> Vec<DialogItem> {
    let mut items = ssh_tab(ssh);
    items.extend(kex_tab(ssh));
    items.extend(authentication_tab(ssh));
    items.into_iter().map(|item| item.hidden(true)).collect()
}

/// The settings the dialog's `values` give, the lists' order as `order` holds it.
pub fn ssh_of(values: &[Value], base: &Ssh, order: &Order) -> Ssh {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let typed = value(REKEY_DATA).text.trim().to_string();
    Ssh {
        compression: value(COMPRESSION).checked(),
        ciphers: order.ciphers.clone(),
        kexes: order.kexes.clone(),
        rekey_time: value(REKEY_TIME)
            .text
            .trim()
            .parse()
            .unwrap_or(base.rekey_time),
        rekey_data: match crate::ssh_options::bytes_of(&typed) {
            Some(_) => typed,
            None => base.rekey_data.clone(),
        },
        no_user_auth: value(BYPASS).checked(),
        try_agent: value(AGENT).checked(),
        keyboard_interactive: value(KEYBOARD).checked(),
        ki_password: value(KEYBOARD_PASSWORD).checked(),
    }
}

/// The lists' order while the dialog is up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    pub ciphers: Vec<String>,
    pub kexes: Vec<String>,
}

impl Order {
    pub fn of(ssh: &Ssh) -> Self {
        Self {
            ciphers: ssh.ciphers.clone(),
            kexes: ssh.kexes.clone(),
        }
    }

    /// Up or Down pressed (`button`): the list's chosen line moved one place, the cursor with it. Whether
    /// the button was one of these.
    pub fn moved(&mut self, controls: &mut Controls, button: usize) -> bool {
        let (list, names, table, up) = match button {
            CIPHER_UP => (CIPHER_LIST, &mut self.ciphers, &CIPHERS[..], true),
            CIPHER_DOWN => (CIPHER_LIST, &mut self.ciphers, &CIPHERS[..], false),
            KEX_UP => (KEX_LIST, &mut self.kexes, &KEXES[..], true),
            KEX_DOWN => (KEX_LIST, &mut self.kexes, &KEXES[..], false),
            _ => return false,
        };
        let at = usize::try_from(controls.value(list).state).unwrap_or(0);
        if let Some(to) = step(at, names.len(), up) {
            names.swap(at, to);
            let lines = labels(table, names);
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            controls.set_lines(list, &lines);
            controls.set_state(list, i32::try_from(to).unwrap_or(0));
        }
        controls.focus(list);
        true
    }
}

/// The place a line at `at` of `count` moves to, none at the end it goes to.
pub fn step(at: usize, count: usize, up: bool) -> Option<usize> {
    match up {
        true => at.checked_sub(1),
        false => (at + 1 < count).then_some(at + 1),
    }
}

/// The password's answer only while keyboard-interactive is tried.
pub fn update(controls: &mut Controls) {
    let keyboard = controls.value(KEYBOARD).checked();
    controls.enable(KEYBOARD_PASSWORD, keyboard);
}

#[cfg(test)]
#[path = "session_ssh_tabs_tests.rs"]
mod tests;
