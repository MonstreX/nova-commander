//! A remote folder over SFTP as NetBox lists it (WinSCP's `TSFTPFileSystem::ReadDirectory`): each item's
//! name, size, kind, last write, rights, owner and group; "." and ".." left out (the panel shows its own
//! ".."); the home folder where a session starts without one.

use russh_sftp::client::SftpSession;

/// How exact a server's time is (WinSCP's `TModificationFmt`): `ls -l`'s "hh:mm" is to the minute, its
/// year to the day; SFTP's, MLSD's, `--full-time`'s to the second.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Precision {
    #[default]
    Second,
    Minute,
    Day,
}

impl Precision {
    /// WinSCP's `ReduceDateTimePrecision`: `seconds` cut to this precision (a disk's time compared with
    /// a server's).
    pub fn reduced(self, seconds: u64) -> u64 {
        match self {
            Self::Second => seconds,
            Self::Minute => seconds - seconds % 60,
            Self::Day => seconds - seconds % 86_400,
        }
    }
}

/// One item of a remote folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub is_link: bool,
    /// Seconds since 1970 (UTC), when the server tells.
    pub modified: Option<u32>,
    /// How exact `modified` is.
    pub precision: Precision,
    /// The Unix rights and kind bits (`0o100644`), when the server tells.
    pub permissions: Option<u32>,
    /// The owner's and group's names (SFTP 3 sends numbers only: then the numbers as text).
    pub owner: Option<String>,
    pub group: Option<String>,
}

/// The items of the remote folder `path`, "." and ".." left out; a link to a folder is a folder (WinSCP
/// asks what each link points to).
pub async fn list(sftp: &SftpSession, path: &str) -> Result<Vec<RemoteEntry>, String> {
    let mut items = read(sftp, path).await?;
    for item in items.iter_mut().filter(|item| item.is_link) {
        // A link that points nowhere stays a file.
        if let Ok(target) = sftp.metadata(child(path, &item.name)).await {
            item.is_dir = target.is_dir();
        }
    }
    Ok(items)
}

/// `name` in the remote folder `path`.
pub fn child(path: &str, name: &str) -> String {
    format!("{}/{name}", path.trim_end_matches('/'))
}

/// The items of the remote folder `path` as they are: a link is a link (deleting it leaves what it points
/// to), "." and ".." left out.
pub async fn read(sftp: &SftpSession, path: &str) -> Result<Vec<RemoteEntry>, String> {
    let read = sftp
        .read_dir(path)
        .await
        .map_err(|error| error.to_string())?;
    Ok(read
        .filter(|item| !matches!(item.file_name().as_str(), "." | ".."))
        .map(|item| {
            let meta = item.metadata();
            let kind = item.file_type();
            RemoteEntry {
                name: item.file_name(),
                size: meta.size.unwrap_or(0),
                is_dir: kind.is_dir(),
                is_link: kind.is_symlink(),
                modified: meta.mtime,
                precision: Precision::Second,
                permissions: meta.permissions,
                owner: meta
                    .user
                    .clone()
                    .or_else(|| meta.uid.map(|id| id.to_string())),
                group: meta
                    .group
                    .clone()
                    .or_else(|| meta.gid.map(|id| id.to_string())),
            }
        })
        .collect())
}

/// The user's home folder on the server (the path of "."), where a session without a folder starts.
pub async fn home(sftp: &SftpSession) -> Result<String, String> {
    sftp.canonicalize(".")
        .await
        .map_err(|error| error.to_string())
}
