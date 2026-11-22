use std::cell::RefCell;
use std::collections::VecDeque;

use nc_plugin_sdk::system::network::{ERROR_LOGON_FAILURE, SCOPE_CONNECTED};

use super::*;

const SHARE: &str = r"\\SERVER\Music";

/// The system played by the test: what each call answers, every call heard.
#[derive(Default)]
struct Net {
    heard: RefCell<Vec<String>>,
    /// Each connection's answer in turn (none left: done).
    connects: RefCell<VecDeque<io::Result<()>>>,
    /// Each disconnection's answer in turn (none left: done).
    disconnects: RefCell<VecDeque<io::Result<()>>>,
    readable: Vec<String>,
    connections: Vec<NetResource>,
    remembered: bool,
    used: u32,
    on_disk: bool,
}

fn failing(code: i32) -> io::Result<()> {
    Err(io::Error::from_raw_os_error(code))
}

fn connection(local: &str, remote: &str) -> NetResource {
    NetResource {
        scope: SCOPE_CONNECTED,
        kind: TYPE_DISK,
        local_name: local.to_string(),
        remote_name: remote.to_string(),
        ..NetResource::default()
    }
}

impl Connections for Net {
    fn connect_as(
        &self,
        local: &str,
        remote: &str,
        logon: Option<Logon<'_>>,
        remember: bool,
    ) -> io::Result<()> {
        let as_whom = logon.map_or("own".to_string(), |logon| {
            format!("{}/{}", logon.user, logon.password)
        });
        self.heard
            .borrow_mut()
            .push(format!("connect {local} {remote} {as_whom} {remember}"));
        self.connects.borrow_mut().pop_front().unwrap_or(Ok(()))
    }

    fn disconnect(&self, name: &str, forget: bool, force: bool) -> io::Result<()> {
        self.heard
            .borrow_mut()
            .push(format!("disconnect {name} {forget} {force}"));
        self.disconnects.borrow_mut().pop_front().unwrap_or(Ok(()))
    }

    fn connections(&self) -> Vec<NetResource> {
        self.connections.clone()
    }

    fn remembered(&self, _letter: char) -> bool {
        self.remembered
    }

    fn used_letters(&self) -> u32 {
        self.used
    }

    fn readable(&self, path: &str) -> bool {
        self.readable.iter().any(|one| one == path)
    }

    fn hands_off(&self, letter: char) -> bool {
        self.heard.borrow_mut().push(format!("hands off {letter}"));
        self.on_disk
    }

    fn off_network(&self) {
        self.heard.borrow_mut().push("off network".to_string());
    }
}

/// The user played by the test: the answers given, every question heard.
#[derive(Default)]
struct User {
    heard: RefCell<Vec<String>>,
    letter: Option<(char, bool)>,
    logon: Option<(String, String)>,
    /// "Remember password" as the dialog was left.
    remember: bool,
    /// The name and password the favorites keep.
    saved: Option<(String, String)>,
    confirm: Option<bool>,
    which: Option<usize>,
    force: bool,
}

impl Asker for User {
    fn letter(&self, free: &[char], permanent: bool) -> Option<(char, bool)> {
        let free: String = free.iter().collect();
        self.heard
            .borrow_mut()
            .push(format!("letter {free} {permanent}"));
        self.letter
    }

    fn logon(&self, remote: &str) -> Option<Given> {
        self.heard.borrow_mut().push(format!("logon {remote}"));
        self.logon.clone().map(|(user, password)| Given {
            user,
            password,
            remember: self.remember,
        })
    }

    fn saved_logon(&self, remote: &str) -> Option<(String, String)> {
        self.heard.borrow_mut().push(format!("saved {remote}"));
        self.saved.clone()
    }

    fn save_logon(&self, remote: &str, user: &str, password: &str) {
        self.heard
            .borrow_mut()
            .push(format!("save {remote} {user}/{password}"));
    }

    fn confirm_disconnect(
        &self,
        local: &str,
        remote: &str,
        remembered: bool,
        reconnect: bool,
    ) -> Option<bool> {
        self.heard
            .borrow_mut()
            .push(format!("confirm {local} {remote} {remembered} {reconnect}"));
        self.confirm
    }

    fn which_local(&self, remote: &str, locals: &[String]) -> Option<usize> {
        self.heard
            .borrow_mut()
            .push(format!("which {remote} {}", locals.join(",")));
        self.which
    }

    fn force(&self, local: &str) -> bool {
        self.heard.borrow_mut().push(format!("force {local}"));
        self.force
    }

    fn error(&self, lines: &[String], error: Option<&io::Error>) {
        let code = error.and_then(io::Error::raw_os_error).unwrap_or(0);
        self.heard
            .borrow_mut()
            .push(format!("error {} {code}", lines.join("|")));
    }
}

fn heard(of: &RefCell<Vec<String>>) -> Vec<String> {
    of.borrow().clone()
}

/// F5: the first free letter from C (A: and C: taken here: D:), the user's own credentials when the
/// share reads; Shift: not remembered.
#[test]
fn a_readable_share_is_mapped_to_the_first_free_letter() {
    let net = Net {
        readable: vec![SHARE.to_string()],
        used: 0b101,
        ..Net::default()
    };
    let user = User::default();
    assert!(map(&net, &user, SHARE, false, true));
    assert!(map(&net, &user, SHARE, false, false));
    assert_eq!(
        heard(&net.heard),
        [
            r"connect D: \\SERVER\Music own true",
            r"connect D: \\SERVER\Music own false"
        ]
    );
    assert!(heard(&user.heard).is_empty(), "nothing asked");
}

/// F6: the letter asked for among the free ones from A, the menu's F6 turning the kind round.
#[test]
fn f6_asks_for_the_letter_and_the_kind() {
    let net = Net {
        readable: vec![SHARE.to_string()],
        used: !0b1_0010, // only B: and E: free
        ..Net::default()
    };
    let user = User {
        letter: Some(('E', false)),
        ..User::default()
    };
    assert!(map(&net, &user, SHARE, true, true));
    assert_eq!(heard(&user.heard), ["letter BE true"]);
    assert_eq!(heard(&net.heard), [r"connect E: \\SERVER\Music own false"]);
    let cancelled = User::default();
    assert!(!map(&net, &cancelled, SHARE, true, true), "Esc: nothing");
}

/// A letter remembered for another connection: F5 takes the next free one, F6 tells.
#[test]
fn a_remembered_letter_is_passed_by_f5_and_told_by_f6() {
    let net = Net {
        readable: vec![SHARE.to_string()],
        used: 0b101,
        connects: RefCell::new(VecDeque::from([failing(ERROR_DEVICE_ALREADY_REMEMBERED)])),
        ..Net::default()
    };
    let user = User::default();
    assert!(map(&net, &user, SHARE, false, true));
    assert_eq!(
        heard(&net.heard),
        [
            r"connect D: \\SERVER\Music own true",
            r"connect E: \\SERVER\Music own true"
        ]
    );
    let net = Net {
        readable: vec![SHARE.to_string()],
        connects: RefCell::new(VecDeque::from([failing(ERROR_DEVICE_ALREADY_REMEMBERED)])),
        ..Net::default()
    };
    let user = User {
        letter: Some(('X', true)),
        ..User::default()
    };
    assert!(!map(&net, &user, SHARE, true, true));
    assert_eq!(
        heard(&user.heard).last().unwrap(),
        "error The disk name has a remembered connection 0"
    );
}

/// No letter free: told, nothing connected.
#[test]
fn no_free_letter_is_told() {
    let net = Net {
        used: u32::MAX,
        ..Net::default()
    };
    let user = User::default();
    assert!(!map(&net, &user, SHARE, false, true));
    assert!(!map(&net, &user, SHARE, true, true));
    assert_eq!(
        heard(&user.heard),
        [
            "error No free disk letters available 0",
            "error No free disk letters available 0"
        ]
    );
    assert!(heard(&net.heard).is_empty());
}

/// A share that does not read: none kept in the favorites, the name and password asked for, then
/// connected with them; a plain name refused tried again as the server's user; the disk read after.
#[test]
fn an_unreadable_share_asks_for_a_name_and_password() {
    let net = Net {
        readable: vec!["D:".to_string()],
        used: 0b111,
        connects: RefCell::new(VecDeque::from([failing(ERROR_LOGON_FAILURE)])),
        ..Net::default()
    };
    let user = User {
        logon: Some(("guest".to_string(), "secret".to_string())),
        ..User::default()
    };
    assert!(map(&net, &user, SHARE, false, false));
    assert_eq!(
        heard(&user.heard),
        [r"saved \\SERVER\Music", r"logon \\SERVER\Music"]
    );
    assert_eq!(
        heard(&net.heard),
        [
            r"connect D: \\SERVER\Music guest/secret false",
            r"connect D: \\SERVER\Music SERVER\guest/secret false"
        ]
    );
}

/// A name and password not given: this share left (FAR: the next ones still mapped), nothing told.
#[test]
fn a_cancelled_logon_leaves_the_share() {
    let net = Net::default();
    let user = User::default();
    assert!(map(&net, &user, SHARE, false, true));
    assert_eq!(
        heard(&user.heard),
        [r"saved \\SERVER\Music", r"logon \\SERVER\Music"]
    );
    assert!(heard(&net.heard).is_empty());
}

/// FAR's `AddConnectionFromFavorites` first: the name and password kept connect without a question;
/// refused, they are asked for.
#[test]
fn the_favorites_name_and_password_come_first() {
    let kept = || Some(("me".to_string(), "kept".to_string()));
    let net = Net {
        readable: vec!["D:".to_string()],
        used: 0b111,
        ..Net::default()
    };
    let user = User {
        saved: kept(),
        ..User::default()
    };
    assert!(map(&net, &user, SHARE, false, false));
    assert_eq!(
        heard(&user.heard),
        [r"saved \\SERVER\Music"],
        "nothing asked"
    );
    assert_eq!(
        heard(&net.heard),
        [r"connect D: \\SERVER\Music me/kept false"]
    );
    let refused = Net {
        connects: RefCell::new(VecDeque::from([
            failing(ERROR_LOGON_FAILURE),
            failing(ERROR_LOGON_FAILURE),
        ])),
        ..Net::default()
    };
    let user = User {
        saved: kept(),
        ..User::default()
    };
    let asked = connect_saved_or_asked(&refused, &user, ("", SHARE), true);
    assert!(asked.is_err(), "cancelled after");
    assert_eq!(
        heard(&user.heard),
        [r"saved \\SERVER\Music", r"logon \\SERVER\Music"]
    );
}

/// "Remember password": kept in the favorites once connected, not when the connection fails.
#[test]
fn remember_password_keeps_the_name_and_password_once_connected() {
    let given = || Some(("guest".to_string(), "secret".to_string()));
    let user = User {
        logon: given(),
        remember: true,
        ..User::default()
    };
    connect_explicit(&Net::default(), &user, ("", SHARE), true).unwrap();
    assert_eq!(
        heard(&user.heard),
        [r"logon \\SERVER\Music", r"save \\SERVER\Music guest/secret"]
    );
    let failed = Net {
        connects: RefCell::new(VecDeque::from([failing(53), failing(53)])),
        ..Net::default()
    };
    let user = User {
        logon: given(),
        remember: true,
        ..User::default()
    };
    assert!(connect_explicit(&failed, &user, ("", SHARE), true).is_err());
    assert_eq!(heard(&user.heard), [r"logon \\SERVER\Music"], "not kept");
}

/// A connection refused otherwise: FAR's "Cannot connect" with the system's reason.
#[test]
fn a_refused_connection_is_told() {
    let net = Net {
        readable: vec![SHARE.to_string()],
        connects: RefCell::new(VecDeque::from([failing(53)])),
        ..Net::default()
    };
    let user = User::default();
    assert!(!map(&net, &user, SHARE, false, true));
    assert_eq!(
        heard(&user.heard),
        [r#"error Cannot connect "\\SERVER\Music" to C: 53"#]
    );
}

/// A session with other credentials: every connection to that server ended (not another's), the
/// connection tried again; a name with a domain is not tried as the server's user.
#[test]
fn a_conflicting_session_is_ended_first() {
    let net = Net {
        connects: RefCell::new(VecDeque::from([
            failing(ERROR_SESSION_CREDENTIAL_CONFLICT),
            Ok(()),
        ])),
        connections: vec![
            connection("Z:", r"\\server\other"),
            connection("", r"\\SERVER2\x"),
        ],
        ..Net::default()
    };
    connect_with_logon(&net, ("", SHARE), (r"DOMAIN\me", "pw"), false).unwrap();
    assert_eq!(
        heard(&net.heard),
        [
            r"connect  \\SERVER\Music DOMAIN\me/pw false",
            r"disconnect \\server\other false true",
            r"disconnect \\SERVER false true",
            "off network",
            r"connect  \\SERVER\Music DOMAIN\me/pw false",
        ]
    );
    let refused = Net {
        connects: RefCell::new(VecDeque::from([failing(ERROR_LOGON_FAILURE)])),
        ..Net::default()
    };
    let error = connect_with_logon(&refused, ("", SHARE), ("me@domain", "pw"), false).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(ERROR_LOGON_FAILURE));
    assert_eq!(heard(&refused.heard).len(), 1, "tried once");
}

/// F8: the share's one disk disconnected after FAR's question; "Reconnect at Logon" off forgets it, the
/// answer handed back to be saved only for a disk restored at logon.
#[test]
fn a_mapped_share_is_disconnected_after_the_question() {
    let net = Net {
        connections: vec![connection("Z:", r"\\server\music")],
        remembered: true,
        ..Net::default()
    };
    let user = User {
        confirm: Some(false),
        ..User::default()
    };
    let asked = Disconnecting {
        confirm: true,
        reconnect: true,
    };
    assert_eq!(unmap(&net, &user, SHARE, asked), (true, Some(false)));
    assert_eq!(heard(&user.heard), [r"confirm Z: \\SERVER\Music true true"]);
    assert_eq!(heard(&net.heard), ["disconnect Z: true false"]);
    let temporary = Net {
        connections: vec![connection("Z:", SHARE)],
        ..Net::default()
    };
    let quiet = Disconnecting {
        confirm: false,
        reconnect: false,
    };
    assert_eq!(
        unmap(&temporary, &User::default(), SHARE, quiet),
        (true, None)
    );
    let refused = User::default();
    assert_eq!(unmap(&net, &refused, SHARE, asked), (false, None), "Cancel");
}

/// More letters for the share: which one asked; none: nothing to do.
#[test]
fn the_letter_to_disconnect_is_asked_among_several() {
    let net = Net {
        connections: vec![
            connection("Y:", SHARE),
            connection("Z:", SHARE),
            connection("X:", r"\\SERVER\Other"),
        ],
        ..Net::default()
    };
    let user = User {
        which: Some(1),
        confirm: Some(true),
        ..User::default()
    };
    let asked = Disconnecting {
        confirm: true,
        reconnect: false,
    };
    assert_eq!(unmap(&net, &user, SHARE, asked), (true, None));
    assert_eq!(heard(&user.heard)[0], r"which \\SERVER\Music Y:,Z:");
    assert_eq!(heard(&net.heard), ["disconnect Z: false false"]);
    let none = Net::default();
    assert_eq!(unmap(&none, &user, SHARE, asked), (false, None));
}

/// A disk in use: the process's folder moved off it and tried again; files open: asked, and ended by
/// force; a failure told.
#[test]
fn a_disk_in_use_is_tried_again_and_forced_when_asked() {
    let net = Net {
        connections: vec![connection("Z:", SHARE)],
        disconnects: RefCell::new(VecDeque::from([
            failing(ERROR_OPEN_FILES),
            failing(ERROR_OPEN_FILES),
        ])),
        on_disk: true,
        ..Net::default()
    };
    let user = User {
        force: true,
        ..User::default()
    };
    let quiet = Disconnecting {
        confirm: false,
        reconnect: false,
    };
    assert_eq!(unmap(&net, &user, SHARE, quiet), (true, None));
    assert_eq!(
        heard(&net.heard),
        [
            "disconnect Z: true false",
            "hands off Z",
            "disconnect Z: true false",
            "disconnect Z: true true"
        ]
    );
    // "Cancel" to the open files: left connected without a word, the next shares still done (FAR's).
    let open = Net {
        connections: vec![connection("Z:", SHARE)],
        disconnects: RefCell::new(VecDeque::from([failing(ERROR_DEVICE_IN_USE)])),
        ..Net::default()
    };
    let kept = User::default();
    assert_eq!(unmap(&open, &kept, SHARE, quiet), (true, None));
    assert_eq!(heard(&kept.heard), ["force Z:"]);
    assert_eq!(
        heard(&open.heard),
        ["disconnect Z: true false", "hands off Z"]
    );
    let failed = Net {
        connections: vec![connection("Z:", SHARE)],
        disconnects: RefCell::new(VecDeque::from([failing(5)])),
        ..Net::default()
    };
    let user = User::default();
    assert_eq!(unmap(&failed, &user, SHARE, quiet), (false, None));
    assert_eq!(heard(&user.heard), ["error Cannot disconnect from Z: 5"]);
}

#[test]
fn names_are_quoted_and_servers_found() {
    assert_eq!(quoted("Music"), "Music");
    assert_eq!(quoted("My Music"), "\"My Music\"");
    assert_eq!(server_of(r"\\SERVER\Music"), "SERVER");
    assert_eq!(server_of(r"\\SERVER"), "SERVER");
}
