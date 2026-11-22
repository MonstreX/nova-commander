//! arclite's "Add files to archive" dialog for an archive that is there (`UpdateDialog` with `new_arc`
//! false, ui.cpp) in its order and words: the level and the method, Solid (7z's only), the advanced
//! parameters (their history `arclite.advanced`), encryption with the password twice or shown, the
//! existing file's answer, moving, ignoring errors, files open for writing, the filter; "Save settings"
//! keeps the answers without closing it. Its handler keeps the items enabled and shown as arclite's
//! `set_control_state` does, and OK checks the password as its `read_controls` does.

use nc_plugin_sdk::{Closed, Controls, Dialog, DialogEvent, DialogItem, Host};

use crate::extract_options::Overwrite;
use crate::layout::Layout;
use crate::update_options::{LEVELS, UpdateOptions, UpdateSettings};
use crate::update_props::{COPY, fit_level, methods_of};

pub const TITLE: &str = "Add files to archive";

/// The dialog's items by index (as `dialog` lays them out).
const LEVEL: usize = 1;
const METHOD_LABEL: usize = 2;
const METHOD: usize = 3;
const SOLID: usize = 4;
const ADVANCED: usize = 6;
const ENCRYPT: usize = 8;
const ENCRYPT_HEADER: usize = 9;
const SHOW_PASSWORD: usize = 10;
const PASSWORD_LABEL: usize = 11;
const PASSWORD: usize = 12;
const PASSWORD_VERIFY: usize = 14;
const PASSWORD_VISIBLE_LABEL: usize = 15;
const PASSWORD_VISIBLE: usize = 16;
/// The answers' radio buttons, in `ANSWERS`' order.
const FIRST_ANSWER: usize = 19;
const MOVE_FILES: usize = 23;
const IGNORE_ERRORS: usize = 24;
const OPEN_SHARED: usize = 25;
pub const OK: usize = 28;
const CANCEL: usize = 29;
const SAVE: usize = 30;

/// The radio buttons' answers in their order.
const ANSWERS: [Overwrite; 3] = [Overwrite::Ask, Overwrite::Overwrite, Overwrite::Skip];

/// The lines of a list.
fn names<const N: usize>(table: &[(&'static str, u32); N]) -> Vec<&'static str> {
    table.iter().map(|(name, _)| *name).collect()
}

/// arclite's `UpdateDialog::show` for an archive that is there.
pub fn dialog(options: &UpdateOptions) -> Dialog {
    let mut layout = Layout::new(TITLE, 68);
    layout.label("Compression &level:");
    let level = (LEVELS.iter())
        .position(|(_, value)| *value == options.level)
        .unwrap_or(0);
    layout.combo(&names(&LEVELS), level);
    layout.spacer(2);
    layout.label("&Method:");
    let methods = methods_of(&options.kind.format);
    let method = (methods.iter())
        .position(|method| method.eq_ignore_ascii_case(&options.method))
        .unwrap_or(0);
    layout.combo(methods, method);
    layout.spacer(2);
    layout.boxed(DialogItem::check("S&olid", options.kind.solid));
    layout.new_line();
    layout.label("Advan&ced parameters:");
    let advanced = DialogItem::edit(&options.advanced).history("arclite.advanced", false);
    layout.edit(advanced, 0);
    layout.new_line();
    layout.separator();
    layout.new_line();
    layout.boxed(DialogItem::check("&Encrypt archive", options.encrypt));
    layout.spacer(2);
    layout.boxed(DialogItem::check3(
        "Encrypt header",
        options.kind.encrypt_header,
    ));
    layout.spacer(2);
    layout.boxed(DialogItem::check("Show password", options.show_password));
    layout.new_line();
    layout.label("Pass&word:");
    layout.edit(DialogItem::password(&options.password), 20);
    layout.spacer(2);
    layout.label("Repeat:");
    layout.edit(DialogItem::password(&options.password), 20);
    layout.reset_line();
    layout.label("Pass&word:");
    layout.edit(DialogItem::edit(&options.password), 20);
    layout.new_line();
    layout.separator();
    layout.new_line();
    answer_rows(&mut layout, options.overwrite);
    let moving = DialogItem::check("&Delete files after archiving", options.move_files);
    layout.boxed(moving);
    layout.spacer(2);
    layout.boxed(DialogItem::check("&Ignore errors", options.ignore_errors));
    layout.new_line();
    let shared = DialogItem::check("Archive files open for w&riting", options.open_shared);
    layout.boxed(shared);
    layout.spacer(2);
    // FAR's custom filter menu is not the plugins' yet (far-parity §14).
    layout.boxed(DialogItem::check("Use &filter", false).disabled(true));
    layout.new_line();
    layout.separator();
    layout.new_line();
    layout.buttons(vec![
        DialogItem::button("OK").default_button(),
        DialogItem::button("Cancel"),
        DialogItem::button("Save settings").no_close(),
    ]);
    layout.new_line();
    let mut dialog = layout.dialog();
    let states: Vec<i32> = dialog.items.iter().map(|item| item.state).collect();
    let state = |item: usize| states.get(item).copied().unwrap_or(0);
    for (item, (enabled, shown)) in control_state(&state, &options.kind.format) {
        dialog.items[item].disabled = !enabled;
        dialog.items[item].hidden = !shown;
    }
    dialog
}

/// "When destination file exists:" and its answers, the one given chosen (Overwrite for its case's too).
fn answer_rows(layout: &mut Layout, given: Overwrite) {
    layout.label("When destination file exists:");
    layout.new_line();
    let labels = ["As&k", "O&verwrite", "Ski&p"];
    for (index, (label, answer)) in labels.iter().zip(ANSWERS).enumerate() {
        layout.spacer(2);
        let on = given == answer
            || (answer == Overwrite::Overwrite && given == Overwrite::OverwriteCase);
        let radio = DialogItem::radio(label, on);
        layout.boxed(if index == 0 { radio.group() } else { radio });
    }
    layout.new_line();
    layout.separator();
    layout.new_line();
}

/// arclite's `set_control_state` for an archive of `format`: each item it changes, whether it is
/// enabled and shown, by the level (Store: no method, no solid), the format (a method for 7z and zip,
/// solid and the header's encryption for 7z, encryption for both), Encrypt and Show password.
fn control_state(state: &dyn Fn(usize) -> i32, format: &str) -> Vec<(usize, (bool, bool))> {
    let (is_7z, is_zip) = (format == "7z", format.eq_ignore_ascii_case("zip"));
    let compressed = state(LEVEL) != 0;
    let encrypt = state(ENCRYPT) == 1 && (is_7z || is_zip);
    let shown = state(SHOW_PASSWORD) == 1;
    let mut items = vec![
        (METHOD_LABEL, ((is_7z || is_zip) && compressed, true)),
        (METHOD, ((is_7z || is_zip) && compressed, true)),
        (SOLID, (is_7z && compressed, true)),
        (ENCRYPT, (is_7z || is_zip, true)),
        (ENCRYPT_HEADER, (is_7z && encrypt, true)),
        (SHOW_PASSWORD, (encrypt, true)),
    ];
    for item in PASSWORD_LABEL..=PASSWORD_VERIFY {
        items.push((item, (encrypt, !shown)));
    }
    for item in PASSWORD_VISIBLE_LABEL..=PASSWORD_VISIBLE {
        items.push((item, (encrypt, shown)));
    }
    items.push((MOVE_FILES, (true, true)));
    items
}

/// The answers as the dialog holds them (arclite's `read_controls`), or why they cannot be taken: the
/// passwords not the same, or none.
fn read(
    options: &UpdateOptions,
    value: &dyn Fn(usize) -> (i32, String),
) -> Result<UpdateOptions, &'static str> {
    let format = options.kind.format.as_str();
    let (is_7z, is_zip) = (format == "7z", format.eq_ignore_ascii_case("zip"));
    let at = |item: usize| usize::try_from(value(item).0).unwrap_or(0);
    let level = LEVELS
        .get(at(LEVEL))
        .map_or(options.level, |(_, level)| *level);
    let level = fit_level(format, level);
    let method = if level == 0 && !is_zip {
        COPY
    } else {
        (methods_of(format).get(at(METHOD))).map_or(&*options.method, |method| method)
    };
    let encrypt = value(ENCRYPT).0 == 1 && (is_7z || is_zip);
    let show_password = value(SHOW_PASSWORD).0 == 1;
    let password = read_password(
        encrypt,
        show_password,
        &|item| value(item).1,
        (PASSWORD, PASSWORD_VERIFY, PASSWORD_VISIBLE),
    )?;
    let overwrite = (ANSWERS.iter().enumerate())
        .find(|(index, _)| value(FIRST_ANSWER + index).0 == 1)
        .map_or(Overwrite::Ask, |(_, answer)| *answer);
    Ok(UpdateOptions {
        level,
        method: method.to_string(),
        advanced: value(ADVANCED).1,
        encrypt,
        show_password,
        password,
        overwrite,
        move_files: value(MOVE_FILES).0 == 1,
        ignore_errors: value(IGNORE_ERRORS).0 == 1,
        open_shared: value(OPEN_SHARED).0 == 1,
        kind: crate::update_options::Kind {
            solid: if is_7z && level != 0 {
                value(SOLID).0 == 1
            } else {
                options.kind.solid
            },
            encrypt_header: if is_7z && encrypt {
                match value(ENCRYPT_HEADER).0 {
                    0 => Some(false),
                    1 => Some(true),
                    _ => None,
                }
            } else {
                options.kind.encrypt_header
            },
            ..options.kind.clone()
        },
    })
}

/// The password as arclite's `read_controls` takes it: none when not encrypting, the line shown, or
/// the hidden one when typed twice the same; never empty when encrypting. `items` are the hidden line,
/// its repeat and the shown one.
pub(crate) fn read_password(
    encrypt: bool,
    show_password: bool,
    text: &dyn Fn(usize) -> String,
    (hidden, repeat, shown): (usize, usize, usize),
) -> Result<String, &'static str> {
    let password = if !encrypt {
        String::new()
    } else if show_password {
        text(shown)
    } else {
        let password = text(hidden);
        if password != text(repeat) {
            return Err("Passwords don't match");
        }
        password
    };
    if encrypt && password.is_empty() {
        return Err("Password is empty");
    }
    Ok(password)
}

/// The dialog's events (arclite's `dialog_proc`): the items follow the level, Encrypt and Show password
/// (the password carried over between its lines); "Save settings" keeps the answers (and says so); OK
/// with the passwords wrong says why and stays.
pub fn handle(
    host: &Host,
    options: &UpdateOptions,
    event: &DialogEvent,
    controls: &mut Controls,
) -> bool {
    let value = |controls: &Controls, item: usize| {
        let value = controls.value(item);
        (value.state, value.text)
    };
    let format = options.kind.format.as_str();
    match *event {
        DialogEvent::Init | DialogEvent::Changed(ENCRYPT) => {
            apply_state(controls, format);
        }
        DialogEvent::Changed(LEVEL) => {
            fit_level_line(controls, format);
            apply_state(controls, format);
        }
        DialogEvent::Changed(SHOW_PASSWORD) => {
            if controls.value(SHOW_PASSWORD).checked() {
                let typed = controls.value(PASSWORD).text;
                controls.set_text(PASSWORD_VISIBLE, &typed);
            } else {
                let typed = controls.value(PASSWORD_VISIBLE).text;
                controls.set_text(PASSWORD, &typed);
                controls.set_text(PASSWORD_VERIFY, &typed);
            }
            apply_state(controls, format);
        }
        DialogEvent::Button(SAVE) => {
            if let Ok(read) = read(options, &|item| value(controls, item)) {
                settings_of(&read).save(host);
                host.message(TITLE, &["Settings saved"]);
            }
            controls.focus(OK);
        }
        DialogEvent::Close(Some(button)) if button != CANCEL => {
            if let Err(why) = read(options, &|item| value(controls, item)) {
                host.message(TITLE, &[why]);
                return false;
            }
        }
        _ => {}
    }
    true
}

/// A level the format does not take put back to the one it writes (arclite's `update_level_list`,
/// whose list disables those lines).
fn fit_level_line(controls: &mut Controls, format: &str) {
    let at = usize::try_from(controls.value(LEVEL).state).unwrap_or(0);
    let Some((_, level)) = LEVELS.get(at) else {
        return;
    };
    let fitted = fit_level(format, *level);
    if fitted != *level
        && let Some(line) = LEVELS.iter().position(|(_, level)| *level == fitted)
    {
        controls.set_state(LEVEL, i32::try_from(line).unwrap_or(0));
    }
}

/// The items enabled and shown as the dialog's values say.
fn apply_state(controls: &mut Controls, format: &str) {
    let states: Vec<i32> = (0..=SAVE).map(|item| controls.value(item).state).collect();
    let state = |item: usize| states.get(item).copied().unwrap_or(0);
    for (item, (enabled, shown)) in control_state(&state, format) {
        controls.enable(item, enabled);
        controls.show(item, shown);
    }
}

/// What "Save settings" keeps of the answers.
fn settings_of(options: &UpdateOptions) -> UpdateSettings {
    UpdateSettings {
        level: options.level,
        method: options.method.clone(),
        advanced: options.advanced.clone(),
        encrypt: options.encrypt,
        show_password: options.show_password,
        move_files: options.move_files,
        ignore_errors: options.ignore_errors,
        overwrite: options.overwrite,
    }
}

/// The answers the dialog closed with by OK; none for Cancel or Esc.
pub fn answers(options: &UpdateOptions, closed: &Closed) -> Option<UpdateOptions> {
    if closed.button.is_none_or(|button| button == CANCEL) {
        return None;
    }
    let value = |item: usize| {
        (closed.values.get(item)).map_or((0, String::new()), |value| {
            (value.state, value.text.clone())
        })
    };
    read(options, &value).ok()
}

#[cfg(test)]
#[path = "update_dialog_tests.rs"]
mod tests;
