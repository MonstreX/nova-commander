//! The tests' proxies inside their own process: SOCKS4, SOCKS5 (with a user and password when given),
//! HTTP `CONNECT` (with Basic authentication when given) and Telnet's `connect host port` line, each
//! joining the client to the target it asks for and counting the connections it carried.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Socks4,
    Socks5,
    Http,
    Telnet,
}

/// A proxy on a port of the loopback.
pub struct TestProxy {
    pub port: u16,
    carried: Arc<AtomicUsize>,
    _runtime: tokio::runtime::Runtime,
}

impl TestProxy {
    /// A proxy of `kind`, asking `login` (user, password) when given.
    pub fn start(kind: Kind, login: Option<(&str, &str)>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let listener = runtime.block_on(TcpListener::bind("127.0.0.1:0")).unwrap();
        let port = listener.local_addr().unwrap().port();
        let carried = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&carried);
        let login = login.map(|(user, password)| (user.to_string(), password.to_string()));
        runtime.spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let (counted, login) = (Arc::clone(&counted), login.clone());
                tokio::spawn(async move {
                    let mut client = client;
                    if let Some(mut target) = handshake(kind, &mut client, &login).await {
                        counted.fetch_add(1, Ordering::SeqCst);
                        let _ = tokio::io::copy_bidirectional(&mut client, &mut target).await;
                    }
                });
            }
        });
        Self {
            port,
            carried,
            _runtime: runtime,
        }
    }

    /// The connections it carried to their targets.
    pub fn carried(&self) -> usize {
        self.carried.load(Ordering::SeqCst)
    }
}

type Login = Option<(String, String)>;

/// The proxy's handshake with `client`: the target connected to, none when refused.
async fn handshake(kind: Kind, client: &mut TcpStream, login: &Login) -> Option<TcpStream> {
    match kind {
        Kind::Socks4 => socks4(client).await,
        Kind::Socks5 => socks5(client, login).await,
        Kind::Http => http(client, login).await,
        Kind::Telnet => telnet(client).await,
    }
}

async fn socks4(client: &mut TcpStream) -> Option<TcpStream> {
    let mut head = [0u8; 8];
    client.read_exact(&mut head).await.ok()?;
    let port = u16::from_be_bytes([head[2], head[3]]);
    let _user = read_zero_ended(client).await?;
    let host = match head[4..7] == [0, 0, 0] && head[7] != 0 {
        true => read_zero_ended(client).await?,
        false => format!("{}.{}.{}.{}", head[4], head[5], head[6], head[7]),
    };
    let target = TcpStream::connect((host.as_str(), port)).await.ok()?;
    client.write_all(&[0, 0x5a, 0, 0, 0, 0, 0, 0]).await.ok()?;
    Some(target)
}

async fn read_zero_ended(client: &mut TcpStream) -> Option<String> {
    let mut bytes = Vec::new();
    loop {
        match client.read_u8().await.ok()? {
            0 => return String::from_utf8(bytes).ok(),
            byte => bytes.push(byte),
        }
    }
}

async fn socks5(client: &mut TcpStream, login: &Login) -> Option<TcpStream> {
    let mut greeting = [0u8; 2];
    client.read_exact(&mut greeting).await.ok()?;
    let mut methods = vec![0u8; usize::from(greeting[1])];
    client.read_exact(&mut methods).await.ok()?;
    match login {
        None => client.write_all(&[5, 0]).await.ok()?,
        Some(login) => socks5_login(client, login).await?,
    }
    socks5_connect(client).await
}

/// SOCKS5's user and password checked.
async fn socks5_login(client: &mut TcpStream, (user, password): &(String, String)) -> Option<()> {
    client.write_all(&[5, 2]).await.ok()?;
    let _version = client.read_u8().await.ok()?;
    let given_user = read_counted(client).await?;
    let given_password = read_counted(client).await?;
    if (&given_user, &given_password) != (user, password) {
        client.write_all(&[1, 1]).await.ok()?;
        return None;
    }
    client.write_all(&[1, 0]).await.ok()
}

/// SOCKS5's CONNECT answered.
async fn socks5_connect(client: &mut TcpStream) -> Option<TcpStream> {
    let mut request = [0u8; 4];
    client.read_exact(&mut request).await.ok()?;
    let host = match request[3] {
        1 => {
            let mut address = [0u8; 4];
            client.read_exact(&mut address).await.ok()?;
            std::net::Ipv4Addr::from(address).to_string()
        }
        _ => read_counted(client).await?,
    };
    let port = client.read_u16().await.ok()?;
    let target = TcpStream::connect((host.as_str(), port)).await.ok()?;
    client
        .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
        .ok()?;
    Some(target)
}

async fn read_counted(client: &mut TcpStream) -> Option<String> {
    let length = client.read_u8().await.ok()?;
    let mut bytes = vec![0u8; usize::from(length)];
    client.read_exact(&mut bytes).await.ok()?;
    String::from_utf8(bytes).ok()
}

async fn http(client: &mut TcpStream, login: &Login) -> Option<TcpStream> {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.push(client.read_u8().await.ok()?);
    }
    let head = String::from_utf8(head).ok()?;
    let target = head.split_whitespace().nth(1)?.to_string();
    if let Some((user, password)) = login {
        let expected = crate::proxy::base64(format!("{user}:{password}").as_bytes());
        if !head.contains(&format!("Proxy-Authorization: Basic {expected}")) {
            let refused = "HTTP/1.1 407 Proxy Authentication Required\r\n\r\n";
            client.write_all(refused.as_bytes()).await.ok()?;
            return None;
        }
    }
    let target = TcpStream::connect(target.as_str()).await.ok()?;
    let answer = "HTTP/1.1 200 Connection established\r\n\r\n";
    client.write_all(answer.as_bytes()).await.ok()?;
    Some(target)
}

async fn telnet(client: &mut TcpStream) -> Option<TcpStream> {
    let mut line = Vec::new();
    loop {
        match client.read_u8().await.ok()? {
            b'\n' => break,
            byte => line.push(byte),
        }
    }
    let line = String::from_utf8(line).ok()?;
    let mut words = line.split_whitespace().skip(1);
    let (host, port) = (words.next()?, words.next()?.parse::<u16>().ok()?);
    TcpStream::connect((host, port)).await.ok()
}
