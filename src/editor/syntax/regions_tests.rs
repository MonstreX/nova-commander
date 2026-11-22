use super::*;
use crate::editor::syntax::parser::{State, parse_line};

fn span(start: usize, end: Option<usize>, region: Option<RegionId>) -> Span {
    Span {
        start,
        end,
        region,
        scheme: None,
        depth: 0,
        flow: false,
    }
}

fn parts(list: &[Part]) -> Vec<(usize, Option<usize>, Option<RegionId>)> {
    (list.iter())
        .map(|part| (part.span.start, part.span.end, part.span.region))
        .collect()
}

#[test]
fn a_region_added_cuts_the_one_under_it_in_two_and_hides_what_it_covers() {
    let lib = Library::default();
    let defs = Defs::default();
    let spans = [
        span(0, None, None),
        span(2, Some(10), Some(1)),
        span(4, Some(6), Some(2)),
        span(4, Some(5), Some(3)),
    ];
    let list = compact(&lib, &defs, &spans);
    assert_eq!(
        parts(&list),
        [
            (0, Some(2), None),
            (2, Some(4), Some(1)),
            (4, Some(5), Some(3)),
            (5, Some(6), Some(2)),
            (6, Some(10), Some(1)),
            (10, None, None),
        ]
    );
    // FarColorer's cursorRegion: the last part with the cursor in it, its end counted in.
    let at = |pos: usize| cursor_part(&list, pos, 12).and_then(|part| part.span.region);
    assert_eq!(at(3), Some(1));
    assert_eq!(at(4), Some(3));
    assert_eq!(at(6), Some(1), "at 6 both 5..6 and 6..10: the later");
    assert_eq!(at(11), None);
}

#[test]
fn a_rust_line_s_brackets_are_pairs_and_the_cursor_s_region_is_found() {
    let catalog =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/colorer/hrc/catalog.xml");
    let mut lib = Library::from_catalog(&catalog);
    let rust = lib.type_named("rust").unwrap();
    lib.load_type(rust);
    let base = lib.file_type(rust).base_scheme.unwrap();
    let line: Vec<char> = "fn f() { g(1) }".chars().collect();
    let parsed = parse_line(&lib, base, &State::default(), &line);
    let defs = Defs::of(&lib);
    let list = compact(&lib, &defs, &parsed.spans);
    let ends = pair_ends(&lib, &defs, &list);
    let shown: Vec<(usize, bool)> = ends.iter().map(|end| (end.start, end.opens)).collect();
    assert!(shown.contains(&(7, true)), "{{: {shown:?}");
    assert!(shown.contains(&(14, false)), "}}: {shown:?}");
    assert!(
        shown.contains(&(10, true)) && shown.contains(&(12, false)),
        "{shown:?}"
    );
    let brace = pair_at(&ends, 7).unwrap();
    assert_eq!((ends[brace].start, ends[brace].opens), (7, true));
    assert_eq!(pair_at(&ends, 3), None, "no pair at 'f'");
    let keyword = cursor_part(&list, 0, line.len()).unwrap();
    let name = &lib.region(keyword.span.region.unwrap()).name;
    assert!(
        name.contains("Keyword") || name.starts_with("rust:"),
        "{name}"
    );
}
