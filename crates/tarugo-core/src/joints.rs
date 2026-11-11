//! Where parts touch (joints) and where they run into each other (overlaps).

use crate::EPS;
use crate::model::{Document, PartId};
use crate::solve::{Aabb, Solved};

/// The two axes other than `axis`, in order.
pub fn other_axes(axis: usize) -> [usize; 2] {
    match axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    }
}

/// Two parts in contact on a plane perpendicular to `axis`: `a` is on the low side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    pub a: PartId,
    pub b: PartId,
    pub axis: usize,
    /// Coordinate of the contact plane.
    pub at: f64,
    /// Contact rectangle on the other two axes (see [`other_axes`]).
    pub min: [f64; 2],
    pub max: [f64; 2],
}

impl Joint {
    pub fn has(&self, part: PartId) -> bool {
        self.a == part || self.b == part
    }

    pub fn joins(&self, p: PartId, q: PartId) -> bool {
        (self.a == p && self.b == q) || (self.a == q && self.b == p)
    }

    pub fn other(&self, part: PartId) -> PartId {
        if self.a == part { self.b } else { self.a }
    }

    pub fn size(&self) -> [f64; 2] {
        [self.max[0] - self.min[0], self.max[1] - self.min[1]]
    }

    pub fn area(&self) -> f64 {
        let s = self.size();
        s[0] * s[1]
    }

    /// Point of the contact rectangle at the fractions `u`, `v`.
    pub fn point(&self, u: f64, v: f64) -> [f64; 3] {
        let [i, j] = other_axes(self.axis);
        let mut p = [0.0; 3];
        p[self.axis] = self.at;
        p[i] = self.min[0] + (self.max[0] - self.min[0]) * u;
        p[j] = self.min[1] + (self.max[1] - self.min[1]) * v;
        p
    }

    /// Fractions of the contact rectangle closest to a point.
    pub fn fractions(&self, p: [f64; 3]) -> (f64, f64) {
        let [i, j] = other_axes(self.axis);
        let s = self.size();
        (
            ((p[i] - self.min[0]) / s[0]).clamp(0.0, 1.0),
            ((p[j] - self.min[1]) / s[1]).clamp(0.0, 1.0),
        )
    }

    /// Does the contact rectangle contain the projection of the point?
    pub fn contains(&self, p: [f64; 3]) -> bool {
        let [i, j] = other_axes(self.axis);
        p[i] >= self.min[0] && p[i] <= self.max[0] && p[j] >= self.min[1] && p[j] <= self.max[1]
    }
}

/// Two parts that share volume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Overlap {
    pub a: PartId,
    pub b: PartId,
    /// One is completely inside the other.
    pub contained: bool,
}

fn joint_between(a: PartId, ba: &Aabb, b: PartId, bb: &Aabb) -> Option<Joint> {
    for axis in 0..3 {
        let (lo, lo_box, hi, hi_box) = if (ba.max[axis] - bb.min[axis]).abs() <= EPS {
            (a, ba, b, bb)
        } else if (bb.max[axis] - ba.min[axis]).abs() <= EPS {
            (b, bb, a, ba)
        } else {
            continue;
        };
        let [i, j] = other_axes(axis);
        if lo_box.overlap(hi_box, i) <= EPS || lo_box.overlap(hi_box, j) <= EPS {
            continue;
        }
        return Some(Joint {
            a: lo,
            b: hi,
            axis,
            at: lo_box.max[axis],
            min: [
                lo_box.min[i].max(hi_box.min[i]),
                lo_box.min[j].max(hi_box.min[j]),
            ],
            max: [
                lo_box.max[i].min(hi_box.max[i]),
                lo_box.max[j].min(hi_box.max[j]),
            ],
        });
    }
    None
}

/// Solved parts in document order.
fn boxes<'a>(doc: &Document, solved: &'a Solved) -> Vec<(PartId, &'a Aabb)> {
    doc.parts
        .iter()
        .filter_map(|p| Some((p.id, solved.get(p.id)?)))
        .collect()
}

pub fn find_joints(doc: &Document, solved: &Solved) -> Vec<Joint> {
    let boxes = boxes(doc, solved);
    let mut out = Vec::new();
    for (n, (a, ba)) in boxes.iter().enumerate() {
        for (b, bb) in &boxes[n + 1..] {
            if let Some(j) = joint_between(*a, ba, *b, bb) {
                out.push(j);
            }
        }
    }
    out
}

pub fn find_overlaps(doc: &Document, solved: &Solved) -> Vec<Overlap> {
    let boxes = boxes(doc, solved);
    let mut out = Vec::new();
    for (n, (a, ba)) in boxes.iter().enumerate() {
        for (b, bb) in &boxes[n + 1..] {
            if (0..3).all(|axis| ba.overlap(bb, axis) > EPS) {
                let inside = |x: &Aabb, y: &Aabb| {
                    (0..3).all(|k| x.min[k] >= y.min[k] - EPS && x.max[k] <= y.max[k] + EPS)
                };
                out.push(Overlap {
                    a: *a,
                    b: *b,
                    contained: inside(ba, bb) || inside(bb, ba),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::front::new_frame;
    use crate::model::{Dim, End, Extent, Side};
    use crate::solve::solve;

    #[test]
    fn frame_has_four_corner_joints() {
        let mut doc = Document::new();
        let f = new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        let s = solve(&doc);
        let joints = find_joints(&doc, &s);
        assert_eq!(joints.len(), 4);
        let j = joints.iter().find(|j| j.joins(f.left, f.top)).unwrap();
        assert_eq!((j.a, j.b, j.axis, j.at), (f.left, f.top, 0, 18.0));
        // Contact: the end of the top board, 18 high and 300 deep.
        assert_eq!((j.min, j.max), ([582.0, 0.0], [600.0, 300.0]));
        assert_eq!(j.point(0.5, 0.5), [18.0, 591.0, 150.0]);
        assert_eq!(j.fractions([0.0, 591.0, 75.0]), (0.5, 0.25));
        assert!(find_overlaps(&doc, &s).is_empty());
    }

    #[test]
    fn crossing_and_contained_parts_overlap() {
        let mut doc = Document::new();
        let f = new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        let m = doc.main_material;
        let span = |lo: f64, hi: f64| Extent::Span {
            lo: End::abs(lo),
            hi: End::abs(hi),
        };
        // A shelf that goes through the left side.
        let crossing = doc.add_part(
            "x".into(),
            m,
            [
                span(-50.0, 400.0),
                Extent::Thick {
                    anchor: End::abs(Dim::num(300.0)),
                    align: Side::Min,
                },
                span(0.0, 300.0),
            ],
        );
        // A block inside the left side.
        let inside = doc.add_part(
            "i".into(),
            m,
            [
                span(2.0, 10.0),
                Extent::Thick {
                    anchor: End::abs(Dim::num(100.0)),
                    align: Side::Min,
                },
                span(10.0, 50.0),
            ],
        );
        let s = solve(&doc);
        let overlaps = find_overlaps(&doc, &s);
        assert_eq!(
            overlaps,
            vec![
                Overlap {
                    a: f.left,
                    b: crossing,
                    contained: false
                },
                Overlap {
                    a: f.left,
                    b: inside,
                    contained: true
                },
            ]
        );
    }
}
