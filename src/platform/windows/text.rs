//! FAR's fuzzy folding of Find file (`fuzzy_searcher_impl::normalize`: ligatures expanded,
//! characters decomposed, digits and compatibility forms folded, the marks left alone dropped).

use windows_sys::Win32::Globalization::{
    C3_ALPHA, C3_DIACRITIC, C3_LEXICAL, C3_NONSPACING, C3_VOWELMARK, CT_CTYPE3, FoldStringW,
    GetStringTypeW, MAP_COMPOSITE, MAP_EXPAND_LIGATURES, MAP_FOLDCZONE, MAP_FOLDDIGITS,
};

/// FAR's `normalize` of its fuzzy search: "Ĝŕžèģōŗż" is "Grzegorz", "ß" "ss", "①" "1"; `text` as it is
/// when the system cannot fold it.
pub fn fuzzy_fold(text: &str) -> String {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return String::new();
    }
    let Some(mut expanded) = fold(&wide, MAP_EXPAND_LIGATURES) else {
        return text.to_string();
    };
    // FAR: old systems do not decompose trailing marks without something after them.
    expanded.push(0);
    let Some(mut folded) = fold(&expanded, MAP_COMPOSITE | MAP_FOLDCZONE | MAP_FOLDDIGITS) else {
        return text.to_string();
    };
    if folded.last() == Some(&0) {
        folded.pop();
    }
    let Some(types) = character_types(&folded) else {
        return text.to_string();
    };
    let kept: Vec<u16> = (folded.iter().zip(&types))
        .filter(|&(_, &kind)| !is_lone_mark(kind))
        .map(|(&unit, _)| unit)
        .collect();
    String::from_utf16_lossy(&kept)
}

/// A mark that is no letter of its own (FAR's `C3_NONSPACING | C3_DIACRITIC | C3_VOWELMARK` without
/// `C3_ALPHA | C3_LEXICAL`).
fn is_lone_mark(kind: u16) -> bool {
    let kind = u32::from(kind);
    kind & (C3_ALPHA | C3_LEXICAL) == 0 && kind & (C3_NONSPACING | C3_DIACRITIC | C3_VOWELMARK) != 0
}

fn fold(source: &[u16], flags: u32) -> Option<Vec<u16>> {
    let length = i32::try_from(source.len()).ok()?;
    // SAFETY: `source` holds `length` units; no buffer is passed, only the size is asked.
    let size = unsafe { FoldStringW(flags, source.as_ptr(), length, std::ptr::null_mut(), 0) };
    let room = usize::try_from(size).ok().filter(|&room| room > 0)?;
    let mut folded = vec![0u16; room];
    // SAFETY: `source` holds `length` units, `folded` room for `size` units.
    let written = unsafe { FoldStringW(flags, source.as_ptr(), length, folded.as_mut_ptr(), size) };
    folded.truncate(
        usize::try_from(written)
            .ok()
            .filter(|&written| written > 0)?,
    );
    Some(folded)
}

fn character_types(text: &[u16]) -> Option<Vec<u16>> {
    let length = i32::try_from(text.len()).ok()?;
    let mut types = vec![0u16; text.len()];
    // SAFETY: `text` holds `length` units and `types` as many.
    let read = unsafe { GetStringTypeW(CT_CTYPE3, text.as_ptr(), length, types.as_mut_ptr()) };
    (read != 0).then_some(types)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_folding_is_fars() {
        assert_eq!(fuzzy_fold("Ĝŕžèģōŗż"), "Grzegorz");
        assert_eq!(fuzzy_fold("їёй"), "іеи");
        assert_eq!(fuzzy_fold("ßß"), "ssss");
        assert_eq!(fuzzy_fold("⓪①②"), "012");
        assert_eq!(fuzzy_fold(""), "");
    }
}
