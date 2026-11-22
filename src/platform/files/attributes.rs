//! The attribute model: what the panels ask of an entry (hidden, system, a folder, a link) the same way on
//! every OS, worked out per OS when the entry is read. The OS's own bits stay inside for the columns
//! and dialogs that show them (FAR's attribute letters on Windows; the mode on Unix with the port).

/// An entry's attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attributes {
    hidden: bool,
    system: bool,
    directory: bool,
    link: bool,
    /// Windows' `FILE_ATTRIBUTE_*` bits; 0 where the OS has none.
    windows: u32,
}

/// Letters in the order of FAR's attribute column (`far/fileattr.cpp`, mostly as in Windows UI).
const ATTRIBUTE_LETTERS: [(char, u32); 22] = [
    ('N', 0x0000_0080),
    ('R', 0x0000_0001),
    ('H', 0x0000_0002),
    ('S', 0x0000_0004),
    ('D', 0x0000_0010),
    ('A', 0x0000_0020),
    ('T', 0x0000_0100),
    ('$', 0x0000_0200),
    ('L', 0x0000_0400),
    ('C', 0x0000_0800),
    ('O', 0x0000_1000),
    ('I', 0x0000_2000),
    ('E', 0x0000_4000),
    ('V', 0x0000_8000),
    ('?', 0x0001_0000),
    ('X', 0x0002_0000),
    ('P', 0x0008_0000),
    ('U', 0x0010_0000),
    ('‼', 0x0004_0000),
    ('!', 0x0040_0000),
    ('B', 0x2000_0000),
    ('*', 0x0000_0040),
];

/// The attributes of FAR's "Attributes" dialog (`AttributeMap`, `far/setattr.cpp`): its ten, then the ten
/// of "More". They are Windows' own; on Unix none is set (its dialog shows the mode instead).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    ReadOnly,
    Archive,
    Hidden,
    System,
    Compressed,
    Encrypted,
    NotIndexed,
    Sparse,
    Temporary,
    Offline,
    ReparsePoint,
    Virtual,
    IntegrityStream,
    NoScrubData,
    Pinned,
    Unpinned,
    RecallOnOpen,
    RecallOnDataAccess,
    StrictlySequential,
    Device,
}

impl Flag {
    /// In the order of the dialog's check boxes, "More" last.
    pub const ALL: [Self; 20] = [
        Self::ReadOnly,
        Self::Archive,
        Self::Hidden,
        Self::System,
        Self::Compressed,
        Self::Encrypted,
        Self::NotIndexed,
        Self::Sparse,
        Self::Temporary,
        Self::Offline,
        Self::ReparsePoint,
        Self::Virtual,
        Self::IntegrityStream,
        Self::NoScrubData,
        Self::Pinned,
        Self::Unpinned,
        Self::RecallOnOpen,
        Self::RecallOnDataAccess,
        Self::StrictlySequential,
        Self::Device,
    ];

    /// Windows' `FILE_ATTRIBUTE_*` bit.
    const fn bit(self) -> u32 {
        match self {
            Self::ReadOnly => READ_ONLY,
            Self::Archive => 0x0000_0020,
            Self::Hidden => HIDDEN,
            Self::System => SYSTEM,
            Self::Compressed => 0x0000_0800,
            Self::Encrypted => 0x0000_4000,
            Self::NotIndexed => 0x0000_2000,
            Self::Sparse => 0x0000_0200,
            Self::Temporary => 0x0000_0100,
            Self::Offline => 0x0000_1000,
            Self::ReparsePoint => REPARSE_POINT,
            Self::Virtual => 0x0001_0000,
            Self::IntegrityStream => 0x0000_8000,
            Self::NoScrubData => 0x0002_0000,
            Self::Pinned => 0x0008_0000,
            Self::Unpinned => 0x0010_0000,
            Self::RecallOnOpen => 0x0004_0000,
            Self::RecallOnDataAccess => 0x0040_0000,
            Self::StrictlySequential => 0x2000_0000,
            Self::Device => 0x0000_0040,
        }
    }
}

const READ_ONLY: u32 = 0x0000_0001;
const HIDDEN: u32 = 0x0000_0002;
const SYSTEM: u32 = 0x0000_0004;
const DIRECTORY: u32 = 0x0000_0010;
const NORMAL: u32 = 0x0000_0080;
const REPARSE_POINT: u32 = 0x0000_0400;

impl Attributes {
    /// From Windows' `FILE_ATTRIBUTE_*` bits.
    pub fn from_windows_bits(bits: u32) -> Self {
        Self {
            hidden: bits & HIDDEN != 0,
            system: bits & SYSTEM != 0,
            directory: bits & DIRECTORY != 0,
            link: bits & REPARSE_POINT != 0,
            windows: bits,
        }
    }

    /// From what every OS tells (Unix: hidden is a leading dot; there is no "system").
    #[cfg(not(windows))]
    pub fn from_flags(hidden: bool, directory: bool, link: bool) -> Self {
        Self {
            hidden,
            directory,
            link,
            ..Self::default()
        }
    }

    /// Windows' "read only", which FAR asks about before deleting; Unix has no such attribute (who may
    /// delete is the folder's permission).
    pub fn is_read_only(self) -> bool {
        self.windows & READ_ONLY != 0
    }

    pub fn is_hidden(self) -> bool {
        self.hidden
    }

    pub fn is_system(self) -> bool {
        self.system
    }

    /// A folder, also a link to one (a symbolic link or junction to a folder carries the attribute
    /// itself, as FAR sees it in the directory listing).
    pub fn is_directory(self) -> bool {
        self.directory
    }

    /// A symbolic link, or on Windows a junction or another reparse point (FAR's
    /// `FILE_ATTRIBUTE_REPARSE_POINT`): the entry has a [`super::LinkKind`].
    pub fn is_link(self) -> bool {
        self.link
    }

    pub fn has(self, flag: Flag) -> bool {
        self.windows & flag.bit() != 0
    }

    /// The same attributes with `flag` set (`on`) or cleared.
    pub fn with(self, flag: Flag, on: bool) -> Self {
        let bits = if on {
            self.windows | flag.bit()
        } else {
            self.windows & !flag.bit()
        };
        if cfg!(windows) {
            Self::from_windows_bits(bits)
        } else {
            Self {
                windows: bits,
                ..self
            }
        }
    }

    /// The same with no folder in it (a folder's stream, which is a file).
    pub fn without_directory(self) -> Self {
        Self {
            directory: false,
            windows: self.windows & !DIRECTORY,
            ..self
        }
    }

    /// Windows' `FILE_ATTRIBUTE_*` bits, 0 where the OS has none (FAR's filters keep their conditions so).
    pub fn windows_flags(self) -> u32 {
        self.windows
    }

    /// Windows' bits, for the calls that set them (`platform` only).
    #[cfg(windows)]
    pub(in crate::platform) fn windows_bits(self) -> u32 {
        self.windows
    }

    /// FAR's attribute column: the letters of the set attributes, at most `width` of them.
    pub fn letters(self, width: usize) -> String {
        let bits = if self.windows == 0 {
            NORMAL
        } else {
            self.windows
        };
        ATTRIBUTE_LETTERS
            .iter()
            .filter(|(_, bit)| bits & bit != 0)
            .map(|(letter, _)| *letter)
            .take(width)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_letters_follow_far_order() {
        let bits = Attributes::from_windows_bits;
        assert_eq!(bits(0).letters(6), "N");
        assert_eq!(bits(0x27).letters(6), "RHSA");
        assert_eq!(bits(0x827).letters(3), "RHS");
        assert!(bits(HIDDEN).is_hidden());
        assert!(bits(SYSTEM).is_system());
        let folder_link = bits(DIRECTORY | REPARSE_POINT);
        assert!(folder_link.is_directory() && folder_link.is_link());
        assert!(!bits(REPARSE_POINT).is_directory());
    }

    #[test]
    fn dialog_flags_are_set_and_cleared_one_by_one() {
        let plain = Attributes::from_windows_bits(DIRECTORY);
        let hidden = plain.with(Flag::Hidden, true).with(Flag::Archive, true);
        assert!(hidden.has(Flag::Hidden) && hidden.has(Flag::Archive) && hidden.is_directory());
        assert_eq!(cfg!(windows), hidden.is_hidden());
        let back = hidden.with(Flag::Hidden, false);
        assert!(!back.has(Flag::Hidden) && back.has(Flag::Archive));
        assert!(Flag::ALL.iter().all(|flag| !plain.has(*flag)));
        let bits: Vec<u32> = Flag::ALL.iter().map(|flag| flag.bit()).collect();
        assert!(
            (1..bits.len()).all(|at| !bits[..at].contains(&bits[at])),
            "one bit each"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn flags_say_the_same_on_every_os() {
        let dot_file = Attributes::from_flags(true, false, false);
        assert!(dot_file.is_hidden());
        assert!(!dot_file.is_system() && !dot_file.is_directory() && !dot_file.is_link());
        assert_eq!(dot_file.letters(6), "N");
    }
}
