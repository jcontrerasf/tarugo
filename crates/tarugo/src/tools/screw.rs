//! Placing screws on joints.

use tarugo_core::analysis::{self, Access, MAX_THROUGH};
use tarugo_core::joints::Joint;
use tarugo_core::model::{Document, PartId, Screw, Side};

use crate::scene::Scene;
use crate::view3d::Hit;

/// Distance from the ends of a joint to its first and last screws (mm), at most.
const END_MARGIN: f64 = 50.0;

/// The joint a screw driven in through the clicked face would reach: the one on the
/// opposite face of that part, behind the point clicked. Returns its index.
pub fn joint_behind(scene: &Scene, hit: &Hit) -> Option<usize> {
    let b = scene.get(hit.part)?;
    if b.size()[hit.axis] > MAX_THROUGH {
        return None;
    }
    let point = [hit.point.x as f64, hit.point.y as f64, hit.point.z as f64];
    scene.joints.iter().position(|j| {
        let behind = match hit.side {
            Side::Max => j.b == hit.part,
            _ => j.a == hit.part,
        };
        j.axis == hit.axis && behind && j.contains(point)
    })
}

/// Fractions of the joint where `count` screws go: one where clicked, or several spread
/// along the joint. Across a narrow joint they go in the middle.
pub fn positions(joint: &Joint, at: [f64; 3], count: usize) -> Vec<(f64, f64)> {
    let size = joint.size();
    let long = if size[0] >= size[1] { 0 } else { 1 };
    let (u, v) = joint.fractions(at);
    let mut clicked = [u, v];
    if size[1 - long] <= MAX_THROUGH {
        clicked[1 - long] = 0.5;
    }
    let pair = |along: f64| {
        let mut p = clicked;
        p[long] = along;
        (p[0], p[1])
    };
    if count <= 1 {
        // To the millimetre along the joint.
        let mm = (clicked[long] * size[long]).round();
        return vec![pair((mm / size[long]).clamp(0.0, 1.0))];
    }
    let margin = END_MARGIN.min(size[long] * 0.2) / size[long];
    (0..count)
        .map(|i| pair(margin + (1.0 - 2.0 * margin) * i as f64 / (count - 1) as f64))
        .collect()
}

pub fn add_screws(doc: &mut Document, through: PartId, into: PartId, at: &[(f64, f64)]) {
    for (u, v) in at {
        let id = doc.new_id();
        doc.screws.push(Screw {
            id,
            through,
            into,
            u: *u,
            v: *v,
        });
    }
}

/// Puts `count` screws in every joint that has none and a drill can reach. Returns how
/// many joints got screws.
pub fn fill_all(doc: &mut Document, scene: &Scene, count: usize) -> usize {
    let mut filled = 0;
    for (n, joint) in scene.joints.iter().enumerate() {
        if scene.places.iter().any(|p| p.joint == n) {
            continue;
        }
        let Some(through) = analysis::default_through(joint, &scene.solved) else {
            continue;
        };
        if matches!(
            analysis::joint_access(doc, &scene.solved, joint),
            Some(Access::Blocked(_)) | None
        ) {
            continue;
        }
        let middle = joint.point(0.5, 0.5);
        add_screws(
            doc,
            through,
            joint.other(through),
            &positions(joint, middle, count.max(2)),
        );
        filled += 1;
    }
    filled
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use tarugo_core::front::new_frame;

    #[test]
    fn click_on_the_outside_of_a_side_reaches_the_board_behind() {
        let mut doc = Document::new();
        let f = new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        let scene = Scene::build(&doc);
        let hit = Hit {
            part: f.left,
            axis: 0,
            side: Side::Min,
            point: Vec3::new(0.0, 590.0, 120.0),
            distance: 0.0,
        };
        let n = joint_behind(&scene, &hit).unwrap();
        let joint = scene.joints[n];
        assert!(joint.joins(f.left, f.top));
        // Nothing behind the middle of the side.
        let miss = Hit {
            point: Vec3::new(0.0, 300.0, 120.0),
            ..hit
        };
        assert!(joint_behind(&scene, &miss).is_none());
        // The inner face has no joint behind either.
        let inner = Hit {
            side: Side::Max,
            ..hit
        };
        assert!(joint_behind(&scene, &inner).is_none());

        // One screw: where clicked along the joint, centered in the board's thickness.
        let at = [0.0, 590.0, 120.0];
        assert_eq!(positions(&joint, at, 1), vec![(0.5, 0.4)]);
        // Three: spread with a margin at the ends.
        let spread = positions(&joint, at, 3);
        let along: Vec<f64> = spread.iter().map(|p| (p.1 * 300.0).round()).collect();
        assert_eq!(along, [50.0, 150.0, 250.0]);
    }

    #[test]
    fn fill_all_screws_every_free_joint_once() {
        let mut doc = Document::new();
        new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        let scene = Scene::build(&doc);
        assert_eq!(fill_all(&mut doc, &scene, 2), 4);
        assert_eq!(doc.screws.len(), 8);
        let scene = Scene::build(&doc);
        assert!(scene.fastening.is_empty(), "{:?}", scene.fastening);
        assert_eq!(fill_all(&mut doc, &scene, 2), 0);
    }
}
