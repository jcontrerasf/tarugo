//! Scripted runs for debug builds: drives the app with fake input and saves screenshots,
//! to check the interface end to end without a person at the mouse.
//!
//! `TARUGO_DEMO="stage:design;click:400:300;shot:/tmp/a.ppm;quit" cargo run -p tarugo`
//!
//! Actions (separated by `;`): `stage:<materials|front|design|screws|cutlist>`,
//! `tool:<select|dividev|divideh|board|stick|screw>`, `move:x:y`, `click:x:y`,
//! `drag:x1:y1:x2:y2`, `type:text`, `key:<enter|escape|delete>`, `view:yaw:pitch`,
//! `depth:<mm>`, `autoscrews`, `analyze`, `screws:<on|off>` (shown in the 3D design), `shape`
//! (free-form mode on the selection), `shapetool:<select|rect|circle|arc|slot|polygon|notch|guide>`,
//! `shift:<on|off>` (held),
//! `lang:<es|en>`, `dark`, `light`, `save:path`, `wait:frames`,
//! `shot:path.ppm` and `quit`. Positions are in points
//! from the top-left corner of the window.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use tarugo_core::model::Dim;

use crate::app::{App, Stage};
use crate::i18n::{self, Lang};
use crate::shape_mode::ShapeTool;
use crate::theme::{self, ThemeChoice};
use crate::tools::{self, Tool};

/// Frames between two actions, for the interface to settle.
const PACE: u64 = 3;

pub struct Demo {
    actions: VecDeque<String>,
    /// Input events still to feed, one batch per frame.
    events: VecDeque<Vec<Event>>,
    frame: u64,
    /// Frame before which no new action starts.
    resume_at: u64,
    /// A screenshot was asked for and has not arrived.
    waiting_shot: Arc<AtomicBool>,
}

impl Demo {
    pub fn from_env() -> Option<Demo> {
        let script = std::env::var("TARUGO_DEMO").ok()?;
        Some(Demo {
            actions: script
                .split(';')
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect(),
            events: VecDeque::new(),
            frame: 0,
            resume_at: 10,
            waiting_shot: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Feeds the next batch of fake input.
    pub fn feed(&mut self, raw: &mut egui::RawInput) {
        if let Some(events) = self.events.pop_front() {
            raw.events.extend(events);
        }
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    fn click(&mut self, pos: Pos2) {
        self.events.push_back(vec![Event::PointerMoved(pos)]);
        self.events.push_back(vec![Self::button(pos, true)]);
        self.events.push_back(vec![Self::button(pos, false)]);
    }

    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.events.push_back(vec![Event::PointerMoved(from)]);
        self.events.push_back(vec![Self::button(from, true)]);
        for step in 1..=8 {
            let pos = from + (to - from) * (step as f32 / 8.0);
            self.events.push_back(vec![Event::PointerMoved(pos)]);
        }
        self.events.push_back(vec![Self::button(to, false)]);
    }

    fn key(&mut self, key: Key) {
        for pressed in [true, false] {
            self.events.push_back(vec![Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]);
        }
    }
}

fn pos(args: &[&str], at: usize) -> Option<Pos2> {
    Some(Pos2::new(
        args.get(at)?.parse().ok()?,
        args.get(at + 1)?.parse().ok()?,
    ))
}

fn save_ppm(path: &PathBuf, image: &egui::ColorImage) {
    let mut data = format!("P6\n{} {}\n255\n", image.width(), image.height()).into_bytes();
    for pixel in &image.pixels {
        data.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b()]);
    }
    if let Err(e) = std::fs::write(path, data) {
        eprintln!("demo: cannot write {}: {e}", path.display());
    }
}

impl App {
    /// Runs the next step of the script, if it is time.
    pub(crate) fn demo_step(&mut self, ctx: &egui::Context) {
        let Some(demo) = &mut self.demo else {
            return;
        };
        ctx.request_repaint();
        demo.frame += 1;
        if !demo.events.is_empty() {
            // Let the interface react to the last of the input before going on.
            demo.resume_at = demo.frame + PACE;
            return;
        }
        if demo.frame < demo.resume_at || demo.waiting_shot.load(Ordering::Relaxed) {
            return;
        }
        demo.resume_at = demo.frame + PACE;
        let Some(action) = demo.actions.pop_front() else {
            return;
        };
        let parts: Vec<&str> = action.split(':').collect();
        let args = &parts[1..];
        let arg = args.first().copied().unwrap_or("");
        match parts[0] {
            "stage" => {
                let stage = match arg {
                    "materials" => Stage::Materials,
                    "front" => Stage::Front,
                    "design" => Stage::Design,
                    "screws" => Stage::Screws,
                    _ => Stage::CutList,
                };
                self.set_stage(stage);
            }
            "tool" => {
                let tool = match arg {
                    "dividev" => Tool::Divide(0),
                    "divideh" => Tool::Divide(1),
                    "board" => Tool::AddBoard,
                    "stick" => Tool::AddStick,
                    "screw" => Tool::Screw,
                    _ => Tool::Select,
                };
                self.set_tool(tool);
            }
            "move" => {
                if let Some(p) = pos(args, 0) {
                    demo.events.push_back(vec![Event::PointerMoved(p)]);
                }
            }
            "click" => {
                if let Some(p) = pos(args, 0) {
                    demo.click(p);
                }
            }
            "drag" => {
                if let (Some(a), Some(b)) = (pos(args, 0), pos(args, 2)) {
                    demo.drag(a, b);
                }
            }
            "type" => demo.events.push_back(vec![Event::Text(args.join(":"))]),
            "key" => {
                let key = match arg {
                    "enter" => Key::Enter,
                    "escape" => Key::Escape,
                    _ => Key::Delete,
                };
                demo.key(key);
            }
            "view" => {
                if let (Some(yaw), Some(pitch)) = (
                    args.first().and_then(|a| a.parse().ok()),
                    args.get(1).and_then(|a| a.parse().ok()),
                ) {
                    self.look_from(yaw, pitch);
                }
            }
            "depth" => {
                let depth = Dim(arg.to_string());
                self.dialog = None;
                self.confirm_depth(depth, Stage::Design);
            }
            "autoscrews" => {
                let mut doc = self.doc().clone();
                tools::screw::fill_all(&mut doc, &self.scene, self.screws.count);
                self.session.edit(|d| *d = doc);
            }
            "analyze" => self.analysis_on = true,
            "screws" => self.prefs.show_screws = arg != "off",
            "save" => {
                self.session.path = Some(PathBuf::from(args.join(":")));
                self.save(false);
            }
            "shape" => {
                if let Some(id) = self.session.selected {
                    self.enter_shape(id);
                }
            }
            "shift" => {
                let modifiers = if arg == "off" {
                    Modifiers::NONE
                } else {
                    Modifiers::SHIFT
                };
                demo.events
                    .push_back(vec![Event::ModifiersChanged(modifiers)]);
            }
            "shapetool" => {
                let tool = match arg {
                    "rect" => ShapeTool::Rect,
                    "circle" => ShapeTool::Circle,
                    "arc" => ShapeTool::Arc,
                    "slot" => ShapeTool::Slot,
                    "polygon" => ShapeTool::Polygon,
                    "notch" => ShapeTool::Notch,
                    "guide" => ShapeTool::Guide,
                    _ => ShapeTool::Select,
                };
                if let Some(ed) = &mut self.shape_editor {
                    ed.set_tool(tool);
                }
            }
            "lang" => {
                let lang = if arg == "en" { Lang::En } else { Lang::Es };
                self.prefs.lang = lang;
                i18n::set_lang(lang);
            }
            "dark" => theme::set_choice(ctx, ThemeChoice::Dark),
            "light" => theme::set_choice(ctx, ThemeChoice::Light),
            "wait" => demo.resume_at = demo.frame + arg.parse().unwrap_or(10),
            "shot" => {
                let path = PathBuf::from(args.join(":"));
                let flag = demo.waiting_shot.clone();
                flag.store(true, Ordering::Relaxed);
                let repaint = ctx.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::ScreenshotCallback::new(move |image| {
                        save_ppm(&path, &image);
                        flag.store(false, Ordering::Relaxed);
                        repaint.request_repaint();
                    }),
                ));
            }
            "quit" => {
                self.session.dirty = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            other => eprintln!("demo: unknown action '{other}'"),
        }
    }
}
