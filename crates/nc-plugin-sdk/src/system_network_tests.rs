use super::*;

fn own_server() -> String {
    format!(r"\\{}", computer_name().expect("the computer's name"))
}

/// The top lists the providers, Microsoft's among them; each a container of its own.
#[test]
fn the_top_lists_the_providers() {
    let top = enumerate(SCOPE_GLOBALNET, TYPE_ANY, None).unwrap();
    let windows = top
        .iter()
        .find(|one| one.provider == "Microsoft Windows Network")
        .expect("Microsoft's provider");
    assert_eq!(windows.display, DISPLAY_NETWORK);
    assert_ne!(windows.usage & USAGE_CONTAINER, 0);
}

/// This computer by its name: a server, its parent its domain or workgroup, its shares under it.
#[test]
fn a_server_its_parent_and_its_shares() {
    let server = own_server();
    let known = resource_information(&server).unwrap();
    assert_eq!(known.display, DISPLAY_SERVER);
    assert!(known.remote_name.eq_ignore_ascii_case(&server), "{known:?}");
    if let Ok(parent) = resource_parent(&known) {
        assert_eq!(parent.display, DISPLAY_DOMAIN, "{parent:?}");
    }
    let shares = enumerate(SCOPE_GLOBALNET, TYPE_ANY, Some(&known)).unwrap();
    let prefix = format!(r"{}\", server.to_lowercase());
    assert!(
        shares.iter().all(|one| one.display == DISPLAY_SHARE
            && one.remote_name.to_lowercase().starts_with(&prefix)),
        "{shares:?}"
    );
}

/// `NetShareEnum` gives the hidden shares too: `IPC$` always.
#[test]
fn a_servers_shares_with_the_hidden_ones() {
    let shares = server_shares(r"\\localhost");
    assert!(shares.iter().any(|one| one.name == "IPC$"), "{shares:?}");
    assert!(server_shares(r"\\").is_empty(), "no server: none");
}

/// The free letters of a mask from a letter on; the system's disk is never free.
#[test]
fn free_letters_and_the_disks_there_are() {
    let used = 0b101; // A: and C:
    assert_eq!(free_letters(used, 'A')[..2], ['B', 'D']);
    assert_eq!(free_letters(used, 'C')[0], 'D', "FAR's map starts at C");
    assert_eq!(free_letters(u32::MAX, 'A'), Vec::<char>::new());
    let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let letter = system.chars().next().unwrap().to_ascii_uppercase();
    assert!(!free_letters(used_letters(), 'A').contains(&letter));
}

/// A connection that is not there cannot be ended: the system's code comes back.
#[test]
fn a_connection_not_there_is_not_ended() {
    let error = disconnect(r"\\no-such-computer-vc-e2e.invalid\x", false, false).unwrap_err();
    assert!(error.raw_os_error().is_some(), "{error:?}");
}

/// A name the network does not have is an error, not a resource.
#[test]
fn an_unknown_name_is_an_error() {
    assert!(resource_information(r"\\no-such-computer-vc-e2e.invalid").is_err());
}
