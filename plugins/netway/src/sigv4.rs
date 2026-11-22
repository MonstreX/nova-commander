//! AWS Signature Version 4 for S3 (the requests WinSCP's `TS3FileSystem` sends through libs3): the
//! canonical request (the path's segments encoded once, the query sorted, the signed headers in order),
//! the string to sign with the day's scope, the signing key's HMAC-SHA256 chain, the `Authorization`
//! header. The payload is `UNSIGNED-PAYLOAD` (over HTTPS, as the SDKs do for streamed uploads) or the hash of
//! an empty body.

use ring::{digest, hmac};

/// The hash of an empty body.
pub const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
/// A streamed body's payload hash.
pub const UNSIGNED: &str = "UNSIGNED-PAYLOAD";

/// Who signs: the access key, its secret and the region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signer {
    pub access_key: String,
    pub secret: String,
    pub region: String,
}

/// A request to sign.
pub struct Request<'a> {
    pub method: &'a str,
    /// The path, not yet encoded.
    pub path: &'a str,
    /// The query's parameters, not yet encoded.
    pub query: &'a [(&'a str, &'a str)],
    /// The headers to sign besides `host`, `x-amz-content-sha256` and `x-amz-date` (lower-case names).
    pub headers: &'a [(&'a str, &'a str)],
    pub host: &'a str,
    pub payload: &'a str,
    /// `YYYYMMDDTHHMMSSZ`.
    pub stamp: &'a str,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// SHA-256 as hex.
pub fn sha256(bytes: &[u8]) -> String {
    hex(digest::digest(&digest::SHA256, bytes).as_ref())
}

fn hmac_of(key: &[u8], data: &str) -> Vec<u8> {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    hmac::sign(&key, data.as_bytes()).as_ref().to_vec()
}

/// RFC 3986's encoding of AWS (`/` kept in a path when `path` is set).
pub fn uri_encode(text: &str, path: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte))
            }
            b'/' if path => out.push('/'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The query's canonical form: encoded, sorted by name then value.
pub fn canonical_query(query: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(String, String)> = (query.iter())
        .map(|(name, value)| (uri_encode(name, false), uri_encode(value, false)))
        .collect();
    pairs.sort();
    let parts: Vec<String> = pairs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    parts.join("&")
}

/// The headers signed, sorted, with their canonical lines and their names.
fn canonical_headers(request: &Request<'_>) -> (String, String) {
    let mut headers: Vec<(String, String)> = vec![
        ("host".to_string(), request.host.to_string()),
        (
            "x-amz-content-sha256".to_string(),
            request.payload.to_string(),
        ),
        ("x-amz-date".to_string(), request.stamp.to_string()),
    ];
    headers.extend(
        (request.headers.iter())
            .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string())),
    );
    headers.sort();
    let lines: String = headers
        .iter()
        .map(|(name, value)| format!("{name}:{value}\n"))
        .collect();
    let names: Vec<&str> = headers.iter().map(|(name, _)| name.as_str()).collect();
    (lines, names.join(";"))
}

impl Signer {
    /// The `Authorization` header of `request`.
    pub fn authorization(&self, request: &Request<'_>) -> String {
        let (headers, signed) = canonical_headers(request);
        let canonical = format!(
            "{}\n{}\n{}\n{headers}\n{signed}\n{}",
            request.method,
            uri_encode(request.path, true),
            canonical_query(request.query),
            request.payload
        );
        let day = &request.stamp[..8];
        let scope = format!("{day}/{}/s3/aws4_request", self.region);
        let to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{scope}\n{}",
            request.stamp,
            sha256(canonical.as_bytes())
        );
        let key = hmac_of(format!("AWS4{}", self.secret).as_bytes(), day);
        let key = hmac_of(&key, &self.region);
        let key = hmac_of(&key, "s3");
        let key = hmac_of(&key, "aws4_request");
        let signature = hex(&hmac_of(&key, &to_sign));
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}",
            self.access_key
        )
    }
}

/// `seconds` since 1970 as SigV4's `YYYYMMDDTHHMMSSZ`.
pub fn stamp(seconds: i64) -> String {
    let (year, month, day) = crate::scp::civil_from_days(seconds.div_euclid(86_400));
    let of_day = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (of_day / 3600, of_day / 60 % 60, of_day % 60);
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

#[cfg(test)]
#[path = "sigv4_tests.rs"]
mod tests;
