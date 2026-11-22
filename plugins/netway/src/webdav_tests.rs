use super::*;
use crate::runtime::Runtime;
use crate::test_dav::TestDav;

fn dav(port: u16, link: &Link) -> Dav {
    let connect = Connect {
        host: "127.0.0.1",
        port,
        link,
        tls: None,
        user: "user",
        password: "pass",
        compression: false,
    };
    Dav::new(&connect).unwrap()
}

/// A folder named with or without its `/` is a folder, its kind kept in its attributes.
#[test]
fn a_folder_is_a_folder_either_way() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("srv/docs")).unwrap();
    let server = TestDav::start(temp.path());
    let runtime = Runtime::new().unwrap();
    let dav = dav(server.port, &Link::default());
    let work = async {
        let plain = dav.entry("/srv/docs").await?;
        let slashed = dav.entry("/srv/docs/").await?;
        let listed = dav.list("/srv").await?;
        Ok::<_, String>((plain, slashed, listed))
    };
    let (plain, slashed, listed) = runtime.wait(work, &|| false).unwrap().unwrap();
    assert!(plain.is_dir && slashed.is_dir && listed[0].is_dir);
    assert!(crate::scp::attributes(&plain).is_dir());
}

#[test]
fn only_socks_and_http_proxies_carry_webdav() {
    let mut link = Link {
        proxy: ProxyMethod::Socks5,
        proxy_host: "gate".to_string(),
        proxy_port: 1080,
        proxy_localhost: true,
        ..Link::default()
    };
    assert!(proxy_of("host", &link).unwrap().is_some());
    link.proxy = ProxyMethod::Telnet;
    assert!(proxy_of("host", &link).is_err());
    link.proxy = ProxyMethod::None;
    assert!(proxy_of("host", &link).unwrap().is_none());
}

/// Against the tests' server: folders listed, made, copied, moved and deleted; a file up, down and resumed.
#[test]
fn the_server_lists_makes_copies_moves_and_transfers() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("srv")).unwrap();
    std::fs::write(temp.path().join("srv/a b.txt"), "alpha").unwrap();
    let server = TestDav::start(temp.path());
    let runtime = Runtime::new().unwrap();
    let dav = dav(server.port, &Link::default());
    let work = async {
        let listed = dav.list("/srv").await?;
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!((listed[0].name.as_str(), listed[0].size), ("a b.txt", 5));
        dav.mkdir("/srv/made").await?;
        let mut up = dav.file("/srv/made/b.txt", true);
        up.write_all(b"beta").await.map_err(|e| e.to_string())?;
        up.shutdown().await.map_err(|e| e.to_string())?;
        let mut down = dav.file("/srv/made/b.txt", false);
        down.seek(1).map_err(|e| e.to_string())?;
        let mut read = Vec::new();
        let mut buffer = [0u8; 64];
        loop {
            let count = down.read(&mut buffer).await.map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            read.extend_from_slice(&buffer[..count]);
        }
        assert_eq!(read, b"eta", "resumed by a Range");
        dav.copy("/srv/made", "/srv/copied").await?;
        dav.rename("/srv/copied", "/srv/moved").await?;
        assert!(dav.entry("/srv/moved").await?.is_dir);
        assert_eq!(dav.canonicalize("srv").await?, "/srv");
        dav.delete("/srv/made").await?;
        Ok::<(), String>(())
    };
    runtime.wait(work, &|| false).unwrap().unwrap();
    assert_eq!(
        std::fs::read_to_string(temp.path().join("srv/moved/b.txt")).unwrap(),
        "beta"
    );
    assert!(!temp.path().join("srv/made").exists());
}

/// A scripted server: each connection one request read whole (its head lower-cased, its body skipped)
/// and answered with `(status, headers, body)`; the heads it got.
fn scripted(
    answers: Vec<(&'static str, String, Vec<u8>)>,
) -> (u16, std::thread::JoinHandle<Vec<String>>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let mut heads = Vec::new();
        for (status, headers, body) in answers {
            let (mut stream, _) = listener.accept().unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap() == 1 {
                head.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&head).to_lowercase();
            let length = (head.lines())
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|length| length.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let mut sent = vec![0u8; length];
            stream.read_exact(&mut sent).unwrap();
            let reply = format!(
                "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(reply.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
            heads.push(head);
        }
        heads
    });
    (port, server)
}

const FOLDER: &str = "<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\"><D:response><D:href>/docs/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>";

/// The header's quoted `name="value"`.
fn quoted<'a>(head: &'a str, name: &str) -> &'a str {
    let start = head.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
    let end = head[start..].find('"').unwrap();
    &head[start..start + end]
}

/// A Digest challenge answered (RFC 2617's response, checked here by its own formula), the next request
/// answering it again with the next count.
#[test]
fn a_digest_challenge_is_answered() {
    let challenge =
        "WWW-Authenticate: Digest realm=\"dav\", nonce=\"n1\", qop=\"auth\"\r\n".to_string();
    let (port, server) = scripted(vec![
        ("401 Unauthorized", challenge, Vec::new()),
        (
            "207 Multi-Status",
            String::new(),
            FOLDER.as_bytes().to_vec(),
        ),
        ("201 Created", String::new(), Vec::new()),
    ]);
    let dav = dav(port, &Link::default());
    let runtime = Runtime::new().unwrap();
    let work = async {
        assert_eq!(dav.canonicalize("/docs").await?, "/docs");
        dav.mkdir("/docs/made").await
    };
    runtime.wait(work, &|| false).unwrap().unwrap();
    let heads = server.join().unwrap();
    assert!(
        !heads[0].contains("authorization"),
        "no password before the challenge"
    );
    let answered = &heads[1];
    assert!(
        answered.contains("authorization: digest username=\"user\""),
        "{answered}"
    );
    assert!(answered.contains("nc=00000001"));
    let hex = |text: String| format!("{:x}", md5::compute(text));
    let ha1 = hex("user:dav:pass".to_string());
    let ha2 = hex("propfind:/docs".to_string());
    let cnonce = quoted(answered, "cnonce");
    let wanted = hex(format!("{ha1}:n1:00000001:{cnonce}:auth:{ha2}"));
    // The head is lower-cased: the method in the hash is "PROPFIND".
    let ha2_upper = hex("PROPFIND:/docs".to_string());
    let wanted_upper = hex(format!("{ha1}:n1:00000001:{cnonce}:auth:{ha2_upper}"));
    let response = quoted(answered, "response");
    assert!(response == wanted_upper && response != wanted, "{answered}");
    assert!(heads[2].contains("nc=00000002"), "{}", heads[2]);
    assert!(heads[2].starts_with("mkcol /docs/made "));
}

/// A server asking for Basic gets it once it asked, then on each request; a second refusal is the
/// answer (no endless retries).
#[test]
fn basic_goes_only_when_asked() {
    let basic = "WWW-Authenticate: Basic realm=\"dav\"\r\n".to_string();
    let (port, server) = scripted(vec![
        ("401 Unauthorized", basic.clone(), Vec::new()),
        (
            "207 Multi-Status",
            String::new(),
            FOLDER.as_bytes().to_vec(),
        ),
        ("401 Unauthorized", basic, Vec::new()),
    ]);
    let dav = dav(port, &Link::default());
    let runtime = Runtime::new().unwrap();
    let work = async {
        assert_eq!(dav.canonicalize("/docs").await?, "/docs");
        dav.mkdir("/docs/made").await
    };
    let refused = runtime.wait(work, &|| false).unwrap();
    assert!(refused.is_err(), "the second refusal is said");
    let heads = server.join().unwrap();
    assert!(!heads[0].contains("authorization"));
    assert!(heads[1].contains("authorization: basic "), "{}", heads[1]);
    assert!(heads[2].contains("authorization: basic "), "{}", heads[2]);
}

/// Basic offered first and Digest after it: Digest answers (neon's preference), not the order.
#[test]
fn digest_is_preferred_when_both_are_offered() {
    let both = concat!(
        "WWW-Authenticate: Basic realm=\"dav\"\r\n",
        "WWW-Authenticate: Digest realm=\"dav\", nonce=\"n1\", qop=\"auth\"\r\n"
    );
    let (port, server) = scripted(vec![
        ("401 Unauthorized", both.to_string(), Vec::new()),
        (
            "207 Multi-Status",
            String::new(),
            FOLDER.as_bytes().to_vec(),
        ),
    ]);
    let dav = dav(port, &Link::default());
    let runtime = Runtime::new().unwrap();
    let listed = runtime.wait(dav.canonicalize("/docs"), &|| false).unwrap();
    assert_eq!(listed.as_deref(), Ok("/docs"));
    let heads = server.join().unwrap();
    assert!(
        heads[1].contains("authorization: digest username=\"user\""),
        "{}",
        heads[1]
    );
}

/// Basic named in any case, with or without its realm, is Basic.
#[test]
fn basic_is_known_in_any_case() {
    for offer in ["BASIC", "bAsIc realm=\"dav\""] {
        let header = format!("WWW-Authenticate: {offer}\r\n");
        let (port, server) = scripted(vec![
            ("401 Unauthorized", header, Vec::new()),
            (
                "207 Multi-Status",
                String::new(),
                FOLDER.as_bytes().to_vec(),
            ),
        ]);
        let dav = dav(port, &Link::default());
        let runtime = Runtime::new().unwrap();
        let listed = runtime.wait(dav.canonicalize("/docs"), &|| false).unwrap();
        assert_eq!(listed.as_deref(), Ok("/docs"), "{offer}");
        let heads = server.join().unwrap();
        assert!(heads[1].contains("authorization: basic "), "{offer}");
    }
}

/// A Digest refused with a new challenge (a stale nonce) answers the new one from its first count; Basic
/// asked after Digest is sent (only Basic refused twice is the answer).
#[test]
fn a_new_challenge_is_answered_again() {
    let digest = |nonce: &str| {
        format!("WWW-Authenticate: Digest realm=\"dav\", nonce=\"{nonce}\", qop=\"auth\"\r\n")
    };
    let basic = "WWW-Authenticate: Basic realm=\"dav\"\r\n".to_string();
    let (port, server) = scripted(vec![
        ("401 Unauthorized", digest("n1"), Vec::new()),
        (
            "207 Multi-Status",
            String::new(),
            FOLDER.as_bytes().to_vec(),
        ),
        ("401 Unauthorized", digest("n2"), Vec::new()),
        ("201 Created", String::new(), Vec::new()),
        ("401 Unauthorized", basic, Vec::new()),
        ("201 Created", String::new(), Vec::new()),
    ]);
    let dav = dav(port, &Link::default());
    let runtime = Runtime::new().unwrap();
    let work = async {
        assert_eq!(dav.canonicalize("/docs").await?, "/docs");
        dav.mkdir("/docs/a").await?;
        dav.mkdir("/docs/b").await
    };
    runtime.wait(work, &|| false).unwrap().unwrap();
    let heads = server.join().unwrap();
    assert!(heads[2].contains("nonce=\"n1\"") && heads[2].contains("nc=00000002"));
    assert!(
        heads[3].contains("nonce=\"n2\"") && heads[3].contains("nc=00000001"),
        "{}",
        heads[3]
    );
    assert!(heads[4].contains("nonce=\"n2\""), "{}", heads[4]);
    assert!(heads[5].contains("authorization: basic "), "{}", heads[5]);
}

/// A listing whose hrefs end in "..", "." and a NUL besides a plain file.
const ODD_FOLDER: &str = concat!(
    "<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\">",
    "<D:response><D:href>/docs/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/>",
    "</D:resourcetype></D:prop></D:propstat></D:response>",
    "<D:response><D:href>/docs/../</D:href><D:propstat><D:prop><D:resourcetype><D:collection/>",
    "</D:resourcetype></D:prop></D:propstat></D:response>",
    "<D:response><D:href>/docs/./</D:href><D:propstat><D:prop><D:resourcetype><D:collection/>",
    "</D:resourcetype></D:prop></D:propstat></D:response>",
    "<D:response><D:href>/docs/a%00b</D:href><D:propstat><D:prop><D:resourcetype/>",
    "</D:prop></D:propstat></D:response>",
    "<D:response><D:href>/docs/ok.txt</D:href><D:propstat><D:prop><D:resourcetype/>",
    "</D:prop></D:propstat></D:response>",
    "</D:multistatus>"
);

/// Whatever the server lists, the session's listings (the panel's and the raw one) keep only the names
/// of items of the folder (WinSCP's `IsValidFile`).
#[test]
fn the_sessions_listings_drop_names_that_leave_the_folder() {
    let listing = || {
        (
            "207 Multi-Status",
            String::new(),
            ODD_FOLDER.as_bytes().to_vec(),
        )
    };
    let (port, server) = scripted(vec![listing(), listing(), listing()]);
    let options = crate::session_options::Options::default();
    let session = crate::fs::Fs::webdav(dav(port, &Link::default()), &options);
    let plain = dav(port, &Link::default());
    let runtime = Runtime::new().unwrap();
    let work = async {
        let all = plain.list("/docs").await?;
        let shown = session.list("/docs").await?;
        let raw = session.list_raw("/docs").await?;
        Ok::<_, String>((all, shown, raw))
    };
    let (all, shown, raw) = runtime.wait(work, &|| false).unwrap().unwrap();
    let names = |entries: &[RemoteEntry]| -> Vec<String> {
        entries.iter().map(|entry| entry.name.clone()).collect()
    };
    assert_eq!(names(&all), ["..", ".", "a\0b", "ok.txt"], "the server's");
    assert_eq!(names(&shown), ["ok.txt"]);
    assert_eq!(names(&raw), ["ok.txt"]);
    assert_eq!(server.join().unwrap().len(), 3);
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut packer = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    packer.write_all(bytes).unwrap();
    packer.finish().unwrap()
}

/// "Enable compression": the listing and a whole download asked gzipped and unpacked; a ranged one not.
#[test]
fn gzipped_answers_are_unpacked() {
    let content: Vec<u8> = (0..200_000u32).map(|at| (at % 7) as u8 + b'a').collect();
    let packed = "Content-Encoding: gzip\r\n".to_string();
    let (port, server) = scripted(vec![
        ("207 Multi-Status", packed.clone(), gzip(FOLDER.as_bytes())),
        ("200 OK", packed, gzip(&content)),
        ("206 Partial Content", String::new(), content[5..].to_vec()),
    ]);
    let connect = Connect {
        host: "127.0.0.1",
        port,
        link: &Link::default(),
        tls: None,
        user: "",
        password: "",
        compression: true,
    };
    let dav = Dav::new(&connect).unwrap();
    let runtime = Runtime::new().unwrap();
    let read_all = |mut file: DavFile| async move {
        let mut read = Vec::new();
        let mut buffer = vec![0u8; 8192];
        loop {
            let count = file.read(&mut buffer).await.map_err(|e| e.to_string())?;
            if count == 0 {
                return Ok::<Vec<u8>, String>(read);
            }
            read.extend_from_slice(&buffer[..count]);
        }
    };
    let work = async {
        assert_eq!(dav.canonicalize("/docs").await?, "/docs");
        let whole = read_all(dav.file("/docs/a.txt", false)).await?;
        let mut ranged = dav.file("/docs/a.txt", false);
        ranged.seek(5).map_err(|e| e.to_string())?;
        let rest = read_all(ranged).await?;
        Ok::<_, String>((whole, rest))
    };
    let (whole, rest) = runtime.wait(work, &|| false).unwrap().unwrap();
    assert!(whole == content, "unpacked whole");
    assert!(rest == content[5..], "the range as it came");
    let heads = server.join().unwrap();
    assert!(heads[0].contains("accept-encoding: gzip"));
    assert!(heads[1].contains("accept-encoding: gzip"));
    assert!(!heads[2].contains("accept-encoding: gzip") && heads[2].contains("range: bytes=5-"));
}
