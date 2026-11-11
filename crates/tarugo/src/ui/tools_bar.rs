//! Tools of the current stage, in a column at the left.

use eframe::egui::Ui;

use crate::app::{App, Stage};
use crate::i18n::t;
use crate::icons;
use crate::theme;
use crate::tools::Tool;

impl App {
    pub(crate) fn tools_bar(&mut self, ui: &mut Ui) {
        let tools: &[(Tool, &str, &str)] = match self.stage {
            Stage::Front => &[
                (Tool::Select, icons::SELECT, t().tool_select),
                (
                    Tool::Divide(0),
                    icons::DIVIDE_VERTICAL,
                    t().tool_divide_vertical,
                ),
                (
                    Tool::Divide(1),
                    icons::DIVIDE_HORIZONTAL,
                    t().tool_divide_horizontal,
                ),
            ],
            Stage::Design => &[
                (Tool::Select, icons::SELECT, t().tool_select),
                (Tool::AddBoard, icons::ADD_BOARD, t().tool_add_board),
                (Tool::AddStick, icons::ADD_STICK, t().tool_add_stick),
            ],
            _ => &[
                (Tool::Select, icons::SELECT, t().tool_select),
                (Tool::Screw, icons::SCREW, t().tool_screw),
            ],
        };
        ui.spacing_mut().item_spacing.y = 4.0;
        for (tool, ico, name) in tools {
            if theme::toggle(ui, self.tool == *tool, ico, name).clicked() {
                self.set_tool(*tool);
            }
        }
        ui.separator();
        if theme::icon_button(ui, icons::FIT, &format!("{}  (F)", t().fit_view)).clicked() {
            self.fit_view();
        }
        if self.stage != Stage::Front {
            let perspective = self.camera.perspective;
            if theme::toggle(ui, perspective, icons::PERSPECTIVE, t().perspective).clicked() {
                self.camera.perspective = !perspective;
            }
        }
    }
}
