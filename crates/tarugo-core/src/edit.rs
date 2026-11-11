//! Editing operations on parts that keep their references: moving a face, moving the whole
//! part, snapping to another part, duplicating beside and removing.

use crate::model::{Dim, Document, End, Extent, FaceRef, PartId, Side};
use crate::solve::{Solved, depends_on};

impl Solved {
    /// Where an end is along `axis`, with the solved boxes.
    pub fn end_value(&self, end: &End, axis: usize) -> Option<f64> {
        let face = |f: &FaceRef| {
            let b = self.get(f.part)?;
            Some(f.side.of(b.min[axis], b.max[axis]))
        };
        Some(match end {
            End::Abs { value } => self.eval(value).ok()?,
            End::Ref { face: f, offset } => face(f)? + self.eval(offset).ok()?,
            End::Between {
                lo,
                hi,
                frac,
                offset,
            } => {
                let (a, b) = (face(lo)?, face(hi)?);
                a + (b - a) * self.eval(frac).ok()? + self.eval(offset).ok()?
            }
        })
    }
}

/// Moves an end by `delta`: an absolute end becomes a number; one that refers to another
/// part keeps the reference and changes its offset.
fn shift_end(end: &mut End, solved: &Solved, axis: usize, delta: f64) {
    let Some(now) = solved.end_value(end, axis) else {
        return;
    };
    match end {
        End::Abs { value } => *value = Dim::num(now + delta),
        End::Ref { offset, .. } | End::Between { offset, .. } => {
            let o = solved.eval(offset).unwrap_or(0.0);
            *offset = Dim::num(o + delta);
        }
    }
}

/// Can `part` refer to `target` along `axis` without making a loop?
pub fn can_refer(doc: &Document, part: PartId, axis: usize, target: PartId) -> bool {
    part != target && doc.part(target).is_some() && !depends_on(doc, target, axis, part)
}

/// Moves one face (`Side::Min` or `Side::Max`) of a part to `target`. On an axis the
/// material fixes, the whole part moves. With `snap` the face stays attached to that face
/// of another part (when that makes no loop).
pub fn move_face(
    doc: &mut Document,
    solved: &Solved,
    part: PartId,
    axis: usize,
    side: Side,
    target: f64,
    snap: Option<FaceRef>,
) {
    let Some(b) = solved.get(part).copied() else {
        return;
    };
    let snap = snap.filter(|s| can_refer(doc, part, axis, s.part));
    let inverted_span = solved.inverted.contains(&part);
    let Some(p) = doc.part_mut(part) else {
        return;
    };
    let delta = target - side.of(b.min[axis], b.max[axis]);
    match &mut p.axes[axis] {
        Extent::Span { lo, hi } => {
            // If the ends are crossed, `lo` is the one drawn at the high side.
            let low_end = (side == Side::Min) != inverted_span;
            let end = if low_end { lo } else { hi };
            match snap {
                Some(face) => {
                    *end = End::Ref {
                        face,
                        offset: Dim::zero(),
                    }
                }
                None => shift_end(end, solved, axis, delta),
            }
        }
        Extent::Thick { anchor, align } => match snap {
            Some(face) => {
                *anchor = End::Ref {
                    face,
                    offset: Dim::zero(),
                };
                *align = side;
            }
            None => shift_end(anchor, solved, axis, delta),
        },
    }
}

/// Moves the whole part by `delta` along `axis`. With `snap`, its `side` face ends up
/// attached to that face of another part.
pub fn translate(
    doc: &mut Document,
    solved: &Solved,
    part: PartId,
    axis: usize,
    delta: f64,
    snap: Option<(Side, FaceRef)>,
) {
    let snap = snap.filter(|(_, f)| can_refer(doc, part, axis, f.part));
    let inverted_span = solved.inverted.contains(&part);
    let Some(p) = doc.part_mut(part) else {
        return;
    };
    match &mut p.axes[axis] {
        Extent::Span { lo, hi } => {
            for end in [&mut *lo, &mut *hi] {
                shift_end(end, solved, axis, delta);
            }
            if let Some((side, face)) = snap
                && side != Side::Center
            {
                let low_end = (side == Side::Min) != inverted_span;
                *(if low_end { lo } else { hi }) = End::Ref {
                    face,
                    offset: Dim::zero(),
                };
            }
        }
        Extent::Thick { anchor, align } => match snap {
            Some((side, face)) => {
                *anchor = End::Ref {
                    face,
                    offset: Dim::zero(),
                };
                *align = side;
            }
            None => shift_end(anchor, solved, axis, delta),
        },
    }
}

/// Sets the length of a part along a free axis, keeping its low face in place.
pub fn set_length(doc: &mut Document, solved: &Solved, part: PartId, axis: usize, length: f64) {
    if let Some(b) = solved.get(part) {
        move_face(
            doc,
            solved,
            part,
            axis,
            Side::Max,
            b.min[axis] + length,
            None,
        );
    }
}

/// Changes what the anchor of a fixed axis is measured from (another face of reference
/// and/or another face of the part itself) without moving the part.
pub fn retarget_anchor(
    doc: &mut Document,
    solved: &Solved,
    part: PartId,
    axis: usize,
    face: Option<FaceRef>,
    new_align: Side,
) {
    let Some(b) = solved.get(part).copied() else {
        return;
    };
    let face = face.filter(|f| can_refer(doc, part, axis, f.part));
    let own = new_align.of(b.min[axis], b.max[axis]);
    let new_anchor = match face {
        Some(f) => {
            let Some(r) = solved.get(f.part) else {
                return;
            };
            End::Ref {
                face: f,
                offset: Dim::num(own - f.side.of(r.min[axis], r.max[axis])),
            }
        }
        None => End::abs(own),
    };
    if let Some(p) = doc.part_mut(part)
        && let Extent::Thick { anchor, align } = &mut p.axes[axis]
    {
        *anchor = new_anchor;
        *align = new_align;
    }
}

/// Removes a part. Ends of other parts that referred to it stay where they are, as plain
/// coordinates; its screws go with it.
pub fn remove_part(doc: &mut Document, solved: &Solved, id: PartId) {
    for part in &mut doc.parts {
        if part.id == id {
            continue;
        }
        for (axis, extent) in part.axes.iter_mut().enumerate() {
            for end in extent.ends_mut() {
                if end.refs().any(|r| r == id) {
                    let value = solved.end_value(end, axis).unwrap_or(0.0);
                    *end = End::abs(value);
                }
            }
        }
    }
    doc.parts.retain(|p| p.id != id);
    doc.screws.retain(|s| s.through != id && s.into != id);
}

/// Copies a part right beside it along `axis` (after it, or before if `before`), attached
/// to the original: boards side by side for a top, for example.
pub fn duplicate_beside(
    doc: &mut Document,
    solved: &Solved,
    id: PartId,
    axis: usize,
    before: bool,
    name: String,
) -> Option<PartId> {
    let b = solved.get(id)?;
    let size = b.size()[axis];
    let mut copy = doc.part(id)?.clone();
    let (touching, sign) = if before {
        (Side::Min, -1.0)
    } else {
        (Side::Max, 1.0)
    };
    copy.axes[axis] = match &copy.axes[axis] {
        Extent::Span { .. } => {
            let near = End::at(id, touching);
            let far = End::at_offset(id, touching, sign * size);
            if before {
                Extent::Span { lo: far, hi: near }
            } else {
                Extent::Span { lo: near, hi: far }
            }
        }
        Extent::Thick { .. } => Extent::Thick {
            anchor: End::at(id, touching),
            align: touching.opposite(),
        },
    };
    copy.id = doc.new_id();
    copy.name = name;
    let new_id = copy.id;
    doc.parts.push(copy);
    Some(new_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::front::{Cota, divide, find_cell, new_frame};
    use crate::model::WIDTH;
    use crate::solve::solve;

    fn shelf_doc() -> (Document, crate::front::Frame, PartId) {
        let mut doc = Document::new();
        let frame = new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        let s = solve(&doc);
        let cell = find_cell(&doc, &s, 400.0, 300.0).unwrap();
        let cota = Cota {
            from_hi: false,
            ref_side: Side::Max,
            align: Side::Min,
            distance: Dim::num(200.0),
        };
        let shelf = divide(&mut doc, &cell, 1, &cota, "shelf");
        (doc, frame, shelf)
    }

    #[test]
    fn moving_a_referenced_face_keeps_the_reference() {
        let (mut doc, _, shelf) = shelf_doc();
        let s = solve(&doc);
        // Shorten the shelf 100 from the right; it still follows the right side.
        move_face(&mut doc, &s, shelf, 0, Side::Max, 682.0, None);
        assert_eq!(solve(&doc).boxes[&shelf].max[0], 682.0);
        doc.set_param(WIDTH, Dim::num(1000.0));
        assert_eq!(solve(&doc).boxes[&shelf].max[0], 882.0);
    }

    #[test]
    fn moving_a_face_of_a_fixed_axis_moves_the_part() {
        let (mut doc, _, shelf) = shelf_doc();
        let s = solve(&doc);
        move_face(&mut doc, &s, shelf, 1, Side::Max, 400.0, None);
        let b = solve(&doc).boxes[&shelf];
        assert_eq!((b.min[1], b.max[1]), (382.0, 400.0));
    }

    #[test]
    fn snapping_attaches_but_never_makes_a_loop() {
        let (mut doc, frame, shelf) = shelf_doc();
        let s = solve(&doc);
        // Shelf's left face to the outer face of the left side.
        let face = FaceRef {
            part: frame.left,
            side: Side::Min,
        };
        move_face(&mut doc, &s, shelf, 0, Side::Min, 0.0, Some(face));
        assert_eq!(solve(&doc).boxes[&shelf].min[0], 0.0);
        // The left side cannot hang from the shelf, which hangs from it: plain move.
        let s = solve(&doc);
        let loop_face = FaceRef {
            part: shelf,
            side: Side::Max,
        };
        assert!(!can_refer(&doc, frame.left, 0, shelf));
        move_face(
            &mut doc,
            &s,
            frame.left,
            0,
            Side::Min,
            50.0,
            Some(loop_face),
        );
        let s = solve(&doc);
        assert!(s.errors.is_empty());
        assert_eq!(s.boxes[&frame.left].min[0], 50.0);
    }

    #[test]
    fn translating_moves_both_ends() {
        let (mut doc, _, shelf) = shelf_doc();
        let s = solve(&doc);
        translate(&mut doc, &s, shelf, 0, 40.0, None);
        translate(&mut doc, &s, shelf, 1, -18.0, None);
        let b = solve(&doc).boxes[&shelf];
        assert_eq!((b.min[0], b.max[0]), (58.0, 822.0));
        assert_eq!((b.min[1], b.max[1]), (200.0, 218.0));
    }

    #[test]
    fn removing_a_part_freezes_what_referred_to_it() {
        let (mut doc, frame, shelf) = shelf_doc();
        let s = solve(&doc);
        remove_part(&mut doc, &s, frame.left);
        let s = solve(&doc);
        assert!(s.errors.is_empty());
        assert_eq!(s.boxes[&shelf].min[0], 18.0);
        assert_eq!(s.boxes[&frame.top].min[0], 18.0);
    }

    #[test]
    fn duplicates_sit_beside_the_original() {
        let (mut doc, _, shelf) = shelf_doc();
        let s = solve(&doc);
        // Along the depth: a second board in front of the shelf.
        let front = duplicate_beside(&mut doc, &s, shelf, 2, false, "front".into()).unwrap();
        let above = duplicate_beside(&mut doc, &s, shelf, 1, false, "above".into()).unwrap();
        let before = duplicate_beside(&mut doc, &s, shelf, 2, true, "back".into()).unwrap();
        let s = solve(&doc);
        assert_eq!(
            (s.boxes[&front].min[2], s.boxes[&front].max[2]),
            (300.0, 600.0)
        );
        assert_eq!(
            (s.boxes[&above].min[1], s.boxes[&above].max[1]),
            (236.0, 254.0)
        );
        assert_eq!(
            (s.boxes[&before].min[2], s.boxes[&before].max[2]),
            (-300.0, 0.0)
        );
    }

    #[test]
    fn retargeting_keeps_the_part_in_place() {
        let (mut doc, frame, shelf) = shelf_doc();
        let s = solve(&doc);
        let before = s.boxes[&shelf];
        let face = FaceRef {
            part: frame.top,
            side: Side::Max,
        };
        retarget_anchor(&mut doc, &s, shelf, 1, Some(face), Side::Center);
        assert_eq!(solve(&doc).boxes[&shelf], before);
        let Extent::Thick { anchor, align } = &doc.part(shelf).unwrap().axes[1] else {
            panic!()
        };
        assert_eq!(*align, Side::Center);
        assert_eq!(*anchor, End::at_offset(frame.top, Side::Max, -373.0));
    }
}
