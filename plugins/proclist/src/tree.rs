//! The processes as a tree by parent (owner, 2026-11-15: Process Explorer's tree, folded at first): a
//! process is under the one that started it while that one is listed and started no later (a parent
//! gone leaves its number to another process, as Process Explorer tells them apart); each level by its
//! start; a branch shown only once unfolded. `_Total` stays first.

use std::collections::{HashMap, HashSet};

use crate::table::Process;

/// A process told apart from a later one with its number: the number and the start.
pub type Started = (u32, i64);

/// Whether a row's branch can be unfolded, is, or has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fold {
    Leaf,
    Folded,
    Unfolded,
}

/// One row of the tree: the process's place in the list given, its depth, its branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub at: usize,
    pub depth: usize,
    pub fold: Fold,
}

/// The parent of `process` among `processes` by number; none for a root.
fn parent_of(process: &Process, by_pid: &HashMap<u32, &Process>) -> Option<u32> {
    // The idle process (0) is nobody's parent: System, which names it, is a root as in Process Explorer.
    if process.is_total() || process.parent == process.pid || process.parent == 0 {
        return None;
    }
    let parent = by_pid.get(&process.parent)?;
    (!parent.is_total() && parent.created <= process.created).then_some(parent.pid)
}

/// Each process's children's places, each level in the order of its start.
fn children(processes: &[Process]) -> (Vec<usize>, HashMap<u32, Vec<usize>>) {
    let by_pid: HashMap<u32, &Process> = processes.iter().map(|one| (one.pid, one)).collect();
    let mut roots = Vec::new();
    let mut under: HashMap<u32, Vec<usize>> = HashMap::new();
    for (at, process) in processes.iter().enumerate() {
        match parent_of(process, &by_pid) {
            Some(parent) => under.entry(parent).or_default().push(at),
            None => roots.push(at),
        }
    }
    break_cycles(processes, &mut roots, &mut under);
    let start = |at: &usize| {
        (
            !processes[*at].is_total(),
            processes[*at].created,
            processes[*at].pid,
        )
    };
    roots.sort_by_key(start);
    for level in under.values_mut() {
        level.sort_by_key(start);
    }
    (roots, under)
}

/// A process no root reaches (parents naming each other, both started in the same moment) made a root,
/// taken from its parent's level: the tree never loses a process.
fn break_cycles(
    processes: &[Process],
    roots: &mut Vec<usize>,
    under: &mut HashMap<u32, Vec<usize>>,
) {
    let mut reached = HashSet::new();
    let reach = |from: usize, reached: &mut HashSet<usize>, under: &HashMap<u32, Vec<usize>>| {
        let mut stack = vec![from];
        while let Some(at) = stack.pop() {
            if reached.insert(at) {
                stack.extend(under.get(&processes[at].pid).into_iter().flatten());
            }
        }
    };
    for root in roots.iter() {
        reach(*root, &mut reached, under);
    }
    for at in 0..processes.len() {
        if reached.contains(&at) {
            continue;
        }
        for level in under.values_mut() {
            level.retain(|child| *child != at);
        }
        roots.push(at);
        reach(at, &mut reached, under);
    }
}

/// The rows shown: the roots, and under each unfolded one its children, depth first.
pub fn rows(processes: &[Process], unfolded: &HashSet<u32>) -> Vec<Row> {
    let (roots, under) = children(processes);
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<(usize, usize)> = roots.iter().rev().map(|at| (*at, 0)).collect();
    while let Some((at, depth)) = stack.pop() {
        let pid = processes[at].pid;
        if !seen.insert(at) {
            continue;
        }
        let branch = under.get(&pid).filter(|level| !level.is_empty());
        let fold = match branch {
            None => Fold::Leaf,
            Some(_) if unfolded.contains(&pid) => Fold::Unfolded,
            Some(_) => Fold::Folded,
        };
        rows.push(Row { at, depth, fold });
        if let (Fold::Unfolded, Some(level)) = (fold, branch) {
            stack.extend(level.iter().rev().map(|child| (*child, depth + 1)));
        }
    }
    rows
}

/// The processes in the panel's order for `order`, each with its row in the tree (the folded branches
/// left out), else by the plugin's sort. The unfolded branches no longer listed are forgotten.
pub fn ordered(
    processes: &[Process],
    order: u32,
    unfolded: &mut HashSet<Started>,
) -> Vec<(usize, Option<Row>)> {
    if order != crate::sort::TREE {
        let places = crate::sort::places(processes, order);
        return places.into_iter().map(|at| (at, None)).collect();
    }
    let listed: HashSet<Started> = processes.iter().map(started).collect();
    unfolded.retain(|one| listed.contains(one));
    let pids: HashSet<u32> = unfolded.iter().map(|(pid, _)| *pid).collect();
    let rows = rows(processes, &pids);
    rows.into_iter().map(|row| (row.at, Some(row))).collect()
}

fn started(process: &Process) -> Started {
    (process.pid, process.created)
}

/// Enter on `process`'s branch: folded unfolds, unfolded folds.
pub fn toggle(unfolded: &mut HashSet<Started>, process: &Process) {
    if !unfolded.remove(&started(process)) {
        unfolded.insert(started(process));
    }
}

/// Whether `pid` has a process under it in the tree.
pub fn has_children(processes: &[Process], pid: u32) -> bool {
    let (_, under) = children(processes);
    under.get(&pid).is_some_and(|level| !level.is_empty())
}

/// The friendly name as the tree shows it: indented by its depth, `+` before a folded branch, `-`
/// before an unfolded one.
pub fn shown(name: &str, row: Row) -> String {
    let marker = match row.fold {
        Fold::Leaf => "  ",
        Fold::Folded => "+ ",
        Fold::Unfolded => "- ",
    };
    format!("{}{marker}{name}", "  ".repeat(row.depth))
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
