//! The formats "Create archive" offers (arclite's `UpdateDialog::show`, its main and other formats): 7z
//! and zip by their own buttons, every other format 7-Zip's library writes in "Other", a format of one
//! file only (gzip, bzip2, xz) not for several files or a folder, SWFc only for one uncompressed Flash
//! file; the others ordered by their library, then by name whatever its case. Each one's extension is
//! its first (arclite's `default_extension`).

use std::path::Path;

use nc_plugin_sdk::sevenzip::Format;

/// A format offered and its extension (with its dot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archiver {
    pub name: String,
    pub extension: String,
}

/// What the dialog offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archivers {
    /// The library writes 7z (its button enabled).
    pub seven: Option<Archiver>,
    pub zip: Archiver,
    pub others: Vec<Archiver>,
}

fn archiver(format: &Format) -> Archiver {
    let extension = (format.extensions.first()).map_or_else(String::new, |ext| format!(".{ext}"));
    Archiver {
        name: format.name.clone(),
        extension,
    }
}

impl Archivers {
    /// No library: zip only (the `zip` crate's).
    pub fn zip_only() -> Self {
        Self {
            seven: None,
            zip: Archiver {
                name: "zip".to_string(),
                extension: ".zip".to_string(),
            },
            others: Vec::new(),
        }
    }

    /// The library's `formats` offered for `sources` (several, or a folder: `multifile`).
    pub fn of(formats: &[Format], sources: &[std::path::PathBuf]) -> Self {
        let multifile = sources.len() > 1 || sources.first().is_some_and(|source| source.is_dir());
        let swf = !multifile
            && sources
                .first()
                .is_some_and(|source| uncompressed_swf(source));
        let mut offered = Self::zip_only();
        offered.seven = (formats.iter())
            .find(|format| format.name == "7z" && format.updatable)
            .map(archiver);
        let mut others: Vec<&Format> = (formats.iter())
            .filter(|format| format.updatable && format.name != "7z" && format.name != "zip")
            .filter(|format| !(multifile && single_file(&format.name)))
            .filter(|format| format.name != "SWFc" || swf)
            .collect();
        others.sort_by(|one, other| {
            (one.library, one.name.to_lowercase()).cmp(&(other.library, other.name.to_lowercase()))
        });
        offered.others = others.into_iter().map(archiver).collect();
        offered
    }

    /// The format `name` when it is offered.
    pub fn find(&self, name: &str) -> Option<&Archiver> {
        let named = |archiver: &&Archiver| archiver.name.eq_ignore_ascii_case(name);
        (self.seven.iter())
            .chain([&self.zip])
            .chain(self.others.iter())
            .find(named)
    }

    /// The extension of the format `name` (none when it is not offered).
    pub fn extension(&self, name: &str) -> String {
        self.find(name)
            .map_or_else(String::new, |archiver| archiver.extension.clone())
    }
}

/// arclite's `is_single_file_format` (SWFc is one too).
fn single_file(name: &str) -> bool {
    matches!(name, "bzip2" | "gzip" | "xz" | "SWFc")
}

/// arclite's `is_SWFu`: a Flash file not compressed ("FWS" and a version under 20).
fn uncompressed_swf(path: &Path) -> bool {
    let mut head = [0u8; 8];
    let read = std::fs::File::open(path).and_then(|mut file| {
        use std::io::Read;
        file.read_exact(&mut head)
    });
    read.is_ok() && &head[..3] == b"FWS" && head[3] < 20
}

#[cfg(test)]
#[path = "create_formats_tests.rs"]
mod tests;
