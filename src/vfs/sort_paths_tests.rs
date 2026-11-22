//! A provider's whole paths sorted and split as FAR's `list_less` and `name_ext` do: names only while
//! the view shows names only (`IgnorePaths`), a dot in a folder never an extension's.

use super::*;

fn file(name: &str) -> Entry {
    Entry {
        name: name.to_string(),
        ..Entry::default()
    }
}

fn names(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

#[test]
fn whole_paths_sort_by_their_names_only_while_the_view_shows_names_only() {
    let mut entries = vec![
        file(r"C:\b\a.txt"),
        file(r"C:\a\c.txt"),
        file(r"D:\z\b.txt"),
    ];
    let mut order = SortOrder::default();
    sort_entries(&mut entries, &order, |_| false);
    assert_eq!(
        names(&entries),
        [r"C:\a\c.txt", r"C:\b\a.txt", r"D:\z\b.txt"],
        "whole paths"
    );
    order.names_only = true;
    sort_entries(&mut entries, &order, |_| false);
    assert_eq!(
        names(&entries),
        [r"C:\b\a.txt", r"D:\z\b.txt", r"C:\a\c.txt"],
        "names only"
    );
    order.mode = SortMode::NameOnly;
    sort_entries(&mut entries, &order, |_| false);
    assert_eq!(
        names(&entries),
        [r"C:\b\a.txt", r"D:\z\b.txt", r"C:\a\c.txt"]
    );
}

#[test]
fn a_dot_in_a_folder_is_no_extension() {
    assert_eq!(split_extension(r"E:\a.b\file"), (r"E:\a.b\file", ""));
    assert_eq!(
        split_extension(r"E:\a.b\file.txt"),
        (r"E:\a.b\file", ".txt")
    );
    assert_eq!(split_extension("a/b.c/d"), ("a/b.c/d", ""));
    assert_eq!(split_extension("name.tar.gz"), ("name.tar", ".gz"));
    assert_eq!(split_extension(".gitignore"), ("", ".gitignore"));
    assert_eq!(name_ext(&file(r"C:\x.y\z")), (r"C:\x.y\z", ""));
}
