//! Materials page: boards (thickness required) and sticks (section required). The stock
//! sizes are optional: they only serve to estimate how much to buy.

use eframe::egui::{self, RichText, Ui};
use tarugo_core::material::{Material, MaterialKind};

use crate::app::App;
use crate::i18n::t;
use crate::icons;
use crate::theme::{self, pal};
use crate::ui::{mm_field, optional_mm_field};

impl App {
    pub(crate) fn materials_page(&mut self, ui: &mut Ui) {
        let p = pal();
        ui.heading(t().stage_materials);
        ui.label(theme::soft(t().materials_intro));
        ui.add_space(12.0);
        let doc = self.doc().clone();
        let mut changed: Option<Material> = None;
        let mut delete = None;
        let mut main = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("materials")
                .num_columns(8)
                .spacing([14.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    for title in [
                        "",
                        t().name,
                        t().kind,
                        t().thickness_or_section,
                        t().stock_length,
                        t().stock_width,
                        t().color,
                        "",
                    ] {
                        ui.label(theme::strong(title).size(12.0).color(p.text_soft));
                    }
                    ui.end_row();
                    for material in &doc.materials {
                        let mut m = material.clone();
                        let is_main = doc.main_material == m.id;
                        let star = ui
                            .add_enabled(
                                m.is_board(),
                                egui::Button::new(
                                    theme::icon(icons::MAIN, is_main).color(if is_main {
                                        p.accent
                                    } else {
                                        p.text_soft
                                    }),
                                )
                                .frame_when_inactive(false),
                            )
                            .on_hover_text(t().main_material_help);
                        if star.clicked() {
                            main = Some(m.id);
                        }
                        ui.add_sized([200.0, 22.0], egui::TextEdit::singleline(&mut m.name));
                        match &mut m.kind {
                            MaterialKind::Board {
                                thickness,
                                stock_length,
                                stock_width,
                            } => {
                                ui.label(format!("{} {}", icons::BOARD, t().board));
                                mm_field(ui, thickness, 200.0);
                                optional_mm_field(ui, stock_length, 10_000.0);
                                optional_mm_field(ui, stock_width, 10_000.0);
                            }
                            MaterialKind::Stick {
                                width,
                                height,
                                stock_length,
                            } => {
                                ui.label(format!("{} {}", icons::STICK, t().stick));
                                ui.horizontal(|ui| {
                                    mm_field(ui, width, 500.0);
                                    ui.label("×");
                                    mm_field(ui, height, 500.0);
                                });
                                optional_mm_field(ui, stock_length, 10_000.0);
                                ui.label("");
                            }
                        }
                        ui.color_edit_button_srgb(&mut m.color);
                        let in_use = self.doc().material_in_use(m.id);
                        let removable = !in_use && !is_main;
                        let help = if removable {
                            t().delete
                        } else {
                            t().material_in_use
                        };
                        ui.add_enabled_ui(removable, |ui| {
                            if theme::mini(ui, removable, icons::DELETE, help).clicked() {
                                delete = Some(m.id);
                            }
                        });
                        ui.end_row();
                        if !m.is_valid() {
                            ui.label("");
                            ui.label(
                                RichText::new(format!("{} {}", icons::ERROR, t().material_invalid))
                                    .color(p.error),
                            );
                            ui.end_row();
                        }
                        if m != *material {
                            changed = Some(m);
                        }
                    }
                });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .button(format!("{} {}", icons::ADD, t().add_board_material))
                    .clicked()
                {
                    let name = t().new_board_name.to_string();
                    self.session.edit(|d| {
                        d.add_material(
                            name,
                            MaterialKind::Board {
                                thickness: 15.0,
                                stock_length: None,
                                stock_width: None,
                            },
                        )
                    });
                }
                if ui
                    .button(format!("{} {}", icons::ADD, t().add_stick_material))
                    .clicked()
                {
                    let name = t().new_stick_name.to_string();
                    self.session.edit(|d| {
                        d.add_material(
                            name,
                            MaterialKind::Stick {
                                width: 41.0,
                                height: 41.0,
                                stock_length: None,
                            },
                        )
                    });
                }
            });
            ui.add_space(8.0);
            ui.label(theme::soft(t().materials_note));
        });
        if let Some(m) = changed {
            let key = format!("material{}", m.id);
            self.session.edit_grouped(&key, |d| {
                if let Some(slot) = d.material_mut(m.id) {
                    *slot = m;
                }
            });
        }
        if let Some(id) = main {
            self.session.edit(|d| d.main_material = id);
        }
        if let Some(id) = delete {
            self.session.edit(|d| d.materials.retain(|m| m.id != id));
        }
    }
}
