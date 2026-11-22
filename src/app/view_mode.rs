//! FAR's file panel view modes: the predefined ones (Ctrl+1 ... Ctrl+0, `far/config.cpp`
//! `InitialModes`) and the user's own (Options > File panel modes).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Ctrl+1: three name columns.
    Brief,
    /// Ctrl+2: two name columns; FAR's default.
    #[default]
    Medium,
    /// Ctrl+3: name, size, date, time.
    Full,
    /// Ctrl+4: name and size.
    Wide,
    /// Ctrl+5: name, size, write/creation/access time, attributes; uses the whole screen width.
    Detailed,
    /// Ctrl+0: selection mark, name, size with grouped digits, date.
    AlternativeFull,
    /// The user's own mode at this place.
    Custom(usize),
    /// Ctrl+6..9 on a provider's panel with its own mode there (ADR-0016 §13): the program has none of
    /// its own at these keys yet (FAR's Descriptions, Long descriptions, File owners, File links).
    Provider(usize),
}

impl ViewMode {
    /// The mode bound to Ctrl+`digit`, as in FAR (6-9 are deferred, see docs/far-parity.md).
    pub fn for_digit(digit: char) -> Option<Self> {
        match digit {
            '1' => Some(Self::Brief),
            '2' => Some(Self::Medium),
            '3' => Some(Self::Full),
            '4' => Some(Self::Wide),
            '5' => Some(Self::Detailed),
            '0' => Some(Self::AlternativeFull),
            _ => None,
        }
    }

    /// Ctrl+`digit` for a provider's own mode where the program has none (6..9).
    pub fn provider_digit(digit: char) -> Option<Self> {
        let number = usize::try_from(digit.to_digit(10)?).ok()?;
        (6..=9).contains(&number).then_some(Self::Provider(number))
    }

    /// The mode at Ctrl+`digit`: the program's, or a provider's at 6..9.
    pub fn for_key(digit: char) -> Option<Self> {
        Self::for_digit(digit).or_else(|| Self::provider_digit(digit))
    }

    /// The mode at Ctrl+`number` (0..9): [`ViewMode::for_key`].
    pub fn for_number(number: usize) -> Option<Self> {
        Self::for_key(char::from_digit(u32::try_from(number).ok()?, 10)?)
    }

    /// FAR's number of the mode: the Ctrl+digit it is at (a user's mode has none).
    pub fn number(self) -> Option<usize> {
        match self {
            Self::AlternativeFull => Some(0),
            Self::Brief => Some(1),
            Self::Medium => Some(2),
            Self::Full => Some(3),
            Self::Wide => Some(4),
            Self::Detailed => Some(5),
            Self::Provider(number) => Some(number),
            Self::Custom(_) => None,
        }
    }

    /// Identifier in settings.json; a user's mode is `custom-1`, `custom-2`...
    pub fn id(self) -> String {
        let id = match self {
            Self::Brief => "brief",
            Self::Medium => "medium",
            Self::Full => "full",
            Self::Wide => "wide",
            Self::Detailed => "detailed",
            Self::AlternativeFull => "alternative",
            Self::Custom(at) => return format!("custom-{}", at + 1),
            Self::Provider(number) => return format!("provider-{number}"),
        };
        id.to_string()
    }

    pub fn from_id(id: impl AsRef<str>) -> Self {
        let id = id.as_ref();
        if let Some(at) = (id.strip_prefix("custom-"))
            .and_then(|number| number.parse::<usize>().ok())
            .and_then(|number| number.checked_sub(1))
        {
            return Self::Custom(at);
        }
        [
            Self::Brief,
            Self::Medium,
            Self::Full,
            Self::Wide,
            Self::Detailed,
            Self::AlternativeFull,
        ]
        .into_iter()
        .find(|mode| mode.id() == id)
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_map_to_far_modes() {
        assert_eq!(ViewMode::for_digit('1'), Some(ViewMode::Brief));
        assert_eq!(ViewMode::for_digit('0'), Some(ViewMode::AlternativeFull));
        assert_eq!(ViewMode::for_digit('7'), None);
    }

    #[test]
    fn ctrl_6_to_9_are_a_providers_and_each_key_has_its_number() {
        for (digit, number) in [('6', 6), ('7', 7), ('8', 8), ('9', 9)] {
            assert_eq!(
                ViewMode::provider_digit(digit),
                Some(ViewMode::Provider(number))
            );
            assert_eq!(ViewMode::Provider(number).number(), Some(number));
        }
        for digit in ['0', '5', 'x'] {
            assert_eq!(ViewMode::provider_digit(digit), None, "{digit}");
        }
        for (digit, number) in [('0', 0), ('1', 1), ('2', 2), ('3', 3), ('4', 4), ('5', 5)] {
            assert_eq!(ViewMode::for_digit(digit).unwrap().number(), Some(number));
        }
        assert_eq!(
            ViewMode::Custom(0).number(),
            None,
            "a user's mode has no key"
        );
        for number in 0..=9 {
            assert_eq!(ViewMode::for_number(number).unwrap().number(), Some(number));
        }
        assert_eq!(ViewMode::for_number(10), None);
        assert_eq!(ViewMode::Provider(7).id(), "provider-7");
        assert_eq!(
            ViewMode::from_id("provider-7"),
            ViewMode::Medium,
            "never shown again by itself"
        );
    }

    #[test]
    fn ids_round_trip_and_unknown_falls_back_to_medium() {
        for digit in ['1', '2', '3', '4', '5', '0'] {
            let mode = ViewMode::for_digit(digit).unwrap();
            assert_eq!(ViewMode::from_id(mode.id()), mode);
        }
        assert_eq!(ViewMode::from_id("bogus"), ViewMode::Medium);
    }
}
