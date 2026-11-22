use super::*;

fn lines(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    (pairs.iter())
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn the_defaults_are_winscps_and_write_nothing() {
    let link = Link::default();
    assert_eq!(
        (link.timeout, link.ping_interval, link.proxy_port),
        (15, 30, 80)
    );
    assert_eq!(link.proxy_host, "proxy");
    assert_eq!(link.telnet_command, "connect %host %port\\n");
    assert_eq!(link.proxy_dns, ProxyDns::Auto);
    assert!(link.lines().is_empty());
}

#[test]
fn winscps_values_are_read_and_written_back() {
    let read = Link::read(&lines(&[
        ("Timeout", "30"),
        ("PingType", "1"),
        ("PingIntervalSecs", "10"),
        ("AddressFamily", "2"),
        ("ProxyMethod", "socks5"),
        ("ProxyHost", "gate"),
        ("ProxyPort", "1080"),
        ("ProxyUsername", "pu"),
        ("ProxyDNS", "2"),
        ("ProxyLocalhost", "1"),
        ("Tunnel", "1"),
        ("TunnelHostName", "jump"),
        ("TunnelPortNumber", "2222"),
        ("TunnelUserName", "tu"),
        ("TunnelPublicKeyFile", r"C:\k.ppk"),
    ]));
    assert_eq!(
        (read.timeout, read.ping, read.ping_interval),
        (30, Ping::NullPacket, 10)
    );
    assert_eq!(read.family, Family::V6);
    assert_eq!(read.proxy, ProxyMethod::Socks5);
    assert_eq!((read.proxy_host.as_str(), read.proxy_port), ("gate", 1080));
    assert_eq!(read.proxy_dns, ProxyDns::Yes);
    assert!(read.proxy_localhost && read.tunnel);
    assert_eq!(
        (read.tunnel_host.as_str(), read.tunnel_port),
        ("jump", 2222)
    );
    assert_eq!(Link::read(&read.lines()), read);
    assert_eq!(
        Link::read(&lines(&[("ProxyMethod", "3")])).proxy,
        ProxyMethod::Http
    );
    assert_eq!(
        Link::read(&lines(&[("ProxyMethod", "cmd")])).proxy,
        ProxyMethod::Local
    );
}

/// The passwords are kept protected, never as typed (Windows: DPAPI; elsewhere not kept yet).
#[cfg(windows)]
#[test]
fn passwords_are_kept_protected() {
    let link = Link {
        proxy_password: "proxy secret".to_string(),
        tunnel_password: "tunnel secret".to_string(),
        ..Link::default()
    };
    let written = link.lines();
    assert!(written.iter().all(|(_, value)| !value.contains("secret")));
    assert_eq!(Link::read(&written), link);
}
