//! FarColorer's editor items over the parse (S2d.7e2; `FarEditor::matchPair`, `selectBlock`,
//! `selectPair`, `selectRegion`, `getNameCurrentScheme`): the pair of the bracket at the cursor gone to
//! or marked with or without its ends, the region at the cursor marked or named. Each waits for the
//! editor's steps and the blocks it reads (it is done again once they are there).

use super::colorer_menu::Action;
use super::dialog::Dialog;
use super::editing::EditorAction;
use super::{App, Screen};
use crate::editor::syntax::highlight::{PairMatch, Wait};
use crate::editor::view::edit_view::{EditView, Place, Range, Step};
use crate::editor::view::{LineSource, Missing};

/// What an item found: the step it makes, or the region's names to show.
enum Done {
    Step(Step),
    Info(Option<String>, Option<String>),
}

impl App {
    /// "Match pair", "Select block", "Select pair", "Select region", "Region info".
    pub(super) fn colorer_parse_action(&mut self, action: Action) {
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        let height = usize::from(editor.text.room.get().1);
        let done =
            editor.text.colorer(|syntax, view| {
                let (line, pos) = view.cursor_char()?;
                match action {
                    Action::SelectRegion | Action::RegionInfo => {
                        let length = view.line_at(line)?.chars.len();
                        let Some(found) = syntax.cursor_region(view, line, pos)? else {
                            return Ok(None);
                        };
                        if action == Action::RegionInfo {
                            return Ok(Some(Done::Info(found.region, found.scheme)));
                        }
                        let end = found.end.unwrap_or(length);
                        Ok((end > found.start)
                            .then(|| Done::Step(region_step(line, found.start, end))))
                    }
                    _ => match syntax.search_pair(view, line, pos)? {
                        Some(pair) if pair.end.is_some() => pair_step(view, &pair, action, height)
                            .map(|step| Some(Done::Step(step)))
                            .map_err(Wait::from),
                        _ => Ok(None),
                    },
                }
            });
        match done {
            None => editor.pending = Some(EditorAction::Colorer(action)),
            Some(Some(Done::Step(step))) => editor.text.press(step),
            Some(Some(Done::Info(region, scheme))) => self.show_region_info(region, scheme),
            Some(None) => {}
        }
    }

    /// FarColorer's "Region name" message, its lines from the left (`FMSG_LEFTALIGN`).
    fn show_region_info(&mut self, region: Option<String>, scheme: Option<String>) {
        let lines = [
            format!("Region: {}", region.unwrap_or_default()),
            format!("Scheme: {}", scheme.unwrap_or_default()),
        ];
        let width = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        let lines = lines.iter().map(|line| format!("{line:width$}")).collect();
        self.show_dialog(Dialog::message("Region name", lines));
    }
}

/// "Select region": the region's characters on the cursor's line (`ECTL_SELECT`, one line).
fn region_step(line: u64, start: usize, end: usize) -> Step {
    Step::SelectRange(Range {
        kind: 1,
        line,
        pos: start as i64,
        width: (end - start) as i64,
        height: 1,
    })
}

/// The lines from `first` to `last` (their first bytes), both counted.
fn lines_between(view: &EditView, first: u64, last: u64) -> Result<i64, Missing> {
    let mut count = 1;
    let mut at = first;
    while at < last {
        match view.line_after(&view.line_at(at)?)? {
            Some(next) => at = next,
            None => break,
        }
        count += 1;
    }
    Ok(count)
}

/// The step of "Match pair", "Select block" or "Select pair" for the pair found.
fn pair_step(
    view: &EditView,
    pair: &PairMatch,
    action: Action,
    height: usize,
) -> Result<Step, Missing> {
    let (sline, start) = pair.start;
    let Some((eline, end)) = pair.end else {
        return Ok(Step::Unselect);
    };
    let (start_s, start_e) = (start.start as i64, start.end as i64);
    let (end_s, end_e) = (end.start as i64, end.end as i64);
    let (x1, x2, y1, y2) = match (action, pair.top) {
        (Action::MatchPair, _) => return match_step(view, pair, height),
        (Action::SelectBlock, true) => (start_s, end_e - 1, sline, eline),
        (Action::SelectBlock, false) => (end_s, start_e - 1, eline, sline),
        (_, true) => (start_e, end_s - 1, sline, eline),
        (_, false) => (end_e, start_s - 1, eline, sline),
    };
    Ok(Step::SelectRange(Range {
        kind: 1,
        line: y1,
        pos: x1,
        width: x2 - x1 + 1,
        height: lines_between(view, y1, y2)?,
    }))
}

/// "Match pair": the cursor on the pair's other end (its last character when it closes the pair),
/// the screen moved to it when it is not shown, its line half a screen down.
fn match_step(view: &EditView, pair: &PairMatch, height: usize) -> Result<Step, Missing> {
    let Some((eline, end)) = pair.end else {
        return Ok(Step::Unselect);
    };
    let pos = match pair.top {
        false => end.start,
        true => end.end.saturating_sub(1),
    };
    let shown = (view.rows(height + 1).iter()).any(|(start, _)| *start == eline);
    let top = match shown {
        true => None,
        false => {
            let mut top = eline;
            for _ in 0..height / 2 {
                match view.line_above(top)? {
                    Some(above) => top = above,
                    None => break,
                }
            }
            Some(top)
        }
    };
    Ok(Step::Place(Place {
        line: Some(eline),
        pos: Some(pos),
        tab_pos: None,
        top,
        left: None,
        overtype: None,
    }))
}

#[cfg(test)]
#[path = "colorer_actions_tests.rs"]
mod tests;
