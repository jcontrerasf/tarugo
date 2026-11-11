//! The 3D view: camera, meshes, the wgpu renderer and picking.

pub mod camera;
pub mod mesh;
pub mod renderer;

use glam::Vec3;
use tarugo_core::model::{PartId, Side};
use tarugo_core::solve::Aabb;

/// What a ray hits first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub part: PartId,
    /// Face hit: perpendicular to this axis, at this side of the part.
    pub axis: usize,
    pub side: Side,
    pub point: Vec3,
    pub distance: f32,
}

/// Where a ray enters a box: distance and face.
pub fn ray_box(origin: Vec3, dir: Vec3, b: &Aabb) -> Option<(f32, usize, Side)> {
    let mut t_near = f32::NEG_INFINITY;
    let mut t_far = f32::INFINITY;
    let mut face = (0, Side::Min);
    for axis in 0..3 {
        let (lo, hi) = (b.min[axis] as f32, b.max[axis] as f32);
        if dir[axis].abs() < 1e-9 {
            if origin[axis] < lo || origin[axis] > hi {
                return None;
            }
            continue;
        }
        let (t0, t1) = (
            (lo - origin[axis]) / dir[axis],
            (hi - origin[axis]) / dir[axis],
        );
        let (enter, exit, side) = if t0 <= t1 {
            (t0, t1, Side::Min)
        } else {
            (t1, t0, Side::Max)
        };
        if enter > t_near {
            t_near = enter;
            face = (axis, side);
        }
        t_far = t_far.min(exit);
    }
    (t_near <= t_far && t_far >= 0.0).then_some((t_near, face.0, face.1))
}

/// The closest part under a ray.
pub fn pick<'a>(
    origin: Vec3,
    dir: Vec3,
    boxes: impl Iterator<Item = (PartId, &'a Aabb)>,
) -> Option<Hit> {
    boxes
        .filter_map(|(part, b)| {
            let (distance, axis, side) = ray_box(origin, dir, b)?;
            Some(Hit {
                part,
                axis,
                side,
                point: origin + dir * distance,
                distance,
            })
        })
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
}

/// How far along the line `point + t * axis` is the spot closest to a ray. `None` if they
/// are parallel.
pub fn closest_on_axis(point: Vec3, axis: Vec3, origin: Vec3, dir: Vec3) -> Option<f32> {
    let w = point - origin;
    let b = axis.dot(dir);
    let denom = 1.0 - b * b;
    if denom.abs() < 1e-6 {
        return None;
    }
    Some((b * dir.dot(w) - axis.dot(w)) / denom)
}

/// Where a ray crosses the plane through `point` with this normal.
pub fn ray_plane(point: Vec3, normal: Vec3, origin: Vec3, dir: Vec3) -> Option<Vec3> {
    let denom = normal.dot(dir);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = normal.dot(point - origin) / denom;
    Some(origin + dir * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_box() -> Aabb {
        Aabb {
            min: [0.0, 0.0, 0.0],
            max: [100.0, 200.0, 300.0],
        }
    }

    #[test]
    fn ray_enters_through_the_facing_side() {
        let b = unit_box();
        let hit = ray_box(Vec3::new(50.0, 50.0, 1000.0), -Vec3::Z, &b).unwrap();
        assert_eq!(hit, (700.0, 2, Side::Max));
        let hit = ray_box(Vec3::new(-10.0, 50.0, 50.0), Vec3::X, &b).unwrap();
        assert_eq!(hit, (10.0, 0, Side::Min));
        assert!(ray_box(Vec3::new(500.0, 50.0, 1000.0), -Vec3::Z, &b).is_none());
        assert!(ray_box(Vec3::new(50.0, 50.0, 1000.0), Vec3::Z, &b).is_none());
    }

    #[test]
    fn picks_the_closest() {
        let near = unit_box();
        let far = Aabb {
            min: [0.0, 0.0, -500.0],
            max: [100.0, 200.0, -400.0],
        };
        let boxes = [(1, &far), (2, &near)];
        let hit = pick(Vec3::new(50.0, 50.0, 1000.0), -Vec3::Z, boxes.into_iter()).unwrap();
        assert_eq!((hit.part, hit.axis, hit.side), (2, 2, Side::Max));
        assert_eq!(hit.point, Vec3::new(50.0, 50.0, 300.0));
    }

    #[test]
    fn axis_and_plane_helpers() {
        // A ray pointing down -Z that passes 40 to the right of the origin.
        let t = closest_on_axis(Vec3::ZERO, Vec3::X, Vec3::new(40.0, 10.0, 500.0), -Vec3::Z);
        assert_eq!(t, Some(40.0));
        assert_eq!(closest_on_axis(Vec3::ZERO, Vec3::Z, Vec3::X, Vec3::Z), None);
        let p = ray_plane(Vec3::ZERO, Vec3::Z, Vec3::new(3.0, 4.0, 10.0), -Vec3::Z);
        assert_eq!(p, Some(Vec3::new(3.0, 4.0, 0.0)));
    }
}
