//! Orbit camera around a target, with perspective or orthographic projection, and the
//! conversions between the screen and the world.

use eframe::egui::{Pos2, Rect, Vec2};
use glam::{Mat4, Vec3, Vec4Swizzles};

/// Vertical field of view.
const FOV: f32 = 0.6;
/// The pitch stops just before looking straight up or down.
const MAX_PITCH: f32 = 1.55;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub target: Vec3,
    /// Angle around the vertical axis: 0 looks at the front of the furniture.
    pub yaw: f32,
    /// Angle above the horizon.
    pub pitch: f32,
    pub distance: f32,
    pub perspective: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: Vec3::ZERO,
            yaw: 0.5,
            pitch: 0.35,
            distance: 2500.0,
            perspective: true,
        }
    }
}

impl Camera {
    /// Straight at the front, orthographic: the view of the front sketch.
    pub fn front() -> Camera {
        Camera {
            yaw: 0.0,
            pitch: 0.0,
            perspective: false,
            ..Camera::default()
        }
    }

    /// From the target towards the eye.
    pub fn back(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        )
    }

    pub fn right(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin())
    }

    pub fn up(&self) -> Vec3 {
        self.back().cross(self.right())
    }

    pub fn eye(&self) -> Vec3 {
        self.target + self.back() * self.distance
    }

    /// Height of the view at the target, in world units.
    fn view_height(&self) -> f32 {
        2.0 * self.distance * (FOV / 2.0).tan()
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_to_rh(self.eye(), -self.back(), self.up());
        let proj = if self.perspective {
            let near = (self.distance * 0.02).max(1.0);
            Mat4::perspective_rh(FOV, aspect, near, self.distance + 100_000.0)
        } else {
            let h = self.view_height() / 2.0;
            Mat4::orthographic_rh(
                -h * aspect,
                h * aspect,
                -h,
                h,
                -100_000.0,
                self.distance + 100_000.0,
            )
        };
        proj * view
    }

    /// World units per screen point at the target.
    pub fn units_per_point(&self, rect: Rect) -> f32 {
        self.view_height() / rect.height().max(1.0)
    }

    /// Screen position of a world point; `None` behind the camera.
    pub fn project(&self, rect: Rect, p: Vec3) -> Option<Pos2> {
        let clip = self.view_proj(rect.aspect_ratio()) * p.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.xyz() / clip.w;
        Some(Pos2::new(
            rect.left() + (ndc.x * 0.5 + 0.5) * rect.width(),
            rect.top() + (0.5 - ndc.y * 0.5) * rect.height(),
        ))
    }

    /// The ray under a screen position: origin and unit direction.
    pub fn ray(&self, rect: Rect, pos: Pos2) -> (Vec3, Vec3) {
        let ndc_x = (pos.x - rect.left()) / rect.width() * 2.0 - 1.0;
        let ndc_y = 1.0 - (pos.y - rect.top()) / rect.height() * 2.0;
        let inv = self.view_proj(rect.aspect_ratio()).inverse();
        let near = inv.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
        let far = inv.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
        (near, (far - near).normalize_or_zero())
    }

    pub fn orbit(&mut self, delta: Vec2) {
        self.yaw -= delta.x * 0.008;
        self.pitch = (self.pitch + delta.y * 0.008).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Moves the target so what is under the pointer follows it.
    pub fn pan(&mut self, delta: Vec2, rect: Rect) {
        let scale = self.units_per_point(rect);
        self.target += (-self.right() * delta.x + self.up() * delta.y) * scale;
    }

    /// Zooms by `factor` (> 1 closer) keeping the world point under `pos` in place.
    pub fn zoom(&mut self, factor: f32, rect: Rect, pos: Pos2) {
        let factor = factor.clamp(0.2, 5.0);
        let new_distance = (self.distance / factor).clamp(50.0, 60_000.0);
        let applied = self.distance / new_distance;
        // Offset of the pointer from the center of the view, in world units at the target.
        let offset = (pos - rect.center()) * self.units_per_point(rect);
        let shift = (self.right() * offset.x - self.up() * offset.y) * (1.0 - 1.0 / applied);
        self.target += shift;
        self.distance = new_distance;
    }

    /// Frames a box.
    pub fn fit(&mut self, min: Vec3, max: Vec3, rect: Rect) {
        self.target = (min + max) / 2.0;
        let size = max - min;
        // What the box takes on screen, roughly: its extent across and along the view.
        let across = (size.x * self.right().x.abs()) + (size.z * self.right().z.abs()) + 1.0;
        let up = self.up();
        let tall = size.x * up.x.abs() + size.y * up.y.abs() + size.z * up.z.abs() + 1.0;
        let aspect = rect.aspect_ratio().max(0.1);
        let height = (tall).max(across / aspect) * 1.25;
        self.distance = (height / (2.0 * (FOV / 2.0).tan())).clamp(50.0, 60_000.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(800.0, 600.0))
    }

    #[test]
    fn target_projects_to_the_center() {
        for perspective in [true, false] {
            let cam = Camera {
                target: Vec3::new(10.0, 20.0, 30.0),
                perspective,
                ..Camera::default()
            };
            let p = cam.project(rect(), cam.target).unwrap();
            assert!((p - rect().center()).length() < 1e-2, "{p:?}");
        }
    }

    #[test]
    fn rays_go_through_what_is_projected() {
        for perspective in [true, false] {
            let cam = Camera {
                perspective,
                ..Camera::default()
            };
            let world = Vec3::new(300.0, -120.0, 80.0);
            let screen = cam.project(rect(), world).unwrap();
            let (origin, dir) = cam.ray(rect(), screen);
            let closest = origin + dir * (world - origin).dot(dir);
            assert!((closest - world).length() < 0.5, "{closest:?}");
        }
    }

    #[test]
    fn front_view_axes() {
        let cam = Camera::front();
        assert!((cam.right() - Vec3::X).length() < 1e-6);
        assert!((cam.up() - Vec3::Y).length() < 1e-6);
        assert!((cam.back() - Vec3::Z).length() < 1e-6);
        // +X is to the right and +Y up on screen.
        let o = cam.project(rect(), Vec3::ZERO).unwrap();
        let x = cam.project(rect(), Vec3::X * 100.0).unwrap();
        let y = cam.project(rect(), Vec3::Y * 100.0).unwrap();
        assert!(x.x > o.x && (x.y - o.y).abs() < 1e-3);
        assert!(y.y < o.y && (y.x - o.x).abs() < 1e-3);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_pointer() {
        let mut cam = Camera::front();
        let pos = Pos2::new(700.0, 200.0);
        let (before, _) = cam.ray(rect(), pos);
        cam.zoom(1.5, rect(), pos);
        let (after, _) = cam.ray(rect(), pos);
        assert!((before.truncate() - after.truncate()).length() < 0.5);
    }

    #[test]
    fn fit_frames_the_box() {
        let mut cam = Camera::default();
        let (min, max) = (Vec3::ZERO, Vec3::new(800.0, 2000.0, 400.0));
        cam.fit(min, max, rect());
        for corner in [
            min,
            max,
            Vec3::new(800.0, 0.0, 0.0),
            Vec3::new(0.0, 2000.0, 400.0),
        ] {
            let p = cam.project(rect(), corner).unwrap();
            assert!(rect().contains(p), "{p:?}");
        }
    }
}
