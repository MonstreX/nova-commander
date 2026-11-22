use super::*;
use crate::runtime::Runtime;
use crate::test_s3::{ACCESS_KEY, SECRET, TestS3};

fn s3(port: u16, link: &Link) -> S3 {
    let connect = Connect {
        host: "127.0.0.1",
        port,
        link,
        tls: None,
        signer: Signer {
            access_key: ACCESS_KEY.to_string(),
            secret: SECRET.to_string(),
            region: "us-east-1".to_string(),
        },
        options: &Default::default(),
    };
    S3::new(connect).unwrap()
}

#[test]
fn paths_split_into_bucket_and_key() {
    assert_eq!(split("/"), ("", ""));
    assert_eq!(split("/docs"), ("docs", ""));
    assert_eq!(split("/docs/a/b.txt"), ("docs", "a/b.txt"));
    assert!(needs_path_style("127.0.0.1") && needs_path_style("localhost"));
    assert!(!needs_path_style("s3.amazonaws.com"));
}

/// Against the tests' server: buckets at the root, folders by prefixes, a file up, down from an offset,
/// copied, renamed and deleted, a folder made and removed.
#[test]
fn the_server_lists_makes_copies_and_transfers() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("docs/deep")).unwrap();
    std::fs::write(temp.path().join("docs/a.txt"), "alpha").unwrap();
    std::fs::write(temp.path().join("docs/deep/b.txt"), "beta").unwrap();
    let server = TestS3::start(temp.path());
    let runtime = Runtime::new().unwrap();
    let s3 = s3(server.port, &Link::default());
    let made = temp.path().join("docs/made");
    let work = async {
        let root = s3.list("/").await?;
        assert!(
            root.iter()
                .any(|entry| entry.name == "docs" && entry.is_dir),
            "{root:?}"
        );
        let listed = s3.list("/docs").await?;
        assert!(
            listed
                .iter()
                .any(|entry| entry.name == "a.txt" && entry.size == 5)
        );
        assert!(
            listed
                .iter()
                .any(|entry| entry.name == "deep" && entry.is_dir)
        );
        let mut up = s3.file("/docs/c.txt", Some(5));
        up.write_all(b"gamma").await.map_err(|e| e.to_string())?;
        up.shutdown().await.map_err(|e| e.to_string())?;
        let mut down = s3.file("/docs/c.txt", None);
        down.seek(2).map_err(|e| e.to_string())?;
        let mut read = Vec::new();
        let mut buffer = [0u8; 64];
        loop {
            let count = down.read(&mut buffer).await.map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            read.extend_from_slice(&buffer[..count]);
        }
        assert_eq!(read, b"mma", "resumed by a Range");
        assert_eq!(s3.entry("/docs/c.txt").await?.size, 5);
        assert!(s3.entry("/docs/deep").await?.is_dir);
        s3.copy("/docs/c.txt", "/docs/d.txt").await?;
        s3.rename("/docs/d.txt", "/docs/e.txt").await?;
        // The tests' server keeps a `made/` object as an empty folder on its disk (and answers no HEAD
        // of it, as S3 does of the object), so the folder is checked there.
        s3.mkdir("/docs/made").await?;
        assert!(made.is_dir());
        s3.rmdir("/docs/made").await?;
        assert!(!made.exists());
        s3.remove("/docs/c.txt").await?;
        assert_eq!(s3.canonicalize("docs").await?, "/docs");
        Ok::<(), String>(())
    };
    runtime.wait(work, &|| false).unwrap().unwrap();
    assert_eq!(
        std::fs::read_to_string(temp.path().join("docs/e.txt")).unwrap(),
        "gamma"
    );
    assert!(!temp.path().join("docs/c.txt").exists());
    assert!(!temp.path().join("docs/d.txt").exists());
}

/// A request's head read off `stream`, an answer of `status` with `body` sent, the connection closed.
fn answer(mut stream: std::net::TcpStream, status: &str, body: &str) -> String {
    use std::io::{Read, Write};
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap() == 1 {
        head.push(byte[0]);
    }
    let reply = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(reply.as_bytes()).unwrap();
    String::from_utf8_lossy(&head).to_lowercase()
}

/// A bucket in another region (WinSCP's region detection): refused with the region it is in, asked again
/// there, and then asked there at once.
#[test]
fn a_bucket_in_another_region_is_asked_there() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let wrong =
            "<Error><Code>AuthorizationHeaderMalformed</Code><Region>eu-west-1</Region></Error>";
        let listed = "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>";
        let mut heads = Vec::new();
        for (index, stream) in listener.incoming().take(3).enumerate() {
            let (status, body) = match index {
                0 => ("400 Bad Request", wrong),
                _ => ("200 OK", listed),
            };
            heads.push(answer(stream.unwrap(), status, body));
        }
        heads
    });
    let runtime = Runtime::new().unwrap();
    let s3 = s3(port, &Link::default());
    let work = async {
        s3.list("/docs").await?;
        s3.list("/docs").await
    };
    assert_eq!(runtime.wait(work, &|| false).unwrap(), Ok(Vec::new()));
    let heads = server.join().unwrap();
    assert!(
        heads[0].contains("/us-east-1/s3/aws4_request"),
        "{}",
        heads[0]
    );
    assert!(
        heads[1].contains("/eu-west-1/s3/aws4_request"),
        "{}",
        heads[1]
    );
    assert!(
        heads[2].contains("/eu-west-1/s3/aws4_request"),
        "{}",
        heads[2]
    );
}

/// "Requester pays": each request says so, signed; the S3 tab's URL style asks for path-style addresses.
#[test]
fn the_s3_tabs_settings_go_into_the_requests() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        answer(stream, "200 OK", "<ListAllMyBucketsResult/>")
    });
    let options = crate::protocol_options::S3 {
        requester_pays: true,
        path_style: true,
        ..Default::default()
    };
    let link = Link::default();
    let connect = Connect {
        host: "s3.example.com",
        port,
        link: &link,
        tls: None,
        signer: Signer {
            access_key: ACCESS_KEY.to_string(),
            secret: SECRET.to_string(),
            region: "us-east-1".to_string(),
        },
        options: &options,
    };
    let s3 = S3::new(connect).unwrap();
    assert!(s3.path_style, "asked, though the host has dots");
    assert_eq!(s3.place("docs", "a.txt").1, "/docs/a.txt");
    // The root's request goes to the test server (its host names the endpoint only).
    let mut local = s3.clone();
    local.host = format!("127.0.0.1:{port}");
    let runtime = Runtime::new().unwrap();
    assert!(runtime.wait(local.list("/"), &|| false).unwrap().is_ok());
    let head = server.join().unwrap();
    assert!(head.contains("x-amz-request-payer: requester"), "{head}");
    assert!(
        head.contains("signedheaders=host;x-amz-content-sha256;x-amz-date;x-amz-request-payer,"),
        "signed: {head}"
    );
}

/// WinSCP's parts: 5 MiB each, larger when 10 000 would not hold the file; one part is no multipart.
#[test]
fn files_over_five_mib_go_up_in_parts() {
    use multipart::{MAX_PARTS, MIN_PART, Multipart, completion, parts_of};
    assert_eq!(parts_of(0), (1, MIN_PART));
    assert_eq!(parts_of(MIN_PART), (1, MIN_PART));
    assert_eq!(parts_of(MIN_PART + 1), (2, MIN_PART));
    let huge = MIN_PART * MAX_PARTS * 2;
    assert_eq!(parts_of(huge), (MAX_PARTS, MIN_PART * 2));
    assert!(Multipart::for_size(MIN_PART).is_none());
    assert!(Multipart::for_size(MIN_PART + 1).is_some());
    assert_eq!(
        completion(&["\"a\"".to_string(), "\"b\"".to_string()]),
        "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>\"a\"</ETag></Part><Part><PartNumber>2</PartNumber><ETag>\"b\"</ETag></Part></CompleteMultipartUpload>"
    );
}

/// A file of 11 MiB through the tests' server: three parts, completed whole.
#[test]
fn a_big_file_is_uploaded_in_parts() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("docs")).unwrap();
    let server = TestS3::start(temp.path());
    let runtime = Runtime::new().unwrap();
    let s3 = s3(server.port, &Link::default());
    let data: Vec<u8> = (0..11 * 1024 * 1024)
        .map(|at: u32| (at % 251) as u8)
        .collect();
    let size = data.len() as u64;
    let work = async {
        let mut up = s3.file("/docs/big.bin", Some(size));
        assert!(up.parts.is_some());
        for piece in data.chunks(300_000) {
            up.write_all(piece).await.map_err(|e| e.to_string())?;
        }
        up.shutdown().await.map_err(|e| e.to_string())?;
        assert_eq!(up.parts.as_ref().map(|parts| parts.etags.len()), Some(3));
        Ok::<(), String>(())
    };
    runtime.wait(work, &|| false).unwrap().unwrap();
    assert!(std::fs::read(temp.path().join("docs/big.bin")).unwrap() == data);
    let short = async {
        let mut up = s3.file("/docs/short.bin", Some(size));
        up.write_all(&data[..MIN_SHORT])
            .await
            .map_err(|e| e.to_string())?;
        up.shutdown().await.map_err(|e| e.to_string())
    };
    let failed = runtime.wait(short, &|| false).unwrap().unwrap_err();
    assert!(failed.contains("ended early"), "{failed}");
    assert!(!temp.path().join("docs/short.bin").exists());
}

/// Less than the file: an upload ended early.
const MIN_SHORT: usize = 6 * 1024 * 1024;

/// A wrong secret is refused by the server's signature check.
#[test]
fn a_wrong_secret_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestS3::start(temp.path());
    let runtime = Runtime::new().unwrap();
    let mut wrong = s3(server.port, &Link::default());
    wrong.signer.secret = "wrong".to_string();
    let listed = runtime.wait(wrong.list("/"), &|| false).unwrap();
    assert!(listed.is_err());
}
