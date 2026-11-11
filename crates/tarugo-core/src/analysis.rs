//! Checks of a design: parts that run into each other, joints without enough screws and
//! screws a drill cannot reach.

use std::collections::HashMap;

use crate::EPS;
use crate::joints::{Joint, Overlap, find_joints, find_overlaps, other_axes};
use crate::model::{Document, PartId, Screw, ScrewId};
use crate::solve::{Aabb, Solved};

/// A screw does not go through anything thicker than this (mm): the joint between two
/// boards edge to edge, or face to face of thick lumber, is not a screwed one.
pub const MAX_THROUGH: f64 = 60.0;
/// Part of a joint that must be reachable to call it accessible.
const FREE_FRACTION: f64 = 0.5;

/// A screw placed in space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrewPlace {
    pub id: ScrewId,
    /// Point on the contact plane.
    pub pos: [f64; 3],
    pub axis: usize,
    /// +1 or -1: the way it travels along the axis, from the head to the tip.
    pub dir: f64,
    /// Where the head is: on the outer face of the part it goes through.
    pub head: [f64; 3],
    /// Index of its joint in the list given.
    pub joint: usize,
}

/// Where each screw is. Screws whose parts no longer touch are left out (see
/// [`Issue::OrphanScrew`]).
pub fn screw_places(doc: &Document, solved: &Solved, joints: &[Joint]) -> Vec<ScrewPlace> {
    doc.screws
        .iter()
        .filter_map(|s| place(s, solved, joints))
        .collect()
}

fn place(screw: &Screw, solved: &Solved, joints: &[Joint]) -> Option<ScrewPlace> {
    let (index, joint) = joints
        .iter()
        .enumerate()
        .find(|(_, j)| j.joins(screw.through, screw.into))?;
    let through = solved.get(screw.through)?;
    let pos = joint.point(screw.u, screw.v);
    let dir = if joint.a == screw.through { 1.0 } else { -1.0 };
    let mut head = pos;
    head[joint.axis] = if dir > 0.0 {
        through.min[joint.axis]
    } else {
        through.max[joint.axis]
    };
    Some(ScrewPlace {
        id: screw.id,
        pos,
        axis: joint.axis,
        dir,
        head,
        joint: index,
    })
}

/// Which of the two parts of a joint a screw would go through by default: the thinner one
/// along the joint's axis, if it is thin enough.
pub fn default_through(joint: &Joint, solved: &Solved) -> Option<PartId> {
    let thickness = |id: PartId| solved.get(id).map(|b| b.size()[joint.axis]);
    let (ta, tb) = (thickness(joint.a)?, thickness(joint.b)?);
    let (part, t) = if ta <= tb {
        (joint.a, ta)
    } else {
        (joint.b, tb)
    };
    (t <= MAX_THROUGH).then_some(part)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Issue {
    /// The part cannot be computed (bad formula, lost reference, loop).
    Broken(PartId),
    /// An end before its start: the part has a negative length.
    Inverted(PartId),
    /// Two parts share volume.
    Overlap(Overlap),
    /// A free-form feature that does not fit.
    Shape(PartId),
    /// The part touches no other.
    Floating(PartId),
    /// The part has fewer than two screws in all: it can come loose or rotate.
    FewScrews { part: PartId, screws: usize },
    /// A joint that could be screwed and has no screws.
    NoScrews { a: PartId, b: PartId },
    /// A joint held by one screw only.
    SingleScrew { a: PartId, b: PartId },
    /// The drill cannot get there: another part covers where the screws would go in.
    Blocked {
        a: PartId,
        b: PartId,
        blocker: PartId,
    },
    /// There is less room than the drill needs.
    Tight { a: PartId, b: PartId, room: f64 },
    /// A screw whose parts no longer touch.
    OrphanScrew(ScrewId),
}

impl Issue {
    /// Parts to highlight for this issue.
    pub fn parts(&self) -> Vec<PartId> {
        match self {
            Issue::Broken(p)
            | Issue::Inverted(p)
            | Issue::Shape(p)
            | Issue::Floating(p)
            | Issue::FewScrews { part: p, .. } => vec![*p],
            Issue::Overlap(o) => vec![o.a, o.b],
            Issue::NoScrews { a, b } | Issue::SingleScrew { a, b } | Issue::Tight { a, b, .. } => {
                vec![*a, *b]
            }
            Issue::Blocked { a, b, blocker } => vec![*a, *b, *blocker],
            Issue::OrphanScrew(_) => Vec::new(),
        }
    }

    /// Does it break the design (not just a recommendation)?
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            Issue::Broken(_)
                | Issue::Inverted(_)
                | Issue::Overlap(_)
                | Issue::Floating(_)
                | Issue::FewScrews { .. }
                | Issue::Blocked { .. }
        )
    }
}

/// Problems of the geometry itself; cheap enough to run on every change.
pub fn check_geometry(doc: &Document, solved: &Solved) -> Vec<Issue> {
    let mut out = Vec::new();
    for part in &doc.parts {
        if solved.errors.contains_key(&part.id) {
            out.push(Issue::Broken(part.id));
        } else if solved.inverted.contains(&part.id) {
            out.push(Issue::Inverted(part.id));
        }
        if let (Some(shape), Some(b)) = (&part.shape, solved.get(part.id))
            && let Some((su, sv)) = face_size(part, b)
            && !shape
                .outline(su, sv, &|d| solved.eval(d))
                .problems
                .is_empty()
        {
            out.push(Issue::Shape(part.id));
        }
    }
    out.extend(find_overlaps(doc, solved).into_iter().map(Issue::Overlap));
    out
}

/// Size of the face of a part: its extent on the two axes other than its normal.
pub fn face_size(part: &crate::model::Part, b: &Aabb) -> Option<(f64, f64)> {
    let [i, j] = other_axes(part.normal_axis()?);
    let s = b.size();
    Some((s[i], s[j]))
}

/// How reachable the entry side of a joint is for a drill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Access {
    Free,
    /// Parts closer than the drill is long cover it; the closest leaves this room.
    Tight(f64),
    /// A part right against the entry face covers it.
    Blocked(PartId),
}

/// Rectangle on the two axes across a joint's axis.
#[derive(Clone, Copy)]
struct Rect {
    min: [f64; 2],
    max: [f64; 2],
}

/// Area of the union of rectangles (clipped already to the region of interest).
fn union_area(rects: &[Rect]) -> f64 {
    let mut xs: Vec<f64> = rects.iter().flat_map(|r| [r.min[0], r.max[0]]).collect();
    let mut ys: Vec<f64> = rects.iter().flat_map(|r| [r.min[1], r.max[1]]).collect();
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    let mut area = 0.0;
    for x in xs.windows(2) {
        for y in ys.windows(2) {
            let (cx, cy) = ((x[0] + x[1]) / 2.0, (y[0] + y[1]) / 2.0);
            let covered = rects
                .iter()
                .any(|r| cx > r.min[0] && cx < r.max[0] && cy > r.min[1] && cy < r.max[1]);
            if covered {
                area += (x[1] - x[0]) * (y[1] - y[0]);
            }
        }
    }
    area
}

/// Access for screws that go through `through` at a joint, over the rectangle `region` of
/// the joint (all of it, or the surroundings of one screw).
fn access(
    doc: &Document,
    solved: &Solved,
    joint: &Joint,
    through: PartId,
    region: Rect,
    drill: f64,
) -> Access {
    let Some(tb) = solved.get(through) else {
        return Access::Free;
    };
    let axis = joint.axis;
    let [i, j] = other_axes(axis);
    // The drill comes from the low side if the part it goes through is the low one.
    let from_low = joint.a == through;
    let entry = if from_low { tb.min[axis] } else { tb.max[axis] };
    let region_area = (region.max[0] - region.min[0]) * (region.max[1] - region.min[1]);
    if region_area <= 0.0 {
        return Access::Free;
    }
    let mut touching = Vec::new();
    let mut near = Vec::new();
    let mut room = f64::MAX;
    let mut blocker = None;
    for part in &doc.parts {
        if joint.has(part.id) {
            continue;
        }
        let Some(b) = solved.get(part.id) else {
            continue;
        };
        let clip = Rect {
            min: [b.min[i].max(region.min[0]), b.min[j].max(region.min[1])],
            max: [b.max[i].min(region.max[0]), b.max[j].min(region.max[1])],
        };
        if clip.max[0] - clip.min[0] <= EPS || clip.max[1] - clip.min[1] <= EPS {
            continue;
        }
        // Distance from the entry face out to the part.
        let gap = if from_low {
            if b.max[axis] > entry + EPS {
                continue;
            }
            entry - b.max[axis]
        } else {
            if b.min[axis] < entry - EPS {
                continue;
            }
            b.min[axis] - entry
        };
        if gap >= drill {
            continue;
        }
        near.push(clip);
        room = room.min(gap.max(0.0));
        if gap <= EPS {
            touching.push(clip);
            blocker.get_or_insert(part.id);
        }
    }
    let limit = region_area * (1.0 - FREE_FRACTION);
    if let Some(blocker) = blocker
        && union_area(&touching) > limit
    {
        return Access::Blocked(blocker);
    }
    if union_area(&near) > limit {
        return Access::Tight(room);
    }
    Access::Free
}

fn whole(joint: &Joint) -> Rect {
    Rect {
        min: joint.min,
        max: joint.max,
    }
}

/// Parts of a joint a screw could go through.
fn screwable(joint: &Joint, solved: &Solved) -> Vec<PartId> {
    [joint.a, joint.b]
        .into_iter()
        .filter(|id| {
            solved
                .get(*id)
                .is_some_and(|b| b.size()[joint.axis] <= MAX_THROUGH)
        })
        .collect()
}

/// The better of two access results.
fn best(a: Access, b: Access) -> Access {
    match (a, b) {
        (Access::Free, _) | (_, Access::Free) => Access::Free,
        (Access::Tight(x), Access::Tight(y)) => Access::Tight(x.max(y)),
        (Access::Tight(x), _) | (_, Access::Tight(x)) => Access::Tight(x),
        (blocked, _) => blocked,
    }
}

/// The worse of two access results.
fn worst(a: Access, b: Access) -> Access {
    match (a, b) {
        (Access::Blocked(p), _) | (_, Access::Blocked(p)) => Access::Blocked(p),
        (Access::Tight(x), Access::Tight(y)) => Access::Tight(x.min(y)),
        (Access::Tight(x), _) | (_, Access::Tight(x)) => Access::Tight(x),
        _ => Access::Free,
    }
}

/// Access of a joint that has no screws yet: the best way in.
pub fn joint_access(doc: &Document, solved: &Solved, joint: &Joint) -> Option<Access> {
    let drill = doc.settings.drill_length;
    screwable(joint, solved)
        .into_iter()
        .map(|through| access(doc, solved, joint, through, whole(joint), drill))
        .reduce(best)
}

/// Checks that every part is held by screws, that joints have at least two, and that a
/// drill can reach them.
pub fn check_fastening(doc: &Document, solved: &Solved) -> Vec<Issue> {
    let joints = find_joints(doc, solved);
    let places = screw_places(doc, solved, &joints);
    let mut out = Vec::new();

    let mut per_joint = vec![0usize; joints.len()];
    let mut per_part: HashMap<PartId, usize> = HashMap::new();
    let by_id: HashMap<ScrewId, &Screw> = doc.screws.iter().map(|s| (s.id, s)).collect();
    for p in &places {
        per_joint[p.joint] += 1;
        for part in [joints[p.joint].a, joints[p.joint].b] {
            *per_part.entry(part).or_default() += 1;
        }
    }
    for screw in &doc.screws {
        if !places.iter().any(|p| p.id == screw.id) {
            out.push(Issue::OrphanScrew(screw.id));
        }
    }

    for part in &doc.parts {
        if solved.get(part.id).is_none() {
            continue;
        }
        if !joints.iter().any(|j| j.has(part.id)) {
            // A single part is not a piece of furniture to hold together.
            if doc.parts.len() > 1 {
                out.push(Issue::Floating(part.id));
            }
            continue;
        }
        let screws = per_part.get(&part.id).copied().unwrap_or(0);
        if screws < 2 {
            out.push(Issue::FewScrews {
                part: part.id,
                screws,
            });
        }
    }

    let drill = doc.settings.drill_length;
    for (n, joint) in joints.iter().enumerate() {
        let (a, b) = (joint.a, joint.b);
        if screwable(joint, solved).is_empty() {
            continue;
        }
        let reach = if per_joint[n] == 0 {
            out.push(Issue::NoScrews { a, b });
            joint_access(doc, solved, joint)
        } else {
            if per_joint[n] == 1 {
                out.push(Issue::SingleScrew { a, b });
            }
            // Each screw where it is: a square as wide as the joint is thin around it.
            let [i, j] = other_axes(joint.axis);
            let half = (joint.size()[0].min(joint.size()[1]) / 2.0).max(1.0);
            places
                .iter()
                .filter(|p| p.joint == n)
                .filter_map(|p| {
                    let through = by_id.get(&p.id)?.through;
                    let region = Rect {
                        min: [
                            (p.pos[i] - half).max(joint.min[0]),
                            (p.pos[j] - half).max(joint.min[1]),
                        ],
                        max: [
                            (p.pos[i] + half).min(joint.max[0]),
                            (p.pos[j] + half).min(joint.max[1]),
                        ],
                    };
                    Some(access(doc, solved, joint, through, region, drill))
                })
                .reduce(worst)
        };
        match reach {
            Some(Access::Blocked(blocker)) => out.push(Issue::Blocked { a, b, blocker }),
            Some(Access::Tight(room)) => out.push(Issue::Tight { a, b, room }),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::front::{Cell, Cota, Frame, divide, find_cell, new_frame};
    use crate::model::{Dim, Side};
    use crate::solve::solve;

    fn cota(distance: f64) -> Cota {
        Cota {
            from_hi: false,
            ref_side: Side::Max,
            align: Side::Min,
            distance: Dim::num(distance),
        }
    }

    fn cell(doc: &Document, x: f64, y: f64) -> Cell {
        find_cell(doc, &solve(doc), x, y).unwrap()
    }

    /// 800 × 600 × 300 frame.
    fn framed() -> (Document, Frame) {
        let mut doc = Document::new();
        let frame = new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        (doc, frame)
    }

    fn add_screws(doc: &mut Document, through: PartId, into: PartId, n: usize) {
        for k in 0..n {
            let id = doc.new_id();
            doc.screws.push(Screw {
                id,
                through,
                into,
                u: 0.5,
                v: (k as f64 + 1.0) / (n as f64 + 1.0),
            });
        }
    }

    fn screw_frame(doc: &mut Document, f: &Frame, n: usize) {
        for side in [f.left, f.right] {
            for board in [f.bottom, f.top] {
                add_screws(doc, side, board, n);
            }
        }
    }

    #[test]
    fn screws_are_placed_through_the_thin_part() {
        let (mut doc, f) = framed();
        add_screws(&mut doc, f.left, f.top, 2);
        let s = solve(&doc);
        let joints = find_joints(&doc, &s);
        let places = screw_places(&doc, &s, &joints);
        assert_eq!(places.len(), 2);
        let p = places[0];
        assert_eq!((p.axis, p.dir), (0, 1.0));
        assert_eq!(p.pos, [18.0, 591.0, 100.0]);
        assert_eq!(p.head, [0.0, 591.0, 100.0]);
        let j = joints.iter().find(|j| j.joins(f.left, f.top)).unwrap();
        assert_eq!(default_through(j, &s), Some(f.left));
    }

    #[test]
    fn counts_screws_per_joint_and_part() {
        let (mut doc, f) = framed();
        let s = solve(&doc);
        let issues = check_fastening(&doc, &s);
        assert_eq!(
            issues
                .iter()
                .filter(|i| matches!(i, Issue::NoScrews { .. }))
                .count(),
            4
        );
        assert_eq!(
            issues
                .iter()
                .filter(|i| matches!(i, Issue::FewScrews { screws: 0, .. }))
                .count(),
            4
        );

        screw_frame(&mut doc, &f, 1);
        let issues = check_fastening(&doc, &s);
        assert_eq!(
            issues
                .iter()
                .filter(|i| matches!(i, Issue::SingleScrew { .. }))
                .count(),
            4
        );
        // One screw at each end: two per part in all.
        assert!(!issues.iter().any(|i| matches!(i, Issue::FewScrews { .. })));

        doc.screws.clear();
        screw_frame(&mut doc, &f, 2);
        assert_eq!(check_fastening(&doc, &s), Vec::new());
    }

    #[test]
    fn a_cross_blocks_the_drill() {
        let (mut doc, f) = framed();
        screw_frame(&mut doc, &f, 2);
        // A vertical division in the middle and a shelf at each side, at the same height.
        let c = cell(&doc, 400.0, 300.0);
        let division = divide(&mut doc, &c, 0, &cota(373.0), "division");
        let c = cell(&doc, 200.0, 300.0);
        let left_shelf = divide(&mut doc, &c, 1, &cota(250.0), "left shelf");
        let c = cell(&doc, 600.0, 300.0);
        let right_shelf = divide(&mut doc, &c, 1, &cota(250.0), "right shelf");
        add_screws(&mut doc, f.bottom, division, 2);
        add_screws(&mut doc, f.top, division, 2);
        add_screws(&mut doc, f.left, left_shelf, 2);
        add_screws(&mut doc, f.right, right_shelf, 2);

        let s = solve(&doc);
        let issues = check_fastening(&doc, &s);
        // Both shelves meet the division right opposite each other: neither can be
        // screwed through it.
        let blocked: Vec<_> = issues
            .iter()
            .filter_map(|i| match i {
                Issue::Blocked { a, b, blocker } => Some((*a, *b, *blocker)),
                _ => None,
            })
            .collect();
        assert_eq!(
            blocked,
            vec![
                (left_shelf, division, right_shelf),
                (division, right_shelf, left_shelf)
            ]
        );

        // Staggered shelves leave the way free... but the cells are wide enough only
        // with a short drill.
        let s0 = solve(&doc);
        crate::edit::translate(&mut doc, &s0, right_shelf, 1, 100.0, None, true);
        let s = solve(&doc);
        let issues = check_fastening(&doc, &s);
        assert!(!issues.iter().any(|i| matches!(i, Issue::Blocked { .. })));
        assert!(!issues.iter().any(|i| matches!(i, Issue::Tight { .. })));
    }

    #[test]
    fn narrow_cells_leave_no_room_for_the_drill() {
        let (mut doc, f) = framed();
        // Two divisions 100 apart, and a shelf from the first one back to the left side.
        let c = cell(&doc, 400.0, 300.0);
        let first = divide(&mut doc, &c, 0, &cota(300.0), "first");
        let c = cell(&doc, 600.0, 300.0);
        let _second = divide(&mut doc, &c, 0, &cota(100.0), "second");
        let c = cell(&doc, 100.0, 300.0);
        let shelf = divide(&mut doc, &c, 1, &cota(250.0), "shelf");
        let s = solve(&doc);
        let issues = check_fastening(&doc, &s);
        // Through `first` the drill has only the 100 mm cell; no screws there yet.
        assert!(issues.contains(&Issue::Tight {
            a: shelf,
            b: first,
            room: 100.0
        }));
        // Through the left side, from outside, it is free.
        assert!(
            !issues
                .iter()
                .any(|i| matches!(i, Issue::Tight { a, b, .. } if (*a, *b) == (f.left, shelf)))
        );
        // With a stubby drill the narrow cell is enough.
        doc.settings.drill_length = 90.0;
        assert!(
            !check_fastening(&doc, &s)
                .iter()
                .any(|i| matches!(i, Issue::Tight { .. }))
        );
    }

    #[test]
    fn floating_parts_and_orphan_screws() {
        let (mut doc, f) = framed();
        screw_frame(&mut doc, &f, 2);
        let s = solve(&doc);
        // Pull the top 50 up: it touches nothing and its screws are left hanging.
        let _ = s;
        doc.part_mut(f.top).unwrap().axes[1] = crate::model::Extent::Thick {
            anchor: crate::model::End::abs(650.0),
            align: Side::Max,
        };
        let s = solve(&doc);
        let issues = check_fastening(&doc, &s);
        assert!(issues.contains(&Issue::Floating(f.top)));
        assert_eq!(
            issues
                .iter()
                .filter(|i| matches!(i, Issue::OrphanScrew(_)))
                .count(),
            4
        );
    }

    #[test]
    fn geometry_issues() {
        let (mut doc, f) = framed();
        let s = solve(&doc);
        assert!(check_geometry(&doc, &s).is_empty());
        // The top pushed down into... still between the sides; move it sideways instead.
        crate::edit::translate(&mut doc, &s, f.top, 0, -10.0, None, true);
        let s = solve(&doc);
        let issues = check_geometry(&doc, &s);
        assert!(matches!(issues.as_slice(), [Issue::Overlap(o)] if o.a == f.left && o.b == f.top));
    }

    #[test]
    fn union_area_counts_shared_parts_once() {
        let r = |x0: f64, y0: f64, x1: f64, y1: f64| Rect {
            min: [x0, y0],
            max: [x1, y1],
        };
        assert_eq!(
            union_area(&[r(0.0, 0.0, 10.0, 10.0), r(5.0, 5.0, 15.0, 15.0)]),
            175.0
        );
        assert_eq!(union_area(&[]), 0.0);
    }
}
