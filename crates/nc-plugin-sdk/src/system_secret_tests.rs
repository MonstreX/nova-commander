use super::*;

const ENTROPY: &[u8] = b"nova-commander test";

#[test]
fn a_secret_comes_back_as_it_was_and_is_not_kept_as_it_was() {
    let secret = "pässword 1".as_bytes();
    let kept = protect(secret, ENTROPY).unwrap();
    assert!(
        !kept.windows(secret.len()).any(|part| part == secret),
        "not in the clear"
    );
    assert_eq!(unprotect(&kept, ENTROPY).unwrap(), secret);
}

#[test]
fn an_empty_secret_comes_back_empty() {
    let kept = protect(b"", ENTROPY).unwrap();
    assert!(!kept.is_empty(), "the system's header still");
    assert_eq!(unprotect(&kept, ENTROPY).unwrap(), b"");
}

#[test]
fn another_entropy_or_broken_data_is_refused() {
    let kept = protect(b"secret", ENTROPY).unwrap();
    assert!(unprotect(&kept, b"another").is_err());
    let mut broken = kept.clone();
    let last = broken.len() - 1;
    broken[last] ^= 0xff;
    assert!(unprotect(&broken, ENTROPY).is_err());
    assert!(unprotect(b"not protected", ENTROPY).is_err());
}
