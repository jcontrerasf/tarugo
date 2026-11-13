//! The front sketch: the starting frame, finding the cell under a point and dividing it.

use crate::expr::format_number;
use crate::model::{DEPTH, Dim, Document, End, Extent, FaceRef, HEIGHT, PartId, Side, WIDTH};
use crate::solve::Solved;
use serde::{Deserialize, Serialize};

/// The four boards of the starting frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub left: PartId,
    pub right: PartId,
    pub bottom: PartId,
    pub top: PartId,
}

fn depth_span() -> Extent {
    Extent::Span {
        lo: End::abs(0.0),
        hi: End::abs(DEPTH),
    }
}

/// Creates the frame out of the main material: two sides of the full height with the bottom
/// and the top between them, sized by the parameters `W`, `H` and `D`.
/// `names` are those of the left, right, bottom and top boards.
pub fn new_frame(
    doc: &mut Document,
    width: f64,
    height: f64,
    depth: f64,
    names: [&str; 4],
) -> Frame {
    doc.set_param(WIDTH, Dim::num(width));
    doc.set_param(HEIGHT, Dim::num(height));
    if doc.param(DEPTH).is_none() {
        doc.set_param(DEPTH, Dim::num(depth));
    }
    let m = doc.main_material;
    let full_height = || Extent::Span {
        lo: End::abs(0.0),
        hi: End::abs(HEIGHT),
    };
    let left = doc.add_part(
        names[0].into(),
        m,
        [
            Extent::Thick {
                anchor: End::abs(0.0),
                align: Side::Min,
            },
            full_height(),
            depth_span(),
        ],
    );
    let right = doc.add_part(
        names[1].into(),
        m,
        [
            Extent::Thick {
                anchor: End::abs(WIDTH),
                align: Side::Max,
            },
            full_height(),
            depth_span(),
        ],
    );
    let between = || Extent::Span {
        lo: End::at(left, Side::Max),
        hi: End::at(right, Side::Min),
    };
    let bottom = doc.add_part(
        names[2].into(),
        m,
        [
            between(),
            Extent::Thick {
                anchor: End::abs(0.0),
                align: Side::Min,
            },
            depth_span(),
        ],
    );
    let top = doc.add_part(
        names[3].into(),
        m,
        [
            between(),
            Extent::Thick {
                anchor: End::abs(HEIGHT),
                align: Side::Max,
            },
            depth_span(),
        ],
    );
    Frame {
        left,
        right,
        bottom,
        top,
    }
}

/// Which boards of the frame run past the others at its corners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameStyle {
    /// The sides take the full height; bottom and top go between them.
    #[default]
    SidesOutside,
    /// Bottom and top take the full width; the sides go between them, as long as every
    /// vertical division.
    CapsOutside,
}

fn frame_spans(frame: &Frame, style: FrameStyle) -> (Extent, Extent) {
    let (lo, hi) = (End::abs(0.0), |name: &str| End::abs(name));
    match style {
        FrameStyle::SidesOutside => (
            Extent::Span { lo, hi: hi(HEIGHT) },
            Extent::Span {
                lo: End::at(frame.left, Side::Max),
                hi: End::at(frame.right, Side::Min),
            },
        ),
        FrameStyle::CapsOutside => (
            Extent::Span {
                lo: End::at(frame.bottom, Side::Max),
                hi: End::at(frame.top, Side::Min),
            },
            Extent::Span { lo, hi: hi(WIDTH) },
        ),
    }
}

/// Makes the corners of the frame as `style` says: the height of the sides and the width
/// of bottom and top change, everything attached to them follows. Screws between two
/// boards of the frame are dropped: their joints are not the same any more.
pub fn set_frame_style(doc: &mut Document, frame: &Frame, style: FrameStyle) {
    let (sides, caps) = frame_spans(frame, style);
    for (id, axis, span) in [
        (frame.left, 1, &sides),
        (frame.right, 1, &sides),
        (frame.bottom, 0, &caps),
        (frame.top, 0, &caps),
    ] {
        if let Some(part) = doc.part_mut(id) {
            part.axes[axis] = span.clone();
        }
    }
    let boards = [frame.left, frame.right, frame.bottom, frame.top];
    doc.screws
        .retain(|s| !(boards.contains(&s.through) && boards.contains(&s.into)));
}

/// The frame of a document and how its corners are, if its four boards are still as
/// [`new_frame`] or [`set_frame_style`] left them.
pub fn find_frame(doc: &Document) -> Option<(Frame, FrameStyle)> {
    let thick_at = |axis: usize, anchor: End, align: Side| {
        let wanted = Extent::Thick { anchor, align };
        let mut found = doc.parts.iter().filter(|p| p.axes[axis] == wanted);
        let part = found.next()?;
        found.next().is_none().then_some(part.id)
    };
    let frame = Frame {
        left: thick_at(0, End::abs(0.0), Side::Min)?,
        right: thick_at(0, End::abs(WIDTH), Side::Max)?,
        bottom: thick_at(1, End::abs(0.0), Side::Min)?,
        top: thick_at(1, End::abs(HEIGHT), Side::Max)?,
    };
    [FrameStyle::SidesOutside, FrameStyle::CapsOutside]
        .into_iter()
        .find(|style| {
            let (sides, caps) = frame_spans(&frame, *style);
            [(frame.left, 1, &sides), (frame.right, 1, &sides)]
                .into_iter()
                .chain([(frame.bottom, 0, &caps), (frame.top, 0, &caps)])
                .all(|(id, axis, span)| doc.part(id).is_some_and(|p| p.axes[axis] == *span))
        })
        .map(|style| (frame, style))
}

/// An empty rectangle of the front, with the parts that close it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub min: [f64; 2],
    pub max: [f64; 2],
    /// Parts at the low and high side of X, then of Y.
    pub bounds: [[PartId; 2]; 2],
}

impl Cell {
    pub fn size(&self, axis: usize) -> f64 {
        self.max[axis] - self.min[axis]
    }
}

/// The cell of the front view that contains the point, if it is closed on its four sides.
pub fn find_cell(doc: &Document, solved: &Solved, x: f64, y: f64) -> Option<Cell> {
    let p = [x, y];
    let mut lo: [Option<(f64, PartId)>; 2] = [None; 2];
    let mut hi: [Option<(f64, PartId)>; 2] = [None; 2];
    for part in &doc.parts {
        let Some(b) = solved.get(part.id) else {
            continue;
        };
        let inside = |a: usize| p[a] > b.min[a] && p[a] < b.max[a];
        if inside(0) && inside(1) {
            return None;
        }
        for a in 0..2 {
            if !inside(1 - a) {
                continue;
            }
            if b.max[a] <= p[a] && lo[a].is_none_or(|(v, _)| b.max[a] > v) {
                lo[a] = Some((b.max[a], part.id));
            }
            if b.min[a] >= p[a] && hi[a].is_none_or(|(v, _)| b.min[a] < v) {
                hi[a] = Some((b.min[a], part.id));
            }
        }
    }
    Some(Cell {
        min: [lo[0]?.0, lo[1]?.0],
        max: [hi[0]?.0, hi[1]?.0],
        bounds: [[lo[0]?.1, hi[0]?.1], [lo[1]?.1, hi[1]?.1]],
    })
}

/// The dimension that places a division: from a face of the part at one end of the cell to
/// a face (or the middle) of the new board.
#[derive(Clone, Debug, PartialEq)]
pub struct Cota {
    /// Measured from the part at the high end of the cell (right or top).
    pub from_hi: bool,
    /// Face of that part the dimension starts at.
    pub ref_side: Side,
    /// Face of the new board the dimension reaches.
    pub align: Side,
    /// Always positive: the direction comes from `from_hi`.
    pub distance: Dim,
}

/// `-(dim)`, simplified for numbers and for what is already negated.
pub fn negate(dim: &Dim) -> Dim {
    let text = dim.0.trim();
    if let Ok(v) = text.parse::<f64>() {
        return Dim(format_number(-v));
    }
    if let Some(inner) = text.strip_prefix("-(").and_then(|t| t.strip_suffix(')'))
        && balanced(inner)
    {
        return Dim(inner.to_string());
    }
    Dim(format!("-({text})"))
}

fn balanced(text: &str) -> bool {
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// The anchor of a division placed by `cota` in `cell` along `axis` (0: a vertical board).
pub fn cota_anchor(cell: &Cell, axis: usize, cota: &Cota) -> End {
    End::Ref {
        face: FaceRef {
            part: cell.bounds[axis][cota.from_hi as usize],
            side: cota.ref_side,
        },
        offset: if cota.from_hi {
            negate(&cota.distance)
        } else {
            cota.distance.clone()
        },
    }
}

fn division(
    doc: &mut Document,
    cell: &Cell,
    axis: usize,
    anchor: End,
    align: Side,
    name: &str,
) -> PartId {
    let across = 1 - axis;
    let span = Extent::Span {
        lo: End::at(cell.bounds[across][0], Side::Max),
        hi: End::at(cell.bounds[across][1], Side::Min),
    };
    let thick = Extent::Thick { anchor, align };
    let axes = if axis == 0 {
        [thick, span, depth_span()]
    } else {
        [span, thick, depth_span()]
    };
    let m = doc.main_material;
    doc.add_part(name.into(), m, axes)
}

/// Adds a board that divides the cell: vertical if `axis` is 0 (placed along X), horizontal
/// if 1. It spans between the parts that close the cell on the other axis.
pub fn divide(doc: &mut Document, cell: &Cell, axis: usize, cota: &Cota, name: &str) -> PartId {
    let anchor = cota_anchor(cell, axis, cota);
    division(doc, cell, axis, anchor, cota.align, name)
}

/// Divides the cell in `n` equal spaces with `n - 1` boards. They stay equal when the cell
/// changes size.
pub fn divide_equal(
    doc: &mut Document,
    cell: &Cell,
    axis: usize,
    n: usize,
    name: &str,
) -> Vec<PartId> {
    let thickness = doc
        .material(doc.main_material)
        .map_or(0.0, |m| m.fixed_size(0, false));
    (1..n)
        .map(|i| {
            // Center of the i-th board for equal clear spaces: see the offset below.
            let anchor = End::Between {
                lo: FaceRef {
                    part: cell.bounds[axis][0],
                    side: Side::Max,
                },
                hi: FaceRef {
                    part: cell.bounds[axis][1],
                    side: Side::Min,
                },
                frac: Dim(format!("{i}/{n}")),
                offset: Dim::num(thickness * (i as f64 / n as f64 - 0.5)),
            };
            let name = format!("{name} {i}");
            division(doc, cell, axis, anchor, Side::Center, &name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solve::solve;

    fn framed() -> (Document, Frame) {
        let mut doc = Document::new();
        let frame = new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        (doc, frame)
    }

    #[test]
    fn frame_has_the_asked_outside_size() {
        let (doc, frame) = framed();
        let s = solve(&doc);
        let b = s.bounds().unwrap();
        assert_eq!((b.min, b.max), ([0.0; 3], [800.0, 600.0, 300.0]));
        let top = s.boxes[&frame.top];
        assert_eq!((top.min[0], top.max[0]), (18.0, 782.0));
        assert_eq!((top.min[1], top.max[1]), (582.0, 600.0));
    }

    #[test]
    fn the_corners_of_the_frame_can_go_either_way() {
        let (mut doc, frame) = framed();
        assert_eq!(find_frame(&doc), Some((frame, FrameStyle::SidesOutside)));
        // A division and a screw between two boards of the frame, before the change.
        let s = solve(&doc);
        let cell = find_cell(&doc, &s, 400.0, 300.0).unwrap();
        let division = divide(
            &mut doc,
            &cell,
            0,
            &Cota {
                from_hi: false,
                ref_side: Side::Max,
                align: Side::Min,
                distance: Dim::num(300.0),
            },
            "d",
        );
        doc.screws.push(crate::model::Screw {
            id: 1,
            through: frame.left,
            into: frame.bottom,
            u: 0.0,
            v: 0.0,
        });
        set_frame_style(&mut doc, &frame, FrameStyle::CapsOutside);
        assert_eq!(find_frame(&doc), Some((frame, FrameStyle::CapsOutside)));
        assert!(doc.screws.is_empty());
        let s = solve(&doc);
        assert!(s.errors.is_empty());
        let b = s.bounds().unwrap();
        assert_eq!((b.min, b.max), ([0.0; 3], [800.0, 600.0, 300.0]));
        let top = s.boxes[&frame.top];
        assert_eq!((top.min[0], top.max[0]), (0.0, 800.0));
        // Sides and division: all between bottom and top, the same length.
        for id in [frame.left, frame.right, division] {
            let b = s.boxes[&id];
            assert_eq!((b.min[1], b.max[1]), (18.0, 582.0));
        }
        set_frame_style(&mut doc, &frame, FrameStyle::SidesOutside);
        assert_eq!(find_frame(&doc), Some((frame, FrameStyle::SidesOutside)));
        // A frame that was changed by hand is left alone.
        doc.part_mut(frame.left).unwrap().axes[1] = Extent::Span {
            lo: End::abs(0.0),
            hi: End::abs(500.0),
        };
        assert_eq!(find_frame(&doc), None);
    }

    #[test]
    fn cell_inside_the_frame() {
        let (doc, frame) = framed();
        let s = solve(&doc);
        let cell = find_cell(&doc, &s, 400.0, 300.0).unwrap();
        assert_eq!((cell.min, cell.max), ([18.0, 18.0], [782.0, 582.0]));
        assert_eq!(
            cell.bounds,
            [[frame.left, frame.right], [frame.bottom, frame.top]]
        );
        assert!(find_cell(&doc, &s, 5.0, 300.0).is_none(), "inside a board");
        assert!(find_cell(&doc, &s, 900.0, 300.0).is_none(), "outside");
    }

    #[test]
    fn division_references_inner_outer_and_center() {
        let (mut doc, _) = framed();
        let s = solve(&doc);
        let cell = find_cell(&doc, &s, 400.0, 300.0).unwrap();
        let x_of = |doc: &Document, id| {
            let b = solve(doc).boxes[&id];
            (b.min[0], b.max[0])
        };
        // 300 from the inner face of the left side to the near face.
        let cota = |from_hi, ref_side, align| Cota {
            from_hi,
            ref_side,
            align,
            distance: Dim::num(300.0),
        };
        let a = divide(&mut doc, &cell, 0, &cota(false, Side::Max, Side::Min), "a");
        assert_eq!(x_of(&doc, a), (318.0, 336.0));
        // 300 from the outer face of the left side to the center.
        let b = divide(
            &mut doc,
            &cell,
            0,
            &cota(false, Side::Min, Side::Center),
            "b",
        );
        assert_eq!(x_of(&doc, b), (291.0, 309.0));
        // 300 from the inner face of the right side to the near (right) face.
        let c = divide(&mut doc, &cell, 0, &cota(true, Side::Min, Side::Max), "c");
        assert_eq!(x_of(&doc, c), (464.0, 482.0));
        // 300 from the outer face of the right side to the far (left) face.
        let d = divide(&mut doc, &cell, 0, &cota(true, Side::Max, Side::Min), "d");
        assert_eq!(x_of(&doc, d), (500.0, 518.0));
        // They follow the width.
        doc.set_param(WIDTH, Dim::num(1000.0));
        assert_eq!(x_of(&doc, a), (318.0, 336.0));
        assert_eq!(x_of(&doc, c), (664.0, 682.0));
    }

    #[test]
    fn horizontal_division_splits_the_cell() {
        let (mut doc, _) = framed();
        let s = solve(&doc);
        let cell = find_cell(&doc, &s, 400.0, 300.0).unwrap();
        let cota = Cota {
            from_hi: false,
            ref_side: Side::Max,
            align: Side::Min,
            distance: Dim("(H - 3*18) / 2".into()),
        };
        let shelf = divide(&mut doc, &cell, 1, &cota, "shelf");
        let s = solve(&doc);
        let b = s.boxes[&shelf];
        assert_eq!((b.min[1], b.max[1]), (291.0, 309.0));
        assert_eq!((b.min[0], b.max[0]), (18.0, 782.0));
        let below = find_cell(&doc, &s, 400.0, 100.0).unwrap();
        assert_eq!((below.min[1], below.max[1]), (18.0, 291.0));
        assert_eq!(below.bounds[1][1], shelf);
    }

    #[test]
    fn equal_division_leaves_equal_spaces() {
        let (mut doc, frame) = framed();
        let s = solve(&doc);
        let cell = find_cell(&doc, &s, 400.0, 300.0).unwrap();
        let ids = divide_equal(&mut doc, &cell, 0, 3, "div");
        assert_eq!(ids.len(), 2);
        for width in [800.0, 1234.0] {
            doc.set_param(WIDTH, Dim::num(width));
            let s = solve(&doc);
            let mut faces = vec![s.boxes[&frame.left].max[0]];
            for id in &ids {
                faces.extend([s.boxes[id].min[0], s.boxes[id].max[0]]);
            }
            faces.push(s.boxes[&frame.right].min[0]);
            let gaps: Vec<f64> = faces.chunks(2).map(|c| c[1] - c[0]).collect();
            let expected = (width - 4.0 * 18.0) / 3.0;
            for g in gaps {
                assert!((g - expected).abs() < 1e-6, "{g} vs {expected}");
            }
        }
    }

    #[test]
    fn negation_round_trips() {
        assert_eq!(negate(&Dim::num(300.0)).0, "-300");
        assert_eq!(negate(&Dim("W/3".into())).0, "-(W/3)");
        assert_eq!(negate(&negate(&Dim("W/3".into()))).0, "W/3");
        assert_eq!(negate(&Dim("-(a)+(b)".into())).0, "-(-(a)+(b))");
    }
}
