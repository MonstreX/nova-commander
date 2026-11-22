//! What the FTP(S) tab changes in an FTP session's start (S6.6e, WinSCP's FileZilla engine): the HOST
//! command before the login (`OPTION_MPEXT_HOST`), the login with the account the server may ask for
//! (`ACCT`, "default" when none is given), the passive reply's address or the server's
//! (`CheckForcePasvIp` with `IsRoutableAddress`), the commands sent after the login (`DoStartup`).

use std::net::IpAddr;

use suppaftp::{FtpError, FtpResult, Mode, Status};

use crate::ftp::Stream;
use crate::link_options::Link;
use crate::protocol_options::Switch;

/// How long an active mode's data connection is waited for.
const ACTIVE_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// FileZilla's `IsRoutableAddress`: not the loopback, a private network, a link-local one or `0.`.
pub fn routable(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            !(v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.octets()[0] == 0)
        }
        IpAddr::V6(v6) => !(v6.is_loopback() || v6.is_unspecified()),
    }
}

/// The host a passive data connection goes to: the reply's address, or the server's (`server`: its
/// address, or its name when the session goes through a proxy) - always when forced, by itself when the
/// reply's address is not routable and the server's is.
pub fn pasv_host(force: Switch, offered: IpAddr, server: &str) -> String {
    let use_server = match force {
        Switch::On => true,
        Switch::Off => false,
        Switch::Auto => !routable(offered) && server.parse::<IpAddr>().is_ok_and(routable),
    };
    match use_server {
        true => server.to_string(),
        false => offered.to_string(),
    }
}

/// The data connections' mode: active, or passive through the session's way to the passive host.
pub fn with_mode(
    stream: Stream,
    (passive, force): (bool, Switch),
    link: &Link,
    server: String,
) -> Stream {
    if !passive {
        let mut stream = stream.active_mode(ACTIVE_WAIT);
        stream.set_mode(Mode::Active);
        return stream;
    }
    let link = link.clone();
    let mut stream = stream.passive_stream_builder(move |address| {
        let link = link.clone();
        let host = pasv_host(force, address.ip(), &server);
        Box::pin(async move {
            (crate::route::tcp(&host, address.port(), &link).await)
                .map_err(|error| FtpError::ConnectionError(std::io::Error::other(error)))
        })
    });
    stream.set_mode(Mode::Passive);
    stream
}

/// FileZilla's HOST before the login; a server without it goes on.
pub async fn host_command(stream: &mut Stream, host: &str) {
    let _ = stream
        .custom_command(format!("HOST {host}"), &[Status::Ready, Status::CommandOk])
        .await;
}

/// USER, PASS when asked, ACCT when asked (the account, else FileZilla's "default").
pub async fn log_in(
    stream: &mut Stream,
    (user, password): (&str, &str),
    account: &str,
) -> FtpResult<()> {
    let asked = [
        Status::LoggedIn,
        Status::NeedPassword,
        Status::LoginNeedAccount,
    ];
    let mut response = stream
        .custom_command(format!("USER {user}"), &asked)
        .await?;
    if response.status == Status::NeedPassword {
        let after = [Status::LoggedIn, Status::LoginNeedAccount];
        response = stream
            .custom_command(format!("PASS {password}"), &after)
            .await?;
    }
    if response.status == Status::LoginNeedAccount {
        let account = match account.trim() {
            "" => "default",
            account => account,
        };
        let done = [Status::LoggedIn, Status::CommandOk];
        stream
            .custom_command(format!("ACCT {account}"), &done)
            .await?;
    }
    Ok(())
}

/// WinSCP's `DoStartup`: each command sent, a reply other than 2xx or 3xx failing the session.
pub async fn post_login(stream: &mut Stream, commands: &[String]) -> Result<(), String> {
    for command in commands.iter().filter(|command| !command.trim().is_empty()) {
        // No reply is listed as expected: each comes back as "unexpected", its code then judged.
        let reply = match stream.custom_command(command.trim(), &[]).await {
            Ok(response) | Err(FtpError::UnexpectedResponse(response)) => response,
            Err(error) => return Err(error.to_string()),
        };
        if !(200..400).contains(&reply.status.code()) {
            let body = String::from_utf8_lossy(&reply.body).trim().to_string();
            return Err(format!("{command}: {body}"));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "ftp_connect_tests.rs"]
mod tests;
