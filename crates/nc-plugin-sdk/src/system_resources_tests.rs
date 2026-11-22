//! Resources written into a copy of this test's own program, read back, deleted.

use super::*;

/// A copy of this test's program in a folder of its own.
fn program_copy() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("copy.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &path).unwrap();
    (temp, path)
}

/// A numbered and a named resource written in one update, each listed with its language and read back;
/// one deleted by a change with no data, the other kept.
#[test]
fn resources_are_written_listed_read_and_deleted() {
    let (_temp, path) = program_copy();
    let numbered = ResourceChange {
        kind: RT_VERSION,
        name: ResourceName::Id(7),
        language: 0x0409,
        data: Some(b"numbered data".to_vec()),
    };
    let named = ResourceChange {
        kind: RT_VERSION,
        name: ResourceName::Text("NAMED".to_string()),
        language: 0,
        data: Some(b"named".to_vec()),
    };
    update_resources(&path, &[numbered.clone(), named.clone()]).unwrap();
    let listed = resources(&path, RT_VERSION).unwrap();
    assert!(
        listed.contains(&(ResourceName::Id(7), 0x0409)),
        "{listed:?}"
    );
    assert!(listed.contains(&(named.name.clone(), 0)), "{listed:?}");
    assert_eq!(
        resource(&path, RT_VERSION, &ResourceName::Id(7), 0x0409).unwrap(),
        b"numbered data"
    );
    assert_eq!(
        resource(&path, RT_VERSION, &named.name, 0).unwrap(),
        b"named"
    );
    let deleted = ResourceChange {
        data: None,
        ..numbered
    };
    update_resources(&path, &[deleted]).unwrap();
    let listed = resources(&path, RT_VERSION).unwrap();
    assert!(
        !listed.contains(&(ResourceName::Id(7), 0x0409)),
        "{listed:?}"
    );
    assert!(listed.contains(&(named.name, 0)), "{listed:?}");
    assert!(resource(&path, RT_VERSION, &ResourceName::Id(7), 0x0409).is_err());
}

/// One change that fails (a resource deleted that is not there) discards the whole update: the change
/// before it is not written either.
#[test]
fn a_failed_change_writes_none() {
    let (_temp, path) = program_copy();
    let before = std::fs::read(&path).unwrap();
    let written = ResourceChange {
        kind: RT_VERSION,
        name: ResourceName::Id(9),
        language: 0,
        data: Some(b"data".to_vec()),
    };
    let missing = ResourceChange {
        kind: RT_ICON,
        name: ResourceName::Text("NONE".to_string()),
        language: 0x0409,
        data: None,
    };
    assert!(update_resources(&path, &[written, missing]).is_err());
    assert!(
        !resources(&path, RT_VERSION)
            .unwrap()
            .contains(&(ResourceName::Id(9), 0))
    );
    assert_eq!(std::fs::read(&path).unwrap(), before, "the file as it was");
}

/// Several languages of one name each listed; each read in its own.
#[test]
fn each_language_of_a_name_is_listed() {
    let (_temp, path) = program_copy();
    let change = |language: u16, data: &[u8]| ResourceChange {
        kind: RT_GROUP_ICON,
        name: ResourceName::Text("TWO".to_string()),
        language,
        data: Some(data.to_vec()),
    };
    update_resources(&path, &[change(0x0409, b"en"), change(0x0419, b"ru")]).unwrap();
    let name = ResourceName::Text("TWO".to_string());
    let mut listed = resources(&path, RT_GROUP_ICON).unwrap();
    listed.sort_by_key(|(_, language)| *language);
    assert_eq!(listed, [(name.clone(), 0x0409), (name.clone(), 0x0419)]);
    assert_eq!(
        resource(&path, RT_GROUP_ICON, &name, 0x0419).unwrap(),
        b"ru"
    );
    assert_eq!(
        resource(&path, RT_GROUP_ICON, &name, 0x0409).unwrap(),
        b"en"
    );
}

/// A type the file has none of: an empty list; a file that is not there: the system's failure for each.
#[test]
fn nothing_there_is_told_so() {
    let (temp, path) = program_copy();
    assert_eq!(resources(&path, RT_GROUP_ICON).unwrap(), []);
    let missing = temp.path().join("missing.exe");
    assert!(resources(&missing, RT_ICON).is_err());
    assert!(resource(&missing, RT_ICON, &ResourceName::Id(1), 0).is_err());
    assert!(update_resources(&missing, &[]).is_err());
}
