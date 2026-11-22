//! An S3 session started as WinSCP starts one (`TS3FileSystem::Open`): "Connecting to host...", the access
//! key ID (the session's user) and the secret access key (its password) asked when the session has none
//! (`S3_ACCESS_KEY_ID_PROMPT`, `S3_SECRET_ACCESS_KEY_PROMPT`), signed for the library's default region
//! (`S3LibDefaultRegion`, a bucket elsewhere found by its answer), the session's folder looked up (HTTPS's
//! certificate checked then, as WebDAV's), "Starting the session...". Virtual-host addresses, path-style
//! ones for an IP address or a single name.

use suppaftp::tokio_rustls::rustls::ClientConfig;

use crate::ask::Asker;
use crate::fs::Fs;
use crate::login::{AUTHENTICATING, CONNECTING, Plan, STARTING, Started};
use crate::s3::{Connect, S3};
use crate::sigv4::Signer;
use crate::ssh::Failure;
use crate::webdav_login::certified;

/// WinSCP's `S3_ACCESS_KEY_ID_PROMPT` and `S3_SECRET_ACCESS_KEY_PROMPT`.
pub const ACCESS_KEY_PROMPT: &str = "A&ccess key ID:";
pub const SECRET_PROMPT: &str = "Secret access &key:";
/// libs3's `S3_DEFAULT_REGION`.
pub const DEFAULT_REGION: &str = "us-east-1";

/// The S3 session `plan` started.
pub async fn start(plan: &Plan, asker: &Asker) -> Result<Started, Failure> {
    asker.tell(CONNECTING);
    let user = match plan.user.trim() {
        "" => (asker.text(ACCESS_KEY_PROMPT, false).await).ok_or(Failure::Cancelled)?,
        user => user.to_string(),
    };
    let password = match plan.password.is_empty() {
        true => (asker.text(SECRET_PROMPT, true).await).ok_or(Failure::Cancelled)?,
        false => plan.password.clone(),
    };
    asker.tell(AUTHENTICATING);
    let open = async |tls: Option<ClientConfig>| {
        let connect = Connect {
            host: &plan.host,
            port: plan.port,
            link: &plan.options.link,
            tls,
            signer: Signer {
                access_key: user.trim().to_string(),
                secret: password.clone(),
                region: match plan.options.s3.default_region.trim() {
                    "" => DEFAULT_REGION.to_string(),
                    region => region.to_string(),
                },
            },
            options: &plan.options.s3,
        };
        let s3 = S3::new(connect).map_err(Failure::Connection)?;
        match s3.canonicalize(&plan.folder).await {
            Ok(folder) => Ok((s3, folder)),
            Err(error) if refused(&error) => Err(Failure::Refused),
            Err(error) => Err(Failure::Connection(error)),
        }
    };
    let (s3, folder) = certified(plan, asker, open).await?;
    asker.tell(STARTING);
    Ok(Started {
        connection: None,
        fs: Fs::s3(s3, &plan.options),
        user,
        password,
        folder: crate::login::panel_folder(&folder),
    })
}

/// The keys refused (libs3's `S3StatusErrorAccessDenied`, `InvalidAccessKeyId`, `SignatureDoesNotMatch`).
fn refused(error: &str) -> bool {
    let code = error.split(':').next().unwrap_or_default();
    matches!(
        code,
        "AccessDenied" | "InvalidAccessKeyId" | "SignatureDoesNotMatch" | "403 Forbidden"
    )
}
