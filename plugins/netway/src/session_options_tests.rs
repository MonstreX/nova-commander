use super::*;

fn lines(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    (pairs.iter())
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn the_defaults_are_winscps_and_write_nothing() {
    let options = Options::default();
    assert!(options.update_directories && options.resolve_symlinks);
    assert!(options.ignore_ls_warnings && options.ls_full_time && options.unset_national_vars);
    assert_eq!(options.listing_command, "ls -la");
    assert_eq!(options.dst_mode, DstMode::Unix);
    assert!(options.lines().is_empty());
}

#[test]
fn winscps_values_are_read_and_written_back() {
    let read = Options::read(&lines(&[
        ("EOLType", "1"),
        ("TimeDifference", "-0.125"),
        ("ConsiderDST", "2"),
        ("DeleteToRecycleBin", "1"),
        ("RecycleBinPath", "/home/u/.trash"),
        ("UpdateDirectories", "0"),
        ("RemoteDirectory", "/var/www"),
        ("ResolveSymlinks", "0"),
        ("SftpServer", "sudo su -c /bin/sftp-server"),
        ("Shell", "/bin/bash"),
        ("ListingCommand", "ls -gla"),
        ("IgnoreLsWarnings", "0"),
        ("SCPLsFullTime", "1"),
        ("UnsetNationalVars", "0"),
        ("Unknown", "kept"),
    ]));
    assert!(read.eol_crlf);
    assert_eq!(read.time_difference, -180);
    assert_eq!(read.dst_mode, DstMode::Keep);
    assert!(read.delete_to_recycle_bin && !read.overwritten_to_recycle_bin);
    assert_eq!(read.remote_directory, "/var/www");
    assert!(!read.update_directories && !read.resolve_symlinks);
    assert_eq!(read.shell, "/bin/bash");
    assert!(!read.ignore_ls_warnings && !read.ls_full_time && !read.unset_national_vars);
    assert_eq!(Options::read(&read.lines()), read);
}

#[test]
fn merging_keeps_the_other_keys_and_drops_defaults() {
    let other = lines(&[("Unknown", "kept"), ("Shell", "/bin/ksh"), ("EOLType", "1")]);
    let options = Options {
        remote_directory: "/srv".to_string(),
        ..Options::default()
    };
    assert_eq!(
        options.merged(&other),
        lines(&[("Unknown", "kept"), ("RemoteDirectory", "/srv")])
    );
}

#[test]
fn the_presets_set_the_ends_of_lines_and_dst() {
    let mut options = Options::default();
    options.windows();
    assert_eq!((options.eol_crlf, options.dst_mode), (true, DstMode::Win));
    options.unix();
    assert_eq!((options.eol_crlf, options.dst_mode), (false, DstMode::Unix));
}
