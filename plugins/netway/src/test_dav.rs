//! The tests' WebDAV server inside their process (ADR-0023 §4): `dav-server` over a folder of the disk
//! (its `LocalFs`, a lock system that grants every lock), served over HTTP/1.1 by `hyper` on a free port of
//! the loopback; any login taken.

use std::path::Path;

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;

/// The server running on its own runtime.
pub struct TestDav {
    pub port: u16,
    _runtime: tokio::runtime::Runtime,
}

impl TestDav {
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
        let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))?;
        let port = listener.local_addr()?.port();
        let handler = dav_server::DavHandler::builder()
            .filesystem(dav_server::localfs::LocalFs::new(root, false, false, false))
            .locksystem(dav_server::fakels::FakeLs::new())
            .build_handler();
        runtime.spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let handler = handler.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |request| {
                        let handler = handler.clone();
                        async move { Ok::<_, std::convert::Infallible>(handler.handle(request).await) }
                    });
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        Ok(Self {
            port,
            _runtime: runtime,
        })
    }
}
