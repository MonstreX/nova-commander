use super::*;
use nc_plugin_sdk::ItemKind;

fn values_of(items: &[DialogItem]) -> Vec<Value> {
    let mut values = vec![Value::default(); FIRST[0]];
    values.extend(items.iter().map(|item| Value {
        text: item.text.clone(),
        state: item.state,
    }));
    values
}

fn changed() -> Link {
    Link {
        timeout: 40,
        ping: Ping::DummyCommand,
        ping_interval: 5,
        family: Family::V4,
        proxy: ProxyMethod::Http,
        proxy_host: "gate".to_string(),
        proxy_port: 3128,
        proxy_user: "pu".to_string(),
        proxy_password: "pp".to_string(),
        telnet_command: "open %host %port".to_string(),
        local_command: "nc %host %port".to_string(),
        proxy_dns: ProxyDns::Yes,
        proxy_localhost: true,
        tunnel: true,
        tunnel_host: "jump".to_string(),
        tunnel_port: 2222,
        tunnel_user: "tu".to_string(),
        tunnel_password: "tp".to_string(),
        tunnel_key: r"C:\k".to_string(),
    }
}

#[test]
fn the_items_are_where_the_dialog_reads_them() {
    let items = items(&Link::default());
    assert_eq!(items.len(), FIRST[3] - FIRST[0]);
    let item = |index: usize| &items[index - FIRST[0]];
    assert_eq!(
        (item(TIMEOUT).kind, item(TIMEOUT).text.as_str()),
        (ItemKind::Fixed, "15")
    );
    assert_eq!(item(PING_DUMMY).kind, ItemKind::Radio);
    assert_eq!(item(FAMILY_V6).text, "IPv&6");
    assert_eq!(item(PROXY).lines[5], "Local");
    assert_eq!(item(PROXY_HOST).text, "proxy");
    assert_eq!(item(PROXY_PASSWORD).kind, ItemKind::Password);
    assert_eq!(item(TELNET).text, "connect %host %port\\n");
    assert_eq!(item(DNS_YES).text, "Yes");
    assert_eq!(item(TUNNEL).kind, ItemKind::Check);
    assert_eq!(item(TUNNEL_PORT).text, "22");
    assert_eq!(item(TUNNEL_KEY).kind, ItemKind::Edit);
    assert!(items.iter().all(|item| item.hidden));
}

#[test]
fn the_settings_come_back_as_they_were_shown() {
    for link in [Link::default(), changed()] {
        let values = values_of(&items(&link));
        assert_eq!(link_of(&values, &Link::default()), link);
    }
}

#[test]
fn only_the_proxys_commands_are_shown_by_update() {
    assert!(shown_by_update(TELNET) && shown_by_update(LOCAL_LABEL));
    assert!(!shown_by_update(LOCALHOST) && !shown_by_update(PROXY_PASSWORD));
}
