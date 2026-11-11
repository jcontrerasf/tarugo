//! Adding a board or a stick by clicking on a face of an existing part.

use tarugo_core::material::MaterialId;
use tarugo_core::model::{Document, End, Extent, PartId, Side};
use tarugo_core::solve::Solved;

use crate::view3d::Hit;

/// Length given along the clicked face's normal when nothing stops the new part.
const DEFAULT_LENGTH: f64 = 300.0;

/// The part closest to `probe` going down (`towards_min`) or up along `axis`, among those
/// the probe is in front of.
fn neighbor(
    doc: &Document,
    solved: &Solved,
    probe: [f64; 3],
    axis: usize,
    towards_min: bool,
) -> Option<PartId> {
    let mut best: Option<(f64, PartId)> = None;
    for part in &doc.parts {
        let Some(b) = solved.get(part.id) else {
            continue;
        };
        let facing = (0..3)
            .filter(|&a| a != axis)
            .all(|a| probe[a] > b.min[a] && probe[a] < b.max[a]);
        if !facing {
            continue;
        }
        let distance = if towards_min {
            probe[axis] - b.max[axis]
        } else {
            b.min[axis] - probe[axis]
        };
        if distance >= 0.0 && best.is_none_or(|(d, _)| distance < d) {
            best = Some((distance, part.id));
        }
    }
    best.map(|(_, id)| id)
}

/// The part that reaches furthest along `axis`, to span the whole furniture.
fn extreme(doc: &Document, solved: &Solved, axis: usize, min: bool) -> Option<PartId> {
    let value = |id: PartId| {
        solved
            .get(id)
            .map(|b| if min { -b.min[axis] } else { b.max[axis] })
    };
    doc.parts
        .iter()
        .filter_map(|p| Some((value(p.id)?, p.id)))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

/// Adds a part of `material` where a face was clicked. A board has its thickness along
/// `axis`; a stick its length. Along the clicked face's normal the part sits on that face;
/// across it, it spans up to the neighbouring parts (or the whole furniture), attached to
/// them so it follows when they move.
pub fn add_part(
    doc: &mut Document,
    solved: &Solved,
    hit: &Hit,
    material: MaterialId,
    axis: usize,
    name: String,
) -> Option<PartId> {
    let board = doc.material(material)?.is_board();
    let outward = if hit.side == Side::Max { 1.0 } else { -1.0 };
    let mut probe = [hit.point.x as f64, hit.point.y as f64, hit.point.z as f64];
    probe[hit.axis] += outward;
    let mut extents = Vec::with_capacity(3);
    for k in 0..3 {
        let thick = board == (k == axis);
        let extent = if thick && k == hit.axis {
            // Lying on the clicked face.
            Extent::Thick {
                anchor: End::at(hit.part, hit.side),
                align: hit.side.opposite(),
            }
        } else if thick {
            Extent::Thick {
                anchor: End::abs(probe[k].round()),
                align: Side::Center,
            }
        } else if k == hit.axis {
            // From the clicked face outwards, up to whatever is in the way.
            let near = End::at(hit.part, hit.side);
            let far = match neighbor(doc, solved, probe, k, hit.side == Side::Min) {
                Some(id) => End::at(id, hit.side.opposite()),
                None => End::at_offset(hit.part, hit.side, outward * DEFAULT_LENGTH),
            };
            if hit.side == Side::Max {
                Extent::Span { lo: near, hi: far }
            } else {
                Extent::Span { lo: far, hi: near }
            }
        } else {
            let end = |towards_min: bool| {
                let (side, far_side) = if towards_min {
                    (Side::Max, Side::Min)
                } else {
                    (Side::Min, Side::Max)
                };
                match neighbor(doc, solved, probe, k, towards_min) {
                    Some(id) => Some(End::at(id, side)),
                    None => Some(End::at(extreme(doc, solved, k, towards_min)?, far_side)),
                }
            };
            Extent::Span {
                lo: end(true)?,
                hi: end(false)?,
            }
        };
        extents.push(extent);
    }
    let axes: [Extent; 3] = extents.try_into().ok()?;
    Some(doc.add_part(name, material, axes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use tarugo_core::front::{Frame, new_frame};
    use tarugo_core::material::MaterialKind;
    use tarugo_core::model::{Dim, WIDTH};
    use tarugo_core::solve::solve;

    fn setup() -> (Document, Frame) {
        let mut doc = Document::new();
        let f = new_frame(&mut doc, 800.0, 600.0, 300.0, ["l", "r", "b", "t"]);
        (doc, f)
    }

    fn hit(part: PartId, axis: usize, side: Side, point: [f32; 3]) -> Hit {
        Hit {
            part,
            axis,
            side,
            point: Vec3::from_array(point),
            distance: 0.0,
        }
    }

    #[test]
    fn shelf_from_the_inner_face_of_a_side() {
        let (mut doc, f) = setup();
        let s = solve(&doc);
        let m = doc.main_material;
        // Inner face of the left side, 250 up.
        let h = hit(f.left, 0, Side::Max, [18.0, 250.2, 100.0]);
        let id = add_part(&mut doc, &s, &h, m, 1, "shelf".into()).unwrap();
        let b = solve(&doc).boxes[&id];
        assert_eq!((b.min, b.max), ([18.0, 241.0, 0.0], [782.0, 259.0, 300.0]));
        // It follows the width.
        doc.set_param(WIDTH, Dim::num(1000.0));
        assert_eq!(solve(&doc).boxes[&id].max[0], 982.0);
    }

    #[test]
    fn cover_on_top_spans_the_whole_furniture() {
        let (mut doc, f) = setup();
        let s = solve(&doc);
        let m = doc.main_material;
        let h = hit(f.top, 1, Side::Max, [400.0, 600.0, 100.0]);
        let id = add_part(&mut doc, &s, &h, m, 1, "cover".into()).unwrap();
        let b = solve(&doc).boxes[&id];
        assert_eq!((b.min, b.max), ([0.0, 600.0, 0.0], [800.0, 618.0, 300.0]));
    }

    #[test]
    fn back_panel_and_vertical_division() {
        let (mut doc, f) = setup();
        let s = solve(&doc);
        let m = doc.main_material;
        // Back face of the bottom: a back panel over the whole back.
        let h = hit(f.bottom, 2, Side::Min, [400.0, 9.0, 0.0]);
        let back = add_part(&mut doc, &s, &h, m, 2, "back".into()).unwrap();
        // Top face of the bottom: a vertical division up to the top board.
        let h = hit(f.bottom, 1, Side::Max, [300.0, 18.0, 100.0]);
        let division = add_part(&mut doc, &s, &h, m, 0, "division".into()).unwrap();
        let s = solve(&doc);
        assert_eq!(
            (s.boxes[&back].min, s.boxes[&back].max),
            ([0.0, 0.0, -18.0], [800.0, 600.0, 0.0])
        );
        assert_eq!(
            (s.boxes[&division].min, s.boxes[&division].max),
            ([291.0, 18.0, 0.0], [309.0, 582.0, 300.0])
        );
    }

    #[test]
    fn stick_standing_on_a_face() {
        let (mut doc, f) = setup();
        let pine = doc.add_material(
            "pine".into(),
            MaterialKind::Stick {
                width: 40.0,
                height: 60.0,
                stock_length: None,
            },
        );
        let s = solve(&doc);
        // A leg under the bottom board.
        let h = hit(f.bottom, 1, Side::Min, [100.0, 0.0, 50.0]);
        let leg = add_part(&mut doc, &s, &h, pine, 1, "leg".into()).unwrap();
        let b = solve(&doc).boxes[&leg];
        assert_eq!((b.min, b.max), ([80.0, -300.0, 20.0], [120.0, 0.0, 80.0]));
    }
}
