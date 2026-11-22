use super::*;

fn lines(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    (pairs.iter())
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn the_defaults_are_winscps_and_write_nothing() {
    let ssh = Ssh::default();
    assert_eq!(
        ssh.ciphers.join(","),
        "aes,chacha20,aesgcm,3des,WARN,des,blowfish,arcfour"
    );
    assert_eq!(ssh.kexes[0], "ntru-curve25519");
    assert_eq!(ssh.kexes[11], "WARN");
    assert_eq!((ssh.rekey_time, ssh.rekey_data.as_str()), (60, "1G"));
    assert!(ssh.try_agent && ssh.keyboard_interactive && ssh.ki_password);
    assert!(ssh.lines().is_empty());
}

#[test]
fn lists_keep_winscps_rules() {
    let list = algo_list("chacha20, AES,bogus,chacha20", &CIPHERS);
    assert_eq!(
        list.join(","),
        "chacha20,aes,aesgcm,3des,WARN,des,blowfish,arcfour"
    );
    assert_eq!(
        label(&KEXES, "dh-gex-sha1"),
        "Diffie-Hellman group exchange"
    );
}

#[test]
fn winscps_values_are_read_and_written_back() {
    let read = Ssh::read(&lines(&[
        ("Compression", "1"),
        ("Cipher", "aesgcm,aes"),
        ("KEX", "ecdh,WARN"),
        ("RekeyTime", "0"),
        ("RekeyBytes", "500M"),
        ("SshNoUserAuth", "1"),
        ("TryAgent", "0"),
        ("AuthKI", "0"),
        ("AuthKIPassword", "0"),
    ]));
    assert!(read.compression && read.no_user_auth);
    assert!(!read.try_agent && !read.keyboard_interactive && !read.ki_password);
    assert_eq!(&read.ciphers[..2], ["aesgcm", "aes"]);
    assert_eq!(&read.kexes[..2], ["ecdh", "WARN"]);
    assert_eq!((read.rekey_time, read.rekey_data.as_str()), (0, "500M"));
    assert_eq!(Ssh::read(&read.lines()), read);
}

#[test]
fn sizes_read_as_winscp_writes_them() {
    assert_eq!(bytes_of("1G"), Some(1 << 30));
    assert_eq!(bytes_of("500m"), Some(500 << 20));
    assert_eq!(bytes_of("100K"), Some(100 << 10));
    assert_eq!(bytes_of("4096"), Some(4096));
    assert_eq!(bytes_of("x"), None);
}

/// `russh` is offered the session's order of what it has: AES-GCM before AES, ECDH first; compression
/// only when on; the rekey limits.
#[test]
fn russh_is_offered_the_sessions_order() {
    let ssh = Ssh {
        compression: true,
        ciphers: algo_list("aesgcm,aes", &CIPHERS),
        kexes: algo_list("ecdh", &KEXES),
        rekey_time: 10,
        rekey_data: "1M".to_string(),
        ..Ssh::default()
    };
    let preferred = ssh.preferred();
    assert_eq!(preferred.cipher[0], cipher::AES_256_GCM);
    assert_eq!(preferred.cipher[2], cipher::AES_256_CTR);
    assert_eq!(preferred.kex[0], kex::CURVE25519);
    assert!(
        preferred
            .kex
            .iter()
            .any(|name| name.as_ref() == "ext-info-c")
    );
    assert_eq!(preferred.compression[0], compression::ZLIB_LEGACY);
    assert_eq!(Ssh::default().preferred().compression.len(), 1);
    let limits = ssh.limits();
    assert_eq!(limits.rekey_write_limit, 1 << 20);
    assert_eq!(limits.rekey_time_limit, Duration::from_secs(600));
}
