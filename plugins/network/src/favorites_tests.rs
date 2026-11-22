use super::*;

/// FAR's `SplitPath`: the server upper case, the share as it is; `Favorites\...` and the slashes off.
#[test]
fn a_remote_name_splits_as_fars() {
    assert_eq!(
        split(r"\\server\Music"),
        ("SERVER".to_string(), "Music".to_string())
    );
    assert_eq!(split(r"\\server"), ("SERVER".to_string(), String::new()));
    assert_eq!(
        split(r"Favorites\\server\x"),
        ("SERVER".to_string(), "x".to_string())
    );
}

/// Written, kept, read back: a server's and a share's own names and passwords, tabs and slashes in them
/// too; written again over what was kept.
#[test]
fn favorites_are_kept_and_read_back() {
    let mut favorites = Favorites::default();
    favorites.write(r"\\server\Music", r"DOMAIN\me", "pa\tss\\word");
    // Line breaks in a password too, a carriage return at its end, on a line that is not the last.
    favorites.write(r"\\third", "u", "a\nb\r");
    favorites.write("", "nobody", "x");
    favorites.write(r"\\other", "guest", "");
    favorites.write(r"\\SERVER\music", "me", "new");
    let read = Favorites::parse(&favorites.text());
    assert_eq!(read, favorites);
    assert_eq!(
        read.logon(r"\\server\MUSIC"),
        Some(("me".to_string(), "new".to_string()))
    );
    assert_eq!(read.logon(r"\\server"), None, "the share's only");
    assert_eq!(
        read.logon(r"\\OTHER"),
        Some(("guest".to_string(), String::new()))
    );
    assert_eq!(
        read.logon(r"\\third"),
        Some(("u".to_string(), "a\nb\r".to_string()))
    );
    assert_eq!(
        read.servers(),
        ["OTHER", "SERVER", "THIRD"],
        "no server: nothing kept"
    );
    assert!(read.contains(r"\\server") && read.contains(r"\\server\Music"));
    assert!(!read.contains(r"\\server\Films") && !read.contains(r"\\fourth"));
}

/// FAR's `RemoveFromFavorites`: a server with its shares; a share alone.
#[test]
fn a_server_goes_with_its_shares() {
    let mut favorites = Favorites::default();
    favorites.write(r"\\server\a", "u", "p");
    favorites.write(r"\\server\b", "u", "p");
    favorites.write(r"\\other", "u", "p");
    favorites.remove(r"\\server\a");
    assert!(!favorites.contains(r"\\server\a") && favorites.contains(r"\\server\b"));
    favorites.remove(r"\\SERVER");
    assert_eq!(favorites.servers(), ["OTHER"]);
}

/// The network the test plays: one provider at the top, nothing below; the answers kept heard (the
/// folder's or the top's, and how many resources).
#[derive(Default)]
struct Top {
    kept: std::cell::RefCell<Vec<String>>,
}

impl Network for Top {
    fn enumerate(&self, parent: Option<&NetResource>) -> io::Result<Vec<NetResource>> {
        match parent {
            None => Ok(vec![NetResource {
                provider: "Microsoft Windows Network".to_string(),
                ..NetResource::default()
            }]),
            Some(_) => Err(io::Error::from(io::ErrorKind::NotFound)),
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
        Ok(())
    }

    fn keep(&self, parent: Option<&NetResource>, answer: io::Result<Vec<NetResource>>) {
        let whose = parent.map_or("top".to_string(), |one| one.remote_name.clone());
        let count = answer.map_or(0, |listed| listed.len());
        self.kept.borrow_mut().push(format!("{whose} {count}"));
    }
}

/// An answer kept: the top's without the folder (the folder is added as the top is read), the folder's
/// own never; another's as it is.
#[test]
fn the_network_keeps_its_own_answers_only() {
    let net = Top::default();
    let kept = Favorites::default();
    let favored = favored(&net, &kept);
    let top = favored.enumerate(None).unwrap();
    favored.keep(None, Ok(top.clone()));
    favored.keep(Some(&folder()), Ok(Vec::new()));
    let server = NetResource {
        remote_name: r"\\SERVER".to_string(),
        ..NetResource::default()
    };
    favored.keep(Some(&server), Ok(top));
    assert_eq!(*net.kept.borrow(), ["top 1", r"\\SERVER 2"]);
}

/// FAR's `EnumFavorites`: the Favorites folder first at the top (named so), its computers under it as
/// servers; the rest the network's.
#[test]
fn the_favorites_folder_is_first_at_the_top() {
    let mut kept = Favorites::default();
    kept.write(r"\\server\Music", "me", "pw");
    kept.write(r"\\alpha", "me", "pw");
    let top_net = Top::default();
    let net = favored(&top_net, &kept);
    let top = net.enumerate(None).unwrap();
    assert!(is_folder(&top[0]) && top.len() == 2);
    let options = crate::options::Options::default();
    assert_eq!(crate::browse::name_of(&top[0], &options), "Favorites");
    let inside = net.enumerate(Some(&top[0])).unwrap();
    let names: Vec<&str> = inside.iter().map(|one| one.remote_name.as_str()).collect();
    assert_eq!(names, [r"\\ALPHA", r"\\SERVER"]);
    assert!(inside.iter().all(|one| one.display == DISPLAY_SERVER));
    assert!(net.enumerate(Some(&top[1])).is_err(), "the network's own");
}

/// A line that is not four fields, or with no server, is let be.
#[test]
fn a_broken_line_is_let_be() {
    let read = Favorites::parse("junk\n\tshare\tu\tp\nSERVER\t\tu\tp");
    assert_eq!(read.servers(), ["SERVER"]);
}
