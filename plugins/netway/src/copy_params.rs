//! WinSCP's copy parameters (`CopyParam.cpp` `TCopyParamType`): the transfer settings NetBox's dialog
//! sets (S6.5c) with WinSCP's defaults; the line the copy dialog shows of them (`GetInfoStr` as NetBox
//! builds it: the transfer type always first, then what differs from the defaults, joined by "; "); the
//! names a transfer writes (`ChangeFileName`), the text mode's choice (`UseAsciiTransfer`), the rights set
//! (`RemoteFileRights`). What a direction cannot use (`TTerminal::UsableCopyParamAttrs`) is left out of the
//! line and disabled in the dialog.

use crate::copy_names::{mask_file_name, restore_chars, valid_local_file_name, walks_out};
use crate::file_masks::{FileMasks, Params};

/// WinSCP's `TTransferMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransferMode {
    #[default]
    Binary,
    Text,
    Automatic,
}

/// WinSCP's `TFileNameCase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NameCase {
    #[default]
    NoChange,
    Upper,
    Lower,
    FirstUpper,
    LowerShort,
}

/// WinSCP's `TResumeSupport`: which files go through a temporary name (`allow_resume`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resume {
    On,
    #[default]
    Smart,
    Off,
}

/// WinSCP's default masks of the files sent as text in the automatic mode.
pub const ASCII_MASK: &str = "*.*html; *.htm; *.txt; *.php; *.php3; *.cgi; *.c; *.cpp; *.h; *.pas; *.bas; *.tex; *.pl; *.js; .htaccess; *.xtml; *.css; *.cfg; *.ini; *.sh; *.xml";

/// WinSCP's `PartialExt`: a file being sent resumably is written under its name with this after it.
pub const PARTIAL_EXT: &str = ".filepart";

/// WinSCP's `TRights::rfDefault`: rw-r--r--.
pub const DEFAULT_RIGHTS: u16 = 0o644;

/// The parameters of a transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyParams {
    /// The name typed after the target's folder (`FileMask`): the first level's names masked by it.
    pub file_mask: String,
    /// "New and updated file(s) only": a target there taken over only by a newer source, unasked.
    pub newer_only: bool,
    /// "Transfer on background (add to queue)" (WinSCP's `Queue`): the copy handed to the program's
    /// background tasks (S6.8c).
    pub queue: bool,
    pub preserve_time: bool,
    pub transfer_mode: TransferMode,
    /// The automatic mode's masks of text files.
    pub ascii_mask: String,
    pub name_case: NameCase,
    /// Characters Windows refuses written as `%XX` and read back (`InvalidCharsReplacement`).
    pub replace_invalid: bool,
    /// "Set permissions" of uploads.
    pub preserve_rights: bool,
    pub rights: u16,
    pub add_x_to_dirs: bool,
    pub ignore_perm_errors: bool,
    /// A download of a file without the owner's write right is read only.
    pub preserve_read_only: bool,
    /// An upload's source loses its "Archive" attribute.
    pub clear_archive: bool,
    pub calculate_size: bool,
    /// The transfer settings' "File mask": the items taken.
    pub include_mask: String,
    /// Bytes a second, 0 none.
    pub speed_limit: u32,
    /// "Use temporary directory for downloads" (the queue's, S6.8): empty none.
    pub temp_path: String,
    pub resume: Resume,
    pub resume_threshold: u64,
}

impl Default for CopyParams {
    /// WinSCP's `TCopyParamType::Default`.
    fn default() -> Self {
        Self {
            file_mask: String::new(),
            newer_only: false,
            queue: false,
            preserve_time: true,
            transfer_mode: TransferMode::Binary,
            ascii_mask: ASCII_MASK.to_string(),
            name_case: NameCase::NoChange,
            replace_invalid: true,
            preserve_rights: false,
            rights: DEFAULT_RIGHTS,
            add_x_to_dirs: true,
            ignore_perm_errors: false,
            preserve_read_only: false,
            clear_archive: false,
            calculate_size: true,
            include_mask: String::new(),
            speed_limit: 0,
            temp_path: String::new(),
            resume: Resume::Smart,
            resume_threshold: 100 * 1024,
        }
    }
}

/// What a transfer's direction cannot use (WinSCP's `cpaNo*` of `UsableCopyParamAttrs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attrs {
    pub no_rights: bool,
    pub no_preserve_read_only: bool,
    pub no_clear_archive: bool,
    pub no_ignore_perm_errors: bool,
}

impl Attrs {
    /// A download's: no rights set, no archive attribute cleared, no permission errors ignored.
    pub fn download() -> Self {
        Self {
            no_rights: true,
            no_clear_archive: true,
            no_ignore_perm_errors: true,
            ..Self::default()
        }
    }

    /// An upload's: no read-only kept; a move's sources go, so no archive attribute is cleared.
    pub fn upload(moving: bool) -> Self {
        Self {
            no_preserve_read_only: true,
            no_clear_archive: moving,
            ..Self::default()
        }
    }
}

/// WinSCP's `TRights::GetText` of the nine rights: `rw-r--r--`.
pub fn rights_text(rights: u16) -> String {
    (0..9)
        .map(|at| {
            let bit = 0o400 >> at;
            match (rights & bit != 0, at % 3) {
                (false, _) => '-',
                (true, 0) => 'r',
                (true, 1) => 'w',
                (true, _) => 'x',
            }
        })
        .collect()
}

/// `rw-r--r--` read back; `None` when it is not nine such characters.
pub fn rights_of_text(text: &str) -> Option<u16> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() != 9 {
        return None;
    }
    chars.iter().enumerate().try_fold(0, |rights, (at, &ch)| {
        let set = ['r', 'w', 'x'][at % 3];
        match ch {
            '-' => Some(rights),
            _ if ch == set => Some(rights | (0o400 >> at)),
            _ => None,
        }
    })
}

/// WinSCP's `TRights::GetOctal`: four digits.
pub fn octal(rights: u16) -> String {
    format!("{rights:04o}")
}

/// WinSCP's `TRights::SetOctal`: three or four octal digits; WinSCP's message when not.
pub fn rights_of_octal(text: &str) -> Result<u16, String> {
    let text = text.trim();
    let wrong = || format!("'{text}' is not valid permission in octal format.");
    if !(3..=4).contains(&text.len()) || !text.chars().all(|ch| ('0'..='7').contains(&ch)) {
        return Err(wrong());
    }
    u16::from_str_radix(text, 8).map_err(|_| wrong())
}

/// WinSCP's `SetSpeedLimit`: "Unlimited" or KB/s.
pub fn speed_text(limit: u32) -> String {
    match limit {
        0 => "Unlimited".to_string(),
        limit => (limit / 1024).to_string(),
    }
}

/// WinSCP's `GetSpeedLimit`: bytes a second of "Unlimited" or KB/s; its message when neither.
pub fn speed_of_text(text: &str) -> Result<u32, String> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("Unlimited") {
        return Ok(0);
    }
    (text.parse::<u32>().ok())
        .and_then(|kb| kb.checked_mul(1024))
        .ok_or_else(|| format!("'{text}' is not a valid speed limit."))
}

/// WinSCP's `ChangeFileName` case: `ncLowerCaseShort` lowers an upper case 8.3 name only.
fn change_case(name: &str, case: NameCase) -> String {
    match case {
        NameCase::NoChange => name.to_string(),
        NameCase::Upper => name.to_uppercase(),
        NameCase::Lower => name.to_lowercase(),
        NameCase::FirstUpper => {
            let mut chars = name.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.as_str().to_lowercase().chars())
                    .collect(),
                None => String::new(),
            }
        }
        NameCase::LowerShort => {
            let short = name.chars().count() <= 12
                && name.find('.').is_none_or(|dot| dot <= 8)
                && name == name.to_uppercase();
            if short {
                name.to_lowercase()
            } else {
                name.to_string()
            }
        }
    }
}

impl CopyParams {
    /// WinSCP's `ChangeFileName`: the first level's name masked, its case changed, a download's written
    /// as Windows takes it (`ValidLocalFileName`) and an upload's read back (`RestoreChars`).
    pub fn change_file_name(&self, name: &str, download: bool, first_level: bool) -> String {
        let name = match first_level {
            true => mask_file_name(name, &self.file_mask),
            false => name.to_string(),
        };
        let name = change_case(&name, self.name_case);
        match (self.replace_invalid, download) {
            // Unlike WinSCP, a name that would leave its folder on the disk is replaced whatever the
            // setting says: never a path out of the target.
            (false, true) if walks_out(&name) => valid_local_file_name(&name),
            (false, _) => name,
            (true, true) => valid_local_file_name(&name),
            (true, false) => restore_chars(&name),
        }
    }

    /// WinSCP's `UseAsciiTransfer` of the file `name`.
    pub fn use_text(&self, name: &str, local: bool, params: Params) -> bool {
        match self.transfer_mode {
            TransferMode::Binary => false,
            TransferMode::Text => true,
            TransferMode::Automatic => FileMasks::parse(&self.ascii_mask)
                .is_ok_and(|masks| masks.matches(name, local, false, Some(params))),
        }
    }

    /// WinSCP's `AllowResume`: a file of `size` named `name` goes through its temporary name (all, those
    /// from the threshold on, none; never a name too long for `.filepart`).
    pub fn allow_resume(&self, size: u64, name: &str) -> bool {
        if name.chars().count() + PARTIAL_EXT.len() > 255 {
            return false;
        }
        match self.resume {
            Resume::On => true,
            Resume::Off => false,
            Resume::Smart => size >= self.resume_threshold,
        }
    }

    /// WinSCP's `RemoteFileRights`: the rights, a folder's with X where it has R.
    pub fn remote_rights(&self, directory: bool) -> u16 {
        match directory && self.add_x_to_dirs {
            true => self.rights | ((self.rights & 0o444) >> 2),
            false => self.rights,
        }
    }

    /// WinSCP's `GetInfoStr` as NetBox builds it, without what `attrs` leaves out.
    pub fn info(&self, attrs: Attrs) -> String {
        let defaults = Self::default();
        let mut items = vec![self.transfer_type()];
        if self.name_case != defaults.name_case {
            let case = [
                "No change",
                "Upper case",
                "Lower case",
                "First upper case",
                "Lower case 8.3",
            ];
            items.push(format!(
                "Filename modification: {}",
                case[self.name_case as usize]
            ));
        }
        if !self.replace_invalid {
            items.push("Do not replace invalid characters".to_string());
        }
        if self.preserve_rights && !attrs.no_rights {
            let mut rights = rights_text(self.rights);
            if self.add_x_to_dirs {
                rights.push_str(", Add X to directories");
            }
            items.push(format!("Set permissions: {rights}"));
        }
        if !self.preserve_time {
            items.push("Do not preserve timestamp".to_string());
        }
        self.push_others(&mut items, attrs);
        items.join("; ")
    }

    /// `COPY_INFO_TRANSFER_TYPE2`: the automatic mode with its masks when they are not the defaults.
    fn transfer_type(&self) -> String {
        let kind = match self.transfer_mode {
            TransferMode::Binary => "Binary".to_string(),
            TransferMode::Text => "Text".to_string(),
            TransferMode::Automatic if self.ascii_mask != ASCII_MASK => {
                format!("Automatic ({})", self.ascii_mask)
            }
            TransferMode::Automatic => "Automatic".to_string(),
        };
        format!("Transfer type: {kind}")
    }

    fn push_others(&self, items: &mut Vec<String>, attrs: Attrs) {
        let rights_or_time = self.preserve_rights || self.preserve_time;
        let others = [
            (
                rights_or_time && self.ignore_perm_errors && !attrs.no_ignore_perm_errors,
                "Ignore permission errors".to_string(),
            ),
            (
                self.preserve_read_only && !attrs.no_preserve_read_only,
                "Preserve read-only".to_string(),
            ),
            (
                !self.calculate_size,
                "Do not calculate transfer size".to_string(),
            ),
            (
                self.clear_archive && !attrs.no_clear_archive,
                "Clear 'Archive' attribute".to_string(),
            ),
            (
                !self.include_mask.is_empty(),
                format!("File mask: {}", self.include_mask),
            ),
            (
                self.speed_limit > 0,
                format!("Transfer speed limit: {} KB/s", self.speed_limit / 1024),
            ),
            (self.newer_only, "New and updated files only".to_string()),
        ];
        items.extend((others.into_iter()).filter_map(|(shown, text)| shown.then_some(text)));
    }
}

#[cfg(test)]
#[path = "copy_params_tests.rs"]
mod tests;
