use super::*;

#[test]
fn hex_reads_back_and_refuses_what_is_not_hex() {
    assert_eq!(hex(&[0, 0x0f, 0xa5, 0xff]), "000fa5ff");
    assert_eq!(unhex("000fa5ff"), Some(vec![0, 0x0f, 0xa5, 0xff]));
    assert_eq!(unhex("000FA5FF"), Some(vec![0, 0x0f, 0xa5, 0xff]));
    assert_eq!(unhex(""), Some(Vec::new()));
    assert_eq!(unhex("abc"), None, "odd");
    assert_eq!(unhex("zz"), None);
    assert_eq!(unhex("éé"), None, "not ASCII");
}

/// Windows keeps the password for the user: not in the clear, read back as it was.
#[cfg(windows)]
#[test]
fn a_kept_password_reads_back_and_is_not_in_the_clear() {
    let kept = keep("s3cr3t пароль").unwrap();
    assert!(!kept.contains(&hex("s3cr3t".as_bytes())), "{kept}");
    assert_eq!(read(&kept).unwrap(), "s3cr3t пароль");
    assert!(read("0011").is_err(), "not the system's");
    assert!(read("xyz").is_err(), "not hex");
}

/// Off Windows no secret is kept yet (the port's keyring, tech-debt [S7]).
#[cfg(not(windows))]
#[test]
fn off_windows_no_password_is_kept() {
    assert!(keep("secret").is_err());
}
