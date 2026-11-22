use super::*;

#[test]
fn iso_times_are_utc() {
    let seconds = seconds_of((2024, 3, 10), (10, 30, 45));
    assert_eq!(
        iso_time("2024-03-10T10:30:45.000Z"),
        Some(u32::try_from(seconds).unwrap())
    );
    assert_eq!(iso_time("yesterday"), None);
}

#[test]
fn buckets_and_objects_are_read() {
    let listed = buckets(
        "<ListAllMyBucketsResult><Buckets><Bucket><Name>docs</Name><CreationDate>2024-03-10T10:30:45.000Z</CreationDate></Bucket><Bucket><Name>b&amp;c</Name></Bucket></Buckets></ListAllMyBucketsResult>",
    )
    .unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].key, "docs/");
    assert!(listed[0].prefix && listed[0].modified.is_some());
    assert_eq!(listed[1].key, "b&c/");
    let page = objects(
        "<ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><IsTruncated>true</IsTruncated><Contents><Key>a.txt</Key><LastModified>2024-03-10T10:30:45.000Z</LastModified><Size>5</Size></Contents><CommonPrefixes><Prefix>deep/</Prefix></CommonPrefixes><NextContinuationToken>t1</NextContinuationToken></ListBucketResult>",
    )
    .unwrap();
    assert_eq!(page.objects.len(), 2);
    assert_eq!(
        (page.objects[0].key.as_str(), page.objects[0].size),
        ("a.txt", 5)
    );
    assert!(page.objects[1].prefix);
    assert_eq!(page.next.as_deref(), Some("t1"));
    let last = objects("<ListBucketResult><IsTruncated>false</IsTruncated><NextContinuationToken>x</NextContinuationToken></ListBucketResult>").unwrap();
    assert_eq!(last.next, None);
}

#[test]
fn errors_say_their_code_and_message() {
    assert_eq!(
        error(
            "<Error><Code>NoSuchKey</Code><Message>The specified key does not exist.</Message></Error>"
        ),
        Some("NoSuchKey: The specified key does not exist.".to_string())
    );
    assert_eq!(error("<Other/>"), None);
}

#[test]
fn a_multipart_uploads_id_is_read() {
    let created = "<InitiateMultipartUploadResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Bucket>docs</Bucket><Key>big.bin</Key><UploadId>u-1</UploadId></InitiateMultipartUploadResult>";
    assert_eq!(upload_id(created).as_deref(), Some("u-1"));
    assert_eq!(upload_id("<InitiateMultipartUploadResult/>"), None);
}

#[test]
fn a_wrong_region_tells_the_right_one() {
    let malformed = "<Error><Code>AuthorizationHeaderMalformed</Code><Message>The authorization header is malformed; the region 'us-east-1' is wrong; expecting 'eu-west-1'</Message><Region>eu-west-1</Region></Error>";
    assert_eq!(error_region(malformed).as_deref(), Some("eu-west-1"));
    assert_eq!(error_region("<Error><Code>NoSuchKey</Code></Error>"), None);
    assert_eq!(error_region("<Error><Region> </Region></Error>"), None);
}
