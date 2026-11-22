use super::*;

/// The OEM code page, as a test knows it.
fn oem(bytes: &[u8]) -> String {
    format!("oem:{}", bytes.len())
}

const FAR_MENU: &str = "\
A:   Archive the file
     7z a \"!.!.7z\" \"!.!\"
--:  Tools
T:   Tools
{
1:   Notepad
     notepad \"!.!\"
::   Colon is the hotkey
     echo colon
}:   Brace is the hotkey
     echo brace
}
F5:  Build
     cargo build
     cargo test
";

fn item(hotkey: &str, label: &str, commands: &[&str]) -> Item {
    Item {
        hotkey: hotkey.to_string(),
        label: label.to_string(),
        commands: commands.iter().map(|line| (*line).to_string()).collect(),
        submenu: None,
    }
}

#[test]
fn reads_items_separators_submenus_and_odd_hotkeys() {
    let menu = parse(FAR_MENU);
    assert_eq!(menu.len(), 4);
    assert_eq!(
        menu[0],
        item("A", "Archive the file", &["7z a \"!.!.7z\" \"!.!\""])
    );
    assert!(menu[1].is_separator() && menu[1].label == "Tools");
    let tools = menu[2].submenu.as_ref().unwrap();
    assert_eq!(tools[0], item("1", "Notepad", &["notepad \"!.!\""]));
    assert_eq!(tools[1], item(":", "Colon is the hotkey", &["echo colon"]));
    assert_eq!(tools[2], item("}", "Brace is the hotkey", &["echo brace"]));
    assert_eq!(menu[3], item("F5", "Build", &["cargo build", "cargo test"]));
}

#[test]
fn what_is_written_reads_back_the_same() {
    let menu = parse(FAR_MENU);
    let text = serialize(&menu);
    assert!(
        text.starts_with("A:   Archive the file\r\n     7z a"),
        "{text}"
    );
    assert!(text.contains("T:   Tools\r\n{\r\n1:   Notepad"), "{text}");
    assert_eq!(parse(&text), menu);
}

#[test]
fn a_file_is_read_in_its_encoding() {
    assert_eq!(decode("A: Ä\n".as_bytes(), oem), "A: Ä\n");
    assert_eq!(decode(b"\xef\xbb\xbfA: x", oem), "A: x");
    let utf16: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain("A: Ж".encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    assert_eq!(decode(&utf16, oem), "A: Ж");
}

#[test]
fn a_missing_menu_is_none_and_an_empty_one_takes_its_file_away() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(MENU_FILE);
    assert_eq!(load(&path, oem).unwrap(), None);
    let menu = vec![item("A", "a", &["echo a"])];
    save(&path, &menu).unwrap();
    assert_eq!(load(&path, oem).unwrap(), Some(menu));
    save(&path, &[]).unwrap();
    assert!(!path.exists());
    save(&path, &[]).unwrap();
}

#[test]
fn what_is_no_unicode_is_the_oem_code_page() {
    assert_eq!(decode(&[0x80, 0x81], oem), "oem:2");
}
