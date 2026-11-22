use std::path::PathBuf;

use crate::jobs::transfer::Request;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::clipboard::Clipboard;
use super::command::Side;
use super::drive_actions::{DriveAction, MenuPlace};
use super::form::{Form, FormOutcome};
use super::line_edit;
use super::line_input::LineInput;
use crate::jobs::tasks::TaskId;
use crate::jobs::transfer::Reply;
use crate::terminal::Shell;
use crate::vfs::VfsPath;

/// What happens when a confirmation dialog's first buttons are pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    Quit,
    /// FAR's "Save setup" question (`Options::Save`, R7 review D9): Save writes the settings.
    SaveSetup,
    /// Shift+F4's question about a folder not there (FAR's `MEditNewPath`): Yes edits the file new,
    /// read in the code page (none: the settings').
    EditNewPath(PathBuf, Option<u32>),
    /// Ctrl+W's Del: Kill ends this process.
    KillTask(u32),
    /// Buttons: Save, Discard, Cancel; `EditorToViewer` the same at F6.
    CloseEditor,
    EditorToViewer,
    /// The viewer's byte editing left with bytes typed (ADR-0027). Buttons: Save, Discard, Cancel.
    ViewerBytes,
    /// FAR's "is a read-only file. Do you wish to edit it?": Yes opens the editor on the file.
    EditReadOnly {
        path: PathBuf,
        remember: bool,
        place: (usize, usize),
    },
    /// FAR's "already loaded. How to open this file?" about the editor window `window` on `path`:
    /// Current / New instance / Reload / Cancel (`modal`: New instance / Cancel).
    Reedit {
        path: PathBuf,
        remember: bool,
        place: (usize, usize),
        window: u64,
        modal: bool,
    },
    /// A question of FAR's before the editor saves (`editor_save`).
    EditorSave(Box<super::editor_save::PendingSave>),
    /// Esc during a copy or move: Yes stops that task, No goes on.
    StopTransfer(TaskId),
    /// Esc while a folder is read: Yes shows what was read so far, No goes on.
    StopReading,
    /// Esc while Find file searches: Yes stops the search, No goes on.
    StopFind,
    /// F10 while tasks run: Wait (quit when they end), Cancel all, Back.
    QuitWithTasks,
    /// Tasks cancelled at F10 that do not stop (review R12): Wait, Quit anyway.
    QuitNotStopping,
    /// The drive menu's questions and its Retry / Cancel: the first button does it, then the menu.
    Drive(DriveAction, MenuPlace),
    /// A message of the drive menu: the menu comes back.
    BackToDriveMenu(MenuPlace),
    /// The report of a copy or move: the second button tries again what errors left out.
    RetrySkipped(Box<Request>),
    /// F7 with the rights missing: the first button makes the folder through the helper, the second
    /// shows why it could not be made.
    MakeDirElevated {
        parent: PathBuf,
        name: String,
        error: String,
    },
    /// Del in a history menu: the first button takes out what is not locked.
    ClearHistory(crate::settings::history::List),
    /// Del in the user's shells: the first button removes the one at this place.
    RemoveUserShell(usize),
    /// Ins in the user menu: a command (first button) or a menu.
    InsertMenuItem,
    /// Del in the user menu: the first button takes the item out.
    RemoveMenuItem,
    /// "Save changes?" of an item's dialog: Yes, No, Cancel.
    SaveMenuItem,
    /// Commands > Edit user menu: Main, Local, Cancel.
    ChooseUserMenu,
    /// Del in the file associations: the first button takes out the one at this place.
    RemoveAssociation(usize),
    /// Del in the filters menu: the first button takes out the user's filter at this place.
    RemoveFilter(usize),
    /// Del in the groups of file masks: the first button takes out the one at this place.
    RemoveMaskGroup(usize),
    /// Ctrl+R in the groups of file masks: the first button puts the default ones back.
    RestoreMaskGroups,
    /// F4 on a folder shortcut whose folder is not there: the first button keeps it as it is now.
    SaveShortcut {
        list: usize,
        at: usize,
        shortcut: Box<crate::settings::FolderShortcut>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    /// Ctrl+G: the command for each selected file.
    ApplyCommand,
    /// Shift+F6 in the drive menu.
    VolumeLabel { root: String, place: MenuPlace },
    /// Options > Folder description files.
    DescriptionFiles,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListAction {
    Shell(Vec<Shell>),
    /// Options > Themes: moving over a theme shows it, Enter keeps it.
    Theme,
    /// F12: the panels (`None`) and the tasks; Del removes an ended task.
    Screens(Vec<super::screens::ScreenEntry>),
    /// The user's shells: Ins adds (a button past the end), F4 changes, Del removes (a button).
    UserShells,
    /// Options > Plugins (P6.0e): each line's plugin and how it is; Enter or Space loads or unloads it,
    /// Ctrl+R starts it anew.
    Plugins(Vec<PluginRow>),
    /// FAR's "Select association": the commands, and the key the file was opened with.
    Association(
        Vec<String>,
        super::associations::AssocMode,
        Box<super::associations::AssocTarget>,
    ),
}

/// A line of Options > Plugins: the plugin's name and how it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRow {
    pub name: String,
    pub state: PluginState,
}

/// How a plugin is, as Options > Plugins says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    Loaded,
    Starting,
    /// Turned off (in the list, or `plugins.<name>.enabled` false in the settings).
    Disabled,
    /// Not loaded otherwise: its start failed, or it is new.
    NotLoaded,
}

impl PluginState {
    /// The word the list shows.
    pub fn word(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::Starting => "starting",
            Self::Disabled => "disabled",
            Self::NotLoaded => "not loaded",
        }
    }

    /// The word in brackets, centred in the longest one's width (`[  loaded  ]`): one even column.
    pub fn shown(self) -> String {
        format!("[{:^10}]", self.word())
    }
}

/// What the window of F3's count says (FAR's `dirinfo_progress`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderCount {
    pub folder: String,
    pub items: u64,
    pub bytes: u64,
}

/// A work's window (`waiting`): what the work is, how it goes, what Esc does - laid out as the copy
/// dialog is (the lines centred, the bar as wide as the window between blank rows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitWindow {
    pub lines: Vec<String>,
    pub bar: Option<WaitBar>,
    /// Esc's line, or the question when the work does not answer.
    pub tail: Vec<String>,
}

/// How a work goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitBar {
    /// FAR's bar with the percent (the editor's save).
    Percent(u64),
    /// No amount known: a block going to and fro, and the time taken.
    Busy(std::time::Duration),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    Message {
        title: String,
        lines: Vec<String>,
        error: bool,
    },
    /// F3's count: no buttons, Esc breaks it (`panel_sizes`).
    Counting(FolderCount),
    Confirm {
        title: String,
        lines: Vec<String>,
        buttons: Vec<&'static str>,
        focused: usize,
        danger: bool,
        action: ConfirmAction,
    },
    Input {
        title: String,
        prompt: String,
        input: LineInput,
        action: InputAction,
    },
    List {
        title: String,
        items: Vec<String>,
        cursor: usize,
        /// The first line shown, kept by the drawing (FAR's `TopPos`).
        top: std::cell::Cell<usize>,
        action: ListAction,
    },
    /// FAR's dialog with items (`app::form`).
    Form { form: Form, action: FormAction },
}

/// What a form's buttons do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormAction {
    /// Shift+F4's "Editor" (FAR's `dlgOpenEditor`).
    EditNew,
    /// The editor's Shift+F2: "Save file as" (FAR's `dlgSaveFileAs`).
    EditorSaveAs,
    /// F7's "Make folder" (FAR's `ShellMakeDir`), in the tree's folder when given.
    MakeDir(Option<PathBuf>),
    /// The code page menu's F4: "Rename the code page" of this one.
    CodePageName(u32),
    /// FarColorer's type menu's F4: "Define key" of this type.
    SyntaxHotkey(String),
    /// The viewer's Alt+F8: "Go to".
    ViewerGoTo,
    /// The editor's Alt+F8: "Go to".
    EditorGoTo,
    /// The editor's F7: "Search", Text / Hex turned through this code page.
    EditorSearch,
    /// The editor's search reached an end: go on from the other one to `start`?
    EditorSearchWrap { backward: bool, start: u64 },
    /// The editor's Ctrl+F7: "Replace".
    EditorReplace,
    /// FAR's "Replace "x" with "y"" of the match at `at`, `size` bytes.
    EditorReplaceAsk { at: u64, size: u64, backward: bool },
    /// The viewer's F7: "Search", Text / Hex turned through this code page.
    ViewerSearch(u32),
    /// The search reached an end: go on from the other one to `start`?
    ViewerSearchWrap { backward: bool, start: u64 },
    /// F5 (copy) or F6 (move) of these items.
    Transfer {
        sources: Vec<VfsPath>,
        operation: super::transfer_dialog::Operation,
        /// The passive panel a plugin's: its target text, left as it is, copies into it (FAR's
        /// `ToPlugin`).
        to_plugin: Option<super::transfer_dialog::PluginTarget>,
    },
    /// A question of a running copy or move: each button's reply, and the "Remember choice" item.
    Answer {
        task: TaskId,
        replies: Vec<Reply>,
        remember: Option<usize>,
    },
    /// "Rename" over the question about an existing file: the name for the task (the question comes
    /// back when it is cancelled); a name not from a root is beside `target`.
    RenameAnswer { task: TaskId, target: VfsPath },
    /// FAR's question before deleting: the item index of its Delete (Move) button.
    Delete {
        request: crate::jobs::delete::Request,
        yes: usize,
    },
    /// A question of a running deletion: each button's reply.
    AnswerDelete {
        task: TaskId,
        replies: Vec<crate::jobs::delete::Reply>,
    },
    /// F9 in the drive menu: the menu comes back at this line afterwards.
    DriveColumns { side: Side, selected: usize },
    /// Ctrl+A: FAR's "Attributes" dialog and what it keeps between keys.
    Attributes(Box<super::attributes::Session>),
    /// Its "More" dialog; the main dialog waits under it.
    AttributesMore {
        session: Box<super::attributes::Session>,
        main: Box<Form>,
    },
    /// A change of a running "Set" failed: Retry, Skip, Skip all, Cancel.
    AnswerAttributes { task: TaskId },
    /// A page of the Options menu.
    Options(super::OptionsPage),
    /// One of the user's shells: the one at this place, or a new one.
    UserShell(Option<usize>),
    /// An item of the user menu.
    MenuItem(Box<super::user_menu_edit::ItemEdit>),
    /// The questions of a user menu command (`!?title?init!`).
    MenuInput(Box<super::metasymbols::Expanded>),
    /// A file association: the one at this place, or a new one before it.
    Association { at: usize, insert: bool },
    /// A group of file masks: the one at this place, or a new one.
    MaskGroup(Option<usize>),
    /// F7 in the groups of file masks: the name their masks must take.
    FindMaskGroup,
    /// A file panel mode's dialog.
    PanelMode(super::panel_modes_menu::ModeEdit),
    /// Alt+F7's dialog.
    FindFile,
    /// Its Advanced.
    FindAdvanced,
    /// Ins, F4, F5 in the filters menu: the filter's dialog and what it keeps.
    Filter(Box<super::filter_edit::FilterEdit>),
    /// Its attributes; the filter's dialog waits under it.
    FilterAttributes {
        edit: Box<super::filter_edit::FilterEdit>,
        main: Box<Form>,
    },
    /// F4 on shortcut `at` of the list of RCtrl+`list`; `raw`: opened from Commands > Folder shortcuts.
    FolderShortcut { list: usize, at: usize, raw: bool },
    /// A plugin's message.
    Plugin(super::plugins::PluginForm),
    /// F4 in F11's menu or the drive menu: FAR's "Assign plugin hot key" for this item.
    PluginHotkey(Box<super::plugins::HotkeyEdit>),
    /// F3 in F11's menu, "Plugins configuration" or the drive menu (of the side): FAR's "Plugin
    /// information"; the menu back at its line.
    PluginInfo(super::plugins::HotkeyMenu, usize, Side),
    /// Gray+ / Gray-: Select (`on`) or Deselect on the panel of `side` (`select_dialog`).
    SelectGroup { side: Side, on: bool },
    /// A drive that cannot be read: Retry or Cancel (`drive_retry`).
    DriveRetry { side: Side, root: String },
    /// Del on a network drive: the Yes button's item and the "Reconnect at logon" item.
    Disconnect {
        root: String,
        place: MenuPlace,
        yes: usize,
        reconnect: Option<usize>,
    },
    /// Ins on a network drive whose saved name or password was refused (`drive_login`).
    NetworkLogin {
        root: String,
        share: String,
        place: MenuPlace,
    },
}

impl Dialog {
    pub fn message(title: &str, lines: Vec<String>) -> Self {
        Self::Message {
            title: title.to_string(),
            lines,
            error: false,
        }
    }

    pub fn error(title: &str, lines: Vec<String>) -> Self {
        Self::Message {
            title: title.to_string(),
            lines,
            error: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogOutcome {
    Stay,
    Cancel,
    /// Index of the pressed button (confirmation dialogs).
    Button(usize),
    /// Text entered in an input dialog.
    Submit(String),
    /// Index of the chosen list item.
    Choose(usize),
}

pub fn handle_key(
    dialog: &mut Dialog,
    key: KeyEvent,
    clipboard: &mut dyn Clipboard,
) -> DialogOutcome {
    // A form's Esc may only close its open list.
    if key.code == KeyCode::Esc && !matches!(dialog, Dialog::Form { .. }) {
        return DialogOutcome::Cancel;
    }
    // A hotkey of the Screens list brings its line to the front (FAR's `VMenu`).
    if let Dialog::List {
        items,
        action: ListAction::Screens(_),
        ..
    } = dialog
        && let Some(line) = super::screens::hotkey_line(key).filter(|&line| line < items.len())
    {
        return DialogOutcome::Choose(line);
    }
    match dialog {
        // The count has ended and its window is not taken away yet.
        Dialog::Counting(_) => DialogOutcome::Cancel,
        Dialog::Message { .. } => match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => DialogOutcome::Cancel,
            _ => DialogOutcome::Stay,
        },
        Dialog::Confirm {
            buttons, focused, ..
        } => confirm_key(buttons, focused, key),
        Dialog::Input { input, .. } => input_key(input, key, clipboard),
        Dialog::List {
            cursor,
            action: ListAction::Screens(_),
            ..
        } if key.code == KeyCode::Delete => DialogOutcome::Button(*cursor),
        Dialog::List {
            items,
            cursor,
            action: ListAction::UserShells,
            ..
        } if matches!(key.code, KeyCode::Insert | KeyCode::Delete | KeyCode::F(4)) => {
            match key.code {
                KeyCode::Insert => DialogOutcome::Button(items.len()),
                _ if items.is_empty() => DialogOutcome::Stay,
                KeyCode::Delete => DialogOutcome::Button(*cursor),
                _ => DialogOutcome::Choose(*cursor),
            }
        }
        Dialog::List {
            items,
            cursor,
            action: ListAction::Plugins(_),
            ..
        } if !items.is_empty() && plugins_key(key).is_some() => {
            plugins_key(key).map_or(DialogOutcome::Stay, |restart| {
                if restart {
                    DialogOutcome::Button(*cursor)
                } else {
                    DialogOutcome::Choose(*cursor)
                }
            })
        }
        Dialog::List { items, cursor, .. } => list_key(items.len(), cursor, key),
        Dialog::Form { form, .. } => match form.handle_key(key, clipboard) {
            FormOutcome::Stay => DialogOutcome::Stay,
            FormOutcome::Cancel => DialogOutcome::Cancel,
            FormOutcome::Button(index) => DialogOutcome::Button(index),
        },
    }
}

/// A question's keys: the moves between its buttons, Enter, a button's `&` hotkey pressing it.
fn confirm_key(labels: &[&str], focused: &mut usize, key: KeyEvent) -> DialogOutcome {
    let buttons = labels.len();
    match key.code {
        KeyCode::Char(ch) if super::editing::is_text_input(key.modifiers) => {
            let ch = ch.to_lowercase().next().unwrap_or(ch);
            let hit = (labels.iter()).position(|label| super::form::hotkey_of(label) == Some(ch));
            if let Some(index) = hit {
                return DialogOutcome::Button(index);
            }
        }
        KeyCode::Left | KeyCode::BackTab => *focused = (*focused + buttons - 1) % buttons,
        KeyCode::Right | KeyCode::Tab => *focused = (*focused + 1) % buttons,
        KeyCode::Enter => return DialogOutcome::Button(*focused),
        _ => {}
    }
    DialogOutcome::Stay
}

/// FAR's dialog edit line: the keys of its `Edit`, Enter submits.
fn input_key(input: &mut LineInput, key: KeyEvent, clipboard: &mut dyn Clipboard) -> DialogOutcome {
    if key.code == KeyCode::Enter {
        return DialogOutcome::Submit(input.text().to_string());
    }
    // A clipboard another program holds for longer than FAR waits only loses this key.
    let _ = line_edit::handle_key(input, key, clipboard);
    DialogOutcome::Stay
}

/// Options > Plugins' own keys: Space loads or unloads (false), Ctrl+R starts anew (true).
fn plugins_key(key: KeyEvent) -> Option<bool> {
    match (key.code, key.modifiers) {
        (KeyCode::Char(' '), ratatui::crossterm::event::KeyModifiers::NONE) => Some(false),
        (KeyCode::Char('r'), ratatui::crossterm::event::KeyModifiers::CONTROL) => Some(true),
        _ => None,
    }
}

fn list_key(items: usize, cursor: &mut usize, key: KeyEvent) -> DialogOutcome {
    if items == 0 {
        return DialogOutcome::Stay;
    }
    match key.code {
        KeyCode::Up => *cursor = (*cursor + items - 1) % items,
        KeyCode::Down => *cursor = (*cursor + 1) % items,
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = items - 1,
        KeyCode::Enter => return DialogOutcome::Choose(*cursor),
        _ => {}
    }
    DialogOutcome::Stay
}

impl super::App {
    /// FAR's `ConfirmAbort` after Esc: "Do you really want to cancel it?", Yes doing `action`.
    pub(super) fn ask_interrupt(&mut self, action: ConfirmAction) {
        self.show_dialog(Dialog::Confirm {
            title: "Operation has been interrupted".to_string(),
            lines: vec!["Do you really want to cancel it?".to_string()],
            buttons: vec!["Yes", "No"],
            focused: 0,
            danger: true,
            action,
        });
    }
}

#[cfg(test)]
#[path = "dialog_tests.rs"]
mod tests;
