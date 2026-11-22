use nc_plugin_sdk::Value;

use super::*;

/// The dialog's values as "Ok" closes it with the items as given.
fn closed(dialog: &Dialog, button: usize) -> Closed {
    let values = (dialog.items.iter())
        .map(|item| Value {
            state: item.state,
            ..Value::default()
        })
        .collect();
    Closed {
        button: Some(button),
        values,
    }
}

/// FAR's defaults: both menus, ".." in the providers' list, the local network first, full share names,
/// hidden shares shown as hidden, browsing up to the favorites.
#[test]
fn the_defaults_are_fars() {
    let options = Options::default();
    assert!(options.add_to_disks && options.add_to_plugins && options.root_double_point);
    assert!(options.local_network && options.scan_network && options.full_path_shares);
    assert!(!options.show_printers && !options.navigate_to_domains);
    assert_eq!(options.hidden_shares, HiddenShares::AsHidden);
    assert_eq!(options.favorites_flags, UPBROWSE_TO_FAVORITES);
}

/// FAR's `Config` in its order: "Ok" gives back what the items say, "Cancel" nothing.
#[test]
fn the_dialog_gives_back_its_items() {
    let options = Options {
        show_printers: true,
        hidden_shares: HiddenShares::Always,
        favorites_flags: 0,
        ..Options::default()
    };
    let dialog = options.dialog();
    assert_eq!(dialog.items[OK].text, "&Ok");
    assert_eq!(dialog.items[NEVER].text, "&Never show");
    assert_eq!(dialog.items[8].text, "Hidden Shares", "the box's title");
    let back = options.from_dialog(&closed(&dialog, OK)).unwrap();
    assert_eq!(back, options);
    assert_eq!(options.from_dialog(&closed(&dialog, OK + 1)), None);
    let hidden = Options::default();
    let back = hidden.from_dialog(&closed(&hidden.dialog(), OK)).unwrap();
    assert_eq!(back.hidden_shares, HiddenShares::AsHidden);
    assert_eq!(back.favorites_flags, UPBROWSE_TO_FAVORITES);
}
