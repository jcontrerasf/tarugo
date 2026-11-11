//! Dimensions drawn on the view ("cotas") and their inline editing.

use eframe::egui::{self, Key, Pos2};
use glam::Vec3;
use tarugo_core::edit;
use tarugo_core::expr::format_number;
use tarugo_core::front;
use tarugo_core::model::{DEPTH, Dim, End, Extent, HEIGHT, PartId, WIDTH};

use crate::app::{App, Stage};
use crate::i18n::t;
use crate::tools::Tool;
use crate::view3d::mesh::vec3;

/// What typing a new value in a dimension changes.
#[derive(Clone, Debug, PartialEq)]
pub enum CotaTarget {
    Param(String),
    /// Distance from the reference of a fixed axis to the part.
    Anchor {
        part: PartId,
        axis: usize,
    },
    /// Length of a part along a free axis.
    Length {
        part: PartId,
        axis: usize,
    },
}

/// A dimension to draw between two world points.
#[derive(Clone, Debug)]
pub struct CotaView {
    pub a: Vec3,
    pub b: Vec3,
    pub text: String,
    /// Editable if it has a target.
    pub target: Option<CotaTarget>,
    /// What the editor starts with (the formula, if there is one).
    pub edit_text: String,
    /// A derived measure, drawn discreetly.
    pub soft: bool,
}

impl CotaView {
    pub fn plain(a: Vec3, b: Vec3, value: f64) -> CotaView {
        CotaView {
            a,
            b,
            text: format_number(value),
            target: None,
            edit_text: String::new(),
            soft: false,
        }
    }
}

/// The inline editor of a dimension.
pub struct CotaEdit {
    pub target: CotaTarget,
    pub text: String,
    /// Screen position of its center.
    pub pos: Pos2,
    /// Just opened: takes the focus and selects everything.
    pub fresh: bool,
}

impl CotaEdit {
    /// Editor for the dimension that places `part` along `axis`, if it has one.
    pub fn for_anchor(app: &App, part: PartId, axis: usize) -> Option<CotaEdit> {
        let target = CotaTarget::Anchor { part, axis };
        let view = app
            .cotas()
            .into_iter()
            .find(|c| c.target.as_ref() == Some(&target))?;
        let camera = if app.stage == Stage::Front {
            app.front_camera
        } else {
            app.camera
        };
        let pos = camera.project(app.view_rect, (view.a + view.b) / 2.0)?;
        Some(CotaEdit {
            target,
            text: view.edit_text,
            pos,
            fresh: true,
        })
    }
}

impl App {
    /// The dimensions to show now: overall size and what places the selected part.
    pub fn cotas(&self) -> Vec<CotaView> {
        let mut out = Vec::new();
        let Some(bounds) = self.scene.solved.bounds() else {
            return out;
        };
        let front = self.stage == Stage::Front;
        let camera = if front {
            self.front_camera
        } else {
            self.camera
        };
        let off = 34.0 * camera.units_per_point(self.view_rect) as f64;
        let doc = self.doc();
        let solved = &self.scene.solved;
        let (min, max) = (bounds.min, bounds.max);
        let zf = max[2];

        // Overall size, editable when a parameter gives it.
        let mut overall = |name: &str, a: [f64; 3], b: [f64; 3], size: f64| {
            let mut view = CotaView::plain(vec3(a), vec3(b), size);
            if let (Some(p), Some(Ok(v))) = (doc.param(name), solved.params.get(name))
                && (v - size).abs() < 0.01
            {
                view.target = Some(CotaTarget::Param(name.to_string()));
                view.edit_text = p.value.0.clone();
            }
            out.push(view);
        };
        overall(
            WIDTH,
            [min[0], min[1] - off, zf],
            [max[0], min[1] - off, zf],
            max[0] - min[0],
        );
        overall(
            HEIGHT,
            [min[0] - off, min[1], zf],
            [min[0] - off, max[1], zf],
            max[1] - min[1],
        );
        if !front {
            overall(
                DEPTH,
                [max[0] + off, min[1], min[2]],
                [max[0] + off, min[1], max[2]],
                max[2] - min[2],
            );
        }

        // Also with the divide tool, to type the dimension of the board just placed.
        let shows_selection = matches!(self.tool, Tool::Select | Tool::Divide(_));
        if !shows_selection || self.drag.is_some() {
            return out;
        }
        let Some(part) = self.session.selected.and_then(|id| doc.part(id)) else {
            return out;
        };
        let Some(b) = solved.get(part.id) else {
            return out;
        };
        let center = b.center();
        let axes = if front { 0..2 } else { 0..3 };
        for axis in axes {
            // A point of the part's middle line along `axis`, on the front in the sketch.
            let on_axis = |value: f64, shift: f64| {
                let mut p = center;
                p[axis] = value;
                if front {
                    p[2] = zf;
                    let across = 1 - axis;
                    p[across] = b.min[across] + (b.max[across] - b.min[across]) * shift;
                }
                vec3(p)
            };
            match &part.axes[axis] {
                Extent::Thick {
                    anchor: End::Ref { face, offset },
                    align,
                } => {
                    let Some(from) = self.face_value(*face, axis) else {
                        continue;
                    };
                    let own = align.of(b.min[axis], b.max[axis]);
                    let mut driving = None;
                    if (own - from).abs() > 0.01 {
                        let mut view = CotaView::plain(
                            on_axis(from, 0.5),
                            on_axis(own, 0.5),
                            (own - from).abs(),
                        );
                        view.target = Some(CotaTarget::Anchor {
                            part: part.id,
                            axis,
                        });
                        view.edit_text = if own < from {
                            front::negate(offset).0
                        } else {
                            offset.0.clone()
                        };
                        out.push(view);
                        driving = Some((from.min(own), from.max(own)));
                    }
                    if front {
                        // Clear space at each side, unless the driving dimension is it.
                        let across = 1 - axis;
                        for (face_at, outward) in [(b.min[axis], -1.0), (b.max[axis], 1.0)] {
                            let mut probe = [center[0], center[1]];
                            probe[axis] = face_at + outward * 0.5;
                            probe[across] = center[across];
                            let Some(cell) = front::find_cell(doc, solved, probe[0], probe[1])
                            else {
                                continue;
                            };
                            let far = if outward < 0.0 {
                                cell.min[axis]
                            } else {
                                cell.max[axis]
                            };
                            let span = (face_at.min(far), face_at.max(far));
                            let same = driving.is_some_and(|d| {
                                (d.0 - span.0).abs() < 0.01 && (d.1 - span.1).abs() < 0.01
                            });
                            if !same && span.1 - span.0 > 0.01 {
                                let mut view = CotaView::plain(
                                    on_axis(face_at, 0.25),
                                    on_axis(far, 0.25),
                                    span.1 - span.0,
                                );
                                view.soft = true;
                                out.push(view);
                            }
                        }
                    }
                }
                Extent::Span { .. } if !front => {
                    let length = b.max[axis] - b.min[axis];
                    let mut view = CotaView::plain(
                        on_axis(b.min[axis], 0.5),
                        on_axis(b.max[axis], 0.5),
                        length,
                    );
                    view.target = Some(CotaTarget::Length {
                        part: part.id,
                        axis,
                    });
                    view.edit_text = format_number(length);
                    out.push(view);
                }
                _ => {}
            }
        }
        out
    }

    /// Applies what was typed in a dimension.
    pub fn commit_cota(&mut self, target: CotaTarget, text: &str) {
        let dim = Dim(text.trim().to_string());
        let value = match self.scene.solved.eval(&dim) {
            Ok(v) => v,
            Err(e) => {
                self.error(super::error_text(&e));
                return;
            }
        };
        match target {
            CotaTarget::Param(name) => {
                if value <= 0.0 {
                    self.error(t().must_be_positive);
                    return;
                }
                self.session.edit(|d| d.set_param(&name, dim));
            }
            CotaTarget::Anchor { part, axis } => {
                // The dimension is typed as a distance; the offset keeps its direction.
                let negative = self.doc().part(part).is_some_and(|p| {
                    matches!(&p.axes[axis], Extent::Thick { anchor: End::Ref { offset, .. }, .. }
                        if self.scene.solved.eval(offset).is_ok_and(|v| v < 0.0))
                });
                let offset = if negative { front::negate(&dim) } else { dim };
                self.session.edit(|d| {
                    if let Some(p) = d.part_mut(part)
                        && let Extent::Thick {
                            anchor: End::Ref { offset: o, .. },
                            ..
                        } = &mut p.axes[axis]
                    {
                        *o = offset;
                    }
                });
            }
            CotaTarget::Length { part, axis } => {
                if value <= 0.0 {
                    self.error(t().must_be_positive);
                    return;
                }
                let solved = self.scene.solved.clone();
                self.session
                    .edit(|d| edit::set_length(d, &solved, part, axis, value));
            }
        }
    }

    /// The floating text field of the dimension being edited.
    pub(crate) fn cota_editor(&mut self, ui: &mut egui::Ui) {
        let Some(edit) = &mut self.cota_edit else {
            return;
        };
        let mut done = None;
        let pos = edit.pos - egui::vec2(48.0, 14.0);
        egui::Area::new(egui::Id::unique("cota_edit"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(3)
                    .show(ui, |ui| {
                        let response =
                            ui.add(egui::TextEdit::singleline(&mut edit.text).desired_width(90.0));
                        if edit.fresh {
                            edit.fresh = false;
                            response.request_focus();
                            if let Some(mut state) =
                                egui::TextEdit::load_state(ui.ctx(), response.id)
                            {
                                let all = egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(edit.text.chars().count()),
                                );
                                state.cursor.set_char_range(Some(all));
                                state.store(ui.ctx(), response.id);
                            }
                        } else if response.lost_focus() {
                            done = Some(!ui.input(|i| i.key_pressed(Key::Escape)));
                        }
                    });
            });
        if let Some(commit) = done
            && let Some(edit) = self.cota_edit.take()
            && commit
        {
            self.commit_cota(edit.target, &edit.text);
        }
    }
}
