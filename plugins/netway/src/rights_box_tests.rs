use super::*;

fn values(items: &[DialogItem]) -> Vec<Value> {
    (items.iter())
        .map(|item| Value {
            text: item.text.clone(),
            state: item.state,
        })
        .collect()
}

#[test]
fn rights_known_unknown_and_applied() {
    let rights = Rights::of(0o755);
    assert!(!rights.is_undef());
    assert_eq!(rights.applied(0o4000), 0o755, "every bit known: as given");
    let common = Rights::common(&[0o644, 0o664]);
    assert!(common.is_undef());
    assert_eq!(common.state(0o020), None, "the group's W differs");
    assert_eq!(common.state(0o400), Some(true));
    assert_eq!(common.state(0o001), Some(false));
    assert_eq!(
        common.applied(0o624),
        0o624 & !0o3 | 0o644,
        "the left bit kept"
    );
}

#[test]
fn the_box_lays_out_netboxs_rows_and_reads_back() {
    let full = RightsBox {
        base: 2,
        specials: true,
        buttons: true,
        add_x: true,
        three_state: false,
    };
    let items = full.items(Rights::of(0o4750), true, true);
    assert_eq!(items.len(), full.end() - full.base);
    assert_eq!(items[full.check(0, 0) - 2].text, "R");
    assert_eq!(items[full.check(0, 3) - 2].text, "Set UID   ");
    assert_eq!(items[full.check(2, 3) - 2].text, "Sticky bit");
    assert_eq!(items[full.octal() - 2].text, "4750");
    assert_eq!(items[full.button(2).unwrap() - 2].text, "A&ll");
    assert_eq!(
        items[full.add_x().unwrap() - 2].text,
        "Add &X to directories"
    );
    let mut read = vec![Value::default(); 2];
    read.extend(values(&items));
    let got = full.rights_of(&|item| read.get(item).cloned().unwrap_or_default());
    assert_eq!(got, Rights::of(0o4750));
    let plain = RightsBox {
        base: 0,
        specials: false,
        buttons: false,
        add_x: false,
        three_state: true,
    };
    let items = plain.items(Rights::common(&[0o644, 0o600]), false, true);
    assert_eq!(items[plain.octal()].text, "", "left rights: no octal");
    assert_eq!(items[plain.check(1, 0)].state, 2, "[?]");
    let got = plain.rights_of(&|item| values(&items).get(item).cloned().unwrap_or_default());
    assert_eq!(got.state(0o040), None);
    assert_eq!(got.state(0o4000), Some(false), "no specials shown: unset");
}
