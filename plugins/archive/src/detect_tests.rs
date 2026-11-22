use super::*;

fn format(name: &str, extensions: &[&str], signatures: &[&[u8]]) -> Format {
    Format {
        name: name.to_string(),
        extensions: extensions.iter().map(|ext| (*ext).to_string()).collect(),
        signatures: signatures.iter().map(|bytes| bytes.to_vec()).collect(),
        ..Format::default()
    }
}

/// 0 7z, 1 zip, 2 tar (its signature 257 bytes in), 3 gzip, 4 Iso, 5 Udf, 6 Split (by extension only).
fn all() -> Vec<Format> {
    let mut tar = format("tar", &["tar"], &[b"ustar"]);
    tar.signature_offset = 257;
    let mut split = format("Split", &["001"], &[]);
    split.flags = flags::BY_EXT_ONLY_OPEN;
    vec![
        format("7z", &["7z"], &[b"7z\xBC\xAF"]),
        format("zip", &["zip", "jar"], &[b"PK\x03\x04", b"PK\x05\x06"]),
        tar,
        format("gzip", &["gz", "tgz"], &[b"\x1F\x8B"]),
        format("Iso", &["iso", "img"], &[]),
        format("Udf", &["udf", "iso", "img"], &[]),
        split,
    ]
}

fn yes(_format: usize, _data: &[u8]) -> IsArc {
    IsArc::Yes
}

fn run(all: &[Format], buffer: &[u8], extension: &str) -> Vec<(usize, u64)> {
    let enabled: Vec<usize> = (0..all.len()).collect();
    let formats = Formats {
        all,
        enabled: &enabled,
        is_arc: &yes,
    };
    let entries = detect(&formats, buffer, true, extension, &mut || None);
    entries
        .iter()
        .map(|entry| (entry.format, entry.offset))
        .collect()
}

/// The signatures found first in their places' order (an archive after a stub at its place), then every
/// other format but those opened by extension only.
#[test]
fn signatures_come_first_in_their_order() {
    let mut buffer = b"MZ stub..".to_vec();
    buffer.extend_from_slice(b"7z\xBC\xAF rest");
    assert_eq!(
        run(&all(), &buffer, ""),
        [(0, 9), (1, 0), (2, 0), (3, 0), (5, 0), (4, 0)]
    );
}

/// A signature with an offset: the archive starts that much before it; none found before the offset.
#[test]
fn a_signatures_offset_is_its_archives_start() {
    let mut buffer = vec![0u8; 257];
    buffer.extend_from_slice(b"ustar");
    let entries = run(&all(), &buffer, "");
    assert_eq!(entries[0], (2, 0));
    let early = run(&all(), b"ustar", "");
    assert!(!early.contains(&(2, 0)) || early[0] != (2, 0));
}

/// The extension's formats before everything (the last of them first), not twice; its case does not
/// matter; a format opened by extension only is tried by it.
#[test]
fn the_extensions_formats_come_before() {
    assert_eq!(run(&all(), b"PK\x03\x04", ".JAR")[0], (1, 0));
    let iso = run(&all(), b"nothing", ".iso");
    assert_eq!(&iso[..2], [(5, 0), (4, 0)]);
    let zip = run(&all(), b"PK\x03\x04", ".zip");
    assert_eq!(zip.iter().filter(|entry| entry.0 == 1).count(), 1);
    assert_eq!(run(&all(), b"x", ".001")[0], (6, 0));
}

/// UDF before ISO even when ISO's extension comes first.
#[test]
fn udf_goes_before_iso() {
    let mut formats = all();
    formats[5].extensions = vec!["udf".to_string()];
    let entries = run(&formats, b"x", ".iso");
    let udf = entries.iter().position(|entry| entry.0 == 5).unwrap();
    let iso = entries.iter().position(|entry| entry.0 == 4).unwrap();
    assert!(udf < iso, "{entries:?}");
}

/// A format's quick look refusing its signature drops that place; asking for more takes it only when
/// the buffer is not the whole file.
#[test]
fn the_quick_look_decides() {
    let formats = all();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let no_gzip = |format: usize, _: &[u8]| {
        if format == 3 {
            IsArc::NeedMore
        } else {
            IsArc::Yes
        }
    };
    let with = Formats {
        all: &formats,
        enabled: &enabled,
        is_arc: &no_gzip,
    };
    let whole = detect(&with, b"\x1F\x8B", true, "", &mut || None);
    assert_ne!(
        whole[0],
        Entry {
            format: 3,
            offset: 0
        },
        "the whole file: not taken"
    );
    let part = detect(&with, b"\x1F\x8B", false, "", &mut || None);
    assert_eq!(
        part[0],
        Entry {
            format: 3,
            offset: 0
        }
    );
}

/// Only the enabled formats are tried.
#[test]
fn only_enabled_formats_are_tried() {
    let formats = all();
    let enabled = [0, 3];
    let with = Formats {
        all: &formats,
        enabled: &enabled,
        is_arc: &yes,
    };
    let entries = detect(&with, b"PK\x03\x04", true, ".zip", &mut || None);
    let tried: Vec<usize> = entries.iter().map(|entry| entry.format).collect();
    assert_eq!(tried, [0, 3]);
}

/// The end record of a zip's file, its disk number `disk`.
fn end_record(disk: u8) -> Vec<u8> {
    let mut record = b"PK\x05\x06".to_vec();
    record.extend_from_slice(&[disk, 0]);
    record.extend_from_slice(&[0; 16]);
    record
}

/// A zip local header after the start (an SFX's archive) is taken when the file's end record is the first
/// volume's, passed over (and every later one) when it names another volume.
#[test]
fn a_later_volumes_local_header_is_passed_over() {
    let mut first = b"stub".to_vec();
    first.extend_from_slice(b"PK\x03\x04");
    first.extend_from_slice(&[0; 40]);
    first.extend_from_slice(&end_record(0));
    assert!(run(&all(), &first, "").contains(&(1, 4)));
    let mut later = b"stub".to_vec();
    later.extend_from_slice(b"PK\x03\x04");
    later.extend_from_slice(&[0; 40]);
    later.extend_from_slice(&end_record(2));
    let entries = run(&all(), &later, "");
    assert!(!entries.contains(&(1, 4)), "{entries:?}");
}

/// Not the whole file: its last 16 KB read for the end record.
#[test]
fn the_files_end_is_read_when_the_start_is_not_all() {
    let formats = all();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let with = Formats {
        all: &formats,
        enabled: &enabled,
        is_arc: &yes,
    };
    let buffer = b"stubPK\x03\x04".to_vec();
    let mut last = vec![0u8; TAIL_CHECK];
    let record = end_record(1);
    let at = TAIL_CHECK - MIN_END;
    last[at..at + record.len()].copy_from_slice(&record);
    let mut reads = 0;
    let entries = detect(&with, &buffer, false, "", &mut || {
        reads += 1;
        Some(last.clone())
    });
    assert_eq!(reads, 1);
    assert!(!entries.contains(&Entry {
        format: 1,
        offset: 4
    }));
    let short = detect(&with, &buffer, false, "", &mut || Some(vec![0; 10]));
    assert!(short.contains(&Entry {
        format: 1,
        offset: 4
    }));
}

/// The same signature in two formats: the later one's (arclite's state matrix keeps one).
#[test]
fn a_shared_signature_is_the_later_formats() {
    let mut formats = all();
    formats.push(format("other", &[], &[b"7z\xBC\xAF"]));
    let entries = run(&formats, b"7z\xBC\xAF", "");
    assert_eq!(entries[0], (7, 0));
    assert!(rfind(b"abcab", b"ab") == Some(3) && rfind(b"a", b"ab").is_none());
}

/// The extension's formats go before those its signatures found; several of one extension each put
/// first in turn (the last of them first); each format tried once.
#[test]
fn the_extensions_formats_go_before_the_signatures() {
    let entries = run(&all(), b"7z\xBC\xAF", ".gz");
    assert_eq!(&entries[..2], [(3, 0), (0, 0)]);
    let mut formats = all();
    formats[2].extensions.push("x".to_string());
    formats[3].extensions.push("x".to_string());
    let shared = run(&formats, b"nothing", ".x");
    assert_eq!(&shared[..2], [(3, 0), (2, 0)]);
    let iso = run(&all(), b"nothing", ".iso");
    for format in 0..6 {
        let times = iso.iter().filter(|entry| entry.0 == format).count();
        assert_eq!(times, 1, "{format}: {iso:?}");
    }
    assert_eq!(run(&all(), b"nothing", ".JAR")[0], (1, 0));
    assert_eq!(run(&all(), b"nothing", ".Zip")[0], (1, 0));
    assert_eq!(run(&all(), b"nothing", "")[0], (0, 0));
}

fn at(format: usize, offset: u64) -> Entry {
    Entry { format, offset }
}

/// arclite's `prioritize`: the first `first` moved before the last `second` ahead of it, once; nothing
/// when `first` comes first or is not there.
#[test]
fn prioritize_moves_the_first_before_the_last_second_ahead() {
    let mut entries = vec![at(4, 0), at(9, 0), at(5, 0)];
    prioritize(&mut entries, 5, 4);
    assert_eq!(entries, [at(5, 0), at(4, 0), at(9, 0)]);
    let mut two = vec![at(4, 0), at(4, 7), at(5, 0)];
    prioritize(&mut two, 5, 4);
    assert_eq!(two, [at(4, 0), at(5, 0), at(4, 7)]);
    let mut once = vec![at(4, 0), at(5, 0), at(4, 7), at(5, 7)];
    prioritize(&mut once, 5, 4);
    assert_eq!(once, [at(5, 0), at(4, 0), at(4, 7), at(5, 7)]);
    let mut ahead = vec![at(5, 0), at(4, 0)];
    prioritize(&mut ahead, 5, 4);
    assert_eq!(ahead, [at(5, 0), at(4, 0)]);
    let mut none = vec![at(4, 0), at(9, 0)];
    prioritize(&mut none, 5, 4);
    assert_eq!(none, [at(4, 0), at(9, 0)]);
}

/// RAR before Split (a `.001` that is a RAR's first volume); DMG before HFS, DMG's signature never
/// searched for (arclite leaves it out).
#[test]
fn rar_goes_before_split_and_dmg_before_hfs() {
    let mut split = format("Split", &["001"], &[]);
    split.flags = flags::BY_EXT_ONLY_OPEN;
    let rars = [split, format("Rar", &["rar"], &[b"Rar!\x1A\x07"])];
    assert_eq!(
        run(&rars, b"Rar!\x1A\x07\x01\x00", ".001"),
        [(1, 0), (0, 0)]
    );
    let disks = [
        format("HFS", &["hfs"], &[b"H+"]),
        format("Dmg", &["dmg"], &[b"koly"]),
    ];
    let mut buffer = b"H+".to_vec();
    buffer.extend_from_slice(&[0; 10]);
    buffer.extend_from_slice(b"koly");
    assert_eq!(run(&disks, &buffer, ""), [(1, 0), (0, 0)]);
}

/// A format's empty signature is not searched for (nor does it stop the search).
#[test]
fn an_empty_signature_is_passed_over() {
    let mut formats = all();
    formats[4].signatures = vec![Vec::new()];
    assert_eq!(run(&formats, b"\x1F\x8B", "")[0], (3, 0));
}

/// The quick look is given the bytes from the archive's start (its signature's offset before the
/// signature) to the buffer's end; a refusal drops the place whether the buffer is all of the file or not.
#[test]
fn the_quick_look_sees_the_archive_from_its_start() {
    let formats = all();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let seen = std::cell::RefCell::new(Vec::new());
    let look = |format: usize, data: &[u8]| {
        seen.borrow_mut()
            .push((format, data.len(), data.first().copied()));
        if format == 3 { IsArc::No } else { IsArc::Yes }
    };
    let with = Formats {
        all: &formats,
        enabled: &enabled,
        is_arc: &look,
    };
    let mut buffer = vec![7u8; 257];
    buffer.extend_from_slice(b"ustar");
    let entries = detect(&with, &buffer, true, "", &mut || None);
    assert_eq!(entries[0], at(2, 0));
    assert_eq!(seen.borrow().as_slice(), [(2, 262, Some(7))]);
    for eof in [true, false] {
        let gzip = detect(&with, b"\x1F\x8B", eof, "", &mut || None);
        assert_ne!(gzip[0], at(3, 0), "eof {eof}");
    }
}

/// The buffer's start, the bytes from `offset`, the file's end; as one file.
fn zip_file(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

/// The zip formats' entries `run` finds in `bytes` at `offset`.
fn zip_at(bytes: &[u8], offset: u64) -> bool {
    run(&all(), bytes, "").contains(&(1, offset))
}

/// A local header at the file's start is taken whatever the end record says: it is tried first.
#[test]
fn a_local_header_at_the_start_is_always_taken() {
    let bytes = zip_file(&[b"PK\x03\x04", &[0; 40], &end_record(2)]);
    assert_eq!(run(&all(), &bytes, "")[0], (1, 0));
}

/// After a local header refused none is taken, nor read for; another format's signature after it is.
#[test]
fn after_one_refused_no_local_header_is_taken() {
    let bytes = zip_file(&[
        b"stub",
        b"PK\x03\x04",
        &[0; 40],
        b"PK\x03\x04",
        &[0; 40],
        b"7z\xBC\xAF",
        &end_record(2),
    ]);
    let entries = run(&all(), &bytes, "");
    assert!(!entries.contains(&(1, 4)) && !entries.contains(&(1, 48)));
    assert!(entries.contains(&(0, 92)), "{entries:?}");
}

/// The end record is looked for from 30 bytes after the local header (its least size) to where a whole
/// one fits; its disk number's two bytes both 0 for the first volume; the last record decides.
#[test]
fn the_end_record_is_looked_for_where_it_fits() {
    let right_after = zip_file(&[b"stub", b"PK\x03\x04", &[0; 26], &end_record(2)]);
    assert!(!zip_at(&right_after, 4));
    let inside = zip_file(&[b"stub", b"PK\x03\x04", &[0; 25], &end_record(2)]);
    assert!(zip_at(&inside, 4));
    let cut = zip_file(&[b"stub", b"PK\x03\x04", &[0; 40], &end_record(2)[..20]]);
    assert!(zip_at(&cut, 4));
    let mut high = end_record(0);
    high[5] = 1;
    let high_disk = zip_file(&[b"stub", b"PK\x03\x04", &[0; 40], &high]);
    assert!(!zip_at(&high_disk, 4));
    let last_first = zip_file(&[
        b"stub",
        b"PK\x03\x04",
        &[0; 40],
        &end_record(2),
        &end_record(0),
    ]);
    assert!(zip_at(&last_first, 4));
    let last_later = zip_file(&[
        b"stub",
        b"PK\x03\x04",
        &[0; 40],
        &end_record(0),
        &end_record(2),
    ]);
    assert!(!zip_at(&last_later, 4));
}

/// Only the last 16 KB before the record's least size is searched.
#[test]
fn only_the_files_last_16_kb_are_searched() {
    let record = end_record(2);
    let far = zip_file(&[
        b"stub",
        b"PK\x03\x04",
        &[0; 40],
        &record,
        &[0; TAIL_CHECK - 3],
    ]);
    assert!(zip_at(&far, 4), "the record just out of reach");
    let near = zip_file(&[
        b"stub",
        b"PK\x03\x04",
        &[0; 40],
        &record,
        &[0; TAIL_CHECK - 4],
    ]);
    assert!(!zip_at(&near, 4), "the record first in reach");
}

/// Not the whole file: its last 16 KB read once, whatever the local headers; a record that does not
/// fit whole before its end not seen; none read when the buffer is all of the file.
#[test]
fn the_files_end_is_read_once() {
    let formats = all();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let with = Formats {
        all: &formats,
        enabled: &enabled,
        is_arc: &yes,
    };
    let tail_with = |at: usize| {
        let mut last = vec![0u8; TAIL_CHECK];
        let record = end_record(1);
        let end = (at + record.len()).min(TAIL_CHECK);
        last[at..end].copy_from_slice(&record[..end - at]);
        last
    };
    let buffer = b"stubPK\x03\x04morePK\x03\x04".to_vec();
    let mut reads = 0;
    let last = tail_with(TAIL_CHECK - MIN_END);
    let entries = detect(&with, &buffer, false, "", &mut || {
        reads += 1;
        Some(last.clone())
    });
    assert_eq!(reads, 1);
    assert!(!entries.contains(&at(1, 4)) && !entries.contains(&at(1, 12)));
    let late = tail_with(TAIL_CHECK - MIN_END + 1);
    let past = detect(&with, &buffer[..8], false, "", &mut || Some(late.clone()));
    assert!(past.contains(&at(1, 4)), "{past:?}");
    let mut whole_reads = 0;
    let whole = detect(&with, &buffer, true, "", &mut || {
        whole_reads += 1;
        None
    });
    assert_eq!(whole_reads, 0);
    assert!(whole.contains(&at(1, 4)) && whole.contains(&at(1, 12)));
}
