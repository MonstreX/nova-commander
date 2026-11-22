use super::*;
use nc_plugin_sdk::ItemKind;

/// The values the items give unchanged, from the first tab's first item on.
fn values_of(items: &[DialogItem]) -> Vec<Value> {
    let mut values = vec![Value::default(); FIRST[ENVIRONMENT]];
    values.extend(items.iter().map(|item| Value {
        text: item.text.clone(),
        state: item.state,
    }));
    values
}

fn changed() -> Options {
    Options {
        eol_crlf: true,
        time_difference: -90,
        dst_mode: DstMode::Keep,
        delete_to_recycle_bin: true,
        recycle_bin_path: "/home/u/.trash".to_string(),
        update_directories: false,
        remote_directory: "/var/www".to_string(),
        resolve_symlinks: false,
        sftp_server: "/usr/lib/sftp-server".to_string(),
        shell: "/bin/bash".to_string(),
        listing_command: "ls -gla".to_string(),
        ignore_ls_warnings: false,
        ls_full_time: false,
        unset_national_vars: false,
        ..Options::default()
    }
}

#[test]
fn the_items_are_where_the_dialog_reads_them() {
    let items = items(&Options::default());
    assert_eq!(items.len(), BUTTONS - FIRST[1]);
    let item = |index: usize| &items[index - FIRST[ENVIRONMENT]];
    assert_eq!(item(EOL).kind, ItemKind::List);
    assert_eq!(item(DST_UNIX).kind, ItemKind::Radio);
    assert_eq!(item(UNIX).text, "Uni&x");
    assert_eq!(item(WINDOWS).kind, ItemKind::Button);
    assert_eq!(item(BIN_PATH).kind, ItemKind::Edit);
    assert_eq!(item(REMOTE_DIRECTORY).kind, ItemKind::Edit);
    assert_eq!(item(SFTP_SERVER).text, "Default");
    assert_eq!(item(SHELL).text, "Default");
    assert_eq!(item(LISTING).text, "ls -la");
    assert_eq!(item(NATIONAL).kind, ItemKind::Check);
    assert!(items.iter().all(|item| item.hidden));
}

#[test]
fn the_options_come_back_as_they_were_shown() {
    for options in [Options::default(), changed()] {
        let values = values_of(&items(&options));
        assert_eq!(options_of(&values, &Options::default()), options);
    }
}

#[test]
fn items_belong_to_their_tabs() {
    assert_eq!(tab_of(0), None);
    assert_eq!(tab_of(1), Some(0));
    assert_eq!(tab_of(EOL), Some(ENVIRONMENT));
    assert_eq!(tab_of(RESOLVE_SYMLINKS), Some(DIRECTORIES));
    assert_eq!(tab_of(SFTP_SERVER), Some(SFTP));
    assert_eq!(tab_of(NATIONAL), Some(SCP));
    assert_eq!(tab_of(FIRST[5]), Some(CONNECTION));
    assert_eq!(tab_of(FIRST[FTP]), Some(FTP));
    assert_eq!(tab_of(FIRST[TLS]), Some(TLS));
    assert_eq!(tab_of(FIRST[WEBDAV]), Some(WEBDAV));
    assert_eq!(tab_of(BUTTONS - 1), Some(S3));
    assert_eq!(tab_of(BUTTONS), None);
}

#[test]
fn the_tabs_follow_the_protocol() {
    assert_eq!(
        there(Protocol::Sftp, false),
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    );
    assert_eq!(there(Protocol::Scp, false), [0, 1, 2, 4, 5, 6, 7, 8, 9, 10]);
    // NetBox's order: FTP(S) before Connection, TLS/SSL after Proxy (for FTPS, HTTPS and S3).
    assert_eq!(there(Protocol::Ftp, false), [0, 1, 2, FTP, 5, 6]);
    assert_eq!(there(Protocol::Ftp, true), [0, 1, 2, FTP, 5, 6, TLS]);
    // WebDAV last, S3 before Connection (S6.6f).
    assert_eq!(there(Protocol::WebDav, false), [0, 1, 2, 5, 6, WEBDAV]);
    assert_eq!(there(Protocol::WebDav, true), [0, 1, 2, 5, 6, TLS, WEBDAV]);
    assert_eq!(there(Protocol::S3, false), [0, 1, 2, S3, 5, 6, TLS]);
}
