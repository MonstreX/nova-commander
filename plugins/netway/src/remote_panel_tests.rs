use super::*;

fn state(plugin: Option<&str>, folder: &str, active: bool) -> Option<PanelState> {
    Some(PanelState {
        plugin: plugin.map(str::to_string),
        folder: folder.to_string(),
        active,
        ..PanelState::default()
    })
}

/// The plugin's panel at its root, the active one first; a disk's panel, another plugin's or one
/// elsewhere is none.
#[test]
fn the_start_goes_to_the_plugins_panel_at_its_root() {
    let both = [
        (Which::Active, state(Some("NetWay"), "/", true)),
        (Which::Passive, state(Some("netway"), "/", false)),
    ];
    assert_eq!(target_panel(&both), Some(Which::Active));
    let passive = [
        (Which::Active, state(None, r"C:\", true)),
        (Which::Passive, state(Some("netway"), "/", false)),
    ];
    assert_eq!(target_panel(&passive), Some(Which::Passive));
    let none = [
        (Which::Active, state(Some("netway"), "/home", true)),
        (Which::Passive, state(Some("archive"), "/", false)),
    ];
    assert_eq!(target_panel(&none), None);
    assert_eq!(target_panel(&[(Which::Active, None)]), None);
}
