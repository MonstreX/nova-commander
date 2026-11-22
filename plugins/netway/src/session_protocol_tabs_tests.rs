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

fn with(ftp: Ftp, tls: Tls) -> Options {
    Options {
        ftp,
        tls,
        ..Options::default()
    }
}

fn changed() -> (Ftp, Tls) {
    let ftp = Ftp {
        passive: false,
        use_mlsd: Switch::Off,
        list_all: Switch::On,
        force_pasv_ip: Switch::On,
        host_command: Switch::Off,
        account: "acct".to_string(),
        vms_all_revisions: true,
        allow_empty_password: true,
        dup_ff: true,
        undup_ff: true,
        post_login_commands: vec!["SITE UMASK 002".to_string(), "NOOP".to_string()],
    };
    let tls = Tls {
        certificate_file: r"C:\keys\client.pem".to_string(),
        session_reuse: false,
    };
    (ftp, tls)
}

#[test]
fn the_items_are_where_the_dialog_reads_them() {
    let items = items(&Options::default());
    assert_eq!(items.len(), FIRST[4] - FIRST[0]);
    let item = |index: usize| &items[index - FIRST[0]];
    assert_eq!(item(USE_MLSD).lines, ["Auto", "Off", "On"]);
    assert_eq!(
        (item(USE_MLSD).kind, item(USE_MLSD).state),
        (ItemKind::List, 0)
    );
    assert_eq!(
        item(HOST_COMMAND - 1).text,
        "Use &HOST command to select host on the server"
    );
    assert_eq!(item(ACCOUNT).kind, ItemKind::Edit);
    assert_eq!(item(PASSIVE).text, "&Passive mode");
    assert_eq!(item(PASSIVE).state, 1, "passive by default");
    assert_eq!(item(UNDUP_FF).text, "&UnDup FF from PWD");
    assert_eq!(item(CERTIFICATE_LABEL).text, "TLS client &certificate:");
    assert_eq!(item(COMMANDS[0] - 1).text, "Post login &commands:");
    assert_eq!(item(COMMANDS[2]).kind, ItemKind::Edit);
    assert_eq!(item(SESSION_REUSE).text, "&Session ID re-use");
    assert!(items.iter().all(|item| item.hidden));
    // The tab's last line above the buttons' separator (line 21).
    assert!(items.iter().all(|item| item.place.3 < 21));
}

#[test]
fn the_settings_come_back_as_they_were_shown() {
    for (ftp, tls) in [(Ftp::default(), Tls::default()), changed()] {
        let values = values_of(&items(&with(ftp.clone(), tls.clone())));
        assert_eq!(ftp_of(&values, &Ftp::default()), ftp);
        assert_eq!(tls_of(&values, &tls), tls);
    }
}

#[test]
fn commands_after_the_third_are_kept() {
    let base = Ftp {
        post_login_commands: ["A", "B", "C", "D"].map(str::to_string).to_vec(),
        ..Ftp::default()
    };
    let values = values_of(&items(&with(base.clone(), Tls::default())));
    assert_eq!(
        ftp_of(&values, &base).post_login_commands,
        base.post_login_commands
    );
}

#[test]
fn the_webdav_and_s3_tabs_show_and_give_their_settings() {
    let mut options = Options::default();
    options.ssh.compression = true;
    options.webdav.liberal_escaping = true;
    options.tls.certificate_file = "client.pem".to_string();
    options.s3 = S3 {
        default_region: "eu-west-1".to_string(),
        path_style: true,
        requester_pays: true,
    };
    let items = items(&options);
    let item = |index: usize| &items[index - FIRST[0]];
    assert_eq!(item(COMPRESSION).text, "Enable &compression");
    assert_eq!(item(COMPRESSION).state, 1);
    assert_eq!(item(WEBDAV_CERTIFICATE).text, "client.pem");
    assert_eq!(item(URL_STYLE).lines, ["Virtual Host", "Path"]);
    assert_eq!(item(REGION).history, "S3DefaultRegion");
    let values = values_of(&items);
    assert_eq!(s3_of(&values), options.s3);
    assert_eq!(webdav_of(&values), options.webdav);
    assert!(compression_of(&values, (true, true)));
}

/// One setting on two tabs: the line changed wins (the client certificate, the compression).
#[test]
fn the_tab_changed_gives_a_shared_setting() {
    let base = Tls {
        certificate_file: "old.pem".to_string(),
        ..Tls::default()
    };
    let mut values = values_of(&items(&with(Ftp::default(), base.clone())));
    values[WEBDAV_CERTIFICATE].text = "webdav.pem".to_string();
    assert_eq!(tls_of(&values, &base).certificate_file, "webdav.pem");
    values[WEBDAV_CERTIFICATE].text = "old.pem".to_string();
    values[CERTIFICATE].text = "ftp.pem".to_string();
    assert_eq!(tls_of(&values, &base).certificate_file, "ftp.pem");
    values[COMPRESSION].state = 1;
    assert!(
        compression_of(&values, (false, false)),
        "the WebDAV tab's change"
    );
    values[COMPRESSION].state = 0;
    assert!(
        compression_of(&values, (false, true)),
        "the SSH tab's change"
    );
}
