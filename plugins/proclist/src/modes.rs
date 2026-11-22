//! FAR's proclist panel modes (`Plist::InitializePanelModes`, `TranslateMode`, `generate_titles`): ten
//! modes in the plugin's own column types, each turned into the program's: `X<what>` (a process's
//! number or counter) becomes the next custom column `C<n>`, `Z<what>` the description `Z` filled with
//! the full path, the window's title, the file's description or the command line; the other types stay
//! as they are. The modes' columns, widths and full screen are read from the settings (`Mode<n>`,
//! FAR's names) and written back by the configuration.
//!
//! Two types more than FAR's: `O`, the owner, which FAR leaves to its own panel's owner column (the
//! program draws no owner column for a plugin's entries), is a custom column of the plugin's here; `XN`,
//! the friendly name with the tree's indents (owner, 2026-11-15), stands for FAR's `N` in every mode:
//! the entry's own name (`svchost.exe 1234`) is the status line's.

use crate::counters::{self, COUNT};

/// The number of modes (FAR's `NPANELMODES`).
pub const MODES: usize = 10;

/// At most this many custom columns in a mode (FAR's `MAX_CUSTOM_COLS`).
pub const MOST_CUSTOM: usize = 20;

/// A mode in the plugin's own types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mode {
    pub columns: String,
    pub widths: String,
    pub full_screen: bool,
}

fn mode(columns: &str, widths: &str, full_screen: bool) -> Mode {
    Mode {
        columns: columns.to_string(),
        widths: widths.to_string(),
        full_screen,
    }
}

/// FAR's local modes (`m_PanelModesDataLocal`).
pub fn defaults() -> Vec<Mode> {
    vec![
        mode("XN,X15F,X16F,X17F,X18S", "12,0,0,0,4", false), // I/O
        mode("XN,XI,XB,XP,X0S,X6F", "0,6,2,2,3,9", false),   // General info
        mode("XN,XN", "0,0", false),                         // Names only
        mode("XN,XI,XB,XC,D,T", "0,6,2,6,0,0", false),       // Startup: PID/Date/Time
        mode("XN,XI,X4F,X6F", "0,6,9,9", false),             // Memory (basic)
        mode(
            "XN,XI,X4F,X6F,X10F,X12F,X0,X1,X2",
            "0,6,9,9,9,9,8,8,8",
            true,
        ), // Extended Memory/Time
        mode("XN,ZD", "12,0", false),                        // Descriptions
        mode(
            "XN,XP,X0S,X1S,X2S,X11S,X14FS,X18S",
            "0,2,3,2,2,6,6,4",
            false,
        ), // Dynamic Performance
        mode("XN,XI,O", "0,6,15", false),                    // Owners
        mode("XN,XI,XT,X3,XG,XU", "0,6,4,5,4,4", false),     // Resources
    ]
}

/// What a custom column shows of a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The friendly name, indented by the tree (`XN`).
    Friendly,
    /// Its base priority (`XP`).
    Priority,
    /// `XI`.
    Pid,
    /// The process that started it (`XC`).
    Parent,
    /// `XT`.
    Threads,
    /// 32 or 64 (`XB`).
    Bits,
    /// `XG`.
    Gdi,
    /// `XU`.
    User,
    /// `domain\user:session` (`O`).
    Owner,
    /// A counter (`X<n>`): per second (`S`, for a time or a count), units of 1000 (`T`), the float style
    /// (`F`).
    Counter {
        index: usize,
        per_second: bool,
        thousand: bool,
        float: bool,
    },
}

/// What the description `Z` shows (`ZP`, `ZW`, `ZD`, `ZC`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Described {
    Path,
    Window,
    FileDescription,
    CommandLine,
}

/// A mode in the program's types: its columns, the custom ones' sources in their order (`C0`, `C1`...)
/// with the place of each among the columns (its width's), what `Z` shows, the columns' titles.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Translated {
    pub columns: String,
    pub custom: Vec<(Source, usize)>,
    pub described: Option<Described>,
    pub titles: Vec<Option<String>>,
}

/// The letter after `X` FAR names a process's number by.
fn number(letter: char) -> Option<Source> {
    Some(match letter {
        'N' => Source::Friendly,
        'P' => Source::Priority,
        'I' => Source::Pid,
        'C' => Source::Parent,
        'T' => Source::Threads,
        'B' => Source::Bits,
        'G' => Source::Gdi,
        'U' => Source::User,
        _ => return None,
    })
}

/// `X<rest>`'s source: a number's letter, or a counter's digits and its modifiers.
fn x_source(rest: &str) -> Option<Source> {
    let mut letters = rest.chars();
    let first = letters.next()?.to_ascii_uppercase();
    if let Some(source) = number(first) {
        return Some(source);
    }
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let index: usize = digits.parse().ok()?;
    let has = |letter: char| rest.chars().any(|one| one.eq_ignore_ascii_case(&letter));
    Some(Source::Counter {
        index,
        per_second: has('S') && counters::per_second(index),
        thousand: has('T'),
        float: has('F'),
    })
}

fn described(letter: char) -> Option<Described> {
    Some(match letter.to_ascii_uppercase() {
        'P' => Described::Path,
        'W' => Described::Window,
        'D' => Described::FileDescription,
        'C' => Described::CommandLine,
        _ => return None,
    })
}

/// One column type turned into the program's: the type, and its source when it is a custom one.
enum Turned {
    Plain(String),
    Custom(Source),
    Description(Described),
}

fn turn(column: &str) -> Option<Turned> {
    let mut letters = column.chars();
    match letters.next().map(|first| first.to_ascii_uppercase()) {
        Some('Z') => {
            let kind = described(letters.next()?)?;
            // FAR takes nothing after `Z`'s letter.
            letters
                .next()
                .is_none()
                .then_some(Turned::Description(kind))
        }
        Some('X') => x_source(letters.as_str()).map(Turned::Custom),
        Some('O') if column.len() == 1 => Some(Turned::Custom(Source::Owner)),
        _ => Some(Turned::Plain(column.to_string())),
    }
}

/// A column's title (FAR's `generate_titles`); none: the program's.
fn title(turned: &Turned) -> Option<String> {
    let text = match turned {
        Turned::Plain(column) if column.eq_ignore_ascii_case("N") => "Module",
        Turned::Plain(_) => return None,
        Turned::Description(kind) => match kind {
            Described::Path => "Full path",
            Described::Window => "Window title",
            Described::FileDescription => "Description",
            Described::CommandLine => "Command Line",
        },
        Turned::Custom(source) => return Some(source_title(*source)),
    };
    Some(text.to_string())
}

fn source_title(source: Source) -> String {
    let text = match source {
        Source::Friendly => "Module",
        Source::Priority => "Priority",
        Source::Pid => "PID",
        Source::Parent => "PtID",
        Source::Threads => "Threads",
        Source::Bits => "Bt",
        Source::Gdi => "GDI",
        Source::User => "USER",
        Source::Owner => "Owner",
        Source::Counter {
            index, per_second, ..
        } => {
            return counters::COUNTERS
                .get(index)
                .map_or_else(String::new, |counter| {
                    if per_second {
                        counter.per_second_column.to_string()
                    } else {
                        counter.column.to_string()
                    }
                });
        }
    };
    text.to_string()
}

/// `columns` in the program's types (FAR's `TranslateMode`); none for a mode FAR refuses (a second `Z`,
/// `Z` with an unknown letter, `X` with neither a letter nor digits, a counter beyond FAR's).
pub fn translate(columns: &str) -> Option<Translated> {
    translate_sized(columns, "")
}

/// `translate` with the mode's `widths`: a source again shares its column only at the same width (its
/// text is made to the first one's).
pub fn translate_sized(columns: &str, widths: &str) -> Option<Translated> {
    let width = |place: usize| widths.split(',').nth(place).map_or("", str::trim);
    let mut translated = Translated::default();
    let mut types = Vec::new();
    for (place, column) in columns.split(',').enumerate() {
        let turned = turn(column)?;
        translated.titles.push(title(&turned));
        match turned {
            Turned::Plain(column) => types.push(column),
            Turned::Description(kind) => {
                if translated.described.replace(kind).is_some() {
                    return None;
                }
                types.push("Z".to_string());
            }
            Turned::Custom(source) => {
                if matches!(source, Source::Counter { index, .. } if index >= COUNT) {
                    return None;
                }
                // A source again shares its column (FAR's stripes: "XN,XN" is two of names).
                let index = (translated.custom.iter())
                    .position(|(known, at)| *known == source && width(*at) == width(place))
                    .unwrap_or_else(|| {
                        translated.custom.push((source, place));
                        translated.custom.len() - 1
                    });
                types.push(format!("C{index}"));
            }
        }
    }
    translated.custom.truncate(MOST_CUSTOM);
    translated.columns = types.join(",");
    Some(translated)
}

/// The settings' names of a mode's values (FAR's `Mode<n>` key).
fn key(index: usize, name: &str) -> String {
    format!("Mode{index}.{name}")
}

/// The modes as the settings have them (FAR's `InitializePanelModes`), the defaults where they have none.
pub fn load(setting: impl Fn(&str) -> Option<String>) -> Vec<Mode> {
    let flag = |text: String| !matches!(text.trim(), "0" | "");
    (defaults().into_iter().enumerate())
        .map(|(index, default)| Mode {
            columns: setting(&key(index, "ColumnsLocal")).unwrap_or(default.columns),
            widths: setting(&key(index, "WidthsLocal")).unwrap_or(default.widths),
            full_screen: setting(&key(index, "FullScreenLocal")).map_or(default.full_screen, flag),
        })
        .collect()
}

/// The modes written to the settings (FAR's `SavePanelModes`).
pub fn save(modes: &[Mode], mut set: impl FnMut(&str, &str)) {
    for (index, mode) in modes.iter().enumerate() {
        set(
            &key(index, "FullScreenLocal"),
            if mode.full_screen { "1" } else { "0" },
        );
        set(&key(index, "ColumnsLocal"), &mode.columns);
        set(&key(index, "WidthsLocal"), &mode.widths);
    }
}

#[cfg(test)]
#[path = "modes_tests.rs"]
mod tests;
