//! The text FAR's proclist writes of a process (`Plist::GetFiles`, `DumpNTCounters`, `PrintModules`,
//! `PrintHandleInfo`, `PrintNTCurDirAndEnv`): its module, path and version, numbers, owner, times, command
//! line, current folder and environment, windows' objects, counters, window, modules and handles, each
//! titled as FAR titles it; made of what [`Facts`] gathered, written as FAR writes it (UTF-16 with its
//! byte order mark).

use nc_plugin_sdk::system::{FileVersion, LocalTime, Module, Owner, ProcessMemory};

use crate::columns::{duration, size};
use crate::counters::{COUNTERS, Kind, Rate};
use crate::options::Options;
use crate::table::Process;

/// A window of the process's, as the text tells it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowFacts {
    pub title: String,
    pub handle: usize,
    pub style: u32,
    pub extended: u32,
}

/// A handle as the text tells it: its value, access, type and its object's name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandleLine {
    pub value: u64,
    pub access: u32,
    pub type_name: String,
    pub name: String,
}

/// Everything the text is made of, gathered by the caller.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub process: Process,
    pub parent_name: Option<String>,
    pub version: Option<FileVersion>,
    pub owner: Option<Owner>,
    /// The start in the local time and now's, the time gone by since in 100 ns units.
    pub started: Option<(LocalTime, LocalTime, u64)>,
    /// What its memory told; none when it could not be opened (FAR's `if (Process)`).
    pub memory: Option<ProcessMemory>,
    pub window: Option<WindowFacts>,
    /// Each module with its version and description when asked.
    pub modules: Option<Vec<(Module, Option<FileVersion>)>>,
    pub handles: Option<Vec<HandleLine>>,
    pub decimal: char,
}

/// FAR's `PrintTitle`: the title, `:`, padded to 30 and a space.
pub fn title(text: &str) -> String {
    let pad = 30usize.saturating_sub(text.chars().count());
    format!("{text}:{:pad$} ", "")
}

fn line(text: &mut String, name: &str, value: &str) {
    text.push_str(&title(name));
    text.push_str(value);
    text.push('\n');
}

/// The module's line, path and version (FAR's first lines).
fn head(text: &mut String, facts: &Facts) {
    let process = &facts.process;
    match process.bits {
        Some(bits) => line(text, "Module", &format!("{}, {bits}-bit", process.name)),
        None => line(text, "Module", &process.name),
    }
    if process.is_total() {
        return;
    }
    if let Some(path) = process.path.as_deref().filter(|path| !path.is_empty()) {
        line(text, "Full path", path);
        if let Some(version) = &facts.version {
            if let Some(number) = &version.version {
                line(text, "File version", number);
            }
            if let Some(description) = &version.description {
                line(text, "Description", description);
            }
        }
    }
}

/// The numbers, the owner and the session.
fn numbers(text: &mut String, facts: &Facts) {
    let process = &facts.process;
    if !process.is_total() {
        line(text, "PID", &process.pid.to_string());
        let parent = match &facts.parent_name {
            Some(name) if process.parent != 0 => format!("{} ({name})", process.parent),
            _ => process.parent.to_string(),
        };
        line(text, "Parent PID", &parent);
        line(text, "Priority", &process.priority.to_string());
    }
    line(text, "Threads", &process.threads.to_string());
    if process.is_total() || process.pid == 0 {
        return;
    }
    if let Some(owner) = &facts.owner {
        let domain = if owner.domain.is_empty() {
            String::new()
        } else {
            format!("{}\\", owner.domain)
        };
        let sid = if owner.sid.is_empty() {
            String::new()
        } else {
            format!(" ({})", owner.sid)
        };
        line(text, "Owner", &format!("{domain}{}{sid}", owner.user));
    }
    // The session is known of every process on this computer (FAR's `SessionId` from the system).
    line(text, "Session", &process.session.to_string());
}

/// "Started at" (the date only on another day than today's) and "Uptime" right under its end.
fn times(text: &mut String, facts: &Facts) {
    let Some((start, now, gone)) = &facts.started else {
        return;
    };
    let ticks = facts.process.created.rem_euclid(10_000_000);
    let same_day = (start.year, start.month, start.day) == (now.year, now.month, now.day);
    let shown = if same_day {
        format!("{}.{ticks:07}", start.time)
    } else {
        format!("{} {}.{ticks:07}", start.date, start.time)
    };
    text.push('\n');
    line(text, "Started at", &shown);
    let width = shown.chars().count();
    line(text, "Uptime", &format!("{:>width$}", duration(*gone)));
}

/// The command line, the current folder, the environment and the windows' objects.
fn parameters(text: &mut String, facts: &Facts, options: &Options) {
    let process = &facts.process;
    if let Some(command) = process
        .command_line
        .as_deref()
        .filter(|line| !line.is_empty())
    {
        text.push_str(&format!("\nCommand Line:\n{command}\n"));
    }
    let Some(memory) = &facts.memory else {
        return;
    };
    text.push('\n');
    if let Some(folder) = &memory.current_folder {
        text.push_str(&format!("Current Directory:\n{folder}\n"));
    }
    if options.export_environment && !memory.environment.is_empty() {
        text.push_str("\nEnvironment:\n");
        for variable in &memory.environment {
            text.push_str(variable);
            text.push('\n');
        }
    }
    if process.gdi != 0 {
        text.push('\n');
        line(text, "GDI Objects", &process.gdi.to_string());
    }
    if process.user != 0 {
        line(text, "USER Objects", &process.user.to_string());
    }
}

/// The counters (FAR's `DumpNTCounters`): as they are, a time with its percent, a count with its rate.
fn counters(text: &mut String, facts: &Facts) {
    let process = &facts.process;
    let decimal = facts.decimal;
    text.push('\n');
    for (index, counter) in COUNTERS.iter().enumerate() {
        text.push_str(&title(counter.name));
        let number = counter.kind == Kind::Number;
        let shown = |value: u64| size(value, 0, number, decimal);
        let (now, result) = (process.counters[index], process.results[index]);
        let value = match counter.rate {
            Rate::Raw => format!("{:>27}", shown(result)),
            Rate::Timer => format!("{:>27} {result:7}%", duration(now)),
            Rate::PerSecond => format!("{:>27} {:>6}/s", shown(now), shown(result)),
        };
        text.push_str(&value);
        text.push('\n');
    }
}

/// FAR's names of the window styles it knows (`window_style`, `window_ex_style`).
const STYLES: [(u32, &str); 16] = [
    (0x8000_0000, "WS_POPUP"),
    (0x4000_0000, "WS_CHILD"),
    (0x2000_0000, "WS_MINIMIZE"),
    (0x1000_0000, "WS_VISIBLE"),
    (0x0800_0000, "WS_DISABLED"),
    (0x0400_0000, "WS_CLIPSIBLINGS"),
    (0x0200_0000, "WS_CLIPCHILDREN"),
    (0x0100_0000, "WS_MAXIMIZE"),
    (0x0080_0000, "WS_BORDER"),
    (0x0040_0000, "WS_DLGFRAME"),
    (0x0020_0000, "WS_VSCROLL"),
    (0x0010_0000, "WS_HSCROLL"),
    (0x0008_0000, "WS_SYSMENU"),
    (0x0004_0000, "WS_THICKFRAME"),
    (0x0002_0000, "WS_MINIMIZEBOX"),
    (0x0001_0000, "WS_MAXIMIZEBOX"),
];

const EXTENDED_STYLES: [(u32, &str); 22] = [
    (0x0000_0001, "WS_EX_DLGMODALFRAME"),
    (0x0000_0004, "WS_EX_NOPARENTNOTIFY"),
    (0x0000_0008, "WS_EX_TOPMOST"),
    (0x0000_0010, "WS_EX_ACCEPTFILES"),
    (0x0000_0020, "WS_EX_TRANSPARENT"),
    (0x0000_0040, "WS_EX_MDICHILD"),
    (0x0000_0080, "WS_EX_TOOLWINDOW"),
    (0x0000_0100, "WS_EX_WINDOWEDGE"),
    (0x0000_0200, "WS_EX_CLIENTEDGE"),
    (0x0000_0400, "WS_EX_CONTEXTHELP"),
    (0x0000_1000, "WS_EX_RIGHT"),
    (0x0000_2000, "WS_EX_RTLREADING"),
    (0x0000_4000, "WS_EX_LEFTSCROLLBAR"),
    (0x0001_0000, "WS_EX_CONTROLPARENT"),
    (0x0002_0000, "WS_EX_STATICEDGE"),
    (0x0004_0000, "WS_EX_APPWINDOW"),
    (0x0008_0000, "WS_EX_LAYERED"),
    (0x0010_0000, "WS_EX_NOINHERITLAYOUT"),
    (0x0020_0000, "WS_EX_NOREDIRECTIONBITMAP"),
    (0x0040_0000, "WS_EX_LAYOUTRTL"),
    (0x0200_0000, "WS_EX_COMPOSITED"),
    (0x0800_0000, "WS_EX_NOACTIVATE"),
];

/// The style's bits, then each known one's name and the rest in hex, each after a space (FAR's
/// `{:08X} {}` with its `StyleStr`: two spaces before the first, one after the bits of none).
pub fn style_text(style: u32, names: &[(u32, &str)]) -> String {
    let mut text = format!("{style:08X} ");
    let mut seen = 0;
    for (bit, name) in names {
        if style & bit != 0 {
            text.push(' ');
            text.push_str(name);
            seen |= bit;
        }
    }
    let unseen = style & !seen;
    if unseen != 0 {
        text.push_str(&format!(" 0x{unseen:08X}"));
    }
    text
}

fn window(text: &mut String, facts: &Facts) {
    let Some(window) = &facts.window else {
        return;
    };
    text.push('\n');
    line(text, "Window title", &window.title);
    line(text, "HWND", &format!("{:#x}", window.handle));
    let styles = style_text(window.style, &STYLES);
    line(text, "Window style", &styles);
    let extended = style_text(window.extended, &EXTENDED_STYLES);
    line(text, "Extended style", &extended);
}

/// FAR's `PrintModuleVersion`: tabs to column 56, the version, spaces to column 72, the description.
fn module_version(text: &mut String, mut length: usize, version: &FileVersion) {
    loop {
        text.push('\t');
        length = (length | 7) + 1;
        if length >= 56 {
            break;
        }
    }
    let number = version.version.as_deref().unwrap_or("");
    text.push_str(number);
    length += number.chars().count();
    if let Some(description) = &version.description {
        loop {
            text.push(' ');
            let before = length;
            length += 1;
            if before >= 72 {
                break;
            }
        }
        text.push_str(description);
    }
}

fn modules(text: &mut String, facts: &Facts, options: &Options) {
    let Some(modules) = &facts.modules else {
        return;
    };
    let digits = if facts.process.bits == Some(32) {
        8
    } else {
        16
    };
    let path_title = if options.export_module_version {
        "Path, version, description"
    } else {
        "Path (version info is not displayed)"
    };
    text.push_str(&format!(
        "\nModules:\n{:<digits$} {:<8} {path_title}\n",
        "Base", "Size"
    ));
    for (module, version) in modules {
        let mut entry = format!("{:0digits$X} {:8X}", module.base, module.size);
        if !module.path.is_empty() {
            entry.push(' ');
            entry.push_str(&module.path);
        }
        let length = entry.chars().count();
        text.push_str(&entry);
        if let Some(version) = version.as_ref().filter(|_| options.export_module_version) {
            module_version(text, length, version);
        }
        text.push('\n');
    }
}

fn handles(text: &mut String, facts: &Facts, options: &Options) {
    let Some(handles) = &facts.handles else {
        return;
    };
    text.push_str(&format!(
        "\nHandles:\n{:6} {:8} {:15} Name\n",
        "Handle", "Access", "Type"
    ));
    for handle in handles {
        if handle.name.is_empty() && !options.export_handles_unnamed {
            continue;
        }
        text.push_str(&format!(
            "{:6X} {:08X} {:15} {}\n",
            handle.value, handle.access, handle.type_name, handle.name
        ));
    }
}

/// The whole text, as FAR's `GetFiles` writes it with `options`.
pub fn text(facts: &Facts, options: &Options) -> String {
    let mut text = String::new();
    head(&mut text, facts);
    numbers(&mut text, facts);
    times(&mut text, facts);
    parameters(&mut text, facts, options);
    if options.export_performance {
        counters(&mut text, facts);
    }
    window(&mut text, facts);
    modules(&mut text, facts, options);
    handles(&mut text, facts, options);
    text
}

/// The text's bytes as FAR writes its file: UTF-16, little end first, after the byte order mark.
pub fn bytes(text: &str) -> Vec<u8> {
    std::iter::once(0xFEFF_u16)
        .chain(text.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// FAR's file name of a process's text: `<name>.txt`, the characters a file name may not have made `_`.
pub fn file_name(name: &str) -> String {
    const INVALID: &str = ":*?\\/\"<>;|";
    let safe: String = (name.chars())
        .map(|letter| {
            if INVALID.contains(letter) || letter < ' ' {
                '_'
            } else {
                letter
            }
        })
        .collect();
    format!("{safe}.txt")
}

/// A registry key's object name as FAR shortens it (`GetNameByType`): `\REGISTRY\MACHINE` is HKLM, its
/// classes HKCR, `\REGISTRY\USER` HKU, the user's own (`sid`) HKCU and its classes `HKCU\Classes`.
pub fn registry_name(name: &str, sid: &str) -> String {
    let Some(rest) = name.strip_prefix("\\REGISTRY\\") else {
        return name.to_string();
    };
    if let Some(user) = rest.strip_prefix("USER") {
        let Some(user) = user.strip_prefix('\\') else {
            return format!("HKU{user}");
        };
        if let Some(own) = user.strip_prefix(sid).filter(|_| !sid.is_empty()) {
            return match own.strip_prefix("_CLASSES") {
                Some(classes) => format!("HKCU\\Classes{classes}"),
                None => format!("HKCU{own}"),
            };
        }
        return format!("HKU\\{user}");
    }
    if let Some(classes) = rest.strip_prefix("MACHINE\\SOFTWARE\\CLASSES") {
        return format!("HKCR{classes}");
    }
    match rest.strip_prefix("MACHINE") {
        Some(machine) => format!("HKLM{machine}"),
        None => name.to_string(),
    }
}

#[cfg(test)]
#[path = "info_tests.rs"]
mod tests;
