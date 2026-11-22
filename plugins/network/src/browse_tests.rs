use std::cell::RefCell;

use nc_plugin_sdk::system::network::{DISPLAY_DOMAIN, SHARE_DISK};

use super::*;

const MS: &str = "Microsoft Windows Network";

fn resource(display: u32, remote: &str, provider: &str) -> NetResource {
    NetResource {
        scope: SCOPE_GLOBALNET,
        kind: if display == DISPLAY_SHARE {
            TYPE_DISK
        } else {
            TYPE_ANY
        },
        display,
        usage: if display == DISPLAY_SHARE {
            1
        } else {
            USAGE_CONTAINER
        },
        remote_name: remote.to_string(),
        provider: provider.to_string(),
        ..NetResource::default()
    }
}

/// A network of the test's own: Microsoft's provider, its WORKGROUP with SERVER, SERVER's shares; a
/// server OTHER only reachable by its name; the calls counted.
#[derive(Default)]
struct Fake {
    asked: RefCell<Vec<String>>,
    refuse_server: bool,
    silent_server: bool,
}

impl Network for Fake {
    fn enumerate(&self, parent: Option<&NetResource>) -> io::Result<Vec<NetResource>> {
        let name = parent.map_or(String::new(), |one| one.remote_name.clone());
        self.asked.borrow_mut().push(format!("enumerate {name}"));
        match name.as_str() {
            "" => Ok(vec![resource(DISPLAY_NETWORK, MS, MS)]),
            MS => Ok(vec![resource(DISPLAY_DOMAIN, "WORKGROUP", MS)]),
            "WORKGROUP" => Ok(vec![resource(DISPLAY_SERVER, r"\\SERVER", MS)]),
            r"\\SERVER" | r"\\OTHER" if self.silent_server => {
                Err(io::Error::from(io::ErrorKind::TimedOut))
            }
            r"\\SERVER" | r"\\OTHER" if self.refuse_server => {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            }
            r"\\SERVER" => Ok(vec![
                NetResource {
                    comment: "Music".to_string(),
                    ..resource(DISPLAY_SHARE, r"\\SERVER\Music", MS)
                },
                NetResource {
                    kind: TYPE_PRINT,
                    ..resource(DISPLAY_SHARE, r"\\SERVER\Printer", MS)
                },
            ]),
            r"\\OTHER" => Ok(vec![resource(DISPLAY_SHARE, r"\\OTHER\Data", MS)]),
            _ => Err(io::Error::from(io::ErrorKind::NotFound)),
        }
    }

    fn connected(&self) -> io::Result<Vec<NetResource>> {
        Ok(vec![NetResource {
            local_name: "Z:".to_string(),
            ..resource(DISPLAY_SHARE, r"\\server\music", MS)
        }])
    }

    fn information(&self, remote: &str) -> io::Result<NetResource> {
        self.asked
            .borrow_mut()
            .push(format!("information {remote}"));
        if remote.eq_ignore_ascii_case(r"\\OTHER") {
            Ok(resource(DISPLAY_SERVER, r"\\OTHER", MS))
        } else {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
    }

    fn parent(&self, _resource: &NetResource) -> io::Result<NetResource> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    fn shares(&self, server: &str) -> Vec<Share> {
        let share = |name: &str| Share {
            name: name.to_string(),
            kind: SHARE_DISK,
            remark: String::new(),
        };
        match server {
            r"\\SERVER" => vec![share("Music"), share("C$"), share("IPC$"), share("ADMIN$")],
            _ => Vec::new(),
        }
    }

    fn connect(&self, resource: &NetResource) -> io::Result<()> {
        self.asked
            .borrow_mut()
            .push(format!("connect {}", resource.remote_name));
        Err(io::Error::from(io::ErrorKind::PermissionDenied))
    }
}

fn names(entries: &[FileEntry]) -> Vec<&str> {
    entries.iter().map(|one| one.name.as_str()).collect()
}

/// FAR's `GetRemoteName`: a provider by its name, a share by its whole path or, without the option,
/// by its own name.
#[test]
fn resources_are_named_as_fars() {
    let options = Options::default();
    assert_eq!(name_of(&resource(DISPLAY_NETWORK, MS, MS), &options), MS);
    let share = resource(DISPLAY_SHARE, r"\\SERVER\Music", MS);
    assert_eq!(name_of(&share, &options), r"\\SERVER\Music");
    let short = Options {
        full_path_shares: false,
        ..options
    };
    assert_eq!(name_of(&share, &short), "Music");
    assert_eq!(
        name_of(&resource(DISPLAY_SERVER, r"\\SERVER", MS), &short),
        "SERVER"
    );
    assert!(is_share(&share));
    assert!(!is_share(&resource(DISPLAY_SERVER, r"\\SERVER", MS)));
}

/// The top, a provider, a domain, a server: each folder by the names on the way; a server's shares with
/// their disks and comments, its hidden shares (not `IPC$`) hidden, printers left out.
#[test]
fn the_folders_down_to_a_servers_shares() {
    let net = Fake::default();
    let options = Options::default();
    let mut browser = Browser::default();
    assert_eq!(names(&browser.list(&net, &options, "/").unwrap()), [MS]);
    let path = format!("/{MS}/WORKGROUP/\\\\SERVER");
    let shares = browser.list(&net, &options, &path).unwrap();
    assert_eq!(
        names(&shares),
        [r"\\SERVER\Music", r"\\SERVER\C$", r"\\SERVER\ADMIN$"]
    );
    assert_eq!(
        shares[0].columns,
        ["Z:", "Music"],
        "its disk and its comment"
    );
    assert!(shares[0].is_dir && !shares[0].hidden);
    assert!(shares[1].hidden, "a $ share shown as hidden");
    let music = browser
        .entry(&net, &options, &path, r"\\SERVER\Music")
        .unwrap();
    assert!(music.as_ref().is_some_and(is_share));

    let printers = Options {
        show_printers: true,
        hidden_shares: HiddenShares::Never,
        ..options
    };
    let mut browser = Browser::default();
    let all = browser.list(&net, &printers, &path).unwrap();
    assert_eq!(names(&all), [r"\\SERVER\Music", r"\\SERVER\Printer"]);
    assert!(!all[1].is_dir && all[1].attributes == VIRTUAL, "a printer");
}

/// A path listed before is not walked again; one never listed is walked from the top once.
#[test]
fn a_path_is_walked_once() {
    let net = Fake::default();
    let options = Options::default();
    let mut browser = Browser::default();
    let path = format!("/{MS}/workgroup");
    browser.list(&net, &options, &path).unwrap();
    let walked = net.asked.borrow().len();
    assert_eq!(walked, 3, "{:?}", net.asked.borrow());
    browser.list(&net, &options, &path).unwrap();
    assert_eq!(net.asked.borrow().len(), walked + 1, "only its own listing");
}

/// FAR's `GotoComputer`: a computer the network does not list is asked for by its name; a name that is
/// none is an error.
#[test]
fn a_computer_by_its_name() {
    let net = Fake::default();
    let options = Options::default();
    let mut browser = Browser::default();
    let data = browser.list(&net, &options, r"/\\other").unwrap();
    assert_eq!(names(&data), [r"\\OTHER\Data"]);
    assert!(browser.list(&net, &options, r"/\\none").is_err());
    assert!(browser.list(&net, &options, "/Nothing").is_err());
}

/// A server that refuses its listing: a connection tried (FAR's "try again with connection"), else an
/// empty folder; the top refused is an error.
#[test]
fn a_refused_listing_is_empty() {
    let net = Fake {
        refuse_server: true,
        ..Fake::default()
    };
    let options = Options {
        hidden_shares: HiddenShares::Never,
        ..Options::default()
    };
    let mut browser = Browser::default();
    let listed = browser.list(&net, &options, r"/\\OTHER").unwrap();
    assert!(listed.is_empty());
    assert!(
        net.asked
            .borrow()
            .iter()
            .any(|one| one == r"connect \\OTHER")
    );
}

/// A listing that does not answer yet is told so (FAR's connection not tried, no empty folder); a share
/// is never listed (Enter and Ctrl+PgDn leave for it), nor connected to.
#[test]
fn a_silent_listing_is_told_a_share_never_listed() {
    let net = Fake {
        silent_server: true,
        ..Fake::default()
    };
    let options = Options::default();
    let mut browser = Browser::default();
    let error = browser.list(&net, &options, r"/\\OTHER").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(
        !net.asked
            .borrow()
            .iter()
            .any(|one| one.starts_with("connect"))
    );

    let net = Fake::default();
    let mut browser = Browser::default();
    let server = format!("/{MS}/WORKGROUP/\\\\SERVER");
    browser.list(&net, &options, &server).unwrap();
    let share = format!(r"{server}/\\SERVER\Music");
    assert!(browser.list(&net, &options, &share).is_err());
    assert!(
        !net.asked
            .borrow()
            .iter()
            .any(|one| one.starts_with("connect"))
    );
}

#[test]
fn paths_split_and_join() {
    assert_eq!(split("/a"), ("/", "a"));
    assert_eq!(split("/a/b"), ("/a", "b"));
    assert_eq!(join("/", "a"), "/a");
    assert_eq!(join("/a", "b"), "/a/b");
}

/// A server's listing as the panel shows it has its hidden shares: `\\server\c$` typed finds the
/// cursor's name `\\SERVER\C$` (the program's names match by case).
#[test]
fn a_hidden_share_is_found_by_its_listing() {
    let net = Fake::default();
    let options = Options::default();
    let server = resource(DISPLAY_SERVER, r"\\SERVER", MS);
    let listed = net.enumerate(Some(&server)).unwrap();
    let shown = with_hidden_shares(&net, &options, Some(&server), listed);
    assert_eq!(
        crate::opening::cursor_name(&shown, &options, r"\\server\c$"),
        r"\\SERVER\C$"
    );
    let domain = resource(DISPLAY_DOMAIN, "WORKGROUP", MS);
    let computers = with_hidden_shares(&net, &options, Some(&domain), Vec::new());
    assert!(computers.is_empty(), "a domain has no hidden shares");
}
