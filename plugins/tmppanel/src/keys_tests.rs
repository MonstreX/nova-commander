//! The temporary panel's keys as the options name them, and the answers that need no host: Goto and
//! safe mode's Ctrl+PgUp on an entry there, gone or "..".

use super::*;

fn named(options: &Options) -> Vec<String> {
    keys(options).into_iter().map(|key| key.key).collect()
}

fn labelled(options: &Options) -> Vec<(String, String)> {
    (keys(options).into_iter())
        .filter(|key| !key.label.is_empty())
        .map(|key| (key.key, key.label))
        .collect()
}

#[test]
fn fars_labels_and_switch_only_with_the_common_panel() {
    let common = Options::default();
    let own = |key: &str, label: &str| (key.to_string(), label.to_string());
    assert_eq!(
        labelled(&common),
        [
            own("F7", "Remove"),
            own("Alt+Shift+F2", "SavLst"),
            own("Alt+Shift+F3", "Goto"),
            own("Alt+Shift+F12", "Switch"),
        ]
    );
    assert!(named(&common).contains(&"Alt+Shift+9".to_string()));
    let alone = Options {
        common_panel: false,
        ..Options::default()
    };
    assert!(
        !labelled(&alone)
            .iter()
            .any(|(key, _)| key == "Alt+Shift+F12")
    );
    assert!(
        !named(&alone)
            .iter()
            .any(|key| key.starts_with("Alt+Shift+0"))
    );
}

#[test]
fn f3_to_f8_but_f7_are_checked_with_fars_modifiers() {
    let keys = named(&Options::default());
    for key in [
        "F3",
        "Shift+F4",
        "Alt+F5",
        "Alt+Shift+F6",
        "F8",
        "Shift+F8",
        "Ctrl+Shift+F3",
        "Ctrl+Alt+F5",
        "Ctrl+Alt+Shift+F6",
    ] {
        assert!(keys.contains(&key.to_string()), "{key}");
    }
    assert_eq!(keys.len(), 14 + 5 * 7 - 1, "Alt+Shift+F3 is Goto's");
    for key in ["Shift+F7", "Ctrl+F3", "Enter", "Ctrl+PgUp", "Shift+Del"] {
        assert!(!keys.contains(&key.to_string()), "{key}");
    }
    assert_eq!(
        keys.iter().filter(|key| *key == "Alt+Shift+F3").count(),
        1,
        "Goto, once"
    );
}

#[test]
fn enter_and_ctrl_pgup_with_their_options() {
    let any = Options {
        any_in_panel: true,
        ..Options::default()
    };
    assert!(named(&any).contains(&"Enter".to_string()));
    assert!(!named(&any).contains(&"Ctrl+PgUp".to_string()));
    let safe = Options {
        safe_mode_panel: true,
        ..Options::default()
    };
    assert!(named(&safe).contains(&"Enter".to_string()));
    assert!(named(&safe).contains(&"Ctrl+PgUp".to_string()));
    for key in ["Del", "Shift+Del", "Alt+Del"] {
        assert!(named(&safe).contains(&key.to_string()), "{key}");
    }
}

fn press(current: Option<&Path>) -> KeyPress {
    KeyPress {
        current: current.map(|path| path.to_string_lossy().into_owned()),
        ..KeyPress::default()
    }
}

#[test]
fn goto_and_ctrl_pgup_go_to_an_entry_there_only() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("a.txt");
    std::fs::write(&file, "a").unwrap();
    let gone = temp.path().join("gone.txt");
    assert_eq!(
        goto(&press(Some(&file))),
        KeyAnswer::go(KeyGo::PassiveFile(file.clone()))
    );
    assert_eq!(
        goto(&press(Some(temp.path()))),
        KeyAnswer::go(KeyGo::PassiveFolder(temp.path().to_path_buf()))
    );
    assert_eq!(goto(&press(Some(&gone))), KeyAnswer::default());
    assert_eq!(
        ctrl_pgup(&press(Some(&file))),
        KeyAnswer::go(KeyGo::ActiveFile(file))
    );
    assert_eq!(ctrl_pgup(&press(Some(&gone))), KeyAnswer::pass());
    assert_eq!(ctrl_pgup(&press(None)), KeyAnswer::pass(), "\"..\"");
}
