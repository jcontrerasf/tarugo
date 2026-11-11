//! Free-form editor of a part: corners, notches, slots and holes placed with dimensions,
//! drawn on its face.

use eframe::egui::{self, Color32, Pos2, RichText, Sense, Shape as Paint, Stroke, Ui, Vec2};
use tarugo_core::analysis::face_size;
use tarugo_core::expr::format_number;
use tarugo_core::joints::other_axes;
use tarugo_core::model::{AXIS_NAMES, Dim};
use tarugo_core::shape::{Corner, Feature, Item, Outline, ProblemKind, Shape};

use crate::app::App;
use crate::i18n::{fill, t};
use crate::icons;
use crate::theme::{self, pal};
use crate::ui::dim_field;

/// Size of the drawing of the face.
const DRAWING: Vec2 = Vec2::new(330.0, 250.0);

fn problem_text(kind: &ProblemKind) -> &'static str {
    match kind {
        ProblemKind::BadDimension => t().shape_bad_dimension,
        ProblemKind::OutOfBounds => t().shape_out_of_bounds,
        ProblemKind::Overlap => t().shape_overlap,
    }
}

impl App {
    pub(crate) fn shape_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.shape_editor else {
            return;
        };
        let (Some(part), Some(b)) = (self.doc().part(id).cloned(), self.scene.get(id).copied())
        else {
            self.shape_editor = None;
            return;
        };
        let (Some(normal), Some((su, sv))) = (part.normal_axis(), face_size(&part, &b)) else {
            self.shape_editor = None;
            return;
        };
        let [i, j] = other_axes(normal);
        let solved = self.scene.solved.clone();
        let original = part.shape.clone().unwrap_or_default();
        let mut shape = original.clone();
        let outline = shape.outline(su, sv, &|d| solved.eval(d));
        let mut open = true;
        egui::Window::new(format!(
            "{} {} — {}",
            icons::SHAPE,
            t().free_form,
            part.name
        ))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(self.view_rect.left_top() + Vec2::new(24.0, 24.0))
        .show(ctx, |ui| {
            ui.label(theme::soft(fill(
                t().shape_intro,
                &[
                    &format_number(su),
                    &format_number(sv),
                    &AXIS_NAMES[i],
                    &AXIS_NAMES[j],
                ],
            )));
            ui.add_space(6.0);
            ui.horizontal_top(|ui| {
                draw_outline(ui, &outline, su, sv);
                ui.vertical(|ui| {
                    ui.set_width(330.0);
                    egui::ScrollArea::vertical()
                        .max_height(DRAWING.y + 60.0)
                        .show(ui, |ui| {
                            corner_rows(ui, &mut shape, &outline, &solved);
                            ui.add_space(8.0);
                            feature_rows(ui, &mut shape, &outline, &solved, su, sv);
                        });
                });
            });
            ui.add_space(6.0);
            ui.label(theme::soft(t().shape_note));
        });
        if shape != original {
            self.session.edit(|d| {
                if let Some(p) = d.part_mut(id) {
                    p.shape = (!shape.is_plain()).then_some(shape);
                }
            });
        }
        if !open {
            self.shape_editor = None;
        }
    }
}

fn draw_outline(ui: &mut Ui, outline: &Outline, su: f64, sv: f64) {
    let p = pal();
    let (rect, _) = ui.allocate_exact_size(DRAWING, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, p.backdrop);
    let inner = rect.shrink(18.0);
    let scale = (inner.width() / su as f32).min(inner.height() / sv as f32);
    let size = Vec2::new(su as f32, sv as f32) * scale;
    let origin = Pos2::new(
        inner.center().x - size.x / 2.0,
        inner.center().y + size.y / 2.0,
    );
    let to_screen = |q: &[f64; 2]| origin + Vec2::new(q[0] as f32, -(q[1] as f32)) * scale;
    // The cut rectangle, then the material: triangles, and the rings on top.
    painter.rect_stroke(
        egui::Rect::from_min_max(to_screen(&[0.0, sv]), to_screen(&[su, 0.0])),
        0.0,
        Stroke::new(1.0, p.text_soft.gamma_multiply(0.6)),
        egui::StrokeKind::Middle,
    );
    let wood = Color32::from_rgb(0xe3, 0xc7, 0x9a);
    let (points, triangles) = outline.triangulate();
    let mut mesh = egui::Mesh::default();
    for q in &points {
        mesh.colored_vertex(to_screen(q), wood);
    }
    for tri in &triangles {
        mesh.add_triangle(tri[0] as u32, tri[1] as u32, tri[2] as u32);
    }
    painter.add(Paint::mesh(mesh));
    for ring in outline.rings() {
        let screen: Vec<Pos2> = ring.iter().map(to_screen).collect();
        painter.add(Paint::closed_line(screen, Stroke::new(1.5, p.edge)));
    }
    painter.text(
        rect.left_bottom() + Vec2::new(6.0, -4.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{} × {} mm", format_number(su), format_number(sv)),
        egui::FontId::proportional(11.0),
        p.text_soft,
    );
}

fn problem_mark(ui: &mut Ui, outline: &Outline, item: Item) {
    if let Some(problem) = outline.problems.iter().find(|q| q.item == item) {
        ui.label(RichText::new(icons::ERROR).color(pal().error))
            .on_hover_text(problem_text(&problem.kind));
    }
}

fn corner_rows(
    ui: &mut Ui,
    shape: &mut Shape,
    outline: &Outline,
    solved: &tarugo_core::solve::Solved,
) {
    theme::section_title(ui, t().corners);
    let names = [
        t().corner_bottom_left,
        t().corner_bottom_right,
        t().corner_top_right,
        t().corner_top_left,
    ];
    egui::Grid::new("corners")
        .num_columns(4)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            for (n, name) in names.iter().enumerate() {
                ui.label(*name);
                let kind = match &shape.corners[n] {
                    Corner::Square => 0,
                    Corner::Round { .. } => 1,
                    Corner::Chamfer { .. } => 2,
                };
                let mut new_kind = kind;
                egui::ComboBox::from_id_salt(("corner", n))
                    .width(90.0)
                    .selected_text([t().corner_square, t().corner_round, t().corner_chamfer][kind])
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut new_kind, 0, t().corner_square);
                        ui.selectable_value(&mut new_kind, 1, t().corner_round);
                        ui.selectable_value(&mut new_kind, 2, t().corner_chamfer);
                    });
                if new_kind != kind {
                    // Keep the size when switching between round and chamfer.
                    let size = match &shape.corners[n] {
                        Corner::Round { radius } => radius.clone(),
                        Corner::Chamfer { size } => size.clone(),
                        Corner::Square => Dim::num(20.0),
                    };
                    shape.corners[n] = match new_kind {
                        1 => Corner::Round { radius: size },
                        2 => Corner::Chamfer { size },
                        _ => Corner::Square,
                    };
                }
                match &mut shape.corners[n] {
                    Corner::Square => {
                        ui.label("");
                    }
                    Corner::Round { radius: dim } | Corner::Chamfer { size: dim } => {
                        if let Some(new) = dim_field(ui, ("corner_dim", n), dim, solved, 60.0) {
                            *dim = new;
                        }
                    }
                }
                problem_mark(ui, outline, Item::Corner(n));
                ui.end_row();
            }
        });
}

/// A labelled dimension field of a feature.
fn labelled(
    ui: &mut Ui,
    label: &str,
    salt: (usize, usize),
    dim: &mut Dim,
    solved: &tarugo_core::solve::Solved,
) {
    ui.label(theme::soft(label));
    if let Some(new) = dim_field(ui, ("feature", salt), dim, solved, 50.0) {
        *dim = new;
    }
}

fn feature_rows(
    ui: &mut Ui,
    shape: &mut Shape,
    outline: &Outline,
    solved: &tarugo_core::solve::Solved,
    su: f64,
    sv: f64,
) {
    theme::section_title(ui, t().features);
    let mut delete = None;
    for (n, feature) in shape.features.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| match feature {
            Feature::Notch {
                edge,
                pos,
                width,
                depth,
                radius,
            } => {
                ui.label(theme::strong(t().notch));
                let edges = [t().side_bottom, t().side_right, t().side_top, t().side_left];
                egui::ComboBox::from_id_salt(("edge", n))
                    .width(70.0)
                    .selected_text(edges[*edge % 4])
                    .show_ui(ui, |ui| {
                        for (k, name) in edges.iter().enumerate() {
                            ui.selectable_value(edge, k, *name);
                        }
                    });
                labelled(ui, t().feature_pos, (n, 0), pos, solved);
                labelled(ui, t().feature_width, (n, 1), width, solved);
                labelled(ui, t().feature_depth, (n, 2), depth, solved);
                labelled(ui, t().feature_radius, (n, 3), radius, solved);
            }
            Feature::Slot {
                x,
                y,
                length,
                width,
                vertical,
            } => {
                ui.label(theme::strong(t().slot));
                labelled(ui, "x", (n, 0), x, solved);
                labelled(ui, "y", (n, 1), y, solved);
                labelled(ui, t().feature_length, (n, 2), length, solved);
                labelled(ui, t().feature_width, (n, 3), width, solved);
                ui.checkbox(vertical, t().vertical);
            }
            Feature::Hole { x, y, diameter } => {
                ui.label(theme::strong(t().hole));
                labelled(ui, "x", (n, 0), x, solved);
                labelled(ui, "y", (n, 1), y, solved);
                labelled(ui, t().feature_diameter, (n, 2), diameter, solved);
            }
        });
        ui.horizontal(|ui| {
            problem_mark(ui, outline, Item::Feature(n));
            if let Some(problem) = outline.problems.iter().find(|q| q.item == Item::Feature(n)) {
                ui.label(
                    RichText::new(problem_text(&problem.kind))
                        .color(pal().error)
                        .size(11.5),
                );
            }
            if theme::mini(ui, false, icons::DELETE, t().delete).clicked() {
                delete = Some(n);
            }
        });
        ui.separator();
    }
    if let Some(n) = delete {
        shape.features.remove(n);
    }
    ui.horizontal(|ui| {
        if ui.button(format!("{} {}", icons::ADD, t().notch)).clicked() {
            shape.features.push(Feature::Notch {
                edge: 0,
                pos: Dim::num((su / 2.0 - 25.0).max(0.0).round()),
                width: Dim::num(50.0_f64.min(su / 2.0).round()),
                depth: Dim::num(20.0_f64.min(sv / 3.0).round()),
                radius: Dim::zero(),
            });
        }
        if ui.button(format!("{} {}", icons::ADD, t().slot)).clicked() {
            shape.features.push(Feature::Slot {
                x: Dim::num((su / 2.0).round()),
                y: Dim::num((sv / 2.0).round()),
                length: Dim::num(60.0_f64.min(su / 2.0).round()),
                width: Dim::num(8.0),
                vertical: false,
            });
        }
        if ui.button(format!("{} {}", icons::ADD, t().hole)).clicked() {
            shape.features.push(Feature::Hole {
                x: Dim::num((su / 2.0).round()),
                y: Dim::num((sv / 2.0).round()),
                diameter: Dim::num(35.0_f64.min(su.min(sv) / 2.0).round()),
            });
        }
    });
}
