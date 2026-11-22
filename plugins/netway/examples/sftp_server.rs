//! The tests' SSH, SFTP, FTP, WebDAV and S3 servers for the e2e stands (ADR-0023 §4; the server of the plugin's tests, user
//! `user`, password `pass`; S3's access key and secret in `test_s3`): `cargo run -p netway --example sftp_server -- <root>
//! [port]` prints `port <n> fingerprint <SHA256:...>`, then `ftp <n>`, `dav <n>` and `s3 <n>`, and serves `<root>` until
//! its input closes.

#[path = "../src/test_dav.rs"]
mod test_dav;
#[path = "../src/test_ftp.rs"]
mod test_ftp;
#[path = "../src/test_s3.rs"]
mod test_s3;
#[path = "../src/test_server.rs"]
mod test_server;
#[path = "../src/test_shell.rs"]
mod test_shell;

use std::io::BufRead;

// What the server's files use of the plugin, at the paths they name it by (`crate::...`).
use netway::{copy_params, scp, scp_listing};

fn main() {
    let Some(root) = std::env::args().nth(1) else {
        eprintln!("usage: sftp_server <root>");
        std::process::exit(2);
    };
    // A port given: the same server back after the stand ended it (its reconnecting).
    let port = (std::env::args().nth(2)).and_then(|port| port.parse().ok());
    let root = std::path::Path::new(&root);
    let started = match port {
        Some(port) => test_server::TestServer::try_start_at(root, port),
        None => test_server::TestServer::try_start(root),
    };
    let server = match started {
        Ok(server) => server,
        Err(error) => {
            eprintln!("sftp_server: {error}");
            std::process::exit(1);
        }
    };
    let fingerprint = test_server::TestServer::try_fingerprint().unwrap_or_default();
    println!("port {} fingerprint {fingerprint}", server.port);
    // The tests' FTP server over the same root, its port on the next line.
    let ftp = test_ftp::TestFtp::try_start(root);
    match &ftp {
        Ok(ftp) => println!("ftp {}", ftp.port),
        Err(error) => println!("ftp none {error}"),
    }
    // The tests' WebDAV server over the same root, its port on the third line.
    let dav = test_dav::TestDav::try_start(root);
    match &dav {
        Ok(dav) => println!("dav {}", dav.port),
        Err(error) => println!("dav none {error}"),
    }
    // The tests' S3 server over the same root (its folders the buckets), its port on the fourth line.
    let s3 = test_s3::TestS3::try_start(root);
    match &s3 {
        Ok(s3) => println!("s3 {}", s3.port),
        Err(error) => println!("s3 none {error}"),
    }
    // Served until the stand closes the input (or ends the process).
    for line in std::io::stdin().lock().lines() {
        if line.is_err() {
            break;
        }
    }
}
