//! The Plugins menu's "Convert archive into SFX" (arclite's `convert_to_sfx`): the "SFX Options" dialog
//! from the kept options, then each selected file of the active panel made self-extracting in turn; Esc
//! or a failure (told in arclite's words) ends it; the panel read again.

use std::path::{Path, PathBuf};

use nc_plugin_sdk::{Host, Which};

use crate::asking::{HostAsks, show_fault};
use crate::create_options::{NewSettings, Profiles};
use crate::sfx::Modules;
use crate::sfx_module::{Unconverted, attach};

/// arclite's `convert_to_sfx` of `files` with the SFX modules `modules`.
pub fn convert_all(host: Host, files: &[PathBuf], modules: &Modules) {
    let kept = NewSettings::load(&host).sfx;
    let profiles = Profiles::load(&host);
    let Some(options) = crate::sfx_dialog::ask(&host, &kept, &profiles, modules) else {
        return;
    };
    let Some(module) = modules
        .by_name(&options.name)
        .map(|at| modules.0[at].clone())
    else {
        return;
    };
    for file in files {
        if !convert_one(host, file, &module, &options) {
            break;
        }
    }
    host.reread(Which::Active, false);
}

/// One file converted; false when it stopped or failed (its failure shown).
fn convert_one(host: Host, file: &Path, module: &Path, options: &crate::sfx::SfxOptions) -> bool {
    let mut asks = HostAsks::new(host, file);
    match attach(file, module, options, &mut asks) {
        Ok(_) => true,
        Err(Unconverted::Stopped) => false,
        Err(Unconverted::Failed(fault)) => {
            show_fault(&host, &fault);
            false
        }
    }
}
