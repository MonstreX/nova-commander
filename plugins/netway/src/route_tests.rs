use super::*;

#[test]
fn local_hosts_go_straight_unless_asked() {
    assert!(is_local("localhost") && is_local("127.0.0.1") && is_local("[::1]"));
    assert!(!is_local("example.com") && !is_local("10.0.0.1"));
    let link = Link {
        proxy: ProxyMethod::Socks5,
        ..Link::default()
    };
    assert!(!proxied("127.0.0.1", &link));
    assert!(proxied("example.com", &link));
    let asked = Link {
        proxy_localhost: true,
        ..link
    };
    assert!(proxied("127.0.0.1", &asked));
    assert!(!proxied("example.com", &Link::default()));
}

/// R7 review A1: the local command's way carries the bytes both ways; dropped, it kills the command and
/// waits for its pumps (left running, they could outlive the plugin's library).
#[test]
fn a_local_commands_way_ends_its_command_and_pumps_when_dropped() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let link = Link {
        local_command: if cfg!(windows) { "more" } else { "cat" }.to_string(),
        ..Link::default()
    };
    let started = std::time::Instant::now();
    runtime.block_on(async {
        let mut way = local_command("h", 1, &link).unwrap();
        way.write_all(b"hi\r\n").await.unwrap();
        way.flush().await.unwrap();
        let mut echoed = [0u8; 2];
        way.read_exact(&mut echoed).await.unwrap();
        assert_eq!(&echoed, b"hi");
        drop(way);
    });
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the drop waited for ever"
    );
}

#[test]
fn the_local_commands_fields_are_put_in() {
    let link = Link {
        proxy_user: "u".to_string(),
        proxy_host: "gate".to_string(),
        proxy_port: 8080,
        ..Link::default()
    };
    assert_eq!(
        command_of(
            "plink %proxyhost:%proxyport -nc %host:%port -l %user",
            "srv",
            22,
            &link
        ),
        "plink gate:8080 -nc srv:22 -l u"
    );
}
