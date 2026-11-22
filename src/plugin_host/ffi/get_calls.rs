//! A panel's files written out by its plugin (FAR's `GetFilesW`, API 5): the entries, the disk's folder
//! they go to, moving or not, and why (`GET_*`); and a disk's files put into a panel that takes paths
//! (FAR's `PutFilesW`, API 5's request with its mode); deleting, making a folder, a new panel and the
//! commands on a panel's file (`DeleteFilesW`, `MakeDirectoryW`, `OFP_CREATE`, `ProcessHostFileW`). Not
//! quiet calls: the plugin's windows are shown.

use nc_plugin_api::{ERROR_CANCELLED, NcGet, NcPut, NcStr};

use super::Loaded;

/// What the program asks a plugin to write out of its panel's folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GetFiles {
    /// Entries of the folder by their names.
    pub names: Vec<String>,
    /// The folder on a disk they go to.
    pub destination: String,
    pub moving: bool,
    /// The ABI's `GET_*`; 0: F5 / F6 with the plugin's dialog.
    pub mode: u32,
    /// The program's folder for names given without one; empty: none.
    pub folder: String,
}

/// What the program asks a panel that takes paths to take in (FAR's `PutFilesInfo`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PutFiles {
    /// Files and folders on a disk by their absolute paths.
    pub sources: Vec<String>,
    pub moving: bool,
    /// The ABI's `GET_*` (FAR's `OPM_*`); 0: F5 / F6.
    pub mode: u32,
    /// The folder they were taken from; empty: none.
    pub folder: String,
}

impl Loaded {
    /// `files` put into the folder `path` of the panel `instance` that takes paths: true done, false
    /// stopped by the user, else the plugin's reason. Not quiet: its windows are shown.
    pub fn put(&self, instance: &str, path: &str, files: &PutFiles) -> Result<bool, String> {
        let table = self.table();
        let sources: Vec<NcStr> = (files.sources.iter())
            .map(|source| NcStr::new(source))
            .collect();
        let raw = NcPut {
            sources: sources.as_ptr(),
            count: sources.len(),
            moving: u32::from(files.moving),
            mode: files.mode,
            folder: NcStr::new(&files.folder),
        };
        // SAFETY: the plugin's own table and state; the texts and the request live through the call.
        let code = unsafe {
            (table.put)(
                table.state,
                NcStr::new(instance),
                NcStr::new(path),
                &raw const raw,
            )
        };
        match code {
            0 => Ok(true),
            ERROR_CANCELLED => Ok(false),
            _ => Err(self.last_error()),
        }
    }

    /// The entries `names` of the folder `path` of the panel `instance` deleted by the plugin, `mode`
    /// FAR's `OPM_*` (its own question for 0): true done, false stopped by the user, else its reason.
    pub fn delete_files(
        &self,
        instance: &str,
        path: &str,
        names: &[String],
        mode: u32,
    ) -> Result<bool, String> {
        let table = self.table();
        let texts: Vec<NcStr> = names.iter().map(|name| NcStr::new(name)).collect();
        // SAFETY: the plugin's own table and state; the texts and the list live through the call.
        let code = unsafe {
            (table.delete_files)(
                table.state,
                NcStr::new(instance),
                NcStr::new(path),
                texts.as_ptr(),
                texts.len(),
                mode,
            )
        };
        self.answered(code)
    }

    /// A folder made by the plugin in the folder `path` of the panel `instance`, `name` asked for (empty:
    /// the plugin asks it): the name made, none when the user stopped it, else its reason.
    pub fn make_folder(
        &self,
        instance: &str,
        path: &str,
        name: &str,
    ) -> Result<Option<String>, String> {
        let table = self.table();
        let mut answer = NcStr::new(name);
        // SAFETY: the plugin's own table and state; the texts live through the call, the answer is the
        // plugin's until its next call and copied at once.
        let code = unsafe {
            (table.make_folder)(
                table.state,
                NcStr::new(instance),
                NcStr::new(path),
                0,
                &raw mut answer,
            )
        };
        // Read before the plugin's next call, as every text of a plugin (review R46: lossy, capped).
        let made = super::text(answer);
        Ok(self.answered(code)?.then_some(made))
    }

    /// A panel of the plugin's with nothing in it yet (FAR's `OpenFilePlugin` with `OFP_CREATE`): its
    /// instance, none when the plugin makes none, else its reason.
    pub fn open_new(&self) -> Result<Option<String>, String> {
        let table = self.table();
        let mut answer = NcStr::EMPTY;
        // SAFETY: the plugin's own table and state; the answer is the plugin's until its next call and
        // copied at once.
        let code = unsafe { (table.open_new)(table.state, &raw mut answer) };
        // Read before the plugin's next call, as every text of a plugin (lossy, capped).
        let instance = super::text(answer);
        self.answered(code)?;
        Ok(Some(instance).filter(|instance| !instance.is_empty()))
    }

    /// The entries `names` of the folder `path` of the panel `instance` handed to the plugin's commands
    /// on the file the panel was opened from (FAR's `ProcessHostFileW`), `mode` FAR's `OPM_*`: what it
    /// did (`HOST_FILE_*`; 0 also when the user stopped it), else its reason.
    pub fn process_host_file(
        &self,
        instance: &str,
        path: &str,
        names: &[String],
        mode: u32,
    ) -> Result<u32, String> {
        let table = self.table();
        let texts: Vec<NcStr> = names.iter().map(|name| NcStr::new(name)).collect();
        let mut done = 0;
        // SAFETY: the plugin's own table and state; the texts, the list and the answer's place live
        // through the call.
        let code = unsafe {
            (table.process_host_file)(
                table.state,
                NcStr::new(instance),
                NcStr::new(path),
                texts.as_ptr(),
                texts.len(),
                mode,
                &raw mut done,
            )
        };
        let answered = self.answered(code)?;
        Ok(if answered { done } else { 0 })
    }

    /// A call's code: done, stopped by the user, or the plugin's reason.
    fn answered(&self, code: i32) -> Result<bool, String> {
        match code {
            0 => Ok(true),
            ERROR_CANCELLED => Ok(false),
            _ => Err(self.last_error()),
        }
    }

    /// `files` of the folder `path` of the panel `instance` written out: true done, false stopped by the
    /// user, else the plugin's reason.
    pub fn get(&self, instance: &str, path: &str, files: &GetFiles) -> Result<bool, String> {
        let table = self.table();
        let names: Vec<NcStr> = files.names.iter().map(|name| NcStr::new(name)).collect();
        let raw = NcGet {
            names: names.as_ptr(),
            count: names.len(),
            destination: NcStr::new(&files.destination),
            moving: u32::from(files.moving),
            mode: files.mode,
            folder: NcStr::new(&files.folder),
        };
        // SAFETY: the plugin's own table and state; the texts and the request live through the call.
        let code = unsafe {
            (table.get)(
                table.state,
                NcStr::new(instance),
                NcStr::new(path),
                &raw const raw,
            )
        };
        match code {
            0 => Ok(true),
            ERROR_CANCELLED => Ok(false),
            _ => Err(self.last_error()),
        }
    }
}
