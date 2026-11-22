//! The proxies a session reaches its host through (WinSCP's `ProxyMethod`, PuTTY's `proxy.c` that WinSCP
//! uses): SOCKS4 (SOCKS4a when the proxy looks the name up), SOCKS5 with its user and password, HTTP
//! `CONNECT` with Basic authentication, Telnet's command (PuTTY's `format_telnet_command`). Each
//! handshake runs over the proxy's connection, which then carries the session.

use std::net::{IpAddr, Ipv4Addr};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// WinSCP's `MSG_PROXY_AUTHENTICATION_FAILED`.
pub const AUTHENTICATION_FAILED: &str = "Proxy authentication failed.";

/// Where the proxy is to connect: an address it is given, or a name it looks up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Address(IpAddr, u16),
    Name(String, u16),
}

/// SOCKS4's request: an IPv4 address, or SOCKS4a's 0.0.0.1 with the name after the user.
pub fn socks4_request(target: &Target, user: &str) -> Result<Vec<u8>, String> {
    let (address, port, name) = match target {
        Target::Address(IpAddr::V4(address), port) => (*address, *port, None),
        Target::Address(IpAddr::V6(_), _) => {
            return Err("SOCKS4 cannot connect to IPv6".to_string());
        }
        Target::Name(name, port) => (Ipv4Addr::new(0, 0, 0, 1), *port, Some(name)),
    };
    let mut request = vec![4, 1];
    request.extend(port.to_be_bytes());
    request.extend(address.octets());
    request.extend(user.as_bytes());
    request.push(0);
    if let Some(name) = name {
        request.extend(name.as_bytes());
        request.push(0);
    }
    Ok(request)
}

/// SOCKS5's greeting: no authentication, and user and password when there is a user.
pub fn socks5_greeting(user: &str) -> Vec<u8> {
    match user.is_empty() {
        true => vec![5, 1, 0],
        false => vec![5, 2, 0, 2],
    }
}

/// SOCKS5's user and password (RFC 1929).
pub fn socks5_login(user: &str, password: &str) -> Vec<u8> {
    let mut login = vec![1, user.len().min(255) as u8];
    login.extend(&user.as_bytes()[..user.len().min(255)]);
    login.push(password.len().min(255) as u8);
    login.extend(&password.as_bytes()[..password.len().min(255)]);
    login
}

/// SOCKS5's CONNECT to the target.
pub fn socks5_request(target: &Target) -> Vec<u8> {
    let mut request = vec![5, 1, 0];
    let port = match target {
        Target::Address(IpAddr::V4(address), port) => {
            request.push(1);
            request.extend(address.octets());
            port
        }
        Target::Address(IpAddr::V6(address), port) => {
            request.push(4);
            request.extend(address.octets());
            port
        }
        Target::Name(name, port) => {
            request.push(3);
            request.push(name.len().min(255) as u8);
            request.extend(&name.as_bytes()[..name.len().min(255)]);
            port
        }
    };
    request.extend(port.to_be_bytes());
    request
}

/// Base64 of `bytes` (HTTP's Basic authentication).
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = chunk
            .iter()
            .fold(0u32, |value, byte| (value << 8) | u32::from(*byte));
        let value = value << (8 * (3 - chunk.len()));
        for at in 0..4 {
            match at <= chunk.len() {
                true => text.push(char::from(
                    ALPHABET[((value >> (18 - 6 * at)) & 63) as usize],
                )),
                false => text.push('='),
            }
        }
    }
    text
}

/// HTTP's `CONNECT host:port` with Basic authentication when there is a user.
pub fn http_request(host: &str, port: u16, user: &str, password: &str) -> String {
    let target = match host.contains(':') {
        true => format!("[{host}]:{port}"),
        false => format!("{host}:{port}"),
    };
    let mut request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if !user.is_empty() {
        let credentials = base64(format!("{user}:{password}").as_bytes());
        request.push_str(&format!("Proxy-Authorization: Basic {credentials}\r\n"));
    }
    request.push_str("\r\n");
    request
}

/// The status of an HTTP proxy's answer's first line, a success 2xx.
pub fn http_status(head: &str) -> Result<(), String> {
    let line = head.lines().next().unwrap_or_default();
    let code: u16 = (line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| format!("Proxy error: {line}"))?;
    match code {
        200..=299 => Ok(()),
        407 => Err(AUTHENTICATION_FAILED.to_string()),
        _ => Err(format!("Proxy error: {line}")),
    }
}

/// PuTTY's `format_telnet_command`: `%host`, `%port`, `%user`, `%pass`, `%proxyhost`, `%proxyport`,
/// `%%`; `\n`, `\r`, `\t`, `\\` and `\xNN`.
pub fn telnet_command(template: &str, fields: [(&str, &str); 6]) -> String {
    let mut text = String::new();
    let mut rest = template;
    while let Some(ch) = rest.chars().next() {
        rest = &rest[ch.len_utf8()..];
        match ch {
            '%' => {
                if let Some(after) = rest.strip_prefix('%') {
                    text.push('%');
                    rest = after;
                    continue;
                }
                let field = (fields.iter()).find(|(name, _)| rest.starts_with(name));
                match field {
                    Some((name, value)) => {
                        text.push_str(value);
                        rest = &rest[name.len()..];
                    }
                    None => text.push('%'),
                }
            }
            '\\' => rest = escaped(rest, &mut text),
            other => text.push(other),
        }
    }
    text
}

/// One escape after a `\` read from `rest` into `text`; what is left.
fn escaped<'a>(rest: &'a str, text: &mut String) -> &'a str {
    let mut chars = rest.chars();
    match chars.next() {
        Some('n') => text.push('\n'),
        Some('r') => text.push('\r'),
        Some('t') => text.push('\t'),
        Some('\\') => text.push('\\'),
        Some('x') => {
            let code = rest
                .get(1..3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok());
            if let Some(code) = code {
                text.push(char::from(code));
                return &rest[3..];
            }
            text.push_str("\\x");
        }
        Some(other) => {
            text.push('\\');
            text.push(other);
        }
        None => {
            text.push('\\');
            return rest;
        }
    }
    &rest[rest.chars().next().map_or(0, char::len_utf8)..]
}

/// The SOCKS4 handshake over `stream`.
pub async fn socks4<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    target: &Target,
    user: &str,
) -> Result<(), String> {
    let request = socks4_request(target, user)?;
    stream
        .write_all(&request)
        .await
        .map_err(|error| error.to_string())?;
    let mut reply = [0u8; 8];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|error| error.to_string())?;
    match reply[1] {
        0x5a => Ok(()),
        0x5c | 0x5d => Err(AUTHENTICATION_FAILED.to_string()),
        code => Err(format!("SOCKS proxy refused the connection ({code:#04x}).")),
    }
}

/// The SOCKS5 handshake over `stream`.
pub async fn socks5<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    target: &Target,
    (user, password): (&str, &str),
) -> Result<(), String> {
    let io = |error: std::io::Error| error.to_string();
    stream.write_all(&socks5_greeting(user)).await.map_err(io)?;
    let mut chosen = [0u8; 2];
    stream.read_exact(&mut chosen).await.map_err(io)?;
    match chosen[1] {
        0 => {}
        2 => {
            stream
                .write_all(&socks5_login(user, password))
                .await
                .map_err(io)?;
            let mut answer = [0u8; 2];
            stream.read_exact(&mut answer).await.map_err(io)?;
            if answer[1] != 0 {
                return Err(AUTHENTICATION_FAILED.to_string());
            }
        }
        _ => return Err(AUTHENTICATION_FAILED.to_string()),
    }
    stream
        .write_all(&socks5_request(target))
        .await
        .map_err(io)?;
    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await.map_err(io)?;
    if head[1] != 0 {
        return Err(format!("SOCKS proxy refused the connection ({}).", head[1]));
    }
    let rest = match head[3] {
        1 => 4 + 2,
        4 => 16 + 2,
        3 => usize::from(stream.read_u8().await.map_err(io)?) + 2,
        _ => return Err("SOCKS proxy answered an unknown address.".to_string()),
    };
    let mut bound = vec![0u8; rest];
    stream.read_exact(&mut bound).await.map_err(io).map(drop)
}

/// The HTTP `CONNECT` over `stream`.
pub async fn http<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    (host, port): (&str, u16),
    (user, password): (&str, &str),
) -> Result<(), String> {
    let io = |error: std::io::Error| error.to_string();
    let request = http_request(host, port, user, password);
    stream.write_all(request.as_bytes()).await.map_err(io)?;
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 64 * 1024 {
            return Err("Proxy error: the answer is too long.".to_string());
        }
        head.push(stream.read_u8().await.map_err(io)?);
    }
    http_status(&String::from_utf8_lossy(&head))
}

#[cfg(test)]
#[path = "proxy_tests.rs"]
mod tests;
