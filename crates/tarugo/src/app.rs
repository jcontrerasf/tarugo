//! Application state and the actions the interface triggers.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, Pos2, Rect};
use tarugo_core::edit;
use tarugo_core::front::{self, FrameStyle};
use tarugo_core::material::MaterialId;
use tarugo_core::model::{DEPTH, Dim, Document, FaceRef, PartId};

use crate::history::Session;
use crate::i18n::{self, fill, t};
use crate::prefs::{self, Prefs};
use crate::scene::Scene;
use crate::shape_mode::ShapeEdit;
use crate::theme::{self, pal};
use crate::tools::{AddOptions, DivideOptions, Drag, ScrewOptions, Tool};
use crate::ui::cota::CotaEdit;
use crate::ui::dialogs::Dialog;
use crate::view3d::camera::Camera;
use crate::view3d::mesh::{Mesh, rgb_color};
use crate::view3d::renderer::Renderer;

pub const APP: &str = "Tarugo";
pub const APP_ID: &str = "tarugo";
/// File extension of the designs.
pub const EXT: &str = "tarugo";
/// Window icon.
pub const ICON: &[u8] = include_bytes!("../assets/icon.png");

/// Steps of a design; each one shows its own tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Materials,
    Front,
    Design,
    Screws,
    CutList,
}

impl Stage {
    pub const ALL: [Stage; 5] = [
        Stage::Materials,
        Stage::Front,
        Stage::Design,
        Stage::Screws,
        Stage::CutList,
    ];

    /// Does this stage show the 3D view?
    pub fn has_view(self) -> bool {
        matches!(self, Stage::Front | Stage::Design | Stage::Screws)
    }
}

/// Something that needs the unsaved changes dealt with first.
#[derive(Clone, Debug)]
pub enum Pending {
    New,
    Open(PathBuf),
    Close,
}

pub struct App {
    pub session: Session,
    pub scene: Scene,
    scene_revision: Option<u64>,
    pub prefs: Prefs,
    pub stage: Stage,
    pub tool: Tool,
    pub renderer: Option<Renderer>,
    /// What the mesh on the GPU was built from: document revision and theme.
    mesh_key: Option<(u64, bool)>,
    pub camera: Camera,
    pub front_camera: Camera,
    pub fit_pending: bool,
    /// The view still shows what the last fit framed (no pan or zoom since): it is
    /// framed again when the view changes size.
    pub framed: Option<egui::Vec2>,
    /// Where the 3D view was last drawn.
    pub view_rect: Rect,
    pub hover: Option<PartId>,
    /// Parts of the issue clicked in the warnings list.
    pub highlight: Vec<PartId>,
    pub drag: Option<Drag>,
    /// Face another part snapped to in the gesture in progress (to mark it).
    pub snap_mark: Vec<FaceRef>,
    pub divide: DivideOptions,
    pub add: AddOptions,
    pub screws: ScrewOptions,
    /// The fastening analysis was asked for: its issues are listed.
    pub analysis_on: bool,
    pub cota_edit: Option<CotaEdit>,
    /// Free-form mode: the part whose outline is being edited in the view.
    pub shape_editor: Option<ShapeEdit>,
    pub dialog: Option<Dialog>,
    pub pending: Option<Pending>,
    /// Message for the status bar; true if it is an error.
    pub message: Option<(String, bool)>,
    /// World position under the pointer, to show in the status bar.
    pub pointer_world: Option<[f64; 3]>,
    close_allowed: bool,
    /// Scripted run (debug builds only).
    #[cfg(debug_assertions)]
    pub demo: Option<crate::demo::Demo>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext, initial: Option<PathBuf>) -> App {
        let prefs: Prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, prefs::KEY))
            .unwrap_or_default();
        i18n::set_lang(prefs.lang);
        theme::install(&cc.egui_ctx, prefs.theme);
        cc.egui_ctx.set_zoom_factor(prefs.ui_zoom.clamp(0.5, 3.0));
        let mut app = App {
            session: Session::new(new_document(prefs.frame_style), None),
            scene: Scene::default(),
            scene_revision: None,
            stage: Stage::Front,
            tool: Tool::Select,
            renderer: cc.wgpu_render_state.as_ref().map(Renderer::new),
            mesh_key: None,
            camera: Camera {
                perspective: prefs.perspective,
                ..Camera::default()
            },
            front_camera: Camera::front(),
            fit_pending: true,
            framed: None,
            view_rect: Rect::NOTHING,
            hover: None,
            highlight: Vec::new(),
            drag: None,
            snap_mark: Vec::new(),
            divide: DivideOptions::default(),
            add: AddOptions::default(),
            screws: ScrewOptions::default(),
            analysis_on: false,
            cota_edit: None,
            shape_editor: None,
            dialog: None,
            pending: None,
            message: None,
            pointer_world: None,
            close_allowed: false,
            #[cfg(debug_assertions)]
            demo: crate::demo::Demo::from_env(),
            prefs,
        };
        if let Some(path) = initial {
            app.open_path(&path);
        }
        app.refresh_scene();
        app
    }

    pub fn doc(&self) -> &Document {
        self.session.doc()
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), false));
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), true));
    }

    pub fn doc_name(&self) -> String {
        self.session
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| t().untitled.to_string())
    }

    /// Solves the document again if it changed.
    pub fn refresh_scene(&mut self) {
        let revision = self.session.revision();
        if self.scene_revision != Some(revision) {
            self.scene = Scene::build(self.session.doc());
            self.scene_revision = Some(revision);
            self.session.prune_selection();
        }
    }

    /// Sends the parts to the GPU again if the document or the theme changed.
    pub fn refresh_mesh(&mut self, dark: bool) {
        let key = (self.session.revision(), dark);
        if self.mesh_key == Some(key) {
            return;
        }
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let p = pal();
        let edge = rgb_color([p.edge.r(), p.edge.g(), p.edge.b()]);
        let wrong = [p.error.r(), p.error.g(), p.error.b()];
        let mut mesh = Mesh::default();
        let doc = self.session.doc();
        for part in &doc.parts {
            let Some(b) = self.scene.get(part.id) else {
                continue;
            };
            let mut color = doc.material(part.material).map_or([200; 3], |m| m.color);
            if self.scene.bad.contains(&part.id) {
                // Halfway to the error color.
                for (c, w) in color.iter_mut().zip(wrong) {
                    *c = ((*c as u16 + w as u16 * 2) / 3) as u8;
                }
            }
            let outline = self
                .scene
                .outlines
                .get(&part.id)
                .map(|(axis, o)| (*axis, o));
            mesh.add_part(b, outline, rgb_color(color), edge);
        }
        renderer.set_mesh(&mesh);
        self.mesh_key = Some(key);
    }

    /// The camera of the current stage.
    pub fn view_camera(&mut self) -> &mut Camera {
        if self.stage == Stage::Front {
            &mut self.front_camera
        } else {
            &mut self.camera
        }
    }

    pub fn set_stage(&mut self, stage: Stage) {
        if stage == self.stage {
            return;
        }
        // The depth is chosen once, when leaving the front sketch for the 3D design.
        if matches!(stage, Stage::Design | Stage::Screws) && !self.doc().depth_set {
            let depth = self
                .doc()
                .param(DEPTH)
                .map_or("400".to_string(), |p| p.value.0.clone());
            self.dialog = Some(Dialog::Depth {
                text: depth,
                then: stage,
            });
            return;
        }
        self.enter_stage(stage);
    }

    pub fn enter_stage(&mut self, stage: Stage) {
        self.cancel_gesture();
        self.leave_shape();
        self.stage = stage;
        self.tool = match stage {
            Stage::Screws => Tool::Screw,
            _ => Tool::Select,
        };
        if stage != Stage::Screws {
            self.session.selected_screw = None;
        }
        self.fit_pending = true;
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.cancel_gesture();
        self.tool = tool;
        self.message = None;
    }

    /// Drops the gesture and the inline editing in progress.
    pub fn cancel_gesture(&mut self) {
        if self.drag.take().is_some() {
            self.session.cancel_group();
        }
        self.snap_mark.clear();
        self.cota_edit = None;
    }

    pub fn replace_document(&mut self, doc: Document, path: Option<PathBuf>) {
        self.cancel_gesture();
        self.session = Session::new(doc, path);
        self.scene_revision = None;
        self.mesh_key = None;
        self.highlight.clear();
        self.shape_editor = None;
        self.analysis_on = false;
        self.stage = if self.doc().depth_set {
            Stage::Design
        } else {
            Stage::Front
        };
        self.tool = Tool::Select;
        self.fit_pending = true;
        self.refresh_scene();
    }

    /// Does `action` now, or after asking about the unsaved changes.
    pub fn request(&mut self, action: Pending) {
        if self.session.dirty {
            self.pending = Some(action);
        } else {
            self.run_pending(action);
        }
    }

    pub fn run_pending(&mut self, action: Pending) {
        match action {
            Pending::New => {
                let doc = new_document(self.prefs.frame_style);
                self.replace_document(doc, None);
            }
            Pending::Open(path) => self.open_path(&path),
            Pending::Close => self.close_allowed = true,
        }
    }

    pub fn open_path(&mut self, path: &Path) {
        let result = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| Document::from_json(&text));
        match result {
            Ok(doc) => {
                self.replace_document(doc, Some(path.to_path_buf()));
                self.prefs.add_recent(path.to_path_buf());
                self.message = None;
            }
            Err(e) => self.error(fill(t().cannot_open, &[&path.display(), &e])),
        }
    }

    pub fn pick_open(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter(t().filter_tarugo, &[EXT])
            .pick_file();
        if let Some(path) = picked {
            self.request(Pending::Open(path));
        }
    }

    /// Saves; asks for a name if there is none or `as_new`. False if not saved.
    pub fn save(&mut self, as_new: bool) -> bool {
        let path = match (&self.session.path, as_new) {
            (Some(p), false) => p.clone(),
            _ => {
                let picked = rfd::FileDialog::new()
                    .add_filter(t().filter_tarugo, &[EXT])
                    .set_file_name(format!("{}.{EXT}", self.doc_name()))
                    .save_file();
                match picked {
                    Some(p) if p.extension().is_none() => p.with_extension(EXT),
                    Some(p) => p,
                    None => return false,
                }
            }
        };
        match std::fs::write(&path, self.doc().to_json()) {
            Ok(()) => {
                self.session.dirty = false;
                self.session.path = Some(path.clone());
                self.prefs.add_recent(path);
                self.info(t().saved);
                true
            }
            Err(e) => {
                self.error(fill(t().cannot_save, &[&path.display(), &e]));
                false
            }
        }
    }

    pub fn undo(&mut self) {
        self.cancel_gesture();
        self.session.undo();
    }

    pub fn redo(&mut self) {
        self.cancel_gesture();
        self.session.redo();
    }

    /// Is the part locked against moving and editing?
    pub fn is_locked(&self, id: PartId) -> bool {
        self.doc().part(id).is_some_and(|p| p.locked)
    }

    /// Says why a locked part did not change.
    pub fn warn_locked(&mut self, id: PartId) {
        let name = self.part_label(id);
        self.info(fill(t().part_locked, &[&name]));
    }

    pub fn delete_selection(&mut self) {
        self.cancel_gesture();
        if let Some(id) = self.session.selected_screw {
            self.session.edit(|d| d.screws.retain(|s| s.id != id));
            self.session.selected_screw = None;
        } else if let Some(id) = self.session.selected {
            if self.is_locked(id) {
                self.warn_locked(id);
                return;
            }
            let solved = self.scene.solved.clone();
            self.session.edit(|d| edit::remove_part(d, &solved, id));
            self.session.selected = None;
            if self.shape_editor.as_ref().is_some_and(|e| e.part == id) {
                self.leave_shape();
            }
        }
    }

    /// Copies the selected part beside it along `axis`.
    pub fn duplicate_beside(&mut self, axis: usize, before: bool) {
        let Some(id) = self.session.selected else {
            return;
        };
        let Some(name) = self.doc().part(id).map(|p| p.name.clone()) else {
            return;
        };
        let solved = self.scene.solved.clone();
        let new = self
            .session
            .edit(|d| edit::duplicate_beside(d, &solved, id, axis, before, name));
        if new.is_some() {
            self.session.select(new);
        }
    }

    /// Sets the depth chosen after the front sketch.
    pub fn confirm_depth(&mut self, depth: Dim, then: Stage) {
        self.session.edit(|d| {
            d.set_param(DEPTH, depth);
            d.depth_set = true;
        });
        self.enter_stage(then);
    }

    /// Material for new parts: the one picked in the tool, or the main one.
    pub fn add_material(&self, board: bool) -> Option<MaterialId> {
        let doc = self.doc();
        let fits = |id: MaterialId| doc.material(id).is_some_and(|m| m.is_board() == board);
        self.add
            .material
            .filter(|id| fits(*id))
            .or(Some(doc.main_material).filter(|id| fits(*id)))
            .or_else(|| {
                doc.materials
                    .iter()
                    .find(|m| m.is_board() == board)
                    .map(|m| m.id)
            })
    }

    pub fn fit_view(&mut self) {
        self.fit_pending = true;
    }

    /// Looks from a standard direction.
    pub fn look_from(&mut self, yaw: f32, pitch: f32) {
        self.camera.yaw = yaw;
        self.camera.pitch = pitch;
        self.fit_pending = true;
    }

    fn update_title(&self, ctx: &egui::Context) {
        let mark = if self.session.dirty { "*" } else { "" };
        let title = format!("{mark}{} — {APP}", self.doc_name());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_allowed {
            if self.session.dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending = Some(Pending::Close);
            }
        } else if self.close_allowed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let hit = |mods: Modifiers, key: Key| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(mods, key)))
        };
        let ctrl = Modifiers::COMMAND;
        let ctrl_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        if hit(ctrl_shift, Key::Z) || hit(ctrl, Key::Y) {
            self.redo();
        } else if hit(ctrl, Key::Z) {
            self.undo();
        }
        if hit(ctrl_shift, Key::S) {
            self.save(true);
        } else if hit(ctrl, Key::S) {
            self.save(false);
        }
        if hit(ctrl, Key::O) {
            self.pick_open();
        }
        if hit(ctrl, Key::N) {
            self.request(Pending::New);
        }
        if typing {
            return;
        }
        if self.shape_editor.is_some() {
            if hit(Modifiers::NONE, Key::Delete)
                || (hit(Modifiers::NONE, Key::Backspace) && !self.shape_backspace())
            {
                self.shape_delete();
            }
            if hit(Modifiers::NONE, Key::Escape) {
                self.shape_escape();
            }
            if hit(Modifiers::NONE, Key::Enter) {
                self.shape_enter();
            }
            if hit(Modifiers::NONE, Key::F) {
                self.shape_fit();
            }
            return;
        }
        if hit(Modifiers::NONE, Key::Delete) || hit(Modifiers::NONE, Key::Backspace) {
            self.delete_selection();
        }
        if hit(Modifiers::NONE, Key::Escape) {
            if self.drag.is_some() || self.cota_edit.is_some() {
                self.cancel_gesture();
            } else if self.tool != Tool::Select && self.stage != Stage::Screws {
                self.set_tool(Tool::Select);
            } else {
                self.session.select(None);
                self.highlight.clear();
            }
        }
        if hit(Modifiers::NONE, Key::F) {
            self.fit_view();
        }
    }

    /// World position of a screen point on the plane of the front of the furniture.
    pub fn front_point(&self, pos: Pos2) -> Option<[f64; 2]> {
        let (origin, dir) = self.front_camera.ray(self.view_rect, pos);
        let z = self.scene.bounds().map_or(0.0, |(_, max)| max.z);
        let p = crate::view3d::ray_plane(glam::Vec3::new(0.0, 0.0, z), glam::Vec3::Z, origin, dir)?;
        Some([p.x as f64, p.y as f64])
    }

    /// Where another part's face is, to draw dimensions from it.
    pub fn face_value(&self, face: FaceRef, axis: usize) -> Option<f64> {
        let b = self.scene.get(face.part)?;
        Some(face.side.of(b.min[axis], b.max[axis]))
    }
}

/// A new design: a square frame out of an 18 mm board.
pub fn new_document(style: FrameStyle) -> Document {
    let mut doc = Document::new();
    doc.materials[0].name = t().default_material.to_string();
    let names = [t().part_left, t().part_right, t().part_bottom, t().part_top];
    let frame = front::new_frame(&mut doc, 600.0, 600.0, 400.0, names);
    front::set_frame_style(&mut doc, &frame, style);
    doc
}

/// Name for a new part: `base` plus the first number not in use.
pub fn part_name(doc: &Document, base: &str) -> String {
    (1..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !doc.parts.iter().any(|p| &p.name == name))
        .unwrap_or_default()
}

impl eframe::App for App {
    #[cfg(debug_assertions)]
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(demo) = &mut self.demo {
            demo.feed(raw_input);
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::sync(ctx);
        #[cfg(debug_assertions)]
        self.demo_step(ctx);
        if self.pending.is_none() && self.dialog.is_none() {
            self.shortcuts(ctx);
        }
        // A grouped edit (typing in a field, dragging a value) ends when the mouse is
        // released and no field has focus.
        let idle = ctx.input(|i| !i.pointer.any_down())
            && ctx.memory(|m| m.focused()).is_none()
            && self.drag.is_none();
        if idle {
            self.session.end_group();
        }
        self.refresh_scene();
        self.update_title(ctx);
        self.handle_close(ctx);
        self.prefs.ui_zoom = ctx.zoom_factor();
        self.prefs.perspective = self.camera.perspective;
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, prefs::KEY, &self.prefs);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let p = pal();
        let bar = egui::Frame::NONE
            .fill(p.panel)
            .inner_margin(egui::Margin::symmetric(10, 6));
        let line = egui::Stroke::new(1.0, p.border);
        egui::Panel::top("top_bar")
            .frame(bar)
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status_bar")
            .frame(bar)
            .show(ui, |ui| self.status_bar(ui));
        if self.stage.has_view() {
            egui::Panel::top("context_bar")
                .frame(
                    egui::Frame::NONE
                        .fill(p.window)
                        .stroke(line)
                        .inner_margin(egui::Margin::symmetric(10, 4)),
                )
                .show(ui, |ui| {
                    if self.shape_editor.is_some() {
                        self.shape_context_bar(ui);
                    } else {
                        self.context_bar(ui);
                    }
                });
            egui::Panel::left("tools")
                .resizable(false)
                .exact_size(46.0)
                .frame(egui::Frame::NONE.fill(p.panel).inner_margin(6))
                .show(ui, |ui| {
                    if self.shape_editor.is_some() {
                        self.shape_tools_bar(ui);
                    } else {
                        self.tools_bar(ui);
                    }
                });
            egui::Panel::right("side_panel")
                .resizable(true)
                .default_size(350.0)
                .min_size(300.0)
                .frame(
                    egui::Frame::NONE
                        .fill(p.panel)
                        .inner_margin(egui::Margin::symmetric(10, 0)),
                )
                .show(ui, |ui| {
                    if self.shape_editor.is_some() {
                        self.shape_panel(ui);
                    } else {
                        self.side_panel(ui);
                    }
                });
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(p.backdrop))
                .show(ui, |ui| {
                    if self.shape_editor.is_some() {
                        self.shape_canvas(ui);
                    } else {
                        self.canvas(ui);
                    }
                });
        } else {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(p.window).inner_margin(20))
                .show(ui, |ui| match self.stage {
                    Stage::Materials => self.materials_page(ui),
                    _ => self.cut_list_page(ui),
                });
        }
        self.dialogs(&ctx);
        self.confirm_dialog(&ctx);
    }
}
