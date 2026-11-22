use super::*;

#[test]
fn paths_are_encoded_and_decoded() {
    assert_eq!(encoded("/a b/ü.txt"), "/a%20b/%C3%BC.txt");
    assert_eq!(decoded("/a%20b/%C3%BC.txt"), "/a b/ü.txt");
    assert_eq!(decoded("100%"), "100%");
}

#[test]
fn http_dates_are_read() {
    let seconds = seconds_of((1994, 11, 6), (8, 49, 37));
    assert_eq!(
        http_date("Sun, 06 Nov 1994 08:49:37 GMT"),
        Some(u32::try_from(seconds).unwrap())
    );
    assert_eq!(http_date("nonsense"), None);
}

#[test]
fn a_multistatus_gives_its_resources() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/srv/</D:href><D:propstat><D:prop>
    <D:resourcetype><D:collection/></D:resourcetype>
  </D:prop></D:propstat></D:response>
  <D:response><D:href>http://host:8080/srv/a%20b.txt</D:href><D:propstat><D:prop>
    <D:resourcetype/><D:getcontentlength>5</D:getcontentlength>
    <D:getlastmodified>Sun, 06 Nov 1994 08:49:37 GMT</D:getlastmodified>
  </D:prop></D:propstat></D:response>
  <ns0:response xmlns:ns0="DAV:"><ns0:href>/srv/docs/</ns0:href><ns0:propstat><ns0:prop>
    <ns0:resourcetype><ns0:collection></ns0:collection></ns0:resourcetype>
  </ns0:prop></ns0:propstat></ns0:response>
</D:multistatus>"#;
    let read = multistatus(xml).unwrap();
    assert_eq!(read.len(), 3);
    assert!(read[0].entry.is_dir);
    assert_eq!(read[1].path, "/srv/a b.txt");
    assert_eq!(read[1].entry.name, "a b.txt");
    assert_eq!(read[1].entry.size, 5);
    assert!(read[1].entry.modified.is_some());
    assert!(read[2].entry.is_dir && read[2].entry.name == "docs");
}
