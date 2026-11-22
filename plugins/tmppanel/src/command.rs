//! The temporary panel opened from the command line and by a list file (FAR's TmpPanel `OpenW`,
//! `TmpPanel.cpp:346-440`). The options are read again first (`GetOptions`); `tmp:` then takes switches,
//! each `+name` or `-name` up to a space or `<` - `safe`, `any`, `replace`, `menu`, `full` turn those
//! options on or off (compared as written, as FAR's `Cmd == Name`) and `+0`..`+9` make that common panel
//! the current one - kept until the options are read again. What is left: nothing opens the panel,
//! `<command` runs the command and takes its output as a list, anything else names a list file (looked for
//! as `FindListFile` does; one not found does nothing). A list replaces the panel's items or adds to them
//! (Replace) or, with "Menu from file list", is shown as a menu (`menu_list`). A list file opened with Enter
//! or Ctrl+PgDn (its masks, not empty: `AnalyseW`) is read the same way.

use std::path::{Path, PathBuf};

use nc_plugin_sdk::{CodePage, Error, KeyAnswer, KeyGo};

use crate::{Options, TmpPanel, listfile};

/// The option a switch turns on (`+`) or off (`-`), as `OpenW` names them.
fn switch<'a>(options: &'a mut Options, name: &str) -> Option<&'a mut bool> {
    Some(match name {
        "safe" => &mut options.safe_mode_panel,
        "any" => &mut options.any_in_panel,
        "replace" => &mut options.replace,
        "menu" => &mut options.menu_for_filelist,
        "full" => &mut options.full_screen_panel,
        _ => return None,
    })
}

/// `OpenW`'s switches at the start of `line`: the options they set, the common panel `+n` names, and the
/// rest, trimmed.
pub fn switches<'a>(line: &'a str, options: &mut Options, current: &mut usize) -> &'a str {
    let mut rest = line.trim_start();
    while rest.len() > 1 && rest.starts_with(['+', '-']) {
        let on = rest.starts_with('+');
        let after = &rest[1..];
        let name = &after[..after.find([' ', '<']).unwrap_or(after.len())];
        if let Some(value) = switch(options, name) {
            *value = on;
        }
        if let (true, Some(digit)) = (on, single_digit(name)) {
            *current = digit;
        }
        rest = after[name.len()..].trim_start();
    }
    rest.trim()
}

fn single_digit(name: &str) -> Option<usize> {
    let mut chars = name.chars();
    let digit = chars.next()?.to_digit(10)?;
    if chars.next().is_some() {
        return None;
    }
    usize::try_from(digit).ok()
}

/// FAR's `AnalyseW`: a file with bytes in it (an empty one is not a list).
fn takes(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}

/// FAR's `unquote`: the quotes of a name dropped.
fn unquote(name: &str) -> String {
    name.replace('"', "")
}

impl TmpPanel {
    /// `tmp:<line>` (FAR's `OpenW` with `OPEN_COMMANDLINE`), `folder` the program's folder.
    pub(crate) fn command(
        &mut self,
        line: &str,
        folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        self.options = Options::read(&self.host);
        let rest = switches(line, &mut self.options, &mut self.current).to_string();
        if rest.is_empty() {
            return Ok(KeyAnswer::go(KeyGo::Open(self.new_instance())));
        }
        if let Some(command) = rest.strip_prefix('<') {
            let Some(output) = self.run_output(command, folder) else {
                return Ok(KeyAnswer::default());
            };
            return self.open_lines(&output, "", folder);
        }
        let name = unquote(&listfile::expand_env(&rest));
        // Not found: nothing, no message (`OpenW`).
        let Some(file) = listfile::find(&name, folder, &self.host.folder()) else {
            return Ok(KeyAnswer::default());
        };
        let lines = self.read_list(&file);
        self.open_lines(&lines, &file.display().to_string(), folder)
    }

    /// A list file opened with Enter or Ctrl+PgDn (FAR's `AnalyseW` and `OPEN_ANALYSE`): an empty one
    /// is not taken.
    pub(crate) fn open_list(
        &mut self,
        path: &Path,
        folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        self.options = Options::read(&self.host);
        // An empty file is no list: looked at before anything is read (review R51); then one that
        // cannot be read is no plugin's either (FAR's `OpenFilePlugin`).
        if !takes(path) {
            return Ok(KeyAnswer::pass());
        }
        let Ok(bytes) = std::fs::read(path) else {
            return Ok(KeyAnswer::pass());
        };
        let lines = self.lines_of(&bytes);
        let answer = self.open_lines(&lines, &path.display().to_string(), folder)?;
        Ok(answer)
    }

    /// The lines of a list as a menu (with "Menu from file list") or on a panel opened for them
    /// (FAR's `ProcessList`); `name` the list file's, the menu's title.
    fn open_lines(
        &mut self,
        lines: &[String],
        name: &str,
        folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        if self.options.menu_for_filelist {
            return Ok(self.menu_from_list(lines, name, folder));
        }
        let instance = self.new_instance();
        if self.options.replace {
            self.list(&instance)?.clear();
        }
        let any = self.options.any_in_panel;
        let items: Vec<PathBuf> = (lines.iter())
            .filter_map(|line| listfile::item(line, folder, any))
            .collect();
        self.put_paths(&instance, &items)?;
        Ok(KeyAnswer::go(KeyGo::Open(instance)))
    }

    /// The list file's lines (FAR's `ReadFileList`); none when it cannot be read.
    fn read_list(&self, path: &Path) -> Vec<String> {
        std::fs::read(path).map_or_else(|_| Vec::new(), |bytes| self.lines_of(&bytes))
    }

    /// A list's lines from its bytes, the OEM page read by the program.
    pub(crate) fn lines_of(&self, bytes: &[u8]) -> Vec<String> {
        let host = self.host;
        let oem = move |line: &[u8]| {
            host.decode(CodePage::Oem, line)
                .unwrap_or_else(|| String::from_utf8_lossy(line).into_owned())
        };
        listfile::lines(bytes, &oem)
    }
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
