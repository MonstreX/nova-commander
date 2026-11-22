//! An FTP session started as WinSCP starts one (`TFTPFileSystem::Open`): "Connecting to host...", the
//! control connection (TLS explicit or implicit: an unknown certificate asked about, WinSCP's
//! `VerifyCertificate`, Yes keeping it, No taking it this time - the loop WebDAV and S3 share), HOST when
//! the session says so, "Authenticating..." with the user and the password (asked when the session has
//! none, unless an empty one is allowed; WinSCP's anonymous login without a user) and the account the
//! server may ask for, the commands after the login, "Starting the session..." with the folder the panel
//! opens at.

use suppaftp::tokio_rustls::rustls::ClientConfig;

use crate::ask::Asker;
use crate::fs::Fs;
use crate::ftp::{self, Connect, Ftp};
use crate::ftp_tls::Unknown;
use crate::login::{AUTHENTICATING, CONNECTING, PASSWORD_PROMPT, Plan, STARTING, Started};
use crate::ssh::Failure;

/// A certificate the user is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    pub host: String,
    pub port: u16,
    pub unknown: Unknown,
}

/// The FTP session `plan` started.
pub async fn start(plan: &Plan, asker: &Asker) -> Result<Started, Failure> {
    asker.tell(CONNECTING);
    let open = async |tls: Option<ClientConfig>| {
        let connect = Connect {
            host: &plan.host,
            port: plan.port,
            encryption: plan.encryption,
            link: &plan.options.link,
            options: &plan.options.ftp,
            tls,
        };
        ftp::open(&connect).await.map_err(Failure::Connection)
    };
    let stream = crate::webdav_login::certified(plan, asker, open).await?;
    asker.tell(AUTHENTICATING);
    let user = plan.user.trim().to_string();
    // "Allow empty password": a user without one is not asked for it.
    let ask =
        !user.is_empty() && plan.password.is_empty() && !plan.options.ftp.allow_empty_password;
    let password = match ask {
        true => (asker.text(PASSWORD_PROMPT, true).await).ok_or(Failure::Cancelled)?,
        false => plan.password.clone(),
    };
    let shift = i64::from(plan.options.time_difference) * 60;
    let login = (user.as_str(), password.as_str());
    let ftp = (Ftp::log_in(stream, login, shift, &plan.options.ftp).await).map_err(refused)?;
    asker.tell(STARTING);
    let fs = Fs::ftp(ftp, &plan.options);
    let wanted = match plan.folder.trim() {
        "" => ".",
        folder => folder,
    };
    let found = (fs.canonicalize(wanted).await)
        .map_err(|error| Failure::Connection(format!("{wanted}: {error}")))?;
    Ok(Started {
        connection: None,
        fs,
        user,
        password,
        folder: crate::login::panel_folder(&found),
    })
}

/// A 530 refusal is "Access denied.".
fn refused(error: String) -> Failure {
    match error.starts_with("530") {
        true => Failure::Refused,
        false => Failure::Connection(error),
    }
}
