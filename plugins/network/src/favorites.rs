//! FAR's Network favorites (`NetFavorites.cpp`): the computers and shares a name and password were
//! remembered for ("Remember password" in the logon dialog), each kept with them. FAR keeps them as the
//! plugin's settings `Favorites\SERVER[\share]` with `UserName` and `Password`; here as one setting of
//! the plugin's, `Favorites`, a line for each: the server (upper case, as FAR's `SplitPath` makes it),
//! the share (empty for the server's own), the name and the password, separated by tabs (a tab, a line
//! break and `\` written as `\t`, `\n`, `\\`). The password is kept as FAR keeps it, as text (owner).

use std::io;

use nc_plugin_sdk::system::network::{
    DISPLAY_DOMAIN, DISPLAY_SERVER, NetResource, SCOPE_GLOBALNET, Share, TYPE_ANY, USAGE_CONTAINER,
};

use crate::browse::Network;

/// The setting the favorites are kept in.
pub const SETTING: &str = "Favorites";
/// FAR's provider of the Favorites folder (`szFavProv`).
pub const PROVIDER: &str = "Far Favorites Provider";
/// The folder's name at the top (FAR's `MFavorites`).
pub const FOLDER: &str = "Favorites";

/// One remembered connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favorite {
    pub server: String,
    pub share: String,
    pub user: String,
    pub password: String,
}

/// FAR's `SplitPath`: the server (upper case) and the share of `remote` (`\\server\share`, `\\server`;
/// FAR's `Favorites\...` too).
pub fn split(remote: &str) -> (String, String) {
    let name = remote.strip_prefix(FOLDER).unwrap_or(remote);
    let name = name.trim_start_matches('\\');
    match name.split_once('\\') {
        Some((server, share)) => (server.to_uppercase(), share.to_string()),
        None => (name.to_uppercase(), String::new()),
    }
}

fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' => out.push_str(r"\\"),
            '\t' => out.push_str(r"\t"),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            other => out.push(other),
        }
    }
    out
}

fn unescaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// The favorites as the setting keeps them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Favorites(Vec<Favorite>);

impl Favorites {
    /// Read from the setting's text; a line not of four fields is let be.
    pub fn parse(text: &str) -> Self {
        let read = (text.lines())
            .filter_map(|line| {
                let fields: Vec<String> = line.split('\t').map(unescaped).collect();
                let [server, share, user, password] = <[String; 4]>::try_from(fields).ok()?;
                (!server.is_empty()).then_some(Favorite {
                    server: server.to_uppercase(),
                    share,
                    user,
                    password,
                })
            })
            .collect();
        Self(read)
    }

    /// The setting's text.
    pub fn text(&self) -> String {
        (self.0.iter())
            .map(|one| {
                [&one.server, &one.share, &one.user, &one.password]
                    .map(|field| escaped(field))
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// FAR's `EnumFavorites`: the servers, each once (its shares' too), by name.
    pub fn servers(&self) -> Vec<String> {
        let mut servers: Vec<String> = self.0.iter().map(|one| one.server.clone()).collect();
        servers.sort();
        servers.dedup();
        servers
    }

    /// FAR's `InFavoriteExists`: whether the server (or the share) of `remote` is kept.
    pub fn contains(&self, remote: &str) -> bool {
        let (server, share) = split(remote);
        (self.0.iter()).any(|one| {
            one.server == server && (share.is_empty() || one.share.eq_ignore_ascii_case(&share))
        })
    }

    /// FAR's `ReadFavoriteItem`: the name and password kept for exactly `remote`.
    pub fn logon(&self, remote: &str) -> Option<(String, String)> {
        let (server, share) = split(remote);
        (self.0.iter())
            .find(|one| one.server == server && one.share.eq_ignore_ascii_case(&share))
            .map(|one| (one.user.clone(), one.password.clone()))
    }

    /// FAR's `WriteFavoriteItem`: the name and password kept for `remote`, over what was kept.
    pub fn write(&mut self, remote: &str, user: &str, password: &str) {
        let (server, share) = split(remote);
        // A resource of no server's (a provider): nothing to keep.
        if server.is_empty() {
            return;
        }
        self.0
            .retain(|one| !(one.server == server && one.share.eq_ignore_ascii_case(&share)));
        self.0.push(Favorite {
            server,
            share,
            user: user.to_string(),
            password: password.to_string(),
        });
    }

    /// FAR's `RemoveFromFavorites` of a Favorites folder's entry (`\\SERVER`): the server with its
    /// shares; a share's name only that share.
    pub fn remove(&mut self, remote: &str) {
        let (server, share) = split(remote);
        self.0.retain(|one| {
            one.server != server || (!share.is_empty() && !one.share.eq_ignore_ascii_case(&share))
        });
    }
}

/// FAR's Favorites folder (`EnumFavorites`'s resource: its provider's, a domain's kind).
pub fn folder() -> NetResource {
    NetResource {
        scope: SCOPE_GLOBALNET,
        kind: TYPE_ANY,
        display: DISPLAY_DOMAIN,
        usage: USAGE_CONTAINER,
        provider: PROVIDER.to_string(),
        ..NetResource::default()
    }
}

/// Whether `resource` is the Favorites folder (FAR's `CheckFavoriteItem`).
pub fn is_folder(resource: &NetResource) -> bool {
    resource.provider == PROVIDER
}

/// The network with FAR's Favorites folder (`NetResourceList::Enumerate` with `EnumFavorites`): first at
/// the top, its computers under it (FAR's `FAVORITES_CHECK_RESOURCES`: taken as servers, not asked).
pub struct Favored<'a> {
    pub inner: &'a dyn Network,
    pub favorites: &'a Favorites,
}

/// `inner` with the Favorites folder of `favorites`.
pub fn favored<'a>(inner: &'a dyn Network, favorites: &'a Favorites) -> Favored<'a> {
    Favored { inner, favorites }
}

impl Network for Favored<'_> {
    fn enumerate(&self, parent: Option<&NetResource>) -> io::Result<Vec<NetResource>> {
        match parent {
            None => {
                let mut listed = vec![folder()];
                listed.extend(self.inner.enumerate(None)?);
                Ok(listed)
            }
            Some(parent) if is_folder(parent) => Ok((self.favorites.servers().into_iter())
                .map(|server| NetResource {
                    scope: SCOPE_GLOBALNET,
                    kind: TYPE_ANY,
                    display: DISPLAY_SERVER,
                    usage: USAGE_CONTAINER,
                    remote_name: format!(r"\\{server}"),
                    ..NetResource::default()
                })
                .collect()),
            Some(parent) => self.inner.enumerate(Some(parent)),
        }
    }

    fn connected(&self) -> io::Result<Vec<NetResource>> {
        self.inner.connected()
    }

    fn information(&self, remote: &str) -> io::Result<NetResource> {
        self.inner.information(remote)
    }

    fn parent(&self, resource: &NetResource) -> io::Result<NetResource> {
        self.inner.parent(resource)
    }

    fn shares(&self, server: &str) -> Vec<Share> {
        self.inner.shares(server)
    }

    fn connect(&self, resource: &NetResource) -> io::Result<()> {
        self.inner.connect(resource)
    }

    fn keep(&self, parent: Option<&NetResource>, answer: io::Result<Vec<NetResource>>) {
        // The top's answer kept is the network's own, the folder added as it is read; the folder's own
        // listing is never the network's to keep.
        let answer = match parent {
            Some(folder) if is_folder(folder) => return,
            None => answer.map(|listed| listed.into_iter().filter(|one| !is_folder(one)).collect()),
            Some(_) => answer,
        };
        self.inner.keep(parent, answer);
    }
}

#[cfg(test)]
#[path = "favorites_tests.rs"]
mod tests;
