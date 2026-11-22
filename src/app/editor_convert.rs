//! Shift+F2's conversion (R7 review D5; FAR's `SaveFile` with another code page, signature and line
//! breaks): the document's bytes decoded in its code page as they are written, its signature left out,
//! its line breaks made one kind when asked, then written in the code page chosen after its signature.

use std::io::{self, Write};

use crate::jobs::find::text::encode_text;

/// What Shift+F2 asked to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Convert {
    /// The document's code page now.
    pub from: u32,
    /// Its signature's bytes (the view's floor), left out.
    pub floor: u64,
    /// The code page written.
    pub page: u32,
    /// FAR's "Add signature (BOM)".
    pub bom: bool,
    /// FAR's "Change line breaks to": none keeps each line's.
    pub eol: Option<&'static str>,
}

/// The signature of `page` (none for a page that has none).
pub fn signature(page: u32) -> &'static [u8] {
    match page {
        65001 => &[0xEF, 0xBB, 0xBF],
        1200 => &[0xFF, 0xFE],
        1201 => &[0xFE, 0xFF],
        _ => &[],
    }
}

/// A writer converting what goes through it as `Convert` says.
pub struct Converter<'a> {
    out: &'a mut dyn Write,
    convert: Convert,
    from: crate::editor::view::Encoding,
    /// Bytes of a character not whole yet.
    pending: Vec<u8>,
    /// The signature's bytes still to leave out.
    skip: u64,
    /// The last character was a `\r` turned into the line break asked.
    after_cr: bool,
}

impl<'a> Converter<'a> {
    /// The signature asked written first.
    pub fn new(out: &'a mut dyn Write, convert: Convert) -> io::Result<Self> {
        if convert.bom {
            out.write_all(signature(convert.page))?;
        }
        Ok(Self {
            out,
            convert,
            from: super::quick_view_text::encoding_of_page(convert.from),
            pending: Vec::new(),
            skip: convert.floor,
            after_cr: false,
        })
    }

    /// What is left written (a last character cut short as it decodes).
    pub fn finish(mut self) -> io::Result<()> {
        self.drain(true)?;
        self.out.flush()
    }

    /// The whole characters held decoded, converted and written.
    fn drain(&mut self, end: bool) -> io::Result<()> {
        let (mut text, mut at) = (String::new(), 0);
        while at < self.pending.len() {
            let Some((ch, len)) = self.from.decode(&self.pending[at..], !end) else {
                break;
            };
            at += len.max(1);
            self.push(&mut text, ch);
        }
        self.pending.drain(..at);
        self.out.write_all(&encode_text(self.convert.page, &text))
    }

    fn push(&mut self, text: &mut String, ch: char) {
        let Some(eol) = self.convert.eol else {
            return text.push(ch);
        };
        match ch {
            '\r' => {
                text.push_str(eol);
                self.after_cr = true;
                return;
            }
            '\n' if self.after_cr => {}
            '\n' => text.push_str(eol),
            _ => text.push(ch),
        }
        self.after_cr = false;
    }
}

impl Write for Converter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let skipped = usize::try_from(self.skip)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        self.skip -= skipped as u64;
        self.pending.extend_from_slice(&bytes[skipped..]);
        self.drain(false)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

#[cfg(test)]
#[path = "editor_convert_tests.rs"]
mod tests;
