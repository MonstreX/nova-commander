use super::*;
use nc_plugin_sdk::ItemKind;

fn sample() -> Session {
    Session {
        protocol: Protocol::Ftp,
        encryption: Encryption::Explicit,
        host: "ftp.example.com".to_string(),
        port: 21,
        user: "me".to_string(),
        password: "pw".to_string(),
        private_key: r"C:\k".to_string(),
        other: vec![("Later".to_string(), "kept".to_string())],
        ..Session::default()
    }
}

/// The values the dialog closes with when nothing was changed.
fn values_of(items: &[DialogItem]) -> Vec<Value> {
    items
        .iter()
        .map(|item| Value {
            state: item.state,
            text: item.text.clone(),
        })
        .collect()
}

/// The protocol's lines stand where the dialog's constants say.
#[test]
fn the_protocols_items_are_where_the_dialog_reads_them() {
    let items = items(&sample());
    // The tab bar, the tabs' items, the buttons' line and the three buttons.
    assert_eq!(items.len(), session_tabs::BUTTONS + 4);
    assert_eq!(
        items[TAB_BAR].text, "[Session] Environment  Directories  FTP(S)  Connection ►",
        "NetBox's FTP(S) tab before Connection"
    );
    assert_eq!(items[PROTOCOL].kind, ItemKind::List);
    assert_eq!(items[PROTOCOL].lines[4], "Amazon S3");
    assert_eq!(items[FALLBACK].kind, ItemKind::Check);
    assert_eq!(items[INSECURE].text, "Insecure");
    assert_eq!(items[ENCRYPTION_LABEL].text, "&Encryption:");
    assert_eq!(items[ENCRYPTION].lines.len(), 3);
}

/// The login's lines and the buttons stand where the dialog's constants say.
#[test]
fn the_logins_items_are_where_the_dialog_reads_them() {
    let items = items(&sample());
    assert_eq!(items[HOST].text, "ftp.example.com");
    assert_eq!(
        (items[PORT].kind, items[PORT].text.as_str()),
        (ItemKind::Fixed, "21")
    );
    assert_eq!(items[USER_LABEL].text, "&User name:");
    assert_eq!(items[USER].text, "me");
    assert_eq!(items[PASSWORD].kind, ItemKind::Password);
    assert_eq!(items[KEY_LABEL].text, "Private &key file:");
    assert_eq!(items[KEY].text, r"C:\k");
    assert_eq!(items[OK].text, "OK");
    assert!(!items[CONNECT].disabled, "Connect connects");
    assert_eq!(CONNECT, OK + 1);
}

/// Unchanged, the dialog gives the session back, keys of later tabs kept.
#[test]
fn an_unchanged_dialog_gives_the_session_back() {
    let session = sample();
    let values = values_of(&items(&session));
    assert_eq!(session_of(&values, &session), session);
}

/// What was typed is the session's: the host and the key trimmed, the port as a number (not one: the
/// session's), no encryption where the protocol shows none.
#[test]
fn what_was_typed_is_the_session() {
    let session = sample();
    let mut values = values_of(&items(&session));
    values[PROTOCOL].state = 0;
    values[HOST].text = "  sftp.example.com ".to_string();
    values[PORT].text = "  2222".to_string();
    values[FALLBACK].state = 1;
    let typed = session_of(&values, &session);
    assert_eq!(typed.protocol, Protocol::Sftp);
    assert_eq!(typed.encryption, Encryption::None, "SFTP shows none");
    assert_eq!(typed.host, "sftp.example.com");
    assert_eq!(typed.port, 2222);
    assert!(typed.scp_fallback);
    values[PORT].text = "x".to_string();
    assert_eq!(session_of(&values, &session).port, 21);
}

/// NetBox's `UpdateControls`: SFTP's fallback, plain FTP's and WebDAV's "Insecure", the encryption off
/// SSH, the key for SSH and the encrypted ones, S3's access keys.
#[test]
fn the_lines_shown_follow_the_protocol() {
    let sftp = shown(Protocol::Sftp, Encryption::None);
    assert!(sftp.fallback && !sftp.insecure && !sftp.encryption && sftp.key);
    let scp = shown(Protocol::Scp, Encryption::None);
    assert!(!scp.fallback && scp.key);
    let ftp = shown(Protocol::Ftp, Encryption::None);
    assert!(ftp.insecure && ftp.encryption && !ftp.key);
    let ftps = shown(Protocol::Ftp, Encryption::Implicit);
    assert!(!ftps.insecure && ftps.key);
    let https = shown(Protocol::WebDav, Encryption::Implicit);
    assert!(!https.insecure && https.key);
    let s3 = shown(Protocol::S3, Encryption::Implicit);
    assert!(s3.access_keys && s3.encryption && !s3.insecure && !s3.key);
    assert_eq!(encryption_lines(Protocol::S3).len(), 2);
    assert_eq!(user_label(true), "&Access key ID:");
    assert_eq!(password_label(true), "&Secret access key:");
}

/// An S3 session opens with its two encryptions, its labels the access keys'.
#[test]
fn an_s3_session_shows_its_access_keys() {
    let session = Session {
        protocol: Protocol::S3,
        encryption: Encryption::Implicit,
        ..Session::default()
    };
    let items = items(&session);
    assert_eq!(items[ENCRYPTION].lines.len(), 2);
    assert_eq!(items[ENCRYPTION].state, 1);
    assert_eq!(items[USER_LABEL].text, "&Access key ID:");
    assert!(items[KEY].disabled);
}

/// NetBox's question before a password is stored: on OK, and on Connect of an edited session only (a new
/// one connects without being saved).
#[test]
fn the_password_is_asked_about_where_it_is_saved() {
    assert!(asks_password(OK, Action::Add) && asks_password(OK, Action::Edit));
    assert!(asks_password(CONNECT, Action::Edit));
    assert!(!asks_password(CONNECT, Action::Add));
    assert!(!asks_password(CONNECT + 1, Action::Edit), "Cancel");
}

/// The other tabs' settings typed are the session's, kept among its other keys.
#[test]
fn the_other_tabs_settings_are_the_sessions() {
    let mut session = sample();
    session.protocol = Protocol::Sftp;
    session.encryption = Encryption::None;
    let mut options = session.options();
    options.remote_directory = "/var/www".to_string();
    options.shell = "/bin/bash".to_string();
    session.set_options(&options);
    let values = values_of(&items(&session));
    let read = session_of(&values, &session);
    assert_eq!(read, session);
    assert_eq!(read.options().remote_directory, "/var/www");
    assert!(
        read.other
            .contains(&("Later".to_string(), "kept".to_string()))
    );
}

/// Ctrl+PgDn / Ctrl+PgUp step round the tabs the protocol has; a click on the bar shows its tab.
#[test]
fn keys_and_clicks_ask_for_tabs() {
    let key = |key: &str| DialogEvent::Key {
        item: None,
        key: key.to_string(),
    };
    assert_eq!(
        tab_asked(&key("Ctrl+PgDn"), (Protocol::Scp, false), 2),
        Some(4)
    );
    assert_eq!(
        tab_asked(&key("Ctrl+PgDn"), (Protocol::Scp, false), 4),
        Some(5)
    );
    assert_eq!(
        tab_asked(&key("Ctrl+PgDn"), (Protocol::Scp, false), 7),
        Some(8)
    );
    assert_eq!(
        tab_asked(&key("Ctrl+PgDn"), (Protocol::Scp, false), 10),
        Some(0)
    );
    assert_eq!(
        tab_asked(&key("Ctrl+PgUp"), (Protocol::Sftp, false), 0),
        Some(10)
    );
    assert_eq!(tab_asked(&key("PgDn"), (Protocol::Sftp, false), 0), None);
    let click = |x: i32, y: i32| DialogEvent::Mouse {
        item: None,
        key: "MsLClick".to_string(),
        x,
        y,
    };
    // "[Session] Environment  Directories  SFTP  SCP/Shell ►" from column 5.
    assert_eq!(
        tab_asked(&click(LEFT + 12, 2), (Protocol::Sftp, false), 0),
        Some(1)
    );
    assert_eq!(
        tab_asked(&click(LEFT + 39, 2), (Protocol::Sftp, false), 0),
        Some(3)
    );
    assert_eq!(
        tab_asked(&click(LEFT + 12, 3), (Protocol::Sftp, false), 0),
        None
    );
}
