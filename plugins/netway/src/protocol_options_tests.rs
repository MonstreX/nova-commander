use super::*;

fn pairs(lines: &[(&str, &str)]) -> Vec<(String, String)> {
    (lines.iter())
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn the_defaults_are_winscps_and_write_nothing() {
    let (ftp, tls) = (Ftp::default(), Tls::default());
    assert!(ftp.passive && tls.session_reuse);
    assert_eq!(ftp.use_mlsd, Switch::Auto);
    assert!(ftp.lines().is_empty() && tls.lines().is_empty());
}

#[test]
fn switches_keep_winscps_values_and_netboxs_lines() {
    assert_eq!(
        Switch::ALL_VALUES.map(Switch::value),
        [0, 1, 2],
        "asOn, asOff, asAuto"
    );
    assert_eq!(
        Switch::ALL_VALUES.map(Switch::index),
        [2, 1, 0],
        "Auto, Off, On"
    );
    for switch in Switch::ALL_VALUES {
        assert_eq!(Switch::at(switch.index()), switch);
        assert_eq!(Switch::of(&switch.value().to_string()), Some(switch));
    }
    assert_eq!(Switch::of("x"), None);
}

#[test]
fn the_settings_go_round_their_lines() {
    let ftp = Ftp {
        passive: false,
        use_mlsd: Switch::Off,
        list_all: Switch::On,
        force_pasv_ip: Switch::On,
        host_command: Switch::On,
        account: "acct".to_string(),
        vms_all_revisions: true,
        allow_empty_password: true,
        dup_ff: true,
        undup_ff: true,
        post_login_commands: vec!["SITE UMASK 002".to_string(), "SITE 100%".to_string()],
    };
    let lines = ftp.lines();
    assert!(lines.contains(&("FtpUseMlsd".to_string(), "1".to_string())));
    assert!(lines.contains(&(
        "PostLoginCommands".to_string(),
        "SITE UMASK 002%0ASITE 100%25".to_string()
    )));
    assert_eq!(Ftp::read(&lines), ftp);
    let tls = Tls {
        certificate_file: r"C:\keys\client.pem".to_string(),
        session_reuse: false,
    };
    assert_eq!(Tls::read(&tls.lines()), tls);
    assert!(lines.iter().all(|(key, _)| KEYS.contains(&key.as_str())));
    assert!(
        tls.lines()
            .iter()
            .all(|(key, _)| KEYS.contains(&key.as_str()))
    );
}

#[test]
fn the_s3_and_webdav_settings_go_round_their_lines() {
    assert!(S3::default().lines().is_empty() && WebDav::default().lines().is_empty());
    let s3 = S3 {
        default_region: "eu-west-1".to_string(),
        path_style: true,
        requester_pays: true,
    };
    let lines = s3.lines();
    assert!(lines.contains(&("S3UrlStyle".to_string(), "1".to_string())));
    assert_eq!(S3::read(&lines), s3);
    let webdav = WebDav {
        liberal_escaping: true,
    };
    assert_eq!(WebDav::read(&webdav.lines()), webdav);
    let all = [lines, webdav.lines()].concat();
    assert!(all.iter().all(|(key, _)| KEYS.contains(&key.as_str())));
}

#[test]
fn values_not_understood_keep_the_defaults() {
    let read = Ftp::read(&pairs(&[
        ("FtpUseMlsd", "7"),
        ("FtpPasvMode", "maybe"),
        ("PostLoginCommands", "%0A%0A"),
    ]));
    assert_eq!(read, Ftp::default());
}
