use super::*;

/// RFC 2617's example: "Mufasa" with "Circle Of Life".
fn rfc_challenge() -> Challenge {
    challenge([
        "Basic realm=\"other\"",
        "Digest realm=\"testrealm@host.com\", qop=\"auth,auth-int\", nonce=\"dcd98b7102dd2f0e8b11d0f600bfb0c093\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"",
    ])
    .unwrap()
}

#[test]
fn the_challenge_is_read_among_the_offers() {
    let read = rfc_challenge();
    assert_eq!(read.realm, "testrealm@host.com");
    assert_eq!(read.nonce, "dcd98b7102dd2f0e8b11d0f600bfb0c093");
    assert_eq!(
        read.opaque.as_deref(),
        Some("5ccc069c403ebaf9f0171e9517f40e41")
    );
    assert!(read.qop_auth);
    assert_eq!(read.algorithm, "MD5");
    assert_eq!(challenge(["Basic realm=\"x\""]), None);
    assert_eq!(
        challenge(["Digest realm=\"x\", nonce=\"n\", algorithm=SHA-512-256"]),
        None
    );
    let escaped = challenge(["Digest realm=\"a \\\"b\\\"\",nonce=n1,algorithm=sha-256"]).unwrap();
    assert_eq!(
        (escaped.realm.as_str(), escaped.nonce.as_str()),
        ("a \"b\"", "n1")
    );
    assert_eq!(escaped.algorithm, "SHA-256");
    assert!(!escaped.qop_auth && escaped.opaque.is_none());
}

/// RFC 2617 section 3.5's response.
#[test]
fn the_response_is_rfc_2617s() {
    let value = authorization(
        &rfc_challenge(),
        ("Mufasa", "Circle Of Life"),
        ("GET", "/dir/index.html"),
        (1, "0a4f113b"),
    );
    assert!(
        value.contains("response=\"6629fae49393a05397450978507c4ef1\""),
        "{value}"
    );
    assert!(value.contains("qop=auth, nc=00000001, cnonce=\"0a4f113b\""));
    assert!(value.ends_with("opaque=\"5ccc069c403ebaf9f0171e9517f40e41\""));
    assert!(value.starts_with("Digest username=\"Mufasa\", realm=\"testrealm@host.com\""));
}

/// RFC 7616 section 3.9.1's SHA-256 response.
#[test]
fn the_sha_256_response_is_rfc_7616s() {
    let challenge = challenge([
        "Digest realm=\"http-auth@example.org\", qop=\"auth, auth-int\", algorithm=SHA-256, nonce=\"7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v\", opaque=\"FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS\"",
    ])
    .unwrap();
    let value = authorization(
        &challenge,
        ("Mufasa", "Circle of Life"),
        ("GET", "/dir/index.html"),
        (1, "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ"),
    );
    assert!(
        value.contains(
            "response=\"753927fa0e85d155564e2e272a28d1802ca10daf4496794697cf8db5856cb6c1\""
        ),
        "{value}"
    );
}

#[test]
fn client_nonces_differ() {
    let (one, two) = (client_nonce(), client_nonce());
    assert_eq!(one.len(), 32);
    assert_ne!(one, two);
}
