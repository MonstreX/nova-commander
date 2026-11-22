//! Find file's "Using code page" list (FAR's `codepages::FillCodePagesList` with Find file's flags,
//! `far/codepage_selection.cpp`, and `FindFiles::MainDlgProc`): "Automatic detection", "All standard
//! code pages" ("Selected code pages" once one is marked), then under "System" the ANSI and OEM code
//! pages, under "Unicode" UTF-8 and UTF-16, under "Other" the rest the system has by their numbers, each
//! line FAR's `FormatCodePageString` ("1251 │ ANSI - Cyrillic"). Space / Ins mark a code page; "All"
//! reads the marked ones, or with none marked FAR's standard five (`InitInFileSearch`). The marks and the
//! choice are saved (FAR's `CodePages.Favorites` with `CPST_FIND`, `FindCodePage`). The favourites
//! of the viewer's and the editor's code page menu stand under "Favorites" before "Other", each code
//! page by the user's name for it when there is one (`*` before it).

use crate::app::code_page_menu::{name_of, page_text, system_name};
use crate::app::form::ListLine;
use crate::jobs::find::text::{CodePages, UTF8, UTF16_BE, UTF16_LE};
use crate::platform::locale::{ansi_code_page, installed_code_pages, oem_code_page};
use crate::settings::CodePageSettings;

/// The list's line of "All standard code pages" (FAR's `TitlePosition`).
const ALL_LINE: usize = 1;
/// UTF-7: FAR's `IsUtfCodePage` keeps it out of "Other".
const UTF7: u32 = 65000;

/// The code page Find file reads files in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum CodePageChoice {
    /// FAR's `CP_DEFAULT`.
    #[default]
    Auto,
    /// FAR's `CP_ALL`.
    All,
    Page(u32),
}

impl CodePageChoice {
    /// As saved: `auto`, `all` or the number.
    pub(super) fn id(self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::All => "all".to_string(),
            Self::Page(id) => id.to_string(),
        }
    }

    pub(super) fn from_id(id: &str) -> Self {
        match id {
            "all" => Self::All,
            id => id.parse().map_or(Self::Auto, Self::Page),
        }
    }

    /// The code page Text and Hex turn one into the other through (FAR's `ConvertHexString`: ANSI for
    /// the automatic and the "All").
    pub(super) fn conversion_page(self) -> u32 {
        match self {
            Self::Page(id) => id,
            Self::Auto | Self::All => ansi_code_page(),
        }
    }

    /// What the files are read in (FAR's `InitInFileSearch`).
    pub(super) fn code_pages(self, marked: &[u32]) -> CodePages {
        match self {
            Self::Auto => CodePages::Auto,
            Self::Page(id) => CodePages::Pages(vec![id]),
            Self::All if !marked.is_empty() => CodePages::Pages(marked.to_vec()),
            Self::All => {
                let mut pages: Vec<u32> = Vec::new();
                for page in standard_pages() {
                    if !pages.contains(&page) {
                        pages.push(page);
                    }
                }
                CodePages::Pages(pages)
            }
        }
    }
}

/// FAR's standard code pages in the order of its list.
fn standard_pages() -> [u32; 5] {
    [ansi_code_page(), oem_code_page(), UTF8, UTF16_LE, UTF16_BE]
}

/// FAR's `MFindFileAllCodePages` / `MFindFileSelectedCodePages` (`SetAllCpTitle`).
fn all_title(any_marked: bool) -> &'static str {
    if any_marked {
        "Selected code pages"
    } else {
        "All standard code pages"
    }
}

/// FAR's `FormatCodePageString`: the number in five columns, `│`, a space (`*`: a user's name), the
/// name.
fn page_line(id: u32, (name, custom): (&str, bool), marked: &[u32]) -> ListLine {
    ListLine::Marked {
        label: page_text(id, name, custom),
        checked: marked.contains(&id),
    }
}

/// The list's lines with `marked` marked, and the line of `chosen` (the first when it is not there);
/// the favourites of the code page menu under "Favorites", the user's names theirs (FAR's
/// `AddCodePages` with `CPST_FAVORITE`, `GetCodePageCustomName`).
pub(super) fn lines(
    chosen: CodePageChoice,
    marked: &[u32],
    pages: &CodePageSettings,
) -> (Vec<ListLine>, usize) {
    let (ansi, oem) = (ansi_code_page(), oem_code_page());
    let mut lines = vec![
        ListLine::Choice("Automatic detection".to_string()),
        ListLine::Choice(all_title(!marked.is_empty()).to_string()),
        ListLine::Section("System".to_string()),
        page_line(ansi, (&system_name(ansi, "ANSI"), false), marked),
        page_line(oem, (&system_name(oem, "OEM"), false), marked),
        ListLine::Section("Unicode".to_string()),
        page_line(UTF8, ("UTF-8", false), marked),
        page_line(UTF16_LE, ("UTF-16 (Little endian)", false), marked),
        page_line(UTF16_BE, ("UTF-16 (Big endian)", false), marked),
    ];
    let standard = standard_pages();
    let (mut favorites, mut others) = (Vec::new(), Vec::new());
    for page in installed_code_pages().iter() {
        if standard.contains(&page.id) || page.id == UTF7 {
            continue;
        }
        let (name, custom) = name_of(pages, page.id, &page.name);
        let line = page_line(page.id, (&name, custom), marked);
        match pages.favorites.contains(&page.id) {
            true => favorites.push(line),
            false => others.push(line),
        }
    }
    for (title, part) in [("Favorites", favorites), ("Other", others)] {
        if !part.is_empty() {
            lines.push(ListLine::Section(title.to_string()));
            lines.extend(part);
        }
    }
    let at = (0..lines.len()).find(|&at| choice_of(&lines, at) == Some(chosen));
    (lines, at.unwrap_or(0))
}

/// The code page of line `at`.
pub(super) fn choice_of(lines: &[ListLine], at: usize) -> Option<CodePageChoice> {
    match (at, lines.get(at)?) {
        (0, _) => Some(CodePageChoice::Auto),
        (ALL_LINE, _) => Some(CodePageChoice::All),
        (_, ListLine::Marked { label, .. }) => page_of(label).map(CodePageChoice::Page),
        _ => None,
    }
}

fn page_of(label: &str) -> Option<u32> {
    label.split('│').next()?.trim().parse().ok()
}

/// The marked code pages, by their numbers.
pub(super) fn marked_of(lines: &[ListLine]) -> Vec<u32> {
    let mut marked: Vec<u32> = (lines.iter())
        .filter_map(|line| match line {
            ListLine::Marked {
                label,
                checked: true,
            } => page_of(label),
            _ => None,
        })
        .collect();
    marked.sort_unstable();
    marked.dedup();
    marked
}

/// "All standard code pages" or "Selected code pages" as the marks are now.
pub(super) fn retitle(lines: &mut [ListLine]) {
    let any = !marked_of(lines).is_empty();
    if let Some(ListLine::Choice(title)) = lines.get_mut(ALL_LINE) {
        all_title(any).clone_into(title);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(lines: &[ListLine]) -> Vec<String> {
        (lines.iter())
            .map(|line| match line {
                ListLine::Section(title) => format!("-- {title}"),
                line => line.label().unwrap_or_default().to_string(),
            })
            .collect()
    }

    fn none() -> CodePageSettings {
        CodePageSettings::default()
    }

    #[test]
    fn the_menu_s_favourites_stand_under_favorites_by_the_user_s_names() {
        let standard = standard_pages();
        let Some(page) = (installed_code_pages().iter())
            .map(|page| page.id)
            .find(|id| !standard.contains(id) && *id != UTF7)
        else {
            return;
        };
        let mut pages = none();
        pages.favorites.push(page);
        pages.names.insert(page.to_string(), "Mine".to_string());
        let (lines, _) = lines(CodePageChoice::Page(page), &[page], &pages);
        let shown = labels(&lines);
        let favorites = shown
            .iter()
            .position(|line| line == "-- Favorites")
            .unwrap();
        assert_eq!(shown[favorites + 1], format!("{page:<5}│*Mine"));
        let other = shown.iter().position(|line| line == "-- Other");
        assert!(other.is_none_or(|other| other > favorites + 1));
        assert!(
            matches!(
                &lines[favorites + 1],
                ListLine::Marked { checked: true, .. }
            ),
            "the mark kept"
        );
        assert_eq!(
            choice_of(&lines, favorites + 1),
            Some(CodePageChoice::Page(page))
        );
    }

    #[test]
    fn the_list_is_fars() {
        let (lines, chosen) = lines(CodePageChoice::Auto, &[], &none());
        let shown = labels(&lines);
        assert_eq!(chosen, 0);
        assert_eq!(
            shown[..3],
            [
                "Automatic detection",
                "All standard code pages",
                "-- System"
            ]
        );
        assert!(
            shown[3].starts_with(&format!("{:<5}│ ", ansi_code_page())),
            "{}",
            shown[3]
        );
        assert!(
            shown[4].starts_with(&format!("{:<5}│ ", oem_code_page())),
            "{}",
            shown[4]
        );
        assert_eq!(
            shown[5..9],
            [
                "-- Unicode",
                "65001│ UTF-8",
                "1200 │ UTF-16 (Little endian)",
                "1201 │ UTF-16 (Big endian)"
            ]
        );
        if cfg!(windows) {
            assert!(shown[3].contains("ANSI") && shown[4].contains("OEM"));
            assert_eq!(shown[9], "-- Other");
            assert!(
                shown.iter().any(|line| line.starts_with("932  │ ")),
                "{shown:?}"
            );
            assert!(
                !shown.iter().any(|line| line.starts_with("65000")),
                "no UTF-7"
            );
            let others: Vec<u32> = shown[10..]
                .iter()
                .filter_map(|line| page_of(line))
                .collect();
            assert!(others.windows(2).all(|pair| pair[0] < pair[1]), "by number");
        }
    }

    #[test]
    fn the_marks_and_the_choice_come_back() {
        let (lines, chosen) = lines(CodePageChoice::Page(UTF16_BE), &[UTF8, UTF16_LE], &none());
        assert_eq!(chosen, 8);
        assert_eq!(
            choice_of(&lines, chosen),
            Some(CodePageChoice::Page(UTF16_BE))
        );
        assert_eq!(marked_of(&lines), [UTF16_LE, UTF8]);
        assert_eq!(lines[1].label(), Some("Selected code pages"));
        assert_eq!(choice_of(&lines, 1), Some(CodePageChoice::All));
        assert_eq!(choice_of(&lines, 2), None, "a separator");
        let (_, gone) = super::lines(CodePageChoice::Page(4_000_000), &[], &none());
        assert_eq!(gone, 0, "a code page no longer there: the automatic");
    }

    #[test]
    fn the_title_follows_the_marks() {
        let (mut lines, _) = lines(CodePageChoice::All, &[UTF8], &none());
        if let ListLine::Marked { checked, .. } = &mut lines[6] {
            *checked = false;
        }
        retitle(&mut lines);
        assert_eq!(lines[1].label(), Some("All standard code pages"));
    }

    #[test]
    fn all_reads_the_marked_or_the_standard_ones() {
        assert_eq!(
            CodePageChoice::All.code_pages(&[866]),
            CodePages::Pages(vec![866])
        );
        let CodePages::Pages(standard) = CodePageChoice::All.code_pages(&[]) else {
            panic!("pages");
        };
        assert_eq!(standard[standard.len() - 3..], [UTF8, UTF16_LE, UTF16_BE]);
        assert_eq!(standard[0], ansi_code_page());
        assert_eq!(CodePageChoice::Auto.code_pages(&[866]), CodePages::Auto);
        assert_eq!(
            CodePageChoice::Page(866).code_pages(&[]),
            CodePages::Pages(vec![866])
        );
    }

    #[test]
    fn the_choice_is_saved_by_its_id() {
        for choice in [
            CodePageChoice::Auto,
            CodePageChoice::All,
            CodePageChoice::Page(1251),
        ] {
            assert_eq!(CodePageChoice::from_id(&choice.id()), choice);
        }
        assert_eq!(CodePageChoice::from_id("rubbish"), CodePageChoice::Auto);
        assert_eq!(CodePageChoice::Page(1200).conversion_page(), UTF16_LE);
        assert_eq!(CodePageChoice::All.conversion_page(), ansi_code_page());
    }
}
