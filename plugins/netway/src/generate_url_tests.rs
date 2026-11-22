use super::*;

fn sftp() -> Parts {
    Parts {
        protocol: Protocol::Sftp,
        encryption: Encryption::None,
        user: "user@corp".to_string(),
        password: "p:w/d".to_string(),
        host: "example.com".to_string(),
        port: 22,
        fingerprint: Some("SHA256:ab+c/d=".to_string()),
        folder: "/home/user/my dir".to_string(),
    }
}

/// WinSCP's `GenerateSessionUrl`: the parts asked for, encoded; the default port left out.
#[test]
fn the_url_has_the_parts_asked_for() {
    let parts = sftp();
    assert_eq!(url(&parts, Include::default()), "sftp://example.com/");
    let user = Include {
        user: true,
        ..Include::default()
    };
    assert_eq!(url(&parts, user), "sftp://user%40corp@example.com/");
    let all = Include {
        user: true,
        password: true,
        host_key: true,
        folder: true,
        http_for_webdav: false,
    };
    assert_eq!(
        url(&parts, all),
        "sftp://user%40corp:p%3Aw%2Fd;fingerprint=ab-c_d@example.com/home/user/my%20dir/"
    );
    let no_user = Include {
        password: true,
        ..Include::default()
    };
    assert_eq!(
        url(&parts, no_user),
        "sftp://example.com/",
        "a password only with the user"
    );
}

#[test]
fn the_scheme_and_the_port_follow_the_protocol() {
    let mut parts = Parts {
        protocol: Protocol::WebDav,
        encryption: Encryption::Implicit,
        host: "fe80::1".to_string(),
        port: 8443,
        ..Parts::default()
    };
    assert_eq!(url(&parts, Include::default()), "davs://[fe80::1]:8443/");
    let http = Include {
        http_for_webdav: true,
        ..Include::default()
    };
    assert_eq!(url(&parts, http), "https://[fe80::1]:8443/");
    parts.port = 443;
    assert_eq!(url(&parts, Include::default()), "davs://[fe80::1]/");
    assert_eq!(scheme(Protocol::Ftp, Encryption::Explicit, false), "ftpes");
    assert_eq!(scheme(Protocol::Ftp, Encryption::Implicit, false), "ftps");
    assert_eq!(scheme(Protocol::S3, Encryption::None, false), "s3plain");
    assert_eq!(scheme(Protocol::Scp, Encryption::None, true), "scp");
    assert_eq!(default_port(Protocol::S3, Encryption::Implicit), 443);
    assert_eq!(default_port(Protocol::Ftp, Encryption::Explicit), 21);
}

#[test]
fn the_dialog_shows_the_url_and_reads_its_checks() {
    let parts = sftp();
    let items = items(&parts);
    assert_eq!(items[RESULT].text, "sftp://user%40corp@example.com/");
    assert!(items[RESULT].read_only);
    assert!(
        items[4].disabled && items[5].disabled,
        "NetBox's checks without effect"
    );
    let mut values: Vec<Value> = (items.iter())
        .map(|item| Value {
            text: item.text.clone(),
            state: item.state,
        })
        .collect();
    values[FOLDER].state = 1;
    let include = include_of(&values);
    assert!(include.user && include.folder && !include.password);
}
