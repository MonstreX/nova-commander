//! What a call hands the host, kept until the plugin's next call (ADR-0016 §7): texts and lists of them.

use crate::api::NcStr;

/// What a call hands the host, kept until the plugin's next call: texts and lists of them.
#[derive(Default)]
pub(crate) struct Kept {
    texts: Vec<String>,
    /// Boxed: a list stays where it is while more are kept.
    lists: Vec<Box<[NcStr]>>,
}

impl Kept {
    pub(crate) fn clear(&mut self) {
        self.texts.clear();
        self.lists.clear();
    }

    /// `text` as the host reads it (its bytes stay where they are while more are kept).
    pub(crate) fn text(&mut self, text: String) -> NcStr {
        self.texts.push(text);
        (self.texts.last()).map_or(NcStr::EMPTY, |text| NcStr::new(text))
    }

    /// `texts` as the host reads a list of them: where it is and how many (none: null).
    pub(crate) fn list(&mut self, texts: Vec<String>) -> (*const NcStr, usize) {
        self.optional_list(texts.into_iter().map(Some).collect())
    }

    /// As [`Kept::list`], a `None` handed as a null text (an empty one is not null: the host tells them
    /// apart).
    pub(crate) fn optional_list(&mut self, texts: Vec<Option<String>>) -> (*const NcStr, usize) {
        if texts.is_empty() {
            return (std::ptr::null(), 0);
        }
        let list: Box<[NcStr]> = (texts.into_iter())
            .map(|text| text.map_or(NcStr::EMPTY, |text| self.text(text)))
            .collect();
        let place = (list.as_ptr(), list.len());
        self.lists.push(list);
        place
    }

    /// How many texts are kept.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.texts.len()
    }
}
