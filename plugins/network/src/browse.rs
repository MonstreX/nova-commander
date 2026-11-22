//! The panel's folders as FAR's `NetBrowser` walks them (`EnumerateNetList`, `GetFindData`,
//! `GetRemoteName`, `GetHiddenShares`, `GotoComputer`): `/` the providers, under each its domains, their
//! servers, a server's shares. A folder is named by FAR's names of the resources on the way to it
//! (`/Microsoft Windows Network/WORKGROUP/\\SERVER`); a computer reached by its name stands at the top
//! (`/\\SERVER`). Each listing remembers its resources by their paths; a path never listed is found by
//! walking it from the top, a `\\name` the network does not list asked for by its name.

use std::collections::HashMap;
use std::io;

use nc_plugin_sdk::FileEntry;
use nc_plugin_sdk::system::network::{
    DISPLAY_NETWORK, DISPLAY_SERVER, DISPLAY_SHARE, NetResource, SCOPE_GLOBALNET, SHARE_PRINT,
    Share, TYPE_ANY, TYPE_DISK, TYPE_PRINT, USAGE_CONTAINER,
};

use crate::options::{HiddenShares, Options};

/// Windows' `FILE_ATTRIBUTE_VIRTUAL`: FAR's mark of a printer.
const VIRTUAL: u32 = 0x1_0000;

/// What the plugin asks of the network (the SDK's calls; a test's own network in the tests).
pub trait Network {
    /// The resources under `parent` (none: the providers).
    fn enumerate(&self, parent: Option<&NetResource>) -> io::Result<Vec<NetResource>>;
    /// The connected disks (`RESOURCE_CONNECTED`), their letters in `local_name`.
    fn connected(&self) -> io::Result<Vec<NetResource>>;
    /// The resource named `remote` (`\\SERVER`).
    fn information(&self, remote: &str) -> io::Result<NetResource>;
    /// The resource above `resource`.
    fn parent(&self, resource: &NetResource) -> io::Result<NetResource>;
    /// A server's shares, the hidden ones too.
    fn shares(&self, server: &str) -> Vec<Share>;
    /// A connection with the user's own credentials, asking nothing.
    fn connect(&self, resource: &NetResource) -> io::Result<()>;
    /// A listing's answer got another way kept for the next listing of `parent` (a network that keeps
    /// none drops it).
    fn keep(&self, _parent: Option<&NetResource>, _answer: io::Result<Vec<NetResource>>) {}
}

/// The SDK's network.
pub struct System;

impl Network for System {
    fn enumerate(&self, parent: Option<&NetResource>) -> io::Result<Vec<NetResource>> {
        nc_plugin_sdk::system::network::enumerate(SCOPE_GLOBALNET, TYPE_ANY, parent)
    }

    fn connected(&self) -> io::Result<Vec<NetResource>> {
        use nc_plugin_sdk::system::network::SCOPE_CONNECTED;
        nc_plugin_sdk::system::network::enumerate(SCOPE_CONNECTED, TYPE_DISK, None)
    }

    fn information(&self, remote: &str) -> io::Result<NetResource> {
        nc_plugin_sdk::system::network::resource_information(remote)
    }

    fn parent(&self, resource: &NetResource) -> io::Result<NetResource> {
        nc_plugin_sdk::system::network::resource_parent(resource)
    }

    fn shares(&self, server: &str) -> Vec<Share> {
        nc_plugin_sdk::system::network::server_shares(server)
    }

    fn connect(&self, resource: &NetResource) -> io::Result<()> {
        nc_plugin_sdk::system::network::add_connection(resource)
    }
}

/// FAR's `GetRemoteName`: the Favorites folder as "Favorites", a provider by its name, a share by its
/// whole path (or its name), another resource by its remote name.
pub fn name_of(resource: &NetResource, options: &Options) -> String {
    if crate::favorites::is_folder(resource) {
        return crate::favorites::FOLDER.to_string();
    }
    if !resource.provider.is_empty()
        && (resource.remote_name.is_empty() || resource.display == DISPLAY_NETWORK)
    {
        return resource.provider.clone();
    }
    if options.full_path_shares {
        return resource.remote_name.clone();
    }
    point_to_name(&resource.remote_name).to_string()
}

/// FAR's `PointToName`: what follows the last backslash.
fn point_to_name(remote: &str) -> &str {
    remote.rsplit('\\').next().unwrap_or(remote)
}

/// A resource one can only connect to, a disk: Enter leaves for its folder (FAR's `ChangeToDirectory`).
pub fn is_share(resource: &NetResource) -> bool {
    resource.usage & USAGE_CONTAINER == 0
        && resource.kind & TYPE_DISK != 0
        && !resource.remote_name.is_empty()
}

fn key(path: &str) -> String {
    path.to_lowercase()
}

/// The folder above `path` and the last name in it.
pub fn split(path: &str) -> (&str, &str) {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some(("", name)) => ("/", name),
        Some((parent, name)) => (parent, name),
        None => ("/", path),
    }
}

/// `name` in the folder `folder`.
pub fn join(folder: &str, name: &str) -> String {
    if folder == "/" {
        format!("/{name}")
    } else {
        format!("{folder}/{name}")
    }
}

/// The panel's folders and the resources they were listed with.
#[derive(Default)]
pub struct Browser {
    known: HashMap<String, NetResource>,
}

impl Browser {
    /// The resource the folder `path` shows (none: the top), found as [`Browser`] says.
    pub fn resolve(
        &mut self,
        net: &dyn Network,
        options: &Options,
        path: &str,
    ) -> io::Result<Option<NetResource>> {
        if path == "/" || path.is_empty() {
            return Ok(None);
        }
        if let Some(known) = self.known.get(&key(path)) {
            return Ok(Some(known.clone()));
        }
        let (folder, name) = split(path);
        let parent = self.resolve(net, options, folder)?;
        let listed = net.enumerate(parent.as_ref()).unwrap_or_default();
        let found = listed
            .into_iter()
            .find(|one| name_of(one, options).eq_ignore_ascii_case(name));
        let resource = match found {
            Some(found) => found,
            // FAR's `GotoComputer`: a computer the network does not list, asked for by its name.
            None if name.starts_with(r"\\") => net.information(name)?,
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("No {name} in the network's {folder}"),
                ));
            }
        };
        self.known.insert(key(path), resource.clone());
        Ok(Some(resource))
    }

    /// The resource of the entry `name` of the folder `folder`, as its listing found it.
    pub fn entry(
        &mut self,
        net: &dyn Network,
        options: &Options,
        folder: &str,
        name: &str,
    ) -> io::Result<Option<NetResource>> {
        self.resolve(net, options, &join(folder, name))
    }

    /// The folder `path` listed (FAR's `GetFindData`): its resources, a server's hidden shares as the
    /// options say, printers when asked for; each entry's columns the disk it is connected as and its
    /// comment.
    pub fn list(
        &mut self,
        net: &dyn Network,
        options: &Options,
        path: &str,
    ) -> io::Result<Vec<FileEntry>> {
        let resource = self.resolve(net, options, path)?;
        // A share is no folder of the panel's (Enter and Ctrl+PgDn leave for it): never listed, never
        // connected to on the way.
        if resource.as_ref().is_some_and(is_share) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} is a share, not a folder of the network", split(path).1),
            ));
        }
        let mut listed = match net.enumerate(resource.as_ref()) {
            Ok(listed) => listed,
            Err(error) if resource.is_none() || crate::timed::not_answering(&error) => {
                return Err(error);
            }
            // FAR's "try again with connection", else an empty folder.
            Err(_) => {
                let current = resource.as_ref();
                let again =
                    current.map(|one| net.connect(one).and_then(|()| net.enumerate(current)));
                again.and_then(Result::ok).unwrap_or_default()
            }
        };
        listed = with_hidden_shares(net, options, resource.as_ref(), listed);
        let connected = net.connected().unwrap_or_default();
        let mut entries = Vec::new();
        for one in listed {
            if one.kind == TYPE_PRINT && !options.show_printers {
                continue;
            }
            let name = name_of(&one, options);
            if name.is_empty() {
                continue;
            }
            entries.push(entry(&one, &name, &connected, options));
            self.known.insert(key(&join(path, &name)), one);
        }
        Ok(entries)
    }

    /// The kind of the folder `path` (`DISPLAY_*`) as already found; none at the top or not found yet.
    pub fn display_of(&self, path: &str) -> Option<u32> {
        self.known.get(&key(path)).map(|resource| resource.display)
    }

    /// A folder's resource put down as found by another way (the path of a computer by its parents).
    pub fn remember(&mut self, path: &str, resource: NetResource) {
        self.known.insert(key(path), resource);
    }
}

/// The listing of `resource` as the panel shows it: a server's with its hidden shares as the options
/// say.
pub fn with_hidden_shares(
    net: &dyn Network,
    options: &Options,
    resource: Option<&NetResource>,
    mut listed: Vec<NetResource>,
) -> Vec<NetResource> {
    if let Some(server) = resource.filter(|one| one.display == DISPLAY_SERVER) {
        let hidden = hidden_shares(net, options, server, &listed);
        listed.extend(hidden);
    }
    listed
}

/// FAR's `GetHiddenShares`: a server's shares ending in `$` (but `IPC$`) its listing lacks, unless the
/// options never show them.
fn hidden_shares(
    net: &dyn Network,
    options: &Options,
    server: &NetResource,
    listed: &[NetResource],
) -> Vec<NetResource> {
    let shares_listed = listed
        .last()
        .is_none_or(|last| last.display == DISPLAY_SHARE);
    if options.hidden_shares == HiddenShares::Never || !shares_listed {
        return Vec::new();
    }
    (net.shares(&server.remote_name).into_iter())
        .filter(|share| share.name.ends_with('$') && !share.name.eq_ignore_ascii_case("IPC$"))
        .map(|share| NetResource {
            scope: SCOPE_GLOBALNET,
            kind: if share.kind == SHARE_PRINT {
                TYPE_PRINT
            } else {
                TYPE_DISK
            },
            display: DISPLAY_SHARE,
            usage: 1,
            remote_name: format!(r"{}\{}", server.remote_name, share.name),
            comment: share.remark,
            provider: server.provider.clone(),
            ..NetResource::default()
        })
        .collect()
}

/// FAR's panel item: a folder (a printer a file), hidden when a `$` share is shown as hidden; its
/// columns the disk it is connected as (FAR's `GetLocalName`) and its comment.
fn entry(
    resource: &NetResource,
    name: &str,
    connected: &[NetResource],
    options: &Options,
) -> FileEntry {
    let local = (connected.iter())
        .find(|one| one.remote_name.eq_ignore_ascii_case(&resource.remote_name))
        .map(|one| one.local_name.clone())
        .unwrap_or_default();
    let printer = resource.kind == TYPE_PRINT;
    let hidden = options.hidden_shares == HiddenShares::AsHidden && name.ends_with('$');
    FileEntry {
        name: name.to_string(),
        is_dir: !printer,
        hidden,
        attributes: if printer { VIRTUAL } else { 0 },
        columns: vec![local, resource.comment.clone()],
        description: resource.comment.clone(),
        ..FileEntry::default()
    }
}

#[cfg(test)]
#[path = "browse_tests.rs"]
mod tests;
