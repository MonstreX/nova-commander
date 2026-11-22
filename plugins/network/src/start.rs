//! Where a panel opens (FAR's `GotoLocalNetwork`, `SetOpenFromCommandLine`, `GotoComputer`): a resource
//! asked for by its name, its folder the names of its parents above it as far as the network tells them
//! (a computer whose parents it does not tell stands at the top), a share's name the cursor's.

use std::io;

use nc_plugin_sdk::system::network::{DISPLAY_NETWORK, DISPLAY_SHARE, NetResource};

use crate::browse::{Network, join, name_of};
use crate::favorites::Favorites;
use crate::options::{Options, UPBROWSE_TO_FAVORITES};

/// A folder to open and the resources found on the way, by their paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: String,
    pub cursor: Option<String>,
    pub known: Vec<(String, NetResource)>,
}

/// FAR's `GotoLocalNetwork`: this computer's domain or workgroup, by its provider; the Favorites folder
/// when this computer is kept there and the options browse up to it (FAR's `GetResourceParent`).
pub fn local_network(
    net: &dyn Network,
    options: &Options,
    favorites: &Favorites,
) -> Option<(String, NetResource)> {
    let name = nc_plugin_sdk::system::network::computer_name()?;
    let own = net.information(&format!(r"\\{name}")).ok()?;
    let microsoft = own.provider.is_empty() || own.provider.contains("Microsoft");
    if !microsoft {
        return None;
    }
    if up_to_favorites(&own, options, favorites) {
        let path = format!("/{}", crate::favorites::FOLDER);
        return Some((path, crate::favorites::folder()));
    }
    let parent = net.parent(&own).ok()?;
    let path = join(&join("/", &parent.provider), &name_of(&parent, options));
    Some((path, parent))
}

/// The resource `net:` names (FAR's `SetOpenFromCommandLine`): `\\name` as given (forward slashes
/// turned), a bare name a computer's (`\\name`) unless `netg:` or the settings take it for a domain's;
/// none for an empty command, which opens the panel as its menu's item does.
pub fn target(prefix: &str, command: &str, options: &Options) -> Option<String> {
    let typed = command.trim().trim_matches('"');
    if typed.is_empty() {
        return None;
    }
    let mut chars: Vec<char> = typed.chars().collect();
    for at in 0..chars.len().min(2) {
        if chars[at] == '/' {
            chars[at] = '\\';
        }
    }
    let typed: String = chars.into_iter().collect();
    let domains = prefix.eq_ignore_ascii_case("netg") || options.navigate_to_domains;
    if domains || typed.starts_with(r"\\") {
        return Some(typed);
    }
    Some(format!(r"\\{}", typed.trim_start_matches('\\')))
}

/// FAR's `ExpandEnvironmentStrings` of a command: each `%NAME%` the variable's value, a name with no
/// value left as it is.
pub fn expand_variables(text: &str) -> String {
    let mut expanded = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            break;
        };
        let name = &after[..end];
        expanded.push_str(&rest[..start]);
        match std::env::var(name).ok().filter(|_| !name.is_empty()) {
            Some(value) => {
                expanded.push_str(&value);
                rest = &after[end + 1..];
            }
            None => {
                expanded.push('%');
                rest = after;
            }
        }
    }
    expanded.push_str(rest);
    expanded
}

/// The prefix the program calls the plugin by from a share's root ("..", Ctrl+PgUp; FAR's call of its
/// Network plugin with `OPEN_FILEPANEL` and the share's name).
pub const SHARE_UP: &str = "net_up";

/// FAR's `SetOpenFromFilePanel`: whether the share `name` is one the plugin goes up from - the network
/// knows it and it is Microsoft's (FAR: "we don't support upwards browsing from NetWare shares").
pub fn goes_up_from(net: &dyn Network, name: &str) -> bool {
    net.information(name).is_ok_and(|resource| {
        resource.provider.is_empty() || resource.provider.contains("Microsoft")
    })
}

/// Whether the folder above `resource` is the Favorites folder (FAR's `GetResourceParent` with "Browse
/// up to favorites if possible", `GetFavoritesParent`: a resource kept there, no share, not the folder
/// itself).
pub fn up_to_favorites(resource: &NetResource, options: &Options, favorites: &Favorites) -> bool {
    options.favorites_flags & UPBROWSE_TO_FAVORITES != 0
        && resource.display != DISPLAY_SHARE
        && !crate::favorites::is_folder(resource)
        && !resource.remote_name.is_empty()
        && favorites.contains(&resource.remote_name)
}

/// FAR's `GotoComputer`: the computer (or domain) of `target` asked for, its folder by its parents (a
/// computer kept in the favorites under the Favorites folder, as the options say), a share in it
/// (`\\server\share`) the cursor's.
pub fn computer(
    net: &dyn Network,
    options: &Options,
    favorites: &Favorites,
    target: &str,
) -> io::Result<Found> {
    let (computer, share) = match target.get(2..).and_then(|rest| rest.find(['\\', '/'])) {
        Some(at) if target.starts_with(r"\\") => (&target[..at + 2], Some(&target[at + 3..])),
        _ => (target, None),
    };
    let computer = computer.to_uppercase();
    let resource = net.information(&computer)?;
    let favored = up_to_favorites(&resource, options, favorites);
    let mut chain = vec![resource];
    if favored {
        chain.push(crate::favorites::folder());
    }
    while chain.len() < 8 && !favored {
        let Some(top) = chain.last() else { break };
        if top.display == DISPLAY_NETWORK {
            break;
        }
        match net.parent(top) {
            Ok(parent) if parent.remote_name != top.remote_name => chain.push(parent),
            _ => break,
        }
    }
    chain.reverse();
    let mut path = "/".to_string();
    let mut known = Vec::new();
    for resource in chain {
        path = join(&path, &name_of(&resource, options));
        known.push((path.clone(), resource));
    }
    let cursor = share.filter(|share| !share.is_empty()).map(|share| {
        let whole = format!(r"{computer}\{}", share.replace('/', "\\"));
        if options.full_path_shares {
            whole
        } else {
            share.to_string()
        }
    });
    Ok(Found {
        path,
        cursor,
        known,
    })
}

#[cfg(test)]
#[path = "start_tests.rs"]
mod tests;
