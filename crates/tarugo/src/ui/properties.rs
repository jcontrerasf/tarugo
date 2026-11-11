//! Properties of the selected part: name, material and how each axis is placed.

use eframe::egui::{self, RichText, Ui};
use tarugo_core::edit::{self, Slot};
use tarugo_core::expr::format_number;
use tarugo_core::model::{AXIS_NAMES, End, Extent, FaceRef, PartId, Side};

use crate::app::App;
use crate::i18n::{fill, t};
use crate::icons;
use crate::theme::{self, pal};
use crate::ui::{dim_field, mm_field, text_commit};

/// Name of a face of a part along an axis.
pub fn side_name(axis: usize, side: Side) -> &'static str {
    match (axis, side) {
        (_, Side::Center) => t().side_center,
        (0, Side::Min) => t().side_left,
        (0, Side::Max) => t().side_right,
        (1, Side::Min) => t().side_bottom,
        (1, Side::Max) => t().side_top,
        (_, Side::Min) => t().side_back,
        (_, Side::Max) => t().side_front,
    }
}

pub fn axis_title(axis: usize) -> String {
    let name = [t().axis_width, t().axis_height, t().axis_depth][axis];
    format!("{} · {name}", AXIS_NAMES[axis])
}

impl App {
    pub(crate) fn properties(&mut self, ui: &mut Ui) {
        let Some(id) = self.session.selected else {
            return;
        };
        let Some(part) = self.doc().part(id).cloned() else {
            return;
        };
        let p = pal();
        egui::Grid::new("part_basics")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label(theme::soft(t().name));
                if let Some(name) = text_commit(ui, ("part_name", id), &part.name, 170.0) {
                    self.session.edit(|d| {
                        if let Some(p) = d.part_mut(id) {
                            p.name = name;
                        }
                    });
                }
                ui.end_row();
                ui.label(theme::soft(t().material));
                let doc = self.doc();
                let current = doc
                    .material(part.material)
                    .map_or(t().none.to_string(), |m| m.name.clone());
                // Only materials of the same kind keep the part's axes meaningful.
                let board = part.thick_axes().len() == 1;
                let mut picked = None;
                egui::ComboBox::from_id_salt("part_material")
                    .selected_text(current)
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        for m in doc.materials.iter().filter(|m| m.is_board() == board) {
                            if ui
                                .selectable_label(m.id == part.material, &m.name)
                                .clicked()
                            {
                                picked = Some(m.id);
                            }
                        }
                    });
                if let Some(m) = picked {
                    self.session.edit(|d| {
                        if let Some(p) = d.part_mut(id) {
                            p.material = m;
                        }
                    });
                }
                ui.end_row();
                if !board {
                    ui.label("");
                    let mut swap = part.swap_section;
                    if ui.checkbox(&mut swap, t().swap_section).changed() {
                        self.session.edit(|d| {
                            if let Some(p) = d.part_mut(id) {
                                p.swap_section = swap;
                            }
                        });
                    }
                    ui.end_row();
                }
            });
        if let Some(e) = self.scene.solved.errors.get(&id) {
            let text = self.solve_error_text(e);
            ui.label(RichText::new(format!("{} {text}", icons::ERROR)).color(p.error));
        }
        for axis in 0..3 {
            ui.add_space(4.0);
            ui.label(theme::strong(axis_title(axis)).size(12.5));
            match &part.axes[axis] {
                Extent::Span { lo, hi } => {
                    self.end_row(ui, id, axis, Slot::Lo, t().end_from, lo);
                    self.end_row(ui, id, axis, Slot::Hi, t().end_to, hi);
                    if let Some(b) = self.scene.get(id) {
                        ui.horizontal(|ui| {
                            ui.label(theme::soft(t().length));
                            let mut length = b.size()[axis];
                            if mm_field(ui, &mut length, 20_000.0).changed() && length >= 1.0 {
                                let solved = self.scene.solved.clone();
                                self.session.edit_grouped(&format!("length{axis}"), |d| {
                                    edit::set_length(d, &solved, id, axis, length)
                                });
                            }
                        });
                    }
                }
                Extent::Thick { anchor, align } => {
                    self.end_row(ui, id, axis, Slot::Anchor, t().end_position, anchor);
                    ui.horizontal(|ui| {
                        ui.label(theme::soft(t().aligned_face));
                        let mut new_align = *align;
                        for side in [Side::Min, Side::Center, Side::Max] {
                            ui.selectable_value(&mut new_align, side, side_name(axis, side));
                        }
                        if new_align != *align {
                            let face = match anchor {
                                End::Ref { face, .. } => Some(*face),
                                _ => None,
                            };
                            let solved = self.scene.solved.clone();
                            self.session.edit(|d| {
                                edit::retarget_anchor(d, &solved, id, axis, face, new_align)
                            });
                        }
                        if let Some(b) = self.scene.get(id) {
                            ui.label(theme::soft(format!("{} mm", format_number(b.size()[axis]))));
                        }
                    });
                }
            }
        }
        ui.add_space(4.0);
    }

    /// One end of the part: where it is, and how to change it.
    fn end_row(
        &mut self,
        ui: &mut Ui,
        id: PartId,
        axis: usize,
        slot: Slot,
        label: &str,
        end: &End,
    ) {
        let solved = self.scene.solved.clone();
        ui.horizontal(|ui| {
            ui.label(theme::soft(label));
            match end {
                End::Abs { value } => {
                    if let Some(dim) =
                        dim_field(ui, ("end", id, axis, slot as u8), value, &solved, 110.0)
                    {
                        self.session
                            .edit(|d| edit::set_end(d, id, axis, slot, End::Abs { value: dim }));
                    }
                }
                End::Ref { face, offset } => {
                    let mut side = face.side;
                    egui::ComboBox::from_id_salt(("ref_side", axis, slot as u8))
                        .selected_text(fill(
                            t().ref_of,
                            &[&side_name(axis, side), &self.part_label(face.part)],
                        ))
                        .width(130.0)
                        .truncate()
                        .show_ui(ui, |ui| {
                            for s in [Side::Min, Side::Center, Side::Max] {
                                ui.selectable_value(&mut side, s, side_name(axis, s));
                            }
                        });
                    if side != face.side {
                        // Another face of the same reference, without moving the part.
                        let new_face = FaceRef {
                            part: face.part,
                            side,
                        };
                        let from = solved.get(face.part).map(|b| {
                            (
                                face.side.of(b.min[axis], b.max[axis]),
                                side.of(b.min[axis], b.max[axis]),
                            )
                        });
                        let old = solved.eval(offset).unwrap_or(0.0);
                        if let Some((was, now)) = from {
                            let new_end = End::Ref {
                                face: new_face,
                                offset: (old + was - now).into(),
                            };
                            self.session
                                .edit(|d| edit::set_end(d, id, axis, slot, new_end));
                        }
                    }
                    ui.label("+");
                    if let Some(dim) =
                        dim_field(ui, ("off", id, axis, slot as u8), offset, &solved, 60.0)
                    {
                        let new_end = End::Ref {
                            face: *face,
                            offset: dim,
                        };
                        self.session
                            .edit(|d| edit::set_end(d, id, axis, slot, new_end));
                    }
                    self.detach_button(ui, id, axis, slot, end);
                }
                End::Between {
                    lo,
                    hi,
                    frac,
                    offset,
                } => {
                    ui.label(fill(
                        t().between_of,
                        &[
                            &frac.0,
                            &self.part_label(lo.part),
                            &self.part_label(hi.part),
                        ],
                    ));
                    if let Some(dim) =
                        dim_field(ui, ("off", id, axis, slot as u8), offset, &solved, 50.0)
                    {
                        let new_end = End::Between {
                            lo: *lo,
                            hi: *hi,
                            frac: frac.clone(),
                            offset: dim,
                        };
                        self.session
                            .edit(|d| edit::set_end(d, id, axis, slot, new_end));
                    }
                    self.detach_button(ui, id, axis, slot, end);
                }
            }
        });
    }

    /// Turns an end that follows another part into a plain coordinate.
    fn detach_button(&mut self, ui: &mut Ui, id: PartId, axis: usize, slot: Slot, end: &End) {
        if theme::mini(ui, false, icons::CANCEL, t().detach).clicked()
            && let Some(value) = self.scene.solved.end_value(end, axis)
        {
            self.session
                .edit(|d| edit::set_end(d, id, axis, slot, End::abs(value)));
        }
    }
}
