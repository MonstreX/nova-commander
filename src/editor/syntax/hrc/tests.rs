//! The HRC library over small grammars written as Colorer's are: prototypes chosen by name and first
//! line, a type loaded when wanted with its regions, entities, imports and schemes, blocks and inherits
//! linked, keyword lists sorted and merged, a DTD include, a scheme off by its parameter.

use std::path::Path;

use super::*;

const PROTO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE hrc [
  <!-- <!ENTITY name PUBLIC "aaa"> -->
  <!ENTITY more-protos SYSTEM "more.ent.hrc">
]>
<hrc version="take5" xmlns="http://colorer.sf.net/2003/hrc">
  <prototype name="def" group="main" description="Default">
    <location link="def.hrc"/>
  </prototype>
  <prototype name="toy" group="main" description="Toy language">
    <location link="toy.hrc"/>
    <filename>/\.toy$/i</filename>
    <firstline>/^#!toy/</firstline>
    <parameters>
      <param name="extra" value="false" description="Extra words"/>
      <param name="firstlines" value="10"/>
    </parameters>
  </prototype>
  &more-protos;
</hrc>
"#;

const MORE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
  <prototype name="other" group="rare" description="Other">
    <location link="toy.hrc"/>
    <filename weight="5">/\.oth$/</filename>
  </prototype>
"#;

const DEF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<hrc version="take5" xmlns="http://colorer.sf.net/2003/hrc">
  <type name="def">
    <region name="Text"/>
    <region name="Syntax" parent="Text"/>
    <region name="Comment" parent="Syntax"/>
    <region name="String" parent="Syntax"/>
    <region name="Keyword" parent="Syntax"/>
    <region name="Symbol" parent="Syntax"/>
    <region name="default"/>
    <scheme name="def">
      <regexp match="/\s+$/" region="Text"/>
    </scheme>
  </type>
</hrc>
"#;

const TOY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<hrc version="take5" xmlns="http://colorer.sf.net/2003/hrc">
  <type name="toy">
    <import type="def"/>
    <region name="Word" parent="def:Keyword" description="A toy word"/>
    <entity name="digits" value="[0-9]+"/>
    <entity name="number" value="%digits;(\.%digits;)?"/>
    <scheme name="String">
      <regexp match="/\\./" region="def:Symbol"/>
    </scheme>
    <scheme name="Extra" if="extra">
      <keywords region="Word"><word name="never"/></keywords>
    </scheme>
    <scheme name="Words">
      <keywords region="Word">
        <word name="zeta"/><word name="alpha"/><word name="alphabet"/><word name="alpha"/>
      </keywords>
      <keywords region="def:Keyword"><word name="beta"/></keywords>
    </scheme>
    <scheme name="toy">
      <inherit scheme="Words"/>
      <inherit scheme="def:def">
        <virtual scheme="def:def" subst-scheme="String"/>
      </inherit>
      <regexp match="/%number;/" region="def:Symbol"/>
      <block start="/(&quot;)/" end="/\y1/" scheme="String" region="def:String"
             region00="def:Symbol" region10="def:Symbol"/>
      <block scheme="String" inner-region="yes" priority="low" content-priority="low">
        <start match="/(\/\*)/" region0="def:Comment"/>
        <end region="def:Comment">/\*\//</end>
      </block>
      <regexp region="def:default" priority="low"><![CDATA[/<=/]]></regexp>
    </scheme>
  </type>
</hrc>
"#;

fn library(dir: &Path) -> Library {
    for (name, text) in [
        ("proto.hrc", PROTO),
        ("more.ent.hrc", MORE),
        ("def.hrc", DEF),
        ("toy.hrc", TOY),
    ] {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let catalog = r#"<catalog><hrc-sets><location link="proto.hrc"/></hrc-sets></catalog>"#;
    std::fs::write(dir.join("catalog.xml"), catalog).unwrap();
    Library::from_catalog(&dir.join("catalog.xml"))
}

#[test]
fn prototypes_are_chosen_by_the_name_and_the_first_line() {
    let temp = tempfile::tempdir().unwrap();
    let lib = library(temp.path());
    assert!(lib.log.is_empty(), "{:?}", lib.log);
    let toy = lib.type_named("toy").unwrap();
    assert_eq!(lib.choose_type("a.TOY", "", 0), Some(toy));
    assert_eq!(lib.choose_type("script", "#!toy run", 0), Some(toy));
    let other = lib.type_named("other").unwrap();
    assert_eq!(
        lib.choose_type("x.oth", "", 0),
        Some(other),
        "from the DTD's include"
    );
    assert_eq!(lib.file_type(toy).group, "main");
    assert_eq!(lib.file_type(toy).param("extra"), Some("false"));
    assert_eq!(lib.file_type(toy).param_int("firstlines", 2), 10);
    assert_eq!(lib.file_type(toy).param_int("extra", 7), 7, "not a number");
    assert_eq!(lib.file_type(toy).description, "Toy language");
    let extra = &lib.file_type(toy).params[0].1;
    assert_eq!(extra.description.as_deref(), Some("Extra words"));
    assert_eq!(lib.file_type(other).choosers[0].priority, 5.0);
    assert_eq!(lib.listed_types().len(), 3);
    assert!(lib.file_type(toy).base_scheme.is_none(), "not loaded yet");
}

/// The scheme's nodes by their kind.
fn nodes(lib: &Library, scheme: SchemeId) -> Vec<&Node> {
    lib.scheme(scheme)
        .nodes
        .iter()
        .map(|&at| lib.node(NodeRef::Node(at)))
        .collect()
}

#[test]
fn a_type_loads_with_its_regions_entities_and_schemes_linked() {
    let temp = tempfile::tempdir().unwrap();
    let mut lib = library(temp.path());
    let toy = lib.type_named("toy").unwrap();
    lib.load_type(toy);
    assert!(lib.log.is_empty(), "{:?}", lib.log);
    let base = lib.file_type(toy).base_scheme.unwrap();
    assert_eq!(lib.scheme(base).name, "toy:toy");
    let word = lib.region_named("toy:Word").unwrap();
    let keyword = lib.region_named("def:Keyword").unwrap();
    assert_eq!(
        lib.region(word).parent,
        Some(keyword),
        "def loaded through the import"
    );
    assert!(lib.region_has_parent(word, lib.region_named("def:Text").unwrap()));
    assert_eq!(lib.region(word).description, "A toy word");
    assert_eq!(lib.region_count(), 8);
}

/// The toy type loaded: the library and its base scheme.
fn loaded(dir: &Path) -> (Library, SchemeId) {
    let mut lib = library(dir);
    let toy = lib.type_named("toy").unwrap();
    lib.load_type(toy);
    let base = lib.file_type(toy).base_scheme.unwrap();
    (lib, base)
}

#[test]
fn inherits_and_blocks_find_their_schemes() {
    let temp = tempfile::tempdir().unwrap();
    let (lib, base) = loaded(temp.path());
    let string = lib.scheme_named("toy:String").unwrap();
    let def = lib.scheme_named("def:def").unwrap();
    let nodes = nodes(&lib, base);
    let Node::Inherit(inherit) = nodes[1] else {
        panic!("{:?}", nodes[1]);
    };
    assert_eq!(
        (
            inherit.scheme,
            inherit.virtuals[0].virt,
            inherit.virtuals[0].subst
        ),
        (Some(def), Some(def), Some(string))
    );
    assert!(lib.scheme(def).virtual_target);
    let Node::Re(number) = nodes[2] else { panic!() };
    let text: Vec<char> = "12.5".chars().collect();
    let mut found = Matches::default();
    assert!(
        number.start.parse(&text, &mut found, Parse::default()),
        "entities put in"
    );
    assert_eq!(found.e[0], 4);
    let Node::Block(quote) = nodes[3] else {
        panic!()
    };
    assert_eq!(quote.scheme, Some(string));
    assert!(quote.end.position_moves() && !quote.start.position_moves());
    assert!(!quote.inner_region && !quote.low_priority && !quote.low_content_priority);
    assert!(!number.low_priority);
    let symbol = lib.region_named("def:Symbol");
    assert_eq!(
        (quote.region, quote.regions[0], quote.regione[0]),
        (lib.region_named("def:String"), symbol, symbol)
    );
}

#[test]
fn a_block_s_start_and_end_elements_and_the_priorities() {
    let temp = tempfile::tempdir().unwrap();
    let (lib, base) = loaded(temp.path());
    let nodes = nodes(&lib, base);
    let Node::Block(comment) = nodes[4] else {
        panic!()
    };
    assert!(comment.inner_region && comment.low_priority && comment.low_content_priority);
    let comment_region = lib.region_named("def:Comment");
    assert_eq!(
        (comment.region, comment.regions[0]),
        (comment_region, comment_region),
        "from <start> and <end>"
    );
    let Node::Re(default) = nodes[5] else {
        panic!()
    };
    assert_eq!(default.region, None, "a :default region is none");
    assert!(default.low_priority);
}

#[test]
fn inherits_are_flattened_and_keywords_merged() {
    let temp = tempfile::tempdir().unwrap();
    let mut lib = library(temp.path());
    let toy = lib.type_named("toy").unwrap();
    lib.load_type(toy);
    let base = lib.file_type(toy).base_scheme.unwrap();
    let search = &lib.scheme(base).search;
    let Node::Keywords(merged) = lib.node(search[0]) else {
        panic!("{:?}", lib.node(search[0]));
    };
    let words: Vec<String> = merged
        .list
        .words
        .iter()
        .map(|w| w.word.iter().collect())
        .collect();
    assert_eq!(
        words,
        ["alpha", "alphabet", "beta", "zeta"],
        "merged, sorted, no repeats"
    );
    assert_eq!(merged.list.words[1].shorter, Some(0));
    assert!(
        matches!(lib.node(search[1]), Node::Inherit(_)),
        "a virtual inherit stays"
    );
    assert_eq!(search.len(), 6);
    let extra = lib.scheme_named("toy:Extra").unwrap();
    assert!(lib.scheme(extra).nodes.is_empty(), "off by its parameter");
}
