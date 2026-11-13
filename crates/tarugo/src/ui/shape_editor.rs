//! Bars and panel of the free-form mode (see `shape_mode.rs`): tools, the list of figures
//! and the dimensions of the selected one, and the corners.

use eframe::egui::{self, RichText, Ui};
use tarugo_core::expr::format_number;
use tarugo_core::model::{AXIS_NAMES, Dim, Side};
use tarugo_core::shape::{Corner, Feature, Item, Outline, ProblemKind, Shape};
use tarugo_core::solve::Solved;

use crate::app::App;
use crate::i18n::{fill, t};
use crate::icons;
use crate::shape_mode::{Pick, ShapeTool};
use crate::theme::{self, pal};
use crate::ui::dim_drag;
use crate::ui::properties::side_name;

fn problem_text(kind: &ProblemKind) -> &'static str {
    match kind {
        ProblemKind::BadDimension => t().shape_bad_dimension,
        ProblemKind::OutOfBounds => t().shape_out_of_bounds,
        ProblemKind::SelfIntersecting => t().shape_self_intersecting,
        ProblemKind::Splits => t().shape_splits,
    }
}

fn problem_mark(ui: &mut Ui, outline: &Outline, item: Item) {
    if let Some(problem) = outline.problems.iter().find(|q| q.item == item) {
        ui.label(RichText::new(icons::ERROR).color(pal().error))
            .on_hover_text(problem_text(&problem.kind));
    }
}

/// Icon and name of a figure.
fn feature_name(feature: &Feature) -> (&'static str, &'static str) {
    match feature {
        Feature::Notch { .. } => (icons::NOTCH, t().notch),
        Feature::Slot { .. } => (icons::SLOT, t().slot),
        Feature::Hole { .. } => (icons::CIRCLE, t().hole),
        Feature::Rect { .. } => (icons::RECTANGLE, t().rectangle),
        Feature::Polygon { .. } => (icons::POLYGON, t().polygon),
    }
}

/// Names of the four edges of the face (in outline order) by where they are on the part:
/// the face spans the axes `[i, j]`.
pub(crate) fn edge_names(axes: [usize; 2]) -> [&'static str; 4] {
    let [i, j] = axes;
    [
        side_name(j, Side::Min),
        side_name(i, Side::Max),
        side_name(j, Side::Max),
        side_name(i, Side::Min),
    ]
}

impl App {
    pub(crate) fn shape_context_bar(&mut self, ui: &mut Ui) {
        ui.set_min_height(30.0);
        let Some(face) = self.shape_face() else {
            return;
        };
        let mut leave = false;
        let mut delete = false;
        ui.horizontal_wrapped(|ui| {
            if theme::primary_button(ui, icons::OK, t().done)
                .on_hover_text(t().done_help)
                .clicked()
            {
                leave = true;
            }
            ui.label(theme::strong(format!(
                "{} {} — {}",
                icons::SHAPE,
                t().free_form,
                face.name
            )));
            if let Some(ed) = &mut self.shape_editor
                && ed.tool == ShapeTool::Guide
            {
                ui.separator();
                ui.selectable_value(&mut ed.guide_axis, 0, t().vertical);
                ui.selectable_value(&mut ed.guide_axis, 1, t().horizontal);
            }
            let selected = self.shape_editor.as_ref().and_then(|e| e.selected);
            if selected.is_some() {
                ui.separator();
                if ui
                    .button(format!("{} {}", icons::DELETE, t().delete))
                    .on_hover_text("Supr / Del")
                    .clicked()
                {
                    delete = true;
                }
            }
        });
        if delete {
            self.shape_delete();
        }
        if leave {
            self.leave_shape();
        }
    }

    pub(crate) fn shape_tools_bar(&mut self, ui: &mut Ui) {
        let tools = [
            (ShapeTool::Select, icons::SELECT, t().tool_select),
            (ShapeTool::Rect, icons::RECTANGLE, t().rectangle),
            (ShapeTool::Circle, icons::CIRCLE, t().hole),
            (ShapeTool::Slot, icons::SLOT, t().slot),
            (ShapeTool::Polygon, icons::POLYGON, t().polygon),
            (ShapeTool::Notch, icons::NOTCH, t().notch),
            (ShapeTool::Guide, icons::GUIDE, t().guide_line),
        ];
        ui.spacing_mut().item_spacing.y = 4.0;
        let current = self.shape_editor.as_ref().map(|e| e.tool);
        for (tool, ico, name) in tools {
            if theme::toggle(ui, current == Some(tool), ico, name).clicked()
                && let Some(ed) = &mut self.shape_editor
            {
                ed.set_tool(tool);
            }
        }
        ui.separator();
        if theme::icon_button(ui, icons::FIT, &format!("{}  (F)", t().fit_view)).clicked() {
            self.shape_fit();
        }
    }

    pub(crate) fn shape_panel(&mut self, ui: &mut Ui) {
        let Some(face) = self.shape_face() else {
            return;
        };
        let mut selected = self.shape_editor.as_ref().and_then(|e| e.selected);
        let mut shape = face.shape.clone();
        let outline = &face.outline;
        let solved = &face.solved;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(8.0);
                if let Some(problem) = outline.problems.iter().find(|q| q.item == Item::Whole) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(icons::ERROR).color(pal().error));
                        ui.label(RichText::new(problem_text(&problem.kind)).color(pal().error));
                    });
                    ui.add_space(6.0);
                }
                theme::section_title(ui, t().figures);
                if shape.features.is_empty() {
                    ui.label(theme::soft(t().no_figures));
                }
                let mut delete = None;
                for (n, feature) in shape.features.iter().enumerate() {
                    let (ico, name) = feature_name(feature);
                    let active = matches!(
                        selected,
                        Some(Pick::Feature(f) | Pick::Vertex(f, _)) if f == n
                    );
                    ui.horizontal(|ui| {
                        if theme::mini(ui, false, icons::DELETE, t().delete).clicked() {
                            delete = Some(n);
                        }
                        let text = format!("{ico}  {name} {}", n + 1);
                        if ui.selectable_label(active, text).clicked() {
                            selected = Some(Pick::Feature(n));
                        }
                        if let Some(problem) =
                            outline.problems.iter().find(|q| q.item == Item::Feature(n))
                        {
                            ui.label(RichText::new(icons::ERROR).color(pal().error));
                            ui.label(
                                RichText::new(problem_text(&problem.kind))
                                    .color(pal().error)
                                    .size(11.5),
                            );
                        }
                    });
                }
                if let Some(n) = delete {
                    shape.features.remove(n);
                    selected = None;
                }
                if let Some(Pick::Feature(n) | Pick::Vertex(n, _)) = selected
                    && n < shape.features.len()
                {
                    ui.add_space(10.0);
                    let (_, name) = feature_name(&shape.features[n]);
                    theme::section_title(ui, &format!("{name} {}", n + 1));
                    let vertex = match selected {
                        Some(Pick::Vertex(_, v)) => Some(v),
                        _ => None,
                    };
                    if let Some(v) =
                        feature_fields(ui, n, &mut shape.features[n], solved, face.axes, vertex)
                    {
                        selected = Some(Pick::Vertex(n, v));
                    }
                }
                ui.add_space(10.0);
                corner_rows(ui, &mut shape, outline, solved, face.axes, selected);
                if !shape.guides.is_empty() {
                    ui.add_space(10.0);
                    theme::section_title(ui, t().guides);
                    let mut delete = None;
                    for (n, guide) in shape.guides.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            if theme::mini(ui, false, icons::DELETE, t().delete).clicked() {
                                delete = Some(n);
                            }
                            let name = if guide.axis % 2 == 0 {
                                format!("{}  x", t().vertical)
                            } else {
                                format!("{}  y", t().horizontal)
                            };
                            let active = selected == Some(Pick::Guide(n));
                            if ui.selectable_label(active, name).clicked() {
                                selected = Some(Pick::Guide(n));
                            }
                            let min = f64::NEG_INFINITY;
                            if let Some(new) =
                                dim_drag(ui, ("guide", n), &guide.at, solved, 70.0, min)
                            {
                                guide.at = new;
                            }
                        });
                    }
                    if let Some(n) = delete {
                        shape.guides.remove(n);
                        selected = None;
                    }
                }
                ui.add_space(10.0);
                let [i, j] = face.axes;
                ui.label(theme::soft(fill(
                    t().shape_intro,
                    &[
                        &format_number(face.su),
                        &format_number(face.sv),
                        &AXIS_NAMES[i],
                        &AXIS_NAMES[j],
                    ],
                )));
                ui.label(theme::soft(t().shape_note));
                ui.add_space(12.0);
            });
        if let Some(ed) = &mut self.shape_editor {
            ed.selected = selected;
        }
        if shape != face.shape {
            // Dragging a value is one step to undo.
            let dragging = ui.input(|i| i.pointer.any_down());
            self.set_shape(face.id, shape, dragging.then_some("shape_field"));
        }
    }
}

fn corner_rows(
    ui: &mut Ui,
    shape: &mut Shape,
    outline: &Outline,
    solved: &Solved,
    axes: [usize; 2],
    selected: Option<Pick>,
) {
    theme::section_title(ui, t().corners);
    // Each corner is where two edges meet: the one before it and the one it starts.
    // Named along the second axis first: "back · left", "front · right".
    let edges = edge_names(axes);
    let names: [String; 4] = std::array::from_fn(|n| {
        let (before, after) = (edges[(n + 3) % 4], edges[n]);
        if n % 2 == 0 {
            format!("{after} · {before}")
        } else {
            format!("{before} · {after}")
        }
    });
    egui::Grid::new("corners")
        .num_columns(4)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            for (n, name) in names.iter().enumerate() {
                if selected == Some(Pick::Corner(n)) {
                    ui.label(theme::strong(name.as_str()).color(pal().accent));
                } else {
                    ui.label(name.as_str());
                }
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
                        if let Some(new) = dim_drag(ui, ("corner_dim", n), dim, solved, 60.0, 0.0) {
                            *dim = new;
                        }
                    }
                }
                problem_mark(ui, outline, Item::Corner(n));
                ui.end_row();
            }
        });
}

/// A row of a grid: the name of a dimension and its field.
fn row(ui: &mut Ui, label: &str, salt: (usize, usize), dim: &mut Dim, solved: &Solved) {
    ui.label(theme::soft(label));
    // Places may be anywhere; sizes and radii are not negative.
    let min = if label == "x" || label == "y" || label == t().feature_pos {
        f64::NEG_INFINITY
    } else {
        0.0
    };
    if let Some(new) = dim_drag(ui, ("feature", salt), dim, solved, 90.0, min) {
        *dim = new;
    }
    ui.end_row();
}

/// The dimensions of a figure. Returns the vertex of a polygon whose row was clicked.
fn feature_fields(
    ui: &mut Ui,
    n: usize,
    feature: &mut Feature,
    solved: &Solved,
    axes: [usize; 2],
    vertex: Option<usize>,
) -> Option<usize> {
    let mut picked = None;
    let grid = egui::Grid::new(("feature", n))
        .num_columns(2)
        .spacing([8.0, 4.0]);
    match feature {
        Feature::Notch {
            edge,
            pos,
            width,
            depth,
            radius,
        } => {
            grid.show(ui, |ui| {
                ui.label(theme::soft(t().feature_edge));
                let edges = edge_names(axes);
                egui::ComboBox::from_id_salt(("edge", n))
                    .width(90.0)
                    .selected_text(edges[*edge % 4])
                    .show_ui(ui, |ui| {
                        for (k, name) in edges.iter().enumerate() {
                            ui.selectable_value(edge, k, *name);
                        }
                    });
                ui.end_row();
                row(ui, t().feature_pos, (n, 0), pos, solved);
                row(ui, t().feature_width, (n, 1), width, solved);
                row(ui, t().feature_depth, (n, 2), depth, solved);
                row(ui, t().feature_radius, (n, 3), radius, solved);
            });
        }
        Feature::Slot {
            x,
            y,
            length,
            width,
            vertical,
        } => {
            grid.show(ui, |ui| {
                row(ui, "x", (n, 0), x, solved);
                row(ui, "y", (n, 1), y, solved);
                row(ui, t().feature_length, (n, 2), length, solved);
                row(ui, t().feature_width, (n, 3), width, solved);
            });
            ui.checkbox(vertical, t().vertical);
        }
        Feature::Hole { x, y, diameter } => {
            grid.show(ui, |ui| {
                row(ui, "x", (n, 0), x, solved);
                row(ui, "y", (n, 1), y, solved);
                row(ui, t().feature_diameter, (n, 2), diameter, solved);
            });
        }
        Feature::Rect {
            x,
            y,
            width,
            height,
            radius,
        } => {
            grid.show(ui, |ui| {
                row(ui, "x", (n, 0), x, solved);
                row(ui, "y", (n, 1), y, solved);
                row(ui, t().feature_width, (n, 2), width, solved);
                row(ui, t().feature_height, (n, 3), height, solved);
                row(ui, t().feature_radius, (n, 4), radius, solved);
            });
        }
        Feature::Polygon { points } => {
            // One radius for every vertex at once.
            ui.horizontal(|ui| {
                ui.label(theme::soft(t().radius_all));
                let shown = points.first().map_or_else(Dim::zero, |v| v.radius.clone());
                let same = points.iter().all(|v| v.radius == shown);
                if let Some(new) = dim_drag(ui, ("radius_all", n, same), &shown, solved, 60.0, 0.0)
                {
                    for v in points.iter_mut() {
                        v.radius = new.clone();
                    }
                }
            });
            ui.add_space(4.0);
            let mut delete = None;
            let count = points.len();
            egui::Grid::new(("vertices", n))
                .num_columns(5)
                .spacing([6.0, 4.0])
                .show(ui, |ui| {
                    ui.label("");
                    ui.label(theme::soft("x"));
                    ui.label(theme::soft("y"));
                    ui.label(theme::soft(t().feature_radius));
                    ui.end_row();
                    for (k, v) in points.iter_mut().enumerate() {
                        let name = format!("{}", k + 1);
                        if ui.selectable_label(vertex == Some(k), name).clicked() {
                            picked = Some(k);
                        }
                        for (c, dim) in [&mut v.x, &mut v.y, &mut v.radius].into_iter().enumerate()
                        {
                            let min = if c == 2 { 0.0 } else { f64::NEG_INFINITY };
                            if let Some(new) =
                                dim_drag(ui, ("vertex", n, k, c), dim, solved, 62.0, min)
                            {
                                *dim = new;
                            }
                        }
                        if count > 3 && theme::mini(ui, false, icons::DELETE, t().delete).clicked()
                        {
                            delete = Some(k);
                        }
                        ui.end_row();
                    }
                });
            if let Some(k) = delete {
                points.remove(k);
                picked = None;
            }
        }
    }
    picked
}
