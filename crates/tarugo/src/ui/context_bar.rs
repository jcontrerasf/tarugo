//! Context bar: the options of the current tool.

use eframe::egui::{self, Align, Layout, RichText, Ui};
use tarugo_core::model::AXIS_NAMES;

use crate::app::{App, Stage};
use crate::i18n::{fill, t};
use crate::icons;
use crate::theme::{self, pal};
use crate::tools::{self, AlignChoice, RefChoice, Tool};

impl App {
    pub(crate) fn context_bar(&mut self, ui: &mut Ui) {
        ui.set_min_height(30.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if self.stage != Stage::Screws {
                ui.label(
                    RichText::new(format!("{} {}", icons::SNAP, t().snap_hint))
                        .color(pal().text_soft),
                );
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.shrink_clip_rect(ui.max_rect());
                match self.tool {
                    Tool::Select => self.select_options(ui),
                    Tool::Divide(_) => self.divide_options(ui),
                    Tool::AddBoard | Tool::AddStick => self.add_options(ui),
                    Tool::Screw => {}
                }
                if self.stage == Stage::Screws {
                    self.screw_options(ui);
                }
            });
        });
    }

    fn select_options(&mut self, ui: &mut Ui) {
        if self.stage == Stage::Screws {
            return;
        }
        let Some(id) = self.session.selected else {
            ui.label(theme::soft(t().nothing_selected));
            return;
        };
        ui.label(theme::strong(self.part_label(id)));
        ui.separator();
        if self.stage == Stage::Design {
            ui.menu_button(
                format!("{} {}", icons::DUPLICATE, t().duplicate_beside),
                |ui| {
                    let names = [
                        [t().dir_left, t().dir_right],
                        [t().dir_below, t().dir_above],
                        [t().dir_behind, t().dir_in_front],
                    ];
                    for (axis, pair) in names.iter().enumerate() {
                        for (before, name) in [(true, pair[0]), (false, pair[1])] {
                            if ui.button(name).clicked() {
                                self.duplicate_beside(axis, before);
                                ui.close();
                            }
                        }
                    }
                },
            );
            if ui
                .button(format!("{} {}", icons::SHAPE, t().free_form))
                .on_hover_text(t().free_form_help)
                .clicked()
            {
                self.shape_editor = Some(id);
            }
        }
        if ui
            .button(format!("{} {}", icons::DELETE, t().delete))
            .on_hover_text("Supr / Del")
            .clicked()
        {
            self.delete_selection();
        }
    }

    fn divide_options(&mut self, ui: &mut Ui) {
        ui.label(theme::soft(t().divide_from));
        for (choice, name) in [
            (RefChoice::Inner, t().ref_inner),
            (RefChoice::Center, t().ref_center),
            (RefChoice::Outer, t().ref_outer),
        ] {
            ui.selectable_value(&mut self.divide.reference, choice, name);
        }
        ui.separator();
        ui.label(theme::soft(t().divide_to));
        for (choice, name) in [
            (AlignChoice::Near, t().align_near),
            (AlignChoice::Center, t().align_center),
            (AlignChoice::Far, t().align_far),
        ] {
            ui.selectable_value(&mut self.divide.align, choice, name);
        }
        ui.separator();
        ui.toggle_value(
            &mut self.divide.equal,
            format!("{} {}", icons::EQUAL, t().equal_spaces),
        )
        .on_hover_text(t().equal_spaces_help);
        if self.divide.equal {
            ui.add(egui::DragValue::new(&mut self.divide.spaces).range(2..=20));
        }
    }

    fn add_options(&mut self, ui: &mut Ui) {
        let board = self.tool == Tool::AddBoard;
        let current = self.add_material(board);
        let doc = self.doc();
        let name = current
            .and_then(|id| doc.material(id))
            .map_or(t().none.to_string(), |m| m.name.clone());
        let mut picked = None;
        ui.label(theme::soft(t().material));
        egui::ComboBox::from_id_salt("add_material")
            .selected_text(name)
            .show_ui(ui, |ui| {
                for m in doc.materials.iter().filter(|m| m.is_board() == board) {
                    if ui
                        .selectable_label(current == Some(m.id), &m.name)
                        .clicked()
                    {
                        picked = Some(m.id);
                    }
                }
            });
        if picked.is_some() {
            self.add.material = picked;
        }
        ui.separator();
        let names = if board {
            ui.label(theme::soft(t().orientation));
            [t().board_side, t().board_flat, t().board_back]
        } else {
            ui.label(theme::soft(t().stick_along));
            [t().stick_width, t().stick_height, t().stick_depth]
        };
        for (axis, name) in names.iter().enumerate() {
            ui.selectable_value(&mut self.add.axis, axis, *name)
                .on_hover_text(AXIS_NAMES[axis]);
        }
    }

    fn screw_options(&mut self, ui: &mut Ui) {
        if self.tool == Tool::Screw {
            ui.label(theme::soft(t().screws_per_click));
            ui.add(egui::DragValue::new(&mut self.screws.count).range(1..=12));
            ui.separator();
        }
        if ui
            .button(format!("{} {}", icons::AUTO, t().auto_screws))
            .on_hover_text(t().auto_screws_help)
            .clicked()
        {
            let count = self.screws.count;
            let mut doc = self.doc().clone();
            let filled = tools::screw::fill_all(&mut doc, &self.scene, count);
            if filled > 0 {
                self.session.edit(|d| *d = doc);
            }
            self.info(fill(t().auto_screws_done, &[&filled]));
        }
        if theme::primary_button(ui, icons::ANALYZE, t().analyze)
            .on_hover_text(t().analyze_help)
            .clicked()
        {
            self.analysis_on = true;
            self.refresh_scene();
            let issues = self.scene.fastening.len() + self.scene.geometry.len();
            if issues == 0 {
                self.info(t().analysis_ok);
            } else {
                self.info(fill(t().analysis_found, &[&issues]));
            }
        }
        ui.separator();
        ui.label(theme::soft(t().drill_length))
            .on_hover_text(t().drill_length_help);
        let mut drill = self.doc().settings.drill_length;
        if super::mm_field(ui, &mut drill, 1000.0).changed() {
            self.session
                .edit_grouped("drill", |d| d.settings.drill_length = drill);
        }
        ui.separator();
        ui.label(fill(t().screw_total, &[&self.doc().screws.len()]));
    }
}
