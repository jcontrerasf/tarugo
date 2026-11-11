//! Dragging a part or one of its faces in the 3D view.

use glam::Vec3;
use tarugo_core::edit;
use tarugo_core::model::{Document, FaceRef, PartId, Side};
use tarugo_core::solve::Solved;

use super::snap;
use crate::view3d::{closest_on_axis, ray_plane};

#[derive(Clone, Debug, PartialEq)]
pub enum DragKind {
    /// One face along its axis: stretches a free axis, moves the part on a fixed one.
    Face { axis: usize, side: Side },
    /// The whole part on the plane perpendicular to `normal`, along `axes`.
    Move { normal: usize, axes: Vec<usize> },
}

/// A drag in progress. Every frame the document is rebuilt from `base`, so the result
/// depends only on where the pointer is now.
pub struct Drag {
    pub base: Document,
    pub solved: Solved,
    pub part: PartId,
    pub kind: DragKind,
    /// World point grabbed.
    pub grab: Vec3,
    /// Moving what a parameter places changes the parameter (the front sketch) instead of
    /// letting go of it (free 3D editing).
    pub via_params: bool,
}

fn unit(axis: usize) -> Vec3 {
    let mut v = Vec3::ZERO;
    v[axis] = 1.0;
    v
}

impl Drag {
    /// The base document with the drag applied for the ray under the pointer. `threshold`
    /// is the snapping distance in mm (`None`: no snapping). Returns the faces snapped to.
    pub fn apply(
        &self,
        origin: Vec3,
        dir: Vec3,
        threshold: Option<f64>,
    ) -> Option<(Document, Vec<FaceRef>)> {
        let b = *self.solved.get(self.part)?;
        let mut doc = self.base.clone();
        let mut marks = Vec::new();
        let find = |axis: usize, candidates: &[(Side, f64)]| {
            threshold
                .and_then(|t| snap::find(&self.base, &self.solved, self.part, axis, candidates, t))
        };
        match &self.kind {
            DragKind::Face { axis, side } => {
                let t = closest_on_axis(self.grab, unit(*axis), origin, dir)? as f64;
                let mut target = side.of(b.min[*axis], b.max[*axis]) + t;
                let is_thick = self.base.part(self.part)?.axes[*axis].is_thick();
                if !is_thick {
                    // A free axis keeps at least 1 mm of length.
                    target = match side {
                        Side::Max => target.max(b.min[*axis] + 1.0),
                        _ => target.min(b.max[*axis] - 1.0),
                    };
                }
                match find(*axis, &[(*side, target)]) {
                    Some(s) => {
                        marks.push(s.face);
                        edit::move_face(
                            &mut doc,
                            &self.solved,
                            self.part,
                            *axis,
                            *side,
                            s.value,
                            Some(s.face),
                            self.via_params,
                        );
                    }
                    None => edit::move_face(
                        &mut doc,
                        &self.solved,
                        self.part,
                        *axis,
                        *side,
                        target.round(),
                        None,
                        self.via_params,
                    ),
                }
            }
            DragKind::Move { normal, axes } => {
                let point = ray_plane(self.grab, unit(*normal), origin, dir)?;
                let delta = point - self.grab;
                for &axis in axes {
                    let d = delta[axis] as f64;
                    let candidates = [(Side::Min, b.min[axis] + d), (Side::Max, b.max[axis] + d)];
                    match find(axis, &candidates) {
                        Some(s) => {
                            marks.push(s.face);
                            let from = s.side.of(b.min[axis], b.max[axis]);
                            edit::translate(
                                &mut doc,
                                &self.solved,
                                self.part,
                                axis,
                                s.value - from,
                                Some((s.side, s.face)),
                                self.via_params,
                            );
                        }
                        None => {
                            // Whole millimetres for the low face.
                            let to = (b.min[axis] + d).round();
                            edit::translate(
                                &mut doc,
                                &self.solved,
                                self.part,
                                axis,
                                to - b.min[axis],
                                None,
                                self.via_params,
                            );
                        }
                    }
                }
            }
        }
        Some((doc, marks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarugo_core::front::new_frame;
    use tarugo_core::solve::solve;

    fn setup() -> (Document, tarugo_core::front::Frame) {
        let mut doc = Document::new();
        let f = new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        (doc, f)
    }

    #[test]
    fn stretching_a_face_follows_the_pointer_and_snaps() {
        let (doc, f) = setup();
        let solved = solve(&doc);
        // Grab the front face of the top board (z = 300) and pull it towards the viewer.
        let drag = Drag {
            base: doc.clone(),
            solved,
            part: f.top,
            kind: DragKind::Face {
                axis: 2,
                side: Side::Max,
            },
            grab: Vec3::new(400.0, 591.0, 300.0),
            via_params: true,
        };
        // A ray coming down from above that passes at z = 352.4.
        let (origin, dir) = (Vec3::new(400.0, 2000.0, 352.4), -Vec3::Y);
        let (moved, marks) = drag.apply(origin, dir, None).unwrap();
        assert!(marks.is_empty());
        assert_eq!(solve(&moved).boxes[&f.top].max[2], 352.0);
        // Back near the front of the sides: it snaps to them.
        let (origin, dir) = (Vec3::new(400.0, 2000.0, 303.0), -Vec3::Y);
        let (moved, marks) = drag.apply(origin, dir, Some(5.0)).unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(solve(&moved).boxes[&f.top].max[2], 300.0);
    }

    #[test]
    fn moving_on_a_plane() {
        let (doc, f) = setup();
        let solved = solve(&doc);
        // Grab the top board by its front face and move it right and up.
        let drag = Drag {
            base: doc.clone(),
            solved,
            part: f.top,
            kind: DragKind::Move {
                normal: 2,
                axes: vec![0, 1],
            },
            grab: Vec3::new(400.0, 591.0, 300.0),
            via_params: true,
        };
        let (origin, dir) = (Vec3::new(440.3, 641.0, 2000.0), -Vec3::Z);
        let (moved, _) = drag.apply(origin, dir, None).unwrap();
        let s = solve(&moved);
        let b = s.boxes[&f.top];
        assert_eq!((b.min[0], b.max[0]), (58.0, 822.0));
        // Along Y the top is placed by H: the whole furniture got 50 taller.
        assert_eq!((b.min[1], b.max[1]), (632.0, 650.0));
        assert_eq!(s.boxes[&f.left].max[1], 650.0);
    }
}
