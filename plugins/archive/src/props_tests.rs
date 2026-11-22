use super::*;

/// arclite's names by id; a format's own name for one it does not know, else the number.
#[test]
fn properties_are_named_as_arclite_names_them() {
    assert_eq!(name(prop::PATH, None), "Path");
    assert_eq!(name(prop::PACKED_SIZE, None), "Packed Size");
    assert_eq!(name(prop::PHYSICAL_SIZE, Some("own")), "Physical Size");
    assert_eq!(name(prop::POSIX_ATTRIBUTES, None), "Mode");
    assert_eq!(name(prop::CHANGED, None), "Metadata Changed");
    assert_eq!(name(104, None), "Dev Minor");
    assert_eq!(name(105, Some("Own")), "Own");
    assert_eq!(name(1, None), "1");
}

/// The values arclite has a function of its own for: sizes short and whole, a CRC in hex by its width,
/// attributes and a Unix mode by letters; nothing for a value of another type.
#[test]
fn shaped_values_are_written_as_arclite_writes_them() {
    assert_eq!(value_text(prop::SIZE, &Prop::U64(5)), "5");
    assert_eq!(
        value_text(prop::SIZE, &Prop::U64(123_456)),
        size_text(123_456)
    );
    assert_eq!(value_text(prop::PHYSICAL_SIZE, &Prop::Text("x".into())), "");
    assert_eq!(value_text(prop::CRC, &Prop::U32(0xAB)), "000000AB");
    assert_eq!(value_text(prop::CRC, &Prop::U64(1)), "0000000000000001");
    assert_eq!(
        value_text(prop::ATTRIBUTES, &Prop::U32(0x21)),
        attributes_text(0x21)
    );
    assert_eq!(
        value_text(prop::POSIX_ATTRIBUTES, &Prop::U32(0o100_644)),
        posix_text(0o100_644)
    );
    assert_eq!(value_text(prop::MODIFIED, &Prop::U32(1)), "");
    assert!(!value_text(prop::MODIFIED, &Prop::Time(133_000_000_000_000_000)).is_empty());
}

/// The others by their type: texts on one line, yes / no, numbers; nothing for none.
#[test]
fn plain_values_are_written_by_their_type() {
    assert_eq!(
        value_text(prop::METHOD, &Prop::Text("LZMA2:12\r\nx".into())),
        "LZMA2:12  x"
    );
    assert_eq!(value_text(prop::SOLID, &Prop::Bool(true)), "Yes");
    assert_eq!(value_text(prop::SOLID, &Prop::Bool(false)), "No");
    assert_eq!(value_text(38, &Prop::U32(2)), "2");
    assert_eq!(value_text(38, &Prop::U64(3)), "3");
    assert_eq!(value_text(38, &Prop::I64(-4)), "-4");
    assert_eq!(value_text(prop::METHOD, &Prop::Empty), "");
    assert_eq!(value_text(prop::CHANGED, &Prop::U64(0)), "0");
}

/// Every property arclite writes by `format_size_prop`, of either width; one it does not, as a number.
#[test]
fn every_size_property_is_written_as_a_size() {
    let sizes = [7, 8, 18, 44, 45, 52, 56, 57, 58, 76, 77, 78, 79, 87, 88];
    let short = size_text(123_456);
    assert_ne!(short, "123456");
    for id in sizes {
        assert_eq!(value_text(id, &Prop::U64(123_456)), short, "{id}");
        assert_eq!(value_text(id, &Prop::U32(123_456)), short, "{id}");
    }
    for id in [6, 9, 17, 19, 36, 43, 46, 53, 55, 59, 75, 80, 86, 89] {
        assert_ne!(value_text(id, &Prop::U64(123_456)), short, "{id}");
    }
    assert_eq!(value_text(prop::OFFSET, &Prop::U64(123_456)), "123456");
}

/// A time of any property as a time, one the system cannot convert as nothing; a shaped property's
/// value of a type its function does not take, binary data and a type not read as nothing.
#[test]
fn values_of_other_types_are_nothing() {
    let time = Prop::Time(133_000_000_000_000_000);
    let written = value_text(prop::MODIFIED, &time);
    assert!(!written.is_empty());
    assert_eq!(value_text(prop::COMMENT, &time), written);
    assert_eq!(value_text(prop::MODIFIED, &Prop::Time(u64::MAX)), "");
    assert_eq!(value_text(prop::CRC, &Prop::Text("1".into())), "");
    assert_eq!(value_text(prop::ATTRIBUTES, &Prop::U64(0x20)), "");
    assert_eq!(value_text(prop::POSIX_ATTRIBUTES, &Prop::U64(0o644)), "");
    assert_eq!(value_text(prop::METHOD, &Prop::Bytes(vec![1])), "");
    assert_eq!(value_text(prop::METHOD, &Prop::Other(99)), "");
    assert_eq!(value_text(prop::COMMENT, &Prop::Text("a\nb".into())), "a b");
}

/// The ratio rounded, at most 100, an empty archive's 100.
#[test]
fn the_ratio_is_rounded() {
    assert_eq!(ratio(50, 100), 50);
    assert_eq!(ratio(1, 3), 33);
    assert_eq!(ratio(2, 3), 67);
    assert_eq!(ratio(300, 100), 100);
    assert_eq!(ratio(10, 0), 100);
}

/// A 7z's info lines (through 7-Zip's installed library): the path first, its own properties, then the
/// ratio, the sizes and the counts.
#[test]
fn a_7zs_info_lines_are_arclites() {
    use nc_plugin_sdk::sevenzip::{InStream, Library, Opening};
    struct Quiet;
    impl Opening for Quiet {}
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("a.7z");
    std::fs::write(&path, include_bytes!("../testdata/a.7z")).unwrap();
    let library = Library::find(folder.path());
    let seven = (library.formats().iter())
        .position(|format| format.name == "7z")
        .unwrap();
    let stream = InStream::file(&path).unwrap();
    let archive = (library.open(seven, &stream, Some(&path), false, &mut Quiet))
        .unwrap()
        .unwrap();
    let items: Vec<Item> = (0..archive.count())
        .map(|index| Item {
            is_dir: archive.property(index, prop::IS_DIR) == Prop::Bool(true),
            ..Item::default()
        })
        .collect();
    let lines = info_lines("C:\\a.7z", &archive, &items, 184);
    let texts: Vec<(&str, &str)> = (lines.iter())
        .map(|line| (line.text.as_str(), line.data.as_str()))
        .collect();
    assert_eq!(texts[0], ("Path", "C:\\a.7z"));
    assert!(texts.contains(&("Solid", "Yes")), "{texts:?}");
    assert!(texts.iter().any(|(text, _)| *text == "Method"), "{texts:?}");
    // A solid block tells its packed size on its first file only: no "Packed size" (arclite's
    // `total_packed_size_defined`).
    let tail = &texts[texts.len() - 4..];
    assert_eq!(tail[0], ("Compression ratio", "100%"));
    assert_eq!(tail[1], ("Data size", "8"));
    assert_eq!(tail[2], ("File count", "2"));
    assert_eq!(tail[3], ("Directory count", "1"));
    assert!(texts.iter().all(|(_, data)| !data.is_empty()), "{texts:?}");
    let top = (0..archive.count())
        .find(|&index| archive.property(index, prop::PATH).text() == Some("top.txt"))
        .unwrap();
    let properties = item_properties(&archive, top);
    let has = |name: &str, value: &str| {
        (properties.iter()).any(|(own, data)| own == name && data == value)
    };
    assert!(has("Path", "top.txt"), "{properties:?}");
    assert!(has("Size", "3") && has("CRC", "1ED91FCA"), "{properties:?}");
    assert!(has("Attributes", &attributes_text(0x20)), "{properties:?}");
    assert!(has("Packed Size", ""), "an empty value too: {properties:?}");
}

/// A tar's info lines: every file tells its packed size, so it is there; a file only (no folder).
#[test]
fn a_tars_info_lines_have_the_packed_size() {
    use nc_plugin_sdk::sevenzip::{InStream, Library, Opening};
    struct Quiet;
    impl Opening for Quiet {}
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("nested.tar");
    std::fs::write(&path, include_bytes!("../testdata/nested.tar")).unwrap();
    let library = Library::find(folder.path());
    let tar = (library.formats().iter())
        .position(|format| format.name == "tar")
        .unwrap();
    let stream = InStream::file(&path).unwrap();
    let archive = (library.open(tar, &stream, Some(&path), false, &mut Quiet))
        .unwrap()
        .unwrap();
    let lines = info_lines("C:\\nested.tar", &archive, &[Item::default()], 92);
    let texts: Vec<(&str, &str)> = (lines.iter())
        .map(|line| (line.text.as_str(), line.data.as_str()))
        .collect();
    assert_eq!(
        texts[texts.len() - 5..],
        [
            ("Compression ratio", "50%"),
            ("Data size", "184"),
            ("Packed size", "512"),
            ("File count", "1"),
            ("Directory count", "0"),
        ]
    );
    assert_eq!(texts[0], ("Path", "C:\\nested.tar"), "{texts:?}");
    assert!(texts.iter().all(|(_, data)| !data.is_empty()), "{texts:?}");
}
