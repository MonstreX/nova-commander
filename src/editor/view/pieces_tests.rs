//! The piece table: the original's bytes through it, inserts and deletes across pieces, undo and redo,
//! the save streamed in order.

use super::*;
use crate::editor::view::BLOCK;

/// A file in memory read by offset.
struct Memory(Vec<u8>);

impl RandomRead for Memory {
    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        let from = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(self.0.len());
        let count = buffer.len().min(self.0.len() - from);
        buffer[..count].copy_from_slice(&self.0[from..from + count]);
        Ok(count)
    }

    fn len(&mut self) -> io::Result<u64> {
        Ok(self.0.len() as u64)
    }
}

/// S2d.6e: saved, the table moves onto the saved file with its steps: undone back past the save to
/// the text deleted before it (read from the old file), changed or not as FAR's `UndoSavePos` says.
#[test]
fn the_steps_are_kept_over_a_save() {
    let old = b"hello big world".to_vec();
    let mut table = table_of(&old);
    table.delete(6, 4);
    table.insert(0, b">");
    let mut saved = Vec::new();
    table
        .write_to(&mut Memory(old.clone()), &mut saved, &mut |_| true)
        .unwrap();
    assert_eq!(saved, b">hello world");
    let mut moved = table
        .rebased(&mut Memory(old.clone()), 64)
        .unwrap()
        .unwrap();
    moved.insert_block(0, saved.clone());
    assert_eq!(whole(&moved), saved);
    assert!(!moved.is_changed(), "saved");
    moved.insert(1, b"!");
    assert!(moved.is_changed());
    moved.undo();
    assert!(!moved.is_changed(), "undone to the saved state");
    moved.undo();
    assert_eq!(whole(&moved), b"hello world");
    moved.undo();
    assert_eq!(whole(&moved), b"hello big world", "the deleted text back");
    assert!(moved.is_changed());
    moved.redo();
    moved.redo();
    assert!(!moved.is_changed());
    let too_much = table.rebased(&mut Memory(old), 3).unwrap();
    assert!(too_much.is_none(), "more than kept: the steps let go");
}

fn table_of(bytes: &[u8]) -> PieceTable {
    let mut table = PieceTable::new(bytes.len() as u64);
    for (index, chunk) in bytes.chunks(BLOCK as usize).enumerate() {
        table.insert_block(index as u64, chunk.to_vec());
    }
    table
}

/// The whole document through `at`, piece by piece.
fn whole(table: &PieceTable) -> Vec<u8> {
    let mut out = Vec::new();
    while (out.len() as u64) < table.len() {
        out.extend_from_slice(table.at(out.len() as u64).unwrap());
    }
    out
}

fn saved(table: &PieceTable, original: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut file = Memory(original.to_vec());
    table.write_to(&mut file, &mut out, &mut |_| true).unwrap();
    out
}

#[test]
fn the_original_reads_through_and_names_its_missing_blocks() {
    let table = PieceTable::new(10);
    assert_eq!(table.at(3), Err(Missing(0)));
    let table = table_of(b"hello world");
    assert_eq!(table.at(6).unwrap(), b"world");
    assert_eq!(table.before(5).unwrap(), b"hello");
    assert!(table.at(11).unwrap().is_empty());
    assert!(!table.is_changed());
}

#[test]
fn inserts_and_deletes_split_the_pieces() {
    let original = b"hello world";
    let mut table = table_of(original);
    table.insert(5, b",");
    assert_eq!(whole(&table), b"hello, world");
    table.insert(0, b">> ");
    table.insert(table.len(), b"!");
    assert_eq!(whole(&table), b">> hello, world!");
    assert_eq!(
        table.before(8).unwrap(),
        b"hello",
        "back to its piece's start"
    );
    table.delete(3, 7);
    assert_eq!(whole(&table), b">> world!", "across pieces");
    assert!(table.is_changed());
    assert_eq!(saved(&table, original), b">> world!");
}

#[test]
fn undo_and_redo_give_the_pieces_back() {
    let original = b"abc";
    let mut table = table_of(original);
    table.insert(1, b"XY");
    table.delete(0, 1);
    assert_eq!(whole(&table), b"XYbc");
    assert_eq!(table.undo(), Some(0));
    assert_eq!(whole(&table), b"aXYbc");
    assert_eq!(table.undo(), Some(1));
    assert_eq!(whole(&table), b"abc");
    assert_eq!(table.undo(), None);
    assert_eq!(table.redo(), Some(1));
    assert_eq!(whole(&table), b"aXYbc");
    table.insert(0, b"!");
    assert_eq!(table.redo(), None, "a new edit drops what could be redone");
}

/// A worker reads the edited document by offset: the edits from the table, the rest from the disk.
#[test]
fn the_document_reads_by_offset_from_the_table_and_the_disk() {
    let original = b"hello world";
    let mut doc = PieceTable::new(original.len() as u64);
    doc.insert(5, b",");
    let mut file = DocFile {
        doc,
        original: Box::new(Memory(original.to_vec())),
    };
    let mut buffer = [0; 4];
    assert_eq!(
        file.read_at(3, &mut buffer).unwrap(),
        2,
        "to the piece's end"
    );
    assert_eq!(&buffer[..2], b"lo");
    assert_eq!(file.read_at(5, &mut buffer).unwrap(), 1);
    assert_eq!(&buffer[..1], b",");
    assert_eq!(file.read_at(6, &mut buffer).unwrap(), 4);
    assert_eq!(&buffer, b" wor");
    assert_eq!(file.len().unwrap(), 12);
    assert_eq!(file.read_at(12, &mut buffer).unwrap(), 0);
}

#[test]
fn a_big_original_saves_by_pieces_and_a_stop_ends_it() {
    let original: Vec<u8> = (0..3 * BLOCK).map(|at| (at % 251) as u8).collect();
    let mut table = PieceTable::new(original.len() as u64);
    table.insert(BLOCK, b"--");
    let mut expected = original.clone();
    expected.splice(BLOCK as usize..BLOCK as usize, *b"--");
    assert_eq!(
        saved(&table, &original),
        expected,
        "never read before: from the disk"
    );
    let mut out = Vec::new();
    let stopped = table.write_to(&mut Memory(original), &mut out, &mut |_| false);
    assert_eq!(stopped.unwrap_err().kind(), io::ErrorKind::Interrupted);
}

/// The save hears the typed pieces' bytes too and stops in them; a file shorter than its pieces fails.
#[test]
fn the_typed_pieces_are_heard_and_stopped_and_a_short_file_fails() {
    let mut table = PieceTable::new(0);
    table.insert(0, b"typed");
    let mut heard = Vec::new();
    let mut out = Vec::new();
    let done = table.write_to(&mut Memory(Vec::new()), &mut out, &mut |done| {
        heard.push(done);
        true
    });
    done.unwrap();
    assert_eq!(
        (out.as_slice(), heard.as_slice()),
        (&b"typed"[..], &[5][..])
    );
    let stopped = table.write_to(&mut Memory(Vec::new()), &mut Vec::new(), &mut |_| false);
    assert_eq!(stopped.unwrap_err().kind(), io::ErrorKind::Interrupted);
    // Not read: from the file, which is shorter.
    let table = PieceTable::new(5);
    let short = table.write_to(&mut Memory(b"he".to_vec()), &mut Vec::new(), &mut |_| true);
    assert_eq!(short.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    // Read: from what was read, whatever the file has now (rewritten in place).
    let table = table_of(b"hello");
    let mut out = Vec::new();
    let kept = table.write_to(&mut Memory(b"HE".to_vec()), &mut out, &mut |_| true);
    kept.unwrap();
    assert_eq!(out, b"hello");
    assert!(table.original_held() && !PieceTable::new(5).original_held());
}

/// Review A1: a step stopped rolls back its edits and its changes of the undo and redo lists (the redo
/// step its edit dropped back); one done keeps them; one that changed nothing rolls back nothing.
#[test]
fn a_stopped_attempt_rolls_back_its_edits_and_both_lists() {
    let mut table = table_of(b"abc");
    table.insert(1, b"X");
    table.undo();
    // A second edit is kept from before the first one.
    table.begin_attempt();
    table.insert(0, b"Y");
    table.delete(2, 1);
    table.roll_back();
    assert_eq!(whole(&table), b"abc");
    assert_eq!(table.counts().1, 0, "the undo step it pushed gone");
    assert!(!table.is_changed());
    assert_eq!(table.redo(), Some(1), "the redo step it dropped back");
    assert_eq!(whole(&table), b"aXbc");
    // Nothing changed: nothing rolled back; done: kept.
    table.begin_attempt();
    table.roll_back();
    assert_eq!(whole(&table), b"aXbc");
    table.begin_attempt();
    table.delete(0, 1);
    table.end_attempt();
    table.roll_back();
    assert_eq!(whole(&table), b"Xbc", "a step done stays");
}

/// Review A1: undos and a redo of a step stopped rolled back in reverse: the steps back on their lists
/// in their order.
#[test]
fn a_stopped_attempt_rolls_back_its_undos_and_redos_in_reverse() {
    let mut table = table_of(b"abc");
    table.insert(1, b"X");
    table.insert(4, b"Z");
    table.begin_attempt();
    assert_eq!(table.undo(), Some(4));
    assert_eq!(table.undo(), Some(1));
    table.roll_back();
    assert_eq!(whole(&table), b"aXbcZ");
    assert_eq!(table.redo(), None, "the redo steps it pushed gone");
    assert_eq!(table.undo(), Some(4));
    assert_eq!(whole(&table), b"aXbc");
    // A redo rolled back: back on the redo list, off the undo one.
    table.begin_attempt();
    assert_eq!(table.redo(), Some(4));
    table.roll_back();
    assert_eq!(whole(&table), b"aXbc");
    assert_eq!(table.counts().1, 1);
    assert_eq!(table.redo(), Some(4));
    assert_eq!(whole(&table), b"aXbcZ");
}

/// A group begun by a step rolled back is gone too: the next edit is a step of its own.
#[test]
fn a_rolled_back_attempt_leaves_no_group_open() {
    let mut table = table_of(b"abc");
    table.insert(0, b"1");
    table.begin_attempt();
    table.begin_group();
    table.insert(0, b"2");
    table.roll_back();
    assert_eq!(table.counts().1, 1);
    table.insert(0, b"3");
    assert_eq!(table.counts().1, 2, "not taken into a group left open");
    assert_eq!(table.undo(), Some(0));
    assert_eq!(whole(&table), b"1abc");
}

/// FAR's `AddUndoData`: a character typed joins the last one's step only right after it, typed, with
/// no other edit, move, undo or redo between.
#[test]
fn only_characters_typed_one_after_another_join_a_step() {
    let typed = |table: &mut PieceTable, at: u64, text: &[u8]| {
        table.type_next();
        table.insert(at, text);
    };
    let mut table = table_of(b"hello");
    typed(&mut table, 0, b"a");
    typed(&mut table, 1, b"b");
    assert_eq!(table.counts().1, 1, "right after it");
    typed(&mut table, 7, b"c");
    assert_eq!(table.counts().1, 2, "typed elsewhere");
    table.end_typing();
    typed(&mut table, 8, b"d");
    assert_eq!(table.counts().1, 3, "a move between");
    table.delete(0, 1);
    typed(&mut table, 8, b"e");
    assert_eq!(table.counts().1, 5, "a deletion between");
    table.undo();
    table.redo();
    typed(&mut table, 9, b"f");
    assert_eq!(table.counts().1, 6, "an undo and a redo between");
    table.insert(10, b"gh");
    typed(&mut table, 12, b"i");
    assert_eq!(table.counts().1, 8, "after text put in, not typed");
    assert_eq!(whole(&table), b"bhellocdefghi");
}

/// The place typing goes on from forgotten by every other change: a deletion after it, an undo, a
/// redo (of nothing too), a group, the place being where the typing ended still; "All"'s replacement
/// asked takes the place of a character typed.
#[test]
fn typing_goes_on_in_its_step_only_from_where_it_ended_untouched() {
    let typed = |table: &mut PieceTable, at: u64, text: &[u8]| {
        table.type_next();
        table.insert(at, text);
    };
    let mut table = table_of(b"abcdef");
    typed(&mut table, 0, b"x");
    table.delete(3, 1);
    typed(&mut table, 1, b"y");
    assert_eq!(table.counts().1, 3, "a deletion past it between");
    table.redo();
    typed(&mut table, 2, b"z");
    assert_eq!(table.counts().1, 4, "a redo of nothing between");
    table.begin_group();
    table.end_group();
    typed(&mut table, 3, b"w");
    assert_eq!(table.counts().1, 5, "a group between");
    typed(&mut table, 4, b"v");
    table.undo();
    typed(&mut table, 5, b"u");
    assert_eq!(table.counts().1, 5, "an undo between: a step of its own");
    assert_eq!(table.undo(), Some(5));
    typed(&mut table, 4, b"t");
    table.type_next();
    table.join_next();
    table.insert(5, b"s");
    assert_eq!(table.counts().1, 6, "typed no more");
    typed(&mut table, 6, b"r");
    assert_eq!(table.counts().1, 7, "not after the replacement's text");
    assert_eq!(whole(&table), b"xyzatsrbdef");
    // Past the end: put in at the end, the step there.
    let mut table = table_of(b"ab");
    table.insert(10, b"c");
    assert_eq!(table.undo(), Some(2));
}

/// A step rolled back after it typed: the next character typed is a step of its own, not none -
/// with no step before it, or with the saved state's step last.
#[test]
fn typing_after_a_rolled_back_attempt_makes_its_own_step() {
    let attempted = |table: &mut PieceTable| {
        table.begin_attempt();
        table.type_next();
        table.insert(0, b"a");
        table.roll_back();
        table.type_next();
        table.insert(1, b"b");
    };
    let mut table = table_of(b"xyz");
    attempted(&mut table);
    assert_eq!(table.counts().1, 1);
    assert_eq!(table.undo(), Some(1));
    assert_eq!(whole(&table), b"xyz");
    let old = b"xyz".to_vec();
    let mut table = table_of(&old);
    table.insert(0, b"q");
    let mut moved = table.rebased(&mut Memory(old), 64).unwrap().unwrap();
    let mut gone = moved.clone();
    attempted(&mut moved);
    assert_eq!(moved.counts().1, 2, "not into the saved state's step");
    assert!(moved.is_changed());
    // The saved state gone, every step undone: none to join.
    gone.undo();
    gone.insert(0, b"p");
    gone.undo();
    attempted(&mut gone);
    assert_eq!(gone.counts().1, 1);
    assert_eq!(gone.undo(), Some(1));
}

/// A joined group that changed nothing makes no step to join: the next one is a step of its own, not
/// taken into the step before (or the saved state's).
#[test]
fn a_joined_group_joins_no_step_an_empty_one_did_not_make() {
    let joined = |table: &mut PieceTable, at: u64| {
        table.join_next();
        table.begin_group();
        table.insert(at, b"r");
        table.end_group();
    };
    let empty = |table: &mut PieceTable| {
        table.join_next();
        table.begin_group();
        table.end_group();
    };
    let mut table = table_of(b"text");
    empty(&mut table);
    joined(&mut table, 0);
    assert_eq!(table.counts().1, 1);
    assert_eq!(table.undo(), Some(0));
    assert_eq!(whole(&table), b"text");
    let old = b"text".to_vec();
    let mut table = table_of(&old);
    table.insert(0, b"q");
    let mut moved = table.rebased(&mut Memory(old), 64).unwrap().unwrap();
    let mut gone = moved.clone();
    empty(&mut moved);
    joined(&mut moved, 0);
    assert_eq!(moved.counts().1, 2, "not into the saved state's step");
    assert!(moved.is_changed());
    // The saved state gone, every step undone: none to join.
    gone.undo();
    gone.insert(0, b"p");
    gone.undo();
    empty(&mut gone);
    joined(&mut gone, 0);
    assert_eq!(gone.counts().1, 1);
    assert_eq!(gone.undo(), Some(0));
}

/// FAR's `undo_block`: groups join the last step only when both are "All"'s ones, nothing else
/// between them.
#[test]
fn only_joined_groups_one_after_another_join_a_step() {
    let group = |table: &mut PieceTable, join: bool, at: u64| {
        if join {
            table.join_next();
        }
        table.begin_group();
        table.insert(at, b"r");
        table.end_group();
    };
    let mut table = table_of(b"text");
    group(&mut table, false, 0);
    group(&mut table, true, 1);
    assert_eq!(table.counts().1, 2, "the step before was no joined one");
    group(&mut table, true, 2);
    assert_eq!(table.counts().1, 2, "joined");
    table.type_next();
    table.insert(3, b"t");
    group(&mut table, true, 4);
    assert_eq!(table.counts().1, 4, "typing between");
    group(&mut table, true, 5);
    table.end_typing();
    group(&mut table, true, 6);
    assert_eq!(table.counts().1, 5, "another step between");
    group(&mut table, false, 7);
    assert_eq!(table.counts().1, 6, "a group not joined");
    assert_eq!(whole(&table), b"rrrtrrrrtext");
}

/// Review A3: bytes typed after the last ones added grow their piece; elsewhere a piece of their own,
/// never an original's or another typed one's grown over bytes not theirs.
#[test]
fn only_bytes_added_right_after_the_last_ones_grow_their_piece() {
    let mut table = table_of(b"ab");
    table.insert(0, b"12");
    // The original's piece ends where the buffer does: still not grown.
    table.insert(4, b"Z");
    assert_eq!(saved(&table, b"ab"), b"12abZ");
    table.insert(5, b"W");
    assert_eq!(table.counts().0, 3, "grown");
    table.insert(1, b"Q");
    assert_eq!(
        whole(&table),
        b"1Q2abZW",
        "within a typed run: a piece of its own"
    );
    let mut table = table_of(b"ab");
    table.insert(0, b"");
    assert!(!table.is_changed(), "nothing put in: no step");
    table.insert(10, b"c");
    table.delete(1, 10);
    assert_eq!(whole(&table), b"a", "past the end: at the end, to the end");
    table.delete(5, 1);
    assert_eq!(table.counts().1, 2, "nothing there: no step");
}

/// A searching worker's copy: the bytes as they are, no undo or redo steps.
#[test]
fn a_reading_copy_has_the_bytes_and_no_steps() {
    let mut table = table_of(b"abc");
    table.insert(1, b"X");
    table.insert(0, b"Y");
    table.undo();
    let mut copy = table.reading_copy();
    assert_eq!(whole(&copy), b"aXbc");
    assert_eq!(copy.counts(), (table.counts().0, 0));
    assert_eq!((copy.undo(), copy.redo()), (None, None));
    assert_eq!(saved(&copy, b"abc"), b"aXbc");
}

/// Saved, undone past the save and edited: that saved state is gone, not reached again.
#[test]
fn the_saved_state_undone_and_edited_past_is_gone() {
    let old = b"abc".to_vec();
    let mut table = table_of(&old);
    table.insert(0, b"x");
    let mut moved = table.rebased(&mut Memory(old), 64).unwrap().unwrap();
    assert!(!moved.is_changed());
    moved.undo();
    moved.begin_attempt();
    moved.insert(0, b"y");
    moved.roll_back();
    moved.redo();
    assert!(!moved.is_changed(), "rolled back: the saved state kept");
    moved.undo();
    moved.insert(0, b"y");
    assert!(
        moved.is_changed(),
        "as many steps as at the save, not the saved text"
    );
}

/// R7 review F2: the pieces' first bytes, kept for the reads and made again past an edit, give every
/// byte where a plain copy has it - edits at the end first, in the middle, undone and done again,
/// between reads.
#[test]
fn reads_between_edits_find_every_byte_where_it_is() {
    let mut table = table_of(b"0123456789abcdefghij");
    let mut model = b"0123456789abcdefghij".to_vec();
    let check = |table: &PieceTable, model: &[u8]| {
        for pos in 0..model.len() as u64 {
            assert_eq!(table.at(pos).unwrap()[0], model[pos as usize], "at {pos}");
        }
        assert!(table.at(model.len() as u64).unwrap().is_empty());
    };
    check(&table, &model);
    for (at, cut, put) in [
        (18, 1, "X"),
        (12, 2, "YY"),
        (5, 0, "ZZZ"),
        (1, 3, ""),
        (0, 0, "<"),
    ] {
        table.delete(at, cut);
        table.insert(at, put.as_bytes());
        let at = at as usize;
        model.splice(at..at + cut as usize, put.bytes());
        check(&table, &model);
    }
    let before = model.clone();
    table.undo();
    table.redo();
    check(&table, &before);
}
