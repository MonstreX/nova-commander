use super::*;
use crate::vfs::VfsPath;

fn entry(name: &str, size: u64, is_dir: bool) -> Entry {
    Entry {
        name: name.into(),
        size,
        is_dir,
        ..Entry::default()
    }
}

fn panel_of(entries: Vec<Entry>) -> Panel {
    let mut panel = Panel::new(VfsPath::parse("nosuch://host/"));
    panel.entries = entries;
    panel.entries_changed();
    panel
}

fn sample() -> Vec<Entry> {
    vec![
        Entry::parent(),
        entry("src", 0, true),
        entry("a", 10, false),
        entry("b", 5, false),
    ]
}

#[test]
fn totals_count_files_and_folders_without_parent() {
    let mut counted = sample();
    counted[1].size = 100;
    assert_eq!(
        Totals::of(sample().iter(), false),
        Totals {
            bytes: 15,
            files: 2,
            folders: 1
        }
    );
    assert_eq!(
        Totals::of(counted.iter(), false).bytes,
        15,
        "the panel: files only"
    );
    assert_eq!(
        Totals::of(counted.iter(), true).bytes,
        115,
        "the selection: with F3's sizes"
    );
}

#[test]
fn the_totals_are_kept_until_the_entries_change() {
    let mut panel = panel_of(sample());
    assert_eq!(panel.totals().bytes, 15);
    // Changed without saying so: the kept totals stay (nothing walks the entries again).
    panel.entries[2].size = 1000;
    assert_eq!(panel.totals().bytes, 15, "kept, not worked out again");
    panel.entries_changed();
    assert_eq!(panel.totals().bytes, 1005);
}

#[test]
fn the_selections_totals_follow_each_change_of_it() {
    let mut panel = panel_of(sample());
    assert_eq!(panel.selected_totals(), SelectedTotals::default());
    panel.selected.insert("a".to_string());
    assert_eq!(panel.selected_totals().totals.bytes, 10);
    panel.selected.extend(["b".to_string(), "src".to_string()]);
    let totals = panel.selected_totals().totals;
    assert_eq!((totals.bytes, totals.files, totals.folders), (15, 2, 1));
    panel.selected.remove("b");
    assert_eq!(panel.selected_totals().totals.bytes, 10);
    panel.selected.retain(|name| name != "a");
    assert_eq!(panel.selected_totals().totals.files, 0);
    panel.selected.replace(BTreeSet::from(["b".to_string()]));
    assert_eq!(panel.selected_totals().totals.bytes, 5);
    panel.selected.clear();
    assert_eq!(panel.selected_totals(), SelectedTotals::default());
}

#[test]
fn a_folder_still_counted_in_the_selection_is_told() {
    let mut panel = panel_of(sample());
    panel.selected.insert("src".to_string());
    assert!(!panel.selected_totals().counting);
    panel.entries[1].folder_size = FolderSize::Counting;
    panel.entries[1].size = 7;
    panel.entries_changed();
    let totals = panel.selected_totals();
    assert!(totals.counting);
    assert_eq!(totals.totals.bytes, 7, "a size counted so far is in");
}

#[test]
fn only_a_real_change_counts() {
    let mut selection = Selection::default();
    assert!(selection.insert("a".to_string()));
    let version = selection.version;
    assert!(!selection.insert("a".to_string()));
    assert!(!selection.remove("b"));
    selection.retain(|_| true);
    selection.extend(["a".to_string()]);
    selection.replace(BTreeSet::from(["a".to_string()]));
    assert_eq!(selection.version, version);
    let mut other = Selection::default();
    other.insert("a".to_string());
    assert_eq!(selection, other, "the names, not the count of changes");
    selection.clear();
    assert!(selection.version > version);
    assert!(selection.is_empty());
}
