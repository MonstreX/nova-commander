//! FAR's special symbols of the user menu, file associations and Apply command (`fnparse.cpp`,
//! `SubstFileName`): `!.!` the file's name, `!\` the folder, `!&` the selected files, `!@!` a file
//! listing them, `!?title?init!` what the user types, `!#` / `!^` / `![` / `!]` the panel the next ones
//! speak of, and the rest of FAR's table. Short (8.3) names are the long ones here.

use std::io;

/// What the symbols speak of in one panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelFacts {
    /// The folder, as the system writes it.
    pub dir: String,
    /// The name under the cursor.
    pub current: String,
    /// The selected names, or the one under the cursor.
    pub selected: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    pub active: PanelFacts,
    pub passive: PanelFacts,
    pub active_is_left: bool,
}

/// A question `!?title?init!` asks before the command runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub title: String,
    pub init: String,
    /// `$history$` before the title: its history's name, and `%history` the answer in the line.
    pub history: Option<String>,
}

impl Prompt {
    /// FAR's history of question `index`: its own name, else `UserVar` and the index from 0.
    pub fn history_name(&self, index: usize) -> String {
        (self.history.clone()).unwrap_or_else(|| format!("UserVar{index}"))
    }
}

/// A command with its symbols replaced, but for the answers to its questions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expanded {
    pieces: Vec<Piece>,
    pub prompts: Vec<Prompt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    Answer(usize),
}

impl Expanded {
    /// The command with `answers` in place of its questions, and of FAR's `%UserVarN` (N from 1) and
    /// `%history` of a named one anywhere in it, whatever their case.
    pub fn fill(&self, answers: &[String]) -> String {
        let mut text: String = (self.pieces.iter())
            .map(|piece| match piece {
                Piece::Text(text) => text.as_str(),
                Piece::Answer(index) => answers.get(*index).map_or("", String::as_str),
            })
            .collect();
        for (index, prompt) in self.prompts.iter().enumerate() {
            let answer = answers.get(index).map_or("", String::as_str);
            text = replace_any_case(&text, &format!("%UserVar{}", index + 1), answer);
            // FAR: a history named as its own `UserVar` of this question replaces nothing more.
            if let Some(name) = prompt
                .history
                .as_ref()
                .filter(|name| **name != format!("UserVar{index}"))
            {
                text = replace_any_case(&text, &format!("%{name}"), answer);
            }
        }
        text
    }
}

/// `text` with each `what` in it, whatever its case, made `with` (FAR's `replace_icase`).
fn replace_any_case(text: &str, what: &str, with: &str) -> String {
    let lower = text.to_lowercase();
    let what = what.to_lowercase();
    // A character whose lower case is longer leaves the places unknown: nothing is replaced then.
    if what.is_empty() || lower.len() != text.len() {
        return text.to_string();
    }
    let mut out = String::new();
    let mut from = 0;
    while let Some(at) = lower[from..].find(&what) {
        out.push_str(&text[from..from + at]);
        out.push_str(with);
        from += at + what.len();
    }
    out.push_str(&text[from..]);
    out
}

/// Makes the file `!@!` names: the panel's names and the modifiers written between `@` and `!`.
pub type ListFile<'a> = &'a mut dyn FnMut(&PanelFacts, &str) -> io::Result<String>;

/// FAR's `name_ext`: the name before its last dot, and what follows it.
fn name_and_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

/// `"name"`; `quote_always` off: only names with a space.
fn quoted(name: &str, quote_always: bool) -> String {
    if quote_always || name.contains(' ') {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

fn with_end_slash(dir: &str) -> String {
    if dir.ends_with(std::path::is_separator) {
        dir.to_string()
    } else {
        format!("{dir}{}", std::path::MAIN_SEPARATOR)
    }
}

/// FAR's `ProcessBrackets`: the length up to and with `end` outside brackets, 0 if it is not there.
fn up_to(text: &str, end: char) -> usize {
    let mut depth = 0usize;
    for (at, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 && ch == end => return at + ch.len_utf8(),
            _ => {}
        }
    }
    0
}

/// FAR's `SkipInputToken` on what follows `!?`: the title, the initial text and the length taken.
fn input_token(tail: &str) -> Option<(&str, &str, usize)> {
    let title_size = up_to(tail, '?');
    if title_size == 0 {
        return None;
    }
    let rest = &tail[title_size..];
    let text_size = up_to(rest, '!');
    if text_size == 0 {
        return None;
    }
    Some((
        &tail[..title_size - 1],
        &rest[..text_size - 1],
        title_size + text_size,
    ))
}

struct Expander<'a, 'b> {
    facts: &'a Facts,
    passive: bool,
    list_file: Option<ListFile<'b>>,
}

impl Expander<'_, '_> {
    fn panel(&self) -> &PanelFacts {
        if self.passive {
            &self.facts.passive
        } else {
            &self.facts.active
        }
    }

    /// The selected names, each quoted (`Q`, the default) or only where it holds a space (`q`).
    fn names(&self, tail: &str) -> (String, usize) {
        let (always, taken) = match tail.chars().next() {
            Some('Q') => (true, 1),
            Some('q') => (false, 1),
            _ => (true, 0),
        };
        let names: Vec<String> = (self.panel().selected.iter())
            .map(|name| quoted(name, always))
            .collect();
        (names.join(" "), taken)
    }

    /// One symbol at the start of `text` (which begins with `!`): what it stands for and its length,
    /// in FAR's order.
    fn symbol(&mut self, text: &str) -> io::Result<(String, usize)> {
        if let Some(length) = self.switch(text) {
            return Ok((String::new(), length));
        }
        if let Some(found) = self.name_symbol(text) {
            return Ok(found);
        }
        if let Some(found) = self.names_symbol(text) {
            return Ok(found);
        }
        if let Some(found) = self.list(text)? {
            return Ok(found);
        }
        if let Some(found) = self.place_symbol(text) {
            return Ok(found);
        }
        let current = self.panel().current.clone();
        Ok((name_and_extension(&current).0.to_string(), 1))
    }

    /// `!#`, `!^`, `![`, `!]`: the panel the next symbols speak of.
    fn switch(&mut self, text: &str) -> Option<usize> {
        let passive = match text.get(..2)? {
            "!#" => true,
            "!^" => false,
            "![" => !self.facts.active_is_left,
            "!]" => self.facts.active_is_left,
            _ => return None,
        };
        self.passive = passive;
        Some(2)
    }

    /// The name under the cursor and its parts.
    fn name_symbol(&self, text: &str) -> Option<(String, usize)> {
        let not_input = |length: usize| !text[length..].starts_with('?');
        let current = self.panel().current.clone();
        let (name, extension) = name_and_extension(&current);
        let (name, extension) = (name.to_string(), extension.to_string());
        if text.starts_with("!!") && not_input(2) {
            Some(("!".to_string(), 2))
        } else if text.starts_with("!.!") && not_input(3) {
            Some((current, 3))
        } else if text.starts_with("!~") {
            Some((name, 2))
        } else if text.starts_with("!`~") {
            Some((extension, 3))
        } else if text.starts_with("!`") {
            Some((extension, 2))
        } else if (text.starts_with("!-!") || text.starts_with("!+!")) && not_input(3) {
            Some((current, 3))
        } else {
            None
        }
    }

    /// `!&~` and `!&`: the selected names.
    fn names_symbol(&self, text: &str) -> Option<(String, usize)> {
        let (tail, length) = match text.strip_prefix("!&~") {
            Some(tail) => (tail, 3),
            None => (text.strip_prefix("!&")?, 2),
        };
        if tail.starts_with('?') {
            return None;
        }
        let (names, taken) = self.names(tail);
        Some((names, length + taken))
    }

    /// The drive, the folder, the description.
    fn place_symbol(&self, text: &str) -> Option<(String, usize)> {
        let dir = &self.panel().dir;
        if text.starts_with("!:") {
            Some((crate::platform::files::paths::root_name(dir), 2))
        } else if text.starts_with("!?!") {
            // Descriptions of files are not kept.
            Some((String::new(), 3))
        } else if text.starts_with("!=\\") || text.starts_with("!=/") {
            Some((with_end_slash(dir), 3))
        } else if text.starts_with("!\\") || text.starts_with("!/") {
            Some((with_end_slash(dir), 2))
        } else {
            None
        }
    }

    /// `!@modifiers!` and `!$modifiers!`: the name of a file listing the selected names.
    fn list(&mut self, text: &str) -> io::Result<Option<(String, usize)>> {
        let Some(tail) = text.strip_prefix("!@").or_else(|| text.strip_prefix("!$")) else {
            return Ok(None);
        };
        let Some(end) = tail
            .find('!')
            .filter(|&end| !tail[end + 1..].starts_with('?'))
        else {
            return Ok(None);
        };
        let modifiers = &tail[..end];
        let length = 2 + end + 1;
        let panel = self.panel().clone();
        let name = match &mut self.list_file {
            Some(make) => make(&panel, modifiers)?,
            // A label shows it as it is (FAR's note 3).
            None => text[..length].to_string(),
        };
        Ok(Some((name, length)))
    }

    /// The symbols of a title or an initial text: only those in brackets are replaced (FAR's
    /// `process_subexpression`), the brackets going when something in them was replaced.
    fn subexpression(&mut self, text: &str) -> io::Result<String> {
        let Some(open) = text.find('(') else {
            return Ok(text.to_string());
        };
        let close_size = up_to(&text[open + 1..], ')');
        if close_size == 0 {
            return Ok(text.to_string());
        }
        let inner = &text[open + 1..open + close_size];
        let mut expanded = Expanded::default();
        self.expand_into(inner, &mut expanded, false)?;
        let replaced = expanded.fill(&[]);
        // Brackets with nothing to replace in them stay as they are.
        if replaced == inner {
            return Ok(text.to_string());
        }
        Ok(format!(
            "{}{replaced}{}",
            &text[..open],
            &text[open + 1 + close_size..]
        ))
    }

    fn expand_into(&mut self, text: &str, out: &mut Expanded, inputs: bool) -> io::Result<()> {
        let mut plain = String::new();
        let mut rest = text;
        while let Some(at) = rest.find('!') {
            plain.push_str(&rest[..at]);
            rest = &rest[at..];
            // `!?!`, the description, before a question (FAR's order).
            let input = Some(rest)
                .filter(|rest| !rest.starts_with("!?!"))
                .and_then(|rest| rest.strip_prefix("!?"));
            if input.is_some_and(|tail| input_token(tail).is_none()) {
                // A question written wrong: its `!` as it is (FAR skips one character).
                plain.push('!');
                rest = &rest[1..];
                continue;
            }
            let input = input.and_then(input_token);
            if let Some((_, _, length)) = input.filter(|_| !inputs) {
                // A label keeps its questions as they are.
                plain.push_str(&rest[..2 + length]);
                rest = &rest[2 + length..];
                continue;
            }
            if let Some((title, init, length)) = input {
                // `$history$` before a title names its history.
                let named = (title.strip_prefix('$')).and_then(|named| named.split_once('$'));
                let title = named.map_or(title, |(_, title)| title);
                let prompt = Prompt {
                    title: self.subexpression(title)?,
                    init: self.subexpression(init)?,
                    // `$$` names none (FAR's empty `strHistory`).
                    history: (named.map(|(history, _)| history.to_string()))
                        .filter(|history| !history.is_empty()),
                };
                out.pieces.push(Piece::Text(std::mem::take(&mut plain)));
                out.pieces.push(Piece::Answer(out.prompts.len()));
                out.prompts.push(prompt);
                rest = &rest[2 + length..];
                continue;
            }
            let (value, length) = self.symbol(rest)?;
            plain.push_str(&value);
            rest = &rest[length..];
        }
        plain.push_str(rest);
        out.pieces.push(Piece::Text(plain));
        Ok(())
    }
}

/// A command of the menu: its symbols replaced, its questions to ask; `list_file` makes `!@!`'s file.
pub fn expand_command(command: &str, facts: &Facts, list_file: ListFile) -> io::Result<Expanded> {
    let mut expander = Expander {
        facts,
        passive: false,
        list_file: Some(list_file),
    };
    let mut expanded = Expanded::default();
    expander.expand_into(command, &mut expanded, true)?;
    Ok(expanded)
}

/// A label of the menu: its symbols replaced, its questions and list files left as they are.
pub fn expand_label(label: &str, facts: &Facts) -> String {
    let mut expander = Expander {
        facts,
        passive: false,
        list_file: None,
    };
    let mut expanded = Expanded::default();
    match expander.expand_into(label, &mut expanded, false) {
        Ok(()) => expanded.fill(&[]),
        Err(_) => label.to_string(),
    }
}

#[cfg(test)]
#[path = "metasymbols_tests.rs"]
mod tests;
