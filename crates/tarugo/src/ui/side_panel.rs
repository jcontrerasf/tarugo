//! Right panel: parts, properties of the selection, parameters and warnings.

use eframe::egui::{self, RichText, Ui};
use tarugo_core::analysis::Issue;
use tarugo_core::expr::format_number;
use tarugo_core::model::{Dim, PartId};

use crate::app::App;
use crate::i18n::{fill, t};
use crate::icons;
use crate::theme::{self, pal};
use crate::ui::{dim_field, error_text, text_commit};

impl App {
    pub(crate) fn side_panel(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(8.0);
                self.warnings_section(ui);
                if self.session.selected.is_some() {
                    theme::section_title(ui, t().properties);
                    self.properties(ui);
                    ui.add_space(10.0);
                }
                theme::section_title(ui, t().parts);
                self.parts_list(ui);
                ui.add_space(10.0);
                theme::section_title(ui, t().parameters);
                self.params_section(ui);
                ui.add_space(12.0);
            });
    }

    fn parts_list(&mut self, ui: &mut Ui) {
        let p = pal();
        let mut select = None;
        let mut lock = None;
        let doc = self.doc();
        if doc.parts.is_empty() {
            ui.label(theme::soft(t().no_parts));
        }
        for part in &doc.parts {
            let selected = self.session.selected == Some(part.id);
            ui.horizontal(|ui| {
                let (ico, help) = if part.locked {
                    (icons::LOCK, t().unlock_part)
                } else {
                    (icons::LOCK_OPEN, t().lock_part)
                };
                if theme::mini(ui, part.locked, ico, help).clicked() {
                    lock = Some((part.id, !part.locked));
                }
                let color = doc.material(part.material).map_or([128; 3], |m| m.color);
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().rect(
                    rect,
                    2.0,
                    egui::Color32::from_rgb(color[0], color[1], color[2]),
                    egui::Stroke::new(1.0, p.border),
                    egui::StrokeKind::Inside,
                );
                let wrong = self.scene.bad.contains(&part.id);
                let size = self.scene.get(part.id).map(|b| b.size());
                let mut text = RichText::new(&part.name);
                if wrong {
                    text = text.color(p.error);
                }
                let response = ui.selectable_label(selected, text);
                if let Some(s) = size {
                    let s = s.map(format_number);
                    ui.label(theme::soft(format!("{} × {} × {}", s[0], s[1], s[2])).size(11.0));
                }
                if response.clicked() {
                    select = Some(part.id);
                }
            });
        }
        if let Some(id) = select {
            self.session.select(Some(id));
            self.highlight.clear();
        }
        if let Some((id, locked)) = lock {
            self.cancel_gesture();
            if locked && self.shape_editor == Some(id) {
                self.shape_editor = None;
            }
            self.session.edit(|d| {
                if let Some(p) = d.part_mut(id) {
                    p.locked = locked;
                }
            });
        }
    }

    fn params_section(&mut self, ui: &mut Ui) {
        let p = pal();
        let params = self.doc().params.clone();
        let mut delete = None;
        egui::Grid::new("params")
            .num_columns(4)
            .spacing([6.0, 4.0])
            .show(ui, |ui| {
                for param in &params {
                    if let Some(name) = text_commit(ui, ("pname", &param.name), &param.name, 56.0) {
                        let old = param.name.clone();
                        let ok = self.session.edit(|d| d.rename_param(&old, name.trim()));
                        if !ok {
                            self.session.undo();
                            self.error(t().bad_param_name);
                        }
                    }
                    let solved = &self.scene.solved;
                    if let Some(dim) =
                        dim_field(ui, ("pvalue", &param.name), &param.value, solved, 90.0)
                    {
                        let name = param.name.clone();
                        self.session.edit(|d| d.set_param(&name, dim));
                    }
                    match self.scene.solved.params.get(&param.name) {
                        Some(Ok(v)) => {
                            ui.label(theme::soft(format!("= {}", format_number(*v))));
                        }
                        Some(Err(e)) => {
                            ui.label(RichText::new(icons::ERROR).color(p.error))
                                .on_hover_text(error_text(e));
                        }
                        None => {
                            ui.label("");
                        }
                    }
                    if theme::mini(ui, false, icons::DELETE, t().delete).clicked() {
                        delete = Some(param.name.clone());
                    }
                    ui.end_row();
                }
            });
        if let Some(name) = delete {
            let mut probe = self.doc().clone();
            if probe.param_in_use(&name) {
                self.error(fill(t().param_in_use, &[&name]));
            } else {
                self.session.edit(|d| d.params.retain(|p| p.name != name));
            }
        }
        if ui
            .button(format!("{} {}", icons::ADD, t().add_parameter))
            .on_hover_text(t().parameters_help)
            .clicked()
        {
            self.session.edit(|d| {
                let name = d.free_param_name("a");
                d.set_param(&name, Dim::num(100.0));
            });
        }
    }

    /// Text of an issue, and whether it is an error.
    pub fn issue_text(&self, issue: &Issue) -> String {
        let name = |id: &PartId| self.part_label(*id);
        match issue {
            Issue::Broken(p) => {
                let why = self
                    .scene
                    .solved
                    .errors
                    .get(p)
                    .map(|e| self.solve_error_text(e))
                    .unwrap_or_default();
                fill(t().issue_broken, &[&name(p), &why])
            }
            Issue::Inverted(p) => fill(t().issue_inverted, &[&name(p)]),
            Issue::Overlap(o) if o.contained => fill(t().issue_inside, &[&name(&o.a), &name(&o.b)]),
            Issue::Overlap(o) => fill(t().issue_overlap, &[&name(&o.a), &name(&o.b)]),
            Issue::Shape(p) => fill(t().issue_shape, &[&name(p)]),
            Issue::Floating(p) => fill(t().issue_floating, &[&name(p)]),
            Issue::FewScrews { part, screws } => match screws {
                0 => fill(t().issue_no_screws_part, &[&name(part)]),
                _ => fill(t().issue_one_screw_part, &[&name(part)]),
            },
            Issue::NoScrews { a, b } => fill(t().issue_joint_no_screws, &[&name(a), &name(b)]),
            Issue::SingleScrew { a, b } => fill(t().issue_joint_one_screw, &[&name(a), &name(b)]),
            Issue::Blocked { a, b, blocker } => {
                fill(t().issue_blocked, &[&name(a), &name(b), &name(blocker)])
            }
            Issue::Tight { a, b, room } => fill(
                t().issue_tight,
                &[
                    &name(a),
                    &name(b),
                    &format_number(*room),
                    &format_number(self.doc().settings.drill_length),
                ],
            ),
            Issue::Cramped {
                a,
                b,
                blocker,
                room,
            } => fill(
                t().issue_cramped,
                &[
                    &name(a),
                    &name(b),
                    &name(blocker),
                    &format_number(*room),
                    &format_number(self.doc().settings.drill_width / 2.0),
                ],
            ),
            Issue::OrphanScrew(_) => t().issue_orphan_screw.to_string(),
        }
    }

    pub fn solve_error_text(&self, e: &tarugo_core::solve::SolveError) -> String {
        use tarugo_core::solve::SolveError;
        match e {
            SolveError::Expr(e) => error_text(e),
            SolveError::MissingPart(_) => t().err_missing_part.to_string(),
            SolveError::Cycle => t().err_part_cycle.to_string(),
            SolveError::MissingMaterial => t().err_missing_material.to_string(),
        }
    }

    fn warnings_section(&mut self, ui: &mut Ui) {
        let p = pal();
        let mut issues: Vec<Issue> = self.scene.geometry.clone();
        if self.analysis_on {
            issues.extend(self.scene.fastening.iter().cloned());
        }
        if issues.is_empty() && !self.analysis_on {
            return;
        }
        theme::section_title(ui, t().warnings);
        if issues.is_empty() {
            ui.label(
                RichText::new(format!("{} {}", icons::ALL_GOOD, t().analysis_ok))
                    .color(p.text_soft),
            );
        }
        let mut clicked = None;
        let mut remove_orphans = false;
        for issue in &issues {
            let (ico, color) = if issue.is_error() {
                (icons::ERROR, p.error)
            } else {
                (icons::WARNING, p.warning)
            };
            let parts = issue.parts();
            let active = !parts.is_empty() && self.highlight == parts;
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(ico).color(color));
                let text = self.issue_text(issue);
                let response = ui.add(
                    egui::Button::selectable(active, text)
                        .wrap()
                        .frame_when_inactive(false),
                );
                if response.clicked() {
                    clicked = Some(parts.clone());
                }
            });
            remove_orphans |= matches!(issue, Issue::OrphanScrew(_));
        }
        if remove_orphans && ui.button(t().remove_orphan_screws).clicked() {
            let placed: Vec<u32> = self.scene.places.iter().map(|p| p.id).collect();
            self.session
                .edit(|d| d.screws.retain(|s| placed.contains(&s.id)));
        }
        if self.analysis_on && ui.small_button(t().hide_analysis).clicked() {
            self.analysis_on = false;
            self.highlight.clear();
        }
        if let Some(parts) = clicked {
            if self.highlight == parts {
                self.highlight.clear();
            } else {
                self.session.select(parts.first().copied());
                self.highlight = parts;
            }
        }
        ui.add_space(10.0);
    }
}
