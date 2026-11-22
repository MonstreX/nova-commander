use super::*;

fn stored() -> Vec<String> {
    ["Work/Prod/db", "Work/web", "root@files.example.com"]
        .map(str::to_string)
        .to_vec()
}

fn connect(prefix: &str, command: &str) -> Address {
    match parse(prefix, command, &stored()) {
        Target::Connect(address) => address,
        Target::Sessions => panic!("{prefix}:{command} named no session"),
    }
}

#[test]
fn nothing_after_a_prefix_is_the_sessions_panel() {
    for (prefix, command) in [
        ("netway", ""),
        ("netway", " // "),
        ("sftp", "//"),
        ("s3", ""),
    ] {
        assert_eq!(
            parse(prefix, command, &stored()),
            Target::Sessions,
            "{prefix}:{command}"
        );
    }
    // No host: nothing to connect to.
    assert_eq!(parse("sftp", "//user@:22/", &stored()), Target::Sessions);
}

#[test]
fn a_stored_session_by_its_name_or_its_whole_name() {
    let by_name = connect("netway", "DB");
    assert_eq!(by_name.stored.as_deref(), Some("Work/Prod/db"));
    assert_eq!((by_name.folder.as_str(), by_name.file.as_str()), ("", ""));
    let whole = connect("netway", "work/prod/db/var/log/");
    assert_eq!(whole.stored.as_deref(), Some("Work/Prod/db"));
    assert_eq!(whole.folder, "/var/log/");
    let file = connect("netway", "//web/etc/hosts");
    assert_eq!(file.stored.as_deref(), Some("Work/web"));
    assert_eq!(
        (file.folder.as_str(), file.file.as_str()),
        ("/etc/", "hosts")
    );
    // Ctrl+F's name in quotes, a name decoded as an address.
    assert_eq!(
        connect("netway", "\"Work/web\"").stored.as_deref(),
        Some("Work/web")
    );
    let decoded = connect("netway", "root%40files.example.com");
    assert_eq!(decoded.stored.as_deref(), Some("root@files.example.com"));
}

#[test]
fn a_name_is_matched_whole_not_by_its_start() {
    // WinSCP takes "dbserver" for the session "db"; a host's name is not a session's.
    let address = connect("netway", "dbserver");
    assert_eq!(address.stored, None);
    assert_eq!(address.session.host, "dbserver");
}

#[test]
fn a_protocol_given_with_a_stored_session_is_its_protocol() {
    let address = connect("scp", "//web");
    assert_eq!(address.stored.as_deref(), Some("Work/web"));
    assert_eq!(address.session.protocol, Protocol::Scp);
}

#[test]
fn an_address_is_a_session_of_its_own() {
    let address = connect("sftp", "//me:se%20cret@example.com:2222/home/me/");
    assert_eq!(address.stored, None);
    let session = &address.session;
    assert_eq!(session.protocol, Protocol::Sftp);
    assert_eq!(session.host, "example.com");
    assert_eq!(session.port, 2222);
    assert_eq!(
        (session.user.as_str(), session.password.as_str()),
        ("me", "se cret")
    );
    assert_eq!(
        (address.folder.as_str(), address.file.as_str()),
        ("/home/me/", "")
    );
}

#[test]
fn a_host_alone_is_sftp_on_its_port() {
    let address = connect("netway", "example.com");
    assert_eq!(address.session.protocol, Protocol::Sftp);
    assert_eq!(address.session.port, SSH_PORT);
    assert_eq!(address.session.user, "");
}

#[test]
fn each_protocol_comes_with_its_encryption_and_port() {
    let cases = [
        (
            "netway",
            "scp://h",
            Protocol::Scp,
            Encryption::None,
            SSH_PORT,
        ),
        (
            "netway",
            "ssh://h",
            Protocol::Sftp,
            Encryption::None,
            SSH_PORT,
        ),
        ("ftp", "//h", Protocol::Ftp, Encryption::None, FTP_PORT),
        (
            "ftps",
            "//h",
            Protocol::Ftp,
            Encryption::Implicit,
            FTPS_IMPLICIT_PORT,
        ),
        (
            "netway",
            "ftpes://h",
            Protocol::Ftp,
            Encryption::Explicit,
            FTP_PORT,
        ),
        (
            "webdav",
            "//h",
            Protocol::WebDav,
            Encryption::None,
            HTTP_PORT,
        ),
        (
            "netway",
            "davs://h",
            Protocol::WebDav,
            Encryption::Implicit,
            HTTPS_PORT,
        ),
        (
            "netway",
            "https://h",
            Protocol::WebDav,
            Encryption::Implicit,
            HTTPS_PORT,
        ),
        ("s3", "//h", Protocol::S3, Encryption::Implicit, HTTPS_PORT),
        (
            "netway",
            "s3plain://h",
            Protocol::S3,
            Encryption::None,
            HTTP_PORT,
        ),
    ];
    for (prefix, command, protocol, encryption, port) in cases {
        let session = connect(prefix, command).session;
        let got = (session.protocol, session.encryption, session.port);
        assert_eq!(got, (protocol, encryption, port), "{prefix}:{command}");
    }
}

#[test]
fn webdav_to_an_s3_host_is_s3() {
    let session = connect("netway", "https://bucket.s3.amazonaws.com").session;
    assert_eq!((session.protocol, session.port), (Protocol::S3, HTTPS_PORT));
    let other = connect("netway", "https://example.com").session;
    assert_eq!(other.protocol, Protocol::WebDav);
    // A port given keeps WebDAV, as WinSCP's.
    let ported = connect("netway", "https://storage.googleapis.com:8443").session;
    assert_eq!((ported.protocol, ported.port), (Protocol::WebDav, 8443));
}

#[test]
fn ports_and_hosts_in_brackets() {
    let v6 = connect("sftp", "//[fe80::1]:2200/").session;
    assert_eq!((v6.host.as_str(), v6.port), ("fe80::1", 2200));
    let bad = connect("sftp", "//h:99999").session;
    assert_eq!(bad.port, SSH_PORT);
    let zero = connect("sftp", "//h:0").session;
    assert_eq!(zero.port, SSH_PORT);
}

#[test]
fn parameters_after_semicolons_are_passed_by() {
    let address = connect("sftp", "//me;fingerprint=ssh-rsa-xx@h/tmp/;save");
    assert_eq!(address.session.user, "me");
    assert_eq!(address.folder, "/tmp/");
}

#[test]
fn decoding_as_winscp() {
    assert_eq!(decode("a+b%20c"), "a b c");
    assert_eq!(decode("%D0%BF%D1%80"), "пр");
    assert_eq!(decode("100%"), "100%");
    assert_eq!(decode("%zz%4"), "%zz%4");
}

#[test]
fn the_prefixes_are_netboxs_with_the_plugins_name() {
    assert_eq!(PREFIXES[0], "netway");
    assert!(PREFIXES.contains(&"sftp") && PREFIXES.contains(&"s3"));
}
