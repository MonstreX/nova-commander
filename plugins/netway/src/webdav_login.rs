//! A WebDAV session started as WinSCP starts one (`TWebDAVFileSystem::Open`): "Connecting to host...",
//! the password asked when the session has a user and none, the session's folder looked up (its first
//! request: HTTPS's certificate checked then - one that does not verify asked about as FTPS's), "Starting
//! the session...".

use std::sync::{Arc, Mutex};

use suppaftp::tokio_rustls::rustls::ClientConfig;

use crate::ask::{Asker, Question, Reply};
use crate::fs::Fs;
use crate::ftp_login::Certificate;
use crate::ftp_tls;
use crate::login::{AUTHENTICATING, CONNECTING, PASSWORD_PROMPT, Plan, STARTING, Started};
use crate::session::Encryption;
use crate::ssh::{Answer, Failure};
use crate::webdav::{Connect, Dav};

/// The WebDAV session `plan` started.
pub async fn start(plan: &Plan, asker: &Asker) -> Result<Started, Failure> {
    asker.tell(CONNECTING);
    let user = plan.user.trim().to_string();
    let password = match (user.is_empty(), plan.password.is_empty()) {
        (false, true) => (asker.text(PASSWORD_PROMPT, true).await).ok_or(Failure::Cancelled)?,
        _ => plan.password.clone(),
    };
    asker.tell(AUTHENTICATING);
    let open = async |tls: Option<ClientConfig>| {
        let connect = Connect {
            host: &plan.host,
            port: plan.port,
            link: &plan.options.link,
            tls,
            user: &user,
            password: &password,
            compression: plan.options.ssh.compression,
        };
        let dav = Dav::new(&connect).map_err(Failure::Connection)?;
        match dav.canonicalize(&plan.folder).await {
            Ok(folder) => Ok((dav, folder)),
            Err(error) if error.starts_with("401") => Err(Failure::Refused),
            Err(error) => Err(Failure::Connection(error)),
        }
    };
    let (dav, folder) = certified(plan, asker, open).await?;
    asker.tell(STARTING);
    Ok(Started {
        connection: None,
        fs: Fs::webdav(dav, &plan.options),
        user,
        password,
        folder: crate::login::panel_folder(&folder),
    })
}

/// What `open` gives in the clear, or over TLS (the session's settings) with the certificates taken: one
/// that does not verify asked about (FTPS's question) and tried again when the user takes it.
pub async fn certified<T>(
    plan: &Plan,
    asker: &Asker,
    open: impl AsyncFn(Option<ClientConfig>) -> Result<T, Failure>,
) -> Result<T, Failure> {
    let file = plan.known_hosts.with_file_name("known_certificates");
    let mut taken = ftp_tls::known(&file, &plan.host, plan.port);
    loop {
        let failed = Arc::new(Mutex::new(None));
        let tls = match plan.encryption {
            Encryption::None => None,
            _ => Some(
                ftp_tls::configured(taken.clone(), Arc::clone(&failed), &plan.options.tls)
                    .map_err(Failure::Connection)?,
            ),
        };
        let error = match open(tls).await {
            Err(Failure::Connection(error)) => error,
            done => return done,
        };
        let Some(unknown) = failed.lock().ok().and_then(|mut failed| failed.take()) else {
            return Err(Failure::Connection(error));
        };
        let certificate = Certificate {
            host: plan.host.clone(),
            port: plan.port,
            unknown,
        };
        match asker.ask(Question::Certificate(certificate.clone())).await {
            Reply::HostKey(Answer::Keep) => {
                let sha256 = &certificate.unknown.sha256;
                let _ = ftp_tls::keep(&file, &plan.host, plan.port, sha256);
                taken.push(sha256.clone());
            }
            Reply::HostKey(Answer::Once) => taken.push(certificate.unknown.sha256.clone()),
            _ => return Err(Failure::Cancelled),
        }
    }
}
