//! WebDAV's file system (WinSCP's `TWebDAVFileSystem`, RFC 4918) over `reqwest`: HTTP or HTTPS (the
//! certificate checked as FTPS's, `ftp_tls`), Basic authentication, or Digest once the server asks for it
//! (`digest`, S6.6f), the session's timeout and its SOCKS or HTTP proxy; folders by PROPFIND (depth 1, an
//! item by depth 0), MKCOL, DELETE, MOVE and COPY (the server copies: "Duplicate"), files down by GET (a
//! seek before it as a Range) and up by PUT, its body streamed from what the work writes. With the WebDAV
//! tab's "Enable compression" the listings and whole downloads are asked gzipped. Times and rights are not
//! set (WebDAV has no way for them).

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::{Client, Method, RequestBuilder, Response, StatusCode};
use tokio::io::AsyncWriteExt;

use crate::digest::{self, Challenge};
use crate::link_options::{Link, ProxyDns, ProxyMethod};
use crate::sftp::RemoteEntry;
use crate::webdav_xml::{self, PROPFIND};

/// A WebDAV session: its client, the server's base URL and its login.
#[derive(Clone)]
pub struct Dav {
    client: Client,
    base: String,
    user: String,
    password: String,
    /// "Enable compression": answers asked gzipped.
    compression: bool,
    /// How the server asked to be logged into.
    login: Arc<Mutex<Login>>,
}

/// The login a server asked for with its 401.
#[derive(Default)]
enum Login {
    /// Not asked yet: requests go without it.
    #[default]
    Unasked,
    Basic,
    /// Its challenge and the requests that answered it.
    Digest(Challenge, u32),
}

/// What a WebDAV session connects with.
pub struct Connect<'a> {
    pub host: &'a str,
    pub port: u16,
    pub link: &'a Link,
    /// HTTPS's TLS configuration; none: HTTP.
    pub tls: Option<suppaftp::tokio_rustls::rustls::ClientConfig>,
    pub user: &'a str,
    pub password: &'a str,
    /// The WebDAV tab's "Enable compression".
    pub compression: bool,
}

/// The session's proxy as `reqwest` takes it (SOCKS4, SOCKS5, HTTP).
fn proxy_of(host: &str, link: &Link) -> Result<Option<reqwest::Proxy>, String> {
    if !crate::route::proxied(host, link) {
        return Ok(None);
    }
    let scheme = match (link.proxy, link.proxy_dns) {
        (ProxyMethod::Socks4, ProxyDns::No) => "socks4",
        (ProxyMethod::Socks4, _) => "socks4a",
        (ProxyMethod::Socks5, ProxyDns::No) => "socks5",
        (ProxyMethod::Socks5, _) => "socks5h",
        (ProxyMethod::Http, _) => "http",
        _ => return Err("WebDAV goes through SOCKS and HTTP proxies only.".to_string()),
    };
    let address = format!("{scheme}://{}:{}", link.proxy_host, link.proxy_port);
    let proxy = reqwest::Proxy::all(address).map_err(|error| error.to_string())?;
    Ok(Some(match link.proxy_user.is_empty() {
        true => proxy,
        false => proxy.basic_auth(&link.proxy_user, &link.proxy_password),
    }))
}

/// A response's failure: its status and the server's words.
async fn failure(response: Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let reason = status.canonical_reason().unwrap_or_default();
    let words = body.trim();
    match words.is_empty() || words.starts_with('<') {
        true => format!("{} {reason}", status.as_u16()),
        false => format!("{} {reason}\n{words}", status.as_u16()),
    }
}

/// A gzipped body's bytes unpacked.
fn inflated(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut inflater = flate2::write::GzDecoder::new(Vec::new());
    inflater
        .write_all(bytes)
        .map_err(|error| error.to_string())?;
    inflater.finish().map_err(|error| error.to_string())
}

/// Whether the answer came gzipped.
fn gzipped(response: &Response) -> bool {
    (response.headers().get("content-encoding"))
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("gzip"))
}

/// The response when it succeeded, else its failure.
async fn succeeded(sent: reqwest::Result<Response>) -> Result<Response, String> {
    let response = sent.map_err(|error| error.to_string())?;
    match response.status().is_success() {
        true => Ok(response),
        false => Err(failure(response).await),
    }
}

impl Dav {
    /// The session as `connect` says (no request sent yet).
    pub fn new(connect: &Connect<'_>) -> Result<Self, String> {
        // The plugin's own TLS configuration always (`reqwest` has no crypto provider of its own here);
        // HTTP's is not used.
        let tls = match &connect.tls {
            Some(tls) => tls.clone(),
            None => crate::ftp_tls::config(Vec::new(), Default::default())?,
        };
        let mut builder = Client::builder().use_preconfigured_tls(tls);
        if connect.link.timeout > 0 {
            builder = builder.connect_timeout(Duration::from_secs(connect.link.timeout.into()));
        }
        if let Some(proxy) = proxy_of(connect.host, connect.link)? {
            builder = builder.proxy(proxy);
        }
        // A redirect would turn PROPFIND into GET: none followed (a folder is asked again with its `/`).
        let client = (builder.redirect(reqwest::redirect::Policy::none()).build())
            .map_err(|error| error.to_string())?;
        let scheme = if connect.tls.is_some() {
            "https"
        } else {
            "http"
        };
        let host = match connect.host.contains(':') {
            true => format!("[{}]", connect.host),
            false => connect.host.to_string(),
        };
        Ok(Self {
            client,
            base: format!("{scheme}://{host}:{}", connect.port),
            user: connect.user.to_string(),
            password: connect.password.to_string(),
            compression: connect.compression,
            login: Arc::default(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, webdav_xml::encoded(path))
    }

    /// A request with the login the server asked for: Digest's answer, or Basic; none before it asked
    /// (neon's: the password never goes out before a challenge says how).
    fn request(&self, method: &[u8], path: &str) -> RequestBuilder {
        let name = String::from_utf8_lossy(method).into_owned();
        let method = Method::from_bytes(method).unwrap_or(Method::GET);
        let request = self.client.request(method, self.url(path));
        if self.user.is_empty() {
            return request;
        }
        let mut known = self.login.lock().ok();
        match known.as_deref_mut() {
            Some(Login::Digest(challenge, count)) => {
                *count += 1;
                let login = (self.user.as_str(), self.password.as_str());
                let uri = webdav_xml::encoded(path);
                let nonce = digest::client_nonce();
                let value =
                    digest::authorization(challenge, login, (&name, &uri), (*count, &nonce));
                request.header("Authorization", value)
            }
            Some(Login::Basic) => request.basic_auth(&self.user, Some(&self.password)),
            Some(Login::Unasked) | None => request,
        }
    }

    /// The request `build` makes of `method` on `path` sent; refused with a Digest challenge, it is taken
    /// and the request sent again with its answer (neon's way).
    async fn fetch(
        &self,
        (method, path): (&[u8], &str),
        build: impl Fn(RequestBuilder) -> RequestBuilder,
    ) -> Result<Response, String> {
        let sent = build(self.request(method, path)).send().await;
        let response = sent.map_err(|error| error.to_string())?;
        if response.status() != StatusCode::UNAUTHORIZED || self.user.is_empty() {
            return succeeded(Ok(response)).await;
        }
        let offers: Vec<&str> = (response.headers().get_all("www-authenticate").iter())
            .filter_map(|value| value.to_str().ok())
            .collect();
        let basic = (offers.iter()).any(|offer| {
            (offer.trim().get(..6)).is_some_and(|scheme| scheme.eq_ignore_ascii_case("basic "))
                || offer.trim().eq_ignore_ascii_case("basic")
        });
        // Digest preferred when both are offered; a scheme already answered and refused again: wrong.
        let login = match digest::challenge(offers) {
            Some(challenge) => Login::Digest(challenge, 0),
            None if basic => Login::Basic,
            None => return succeeded(Ok(response)).await,
        };
        if !self.asked_for(login) {
            return succeeded(Ok(response)).await;
        }
        succeeded(build(self.request(method, path)).send().await).await
    }

    /// The `login` the server asked for kept; false when it is Basic again (refused already).
    fn asked_for(&self, login: Login) -> bool {
        let Ok(mut known) = self.login.lock() else {
            return false;
        };
        if matches!((&*known, &login), (Login::Basic, Login::Basic)) {
            return false;
        }
        *known = login;
        true
    }

    /// The body, unpacked when it came gzipped.
    async fn body(&self, response: Response) -> Result<String, String> {
        let packed = gzipped(&response);
        let bytes = response.bytes().await.map_err(|error| error.to_string())?;
        let bytes = match packed {
            true => inflated(&bytes)?,
            false => bytes.to_vec(),
        };
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// "Enable compression": the answer asked gzipped.
    fn packed(&self, request: RequestBuilder) -> RequestBuilder {
        match self.compression {
            true => request.header("Accept-Encoding", "gzip"),
            false => request,
        }
    }

    async fn propfind(&self, path: &str, depth: &str) -> Result<Vec<webdav_xml::Resource>, String> {
        let build = |request: RequestBuilder| {
            self.packed(request)
                .header("Depth", depth)
                .header("Content-Type", "application/xml; charset=utf-8")
                .body(PROPFIND)
        };
        let response = self.fetch((b"PROPFIND", path), build).await?;
        webdav_xml::multistatus(&self.body(response).await?)
    }

    /// The folder's items (the folder itself left out).
    pub async fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, String> {
        let folder = format!("{}/", path.trim_end_matches('/'));
        let resources = self.propfind(&folder, "1").await?;
        let own = folder.trim_end_matches('/');
        Ok((resources.into_iter())
            .filter(|resource| resource.path.trim_end_matches('/') != own)
            .filter(|resource| !resource.entry.name.is_empty())
            .map(|resource| resource.entry)
            .collect())
    }

    /// The item `path`.
    pub async fn entry(&self, path: &str) -> Result<RemoteEntry, String> {
        let mut found = match self.propfind(path, "0").await {
            Ok(found) if !found.is_empty() => found,
            // A folder named without its `/` (redirected): asked again with it.
            first => match self
                .propfind(&format!("{}/", path.trim_end_matches('/')), "0")
                .await
            {
                Ok(found) => found,
                Err(_) => first?,
            },
        };
        let mut entry = (found.pop())
            .ok_or_else(|| format!("{path}: No such file"))?
            .entry;
        if path.trim_end_matches('/').is_empty() {
            entry.name = "/".to_string();
            entry.is_dir = true;
        }
        Ok(entry)
    }

    /// The server's path of `path`: the root for "." (WebDAV has no home), else the folder if it is one.
    pub async fn canonicalize(&self, path: &str) -> Result<String, String> {
        let path = match path.trim() {
            "" | "." => "/".to_string(),
            path => format!("/{}", path.trim_matches('/')),
        };
        let entry = self.entry(&path).await?;
        match entry.is_dir {
            true => Ok(path),
            false => Err(format!("{path}: not a folder")),
        }
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), String> {
        self.fetch((b"MKCOL", path), |request| request)
            .await
            .map(drop)
    }

    pub async fn delete(&self, path: &str) -> Result<(), String> {
        self.fetch((b"DELETE", path), |request| request)
            .await
            .map(drop)
    }

    /// MOVE or COPY of `from` to `to`, never over an item there.
    async fn transfer(&self, method: &[u8], from: &str, to: &str) -> Result<(), String> {
        let build = |request: RequestBuilder| {
            request
                .header("Destination", self.url(to))
                .header("Overwrite", "F")
                .header("Depth", "infinity")
        };
        self.fetch((method, from), build).await.map(drop)
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        self.transfer(b"MOVE", from, to).await
    }

    pub async fn copy(&self, from: &str, to: &str) -> Result<(), String> {
        self.transfer(b"COPY", from, to).await
    }

    /// A file of `path` to read (from a seek's offset) or to write anew.
    pub fn file(&self, path: &str, writing: bool) -> DavFile {
        DavFile {
            dav: self.clone(),
            path: path.to_string(),
            writing,
            offset: 0,
            reading: None,
            pending: Vec::new(),
            sending: None,
            inflater: None,
        }
    }
}

/// An upload under way: what the work writes, and the PUT that sends it.
struct Sending {
    writer: tokio::io::DuplexStream,
    put: tokio::task::JoinHandle<Result<(), String>>,
}

/// A file's transfer: its GET or PUT sent at the first read or write.
pub struct DavFile {
    dav: Dav,
    path: String,
    writing: bool,
    offset: u64,
    reading: Option<Response>,
    pending: Vec<u8>,
    sending: Option<Sending>,
    /// A gzipped download's unpacking.
    inflater: Option<flate2::write::GzDecoder<Vec<u8>>>,
}

fn io(error: String) -> std::io::Error {
    std::io::Error::other(error)
}

impl DavFile {
    /// The GET sent: a Range from the offset (never gzipped), else the whole file (gzipped when asked).
    async fn opened(&mut self) -> std::io::Result<()> {
        let offset = self.offset;
        let build = |request: RequestBuilder| match offset {
            0 => self.dav.packed(request),
            _ => request.header("Range", format!("bytes={offset}-")),
        };
        let response = (self.dav.fetch((b"GET", &self.path), build).await).map_err(io)?;
        if offset > 0 && response.status() != StatusCode::PARTIAL_CONTENT {
            return Err(io("The server cannot resume the transfer.".to_string()));
        }
        if gzipped(&response) {
            self.inflater = Some(flate2::write::GzDecoder::new(Vec::new()));
        }
        self.reading = Some(response);
        Ok(())
    }

    /// The next bytes of the answer (unpacked when gzipped); none at its end.
    async fn next_bytes(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        let Some(response) = self.reading.as_mut() else {
            return Ok(None);
        };
        let chunk = (response.chunk().await).map_err(|error| io(error.to_string()))?;
        let Some(inflater) = self.inflater.as_mut() else {
            return Ok(chunk.map(|chunk| chunk.to_vec()));
        };
        let more = chunk.is_some();
        match chunk {
            Some(chunk) => inflater.write_all(&chunk)?,
            None => inflater.try_finish()?,
        }
        // A chunk that unpacks to nothing yet is not the end: an empty piece, read on.
        let unpacked = std::mem::take(inflater.get_mut());
        Ok((more || !unpacked.is_empty()).then_some(unpacked))
    }

    pub async fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.reading.is_none() {
            self.opened().await?;
        }
        while self.pending.is_empty() {
            match self.next_bytes().await? {
                Some(bytes) => self.pending = bytes,
                None => break,
            }
        }
        let taken = buffer.len().min(self.pending.len());
        buffer[..taken].copy_from_slice(&self.pending[..taken]);
        self.pending.drain(..taken);
        Ok(taken)
    }

    fn sending(&mut self) -> &mut Sending {
        self.sending.get_or_insert_with(|| {
            let (writer, reader) = tokio::io::duplex(64 * 1024);
            let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(reader));
            let request = self.dav.request(b"PUT", &self.path).body(body);
            let put = tokio::spawn(async move { succeeded(request.send().await).await.map(drop) });
            Sending { writer, put }
        })
    }

    pub async fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.sending().writer.write_all(bytes).await
    }

    /// Where a read starts (a Range), before it is sent.
    pub fn seek(&mut self, offset: u64) -> std::io::Result<u64> {
        match (self.reading.is_none(), self.writing) {
            (true, false) => {
                self.offset = offset;
                Ok(offset)
            }
            _ if offset == 0 => Ok(0),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Operation not supported.",
            )),
        }
    }

    /// The upload's end: its body closed, the server's answer awaited.
    pub async fn shutdown(&mut self) -> std::io::Result<()> {
        if !self.writing {
            return Ok(());
        }
        let Sending { mut writer, put } = match self.sending.take() {
            Some(sending) => sending,
            None => {
                self.sending();
                self.sending
                    .take()
                    .ok_or_else(|| io("no upload".to_string()))?
            }
        };
        writer.shutdown().await?;
        drop(writer);
        let done = put.await.map_err(|error| io(error.to_string()))?;
        done.map_err(io)
    }
}

#[cfg(test)]
#[path = "webdav_tests.rs"]
mod tests;
