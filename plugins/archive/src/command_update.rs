//! `arc:c` and `arc:u` as arclite's `cmdline_update` runs them (its `parse_update_command`): a new
//! archive of the format `-t` names (7-Zip's name), the profile's (`-pr`), else the one its extension
//! is of (`find_by_ext`; 7z when none), made from the profile's options or arclite's defaults with the
//! parameters over them; or the files added to an archive there with its own level, method, solid and
//! encryption (`load_update_props`) unless given. A zip is written by the `zip` crate (S6.3d), every
//! other format through 7-Zip's library; no dialog and no log.

use std::path::{Path, PathBuf};

use nc_plugin_sdk::sevenzip::Format;
use nc_plugin_sdk::{Error, ErrorKind, Which};

use crate::asking::HostAsks;
use crate::command_line::UpdateCommand;
use crate::command_run::{bad_format, not_written};
use crate::create_options::{FORMAT, ProfileOptions, Profiles, method_for};
use crate::extract_options::{Overwrite, Tri};
use crate::update::Faults;
use crate::update_options::{Kind, UpdateOptions};
use crate::update_props::LIBRARY_METHODS;
use crate::{ArchivePlugin, creating, updating};

/// The method `-m` names as 7-Zip names it (`copy`: stored): one of arclite's `c_methods`, else refused.
pub(crate) fn method_named(named: &str) -> Result<Option<String>, Error> {
    if named.eq_ignore_ascii_case("copy") {
        return Ok(None);
    }
    (LIBRARY_METHODS.iter())
        .find(|method| method.eq_ignore_ascii_case(named))
        .map(|method| Some((*method).to_string()))
        .ok_or_else(|| not_written(&format!("7-Zip's library writes no {named}")))
}

/// The new archive's format as arclite's `parse_update_command` takes it: `-t`, the profile's (`-pr`),
/// else the first of `formats` with the archive's extension (`find_by_ext`), else the profile's (7z);
/// with no library a zip's `.zip` is zip.
pub(crate) fn new_format(
    command: &UpdateCommand,
    profile: &ProfileOptions,
    archive: &Path,
    formats: &[Format],
) -> String {
    if let Some(format) = &command.format {
        return format.clone();
    }
    if command.profile.is_some() {
        return profile.format.clone();
    }
    let ext = (archive.extension()).map(|ext| ext.to_string_lossy().into_owned());
    let Some(ext) = ext else {
        return profile.format.clone();
    };
    if formats.is_empty() {
        let zip = ext.eq_ignore_ascii_case("zip");
        return if zip { FORMAT } else { "7z" }.to_string();
    }
    (formats.iter())
        .find(|format| {
            format
                .extensions
                .iter()
                .any(|known| known.eq_ignore_ascii_case(&ext))
        })
        .map_or_else(|| profile.format.clone(), |format| format.name.clone())
}

/// `-eh`'s answer as the header's encryption.
fn header_of(tri: Option<Tri>, otherwise: Option<bool>) -> Option<bool> {
    match tri {
        Some(Tri::True) => Some(true),
        Some(Tri::False) => Some(false),
        Some(Tri::Undef) => None,
        None => otherwise,
    }
}

impl ArchivePlugin {
    /// `c` (or `u` of an archive not there): the profile's options (`-pr`, else arclite's defaults) with
    /// the parameters given over them; the archive made, the cursor on it where a panel shows its folder.
    pub(crate) fn create_line(
        &mut self,
        command: &UpdateCommand,
        archive: &Path,
        files: &[PathBuf],
    ) -> Result<(), Error> {
        let mut profile = match &command.profile {
            Some(name) => {
                let profiles = Profiles::load(&self.host);
                let at = profiles
                    .by_name(name)
                    .ok_or_else(|| bad_format(&self.host))?;
                profiles.0[at].options.clone()
            }
            None => ProfileOptions::default(),
        };
        let library = self.library().clone();
        let formats = library.formats();
        let format =
            self.writable_format(&new_format(command, &profile, archive, &formats), &formats)?;
        let options = self.new_options(command, &mut profile, &format)?;
        let (src_dir, _) = crate::update::under_folder(files, None);
        let request = creating::Request {
            folder: &src_dir,
            sources: files,
            asked: crate::create_options::Asked::Menu,
            library: Some(&library),
        };
        let host = self.host;
        let mut asks = HostAsks::new(host, archive).creating();
        let mut faults = Faults {
            asks: &mut asks,
            ignore_errors: options.ignore_errors,
            log: Vec::new(),
        };
        let mut place = |temp: &Path, archive: &Path| self.swap_in(archive, temp);
        creating::written(archive, &request, &options, &mut faults, &mut place)?;
        self.show_made(archive, &[Which::Active, Which::Passive]);
        Ok(())
    }

    /// The format named (7-Zip's own name of it) when it is written: zip, or one the library writes; an
    /// unknown name arclite's syntax (`find_by_name` failing), one it does not write refused.
    fn writable_format(&self, named: &str, formats: &[Format]) -> Result<String, Error> {
        if named.eq_ignore_ascii_case(FORMAT) {
            return Ok(FORMAT.to_string());
        }
        let found = (formats.iter()).find(|format| format.name.eq_ignore_ascii_case(named));
        match found {
            None => Err(bad_format(&self.host)),
            Some(format) if !format.updatable => Err(Error::new(
                ErrorKind::Unsupported,
                updating::NOT_WRITTEN_YET.to_string(),
            )),
            Some(format) => Ok(format.name.clone()),
        }
    }

    /// A new archive's options of `format` from the profile and the parameters.
    fn new_options(
        &self,
        command: &UpdateCommand,
        profile: &mut ProfileOptions,
        format: &str,
    ) -> Result<UpdateOptions, Error> {
        // arclite's `-sfx[:module]`: 7z.sfx when none is named, the profile's other SFX options kept.
        if let Some(module) = &command.sfx {
            profile.create_sfx = true;
            profile.sfx.name = if module.is_empty() {
                crate::sfx::DEFAULT_MODULE.to_string()
            } else {
                module.clone()
            };
        }
        profile.format = format.to_string();
        let modules = crate::sfx::Modules(self.library().sfx_modules());
        let sfx = profile.sfx_of(&modules);
        if profile.create_sfx && format == "7z" && sfx.is_none() {
            return Err(not_written(crate::sfx::WRONG_MODULE));
        }
        let split = command
            .volumes
            .clone()
            .or_else(|| (profile.enable_volumes).then(|| profile.volume_size.clone()));
        if let Some(level) = command.level {
            profile.level = level;
        }
        let mut method = method_for(format, &profile.method);
        if let Some(named) = &command.method {
            match method_named(named)? {
                Some(named) => method = named,
                None => profile.level = 0,
            }
        }
        if let Some(password) = &command.password {
            profile.encrypt = true;
            profile.password = password.clone();
        }
        Ok(UpdateOptions {
            level: profile.level,
            method,
            advanced: command.advanced.clone().unwrap_or(profile.advanced.clone()),
            encrypt: profile.encrypt,
            show_password: false,
            password: if profile.encrypt {
                profile.password.clone()
            } else {
                String::new()
            },
            overwrite: Overwrite::Overwrite,
            move_files: command.move_files.unwrap_or(profile.move_files),
            ignore_errors: command.ignore_errors.unwrap_or(profile.ignore_errors),
            open_shared: true,
            kind: Kind {
                format: format.to_string(),
                solid: command.solid.unwrap_or(profile.solid),
                encrypt_header: header_of(command.encrypt_header, profile.encrypt_header),
                volume_size: split.map(|size| crate::create_dialog::parse_size(&size)),
                sfx,
            },
        })
    }

    /// `u` of an archive there: its own level, method, solid and encryption (arclite's
    /// `load_update_props`) unless given, the files added at its root, the answer for a file there
    /// `-o`'s (else asked).
    pub(crate) fn add_line(
        &mut self,
        command: &UpdateCommand,
        archive: &Path,
        files: &[PathBuf],
    ) -> Result<(), Error> {
        let opened = self.command_archive(archive, "")?;
        updating::changeable(&opened)?;
        let (level, method, encrypted, kind) = archive_props(&opened);
        let method = match &command.method {
            Some(named) => method_named(named)?,
            None => Some(method),
        };
        let level = match (&method, command.level) {
            (None, _) => 0,
            (_, Some(level)) => level,
            (_, None) => level,
        };
        let (encrypt, password) = match &command.password {
            Some(password) => (true, password.clone()),
            // The archive's own: one it was opened with (none from the command line: not encrypted).
            None => (
                encrypted && opened.password.is_some(),
                opened.password.clone().unwrap_or_default(),
            ),
        };
        let options = UpdateOptions {
            level,
            method: method.unwrap_or_else(|| method_for(&kind.format, "")),
            advanced: command.advanced.clone().unwrap_or_default(),
            encrypt,
            show_password: false,
            password,
            overwrite: command.overwrite.unwrap_or(Overwrite::Ask),
            move_files: command.move_files.unwrap_or(false),
            ignore_errors: command.ignore_errors.unwrap_or(false),
            open_shared: true,
            kind: Kind {
                solid: command.solid.unwrap_or(kind.solid),
                encrypt_header: header_of(command.encrypt_header, None),
                ..kind
            },
        };
        let request = updating::Request {
            path: "/",
            sources: files,
            folder: None,
            moving: options.move_files,
            mode: nc_plugin_sdk::GetMode::default(),
        };
        let Some(written) = updating::write_with(self.host, &opened, &request, &options, false)?
        else {
            return Ok(());
        };
        drop(opened);
        self.swap_in(archive, &written.temp)?;
        updating::after_with(self.host, archive, written, false)
    }
}

/// arclite's `load_update_props` for `arc:u`: the level, method, encryption and what the format is
/// written as.
fn archive_props(opened: &crate::opened::Opened) -> (u32, String, bool, Kind) {
    let options = updating::options_for(
        opened,
        &crate::update_options::UpdateSettings::default(),
        false,
    );
    let props = match opened.library() {
        Some((chain, _)) => {
            crate::update_props::load_update_props(chain.archive(), options.kind.is_zip())
        }
        None => crate::update_props::ArchiveProps {
            level: 7,
            method: options.method.clone(),
            solid: false,
            encrypted: false,
        },
    };
    (props.level, props.method, props.encrypted, options.kind)
}

#[cfg(test)]
#[path = "command_update_tests.rs"]
mod tests;
