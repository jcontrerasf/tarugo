//! Snapping a moving face to the faces of the other parts.

use tarugo_core::edit::can_refer;
use tarugo_core::model::{Document, FaceRef, PartId, Side};
use tarugo_core::solve::Solved;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    /// Face of the moving part that snapped.
    pub side: Side,
    /// What it snapped to.
    pub face: FaceRef,
    pub value: f64,
}

/// The closest face of another part to any of the moving faces (`candidates`: face of the
/// part and where it would be), within `threshold`. Parts that depend on the moving one
/// along the axis are skipped: attaching to them would make a loop.
pub fn find(
    doc: &Document,
    solved: &Solved,
    part: PartId,
    axis: usize,
    candidates: &[(Side, f64)],
    threshold: f64,
) -> Option<Snap> {
    let mut best: Option<(f64, Snap)> = None;
    for other in &doc.parts {
        let Some(b) = solved.get(other.id) else {
            continue;
        };
        if !can_refer(doc, part, axis, other.id) {
            continue;
        }
        // Faces first: on a tie they win over the middle plane.
        for target in [Side::Min, Side::Max, Side::Center] {
            let value = target.of(b.min[axis], b.max[axis]);
            for (side, at) in candidates {
                let distance = (at - value).abs();
                if distance <= threshold && best.is_none_or(|(d, _)| distance < d - 1e-9) {
                    best = Some((
                        distance,
                        Snap {
                            side: *side,
                            face: FaceRef {
                                part: other.id,
                                side: target,
                            },
                            value,
                        },
                    ));
                }
            }
        }
    }
    best.map(|(_, snap)| snap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarugo_core::front::new_frame;
    use tarugo_core::solve::solve;

    #[test]
    fn snaps_to_the_closest_face_and_avoids_loops() {
        let mut doc = Document::new();
        let f = new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        let s = solve(&doc);
        // The top's left face near the outer face of the left side.
        let snap = find(&doc, &s, f.top, 0, &[(Side::Min, 3.0)], 5.0).unwrap();
        assert_eq!(
            (snap.face.part, snap.face.side, snap.value),
            (f.left, Side::Min, 0.0)
        );
        assert!(find(&doc, &s, f.top, 0, &[(Side::Min, 100.0)], 5.0).is_none());
        // The left side cannot snap to the top (which hangs from it), only to the right.
        let snap = find(&doc, &s, f.left, 0, &[(Side::Max, 20.0)], 5.0);
        assert!(snap.is_none());
        let snap = find(&doc, &s, f.left, 0, &[(Side::Max, 780.0)], 5.0).unwrap();
        assert_eq!((snap.face.part, snap.face.side), (f.right, Side::Min));
    }
}
