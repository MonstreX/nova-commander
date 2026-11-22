//! FTPS's certificates as WinSCP checks them (`TFTPFileSystem::VerifyCertificate`): against the system's
//! trusted roots; one that does not verify is asked about with WinSCP's `MSG_VERIFY_CERT_PROMPT3` (its
//! SHA-256 and SHA-1 fingerprints and why it failed) - Yes keeps it in the plugin's known certificates, No
//! takes it this time, Cancel refuses - and a known one is taken again without asking. The session's TLS
//! settings (S6.6e): its client certificate (`TlsCertificateFile`, PEM: the certificates and the key) and
//! whether a TLS session is resumed (`SslSessionReuse`).

use std::path::Path;
use std::sync::{Arc, Mutex};

use suppaftp::tokio_rustls::rustls::client::WebPkiServerVerifier;
use suppaftp::tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use suppaftp::tokio_rustls::rustls::pki_types::pem::PemObject;
use suppaftp::tokio_rustls::rustls::pki_types::{
    CertificateDer, PrivateKeyDer, ServerName, UnixTime,
};
use suppaftp::tokio_rustls::rustls::{
    self, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};

use crate::protocol_options::Tls;

/// A certificate that did not verify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unknown {
    pub sha256: String,
    pub sha1: String,
    /// Why it did not verify.
    pub reason: String,
}

/// Bytes as WinSCP shows a fingerprint: hex pairs with colons.
pub fn fingerprint(bytes: &[u8]) -> String {
    let pairs: Vec<String> = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    pairs.join(":")
}

fn digest(algorithm: &'static ring::digest::Algorithm, der: &[u8]) -> String {
    fingerprint(ring::digest::digest(algorithm, der).as_ref())
}

/// WinSCP's `MSG_VERIFY_CERT_PROMPT3` with `MSG_CERT_TEXT2`'s fingerprints and summary.
pub fn prompt(unknown: &Unknown) -> String {
    format!(
        "The server's certificate is not known. You have no guarantee that the server is the computer you think it is.\n \nServer's certificate details follow:\n \nFingerprints:\n- SHA-256: {}\n- SHA-1: {}\n \nSummary: {}\n \nIf you trust this certificate, press Yes. To connect without storing certificate, press No. To abandon the connection press Cancel.\n \nContinue connecting and store the certificate?",
        unknown.sha256, unknown.sha1, unknown.reason
    )
}

/// The known certificates' file: `host:port sha256` lines.
pub fn known(file: &Path, host: &str, port: u16) -> Vec<String> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let place = format!("{host}:{port}");
    (text.lines())
        .filter_map(|line| line.split_once(' '))
        .filter(|(at, _)| *at == place)
        .map(|(_, sha256)| sha256.trim().to_string())
        .collect()
}

/// `sha256` kept for `host`'s `port`.
pub fn keep(file: &Path, host: &str, port: u16, sha256: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let mut out = (std::fs::OpenOptions::new().create(true).append(true)).open(file)?;
    writeln!(out, "{host}:{port} {sha256}")
}

/// The system's verifier, the certificates taken as they are, and the last one that failed.
#[derive(Debug)]
struct Verifier {
    system: Option<Arc<WebPkiServerVerifier>>,
    taken: Vec<String>,
    failed: Arc<Mutex<Option<Unknown>>>,
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let sha256 = digest(&ring::digest::SHA256, end_entity);
        if self.taken.contains(&sha256) {
            return Ok(ServerCertVerified::assertion());
        }
        let verified = match &self.system {
            Some(system) => system.verify_server_cert(
                end_entity,
                intermediates,
                server_name,
                ocsp_response,
                now,
            ),
            None => Err(rustls::Error::General("No trusted roots.".to_string())),
        };
        if let Err(error) = &verified
            && let Ok(mut failed) = self.failed.lock()
        {
            *failed = Some(Unknown {
                sha256,
                sha1: digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, end_entity),
                reason: error.to_string(),
            });
        }
        verified
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &algorithms())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &algorithms())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        algorithms().supported_schemes()
    }
}

fn algorithms() -> rustls::crypto::WebPkiSupportedAlgorithms {
    rustls::crypto::ring::default_provider().signature_verification_algorithms
}

/// The system's trusted roots (none: every certificate is asked about).
fn system_verifier() -> Option<Arc<WebPkiServerVerifier>> {
    let mut roots = RootCertStore::empty();
    for certificate in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(certificate);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider)
        .build()
        .ok()
}

/// A client certificate: its chain and its key.
pub type ClientCertificate = (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>);

/// The client certificate and its key in a PEM file (WinSCP's `TlsCertificateFile`).
pub fn client_certificate(file: &Path) -> Result<ClientCertificate, String> {
    let named = |what: &str| format!("Cannot load certificate file '{}': {what}", file.display());
    let bytes = std::fs::read(file).map_err(|error| named(&error.to_string()))?;
    let chain: Vec<CertificateDer<'static>> = (CertificateDer::pem_slice_iter(&bytes))
        .filter_map(Result::ok)
        .collect();
    if chain.is_empty() {
        return Err(named("no certificate in it"));
    }
    let key = PrivateKeyDer::from_pem_slice(&bytes).map_err(|_| named("no private key in it"))?;
    Ok((chain, key))
}

/// The TLS configuration taking `taken` certificates as they are; a failing one written into `failed`.
pub fn config(
    taken: Vec<String>,
    failed: Arc<Mutex<Option<Unknown>>>,
) -> Result<ClientConfig, String> {
    configured(taken, failed, &Tls::default())
}

/// [`config`] with the session's TLS settings: its client certificate, its sessions' reuse.
pub fn configured(
    taken: Vec<String>,
    failed: Arc<Mutex<Option<Unknown>>>,
    tls: &Tls,
) -> Result<ClientConfig, String> {
    let verifier = Verifier {
        system: system_verifier(),
        taken,
        failed,
    };
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = (ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions())
    .map_err(|error| error.to_string())?;
    let builder = builder
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier));
    let mut config = match tls.certificate_file.trim() {
        "" => builder.with_no_client_auth(),
        file => {
            let (chain, key) = client_certificate(Path::new(file))?;
            (builder.with_client_auth_cert(chain, key)).map_err(|error| error.to_string())?
        }
    };
    if !tls.session_reuse {
        config.resumption = rustls::client::Resumption::disabled();
    }
    Ok(config)
}

#[cfg(test)]
#[path = "ftp_tls_tests.rs"]
mod tests;
