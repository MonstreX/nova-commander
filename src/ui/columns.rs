//! Column layout of FAR's file panel modes (the modes are `app::panel_modes`): how the widths are
//! computed (`FileList::PrepareColumnWidths`) and how repeated column groups form stripes
//! (`FileList::PrepareStripes`).

pub use crate::app::{ColumnDef, ColumnFlags, ColumnKind};

/// A column with its final width; 0 is an empty one (FAR's negative width): not shown, it takes no room
/// and has no divider, but counts in the stripes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    pub kind: ColumnKind,
    pub flags: ColumnFlags,
    pub width: usize,
}

/// FAR's `PrepareColumnWidths`: fixed widths first, the columns in percent their part of the rest, what
/// is left shared by the zero-width columns; columns that do not fit are dropped from the right, the
/// last one is shrunk if that is enough. `text_width` is the width inside the frame; columns are
/// separated by one character.
pub fn layout(defs: &[ColumnDef], text_width: usize) -> Vec<Column> {
    let percent = |def: &ColumnDef| def.percent && def.width != 0 && !def.empty;
    let mut columns: Vec<Column> = defs
        .iter()
        .map(|def| Column {
            kind: def.kind,
            flags: def.flags,
            width: if def.empty {
                0
            } else if def.width == 0 {
                def.default_width()
            } else {
                def.width
            },
        })
        .collect();
    let separators = (defs.iter().filter(|def| !def.empty).count()).saturating_sub(1);
    let fixed: usize = (columns.iter().zip(defs))
        .filter(|(_, def)| !percent(def))
        .map(|(column, _)| column.width)
        .sum();
    let mut extra = text_width.saturating_sub(fixed.saturating_add(separators));
    let mut auto_count = (columns.iter().zip(defs))
        .filter(|(column, def)| column.width == 0 && !percent(def) && !def.empty)
        .count();
    extra = share_percent(&mut columns, defs, extra, auto_count);
    for (column, def) in columns.iter_mut().zip(defs) {
        if column.width != 0 || percent(def) || def.empty {
            continue;
        }
        let width = (extra / auto_count).max(1);
        column.width = width;
        extra = extra.saturating_sub(width);
        auto_count -= 1;
    }
    loop {
        let shown = columns.iter().filter(|column| column.width > 0).count();
        let total: usize =
            columns.iter().map(|column| column.width).sum::<usize>() + shown.saturating_sub(1);
        if total <= text_width {
            break;
        }
        if columns.len() <= 1 {
            if let Some(first) = columns.first_mut() {
                first.width = text_width;
            }
            break;
        }
        let last = columns.len() - 1;
        let without_last = total - columns[last].width;
        // Shrunk to nothing it would be an empty column: dropped (an empty one is dropped too).
        if columns[last].width > 0 && text_width > without_last {
            columns[last].width = text_width - without_last;
            break;
        }
        columns.pop();
    }
    columns
}

/// FAR's percent columns: each its part of `extra` (all of it when the parts pass 100 or no column
/// shares the rest), the last one what is left of them, at least 1; returns what is left after them.
fn share_percent(
    columns: &mut [Column],
    defs: &[ColumnDef],
    extra: usize,
    auto_count: usize,
) -> usize {
    let parts: Vec<usize> = (defs.iter().enumerate())
        .filter(|(_, def)| def.percent && def.width != 0 && !def.empty)
        .map(|(at, _)| at)
        .collect();
    if parts.is_empty() {
        return extra;
    }
    let total: usize = parts.iter().map(|at| defs[*at].width).sum();
    let shared = if total > 100 || auto_count == 0 {
        extra
    } else {
        extra.saturating_mul(total) / 100
    };
    let mut taken = 0;
    for (left, at) in parts.iter().rev().enumerate().rev() {
        let width = if left > 0 {
            shared.saturating_mul(defs[*at].width) / total
        } else {
            shared.saturating_sub(taken)
        };
        let width = width.max(1);
        taken += width;
        columns[*at].width = width;
    }
    extra.saturating_sub(taken)
}

/// FAR's `PrepareStripes`: the smallest group of columns that repeats over the whole list.
/// Returns (stripes, columns per stripe).
pub fn stripes(columns: &[Column]) -> (usize, usize) {
    let count = columns.len();
    for stride in 1..=count / 2 {
        if !count.is_multiple_of(stride) {
            continue;
        }
        let first = &columns[..stride];
        let repeats = columns
            .chunks(stride)
            .all(|chunk| chunk.iter().zip(first).all(|(a, b)| a.kind == b.kind));
        if repeats {
            return (count / stride, stride);
        }
    }
    (1, count.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ViewMode;

    fn mode_spec(mode: ViewMode) -> crate::app::ModeSpec {
        crate::app::PanelModes::default().spec(mode).clone()
    }

    fn widths(columns: &[Column]) -> Vec<usize> {
        columns.iter().map(|column| column.width).collect()
    }

    #[test]
    fn full_mode_gives_the_rest_to_the_name() {
        // A 40-column panel has 38 characters inside the frame.
        let columns = layout(&mode_spec(ViewMode::Full).panel, 38);
        assert_eq!(widths(&columns), vec![16, 6, 8, 5]);
    }

    #[test]
    fn brief_mode_splits_names_evenly_and_forms_three_stripes() {
        let columns = layout(&mode_spec(ViewMode::Brief).panel, 38);
        assert_eq!(widths(&columns), vec![12, 12, 12]);
        assert_eq!(stripes(&columns), (3, 1));
        let medium = layout(&mode_spec(ViewMode::Medium).panel, 38);
        assert_eq!(stripes(&medium), (2, 1));
        assert_eq!(
            stripes(&layout(&mode_spec(ViewMode::Full).panel, 38)),
            (1, 4)
        );
    }

    #[test]
    fn an_empty_column_takes_no_room_but_counts_in_the_stripes() {
        let defs = crate::app::panel_modes::columns("N,S,N,S", "0,6,0,-1");
        let columns = layout(&defs, 38);
        // 38 less the size (6) and two dividers (the empty column has none): 30 for the two names.
        assert_eq!(widths(&columns), vec![15, 6, 15, 0]);
        assert_eq!(
            stripes(&columns),
            (2, 2),
            "FAR's stripes go by the columns' types"
        );
    }

    #[test]
    fn columns_that_do_not_fit_are_dropped_or_shrunk() {
        let columns = layout(&mode_spec(ViewMode::Detailed).panel, 30);
        // name 1 + size 6 + allocated 6 + three 14-wide times + attributes do not fit 30: columns are
        // dropped from the right and the last remaining one shrinks to the rest.
        assert_eq!(widths(&columns), vec![1, 6, 6, 14]);
        let tiny = layout(&mode_spec(ViewMode::Full).panel, 3);
        assert_eq!(widths(&tiny), vec![1, 1]);
    }

    #[test]
    fn alternative_full_marks_and_groups() {
        let columns = layout(&mode_spec(ViewMode::AlternativeFull).panel, 38);
        assert!(columns[0].flags.mark);
        assert!(columns[1].flags.group_digits);
        assert_eq!(widths(&columns), vec![18, 10, 8]);
    }

    #[test]
    fn percent_columns_take_their_part_of_what_the_fixed_ones_leave() {
        let defs = crate::app::panel_modes::columns("N,N,S", "40%,0,6");
        // 38 less the size and two separators is 30: the first name 40% of it, the second the rest.
        let columns = layout(&defs, 38);
        assert_eq!(widths(&columns), vec![12, 18, 6]);
        let two = crate::app::panel_modes::columns("N,N,N", "30%,20%,0");
        assert_eq!(widths(&layout(&two, 52)), vec![15, 10, 25]);
        let alone = crate::app::panel_modes::columns("N,S", "50%,6");
        assert_eq!(
            widths(&layout(&alone, 38)),
            vec![31, 6],
            "no column shares the rest: all of it"
        );
    }
}
