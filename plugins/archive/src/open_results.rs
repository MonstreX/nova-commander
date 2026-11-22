//! What 7-Zip's library tells of an archive as it opened it (arclite's `Archive::read_open_results`,
//! `get_open_errors`, `get_open_warnings`): its error and warning flags in arclite's words, the texts it
//! gives, the data after the archive's end (a warning) or an archive longer than its file (an error) by
//! its physical size. A test tells them with its own failures (arclite's `test_files`, `bulk_test`).

use nc_plugin_sdk::sevenzip::{Archive, Prop, prop};

/// 7-Zip's `kpv_ErrorFlags_*` and arclite's words for each (`flags2texts`).
const FLAGS: [(u32, &str); 11] = [
    (1, "Is not archive"),
    (2, "Headers Error"),
    (4, "EncryptedHeadersError"),
    (8, "Unavailable data"),
    (16, "UnconfirmedStart"),
    (32, "Unexpected end of data"),
    (64, "There are some data after the end of the payload data"),
    (128, "Unsupported compression method"),
    (256, "UnsupportedFeature"),
    (512, "Data error"),
    (1024, "CRC check failed"),
];

const UNEXPECTED_END: u32 = 32;
const DATA_AFTER_END: u32 = 64;

/// The archive's open errors and warnings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenResults {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// arclite's `flags2texts`: each flag's words in its order, the bits left as "Unknown error: N".
pub fn flag_texts(flags: u32) -> Vec<String> {
    let mut left = flags;
    let mut texts = Vec::new();
    for (flag, text) in FLAGS {
        if left & flag == flag {
            texts.push(text.to_string());
            left &= !flag;
        }
    }
    if left != 0 {
        texts.push(format!("Unknown error: {left}"));
    }
    texts
}

fn flags_of(archive: &Archive, id: u32) -> u32 {
    match archive.archive_property(id) {
        Prop::U32(flags) => flags,
        Prop::U64(flags) => u32::try_from(flags).unwrap_or(u32::MAX),
        _ => 0,
    }
}

fn text_of(archive: &Archive, id: u32) -> Option<String> {
    archive
        .archive_property(id)
        .text()
        .map(str::to_string)
        .filter(|text| !text.is_empty())
}

/// arclite's `read_open_results` of `archive` (its file `file_size` long, when it is the file's own; none
/// for one inside another): the flags and texts, the physical size against the file's.
pub fn read(archive: &Archive, file_size: Option<u64>) -> OpenResults {
    let mut errors = flags_of(archive, prop::ERROR_FLAGS);
    let mut warnings = flags_of(archive, prop::WARNING_FLAGS);
    let physical = archive
        .archive_property(prop::PHYSICAL_SIZE)
        .number()
        .unwrap_or(0);
    if let Some(file_size) = file_size
        && physical != 0
    {
        let offset = archive.archive_property(prop::OFFSET).number().unwrap_or(0);
        let end = offset.saturating_add(physical);
        if end < file_size {
            warnings |= DATA_AFTER_END;
        } else if end > file_size {
            errors |= UNEXPECTED_END;
        }
    }
    let mut results = OpenResults {
        errors: flag_texts(errors),
        warnings: flag_texts(warnings),
    };
    results.errors.extend(text_of(archive, prop::ERROR));
    results.warnings.extend(text_of(archive, prop::WARNING));
    results
}

#[cfg(test)]
#[path = "open_results_tests.rs"]
mod tests;
