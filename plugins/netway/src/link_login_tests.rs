//! Sessions through the tests' proxies and through an SSH tunnel on the tests' server.

use super::*;
use crate::link_options::{Family, Link, ProxyMethod};
use crate::test_proxy::{Kind, TestProxy};

/// The plan of a session at the server's `port` with the password and `link`.
fn linked(port: u16, known: &Path, link: Link) -> Plan {
    let mut plan = plan(port, known);
    plan.password = PASSWORD.to_string();
    plan.options.link = link;
    plan
}

/// A link through the proxy of `method` at `port` (the loopback proxied too), with `login`.
fn through(method: ProxyMethod, port: u16, (user, password): (&str, &str)) -> Link {
    Link {
        proxy: method,
        proxy_host: "127.0.0.1".to_string(),
        proxy_port: port,
        proxy_user: user.to_string(),
        proxy_password: password.to_string(),
        proxy_localhost: true,
        ..Link::default()
    }
}

/// Each proxy carries the session: SOCKS4, SOCKS5 and HTTP with their logins, Telnet's command.
#[test]
fn the_proxies_carry_the_session() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let known = temp.path().join("known_hosts");
    let cases = [
        (Kind::Socks4, ProxyMethod::Socks4, None),
        (Kind::Socks5, ProxyMethod::Socks5, Some(("pu", "pp"))),
        (Kind::Http, ProxyMethod::Http, Some(("pu", "pp"))),
        (Kind::Telnet, ProxyMethod::Telnet, None),
    ];
    for (kind, method, login) in cases {
        let proxy = TestProxy::start(kind, login);
        let link = through(method, proxy.port, login.unwrap_or(("", "")));
        let (done, _) = started(&linked(server.port, &known, link), &[]);
        assert!(done.is_ok(), "{method:?}: {done:?}");
        assert_eq!(proxy.carried(), 1, "{method:?}");
    }
}

/// A wrong proxy password is WinSCP's "Proxy authentication failed."; the loopback goes straight unless
/// asked to be proxied.
#[test]
fn a_refused_proxy_login_fails_and_the_loopback_goes_straight() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let known = temp.path().join("known_hosts");
    let proxy = TestProxy::start(Kind::Socks5, Some(("pu", "pp")));
    let wrong = through(ProxyMethod::Socks5, proxy.port, ("pu", "wrong"));
    let (done, _) = started(&linked(server.port, &known, wrong), &[]);
    let failure = done.map(|started| started.folder).unwrap_err();
    assert!(
        matches!(&failure, Failure::Connection(text) if text == crate::proxy::AUTHENTICATION_FAILED),
        "{failure:?}"
    );
    let straight = Link {
        proxy_localhost: false,
        ..through(ProxyMethod::Socks5, proxy.port, ("pu", "pp"))
    };
    assert!(
        started(&linked(server.port, &known, straight), &[])
            .0
            .is_ok()
    );
    assert_eq!(proxy.carried(), 0);
}

/// "Connect through SSH tunnel": "Opening tunnel..." then the session through the tunnel host's channel.
#[test]
fn the_session_runs_through_its_tunnel() {
    let temp = tempfile::tempdir().unwrap();
    let target = TestServer::start(temp.path());
    let jump = TestServer::start(temp.path());
    let known = temp.path().join("known_hosts");
    let link = Link {
        tunnel: true,
        tunnel_host: "127.0.0.1".to_string(),
        tunnel_port: jump.port,
        tunnel_user: USER.to_string(),
        tunnel_password: PASSWORD.to_string(),
        ..Link::default()
    };
    let (done, seen) = started(&linked(target.port, &known, link), &[]);
    let started = done.unwrap();
    assert_eq!(started.fs.protocol(), "SFTP");
    let statuses: Vec<&Question> = (seen.iter())
        .filter(|question| matches!(question, Question::Status(_)))
        .collect();
    assert!(
        matches!(statuses.first(), Some(Question::Status(text)) if text == OPENING_TUNNEL),
        "{statuses:?}"
    );
}

/// The IP version asked for: the loopback's IPv4 address has no IPv6 one.
#[test]
fn the_ip_version_is_kept_to() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let runtime = Runtime::new().unwrap();
    let v4 = runtime.wait(route::direct("127.0.0.1", server.port, Family::V4), &|| {
        false
    });
    assert!(v4.unwrap().is_ok());
    let v6 = runtime.wait(route::direct("127.0.0.1", server.port, Family::V6), &|| {
        false
    });
    assert!(v6.unwrap().is_err());
}

/// The SSH and KEX tabs' choices are what `russh` negotiates with: compression on, ChaCha20 and
/// Diffie-Hellman group 14 first, short rekey limits.
#[test]
fn the_ssh_and_kex_choices_connect() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let mut plan = linked(
        server.port,
        &temp.path().join("known_hosts"),
        Link::default(),
    );
    let ssh = &mut plan.options.ssh;
    ssh.compression = true;
    ssh.ciphers = crate::ssh_options::algo_list("chacha20", &crate::ssh_options::CIPHERS);
    ssh.kexes = crate::ssh_options::algo_list("dh-group14-sha1", &crate::ssh_options::KEXES);
    ssh.rekey_time = 1;
    ssh.rekey_data = "1M".to_string();
    assert!(started(&plan, &[]).0.is_ok());
}

/// Keyboard-interactive off: the server's question never comes, the login is refused; "Respond with
/// password to the first prompt" off: the question is shown and the typed answer given.
#[test]
fn keyboard_interactive_follows_the_authentication_tab() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let mut plan = linked(
        server.port,
        &temp.path().join("known_hosts"),
        Link::default(),
    );
    plan.user = QUESTIONED_USER.to_string();
    plan.options.ssh.keyboard_interactive = false;
    let (done, _) = started(&plan, &[]);
    assert!(matches!(
        done.map(|_| ()),
        Err(Failure::Refused | Failure::Cancelled)
    ));
    plan.options.ssh.keyboard_interactive = true;
    plan.options.ssh.ki_password = false;
    let (done, seen) = started(&plan, &[PASSWORD]);
    assert!(done.is_ok());
    assert!(texts(&seen).iter().any(|text| text == QUESTION), "{seen:?}");
}

/// An FTP session starts with WinSCP's statuses, its password asked when it has none; with no user it logs
/// in anonymously; it opens at the folder given.
#[test]
fn an_ftp_session_logs_in_and_opens_its_folder() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("docs")).unwrap();
    let server = crate::test_ftp::TestFtp::start(temp.path());
    let mut plan = plan(server.port, &temp.path().join("known_hosts"));
    plan.protocol = crate::session::Protocol::Ftp;
    plan.folder = "/docs".to_string();
    let (done, seen) = started(&plan, &["secret"]);
    let started_ftp = done.unwrap();
    assert_eq!(
        (started_ftp.fs.protocol(), started_ftp.folder.as_str()),
        ("FTP", "/docs")
    );
    assert!(started_ftp.connection.is_none());
    assert_eq!(texts(&seen), [PASSWORD_PROMPT]);
    plan.user = String::new();
    plan.folder = String::new();
    let (done, seen) = started(&plan, &[]);
    assert_eq!(done.unwrap().folder, "/");
    assert!(texts(&seen).is_empty(), "anonymous: nothing asked");
}

/// A WebDAV session opens at its folder, the password asked when it has none.
#[test]
fn a_webdav_session_opens_its_folder() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("docs")).unwrap();
    let server = crate::test_dav::TestDav::start(temp.path());
    let mut plan = plan(server.port, &temp.path().join("known_hosts"));
    plan.protocol = crate::session::Protocol::WebDav;
    plan.folder = "docs".to_string();
    let (done, seen) = started(&plan, &["secret"]);
    let started_dav = done.unwrap();
    assert_eq!(started_dav.fs.protocol(), "WebDAV");
    assert_eq!(started_dav.folder, "/docs");
    assert_eq!(texts(&seen), [PASSWORD_PROMPT]);
}

/// An S3 session opens at its bucket, the access key ID and the secret asked when it has none; a wrong
/// secret is refused.
#[test]
fn an_s3_session_opens_its_bucket() {
    use crate::s3_login::{ACCESS_KEY_PROMPT, SECRET_PROMPT};
    use crate::test_s3::{ACCESS_KEY, SECRET, TestS3};
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("docs")).unwrap();
    let server = TestS3::start(temp.path());
    let mut plan = plan(server.port, &temp.path().join("known_hosts"));
    plan.protocol = crate::session::Protocol::S3;
    plan.user = String::new();
    plan.folder = "docs".to_string();
    let (done, seen) = started(&plan, &[ACCESS_KEY, SECRET]);
    let started_s3 = done.unwrap();
    assert_eq!(started_s3.fs.protocol(), "S3");
    assert_eq!(started_s3.folder, "/docs");
    assert_eq!(started_s3.user, ACCESS_KEY);
    assert!(started_s3.connection.is_none());
    assert_eq!(texts(&seen), [ACCESS_KEY_PROMPT, SECRET_PROMPT]);
    plan.user = ACCESS_KEY.to_string();
    plan.password = "wrong".to_string();
    let (done, seen) = started(&plan, &[]);
    assert!(matches!(done, Err(Failure::Refused)), "{done:?}");
    assert!(texts(&seen).is_empty());
    plan.folder = String::new();
    let (done, _) = started(&plan, &[]);
    assert!(
        matches!(done, Err(Failure::Refused)),
        "at the root: {done:?}"
    );
    plan.password = SECRET.to_string();
    assert_eq!(started(&plan, &[]).0.unwrap().folder, "/");
}
