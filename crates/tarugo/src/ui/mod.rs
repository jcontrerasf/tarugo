//! Panels and bars around the 3D view, and the widgets they share.

pub mod context_bar;
pub mod cota;
pub mod cut_list;
pub mod dialogs;
pub mod materials;
pub mod properties;
pub mod shape_editor;
pub mod side_panel;
pub mod status_bar;
pub mod tools_bar;
pub mod top_bar;

use std::hash::Hash;

use eframe::egui::{self, Response, Ui};
use tarugo_core::expr::{ExprError, format_number};
use tarugo_core::model::Dim;
use tarugo_core::solve::Solved;

use crate::i18n::{fill, t};
use crate::theme::pal;

/// What is wrong with a formula, in the user's language.
pub fn error_text(e: &ExprError) -> String {
    match e {
        ExprError::Empty => t().err_empty.to_string(),
        ExprError::Syntax(_) => t().err_syntax.to_string(),
        ExprError::UnknownName(n) => fill(t().err_unknown_name, &[n]),
        ExprError::UnknownFunction(n) => fill(t().err_unknown_function, &[n]),
        ExprError::Arity(n) => fill(t().err_arity, &[n]),
        ExprError::DivisionByZero => t().err_division.to_string(),
        ExprError::NotFinite => t().err_not_finite.to_string(),
        ExprError::Cycle(n) => fill(t().err_cycle, &[n]),
    }
}

/// Text field for a dimension: a number or a formula. While it has focus it keeps what is
/// being typed; when it loses it, returns the new dimension if it is valid and different.
/// A formula shows its value on hover; an invalid text is red.
pub fn dim_field(
    ui: &mut Ui,
    salt: impl Hash + std::fmt::Debug,
    dim: &Dim,
    solved: &Solved,
    width: f32,
) -> Option<Dim> {
    let id = ui.scope_id().with(("dim", salt));
    let mut text = ui
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| dim.0.clone());
    let typed = Dim(text.trim().to_string());
    let value = solved.eval(&typed);
    let mut edit = egui::TextEdit::singleline(&mut text)
        .id(id)
        .desired_width(width);
    if value.is_err() {
        edit = edit.text_color(pal().error);
    }
    let response = ui.add(edit);
    let response = match &value {
        Ok(v) if typed.is_formula() => response.on_hover_text(format!("= {}", format_number(*v))),
        Err(e) => response.on_hover_text(error_text(e)),
        _ => response,
    };
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    let committed = Dim(text.trim().to_string());
    (response.lost_focus() && committed != *dim && solved.eval(&committed).is_ok())
        .then_some(committed)
}

/// Like [`dim_field`] for a value that is also set by dragging: a plain number changes
/// while it is dragged sideways (Shift: tenths), never below `min`; a click opens it to type
/// a number or a formula.
pub fn dim_drag(
    ui: &mut Ui,
    salt: impl Hash + std::fmt::Debug + Copy,
    dim: &Dim,
    solved: &Solved,
    width: f32,
    min: f64,
) -> Option<Dim> {
    // The id of the text field of `dim_field`, to give it the focus.
    let id = ui.scope_id().with(("dim", salt));
    let editing_id = id.with("editing");
    let start_id = id.with("drag_start");
    // 1: just opened (the focus is on its way), 2: being typed in.
    let editing = ui.data(|d| d.get_temp::<u8>(editing_id)).unwrap_or(0);
    if editing > 0 {
        if editing == 1 {
            ui.memory_mut(|m| m.request_focus(id));
        }
        let out = dim_field(ui, salt, dim, solved, width);
        if editing == 1
            && let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id)
        {
            // Everything selected: what is typed replaces the value.
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(dim.0.chars().count()),
            );
            state.cursor.set_char_range(Some(all));
            state.store(ui.ctx(), id);
        }
        let next = if editing == 1 || ui.memory(|m| m.has_focus(id)) {
            2u8
        } else {
            0
        };
        ui.data_mut(|d| d.insert_temp(editing_id, next));
        return out;
    }
    let value = solved.eval(dim);
    let number = value.as_ref().ok().copied().filter(|_| !dim.is_formula());
    let size = egui::vec2(width + 8.0, ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let visuals = ui.style().interact(&response);
    ui.painter().rect(
        rect,
        visuals.corner_radius,
        ui.visuals().text_edit_bg_color(),
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let color = if value.is_err() {
        pal().error
    } else {
        visuals.text_color()
    };
    ui.painter().with_clip_rect(rect.shrink(2.0)).text(
        rect.left_center() + egui::vec2(4.0, 0.0),
        egui::Align2::LEFT_CENTER,
        &dim.0,
        egui::TextStyle::Body.resolve(ui.style()),
        color,
    );
    let response = match &value {
        Ok(v) if number.is_none() => response.on_hover_text(format!("= {}", format_number(*v))),
        Err(e) => response.on_hover_text(error_text(e)),
        _ => response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal),
    };
    if response.clicked() {
        ui.data_mut(|d| d.insert_temp(editing_id, 1u8));
    }
    let number = number?;
    if response.drag_started() {
        ui.data_mut(|d| d.insert_temp(start_id, (number, 0.0f32)));
    }
    if !response.dragged() {
        return None;
    }
    let (start, moved) = ui
        .data(|d| d.get_temp::<(f64, f32)>(start_id))
        .unwrap_or((number, 0.0));
    let moved = moved + response.drag_delta().x;
    ui.data_mut(|d| d.insert_temp(start_id, (start, moved)));
    let step = if ui.input(|i| i.modifiers.shift) {
        0.1
    } else {
        1.0
    };
    let new = ((start + moved as f64 * step) / step).round() * step;
    let new = Dim::num((new.max(min) * 10.0).round() / 10.0);
    (new != *dim).then_some(new)
}

/// A length in millimetres edited by dragging or typing. True if it changed.
pub fn mm_field(ui: &mut Ui, value: &mut f64, max: f64) -> Response {
    ui.add(
        egui::DragValue::new(value)
            .speed(1.0)
            .range(0.0..=max)
            .max_decimals(1)
            .suffix(" mm"),
    )
}

/// Like [`mm_field`] for a size that may be left unset (shown as a dash).
pub fn optional_mm_field(ui: &mut Ui, value: &mut Option<f64>, max: f64) -> Response {
    let mut v = value.unwrap_or(0.0);
    let response = ui.add(
        egui::DragValue::new(&mut v)
            .speed(5.0)
            .range(0.0..=max)
            .max_decimals(1)
            .custom_formatter(|v, _| {
                if v <= 0.0 {
                    "—".to_string()
                } else {
                    format!("{} mm", format_number(v))
                }
            })
            .custom_parser(|s| {
                let s = s.trim().trim_end_matches("mm").trim().replace(',', ".");
                if s.is_empty() || s == "—" {
                    Some(0.0)
                } else {
                    s.parse().ok()
                }
            }),
    );
    if response.changed() {
        *value = (v > 0.0).then_some(v);
    }
    response
}

/// Text field that keeps what is typed while it has focus and returns it when it loses it,
/// if it changed.
pub fn text_commit(
    ui: &mut Ui,
    salt: impl Hash + std::fmt::Debug,
    current: &str,
    width: f32,
) -> Option<String> {
    let id = ui.scope_id().with(("text", salt));
    let mut text = ui
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| current.to_string());
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(width),
    );
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    (response.lost_focus() && text != current).then_some(text)
}
