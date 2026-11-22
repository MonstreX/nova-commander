//! F1: the program's help lines in a message (FAR's help viewer is not there yet).

use super::App;
use super::dialog::Dialog;

const HELP_LINES: [&str; 17] = [
    "Tab            switch panel        Insert        select file",
    "Enter          open / run          Ctrl+Enter    name to command line",
    "Ctrl+PgUp      parent directory    Ctrl+PgDn     enter directory",
    "F3 / F4        view / edit         F5 / F6       copy / move",
    "F7 / F8        mkdir / delete      F9 / F10      menu / quit",
    "F2             user menu           Alt+F1/F2     change drive",
    "Ctrl+O         terminal on/off     Ctrl+U        swap panels",
    "Ctrl+Up/Down   panel height        Ctrl+R        re-read panel",
    "Ctrl+F3..F9    sort (again: reverse)  Ctrl+F12 sort menu  Ctrl+1..5,0 views",
    "Ctrl+E/Ctrl+X  command history     Ctrl+C        interrupt shell",
    "",
    "Command line: commands run in the active panel's directory; while a",
    "command runs its output is shown and keys go to it.",
    "`cd <dir>` and `D:` change the panel's directory.",
    "When a program asks something (Y/N, input), the text goes to it as typed.",
    "Shift+Enter always sends the line exactly as typed.",
    "Editor: F2 or Ctrl+S save, Esc close. Viewer: Esc close.",
];

impl App {
    /// F1: the program's help lines.
    pub(super) fn show_help(&mut self) {
        let lines = HELP_LINES.iter().map(|line| (*line).to_string()).collect();
        self.show_dialog(Dialog::message("Help", lines));
    }
}
