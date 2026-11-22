use super::*;

fn through(mut mode: TextMode, pieces: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for piece in pieces {
        out.extend(mode.convert(piece));
    }
    out.extend(mode.finish());
    out
}

#[test]
fn a_download_writes_lf_as_crlf_and_keeps_crlf() {
    assert_eq!(
        through(TextMode::download(), &[b"a\nb\r\nc\n"]),
        b"a\r\nb\r\nc\r\n"
    );
    assert_eq!(
        through(TextMode::download(), &[b"a\r", b"\nb\n"]),
        b"a\r\nb\r\n",
        "a CRLF split between pieces"
    );
    assert_eq!(
        through(TextMode::download(), &[b"a\rb"]),
        b"a\rb",
        "a lone CR kept"
    );
}

#[test]
fn an_upload_writes_crlf_as_lf() {
    assert_eq!(through(TextMode::upload(), &[b"a\r\nb\r\n"]), b"a\nb\n");
    assert_eq!(
        through(TextMode::upload(), &[b"a\r", b"\nb"]),
        b"a\nb",
        "split between pieces"
    );
    assert_eq!(
        through(TextMode::upload(), &[b"a\rb\r"]),
        b"a\rb\r",
        "lone CRs kept"
    );
    assert_eq!(through(TextMode::upload(), &[b"a\n"]), b"a\n");
}

#[test]
fn the_throttle_waits_only_with_a_limit_and_when_ahead() {
    let mut free = Throttle::new(0);
    assert_eq!(free.after(1 << 30), Duration::ZERO);
    let mut slow = Throttle::new(1024);
    let wait = slow.after(2048);
    // Two seconds' worth from the start, however long the test took to get here.
    let due = wait + slow.started.elapsed();
    assert!(wait <= Duration::from_secs(2), "{wait:?}");
    assert!(due >= Duration::from_millis(1999), "{due:?}");
}
