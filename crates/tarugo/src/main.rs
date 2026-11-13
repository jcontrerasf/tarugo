//! Tarugo: design furniture out of boards and lumber.
//!
//! `cargo run -p tarugo -- [design.tarugo]`

// No console window on Windows (kept in debug builds to see messages).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod canvas;
#[cfg(debug_assertions)]
mod demo;
mod history;
mod i18n;
mod icons;
mod prefs;
mod scene;
mod shape_mode;
mod theme;
mod tools;
mod ui;
mod view3d;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(app::APP)
            .with_app_id(app::APP_ID)
            .with_icon(eframe::icon_data::from_png_bytes(app::ICON).expect("valid PNG icon"))
            .with_inner_size([1360.0, 840.0])
            .with_min_inner_size([900.0, 560.0]),
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    eframe::run_native(
        app::APP,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, initial)))),
    )
}

/// On Windows wgpu tries Vulkan first, and some Vulkan drivers/layers crash with an access
/// violation while creating the window. DX12 (with GL as fallback) is always available on
/// Windows 10/11. `WGPU_BACKEND` still takes precedence.
fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    let mut opts = eframe::egui_wgpu::WgpuConfiguration::default();
    if cfg!(windows)
        && std::env::var_os("WGPU_BACKEND").is_none()
        && let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut opts.wgpu_setup
    {
        setup.instance_descriptor.backends =
            eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::GL;
    }
    opts
}
