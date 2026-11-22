//! The session dialog's Connection, Proxy and Tunnel tabs (NetBox's `TSessionDialog`, S6.5h): the
//! server's response timeout, keepalives (off, null SSH packets, dummy protocol commands) and their
//! interval, the IP version; the proxy's type (None, SOCKS4, SOCKS5, HTTP, Telnet, Local), host, port,
//! user, password, Telnet or local command, "Consider proxying local host connections", "Do DNS name
//! lookup at proxy end"; "Connect through SSH tunnel" with the tunnel's host, port, user, password and
//! private key. What the choices leave unused is disabled (`UpdateControls`).

use nc_plugin_sdk::{Controls, DialogItem, Value};

use crate::link_options::{Family, Link, Ping, ProxyDns, ProxyMethod};
use crate::placed::{at, heading, line};
use crate::session::Protocol;

/// The first item of Connection, Proxy and Tunnel, and the one after them.
pub const FIRST: [usize; 4] = [60, 74, 95, 108];

// The items, in their order.
const TIMEOUT: usize = 62;
const PING_OFF: usize = 65;
const PING_NULL: usize = 66;
const PING_DUMMY: usize = 67;
const INTERVAL_LABEL: usize = 68;
const INTERVAL: usize = 69;
const FAMILY_V4: usize = 72;
const FAMILY_V6: usize = 73;
pub const PROXY: usize = 76;
const PROXY_HOST_LABEL: usize = 77;
const PROXY_PORT_LABEL: usize = 78;
const PROXY_HOST: usize = 79;
const PROXY_PORT: usize = 80;
const PROXY_USER_LABEL: usize = 81;
const PROXY_PASSWORD_LABEL: usize = 82;
const PROXY_USER: usize = 83;
const PROXY_PASSWORD: usize = 84;
const TELNET_LABEL: usize = 86;
const TELNET: usize = 87;
const LOCAL_LABEL: usize = 88;
const LOCAL: usize = 89;
const LOCALHOST: usize = 90;
const DNS_LABEL: usize = 91;
const DNS_NO: usize = 92;
const DNS_AUTO: usize = 93;
const DNS_YES: usize = 94;
pub const TUNNEL: usize = 96;
const TUNNEL_HOST: usize = 100;
const TUNNEL_PORT: usize = 101;
const TUNNEL_USER: usize = 104;
const TUNNEL_PASSWORD: usize = 105;
const TUNNEL_KEY: usize = 107;

/// The items whose change changes what is enabled.
pub const FOLLOWED: [usize; 5] = [PING_OFF, PING_NULL, PING_DUMMY, PROXY, TUNNEL];

/// Each tab's item the focus goes to.
pub const FOCUS: [usize; 3] = [TIMEOUT, PROXY, TUNNEL];

const LEFT: i32 = 5;
const WIDTH: i32 = 59;

fn connection(link: &Link) -> Vec<DialogItem> {
    let ping = |kind: Ping| link.ping == kind;
    let family = |kind: Family| link.family == kind;
    vec![
        heading("Timeouts", LEFT - 1, 3, WIDTH + 2),
        at(DialogItem::text("Server &response timeout:"), LEFT, 4),
        line(
            DialogItem::fixed("####", &link.timeout.to_string()),
            31,
            4,
            4,
        ),
        at(DialogItem::text("seconds (0 - no timeout)"), 36, 4),
        heading("Keepalives", LEFT - 1, 5, WIDTH + 2),
        at(DialogItem::radio("&Off", ping(Ping::Off)).group(), LEFT, 6),
        at(
            DialogItem::radio("Sending of &null SSH packets", ping(Ping::NullPacket)),
            LEFT,
            7,
        ),
        at(
            DialogItem::radio(
                "Executing &dummy protocol commands",
                ping(Ping::DummyCommand),
            ),
            LEFT,
            8,
        ),
        at(DialogItem::text("Seconds &between keepalives:"), LEFT, 9),
        line(
            DialogItem::fixed("####", &link.ping_interval.to_string()),
            36,
            9,
            4,
        ),
        heading("Internet protocol version", LEFT - 1, 10, WIDTH + 2),
        at(
            DialogItem::radio("A&uto", family(Family::Auto)).group(),
            LEFT,
            11,
        ),
        at(DialogItem::radio("IPv&4", family(Family::V4)), 16, 11),
        at(DialogItem::radio("IPv&6", family(Family::V6)), 27, 11),
    ]
}

fn proxy(link: &Link) -> Vec<DialogItem> {
    let methods: Vec<&str> = ProxyMethod::ALL.iter().map(|one| one.label()).collect();
    let chosen = ProxyMethod::ALL.iter().position(|one| *one == link.proxy);
    let dns = |kind: ProxyDns| link.proxy_dns == kind;
    vec![
        heading("Proxy", LEFT - 1, 3, WIDTH + 2),
        at(DialogItem::text("Proxy &type:"), LEFT, 4),
        line(DialogItem::list(&methods, chosen.unwrap_or(0)), 21, 4, 12),
        at(DialogItem::text("Pro&xy host name:"), LEFT, 5),
        at(DialogItem::text("Po&rt number:"), 52, 5),
        line(DialogItem::edit(&link.proxy_host), LEFT, 6, 45),
        line(
            DialogItem::fixed("999999", &link.proxy_port.to_string()),
            52,
            6,
            6,
        ),
        at(DialogItem::text("&User name:"), LEFT, 7),
        at(DialogItem::text("&Password:"), 35, 7),
        line(DialogItem::edit(&link.proxy_user), LEFT, 8, 28),
        line(DialogItem::password(&link.proxy_password), 35, 8, 29),
        heading("Proxy settings", LEFT - 1, 9, WIDTH + 2),
        at(DialogItem::text("Telnet co&mmand:"), LEFT, 10),
        line(DialogItem::edit(&link.telnet_command), 28, 10, 36),
        at(DialogItem::text("Local proxy co&mmand:"), LEFT, 10),
        line(DialogItem::edit(&link.local_command), 28, 10, 36),
        at(
            DialogItem::check(
                "Co&nsider proxying local host connections",
                link.proxy_localhost,
            ),
            LEFT,
            11,
        ),
        at(
            DialogItem::text("Do &DNS name lookup at proxy end:"),
            LEFT,
            12,
        ),
        at(DialogItem::radio("No", dns(ProxyDns::No)).group(), 40, 12),
        at(DialogItem::radio("Auto", dns(ProxyDns::Auto)), 47, 12),
        at(DialogItem::radio("Yes", dns(ProxyDns::Yes)), 56, 12),
    ]
}

fn tunnel(link: &Link) -> Vec<DialogItem> {
    vec![
        heading("Tunnel", LEFT - 1, 3, WIDTH + 2),
        at(
            DialogItem::check("&Connect through SSH tunnel", link.tunnel),
            LEFT,
            4,
        ),
        heading("Host to setup tunnel on", LEFT - 1, 5, WIDTH + 2),
        at(DialogItem::text("&Host name:"), LEFT, 6),
        at(DialogItem::text("Po&rt number:"), 52, 6),
        line(DialogItem::edit(&link.tunnel_host), LEFT, 7, 45),
        line(
            DialogItem::fixed("999999", &link.tunnel_port.to_string()),
            52,
            7,
            6,
        ),
        at(DialogItem::text("&User name:"), LEFT, 8),
        at(DialogItem::text("&Password:"), 35, 8),
        line(DialogItem::edit(&link.tunnel_user), LEFT, 9, 28),
        line(DialogItem::password(&link.tunnel_password), 35, 9, 29),
        at(DialogItem::text("Private &key file:"), LEFT, 10),
        line(DialogItem::edit(&link.tunnel_key), LEFT, 11, WIDTH),
    ]
}

/// The three tabs' items, hidden.
pub fn items(link: &Link) -> Vec<DialogItem> {
    let mut items = connection(link);
    items.extend(proxy(link));
    items.extend(tunnel(link));
    items.into_iter().map(|item| item.hidden(true)).collect()
}

/// The settings the dialog's `values` give, over `base` (a number not typed keeps it).
pub fn link_of(values: &[Value], base: &Link) -> Link {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let checked = |item: usize| value(item).checked();
    let text = |item: usize| value(item).text.trim().to_string();
    let number = |item: usize, old: u32| value(item).text.trim().parse().unwrap_or(old);
    let port = |item: usize, old: u16| value(item).text.trim().parse().unwrap_or(old);
    let chosen = usize::try_from(value(PROXY).state).unwrap_or(0);
    Link {
        timeout: number(TIMEOUT, base.timeout),
        ping: pick(
            [
                (PING_NULL, Ping::NullPacket),
                (PING_DUMMY, Ping::DummyCommand),
            ],
            &checked,
        )
        .unwrap_or(Ping::Off),
        ping_interval: number(INTERVAL, base.ping_interval),
        family: pick([(FAMILY_V4, Family::V4), (FAMILY_V6, Family::V6)], &checked)
            .unwrap_or(Family::Auto),
        proxy: ProxyMethod::ALL.get(chosen).copied().unwrap_or_default(),
        proxy_host: text(PROXY_HOST),
        proxy_port: port(PROXY_PORT, base.proxy_port),
        proxy_user: text(PROXY_USER),
        proxy_password: value(PROXY_PASSWORD).text,
        telnet_command: text(TELNET),
        local_command: text(LOCAL),
        proxy_dns: pick([(DNS_NO, ProxyDns::No), (DNS_YES, ProxyDns::Yes)], &checked)
            .unwrap_or(ProxyDns::Auto),
        proxy_localhost: checked(LOCALHOST),
        tunnel: checked(TUNNEL),
        tunnel_host: text(TUNNEL_HOST),
        tunnel_port: port(TUNNEL_PORT, base.tunnel_port),
        tunnel_user: text(TUNNEL_USER),
        tunnel_password: value(TUNNEL_PASSWORD).text,
        tunnel_key: text(TUNNEL_KEY),
    }
}

/// The choice of the first radio button checked among `choices`.
fn pick<T: Copy, const N: usize>(
    choices: [(usize, T); N],
    checked: &impl Fn(usize) -> bool,
) -> Option<T> {
    choices
        .iter()
        .find(|(item, _)| checked(*item))
        .map(|(_, choice)| *choice)
}

/// What is unused disabled or hidden (NetBox's `UpdateControls`): null packets but for SSH, the interval
/// while keepalives are off; the proxy's fields by its type (the Telnet or the local command shown);
/// the tunnel's fields while it is off.
pub fn update(controls: &mut Controls, protocol: Protocol, shown_proxy: bool) {
    controls.enable(PING_NULL, protocol.is_ssh());
    let pinging = !controls.value(PING_OFF).checked();
    controls.enable(INTERVAL_LABEL, pinging);
    controls.enable(INTERVAL, pinging);
    let state = usize::try_from(controls.value(PROXY).state).unwrap_or(0);
    let method = ProxyMethod::ALL.get(state).copied().unwrap_or_default();
    let used = method != ProxyMethod::None;
    let remote = used && method != ProxyMethod::Local;
    for item in [PROXY_HOST_LABEL, PROXY_PORT_LABEL, PROXY_HOST, PROXY_PORT] {
        controls.enable(item, remote);
    }
    let login = matches!(
        method,
        ProxyMethod::Socks5 | ProxyMethod::Http | ProxyMethod::Telnet
    );
    controls.enable(PROXY_USER_LABEL, remote);
    controls.enable(PROXY_USER, remote);
    controls.enable(PROXY_PASSWORD_LABEL, login);
    controls.enable(PROXY_PASSWORD, login);
    let local = method == ProxyMethod::Local;
    for (item, shown) in [
        (TELNET_LABEL, !local),
        (TELNET, !local),
        (LOCAL_LABEL, local),
        (LOCAL, local),
    ] {
        controls.show(item, shown && shown_proxy);
    }
    controls.enable(TELNET_LABEL, method == ProxyMethod::Telnet);
    controls.enable(TELNET, method == ProxyMethod::Telnet);
    for item in [LOCALHOST, DNS_LABEL, DNS_NO, DNS_AUTO, DNS_YES] {
        controls.enable(item, used);
    }
    let tunnel = controls.value(TUNNEL).checked();
    for item in FIRST[2] + 2..FIRST[3] {
        controls.enable(item, tunnel);
    }
}

/// Whether item `item` is one `update` shows or hides itself (the proxy's two commands).
pub fn shown_by_update(item: usize) -> bool {
    (TELNET_LABEL..=LOCAL).contains(&item)
}

#[cfg(test)]
#[path = "session_link_tabs_tests.rs"]
mod tests;
