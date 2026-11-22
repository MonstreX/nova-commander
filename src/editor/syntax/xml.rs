//! Colorer's XML (`xml/XMLNode`, `LibXmlReader`): a file read into elements with their attributes, their
//! children and their text (the first CDATA section or the first text that is not blank, trimmed), the
//! DTD's entities put in first - `<!ENTITY name SYSTEM "file">` includes a file beside it, `<!ENTITY name
//! "text">` a text - as libxml2 does with `XML_PARSE_NOENT`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;

/// One element (`XMLNode`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Element {
    /// Its local name (no namespace prefix).
    pub name: String,
    pub text: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
}

impl Element {
    /// An attribute's value.
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// An attribute's value, empty when it is not there (`getAttrValue`).
    pub fn value(&self, key: &str) -> &str {
        self.attr(key).unwrap_or("")
    }
}

/// A DTD entity: its text or the file it includes.
#[derive(Clone, Debug)]
enum Entity {
    Text(String),
    File(PathBuf),
}

/// Includes go no deeper than this.
const DEPTH: usize = 8;

/// The file's root element.
pub fn read(path: &Path) -> Result<Element, String> {
    let source = read_text(path)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    parse_text(&source, dir).map_err(|error| format!("{}: {error}", path.display()))
}

fn read_text(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
}

/// XML text into its root element, includes read from `dir`.
pub fn parse_text(source: &str, dir: &Path) -> Result<Element, String> {
    let (body, entities) = split_doctype(source, dir);
    let expanded = expand(&body, &entities, 0);
    build(&expanded)
}

/// The document without its DOCTYPE and the entities the DOCTYPE declares.
fn split_doctype(source: &str, dir: &Path) -> (String, HashMap<String, Entity>) {
    let Some(start) = source.find("<!DOCTYPE") else {
        return (source.to_string(), HashMap::new());
    };
    let rest = &source[start..];
    let (subset, end) = match (rest.find('['), rest.find('>')) {
        (Some(open), Some(close)) if open < close => {
            let close = subset_end(rest, open + 1).unwrap_or(rest.len());
            (
                &rest[open + 1..close],
                rest[close..]
                    .find('>')
                    .map_or(rest.len(), |at| close + at + 1),
            )
        }
        (_, Some(close)) => ("", close + 1),
        _ => ("", rest.len()),
    };
    let entities = declarations(subset, dir);
    let body = format!("{}{}", &source[..start], &rest[end..]);
    (body, entities)
}

/// Where the internal subset's `]` is, past quoted texts and comments.
fn subset_end(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b']' => return Some(i),
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += text[i + 1..]
                    .find(quote as char)
                    .map_or(bytes.len(), |at| at + 1);
            }
            b'<' if text[i..].starts_with("<!--") => {
                i += text[i..].find("-->").map_or(bytes.len(), |at| at + 2);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// `<!ENTITY ...>` declarations of a subset (comments and parameter entities skipped).
fn declarations(subset: &str, dir: &Path) -> HashMap<String, Entity> {
    let mut entities = HashMap::new();
    let mut rest = subset;
    while let Some(at) = rest.find("<!") {
        rest = &rest[at..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |end| &rest[end + 3..]);
            continue;
        }
        let end = rest.find('>').unwrap_or(rest.len());
        if let Some(body) = rest[..end].strip_prefix("<!ENTITY")
            && let Some((name, entity)) = declaration(body, dir)
        {
            entities.entry(name).or_insert(entity);
        }
        rest = rest.get(end + 1..).unwrap_or("");
    }
    entities
}

fn declaration(body: &str, dir: &Path) -> Option<(String, Entity)> {
    let mut words = body.trim_start();
    if words.starts_with('%') {
        return None;
    }
    let name_end = words.find(char::is_whitespace)?;
    let name = words[..name_end].to_string();
    words = words[name_end..].trim_start();
    // XML's quotes only, cut by their own length (review B6: another first character panicked).
    let quoted = |text: &str| {
        let quote = text
            .chars()
            .next()
            .filter(|quote| matches!(quote, '"' | '\''))?;
        let text = &text[quote.len_utf8()..];
        let end = text.find(quote)?;
        Some((
            text[..end].to_string(),
            text[end + quote.len_utf8()..].trim_start().to_string(),
        ))
    };
    if let Some(rest) = words.strip_prefix("SYSTEM") {
        let (file, _) = quoted(rest.trim_start())?;
        return Some((name, Entity::File(dir.join(file))));
    }
    if let Some(rest) = words.strip_prefix("PUBLIC") {
        let (_, rest) = quoted(rest.trim_start())?;
        let (file, _) = quoted(&rest)?;
        return Some((name, Entity::File(dir.join(file))));
    }
    let (text, _) = quoted(words)?;
    Some((name, Entity::Text(text)))
}

/// `&name;` of the declared entities put in, past comments and CDATA sections.
fn expand(text: &str, entities: &HashMap<String, Entity>, depth: usize) -> String {
    if entities.is_empty() || depth > DEPTH {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(['&', '<']) {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let skip = [("<!--", "-->"), ("<![CDATA[", "]]>")]
            .iter()
            .find(|(open, _)| rest.starts_with(open));
        if let Some((_, close)) = skip {
            let end = rest.find(close).map_or(rest.len(), |end| end + close.len());
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }
        let reference = rest.strip_prefix('&').and_then(|after| {
            let end = after.find(';')?;
            let name = &after[..end];
            entities.get(name).map(|entity| (entity, end + 2))
        });
        match reference {
            Some((entity, len)) => {
                out.push_str(&replacement(entity, entities, depth));
                rest = &rest[len..];
            }
            None => {
                out.push_str(&rest[..1]);
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// An entity's text, a file's without its XML declaration, its own references put in.
fn replacement(entity: &Entity, entities: &HashMap<String, Entity>, depth: usize) -> String {
    let text = match entity {
        Entity::Text(text) => text.clone(),
        Entity::File(path) => {
            let Ok(text) = read_text(path) else {
                return String::new();
            };
            match text.trim_start().strip_prefix("<?xml") {
                Some(rest) => rest
                    .find("?>")
                    .map_or(String::new(), |end| rest[end + 2..].to_string()),
                None => text,
            }
        }
    };
    expand(&text, entities, depth + 1)
}

/// An element being read: its parts of text, a CDATA section or a text between its children.
#[derive(Default)]
struct Open {
    element: Element,
    found: Option<String>,
    text: String,
}

impl Open {
    /// The text read since the last child closed as one text node.
    fn end_text(&mut self) {
        let text = std::mem::take(&mut self.text);
        if self.found.is_none() && !text.trim().is_empty() {
            self.found = Some(text.trim().to_string());
        }
    }

    fn close(mut self) -> Element {
        self.end_text();
        self.element.text = self.found.unwrap_or_default();
        self.element
    }
}

fn open(start: &quick_xml::events::BytesStart) -> Result<Open, String> {
    let name = start.local_name().as_ref().to_string();
    let mut attrs = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        let key = attr.key.local_name().as_ref().to_string();
        let value = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| e.to_string())?
            .into_owned();
        attrs.push((key, value));
    }
    Ok(Open {
        element: Element {
            name,
            attrs,
            ..Element::default()
        },
        ..Open::default()
    })
}

/// The elements of an XML text (its entities put in already).
fn build(text: &str) -> Result<Element, String> {
    let mut reader = quick_xml::Reader::from_str(text);
    let mut stack: Vec<Open> = Vec::new();
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        match event {
            Event::Start(start) => {
                if let Some(parent) = stack.last_mut() {
                    parent.end_text();
                }
                stack.push(open(&start)?);
            }
            Event::Empty(start) => {
                let element = open(&start)?.close();
                match stack.last_mut() {
                    Some(parent) => {
                        parent.end_text();
                        parent.element.children.push(element);
                    }
                    None => return Ok(element),
                }
            }
            Event::End(_) => {
                let element = stack.pop().ok_or("an end without its start")?.close();
                match stack.last_mut() {
                    Some(parent) => parent.element.children.push(element),
                    None => return Ok(element),
                }
            }
            Event::Text(text) => add_text(&mut stack, &text.xml10_content()),
            Event::GeneralRef(reference) => {
                let raw = format!("&{};", reference.xml10_content());
                let resolved =
                    quick_xml::escape::unescape(&raw).map_or(raw.clone(), |c| c.into_owned());
                add_text(&mut stack, &resolved);
            }
            Event::CData(data) => {
                if let Some(top) = stack.last_mut() {
                    top.end_text();
                    if top.found.is_none() {
                        top.found = Some(data.xml10_content().into_owned());
                    }
                }
            }
            Event::Comment(_) | Event::PI(_) => {
                if let Some(top) = stack.last_mut() {
                    top.end_text();
                }
            }
            Event::Eof => return Err("no root element".to_string()),
            _ => {}
        }
    }
}

fn add_text(stack: &mut [Open], text: &str) {
    if let Some(top) = stack.last_mut() {
        top.text.push_str(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review B6: an entity's value in a quote that is no XML quote (a schema written wrong) is no
    /// entity - no panic on its first character's bytes.
    #[test]
    fn an_entity_in_odd_quotes_is_no_entity() {
        let dir = Path::new("hrc");
        assert!(declaration("a «x»", dir).is_none());
        assert!(declaration("a SYSTEM «x»", dir).is_none());
        assert_eq!(
            declaration("a 'it's'", dir).map(|(name, _)| name),
            Some("a".to_string())
        );
        match declaration("b \"ä\"", dir) {
            Some((_, Entity::Text(text))) => assert_eq!(text, "ä"),
            other => panic!("{other:?}"),
        }
    }
}
