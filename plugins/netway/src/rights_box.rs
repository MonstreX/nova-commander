//! NetBox's rights in a dialog (`WinSCPDialogs.cpp` `TRightsContainer`, WinSCP's `TRights`) laid out in
//! rows: "O&wner ", "Grou&p ", "Ot&hers" with R, W, X (and "Set UID", "Set GID", "Sticky bit"), "Oct&al:"
//! with "&None", "&Default", "A&ll", "Add &X to directories". Each right set, unset or (several files)
//! left as each file has it (`[?]`); the octal line follows the boxes and the boxes the octal typed (empty
//! while a right is left as it is).

use nc_plugin_sdk::{Controls, DialogItem, Value};

use crate::copy_params::{DEFAULT_RIGHTS, octal, rights_of_octal};

/// The rows' labels and their special bits' labels and bits.
const ROWS: [(&str, &str, u16); 3] = [
    ("O&wner ", "Set UID   ", 0o4000),
    ("Grou&p ", "Set GID   ", 0o2000),
    ("Ot&hers", "Sticky bit", 0o1000),
];

/// The bits every box can set.
const ALL_BITS: u16 = 0o7777;

/// Rights where some may be left as each file has them: those set, those unset, the others kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rights {
    pub set: u16,
    pub unset: u16,
}

impl Rights {
    /// Every right known: `number`'s set, the others unset.
    pub fn of(number: u16) -> Self {
        Self {
            set: number & ALL_BITS,
            unset: !number & ALL_BITS,
        }
    }

    /// WinSCP's `TRights::GetIsUndef`.
    pub fn is_undef(self) -> bool {
        (self.set | self.unset) & ALL_BITS != ALL_BITS
    }

    /// The rights of several files: a bit they all have set, all have unset, or else left.
    pub fn common(numbers: &[u16]) -> Self {
        let set = numbers.iter().fold(ALL_BITS, |set, number| set & number);
        let unset = numbers
            .iter()
            .fold(ALL_BITS, |unset, number| unset & !number);
        Self { set, unset }
    }

    /// A file's `old` rights changed by these.
    pub fn applied(self, old: u16) -> u16 {
        (old & !self.unset) | self.set
    }

    fn state(self, bit: u16) -> Option<bool> {
        match (self.set & bit != 0, self.unset & bit != 0) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        }
    }
}

/// Which parts the box has and where its items start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RightsBox {
    pub base: usize,
    pub specials: bool,
    pub buttons: bool,
    pub add_x: bool,
    /// The boxes may be left as each file has it (several files, or recursively).
    pub three_state: bool,
}

impl RightsBox {
    fn row_len(self) -> usize {
        4 + usize::from(self.specials)
    }

    /// The item of the box at `row` (owner, group, others) and `column` (R, W, X, the special).
    pub fn check(self, row: usize, column: usize) -> usize {
        self.base + row * self.row_len() + 1 + column
    }

    /// The bit of the box at `row` and `column`.
    pub fn bit(row: usize, column: usize) -> u16 {
        match column {
            3 => ROWS[row].2,
            _ => 0o400 >> (row * 3 + column),
        }
    }

    fn columns(self) -> usize {
        3 + usize::from(self.specials)
    }

    pub fn octal(self) -> usize {
        self.base + 3 * self.row_len() + 1
    }

    /// The buttons' items: None, Default, All.
    pub fn button(self, which: usize) -> Option<usize> {
        self.buttons.then(|| self.octal() + 1 + which)
    }

    pub fn add_x(self) -> Option<usize> {
        let after = self.octal() + 1 + if self.buttons { 3 } else { 0 };
        self.add_x.then_some(after)
    }

    /// The items after the box.
    pub fn end(self) -> usize {
        self.add_x().map_or(
            self.octal() + 1 + if self.buttons { 3 } else { 0 },
            |item| item + 1,
        )
    }

    /// Its items for `rights`, disabled when `enabled` is off.
    pub fn items(self, rights: Rights, add_x: bool, enabled: bool) -> Vec<DialogItem> {
        let mut items = Vec::new();
        for (row, (label, special, _)) in ROWS.iter().enumerate() {
            items.push(DialogItem::text(label).disabled(!enabled));
            for column in 0..self.columns() {
                let name = match column {
                    3 => special,
                    _ => ["R", "W", "X"][column],
                };
                let state = rights.state(Self::bit(row, column));
                let check = match self.three_state {
                    true => DialogItem::check3(name, state),
                    false => DialogItem::check(name, state == Some(true)),
                };
                items.push(check.same_row().disabled(!enabled));
            }
        }
        items.push(DialogItem::text("Oct&al:").disabled(!enabled));
        let shown = if rights.is_undef() {
            String::new()
        } else {
            octal(rights.set)
        };
        items.push(
            DialogItem::fixed("9999", &shown)
                .same_row()
                .disabled(!enabled),
        );
        if self.buttons {
            for label in ["&None", "&Default", "A&ll"] {
                items.push(
                    DialogItem::button(label)
                        .no_close()
                        .same_row()
                        .disabled(!enabled),
                );
            }
        }
        if self.add_x {
            items.push(DialogItem::check("Add &X to directories", add_x).disabled(!enabled));
        }
        items
    }

    /// The rights the boxes give.
    pub fn rights_of(self, value: &dyn Fn(usize) -> Value) -> Rights {
        let mut rights = Rights::default();
        for row in 0..3 {
            for column in 0..self.columns() {
                let bit = Self::bit(row, column);
                match value(self.check(row, column)).state {
                    1 => rights.set |= bit,
                    0 => rights.unset |= bit,
                    _ => {}
                }
            }
        }
        if !self.specials {
            rights.unset |= 0o7000;
        }
        rights
    }

    fn show(self, controls: &mut Controls, rights: Rights) {
        for row in 0..3 {
            for column in 0..self.columns() {
                let state = rights.state(Self::bit(row, column)).map_or(2, i32::from);
                controls.set_state(self.check(row, column), state);
            }
        }
        let shown = if rights.is_undef() {
            String::new()
        } else {
            octal(rights.set)
        };
        controls.set_text(self.octal(), &shown);
    }

    /// `TRightsContainer`'s `UpdateControls`, `OctalEditExit`, `RightsButtonClick` after the item `changed`
    /// (a button pressed: `pressed`): true when it was the box's.
    pub fn follow(self, controls: &mut Controls, changed: usize, pressed: bool) -> bool {
        let numbers = [0, DEFAULT_RIGHTS, 0o777];
        if pressed {
            let Some(which) = (0..3).find(|which| self.button(*which) == Some(changed)) else {
                return false;
            };
            self.show(controls, Rights::of(numbers[which]));
        } else if changed == self.octal() {
            if let Ok(number) = rights_of_octal(&controls.value(changed).text) {
                self.show(controls, Rights::of(number));
            }
        } else if (self.base..self.octal()).contains(&changed) {
            let values: Vec<Value> = (0..self.end()).map(|item| controls.value(item)).collect();
            let rights = self.rights_of(&|item| values.get(item).cloned().unwrap_or_default());
            let shown = if rights.is_undef() {
                String::new()
            } else {
                octal(rights.set)
            };
            controls.set_text(self.octal(), &shown);
        } else {
            return false;
        }
        self.follow_add_x(controls);
        true
    }

    /// "Add X to directories" only while some X is not set (WinSCP's `UpdateControls`).
    fn follow_add_x(self, controls: &mut Controls) {
        let Some(item) = self.add_x() else {
            return;
        };
        let all_x = (0..3).all(|row| controls.value(self.check(row, 2)).state == 1);
        if all_x {
            controls.set_checked(item, false);
        }
        controls.enable(item, !all_x);
    }

    /// The whole box enabled or not.
    pub fn enable(self, controls: &mut Controls, enabled: bool) {
        for item in self.base..self.end() {
            controls.enable(item, enabled);
        }
        if enabled {
            self.follow_add_x(controls);
        }
    }
}

#[cfg(test)]
#[path = "rights_box_tests.rs"]
mod tests;
