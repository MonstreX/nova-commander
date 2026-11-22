//! The way to a session's host (the session dialog's Connection and Proxy tabs): straight, by the IP
//! version asked for, or through the proxy (SOCKS4, SOCKS5, HTTP, Telnet, or a local command whose input
//! and output carry the session, PuTTY's `pmCmd`); a local host straight unless "Consider proxying local
//! host connections"; all of it within the server's response timeout. The tunnel's way is the tunnel
//! connection's channel (`login`).

use std::net::IpAddr;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::link_options::{Family, Link, ProxyDns, ProxyMethod};
use crate::proxy::{self, Target};

/// A connection's bytes both ways.
pub trait Wire: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Wire for T {}

pub type Stream = Box<dyn Wire>;

/// The failure of a way not made in time.
pub const TIMED_OUT: &str = "Connection timed out.";

/// A host the machine itself is (PuTTY's `sk_hostname_is_local`).
pub fn is_local(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost") || host == "::1" || host.starts_with("127.")
}

/// Whether the session's way to `host` goes through its proxy.
pub fn proxied(host: &str, link: &Link) -> bool {
    link.proxy != ProxyMethod::None && (link.proxy_localhost || !is_local(host))
}

/// `work` within the session's timeout (0: none).
pub async fn within<T>(
    link: &Link,
    work: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    match link.timeout {
        0 => work.await,
        seconds => (tokio::time::timeout(Duration::from_secs(seconds.into()), work).await)
            .map_err(|_| TIMED_OUT.to_string())?,
    }
}

/// The way to `host`'s `port` as the session's `link` says, within its timeout (0: none).
pub async fn stream(host: &str, port: u16, link: &Link) -> Result<Stream, String> {
    let work = connect(host, port, link);
    match link.timeout {
        0 => work.await,
        seconds => (tokio::time::timeout(Duration::from_secs(seconds.into()), work).await)
            .map_err(|_| TIMED_OUT.to_string())?,
    }
}

async fn connect(host: &str, port: u16, link: &Link) -> Result<Stream, String> {
    match link.proxy {
        ProxyMethod::Local if proxied(host, link) => local_command(host, port, link),
        _ => Ok(Box::new(through(host, port, link).await?)),
    }
}

/// A TCP connection to `host`'s `port` as the session's `link` says, within its timeout: straight or
/// through a proxy over TCP (FTP's control and data connections; a local command's proxy carries none).
pub async fn tcp(host: &str, port: u16, link: &Link) -> Result<TcpStream, String> {
    if link.proxy == ProxyMethod::Local && proxied(host, link) {
        return Err("FTP cannot go through a local proxy command.".to_string());
    }
    let work = through(host, port, link);
    match link.timeout {
        0 => work.await,
        seconds => (tokio::time::timeout(Duration::from_secs(seconds.into()), work).await)
            .map_err(|_| TIMED_OUT.to_string())?,
    }
}

/// The TCP way: straight, or the proxy's connection after its handshake.
async fn through(host: &str, port: u16, link: &Link) -> Result<TcpStream, String> {
    if !proxied(host, link) {
        return direct(host, port, link.family).await;
    }
    let mut tcp = direct(&link.proxy_host, link.proxy_port, link.family).await?;
    let login = (link.proxy_user.as_str(), link.proxy_password.as_str());
    match link.proxy {
        ProxyMethod::Socks4 => {
            let target = target_of(host, port, link).await?;
            proxy::socks4(&mut tcp, &target, &link.proxy_user).await?;
        }
        ProxyMethod::Socks5 => {
            let target = target_of(host, port, link).await?;
            proxy::socks5(&mut tcp, &target, login).await?;
        }
        ProxyMethod::Http => proxy::http(&mut tcp, (host, port), login).await?,
        _ => {
            let command = command_of(&link.telnet_command, host, port, link);
            (tcp.write_all(command.as_bytes()).await).map_err(|error| error.to_string())?;
        }
    }
    Ok(tcp)
}

/// The telnet and local commands' fields.
fn fields(host: &str, port: u16, link: &Link) -> [(&'static str, String); 6] {
    [
        ("host", host.to_string()),
        ("port", port.to_string()),
        ("user", link.proxy_user.clone()),
        ("pass", link.proxy_password.clone()),
        ("proxyhost", link.proxy_host.clone()),
        ("proxyport", link.proxy_port.to_string()),
    ]
}

/// The command's text with the fields put in.
fn command_of(template: &str, host: &str, port: u16, link: &Link) -> String {
    let fields = fields(host, port, link);
    let borrowed: Vec<(&str, &str)> = (fields.iter())
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let borrowed: [(&str, &str); 6] = borrowed.try_into().unwrap_or([("", ""); 6]);
    proxy::telnet_command(template, borrowed)
}

/// Where the proxy connects: an address as given or looked up here ("Do DNS name lookup at proxy end":
/// No), else the name for the proxy to look up.
async fn target_of(host: &str, port: u16, link: &Link) -> Result<Target, String> {
    if let Ok(address) = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
    {
        return Ok(Target::Address(address, port));
    }
    if link.proxy_dns != ProxyDns::No {
        return Ok(Target::Name(host.to_string(), port));
    }
    let address = addresses(host, port, link.family).await?;
    let first = address
        .first()
        .ok_or_else(|| format!("Host \"{host}\" does not exist."))?;
    Ok(Target::Address(first.ip(), port))
}

/// `host`'s addresses of the IP version asked for.
async fn addresses(
    host: &str,
    port: u16,
    family: Family,
) -> Result<Vec<std::net::SocketAddr>, String> {
    let found = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| format!("Host \"{host}\" does not exist."))?;
    let wanted: Vec<_> = found
        .filter(|address| match family {
            Family::Auto => true,
            Family::V4 => address.is_ipv4(),
            Family::V6 => address.is_ipv6(),
        })
        .collect();
    match wanted.is_empty() {
        true => Err(format!("Host \"{host}\" does not exist.")),
        false => Ok(wanted),
    }
}

/// A TCP connection to `host`'s `port`, each address of the IP version tried in turn.
pub async fn direct(host: &str, port: u16, family: Family) -> Result<TcpStream, String> {
    let mut failure = String::new();
    for address in addresses(host, port, family).await? {
        match TcpStream::connect(address).await {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(error) => failure = error.to_string(),
        }
    }
    Err(failure)
}

/// The local proxy command run by the system's shell, its output and input as the way (the blocking
/// pipes bridged to the runtime by threads of their own).
fn local_command(host: &str, port: u16, link: &Link) -> Result<Stream, String> {
    let command = command_of(&link.local_command, host, port, link);
    let mut shell = match cfg!(windows) {
        true => std::process::Command::new("cmd"),
        false => std::process::Command::new("sh"),
    };
    shell
        .arg(if cfg!(windows) { "/C" } else { "-c" })
        .arg(&command);
    let mut child = (shell.stdin(std::process::Stdio::piped()))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        // The template shown, not the command: its %pass would put the password on the screen.
        .map_err(|error| format!("Cannot execute '{}'.\n{error}", link.local_command))?;
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (mut from_child, mut to_child) = tokio::io::split(theirs);
    let mut output = child.stdout.take().ok_or("no output")?;
    let mut input = child.stdin.take().ok_or("no input")?;
    let child = std::sync::Arc::new(std::sync::Mutex::new(child));
    let killed = std::sync::Arc::clone(&child);
    let runtime = tokio::runtime::Handle::current();
    let reading = runtime.clone();
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut buffer = vec![0u8; 32 * 1024];
        while let Ok(read @ 1..) = output.read(&mut buffer) {
            if reading
                .block_on(to_child.write_all(&buffer[..read]))
                .is_err()
            {
                break;
            }
        }
        let _ = reading.block_on(to_child.shutdown());
    });
    let writer = std::thread::spawn(move || {
        use std::io::Write;
        let mut buffer = vec![0u8; 32 * 1024];
        while let Ok(read @ 1..) = runtime.block_on(from_child.read(&mut buffer)) {
            if input.write_all(&buffer[..read]).is_err() {
                break;
            }
        }
        kill(&killed);
    });
    Ok(Box::new(Pumped {
        wire: Some(ours),
        child,
        pumps: vec![reader, writer],
    }))
}

/// The local command killed (it may have ended already).
fn kill(child: &std::sync::Mutex<std::process::Child>) {
    let mut child = child
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = child.kill();
    let _ = child.wait();
}

/// The local command's way: its bytes, the command and the threads pumping them - killed and waited for
/// as it goes (R7 review A1: left running, the threads could outlive the plugin's library).
struct Pumped {
    wire: Option<tokio::io::DuplexStream>,
    child: std::sync::Arc<std::sync::Mutex<std::process::Child>>,
    pumps: Vec<std::thread::JoinHandle<()>>,
}

impl Drop for Pumped {
    fn drop(&mut self) {
        // The way closed and the command killed: its pipes end, so does each pump.
        drop(self.wire.take());
        kill(&self.child);
        for pump in self.pumps.drain(..) {
            let _ = pump.join();
        }
    }
}

impl AsyncRead for Pumped {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.wire.as_mut() {
            Some(wire) => std::pin::Pin::new(wire).poll_read(context, buffer),
            None => std::task::Poll::Ready(Ok(())),
        }
    }
}

impl AsyncWrite for Pumped {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.wire.as_mut() {
            Some(wire) => std::pin::Pin::new(wire).poll_write(context, bytes),
            None => std::task::Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into())),
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.wire.as_mut() {
            Some(wire) => std::pin::Pin::new(wire).poll_flush(context),
            None => std::task::Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.wire.as_mut() {
            Some(wire) => std::pin::Pin::new(wire).poll_shutdown(context),
            None => std::task::Poll::Ready(Ok(())),
        }
    }
}

#[cfg(test)]
#[path = "route_tests.rs"]
mod tests;
