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
