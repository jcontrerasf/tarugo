//! The divide tool of the front sketch: where the new board would go and its dimension.

use tarugo_core::front::{Cell, Cota, find_cell};
use tarugo_core::model::{Dim, Document, Side};
use tarugo_core::solve::Solved;

use super::{AlignChoice, DivideOptions, RefChoice};

/// What a click of the divide tool would do.
#[derive(Clone, Debug)]
pub struct Preview {
    pub cell: Cell,
    /// Axis the board is placed along (0: a vertical board).
    pub axis: usize,
    pub cota: Cota,
    /// Where the dimension starts (the reference face).
    pub from: f64,
    /// Intervals the new boards take along the axis (one, or several for equal spaces).
    pub boards: Vec<(f64, f64)>,
}

/// Face of the part at the end of a cell a dimension starts from.
pub fn ref_side(choice: RefChoice, from_hi: bool) -> Side {
    match (choice, from_hi) {
        (RefChoice::Center, _) => Side::Center,
        (RefChoice::Inner, false) | (RefChoice::Outer, true) => Side::Max,
        (RefChoice::Inner, true) | (RefChoice::Outer, false) => Side::Min,
    }
}

/// Face of the new board a dimension reaches.
pub fn align_side(choice: AlignChoice, from_hi: bool) -> Side {
    match (choice, from_hi) {
        (AlignChoice::Center, _) => Side::Center,
        (AlignChoice::Near, false) | (AlignChoice::Far, true) => Side::Min,
        (AlignChoice::Near, true) | (AlignChoice::Far, false) => Side::Max,
    }
}

pub fn preview(
    doc: &Document,
    solved: &Solved,
    options: &DivideOptions,
    axis: usize,
    point: [f64; 2],
) -> Option<Preview> {
    let cell = find_cell(doc, solved, point[0], point[1])?;
    let t = doc.material(doc.main_material)?.fixed_size(0, false);
    let (lo, hi) = (cell.min[axis], cell.max[axis]);
    if hi - lo < t + 2.0 {
        return None;
    }
    let from_hi = point[axis] > (lo + hi) / 2.0;
    let side = ref_side(options.reference, from_hi);
    let align = align_side(options.align, from_hi);
    let bound = solved.get(cell.bounds[axis][from_hi as usize])?;
    let from = side.of(bound.min[axis], bound.max[axis]);

    if options.equal {
        let n = options.spaces.max(2);
        let gap = (hi - lo - (n - 1) as f64 * t) / n as f64;
        if gap < 1.0 {
            return None;
        }
        let boards = (1..n)
            .map(|i| {
                let start = lo + i as f64 * gap + (i - 1) as f64 * t;
                (start, start + t)
            })
            .collect();
        return Some(Preview {
            cell,
            axis,
            cota: Cota {
                from_hi,
                ref_side: side,
                align,
                distance: Dim::num(gap),
            },
            from,
            boards,
        });
    }

    // The pointer marks the middle of the board, kept inside the cell.
    let center = point[axis].clamp(lo + t / 2.0, hi - t / 2.0);
    let face = align.of(center - t / 2.0, center + t / 2.0);
    let sign = if from_hi { -1.0 } else { 1.0 };
    // Whole millimetres, and never across the reference.
    let distance = ((face - from) * sign).round().max(0.0);
    let face = from + sign * distance;
    let start = match align {
        Side::Min => face,
        Side::Center => face - t / 2.0,
        Side::Max => face - t,
    };
    Some(Preview {
        cell,
        axis,
        cota: Cota {
            from_hi,
            ref_side: side,
            align,
            distance: Dim::num(distance),
        },
        from,
        boards: vec![(start, start + t)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarugo_core::front::{divide, new_frame};
    use tarugo_core::solve::solve;

    #[test]
    fn preview_matches_what_gets_created() {
        let mut doc = Document::new();
        new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        let s = solve(&doc);
        for (x, reference, align) in [
            (250.4, RefChoice::Inner, AlignChoice::Near),
            (250.4, RefChoice::Outer, AlignChoice::Center),
            (610.7, RefChoice::Inner, AlignChoice::Far),
            (610.7, RefChoice::Center, AlignChoice::Near),
        ] {
            let options = DivideOptions {
                reference,
                align,
                ..DivideOptions::default()
            };
            let p = preview(&doc, &s, &options, 0, [x, 300.0]).unwrap();
            let mut copy = doc.clone();
            let id = divide(&mut copy, &p.cell, 0, &p.cota, "d");
            let b = solve(&copy).boxes[&id];
            assert_eq!((b.min[0], b.max[0]), p.boards[0], "{reference:?} {align:?}");
            // Within a millimetre of the pointer.
            assert!(((b.min[0] + b.max[0]) / 2.0 - x).abs() <= 1.0);
        }
    }

    #[test]
    fn equal_spaces_preview() {
        let mut doc = Document::new();
        new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        let s = solve(&doc);
        let options = DivideOptions {
            equal: true,
            spaces: 3,
            ..DivideOptions::default()
        };
        let p = preview(&doc, &s, &options, 1, [400.0, 300.0]).unwrap();
        assert_eq!(p.boards, vec![(194.0, 212.0), (388.0, 406.0)]);
        assert!(preview(&doc, &s, &options, 1, [5.0, 300.0]).is_none());
    }
}
