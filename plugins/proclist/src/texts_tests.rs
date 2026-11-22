use super::*;

#[test]
fn a_text_is_read_in_pieces_and_ends() {
    let text = b"0123456789";
    let mut buffer = [0u8; 4];
    assert_eq!(slice(text, 0, &mut buffer), 4);
    assert_eq!(&buffer, b"0123");
    assert_eq!(slice(text, 8, &mut buffer), 2);
    assert_eq!(&buffer[..2], b"89");
    assert_eq!(slice(text, 10, &mut buffer), 0, "at its end");
    assert_eq!(slice(text, u64::MAX, &mut buffer), 0, "past it");
}

#[test]
fn a_text_is_stale_after_its_age() {
    let made = Instant::now();
    let second = Duration::from_secs(1);
    assert!(!stale(made, made, second));
    assert!(!stale(made, made + Duration::from_millis(999), second));
    assert!(stale(made, made + second, second));
    assert!(!stale(made + second, made, second), "a clock before it");
}
