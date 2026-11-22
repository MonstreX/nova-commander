use super::*;

/// AWS's documented example ("Signature Calculations for the Authorization Header", GET Object).
#[test]
fn awss_get_object_example_signs_alike() {
    let signer = Signer {
        access_key: "AKIAIOSFODNN7EXAMPLE".to_string(),
        secret: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
        region: "us-east-1".to_string(),
    };
    let request = Request {
        method: "GET",
        path: "/test.txt",
        query: &[],
        headers: &[("range", "bytes=0-9")],
        host: "examplebucket.s3.amazonaws.com",
        payload: EMPTY_SHA256,
        stamp: "20130524T000000Z",
    };
    assert_eq!(
        signer.authorization(&request),
        "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
    );
}

#[test]
fn encodings_queries_and_stamps_are_awss() {
    assert_eq!(uri_encode("/a b/ü~x", true), "/a%20b/%C3%BC~x");
    assert_eq!(uri_encode("a/b", false), "a%2Fb");
    assert_eq!(
        canonical_query(&[("prefix", "a b/"), ("list-type", "2"), ("delimiter", "/")]),
        "delimiter=%2F&list-type=2&prefix=a%20b%2F"
    );
    assert_eq!(sha256(b""), EMPTY_SHA256);
    assert_eq!(stamp(1_369_353_600), "20130524T000000Z");
}
