use super::*;
use crate::table::TOTAL;

fn process(pid: u32, parent: u32, created: i64) -> Process {
    Process {
        pid,
        parent,
        created,
        name: format!("p{pid}"),
        ..Process::default()
    }
}

/// `_Total`; System (4) starting smss (100, then 300) and its child 200; explorer (500) whose parent
/// is gone; 600 whose parent's number 500 came back later than it started (another process).
fn listed() -> Vec<Process> {
    vec![
        Process {
            pid: TOTAL,
            ..Process::default()
        },
        process(0, 0, 0),
        process(4, 0, 1),
        process(300, 4, 30),
        process(100, 4, 10),
        process(200, 100, 20),
        process(500, 7777, 50),
        process(600, 500, 40),
    ]
}

fn pids(processes: &[Process], rows: &[Row]) -> Vec<(u32, usize, Fold)> {
    (rows.iter())
        .map(|row| (processes[row.at].pid, row.depth, row.fold))
        .collect()
}

#[test]
fn the_tree_is_folded_at_first() {
    let processes = listed();
    let rows = rows(&processes, &HashSet::new());
    assert_eq!(
        pids(&processes, &rows),
        [
            (TOTAL, 0, Fold::Leaf),
            (0, 0, Fold::Leaf),
            (4, 0, Fold::Folded),
            (600, 0, Fold::Leaf),
            (500, 0, Fold::Leaf),
        ],
        "the roots by their start; 600 is no child of the later 500"
    );
}

#[test]
fn an_unfolded_branch_shows_its_children_by_their_start() {
    let processes = listed();
    let unfolded: HashSet<u32> = [4, 100].into_iter().collect();
    let rows = rows(&processes, &unfolded);
    assert_eq!(
        pids(&processes, &rows)[2..6],
        [
            (4, 0, Fold::Unfolded),
            (100, 1, Fold::Unfolded),
            (200, 2, Fold::Leaf),
            (300, 1, Fold::Leaf),
        ]
    );
    let folded_inner: HashSet<u32> = [4].into_iter().collect();
    let rows = rows_of(&processes, &folded_inner);
    assert_eq!(rows[3], (100, 1, Fold::Folded), "its own branch folded");
    assert_eq!(rows.len(), 7);
}

fn rows_of(processes: &[Process], unfolded: &HashSet<u32>) -> Vec<(u32, usize, Fold)> {
    pids(processes, &rows(processes, unfolded))
}

/// Two processes naming each other, started in the same moment: no root reaches them, yet both are
/// listed, the first a root and the other under it.
#[test]
fn a_parent_cycle_loses_no_process() {
    let processes = vec![process(4, 0, 1), process(70, 80, 5), process(80, 70, 5)];
    let unfolded: HashSet<u32> = [70].into_iter().collect();
    assert_eq!(
        rows_of(&processes, &unfolded),
        [
            (4, 0, Fold::Leaf),
            (70, 0, Fold::Unfolded),
            (80, 1, Fold::Leaf),
        ]
    );
}

#[test]
fn children_are_told() {
    let processes = listed();
    assert!(has_children(&processes, 4) && has_children(&processes, 100));
    assert!(!has_children(&processes, 200));
    assert!(!has_children(&processes, 500), "600 started before it");
    assert!(
        !has_children(&processes, 0),
        "the idle process is its own parent"
    );
}

#[test]
fn a_row_is_indented_with_its_marker() {
    let row = |depth, fold| Row { at: 0, depth, fold };
    assert_eq!(shown("System", row(0, Fold::Folded)), "+ System");
    assert_eq!(shown("smss", row(1, Fold::Unfolded)), "  - smss");
    assert_eq!(shown("csrss", row(2, Fold::Leaf)), "      csrss");
}

/// Enter on a branch unfolds it, again folds it; a later process with the same number is another.
#[test]
fn enter_folds_and_unfolds_a_branch() {
    let processes = listed();
    let system = &processes[2];
    let mut unfolded = HashSet::new();
    toggle(&mut unfolded, system);
    assert_eq!(unfolded, [(4, 1)].into_iter().collect());
    toggle(&mut unfolded, system);
    assert!(unfolded.is_empty(), "folded again");
    toggle(&mut unfolded, &process(4, 0, 99));
    assert_eq!(
        ordered(&processes, crate::sort::TREE, &mut unfolded)[2]
            .1
            .map(|row| row.fold),
        Some(Fold::Folded),
        "the number's earlier process stays folded"
    );
    assert!(unfolded.is_empty(), "the one no longer listed forgotten");
}

/// The tree's rows for the tree; another sort lists every process flat, by the plugin's order.
#[test]
fn the_tree_or_a_flat_list() {
    let processes = listed();
    let mut unfolded = [(4, 1)].into_iter().collect();
    let tree = ordered(&processes, crate::sort::TREE, &mut unfolded);
    let shown: Vec<(u32, Option<usize>)> = (tree.iter())
        .map(|(at, row)| (processes[*at].pid, row.map(|row| row.depth)))
        .collect();
    assert_eq!(
        shown,
        [
            (TOTAL, Some(0)),
            (0, Some(0)),
            (4, Some(0)),
            (100, Some(1)),
            (300, Some(1)),
            (600, Some(0)),
            (500, Some(0)),
        ]
    );
    let flat = ordered(&processes, crate::sort::PID, &mut unfolded);
    let pids: Vec<u32> = flat.iter().map(|(at, _)| processes[*at].pid).collect();
    assert_eq!(pids, [TOTAL, 0, 4, 100, 200, 300, 500, 600]);
    assert!(flat.iter().all(|(_, row)| row.is_none()));
}
