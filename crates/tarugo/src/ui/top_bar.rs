//! Top bar: document, file and edit actions, the stages of the design and the menu.

use eframe::egui::{self, Align, Layout, RichText, Ui};

use crate::app::{APP, App, Pending, Stage};
use crate::i18n::{self, Lang, t};
use crate::icons;
use crate::theme::{self, ThemeChoice, pal};
use crate::ui::dialogs::Dialog;

pub fn stage_name(stage: Stage) -> (&'static str, &'static str) {
    match stage {
        Stage::Materials => (icons::MATERIALS, t().stage_materials),
        Stage::Front => (icons::FRONT, t().stage_front),
        Stage::Design => (icons::DESIGN, t().stage_design),
        Stage::Screws => (icons::SCREWS, t().stage_screws),
        Stage::CutList => (icons::CUT_LIST, t().stage_cut_list),
    }
}

impl App {
    pub(crate) fn top_bar(&mut self, ui: &mut Ui) {
        // Right to left first: the menu stays visible in a narrow window.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button(theme::icon(icons::MENU, false), |ui| self.main_menu(ui));
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.shrink_clip_rect(ui.max_rect());
                self.top_bar_left(ui);
            });
        });
    }

    fn top_bar_left(&mut self, ui: &mut Ui) {
        let p = pal();
        ui.label(theme::strong(APP).size(15.0));
        let mark = if self.session.dirty { " •" } else { "" };
        ui.label(RichText::new(format!("{}{mark}", self.doc_name())).color(p.text_soft));
        ui.separator();
        ui.spacing_mut().item_spacing.x = 2.0;
        if theme::icon_button(ui, icons::NEW, &format!("{}  (Ctrl+N)", t().new_doc)).clicked() {
            self.request(Pending::New);
        }
        if theme::icon_button(ui, icons::OPEN, &format!("{}  (Ctrl+O)", t().open)).clicked() {
            self.pick_open();
        }
        if theme::icon_button(ui, icons::SAVE, &format!("{}  (Ctrl+S)", t().save)).clicked() {
            self.save(false);
        }
        ui.add_space(6.0);
        let (can_undo, can_redo) = (self.session.can_undo(), self.session.can_redo());
        if theme::icon_button_if(
            ui,
            can_undo,
            icons::UNDO,
            &format!("{}  (Ctrl+Z)", t().undo),
        )
        .clicked()
        {
            self.undo();
        }
        if theme::icon_button_if(
            ui,
            can_redo,
            icons::REDO,
            &format!("{}  (Ctrl+Y)", t().redo),
        )
        .clicked()
        {
            self.redo();
        }
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.separator();
        // The stages, in the order a design goes through them.
        ui.spacing_mut().item_spacing.x = 4.0;
        let compact = ui.available_width() < 600.0;
        for (n, stage) in Stage::ALL.into_iter().enumerate() {
            if n > 0 {
                ui.label(RichText::new(icons::NEXT).size(11.0).color(p.text_soft));
            }
            let (ico, name) = stage_name(stage);
            let active = self.stage == stage;
            // In a narrow window the stages keep only their icon.
            let text = if compact {
                RichText::new(ico)
            } else {
                RichText::new(format!("{ico}  {name}"))
            };
            let text = if active {
                text.color(egui::Color32::WHITE)
            } else {
                text
            };
            let mut button = egui::Button::new(text).min_size(egui::vec2(0.0, 28.0));
            button = if active {
                button.fill(p.accent)
            } else {
                button.frame_when_inactive(false)
            };
            if ui.add(button).on_hover_text(name).clicked() {
                self.set_stage(stage);
            }
        }
    }

    fn main_menu(&mut self, ui: &mut Ui) {
        if ui
            .button(format!("{} {}…", icons::SAVE, t().save_as))
            .clicked()
        {
            self.save(true);
        }
        ui.menu_button(format!("{} {}", icons::RECENT, t().recent), |ui| {
            if self.prefs.recent.is_empty() {
                ui.weak(t().nothing_yet);
            }
            let mut open = None;
            for path in &self.prefs.recent {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if ui
                    .button(format!("{} {name}", icons::FILE))
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    open = Some(path.clone());
                }
            }
            if let Some(path) = open {
                self.request(Pending::Open(path));
            }
        });
        ui.separator();
        ui.menu_button(format!("{} {}", icons::LANGUAGE, t().language), |ui| {
            for lang in [Lang::Es, Lang::En] {
                let name = match lang {
                    Lang::Es => i18n::ES.language_name,
                    Lang::En => i18n::EN.language_name,
                };
                if ui.radio(self.prefs.lang == lang, name).clicked() {
                    self.prefs.lang = lang;
                    i18n::set_lang(lang);
                }
            }
        });
        ui.menu_button(format!("{} {}", icons::THEME_SYSTEM, t().theme), |ui| {
            for (choice, ico, name) in [
                (ThemeChoice::System, icons::THEME_SYSTEM, t().theme_system),
                (ThemeChoice::Light, icons::THEME_LIGHT, t().theme_light),
                (ThemeChoice::Dark, icons::THEME_DARK, t().theme_dark),
            ] {
                if ui
                    .radio(self.prefs.theme == choice, format!("{ico} {name}"))
                    .clicked()
                {
                    self.prefs.theme = choice;
                    theme::set_choice(ui.ctx(), choice);
                }
            }
        });
        ui.separator();
        if ui
            .button(format!("{} {}", icons::INFO, t().about))
            .clicked()
        {
            self.dialog = Some(Dialog::About);
        }
    }
}
