use super::*;

#[test]
fn the_line_starts_with_the_transfer_type_and_tells_what_differs() {
    let defaults = CopyParams::default();
    assert_eq!(defaults.info(Attrs::default()), "Transfer type: Binary");
    let changed = CopyParams {
        transfer_mode: TransferMode::Automatic,
        name_case: NameCase::Lower,
        replace_invalid: false,
        preserve_rights: true,
        preserve_time: false,
        ignore_perm_errors: true,
        preserve_read_only: true,
        calculate_size: false,
        clear_archive: true,
        include_mask: "*.txt".to_string(),
        speed_limit: 64 * 1024,
        newer_only: true,
        ..defaults.clone()
    };
    assert_eq!(
        changed.info(Attrs::default()),
        "Transfer type: Automatic; Filename modification: Lower case; Do not replace invalid \
         characters; Set permissions: rw-r--r--, Add X to directories; Do not preserve timestamp; \
         Ignore permission errors; Preserve read-only; Do not calculate transfer size; Clear \
         'Archive' attribute; File mask: *.txt; Transfer speed limit: 64 KB/s; New and updated \
         files only"
    );
    let down = changed.info(Attrs::download());
    assert!(
        !down.contains("Set permissions") && !down.contains("Clear") && !down.contains("Ignore")
    );
    assert!(down.contains("Preserve read-only"));
    let up = changed.info(Attrs::upload(false));
    assert!(!up.contains("read-only") && up.contains("Clear 'Archive'"));
    assert!(
        !changed.info(Attrs::upload(true)).contains("Clear"),
        "a move's sources go"
    );
    let text = CopyParams {
        transfer_mode: TransferMode::Automatic,
        ascii_mask: "*.txt".to_string(),
        ..defaults
    };
    assert_eq!(
        text.info(Attrs::default()),
        "Transfer type: Automatic (*.txt)"
    );
}

#[test]
fn rights_read_as_text_and_octal_both_ways() {
    assert_eq!(rights_text(0o644), "rw-r--r--");
    assert_eq!(rights_text(0o751), "rwxr-x--x");
    assert_eq!(rights_of_text("rwxr-x--x"), Some(0o751));
    assert_eq!(rights_of_text("rwz------"), None);
    assert_eq!(octal(0o644), "0644");
    assert_eq!(rights_of_octal("755"), Ok(0o755));
    assert_eq!(rights_of_octal("0600"), Ok(0o600));
    assert_eq!(
        rights_of_octal("99"),
        Err("'99' is not valid permission in octal format.".to_string())
    );
    assert!(rights_of_octal("0789").is_err());
}

#[test]
fn a_folders_rights_get_x_where_they_read() {
    let params = CopyParams {
        rights: 0o640,
        ..CopyParams::default()
    };
    assert_eq!(params.remote_rights(false), 0o640);
    assert_eq!(params.remote_rights(true), 0o750);
    let without = CopyParams {
        add_x_to_dirs: false,
        ..params
    };
    assert_eq!(without.remote_rights(true), 0o640);
}

#[test]
fn speeds_read_as_kb_or_unlimited() {
    assert_eq!(speed_text(0), "Unlimited");
    assert_eq!(speed_text(256 * 1024), "256");
    assert_eq!(speed_of_text("unlimited"), Ok(0));
    assert_eq!(speed_of_text(" 128 "), Ok(128 * 1024));
    assert_eq!(
        speed_of_text("fast"),
        Err("'fast' is not a valid speed limit.".to_string())
    );
}

#[test]
fn names_change_by_case_and_go_down_and_up_with_tokens() {
    let mut params = CopyParams::default();
    assert_eq!(params.change_file_name("a:b.txt", true, false), "a%3Ab.txt");
    assert_eq!(
        params.change_file_name("a%3Ab.txt", false, false),
        "a:b.txt"
    );
    params.file_mask = "*.bak".to_string();
    assert_eq!(
        params.change_file_name("notes.txt", true, true),
        "notes.bak"
    );
    assert_eq!(
        params.change_file_name("notes.txt", true, false),
        "notes.txt"
    );
    params.file_mask.clear();
    params.name_case = NameCase::Upper;
    assert_eq!(params.change_file_name("Read.me", true, false), "READ.ME");
    params.name_case = NameCase::FirstUpper;
    assert_eq!(params.change_file_name("rEAD.ME", true, false), "Read.me");
    params.name_case = NameCase::LowerShort;
    assert_eq!(
        params.change_file_name("README.TXT", true, false),
        "readme.txt"
    );
    assert_eq!(
        params.change_file_name("LONGNAME1.TXT", true, false),
        "LONGNAME1.TXT"
    );
    assert_eq!(
        params.change_file_name("Mixed.TXT", true, false),
        "Mixed.TXT"
    );
    params.name_case = NameCase::NoChange;
    params.replace_invalid = false;
    assert_eq!(params.change_file_name("a*b", true, false), "a*b");
    // A drive's or a folder's separator is replaced whatever the setting (ADR-0023's review addendum).
    assert_eq!(params.change_file_name("a:b", true, false), "a%3Ab");
    assert_eq!(params.change_file_name("a:b", false, false), "a:b");
}

#[test]
fn the_text_mode_is_chosen_by_the_mode_and_the_masks() {
    let any = Params::default();
    let mut params = CopyParams::default();
    assert!(!params.use_text("a.txt", false, any));
    params.transfer_mode = TransferMode::Text;
    assert!(params.use_text("a.bin", false, any));
    params.transfer_mode = TransferMode::Automatic;
    assert!(params.use_text("index.HTML", false, any));
    assert!(params.use_text(".htaccess", true, any));
    assert!(!params.use_text("a.zip", false, any));
}
