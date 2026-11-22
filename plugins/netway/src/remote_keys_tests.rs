use super::*;

#[test]
fn the_key_bar_is_netboxs() {
    let keys = keys();
    let label = |key: &str| {
        (keys.iter())
            .find(|each| each.key == key)
            .map(|each| each.label.clone())
    };
    assert_eq!(label("Shift+F5").as_deref(), Some("Duplicate"));
    assert_eq!(label("Shift+F6").as_deref(), Some("MoveTo"));
    assert_eq!(label("Alt+Shift+F6").as_deref(), Some("Rename"));
    assert_eq!(label("Shift+F1").as_deref(), Some(" "), "cleared");
    assert_eq!(label("Ctrl+A").as_deref(), Some(""), "the program's label");
}

#[test]
fn addresses_and_tokens_are_winscps() {
    assert_eq!(token("/a b/c"), "\"/a b/c\" ");
    assert_eq!(token("/abc"), "/abc ");
    assert_eq!(
        crate::remote::session_url("sftp", "user", "host", 22),
        "sftp://user@host"
    );
    assert_eq!(
        crate::remote::session_url("scp", "u", "::1", 2222),
        "scp://u@[::1]:2222"
    );
    assert_eq!(same(&[Some(1), Some(1)]), Some(1));
    assert_eq!(same(&[Some(1), Some(2)]), None);
    assert_eq!(same(&[None]), None);
}
