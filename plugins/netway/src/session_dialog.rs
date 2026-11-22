//! NetBox's session dialog (`WinSCPDialogs.cpp` `TSessionDialog`, 69 x 25): the tab bar on top, the
//! Session tab (S6.4b) - the file protocol with SFTP's "Allow SCP fallback" or "Insecure", the encryption
//! of FTP, WebDAV and S3, the host and its port, the user and the password (S3's access keys), the private
//! key file - and the tabs after it (S6.5g, `session_tabs`); OK, Connect and Cancel. Choosing another
//! protocol or encryption moves a port that was another protocol's default to its own
//! (`TransferProtocolComboChange`) and changes the tabs there. Connect connects (S6.5a; an edited
//! session saved first); the "..." browse, "Display Public Key" and PuTTYgen's "Generate Key" are put off
//! (ADR-0023 §6).

use nc_plugin_sdk::{Controls, Dialog, DialogEvent, DialogItem, Host, Message, Value};

use crate::placed::{at, heading, line};
use crate::session::{Encryption, Protocol, S3_HOST, Session};
use crate::session_link_tabs;
use crate::session_protocol_tabs;
use crate::session_ssh_tabs::{self, Order};
use crate::session_tabs::{self, FIRST, NAMES};
use crate::tab_bar::{self, Row};

/// What the dialog is for (NetBox's `saAdd`, `saEdit`, `saConnect`), its title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Add,
    Edit,
}

impl Action {
    fn title(self) -> &'static str {
        match self {
            Self::Add => "Add session",
            Self::Edit => "Edit session",
        }
    }
}

// The items, in their order.
const TAB_BAR: usize = 0;
const PROTOCOL: usize = 3;
const FALLBACK: usize = 4;
const INSECURE: usize = 5;
const ENCRYPTION_LABEL: usize = 6;
const ENCRYPTION: usize = 7;
const HOST: usize = 10;
const PORT: usize = 12;
const USER_LABEL: usize = 13;
const USER: usize = 14;
const PASSWORD_LABEL: usize = 15;
const PASSWORD: usize = 16;
const KEY_LABEL: usize = 18;
const KEY: usize = 19;
const OK: usize = session_tabs::BUTTONS + 1;
const CONNECT: usize = session_tabs::BUTTONS + 2;

/// Each tab's item the focus goes to when it is shown (NetBox's first one that takes it).
const FOCUS: [usize; 15] = [
    PROTOCOL,
    24,
    42,
    49,
    52,
    session_link_tabs::FOCUS[0],
    session_link_tabs::FOCUS[1],
    session_link_tabs::FOCUS[2],
    session_ssh_tabs::FOCUS[0],
    session_ssh_tabs::FOCUS[1],
    session_ssh_tabs::FOCUS[2],
    session_protocol_tabs::FOCUS[0],
    session_protocol_tabs::FOCUS[1],
    session_protocol_tabs::FOCUS[2],
    session_protocol_tabs::FOCUS[3],
];

/// The encryptions S3 offers: only the first two (NetBox's `UpdateControls`).
const S3_ENCRYPTIONS: usize = 2;

const LEFT: i32 = 5;
const WIDTH: i32 = 59;
/// The tab bar's columns.
const BAR: usize = 59;

/// Which of the tab's lines show, and how, for a protocol and an encryption (NetBox's
/// `UpdateControls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub fallback: bool,
    pub insecure: bool,
    pub encryption: bool,
    pub key: bool,
    /// The login's labels are S3's access keys.
    pub access_keys: bool,
}

pub fn shown(protocol: Protocol, encryption: Encryption) -> Shown {
    let encrypted = encryption != Encryption::None;
    Shown {
        fallback: protocol == Protocol::Sftp,
        insecure: matches!(protocol, Protocol::Ftp | Protocol::WebDav) && !encrypted,
        encryption: !protocol.is_ssh(),
        key: protocol.is_ssh()
            || (matches!(protocol, Protocol::Ftp | Protocol::WebDav) && encrypted),
        access_keys: protocol == Protocol::S3,
    }
}

fn user_label(access_keys: bool) -> &'static str {
    if access_keys {
        "&Access key ID:"
    } else {
        "&User name:"
    }
}

fn password_label(access_keys: bool) -> &'static str {
    if access_keys {
        "&Secret access key:"
    } else {
        "&Password:"
    }
}

/// The encryptions' labels a protocol offers.
fn encryption_lines(protocol: Protocol) -> Vec<&'static str> {
    let count = if protocol == Protocol::S3 {
        S3_ENCRYPTIONS
    } else {
        Encryption::ALL.len()
    };
    Encryption::ALL[..count]
        .iter()
        .map(|one| one.label())
        .collect()
}

fn index_of<T: PartialEq>(all: &[T], one: &T) -> usize {
    all.iter().position(|each| each == one).unwrap_or(0)
}

/// The session's protocol and whether it is encrypted: what decides its tabs.
type Kind = (Protocol, bool);

/// The tab bar for `kind` with `tab` shown.
fn bar((protocol, encrypted): Kind, tab: usize) -> Row {
    tab_bar::row(&NAMES, &session_tabs::there(protocol, encrypted), tab, BAR)
}

/// The Session tab's items.
fn session_items(session: &Session) -> Vec<DialogItem> {
    let show = shown(session.protocol, session.encryption);
    let protocols: Vec<&str> = Protocol::ALL.iter().map(|one| one.label()).collect();
    let encryptions = encryption_lines(session.protocol);
    let encryption = index_of(&Encryption::ALL, &session.encryption);
    let keys = show.access_keys;
    vec![
        heading("Session", LEFT - 1, 3, WIDTH + 2),
        at(DialogItem::text("&File protocol:"), LEFT, 4),
        line(
            DialogItem::list(&protocols, index_of(&Protocol::ALL, &session.protocol)),
            21,
            4,
            10,
        ),
        at(
            DialogItem::check("Allow &SCP fallback", session.scp_fallback),
            33,
            4,
        )
        .hidden(!show.fallback),
        at(DialogItem::text("Insecure"), 33, 4).hidden(!show.insecure),
        at(DialogItem::text("&Encryption:"), LEFT, 5).hidden(!show.encryption),
        line(
            DialogItem::list(&encryptions, encryption.min(encryptions.len() - 1)),
            21,
            5,
            30,
        )
        .hidden(!show.encryption),
        heading("", LEFT - 1, 6, WIDTH + 2),
        at(DialogItem::text("&Host name:"), LEFT, 7),
        line(DialogItem::edit(&session.host).focus(), LEFT, 8, 45),
        at(DialogItem::text("Po&rt number:"), 52, 7),
        line(
            DialogItem::fixed("999999", &session.port.to_string()),
            52,
            8,
            6,
        ),
        at(DialogItem::text(user_label(keys)), LEFT, 9),
        line(DialogItem::edit(&session.user), LEFT, 10, 28),
        at(DialogItem::text(password_label(keys)), 35, 9),
        line(DialogItem::password(&session.password), 35, 10, 29),
        heading("", LEFT - 1, 11, WIDTH + 2),
        at(DialogItem::text("Private &key file:"), LEFT, 12).disabled(!show.key),
        line(DialogItem::edit(&session.private_key), LEFT, 13, WIDTH).disabled(!show.key),
        at(
            DialogItem::text("Click the tabs on top or use Ctrl+PgUp and Ctrl+PgDn keys to"),
            LEFT,
            18,
        ),
        at(
            DialogItem::text("display additional session options."),
            LEFT,
            19,
        ),
    ]
}

/// The dialog's items for `session`.
pub fn items(session: &Session) -> Vec<DialogItem> {
    let encrypted = session.encryption != Encryption::None;
    let row = bar((session.protocol, encrypted), 0);
    let mut items = vec![line(DialogItem::text(&row.text), LEFT, 2, WIDTH)];
    items.extend(session_items(session));
    items.extend(session_tabs::items(&session.options()));
    items.extend([
        DialogItem::separator().at(-1, 21, -1, 21),
        at(DialogItem::button("OK").default_button(), 19, 22),
        at(DialogItem::button("Co&nnect"), 26, 22),
        at(DialogItem::button("Cancel"), 38, 22),
    ]);
    items
}

/// The session the dialog's `values` give, over `base` (what the tabs do not show stays).
pub fn session_of(values: &[Value], base: &Session) -> Session {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let chosen = |item: usize| usize::try_from(value(item).state).unwrap_or(0);
    let protocol = Protocol::ALL
        .get(chosen(PROTOCOL))
        .copied()
        .unwrap_or_default();
    let encryption = Encryption::ALL
        .get(chosen(ENCRYPTION))
        .copied()
        .unwrap_or_default();
    let show = shown(protocol, encryption);
    let mut session = Session {
        protocol,
        scp_fallback: value(FALLBACK).checked(),
        encryption: if show.encryption {
            encryption
        } else {
            Encryption::None
        },
        host: value(HOST).text.trim().to_string(),
        port: value(PORT).text.trim().parse().unwrap_or(base.port),
        user: value(USER).text,
        password: value(PASSWORD).text,
        private_key: value(KEY).text.trim().to_string(),
        other: base.other.clone(),
    };
    session.set_options(&session_tabs::options_of(values, &base.options()));
    session
}

/// NetBox's question before a password typed is stored (`CloseQuery`'s `NB_SAVE_PASSWORD`), in the
/// program's words for DPAPI (WinSCP's "can easily be recovered" is not so here).
pub const SAVE_PASSWORD: &str = "You are about to store session with filled password.\n \nThe password is kept protected by Windows for your user account: only you, signed in on this computer, can use it, but any program you run can too.\n \nIf you do not save password along with other session options, you will be prompted for it once you attempt to open the stored session.\n \nSave the password anyway?";

/// The dialog closed by OK or Connect: the session as the user left it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub session: Session,
    /// Connect pressed (NetBox's `brConnect`): an edited session saved and connected to, a new one
    /// connected to without being saved.
    pub connect: bool,
}

/// Whether closing by `button` asks NetBox's question before a password typed is stored: OK, and
/// Connect of an edited session (a new one is not saved by it).
pub fn asks_password(button: usize, action: Action) -> bool {
    button == OK || (button == CONNECT && action == Action::Edit)
}

/// The tab a key or a click asks for, from `tab` shown: Ctrl+PgDn / Ctrl+PgUp, a click on the bar.
pub fn tab_asked(event: &DialogEvent, kind: Kind, tab: usize) -> Option<usize> {
    let there = session_tabs::there(kind.0, kind.1);
    match event {
        DialogEvent::Key { key, .. } if key == "Ctrl+PgDn" => {
            Some(tab_bar::step(&there, tab, true))
        }
        DialogEvent::Key { key, .. } if key == "Ctrl+PgUp" => {
            Some(tab_bar::step(&there, tab, false))
        }
        DialogEvent::Mouse { key, x, y: 2, .. } if key == "MsLClick" => {
            let column = usize::try_from(*x - LEFT).ok()?;
            tab_bar::tab_at(&bar(kind, tab), column)
        }
        _ => None,
    }
}

/// The session dialog for `session`: as the user left it on OK or Connect, none on Cancel or Esc.
pub fn edit(host: &Host, session: &Session, action: Action) -> Option<Edited> {
    let dialog = Dialog::new(action.title(), items(session))
        .size(69, 25)
        .hear_keys();
    let mut tab = 0;
    let mut order = Order::of(&session.options().ssh);
    let closed = host.dialog_with(&dialog, |event, controls| {
        let kind = kind_of(controls);
        if let Some(asked) = tab_asked(&event, kind, tab) {
            select(controls, kind, tab, asked);
            tab = asked;
            return false;
        }
        match event {
            DialogEvent::Init => session_tabs::update(controls, kind, tab),
            DialogEvent::Button(button) if order.moved(controls, button) => {}
            DialogEvent::Changed(item)
                if session_link_tabs::FOLLOWED.contains(&item)
                    || item == session_ssh_tabs::KEYBOARD =>
            {
                session_tabs::update(controls, kind, tab);
                passive_told(host, controls, kind.0);
            }
            DialogEvent::Changed(PROTOCOL | ENCRYPTION) => {
                follow_protocol(controls);
                passive_told(host, controls, protocol_of(controls));
            }
            DialogEvent::Changed(
                session_tabs::DELETE_TO_BIN | session_tabs::OVERWRITTEN_TO_BIN,
            ) => {
                session_tabs::update(controls, kind, tab);
            }
            DialogEvent::Button(session_tabs::UNIX) => session_tabs::preset(controls, false),
            DialogEvent::Button(session_tabs::WINDOWS) => session_tabs::preset(controls, true),
            DialogEvent::Close(Some(button)) if asks_password(button, action) => {
                return password_agreed(host, controls, session);
            }
            _ => {}
        }
        true
    });
    let connect = closed.button == Some(CONNECT);
    (closed.button == Some(OK) || connect).then(|| {
        let mut edited = session_of(&closed.values, session);
        let mut options = edited.options();
        options.ssh.ciphers = order.ciphers.clone();
        options.ssh.kexes = order.kexes.clone();
        edited.set_options(&options);
        Edited {
            session: edited,
            connect,
        }
    })
}

fn chosen(controls: &Controls, item: usize) -> usize {
    usize::try_from(controls.value(item).state).unwrap_or(0)
}

fn protocol_of(controls: &Controls) -> Protocol {
    Protocol::ALL
        .get(chosen(controls, PROTOCOL))
        .copied()
        .unwrap_or_default()
}

/// The protocol chosen and whether the encryption chosen (when it shows) is one.
fn kind_of(controls: &Controls) -> Kind {
    let protocol = protocol_of(controls);
    let encryption = Encryption::ALL[chosen(controls, ENCRYPTION).min(Encryption::ALL.len() - 1)];
    let encrypted = shown(protocol, encryption).encryption && encryption != Encryption::None;
    (protocol, encrypted)
}

/// FTP through a proxy: passive mode checked, the user told so (NetBox's `NB_FTP_PASV_MODE_REQUIRED`).
fn passive_told(host: &Host, controls: &mut Controls, protocol: Protocol) {
    let proxied = controls.value(session_link_tabs::PROXY).state != 0;
    if session_protocol_tabs::passive_made(controls, protocol, proxied) {
        let lines = [session_protocol_tabs::PASSIVE_REQUIRED];
        host.show(&Message::new("Information", &lines).buttons(&["&OK"]));
    }
}

/// Tab `to` shown in place of `from` (NetBox's `SelectTab`): their items hidden and shown, the Session
/// tab's as its protocol shows them, the bar drawn anew, the focus on the tab's first item.
fn select(controls: &mut Controls, kind: Kind, from: usize, to: usize) {
    for item in FIRST[from]..FIRST[from + 1] {
        controls.show(item, false);
    }
    for item in FIRST[to]..FIRST[to + 1] {
        controls.show(item, true);
    }
    if to == 0 {
        session_shown(controls, kind.0);
    }
    session_tabs::update(controls, kind, to);
    controls.set_text(TAB_BAR, &bar(kind, to).text);
    controls.focus(FOCUS[to]);
}

/// A password typed (not the one the session had) is stored only after NetBox's question.
fn password_agreed(host: &Host, controls: &Controls, session: &Session) -> bool {
    let typed = controls.value(PASSWORD).text;
    if typed.is_empty() || typed == session.password {
        return true;
    }
    let lines: Vec<&str> = SAVE_PASSWORD.split('\n').collect();
    let message = Message::new("Warning", &lines)
        .warning()
        .buttons(&["&Ok", "&Cancel"]);
    host.show(&message) == Some(0)
}

/// The Session tab's lines the protocol and the encryption show.
fn session_shown(controls: &mut Controls, protocol: Protocol) {
    let encryption = Encryption::ALL[chosen(controls, ENCRYPTION).min(Encryption::ALL.len() - 1)];
    let show = shown(protocol, encryption);
    controls.show(FALLBACK, show.fallback);
    controls.show(INSECURE, show.insecure);
    controls.show(ENCRYPTION_LABEL, show.encryption);
    controls.show(ENCRYPTION, show.encryption);
}

/// Another protocol or encryption chosen: the lines it shows, its encryptions, the port, S3's host and
/// the tabs (NetBox's `TransferProtocolComboChange` and `UpdateControls`).
fn follow_protocol(controls: &mut Controls) {
    let protocol = protocol_of(controls);
    let lines = encryption_lines(protocol);
    controls.set_lines(ENCRYPTION, &lines);
    let index = chosen(controls, ENCRYPTION).min(lines.len() - 1);
    controls.set_state(ENCRYPTION, i32::try_from(index).unwrap_or(0));
    let encryption = Encryption::ALL[index];
    let show = shown(protocol, encryption);
    session_shown(controls, protocol);
    controls.enable(KEY_LABEL, show.key);
    controls.enable(KEY, show.key);
    controls.set_text(USER_LABEL, user_label(show.access_keys));
    controls.set_text(PASSWORD_LABEL, password_label(show.access_keys));
    if let Ok(port) = controls.value(PORT).text.trim().parse::<u16>() {
        let port = Session::port_for(protocol, encryption, port);
        controls.set_text(PORT, &port.to_string());
    }
    if protocol == Protocol::S3 && controls.value(HOST).text.trim().is_empty() {
        controls.set_text(HOST, S3_HOST);
    }
    let kind = kind_of(controls);
    controls.set_text(TAB_BAR, &bar(kind, 0).text);
    session_tabs::update(controls, kind, 0);
}

#[cfg(test)]
#[path = "session_dialog_tests.rs"]
mod tests;
