//! WebDAV's XML (RFC 4918) as WinSCP's `TWebDAVFileSystem` reads it through neon: PROPFIND's request for
//! the resource type, length and last change, and its `multistatus` answer - each `response`'s `href`
//! (percent-decoded, its last segment the name), whether it is a `collection`, its `getcontentlength` and
//! `getlastmodified` (RFC 1123 dates, UTC).

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::scp_listing::seconds_of;
use crate::sftp::RemoteEntry;

/// PROPFIND's body asking what the panel shows.
pub const PROPFIND: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:propfind xmlns:D=\"DAV:\"><D:prop><D:resourcetype/><D:getcontentlength/><D:getlastmodified/></D:prop></D:propfind>";

/// One `response` of a `multistatus`: its decoded path and what it says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resource {
    pub path: String,
    pub entry: RemoteEntry,
}

/// `%XX` escapes decoded (UTF-8; a broken one kept as it is).
pub fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = (bytes[at] == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match hex {
            Some(byte) => {
                out.push(byte);
                at += 3;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A path's segments percent-encoded for a URL (unreserved characters and `/` kept).
pub fn encoded(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(char::from(byte));
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// RFC 1123's `Sun, 06 Nov 1994 08:49:37 GMT` as seconds since 1970.
pub fn http_date(text: &str) -> Option<u32> {
    let mut words = text.split_whitespace().skip(1);
    let day: i64 = words.next()?.parse().ok()?;
    let month_name = words.next()?;
    let month = MONTHS.iter().position(|month| *month == month_name)?;
    let year: i64 = words.next()?.parse().ok()?;
    let mut clock = words
        .next()?
        .split(':')
        .map(|part| part.parse::<i64>().ok());
    let time = (clock.next()??, clock.next()??, clock.next()??);
    let month = i64::try_from(month).ok()? + 1;
    u32::try_from(seconds_of((year, month, day), time)).ok()
}

/// The element's local name (its namespace's prefix dropped), lower case.
fn local(name: &str) -> String {
    let local = name.rsplit(':').next().unwrap_or_default();
    local.to_ascii_lowercase()
}

/// An entity reference's text (`amp`, `lt`, `gt`, `quot`, `apos`, `#NN`, `#xNN`).
fn entity(name: &str) -> String {
    let code = match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        other => other.strip_prefix('#').and_then(|number| {
            let value = match number.strip_prefix('x') {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => number.parse().ok(),
            };
            value.and_then(char::from_u32)
        }),
    };
    code.map_or_else(|| format!("&{name};"), String::from)
}

/// A `multistatus` read.
pub fn multistatus(xml: &str) -> Result<Vec<Resource>, String> {
    let mut reader = Reader::from_str(xml);
    let mut found = Vec::new();
    let mut current: Option<Resource> = None;
    let mut text = String::new();
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(start) => {
                text.clear();
                match local(start.name().as_ref()).as_str() {
                    "response" => current = Some(Resource::default()),
                    "collection" => set_folder(&mut current),
                    _ => {}
                }
            }
            Event::Empty(empty) if local(empty.name().as_ref()) == "collection" => {
                set_folder(&mut current);
            }
            Event::Text(part) => text.push_str(&part.into_inner()),
            Event::GeneralRef(reference) => text.push_str(&entity(&reference.into_inner())),
            Event::End(end) => {
                let field = local(end.name().as_ref());
                if field == "response"
                    && let Some(mut resource) = current.take()
                {
                    // WebDAV tells no rights: the kind with a writable file's or folder's.
                    let entry = &mut resource.entry;
                    entry.permissions = Some(match entry.is_dir {
                        true => 0o040_755,
                        false => 0o100_644,
                    });
                    found.push(resource);
                }
                take(&mut current, &field, &text);
                text.clear();
            }
            Event::Eof => return Ok(found),
            _ => {}
        }
    }
}

fn set_folder(current: &mut Option<Resource>) {
    if let Some(resource) = current {
        resource.entry.is_dir = true;
    }
}

fn take(current: &mut Option<Resource>, field: &str, value: &str) {
    let Some(resource) = current else {
        return;
    };
    let value = value.trim();
    match field {
        "href" => {
            let path = decoded(value);
            // A full URL's path only.
            let path = match path.split_once("://") {
                Some((_, rest)) => rest
                    .find('/')
                    .map_or("/".to_string(), |at| rest[at..].to_string()),
                None => path,
            };
            let name = path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default();
            resource.entry.name = name.to_string();
            resource.path = path;
        }
        "getcontentlength" => resource.entry.size = value.parse().unwrap_or(0),
        "getlastmodified" => resource.entry.modified = http_date(value),
        _ => {}
    }
}

#[cfg(test)]
#[path = "webdav_xml_tests.rs"]
mod tests;
