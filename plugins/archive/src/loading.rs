//! A file read as an archive (arclite's `Archive::open` and `Plugin::open`): through 7-Zip's library
//! (ADR-0022), every format tried in arclite's order and the first that opens taken (the format menu
//! lists every way and asks which); nothing opens when no library is found (arclite's).

use std::path::Path;

use nc_plugin_sdk::sevenzip::{Format, Library};
use nc_plugin_sdk::{Error, ErrorKind, KeyAnswer, KeyGo, Menu, MenuItem, OpenHow};

use crate::library_open::{self, Chain};
use crate::open_asking::OpenAsks;
use crate::opened::Opened;
use crate::opening;
use crate::options::TITLE;
use crate::{ArchivePlugin, io_error};

impl ArchivePlugin {
    /// 7-Zip's library, found the first time it is needed, the configuration's code pages given to it
    /// (arclite's `ArcAPI::get` with `Patch7zCP::SetCP`).
    pub(crate) fn library(&self) -> &Library {
        self.library.get_or_init(|| {
            let library = Library::find(&self.host.folder());
            library.set_code_pages(self.options.oem_cp, self.options.ansi_cp);
            library
        })
    }

    /// `path` read as an archive for what is done to its files (the menu's Extract / Test, `arc:`'s
    /// commands, an archive written anew read again): the first way it opens; none when it is no
    /// archive. One 7-Zip's library reads is not written until S6.3c (`updating::changeable`).
    pub(crate) fn load(&self, path: &Path) -> Result<Option<Opened>, Error> {
        self.load_with(path, &How::default())
    }

    /// `path` read as an archive as `how` says; none when it is no archive. A stop, a password refused,
    /// Esc in the menu: `ErrorKind::Cancelled`.
    pub(crate) fn load_with(&self, path: &Path, how: &How) -> Result<Option<Opened>, Error> {
        if std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir()) {
            return Ok(None);
        }
        let library = self.library();
        let formats = library.formats();
        if formats.is_empty() {
            return Ok(None);
        }
        let enabled: Vec<usize> =
            (how.tried.clone()).unwrap_or_else(|| (0..formats.len()).collect());
        let request = library_open::Request {
            library,
            formats: &formats,
            enabled: &enabled,
            every: how.every,
        };
        // arclite's loop (plugin.cpp `open`): a password typed that opened nothing is asked again,
        // until one opens it or Cancel.
        let (chains, asker) = loop {
            let mut asker = OpenAsks::new(self.host, path, how.password.clone());
            asker.silent = how.silent;
            let chains = library_open::open(&request, path, &mut asker).map_err(|error| {
                if error.kind() == std::io::ErrorKind::Interrupted {
                    Error::new(ErrorKind::Cancelled, "")
                } else {
                    io_error(&error)
                }
            })?;
            if !chains.is_empty() || !asker.typed() {
                break (chains, asker);
            }
        };
        let chosen = match how.silent {
            true => chains.into_iter().next(),
            false => self.chosen(chains, &formats)?,
        };
        let Some(chain) = chosen else {
            return Ok(None);
        };
        let name = chain.name(&formats);
        let file_name =
            (path.file_name()).map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let format = chain.format().and_then(|format| formats.get(format));
        let default_name = crate::library_items::default_name(&file_name, format);
        let mode = format.and_then(crate::partition::part_mode);
        // arclite's `updatable`: one archive of the file's own level, of a format the library writes.
        let writes = (chain.parts.len() == 1)
            .then_some(format)
            .flatten()
            .filter(|format| format.updatable)
            .map(|format| format.name.clone());
        let opened = Opened::from_library(path, chain, (name, &default_name), asker.password());
        let mut opened = opened
            .map_err(|error| io_error(&error))?
            .with_partitions(mode);
        opened.writes = writes;
        Ok(Some(opened))
    }

    /// arclite's `Plugin::open`: the one way, or the format menu's choice among several.
    fn chosen(&self, mut chains: Vec<Chain>, formats: &[Format]) -> Result<Option<Chain>, Error> {
        if chains.len() < 2 {
            return Ok(chains.pop());
        }
        let items = (chains.iter())
            .map(|chain| MenuItem::new(&chain.name(formats)))
            .collect();
        let chosen = self.host.menu(&Menu::new(TITLE, items));
        match chosen.and_then(|chosen| chosen.item) {
            Some(index) if index < chains.len() => Ok(Some(chains.swap_remove(index))),
            _ => Err(Error::new(ErrorKind::Cancelled, "")),
        }
    }

    /// arclite's `analyse_open` for a file offered (`OpenFilePlugin`): Shift+F2, Shift+F3 and Alt+F7's
    /// search (`OFP_EXTRACT`, `OFP_COMMANDS`, `OFP_SEARCH`) check the masks as Enter does; only Ctrl+PgDn
    /// (`OPM_PGDN`) may pass them by. The search's opens it without any window (`OPM_FIND`): no
    /// progress, no password (encrypted headers: not opened), the first way of several, a stop without
    /// a question.
    pub(crate) fn open_offered(&mut self, path: &Path, how: OpenHow) -> KeyAnswer {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let host = self.host;
        let pgdn = how == OpenHow::PgDn;
        let tried = name.is_some_and(|name| {
            opening::takes(&self.options, &name, pgdn, |masks, name| {
                host.match_masks(masks, name)
            })
        });
        if !tried {
            return KeyAnswer::pass();
        }
        // arclite's enabled / disabled formats; none left: not the plugin's (its `E_INVALIDARG`).
        let formats = self.library().formats();
        let tried = self.options.tried_formats(&formats, pgdn);
        if tried.as_ref().is_some_and(Vec::is_empty) {
            return KeyAnswer::pass();
        }
        let silent = how == OpenHow::Search;
        let how = How {
            tried,
            silent,
            ..How::default()
        };
        // A file that cannot be read is not the plugin's: the program does what it does with it.
        match self.open_with(path, &how) {
            Ok(Some(instance)) => KeyAnswer::go(KeyGo::Open(instance)),
            // A stop or a password refused: nothing more (arclite's `E_ABORT`); the search goes on.
            Err(error) if error.kind == ErrorKind::Cancelled && !silent => KeyAnswer::default(),
            Ok(None) | Err(_) => KeyAnswer::pass(),
        }
    }

    /// `path` as an archive on a new panel: its instance; none when it is no archive.
    pub(crate) fn open(
        &mut self,
        path: &Path,
        password: Option<String>,
    ) -> Result<Option<String>, Error> {
        let how = How {
            password,
            ..How::default()
        };
        self.open_with(path, &how)
    }

    /// `path` on a new panel as `how` says.
    pub(crate) fn open_with(&mut self, path: &Path, how: &How) -> Result<Option<String>, Error> {
        let Some(mut opened) = self.load_with(path, how)? else {
            return Ok(None);
        };
        if how.password.is_some() {
            opened.password.clone_from(&how.password);
        }
        self.opened += 1;
        let instance = self.opened.to_string();
        self.panels.insert(instance.clone(), opened);
        Ok(Some(instance))
    }
}

/// How a file is opened.
#[derive(Debug, Clone, Default)]
pub(crate) struct How {
    /// The password of encrypted headers (`arc:-p`); asked when none.
    pub password: Option<String>,
    /// The format menu's: every way, chosen in a menu (else the first).
    pub every: bool,
    /// The formats tried (the configuration's enabled / disabled ones, arclite's `analyse_open`);
    /// none: every one.
    pub tried: Option<Vec<usize>>,
    /// Alt+F7's search (`OPM_FIND`): no window at all, no password asked, the first way taken.
    pub silent: bool,
}
