//! NetBox's tab bar (`TTabbedDialog`, `TSessionDialog::ChangeTabs`, `GetVisibleTabsCount`): the tabs'
//! names in a row above the tab, the shown one in brackets, the ones the protocol has no use for left
//! out; when they do not fit, the row scrolls to the shown one with `◄` / `►` where more are. Ctrl+PgDn /
//! Ctrl+PgUp step to the next / previous tab that is there, round (`TTabbedDialog::Key`); a click on a
//! name shows its tab.

/// The row as drawn: its text and where each tab's name is (from, to: columns of the text; the tab).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub text: String,
    pub places: Vec<(usize, usize, usize)>,
}

const LEFT: &str = "\u{25c4}";
const RIGHT: &str = "\u{25ba}";

/// A tab's label in the row: the shown one in brackets.
fn label(name: &str, shown: bool) -> String {
    let name = name.replace('&', "");
    match shown {
        true => format!("[{name}]"),
        false => format!(" {name} "),
    }
}

/// The tabs `there` (their indexes, in order) drawn in `width` columns with `shown` in brackets.
pub fn row(names: &[&str], there: &[usize], shown: usize, width: usize) -> Row {
    let labels: Vec<(usize, String)> = (there.iter())
        .map(|&tab| (tab, label(names[tab], tab == shown)))
        .collect();
    let at = labels
        .iter()
        .position(|(tab, _)| *tab == shown)
        .unwrap_or(0);
    // The first tab drawn: from the start while the shown one fits, else the shown one last.
    let mut first = 0;
    while first < at && !fits(&labels[first..=at], first > 0, width) {
        first += 1;
    }
    let mut text = String::new();
    let mut places = Vec::new();
    if first > 0 {
        text.push_str(LEFT);
    }
    let mut column = text.chars().count();
    for (index, (tab, label)) in labels.iter().enumerate().skip(first) {
        let length = label.chars().count();
        let more = index + 1 < labels.len();
        let room = width - usize::from(more);
        if column + length > room {
            text.push_str(RIGHT);
            break;
        }
        places.push((column, column + length, *tab));
        text.push_str(label);
        column += length;
    }
    Row { text, places }
}

/// Whether `labels` fit in `width` after a `◄` (`scrolled`) and before a `►`.
fn fits(labels: &[(usize, String)], scrolled: bool, width: usize) -> bool {
    let length: usize = labels.iter().map(|(_, label)| label.chars().count()).sum();
    // Room for the `►` after them too.
    length + usize::from(scrolled) < width
}

/// The tab at column `column` of the row.
pub fn tab_at(row: &Row, column: usize) -> Option<usize> {
    (row.places.iter())
        .find(|(from, to, _)| (*from..*to).contains(&column))
        .map(|place| place.2)
}

/// The tab after (`forward`) or before `shown` among `there`, round.
pub fn step(there: &[usize], shown: usize, forward: bool) -> usize {
    let Some(at) = there.iter().position(|tab| *tab == shown) else {
        return there.first().copied().unwrap_or(shown);
    };
    let count = there.len();
    match forward {
        true => there[(at + 1) % count],
        false => there[(at + count - 1) % count],
    }
}

#[cfg(test)]
#[path = "tab_bar_tests.rs"]
mod tests;
