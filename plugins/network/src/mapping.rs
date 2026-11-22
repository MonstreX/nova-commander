//! Shares connected as disks and disconnected (FAR's Network `MapNetworkDrive`, `AddConnectionExplicit`,
//! `AddConnectionWithLogon`, `DisconnectFromServer`, `CancelConnection`, `GetDriveToDisconnect`,
//! `ConfirmCancelConnection`): F5 the first free letter from C, F6 a letter asked for (F6 in its menu
//! turning permanent and temporary round), Shift with them a connection not restored at logon; a share
//! that cannot be read connected with a name and password asked for (a name refused tried again as the
//! server's user, a conflicting session ended first). F8 ends a share's disk connection, asking which
//! letter when it has more and whether it is to be restored at logon; files open on it asked about.
//! What is asked of the user goes through [`Asker`], what of the system through [`Connections`].

use std::io;

use nc_plugin_sdk::system::network::{
    ERROR_ACCESS_DENIED, ERROR_CANCELLED, ERROR_DEVICE_ALREADY_REMEMBERED, ERROR_DEVICE_IN_USE,
    ERROR_OPEN_FILES, ERROR_SESSION_CREDENTIAL_CONFLICT, Logon, NetResource, TYPE_DISK,
    free_letters,
};

/// What the connections need of the system (the SDK's calls; a test's own in the tests).
pub trait Connections {
    /// `remote` connected as `local` (`X:`; empty: no letter), as `logon` (none: the user's own).
    fn connect_as(
        &self,
        local: &str,
        remote: &str,
        logon: Option<Logon<'_>>,
        remember: bool,
    ) -> io::Result<()>;
    /// The connection `name` ended (`forget`: not restored at logon; `force`: with files open).
    fn disconnect(&self, name: &str, forget: bool, force: bool) -> io::Result<()>;
    /// The connections there are, of any kind.
    fn connections(&self) -> Vec<NetResource>;
    /// Whether the disk `letter` is restored at logon.
    fn remembered(&self, letter: char) -> bool;
    /// The disks there are, a bit for each letter from A.
    fn used_letters(&self) -> u32;
    /// Whether the folder `path` can be read (FAR's `IsReadable`).
    fn readable(&self, path: &str) -> bool;
    /// The process's folder moved off the disk `letter` (FAR's `HandsOffDisconnectDrive`): whether it was
    /// on it.
    fn hands_off(&self, letter: char) -> bool;
    /// The process's folder moved to the temporary folder when it is a network path (`\\...`; the end
    /// of FAR's `DisconnectFromServer`).
    fn off_network(&self);
}

/// What the connections ask of the user (the program's windows; a test's answers in the tests).
pub trait Asker {
    /// FAR's `AskMapDrive`: one of the `free` letters, the connection permanent or not as the menu was
    /// turned (F6); none when cancelled.
    fn letter(&self, free: &[char], permanent: bool) -> Option<(char, bool)>;
    /// FAR's `GetNameAndPassword` for `remote`: the name, the password and "Remember password"; none
    /// when cancelled.
    fn logon(&self, remote: &str) -> Option<Given>;
    /// FAR's `ReadFavoriteItem`: the name and password kept for `remote` in the favorites.
    fn saved_logon(&self, remote: &str) -> Option<(String, String)>;
    /// FAR's `WriteFavoriteItem`: the name and password kept for `remote` in the favorites.
    fn save_logon(&self, remote: &str, user: &str, password: &str);
    /// FAR's `ConfirmCancelConnection`: Yes with "Reconnect at Logon" (`reconnect` at first, changeable
    /// only when `remembered`) as it was left; none when cancelled.
    fn confirm_disconnect(
        &self,
        local: &str,
        remote: &str,
        remembered: bool,
        reconnect: bool,
    ) -> Option<bool>;
    /// FAR's `GetDriveToDisconnect`: which of the `locals` `remote` is connected as; none when cancelled.
    fn which_local(&self, remote: &str, locals: &[String]) -> Option<usize>;
    /// FAR's open files question for `local`: whether it is disconnected anyway.
    fn force(&self, local: &str) -> bool;
    /// FAR's "Error" message with `lines` and the system's reason (`error`, when one).
    fn error(&self, lines: &[String], error: Option<&io::Error>);
}

/// The system's code of `error`.
fn code(error: &io::Error) -> i32 {
    error.raw_os_error().unwrap_or(0)
}

fn cancelled() -> io::Error {
    io::Error::from_raw_os_error(ERROR_CANCELLED)
}

/// The server's name in `remote` (`\\SERVER\share` → `SERVER`).
pub fn server_of(remote: &str) -> &str {
    let name = remote.trim_start_matches('\\');
    name.split('\\').next().unwrap_or(name)
}

/// FAR's `QuoteSpaceOnly`: a name with a space quoted.
pub fn quoted(name: &str) -> String {
    if name.contains(' ') {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

/// FAR's `GetFreeLetter`: the first letter from C not in `used`, marked used.
fn take_free(used: &mut u32) -> Option<char> {
    let letter = free_letters(*used, 'C').into_iter().next()?;
    *used |= 1 << (u32::from(letter) - u32::from('A'));
    Some(letter)
}

/// FAR's `DisconnectFromServer`: every connection to the server of `remote` ended (its shares' and its
/// own), the process's folder moved off the network.
pub fn disconnect_from_server(net: &dyn Connections, remote: &str) {
    let server = server_of(remote);
    for one in net.connections() {
        if server_of(&one.remote_name).eq_ignore_ascii_case(server) {
            // A connection that will not end leaves the next try to tell.
            let _ = net.disconnect(&one.remote_name, false, true);
        }
    }
    // The server's own (IPC) connection, when there is one: nothing to tell otherwise.
    let _ = net.disconnect(&format!(r"\\{server}"), false, true);
    net.off_network();
}

/// FAR's `AddConnectionWithLogon`: `remote` connected as `local` with the name and password; a session
/// with other credentials ended first and tried again; a plain name refused tried again as the server's
/// user (`SERVER\name`).
pub fn connect_with_logon(
    net: &dyn Connections,
    (local, remote): (&str, &str),
    (user, password): (&str, &str),
    remember: bool,
) -> io::Result<()> {
    let mut user = user.to_string();
    loop {
        let logon = || Logon {
            user: &user,
            password,
        };
        let mut result = net.connect_as(local, remote, Some(logon()), remember);
        if result
            .as_ref()
            .is_err_and(|error| code(error) == ERROR_SESSION_CREDENTIAL_CONFLICT)
        {
            disconnect_from_server(net, remote);
            result = net.connect_as(local, remote, Some(logon()), remember);
        }
        let Err(error) = result else {
            return Ok(());
        };
        if user.is_empty() || user.contains('\\') || user.contains('@') {
            return Err(error);
        }
        user = format!(r"{}\{user}", server_of(remote));
    }
}

/// The logon dialog's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Given {
    pub user: String,
    pub password: String,
    /// "Remember password": kept in the favorites once connected.
    pub remember: bool,
}

/// FAR's `AddConnectionExplicit`: the name and password asked for, then connected with them, kept in
/// the favorites with "Remember password"; cancelled: `ERROR_CANCELLED`.
pub fn connect_explicit(
    net: &dyn Connections,
    ask: &dyn Asker,
    (local, remote): (&str, &str),
    remember: bool,
) -> io::Result<()> {
    let given = ask.logon(remote).ok_or_else(cancelled)?;
    connect_with_logon(
        net,
        (local, remote),
        (&given.user, &given.password),
        remember,
    )?;
    if given.remember {
        ask.save_logon(remote, &given.user, &given.password);
    }
    Ok(())
}

/// FAR's `AddConnectionFromFavorites || AddConnectionExplicit`: the name and password the favorites keep
/// for `remote` tried first, asked for when there are none or they are refused.
pub fn connect_saved_or_asked(
    net: &dyn Connections,
    ask: &dyn Asker,
    (local, remote): (&str, &str),
    remember: bool,
) -> io::Result<()> {
    if let Some((user, password)) = ask.saved_logon(remote)
        && connect_with_logon(net, (local, remote), (&user, &password), remember).is_ok()
    {
        return Ok(());
    }
    connect_explicit(net, ask, (local, remote), remember)
}

/// FAR's `MapNetworkDrive`: `remote` connected as a disk, its letter asked for (`choose`) or the first
/// free one, `permanent` or not; false when it was not (told) and the next shares are left.
pub fn map(
    net: &dyn Connections,
    ask: &dyn Asker,
    remote: &str,
    choose: bool,
    permanent: bool,
) -> bool {
    let mut used = net.used_letters();
    let (letter, permanent) = if choose {
        let free = free_letters(used, 'A');
        if free.is_empty() {
            return no_free_letters(ask);
        }
        match ask.letter(&free, permanent) {
            Some(chosen) => chosen,
            None => return false,
        }
    } else {
        match take_free(&mut used) {
            Some(letter) => (letter, permanent),
            None => return no_free_letters(ask),
        }
    };
    let mut letter = letter;
    loop {
        let local = format!("{letter}:");
        let result = if net.readable(remote) {
            net.connect_as(&local, remote, None, permanent)
        } else {
            connect_saved_or_asked(net, ask, (&local, remote), permanent).and_then(|()| {
                if net.readable(&local) {
                    Ok(())
                } else {
                    Err(io::Error::from_raw_os_error(ERROR_ACCESS_DENIED))
                }
            })
        };
        let error = match result {
            Ok(()) => return true,
            // FAR: a name and password not given leaves this share, the next ones still mapped.
            Err(error) if code(&error) == ERROR_CANCELLED => return true,
            Err(error) => error,
        };
        if code(&error) != ERROR_DEVICE_ALREADY_REMEMBERED {
            let text: String = remote.chars().take(40).collect();
            ask.error(
                &[format!("Cannot connect \"{text}\" to {local}")],
                Some(&error),
            );
            return false;
        }
        if choose {
            ask.error(
                &["The disk name has a remembered connection".to_string()],
                None,
            );
            return false;
        }
        match take_free(&mut used) {
            Some(next) => letter = next,
            None => return no_free_letters(ask),
        }
    }
}

fn no_free_letters(ask: &dyn Asker) -> bool {
    ask.error(&["No free disk letters available".to_string()], None);
    false
}

/// The settings a disconnection reads and leaves: whether the program asks first (its
/// "RemoveConnection" confirmation), "Reconnect at Logon" as last left (FAR's `DisconnectMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disconnecting {
    pub confirm: bool,
    pub reconnect: bool,
}

/// FAR's `CancelConnection`: the disk `remote` is connected as ended, asked which when it is more than
/// one, confirmed as the program says; `reconnect` left as the question left it (saved by the caller
/// when the disk was restored at logon: the second of the answer). False when it was not (told when it
/// failed), and the next shares are left.
pub fn unmap(
    net: &dyn Connections,
    ask: &dyn Asker,
    remote: &str,
    settings: Disconnecting,
) -> (bool, Option<bool>) {
    let locals: Vec<String> = (net.connections().into_iter())
        .filter(|one| one.kind == TYPE_DISK)
        .filter(|one| !one.local_name.is_empty() && one.remote_name.eq_ignore_ascii_case(remote))
        .map(|one| one.local_name)
        .take(10)
        .collect();
    let local = match locals.len() {
        0 => return (false, None),
        1 => locals[0].clone(),
        _ => match ask.which_local(remote, &locals) {
            Some(index) => locals[index].clone(),
            None => return (false, None),
        },
    };
    let letter = local.chars().next().unwrap_or('?');
    let remembered = net.remembered(letter);
    let reconnect = if settings.confirm {
        match ask.confirm_disconnect(&local, remote, remembered, settings.reconnect) {
            Some(reconnect) => reconnect,
            None => return (false, None),
        }
    } else {
        settings.reconnect
    };
    let saved = remembered.then_some(reconnect);
    let forget = !reconnect;
    let mut result = net.disconnect(&local, forget, false);
    if result.is_err() && net.hands_off(letter) {
        result = net.disconnect(&local, forget, false);
    }
    let Err(error) = result else {
        return (true, saved);
    };
    let open = matches!(code(&error), ERROR_OPEN_FILES | ERROR_DEVICE_IN_USE);
    if open {
        // FAR: "Cancel" leaves the disk connected without a word, the next shares still disconnected.
        if !ask.force(&local) || net.disconnect(&local, forget, true).is_ok() {
            return (true, saved);
        }
    }
    ask.error(&[format!("Cannot disconnect from {local}")], Some(&error));
    (false, saved)
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
mod tests;
