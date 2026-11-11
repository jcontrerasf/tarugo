//! Cut list page: the table, its export and the stock estimate.

use eframe::egui::{self, RichText, Ui};
use tarugo_core::cutlist::{self, Row};
use tarugo_core::expr::format_number;

use crate::app::App;
use crate::i18n::{fill, t};
use crate::icons;
use crate::theme::{self, pal};
use crate::ui::mm_field;

impl App {
    fn csv(&self, rows: &[Row]) -> String {
        let headers = [
            t().material,
            t().quantity,
            t().length_mm,
            t().width_mm,
            t().thickness_mm,
            t().parts,
        ];
        cutlist::to_csv(self.doc(), rows, headers)
    }

    pub(crate) fn cut_list_page(&mut self, ui: &mut Ui) {
        let p = pal();
        let rows = cutlist::cut_list(self.doc(), &self.scene.solved);
        ui.horizontal(|ui| {
            ui.heading(t().stage_cut_list);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::primary_button(ui, icons::EXPORT, t().export_csv).clicked() {
                    self.export_csv(&rows);
                }
                if ui
                    .button(format!("{} {}", icons::COPY, t().copy))
                    .on_hover_text(t().copy_help)
                    .clicked()
                {
                    // Tabs paste straight into a spreadsheet.
                    let text = self.csv(&rows).replace(',', "\t");
                    ui.ctx().copy_text(text);
                    self.info(t().copied);
                }
            });
        });
        ui.label(theme::soft(t().cut_list_intro));
        if !self.scene.solved.errors.is_empty() {
            ui.label(
                RichText::new(format!("{} {}", icons::ERROR, t().cut_list_broken)).color(p.error),
            );
        }
        ui.add_space(12.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            if rows.is_empty() {
                ui.label(theme::soft(t().no_parts));
                return;
            }
            let doc = self.doc();
            egui::Grid::new("cut_list")
                .num_columns(6)
                .spacing([22.0, 7.0])
                .striped(true)
                .show(ui, |ui| {
                    for title in [
                        t().material,
                        t().quantity,
                        t().length_mm,
                        t().width_mm,
                        t().thickness_mm,
                        t().parts,
                    ] {
                        ui.label(theme::strong(title).size(12.0).color(p.text_soft));
                    }
                    ui.end_row();
                    for row in &rows {
                        let material = doc.material(row.material);
                        ui.horizontal(|ui| {
                            if let Some(m) = material {
                                let c = m.color;
                                theme::swatch(ui, egui::Color32::from_rgb(c[0], c[1], c[2]), 12.0);
                                ui.label(&m.name);
                            }
                        });
                        ui.label(theme::strong(row.quantity.to_string()));
                        ui.monospace(format_number(row.length));
                        ui.monospace(format_number(row.width));
                        ui.monospace(format_number(row.thickness));
                        ui.horizontal(|ui| {
                            ui.label(cutlist::join_names(&row.names));
                            if row.shaped {
                                ui.label(RichText::new(icons::SHAPE).color(p.warning))
                                    .on_hover_text(t().shaped_note);
                            }
                        });
                        ui.end_row();
                    }
                });
            let total: usize = rows.iter().map(|r| r.quantity).sum();
            ui.add_space(6.0);
            ui.label(theme::soft(fill(t().cut_list_total, &[&total])));

            ui.add_space(18.0);
            theme::section_title(ui, t().stock_estimate);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(theme::soft(t().kerf));
                let mut kerf = self.doc().settings.kerf;
                if mm_field(ui, &mut kerf, 20.0).changed() {
                    self.session
                        .edit_grouped("kerf", |d| d.settings.kerf = kerf);
                }
            });
            ui.add_space(4.0);
            let doc = self.doc();
            for need in cutlist::stock_needs(doc, &rows) {
                let Some(material) = doc.material(need.material) else {
                    continue;
                };
                let amount = if material.is_board() {
                    fill(
                        t().need_area,
                        &[&need.pieces, &format!("{:.2}", need.total)],
                    )
                } else {
                    fill(
                        t().need_length,
                        &[&need.pieces, &format!("{:.2}", need.total)],
                    )
                };
                let units = match (need.units, material.is_board()) {
                    (Some(n), true) => fill(t().need_sheets, &[&n]),
                    (Some(n), false) => fill(t().need_sticks, &[&n]),
                    (None, _) => t().need_unknown.to_string(),
                };
                ui.horizontal(|ui| {
                    ui.label(theme::strong(&material.name));
                    ui.label(amount);
                    ui.label(
                        RichText::new(format!("→ {units}")).color(if need.units.is_some() {
                            p.text
                        } else {
                            p.text_soft
                        }),
                    );
                    if need.too_big > 0 {
                        ui.label(
                            RichText::new(format!(
                                "{} {}",
                                icons::WARNING,
                                fill(t().need_too_big, &[&need.too_big])
                            ))
                            .color(p.warning),
                        );
                    }
                });
            }
            ui.add_space(6.0);
            ui.label(theme::soft(t().stock_note));
        });
    }

    fn export_csv(&mut self, rows: &[Row]) {
        let picked = rfd::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .set_file_name(format!("{}.csv", self.doc_name()))
            .save_file();
        let Some(path) = picked else {
            return;
        };
        let path = if path.extension().is_none() {
            path.with_extension("csv")
        } else {
            path
        };
        match std::fs::write(&path, self.csv(rows)) {
            Ok(()) => self.info(fill(t().exported, &[&path.display()])),
            Err(e) => self.error(fill(t().cannot_save, &[&path.display(), &e])),
        }
    }
}
