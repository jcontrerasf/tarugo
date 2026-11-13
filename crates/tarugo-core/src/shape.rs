//! Free-form outline of a part: corner treatments and figures (notches, slots, holes,
//! rectangles, polygons) that are subtracted from its face. A figure inside the face is a
//! cutout; one across its border bites the outline. Everything is placed with dimensions,
//! in (u, v) coordinates from the lower-left corner of the face. The cut list keeps using
//! the bounding rectangle.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
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
    /// A rectangle from its lower-left corner (x, y); `radius` rounds its four corners.
    Rect {
        x: Dim,
        y: Dim,
        width: Dim,
        height: Dim,
        radius: Dim,
    },
    /// A polygon, convex or not, with a radius at every vertex.
    Polygon { points: Vec<Vertex> },
    /// A piece of a circle closed by its chord: centered at (x, y), it bulges towards
    /// `angle` and spans `sweep` (both in degrees; 180 is a half circle).
    Arc {
        x: Dim,
        y: Dim,
        radius: Dim,
        angle: Dim,
        sweep: Dim,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub x: Dim,
    pub y: Dim,
    #[serde(default = "Dim::zero")]
    pub radius: Dim,
}

/// Something of a feature that the mouse drags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// Side or corner of a rectangle: per axis -1 (low side), 0 (not moved) or 1 (high).
    Rect([i8; 2]),
    /// Rim of a hole.
    Radius,
    /// End of a slot: the low one or the high one.
    SlotEnd(bool),
    SlotWidth,
    Vertex(usize),
    /// Middle of the polygon edge that leaves this vertex: dragging it adds a vertex.
    Midpoint(usize),
    NotchStart,
    NotchEnd,
    NotchDepth,
    /// Middle of an arc: its radius and where it bulges to.
    ArcApex,
    /// One end of an arc: how much it spans.
    ArcEnd,
}

pub type Point = [f64; 2];
type Eval<'a> = &'a dyn Fn(&Dim) -> Result<f64, ExprError>;

/// Corners in outline order: lower-left, lower-right, upper-right, upper-left.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub corners: [Corner; 4],
    pub features: Vec<Feature>,
    /// Lines to draw against: they cut nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<Guide>,
}

/// A guide line across the face: at `at` on `axis` (0: a vertical line at that x; 1: a
/// horizontal one at that y).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub axis: usize,
    pub at: Dim,
}

/// What is wrong with a corner or a feature (it is then left out of the outline).
#[derive(Clone, Debug, PartialEq)]
pub enum ProblemKind {
    /// A dimension that cannot be evaluated or is not positive.
    BadDimension,
    /// It does not touch the part (or a corner does not fit).
    OutOfBounds,
    /// A polygon whose sides cross.
    SelfIntersecting,
    /// The cuts leave the part in several pieces (only the largest is kept).
    Splits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Corner(usize),
    Feature(usize),
    Whole,
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
    /// The figure every feature with good dimensions cuts away (feature index,
    /// counter-clockwise ring), also where it lies outside the part.
    pub cutters: Vec<(usize, Vec<[f64; 2]>)>,
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

/// A plain number for a dimension set with the mouse.
fn number(v: f64) -> Dim {
    let v = (v * 10.0).round() / 10.0;
    Dim::num(if v == 0.0 { 0.0 } else { v })
}

/// Signed area of a ring: positive if it turns counter-clockwise.
pub fn ring_area(ring: &[Point]) -> f64 {
    let mut sum = 0.0;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        sum += a[0] * b[1] - b[0] * a[1];
    }
    sum / 2.0
}

/// Lowest and highest corner of the box around a ring.
pub fn ring_bounds(ring: &[Point]) -> (Point, Point) {
    let mut min = [f64::MAX; 2];
    let mut max = [f64::MIN; 2];
    for p in ring {
        for a in 0..2 {
            min[a] = min[a].min(p[a]);
            max[a] = max[a].max(p[a]);
        }
    }
    (min, max)
}

pub fn point_in_ring(ring: &[Point], p: Point) -> bool {
    let mut inside = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
        {
            inside = !inside;
        }
    }
    inside
}

/// The target closest to `value`, if one is within `tolerance`.
pub fn snap(value: f64, targets: &[f64], tolerance: f64) -> Option<f64> {
    targets
        .iter()
        .copied()
        .filter(|t| (t - value).abs() <= tolerance)
        .min_by(|a, b| (a - value).abs().total_cmp(&(b - value).abs()))
}

/// Where `to` goes so that the line from `from` runs at a multiple of `step` degrees; its
/// length is what `to` reaches along that direction, in whole millimetres.
pub fn snap_angle(from: Point, to: Point, step: f64) -> Point {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let step = step.to_radians();
    let angle = (dy.atan2(dx) / step).round() * step;
    let (c, s) = (angle.cos(), angle.sin());
    let len = (dx * c + dy * s).round();
    [from[0] + len * c, from[1] + len * s]
}

/// The length closest to `len` (within `tolerance`) at which the ray from `from` along
/// the unit direction `dir` crosses one of the lines `targets` (per axis: `x = t`,
/// `y = t`). Returns it with the line crossed: axis and value.
pub fn snap_along(
    from: Point,
    dir: Point,
    len: f64,
    targets: &[Vec<f64>; 2],
    tolerance: f64,
) -> Option<(f64, usize, f64)> {
    let mut best: Option<(f64, usize, f64)> = None;
    for axis in 0..2 {
        if dir[axis].abs() < 1e-6 {
            continue;
        }
        for &t in &targets[axis] {
            let at = (t - from[axis]) / dir[axis];
            if at > EPS
                && (at - len).abs() <= tolerance
                && best.is_none_or(|b| (at - len).abs() < (b.0 - len).abs())
            {
                best = Some((at, axis, t));
            }
        }
    }
    best
}

/// The polygon through `points` with a fillet of `radii[n]` at vertex `n`, convex or not.
/// Two fillets that do not fit on the side they share shrink in proportion; one alone may
/// take the whole side.
pub fn rounded_ring(points: &[Point], radii: &[f64]) -> Vec<Point> {
    let n = points.len();
    // Per vertex: directions to its neighbours, the angle between them and how much of
    // each side its fillet wants.
    struct Fillet {
        v1: Point,
        v2: Point,
        theta: f64,
        want: f64,
    }
    let fillets: Vec<Option<Fillet>> = (0..n)
        .map(|i| {
            let c = points[i];
            let r = radii.get(i).copied().unwrap_or(0.0);
            let (p, q) = (points[(i + n - 1) % n], points[(i + 1) % n]);
            let (d1, d2) = ([p[0] - c[0], p[1] - c[1]], [q[0] - c[0], q[1] - c[1]]);
            let (l1, l2) = (d1[0].hypot(d1[1]), d2[0].hypot(d2[1]));
            if r <= EPS || l1 <= EPS || l2 <= EPS {
                return None;
            }
            let (v1, v2) = ([d1[0] / l1, d1[1] / l1], [d2[0] / l2, d2[1] / l2]);
            let theta = (v1[0] * v2[0] + v1[1] * v2[1]).clamp(-1.0, 1.0).acos();
            let tan_half = (theta / 2.0).tan();
            // Straight through, or folded back on itself: nothing to round.
            (1e-3..1e6).contains(&tan_half).then_some(Fillet {
                v1,
                v2,
                theta,
                want: r / tan_half,
            })
        })
        .collect();
    let want = |i: usize| fillets[i % n].as_ref().map_or(0.0, |f| f.want);
    // How much of what they want the two fillets of the side after vertex `i` can have.
    let fit = |i: usize| {
        let (a, b) = (points[i % n], points[(i + 1) % n]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let sum = want(i) + want(i + 1);
        if sum > len { len / sum } else { 1.0 }
    };
    let mut ring = Vec::new();
    for i in 0..n {
        let c = points[i];
        let Some(f) = &fillets[i] else {
            ring.push(c);
            continue;
        };
        let t = f.want * fit(i + n - 1).min(fit(i));
        let r = t * (f.theta / 2.0).tan();
        let (v1, v2) = (f.v1, f.v2);
        let a = [c[0] + v1[0] * t, c[1] + v1[1] * t];
        let b = [c[0] + v2[0] * t, c[1] + v2[1] * t];
        let bis = [v1[0] + v2[0], v1[1] + v2[1]];
        let bl = bis[0].hypot(bis[1]);
        let reach = r / (f.theta / 2.0).sin();
        let o = [c[0] + bis[0] / bl * reach, c[1] + bis[1] / bl * reach];
        let a0 = (a[1] - o[1]).atan2(a[0] - o[0]);
        let a1 = (b[1] - o[1]).atan2(b[0] - o[0]);
        let mut sweep = a1 - a0;
        while sweep > std::f64::consts::PI {
            sweep -= std::f64::consts::TAU;
        }
        while sweep < -std::f64::consts::PI {
            sweep += std::f64::consts::TAU;
        }
        let steps =
            ((sweep.abs() / std::f64::consts::FRAC_PI_2 * ARC_STEPS as f64).ceil() as usize).max(2);
        ring.push(a);
        for k in 1..steps {
            let ang = a0 + sweep * k as f64 / steps as f64;
            ring.push([o[0] + r * ang.cos(), o[1] + r * ang.sin()]);
        }
        ring.push(b);
    }
    dedup_ring(&mut ring);
    ring
}

/// Do two sides of the polygon cross?
fn self_intersects(points: &[Point]) -> bool {
    let n = points.len();
    let side = |a: Point, b: Point, c: Point| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let (a, b) = (points[i], points[(i + 1) % n]);
            let (c, d) = (points[j], points[(j + 1) % n]);
            if side(a, b, c) * side(a, b, d) < 0.0 && side(c, d, a) * side(c, d, b) < 0.0 {
                return true;
            }
        }
    }
    false
}

/// The figure a feature cuts away from a face of `su` × `sv`.
fn feature_ring(f: &Feature, su: f64, sv: f64, eval: Eval) -> Result<Vec<Point>, ProblemKind> {
    let get = |d: &Dim| eval(d).map_err(|_| ProblemKind::BadDimension);
    let ring = match f {
        Feature::Notch {
            edge,
            pos,
            width,
            depth,
            radius,
        } => {
            let (pos, width, depth, radius) = (get(pos)?, get(width)?, get(depth)?, get(radius)?);
            if *edge >= EDGES || width <= EPS || depth <= EPS || radius < 0.0 {
                return Err(ProblemKind::BadDimension);
            }
            let (origin, dir, len, _) = edge_frame(*edge, su, sv);
            // `pos` counts from the left/bottom end; edges 2 and 3 run the other way.
            let s0 = if *edge >= 2 { len - pos - width } else { pos };
            let s1 = s0 + width;
            let r = radius.min(width / 2.0).min(depth);
            // It sticks out of the part, so only its inner corners matter.
            let out = -depth.max(10.0);
            let pt = |s: f64, d: f64| edge_point(origin, dir, s, d);
            rounded_ring(
                &[pt(s0, out), pt(s1, out), pt(s1, depth), pt(s0, depth)],
                &[0.0, 0.0, r, r],
            )
        }
        Feature::Hole { x, y, diameter } => {
            let (x, y, d) = (get(x)?, get(y)?, get(diameter)?);
            if d <= 2.0 * EPS {
                return Err(ProblemKind::BadDimension);
            }
            stadium([x, y], [0.0, 0.0], d / 2.0)
        }
        Feature::Slot {
            x,
            y,
            length,
            width,
            vertical,
        } => {
            let (x, y, l, w) = (get(x)?, get(y)?, get(length)?, get(width)?);
            if w <= 2.0 * EPS {
                return Err(ProblemKind::BadDimension);
            }
            // Straight part between the centers of the two round ends.
            let straight = ((l - w) / 2.0).max(0.0);
            let half = if *vertical {
                [0.0, straight]
            } else {
                [straight, 0.0]
            };
            stadium([x, y], half, w / 2.0)
        }
        Feature::Rect {
            x,
            y,
            width,
            height,
            radius,
        } => {
            let (x, y, w, h, r) = (get(x)?, get(y)?, get(width)?, get(height)?, get(radius)?);
            if w <= EPS || h <= EPS || r < 0.0 {
                return Err(ProblemKind::BadDimension);
            }
            rounded_ring(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]], &[r; 4])
        }
        Feature::Arc {
            x,
            y,
            radius,
            angle,
            sweep,
        } => {
            let (x, y, r, angle, sweep) =
                (get(x)?, get(y)?, get(radius)?, get(angle)?, get(sweep)?);
            if r <= EPS || sweep <= EPS || sweep > 360.0 + EPS {
                return Err(ProblemKind::BadDimension);
            }
            let steps = ((sweep / 360.0 * CIRCLE_STEPS as f64).ceil() as usize).max(4);
            let mut ring: Vec<Point> = (0..=steps)
                .map(|k| {
                    let a = (angle - sweep / 2.0 + sweep * k as f64 / steps as f64).to_radians();
                    [x + r * a.cos(), y + r * a.sin()]
                })
                .collect();
            dedup_ring(&mut ring);
            ring
        }
        Feature::Polygon { points } => {
            let mut at = Vec::with_capacity(points.len());
            let mut radii = Vec::with_capacity(points.len());
            for v in points {
                at.push([get(&v.x)?, get(&v.y)?]);
                let r = get(&v.radius)?;
                if r < 0.0 {
                    return Err(ProblemKind::BadDimension);
                }
                radii.push(r);
            }
            dedup_ring(&mut at);
            if at.len() != points.len() || at.len() < 3 {
                return Err(ProblemKind::BadDimension);
            }
            if self_intersects(&at) {
                return Err(ProblemKind::SelfIntersecting);
            }
            rounded_ring(&at, &radii)
        }
    };
    let mut ring = ring;
    let area = ring_area(&ring);
    if area.abs() <= EPS {
        return Err(ProblemKind::BadDimension);
    }
    if area < 0.0 {
        ring.reverse();
    }
    Ok(ring)
}

impl Shape {
    /// Nothing is cut: the part is its rectangle (it may still have guide lines).
    pub fn is_plain(&self) -> bool {
        self.corners.iter().all(|c| *c == Corner::Square) && self.features.is_empty()
    }

    /// Nothing worth keeping: neither cuts nor guide lines.
    pub fn is_empty(&self) -> bool {
        self.is_plain() && self.guides.is_empty()
    }

    pub fn dims_mut(&mut self) -> Vec<&mut Dim> {
        let mut out: Vec<&mut Dim> = self.guides.iter_mut().map(|g| &mut g.at).collect();
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
                Feature::Arc {
                    x,
                    y,
                    radius,
                    angle,
                    sweep,
                } => out.extend([x, y, radius, angle, sweep]),
                Feature::Rect {
                    x,
                    y,
                    width,
                    height,
                    radius,
                } => out.extend([x, y, width, height, radius]),
                Feature::Polygon { points } => {
                    for v in points {
                        out.extend([&mut v.x, &mut v.y, &mut v.radius]);
                    }
                }
            }
        }
        out
    }

    /// Size of every corner treatment (0 for a square one or one that does not fit).
    fn corner_sizes(&self, su: f64, sv: f64, eval: Eval, problems: &mut Vec<Problem>) -> [f64; 4] {
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
                _ => problems.push(Problem {
                    item: Item::Corner(i),
                    kind: ProblemKind::BadDimension,
                }),
            }
        }
        for edge in 0..EDGES {
            let (a, b) = (edge, (edge + 1) % 4);
            let len = edge_frame(edge, su, sv).2;
            if corner[a] + corner[b] > len + EPS {
                // Drop the bigger of the two.
                let drop = if corner[a] >= corner[b] { a } else { b };
                corner[drop] = 0.0;
                problems.push(Problem {
                    item: Item::Corner(drop),
                    kind: ProblemKind::OutOfBounds,
                });
            }
        }
        corner
    }

    /// The outline for a face of `su` × `sv`: the rectangle with its corners worked, minus
    /// every figure. Anything that is wrong is reported and left out, so the result is
    /// always a valid polygon.
    pub fn outline(&self, su: f64, sv: f64, eval: Eval) -> Outline {
        let mut out = Outline::default();
        let corner = self.corner_sizes(su, sv, eval, &mut out.problems);
        let mut base = Vec::new();
        for (edge, &c) in corner.iter().enumerate() {
            let (origin, dir, _, _) = edge_frame(edge, su, sv);
            let pt = |s: f64, d: f64| edge_point(origin, dir, s, d);
            match &self.corners[edge] {
                _ if c == 0.0 => base.push(pt(0.0, 0.0)),
                Corner::Round { .. } => {
                    for k in 0..=ARC_STEPS {
                        let a = std::f64::consts::FRAC_PI_2 * k as f64 / ARC_STEPS as f64;
                        base.push(pt(c - c * a.cos(), c - c * a.sin()));
                    }
                }
                _ => {
                    base.push(pt(0.0, c));
                    base.push(pt(c, 0.0));
                }
            }
        }
        dedup_ring(&mut base);

        for (i, f) in self.features.iter().enumerate() {
            match feature_ring(f, su, sv, eval) {
                Ok(ring) => {
                    // Off the part it cuts nothing, but it stays to be brought back.
                    let (min, max) = ring_bounds(&ring);
                    if min[0] >= su - EPS || min[1] >= sv - EPS || max[0] <= EPS || max[1] <= EPS {
                        out.problems.push(Problem {
                            item: Item::Feature(i),
                            kind: ProblemKind::OutOfBounds,
                        });
                    }
                    out.cutters.push((i, ring));
                }
                Err(kind) => out.problems.push(Problem {
                    item: Item::Feature(i),
                    kind,
                }),
            }
        }
        if out.cutters.is_empty() {
            out.outer = base;
            return out;
        }

        let clip: Vec<Vec<Point>> = out.cutters.iter().map(|(_, r)| r.clone()).collect();
        let mut pieces = base.overlay(&clip, OverlayRule::Difference, FillRule::NonZero);
        let piece_area =
            |piece: &Vec<Vec<Point>>| -> f64 { piece.iter().map(|r| ring_area(r).abs()).sum() };
        // Slivers left by cuts that only just touch do not count as pieces.
        pieces.retain(|piece| !piece.is_empty() && ring_area(&piece[0]).abs() > 1.0);
        if pieces.len() != 1 {
            out.problems.push(Problem {
                item: Item::Whole,
                kind: ProblemKind::Splits,
            });
        }
        let largest = pieces
            .into_iter()
            .max_by(|a, b| piece_area(a).total_cmp(&piece_area(b)));
        let Some(piece) = largest else {
            out.outer = base;
            return out;
        };
        for (n, mut ring) in piece.into_iter().enumerate() {
            dedup_ring(&mut ring);
            // Outer ring counter-clockwise, holes clockwise.
            if (ring_area(&ring) > 0.0) != (n == 0) {
                ring.reverse();
            }
            if n == 0 {
                out.outer = ring;
            } else {
                out.holes.push(ring);
            }
        }
        out
    }

    /// Moves a figure by `delta`. What it moves becomes a plain number; a notch only
    /// slides along its edge.
    pub fn translate(&mut self, feature: usize, delta: Point, eval: Eval) {
        let Some(f) = self.features.get_mut(feature) else {
            return;
        };
        let shift = |dim: &mut Dim, by: f64| {
            if by.abs() > 1e-9
                && let Ok(v) = eval(dim)
            {
                *dim = number(v + by);
            }
        };
        match f {
            Feature::Notch { edge, pos, .. } => shift(pos, delta[*edge % 2]),
            Feature::Slot { x, y, .. }
            | Feature::Hole { x, y, .. }
            | Feature::Arc { x, y, .. }
            | Feature::Rect { x, y, .. } => {
                shift(x, delta[0]);
                shift(y, delta[1]);
            }
            Feature::Polygon { points } => {
                for v in points {
                    shift(&mut v.x, delta[0]);
                    shift(&mut v.y, delta[1]);
                }
            }
        }
    }

    /// What can be dragged of a figure, and where it is on a face of `su` × `sv`.
    pub fn handles(&self, feature: usize, su: f64, sv: f64, eval: Eval) -> Vec<(Handle, Point)> {
        let get = |d: &Dim| eval(d).unwrap_or(0.0);
        let mut out = Vec::new();
        match self.features.get(feature) {
            None => {}
            Some(Feature::Rect {
                x,
                y,
                width,
                height,
                ..
            }) => {
                let (x, y, w, h) = (get(x), get(y), get(width), get(height));
                for sx in [-1i8, 0, 1] {
                    for sy in [-1i8, 0, 1] {
                        if sx != 0 || sy != 0 {
                            let at = [
                                x + w * (sx as f64 + 1.0) / 2.0,
                                y + h * (sy as f64 + 1.0) / 2.0,
                            ];
                            out.push((Handle::Rect([sx, sy]), at));
                        }
                    }
                }
            }
            Some(Feature::Hole { x, y, diameter }) => {
                out.push((Handle::Radius, [get(x) + get(diameter) / 2.0, get(y)]));
            }
            Some(Feature::Arc {
                x,
                y,
                radius,
                angle,
                sweep,
            }) => {
                let (c, r, angle, sweep) = ([get(x), get(y)], get(radius), get(angle), get(sweep));
                let on = |deg: f64| {
                    let a = deg.to_radians();
                    [c[0] + r * a.cos(), c[1] + r * a.sin()]
                };
                out.push((Handle::ArcApex, on(angle)));
                out.push((Handle::ArcEnd, on(angle + sweep / 2.0)));
            }
            Some(Feature::Slot {
                x,
                y,
                length,
                width,
                vertical,
            }) => {
                let (c, l, w) = ([get(x), get(y)], get(length), get(width));
                let a = usize::from(*vertical);
                let mut lo = c;
                let mut hi = c;
                let mut side = c;
                lo[a] -= l / 2.0;
                hi[a] += l / 2.0;
                side[1 - a] += w / 2.0;
                out.push((Handle::SlotEnd(false), lo));
                out.push((Handle::SlotEnd(true), hi));
                out.push((Handle::SlotWidth, side));
            }
            Some(Feature::Polygon { points }) => {
                let at: Vec<Point> = points.iter().map(|v| [get(&v.x), get(&v.y)]).collect();
                for (n, p) in at.iter().enumerate() {
                    out.push((Handle::Vertex(n), *p));
                }
                for (n, p) in at.iter().enumerate() {
                    let q = at[(n + 1) % at.len()];
                    out.push((
                        Handle::Midpoint(n),
                        [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0],
                    ));
                }
            }
            Some(Feature::Notch {
                edge,
                pos,
                width,
                depth,
                ..
            }) => {
                let (pos, width, depth) = (get(pos), get(width), get(depth));
                let (origin, dir, len, _) = edge_frame(*edge, su, sv);
                let s0 = if *edge >= 2 { len - pos - width } else { pos };
                let pt = |s: f64, d: f64| edge_point(origin, dir, s, d);
                // In world terms the notch starts at `pos` on every edge.
                let (start, end) = if *edge >= 2 {
                    (s0 + width, s0)
                } else {
                    (s0, s0 + width)
                };
                out.push((Handle::NotchStart, pt(start, depth / 2.0)));
                out.push((Handle::NotchEnd, pt(end, depth / 2.0)));
                out.push((Handle::NotchDepth, pt(s0 + width / 2.0, depth)));
            }
        }
        out
    }

    /// Takes a handle of a figure to the point `to`.
    pub fn drag_handle(
        &mut self,
        feature: usize,
        handle: Handle,
        to: Point,
        su: f64,
        sv: f64,
        eval: Eval,
    ) {
        let get = |d: &Dim| eval(d).unwrap_or(0.0);
        match (self.features.get_mut(feature), handle) {
            (
                Some(Feature::Rect {
                    x,
                    y,
                    width,
                    height,
                    ..
                }),
                Handle::Rect(sides),
            ) => {
                for (a, (lo, size)) in [(x, width), (y, height)].into_iter().enumerate() {
                    let (l, h) = (get(lo), get(lo) + get(size));
                    match sides[a] {
                        -1 => {
                            let l = to[a].min(h - 1.0);
                            *lo = number(l);
                            *size = number(h - l);
                        }
                        1 => *size = number((to[a] - l).max(1.0)),
                        _ => {}
                    }
                }
            }
            (Some(Feature::Hole { x, y, diameter }), Handle::Radius) => {
                let r = (to[0] - get(x)).hypot(to[1] - get(y));
                *diameter = number((2.0 * r).max(1.0));
            }
            (
                Some(Feature::Arc {
                    x,
                    y,
                    radius,
                    angle,
                    sweep,
                }),
                handle,
            ) => {
                let d = [to[0] - get(x), to[1] - get(y)];
                let towards = d[1].atan2(d[0]).to_degrees();
                match handle {
                    Handle::ArcApex if d[0].hypot(d[1]) >= 1.0 => {
                        *radius = number(d[0].hypot(d[1]));
                        *angle = number(towards);
                    }
                    Handle::ArcEnd => {
                        // Half the span is the turn from the middle of the arc to here.
                        let half = (towards - get(angle)).rem_euclid(360.0);
                        let half = if half > 180.0 { 360.0 - half } else { half };
                        *sweep = number((2.0 * half).round().clamp(1.0, 360.0));
                    }
                    _ => {}
                }
            }
            (
                Some(Feature::Slot {
                    x,
                    y,
                    length,
                    width,
                    vertical,
                }),
                Handle::SlotEnd(high),
            ) => {
                let center = if *vertical { y } else { x };
                let a = usize::from(*vertical);
                let (c, l, w) = (get(center), get(length), get(width));
                let (lo, hi) = if high {
                    (c - l / 2.0, to[a].max(c - l / 2.0 + w))
                } else {
                    (to[a].min(c + l / 2.0 - w), c + l / 2.0)
                };
                *center = number((lo + hi) / 2.0);
                *length = number(hi - lo);
            }
            (
                Some(Feature::Slot {
                    x,
                    y,
                    length,
                    width,
                    vertical,
                }),
                Handle::SlotWidth,
            ) => {
                let across = if *vertical {
                    to[0] - get(x)
                } else {
                    to[1] - get(y)
                };
                *width = number((2.0 * across.abs()).clamp(1.0, get(length).max(1.0)));
            }
            (Some(Feature::Polygon { points }), Handle::Vertex(n)) => {
                if let Some(v) = points.get_mut(n) {
                    v.x = number(to[0]);
                    v.y = number(to[1]);
                }
            }
            (
                Some(Feature::Notch {
                    edge,
                    pos,
                    width,
                    depth,
                    ..
                }),
                handle,
            ) => {
                // Along the edge in world terms: x for bottom and top, y for the sides.
                let a = *edge % 2;
                let (p, w) = (get(pos), get(width));
                match handle {
                    Handle::NotchStart => {
                        let start = to[a].min(p + w - 1.0);
                        *pos = number(start);
                        *width = number(p + w - start);
                    }
                    Handle::NotchEnd => *width = number((to[a] - p).max(1.0)),
                    Handle::NotchDepth => {
                        let d = match *edge {
                            0 => to[1],
                            1 => su - to[0],
                            2 => sv - to[1],
                            _ => to[0],
                        };
                        *depth = number(d.max(1.0));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Where the vertices of a polygon are.
    pub fn polygon_points(&self, feature: usize, eval: Eval) -> Option<Vec<Point>> {
        let Some(Feature::Polygon { points }) = self.features.get(feature) else {
            return None;
        };
        points
            .iter()
            .map(|v| Some([eval(&v.x).ok()?, eval(&v.y).ok()?]))
            .collect()
    }

    /// Makes the side of a polygon that leaves vertex `edge` measure `length`: the vertex
    /// at its end moves along the side, nothing else does. True if it did.
    pub fn set_edge_length(
        &mut self,
        feature: usize,
        edge: usize,
        length: f64,
        eval: Eval,
    ) -> bool {
        let Some(at) = self.polygon_points(feature, eval) else {
            return false;
        };
        if edge >= at.len() || length <= EPS {
            return false;
        }
        let end = (edge + 1) % at.len();
        let (a, b) = (at[edge], at[end]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        if len <= EPS {
            return false;
        }
        if let Some(Feature::Polygon { points }) = self.features.get_mut(feature) {
            points[end].x = number(a[0] + (b[0] - a[0]) / len * length);
            points[end].y = number(a[1] + (b[1] - a[1]) / len * length);
        }
        true
    }

    /// Adds a vertex to a polygon after vertex `after`. Returns its index.
    pub fn insert_vertex(&mut self, feature: usize, after: usize, at: Point) -> Option<usize> {
        let Some(Feature::Polygon { points }) = self.features.get_mut(feature) else {
            return None;
        };
        let n = (after + 1).min(points.len());
        points.insert(
            n,
            Vertex {
                x: number(at[0]),
                y: number(at[1]),
                radius: Dim::zero(),
            },
        );
        Some(n)
    }

    /// Removes a vertex of a polygon, which keeps at least three. True if it did.
    pub fn remove_vertex(&mut self, feature: usize, vertex: usize) -> bool {
        match self.features.get_mut(feature) {
            Some(Feature::Polygon { points }) if points.len() > 3 && vertex < points.len() => {
                points.remove(vertex);
                true
            }
            _ => false,
        }
    }

    /// Where the handle of a corner is: at the end of its treatment on the edge that
    /// leaves it (at the corner itself while it is square).
    pub fn corner_handle(&self, corner: usize, su: f64, sv: f64, eval: Eval) -> Point {
        let size = match &self.corners[corner % 4] {
            Corner::Square => 0.0,
            Corner::Round { radius: d } | Corner::Chamfer { size: d } => {
                eval(d).unwrap_or(0.0).max(0.0)
            }
        };
        let (origin, dir, len, _) = edge_frame(corner % 4, su, sv);
        edge_point(origin, dir, size.min(len), 0.0)
    }

    /// Sets the size of a corner from where its handle is dragged to. A square corner
    /// becomes round; dragged back to nothing it is square again.
    pub fn drag_corner(&mut self, corner: usize, to: Point, su: f64, sv: f64) {
        let (origin, dir, len, across) = edge_frame(corner % 4, su, sv);
        // Along either of the two edges that meet at the corner.
        let d = [to[0] - origin[0], to[1] - origin[1]];
        let s = (d[0] * dir[0] + d[1] * dir[1]).max(d[1] * dir[0] - d[0] * dir[1]);
        let s = s.clamp(0.0, len.min(across)).round();
        let c = &mut self.corners[corner % 4];
        *c = match c {
            _ if s < 1.0 => Corner::Square,
            Corner::Chamfer { .. } => Corner::Chamfer { size: number(s) },
            _ => Corner::Round { radius: number(s) },
        };
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
        self.rings().map(|r| ring_area(r)).sum()
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

    fn has(ring: &[Point], p: Point) -> bool {
        ring.iter()
            .any(|q| (q[0] - p[0]).abs() < 1e-3 && (q[1] - p[1]).abs() < 1e-3)
    }

    fn rect(x: f64, y: f64, w: f64, h: f64, r: f64) -> Feature {
        Feature::Rect {
            x: d(x),
            y: d(y),
            width: d(w),
            height: d(h),
            radius: d(r),
        }
    }

    fn hole(x: f64, y: f64, diameter: f64) -> Feature {
        Feature::Hole {
            x: d(x),
            y: d(y),
            diameter: d(diameter),
        }
    }

    fn polygon(points: &[(f64, f64, f64)]) -> Feature {
        Feature::Polygon {
            points: points
                .iter()
                .map(|&(x, y, r)| Vertex {
                    x: d(x),
                    y: d(y),
                    radius: d(r),
                })
                .collect(),
        }
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
        assert!(o.problems.is_empty(), "{:?}", o.problems);
        assert!((o.area() - (120_000.0 - 4.0 * 800.0)).abs() < 0.5);
        assert!(o.outer.len() == 20 && o.holes.is_empty());
        assert!(ring_area(&o.outer) > 0.0);
        // Positions count from the left/bottom on every edge.
        assert!(has(&o.outer, [100.0, 20.0]));
        assert!(has(&o.outer, [100.0, 280.0]));
        assert!(has(&o.outer, [380.0, 100.0]));
        assert!(has(&o.outer, [20.0, 100.0]));
    }

    #[test]
    fn holes_and_slots_are_triangulated_around() {
        let mut s = Shape::default();
        s.features.push(hole(100.0, 100.0, 40.0));
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
        assert!(o.holes.iter().all(|h| ring_area(h) < 0.0));
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
        let slot = o
            .holes
            .iter()
            .find(|h| point_in_ring(h, [250.0, 150.0]))
            .unwrap();
        let (min, max) = ring_bounds(slot);
        assert!((max[1] - min[1] - 100.0).abs() < 1e-3);
    }

    #[test]
    fn a_rectangle_inside_is_a_cutout_and_across_the_border_a_bite() {
        let mut s = Shape::default();
        s.features.push(rect(100.0, 100.0, 80.0, 50.0, 0.0));
        s.features.push(rect(350.0, 200.0, 100.0, 40.0, 0.0));
        let o = s.outline(400.0, 300.0, &eval);
        assert!(o.problems.is_empty());
        assert_eq!(o.holes.len(), 1);
        assert!((o.area() - (120_000.0 - 4000.0 - 2000.0)).abs() < 0.5);
        assert!(has(&o.outer, [350.0, 200.0]) && has(&o.outer, [350.0, 240.0]));
        // The whole figure is kept for the editor, also what sticks out.
        assert_eq!(o.cutters.len(), 2);
        assert!(has(&o.cutters[1].1, [450.0, 240.0]));
    }

    #[test]
    fn a_concave_polygon_with_round_corners() {
        // An L: 200 × 200 with a 100 × 100 bite at its upper right.
        let l = [
            (50.0, 50.0),
            (250.0, 50.0),
            (250.0, 150.0),
            (150.0, 150.0),
            (150.0, 250.0),
            (50.0, 250.0),
        ];
        let sharp: Vec<_> = l.iter().map(|&(x, y)| (x, y, 0.0)).collect();
        let mut s = Shape::default();
        s.features.push(polygon(&sharp));
        let o = s.outline(400.0, 300.0, &eval);
        assert!(o.problems.is_empty());
        assert!((o.area() - (120_000.0 - 30_000.0)).abs() < 0.5);

        // Radius 20 everywhere: five convex corners lose material from the cut, the
        // concave one adds to it.
        let round: Vec<_> = l.iter().map(|&(x, y)| (x, y, 20.0)).collect();
        s.features[0] = polygon(&round);
        let o = s.outline(400.0, 300.0, &eval);
        assert!(o.problems.is_empty());
        let fillet = 400.0 - std::f64::consts::PI * 100.0;
        let cut = 30_000.0 - 5.0 * fillet + fillet;
        assert!((o.area() - (120_000.0 - cut)).abs() < 15.0, "{}", o.area());
        // The concave corner is rounded too: its vertex is no longer on the cutout.
        assert!(!has(&o.holes[0], [150.0, 150.0]));
        assert!(has(&o.holes[0], [150.0, 170.0]) && has(&o.holes[0], [170.0, 150.0]));
    }

    #[test]
    fn rounding_takes_at_most_half_a_side() {
        let square = [[0.0, 0.0], [40.0, 0.0], [40.0, 40.0], [0.0, 40.0]];
        let ring = rounded_ring(&square, &[100.0; 4]);
        // A circle of radius 20.
        assert!((ring_area(&ring) - std::f64::consts::PI * 400.0).abs() < 10.0);
        assert!(
            ring.iter()
                .all(|p| ((p[0] - 20.0).hypot(p[1] - 20.0) - 20.0).abs() < 1e-6)
        );
        // Without radius the points come back as they are.
        assert_eq!(rounded_ring(&square, &[0.0; 4]), square.to_vec());
    }

    #[test]
    fn a_fillet_alone_takes_the_whole_side() {
        // 100 × 50 with radius 50 at both ends of the top side: a half circle on top.
        let ring = rounded_ring(
            &[[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]],
            &[0.0, 0.0, 50.0, 50.0],
        );
        assert!((ring_area(&ring) - std::f64::consts::PI * 1250.0).abs() < 40.0);
        assert!(has(&ring, [50.0, 50.0]) && !has(&ring, [100.0, 50.0]));
        // Two that do not fit share the side in proportion to what they want.
        let ring = rounded_ring(
            &[[0.0, 0.0], [90.0, 0.0], [90.0, 200.0], [0.0, 200.0]],
            &[0.0, 0.0, 60.0, 30.0],
        );
        assert!(has(&ring, [30.0, 200.0]) && has(&ring, [90.0, 140.0]));
    }

    fn arc(x: f64, y: f64, radius: f64, angle: f64, sweep: f64) -> Feature {
        Feature::Arc {
            x: d(x),
            y: d(y),
            radius: d(radius),
            angle: d(angle),
            sweep: d(sweep),
        }
    }

    #[test]
    fn arcs_are_pieces_of_a_circle_closed_by_their_chord() {
        let half = std::f64::consts::PI * 2500.0 / 2.0;
        let mut s = Shape::default();
        // A half circle that bulges up.
        s.features.push(arc(200.0, 100.0, 50.0, 90.0, 180.0));
        let o = s.outline(400.0, 300.0, &eval);
        assert!(o.problems.is_empty());
        assert!((120_000.0 - o.area() - half).abs() < 40.0, "{}", o.area());
        let (min, max) = ring_bounds(&o.holes[0]);
        assert!((min[0] - 150.0).abs() < 1e-3 && (max[0] - 250.0).abs() < 1e-3);
        assert!((min[1] - 100.0).abs() < 1e-3 && (max[1] - 150.0).abs() < 1e-3);
        // Towards the right, a quarter; then the whole circle.
        s.features[0] = arc(200.0, 100.0, 50.0, 0.0, 90.0);
        let (min, max) = ring_bounds(&s.outline(400.0, 300.0, &eval).holes[0]);
        assert!((min[0] - 50.0 * 45f64.to_radians().cos() - 200.0).abs() < 1e-3);
        assert!((max[0] - 250.0).abs() < 1e-3);
        s.features[0] = arc(200.0, 100.0, 50.0, 0.0, 360.0);
        let o = s.outline(400.0, 300.0, &eval);
        assert!((120_000.0 - o.area() - 2.0 * half).abs() < 60.0);
        // What makes no sense is left out.
        s.features[0] = arc(200.0, 100.0, 50.0, 0.0, 400.0);
        assert_eq!(s.outline(400.0, 300.0, &eval).problems.len(), 1);

        // Its handles: the middle sets radius and direction, an end how much it spans.
        s.features[0] = arc(200.0, 100.0, 50.0, 90.0, 180.0);
        let handles = s.handles(0, 400.0, 300.0, &eval);
        assert_eq!(handles[0].0, Handle::ArcApex);
        assert!(has(&[handles[0].1], [200.0, 150.0]) && has(&[handles[1].1], [150.0, 100.0]));
        s.drag_handle(0, Handle::ArcApex, [260.0, 100.0], 400.0, 300.0, &eval);
        s.drag_handle(0, Handle::ArcEnd, [200.0, 130.0], 400.0, 300.0, &eval);
        assert_eq!(s.features[0], arc(200.0, 100.0, 60.0, 0.0, 180.0));
        s.drag_handle(0, Handle::ArcEnd, [230.0, 70.0], 400.0, 300.0, &eval);
        assert_eq!(s.features[0], arc(200.0, 100.0, 60.0, 0.0, 90.0));
        s.translate(0, [10.0, -5.0], &eval);
        assert_eq!(s.features[0], arc(210.0, 95.0, 60.0, 0.0, 90.0));
        let back: Shape = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn a_locked_side_reaches_the_lines_of_other_points() {
        let targets = [vec![100.0, 300.0], vec![50.0]];
        // Straight down from (102, 200): close to y = 50.
        let hit = snap_along([102.0, 200.0], [0.0, -1.0], 146.0, &targets, 6.0);
        assert_eq!(hit, Some((150.0, 1, 50.0)));
        // At 30°: where it crosses x = 300.
        let dir = [30f64.to_radians().cos(), 0.5];
        let (len, axis, line) = snap_along([100.0, 0.0], dir, 228.0, &targets, 6.0).unwrap();
        assert!((len - 200.0 / dir[0]).abs() < 1e-9 && axis == 0 && line == 300.0);
        // Nothing close, and never backwards.
        assert_eq!(
            snap_along([102.0, 200.0], [0.0, -1.0], 100.0, &targets, 6.0),
            None
        );
        assert_eq!(
            snap_along([102.0, 200.0], [0.0, 1.0], 3.0, &targets, 200.0),
            None
        );
    }

    #[test]
    fn the_length_of_a_side_moves_only_its_end() {
        let mut s = Shape::default();
        s.features.push(polygon(&[
            (0.0, 0.0, 0.0),
            (30.0, 40.0, 5.0),
            (100.0, 40.0, 0.0),
            (100.0, 0.0, 0.0),
        ]));
        assert!(s.set_edge_length(0, 0, 100.0, &eval));
        // The closing side: its end is the first vertex.
        assert!(s.set_edge_length(0, 3, 80.0, &eval));
        assert_eq!(
            s.features[0],
            polygon(&[
                (20.0, 0.0, 0.0),
                (60.0, 80.0, 5.0),
                (100.0, 40.0, 0.0),
                (100.0, 0.0, 0.0),
            ])
        );
        assert!(!s.set_edge_length(0, 9, 80.0, &eval));
        assert!(!s.set_edge_length(0, 0, 0.0, &eval));
    }

    #[test]
    fn figures_that_touch_merge() {
        let mut s = Shape::default();
        s.features.push(hole(200.0, 150.0, 40.0));
        s.features.push(hole(220.0, 150.0, 40.0));
        let o = s.outline(400.0, 300.0, &eval);
        assert!(o.problems.is_empty());
        assert_eq!(o.holes.len(), 1);
        let (min, max) = ring_bounds(&o.holes[0]);
        assert!((max[0] - min[0] - 60.0).abs() < 1e-3);
    }

    #[test]
    fn a_cut_across_the_part_splits_it() {
        let mut s = Shape::default();
        s.features.push(rect(100.0, -10.0, 20.0, 320.0, 0.0));
        let o = s.outline(400.0, 300.0, &eval);
        assert_eq!(
            o.problems,
            vec![Problem {
                item: Item::Whole,
                kind: ProblemKind::Splits
            }]
        );
        // The largest piece stays: the one at the right.
        assert!((o.area() - 280.0 * 300.0).abs() < 0.5);
    }

    #[test]
    fn what_is_wrong_is_reported_and_skipped() {
        let mut s = Shape::default();
        s.corners[0] = Corner::Round { radius: d(250.0) };
        s.corners[1] = Corner::Round { radius: d(200.0) };
        s.features = vec![
            // Outside the part.
            hole(500.0, 100.0, 20.0),
            hole(200.0, 150.0, 20.0),
            Feature::Hole {
                x: Dim("oops".into()),
                y: d(150.0),
                diameter: d(20.0),
            },
            // A bow tie.
            polygon(&[
                (50.0, 50.0, 0.0),
                (150.0, 150.0, 0.0),
                (150.0, 50.0, 0.0),
                (50.0, 150.0, 0.0),
            ]),
            rect(10.0, 10.0, 0.0, 20.0, 0.0),
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
                (Item::Corner(0), ProblemKind::OutOfBounds),
                (Item::Feature(0), ProblemKind::OutOfBounds),
                (Item::Feature(2), ProblemKind::BadDimension),
                (Item::Feature(3), ProblemKind::SelfIntersecting),
                (Item::Feature(4), ProblemKind::BadDimension),
            ]
        );
        assert_eq!(o.holes.len(), 1);
        // What is off the part keeps its figure, to be dragged back.
        assert_eq!(o.cutters.iter().map(|c| c.0).collect::<Vec<_>>(), [0, 1]);
    }

    #[test]
    fn moving_a_figure_keeps_what_it_does_not_touch() {
        let mut s = Shape::default();
        s.features.push(Feature::Rect {
            x: Dim("10*2".into()),
            y: Dim("30+0".into()),
            width: d(50.0),
            height: d(40.0),
            radius: d(0.0),
        });
        s.features.push(polygon(&[
            (0.0, 0.0, 0.0),
            (10.0, 0.0, 5.0),
            (0.0, 10.0, 0.0),
        ]));
        s.features.push(Feature::Notch {
            edge: 1,
            pos: d(100.0),
            width: d(40.0),
            depth: d(20.0),
            radius: d(0.0),
        });
        s.translate(0, [12.34, 0.0], &eval);
        s.translate(1, [5.0, -5.0], &eval);
        s.translate(2, [30.0, 7.0], &eval);
        // Only x moved: y keeps its formula.
        assert_eq!(
            s.features[0],
            Feature::Rect {
                x: d(32.3),
                y: Dim("30+0".into()),
                width: d(50.0),
                height: d(40.0),
                radius: d(0.0),
            }
        );
        assert_eq!(
            s.features[1],
            polygon(&[(5.0, -5.0, 0.0), (15.0, -5.0, 5.0), (5.0, 5.0, 0.0)])
        );
        // A notch on the right edge slides along y only.
        assert!(matches!(&s.features[2], Feature::Notch { pos, .. } if *pos == d(107.0)));
    }

    #[test]
    fn handles_resize_the_figures() {
        let (su, sv) = (400.0, 300.0);
        let mut s = Shape {
            features: vec![
                rect(100.0, 100.0, 80.0, 50.0, 0.0),
                hole(200.0, 150.0, 40.0),
                Feature::Slot {
                    x: d(250.0),
                    y: d(150.0),
                    length: d(100.0),
                    width: d(20.0),
                    vertical: false,
                },
                polygon(&[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (0.0, 10.0, 0.0)]),
                Feature::Notch {
                    edge: 2,
                    pos: d(100.0),
                    width: d(40.0),
                    depth: d(20.0),
                    radius: d(0.0),
                },
            ],
            ..Shape::default()
        };
        let at = |s: &Shape, f: usize, h: Handle| {
            s.handles(f, su, sv, &eval)
                .into_iter()
                .find(|(k, _)| *k == h)
                .map(|(_, p)| p)
                .unwrap()
        };
        assert_eq!(at(&s, 0, Handle::Rect([-1, -1])), [100.0, 100.0]);
        assert_eq!(at(&s, 0, Handle::Rect([1, 0])), [180.0, 125.0]);
        s.drag_handle(0, Handle::Rect([-1, 1]), [90.0, 170.0], su, sv, &eval);
        assert_eq!(s.features[0], rect(90.0, 100.0, 90.0, 70.0, 0.0));
        // A side never crosses the opposite one.
        s.drag_handle(0, Handle::Rect([1, 0]), [0.0, 0.0], su, sv, &eval);
        assert_eq!(s.features[0], rect(90.0, 100.0, 1.0, 70.0, 0.0));

        assert_eq!(at(&s, 1, Handle::Radius), [220.0, 150.0]);
        s.drag_handle(1, Handle::Radius, [200.0, 180.0], su, sv, &eval);
        assert_eq!(s.features[1], hole(200.0, 150.0, 60.0));

        assert_eq!(at(&s, 2, Handle::SlotEnd(true)), [300.0, 150.0]);
        s.drag_handle(2, Handle::SlotEnd(true), [320.0, 0.0], su, sv, &eval);
        s.drag_handle(2, Handle::SlotWidth, [0.0, 165.0], su, sv, &eval);
        assert_eq!(
            s.features[2],
            Feature::Slot {
                x: d(260.0),
                y: d(150.0),
                length: d(120.0),
                width: d(30.0),
                vertical: false,
            }
        );

        assert_eq!(at(&s, 3, Handle::Midpoint(0)), [5.0, 0.0]);
        assert_eq!(s.insert_vertex(3, 0, [5.0, -5.0]), Some(1));
        s.drag_handle(3, Handle::Vertex(1), [6.0, -8.0], su, sv, &eval);
        assert_eq!(
            s.features[3],
            polygon(&[
                (0.0, 0.0, 0.0),
                (6.0, -8.0, 0.0),
                (10.0, 0.0, 0.0),
                (0.0, 10.0, 0.0)
            ])
        );
        assert!(s.remove_vertex(3, 1));
        assert!(!s.remove_vertex(3, 1));

        // A notch on the top edge: from x = 100 to 140, 20 deep.
        assert_eq!(at(&s, 4, Handle::NotchStart), [100.0, 290.0]);
        assert_eq!(at(&s, 4, Handle::NotchEnd), [140.0, 290.0]);
        assert_eq!(at(&s, 4, Handle::NotchDepth), [120.0, 280.0]);
        s.drag_handle(4, Handle::NotchStart, [90.0, 0.0], su, sv, &eval);
        s.drag_handle(4, Handle::NotchDepth, [0.0, 250.0], su, sv, &eval);
        assert!(matches!(
            &s.features[4],
            Feature::Notch { pos, width, depth, .. }
                if *pos == d(90.0) && *width == d(50.0) && *depth == d(50.0)
        ));
    }

    #[test]
    fn corners_are_dragged_from_square_and_back() {
        let (su, sv) = (400.0, 300.0);
        let mut s = Shape::default();
        assert_eq!(s.corner_handle(1, su, sv, &eval), [400.0, 0.0]);
        s.drag_corner(1, [395.0, 30.2], su, sv);
        assert_eq!(s.corners[1], Corner::Round { radius: d(30.0) });
        assert_eq!(s.corner_handle(1, su, sv, &eval), [400.0, 30.0]);
        s.corners[1] = Corner::Chamfer { size: d(30.0) };
        s.drag_corner(1, [400.0, 45.0], su, sv);
        assert_eq!(s.corners[1], Corner::Chamfer { size: d(45.0) });
        // Along the other edge of the corner too.
        s.drag_corner(1, [340.0, 10.0], su, sv);
        assert_eq!(s.corners[1], Corner::Chamfer { size: d(60.0) });
        s.drag_corner(1, [400.0, -3.0], su, sv);
        assert_eq!(s.corners[1], Corner::Square);
    }

    #[test]
    fn lines_lock_to_steps_of_an_angle() {
        let near = |a: Point, b: Point| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        // Almost horizontal: flat, and as long as it reaches across.
        assert!(near(
            snap_angle([10.0, 10.0], [110.4, 14.0], 15.0),
            [110.0, 10.0]
        ));
        // Close to 45°.
        let p = snap_angle([0.0, 0.0], [70.0, 72.0], 15.0);
        assert!((p[0] - p[1]).abs() < 1e-9 && (p[0].hypot(p[1]) - 100.0).abs() < 1e-9);
        // 30° downwards to the left.
        let p = snap_angle([0.0, 0.0], [-87.0, -49.0], 15.0);
        assert!(near(p, [-100.0 * 30f64.to_radians().cos(), -50.0]));
    }

    #[test]
    fn guides_cut_nothing_and_are_kept() {
        let mut s = Shape::default();
        s.guides.push(Guide {
            axis: 0,
            at: d(100.0),
        });
        assert!(s.is_plain() && !s.is_empty());
        assert_eq!(s.outline(400.0, 300.0, &eval), Outline::rect(400.0, 300.0));
        assert_eq!(s.dims_mut().len(), 1);
        let back: Shape = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn snapping_and_picking() {
        assert_eq!(snap(98.0, &[0.0, 100.0, 103.0], 5.0), Some(100.0));
        assert_eq!(snap(50.0, &[0.0, 100.0], 5.0), None);
        let ring = [
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [5.0, 5.0],
            [0.0, 10.0],
        ];
        assert!(point_in_ring(&ring, [2.0, 5.0]));
        assert!(!point_in_ring(&ring, [5.0, 8.0]));
    }

    #[test]
    fn files_from_before_the_figures_still_load() {
        let old = r#"{
            "corners": [{"type": "square"}, {"type": "round", "radius": "20"},
                        {"type": "square"}, {"type": "chamfer", "size": "T"}],
            "features": [
                {"type": "notch", "edge": 0, "pos": "100", "width": "40", "depth": "20",
                 "radius": "0"},
                {"type": "slot", "x": "250", "y": "150", "length": "100", "width": "20",
                 "vertical": true},
                {"type": "hole", "x": "100", "y": "100", "diameter": "40"}
            ]
        }"#;
        let s: Shape = serde_json::from_str(old).unwrap();
        assert_eq!(s.features.len(), 3);
        assert_eq!(s.corners[1], Corner::Round { radius: d(20.0) });
        // A vertex may come without its radius.
        let v: Vertex = serde_json::from_str(r#"{"x": "1", "y": "2"}"#).unwrap();
        assert_eq!(v.radius, Dim::zero());
    }
}
