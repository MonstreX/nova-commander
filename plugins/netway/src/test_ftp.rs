//! The tests' FTP server inside their process (ADR-0023 §4): `libunftp` over a folder of the disk, any
//! user and password let in (its anonymous authenticator), on a free port of the loopback.

use std::path::Path;

/// The server running on its own runtime.
pub struct TestFtp {
    pub port: u16,
    _runtime: tokio::runtime::Runtime,
}

impl TestFtp {
    /// A server over `root` (the tests').
    #[cfg(test)]
    pub fn start(root: &Path) -> Self {
        Self::try_start(root).unwrap()
    }

    pub fn try_start(root: &Path) -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let free = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = free.local_addr()?.port();
        drop(free);
        // The root opens now, or the server is not started.
        unftp_sbe_fs::Filesystem::new(root)?;
        let root = root.to_path_buf();
        let server = libunftp::ServerBuilder::new(Box::new(move || {
            match unftp_sbe_fs::Filesystem::new(root.clone()) {
                Ok(storage) => storage,
                // The root went away while serving: the tests' server cannot go on.
                Err(error) => {
                    eprintln!("the tests' FTP server lost its root: {error}");
                    std::process::exit(1)
                }
            }
        }))
        .greeting("NetWay's tests' FTP server")
        .build()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
        runtime.spawn(async move { server.listen(format!("127.0.0.1:{port}")).await });
        // Up once it answers.
        for _ in 0..100 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Ok(Self {
            port,
            _runtime: runtime,
        })
    }
}
