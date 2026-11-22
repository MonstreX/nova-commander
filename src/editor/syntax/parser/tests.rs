//! The text parser over a small grammar written as Colorer's: keywords with their word boundaries,
//! regular expressions, a block over lines kept in the state, its end referring to its start (`\y`),
//! a block's regions at its start and end, an inherited scheme with a virtual substitute.

use std::path::Path;

use super::*;

const PROTO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<hrc version="take5" xmlns="http://colorer.sf.net/2003/hrc">
  <prototype name="def" group="main" description="Default"><location link="def.hrc"/></prototype>
  <prototype name="toy" group="main" description="Toy"><location link="toy.hrc"/></prototype>
</hrc>
"#;

const DEF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<hrc version="take5" xmlns="http://colorer.sf.net/2003/hrc">
  <type name="def">
    <region name="Comment"/><region name="String"/><region name="Keyword"/>
    <region name="Number"/><region name="Symbol"/><region name="Word"/>
    <scheme name="def"/>
  </type>
</hrc>
"#;

const TOY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<hrc version="take5" xmlns="http://colorer.sf.net/2003/hrc">
  <type name="toy">
    <import type="def"/>
    <scheme name="Comment">
      <regexp match="/TODO/" region="def:Keyword"/>
    </scheme>
    <scheme name="String">
      <regexp match="/\\./" region="def:Symbol"/>
    </scheme>
    <scheme name="Words">
      <regexp match="/[a-z]+/" region="def:Word"/>
    </scheme>
    <scheme name="Loud">
      <regexp match="/[a-z]+/" region="def:Keyword"/>
    </scheme>
    <scheme name="Bracket">
      <inherit scheme="Words"/>
    </scheme>
    <scheme name="toy">
      <keywords region="def:Keyword"><word name="if"/><word name="ifdef"/><symb name="=="/></keywords>
      <block start="/(\/\*)/" end="/(\*\/)/" scheme="Comment" region="def:Comment"
             region00="def:Symbol" region10="def:Symbol"/>
      <block start="/(['&quot;])/" end="/\y1/" scheme="String" region="def:String"/>
      <block start="/(\[)/" end="/(\])/" scheme="Bracket">
      </block>
      <inherit scheme="Bracket">
        <virtual scheme="Words" subst-scheme="Loud"/>
      </inherit>
      <regexp match="/\d+/" region="def:Number"/>
    </scheme>
  </type>
</hrc>
"#;

fn library(dir: &Path) -> (Library, SchemeId) {
    for (name, text) in [("proto.hrc", PROTO), ("def.hrc", DEF), ("toy.hrc", TOY)] {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let catalog = r#"<catalog><hrc-sets><location link="proto.hrc"/></hrc-sets></catalog>"#;
    std::fs::write(dir.join("catalog.xml"), catalog).unwrap();
    let mut lib = Library::from_catalog(&dir.join("catalog.xml"));
    let toy = lib.type_named("toy").unwrap();
    lib.load_type(toy);
    assert!(lib.log.is_empty(), "{:?}", lib.log);
    let base = lib.file_type(toy).base_scheme.unwrap();
    (lib, base)
}

/// The line's spans with regions, as (start, end, short region name).
fn spans(lib: &Library, parsed: &Parsed) -> Vec<(usize, Option<usize>, String)> {
    parsed
        .spans
        .iter()
        .filter_map(|span| {
            let name = lib
                .region(span.region?)
                .name
                .trim_start_matches("def:")
                .to_string();
            Some((span.start, span.end, name))
        })
        .collect()
}

fn line(text: &str) -> Vec<char> {
    text.chars().collect()
}

#[test]
fn keywords_keep_their_word_boundaries_and_numbers_are_found() {
    let temp = tempfile::tempdir().unwrap();
    let (lib, base) = library(temp.path());
    let parsed = parse_line(
        &lib,
        base,
        &State::default(),
        &line("if ifdef iffy 12 == 3"),
    );
    let got = spans(&lib, &parsed);
    assert_eq!(
        got,
        [
            (0, Some(2), "Keyword".to_string()),
            (3, Some(8), "Keyword".to_string()),
            (9, Some(13), "Keyword".to_string()),
            (14, Some(16), "Number".to_string()),
            (17, Some(19), "Keyword".to_string()),
            (20, Some(21), "Number".to_string()),
        ],
        "iffy is no keyword (the word goes on): all of it is the substitute Loud's word"
    );
    assert!(parsed.next.levels.is_empty());
}

#[test]
fn a_block_over_lines_is_kept_in_the_state() {
    let temp = tempfile::tempdir().unwrap();
    let (lib, base) = library(temp.path());
    let first = parse_line(&lib, base, &State::default(), &line("1 /* a"));
    assert_eq!(
        spans(&lib, &first),
        [
            (0, Some(1), "Number".to_string()),
            (2, None, "Comment".to_string()),
            (2, Some(4), "Symbol".to_string()),
        ]
    );
    assert_eq!(first.next.levels.len(), 1, "the comment goes on");
    let second = parse_line(&lib, base, &first.next, &line("TODO */ 7"));
    assert_eq!(
        spans(&lib, &second),
        [
            (0, Some(7), "Comment".to_string()),
            (0, Some(4), "Keyword".to_string()),
            (5, Some(7), "Symbol".to_string()),
            (8, Some(9), "Number".to_string()),
        ],
        "the comment from the line's start to its end"
    );
    assert!(second.next.levels.is_empty());
}

#[test]
fn a_block_s_end_refers_to_its_start() {
    let temp = tempfile::tempdir().unwrap();
    let (lib, base) = library(temp.path());
    let first = parse_line(&lib, base, &State::default(), &line("'it\"s"));
    assert_eq!(
        first.next.levels.len(),
        1,
        "a double quote ends no single one"
    );
    let second = parse_line(&lib, base, &first.next, &line("\\' end' 5"));
    assert_eq!(
        spans(&lib, &second),
        [
            (0, Some(7), "String".to_string()),
            (0, Some(2), "Symbol".to_string()),
            (8, Some(9), "Number".to_string()),
        ]
    );
}

#[test]
fn a_virtual_substitute_holds_in_its_inherit_only() {
    let temp = tempfile::tempdir().unwrap();
    let (lib, base) = library(temp.path());
    let parsed = parse_line(&lib, base, &State::default(), &line("ab [cd]"));
    assert_eq!(
        spans(&lib, &parsed),
        [
            (0, Some(2), "Keyword".to_string()),
            (4, Some(6), "Word".to_string()),
        ],
        "the inherit's Words is Loud; the block's Bracket is plain"
    );
}

/// Review B7: a scheme that inherits itself (a user's schema written wrong) is searched a bounded depth
/// - Colorer's recursion overflowed the stack and the program ended.
#[test]
fn a_scheme_inheriting_itself_ends() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let proto = PROTO.replace("toy", "loop");
    let looped = r#"<?xml version="1.0" encoding="UTF-8"?>
<hrc version="take5" xmlns="http://colorer.sf.net/2003/hrc">
  <type name="loop">
    <import type="def"/>
    <scheme name="loop">
      <inherit scheme="loop"/>
      <regexp match="/x/" region="def:Word"/>
    </scheme>
  </type>
</hrc>
"#;
    for (name, text) in [
        ("proto.hrc", proto.as_str()),
        ("def.hrc", DEF),
        ("loop.hrc", looped),
    ] {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let catalog = r#"<catalog><hrc-sets><location link="proto.hrc"/></hrc-sets></catalog>"#;
    std::fs::write(dir.join("catalog.xml"), catalog).unwrap();
    let mut lib = Library::from_catalog(&dir.join("catalog.xml"));
    let id = lib.type_named("loop").unwrap();
    lib.load_type(id);
    let base = lib.file_type(id).base_scheme.unwrap();
    let parsed = parse_line(&lib, base, &State::default(), &line("ab x"));
    assert!(parsed.spans.len() <= 4, "it ends: {:?}", parsed.spans.len());
}
