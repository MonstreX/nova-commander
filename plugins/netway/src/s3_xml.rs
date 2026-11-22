//! S3's XML answers as WinSCP's `TS3FileSystem` reads them through libs3: `ListAllMyBucketsResult` (the
//! buckets: the root's folders), `ListBucketResult` of ListObjectsV2 with a delimiter (the objects under a
//! prefix: files, their common prefixes: folders; `IsTruncated` / `NextContinuationToken` for the next
//! page), and `Error`'s code and message. Times are ISO 8601 in UTC.

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::scp_listing::seconds_of;

/// An object or a common prefix under the prefix listed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Object {
    /// The key (a prefix ends with `/`).
    pub key: String,
    pub size: u64,
    pub modified: Option<u32>,
    pub prefix: bool,
}

/// A page of a listing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    pub objects: Vec<Object>,
    /// The next page's token when the listing goes on.
    pub next: Option<String>,
}

/// ISO 8601's `2024-03-10T10:30:45.000Z`.
pub fn iso_time(text: &str) -> Option<u32> {
    let (date, time) = text.trim().split_once('T')?;
    let mut date = date.split('-').map(|part| part.parse::<i64>().ok());
    let date = (date.next()??, date.next()??, date.next()??);
    let time = time.trim_end_matches('Z');
    let time = time.split('.').next()?;
    let mut clock = time.split(':').map(|part| part.parse::<i64>().ok());
    let clock = (clock.next()??, clock.next()??, clock.next()??);
    u32::try_from(seconds_of(date, clock)).ok()
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or_default()
}

/// The elements' texts with the names of the element around them (`Contents` for `Key`), in order; each
/// record (`Bucket`, `Contents`, `CommonPrefixes`, `Error`) opened by its own name.
fn texts(xml: &str) -> Result<Vec<(String, String, String)>, String> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<String> = Vec::new();
    let mut found = Vec::new();
    let mut text = String::new();
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(start) => {
                stack.push(local(start.name().as_ref()).to_string());
                text.clear();
            }
            Event::Text(part) => text.push_str(&part.into_inner()),
            Event::GeneralRef(reference) => text.push_str(&match reference.into_inner().as_ref() {
                "amp" => "&".to_string(),
                "lt" => "<".to_string(),
                "gt" => ">".to_string(),
                "quot" => "\"".to_string(),
                "apos" => "'".to_string(),
                other => format!("&{other};"),
            }),
            Event::End(_) => {
                let name = stack.pop().unwrap_or_default();
                let parent = stack.last().cloned().unwrap_or_default();
                found.push((parent, name, std::mem::take(&mut text)));
            }
            Event::Eof => return Ok(found),
            _ => {}
        }
    }
}

/// The buckets' names with their creation times.
pub fn buckets(xml: &str) -> Result<Vec<Object>, String> {
    let mut buckets: Vec<Object> = Vec::new();
    for (parent, name, text) in texts(xml)? {
        match (parent.as_str(), name.as_str()) {
            ("Bucket", "Name") => buckets.push(Object {
                key: format!("{text}/"),
                prefix: true,
                ..Object::default()
            }),
            ("Bucket", "CreationDate") => {
                if let Some(bucket) = buckets.last_mut() {
                    bucket.modified = iso_time(&text);
                }
            }
            _ => {}
        }
    }
    Ok(buckets)
}

/// A ListObjectsV2 page.
pub fn objects(xml: &str) -> Result<Page, String> {
    let mut page = Page::default();
    let mut truncated = false;
    let mut current = Object::default();
    for (parent, name, text) in texts(xml)? {
        match (parent.as_str(), name.as_str()) {
            ("Contents", "Key") => current.key = text,
            ("Contents", "Size") => current.size = text.trim().parse().unwrap_or(0),
            ("Contents", "LastModified") => current.modified = iso_time(&text),
            ("CommonPrefixes", "Prefix") => page.objects.push(Object {
                key: text,
                prefix: true,
                ..Object::default()
            }),
            (_, "Contents") => page.objects.push(std::mem::take(&mut current)),
            (_, "IsTruncated") => truncated = text.trim() == "true",
            (_, "NextContinuationToken") => page.next = Some(text),
            _ => {}
        }
    }
    if !truncated {
        page.next = None;
    }
    Ok(page)
}

/// CreateMultipartUpload's `UploadId`.
pub fn upload_id(xml: &str) -> Option<String> {
    let found = texts(xml).ok()?;
    (found.into_iter())
        .find(|(parent, name, _)| parent == "InitiateMultipartUploadResult" && name == "UploadId")
        .map(|(_, _, text)| text)
        .filter(|id| !id.is_empty())
}

/// The `Error`'s element `wanted`.
fn error_field(xml: &str, wanted: &str) -> Option<String> {
    let found = texts(xml).ok()?;
    (found.into_iter())
        .find(|(parent, name, _)| parent == "Error" && name == wanted)
        .map(|(_, _, text)| text)
}

/// The region an `Error` says the bucket is in (`AuthorizationHeaderMalformed`'s, `PermanentRedirect`'s).
pub fn error_region(xml: &str) -> Option<String> {
    error_field(xml, "Region").filter(|region| !region.trim().is_empty())
}

/// An `Error`'s code and message ("NoSuchKey: The specified key does not exist.").
pub fn error(xml: &str) -> Option<String> {
    let field = |wanted: &str| error_field(xml, wanted);
    let code = field("Code")?;
    Some(match field("Message") {
        Some(message) => format!("{code}: {message}"),
        None => code,
    })
}

#[cfg(test)]
#[path = "s3_xml_tests.rs"]
mod tests;
