//! Command prefixes (FAR's farcmds, `far-source/plugins/farcmds`): `view:`, `edit:`, `goto:`,
//! `whereis:`, `clip:`, `link:`, `run:`, `load:` and `unload:` on the command line, in the user menu and the
//! file associations as every prefix is. A command wrong in its form shows its syntax (FAR shows the
//! command's help, which the program has not yet). Its configuration is FAR's dialog at its places
//! (`ConfigureW`, the labels of `FARCmdsEng.lng`). A command's output (`<command`): `output`.

use std::path::{Path, PathBuf};

use nc_plugin_sdk::{
    Dialog, DialogItem, Error, Host, Info, KeyAnswer, Message, Plugin, Value, Which, export_plugin,
};

pub mod go;
pub mod link;
pub mod lnk;
pub mod output;
pub mod parse;
pub mod search;
pub mod text;

use parse::{Catch, Command, Screen};

/// Its title, its configuration's and its messages'.
const TITLE: &str = "Command prefixes";
/// FAR's default size of the data read (`MaxDataSize`: 1 MB).
const MOST: u64 = 1_048_576;
/// The configuration's items that matter.
pub const SCREEN: usize = 0;
pub const CATCH: usize = 4;
pub const EMPTY_FILES: usize = 8;
pub const NEW_FILES: usize = 10;
pub const MOST_LINE: usize = 13;
pub const OK: usize = 15;

/// FAR's options (`Options`, its settings' names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub screen: Screen,
    pub catch: Catch,
    /// "View/edit empty files" (`ViewZeroFiles`).
    pub empty_files: bool,
    /// "Edit new files" (`EditNewFiles`).
    pub new_files: bool,
    /// "The size of read data" (`MaxDataSize`).
    pub most: u64,
    /// "Command/object separator" (`Separator`, not in the dialog).
    pub separator: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            screen: Screen::Hide,
            catch: Catch::Both,
            empty_files: true,
            new_files: true,
            most: MOST,
            separator: " ".to_string(),
        }
    }
}

impl Options {
    pub fn read(host: &Host) -> Self {
        let most = u64::try_from(host.int_setting("MaxDataSize", 1_048_576)).unwrap_or(MOST);
        Self {
            screen: Screen::of(host.int_setting("ShowCmdOutput", 0)),
            catch: Catch::of(host.int_setting("CatchMode", 0)),
            empty_files: host.bool_setting("ViewZeroFiles", true),
            new_files: host.bool_setting("EditNewFiles", true),
            most: if most == 0 { MOST } else { most },
            separator: host.setting("Separator").unwrap_or_else(|| " ".to_string()),
        }
    }

    pub fn write(&self, host: &Host) {
        host.set_int("ShowCmdOutput", self.screen.index());
        host.set_int("CatchMode", self.catch.index());
        host.set_bool("ViewZeroFiles", self.empty_files);
        host.set_bool("EditNewFiles", self.new_files);
        host.set_int("MaxDataSize", i64::try_from(self.most).unwrap_or(i64::MAX));
    }
}

/// FAR's configuration dialog as its `PluginDialogBuilder` lays it out: 53 x 19.
pub fn dialog(options: &Options) -> Dialog {
    let radio = |label: &str, on: bool, y: i32| DialogItem::radio(label, on).at(5, y, 5, y);
    let line = |y: i32| DialogItem::separator().at(-1, y, -1, y);
    let screen = options.screen.index();
    let catch = options.catch.index();
    let items = vec![
        radio("&Hide external command output", screen == 0, 2).group(),
        radio("&Keep external command output", screen == 1, 3),
        radio("D&uplicate Intercepted output to screen", screen == 2, 4),
        line(5),
        radio("Intercept &both stdout and stderr", catch == 0, 6).group(),
        radio("Intercept std&out only", catch == 1, 7),
        radio("Intercept std&err only", catch == 2, 8),
        radio("Intercept stdout &and stderr separately", catch == 3, 9),
        DialogItem::check("View/edit e&mpty files", options.empty_files).at(5, 10, 5, 10),
        line(11),
        DialogItem::check("Edit &new files", options.new_files).at(5, 12, 5, 12),
        line(13),
        DialogItem::text("The &size of read data").at(5, 14, 26, 14),
        DialogItem::fixed("9999999999", &options.most.to_string()).at(28, 14, 37, 14),
        line(15),
        DialogItem::button("&OK")
            .default_button()
            .at(18, 16, 23, 16),
        DialogItem::button("Cancel").at(25, 16, 34, 16),
    ];
    Dialog::new(TITLE, items).size(53, 19)
}

/// The options the dialog's OK left (`values` one per item); the separator as it was.
pub fn from_dialog(values: &[Value], separator: &str) -> Options {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let chosen = |first: usize, count: usize| {
        let on = (0..count)
            .find(|at| value(first + at).checked())
            .unwrap_or(0);
        i64::try_from(on).unwrap_or(0)
    };
    let most = value(MOST_LINE).text.trim().parse().unwrap_or(0);
    Options {
        screen: Screen::of(chosen(SCREEN, 3)),
        catch: Catch::of(chosen(CATCH, 4)),
        empty_files: value(EMPTY_FILES).checked(),
        new_files: value(NEW_FILES).checked(),
        most: if most == 0 { MOST } else { most },
        separator: separator.to_string(),
    }
}

/// A command's syntax, shown when it is wrong (FAR shows its help topic).
pub fn syntax(prefix: &str) -> Vec<&'static str> {
    match prefix {
        "view" => vec!["view:filename", "view:<command"],
        "edit" => vec!["edit:[line,column] filename", "edit:<command"],
        "goto" => vec!["goto:path", "goto:<filename"],
        "clip" => vec!["clip:filename", "clip:<command"],
        "whereis" => vec!["whereis:object", "whereis:<filename"],
        "link" => vec!["link:[/msg] [/n] [/s] source destination"],
        "run" => vec!["run:filename<command"],
        "load" => vec!["load:plugin", "load:<filename"],
        _ => vec!["unload:plugin", "unload:<filename"],
    }
}

pub struct Commands {
    host: Host,
    options: Options,
}

impl Commands {
    /// The program's folder (FAR's `%FARHOME%`): two up from the plugin's.
    fn program_folder(&self) -> PathBuf {
        let folder = self.host.folder();
        (folder.parent().and_then(Path::parent)).map_or_else(|| folder.clone(), Path::to_path_buf)
    }

    /// The object, or with `<` the first line of the file it names (FAR's `__getContent`).
    fn content(&self, command: &Command, folder: &Path) -> Option<String> {
        if !command.from {
            return Some(command.object.clone());
        }
        let path = text::full_path(&text::unquote(&text::expand(&command.object)), folder);
        let read = text::read_text(&path, self.options.most, true)?;
        // An empty first line names nothing (FAR's `OpenW` does nothing then).
        Some(text::first_line(&read).to_string()).filter(|line| !line.trim().is_empty())
    }

    /// A file named by the object, whole.
    fn file(&self, command: &Command, folder: &Path) -> PathBuf {
        text::full_path(&text::expand(&text::unquote(&command.object)), folder)
    }

    fn run(&mut self, command: &Command, folder: &Path) {
        let output = matches!(command.prefix.as_str(), "view" | "edit" | "clip" | "run");
        if command.from && output {
            self.output(command, folder);
            return;
        }
        match command.prefix.as_str() {
            "view" => self.view(command, folder),
            "edit" => self.edit(command, folder),
            "clip" => {
                let read = text::read_text(&self.file(command, folder), self.options.most, false);
                let _ = self.host.set_clipboard(&read.unwrap_or_default());
            }
            "goto" => self.goto(command, folder),
            "whereis" => self.where_is(command, folder),
            "load" | "unload" => self.plugins(command, folder),
            _ => self.link(command, folder),
        }
    }

    /// `<command` run, its output viewed, edited, put in the clipboard or left in `run:`'s file.
    fn output(&self, command: &Command, folder: &Path) {
        let prefix = command.prefix.as_str();
        let separate = matches!(prefix, "view" | "edit" | "clip");
        let (take_out, take_err) = output::streams(command.catch, separate);
        let both = take_out && !take_err && matches!(command.catch, Catch::Both | Catch::Apart);
        let (work, rest) = output::working_folder(&command.object);
        // A working folder not there: the current one (FAR does not look at `SetCurrentDirectory`).
        let run_in = (work.map(|work| text::full_path(&work, folder)))
            .filter(|work| work.is_dir())
            .unwrap_or_else(|| folder.to_path_buf());
        let line = output::command_line(rest, folder, &self.program_folder());
        if command
            .run_file
            .as_deref()
            .is_some_and(|file| file.trim().is_empty())
        {
            let lines = ["run: needs the file for the output", "run:filename<command"];
            let _ = self.host.message(TITLE, &lines);
            return;
        }
        let into = |which: &str| match &command.run_file {
            Some(file) => text::full_path(&text::expand(&text::unquote(file)), folder),
            None => output::temporary(which),
        };
        let files = output::Files {
            out: take_out.then(|| into("out")),
            err: take_err.then(|| into("err")),
        };
        let ran = output::run(&self.host, TITLE, &line, &run_in, &files, both);
        self.finish_output(command, files, &ran);
    }

    /// After the run: the output shown, put in the clipboard or left in `run:`'s file; a stop told (ADR-0019:
    /// what it wrote shown, the clipboard as it was), a run that could not start told.
    fn finish_output(&self, command: &Command, files: output::Files, ran: &output::Ran) {
        let prefix = command.prefix.as_str();
        match ran {
            output::Ran::Failed(why) => {
                let _ = self
                    .host
                    .show(&Message::new(TITLE, &["Cannot run the command", why]).warning());
                if prefix == "run" {
                    // A file it made and nothing wrote goes (FAR's `killTemp`).
                    for file in [&files.out, &files.err].into_iter().flatten() {
                        if std::fs::metadata(file).is_ok_and(|meta| meta.len() == 0) {
                            let _ = std::fs::remove_file(file);
                        }
                    }
                    return;
                }
            }
            output::Ran::Stopped => {
                let what = match prefix {
                    "clip" => "The clipboard is as it was.",
                    "run" => "Its file holds what it wrote.",
                    _ => "What it wrote is shown.",
                };
                let lines = ["The command was ended with all it started.", what];
                let _ = self.host.show(&Message::new("Stopped", &lines));
            }
            output::Ran::Done => {}
        }
        if prefix == "run" {
            return;
        }
        let failed = matches!(ran, output::Ran::Failed(_));
        let shown = match prefix {
            "view" | "edit" if !failed => self.show_output(command, &files),
            "clip" if *ran == output::Ran::Done => {
                let read = (files.out.as_ref())
                    .and_then(|out| text::read_text(out, self.options.most, true));
                let _ = self.host.set_clipboard(&read.unwrap_or_default());
                None
            }
            _ => None,
        };
        for file in [files.out, files.err].into_iter().flatten() {
            if Some(&file) != shown.as_ref() {
                let _ = std::fs::remove_file(file);
            }
        }
    }

    /// The output's file viewed or edited, deleted as it closes (the errors' when only they were taken
    /// or the output is empty); the one shown.
    fn show_output(&self, command: &Command, files: &output::Files) -> Option<PathBuf> {
        // The output when it has something, else the errors when they have, else (empty files shown) the
        // empty output.
        let size = |path: &PathBuf| std::fs::metadata(path).map(|meta| meta.len()).ok();
        let full = |path: &&PathBuf| size(path).is_some_and(|size| size > 0);
        let empty = |path: &&PathBuf| self.options.empty_files && size(path).is_some();
        let path = (files.out.as_ref().filter(full))
            .or_else(|| files.err.as_ref().filter(full))
            .or_else(|| files.out.as_ref().or(files.err.as_ref()).filter(empty))?
            .clone();
        let place = |value: Option<i64>| {
            value
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(0)
        };
        let opened = match command.prefix.as_str() {
            "view" => self.host.view(&path, true),
            _ => (self.host).edit(&path, place(command.line), place(command.column), true),
        };
        opened.then_some(path)
    }

    fn view(&self, command: &Command, folder: &Path) {
        let path = self.file(command, folder);
        if path.exists() {
            let _ = self.host.view(&path, false);
        }
    }

    fn edit(&self, command: &Command, folder: &Path) {
        let path = self.file(command, folder);
        let place = |value: Option<i64>| {
            value
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(0)
        };
        // A file not there waits for the editor's new files (tech-debt P6.2a).
        if path.exists() {
            let _ = (self.host).edit(&path, place(command.line), place(command.column), false);
        }
    }

    fn goto(&self, command: &Command, folder: &Path) {
        let shortcut = command.from.then(|| {
            let path = text::full_path(&text::unquote(&text::expand(&command.object)), folder);
            text::read_bytes(&path, self.options.most).and_then(|bytes| lnk::target(&bytes))
        });
        let path = shortcut.flatten().or_else(|| self.content(command, folder));
        if let Some(path) = path {
            go::to(&self.host, &text::expand(&path));
        }
    }

    fn where_is(&self, command: &Command, folder: &Path) {
        let Some(name) = self.content(command, folder) else {
            return;
        };
        let name = text::unquote(&name);
        if name.trim().is_empty() {
            return;
        }
        let found = search::find(&name, folder, &self.program_folder(), true);
        let path = found.map_or(name, |found| found.display().to_string());
        go::to(&self.host, &path);
    }

    fn plugins(&self, command: &Command, folder: &Path) {
        let Some(name) = self.content(command, folder) else {
            return;
        };
        let name = text::expand(&text::unquote(&name));
        // A path made whole against the current folder (FAR's `ConvertNameToFull`); a bare name stays one.
        let path = text::full_path(&name, folder);
        let name = match name.contains(['\\', '/']) || path.exists() {
            true => path.display().to_string(),
            false => name,
        };
        let _ = match command.prefix.as_str() {
            "load" => self.host.load_plugin(&name),
            _ => self.host.unload_plugin(&name),
        };
    }

    fn link(&self, command: &Command, folder: &Path) {
        let Some(asked) = link::parse(&command.object) else {
            let _ = self.host.message(TITLE, &syntax("link"));
            return;
        };
        let source = text::full_path(&text::expand(&asked.source), folder);
        let kind = link::kind(&asked, &source);
        let path = link::link_path(&asked, kind, folder);
        let source = match kind {
            link::Kind::Mount => PathBuf::from(format!("{}:\\", &asked.source[..1])),
            _ => source,
        };
        match link::make(kind, &source, &path) {
            Ok(()) if !asked.keep_panels => {
                let _ = self.host.reread(Which::Active, true);
                let _ = self.host.reread(Which::Passive, true);
            }
            Ok(()) => {}
            Err(error) if asked.tell => self.tell_failure(kind, &asked.source, &path, &error),
            Err(_) => {}
        }
    }

    /// FAR's warning after a link that could not be made (`MkSymLink`'s messages).
    fn tell_failure(&self, kind: link::Kind, source: &str, path: &Path, error: &std::io::Error) {
        let shown = path.display().to_string();
        let lines = match kind {
            link::Kind::Mount => vec![
                format!("Attempt to volume mount '{source}'"),
                format!("at '{shown}' failed"),
            ],
            _ => vec!["Cannot create the link".to_string(), shown],
        };
        let mut lines = lines;
        lines.push(error.to_string());
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let message = Message::new("Error", &lines).warning();
        let _ = self.host.show(&message);
    }
}

impl Plugin for Commands {
    fn new(host: Host) -> Self {
        let options = Options::read(&host);
        Self { host, options }
    }

    fn info(&self) -> Info {
        Info {
            title: TITLE.to_string(),
            description: "view:, edit:, goto:, whereis:, clip:, link:, run:, load:, unload:"
                .to_string(),
            author: "nova-commander".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn prefixes(&self) -> Vec<String> {
        parse::PREFIXES.iter().map(ToString::to_string).collect()
    }

    fn open_command(
        &mut self,
        prefix: &str,
        command: &str,
        folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        // No folder (a panel not on a disk): the program's current folder, as FAR's.
        let folder = folder
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok());
        let folder = folder.unwrap_or_else(|| self.host.folder());
        let defaults = (self.options.catch, self.options.screen);
        match parse::parse(prefix, command, &self.options.separator, defaults) {
            Some(command) => self.run(&command, &folder),
            None => {
                self.host
                    .message(TITLE, &syntax(&prefix.to_ascii_lowercase()));
            }
        }
        Ok(KeyAnswer::default())
    }

    fn config_items(&self) -> Vec<String> {
        vec![TITLE.to_string()]
    }

    fn configure(&mut self, _index: usize) -> bool {
        let closed = self.host.dialog(&dialog(&self.options));
        if closed.button != Some(OK) {
            return false;
        }
        self.options = from_dialog(&closed.values, &self.options.separator);
        self.options.write(&self.host);
        true
    }
}

export_plugin!(Commands);

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
