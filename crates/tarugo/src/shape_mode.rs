//! Free-form mode: the view shows the face of one part, flat, and its figures are drawn
//! and handled with the mouse. The bars and the panel of this mode are in
//! `ui/shape_editor.rs`.

use eframe::egui::{
    self, Align2, Color32, FontId, PointerButton, Pos2, Rect, Response, Sense, Shape as Paint,
    Stroke, Ui, Vec2,
};
use tarugo_core::analysis::face_size;
use tarugo_core::expr::format_number;
use tarugo_core::joints::other_axes;
use tarugo_core::model::{Dim, Document, PartId};
use tarugo_core::shape::{self, Feature, Guide, Handle, Outline, Point, Shape, Vertex};
use tarugo_core::solve::Solved;

use crate::app::App;
use crate::i18n::t;
use crate::theme::pal;
use crate::ui::shape_editor::edge_names;

/// How close (in points) the pointer picks a handle or snaps to a line.
const GRAB: f32 = 8.0;
const SNAP: f32 = 6.0;
/// Width of a slot drawn with the mouse (mm).
const SLOT_WIDTH: f64 = 8.0;
/// Step of the angles a polygon side locks to with Ctrl or Shift (degrees).
const ANGLE_STEP: f64 = 15.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeTool {
    Select,
    Rect,
    Circle,
    Slot,
    Polygon,
    Notch,
    /// Places guide lines.
    Guide,
}

impl ShapeTool {
    pub fn hint(self) -> &'static str {
        match self {
            ShapeTool::Select => t().hint_shape_select,
            ShapeTool::Rect => t().hint_shape_rect,
            ShapeTool::Circle => t().hint_shape_circle,
            ShapeTool::Slot => t().hint_shape_slot,
            ShapeTool::Polygon => t().hint_shape_polygon,
            ShapeTool::Notch => t().hint_shape_notch,
            ShapeTool::Guide => t().hint_shape_guide,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Corner(usize),
    Feature(usize),
    /// A vertex of a polygon: feature, vertex.
    Vertex(usize, usize),
    Guide(usize),
}

/// What the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Handle(usize, Handle, Point),
    Corner(usize),
    Feature(usize),
    Guide(usize),
}

enum Grab {
    Guide(usize),
    Move(usize),
    Handle(usize, Handle),
    Corner(usize),
    /// A figure being drawn from where the button went down.
    Create,
}

/// A drag in progress: every step starts again from the shape as it was.
struct Gesture {
    grab: Grab,
    original: Shape,
    start: Point,
    /// Box around the figure being moved.
    bounds: (Point, Point),
    /// Lines to snap to, per axis.
    targets: [Vec<f64>; 2],
}

pub struct ShapeEdit {
    pub part: PartId,
    pub tool: ShapeTool,
    pub selected: Option<Pick>,
    /// Point of the face at the middle of the view, and points per millimetre.
    center: Point,
    scale: f32,
    pub fit_pending: bool,
    /// What the guide tool places: 0 vertical lines, 1 horizontal ones.
    pub guide_axis: usize,
    gesture: Option<Gesture>,
    /// Points of the polygon being drawn.
    drawing: Vec<Point>,
    /// Lines the gesture snapped to in this frame: axis and value.
    guides: Vec<(usize, f64)>,
}

impl ShapeEdit {
    fn new(part: PartId) -> ShapeEdit {
        ShapeEdit {
            part,
            tool: ShapeTool::Select,
            selected: None,
            center: [0.0, 0.0],
            scale: 1.0,
            fit_pending: true,
            gesture: None,
            drawing: Vec::new(),
            guide_axis: 0,
            guides: Vec::new(),
        }
    }

    pub fn set_tool(&mut self, tool: ShapeTool) {
        self.tool = tool;
        self.drawing.clear();
    }

    pub fn selected_feature(&self) -> Option<usize> {
        match self.selected {
            Some(Pick::Feature(f) | Pick::Vertex(f, _)) => Some(f),
            _ => None,
        }
    }

    /// Drops a selection that no longer exists (after an undo, a delete).
    fn sanitize(&mut self, shape: &Shape) {
        let ok = match self.selected {
            None | Some(Pick::Corner(_)) => true,
            Some(Pick::Feature(f)) => f < shape.features.len(),
            Some(Pick::Guide(n)) => n < shape.guides.len(),
            Some(Pick::Vertex(f, v)) => matches!(
                shape.features.get(f),
                Some(Feature::Polygon { points }) if v < points.len()
            ),
        };
        if !ok {
            self.selected = None;
        }
    }
}

/// The face being edited, as the document has it now.
pub struct Face {
    pub id: PartId,
    pub name: String,
    pub su: f64,
    pub sv: f64,
    /// Axes of the furniture across and up the drawing.
    pub axes: [usize; 2],
    pub shape: Shape,
    pub outline: Outline,
    pub solved: Solved,
    pub color: Color32,
}

impl Face {
    fn eval(&self) -> impl Fn(&Dim) -> Result<f64, tarugo_core::expr::ExprError> + '_ {
        |d| self.solved.eval(d)
    }

    fn ring(&self, feature: usize) -> Option<&Vec<Point>> {
        self.outline
            .cutters
            .iter()
            .find(|(i, _)| *i == feature)
            .map(|(_, r)| r)
    }

    /// Where a guide line is.
    fn guide(&self, n: usize) -> Option<(usize, f64)> {
        let g = self.shape.guides.get(n)?;
        Some((g.axis % 2, self.solved.eval(&g.at).ok()?))
    }

    /// Lines worth snapping to on each axis: the guide lines (but `skip_guide`), and the
    /// edges and middle of the face and of every figure but `skip`.
    fn targets(&self, skip: Option<usize>, skip_guide: Option<usize>) -> [Vec<f64>; 2] {
        let mut out = [
            vec![0.0, self.su / 2.0, self.su],
            vec![0.0, self.sv / 2.0, self.sv],
        ];
        for n in 0..self.shape.guides.len() {
            if Some(n) != skip_guide
                && let Some((axis, at)) = self.guide(n)
            {
                out[axis].push(at);
            }
        }
        for (i, ring) in &self.outline.cutters {
            if Some(*i) == skip {
                continue;
            }
            let (min, max) = shape::ring_bounds(ring);
            for a in 0..2 {
                out[a].extend([min[a], (min[a] + max[a]) / 2.0, max[a]]);
            }
        }
        out
    }
}

/// Where the face is on the screen.
#[derive(Clone, Copy)]
struct View {
    rect: Rect,
    center: Point,
    scale: f32,
}

impl View {
    fn to_screen(self, p: Point) -> Pos2 {
        self.rect.center()
            + Vec2::new(
                (p[0] - self.center[0]) as f32,
                -((p[1] - self.center[1]) as f32),
            ) * self.scale
    }

    fn to_face(self, pos: Pos2) -> Point {
        let d = (pos - self.rect.center()) / self.scale;
        [self.center[0] + d.x as f64, self.center[1] - d.y as f64]
    }
}

/// The figure a tool draws from `a` to `b`, if it is big enough to be one.
fn drawn(tool: ShapeTool, a: Point, b: Point) -> Option<Feature> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    match tool {
        ShapeTool::Rect if dx.abs() >= 1.0 && dy.abs() >= 1.0 => Some(Feature::Rect {
            x: Dim::num(a[0].min(b[0])),
            y: Dim::num(a[1].min(b[1])),
            width: Dim::num(dx.abs()),
            height: Dim::num(dy.abs()),
            radius: Dim::zero(),
        }),
        ShapeTool::Circle if dx.hypot(dy) >= 1.0 => Some(Feature::Hole {
            x: Dim::num(a[0]),
            y: Dim::num(a[1]),
            diameter: Dim::num((2.0 * dx.hypot(dy)).round()),
        }),
        ShapeTool::Slot if dx.abs().max(dy.abs()) > SLOT_WIDTH => {
            let vertical = dy.abs() > dx.abs();
            let (x, y) = if vertical {
                (a[0], (a[1] + b[1]) / 2.0)
            } else {
                ((a[0] + b[0]) / 2.0, a[1])
            };
            Some(Feature::Slot {
                x: Dim::num(x),
                y: Dim::num(y),
                length: Dim::num(dx.abs().max(dy.abs())),
                width: Dim::num(SLOT_WIDTH),
                vertical,
            })
        }
        _ => None,
    }
}

/// The figure a tool leaves with a single click at `at`.
fn stamped(tool: ShapeTool, at: Point, su: f64, sv: f64) -> Option<Feature> {
    let small = su.min(sv);
    match tool {
        ShapeTool::Rect => {
            let (w, h) = ((su / 3.0).min(60.0).round(), (sv / 3.0).min(40.0).round());
            Some(Feature::Rect {
                x: Dim::num((at[0] - w / 2.0).round()),
                y: Dim::num((at[1] - h / 2.0).round()),
                width: Dim::num(w),
                height: Dim::num(h),
                radius: Dim::zero(),
            })
        }
        ShapeTool::Circle => Some(Feature::Hole {
            x: Dim::num(at[0]),
            y: Dim::num(at[1]),
            diameter: Dim::num((small / 2.0).min(35.0).round()),
        }),
        ShapeTool::Slot => Some(Feature::Slot {
            x: Dim::num(at[0]),
            y: Dim::num(at[1]),
            length: Dim::num((su / 2.0).clamp(SLOT_WIDTH, 60.0).round()),
            width: Dim::num(SLOT_WIDTH),
            vertical: false,
        }),
        ShapeTool::Notch => {
            // On the edge closest to the click.
            let gaps = [
                at[1].abs(),
                (su - at[0]).abs(),
                (sv - at[1]).abs(),
                at[0].abs(),
            ];
            let edge = (0..4).min_by(|a, b| gaps[*a].total_cmp(&gaps[*b]))?;
            let (len, across) = if edge % 2 == 0 { (su, sv) } else { (sv, su) };
            let width = (len / 2.0).min(50.0).round();
            let pos = (at[edge % 2] - width / 2.0).clamp(0.0, len - width).round();
            Some(Feature::Notch {
                edge,
                pos: Dim::num(pos),
                width: Dim::num(width),
                depth: Dim::num((across / 3.0).min(20.0).round()),
                radius: Dim::zero(),
            })
        }
        _ => None,
    }
}

impl App {
    /// Opens the free form of a part in the view.
    pub fn enter_shape(&mut self, id: PartId) {
        if self.is_locked(id) {
            self.warn_locked(id);
            return;
        }
        let has_face = self
            .doc()
            .part(id)
            .is_some_and(|p| p.normal_axis().is_some());
        if !has_face || self.scene.get(id).is_none() {
            return;
        }
        self.cancel_gesture();
        self.message = None;
        self.shape_editor = Some(ShapeEdit::new(id));
    }

    /// Back to the 3D design.
    pub fn leave_shape(&mut self) {
        if let Some(ed) = self.shape_editor.take()
            && ed.gesture.is_some()
        {
            self.session.cancel_group();
        }
        self.session.end_group();
    }

    pub(crate) fn shape_face(&self) -> Option<Face> {
        let id = self.shape_editor.as_ref()?.part;
        self.face_of(id)
    }

    fn face_of(&self, id: PartId) -> Option<Face> {
        let doc = self.doc();
        let part = doc.part(id)?;
        let b = self.scene.get(id)?;
        let normal = part.normal_axis()?;
        let (su, sv) = face_size(part, b)?;
        let solved = self.scene.solved.clone();
        let shape = part.shape.clone().unwrap_or_default();
        let outline = shape.outline(su, sv, &|d| solved.eval(d));
        let c = doc.material(part.material).map_or([200; 3], |m| m.color);
        Some(Face {
            id,
            name: part.name.clone(),
            su,
            sv,
            axes: other_axes(normal),
            shape,
            outline,
            solved,
            color: Color32::from_rgb(c[0], c[1], c[2]),
        })
    }

    /// Stores the shape of a part. `group`: the gesture this is a step of, undone as a whole.
    pub(crate) fn set_shape(&mut self, id: PartId, shape: Shape, group: Option<&str>) {
        let current = self
            .doc()
            .part(id)
            .map(|p| p.shape.clone().unwrap_or_default());
        if current.as_ref() == Some(&shape) {
            return;
        }
        let apply = |d: &mut Document| {
            if let Some(p) = d.part_mut(id) {
                p.shape = (!shape.is_empty()).then_some(shape);
            }
        };
        if let Some(key) = group {
            self.session.edit_grouped(key, apply);
        } else {
            self.session.edit(apply);
        }
        self.refresh_scene();
    }

    /// Esc: drops the gesture or the drawing, then the selection, then leaves the mode.
    pub(crate) fn shape_escape(&mut self) {
        let Some(ed) = &mut self.shape_editor else {
            return;
        };
        if ed.gesture.take().is_some() {
            self.session.cancel_group();
        } else if !ed.drawing.is_empty() {
            ed.drawing.clear();
        } else if ed.tool != ShapeTool::Select {
            ed.set_tool(ShapeTool::Select);
        } else if ed.selected.is_some() {
            ed.selected = None;
        } else {
            self.leave_shape();
        }
    }

    /// Delete: removes the selected figure or vertex, or squares the selected corner.
    pub(crate) fn shape_delete(&mut self) {
        let Some(face) = self.shape_face() else {
            return;
        };
        let Some(pick) = self.shape_editor.as_ref().and_then(|e| e.selected) else {
            return;
        };
        let mut shape = face.shape.clone();
        let mut selected = None;
        match pick {
            Pick::Corner(n) => {
                shape.corners[n % 4] = shape::Corner::Square;
                selected = Some(pick);
            }
            Pick::Guide(n) => {
                if n < shape.guides.len() {
                    shape.guides.remove(n);
                }
            }
            Pick::Vertex(f, v) if shape.remove_vertex(f, v) => selected = Some(Pick::Feature(f)),
            Pick::Feature(f) | Pick::Vertex(f, _) => {
                if f < shape.features.len() {
                    shape.features.remove(f);
                }
            }
        }
        if let Some(ed) = &mut self.shape_editor {
            ed.selected = selected;
        }
        self.set_shape(face.id, shape, None);
    }

    /// Enter: closes the polygon being drawn.
    pub(crate) fn shape_close_polygon(&mut self) {
        let Some(face) = self.shape_face() else {
            return;
        };
        if let Some(mut ed) = self.shape_editor.take() {
            self.close_polygon(&mut ed, &face);
            self.shape_editor = Some(ed);
        }
    }

    fn close_polygon(&mut self, ed: &mut ShapeEdit, face: &Face) {
        let points = std::mem::take(&mut ed.drawing);
        if points.len() < 3 {
            return;
        }
        let mut shape = face.shape.clone();
        shape.features.push(Feature::Polygon {
            points: points
                .iter()
                .map(|p| Vertex {
                    x: Dim::num((p[0] * 10.0).round() / 10.0),
                    y: Dim::num((p[1] * 10.0).round() / 10.0),
                    radius: Dim::zero(),
                })
                .collect(),
        });
        ed.selected = Some(Pick::Feature(shape.features.len() - 1));
        ed.tool = ShapeTool::Select;
        self.set_shape(face.id, shape, None);
    }

    pub(crate) fn shape_fit(&mut self) {
        if let Some(ed) = &mut self.shape_editor {
            ed.fit_pending = true;
        }
    }

    pub(crate) fn shape_canvas(&mut self, ui: &mut Ui) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        self.view_rect = rect;
        self.pointer_world = None;
        self.refresh_scene();
        let Some(face) = self.shape_face() else {
            self.leave_shape();
            return;
        };
        let Some(mut ed) = self.shape_editor.take() else {
            return;
        };
        ed.sanitize(&face.shape);
        if ed.fit_pending && rect.width() > 10.0 {
            ed.fit_pending = false;
            ed.center = [face.su / 2.0, face.sv / 2.0];
            let room = (rect.size() - Vec2::splat(96.0)).max(Vec2::splat(40.0));
            ed.scale = (room.x / face.su as f32).min(room.y / face.sv as f32);
        }
        // Wheel zooms at the pointer; middle or right button pans.
        if let Some(pos) = response.hover_pos() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = (scroll * 0.004).exp() * pinch;
            if (factor - 1.0).abs() > 1e-4 {
                let view = View {
                    rect,
                    center: ed.center,
                    scale: ed.scale,
                };
                let under = view.to_face(pos);
                ed.scale = (ed.scale * factor).clamp(0.02, 200.0);
                let d = (pos - rect.center()) / ed.scale;
                ed.center = [under[0] - d.x as f64, under[1] + d.y as f64];
            }
        }
        let mut pan = response.dragged_by(PointerButton::Middle)
            || response.dragged_by(PointerButton::Secondary);
        let view = View {
            rect,
            center: ed.center,
            scale: ed.scale,
        };
        let hover = self.shape_input(ui, &response, &mut ed, &face, &view, &mut pan);
        if pan {
            let d = response.drag_delta() / ed.scale;
            ed.center = [ed.center[0] - d.x as f64, ed.center[1] + d.y as f64];
        }
        // What the gestures of this frame left.
        let part = ed.part;
        if let Some(face) = self.face_of(part) {
            ed.sanitize(&face.shape);
            let view = View {
                rect,
                center: ed.center,
                scale: ed.scale,
            };
            let lock = angle_lock(ui);
            paint(ui, &ed, &face, &view, hover, response.hover_pos(), lock);
        }
        self.shape_editor = Some(ed);
    }

    /// Handles the primary button for the tool in use. Returns what the pointer is over.
    fn shape_input(
        &mut self,
        ui: &Ui,
        response: &Response,
        ed: &mut ShapeEdit,
        face: &Face,
        view: &View,
        pan: &mut bool,
    ) -> Option<Target> {
        let primary = PointerButton::Primary;
        let pos = response.interact_pointer_pos().or(response.hover_pos());
        let eval = face.eval();
        let lock = angle_lock(ui);
        let tolerance = (SNAP / view.scale) as f64;
        ed.guides.clear();

        if response.drag_started_by(primary) {
            let origin = ui.input(|i| i.pointer.press_origin()).or(pos);
            if let Some(origin) = origin {
                let at = view.to_face(origin);
                let mut original = face.shape.clone();
                let mut bounds = ([0.0; 2], [0.0; 2]);
                let mut skip = None;
                let mut skip_guide = None;
                let grab = match ed.tool {
                    ShapeTool::Select => match target_at(ed, face, view, origin) {
                        Some(Target::Handle(f, Handle::Midpoint(n), p)) => {
                            skip = Some(f);
                            original.insert_vertex(f, n, p).map(|v| {
                                ed.selected = Some(Pick::Vertex(f, v));
                                Grab::Handle(f, Handle::Vertex(v))
                            })
                        }
                        Some(Target::Handle(f, handle, _)) => {
                            skip = Some(f);
                            if let Handle::Vertex(v) = handle {
                                ed.selected = Some(Pick::Vertex(f, v));
                            }
                            Some(Grab::Handle(f, handle))
                        }
                        Some(Target::Corner(n)) => {
                            ed.selected = Some(Pick::Corner(n));
                            Some(Grab::Corner(n))
                        }
                        Some(Target::Guide(n)) => {
                            skip_guide = Some(n);
                            ed.selected = Some(Pick::Guide(n));
                            Some(Grab::Guide(n))
                        }
                        Some(Target::Feature(f)) => {
                            skip = Some(f);
                            ed.selected = Some(Pick::Feature(f));
                            if let Some(ring) = face.ring(f) {
                                bounds = shape::ring_bounds(ring);
                            }
                            Some(Grab::Move(f))
                        }
                        None => None,
                    },
                    ShapeTool::Rect | ShapeTool::Circle | ShapeTool::Slot => Some(Grab::Create),
                    ShapeTool::Polygon | ShapeTool::Notch | ShapeTool::Guide => None,
                };
                ed.gesture = grab.map(|grab| {
                    let targets = face.targets(skip, skip_guide);
                    let start = if matches!(grab, Grab::Create) {
                        snap_point(&mut ed.guides, &targets, tolerance, at)
                    } else {
                        at
                    };
                    Gesture {
                        grab,
                        original,
                        start,
                        bounds,
                        targets,
                    }
                });
            }
        }

        if response.dragged_by(primary) {
            if let (Some(g), Some(pos)) = (ed.gesture.take(), pos) {
                let at = view.to_face(pos);
                let mut shape = g.original.clone();
                match g.grab {
                    Grab::Move(f) => {
                        let delta = snap_move(&mut ed.guides, &g, tolerance, at);
                        shape.translate(f, delta, &eval);
                    }
                    Grab::Handle(f, handle) => {
                        // The rim of a hole and the width of a slot are distances.
                        let to = match handle {
                            Handle::Radius | Handle::SlotWidth => at,
                            // Locked to the angle of the side that arrives at the vertex.
                            Handle::Vertex(v) if lock => {
                                match vertex_before(&g.original, f, v, &eval) {
                                    Some(from) => shape::snap_angle(from, at, ANGLE_STEP),
                                    None => at,
                                }
                            }
                            _ => snap_point(&mut ed.guides, &g.targets, tolerance, at),
                        };
                        shape.drag_handle(f, handle, to, face.su, face.sv, &eval);
                    }
                    Grab::Corner(n) => shape.drag_corner(n, at, face.su, face.sv),
                    Grab::Guide(n) => {
                        let to = snap_point(&mut ed.guides, &g.targets, tolerance, at);
                        if let Some(guide) = shape.guides.get_mut(n) {
                            guide.at = Dim::num(to[guide.axis % 2]);
                        }
                    }
                    Grab::Create => {
                        let to = snap_point(&mut ed.guides, &g.targets, tolerance, at);
                        if let Some(feature) = drawn(ed.tool, g.start, to) {
                            shape.features.push(feature);
                        }
                    }
                }
                self.set_shape(face.id, shape, Some("shape"));
                ed.gesture = Some(g);
            } else if ed.gesture.is_none() && ed.tool == ShapeTool::Select {
                // Dragging the empty view moves it.
                *pan = true;
            }
        }

        if response.drag_stopped_by(primary)
            && let Some(g) = ed.gesture.take()
        {
            if matches!(g.grab, Grab::Create) {
                let count = self
                    .doc()
                    .part(face.id)
                    .and_then(|p| p.shape.as_ref())
                    .map_or(0, |s| s.features.len());
                if count > g.original.features.len() {
                    ed.selected = Some(Pick::Feature(count - 1));
                    ed.tool = ShapeTool::Select;
                }
            }
            self.session.end_group();
        }

        if response.clicked_by(primary)
            && let Some(pos) = pos
        {
            let at = view.to_face(pos);
            let targets = face.targets(None, None);
            match ed.tool {
                ShapeTool::Select => {
                    ed.selected = match target_at(ed, face, view, pos) {
                        Some(Target::Handle(f, Handle::Vertex(v), _)) => Some(Pick::Vertex(f, v)),
                        Some(Target::Handle(f, ..)) => Some(Pick::Feature(f)),
                        Some(Target::Corner(n)) => Some(Pick::Corner(n)),
                        Some(Target::Feature(f)) => Some(Pick::Feature(f)),
                        Some(Target::Guide(n)) => Some(Pick::Guide(n)),
                        None => None,
                    };
                }
                ShapeTool::Polygon => {
                    let near = |p: &Point| view.to_screen(*p).distance(pos) <= GRAB;
                    if ed.drawing.len() >= 3 && ed.drawing.first().is_some_and(near) {
                        self.close_polygon(ed, face);
                    } else if !ed.drawing.last().is_some_and(near) {
                        let p = match ed.drawing.last() {
                            Some(last) if lock => shape::snap_angle(*last, at, ANGLE_STEP),
                            _ => snap_point(&mut ed.guides, &targets, tolerance, at),
                        };
                        ed.drawing.push(p);
                    }
                }
                ShapeTool::Guide => {
                    let at = snap_point(&mut ed.guides, &targets, tolerance, at);
                    let mut shape = face.shape.clone();
                    shape.guides.push(Guide {
                        axis: ed.guide_axis % 2,
                        at: Dim::num(at[ed.guide_axis % 2]),
                    });
                    ed.selected = Some(Pick::Guide(shape.guides.len() - 1));
                    self.set_shape(face.id, shape, None);
                }
                tool => {
                    let at = snap_point(&mut ed.guides, &targets, tolerance, at);
                    if let Some(feature) = stamped(tool, at, face.su, face.sv) {
                        let mut shape = face.shape.clone();
                        shape.features.push(feature);
                        ed.selected = Some(Pick::Feature(shape.features.len() - 1));
                        ed.tool = ShapeTool::Select;
                        self.set_shape(face.id, shape, None);
                    }
                }
            }
        }
        if response.double_clicked_by(primary) && ed.tool == ShapeTool::Polygon {
            self.close_polygon(ed, face);
        }

        if ed.gesture.is_some() {
            return None;
        }
        response
            .hover_pos()
            .and_then(|p| target_at(ed, face, view, p))
    }
}

/// What the select tool finds under a point: a handle of the selected figure, the handle
/// of a corner or the topmost figure.
fn target_at(ed: &ShapeEdit, face: &Face, view: &View, pos: Pos2) -> Option<Target> {
    if ed.tool != ShapeTool::Select {
        return None;
    }
    let eval = face.eval();
    let near = |p: Point| {
        let d = view.to_screen(p).distance(pos);
        (d <= GRAB).then_some(d)
    };
    if let Some(f) = ed.selected_feature() {
        let best = face
            .shape
            .handles(f, face.su, face.sv, &eval)
            .into_iter()
            .filter_map(|(h, p)| near(p).map(|d| (d, h, p)))
            // A vertex wins over the middle of a side at the same distance.
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, h, p)) = best {
            return Some(Target::Handle(f, h, p));
        }
    }
    let corner = (0..4)
        .filter_map(|n| near(face.shape.corner_handle(n, face.su, face.sv, &eval)).map(|d| (d, n)))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, n)) = corner {
        return Some(Target::Corner(n));
    }
    let guide = (0..face.shape.guides.len())
        .filter_map(|n| {
            let (axis, at) = face.guide(n)?;
            let line = view.to_screen(if axis == 0 { [at, 0.0] } else { [0.0, at] });
            let d = if axis == 0 {
                (line.x - pos.x).abs()
            } else {
                (line.y - pos.y).abs()
            };
            (d <= GRAB / 2.0).then_some((d, n))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, n)) = guide {
        return Some(Target::Guide(n));
    }
    let at = view.to_face(pos);
    face.outline
        .cutters
        .iter()
        .rev()
        .find(|(_, ring)| shape::point_in_ring(ring, at))
        .map(|(i, _)| Target::Feature(*i))
}

/// Ctrl or Shift held: the sides of a polygon run at multiples of [`ANGLE_STEP`].
fn angle_lock(ui: &Ui) -> bool {
    ui.input(|i| i.modifiers.shift || i.modifiers.ctrl || i.modifiers.command)
}

/// Where the vertex before `vertex` of a polygon is.
fn vertex_before(
    shape: &Shape,
    feature: usize,
    vertex: usize,
    eval: &dyn Fn(&Dim) -> Result<f64, tarugo_core::expr::ExprError>,
) -> Option<Point> {
    let Some(Feature::Polygon { points }) = shape.features.get(feature) else {
        return None;
    };
    let before = points.get((vertex + points.len() - 1) % points.len())?;
    Some([eval(&before.x).ok()?, eval(&before.y).ok()?])
}

/// A point on whole millimetres, or on the lines close to it.
fn snap_point(
    guides: &mut Vec<(usize, f64)>,
    targets: &[Vec<f64>; 2],
    tolerance: f64,
    at: Point,
) -> Point {
    let mut out = at;
    for a in 0..2 {
        out[a] = match shape::snap(at[a], &targets[a], tolerance) {
            Some(line) => {
                guides.push((a, line));
                line
            }
            None => at[a].round(),
        };
    }
    out
}

/// How far the figure of a gesture has moved: whole millimetres, or what takes one of its
/// sides or its middle onto a line close to it.
fn snap_move(guides: &mut Vec<(usize, f64)>, g: &Gesture, tolerance: f64, at: Point) -> Point {
    let mut delta = [0.0; 2];
    for a in 0..2 {
        let raw = at[a] - g.start[a];
        let (min, max) = (g.bounds.0[a], g.bounds.1[a]);
        let best = [min, (min + max) / 2.0, max]
            .into_iter()
            .filter_map(|line| {
                shape::snap(line + raw, &g.targets[a], tolerance).map(|to| (to - line, to))
            })
            .min_by(|x, y| (x.0 - raw).abs().total_cmp(&(y.0 - raw).abs()));
        delta[a] = match best {
            Some((by, line)) => {
                guides.push((a, line));
                by
            }
            None => raw.round(),
        };
    }
    delta
}

/// A measure for a label: tenths are enough.
fn measure(v: f64) -> String {
    format_number((v * 10.0).round() / 10.0)
}

fn label(painter: &egui::Painter, pos: Pos2, align: Align2, text: String, color: Color32) {
    let galley = painter.layout_no_wrap(text, FontId::proportional(11.5), color);
    let rect = align.anchor_size(pos, galley.size());
    painter.rect_filled(rect.expand(2.0), 3.0, pal().backdrop.gamma_multiply(0.85));
    painter.galley(rect.min, galley, color);
}

fn paint(
    ui: &Ui,
    ed: &ShapeEdit,
    face: &Face,
    view: &View,
    hover: Option<Target>,
    pointer: Option<Pos2>,
    lock: bool,
) {
    let p = pal();
    let painter = ui.painter_at(view.rect);
    let screen = |q: &Point| view.to_screen(*q);
    let (su, sv) = (face.su, face.sv);

    // The rectangle that is cut, then the material and its rings.
    let whole = Rect::from_min_max(screen(&[0.0, sv]), screen(&[su, 0.0]));
    painter.rect_stroke(
        whole,
        0.0,
        Stroke::new(1.0, p.text_soft.gamma_multiply(0.6)),
        egui::StrokeKind::Middle,
    );
    let (points, triangles) = face.outline.triangulate();
    let mut mesh = egui::Mesh::default();
    for q in &points {
        mesh.colored_vertex(screen(q), face.color);
    }
    for tri in &triangles {
        mesh.add_triangle(tri[0] as u32, tri[1] as u32, tri[2] as u32);
    }
    painter.add(Paint::mesh(mesh));

    // Every figure whole, also where it is outside the part.
    let selected = ed.selected_feature();
    let hovered = match hover {
        Some(Target::Feature(f) | Target::Handle(f, ..)) => Some(f),
        _ => None,
    };
    for (i, ring) in &face.outline.cutters {
        if Some(*i) != selected {
            let stroke = if Some(*i) == hovered {
                Stroke::new(1.5, p.accent)
            } else {
                Stroke::new(1.0, p.text_soft.gamma_multiply(0.7))
            };
            painter.add(Paint::closed_line(
                ring.iter().map(screen).collect(),
                stroke,
            ));
        }
    }
    for ring in face.outline.rings() {
        painter.add(Paint::closed_line(
            ring.iter().map(screen).collect(),
            Stroke::new(1.5, p.edge),
        ));
    }

    // Guide lines; the one the guide tool would place follows the pointer.
    let guide_line = |axis: usize, at: f64, stroke: Stroke, name: bool| {
        let on = screen(&if axis == 0 { [at, 0.0] } else { [0.0, at] });
        let (a, b) = if axis == 0 {
            (
                Pos2::new(on.x, view.rect.top()),
                Pos2::new(on.x, view.rect.bottom()),
            )
        } else {
            (
                Pos2::new(view.rect.left(), on.y),
                Pos2::new(view.rect.right(), on.y),
            )
        };
        painter.extend(Paint::dashed_line(&[a, b], stroke, 6.0, 4.0));
        if name {
            let (at_pos, align) = if axis == 0 {
                (a + Vec2::new(4.0, 6.0), Align2::LEFT_TOP)
            } else {
                (a + Vec2::new(6.0, -4.0), Align2::LEFT_BOTTOM)
            };
            label(&painter, at_pos, align, format_number(at), stroke.color);
        }
    };
    for n in 0..face.shape.guides.len() {
        if let Some((axis, at)) = face.guide(n) {
            let hot = hover == Some(Target::Guide(n)) || ed.selected == Some(Pick::Guide(n));
            let stroke = if hot {
                Stroke::new(1.5, p.snap)
            } else {
                Stroke::new(1.0, p.snap.gamma_multiply(0.6))
            };
            guide_line(axis, at, stroke, hot);
        }
    }
    if ed.tool == ShapeTool::Guide
        && let Some(pos) = pointer
    {
        let axis = ed.guide_axis % 2;
        let at = view.to_face(pos)[axis].round();
        guide_line(axis, at, Stroke::new(1.0, p.snap), true);
    }

    // Where each edge of the drawing is on the furniture.
    let names = edge_names(face.axes);
    let soft = p.text_soft;
    let font = FontId::proportional(11.5);
    let gap = 8.0;
    painter.text(
        whole.center_bottom() + Vec2::new(0.0, gap),
        Align2::CENTER_TOP,
        names[0],
        font.clone(),
        soft,
    );
    painter.text(
        whole.right_center() + Vec2::new(gap, 0.0),
        Align2::LEFT_CENTER,
        names[1],
        font.clone(),
        soft,
    );
    painter.text(
        whole.center_top() - Vec2::new(0.0, gap),
        Align2::CENTER_BOTTOM,
        names[2],
        font.clone(),
        soft,
    );
    painter.text(
        whole.left_center() - Vec2::new(gap, 0.0),
        Align2::RIGHT_CENTER,
        names[3],
        font,
        soft,
    );
    painter.text(
        view.rect.left_bottom() + Vec2::new(10.0, -8.0),
        Align2::LEFT_BOTTOM,
        format!("{} × {} mm", format_number(su), format_number(sv)),
        FontId::proportional(11.5),
        soft,
    );

    // Lines the gesture snapped to.
    for (axis, value) in &ed.guides {
        let at = screen(&if *axis == 0 {
            [*value, 0.0]
        } else {
            [0.0, *value]
        });
        let stroke = Stroke::new(1.0, p.snap);
        if *axis == 0 {
            painter.vline(at.x, view.rect.y_range(), stroke);
        } else {
            painter.hline(view.rect.x_range(), at.y, stroke);
        }
    }

    // The selected figure: its outline, where it is, its size and its handles.
    if let Some(f) = selected {
        if let Some(ring) = face.ring(f) {
            painter.add(Paint::closed_line(
                ring.iter().map(screen).collect(),
                Stroke::new(2.0, p.accent),
            ));
            let (min, max) = shape::ring_bounds(ring);
            let mid = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
            let line = Stroke::new(1.0, p.cota);
            if min[0] > 0.5 {
                let (a, b) = (screen(&[0.0, mid[1]]), screen(&[min[0], mid[1]]));
                painter.line_segment([a, b], line);
                label(
                    &painter,
                    a.lerp(b, 0.5) - Vec2::new(0.0, 4.0),
                    Align2::CENTER_BOTTOM,
                    measure(min[0]),
                    p.cota,
                );
            }
            if min[1] > 0.5 {
                let (a, b) = (screen(&[mid[0], 0.0]), screen(&[mid[0], min[1]]));
                painter.line_segment([a, b], line);
                label(
                    &painter,
                    a.lerp(b, 0.5) + Vec2::new(5.0, 0.0),
                    Align2::LEFT_CENTER,
                    measure(min[1]),
                    p.cota,
                );
            }
            label(
                &painter,
                screen(&[mid[0], max[1]]) - Vec2::new(0.0, 12.0),
                Align2::CENTER_BOTTOM,
                format!(
                    "{} × {}",
                    measure(max[0] - min[0]),
                    measure(max[1] - min[1])
                ),
                p.cota,
            );
        }
        if ed.tool == ShapeTool::Select {
            let eval = face.eval();
            for (handle, at) in face.shape.handles(f, su, sv, &eval) {
                let at = screen(&at);
                let hot = matches!(hover, Some(Target::Handle(_, h, _)) if h == handle);
                let picked = matches!(
                    (handle, ed.selected),
                    (Handle::Vertex(v), Some(Pick::Vertex(_, s))) if v == s
                );
                let fill = if picked || hot { p.accent } else { p.window };
                if matches!(handle, Handle::Midpoint(_)) {
                    painter.circle(at, 3.0, fill, Stroke::new(1.0, p.accent));
                } else {
                    let square = Rect::from_center_size(at, Vec2::splat(8.0));
                    painter.rect(
                        square,
                        1.5,
                        fill,
                        Stroke::new(1.5, p.accent),
                        egui::StrokeKind::Middle,
                    );
                }
            }
        }
    }

    // Handles of the corners.
    if ed.tool == ShapeTool::Select {
        let eval = face.eval();
        for n in 0..4 {
            let at = screen(&face.shape.corner_handle(n, su, sv, &eval));
            let hot = hover == Some(Target::Corner(n)) || ed.selected == Some(Pick::Corner(n));
            let fill = if hot { p.cota } else { p.window };
            painter.circle(at, 4.5, fill, Stroke::new(1.5, p.cota));
        }
    }

    // The polygon being drawn, up to the pointer.
    if !ed.drawing.is_empty() {
        let mut line: Vec<Pos2> = ed.drawing.iter().map(screen).collect();
        if let (Some(pos), Some(last)) = (pointer, ed.drawing.last()) {
            // With the angle locked the next side shows where it would go.
            line.push(if lock {
                screen(&shape::snap_angle(*last, view.to_face(pos), ANGLE_STEP))
            } else {
                pos
            });
        }
        painter.add(Paint::line(line.clone(), Stroke::new(1.5, p.accent)));
        for (n, at) in line.iter().take(ed.drawing.len()).enumerate() {
            let r = if n == 0 { 5.0 } else { 3.0 };
            painter.circle(*at, r, p.window, Stroke::new(1.5, p.accent));
        }
    }
}
