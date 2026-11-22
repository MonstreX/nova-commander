//! HTTP Digest authentication (RFC 2617, RFC 7616) as neon answers a WebDAV server's challenge for WinSCP:
//! the challenge's realm, nonce, opaque value, `qop` and algorithm (MD5, MD5-sess, SHA-256) read from
//! `WWW-Authenticate`; each request's `Authorization` with its own nonce count and client nonce.

use ring::rand::SecureRandom;

/// A server's Digest challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    /// `qop` offers "auth".
    pub qop_auth: bool,
    /// "MD5", "MD5-sess" or "SHA-256".
    pub algorithm: String,
}

/// `key=value` and `key="value"` pairs (a quoted value may hold commas and `\"`).
fn pairs(text: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut rest = text.trim();
    while let Some((key, after)) = rest.split_once('=') {
        let key = key
            .trim()
            .trim_start_matches(',')
            .trim()
            .to_ascii_lowercase();
        let after = after.trim_start();
        let (value, next) = match after.strip_prefix('"') {
            Some(quoted) => {
                let mut value = String::new();
                let mut chars = quoted.char_indices();
                let mut end = quoted.len();
                while let Some((at, ch)) = chars.next() {
                    match ch {
                        '\\' => value.extend(chars.next().map(|(_, escaped)| escaped)),
                        '"' => {
                            end = at + 1;
                            break;
                        }
                        other => value.push(other),
                    }
                }
                (value, &quoted[end..])
            }
            None => {
                let end = after.find(',').unwrap_or(after.len());
                (after[..end].trim().to_string(), &after[end..])
            }
        };
        found.push((key, value));
        rest = next.trim_start().trim_start_matches(',');
    }
    found
}

/// The Digest challenge among `WWW-Authenticate`'s values (none: no Digest offered, or one not answered).
pub fn challenge<'a>(headers: impl IntoIterator<Item = &'a str>) -> Option<Challenge> {
    let header = (headers.into_iter())
        .find_map(|value| value.trim().strip_prefix("Digest ").map(str::to_string))?;
    let found = pairs(&header);
    let field = |name: &str| {
        (found.iter())
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    let algorithm = field("algorithm").unwrap_or_else(|| "MD5".to_string());
    let known = ["MD5", "MD5-sess", "SHA-256"];
    let algorithm = known
        .iter()
        .find(|one| one.eq_ignore_ascii_case(&algorithm))?;
    Some(Challenge {
        realm: field("realm").unwrap_or_default(),
        nonce: field("nonce")?,
        opaque: field("opaque"),
        qop_auth: field("qop").is_some_and(|qop| qop.split(',').any(|one| one.trim() == "auth")),
        algorithm: algorithm.to_string(),
    })
}

fn hash(algorithm: &str, text: &str) -> String {
    let bytes = match algorithm {
        "SHA-256" => ring::digest::digest(&ring::digest::SHA256, text.as_bytes())
            .as_ref()
            .to_vec(),
        _ => md5::compute(text.as_bytes()).0.to_vec(),
    };
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A client nonce: 16 random bytes in hex.
pub fn client_nonce() -> String {
    let mut bytes = [0u8; 16];
    let _ = ring::rand::SystemRandom::new().fill(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The `Authorization` value for `method` of `uri` by `user` with `password`, the `count`th request
/// answering this challenge, with the client nonce `cnonce`.
pub fn authorization(
    challenge: &Challenge,
    (user, password): (&str, &str),
    (method, uri): (&str, &str),
    (count, cnonce): (u32, &str),
) -> String {
    let algorithm = challenge.algorithm.as_str();
    let base = algorithm.trim_end_matches("-sess");
    let mut ha1 = hash(base, &format!("{user}:{}:{password}", challenge.realm));
    if algorithm.ends_with("-sess") {
        ha1 = hash(base, &format!("{ha1}:{}:{cnonce}", challenge.nonce));
    }
    let ha2 = hash(base, &format!("{method}:{uri}"));
    let nc = format!("{count:08x}");
    let response = match challenge.qop_auth {
        true => hash(
            base,
            &format!("{ha1}:{}:{nc}:{cnonce}:auth:{ha2}", challenge.nonce),
        ),
        false => hash(base, &format!("{ha1}:{}:{ha2}", challenge.nonce)),
    };
    let mut value = format!(
        "Digest username=\"{user}\", realm=\"{}\", nonce=\"{}\", uri=\"{uri}\", algorithm={algorithm}, response=\"{response}\"",
        challenge.realm, challenge.nonce
    );
    if challenge.qop_auth {
        value.push_str(&format!(", qop=auth, nc={nc}, cnonce=\"{cnonce}\""));
    }
    if let Some(opaque) = &challenge.opaque {
        value.push_str(&format!(", opaque=\"{opaque}\""));
    }
    value
}

#[cfg(test)]
#[path = "digest_tests.rs"]
mod tests;
