//! The highlighting of an edited document with Colorer's own schemes (`assets/colorer/hrc`): the rows
//! painted by their classes, a comment over many lines coloured from a kept state, an edit dropping the
//! states past it, a screen far past them coloured from lines above it.

use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::*;
use crate::editor::view::edit_view::{EditView, Step};
use crate::editor::view::pieces::PieceTable;
use crate::editor::view::{BLOCK, Encoding};

/// The program's schemes, read once for the tests.
fn syntax() -> Arc<Syntax> {
    static SYNTAX: OnceLock<Arc<Syntax>> = OnceLock::new();
    Arc::clone(SYNTAX.get_or_init(|| {
        let catalog = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/colorer/hrc/catalog.xml");
        let syntax = Syntax::open(&catalog);
        let id = syntax.choose("main.rs", "").unwrap();
        syntax.load(id).unwrap();
        Arc::new(syntax)
    }))
}

fn view_of(text: &str) -> EditView {
    let bytes = text.as_bytes();
    let mut doc = PieceTable::new(bytes.len() as u64);
    for (index, chunk) in bytes.chunks(BLOCK as usize).enumerate() {
        doc.insert_block(index as u64, chunk.to_vec());
    }
    EditView::new(doc, (Encoding::Utf8, 0), "\r\n")
}

fn rust(view: &EditView) -> Highlighter {
    let syntax = syntax();
    let id = syntax.choose("main.rs", "").unwrap();
    let base = syntax.load(id).unwrap();
    Highlighter::new(syntax, base, view.window().2)
}

/// A row's paints as (from, to, class).
fn row(paints: &[Paint]) -> Vec<(usize, usize, &'static str)> {
    paints.iter().map(|p| (p.from, p.to, p.class)).collect()
}

#[test]
fn rows_are_painted_by_their_classes() {
    let view = view_of("fn main() {\n\t// hi\n    let x = 1;\n}\n");
    let mut highlighter = rust(&view);
    let id = syntax().choose("main.rs", "").unwrap();
    assert_eq!(syntax().type_name(id).unwrap().0, "Rust");
    let rows = highlighter.paint(&view, &view.rows(4), 40);
    assert_eq!(rows.len(), 4);
    assert!(row(&rows[0]).contains(&(0, 2, "Keyword")), "{:?}", rows[0]);
    assert!(
        row(&rows[1])
            .iter()
            .any(|&(from, to, class)| from == 8 && to == 13 && class.starts_with("Comment")),
        "the comment after the tab's cells: {:?}",
        rows[1]
    );
    assert!(row(&rows[2]).contains(&(4, 7, "Keyword")), "{:?}", rows[2]);
    assert!(
        row(&rows[2])
            .iter()
            .any(|&(from, _, class)| from == 12 && class.starts_with("Number"))
    );
}

/// A comment over 300 lines, then code.
fn long_comment() -> String {
    let mut text = String::from("/*\n");
    for n in 0..300 {
        text.push_str(&format!("line {n}\n"));
    }
    text.push_str("*/\nfn after() {}\n");
    text
}

fn comment_rows(rows: &[Vec<Paint>]) -> usize {
    rows.iter()
        .filter(|paints| paints.iter().any(|p| p.class.starts_with("Comment")))
        .count()
}

#[test]
fn a_comment_over_many_lines_is_coloured_from_a_kept_state() {
    let mut view = view_of(&long_comment());
    let mut highlighter = rust(&view);
    while highlighter.fill(&view, Duration::from_secs(5)) {}
    assert!(
        highlighter.marks.len() >= 300 / STEP,
        "{}",
        highlighter.marks.len()
    );
    view.apply(&Step::GoTo(200, 0), 10).unwrap();
    view.scroll(40, 10).unwrap();
    let rows = highlighter.paint(&view, &view.rows(10), 40);
    assert_eq!(
        comment_rows(&rows),
        10,
        "in the comment, from the state before it"
    );
    // An edit in the comment's first line drops the states past it.
    let mut edited = view_of(&long_comment());
    let mut again = rust(&edited);
    while again.fill(&edited, Duration::from_secs(5)) {}
    edited.apply(&Step::GoTo(0, 0), 10).unwrap();
    edited.apply(&Step::Type('x'), 10).unwrap();
    again.changed(edited.take_changed().unwrap());
    assert_eq!(again.marks.len(), 1, "only the first line's state stays");
}

#[test]
fn a_screen_far_past_the_kept_states_is_coloured_from_lines_above_it() {
    let mut view = view_of(&long_comment());
    let mut highlighter = rust(&view);
    view.apply(&Step::GoTo(296, 0), 10).unwrap();
    view.scroll(40, 10).unwrap();
    let rows = highlighter.paint(&view, &view.rows(10), 40);
    assert_eq!(rows.len(), 10);
    assert_eq!(
        comment_rows(&rows),
        0,
        "200 lines above it are no comment's start: taken as code until the states come"
    );
    while highlighter.fill(&view, Duration::from_secs(5)) {}
    let rows = highlighter.paint(&view, &view.rows(10), 40);
    assert_eq!(comment_rows(&rows), 10, "the comment's last lines, exact");
}

/// Review B8: once the states reach the end nothing is parsed again at each tick, until an edit.
#[test]
fn the_states_at_the_end_are_not_filled_again_until_an_edit() {
    let mut view = view_of(&long_comment());
    let mut highlighter = rust(&view);
    while highlighter.fill(&view, Duration::from_secs(5)) {}
    assert!(highlighter.filled, "at the end");
    assert!(
        highlighter.outline.next.is_none(),
        "review B3: the outlines collected ahead by the ticks too"
    );
    let marks = highlighter.marks.len();
    assert!(!highlighter.fill(&view, Duration::from_secs(5)));
    assert_eq!(highlighter.marks.len(), marks);
    view.apply(&Step::GoTo(250, 0), 10).unwrap();
    view.apply(&Step::Type('x'), 10).unwrap();
    highlighter.changed(view.take_changed().unwrap());
    assert!(!highlighter.filled, "an edit: filled again from it");
    while highlighter.fill(&view, Duration::from_secs(5)) {}
    assert_eq!(highlighter.marks.len(), marks, "the states back to the end");
}

/// The type named `name`.
fn syntax_type(syntax: &Syntax, name: &str) -> TypeId {
    syntax.lib.read().unwrap().type_named(name).unwrap()
}

/// Review B5 (Colorer's `chooseFileTypeCh`): the type is chosen by the text's first lines, not its
/// first one alone - `#include` on the second line of a file with no extension is C/C++.
#[test]
fn the_type_is_chosen_by_the_texts_first_lines() {
    let syntax = syntax();
    let lines = ["a note".to_string(), "#include <stdio.h>".to_string()];
    let mut first = |index: usize| Ok((index == 0).then(|| lines[0].clone()));
    let alone = syntax.try_choose("noext", &mut first, None);
    assert_ne!(
        alone,
        Chosen::Type(syntax_type(&syntax, "cpp"), "cpp".to_string()),
        "the first line alone is no C/C++"
    );
    let mut line = |index: usize| Ok(lines.get(index).cloned());
    let chosen = syntax.try_choose("noext", &mut line, None);
    let Chosen::Type(_, name) = chosen else {
        panic!("{chosen:?}");
    };
    assert_eq!(name, "cpp");
    let mut missing = |_: usize| Err(());
    assert_eq!(
        syntax.try_choose("noext", &mut missing, None),
        Chosen::Busy,
        "a line not read yet: asked again"
    );
}

/// Colorer's text start: four lines of 800 characters at most; a type named is taken without the text
/// (a line not read yet asks nothing), a name no type has is none; the workers' `choose_by_start` the
/// same by the lines given.
#[test]
fn the_texts_start_is_its_first_four_lines_and_a_named_type_needs_none() {
    let syntax = syntax();
    let cpp = Chosen::Type(syntax_type(&syntax, "cpp"), "cpp".to_string());
    let include_at = |index: usize, first: String| {
        let mut lines = vec![
            first,
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ];
        lines[index] = "#include <stdio.h>".to_string();
        lines
    };
    let chosen = |lines: Vec<String>| {
        let mut line = |index: usize| Ok(lines.get(index).cloned());
        syntax.try_choose("noext", &mut line, None)
    };
    assert_eq!(chosen(include_at(3, String::new())), cpp, "the fourth line");
    assert_ne!(chosen(include_at(4, String::new())), cpp, "the fifth not");
    assert_ne!(
        chosen(include_at(1, "a".repeat(900))),
        cpp,
        "past 800 characters"
    );
    let near = format!("{}abc", "a note. ".repeat(99));
    assert_ne!(
        chosen(include_at(1, near)),
        cpp,
        "the next line cut at 800 characters in all"
    );
    let lines = include_at(1, "a note".to_string());
    assert_eq!(
        syntax.choose_by_start("noext", &lines),
        Some(syntax_type(&syntax, "cpp"))
    );
    let mut missing = |_: usize| Err(());
    assert_eq!(syntax.try_choose("noext", &mut missing, Some("cpp")), cpp);
    assert_eq!(
        syntax.try_choose("noext", &mut missing, Some("no such type")),
        Chosen::Broken
    );
}

/// Review B1 (Colorer's `chunkLongLines`): a comment opened far along a long line is seen, and the
/// next line is in it - past about 300 characters a line was left uncoloured.
#[test]
fn a_long_lines_far_end_is_parsed_too() {
    let long = "let a = 1; ".repeat(32);
    let text = format!("{long}/* open\nstill comment */\nfn x() {{}}\n");
    let view = view_of(&text);
    let mut highlighter = rust(&view);
    let rows = highlighter.paint(&view, &view.rows(3), 400);
    let comment_from = |paints: &[Paint]| {
        (paints.iter())
            .find(|p| p.class.starts_with("Comment"))
            .map(|p| p.from)
    };
    assert_eq!(
        comment_from(&rows[0]),
        Some(long.len()),
        "the comment at column {}",
        long.len()
    );
    assert_eq!(comment_from(&rows[1]), Some(0), "the next line in it");
    assert_eq!(comment_from(&rows[2]), None, "closed before the last line");
}

/// R7 review B5: an action of the screen's thread does not wait for the library a worker reads schemes
/// into - it is told to wait and is done again; the type menu has the types meanwhile.
#[test]
fn the_screens_actions_do_not_wait_for_the_library() {
    let catalog = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/colorer/hrc/catalog.xml");
    let syntax = Arc::new(Syntax::open(&catalog));
    let (release, held) = std::sync::mpsc::channel::<()>();
    let worker = Arc::clone(&syntax);
    let (taken, told) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _lib = worker.lib.write().unwrap();
        taken.send(()).unwrap();
        let _ = held.recv();
    });
    told.recv().unwrap();
    assert_eq!(syntax.read_now().err(), Some(Wait::Library), "done again");
    let types = syntax.types();
    assert!(
        types.iter().any(|entry| entry.name == "rust"),
        "the menu's types"
    );
    release.send(()).unwrap();
    holder.join().unwrap();
    assert!(syntax.read_now().unwrap().is_some());
    assert_eq!(syntax.types(), types);
}
