use super::*;
use std::collections::HashMap;

/// The settings as kept, read back the same.
#[test]
fn the_transfer_settings_go_out_and_come_back() {
    let params = CopyParams {
        transfer_mode: TransferMode::Automatic,
        ascii_mask: "*.txt".to_string(),
        name_case: NameCase::LowerShort,
        replace_invalid: false,
        preserve_rights: true,
        rights: 0o750,
        add_x_to_dirs: false,
        ignore_perm_errors: true,
        preserve_read_only: true,
        clear_archive: true,
        calculate_size: false,
        include_mask: "| *.bak".to_string(),
        speed_limit: 8 * 1024,
        temp_path: "C:\\Temp".to_string(),
        newer_only: true,
        queue: true,
        preserve_time: false,
        resume: Resume::Off,
        resume_threshold: 5,
        file_mask: String::new(),
    };
    let kept: HashMap<&str, String> = copy_param_entries(&params).into_iter().collect();
    assert_eq!(kept["Text"], "rwxr-x---");
    assert_eq!(kept["TransferMode"], "2");
    assert_eq!(kept["FileNameCase"], "4");
    assert_eq!(kept["ReplaceInvalidChars"], "0");
    assert_eq!(kept["CPSLimit"], "8192");
    assert_eq!(kept["Queue"], "1", "WinSCP's Queue");
    let back = copy_params_from(&|name| kept.get(name).cloned());
    assert_eq!(back, params);
}

/// Nothing kept, or what cannot be read: WinSCP's defaults.
#[test]
fn what_is_not_kept_is_the_default() {
    assert_eq!(copy_params_from(&|_| None), CopyParams::default());
    let wrong: HashMap<&str, &str> = [
        ("TransferMode", "7"),
        ("FileNameCase", "x"),
        ("Text", "rwx"),
        ("PreserveTime", "maybe"),
    ]
    .into_iter()
    .collect();
    let read = copy_params_from(&|name| wrong.get(name).map(ToString::to_string));
    assert_eq!(read, CopyParams::default());
}

#[test]
fn netbox_presets_and_editor_defaults() {
    let names: Vec<&str> = presets().iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        [
            "Default",
            "Text files (ASCII)",
            "Binary all",
            "No preserve time"
        ]
    );
    let presets = presets();
    assert_eq!(presets[1].1.transfer_mode, TransferMode::Text);
    assert!(presets[2].1.preserve_rights);
    assert!(!presets[3].1.preserve_time);
    let editor = EditorOptions::default();
    assert!(editor.download_default && editor.upload_same && editor.upload_on_save);
}
