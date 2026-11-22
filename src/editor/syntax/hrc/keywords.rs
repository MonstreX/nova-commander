//! A scheme's keyword list (`KeywordList`): sorted, without repeats, each word knowing the longest shorter
//! word it starts with, and where the words of each ASCII first character lie.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use super::super::regex::{CharClass, Mask};
use super::RegionId;

/// One word or symbol (`KeywordInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keyword {
    pub word: Vec<char>,
    pub region: Option<RegionId>,
    pub is_symbol: bool,
    /// The word before it in the list that it starts with (`indexOfShorter`).
    pub shorter: Option<usize>,
}

/// The list (`KeywordList`).
#[derive(Clone, Debug, Default)]
pub struct KeywordList {
    pub match_case: bool,
    pub has_symbols: bool,
    pub has_non_symbols: bool,
    pub min_len: usize,
    pub first_char: CharClass,
    pub first_ascii: Mask,
    pub ascii_begin: Vec<usize>,
    pub ascii_end: Vec<usize>,
    pub words: Vec<Keyword>,
}

impl KeywordList {
    pub fn new(match_case: bool) -> KeywordList {
        KeywordList {
            match_case,
            min_len: usize::MAX,
            ascii_begin: vec![0; 128],
            ascii_end: vec![0; 128],
            ..KeywordList::default()
        }
    }

    /// A word added (its first character among the first ones).
    pub fn add(&mut self, word: Vec<char>, region: Option<RegionId>, is_symbol: bool) {
        if let Some(&first) = word.first() {
            self.first_char.add(first);
        }
        self.min_len = self.min_len.min(word.len());
        self.words.push(Keyword {
            word,
            region,
            is_symbol,
            shorter: None,
        });
    }

    /// Sorted and without repeats (`sortList`).
    pub fn sort(&mut self) {
        self.words.sort_by(|a, b| {
            a.word
                .cmp(&b.word)
                .then(a.region.cmp(&b.region))
                .then(a.is_symbol.cmp(&b.is_symbol))
        });
        self.words.dedup_by(|b, a| {
            a.region == b.region && a.is_symbol == b.is_symbol && a.word == b.word
        });
    }

    /// The shorter words and the first characters' ranges (`substrIndex`).
    pub fn index(&mut self) {
        self.has_symbols = self.words.iter().any(|word| word.is_symbol);
        self.has_non_symbols = self.words.iter().any(|word| !word.is_symbol);
        self.first_ascii = [0; 2];
        self.ascii_begin = vec![0; 128];
        self.ascii_end = vec![0; 128];
        // Colorer's nearest shorter word that begins the word, found in one pass over the sorted list
        // (its own scan back is quadratic on long lists): the words before it that begin it are a chain
        // on a stack, those that do not begin the next word let go.
        let mut chain: Vec<usize> = Vec::new();
        for i in 0..self.words.len() {
            let long = &self.words[i].word;
            while chain
                .last()
                .is_some_and(|&top| !long.starts_with(&self.words[top].word))
            {
                chain.pop();
            }
            let shorter = (chain.iter().rev()).find(|&&ii| self.words[ii].word.len() < long.len());
            // An empty word begins no other (its first character is none).
            if long.is_empty() {
                self.words[i].shorter = None;
                continue;
            }
            self.words[i].shorter = shorter.copied();
            chain.push(i);
        }
        let mut i = 0;
        while i < self.words.len() {
            let first = self.words[i].word.first().copied().unwrap_or('\0');
            let mut j = i + 1;
            while j < self.words.len() && self.words[j].word.first().copied() == Some(first) {
                j += 1;
            }
            let c = first as usize;
            if c < 128 {
                self.first_ascii[c >> 6] |= 1 << (c & 63);
                self.ascii_begin[c] = i;
                self.ascii_end[c] = j;
            }
            i = j;
        }
    }

    /// Lists next to each other merged into one (`mergeKeywordNodes`): sorted stably, repeats of a word
    /// and its kind dropped.
    pub fn merged(lists: &[&KeywordList]) -> KeywordList {
        let mut merged = KeywordList::new(lists.first().is_some_and(|list| list.match_case));
        for list in lists {
            for word in &list.words {
                merged.add(word.word.clone(), word.region, word.is_symbol);
            }
        }
        merged
            .words
            .sort_by(|a, b| a.word.cmp(&b.word).then(a.is_symbol.cmp(&b.is_symbol)));
        merged
            .words
            .dedup_by(|b, a| a.is_symbol == b.is_symbol && a.word == b.word);
        merged.index();
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Colorer's `substrIndex` scan back, word by word.
    fn scanned(words: &[Keyword]) -> Vec<Option<usize>> {
        (0..words.len())
            .map(|i| {
                (0..i).rev().find(|&ii| {
                    let (long, short) = (&words[i].word, &words[ii].word);
                    let shorter = short.len() < long.len() && long.starts_with(short);
                    !short.is_empty() && shorter
                })
            })
            .collect()
    }

    #[test]
    fn the_shorter_words_are_colorers_found_in_one_pass() {
        let mut list = KeywordList::new(true);
        for (word, symbol) in [
            ("a", false),
            ("ab", false),
            ("abc", false),
            ("abd", false),
            ("abd", true),
            ("abde", false),
            ("ac", false),
            ("b", false),
            ("ba", false),
            ("bab", false),
            ("c", false),
            ("ca", false),
        ] {
            list.add(word.chars().collect(), None, symbol);
        }
        list.sort();
        list.index();
        let shorter: Vec<Option<usize>> = list.words.iter().map(|word| word.shorter).collect();
        assert_eq!(shorter, scanned(&list.words));
        assert_eq!(
            shorter[5],
            Some(4),
            "abde: abd (the symbol one), the nearest that begins it"
        );
    }
}
