//! Triangles and outline segments of the parts for the 3D view.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use tarugo_core::joints::other_axes;
use tarugo_core::shape::Outline;
use tarugo_core::solve::Aabb;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    /// Zero for what is not lit (lines).
    pub normal: [f32; 3],
    /// RGBA, gamma encoded.
    pub color: [f32; 4],
}

#[derive(Default)]
pub struct Mesh {
    pub triangles: Vec<Vertex>,
    pub lines: Vec<Vertex>,
}

/// Angle between consecutive wall segments above which the corner gets an edge line.
const SHARP_COS: f32 = 0.9;

/// sRGB bytes as the color the shader works with (gamma space, like egui's textures).
pub fn rgb_color(rgb: [u8; 3]) -> [f32; 4] {
    [
        rgb[0] as f32 / 255.0,
        rgb[1] as f32 / 255.0,
        rgb[2] as f32 / 255.0,
        1.0,
    ]
}

pub fn vec3(p: [f64; 3]) -> Vec3 {
    Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32)
}

impl Mesh {
    fn triangle(&mut self, p: [Vec3; 3], normal: Vec3, color: [f32; 4]) {
        for pos in p {
            self.triangles.push(Vertex {
                pos: pos.to_array(),
                normal: normal.to_array(),
                color,
            });
        }
    }

    fn quad(&mut self, p: [Vec3; 4], normal: Vec3, color: [f32; 4]) {
        self.triangle([p[0], p[1], p[2]], normal, color);
        self.triangle([p[0], p[2], p[3]], normal, color);
    }

    pub fn line(&mut self, a: Vec3, b: Vec3, color: [f32; 4]) {
        for pos in [a, b] {
            self.lines.push(Vertex {
                pos: pos.to_array(),
                normal: [0.0; 3],
                color,
            });
        }
    }

    /// A part: its box, or its free-form outline extruded along `normal_axis`.
    pub fn add_part(
        &mut self,
        b: &Aabb,
        outline: Option<(usize, &Outline)>,
        color: [f32; 4],
        edge: [f32; 4],
    ) {
        match outline {
            Some((axis, outline)) => self.add_extrusion(b, axis, outline, color, edge),
            None => {
                // The box is the extrusion of its rectangle along X.
                let s = b.size();
                self.add_extrusion(b, 0, &Outline::rect(s[1], s[2]), color, edge);
            }
        }
    }

    fn add_extrusion(
        &mut self,
        b: &Aabb,
        axis: usize,
        outline: &Outline,
        color: [f32; 4],
        edge: [f32; 4],
    ) {
        let [i, j] = other_axes(axis);
        // World point of an outline point, at the low or high cap.
        let at = |p: [f64; 2], high: bool| {
            let mut w = [0.0; 3];
            w[axis] = if high { b.max[axis] } else { b.min[axis] };
            w[i] = b.min[i] + p[0];
            w[j] = b.min[j] + p[1];
            vec3(w)
        };
        let mut axis_dir = Vec3::ZERO;
        axis_dir[axis] = 1.0;

        let (points, triangles) = outline.triangulate();
        for high in [false, true] {
            let normal = if high { axis_dir } else { -axis_dir };
            for t in &triangles {
                self.triangle(t.map(|k| at(points[k], high)), normal, color);
            }
        }
        for ring in outline.rings() {
            let n = ring.len();
            let wall_normal = |k: usize| {
                let (a, c) = (ring[k], ring[(k + 1) % n]);
                let mut normal = Vec3::ZERO;
                normal[i] = (c[1] - a[1]) as f32;
                normal[j] = -(c[0] - a[0]) as f32;
                normal.normalize_or_zero()
            };
            for k in 0..n {
                let (a, c) = (ring[k], ring[(k + 1) % n]);
                let normal = wall_normal(k);
                self.quad(
                    [at(a, false), at(c, false), at(c, true), at(a, true)],
                    normal,
                    color,
                );
                self.line(at(a, false), at(c, false), edge);
                self.line(at(a, true), at(c, true), edge);
                // An edge down the wall only where it turns sharply (not along arcs).
                if wall_normal((k + n - 1) % n).dot(normal) < SHARP_COS {
                    self.line(at(a, false), at(a, true), edge);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_has_twelve_triangles_and_edges() {
        let mut mesh = Mesh::default();
        let b = Aabb {
            min: [0.0, 0.0, 0.0],
            max: [18.0, 600.0, 300.0],
        };
        mesh.add_part(&b, None, [1.0; 4], [0.0; 4]);
        assert_eq!(mesh.triangles.len(), 36);
        assert_eq!(mesh.lines.len(), 24);
        for v in &mesh.triangles {
            for a in 0..3 {
                assert!(v.pos[a] >= b.min[a] as f32 && v.pos[a] <= b.max[a] as f32);
            }
            // Normals point out of the box.
            let n = Vec3::from_array(v.normal);
            let center = vec3(b.center());
            assert!(n.dot(Vec3::from_array(v.pos) - center) > 0.0);
        }
    }
}
