use std::cell::RefCell;

use nc_plugin_sdk::system::network::{DISPLAY_SERVER, DISPLAY_SHARE, Share};

use super::*;

/// A network answering each listing with `answer`, the calls and the kept answers counted.
struct Fake {
    answer: io::ErrorKind,
    listed: bool,
    asked: RefCell<Vec<String>>,
    /// The system's code a connection fails with (none: refused plainly).
    connect_code: Option<i32>,
}

impl Fake {
    fn new(listed: bool, answer: io::ErrorKind) -> Self {
        Self {
            answer,
            listed,
            asked: RefCell::default(),
            connect_code: None,
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.borrow().clone()
    }
}

impl Network for Fake {
    fn enumerate(&self, _parent: Option<&NetResource>) -> io::Result<Vec<NetResource>> {
        self.asked.borrow_mut().push("enumerate".to_string());
        if self.listed {
            Ok(vec![NetResource::default()])
        } else {
            Err(io::Error::from(self.answer))
        }
    }

    fn connected(&self) -> io::Result<Vec<NetResource>> {
        Ok(Vec::new())
    }

    fn information(&self, _remote: &str) -> io::Result<NetResource> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    fn parent(&self, _resource: &NetResource) -> io::Result<NetResource> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    fn shares(&self, _server: &str) -> Vec<Share> {
        Vec::new()
    }

    fn connect(&self, _resource: &NetResource) -> io::Result<()> {
        self.asked.borrow_mut().push("connect".to_string());
        Err(self.connect_code.map_or_else(
            || io::Error::from(io::ErrorKind::PermissionDenied),
            io::Error::from_raw_os_error,
        ))
    }

    fn keep(&self, _parent: Option<&NetResource>, answer: io::Result<Vec<NetResource>>) {
        let count = answer.map_or(0, |listed| listed.len());
        self.asked.borrow_mut().push(format!("keep {count}"));
    }
}

fn server() -> NetResource {
    NetResource {
        display: DISPLAY_SERVER,
        usage: USAGE_CONTAINER,
        remote_name: r"\\SERVER".to_string(),
        ..NetResource::default()
    }
}

/// No name and password wanted: never asked.
fn never(_resource: &NetResource) -> io::Result<()> {
    panic!("no name and password asked for")
}

/// A container that lists opens, its listing kept for the panel's reading.
#[test]
fn a_container_that_lists_opens_and_keeps_its_listing() {
    let net = Fake::new(true, io::ErrorKind::Other);
    assert!(container_opens(&net, &server(), true, &never).is_ok());
    assert_eq!(net.asked(), ["enumerate", "keep 1"]);
}

/// Refused, a connection is tried (FAR's `AddConnection`) and its failure told; one that does not answer
/// yet is told so, no connection tried.
#[test]
fn a_refused_container_is_connected_one_not_answering_told() {
    let refused = Fake::new(false, io::ErrorKind::PermissionDenied);
    let error = container_opens(&refused, &server(), true, &never).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(refused.asked(), ["enumerate", "connect"]);
    let silent = Fake::new(false, io::ErrorKind::TimedOut);
    let error = container_opens(&silent, &server(), true, &never).unwrap_err();
    assert!(not_answering(&error));
    assert_eq!(silent.asked(), ["enumerate"]);
}

/// A logon refused: the name and password asked for (FAR's `AddConnectionExplicit`), its answer the
/// container's; FAR's codes for a share and for a container.
#[test]
fn a_refused_logon_asks_for_a_name_and_password() {
    let mut net = Fake::new(false, io::ErrorKind::PermissionDenied);
    net.connect_code = Some(ERROR_LOGON_FAILURE);
    let asked = RefCell::new(Vec::new());
    let explicit = |resource: &NetResource| {
        asked.borrow_mut().push(resource.remote_name.clone());
        Err(io::Error::from_raw_os_error(ERROR_CANCELLED))
    };
    let error = container_opens(&net, &server(), true, &explicit).unwrap_err();
    assert!(cancelled(&error));
    assert_eq!(*asked.borrow(), [r"\\SERVER"]);
    let code = |code: i32| io::Error::from_raw_os_error(code);
    assert!(share_asks_logon(&code(ERROR_INVALID_HANDLE)));
    assert!(!share_asks_logon(&code(ERROR_LOGON_TYPE_NOT_GRANTED)));
    assert!(container_asks_logon(&code(ERROR_LOGON_TYPE_NOT_GRANTED)));
    assert!(!container_asks_logon(&code(ERROR_INVALID_HANDLE)));
    assert!(!share_asks_logon(&code(53)), "a computer not found: told");
}

/// Domains and networks are taken as they are when the network is not scanned (FAR's `ScanNetwork` off).
#[test]
fn an_unscanned_domain_is_not_asked() {
    let net = Fake::new(false, io::ErrorKind::TimedOut);
    let domain = NetResource {
        display: DISPLAY_DOMAIN,
        ..server()
    };
    assert!(container_opens(&net, &domain, false, &never).is_ok());
    assert!(net.asked().is_empty());
    assert!(
        container_opens(&net, &server(), false, &never).is_err(),
        "a server is"
    );
}

/// Enter leaves for the share's own path; Ctrl+PgDn for the disk it is connected as, when it is one.
#[test]
fn a_share_is_entered_by_its_path_or_its_disk() {
    let share = NetResource {
        display: DISPLAY_SHARE,
        remote_name: r"\\SERVER\Music".to_string(),
        ..NetResource::default()
    };
    let connected = vec![NetResource {
        local_name: "Z:".to_string(),
        remote_name: r"\\server\music".to_string(),
        ..NetResource::default()
    }];
    assert_eq!(
        share_folder(&share, &connected, false),
        PathBuf::from(r"\\SERVER\Music")
    );
    assert_eq!(
        share_folder(&share, &connected, true),
        PathBuf::from(r"Z:\")
    );
    assert_eq!(
        share_folder(&share, &[], true),
        PathBuf::from(r"\\SERVER\Music")
    );
}

#[test]
fn the_system_number_is_left_out() {
    let error = io::Error::from_raw_os_error(5);
    assert!(!system_text(&error).contains("os error"));
    let own = io::Error::other("plain");
    assert_eq!(system_text(&own), "plain");
}

#[test]
fn ctrl_digits_are_the_programs() {
    let listed: Vec<String> = keys().into_iter().map(|key| key.key).collect();
    assert!(listed.iter().any(|key| key == "Ctrl+4"));
    assert!(listed.iter().any(|key| key == "Ctrl+PgDn"));
}

/// Ctrl+0..5 are the program's modes, 6..9 only where the plugin has its own: none here, the mode
/// chosen stays.
#[test]
fn only_a_mode_that_exists_is_chosen() {
    assert!((0..6).all(crate::mode_exists));
    assert!(!(6..10).any(crate::mode_exists));
    assert!(!crate::mode_exists(u32::MAX));
}
