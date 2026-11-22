//! The probe: a plugin the tests load (ADR-0016), not shipped. Its commands say hello through the host's
//! message window, fail with an error, and panic. A file in its folder makes it misbehave as a test asks:
//! `panic-in-new`, `message-in-new` and `message-in-info` (a message while it starts), `panic-in-info`,
//! `panic-in-commands`; `commands.txt` gives its menu items (one a line) instead of its own, and
//! `message.txt` the message "Say hello" sends (its title on the first line, the rest handed over as it
//! is). A command past its own three panics with a formatted message (a `String` panic). With `windows` in
//! its folder it has eight more (ADR-0016 5b-1, 5b-2): "Ask" (a warning message with three buttons),
//! "Choose" (a menu with a separator, a grey line, a check mark, automatic hotkeys and the break keys F4
//! and Shift+Del), "Type" (an input line), each telling the answer in a message titled "Answer" and its
//! log; "Work" (a window without buttons for 300 ms), "Log" (two lines in its log) and "Dialog" (5b-2: a
//! name that may not be empty on OK - a message says so and the dialog stays -, "Upper case" turning the
//! name to capitals, "Show code" showing a hidden password line, two radio buttons on one row, a list,
//! "Fill" that does not close; every event in its log, the end told in "Answer"), "Error" (E11: a warning
//! with the system's text of a file not found, separators with titles, a long line and Retry / Cancel) and
//! "Options" (E12, E13: a menu with hidden, grayed and `&` lines going round its ends, a password line with
//! buttons). `disks.txt` gives its drive menu items (5b-3, one a line; none without it): one chosen is told
//! in a message titled "Disk" and its log, an item named "Fail" fails; one named "Panel <name>" (an F11
//! command so named in `commands.txt` too) opens the probe's panel `<name>` (5c): the folders of `panel`
//! below, titled "Probe <name>:<path>" (the instance `untitled` without a title), its closing in its log;
//! its files read, written, made, renamed and removed (5c-2), each change in its log, what goes in given
//! its times and marks (P6.0d, "kept times"); with `synchro` it asks for its synchro from a thread of its
//! own as it starts and logs it as it comes (P6.0e); the instance `asking`
//! asks in a message before making a folder (a window in a panel's call) and logs the answer. Its panels
//! have their own modes at Ctrl+3 and Ctrl+6 with texts for `C0`, `C1` and `Z` (5c-3; not `real` and
//! `untitled`); the instances `six` and `seven` open at Ctrl+6 and Ctrl+7 (5d-1: it has no mode at Ctrl+7),
//! `sorted` by size, descending (API 2: FAR's `StartSortMode`); beside them the info panel shows a
//! separator "Probe <name>" and the line "Path" with the folder (API 3, FAR's `InfoLines`).
//! `configs.txt` gives its configuration items (5d-3a, one a line): one chosen counts in its setting
//! `count`, keeps its label in `label` and says the settings changed - then its F11 commands end with
//! "Configured <count>" -, one named "Unchanged" changes nothing, one named "Panic" panics, one named
//! "Busy" shows a window without buttons for 300 ms and says the settings changed. A command named "Count"
//! (in `commands.txt`, API 2, ADR-0019 §5) counts to 200 in 10 s telling its progress ("item <n>") and
//! ends at once when asked to stop, its log saying where ("stopped at <n>"); one named "Stubborn" never
//! looks: it sleeps 30 s, or until a file `release` is in its folder. `slow-start` makes it take 2 s to
//! start (review R17: the plugins start side by side). A command named "Unload <name>" unloads that
//! plugin (P6.0e, FAR's `PCTL_UNLOADPLUGIN`; itself too) and logs the answer and whether it was asked to
//! stop then ("unload <name> <answer> cancelled <bool>").

use std::path::PathBuf;
use std::time::Duration;

use nc_plugin_sdk::{
    Controls, Dialog, DialogEvent, DialogItem, Error, FileEntry, Host, Info, Menu, MenuItem,
    Message, PanelMode, Plugin, WriteMode, export_plugin,
};

mod editor;
mod panel;
mod panels;

/// The commands `windows` adds after the three.
const WINDOWS: [&str; 8] = [
    "Ask", "Choose", "Type", "Work", "Log", "Dialog", "Error", "Options",
];

pub struct Probe {
    host: Host,
    folder: PathBuf,
    panels: panel::Panels,
}

impl Probe {
    fn asked(&self, marker: &str) -> bool {
        self.folder.join(marker).exists()
    }

    /// What the marker file `marker` says, if it is there.
    fn told(&self, marker: &str) -> Option<String> {
        std::fs::read_to_string(self.folder.join(marker)).ok()
    }

    fn say_hello(&self) -> bool {
        match self.told("message.txt") {
            Some(text) => {
                let (title, body) = text.split_once('\n').unwrap_or((text.as_str(), ""));
                self.host.message(title, &[body])
            }
            None => self
                .host
                .message("Probe", &["Hello from the probe", "second line"]),
        }
    }

    /// "Count": 200 items in 10 s, its progress told, ended at once when asked to stop (ADR-0019 §5).
    fn count(&self) -> Option<String> {
        const ITEMS: u64 = 200;
        for item in 0..ITEMS {
            if self.host.cancelled() {
                self.host.log(&format!("stopped at {item}"));
                return None;
            }
            self.host.progress(item, ITEMS, &format!("item {item}"));
            std::thread::sleep(Duration::from_millis(50));
        }
        self.host.log("counted");
        None
    }

    /// "Stubborn": never looks whether it was asked to stop; 30 s, or until `release` is in its folder.
    fn stubborn(&self) -> Option<String> {
        let until = std::time::Instant::now() + Duration::from_secs(30);
        while std::time::Instant::now() < until && !self.asked("release") {
            std::thread::sleep(Duration::from_millis(20));
        }
        self.host.log("stubborn ended");
        None
    }

    /// The answer in a message and the log.
    fn tell(&self, answer: &str) {
        self.tell_in("Answer", answer);
    }

    /// `text` in a message titled `title` and the log.
    fn tell_in(&self, title: &str, text: &str) {
        self.host.log(text);
        self.host.message(title, &[text]);
    }

    fn windows(&self, index: usize) {
        match index {
            3 => {
                let message = Message::new("Question", &["Go on?", "Really"])
                    .buttons(&["&Yes", "&No", "&Cancel"])
                    .warning();
                self.tell(&format!("{:?}", self.host.show(&message)));
            }
            4 => {
                let items = vec![
                    MenuItem::new("First"),
                    MenuItem::separator(),
                    MenuItem::new("Grey").disabled(true),
                    MenuItem::new("Third").checked(true),
                ];
                let menu = Menu::new("Probe menu", items)
                    .bottom("F4 Shift+Del")
                    .selected(3)
                    .break_keys(&["F4", "Shift+Del"])
                    .auto_hotkeys();
                self.tell(&format!("{:?}", self.host.menu(&menu)));
            }
            5 => {
                let typed = self.host.input("Probe", "Type something:", "start");
                self.tell(&format!("{typed:?}"));
            }
            6 => {
                self.host.busy("Probe", &["Working..."]);
                std::thread::sleep(Duration::from_millis(300));
                self.host.log("worked");
            }
            7 => self.host.log("first line\nsecond line"),
            8 => self.dialog(),
            9 => self.error_message(),
            _ => self.options(),
        }
    }

    /// "Error" (ADR-0016 §15 E11): a file not found left as the thread's last error, then FAR's message
    /// with it, separators with titles, a line too long for the screen and the program's Retry / Cancel.
    fn error_message(&self) {
        let long = format!("Long {}", "x".repeat(300));
        let message = Message::new(
            "Trouble",
            &["Cannot read it", "\u{1}Details", &long, "\u{2}Why"],
        )
        .error()
        .button_set(nc_plugin_sdk::ButtonSet::RetryCancel)
        .warning();
        // Last before the call: nothing between may change the thread's last error.
        let missing = std::fs::File::open(self.host.folder().join("no such file"));
        let answer = self.host.show(&message);
        drop(missing);
        self.tell(&format!("{answer:?}"));
    }

    /// "Options" (ADR-0016 §15 E12, E13): a menu with a hidden, a grayed line and `&` shown as it is,
    /// going round its ends, then a password line with "OK" / "Cancel" that takes an empty answer.
    fn options(&self) {
        let menu = Menu::new(
            "Options",
            vec![
                MenuItem::new("R&D"),
                MenuItem::new("Hidden one").hidden(true),
                MenuItem::new("Grayed one").grayed(true),
                MenuItem::new("Last"),
            ],
        )
        .wrap()
        .show_ampersand();
        self.tell(&format!("{:?}", self.host.menu(&menu)));
        let options = nc_plugin_sdk::InputOptions {
            password: true,
            buttons: true,
            enable_empty: true,
            ..nc_plugin_sdk::InputOptions::default()
        };
        let typed = self.host.input_with("Secret", "Code:", "", options);
        self.tell(&format!("{typed:?}"));
    }

    fn dialog(&self) {
        if self.asked("api3") {
            self.api3_dialog();
            return;
        }
        let items = vec![
            DialogItem::text("&Name:"),
            DialogItem::edit("start"),
            DialogItem::check("&Upper case", false),
            DialogItem::check("&Show code", false),
            DialogItem::password("secret").hidden(true),
            DialogItem::radio("&One", true).group(),
            DialogItem::radio("&Two", false).same_row(),
            DialogItem::list(&["Red", "Green", "Blue"], 1),
            DialogItem::button("OK").default_button(),
            DialogItem::button("&Fill").no_close(),
            DialogItem::button("Cancel"),
        ];
        let host = self.host;
        let dialog = Dialog::new("Probe dialog", items);
        let closed = self.host.dialog_with(&dialog, |event, controls| {
            host.log(&format!("{event:?}"));
            on_dialog_event(host, event, controls)
        });
        let value = |item: usize| closed.values.get(item).cloned().unwrap_or_default();
        self.tell(&format!(
            "{:?} name={} upper={} code={} radio={} list={}",
            closed.button,
            value(1).text,
            value(2).state,
            value(4).text,
            value(5).state,
            value(7).text
        ));
    }
}

impl Probe {
    /// With `api3` in its folder: a list box, a masked line and a line with a history, the handler hearing
    /// the keys first and taking F4 to give the list new lines.
    fn api3_dialog(&self) {
        let items = vec![
            DialogItem::list_box("Pick", &["Alpha", "Beta", "Gamma"], 0).height(3),
            DialogItem::fixed("99:99", "12:30"),
            DialogItem::edit("").history("ProbeLine", true),
            DialogItem::button("OK").default_button(),
            DialogItem::button("Cancel"),
        ];
        let dialog = Dialog::new("Probe API 3", items).hear_keys();
        let closed = self
            .host
            .dialog_with(&dialog, |event, controls| match event {
                DialogEvent::Key { key, .. } if key == "F4" => {
                    controls.set_lines(0, &["Delta", "Epsilon"]);
                    false
                }
                _ => true,
            });
        let value = |item: usize| closed.values.get(item).cloned().unwrap_or_default();
        self.tell(&format!(
            "{:?} pick={} time={} line={}",
            closed.button,
            value(0).text,
            value(1).text,
            value(2).text
        ));
    }
}

/// The probe dialog's handler: false keeps it open on closing.
fn on_dialog_event(host: Host, event: DialogEvent, controls: &mut Controls) -> bool {
    match event {
        DialogEvent::Changed(2) if controls.value(2).checked() => {
            controls.set_text(1, &controls.value(1).text.to_uppercase());
        }
        DialogEvent::Changed(3) => {
            controls.show(4, controls.value(3).checked());
        }
        // With `nested` in its folder "Fill" first opens a dialog of its own (ADR-0016 §15 E14: FAR's nested
        // modal dialogs), the button that closed it written into the name.
        DialogEvent::Button(9) if host.folder().join("nested").exists() => {
            let inner = Dialog::new(
                "Inner dialog",
                vec![
                    DialogItem::text("Inside"),
                    DialogItem::button("OK").default_button(),
                ],
            );
            let closed = host.dialog(&inner);
            controls.set_text(1, &format!("inner {:?}", closed.button));
        }
        DialogEvent::Button(9) => {
            controls.set_text(1, "filled");
            controls.enable(7, false);
        }
        DialogEvent::Close(Some(8)) if controls.value(1).text.is_empty() => {
            host.message("Probe", &["The name is empty"]);
            controls.focus(1);
            return false;
        }
        _ => {}
    }
    true
}

impl Plugin for Probe {
    fn new(host: Host) -> Self {
        let probe = Self {
            host,
            folder: host.folder(),
            panels: panel::Panels::new(host.folder()),
        };
        assert!(!probe.asked("panic-in-new"), "the probe panics starting");
        if probe.asked("slow-start") {
            std::thread::sleep(Duration::from_secs(2));
        }
        if probe.asked("message-in-new") {
            // Not shown: the program answers Esc at once while a plugin starts.
            let shown = host.message("Probe", &["starting"]);
            std::fs::write(probe.folder.join("answered"), shown.to_string()).ok();
        }
        if probe.asked("synchro") {
            // A thread of its own asks to be called back on the plugin's thread (P6.0e).
            let synchro = host.synchro();
            let _ = std::thread::spawn(move || synchro.ask(7)).join();
        }
        probe
    }

    fn info(&self) -> Info {
        assert!(!self.asked("panic-in-info"), "the probe panics telling");
        if self.asked("message-in-info") {
            // Asked while the program waits for the start: Esc at once.
            let shown = self.host.message("Probe", &["telling"]);
            std::fs::write(self.folder.join("answered"), shown.to_string()).ok();
        }
        Info {
            title: "Probe".to_string(),
            description: "The tests' plugin".to_string(),
            author: "nova-commander".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<String> {
        assert!(!self.asked("panic-in-commands"), "the probe panics listing");
        if let Some(text) = self.told("commands.txt") {
            return text.lines().map(String::from).collect();
        }
        let mut commands: Vec<String> = ["Say hello", "Fail", "Panic"].map(String::from).to_vec();
        if self.asked("windows") {
            commands.extend(WINDOWS.map(String::from));
        }
        if let Some(count) = self.host.setting("count") {
            commands.push(format!("Configured {count}"));
        }
        commands
    }

    fn config_items(&self) -> Vec<String> {
        let text = self.told("configs.txt").unwrap_or_default();
        text.lines().map(String::from).collect()
    }

    fn configure(&mut self, index: usize) -> bool {
        let label = self.config_items().get(index).cloned().unwrap_or_default();
        match label.as_str() {
            "Unchanged" => false,
            "Panic" => panic!("the probe's configuration panics on purpose"),
            "Busy" => {
                self.host.busy("Probe", &["Configuring"]);
                std::thread::sleep(Duration::from_millis(300));
                true
            }
            _ => {
                let count = self.host.int_setting("count", 0) + 1;
                self.host.set_int("count", count);
                self.host.set_setting("label", &label);
                true
            }
        }
    }

    fn editor_items(&self) -> Vec<String> {
        editor::ITEMS.map(String::from).to_vec()
    }

    fn run_editor_item(&mut self, index: usize) -> Result<(), String> {
        editor::run(&self.host, index)
    }

    fn disks(&self) -> Vec<String> {
        let text = self.told("disks.txt").unwrap_or_default();
        text.lines().map(String::from).collect()
    }

    fn open_disk(&mut self, index: usize) -> Result<Option<String>, String> {
        let disks = self.disks();
        let Some(label) = disks.get(index) else {
            return Err(format!("The probe has no disk {index}"));
        };
        if let Some(instance) = label.strip_prefix("Panel ") {
            return Ok(Some(instance.to_string()));
        }
        if label == "Fail" {
            return Err("The probe's disk fails on purpose".to_string());
        }
        if label == "Same folder" {
            return self.same_folder(true);
        }
        self.tell_in("Disk", &format!("{index} {label}"));
        Ok(None)
    }

    fn run_command(&mut self, index: usize) -> Result<Option<String>, String> {
        let label = self.commands().get(index).cloned().unwrap_or_default();
        if let Some(instance) = label.strip_prefix("Panel ") {
            return Ok(Some(instance.to_string()));
        }
        if let Some(name) = label.strip_prefix("Unload ") {
            let unloaded = self.host.unload_plugin(name);
            let cancelled = self.host.cancelled();
            (self.host).log(&format!("unload {name} {unloaded} cancelled {cancelled}"));
            return Ok(None);
        }
        match label.as_str() {
            "Count" => return Ok(self.count()),
            "Stubborn" => return Ok(self.stubborn()),
            "Same folder" => return self.same_folder(false),
            "Report panels" => return self.panel_report(),
            "Run script" => return self.panel_script(),
            _ => {}
        }
        match index {
            0 => {
                if self.say_hello() {
                    Ok(None)
                } else {
                    Err("closed with Esc".to_string())
                }
            }
            1 => Err("The probe fails on purpose".to_string()),
            2 => panic!("the probe panics on purpose"),
            3..=10 if self.asked("windows") => {
                self.windows(index);
                Ok(None)
            }
            _ => panic!("the probe has no command {index}"),
        }
    }

    fn list(&mut self, instance: &str, path: &str) -> Result<Vec<FileEntry>, Error> {
        self.panels_in_list();
        self.panels.list(instance, path)
    }

    fn stat(&mut self, instance: &str, path: &str) -> Result<FileEntry, Error> {
        self.panels.stat(instance, path)
    }

    fn read(
        &mut self,
        instance: &str,
        path: &str,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize, Error> {
        self.panels.read(instance, path, offset, buffer)
    }

    fn write(
        &mut self,
        instance: &str,
        path: &str,
        mode: WriteMode,
        data: &[u8],
    ) -> Result<(), Error> {
        self.panels.write(instance, path, mode, data)?;
        if mode == WriteMode::New {
            self.host.log(&format!("made file {instance}:{path}"));
        }
        Ok(())
    }

    fn write_end(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        self.host.log(&format!("ended file {instance}:{path}"));
        Ok(())
    }

    fn write_abort(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        self.host.log(&format!("aborted file {instance}:{path}"));
        Ok(())
    }

    fn make_dir(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        if instance == "asking" {
            // A window in a panel's call: the program answers Esc at once (ADR-0016 §13).
            let shown = self.host.message("Probe", &["Make it?"]);
            self.host.log(&format!("asked making {path}: {shown}"));
        }
        self.panels.make_dir(instance, path)?;
        self.host.log(&format!("made folder {instance}:{path}"));
        Ok(())
    }

    fn rename(&mut self, instance: &str, from: &str, to: &str, replace: bool) -> Result<(), Error> {
        self.panels.rename(instance, from, to, replace)?;
        self.host.log(&format!("renamed {instance}:{from} to {to}"));
        Ok(())
    }

    fn remove(&mut self, instance: &str, path: &str, folder: bool) -> Result<(), Error> {
        self.panels.remove(instance, path, folder)?;
        self.host.log(&format!("removed {instance}:{path}"));
        Ok(())
    }

    fn synchro(&mut self, param: u64) {
        self.host.log(&format!("synchro {param}"));
    }

    fn set_times(
        &mut self,
        instance: &str,
        path: &str,
        times: &nc_plugin_sdk::Times,
    ) -> Result<(), Error> {
        self.panels.set_times(instance, path, times)?;
        self.host.log(&format!("kept times {instance}:{path}"));
        Ok(())
    }

    fn panel_title(&self, instance: &str, path: &str) -> String {
        if instance == "untitled" {
            return String::new();
        }
        format!("Probe {instance}:{path}")
    }

    fn panel_modes(&self, instance: &str, _path: &str) -> Vec<Option<PanelMode>> {
        panel::modes(instance)
    }

    /// `ownreal` reads and deletes its real names itself (FAR's own `GetFiles` / `DeleteFiles`).
    fn panel_flags(&self, instance: &str) -> nc_plugin_sdk::PanelFlags {
        nc_plugin_sdk::PanelFlags {
            own_get: instance == "ownreal",
            own_delete: instance == "ownreal",
            ..nc_plugin_sdk::PanelFlags::default()
        }
    }

    fn panel_format(&self, instance: &str, _path: &str) -> String {
        if instance == "untitled" {
            return String::new();
        }
        "Probe".to_string()
    }

    fn panel_start_sort(
        &self,
        instance: &str,
        _path: &str,
    ) -> Option<(nc_plugin_sdk::SortBy, bool)> {
        (instance == "sorted").then_some((nc_plugin_sdk::SortBy::Size, true))
    }

    /// API 3: the info panel's "Plugin" section, a separator and the path.
    fn panel_info_lines(&self, instance: &str, path: &str) -> Vec<nc_plugin_sdk::InfoLine> {
        vec![
            nc_plugin_sdk::InfoLine::separator(format!("Probe {instance}")),
            nc_plugin_sdk::InfoLine::new("Path", path),
        ]
    }

    fn panel_start_mode(&self, instance: &str, _path: &str) -> Option<usize> {
        match instance {
            "six" => Some(6),
            "seven" => Some(7),
            _ => None,
        }
    }

    fn close_panel(&mut self, instance: &str) {
        self.host.log(&format!("closed {instance}"));
    }
}

export_plugin!(Probe);
