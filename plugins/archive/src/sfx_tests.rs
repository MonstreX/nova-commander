//! The SFX options kept by name, the modules arclite knows, the install configuration's text.

use std::cell::RefCell;
use std::collections::HashMap;

use super::*;

fn every_value() -> SfxOptions {
    SfxOptions {
        name: "7zS.sfx".to_string(),
        replace_icon: true,
        icon_path: r"C:\a.ico".to_string(),
        replace_version: true,
        ver_info: VersionInfo {
            version: "1.2.3.4".to_string(),
            comments: "c".to_string(),
            company_name: "co".to_string(),
            file_description: "fd".to_string(),
            legal_copyright: "lc".to_string(),
            product_name: "pn".to_string(),
        },
        append_install_config: true,
        install_config: InstallConfig {
            title: "t".to_string(),
            begin_prompt: "bp".to_string(),
            progress: "no".to_string(),
            run_program: "rp".to_string(),
            directory: "d".to_string(),
            execute_file: "ef".to_string(),
            execute_parameters: "ep".to_string(),
        },
    }
}

/// Every value written under its arclite name and read back the same; none kept: the defaults.
#[test]
fn options_are_kept_under_arclites_names() {
    let texts = RefCell::new(HashMap::new());
    let flags = RefCell::new(HashMap::new());
    let options = every_value();
    options.write(
        &mut |name, text| {
            texts
                .borrow_mut()
                .insert(name.to_string(), text.to_string());
        },
        &mut |name, value| {
            flags.borrow_mut().insert(name.to_string(), value);
        },
    );
    assert_eq!(texts.borrow().len(), 15);
    assert_eq!(texts.borrow()["ver_info.company_name"], "co");
    assert_eq!(texts.borrow()["install_config.execute_parameters"], "ep");
    assert_eq!(flags.borrow().len(), 3);
    assert!(flags.borrow()["append_install_config"]);
    let read = SfxOptions::read(&|name| texts.borrow().get(name).cloned(), &|name| {
        flags.borrow().get(name).copied().unwrap_or(false)
    });
    assert_eq!(read, options);
    assert_eq!(
        SfxOptions::read(&|_| None, &|_| false),
        SfxOptions::default()
    );
}

/// A known module by its words and abilities, whatever its name's case; another "Unknown [name]" with
/// every ability; a name looked for whatever its case.
#[test]
fn modules_are_known_by_name() {
    let modules = Modules(vec![
        PathBuf::from(r"C:\7\7z.sfx"),
        PathBuf::from(r"C:\7\7ZSD.SFX"),
        PathBuf::from(r"C:\7\7zS2con.sfx"),
        PathBuf::from(r"C:\7\mine.sfx"),
    ]);
    assert_eq!(modules.description(0), "Standard GUI");
    assert!(modules.all_codecs(0) && !modules.install_config(0));
    assert_eq!(modules.description(1), "Installer GUI");
    assert!(!modules.all_codecs(1) && modules.install_config(1));
    assert_eq!(modules.description(2), "Small installer console");
    assert!(!modules.all_codecs(2) && !modules.install_config(2));
    assert_eq!(modules.description(3), "Unknown [mine.sfx]");
    assert!(modules.all_codecs(3) && modules.install_config(3));
    assert_eq!(modules.by_name("7zsd.sfx"), Some(1));
    assert_eq!(modules.by_name("7zCon.sfx"), None);
}

/// The configuration's lines given, in arclite's order and quotes, between its markers.
#[test]
fn the_install_config_is_arclites() {
    assert_eq!(
        install_config_text(&InstallConfig::default()),
        b";!@Install@!UTF-8!\n;!@InstallEnd@!\n"
    );
    let config = InstallConfig {
        title: "Setup".to_string(),
        progress: "yes".to_string(),
        execute_file: "setup.exe".to_string(),
        ..InstallConfig::default()
    };
    assert_eq!(
        String::from_utf8(install_config_text(&config)).unwrap(),
        ";!@Install@!UTF-8!\nTitle=\"Setup\"\nProgress=\"yes\"\nExecuteFile=\"setup.exe\"\n;!@InstallEnd@!\n"
    );
}
