use super::*;

fn state(plugin: Option<&str>, folder: &str, active: bool) -> Option<PanelState> {
    Some(PanelState {
        plugin: plugin.map(str::to_string),
        folder: folder.to_string(),
        active,
        ..PanelState::default()
    })
}

/// The panel taken to its first folder: one of the plugin's at its root, the active one first; none
/// when no such panel is shown.
#[test]
fn the_panel_at_the_plugins_root_is_taken_there() {
    let both = [
        (Which::Active, state(Some(NAME), "/", true)),
        (Which::Passive, state(Some(NAME), "/", false)),
    ];
    assert_eq!(target_panel(&both), Some(Which::Active));
    let passive = [
        (Which::Active, state(None, r"E:\work", true)),
        (Which::Passive, state(Some(NAME), "/", false)),
    ];
    assert_eq!(target_panel(&passive), Some(Which::Passive));
    let moved = [
        (
            Which::Active,
            state(Some(NAME), "/Microsoft Windows Network", true),
        ),
        (Which::Passive, state(Some("proclist"), "/", false)),
    ];
    assert_eq!(
        target_panel(&moved),
        None,
        "already elsewhere, or another plugin's"
    );
    assert_eq!(target_panel(&[(Which::Active, None)]), None);
}

/// FAR's `SetCursorToShare`: the listing's name, whatever its case; the asked one when not listed.
#[test]
fn the_cursor_takes_the_listings_case() {
    let options = Options::default();
    let listed = vec![NetResource {
        remote_name: r"\\wsl.localhost\MyUbuntu".to_string(),
        ..NetResource::default()
    }];
    assert_eq!(
        cursor_name(&listed, &options, r"\\WSL.LOCALHOST\myubuntu"),
        r"\\wsl.localhost\MyUbuntu"
    );
    assert_eq!(
        cursor_name(&listed, &options, r"\\WSL.LOCALHOST\C$"),
        r"\\WSL.LOCALHOST\C$"
    );
}
