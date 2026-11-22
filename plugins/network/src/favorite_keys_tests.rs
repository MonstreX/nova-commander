use super::*;

/// FAR's `RemoveItems` question: one computer by its name, more by their count.
#[test]
fn the_question_names_one_and_counts_more() {
    let one = [r"\\SERVER".to_string()];
    assert_eq!(remove_question(&one), r#"Remove "\\SERVER" from Favorites"#);
    let two = [r"\\A".to_string(), r"\\B".to_string()];
    assert_eq!(remove_question(&two), "Remove 2 items from Favorites");
}

fn server() -> NetResource {
    NetResource {
        display: nc_plugin_sdk::system::network::DISPLAY_SERVER,
        remote_name: r"\\SERVER".to_string(),
        provider: "Microsoft Windows Network".to_string(),
        ..NetResource::default()
    }
}

/// FAR's `GetFavoritesParent`: ".." on a kept computer reached through its workgroup goes up to the
/// Favorites folder, the computer under the cursor; not with the option off, not for one not kept, not
/// from the folder's own entries, not at the top; a kept domain goes up too, a share never.
#[test]
fn dot_dot_goes_up_to_the_favorites_as_fars() {
    let mut kept = Favorites::default();
    kept.write(r"\\server", "me", "pw");
    kept.write("WORKGROUP", "me", "pw");
    let options = Options::default();
    let path = r"/Microsoft Windows Network/WORKGROUP/\\SERVER";
    assert_eq!(
        favorites_parent(path, &server(), &options, &kept),
        Some(("/Favorites".to_string(), r"\\SERVER".to_string()))
    );
    let off = Options {
        favorites_flags: 0,
        ..Options::default()
    };
    assert_eq!(favorites_parent(path, &server(), &off, &kept), None);
    assert_eq!(
        favorites_parent(path, &server(), &options, &Favorites::default()),
        None,
        "not kept"
    );
    let inside = r"/Favorites/\\SERVER";
    assert_eq!(favorites_parent(inside, &server(), &options, &kept), None);
    assert_eq!(favorites_parent("/", &server(), &options, &kept), None);
    let domain = NetResource {
        display: nc_plugin_sdk::system::network::DISPLAY_DOMAIN,
        remote_name: "WORKGROUP".to_string(),
        ..server()
    };
    let at_domain = "/Microsoft Windows Network/WORKGROUP";
    assert!(favorites_parent(at_domain, &domain, &options, &kept).is_some());
    let share = NetResource {
        display: nc_plugin_sdk::system::network::DISPLAY_SHARE,
        remote_name: r"\\SERVER\Music".to_string(),
        ..server()
    };
    kept.write(r"\\server\Music", "me", "pw");
    let at_share = r"/Microsoft Windows Network/WORKGROUP/\\SERVER/\\SERVER\Music";
    assert_eq!(favorites_parent(at_share, &share, &options, &kept), None);
}

/// The Favorites folder's path, whatever its case.
#[test]
fn the_folder_is_known_by_its_path() {
    assert_eq!(folder_path(), "/Favorites");
    assert!(in_folder("/favorites") && !in_folder(r"/Favorites/\\SERVER"));
}
