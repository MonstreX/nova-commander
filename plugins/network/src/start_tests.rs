use nc_plugin_sdk::system::network::{DISPLAY_DOMAIN, DISPLAY_SERVER, Share};

use super::*;

const MS: &str = "Microsoft Windows Network";

/// SERVER in WORKGROUP of Microsoft's provider; WSL a computer whose parents the network does not tell.
struct Fake;

fn resource(display: u32, remote: &str, provider: &str) -> NetResource {
    NetResource {
        display,
        remote_name: remote.to_string(),
        provider: provider.to_string(),
        ..NetResource::default()
    }
}

impl Network for Fake {
    fn enumerate(&self, _parent: Option<&NetResource>) -> io::Result<Vec<NetResource>> {
        Ok(Vec::new())
    }

    fn connected(&self) -> io::Result<Vec<NetResource>> {
        Ok(Vec::new())
    }

    fn information(&self, remote: &str) -> io::Result<NetResource> {
        match remote {
            r"\\SERVER" => Ok(resource(DISPLAY_SERVER, r"\\SERVER", MS)),
            r"\\WSL.LOCALHOST" => Ok(resource(DISPLAY_SERVER, r"\\wsl.localhost", "Plan 9")),
            "WORKGROUP" => Ok(resource(DISPLAY_DOMAIN, "WORKGROUP", MS)),
            _ => Err(io::Error::from(io::ErrorKind::NotFound)),
        }
    }

    fn parent(&self, resource: &NetResource) -> io::Result<NetResource> {
        match resource.remote_name.as_str() {
            r"\\SERVER" => Ok(resource_parent_of("WORKGROUP", DISPLAY_DOMAIN)),
            "WORKGROUP" => Ok(resource_parent_of(MS, DISPLAY_NETWORK)),
            _ => Err(io::Error::from(io::ErrorKind::Unsupported)),
        }
    }

    fn shares(&self, _server: &str) -> Vec<Share> {
        Vec::new()
    }

    fn connect(&self, _resource: &NetResource) -> io::Result<()> {
        Ok(())
    }
}

fn resource_parent_of(remote: &str, display: u32) -> NetResource {
    resource(display, remote, MS)
}

/// FAR's `SetOpenFromFilePanel`: up from a share the network knows and Microsoft's provider has; the
/// WSL computer's (Plan 9) and an unknown one left to the program.
#[test]
fn up_only_from_microsofts_shares() {
    assert!(goes_up_from(&Fake, r"\\SERVER"));
    assert!(!goes_up_from(&Fake, r"\\WSL.LOCALHOST"));
    assert!(!goes_up_from(&Fake, r"\\NOWHERE\x"));
    assert!(SHARE_UP.eq_ignore_ascii_case("NET_UP"));
}

/// FAR's `SetOpenFromCommandLine`: `\\name` as typed (forward slashes turned), a bare name a
/// computer's, a domain's with `netg:` or the setting; nothing typed, nothing named.
#[test]
fn the_command_names_a_resource() {
    let options = Options::default();
    assert_eq!(
        target("net", r"\\server\share", &options).as_deref(),
        Some(r"\\server\share")
    );
    assert_eq!(
        target("net", "//server", &options).as_deref(),
        Some(r"\\server")
    );
    assert_eq!(
        target("net", " server ", &options).as_deref(),
        Some(r"\\server")
    );
    assert_eq!(
        target("netg", "WORKGROUP", &options).as_deref(),
        Some("WORKGROUP")
    );
    let domains = Options {
        navigate_to_domains: true,
        ..options
    };
    assert_eq!(
        target("net", "WORKGROUP", &domains).as_deref(),
        Some("WORKGROUP")
    );
    assert_eq!(target("net", "  ", &options), None);
}

/// FAR's `ExpandEnvironmentStrings`: a variable's value in its place, an unknown or empty name left.
#[test]
fn variables_are_expanded() {
    let name = std::env::vars()
        .map(|(name, _)| name)
        .find(|name| !name.contains('%'));
    let name = name.expect("the test's process has variables");
    let value = std::env::var(&name).unwrap();
    assert_eq!(
        expand_variables(&format!(r"\\%{name}%\x")),
        format!(r"\\{value}\x")
    );
    assert_eq!(
        expand_variables("%NO_SUCH_VARIABLE_VC_E2E%"),
        "%NO_SUCH_VARIABLE_VC_E2E%"
    );
    assert_eq!(expand_variables("50%%"), "50%%");
    assert_eq!(expand_variables("plain"), "plain");
}

/// FAR's `GotoComputer`: the computer's folder by its parents, its share the cursor's (its whole name,
/// or its own with short names).
#[test]
fn a_computer_opens_under_its_parents() {
    let options = Options::default();
    let found = computer(&Fake, &options, &Favorites::default(), r"\\server\Music").unwrap();
    assert_eq!(found.path, format!(r"/{MS}/WORKGROUP/\\SERVER"));
    assert_eq!(found.cursor.as_deref(), Some(r"\\SERVER\Music"));
    assert_eq!(found.known.len(), 3);
    assert_eq!(found.known[2].1.remote_name, r"\\SERVER");
    let short = Options {
        full_path_shares: false,
        ..options
    };
    let found = computer(&Fake, &short, &Favorites::default(), r"\\server\Music").unwrap();
    assert_eq!(found.cursor.as_deref(), Some("Music"));
    assert_eq!(found.path, format!("/{MS}/WORKGROUP/SERVER"));
}

/// A computer whose parents are not told stands at the top; a name the network lacks is an error.
#[test]
fn a_computer_without_parents_stands_at_the_top() {
    let options = Options::default();
    let found = computer(&Fake, &options, &Favorites::default(), r"\\wsl.localhost").unwrap();
    assert_eq!(found.path, r"/\\wsl.localhost");
    assert_eq!(found.cursor, None);
    assert!(computer(&Fake, &options, &Favorites::default(), r"\\nowhere").is_err());
    let domain = computer(&Fake, &options, &Favorites::default(), "WORKGROUP").unwrap();
    assert_eq!(domain.path, format!("/{MS}/WORKGROUP"));
}

/// "Browse up to favorites if possible": a computer kept in the favorites stands under the Favorites
/// folder, its parents not asked; with the option off, as any other.
#[test]
fn a_favorite_computer_stands_under_the_favorites() {
    let mut favorites = Favorites::default();
    favorites.write(r"\\server\Music", "me", "pw");
    let options = Options::default();
    let found = computer(&Fake, &options, &favorites, r"\\server\Music").unwrap();
    assert_eq!(found.path, r"/Favorites/\\SERVER");
    assert!(crate::favorites::is_folder(&found.known[0].1));
    let off = Options {
        favorites_flags: 0,
        ..Options::default()
    };
    let found = computer(&Fake, &off, &favorites, r"\\server").unwrap();
    assert_eq!(found.path, format!(r"/{MS}/WORKGROUP/\\SERVER"));
}
