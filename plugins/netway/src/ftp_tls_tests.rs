use super::*;

#[test]
fn fingerprints_are_hex_pairs() {
    assert_eq!(fingerprint(&[0, 15, 255]), "00:0f:ff");
    let sha256 = digest(&ring::digest::SHA256, b"abc");
    assert!(sha256.starts_with("ba:78:16:bf"));
}

#[test]
fn known_certificates_are_kept_by_place() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("sub").join("known_certificates");
    assert!(known(&file, "h", 21).is_empty());
    keep(&file, "h", 21, "aa:bb").unwrap();
    keep(&file, "other", 21, "cc:dd").unwrap();
    assert_eq!(known(&file, "h", 21), ["aa:bb"]);
    assert!(known(&file, "h", 990).is_empty());
}

#[test]
fn the_question_shows_the_fingerprints_and_why() {
    let unknown = Unknown {
        sha256: "aa".to_string(),
        sha1: "bb".to_string(),
        reason: "invalid peer certificate: UnknownIssuer".to_string(),
    };
    let text = prompt(&unknown);
    assert!(text.starts_with("The server's certificate is not known."));
    assert!(text.contains("- SHA-256: aa\n- SHA-1: bb"));
    assert!(text.contains("Summary: invalid peer certificate: UnknownIssuer"));
    assert!(text.ends_with("Continue connecting and store the certificate?"));
    assert!(config(Vec::new(), Arc::default()).is_ok());
}

/// The session's client certificate is offered, a file without one refused; no reuse: no resumption.
#[test]
fn the_sessions_tls_settings_are_applied() {
    let pem = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/client.pem");
    let (chain, _) = client_certificate(&pem).unwrap();
    assert_eq!(chain.len(), 1);
    let tls = Tls {
        certificate_file: pem.display().to_string(),
        session_reuse: false,
    };
    let config = configured(Vec::new(), Arc::default(), &tls).unwrap();
    assert!(config.client_auth_cert_resolver.has_certs());
    let resumption = |config: &ClientConfig| format!("{:?}", config.resumption);
    assert!(resumption(&config).contains("Disabled"), "no reuse");
    let plain = config_default();
    assert!(!plain.client_auth_cert_resolver.has_certs());
    assert!(!resumption(&plain).contains("Disabled"), "reused");
    let temp = tempfile::tempdir().unwrap();
    let empty = temp.path().join("empty.pem");
    std::fs::write(&empty, "nothing").unwrap();
    let refused = client_certificate(&empty).unwrap_err();
    assert!(
        refused.starts_with("Cannot load certificate file"),
        "{refused}"
    );
    assert!(client_certificate(&temp.path().join("none.pem")).is_err());
}

fn config_default() -> ClientConfig {
    config(Vec::new(), Arc::default()).unwrap()
}
