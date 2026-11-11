//! Dialogs: the depth after the front sketch, about, and unsaved changes.

use eframe::egui::{self, Align2, Key, RichText};
use tarugo_core::model::Dim;

use crate::app::{APP, App, Stage};
use crate::i18n::t;
use crate::icons;
use crate::theme::{self, pal};

pub enum Dialog {
    /// Asks for the depth before going on to `then`.
    Depth {
        text: String,
        then: Stage,
    },
    About,
}

fn window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
}

impl App {
    pub(crate) fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut close = false;
        let mut depth = None;
        match dialog {
            Dialog::Depth { text, then } => {
                let value = self.scene.solved.eval(&Dim(text.trim().to_string()));
                let valid = value.as_ref().is_ok_and(|v| *v > 0.0);
                window(t().depth_title).show(ctx, |ui| {
                    ui.label(t().depth_text);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let response =
                            ui.add(egui::TextEdit::singleline(text).desired_width(120.0));
                        if !response.has_focus() && !response.lost_focus() {
                            response.request_focus();
                        }
                        ui.label("mm");
                        if !valid {
                            ui.label(RichText::new(icons::ERROR).color(pal().error));
                        }
                    });
                    ui.add_space(8.0);
                    let enter = ui.input(|i| i.key_pressed(Key::Enter));
                    ui.horizontal(|ui| {
                        let ok = ui.add_enabled_ui(valid, |ui| {
                            theme::primary_button(ui, icons::OK, t().ok).clicked()
                        });
                        if (ok.inner || enter) && valid {
                            depth = Some((Dim(text.trim().to_string()), *then));
                        }
                        if ui.button(t().cancel).clicked() {
                            close = true;
                        }
                    });
                });
                if ctx.input(|i| i.key_pressed(Key::Escape)) {
                    close = true;
                }
            }
            Dialog::About => {
                window(&format!("{} {APP}", t().about)).show(ctx, |ui| {
                    ui.set_max_width(380.0);
                    ui.label(theme::strong(format!(
                        "{APP} {}",
                        env!("CARGO_PKG_VERSION")
                    )));
                    ui.label(t().about_text);
                    ui.add_space(8.0);
                    if ui.button(t().ok).clicked() {
                        close = true;
                    }
                });
            }
        }
        if let Some((dim, then)) = depth {
            self.dialog = None;
            self.confirm_depth(dim, then);
        } else if close {
            self.dialog = None;
        }
    }

    /// Asks what to do with the unsaved changes before a pending action.
    pub(crate) fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending.clone() else {
            return;
        };
        let mut choice = None;
        window(t().unsaved_title).show(ctx, |ui| {
            ui.label(t().unsaved_text);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, icons::SAVE, t().save).clicked() {
                    choice = Some(Some(true));
                }
                if ui.button(t().discard).clicked() {
                    choice = Some(Some(false));
                }
                if ui.button(t().cancel).clicked() {
                    choice = Some(None);
                }
            });
        });
        match choice {
            Some(Some(save)) => {
                if !save || self.save(false) {
                    self.pending = None;
                    self.session.dirty = false;
                    self.run_pending(action);
                }
            }
            Some(None) => self.pending = None,
            None => {}
        }
    }
}
