//! Free-form outline of a part: corner treatments, notches on the edges, slots and holes,
//! all placed with dimensions. It is drawn on the face of the part, in (u, v) coordinates
//! from its lower-left corner. The cut list keeps using the bounding rectangle.

use serde::{Deserialize, Serialize};

use crate::EPS;
use crate::expr::ExprError;
use crate::model::Dim;

/// Segments of a quarter circle.
const ARC_STEPS: usize = 8;
/// Segments of a full circle.
const CIRCLE_STEPS: usize = 32;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Corner {
    #[default]
    Square,
    Round {
        radius: Dim,
    },
    Chamfer {
        size: Dim,
    },
}

/// Edges in outline order: bottom, right, top, left.
pub const EDGES: usize = 4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Feature {
    /// A bite out of an edge. `pos` is measured along the edge from its left or bottom end
    /// to the near side of the notch; `radius` rounds its inner corners.
    Notch {
        edge: usize,
        pos: Dim,
        width: Dim,
        depth: Dim,
        radius: Dim,
    },
    /// A through slot with round ends, centered at (x, y).
    Slot {
        x: Dim,
        y: Dim,
        length: Dim,
        width: Dim,
        vertical: bool,
    },
    /// A round hole centered at (x, y).
    Hole { x: Dim, y: Dim, diameter: Dim },
}

/// Corners in outline order: lower-left, lower-right, upper-right, upper-left.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub corners: [Corner; 4],
    pub features: Vec<Feature>,
}

/// What is wrong with a corner or a feature (it is then left out of the outline).
#[derive(Clone, Debug, PartialEq)]
pub enum ProblemKind {
    /// A dimension that cannot be evaluated or is not positive.
    BadDimension,
    /// It does not fit in the part.
    OutOfBounds,
    /// It runs into another feature or a corner.
    Overlap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Corner(usize),
    Feature(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub item: Item,
    pub kind: ProblemKind,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    /// Outer ring, counter-clockwise.
    pub outer: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
    pub problems: Vec<Problem>,
}

#[derive(Clone, Copy)]
struct Rect {
    min: [f64; 2],
    max: [f64; 2],
}

impl Rect {
    fn overlaps(&self, o: &Rect) -> bool {
        (0..2).all(|a| self.min[a] < o.max[a] - EPS && o.min[a] < self.max[a] - EPS)
    }

    fn inside(&self, o: &Rect) -> bool {
        (0..2).all(|a| self.min[a] >= o.min[a] - EPS && self.max[a] <= o.max[a] + EPS)
    }
}

/// A notch in the frame of its edge: from `s0` to `s1` along it, `depth` into the part.
struct Notch {
    s0: f64,
    s1: f64,
    depth: f64,
    radius: f64,
}

/// Frame of an edge: origin (its first corner, going counter-clockwise), direction, length
/// and the size of the part across it.
fn edge_frame(edge: usize, su: f64, sv: f64) -> ([f64; 2], [f64; 2], f64, f64) {
    match edge {
        0 => ([0.0, 0.0], [1.0, 0.0], su, sv),
        1 => ([su, 0.0], [0.0, 1.0], sv, su),
        2 => ([su, sv], [-1.0, 0.0], su, sv),
        _ => ([0.0, sv], [0.0, -1.0], sv, su),
    }
}

/// Point at `s` along the edge and `d` into the part.
fn edge_point(origin: [f64; 2], dir: [f64; 2], s: f64, d: f64) -> [f64; 2] {
    let inward = [-dir[1], dir[0]];
    [
        origin[0] + dir[0] * s + inward[0] * d,
        origin[1] + dir[1] * s + inward[1] * d,
    ]
}

impl Shape {
    pub fn is_plain(&self) -> bool {
        self.corners.iter().all(|c| *c == Corner::Square) && self.features.is_empty()
    }

    pub fn dims_mut(&mut self) -> Vec<&mut Dim> {
        let mut out: Vec<&mut Dim> = Vec::new();
        for c in &mut self.corners {
            match c {
                Corner::Square => {}
                Corner::Round { radius } => out.push(radius),
                Corner::Chamfer { size } => out.push(size),
            }
        }
        for f in &mut self.features {
            match f {
                Feature::Notch {
                    pos,
                    width,
                    depth,
                    radius,
                    ..
                } => out.extend([pos, width, depth, radius]),
                Feature::Slot {
                    x,
                    y,
                    length,
                    width,
                    ..
                } => out.extend([x, y, length, width]),
                Feature::Hole { x, y, diameter } => out.extend([x, y, diameter]),
            }
        }
        out
    }

    /// The outline for a face of `su` × `sv`. Anything that does not fit is reported and
    /// left out, so the result is always a valid polygon.
    pub fn outline(
        &self,
        su: f64,
        sv: f64,
        eval: &dyn Fn(&Dim) -> Result<f64, ExprError>,
    ) -> Outline {
        let mut out = Outline::default();
        let mut problem = |item, kind| out.problems.push(Problem { item, kind });
        let whole = Rect {
            min: [0.0, 0.0],
            max: [su, sv],
        };

        // Corners.
        let mut corner = [0.0f64; 4];
        for (i, c) in self.corners.iter().enumerate() {
            let dim = match c {
                Corner::Square => continue,
                Corner::Round { radius } => radius,
                Corner::Chamfer { size } => size,
            };
            match eval(dim) {
                Ok(v) if v > EPS => corner[i] = v,
                Ok(v) if v.abs() <= EPS => {}
                _ => problem(Item::Corner(i), ProblemKind::BadDimension),
            }
        }
        for edge in 0..EDGES {
            let (a, b) = (edge, (edge + 1) % 4);
            let len = edge_frame(edge, su, sv).2;
            if corner[a] + corner[b] > len + EPS {
                // Drop the bigger of the two.
                let drop = if corner[a] >= corner[b] { a } else { b };
                corner[drop] = 0.0;
                problem(Item::Corner(drop), ProblemKind::OutOfBounds);
            }
        }
        let corner_rect = |i: usize| -> Rect {
            let c = corner[i];
            let (x0, y0) = match i {
                0 => (0.0, 0.0),
                1 => (su - c, 0.0),
                2 => (su - c, sv - c),
                _ => (0.0, sv - c),
            };
            Rect {
                min: [x0, y0],
                max: [x0 + c, y0 + c],
            }
        };
        let mut taken: Vec<Rect> = (0..4)
            .filter(|&i| corner[i] > 0.0)
            .map(corner_rect)
            .collect();

        // Notches, per edge.
        let mut notches: [Vec<Notch>; 4] = Default::default();
        for (i, f) in self.features.iter().enumerate() {
            let Feature::Notch {
                edge,
                pos,
                width,
                depth,
                radius,
            } = f
            else {
                continue;
            };
            let item = Item::Feature(i);
            let (Ok(pos), Ok(width), Ok(depth), Ok(radius)) =
                (eval(pos), eval(width), eval(depth), eval(radius))
            else {
                problem(item, ProblemKind::BadDimension);
                continue;
            };
            if *edge >= EDGES || width <= EPS || depth <= EPS || radius < 0.0 {
                problem(item, ProblemKind::BadDimension);
                continue;
            }
            let (origin, dir, len, across) = edge_frame(*edge, su, sv);
            // `pos` counts from the left/bottom end; edges 2 and 3 run the other way.
            let s0 = if *edge >= 2 { len - pos - width } else { pos };
            let s1 = s0 + width;
            let (c0, c1) = (corner[*edge], corner[(*edge + 1) % 4]);
            if s0 < c0 - EPS || s1 > len - c1 + EPS || depth > across - EPS {
                problem(item, ProblemKind::OutOfBounds);
                continue;
            }
            let (p, q) = (
                edge_point(origin, dir, s0, 0.0),
                edge_point(origin, dir, s1, depth),
            );
            let rect = Rect {
                min: [p[0].min(q[0]), p[1].min(q[1])],
                max: [p[0].max(q[0]), p[1].max(q[1])],
            };
            if taken.iter().any(|t| t.overlaps(&rect)) {
                problem(item, ProblemKind::Overlap);
                continue;
            }
            taken.push(rect);
            notches[*edge].push(Notch {
                s0,
                s1,
                depth,
                radius: radius.min(width / 2.0).min(depth),
            });
        }

        // Outer ring.
        for edge in 0..EDGES {
            let (origin, dir, _, _) = edge_frame(edge, su, sv);
            let pt = |s: f64, d: f64| edge_point(origin, dir, s, d);
            let c = corner[edge];
            match &self.corners[edge] {
                _ if c == 0.0 => out.outer.push(pt(0.0, 0.0)),
                Corner::Round { .. } => {
                    for k in 0..=ARC_STEPS {
                        let a = std::f64::consts::FRAC_PI_2 * k as f64 / ARC_STEPS as f64;
                        out.outer.push(pt(c - c * a.cos(), c - c * a.sin()));
                    }
                }
                _ => {
                    out.outer.push(pt(0.0, c));
                    out.outer.push(pt(c, 0.0));
                }
            }
            notches[edge].sort_by(|a, b| a.s0.total_cmp(&b.s0));
            for n in &notches[edge] {
                let r = n.radius;
                out.outer.push(pt(n.s0, 0.0));
                if r > EPS {
                    for k in 0..=ARC_STEPS {
                        let a = std::f64::consts::FRAC_PI_2 * k as f64 / ARC_STEPS as f64;
                        out.outer
                            .push(pt(n.s0 + r - r * a.cos(), n.depth - r + r * a.sin()));
                    }
                    for k in 0..=ARC_STEPS {
                        let a = std::f64::consts::FRAC_PI_2 * k as f64 / ARC_STEPS as f64;
                        out.outer
                            .push(pt(n.s1 - r + r * a.sin(), n.depth - r + r * a.cos()));
                    }
                } else {
                    out.outer.push(pt(n.s0, n.depth));
                    out.outer.push(pt(n.s1, n.depth));
                }
                out.outer.push(pt(n.s1, 0.0));
            }
        }
        dedup_ring(&mut out.outer);

        // Slots and holes.
        for (i, f) in self.features.iter().enumerate() {
            let item = Item::Feature(i);
            let (center, half, r) = match f {
                Feature::Notch { .. } => continue,
                Feature::Hole { x, y, diameter } => {
                    let (Ok(x), Ok(y), Ok(d)) = (eval(x), eval(y), eval(diameter)) else {
                        out.problems.push(Problem {
                            item,
                            kind: ProblemKind::BadDimension,
                        });
                        continue;
                    };
                    ([x, y], [0.0, 0.0], d / 2.0)
                }
                Feature::Slot {
                    x,
                    y,
                    length,
                    width,
                    vertical,
                } => {
                    let (Ok(x), Ok(y), Ok(l), Ok(w)) =
                        (eval(x), eval(y), eval(length), eval(width))
                    else {
                        out.problems.push(Problem {
                            item,
                            kind: ProblemKind::BadDimension,
                        });
                        continue;
                    };
                    // Straight part between the centers of the two round ends.
                    let straight = ((l - w) / 2.0).max(0.0);
                    let half = if *vertical {
                        [0.0, straight]
                    } else {
                        [straight, 0.0]
                    };
                    ([x, y], half, w / 2.0)
                }
            };
            if r <= EPS {
                out.problems.push(Problem {
                    item,
                    kind: ProblemKind::BadDimension,
                });
                continue;
            }
            let rect = Rect {
                min: [center[0] - half[0] - r, center[1] - half[1] - r],
                max: [center[0] + half[0] + r, center[1] + half[1] + r],
            };
            let kind = if !rect.inside(&whole)
                || (0..2).any(|a| rect.min[a] < EPS || rect.max[a] > [su, sv][a] - EPS)
            {
                Some(ProblemKind::OutOfBounds)
            } else if taken.iter().any(|t| t.overlaps(&rect)) {
                Some(ProblemKind::Overlap)
            } else {
                None
            };
            if let Some(kind) = kind {
                out.problems.push(Problem { item, kind });
                continue;
            }
            taken.push(rect);
            out.holes.push(stadium(center, half, r));
        }
        out
    }
}

/// Ring (clockwise) of a circle of radius `r` stretched by `half` to each side.
fn stadium(center: [f64; 2], half: [f64; 2], r: f64) -> Vec<[f64; 2]> {
    let mut ring = Vec::with_capacity(CIRCLE_STEPS + 2);
    for k in 0..CIRCLE_STEPS {
        let a = -std::f64::consts::TAU * k as f64 / CIRCLE_STEPS as f64;
        let (c, s) = (a.cos(), a.sin());
        // Each half of the circle moves to its end of the slot.
        let along = if half[0] > 0.0 { c } else { s };
        let sign = if along >= 0.0 { 1.0 } else { -1.0 };
        ring.push([
            center[0] + r * c + sign * half[0],
            center[1] + r * s + sign * half[1],
        ]);
    }
    ring
}

/// Removes consecutive repeated points (also across the closing of the ring).
fn dedup_ring(ring: &mut Vec<[f64; 2]>) {
    let same = |a: &[f64; 2], b: &[f64; 2]| (a[0] - b[0]).abs() < EPS && (a[1] - b[1]).abs() < EPS;
    ring.dedup_by(|a, b| same(a, b));
    while ring.len() > 1 && same(&ring[0], &ring[ring.len() - 1]) {
        ring.pop();
    }
}

impl Outline {
    /// A plain rectangle.
    pub fn rect(su: f64, sv: f64) -> Outline {
        Outline {
            outer: vec![[0.0, 0.0], [su, 0.0], [su, sv], [0.0, sv]],
            ..Outline::default()
        }
    }

    /// All the rings: the outer one first.
    pub fn rings(&self) -> impl Iterator<Item = &Vec<[f64; 2]>> {
        std::iter::once(&self.outer).chain(self.holes.iter())
    }

    /// Every point (outer ring, then the holes) and the triangles that fill the outline.
    pub fn triangulate(&self) -> (Vec<[f64; 2]>, Vec<[usize; 3]>) {
        let points: Vec<[f64; 2]> = self.rings().flatten().copied().collect();
        let flat: Vec<f64> = points.iter().flatten().copied().collect();
        let mut hole_starts = Vec::new();
        let mut at = self.outer.len();
        for h in &self.holes {
            hole_starts.push(at);
            at += h.len();
        }
        let indices = earcutr::earcut(&flat, &hole_starts, 2).unwrap_or_default();
        let triangles = indices.as_chunks::<3>().0.to_vec();
        (points, triangles)
    }

    /// Area of the outline (holes subtracted).
    pub fn area(&self) -> f64 {
        let ring_area = |r: &Vec<[f64; 2]>| {
            let mut sum = 0.0;
            for i in 0..r.len() {
                let (a, b) = (r[i], r[(i + 1) % r.len()]);
                sum += a[0] * b[1] - b[0] * a[1];
            }
            sum / 2.0
        };
        self.rings().map(ring_area).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(d: &Dim) -> Result<f64, ExprError> {
        crate::expr::parse(&d.0)?.eval(&mut |n| Err(ExprError::UnknownName(n.to_string())))
    }

    fn d(v: f64) -> Dim {
        Dim::num(v)
    }

    #[test]
    fn plain_shape_is_the_rectangle() {
        let o = Shape::default().outline(400.0, 300.0, &eval);
        assert_eq!(o.outer, Outline::rect(400.0, 300.0).outer);
        assert!(o.problems.is_empty());
        assert!((o.area() - 120_000.0).abs() < 1e-6);
    }

    #[test]
    fn chamfer_and_round_corners() {
        let mut s = Shape::default();
        s.corners[0] = Corner::Chamfer { size: d(50.0) };
        s.corners[2] = Corner::Round { radius: d(100.0) };
        let o = s.outline(400.0, 300.0, &eval);
        assert!(o.problems.is_empty());
        let expected = 120_000.0 - 1250.0 - (10_000.0 - std::f64::consts::PI * 2500.0);
        // The arc is a polygon: slightly less material removed... within 1 %.
        assert!((o.area() - expected).abs() < 100.0, "{}", o.area());
        assert!(o.outer.contains(&[0.0, 50.0]) && o.outer.contains(&[50.0, 0.0]));
    }

    #[test]
    fn notches_cut_into_each_edge() {
        let mut s = Shape::default();
        for edge in 0..4 {
            s.features.push(Feature::Notch {
                edge,
                pos: d(100.0),
                width: d(40.0),
                depth: d(20.0),
                radius: d(0.0),
            });
        }
        let o = s.outline(400.0, 300.0, &eval);
        assert!(o.problems.is_empty());
        assert!((o.area() - (120_000.0 - 4.0 * 800.0)).abs() < 1e-6);
        // Positions count from the left/bottom on every edge.
        assert!(o.outer.contains(&[100.0, 20.0]));
        assert!(o.outer.contains(&[100.0, 280.0]));
        assert!(o.outer.contains(&[380.0, 100.0]));
        assert!(o.outer.contains(&[20.0, 100.0]));
    }

    #[test]
    fn holes_and_slots_are_triangulated_around() {
        let mut s = Shape::default();
        s.features.push(Feature::Hole {
            x: d(100.0),
            y: d(100.0),
            diameter: d(40.0),
        });
        s.features.push(Feature::Slot {
            x: d(250.0),
            y: d(150.0),
            length: d(100.0),
            width: d(20.0),
            vertical: true,
        });
        let o = s.outline(400.0, 300.0, &eval);
        assert!(o.problems.is_empty());
        assert_eq!(o.holes.len(), 2);
        let (points, triangles) = o.triangulate();
        let tri_area: f64 = triangles
            .iter()
            .map(|t| {
                let (a, b, c) = (points[t[0]], points[t[1]], points[t[2]]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0
            })
            .sum();
        assert!((tri_area - o.area()).abs() < 1e-3);
        // The slot is 100 long in v.
        let slot = &o.holes[1];
        let (lo, hi) = slot.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        });
        assert!((hi - lo - 100.0).abs() < 1e-6);
    }

    #[test]
    fn what_does_not_fit_is_reported_and_skipped() {
        let mut s = Shape::default();
        s.corners[0] = Corner::Round { radius: d(50.0) };
        s.features = vec![
            // Runs into the rounded corner.
            Feature::Notch {
                edge: 0,
                pos: d(20.0),
                width: d(40.0),
                depth: d(20.0),
                radius: d(0.0),
            },
            // Deeper than the part.
            Feature::Notch {
                edge: 1,
                pos: d(100.0),
                width: d(40.0),
                depth: d(500.0),
                radius: d(0.0),
            },
            // Sticks out.
            Feature::Hole {
                x: d(395.0),
                y: d(100.0),
                diameter: d(20.0),
            },
            Feature::Hole {
                x: d(200.0),
                y: d(150.0),
                diameter: d(20.0),
            },
            // On top of the previous hole.
            Feature::Hole {
                x: d(205.0),
                y: d(150.0),
                diameter: d(20.0),
            },
            Feature::Hole {
                x: Dim("oops".into()),
                y: d(150.0),
                diameter: d(20.0),
            },
        ];
        let o = s.outline(400.0, 300.0, &eval);
        let kinds: Vec<_> = o
            .problems
            .iter()
            .map(|p| (p.item, p.kind.clone()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (Item::Feature(0), ProblemKind::OutOfBounds),
                (Item::Feature(1), ProblemKind::OutOfBounds),
                (Item::Feature(2), ProblemKind::OutOfBounds),
                (Item::Feature(4), ProblemKind::Overlap),
                (Item::Feature(5), ProblemKind::BadDimension),
            ]
        );
        assert_eq!(o.holes.len(), 1);
    }
}
