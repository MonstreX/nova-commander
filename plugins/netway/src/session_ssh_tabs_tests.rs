use super::*;
use nc_plugin_sdk::ItemKind;

fn values_of(items: &[DialogItem]) -> Vec<Value> {
    let mut values = vec![Value::default(); FIRST[0]];
    values.extend(items.iter().map(|item| Value {
        text: item.text.clone(),
        state: item.state,
    }));
    values
}

#[test]
fn the_items_are_where_the_dialog_reads_them() {
    let items = items(&Ssh::default());
    assert_eq!(items.len(), FIRST[3] - FIRST[0]);
    let item = |index: usize| &items[index - FIRST[0]];
    assert_eq!(item(COMPRESSION).kind, ItemKind::Check);
    assert_eq!(item(CIPHER_LIST).kind, ItemKind::ListBox);
    assert_eq!(item(CIPHER_LIST).lines[4], "-- warn below here --");
    assert_eq!(item(CIPHER_UP).text, "&Up");
    assert_eq!(item(KEX_LIST).lines[3], "ECDH key exchange");
    assert_eq!(item(KEX_DOWN).text, "&Down");
    assert_eq!(item(REKEY_TIME).text, "60");
    assert_eq!(item(REKEY_DATA).text, "1G");
    assert_eq!(item(KEYBOARD_PASSWORD).kind, ItemKind::Check);
    assert!(items.iter().all(|item| item.hidden));
}

#[test]
fn the_settings_come_back_with_the_lists_order() {
    let changed = Ssh {
        compression: true,
        ciphers: crate::ssh_options::algo_list("chacha20", &CIPHERS),
        rekey_time: 0,
        rekey_data: "500M".to_string(),
        no_user_auth: true,
        try_agent: false,
        keyboard_interactive: false,
        ki_password: false,
        ..Ssh::default()
    };
    for ssh in [Ssh::default(), changed] {
        let values = values_of(&items(&ssh));
        assert_eq!(ssh_of(&values, &Ssh::default(), &Order::of(&ssh)), ssh);
    }
    let mut values = values_of(&items(&Ssh::default()));
    values[REKEY_DATA].text = "lots".to_string();
    let read = ssh_of(&values, &Ssh::default(), &Order::of(&Ssh::default()));
    assert_eq!(
        read.rekey_data, "1G",
        "a size not understood keeps the old one"
    );
}

#[test]
fn lines_step_up_and_down_within_the_list() {
    assert_eq!(step(0, 3, true), None);
    assert_eq!(step(2, 3, true), Some(1));
    assert_eq!(step(2, 3, false), None);
    assert_eq!(step(0, 3, false), Some(1));
}
