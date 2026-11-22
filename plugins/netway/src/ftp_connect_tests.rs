use super::*;
use crate::ftp::{Connect, Ftp, open};
use crate::protocol_options::Ftp as Options;
use crate::runtime::Runtime;

/// A scripted FTP server for one session: each command's reply by its start; the commands it got.
fn scripted() -> (u16, std::thread::JoinHandle<Vec<String>>) {
    use std::io::{BufRead, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        // A client gone quiet ends the script (the test then tells what came).
        let quiet = Some(std::time::Duration::from_secs(10));
        stream.set_read_timeout(quiet).unwrap();
        let mut writer = stream.try_clone().unwrap();
        writer.write_all(b"220 ready\r\n").unwrap();
        let mut got = Vec::new();
        for line in std::io::BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            let reply = match line.split(' ').next().unwrap_or_default() {
                "HOST" => "220 host taken",
                "USER" => "331 password",
                "PASS" => "332 account",
                "ACCT" => "230 logged in",
                "FEAT" => "211-Features:\r\n211 End",
                "TYPE" => "200 binary",
                _ if line == "SITE BAD" => "500 refused",
                _ => "200 done",
            };
            got.push(line);
            writer.write_all(format!("{reply}\r\n").as_bytes()).unwrap();
        }
        got
    });
    (port, server)
}

fn started(port: u16, options: &Options) -> Result<Ftp, String> {
    let link = Link::default();
    let connect = Connect {
        host: "127.0.0.1",
        port,
        encryption: crate::session::Encryption::None,
        link: &link,
        options,
        tls: None,
    };
    let work = async {
        let stream = open(&connect).await?;
        Ftp::log_in(stream, ("u", "p"), 0, options).await
    };
    Runtime::new().unwrap().wait(work, &|| false).unwrap()
}

/// HOST first when asked, the account when the server asks for one, the commands after the login.
#[test]
fn host_account_and_commands_after_the_login_are_sent() {
    let (port, server) = scripted();
    let options = Options {
        host_command: Switch::On,
        account: "acct1".to_string(),
        post_login_commands: vec!["SITE UMASK 002".to_string()],
        ..Options::default()
    };
    drop(started(port, &options).unwrap());
    assert_eq!(
        server.join().unwrap(),
        [
            "HOST 127.0.0.1",
            "USER u",
            "PASS p",
            "ACCT acct1",
            "FEAT",
            "TYPE I",
            "SITE UMASK 002"
        ]
    );
    let (port, server) = scripted();
    let refused = Options {
        post_login_commands: vec!["SITE BAD".to_string()],
        ..Options::default()
    };
    let failed = started(port, &refused).err().unwrap_or_default();
    assert!(failed.starts_with("SITE BAD: 500"), "{failed}");
    let got = server.join().unwrap();
    assert_eq!(
        got[2], "ACCT default",
        "FileZilla's account when none is given"
    );
    assert!(
        !got.iter().any(|line| line.starts_with("HOST")),
        "HOST: Auto is off"
    );
}

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

#[test]
fn private_and_loopback_addresses_are_not_routable() {
    for address in [
        "127.0.0.1",
        "10.1.2.3",
        "192.168.0.5",
        "169.254.1.1",
        "0.1.2.3",
        "172.16.0.1",
        "172.31.9.9",
    ] {
        assert!(!routable(ip(address)), "{address}");
    }
    for address in ["8.8.8.8", "172.32.0.1", "172.15.0.1", "2001:db8::1"] {
        assert!(routable(ip(address)), "{address}");
    }
    assert!(!routable(ip("::1")));
}

/// FileZilla's `CheckForcePasvIp`: On the server's, Off the reply's, Auto the server's only for an
/// unroutable reply from a routable server (a proxied session's host name is never taken by itself).
#[test]
fn the_passive_host_follows_the_switch() {
    let private = ip("192.168.1.10");
    assert_eq!(pasv_host(Switch::On, private, "203.0.113.5"), "203.0.113.5");
    assert_eq!(
        pasv_host(Switch::Off, private, "203.0.113.5"),
        "192.168.1.10"
    );
    assert_eq!(
        pasv_host(Switch::Auto, private, "203.0.113.5"),
        "203.0.113.5"
    );
    assert_eq!(
        pasv_host(Switch::Auto, ip("198.51.100.7"), "203.0.113.5"),
        "198.51.100.7"
    );
    assert_eq!(pasv_host(Switch::Auto, private, "10.0.0.1"), "192.168.1.10");
    assert_eq!(
        pasv_host(Switch::Auto, private, "ftp.example.com"),
        "192.168.1.10"
    );
    assert_eq!(
        pasv_host(Switch::On, private, "ftp.example.com"),
        "ftp.example.com"
    );
}
