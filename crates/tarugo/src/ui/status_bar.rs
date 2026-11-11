//! Status bar: messages and hints on the left; counts and the pointer on the right.

use eframe::egui::{self, Align, Layout, RichText, Ui};
use tarugo_core::expr::format_number;

use crate::app::App;
use crate::i18n::{fill, t};
use crate::theme::pal;

impl App {
    pub(crate) fn status_bar(&mut self, ui: &mut Ui) {
        let p = pal();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if self.stage.has_view() {
                // Fixed width: does not jump when the pointer moves or leaves the view.
                let pos = self.pointer_world.map_or(String::new(), |w| {
                    format!("{:5.0}, {:5.0}, {:5.0}", w[0], w[1], w[2])
                });
                ui.add_sized(
                    [150.0, 20.0],
                    egui::Label::new(RichText::new(pos).monospace().color(p.text_soft)),
                );
                ui.separator();
            }
            let doc = self.doc();
            let counts = fill(t().status_counts, &[&doc.parts.len(), &doc.screws.len()]);
            ui.label(RichText::new(counts).color(p.text_soft));
            if let Some(b) = self.scene.solved.bounds() {
                ui.separator();
                let s = b.size().map(format_number);
                ui.label(
                    RichText::new(format!("{} × {} × {} mm", s[0], s[1], s[2])).color(p.text_soft),
                );
            }
            ui.separator();
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.shrink_clip_rect(ui.max_rect());
                let status = self.view_status();
                match (&self.message, status) {
                    (Some((text, true)), _) => {
                        ui.add(egui::Label::new(RichText::new(text).color(p.error)).truncate());
                    }
                    (_, Some(text)) => {
                        ui.add(egui::Label::new(text).truncate());
                    }
                    (Some((text, false)), _) => {
                        ui.add(egui::Label::new(text.as_str()).truncate());
                    }
                    (None, None) => {
                        let hint = if self.stage.has_view() {
                            self.tool.hint()
                        } else {
                            ""
                        };
                        ui.add(egui::Label::new(RichText::new(hint).color(p.text_soft)).truncate());
                    }
                }
            });
        });
    }
}
