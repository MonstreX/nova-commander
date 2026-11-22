//! Operations on the server's files (WinSCP's `TTerminal::DeleteFiles`, `CreateDirectory`, `RenameFile`,
//! `MoveFiles`, `CopyFiles`, `ChangeFilesProperties`, `CreateLink` over SFTP): run on the runtime as the
//! transfers are, each step's failure asked with WinSCP's headline (Abort / Retry / Skip / Skip all),
//! the progress counted by items. A folder deleted after its items, a link deleted as itself (never
//! what it points to); moved files renamed into the target with the mask on their names; duplicated files
//! copied through SFTP piece by piece with their times (WinSCP's copy through the shell's `cp` comes with
//! S6.5f's command session); properties set on each item, a folder's items too when recursive.

use russh_sftp::protocol::FileAttributes;

use crate::copy_names::mask_file_name;
use crate::copy_work::{Answers, Fault, Halted, Job, PIECE, fault};
use crate::rights_box::Rights;
use crate::sftp::{RemoteEntry, child};

/// An item of the server met in a walk: its full path and what it is.
#[derive(Debug, Clone)]
pub struct Found {
    pub path: String,
    pub entry: RemoteEntry,
}

/// The properties to set (WinSCP's `TRemoteProperties` that changed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Properties {
    pub rights: Option<Rights>,
    /// A folder's X where it reads (WinSCP's `AddXToDirectories`).
    pub add_x: bool,
    pub recursive: bool,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

impl Properties {
    /// A file's or folder's rights `old` as they become.
    pub fn rights_for(&self, old: u16, directory: bool) -> Option<u16> {
        let new = self.rights?.applied(old);
        Some(match directory && self.add_x {
            true => new | ((new & 0o444) >> 2),
            false => new,
        })
    }
}

/// WinSCP's headlines of the operations' failures.
pub fn delete_error(path: &str) -> String {
    format!("Error deleting file '{path}'.")
}

pub fn folder_error(path: &str) -> String {
    format!("Error creating folder '{path}'.")
}

pub fn rename_error(from: &str, to: &str) -> String {
    format!("Error renaming file '{from}' to '{to}'.")
}

pub fn copy_error(from: &str, to: &str) -> String {
    format!("Error copying file '{from}' to '{to}'.")
}

pub fn properties_error(path: &str) -> String {
    format!("Cannot change properties of file '{path}'.")
}

pub fn link_error(path: &str) -> String {
    format!("Cannot create link '{path}'.")
}

/// The target of `name` moved or copied into `target` with WinSCP's mask (`*.*` or `*`: the same name).
/// WinSCP's `RecycleFile` mask: `*-yyyymmdd-hhnnss.*` of the local time now.
pub fn recycle_mask() -> String {
    let now = nc_plugin_sdk::system::dos_time_of(nc_plugin_sdk::system::now_ticks());
    let ([year, month, day], [hour, minute, second]) = now.unwrap_or_default();
    format!("*-{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}.*")
}

pub fn target_of(name: &str, target: &str, mask: &str) -> String {
    child(target, &mask_file_name(name, mask))
}

impl Job<'_> {
    /// The item `path` as it is (a link as a link); none when skipped after a failure.
    async fn found(&self, answers: &mut Answers, path: &str) -> Result<Option<Found>, Halted> {
        let sftp = self.fs;
        let read = async || sftp.symlink_metadata(path).await.map_err(fault);
        let meta = self.retrying(answers, path, read).await?;
        Ok(meta.map(|meta| Found {
            path: path.to_string(),
            entry: RemoteEntry {
                name: path.rsplit('/').next().unwrap_or(path).to_string(),
                size: meta.size.unwrap_or(0),
                is_dir: meta.is_dir(),
                is_link: meta.is_symlink(),
                modified: meta.mtime,
                permissions: meta.permissions,
                ..RemoteEntry::default()
            },
        }))
    }

    /// The items named in `folder` and (`deep`) every folder's items, a folder's after it or (`items_first`)
    /// before it; a link never entered.
    pub async fn walk(
        &self,
        answers: &mut Answers,
        (folder, names): (&str, &[String]),
        deep: bool,
        items_first: bool,
    ) -> Result<Vec<Found>, Halted> {
        let mut stack: Vec<(Found, bool)> = Vec::new();
        for name in names.iter().rev() {
            if let Some(found) = self.found(answers, &child(folder, name)).await? {
                stack.push((found, false));
            }
        }
        let mut walked = Vec::new();
        while let Some((found, entered)) = stack.pop() {
            self.halt_if_stopped()?;
            let folder = found.entry.is_dir && !found.entry.is_link && deep;
            if !folder || entered {
                walked.push(found);
                continue;
            }
            self.progress.walking(&found.path);
            let sftp = self.fs;
            let path = found.path.clone();
            let list = async || sftp.list_raw(&path).await.map_err(Fault::Failed);
            let items = self
                .retrying(answers, &path, list)
                .await?
                .unwrap_or_default();
            match items_first {
                true => stack.push((found, true)),
                false => walked.push(found),
            }
            for item in items.into_iter().rev() {
                let path = child(&path, &item.name);
                stack.push((Found { path, entry: item }, false));
            }
        }
        Ok(walked)
    }

    /// The counts of the walk told to the progress.
    fn counted(&self, found: &[Found]) {
        let files = u64::try_from(found.len()).unwrap_or(u64::MAX);
        self.progress.walked(0, files, false);
    }

    /// WinSCP's `DeleteFiles`: the items named, a folder after its items.
    pub async fn delete(&self, folder: &str, names: &[String]) -> Result<(), Halted> {
        // WinSCP's `DeleteToRecycleBin`: moved into the bin, unless they are in it already.
        if let Some(bin) = self.fs.recycle_bin(false)
            && bin.trim_end_matches('/') != folder.trim_end_matches('/')
        {
            return (self.move_files(folder, names, (bin, &recycle_mask()))).await;
        }
        let mut answers = Answers::default();
        let found = self.walk(&mut answers, (folder, names), true, true).await?;
        self.counted(&found);
        for item in found {
            self.progress.file(&item.path, folder, 0);
            let sftp = self.fs;
            let path = item.path.clone();
            let folder = item.entry.is_dir && !item.entry.is_link;
            let remove = async || match folder {
                true => sftp.remove_dir(path.as_str()).await.map_err(fault),
                false => sftp.remove_file(path.as_str()).await.map_err(fault),
            };
            self.retrying_about(&mut answers, &delete_error(&item.path), remove)
                .await?;
            self.progress.file_done();
            self.check_stop().map_err(|_| Halted)?;
        }
        Ok(())
    }

    /// WinSCP's `CreateDirectory` with its rights when given.
    pub async fn make_folder(&self, path: &str, rights: Option<u16>) -> Result<(), Halted> {
        let mut answers = Answers::default();
        let sftp = self.fs;
        let make = async || {
            sftp.create_dir(path).await.map_err(fault)?;
            if let Some(rights) = rights {
                let attributes = FileAttributes {
                    permissions: Some(u32::from(rights)),
                    ..FileAttributes::empty()
                };
                sftp.set_metadata(path, attributes).await.map_err(fault)?;
            }
            Ok(())
        };
        self.retrying_about(&mut answers, &folder_error(path), make)
            .await
            .map(|_| ())
    }

    /// WinSCP's `RenameFile` / `MoveFiles` of the items named into `target` with `mask`.
    pub async fn move_files(
        &self,
        folder: &str,
        names: &[String],
        (target, mask): (&str, &str),
    ) -> Result<(), Halted> {
        let mut answers = Answers::default();
        self.progress
            .walked(0, u64::try_from(names.len()).unwrap_or(0), false);
        for name in names {
            let (from, to) = (child(folder, name), target_of(name, target, mask));
            self.progress.file(&from, target, 0);
            let sftp = self.fs;
            let rename = async || sftp.rename(from.as_str(), to.as_str()).await.map_err(fault);
            self.retrying_about(&mut answers, &rename_error(&from, &to), rename)
                .await?;
            self.progress.file_done();
        }
        Ok(())
    }

    /// WinSCP's `CopyFiles` on the server: each item copied into `target` with `mask`, a folder with all
    /// its items, the times kept - by `cp -p -r -f` when the session runs commands (SCP's; SFTP's
    /// secondary shell), else through the client.
    pub async fn duplicate(
        &self,
        folder: &str,
        names: &[String],
        (target, mask): (&str, &str),
    ) -> Result<(), Halted> {
        let mut answers = Answers::default();
        let left = (self.copied_on_server(&mut answers, folder, names, (target, mask))).await?;
        if left.is_empty() {
            return Ok(());
        }
        let found = self
            .walk(&mut answers, (folder, &left), true, false)
            .await?;
        self.counted(&found);
        for item in &found {
            let relative = item.path[folder.trim_end_matches('/').len() + 1..].to_string();
            let (first, rest) = relative.split_once('/').unwrap_or((&relative, ""));
            let mut to = target_of(first, target, mask);
            if !rest.is_empty() {
                to = child(&to, rest);
            }
            self.progress.file(&item.path, target, item.entry.size);
            let copy = async || self.copy_one(item, &to).await;
            self.retrying_about(&mut answers, &copy_error(&item.path, &to), copy)
                .await?;
            self.progress.file_done();
        }
        Ok(())
    }

    /// The items copied on the server (its `cp`, S3's CopyObject); the names left to copy through the
    /// client: all when the session runs no commands (the first tells), S3's folders.
    async fn copied_on_server(
        &self,
        answers: &mut Answers,
        folder: &str,
        names: &[String],
        (target, mask): (&str, &str),
    ) -> Result<Vec<String>, Halted> {
        let total = u64::try_from(names.len()).unwrap_or(0);
        let mut left = Vec::new();
        for (at, name) in names.iter().enumerate() {
            let (from, to) = (child(folder, name), target_of(name, target, mask));
            // The first tried quietly: a session's commands refused copy nothing on the server.
            let first = match at {
                0 => Some(self.fs.copy(&from, &to).await),
                _ => None,
            };
            if matches!(first, Some(Ok(false))) && self.fs.commands().is_some() {
                return Ok(names.to_vec());
            }
            if at == 0 {
                self.progress.walked(0, total, false);
            }
            self.progress.file(&from, target, 0);
            let copied = match first {
                Some(Ok(copied)) => copied,
                _ => {
                    let fs = self.fs;
                    let copy = async || fs.copy(&from, &to).await.map_err(fault);
                    let headline = copy_error(&from, &to);
                    let done = self.retrying_about(answers, &headline, copy);
                    done.await? != Some(false)
                }
            };
            if !copied {
                left.push(name.clone());
            }
            self.progress.file_done();
        }
        Ok(left)
    }

    /// One item copied on the server: a folder made, a link made anew, a file's bytes and times.
    async fn copy_one(&self, item: &Found, to: &str) -> Result<(), Fault> {
        self.progress.restart_file();
        if item.entry.is_link {
            let target = self.fs.read_link(item.path.as_str()).await.map_err(fault)?;
            return self.fs.symlink(to, target).await.map_err(fault);
        }
        if item.entry.is_dir {
            return self.fs.create_dir(to).await.map_err(fault);
        }
        let mut source = self.fs.open(item.path.as_str()).await.map_err(fault)?;
        let rights = (item.entry.permissions).map_or(0o644, |rights| {
            u16::try_from(rights & 0o7777).unwrap_or(0o644)
        });
        let mut target = (self.fs.create(to, item.entry.size, rights))
            .await
            .map_err(fault)?;
        let mut buffer = vec![0; PIECE];
        loop {
            self.check_stop()?;
            let read = source.read(&mut buffer).await.map_err(fault)?;
            if read == 0 {
                break;
            }
            target.write_all(&buffer[..read]).await.map_err(fault)?;
            self.progress.add(read as u64);
        }
        target.shutdown().await.map_err(fault)?;
        let attributes = FileAttributes {
            mtime: item.entry.modified,
            atime: item.entry.modified,
            permissions: item.entry.permissions.map(|rights| rights & 0o7777),
            ..FileAttributes::empty()
        };
        self.fs.set_metadata(to, attributes).await.map_err(fault)
    }

    /// WinSCP's `ChangeFilesProperties`: the items named (and their items when recursive).
    pub async fn set_properties(
        &self,
        folder: &str,
        names: &[String],
        properties: Properties,
    ) -> Result<(), Halted> {
        let mut answers = Answers::default();
        let found = (self.walk(&mut answers, (folder, names), properties.recursive, false)).await?;
        self.counted(&found);
        for item in found {
            self.progress.file(&item.path, folder, 0);
            let old = item.entry.permissions.unwrap_or(0) & 0o7777;
            let rights = u16::try_from(old).unwrap_or(0);
            let attributes = FileAttributes {
                permissions: properties
                    .rights_for(rights, item.entry.is_dir)
                    .map(u32::from),
                uid: properties.uid,
                gid: properties.gid,
                ..FileAttributes::empty()
            };
            let sftp = self.fs;
            let path = item.path.clone();
            let set = async || {
                sftp.set_metadata(path.as_str(), attributes.clone())
                    .await
                    .map_err(fault)
            };
            self.retrying_about(&mut answers, &properties_error(&item.path), set)
                .await?;
            self.progress.file_done();
        }
        Ok(())
    }

    /// WinSCP's `CreateLink`: `path` pointing to `point_to`, a symbolic link or a hard one.
    pub async fn link(&self, path: &str, point_to: &str, symbolic: bool) -> Result<(), Halted> {
        let mut answers = Answers::default();
        let sftp = self.fs;
        let make = async || match symbolic {
            true => sftp.symlink(path, point_to).await.map_err(fault),
            false => match sftp.hardlink(point_to, path).await.map_err(fault)? {
                true => Ok(()),
                false => Err(Fault::Failed("The server makes no hard links".to_string())),
            },
        };
        self.retrying_about(&mut answers, &link_error(path), make)
            .await
            .map(|_| ())
    }
}
