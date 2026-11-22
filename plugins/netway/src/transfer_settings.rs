//! NetBox's transfer settings dialog (`WinSCPDialogs.cpp` `TCopyParamsContainer` in `CopyParamDialog`;
//! its rights in `TRightsContainer`): "Transfer settings" from the copy dialog, "NetWay - Transfer
//! settings" from the plugin's configuration. Its boxes as NetBox's (FAR's `DI_SINGLEBOX` one row high:
//! lines with their titles): the transfer mode with the text mode's masks, the file names' case and
//! "Replace \:*"?", the download's "Preserve read-only", the upload's "Set permissions" with the owner's,
//! the group's and the others' R W X, the octal rights and "Add X to directories", "Ignore permission
//! errors", "Clear 'Archive' attribute", the common "Preserve timestamp" and "Calculate total size"; the
//! file mask, the speed, the temporary folder. What a direction cannot use is disabled. OK checks the
//! masks, the speed and the octal rights (WinSCP's messages) and stays on a wrong one. NetBox's speed is a
//! combo box of 1024 down to 8: here an edit line with its history (the program has no editable combo).

use nc_plugin_sdk::{Controls, Dialog, DialogEvent, DialogItem, Host, Message, Value};

use crate::copy_params::{
    Attrs, CopyParams, NameCase, TransferMode, octal, rights_of_octal, speed_of_text, speed_text,
};
use crate::file_masks::FileMasks;

// The items, in their order.
const TEXT_MODE: usize = 2;
const BINARY: usize = 3;
const AUTOMATIC: usize = 4;
const ASCII_LABEL: usize = 5;
const ASCII_MASK: usize = 6;
const NO_CHANGE: usize = 8;
const REPLACE: usize = 13;
const READ_ONLY: usize = 15;
const RIGHTS: usize = 16;
/// The rights' first item (the owner's label) and their last (the octal line).
const RIGHTS_FIRST: usize = 17;
const OCTAL: usize = 30;
const ADD_X: usize = 31;
const IGNORE: usize = 32;
const CLEAR_ARCHIVE: usize = 33;
const PRESERVE_TIME: usize = 35;
const CALCULATE: usize = 36;
const FILE_MASK: usize = 39;
const SPEED: usize = 41;
const USE_TEMP: usize = 42;
const TEMP_LABEL: usize = 43;
const TEMP_DIR: usize = 44;
const OK: usize = 46;

/// The name cases' radio buttons in their items' order (NetBox's: no change, upper, first upper, lower,
/// 8.3).
const CASES: [NameCase; 5] = [
    NameCase::NoChange,
    NameCase::Upper,
    NameCase::FirstUpper,
    NameCase::Lower,
    NameCase::LowerShort,
];

/// The check box of each right, owner's R first.
fn right_item(bit: usize) -> usize {
    RIGHTS_FIRST + (bit / 3) * 4 + 1 + bit % 3
}

/// The dialog's columns: the left box's, the right box's.
const LEFT: i32 = 5;
const RIGHT: i32 = 45;

fn at(item: DialogItem, x: i32, y: i32) -> DialogItem {
    item.at(x, y, x, y)
}

fn line(item: DialogItem, x: i32, y: i32, width: i32) -> DialogItem {
    item.at(x, y, x + width - 1, y)
}

fn heading(title: &str, x: i32, y: i32, width: i32) -> DialogItem {
    line(DialogItem::single_box(title), x, y, width)
}

/// The rights' rows: a label and R, W, X.
fn rights_items(rights: u16, enabled: bool) -> Vec<DialogItem> {
    let mut items = Vec::new();
    for (row, label) in ["O&wner ", "Grou&p ", "Ot&hers"].iter().enumerate() {
        let y = 4 + i32::try_from(row).unwrap_or(0);
        items.push(at(DialogItem::text(label), RIGHT, y).disabled(!enabled));
        for (column, name) in ["R", "W", "X"].iter().enumerate() {
            let bit = 0o400 >> (row * 3 + column);
            let x = RIGHT + 8 + 6 * i32::try_from(column).unwrap_or(0);
            let check = DialogItem::check(name, rights & bit != 0);
            items.push(at(check, x, y).disabled(!enabled));
        }
    }
    items.push(at(DialogItem::text("Oct&al:"), RIGHT, 7).disabled(!enabled));
    items.push(line(DialogItem::fixed("9999", &octal(rights)), RIGHT + 8, 7, 5).disabled(!enabled));
    items
}

/// NetBox's `UpdateControls` of "Ignore permission errors": with rights or times set.
fn ignore_enabled(rights: bool, time: bool, attrs: Attrs) -> bool {
    (rights || time) && !attrs.no_ignore_perm_errors
}

/// The left column: the transfer mode, the names, the download's options.
fn left_items(params: &CopyParams) -> Vec<DialogItem> {
    let mode = params.transfer_mode;
    let automatic = mode == TransferMode::Automatic;
    let case = |at_case: NameCase| params.name_case == at_case;
    vec![
        at(
            DialogItem::radio("&Text (plain text, html, ...)", mode == TransferMode::Text).group(),
            LEFT,
            3,
        ),
        at(
            DialogItem::radio("&Binary (archives, doc, ...)", mode == TransferMode::Binary),
            LEFT,
            4,
        ),
        at(DialogItem::radio("Automat&ic", automatic), LEFT, 5),
        at(
            DialogItem::text("Transfer following files in t&ext mode"),
            LEFT,
            6,
        )
        .disabled(!automatic),
        line(
            DialogItem::edit(&params.ascii_mask).history("WinscpAsciiMask", false),
            LEFT,
            7,
            37,
        )
        .disabled(!automatic),
        heading("Filename modification", LEFT - 1, 8, 39),
        at(
            DialogItem::radio("&No change     ", case(NameCase::NoChange)).group(),
            LEFT,
            9,
        ),
        at(
            DialogItem::radio("&Upper case", case(NameCase::Upper)),
            LEFT + 19,
            9,
        ),
        at(
            DialogItem::radio("&First upper   ", case(NameCase::FirstUpper)),
            LEFT,
            10,
        ),
        at(
            DialogItem::radio("Lo&wer case", case(NameCase::Lower)),
            LEFT + 19,
            10,
        ),
        at(
            DialogItem::radio("Lower case &8.3", case(NameCase::LowerShort)),
            LEFT,
            11,
        ),
        at(
            DialogItem::check("&Replace \\:*\"?", params.replace_invalid),
            LEFT + 19,
            11,
        ),
        heading("Download options", LEFT - 1, 12, 39),
    ]
}

/// The dialog's items for `params` with what `attrs` leaves out disabled; `temporary` for the viewer's
/// and the editor's transfers (no archive attribute cleared).
pub fn items(params: &CopyParams, attrs: Attrs, temporary: bool) -> Vec<DialogItem> {
    let rights_on = params.preserve_rights && !attrs.no_rights;
    let all_x = params.rights & 0o111 == 0o111;
    let mut items = vec![
        heading("Transfer mode", LEFT - 1, 2, 39),
        heading("Upload options", RIGHT - 1, 2, 31),
    ];
    items.extend(left_items(params));
    items.push(
        at(
            DialogItem::check("Preserve rea&d-only", params.preserve_read_only),
            LEFT,
            13,
        )
        .disabled(attrs.no_preserve_read_only),
    );
    items.push(
        at(
            DialogItem::check("&Set permissions", params.preserve_rights),
            RIGHT,
            3,
        )
        .disabled(attrs.no_rights),
    );
    items.extend(rights_items(params.rights, rights_on));
    let add_x = params.add_x_to_dirs && !all_x;
    items.extend([
        at(DialogItem::check("Add &X to directories", add_x), RIGHT, 8)
            .disabled(!rights_on || all_x),
        at(
            DialogItem::check("&Ignore permission errors", params.ignore_perm_errors),
            RIGHT,
            9,
        )
        .disabled(!ignore_enabled(rights_on, params.preserve_time, attrs)),
        at(
            DialogItem::check("Clear 'Archi&ve' attribute", params.clear_archive),
            RIGHT,
            10,
        )
        .disabled(temporary || attrs.no_clear_archive),
        heading("Common options", RIGHT - 1, 11, 31),
        at(
            DialogItem::check("Preserve timesta&mp", params.preserve_time),
            RIGHT,
            12,
        ),
        at(
            DialogItem::check("&Calculate total size", params.calculate_size),
            RIGHT,
            13,
        ),
    ]);
    items.extend(other_items(params));
    items
}

/// "Other": the file mask, the speed, the temporary folder; then OK and Cancel.
fn other_items(params: &CopyParams) -> Vec<DialogItem> {
    let temp = !params.temp_path.is_empty();
    let folder = match temp {
        true => params.temp_path.clone(),
        false => std::env::temp_dir().display().to_string(),
    };
    vec![
        DialogItem {
            text: " Other ".to_string(),
            ..DialogItem::separator()
        }
        .at(-1, 14, -1, 14),
        at(DialogItem::text("File mas&k (example: *|.svn/):"), LEFT, 15),
        line(
            DialogItem::edit(&params.include_mask).history("WinscpFileMask", false),
            LEFT,
            16,
            37,
        ),
        at(DialogItem::text("Sp&eed (KB/s):"), RIGHT, 16),
        line(
            DialogItem::edit(&speed_text(params.speed_limit)).history("SpeedLimit", false),
            RIGHT + 15,
            16,
            12,
        ),
        at(
            DialogItem::check("Use te&mporary directory for downloads", temp),
            LEFT,
            17,
        ),
        at(DialogItem::text("&Temporary directory:"), LEFT, 18).disabled(!temp),
        line(DialogItem::edit(&folder).edit_path(), LEFT + 21, 18, 18).disabled(!temp),
        DialogItem::separator().at(-1, 19, -1, 19),
        at(DialogItem::button("&Ok").default_button(), 31, 20),
        at(DialogItem::button("&Cancel"), 38, 20),
    ]
}

/// The rights the check boxes give.
fn checked_rights(value: &dyn Fn(usize) -> Value) -> u16 {
    (0..9).fold(0, |rights, bit| match value(right_item(bit)).checked() {
        true => rights | (0o400 >> bit),
        false => rights,
    })
}

/// NetBox's `GetParams`: the parameters the dialog's `values` give over `base`.
pub fn params_of(values: &[Value], base: &CopyParams) -> CopyParams {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let modes = [
        (TEXT_MODE, TransferMode::Text),
        (BINARY, TransferMode::Binary),
        (AUTOMATIC, TransferMode::Automatic),
    ];
    let transfer_mode = (modes.into_iter())
        .find(|(item, _)| value(*item).checked())
        .map_or(TransferMode::Binary, |(_, mode)| mode);
    let case_at = (0..CASES.len()).find(|at| value(NO_CHANGE + at).checked());
    let rights = rights_of_octal(&value(OCTAL).text).unwrap_or_else(|_| checked_rights(&value));
    CopyParams {
        transfer_mode,
        ascii_mask: match transfer_mode {
            TransferMode::Automatic => value(ASCII_MASK).text,
            _ => base.ascii_mask.clone(),
        },
        name_case: case_at.map_or(NameCase::NoChange, |at| CASES[at]),
        replace_invalid: value(REPLACE).checked(),
        preserve_read_only: value(READ_ONLY).checked(),
        preserve_rights: value(RIGHTS).checked(),
        rights,
        add_x_to_dirs: value(ADD_X).checked(),
        ignore_perm_errors: value(IGNORE).checked(),
        clear_archive: value(CLEAR_ARCHIVE).checked(),
        preserve_time: value(PRESERVE_TIME).checked(),
        calculate_size: value(CALCULATE).checked(),
        include_mask: value(FILE_MASK).text,
        speed_limit: speed_of_text(&value(SPEED).text).unwrap_or(base.speed_limit),
        temp_path: match value(USE_TEMP).checked() {
            true => value(TEMP_DIR).text.trim().to_string(),
            false => String::new(),
        },
        ..base.clone()
    }
}

/// What is wrong with the values for OK (WinSCP's messages), none when nothing is.
pub fn wrong(values: &[Value]) -> Option<(usize, String)> {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    if value(AUTOMATIC).checked()
        && let Err(error) = FileMasks::parse(&value(ASCII_MASK).text)
    {
        return Some((ASCII_MASK, error));
    }
    if let Err(error) = FileMasks::parse(&value(FILE_MASK).text) {
        return Some((FILE_MASK, error));
    }
    if let Err(error) = speed_of_text(&value(SPEED).text) {
        return Some((SPEED, error));
    }
    let typed = value(OCTAL).text;
    match rights_of_octal(&typed) {
        Err(error) if value(RIGHTS).checked() && !typed.trim().is_empty() => Some((OCTAL, error)),
        _ => None,
    }
}

/// The values of the dialog now.
fn values_of(controls: &Controls) -> Vec<Value> {
    (0..=OK).map(|item| controls.value(item)).collect()
}

/// NetBox's `UpdateControls` and `TRightsContainer`'s: what is enabled follows the check boxes, the
/// octal rights the R W X boxes and the boxes the octal rights typed.
fn follow(controls: &mut Controls, changed: usize, attrs: Attrs) {
    let checked = |controls: &Controls, item: usize| controls.value(item).checked();
    if changed == OCTAL {
        if let Ok(rights) = rights_of_octal(&controls.value(OCTAL).text) {
            for bit in 0..9 {
                controls.set_checked(right_item(bit), rights & (0o400 >> bit) != 0);
            }
        }
    } else if (RIGHTS_FIRST..OCTAL).contains(&changed) {
        let values = values_of(controls);
        let rights = checked_rights(&|item| values.get(item).cloned().unwrap_or_default());
        controls.set_text(OCTAL, &octal(rights));
    }
    let automatic = checked(controls, AUTOMATIC);
    controls.enable(ASCII_LABEL, automatic);
    controls.enable(ASCII_MASK, automatic);
    let rights_on = checked(controls, RIGHTS) && !attrs.no_rights;
    for item in RIGHTS_FIRST..=OCTAL {
        controls.enable(item, rights_on);
    }
    let all_x = (0..3).all(|group| checked(controls, right_item(group * 3 + 2)));
    if all_x {
        controls.set_checked(ADD_X, false);
    }
    controls.enable(ADD_X, rights_on && !all_x);
    let time = checked(controls, PRESERVE_TIME);
    controls.enable(IGNORE, ignore_enabled(rights_on, time, attrs));
    let temp = checked(controls, USE_TEMP);
    controls.enable(TEMP_LABEL, temp);
    controls.enable(TEMP_DIR, temp);
}

/// The dialog titled `title` over `params`: the parameters on OK, none on Cancel or Esc.
pub fn ask(
    host: &Host,
    title: &str,
    params: &CopyParams,
    attrs: Attrs,
    temporary: bool,
) -> Option<CopyParams> {
    let dialog = Dialog::new(title, items(params, attrs, temporary)).size(79, 23);
    let closed = host.dialog_with(&dialog, |event, controls| match event {
        DialogEvent::Changed(item) => {
            follow(controls, item, attrs);
            true
        }
        DialogEvent::Close(Some(OK)) => match wrong(&values_of(controls)) {
            Some((item, error)) => {
                let _ = host.show(&Message::new("Error", &[&error]).warning().buttons(&["&Ok"]));
                controls.focus(item);
                false
            }
            None => true,
        },
        _ => true,
    });
    (closed.button == Some(OK)).then(|| params_of(&closed.values, params))
}

#[cfg(test)]
#[path = "transfer_settings_tests.rs"]
mod tests;
