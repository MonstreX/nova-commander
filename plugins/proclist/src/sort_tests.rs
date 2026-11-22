use super::*;
use crate::table::TOTAL;

fn process(pid: u32, parent: u32, threads: u32, owner: &str) -> Process {
    let mut process = Process {
        pid,
        parent,
        threads,
        priority: i32::try_from(pid % 7).unwrap_or(0),
        owner: Some(owner.to_string()),
        ..Process::default()
    };
    process.counters[0] = u64::from(100 - pid);
    process.results[0] = u64::from(pid);
    process
}

fn pids(processes: &[&Process]) -> Vec<u32> {
    processes.iter().map(|process| process.pid).collect()
}

/// The plugin's orders: `_Total` first, then by the value; the same value as listed.
#[test]
fn the_plugins_own_orders() {
    let total = Process {
        pid: TOTAL,
        ..Process::default()
    };
    let (a, b, c) = (
        process(12, 4, 3, "b"),
        process(8, 9, 1, "A"),
        process(20, 4, 5, "c"),
    );
    let listed = [&a, &total, &b, &c];
    let ordered = |mode| {
        let mut processes = listed.to_vec();
        order(&mut processes, mode);
        pids(&processes)
    };
    assert_eq!(ordered(PID), [TOTAL, 8, 12, 20]);
    assert_eq!(
        ordered(PARENT),
        [TOTAL, 12, 20, 8],
        "the same parent as listed"
    );
    assert_eq!(ordered(THREADS), [TOTAL, 8, 12, 20]);
    assert_eq!(ordered(PRIORITY), [TOTAL, 8, 12, 20]);
    assert_eq!(ordered(OWNER), [TOTAL, 8, 12, 20], "the owner without case");
    assert_eq!(ordered(COUNTER), [TOTAL, 20, 12, 8], "by the counter");
    assert_eq!(
        ordered(COUNTER | PER_SECOND),
        [TOTAL, 8, 12, 20],
        "per second"
    );
    assert_eq!(
        ordered(NAME),
        [12, TOTAL, 8, 20],
        "the program's own: as listed"
    );
}

#[test]
fn the_program_sorts_by_its_modes_the_plugin_by_the_others() {
    assert_eq!(panel_code(NAME), api::SORT_NAME);
    assert_eq!(panel_code(CREATED), api::SORT_CREATED);
    for own in [
        PID,
        PARENT,
        PRIORITY,
        THREADS,
        OWNER,
        COUNTER + 3,
        COUNTER | PER_SECOND,
    ] {
        assert_eq!(panel_code(own), api::SORT_UNSORTED, "{own}");
    }
    for mode in [
        UNSORTED,
        NAME,
        EXTENSION,
        MODIFIED,
        CREATED,
        ACCESSED,
        SIZE,
        DESCRIPTION,
    ] {
        assert_eq!(far_mode(panel_code(mode)), Some(mode));
    }
    assert_eq!(start(PID), Some((SortBy::Unsorted, false)));
    assert_eq!(start(SIZE), Some((SortBy::Size, false)));
    assert_eq!(start(CHANGED), None, "no start sort of the SDK's");
}

/// FAR saves its own mode while the panel is in its slot, the panel's otherwise; a per second one is
/// read back without its per second.
#[test]
fn the_mode_is_saved_and_read_as_far_does() {
    assert_eq!(saved(PID, api::SORT_UNSORTED), PID);
    assert_eq!(saved(PID, api::SORT_SIZE), SIZE);
    assert_eq!(saved(UNSORTED, api::SORT_MODIFIED), MODIFIED);
    assert_eq!(loaded(i64::from((COUNTER + 2) | PER_SECOND)), COUNTER + 2);
    assert_eq!(loaded(i64::from(PID)), PID);
    assert_eq!(loaded(-5), UNSORTED);
}

#[test]
fn the_menu_has_fars_items_then_each_counter() {
    let (items, modes) = menu(COUNTER | PER_SECOND, true);
    assert_eq!(items.len(), modes.len());
    assert_eq!(items[0].text, "&Name                 Ctrl+F3");
    assert_eq!(items[10].text, "Priority");
    assert_eq!(items[11].text, "Process tree");
    assert_eq!(modes[11], Some(TREE));
    assert!(items[12].separator && modes[12].is_none());
    assert_eq!(items[13].text, "Processor Time");
    assert_eq!(items[14].text, "% Processor Time");
    assert_eq!(modes[14], Some(COUNTER | PER_SECOND));
    assert_eq!(items[16].text, "% Privileged Time");
    let handles = items
        .iter()
        .position(|item| item.text == "Handle Count")
        .unwrap();
    assert_eq!(items[handles + 1].text, "Handle Count/s");
    let bytes = items
        .iter()
        .position(|item| item.text == "Working Set")
        .unwrap();
    assert_eq!(
        items[bytes + 1].text,
        "Working Set Peak",
        "no per second for a size"
    );
    let checked: Vec<usize> = (items.iter().enumerate())
        .filter_map(|(at, item)| item.checked.then_some(at))
        .collect();
    assert_eq!(checked, [14]);
    let (unknown, _) = menu(PID, false);
    assert!(
        unknown.iter().all(|item| !item.checked),
        "an unknown sort checks none"
    );
}

#[test]
fn a_sort_picked_again_turns_its_order_round() {
    assert_eq!(picked(SIZE, SIZE, true, true), (api::SORT_SIZE, false));
    assert_eq!(picked(SIZE, SIZE, false, true), (api::SORT_SIZE, true));
    assert_eq!(picked(NAME, SIZE, false, true), (api::SORT_SIZE, true));
    assert_eq!(picked(NAME, PID, true, false), (api::SORT_UNSORTED, false));
    assert!(descending(COUNTER + 5), "a counter starts descending");
    assert!(descending(THREADS));
    assert!(!descending(PID));
    assert_eq!(of_key("Ctrl+F8").map(|one| one.mode), Some(CREATED));
    assert!(of_key("Ctrl+F12").is_none());
    assert!(known(api::SORT_UNSORTED) && known(api::SORT_NAME));
    assert!(!known(api::SORT_MODIFIED), "not one of the menu's");
}

/// The tree is the plugin's own order, saved and read back as it is.
#[test]
fn the_tree_is_the_plugins_own_sort() {
    assert_eq!(panel_code(TREE), api::SORT_UNSORTED);
    assert_eq!(loaded(i64::from(TREE)), TREE, "not cut as a per second one");
    assert_eq!(saved(TREE, api::SORT_UNSORTED), TREE);
    assert_eq!(
        picked(TREE, TREE, false, false),
        (api::SORT_UNSORTED, false),
        "picked again the tree stays the right way up"
    );
    assert_eq!(picked(TREE, TREE, true, false), (api::SORT_UNSORTED, false));
    assert_eq!(picked(SIZE, TREE, true, false), (api::SORT_UNSORTED, false));
    assert_eq!(start(TREE), Some((SortBy::Unsorted, false)));
    assert!(!descending(TREE));
}
