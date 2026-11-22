//! S3's file system (WinSCP's `TS3FileSystem`) over `reqwest` with SigV4 (`sigv4`): the root lists the
//! buckets, a bucket's keys are folders by their `/`s (ListObjectsV2 with the delimiter, page after page);
//! an object by HEAD, a folder by its prefix; files down by GET (a seek's Range) and up by PUT (its length
//! known first); folders made as empty `dir/` objects (and buckets at the root); renaming a file is copying
//! and deleting it; "Duplicate" of a file is the server's copy. Virtual-host addresses (`bucket.host`), or
//! path-style ones for an address or when the session asks. Times and rights are not set.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::{Client, Method, Response, StatusCode};
use tokio::io::AsyncWriteExt;

use crate::link_options::Link;
use crate::s3_xml;
use crate::sftp::RemoteEntry;
use crate::sigv4::{self, Request, Signer};

/// An S3 session.
#[derive(Clone)]
pub struct S3 {
    client: Client,
    scheme: &'static str,
    /// The endpoint as the `Host` header names it (`host[:port]`).
    host: String,
    signer: Signer,
    path_style: bool,
    /// "Requester pays": each request says the requester pays (`x-amz-request-payer`).
    requester_pays: bool,
    /// The buckets in other regions than the session's, as they told.
    regions: Arc<Mutex<HashMap<String, String>>>,
}

/// What an S3 session connects with.
pub struct Connect<'a> {
    pub host: &'a str,
    pub port: u16,
    pub link: &'a Link,
    pub tls: Option<suppaftp::tokio_rustls::rustls::ClientConfig>,
    pub signer: Signer,
    /// The S3 tab's: path-style addresses (`host/bucket/key`, else virtual-host ones: `bucket.host/key`),
    /// "Requester pays".
    pub options: &'a crate::protocol_options::S3,
}

/// A path's bucket and key (`/bucket/a/b` → `bucket`, `a/b`).
pub fn split(path: &str) -> (&str, &str) {
    let path = path.trim_start_matches('/');
    path.split_once('/').unwrap_or((path, ""))
}

/// Whether the endpoint is an address or a single name (path-style then).
pub fn needs_path_style(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok() || !host.contains('.')
}

fn now() -> i64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0))
}

/// A response's failure: S3's code and message, else its status.
async fn failure(response: Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    failed(status, &body)
}

/// The failure of a response with `status` and `body`.
fn failed(status: StatusCode, body: &str) -> String {
    match s3_xml::error(body) {
        Some(error) => error,
        None => format!(
            "{} {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or_default()
        ),
    }
}

/// What a request sends besides its method and path.
#[derive(Default)]
struct Extra<'a> {
    query: &'a [(&'a str, &'a str)],
    headers: &'a [(&'a str, &'a str)],
    body: Option<reqwest::Body>,
}

impl S3 {
    pub fn new(connect: Connect<'_>) -> Result<Self, String> {
        let tls = match &connect.tls {
            Some(tls) => tls.clone(),
            None => crate::ftp_tls::config(Vec::new(), Default::default())?,
        };
        let mut builder = Client::builder()
            .use_preconfigured_tls(tls)
            .redirect(reqwest::redirect::Policy::none());
        if connect.link.timeout > 0 {
            builder = builder.connect_timeout(Duration::from_secs(connect.link.timeout.into()));
        }
        let client = builder.build().map_err(|error| error.to_string())?;
        let (scheme, default) = match connect.tls {
            Some(_) => ("https", 443),
            None => ("http", 80),
        };
        let host = match connect.port == default {
            true => connect.host.to_string(),
            false => format!("{}:{}", connect.host, connect.port),
        };
        Ok(Self {
            client,
            scheme,
            host,
            signer: connect.signer,
            path_style: connect.options.path_style || needs_path_style(connect.host),
            requester_pays: connect.options.requester_pays,
            regions: Arc::default(),
        })
    }

    /// The request's host and the path signed and sent, for a bucket's key.
    fn place(&self, bucket: &str, key: &str) -> (String, String) {
        match (bucket.is_empty(), self.path_style) {
            (true, _) => (self.host.clone(), "/".to_string()),
            (false, true) => (self.host.clone(), format!("/{bucket}/{key}")),
            (false, false) => (format!("{bucket}.{}", self.host), format!("/{key}")),
        }
    }

    /// The region the bucket's requests are signed for: the one it told, else the session's.
    fn region(&self, bucket: &str) -> String {
        let known = self
            .regions
            .lock()
            .ok()
            .and_then(|regions| regions.get(bucket).cloned());
        known.unwrap_or_else(|| self.signer.region.clone())
    }

    /// The request sent; a bucket in another region (WinSCP's "Unknown bucket, will detect its region")
    /// asked again in it once, when the request has no body to send again.
    async fn send(
        &self,
        method: &str,
        (bucket, key): (&str, &str),
        mut extra: Extra<'_>,
    ) -> Result<Response, String> {
        let body = extra.body.take();
        let again = body.is_none() && !bucket.is_empty();
        let region = self.region(bucket);
        let response = self
            .attempt(method, (bucket, key), &extra, (&region, body))
            .await?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let told = (response.headers().get("x-amz-bucket-region"))
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let text = response.text().await.unwrap_or_default();
        let told = told.or_else(|| s3_xml::error_region(&text));
        match told.filter(|told| again && *told != region) {
            Some(told) => {
                if let Ok(mut regions) = self.regions.lock() {
                    regions.insert(bucket.to_string(), told.clone());
                }
                let response = self
                    .attempt(method, (bucket, key), &extra, (&told, None))
                    .await?;
                match response.status().is_success() {
                    true => Ok(response),
                    false => Err(failure(response).await),
                }
            }
            None => Err(failed(status, &text)),
        }
    }

    /// One request signed for `region`.
    async fn attempt(
        &self,
        method: &str,
        (bucket, key): (&str, &str),
        extra: &Extra<'_>,
        (region, body): (&str, Option<reqwest::Body>),
    ) -> Result<Response, String> {
        let (host, path) = self.place(bucket, key);
        let stamp = sigv4::stamp(now());
        let payload = match body {
            Some(_) => sigv4::UNSIGNED,
            None => sigv4::EMPTY_SHA256,
        };
        let mut headers = extra.headers.to_vec();
        if self.requester_pays {
            headers.push(("x-amz-request-payer", "requester"));
        }
        let request = Request {
            method,
            path: &path,
            query: extra.query,
            headers: &headers,
            host: &host,
            payload,
            stamp: &stamp,
        };
        let signer = Signer {
            region: region.to_string(),
            ..self.signer.clone()
        };
        let authorization = signer.authorization(&request);
        let mut url = format!("{}://{host}{}", self.scheme, sigv4::uri_encode(&path, true));
        if !extra.query.is_empty() {
            url.push('?');
            url.push_str(&sigv4::canonical_query(extra.query));
        }
        let method = Method::from_bytes(method.as_bytes()).unwrap_or(Method::GET);
        let mut builder = (self.client.request(method, url))
            .header("x-amz-date", &stamp)
            .header("x-amz-content-sha256", payload)
            .header("authorization", authorization);
        for (name, value) in headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = body {
            builder = builder.body(body);
        }
        builder.send().await.map_err(|error| error.to_string())
    }

    /// The folder's items: the buckets at the root, else the keys and prefixes under it.
    pub async fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, String> {
        let (bucket, key) = split(path);
        if bucket.is_empty() {
            let response = self.send("GET", ("", ""), Extra::default()).await?;
            let text = response.text().await.map_err(|error| error.to_string())?;
            return Ok(entries("", &s3_xml::buckets(&text)?));
        }
        let prefix = match key.trim_end_matches('/') {
            "" => String::new(),
            key => format!("{key}/"),
        };
        let mut found = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut query = vec![
                ("list-type", "2"),
                ("delimiter", "/"),
                ("prefix", prefix.as_str()),
            ];
            if let Some(token) = &token {
                query.push(("continuation-token", token.as_str()));
            }
            let extra = Extra {
                query: &query,
                ..Extra::default()
            };
            let response = self.send("GET", (bucket, ""), extra).await?;
            let text = response.text().await.map_err(|error| error.to_string())?;
            let page = s3_xml::objects(&text)?;
            found.extend(entries(&prefix, &page.objects));
            match page.next {
                Some(next) => token = Some(next),
                None => return Ok(found),
            }
        }
    }

    /// The item `path`: an object, else a folder when its prefix has keys (a bucket is a folder).
    pub async fn entry(&self, path: &str) -> Result<RemoteEntry, String> {
        let (bucket, key) = split(path);
        let name = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default();
        let folder = RemoteEntry {
            name: name.to_string(),
            is_dir: true,
            permissions: Some(0o040_755),
            ..RemoteEntry::default()
        };
        if key.trim_end_matches('/').is_empty() {
            // The root is there; a bucket is asked about (HeadBucket).
            if !bucket.is_empty() {
                self.send("HEAD", (bucket, ""), Extra::default()).await?;
            }
            return Ok(folder);
        }
        if let Ok(response) = self.send("HEAD", (bucket, key), Extra::default()).await {
            let header = |name: &str| {
                (response.headers().get(name))
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string)
            };
            return Ok(RemoteEntry {
                name: name.to_string(),
                size: header("content-length")
                    .and_then(|size| size.parse().ok())
                    .unwrap_or(0),
                modified: header("last-modified")
                    .and_then(|date| crate::webdav_xml::http_date(&date)),
                permissions: Some(0o100_644),
                ..RemoteEntry::default()
            });
        }
        let prefix = format!("{}/", key.trim_end_matches('/'));
        if (self.send("HEAD", (bucket, &prefix), Extra::default()).await).is_ok() {
            return Ok(folder);
        }
        let query = [
            ("list-type", "2"),
            ("max-keys", "1"),
            ("prefix", prefix.as_str()),
        ];
        let extra = Extra {
            query: &query,
            ..Extra::default()
        };
        let response = self.send("GET", (bucket, ""), extra).await?;
        let text = response.text().await.map_err(|error| error.to_string())?;
        match s3_xml::objects(&text)?.objects.is_empty() {
            true => Err(format!("{path}: NoSuchKey")),
            false => Ok(folder),
        }
    }

    /// The server's path of `path`: the root for "." (S3 has no home: its buckets listed, which the keys
    /// must be good for), else the folder if it is one.
    pub async fn canonicalize(&self, path: &str) -> Result<String, String> {
        let path = match path.trim().trim_matches('/') {
            "" | "." => {
                self.send("GET", ("", ""), Extra::default()).await?;
                return Ok("/".to_string());
            }
            path => format!("/{path}"),
        };
        match self.entry(&path).await?.is_dir {
            true => Ok(path),
            false => Err(format!("{path}: not a folder")),
        }
    }

    /// A bucket at the root, else an empty `dir/` object.
    pub async fn mkdir(&self, path: &str) -> Result<(), String> {
        let (bucket, key) = split(path);
        let key = match key.trim_end_matches('/') {
            "" => String::new(),
            key => format!("{key}/"),
        };
        let extra = Extra {
            headers: &[("content-length", "0")],
            body: (!key.is_empty()).then(|| reqwest::Body::from(Vec::new())),
            ..Extra::default()
        };
        self.send("PUT", (bucket, &key), extra).await.map(drop)
    }

    pub async fn remove(&self, path: &str) -> Result<(), String> {
        self.send("DELETE", split(path), Extra::default())
            .await
            .map(drop)
    }

    /// A bucket, else its `dir/` object (a folder with no such object simply goes when empty).
    pub async fn rmdir(&self, path: &str) -> Result<(), String> {
        let (bucket, key) = split(path);
        let key = match key.trim_end_matches('/') {
            "" => String::new(),
            key => format!("{key}/"),
        };
        self.send("DELETE", (bucket, &key), Extra::default())
            .await
            .map(drop)
    }

    /// The object `from` copied to `to` by the server (CopyObject).
    pub async fn copy(&self, from: &str, to: &str) -> Result<(), String> {
        let (bucket, key) = split(from);
        let source = sigv4::uri_encode(&format!("/{bucket}/{key}"), true);
        let headers = [("x-amz-copy-source", source.as_str())];
        let extra = Extra {
            headers: &headers,
            ..Extra::default()
        };
        self.send("PUT", split(to), extra).await.map(drop)
    }

    /// A file renamed: copied, then deleted.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        if self.entry(from).await?.is_dir {
            return Err("Operation not supported.".to_string());
        }
        self.copy(from, to).await?;
        self.remove(from).await
    }

    /// A file of `path` to read (from a seek's offset), or to write `size` bytes to.
    pub fn file(&self, path: &str, writing: Option<u64>) -> S3File {
        S3File {
            s3: self.clone(),
            path: path.to_string(),
            writing,
            offset: 0,
            reading: None,
            pending: Vec::new(),
            sending: None,
            parts: writing.and_then(multipart::Multipart::for_size),
        }
    }
}

/// The items listed under `prefix` (the folder's own object left out), prefixes as folders.
fn entries(prefix: &str, objects: &[s3_xml::Object]) -> Vec<RemoteEntry> {
    (objects.iter())
        .filter(|object| object.key != prefix)
        // A key outside the prefix asked for (a server's mistake) is no item of the folder.
        .filter_map(|object| Some((object, object.key.strip_prefix(prefix)?)))
        .map(|(object, rest)| {
            let name = rest.trim_end_matches('/');
            RemoteEntry {
                name: name.to_string(),
                size: object.size,
                is_dir: object.prefix,
                modified: object.modified,
                permissions: Some(if object.prefix { 0o040_755 } else { 0o100_644 }),
                ..RemoteEntry::default()
            }
        })
        .filter(|entry| !entry.name.is_empty())
        .collect()
}

/// An upload under way: what the work writes, and the PUT that sends it (its ETag when done).
struct Sending {
    writer: tokio::io::DuplexStream,
    put: tokio::task::JoinHandle<Result<String, String>>,
}

impl Sending {
    /// A PUT of `length` bytes to `path` with `query` (a part's), its body what is written.
    fn start(s3: &S3, path: &str, length: u64, query: Vec<(String, String)>) -> Self {
        let (s3, path) = (s3.clone(), path.to_string());
        let (writer, reader) = tokio::io::duplex(64 * 1024);
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(reader));
        let put = tokio::spawn(async move {
            let length = length.to_string();
            let headers = [("content-length", length.as_str())];
            let query: Vec<(&str, &str)> = (query.iter())
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            let extra = Extra {
                query: &query,
                headers: &headers,
                body: Some(body),
            };
            let response = s3.send("PUT", split(&path), extra).await?;
            let etag = response.headers().get("etag");
            Ok(etag
                .and_then(|etag| etag.to_str().ok())
                .unwrap_or_default()
                .to_string())
        });
        Self { writer, put }
    }

    /// The body closed, the server's answer awaited: the ETag.
    async fn finished(self) -> std::io::Result<String> {
        let Self { mut writer, put } = self;
        writer.shutdown().await?;
        drop(writer);
        put.await
            .map_err(|error| io(error.to_string()))?
            .map_err(io)
    }
}

/// A file's transfer: its GET or PUT sent at the first read or write.
pub struct S3File {
    s3: S3,
    path: String,
    /// The bytes an upload sends; none: a download.
    writing: Option<u64>,
    offset: u64,
    reading: Option<Response>,
    pending: Vec<u8>,
    sending: Option<Sending>,
    /// A file over 5 MiB goes up in parts (WinSCP's multipart upload).
    parts: Option<multipart::Multipart>,
}

fn io(error: String) -> std::io::Error {
    std::io::Error::other(error)
}

impl S3File {
    pub async fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.reading.is_none() {
            let range = format!("bytes={}-", self.offset);
            let headers = [("range", range.as_str())];
            let extra = Extra {
                headers: if self.offset > 0 { &headers } else { &[] },
                ..Extra::default()
            };
            let response = self
                .s3
                .send("GET", split(&self.path), extra)
                .await
                .map_err(io)?;
            if self.offset > 0 && response.status() != StatusCode::PARTIAL_CONTENT {
                return Err(io("The server cannot resume the transfer.".to_string()));
            }
            self.reading = Some(response);
        }
        if self.pending.is_empty()
            && let Some(response) = self.reading.as_mut()
            && let Some(chunk) = response
                .chunk()
                .await
                .map_err(|error| io(error.to_string()))?
        {
            self.pending = chunk.to_vec();
        }
        let taken = buffer.len().min(self.pending.len());
        buffer[..taken].copy_from_slice(&self.pending[..taken]);
        self.pending.drain(..taken);
        Ok(taken)
    }

    fn sending(&mut self) -> &mut Sending {
        let length = self.writing.unwrap_or(0);
        let (s3, path) = (&self.s3, &self.path);
        (self.sending).get_or_insert_with(|| Sending::start(s3, path, length, Vec::new()))
    }

    pub async fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if self.parts.is_some() {
            return self.write_parts(bytes).await;
        }
        self.sending().writer.write_all(bytes).await
    }

    /// Where a read starts (a Range), before it is sent.
    pub fn seek(&mut self, offset: u64) -> std::io::Result<u64> {
        match (self.reading.is_none(), self.writing) {
            (true, None) => {
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
        if self.writing.is_none() {
            return Ok(());
        }
        if self.parts.is_some() {
            return self.finish_parts().await;
        }
        self.sending();
        let Some(sending) = self.sending.take() else {
            return Err(io("no upload".to_string()));
        };
        sending.finished().await.map(drop)
    }
}

#[path = "s3_multipart.rs"]
mod multipart;

#[cfg(test)]
#[path = "s3_tests.rs"]
mod tests;
