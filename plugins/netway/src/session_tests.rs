use super::*;

fn sample() -> Session {
    Session {
        protocol: Protocol::Ftp,
        scp_fallback: true,
        encryption: Encryption::Explicit,
        host: "files.example.com".to_string(),
        port: 2121,
        user: "me".to_string(),
        password: "not written by text".to_string(),
        private_key: r"C:\keys\id_ed25519".to_string(),
        other: vec![("RemoteDirectory".to_string(), "/srv".to_string())],
    }
}

/// A session's file read back is the session; the password only as the store gave its kept text.
#[test]
fn a_sessions_text_reads_back_and_keeps_unknown_keys() {
    let session = sample();
    let text = session.text(Some("0a0b"));
    assert!(!text.contains("not written"), "{text}");
    assert!(text.ends_with("RemoteDirectory=/srv\n"), "{text}");
    let (read, kept) = Session::parse(&text);
    assert_eq!(kept.as_deref(), Some("0a0b"));
    assert_eq!(
        read,
        Session {
            password: String::new(),
            ..session
        }
    );
    let (_, none) = Session::parse(&sample().text(None));
    assert_eq!(none, None, "no password kept");
}

/// Comments, lines without `=`, a byte order mark and values not understood leave the defaults.
#[test]
fn what_is_not_understood_leaves_the_defaults() {
    let text = "\u{feff}; NetWay\n# note\nnonsense\nProtocol=gopher\nPort=99999\nEncryption=x\nHost= a b \n";
    let (read, kept) = Session::parse(text);
    assert_eq!(kept, None);
    assert_eq!(read.protocol, Protocol::Sftp);
    assert_eq!(read.port, SSH_PORT);
    assert_eq!(read.encryption, Encryption::None);
    assert_eq!(read.host, " a b ", "a value as it is");
    let (scp, _) = Session::parse("Protocol=SCP\nScpFallback=1\n");
    assert_eq!((scp.protocol, scp.scp_fallback), (Protocol::Scp, true));
}

#[test]
fn the_default_name_is_user_at_host_or_the_host() {
    let mut session = Session {
        host: " example.com ".to_string(),
        ..Session::default()
    };
    assert_eq!(session.default_name(), "example.com");
    session.user = "root".to_string();
    assert_eq!(session.default_name(), "root@example.com");
}

/// NetBox's port switch: another protocol's default port follows the protocol, a port of one's own
/// stays.
#[test]
fn the_port_follows_the_protocol_as_netboxs() {
    let port = |protocol, encryption, port| Session::port_for(protocol, encryption, port);
    assert_eq!(port(Protocol::Ftp, Encryption::None, 22), 21);
    assert_eq!(port(Protocol::Ftp, Encryption::Implicit, 21), 990);
    assert_eq!(port(Protocol::Ftp, Encryption::Explicit, 990), 21);
    assert_eq!(port(Protocol::Sftp, Encryption::None, 21), 22);
    assert_eq!(
        port(Protocol::Scp, Encryption::None, 990),
        990,
        "not SSH's switch"
    );
    assert_eq!(port(Protocol::WebDav, Encryption::None, 443), 80);
    assert_eq!(port(Protocol::WebDav, Encryption::Implicit, 80), 443);
    assert_eq!(port(Protocol::S3, Encryption::None, 22), 443);
    assert_eq!(
        port(Protocol::Ftp, Encryption::None, 2222),
        2222,
        "one's own"
    );
}

#[test]
fn protocols_and_encryptions_by_their_keys() {
    for protocol in Protocol::ALL {
        assert_eq!(Protocol::from_key(protocol.key()), Some(protocol));
    }
    assert_eq!(Protocol::from_key("WEBDAV"), Some(Protocol::WebDav));
    assert!(Protocol::Scp.is_ssh() && !Protocol::Ftp.is_ssh());
    for encryption in Encryption::ALL {
        assert_eq!(Encryption::from_key(encryption.key()), Some(encryption));
    }
}

/// A name is a file's name: none of Windows' restricted characters, no empty part, no `..`.
#[test]
fn names_with_restricted_symbols_are_refused() {
    assert_eq!(name_error("prod"), None);
    assert_eq!(name_error("root@host.com"), None);
    for bad in ["a:b", "a\\b", "a/b", "a?", "\"a\"", "dot.", "..", "tab\t"] {
        assert!(name_error(bad).is_some(), "{bad}");
    }
    assert_eq!(
        name_error(" ").as_deref(),
        Some("The session name can not be empty")
    );
    assert_eq!(path_error("Work/Prod/db"), None);
    assert!(path_error("Work//db").is_some(), "an empty folder");
    assert!(path_error("/db").is_some());
}

/// A session's file names a protocol it knows; any other text is no session's.
#[test]
fn a_session_file_is_told_by_its_protocol() {
    let written = Session::default().text(None);
    assert!(Session::is_session_text(&written));
    assert!(Session::is_session_text(
        "\u{feff}Host=h\n Protocol = WebDAV \n"
    ));
    for other in [
        "",
        "Host=h\nPort=22\n",
        "Protocol=gopher\n",
        "<xml/>",
        "protocol=sftp",
    ] {
        assert!(!Session::is_session_text(other), "{other:?}");
    }
}
