//! The line index: breaks of every kind, across pieces and growth, checkpoints, lines past them.

use super::*;

fn starts(units: Units, pieces: &[&[u8]]) -> Vec<u64> {
    let mut breaks = Breaks::new(units);
    let mut found = Vec::new();
    let mut at = 0;
    for piece in pieces {
        breaks.feed(at, piece, &mut |next| {
            found.push(next);
            true
        });
        at += piece.len() as u64;
    }
    breaks.finish(&mut |next| {
        found.push(next);
        true
    });
    found
}

#[test]
fn every_kind_of_break_ends_a_line() {
    assert_eq!(starts(Units::Bytes, &[b"a\nb\r\nc\rd"]), [2, 5, 7]);
    assert_eq!(
        starts(Units::Bytes, &[b"a\r", b"\nb"]),
        [3],
        "\\r\\n across pieces"
    );
    assert_eq!(
        starts(Units::Bytes, &[b"a\r", b"b"]),
        [2],
        "a lone \\r at a piece's end"
    );
    assert_eq!(
        starts(Units::Bytes, &[b"a\r"]),
        [2],
        "the file ends with \\r"
    );
    assert_eq!(starts(Units::Bytes, &[b"\n\n"]), [1, 2]);
    let le: Vec<u8> = "a\r\nb\nc"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(starts(Units::Utf16Le, &[&le]), [6, 10]);
    let be: Vec<u8> = "a\nb".encode_utf16().flat_map(u16::to_be_bytes).collect();
    assert_eq!(starts(Units::Utf16Be, &[&be]), [4]);
    // A unit whose high byte is a break's code is no break.
    let odd: Vec<u8> = "\u{0A41}"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(starts(Units::Utf16Le, &[&odd]), Vec::<u64>::new());
}

#[test]
fn a_counted_cr_at_the_end_pairs_with_the_lf_the_file_grows_with() {
    let mut breaks = Breaks::new(Units::Bytes);
    let mut found = Vec::new();
    breaks.feed(0, b"a\r", &mut |next| {
        found.push(next);
        true
    });
    breaks.finish(&mut |next| {
        found.push(next);
        true
    });
    breaks.feed(2, b"\nb\n", &mut |next| {
        found.push(next);
        true
    });
    assert_eq!(found, [2, 5], "no break of its own for the \\n");
    assert!(breaks.paired, "the line found at 2 starts past the \\n");
}

/// Review A8: a line kept as a checkpoint after a `\r` that ended the file starts past the `\n` the
/// file then grows with.
#[test]
fn a_checkpoint_after_a_counted_cr_moves_past_the_lf_the_file_grows_with() {
    let temp = tempfile::tempdir().unwrap();
    let text = format!("{}x\r", "l\n".repeat(1023));
    let path = file_with(&temp, text.as_bytes());
    let index = LineIndex::start(path.clone(), Units::Bytes, 0);
    assert_eq!(index.line_start(1024, &never), Ok(2048));
    let local = path.as_local().unwrap().to_path_buf();
    std::fs::write(&local, format!("{text}\nend\n")).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while index.line_start(1025, &never) == Err(NotFound::Past) {
        assert!(std::time::Instant::now() < deadline, "never grew");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(index.line_start(1024, &never), Ok(2049), "past the \\n");
    assert_eq!(index.line_start(1025, &never), Ok(2053));
}

/// Review A8: the `\n` paired with a counted `\r` moves only a checkpoint of the line after it, never
/// one of a line before (line 0's here).
#[test]
fn a_paired_lf_moves_no_checkpoint_of_another_line() {
    let temp = tempfile::tempdir().unwrap();
    let path = file_with(&temp, b"a\r");
    let index = LineIndex::start(path.clone(), Units::Bytes, 0);
    assert_eq!(index.line_start(1, &never), Ok(2));
    let local = path.as_local().unwrap().to_path_buf();
    std::fs::write(&local, b"a\r\nb\nc").unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while index.line_start(2, &never) == Err(NotFound::Past) {
        assert!(std::time::Instant::now() < deadline, "never grew");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(index.line_start(0, &never), Ok(0), "line 0 where it was");
    assert_eq!(index.line_start(2, &never), Ok(5));
}

fn file_with(temp: &tempfile::TempDir, bytes: &[u8]) -> VfsPath {
    let path = temp.path().join("lines.txt");
    std::fs::write(&path, bytes).unwrap();
    VfsPath::local(&path)
}

fn never() -> bool {
    false
}

#[test]
fn lines_are_found_past_their_checkpoints() {
    let temp = tempfile::tempdir().unwrap();
    let text: String = (0..5000).map(|n| format!("line {n}\n")).collect();
    let index = LineIndex::start(file_with(&temp, text.as_bytes()), Units::Bytes, 0);
    let offset = |line: usize| {
        text.lines()
            .take(line)
            .map(|it| it.len() as u64 + 1)
            .sum::<u64>()
    };
    for line in [0, 1, 1023, 1024, 1025, 3000, 4999, 5000] {
        assert_eq!(
            index.line_start(line, &never),
            Ok(offset(line as usize)),
            "line {line}"
        );
    }
    assert_eq!(index.line_start(5001, &never), Err(NotFound::Past));
    let state = index.shared.state();
    assert!(state.done && state.scanned == text.len() as u64);
    assert_eq!(state.checkpoints.len(), 5, "0, 1024, 2048, 3072, 4096");
}

#[test]
fn a_byte_order_mark_is_passed_and_utf16_counted_by_units() {
    let temp = tempfile::tempdir().unwrap();
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend("one\ntwo\nthree".encode_utf16().flat_map(u16::to_le_bytes));
    let index = LineIndex::start(file_with(&temp, &bytes), Units::Utf16Le, 2);
    assert_eq!(index.line_start(0, &never), Ok(2));
    assert_eq!(index.line_start(2, &never), Ok(2 + 16));
}

#[test]
fn a_file_that_grows_is_counted_on_and_a_wait_can_be_stopped() {
    let temp = tempfile::tempdir().unwrap();
    let path = file_with(&temp, b"a\nb\n");
    let index = LineIndex::start(path.clone(), Units::Bytes, 0);
    assert_eq!(index.line_start(3, &never), Err(NotFound::Past));
    let local = path.as_local().unwrap().to_path_buf();
    std::fs::write(&local, b"a\nb\nc\nd\n").unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        match index.line_start(3, &never) {
            Ok(start) => break assert_eq!(start, 6),
            Err(NotFound::Past) => assert!(std::time::Instant::now() < deadline, "never grew"),
            Err(other) => panic!("{other:?}"),
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let missing = LineIndex::start(VfsPath::local(temp.path().join("none")), Units::Bytes, 0);
    assert!(matches!(
        missing.line_start(5, &|| true),
        Err(NotFound::Failed(_) | NotFound::Stopped)
    ));
}
