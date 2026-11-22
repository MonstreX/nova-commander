use std::path::PathBuf;

use nc_plugin_sdk::{ItemKind, Value};

use super::*;
use crate::create_options::{Profile, ProfileOptions};

fn modules() -> Modules {
    Modules(vec![
        PathBuf::from(r"C:\7\7z.sfx"),
        PathBuf::from(r"C:\7\7zSD.sfx"),
    ])
}

fn shown() -> SfxDialog {
    let sfx = |name: &str, title: &str| SfxOptions {
        name: name.to_string(),
        append_install_config: !title.is_empty(),
        install_config: InstallConfig {
            title: title.to_string(),
            ..InstallConfig::default()
        },
        ..SfxOptions::default()
    };
    let profile = |name: &str, create_sfx: bool, options: SfxOptions| Profile {
        name: name.to_string(),
        options: ProfileOptions {
            create_sfx,
            sfx: options,
            ..ProfileOptions::default()
        },
    };
    let profiles = Profiles(vec![
        profile("gui", true, sfx("7z.sfx", "")),
        profile("plain", false, sfx("7z.sfx", "")),
        profile("setup", true, sfx("7zSD.sfx", "Setup")),
    ]);
    SfxDialog::new(&profiles, &modules())
}

/// The items' values as a dialog closed with `changed` holds them.
fn values(dialog: &Dialog, changed: &[(usize, i32, &str)]) -> Vec<Value> {
    let mut values: Vec<Value> = (dialog.items.iter())
        .map(|item| Value {
            state: item.state,
            text: item.text.clone(),
        })
        .collect();
    for &(item, state, text) in changed {
        values[item].state = state;
        values[item].text = text.to_string();
    }
    values
}

fn closed(dialog: &Dialog, button: usize, changed: &[(usize, i32, &str)]) -> Closed {
    Closed {
        button: Some(button),
        values: values(dialog, changed),
    }
}

/// The dialog of the options naming 7z.sfx.
fn gui_dialog() -> Dialog {
    shown().dialog(&SfxOptions {
        name: "7z.SFX".to_string(),
        ..SfxOptions::default()
    })
}

/// arclite's rows in their words and order: only the profiles making an SFX offered (no profile
/// chosen), the modules by their descriptions, the module of the options chosen.
#[test]
fn the_dialog_is_arclites() {
    let dialog = gui_dialog();
    assert_eq!(dialog.title, TITLE);
    let text = |item: usize| dialog.items[item].text.as_str();
    assert_eq!(text(PROFILE - 1), "Apply &profile:");
    assert_eq!(dialog.items[PROFILE].lines, ["gui", "setup", ""]);
    assert_eq!(dialog.items[PROFILE].state, 2);
    assert_eq!(text(MODULE - 1), "&Module name:");
    assert_eq!(
        dialog.items[MODULE].lines,
        ["Standard GUI", "Installer GUI", ""]
    );
    assert_eq!(dialog.items[MODULE].state, 0);
}

/// The icon's, version's and install configuration's rows and the buttons in arclite's words.
#[test]
fn the_dialogs_rows_are_arclites() {
    let dialog = gui_dialog();
    let text = |item: usize| dialog.items[item].text.as_str();
    assert_eq!(text(REPLACE_ICON), "Replace &icon");
    assert_eq!(text(ICON_PATH - 1), "Icon pat&h:");
    assert_eq!(dialog.items[ICON_PATH].history, "arclite.icon_path");
    assert_eq!(text(REPLACE_VERSION), "Replace &version");
    assert_eq!(text(VERSION_EDITS[0] - 1), "Product &name:");
    assert_eq!(text(VERSION_EDITS[5] - 1), "&Legal copyright:");
    assert_eq!(text(APPEND_CONFIG), "In&stall configuration");
    assert_eq!(text(CONFIG_ITEMS[6] - 1), "Execute p&arameters:");
    assert_eq!([text(OK), text(CANCEL)], ["OK", "Cancel"]);
}

/// The progress meter a check box of three states, its third first; the items disabled with their check
/// boxes off and a module reading no configuration; the edits in one column.
#[test]
fn the_dialogs_items_start_as_arclites() {
    let dialog = gui_dialog();
    let text = |item: usize| dialog.items[item].text.as_str();
    assert_eq!(text(CONFIG_ITEMS[2] - 1), "Pro&gress meter:");
    assert_eq!(dialog.items[CONFIG_ITEMS[2]].kind, ItemKind::Check);
    assert!(dialog.items[CONFIG_ITEMS[2]].three_state);
    assert_eq!(dialog.items[CONFIG_ITEMS[2]].state, 2, "neither yes nor no");
    for item in [ICON_PATH, VERSION_EDITS[3], APPEND_CONFIG, CONFIG_ITEMS[0]] {
        assert!(dialog.items[item].disabled, "{item}");
    }
    assert!(!dialog.items[REPLACE_VERSION].disabled);
    assert_eq!(
        dialog.items[VERSION_EDITS[0]].place.0, dialog.items[VERSION_EDITS[3]].place.0,
        "the edits in one column"
    );
}

/// The items follow their check boxes and the module: an installer's configuration enabled, its items
/// with "Install configuration".
#[test]
fn the_items_follow_their_check_boxes() {
    let shown = shown();
    let state = |item: usize| match item {
        REPLACE_ICON | REPLACE_VERSION | APPEND_CONFIG | MODULE => 1,
        _ => 0,
    };
    let enabled: std::collections::HashMap<usize, bool> =
        shown.control_state(&state).into_iter().collect();
    assert!(enabled[&ICON_PATH] && enabled[&VERSION_EDITS[2]]);
    assert!(enabled[&APPEND_CONFIG] && enabled[&CONFIG_ITEMS[6]]);
    let no_module = |item: usize| if item == MODULE { 2 } else { state(item) };
    let enabled: std::collections::HashMap<usize, bool> =
        shown.control_state(&no_module).into_iter().collect();
    assert!(!enabled[&APPEND_CONFIG] && !enabled[&CONFIG_ITEMS[0]]);
}

/// The items `control_state` names with each one's state, by item.
fn enabled_by(
    shown: &SfxDialog,
    on: &[usize],
    module: i32,
) -> std::collections::BTreeMap<usize, bool> {
    let state = |item: usize| {
        if item == MODULE {
            module
        } else {
            i32::from(on.contains(&item))
        }
    };
    shown.control_state(&state).into_iter().collect()
}

/// Each check box off disables its own items only: the icon's label and path, every version row's label
/// and edit, the configuration's rows (its check box still enabled for an installer's module).
#[test]
fn each_check_box_disables_its_own_items() {
    let shown = shown();
    let all = enabled_by(&shown, &[], 1);
    assert_eq!(
        all.keys().copied().collect::<Vec<_>>(),
        (ICON_PATH - 1..=CONFIG_ITEMS[6])
            .filter(|item| *item != REPLACE_VERSION)
            .collect::<Vec<_>>(),
        "every item after the check boxes"
    );
    assert!(!all[&(ICON_PATH - 1)] && !all[&ICON_PATH]);
    assert!(!all[&(VERSION_EDITS[0] - 1)] && !all[&VERSION_EDITS[5]]);
    assert!(all[&APPEND_CONFIG], "an installer's module");
    assert!(!all[&(CONFIG_ITEMS[0] - 1)] && !all[&CONFIG_ITEMS[6]]);
    let version = enabled_by(&shown, &[REPLACE_VERSION], 1);
    assert!(version[&(VERSION_EDITS[0] - 1)] && version[&VERSION_EDITS[5]]);
    assert!(!version[&ICON_PATH] && !version[&CONFIG_ITEMS[0]]);
    let icon = enabled_by(&shown, &[REPLACE_ICON], 0);
    assert!(icon[&(ICON_PATH - 1)] && icon[&ICON_PATH]);
    assert!(!icon[&VERSION_EDITS[0]] && !icon[&APPEND_CONFIG]);
    let config = enabled_by(&shown, &[APPEND_CONFIG], 0);
    assert!(
        !config[&APPEND_CONFIG] && !config[&CONFIG_ITEMS[2]],
        "a GUI module"
    );
    let negative = enabled_by(&shown, &[APPEND_CONFIG], -1);
    assert!(!negative[&APPEND_CONFIG], "no module");
}

/// The version's six lines into their fields; the progress meter's states as `no`, `yes` and nothing;
/// the configuration's texts into their fields.
#[test]
fn the_version_and_configuration_are_read_into_their_fields() {
    let shown = shown();
    let dialog = shown.dialog(&SfxOptions::default());
    let mut changed: Vec<(usize, i32, &str)> = vec![
        (MODULE, 1, ""),
        (REPLACE_VERSION, 1, ""),
        (APPEND_CONFIG, 1, ""),
    ];
    let versions = ["pn", "1.2", "co", "fd", "cm", "lc"];
    changed.extend(
        VERSION_EDITS
            .iter()
            .zip(versions)
            .map(|(item, text)| (*item, 0, text)),
    );
    let configs = ["t", "bp", "", "rp", "d", "ef", "ep"];
    changed.extend(
        CONFIG_ITEMS
            .iter()
            .zip(configs)
            .map(|(item, text)| (*item, 0, text)),
    );
    let read_with = |progress: i32| {
        let mut all = changed.clone();
        all.push((CONFIG_ITEMS[2], progress, ""));
        let values = values(&dialog, &all);
        shown.read(&|item| (values[item].state, values[item].text.clone()))
    };
    let read = read_with(0).unwrap();
    let expected = VersionInfo {
        version: "1.2".to_string(),
        comments: "cm".to_string(),
        company_name: "co".to_string(),
        file_description: "fd".to_string(),
        legal_copyright: "lc".to_string(),
        product_name: "pn".to_string(),
    };
    assert_eq!(read.ver_info, expected);
    let config = InstallConfig {
        title: "t".to_string(),
        begin_prompt: "bp".to_string(),
        progress: "no".to_string(),
        run_program: "rp".to_string(),
        directory: "d".to_string(),
        execute_file: "ef".to_string(),
        execute_parameters: "ep".to_string(),
    };
    assert_eq!(read.install_config, config);
    assert_eq!(read_with(1).unwrap().install_config.progress, "yes");
    assert_eq!(read_with(2).unwrap().install_config.progress, "");
    assert!(
        !read.replace_icon && read.icon_path.is_empty(),
        "the icon's path not read"
    );
}

/// The dialog of options naming a module, their check boxes and texts in the items.
#[test]
fn the_dialog_shows_the_options() {
    let options = SfxOptions {
        name: "7zSD.sfx".to_string(),
        replace_icon: true,
        icon_path: r"C:\a.ico".to_string(),
        replace_version: true,
        ver_info: VersionInfo {
            product_name: "pn".to_string(),
            legal_copyright: "lc".to_string(),
            ..VersionInfo::default()
        },
        append_install_config: true,
        install_config: InstallConfig {
            title: "t".to_string(),
            progress: "no".to_string(),
            execute_parameters: "ep".to_string(),
            ..InstallConfig::default()
        },
    };
    let dialog = shown().dialog(&options);
    let item = |at: usize| &dialog.items[at];
    assert_eq!(item(MODULE).state, 1);
    assert_eq!(item(REPLACE_ICON).state, 1);
    assert_eq!(item(ICON_PATH).text, r"C:\a.ico");
    assert!(!item(ICON_PATH).disabled);
    assert_eq!(item(VERSION_EDITS[0]).text, "pn");
    assert_eq!(item(VERSION_EDITS[5]).text, "lc");
    assert_eq!(item(CONFIG_ITEMS[0]).text, "t");
    assert_eq!(item(CONFIG_ITEMS[2]).state, 0, "no");
    assert_eq!(item(CONFIG_ITEMS[6]).text, "ep");
    assert!(!item(CONFIG_ITEMS[6]).disabled);
    assert_eq!(dialog.items[PROFILE].state, 2, "no profile matches");
}

/// OK: the options read as arclite's `read_controls` reads them, what is not replaced left empty; no
/// module, or an icon's path that is not there: refused with arclite's words.
#[test]
fn ok_reads_the_options() {
    let shown = shown();
    let dialog = shown.dialog(&SfxOptions::default());
    let ico = tempfile::NamedTempFile::new().unwrap();
    let icon = ico.path().display().to_string();
    let changed = [
        (MODULE, 1, ""),
        (REPLACE_ICON, 1, ""),
        (ICON_PATH, 0, icon.as_str()),
        (VERSION_EDITS[1], 0, "1.0"),
        (APPEND_CONFIG, 1, ""),
        (CONFIG_ITEMS[0], 0, "Setup"),
        (CONFIG_ITEMS[2], 1, ""),
        (CONFIG_ITEMS[5], 0, "setup.exe"),
    ];
    let read = shown.answers(&closed(&dialog, OK, &changed)).unwrap();
    assert_eq!(read.name, "7zSD.sfx");
    assert!(read.replace_icon && read.icon_path == icon);
    assert!(!read.replace_version && read.ver_info.version.is_empty());
    assert!(read.append_install_config);
    assert_eq!(read.install_config.title, "Setup");
    assert_eq!(read.install_config.progress, "yes");
    assert_eq!(read.install_config.execute_file, "setup.exe");
    let values = values(&dialog, &changed);
    let value = |item: usize| (values[item].state, values[item].text.clone());
    assert_eq!(shown.read(&value), Ok(read));
    let no_module = |item: usize| {
        if item == MODULE {
            (2, String::new())
        } else {
            value(item)
        }
    };
    assert_eq!(shown.read(&no_module), Err(WRONG_MODULE));
    let missing = |item: usize| {
        if item == ICON_PATH {
            (0, r"C:\none\none.ico".to_string())
        } else {
            value(item)
        }
    };
    assert_eq!(shown.read(&missing), Err("Icon path is invalid"));
    let gui = |item: usize| {
        if item == MODULE {
            (0, String::new())
        } else {
            value(item)
        }
    };
    let read = shown.read(&gui).unwrap();
    assert!(!read.append_install_config, "the module reads none");
    assert_eq!(read.install_config, InstallConfig::default());
    assert!(shown.answers(&closed(&dialog, CANCEL, &[])).is_none());
}
