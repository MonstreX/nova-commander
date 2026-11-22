use super::*;

fn item(text: &str) -> ShellItem {
    ShellItem {
        id: 10,
        text: text.to_string(),
        verb: Some("open".to_string()),
        help: Some("Opens the item.".to_string()),
        ..ShellItem::default()
    }
}

fn options(extra: Extra, different_only: bool) -> Options {
    Options {
        extra,
        different_only,
        ..Options::default()
    }
}

#[test]
fn a_line_shows_the_item_and_what_is_configured_after_it() {
    let plain = options(Extra::Nothing, false);
    assert_eq!(line(&item("&Open"), &plain).text, "&Open");
    let verb = options(Extra::Verb, false);
    assert_eq!(line(&item("&Edit"), &verb).text, "&Edit (open)");
    let help = options(Extra::Help, false);
    assert_eq!(line(&item("&Open"), &help).text, "&Open (Opens the item)");
    let different = options(Extra::Verb, true);
    assert_eq!(
        line(&item("&Open"), &different).text,
        "&Open",
        "the same: hidden"
    );
    let separator = ShellItem {
        separator: true,
        ..ShellItem::default()
    };
    assert!(line(&separator, &plain).separator);
    let state = ShellItem {
        checked: true,
        disabled: true,
        ..item("x")
    };
    let shown = line(&state, &plain);
    assert!(shown.checked && shown.disabled);
}

#[test]
fn an_item_its_object_draws_shows_its_help_or_verb_in_braces() {
    let drawn = ShellItem {
        owner_drawn: true,
        ..item("")
    };
    assert_eq!(
        line(&drawn, &options(Extra::Nothing, false)).text,
        "{Opens the item}"
    );
    assert_eq!(
        line(&drawn, &options(Extra::Help, false)).text,
        "-- owner drawn -- (Opens the item)"
    );
    let bare = ShellItem {
        owner_drawn: true,
        ..ShellItem::default()
    };
    assert_eq!(
        line(&bare, &options(Extra::Nothing, false)).text,
        "-- owner drawn --"
    );
    assert_eq!(undoubled("AboutA&bout"), "A&bout");
    assert_eq!(undoubled("Abc"), "Abc");
    assert_eq!(undoubled("Open file"), "Open file");
}

#[test]
fn a_submenus_mark_stands_at_one_column() {
    let items = [
        item("&Open"),
        ShellItem {
            submenu: true,
            ..item("Se&nd to")
        },
        ShellItem {
            submenu: true,
            ..item("Open &with")
        },
    ];
    let lines = with_arrows(
        items.iter().map(|item| MenuItem::new(&item.text)).collect(),
        &items,
    );
    assert_eq!(lines[0].text, "&Open");
    assert_eq!(lines[1].text, format!("Se&nd to  {ARROW}"));
    assert_eq!(lines[2].text, format!("Open &with{ARROW}"));
}
