use super::*;

#[test]
fn socks4_asks_by_address_or_by_name() {
    let by_address = Target::Address("10.0.0.2".parse().unwrap(), 22);
    assert_eq!(
        socks4_request(&by_address, "u").unwrap(),
        [4, 1, 0, 22, 10, 0, 0, 2, b'u', 0]
    );
    let by_name = Target::Name("h".to_string(), 2222);
    assert_eq!(
        socks4_request(&by_name, "").unwrap(),
        [4, 1, 8, 174, 0, 0, 0, 1, 0, b'h', 0]
    );
    let v6 = Target::Address("::1".parse().unwrap(), 22);
    assert!(socks4_request(&v6, "").is_err());
}

#[test]
fn socks5_greets_logs_in_and_asks() {
    assert_eq!(socks5_greeting(""), [5, 1, 0]);
    assert_eq!(socks5_greeting("u"), [5, 2, 0, 2]);
    assert_eq!(socks5_login("u", "pw"), [1, 1, b'u', 2, b'p', b'w']);
    let by_name = Target::Name("ab".to_string(), 22);
    assert_eq!(socks5_request(&by_name), [5, 1, 0, 3, 2, b'a', b'b', 0, 22]);
    let by_address = Target::Address("1.2.3.4".parse().unwrap(), 80);
    assert_eq!(socks5_request(&by_address), [5, 1, 0, 1, 1, 2, 3, 4, 0, 80]);
}

#[test]
fn http_connects_with_basic_authentication() {
    assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
    assert_eq!(base64(b"a"), "YQ==");
    assert_eq!(base64(b"ab"), "YWI=");
    assert_eq!(
        http_request("h", 22, "", ""),
        "CONNECT h:22 HTTP/1.1\r\nHost: h:22\r\n\r\n"
    );
    let with_login = http_request("::1", 22, "user", "pass");
    assert!(with_login.starts_with("CONNECT [::1]:22 HTTP/1.1\r\n"));
    assert!(with_login.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"));
    assert_eq!(
        http_status("HTTP/1.1 200 Connection established\r\n\r\n"),
        Ok(())
    );
    assert_eq!(
        http_status("HTTP/1.1 407 Proxy Authentication Required\r\n"),
        Err(AUTHENTICATION_FAILED.to_string())
    );
    assert!(http_status("HTTP/1.0 403 Forbidden").is_err());
}

#[test]
fn telnet_commands_take_their_fields_and_escapes() {
    let fields = [
        ("host", "srv"),
        ("port", "22"),
        ("user", "u"),
        ("pass", "p"),
        ("proxyhost", "gate"),
        ("proxyport", "23"),
    ];
    assert_eq!(
        telnet_command("connect %host %port\\n", fields),
        "connect srv 22\n"
    );
    assert_eq!(
        telnet_command(
            "%user:%pass@%proxyhost:%proxyport 100%% \\x41\\t\\\\",
            fields
        ),
        "u:p@gate:23 100% A\t\\"
    );
    assert_eq!(telnet_command("%other \\q", fields), "%other \\q");
}
