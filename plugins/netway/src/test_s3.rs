//! The tests' S3 server inside their process (ADR-0023 §4): `s3s` with `s3s-fs` over a folder of the disk
//! (its folders the buckets), the requests' SigV4 signatures checked against one access key, served over
//! HTTP/1.1 by `hyper` on a free port of the loopback (path-style addresses).

use std::path::Path;

use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;

/// The tests' access key and its secret.
pub const ACCESS_KEY: &str = "AKIANETWAYTESTS";
pub const SECRET: &str = "netway-tests-secret";

/// The server running on its own runtime.
pub struct TestS3 {
    pub port: u16,
    _runtime: tokio::runtime::Runtime,
}

impl TestS3 {
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
        let storage = s3s_fs::FileSystem::new(root)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let mut builder = s3s::service::S3ServiceBuilder::new(storage);
        builder.set_auth(s3s::auth::SimpleAuth::from_single(ACCESS_KEY, SECRET));
        let service = builder.build();
        runtime.spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let service = service.clone();
                tokio::spawn(async move {
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
