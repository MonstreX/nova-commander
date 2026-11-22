//! FAR's `FindSearchResultsPanel`: the first empty common panel, else the next in turn.

use super::*;

#[test]
fn the_first_empty_panel_takes_the_results() {
    let mut last = 0;
    let mut empty = [false; PANELS];
    empty[3] = true;
    empty[7] = true;
    assert_eq!(results_panel(&empty, &mut last), 3);
    assert_eq!(last, 0, "the turn is kept for when none is empty");
}

#[test]
fn with_every_panel_full_they_take_turns_round() {
    let full = [false; PANELS];
    let mut last = 0;
    let taken: Vec<usize> = (0..12).map(|_| results_panel(&full, &mut last)).collect();
    assert_eq!(taken, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1]);
}
