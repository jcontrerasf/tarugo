//! The 3D view: navigation, the tools' input and everything drawn over the scene.

use eframe::egui::{
    self, Align2, Color32, FontId, PointerButton, Pos2, Rect, Response, Sense, Shape, Stroke, Ui,
    Vec2,
};
use glam::Vec3;
use tarugo_core::front;
use tarugo_core::joints::other_axes;
use tarugo_core::model::{PartId, Side};
use tarugo_core::solve::Aabb;

use crate::app::{App, Stage, part_name};
use crate::i18n::{fill, t};
use crate::theme::pal;
use crate::tools::{self, Drag, DragKind, Tool};
use crate::ui::cota::{CotaEdit, CotaView};
use crate::view3d::camera::Camera;
use crate::view3d::mesh::{rgb_color, vec3};
use crate::view3d::{Hit, pick};

/// Pointer distance (points) to grab a handle or a screw.
const GRAB: f32 = 10.0;
/// Snapping distance on screen (points).
const SNAP: f32 = 9.0;

/// What the tool under the pointer would do with a click, drawn as a ghost.
#[derive(Default)]
pub enum Preview {
    #[default]
    None,
    Divide(tools::divide::Preview),
    Part(Aabb),
    /// Head and tip of each screw.
    Screws(Vec<(Vec3, Vec3)>),
}

fn unit(axis: usize) -> Vec3 {
    let mut v = Vec3::ZERO;
    v[axis] = 1.0;
    v
}

fn corners(b: &Aabb) -> [Vec3; 8] {
    let (lo, hi) = (vec3(b.min), vec3(b.max));
    std::array::from_fn(|i| {
        Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    })
}

/// The 12 edges of a box, as pairs of corner indices (see [`corners`]).
const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (2, 3),
    (4, 5),
    (6, 7),
    (0, 2),
    (1, 3),
    (4, 6),
    (5, 7),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

fn face_center(b: &Aabb, axis: usize, side: Side) -> Vec3 {
    let mut c = vec3(b.center());
    c[axis] = side.of(b.min[axis], b.max[axis]) as f32;
    c
}

fn face_corners(b: &Aabb, axis: usize, side: Side) -> [Vec3; 4] {
    let [i, j] = other_axes(axis);
    let at = side.of(b.min[axis], b.max[axis]);
    let point = |u: f64, v: f64| {
        let mut p = [0.0; 3];
        p[axis] = at;
        p[i] = u;
        p[j] = v;
        vec3(p)
    };
    [
        point(b.min[i], b.min[j]),
        point(b.max[i], b.min[j]),
        point(b.max[i], b.max[j]),
        point(b.min[i], b.max[j]),
    ]
}

impl App {
    fn hit_at(&self, camera: &Camera, pos: Pos2) -> Option<Hit> {
        let (origin, dir) = camera.ray(self.view_rect, pos);
        pick(origin, dir, self.scene.boxes(self.doc()))
    }

    /// Face handles of the selected part: axis, side and center.
    fn handles(&self) -> Vec<(usize, Side, Vec3)> {
        if self.stage != Stage::Design || self.tool != Tool::Select {
            return Vec::new();
        }
        let Some(b) = self
            .session
            .selected
            .filter(|id| !self.is_locked(*id))
            .and_then(|id| self.scene.get(id))
        else {
            return Vec::new();
        };
        (0..3)
            .flat_map(|axis| {
                [Side::Min, Side::Max].map(|side| (axis, side, face_center(b, axis, side)))
            })
            .collect()
    }

    fn handle_at(&self, camera: &Camera, pos: Pos2) -> Option<(usize, Side, Vec3)> {
        self.handles()
            .into_iter()
            .filter_map(|h| {
                let screen = camera.project(self.view_rect, h.2)?;
                let d = screen.distance(pos);
                (d <= GRAB).then_some((d, h))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, h)| h)
    }

    /// The screw whose head is under the pointer.
    fn screw_at(&self, camera: &Camera, pos: Pos2) -> Option<u32> {
        self.scene
            .places
            .iter()
            .filter_map(|p| {
                let screen = camera.project(self.view_rect, vec3(p.head))?;
                let d = screen.distance(pos);
                (d <= GRAB).then_some((d, p.id))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    }

    pub(crate) fn canvas(&mut self, ui: &mut Ui) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        self.view_rect = rect;
        let resized = self.framed.is_some_and(|size| size != rect.size());
        if (self.fit_pending || resized) && rect.width() > 10.0 {
            self.fit_pending = false;
            self.framed = Some(rect.size());
            if let Some((min, max)) = self.scene.bounds() {
                // Room around for the overall dimensions.
                let margin = (max - min).max_element() * 0.12 + 30.0;
                let front = self.stage == Stage::Front;
                self.view_camera()
                    .fit(min - Vec3::splat(margin), max + Vec3::splat(margin), rect);
                if front {
                    self.front_camera.yaw = 0.0;
                    self.front_camera.pitch = 0.0;
                }
            }
        }
        self.navigate(ui, &response);
        let preview = self.tool_input(ui, &response);
        self.refresh_scene();
        self.paint_scene(ui, rect);
        let camera = *self.view_camera();
        let painter = ui.painter_at(rect);
        self.paint_marks(&painter, &camera);
        self.paint_preview(&painter, &camera, &preview);
        self.paint_screws(&painter, &camera);
        self.paint_handles(&painter, &camera, response.hover_pos());
        let cotas = self.cotas();
        self.paint_cotas(ui, &camera, &cotas);
        if self.stage != Stage::Front {
            self.view_cube(ui, rect);
        }
        self.cota_editor(ui);
    }

    fn navigate(&mut self, ui: &Ui, response: &Response) {
        let rect = self.view_rect;
        let front = self.stage == Stage::Front;
        if let Some(pos) = response.hover_pos() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = (scroll * 0.004).exp() * pinch;
            if (factor - 1.0).abs() > 1e-4 {
                self.framed = None;
                self.view_camera().zoom(factor, rect, pos);
            }
        }
        let delta = response.drag_delta();
        let shift = ui.input(|i| i.modifiers.shift);
        let pan = response.dragged_by(PointerButton::Middle)
            || (response.dragged_by(PointerButton::Secondary) && (front || shift));
        if pan {
            self.framed = None;
            self.view_camera().pan(delta, rect);
        } else if response.dragged_by(PointerButton::Secondary) {
            self.camera.orbit(delta);
        }
    }

    /// Handles the primary button for the current tool. Returns what to draw as preview.
    fn tool_input(&mut self, ui: &Ui, response: &Response) -> Preview {
        let rect = self.view_rect;
        let camera = *self.view_camera();
        let front = self.stage == Stage::Front;
        let hover_pos = response.hover_pos();
        let hit = hover_pos.and_then(|p| self.hit_at(&camera, p));
        if self.drag.is_none() {
            self.hover = hit.map(|h| h.part);
        }
        self.pointer_world = hit.map(|h| [h.point.x as f64, h.point.y as f64, h.point.z as f64]);
        if front && let Some(pos) = hover_pos {
            self.pointer_world = self.front_point(pos).map(|p| [p[0], p[1], 0.0]);
        }

        // Start of a drag with the main button.
        if response.drag_started_by(PointerButton::Primary) {
            self.cota_edit = None;
            let press = ui
                .input(|i| i.pointer.press_origin())
                .or(hover_pos)
                .unwrap_or(rect.center());
            let pressed = self.hit_at(&camera, press);
            let mut kind = None;
            if self.tool == Tool::Select && self.stage != Stage::Screws {
                if let (Some((axis, side, center)), Some(part)) =
                    (self.handle_at(&camera, press), self.session.selected)
                {
                    kind = Some((part, DragKind::Face { axis, side }, center));
                } else if let Some(h) = pressed {
                    self.session.select(Some(h.part));
                    self.highlight.clear();
                    let doc = self.doc();
                    let axes: Vec<usize> = if self.is_locked(h.part) {
                        Vec::new()
                    } else if front {
                        // On the front sketch boards only slide across their thickness.
                        (0..2)
                            .filter(|&a| doc.part(h.part).is_some_and(|p| p.axes[a].is_thick()))
                            .collect()
                    } else {
                        other_axes(h.axis).to_vec()
                    };
                    let normal = if front { 2 } else { h.axis };
                    if !axes.is_empty() {
                        kind = Some((h.part, DragKind::Move { normal, axes }, h.point));
                    } else if self.is_locked(h.part) {
                        self.warn_locked(h.part);
                    }
                }
            }
            if let Some((part, kind, grab)) = kind {
                self.drag = Some(Drag {
                    base: self.doc().clone(),
                    solved: self.scene.solved.clone(),
                    part,
                    kind,
                    grab,
                    via_params: front,
                });
            }
        }

        if response.dragged_by(PointerButton::Primary) {
            if let Some(drag) = &self.drag {
                if let Some(pos) = response.interact_pointer_pos() {
                    let (origin, dir) = camera.ray(rect, pos);
                    // Alt turns snapping off.
                    let snapping = !ui.input(|i| i.modifiers.alt);
                    let threshold = snapping.then(|| (SNAP * camera.units_per_point(rect)) as f64);
                    if let Some((doc, marks)) = drag.apply(origin, dir, threshold) {
                        self.session.edit_grouped("drag", |d| *d = doc);
                        self.snap_mark = marks;
                    }
                }
            } else if front {
                self.framed = None;
                self.front_camera.pan(response.drag_delta(), rect);
            } else {
                self.camera.orbit(response.drag_delta());
            }
        }
        if response.drag_stopped_by(PointerButton::Primary) && self.drag.take().is_some() {
            self.session.end_group();
            self.snap_mark.clear();
        }
        if self.drag.is_some() {
            return Preview::None;
        }

        let clicked = response.clicked_by(PointerButton::Primary);
        if clicked {
            self.cota_edit = None;
            self.message = None;
        }
        match self.tool {
            Tool::Select => {
                if clicked {
                    let screw = hover_pos
                        .filter(|_| self.screws_visible())
                        .and_then(|p| self.screw_at(&camera, p));
                    self.highlight.clear();
                    match screw {
                        Some(id) => {
                            self.session.select(None);
                            self.session.selected_screw = Some(id);
                        }
                        None => self.session.select(hit.map(|h| h.part)),
                    }
                }
                Preview::None
            }
            Tool::Divide(axis) => {
                let Some(point) = hover_pos.and_then(|p| self.front_point(p)) else {
                    return Preview::None;
                };
                let Some(preview) = tools::divide::preview(
                    self.doc(),
                    &self.scene.solved,
                    &self.divide,
                    axis,
                    point,
                ) else {
                    return Preview::None;
                };
                if clicked {
                    self.place_division(&preview);
                    return Preview::None;
                }
                Preview::Divide(preview)
            }
            Tool::AddBoard | Tool::AddStick => {
                let board = self.tool == Tool::AddBoard;
                let (Some(h), Some(material)) = (hit, self.add_material(board)) else {
                    if clicked && self.add_material(board).is_none() {
                        self.error(t().no_material_of_kind);
                    }
                    return Preview::None;
                };
                let base = if board {
                    t().part_board
                } else {
                    t().part_stick
                };
                let name = part_name(self.doc(), base);
                let axis = self.add.axis;
                if clicked {
                    let solved = self.scene.solved.clone();
                    let id = self
                        .session
                        .edit(|d| tools::add::add_part(d, &solved, &h, material, axis, name));
                    self.session.select(id);
                    return Preview::None;
                }
                // The ghost: what the part would be, solved on a copy.
                let mut copy = self.doc().clone();
                tools::add::add_part(&mut copy, &self.scene.solved, &h, material, axis, name)
                    .and_then(|id| tarugo_core::solve::solve(&copy).get(id).copied())
                    .map_or(Preview::None, Preview::Part)
            }
            Tool::Screw => {
                if clicked && let Some(id) = hover_pos.and_then(|p| self.screw_at(&camera, p)) {
                    self.session.select(None);
                    self.session.selected_screw = Some(id);
                    return Preview::None;
                }
                let Some((h, n)) =
                    hit.and_then(|h| Some((h, tools::screw::joint_behind(&self.scene, &h)?)))
                else {
                    if clicked {
                        self.session.selected_screw = None;
                        if hit.is_some() {
                            self.info(t().no_joint_behind);
                        }
                    }
                    return Preview::None;
                };
                let joint = self.scene.joints[n];
                let at = [h.point.x as f64, h.point.y as f64, h.point.z as f64];
                let places = tools::screw::positions(&joint, at, self.screws.count);
                if clicked {
                    let (through, into) = (h.part, joint.other(h.part));
                    self.session
                        .edit(|d| tools::screw::add_screws(d, through, into, &places));
                    self.session.selected_screw = None;
                    return Preview::None;
                }
                let inward = if h.side == Side::Max { -1.0 } else { 1.0 };
                Preview::Screws(
                    places
                        .iter()
                        .map(|(u, v)| {
                            let tip = vec3(joint.point(*u, *v));
                            let mut head = tip;
                            head[h.axis] = h.point[h.axis];
                            (head, tip + unit(h.axis) * inward * SCREW_DEPTH)
                        })
                        .collect(),
                )
            }
        }
    }

    fn place_division(&mut self, preview: &tools::divide::Preview) {
        let axis = preview.axis;
        let base = if axis == 0 {
            t().part_division
        } else {
            t().part_shelf
        };
        if self.divide.equal {
            let n = self.divide.spaces.max(2);
            let name = base.to_string();
            let cell = preview.cell;
            let ids = self
                .session
                .edit(|d| front::divide_equal(d, &cell, axis, n, &name));
            self.session.select(ids.first().copied());
            return;
        }
        let name = part_name(self.doc(), base);
        let (cell, cota) = (preview.cell, preview.cota.clone());
        let id = self
            .session
            .edit(|d| front::divide(d, &cell, axis, &cota, &name));
        self.session.select(Some(id));
        // Straight to typing the exact dimension.
        self.refresh_scene();
        self.cota_edit = CotaEdit::for_anchor(self, id, axis);
    }

    fn paint_scene(&mut self, ui: &Ui, rect: Rect) {
        let dark = ui.visuals().dark_mode;
        self.refresh_mesh(dark);
        let camera = *self.view_camera();
        let ppp = ui.ctx().pixels_per_point();
        let size = [
            (rect.width() * ppp).round() as u32,
            (rect.height() * ppp).round() as u32,
        ];
        let p = pal();
        let clear = rgb_color([p.backdrop.r(), p.backdrop.g(), p.backdrop.b()]);
        let light = camera.back() * 0.55 + camera.up() * 0.75 - camera.right() * 0.4;
        let texture = self
            .renderer
            .as_mut()
            .and_then(|r| r.render(size, camera.view_proj(rect.aspect_ratio()), light, clear));
        match texture {
            Some(id) => {
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                ui.painter_at(rect).image(id, rect, uv, Color32::WHITE);
            }
            None => {
                ui.painter_at(rect).text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    t().no_3d,
                    FontId::proportional(14.0),
                    p.text_soft,
                );
            }
        }
    }

    fn paint_box(&self, painter: &egui::Painter, camera: &Camera, b: &Aabb, stroke: Stroke) {
        let c = corners(b).map(|p| camera.project(self.view_rect, p));
        for (i, j) in EDGES {
            if let (Some(a), Some(b)) = (c[i], c[j]) {
                painter.line_segment([a, b], stroke);
            }
        }
    }

    fn paint_quad(
        &self,
        painter: &egui::Painter,
        camera: &Camera,
        quad: [Vec3; 4],
        fill: Color32,
        stroke: Stroke,
    ) {
        let points: Vec<Pos2> = quad
            .iter()
            .filter_map(|p| camera.project(self.view_rect, *p))
            .collect();
        if points.len() == 4 {
            painter.add(Shape::convex_polygon(points, fill, stroke));
        }
    }

    /// Hover, selection, issue highlight and snap marks.
    fn paint_marks(&self, painter: &egui::Painter, camera: &Camera) {
        let p = pal();
        for id in &self.highlight {
            if let Some(b) = self.scene.get(*id) {
                self.paint_box(painter, camera, b, Stroke::new(2.5, p.warning));
            }
        }
        if let Some(b) = self
            .hover
            .filter(|h| Some(*h) != self.session.selected)
            .and_then(|id| self.scene.get(id))
        {
            self.paint_box(
                painter,
                camera,
                b,
                Stroke::new(1.5, p.accent.gamma_multiply(0.6)),
            );
        }
        if let Some(b) = self.session.selected.and_then(|id| self.scene.get(id)) {
            self.paint_box(painter, camera, b, Stroke::new(2.0, p.accent));
        }
        for face in &self.snap_mark {
            if let Some(b) = self.scene.get(face.part) {
                let axis = match &self.drag {
                    Some(Drag {
                        kind: DragKind::Face { axis, .. },
                        ..
                    }) => vec![*axis],
                    Some(Drag {
                        kind: DragKind::Move { axes, .. },
                        ..
                    }) => axes.clone(),
                    None => Vec::new(),
                };
                // The face snapped to: on whichever dragged axis the moving part meets it.
                for axis in axis {
                    let moving = self
                        .drag
                        .as_ref()
                        .and_then(|d| self.scene.get(d.part))
                        .copied();
                    let at = face.side.of(b.min[axis], b.max[axis]);
                    let touches = moving.is_some_and(|m| {
                        (m.min[axis] - at).abs() < 0.01 || (m.max[axis] - at).abs() < 0.01
                    });
                    if touches {
                        let quad = face_corners(b, axis, face.side);
                        self.paint_quad(
                            painter,
                            camera,
                            quad,
                            p.snap.gamma_multiply(0.25),
                            Stroke::new(2.0, p.snap),
                        );
                    }
                }
            }
        }
    }

    fn paint_preview(&self, painter: &egui::Painter, camera: &Camera, preview: &Preview) {
        let p = pal();
        let ghost = p.accent.gamma_multiply(0.35);
        let line = Stroke::new(1.5, p.accent);
        match preview {
            Preview::None => {}
            Preview::Part(b) => {
                for axis in 0..3 {
                    for side in [Side::Min, Side::Max] {
                        self.paint_quad(
                            painter,
                            camera,
                            face_corners(b, axis, side),
                            p.accent.gamma_multiply(0.12),
                            Stroke::NONE,
                        );
                    }
                }
                self.paint_box(painter, camera, b, line);
            }
            Preview::Divide(d) => {
                let z = self.scene.bounds().map_or(0.0, |(_, max)| max.z);
                let across = 1 - d.axis;
                let point = |along: f64, other: f64| {
                    let mut v = [0.0f32; 3];
                    v[d.axis] = along as f32;
                    v[across] = other as f32;
                    v[2] = z;
                    Vec3::from_array(v)
                };
                let (lo, hi) = (d.cell.min[across], d.cell.max[across]);
                for (a, b) in &d.boards {
                    let quad = [point(*a, lo), point(*b, lo), point(*b, hi), point(*a, hi)];
                    self.paint_quad(painter, camera, quad, ghost, line);
                }
                let mid = (lo + hi) / 2.0;
                if self.divide.equal {
                    // The clear space, once.
                    if let Some((first, _)) = d.boards.first() {
                        let gap = first - d.cell.min[d.axis];
                        let view = CotaView::plain(
                            point(d.cell.min[d.axis], mid),
                            point(*first, mid),
                            gap,
                        );
                        self.paint_cota(painter, camera, &view, false);
                    }
                } else if let Some((a, b)) = d.boards.first() {
                    let reached = d.cota.align.of(*a, *b);
                    let distance = (reached - d.from).abs();
                    let view = CotaView::plain(point(d.from, mid), point(reached, mid), distance);
                    self.paint_cota(painter, camera, &view, false);
                    // The clear space left at the other side.
                    let (far, edge) = if d.cota.from_hi {
                        (d.cell.min[d.axis], *a)
                    } else {
                        (d.cell.max[d.axis], *b)
                    };
                    let other = lo + (hi - lo) * 0.3;
                    let mut view =
                        CotaView::plain(point(edge, other), point(far, other), (far - edge).abs());
                    view.soft = true;
                    self.paint_cota(painter, camera, &view, false);
                }
            }
            Preview::Screws(screws) => {
                for (head, tip) in screws {
                    self.paint_screw(painter, camera, *head, *tip, p.accent, 5.0);
                }
            }
        }
    }

    fn paint_screw(
        &self,
        painter: &egui::Painter,
        camera: &Camera,
        head: Vec3,
        tip: Vec3,
        color: Color32,
        radius: f32,
    ) {
        let (Some(a), Some(b)) = (
            camera.project(self.view_rect, head),
            camera.project(self.view_rect, tip),
        ) else {
            return;
        };
        painter.line_segment([a, b], Stroke::new(2.0, color));
        painter.circle(a, radius, color, Stroke::new(1.5, pal().window));
        // The cross of the head.
        let r = radius * 0.55;
        let cross = Stroke::new(1.0, pal().window);
        painter.line_segment([a - Vec2::new(r, 0.0), a + Vec2::new(r, 0.0)], cross);
        painter.line_segment([a - Vec2::new(0.0, r), a + Vec2::new(0.0, r)], cross);
    }

    /// Screws show in their stage, and in the 3D design unless hidden.
    fn screws_visible(&self) -> bool {
        match self.stage {
            Stage::Screws => true,
            Stage::Design => self.prefs.show_screws,
            _ => false,
        }
    }

    fn paint_screws(&self, painter: &egui::Painter, camera: &Camera) {
        if !self.screws_visible() {
            return;
        }
        let p = pal();
        let big = self.stage == Stage::Screws;
        for place in &self.scene.places {
            let selected = self.session.selected_screw == Some(place.id);
            let color = if selected { p.accent } else { p.screw };
            let head = vec3(place.head);
            let tip = vec3(place.pos) + unit(place.axis) * place.dir as f32 * SCREW_DEPTH;
            let radius = if selected {
                6.0
            } else if big {
                4.5
            } else {
                3.0
            };
            self.paint_screw(painter, camera, head, tip, color, radius);
        }
    }

    fn paint_handles(&self, painter: &egui::Painter, camera: &Camera, pointer: Option<Pos2>) {
        let p = pal();
        let dragged = match &self.drag {
            Some(Drag {
                kind: DragKind::Face { axis, side },
                ..
            }) => Some((*axis, *side)),
            _ => None,
        };
        let hovered = pointer.and_then(|pos| self.handle_at(camera, pos));
        for (axis, side, center) in self.handles() {
            let Some(pos) = camera.project(self.view_rect, center) else {
                continue;
            };
            let active = dragged == Some((axis, side))
                || (dragged.is_none() && hovered.is_some_and(|h| (h.0, h.1) == (axis, side)));
            let (radius, fill) = if active {
                (6.5, p.accent)
            } else {
                (4.5, p.window)
            };
            painter.circle(pos, radius, fill, Stroke::new(1.5, p.accent));
        }
    }

    /// A dimension line with its value. Returns the rectangle of the label.
    pub(crate) fn paint_cota(
        &self,
        painter: &egui::Painter,
        camera: &Camera,
        cota: &CotaView,
        hovered: bool,
    ) -> Option<Rect> {
        let p = pal();
        let a = camera.project(self.view_rect, cota.a)?;
        let b = camera.project(self.view_rect, cota.b)?;
        let color = if cota.soft { p.text_soft } else { p.cota };
        let stroke = Stroke::new(1.2, color);
        painter.line_segment([a, b], stroke);
        let dir = (b - a).normalized();
        if dir.is_finite() {
            let tick = Vec2::new(-dir.y, dir.x) * 5.0;
            painter.line_segment([a - tick, a + tick], stroke);
            painter.line_segment([b - tick, b + tick], stroke);
        }
        let galley = painter.layout_no_wrap(cota.text.clone(), FontId::proportional(12.5), color);
        let label = Rect::from_center_size(a + (b - a) * 0.5, galley.size() + Vec2::new(10.0, 4.0));
        let editable = cota.target.is_some();
        let fill = if hovered && editable {
            p.accent_soft
        } else {
            p.window
        };
        let border = if editable { color } else { p.border };
        painter.rect(
            label,
            4.0,
            fill,
            Stroke::new(1.0, border),
            egui::StrokeKind::Inside,
        );
        painter.galley(label.center() - galley.size() / 2.0, galley, color);
        Some(label)
    }

    /// Dimensions of the furniture and of the selection; the ones with a target open an
    /// inline editor when clicked.
    fn paint_cotas(&mut self, ui: &mut Ui, camera: &Camera, cotas: &[CotaView]) {
        let painter = ui.painter_at(self.view_rect);
        let mut open = None;
        for (n, cota) in cotas.iter().enumerate() {
            // First pass without hover to learn where the label is.
            let Some(label) = self.paint_cota(&painter, camera, cota, false) else {
                continue;
            };
            let Some(target) = &cota.target else {
                continue;
            };
            let response = ui
                .interact(label, ui.scope_id().with(("cota", n)), Sense::click())
                .on_hover_text(t().cota_edit_help);
            if response.hovered() {
                self.paint_cota(&painter, camera, cota, true);
            }
            if response.clicked() {
                open = Some(CotaEdit {
                    target: target.clone(),
                    text: cota.edit_text.clone(),
                    pos: label.center(),
                    fresh: true,
                });
            }
        }
        if open.is_some() {
            self.cota_edit = open;
        }
    }

    /// Small cube at the corner that shows the orientation. Each face is split 3 × 3: the
    /// middle sets the view of that face, the borders the view from an edge and the corners
    /// the one from a vertex. Dragging it orbits.
    fn view_cube(&mut self, ui: &mut Ui, rect: Rect) {
        /// Where a face (from -1 to 1) is cut into its zones.
        const CUTS: [f32; 4] = [-1.0, -0.56, 0.56, 1.0];
        let p = pal();
        let size = 26.0;
        let center = Pos2::new(rect.right() - 62.0, rect.top() + 62.0);
        let area = Rect::from_center_size(center, Vec2::splat(size * 3.6));
        let response = ui.interact(
            area,
            ui.scope_id().with("view_cube"),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        let camera = self.camera;
        let project =
            |v: Vec3| center + Vec2::new(v.dot(camera.right()), -v.dot(camera.up())) * size;
        let faces: [(Vec3, &str); 6] = [
            (Vec3::Z, t().view_front),
            (-Vec3::Z, t().view_back),
            (Vec3::X, t().view_right),
            (-Vec3::X, t().view_left),
            (Vec3::Y, t().view_top),
            (-Vec3::Y, t().view_bottom),
        ];
        // Zones of the faces turned to the camera: outline, the direction they look
        // from and whether they are the middle of a face.
        let mut zones: Vec<([Pos2; 4], Vec3, bool)> = Vec::new();
        let mut labels = Vec::new();
        for (normal, name) in faces {
            let facing = normal.dot(camera.back());
            if facing <= 0.05 {
                continue;
            }
            // Two axes across the face.
            let u = if normal.y.abs() > 0.5 {
                Vec3::X
            } else {
                Vec3::Y
            };
            let v = normal.cross(u);
            for i in 0..3 {
                for j in 0..3 {
                    let corner = |a: f32, b: f32| project(normal + u * a + v * b);
                    let (a0, a1, b0, b1) = (CUTS[i], CUTS[i + 1], CUTS[j], CUTS[j + 1]);
                    zones.push((
                        [
                            corner(a0, b0),
                            corner(a1, b0),
                            corner(a1, b1),
                            corner(a0, b1),
                        ],
                        normal + u * (i as f32 - 1.0) + v * (j as f32 - 1.0),
                        i == 1 && j == 1,
                    ));
                }
            }
            if facing > 0.45 {
                labels.push((project(normal), name));
            }
        }
        // An edge or a corner spans several faces: all its zones light up.
        let picked = response
            .hover_pos()
            .and_then(|pos| zones.iter().find(|z| quad_contains(&z.0, pos)))
            .map(|z| z.1);
        for (quad, direction, middle) in &zones {
            let fill = if picked == Some(*direction) {
                p.accent_soft
            } else if *middle {
                p.window
            } else {
                p.panel
            };
            painter.add(Shape::convex_polygon(
                quad.to_vec(),
                fill,
                Stroke::new(1.0, p.border),
            ));
        }
        for (pos, name) in labels {
            painter.text(
                pos,
                Align2::CENTER_CENTER,
                name,
                FontId::proportional(10.5),
                p.text_soft,
            );
        }
        if response.dragged() {
            self.camera.orbit(response.drag_delta());
        } else if response.clicked()
            && let Some(direction) = picked
        {
            let (yaw, pitch) = view_angles(direction, camera.yaw);
            self.look_from(yaw, pitch);
        }
    }

    /// Text for the status bar about what is under the pointer or being dragged.
    pub fn view_status(&self) -> Option<String> {
        if let Some(drag) = &self.drag {
            let b = self.scene.get(drag.part)?;
            let s = b.size();
            return Some(fill(
                t().status_size,
                &[
                    &tarugo_core::expr::format_number(s[0]),
                    &tarugo_core::expr::format_number(s[1]),
                    &tarugo_core::expr::format_number(s[2]),
                ],
            ));
        }
        None
    }

    /// Name of a part, for messages.
    pub fn part_label(&self, id: PartId) -> String {
        self.doc()
            .part(id)
            .map_or_else(|| "?".to_string(), |p| p.name.clone())
    }
}

/// How far into the second part a screw is drawn (mm).
const SCREW_DEPTH: f32 = 30.0;

/// Is the point inside a convex quad? Either winding.
fn quad_contains(quad: &[Pos2; 4], pos: Pos2) -> bool {
    let side = |i: usize| {
        let (a, b) = (quad[i], quad[(i + 1) % 4]);
        (b - a).x * (pos - a).y - (b - a).y * (pos - a).x
    };
    (0..4).all(|i| side(i) >= 0.0) || (0..4).all(|i| side(i) <= 0.0)
}

/// Yaw and pitch of the camera to look from `direction` (from the target to the eye).
/// Straight from above or below the yaw stays as it is.
fn view_angles(direction: Vec3, yaw: f32) -> (f32, f32) {
    let d = direction.normalize_or_zero();
    let pitch = d.y.clamp(-1.0, 1.0).asin().clamp(-1.55, 1.55);
    if d.x.abs() < 1e-4 && d.z.abs() < 1e-4 {
        (yaw, pitch)
    } else {
        (d.x.atan2(d.z), pitch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_angles_look_from_faces_edges_and_corners() {
        let close =
            |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4;
        let quarter = std::f32::consts::FRAC_PI_4;
        assert!(close(view_angles(Vec3::Z, 0.7), (0.0, 0.0)));
        assert!(close(view_angles(Vec3::X, 0.7), (2.0 * quarter, 0.0)));
        // From above the yaw is kept.
        assert!(close(view_angles(Vec3::Y, 0.7), (0.7, 1.55)));
        // The edge between the front and the top; the front, right, top corner.
        assert!(close(
            view_angles(Vec3::new(0.0, 1.0, 1.0), 0.7),
            (0.0, quarter)
        ));
        let corner = view_angles(Vec3::ONE, 0.7);
        assert!(close(corner, (quarter, (1.0f32 / 3.0f32.sqrt()).asin())));
        // The camera ends up looking from that direction.
        let cam = Camera {
            yaw: corner.0,
            pitch: corner.1,
            ..Camera::default()
        };
        assert!((cam.back() - Vec3::ONE.normalize()).length() < 1e-4);
    }

    #[test]
    fn quad_contains_either_winding() {
        let quad = [
            Pos2::new(0.0, 0.0),
            Pos2::new(10.0, 0.0),
            Pos2::new(10.0, 10.0),
            Pos2::new(0.0, 10.0),
        ];
        let mut reversed = quad;
        reversed.reverse();
        for q in [quad, reversed] {
            assert!(quad_contains(&q, Pos2::new(5.0, 5.0)));
            assert!(!quad_contains(&q, Pos2::new(12.0, 5.0)));
        }
    }
}
