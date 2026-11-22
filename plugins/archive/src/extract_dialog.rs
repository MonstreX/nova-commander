//! arclite's "Extract files" dialog (`ExtractDialog`, ui.cpp) in its order and words: the folder (its
//! history `arclite.extract_dir`), Ignore errors, what to do with an existing file, deleting the files
//! from the archive or the archive itself, a separate folder, going there, the password; "Save settings"
//! keeps the answers without closing it.

use std::path::PathBuf;

use nc_plugin_sdk::{Closed, Controls, Dialog, DialogEvent, DialogItem, Host};

use crate::extract_options::{ExtractOptions, ExtractSettings, Overwrite, Tri};
use crate::layout::Layout;

pub const TITLE: &str = "Extract files";

/// The dialog's items by index (as `dialog` lays them out).
const DST_DIR: usize = 1;
const IGNORE_ERRORS: usize = 3;
/// The overwrite answers' radio buttons, in `ANSWERS`' order.
const FIRST_ANSWER: usize = 5;
const MOVE_FILES: usize = 10;
const DELETE_ARCHIVE: usize = 11;
const SEPARATE_DIR: usize = 12;
const OPEN_DIR: usize = 13;
const PASSWORD: usize = 15;
pub const OK: usize = 17;
const CANCEL: usize = 18;
const SAVE: usize = 19;

/// The radio buttons' answers in their order.
const ANSWERS: [Overwrite; 5] = [
    Overwrite::Ask,
    Overwrite::Overwrite,
    Overwrite::Skip,
    Overwrite::Rename,
    Overwrite::Append,
];

/// The folder as the line shows it: with its closing `\` (arclite's `add_trailing_slash`).
fn with_slash(folder: &str) -> String {
    if folder.is_empty() || folder.ends_with(['\\', '/']) {
        folder.to_string()
    } else {
        format!("{folder}\\")
    }
}

/// arclite's `ExtractDialog::show`.
pub fn dialog(options: &ExtractOptions) -> Dialog {
    let mut layout = Layout::new(TITLE, 60);
    layout.label("&Extract files to:");
    layout.new_line();
    let folder = with_slash(&options.dst_dir.to_string_lossy());
    let line = DialogItem::edit(&folder)
        .history("arclite.extract_dir", false)
        .edit_path();
    layout.edit(line, 60);
    layout.new_line();
    layout.separator();
    layout.new_line();
    layout.boxed(DialogItem::check("&Ignore errors", options.ignore_errors));
    layout.new_line();
    layout.label("When destination file exists:");
    layout.new_line();
    let labels = ["As&k", "&Overwrite", "&Skip", "&Rename", "&Append"];
    for (index, (label, answer)) in labels.iter().zip(ANSWERS).enumerate() {
        if index == 3 {
            layout.new_line();
        }
        layout.spacer(2);
        let on = options.overwrite == answer
            || (answer == Overwrite::Overwrite && options.overwrite == Overwrite::OverwriteCase);
        let radio = DialogItem::radio(label, on);
        layout.boxed(if index == 0 { radio.group() } else { radio });
    }
    layout.new_line();
    let moving = DialogItem::check3(
        "&Delete files from archive after extraction",
        options.move_files.check(),
    );
    layout.boxed(moving.disabled(options.move_files == Tri::Undef));
    layout.new_line();
    let delete = DialogItem::check("De&lete archive after extraction", options.delete_archive);
    layout.boxed(delete.disabled(options.disable_delete_archive));
    layout.new_line();
    let separate = DialogItem::check3(
        "Ex&tract archive into separate directory",
        options.separate_dir.check(),
    );
    layout.boxed(separate);
    layout.new_line();
    let open = DialogItem::check3(
        "&Go to target directory after extraction",
        options.open_dir.check(),
    );
    layout.boxed(open.disabled(options.open_dir == Tri::Undef));
    layout.new_line();
    layout.label("Pass&word:");
    layout.edit(DialogItem::password(&options.password), 20);
    layout.new_line();
    layout.separator();
    layout.new_line();
    layout.buttons(vec![
        DialogItem::button("OK").default_button(),
        DialogItem::button("Cancel"),
        DialogItem::button("Sa&ve settings").no_close(),
    ]);
    layout.new_line();
    layout.dialog()
}

/// The answers as the dialog holds them (arclite's `read_controls`): the folder without its quotes and
/// spaces at its ends, `%NAME%` expanded once it was changed; what is not offered stays as it was.
fn read(options: &ExtractOptions, value: &dyn Fn(usize) -> (i32, String)) -> ExtractOptions {
    let typed = value(DST_DIR).1.trim().replace('"', "");
    let offered = with_slash(&options.dst_dir.to_string_lossy());
    let dst_dir = if with_slash(&typed) == offered {
        typed
    } else {
        crate::opening::expand_env(&typed)
    };
    let overwrite = (ANSWERS.iter().enumerate())
        .find(|(index, _)| value(FIRST_ANSWER + index).0 == 1)
        .map_or(Overwrite::Ask, |(_, answer)| *answer);
    let tri = |item: usize| Tri::of_state(value(item).0);
    ExtractOptions {
        dst_dir: PathBuf::from(dst_dir),
        ignore_errors: value(IGNORE_ERRORS).0 == 1,
        overwrite,
        move_files: match options.move_files {
            Tri::Undef => Tri::Undef,
            _ => tri(MOVE_FILES),
        },
        password: value(PASSWORD).1,
        separate_dir: tri(SEPARATE_DIR),
        delete_archive: value(DELETE_ARCHIVE).0 == 1,
        disable_delete_archive: options.disable_delete_archive,
        open_dir: match options.open_dir {
            Tri::Undef => Tri::Undef,
            _ => tri(OPEN_DIR),
        },
    }
}

/// The dialog's events (arclite's `dialog_proc`): "Delete archive" turns "Delete files" off; "Save
/// settings" keeps the answers (and says so).
pub fn handle(
    host: &Host,
    options: &ExtractOptions,
    event: &DialogEvent,
    controls: &mut Controls,
) -> bool {
    match *event {
        DialogEvent::Changed(DELETE_ARCHIVE) => {
            let deleting = controls.value(DELETE_ARCHIVE).checked();
            controls.enable(MOVE_FILES, options.move_files != Tri::Undef && !deleting);
        }
        DialogEvent::Button(SAVE) => {
            let value = |item: usize| {
                let value = controls.value(item);
                (value.state, value.text)
            };
            let read = read(options, &value);
            let kept = ExtractSettings {
                ignore_errors: read.ignore_errors,
                overwrite: read.overwrite,
                separate_dir: read.separate_dir,
                open_dir: match read.open_dir {
                    Tri::Undef => ExtractSettings::load(host).open_dir,
                    open => open == Tri::True,
                },
            };
            kept.save(host);
            host.message(TITLE, &["Settings saved"]);
            controls.focus(OK);
        }
        _ => {}
    }
    true
}

/// The answers the dialog closed with by OK; none for Cancel or Esc.
pub fn answers(options: &ExtractOptions, closed: &Closed) -> Option<ExtractOptions> {
    if closed.button.is_none_or(|button| button == CANCEL) {
        return None;
    }
    let value = |item: usize| {
        (closed.values.get(item)).map_or((0, String::new()), |value| {
            (value.state, value.text.clone())
        })
    };
    Some(read(options, &value))
}

#[cfg(test)]
#[path = "extract_dialog_tests.rs"]
mod tests;
