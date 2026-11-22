use super::*;

fn pair(a: &[u8], b: &[u8]) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let (first, second) = (folder.path().join("a"), folder.path().join("b"));
    std::fs::write(&first, a).unwrap();
    std::fs::write(&second, b).unwrap();
    (folder, first, second)
}

fn compared(a: &[u8], b: &[u8], ignore: Ignore) -> Contents {
    let (_folder, first, second) = pair(a, b);
    compare(&first, &second, ignore, LEAST_BLOCK, &|| false)
}

#[test]
fn byte_for_byte_over_several_blocks() {
    let big = vec![7u8; LEAST_BLOCK * 2 + 5];
    assert_eq!(compared(&big, &big, Ignore::Nothing), Contents::Same);
    let mut other = big.clone();
    other[LEAST_BLOCK + 3] = 8;
    assert_eq!(compared(&big, &other, Ignore::Nothing), Contents::Different);
    assert_eq!(
        compared(&big, &big[..big.len() - 1], Ignore::Nothing),
        Contents::Different
    );
    assert_eq!(compared(b"", b"", Ignore::Nothing), Contents::Same);
    assert_eq!(
        compared(b"a\r\n", b"a\n", Ignore::Nothing),
        Contents::Different
    );
}

#[test]
fn new_lines_of_any_kind_are_one() {
    let lines = Ignore::NewLines;
    assert_eq!(compared(b"a\r\nb\r\n", b"a\nb\n", lines), Contents::Same);
    assert_eq!(compared(b"a\rb", b"a\nb", lines), Contents::Same);
    assert_eq!(
        compared(b"a\r\n\r\nb", b"a\n\nb", lines),
        Contents::Same,
        "two lines"
    );
    assert_eq!(compared(b"a\n", b"a", lines), Contents::Different);
    assert_eq!(compared(b"a b", b"a  b", lines), Contents::Different);
}

#[test]
fn white_space_is_left_out() {
    let space = Ignore::Whitespace;
    assert_eq!(compared(b"a b\tc\r\n", b"abc", space), Contents::Same);
    assert_eq!(compared(b" x ", b"x", space), Contents::Same);
    assert_eq!(compared(b"a b", b"a c", space), Contents::Different);
}

#[test]
fn a_file_not_there_is_told_and_a_stop_ends_it() {
    let (folder, first, _) = pair(b"a", b"a");
    let none = folder.path().join("none");
    assert_eq!(
        compare(&first, &none, Ignore::Nothing, LEAST_BLOCK, &|| false),
        Contents::NotOpened
    );
    let (_folder, first, second) = pair(b"a", b"a");
    assert_eq!(
        compare(&first, &second, Ignore::Nothing, LEAST_BLOCK, &|| true),
        Contents::Stopped
    );
}

#[test]
fn a_new_line_split_by_a_blocks_end_is_one() {
    let mut split = vec![b'a'; LEAST_BLOCK - 1];
    split.extend_from_slice(b"\r\nz");
    let mut plain = vec![b'a'; LEAST_BLOCK - 1];
    plain.extend_from_slice(b"\nz");
    assert_eq!(compared(&split, &plain, Ignore::NewLines), Contents::Same);
    let mut spaces = vec![b'a'];
    spaces.extend(vec![b' '; LEAST_BLOCK * 3]);
    spaces.push(b'b');
    assert_eq!(compared(&spaces, b"ab", Ignore::Whitespace), Contents::Same);
}

#[test]
fn a_stop_is_heard_inside_a_long_run_of_white_space() {
    let (_folder, first, second) = pair(&vec![b' '; LEAST_BLOCK * 4], b"");
    let asked = std::cell::Cell::new(0);
    let stop = || {
        asked.set(asked.get() + 1);
        asked.get() > 2
    };
    assert_eq!(
        compare(&first, &second, Ignore::Whitespace, LEAST_BLOCK, &stop),
        Contents::Stopped
    );
}
