//! The egui application: owns the config, the camera runtimes and the audio.

mod bar;
mod dialogs;
mod runtime;
mod style;
mod tiles;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Key, RichText, ViewportCommand};

use crate::audio::{AudioOutput, Mixer};
use crate::canvas::{self, Rect};
use crate::config::{Camera, Config, Tile, store};
use crate::stream::ffmpeg;

use bar::BarVisibility;
use dialogs::{CameraForm, Dialog};
use runtime::Runtimes;
use tiles::CanvasState;

/// Wait this long after the last change before writing config.json.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
const NOTICE_TIME: Duration = Duration::from_secs(8);

pub struct VyuApp {
    cfg: Config,
    cfg_path: PathBuf,
    demo: bool,
    rts: Runtimes,
    mixer: Arc<Mutex<Mixer>>,
    _audio: Option<AudioOutput>,
    canvas: CanvasState,
    canvas_size: (f32, f32),
    dialog: Dialog,
    bar: BarVisibility,
    notice: Option<(String, Instant)>,
    dirty_since: Option<Instant>,
    first_frame: bool,
}

impl VyuApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        ffmpeg::init();
        style::apply(&cc.egui_ctx);
        let demo = std::env::var_os("VYU_DEMO").is_some();
        let cfg_path = if demo { store::config_dir().join("config-demo.json") } else { store::config_path() };
        let loaded = store::load(&cfg_path);
        let mut cfg = loaded.config;
        if demo && cfg.cameras.is_empty() {
            add_demo_cameras(&mut cfg);
        }

        let mut notice = None;
        if let Some(backup) = loaded.recovered_backup {
            let name = backup.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            notice = Some((format!("A configuração estava corrompida e foi guardada em {name}. Começando do zero."), Instant::now()));
        }

        let mixer = Arc::new(Mutex::new(Mixer::new()));
        let audio = match AudioOutput::start(mixer.clone()) {
            Ok(a) => Some(a),
            Err(e) => {
                tracing::warn!("sem áudio: {e}");
                notice.get_or_insert((format!("Sem som: {e}"), Instant::now()));
                None
            }
        };

        Self {
            cfg,
            cfg_path,
            demo,
            rts: Runtimes::new(),
            mixer,
            _audio: audio,
            canvas: CanvasState::default(),
            canvas_size: (1280.0, 720.0),
            dialog: Dialog::None,
            bar: BarVisibility::new(Instant::now()),
            notice,
            dirty_since: None,
            first_frame: true,
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty_since = Some(Instant::now());
    }

    fn save_now(&mut self) {
        if let Err(e) = store::save(&self.cfg_path, &self.cfg) {
            tracing::error!("não foi possível salvar a configuração: {e}");
        }
        self.dirty_since = None;
    }

    fn save_if_due(&mut self, ctx: &egui::Context) {
        if let Some(since) = self.dirty_since {
            let left = SAVE_DEBOUNCE.saturating_sub(since.elapsed());
            if left.is_zero() {
                self.save_now();
            } else {
                ctx.request_repaint_after(left);
            }
        }
    }

    fn open_add_camera(&mut self) {
        self.dialog = Dialog::Camera(CameraForm::default());
    }

    fn edit_camera(&mut self, id: uuid::Uuid) {
        if let Some(cam) = self.cfg.camera(id) {
            self.dialog = Dialog::Camera(CameraForm::edit(cam));
        }
    }

    fn remove_camera(&mut self, id: uuid::Uuid) {
        runtime::restart(&mut self.rts, &self.mixer, id);
        self.cfg.remove_camera(id);
        let _ = crate::secrets::delete_password(id);
        self.mark_dirty();
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog.is_open() || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        let (f11, esc, ctrl_n, m, digit) = ctx.input(|i| {
            let digit = (1..=9).find(|n| i.modifiers.command && i.key_pressed(digit_key(*n)));
            (
                i.key_pressed(Key::F11),
                i.key_pressed(Key::Escape),
                i.modifiers.command && i.key_pressed(Key::N),
                !i.modifiers.any() && i.key_pressed(Key::M),
                digit,
            )
        });
        if f11 {
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!fullscreen));
        }
        if esc {
            if self.canvas.focused.is_some() {
                self.canvas.focused = None;
            } else if fullscreen {
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
            }
        }
        if ctrl_n {
            self.open_add_camera();
        }
        if m {
            if let Some(id) = self.canvas.hovered {
                if let Some(t) = self.cfg.active_mut().tiles.iter_mut().find(|t| t.camera_id == id) {
                    t.muted = !t.muted;
                    self.mark_dirty();
                }
            }
        }
        if let Some(n) = digit {
            if let Some(l) = self.cfg.layouts.get(n - 1) {
                self.cfg.active_layout = l.name.clone();
                self.mark_dirty();
            }
        }
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let mut clicked = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() * 0.30);
                ui.label(RichText::new("vyu").size(42.0).strong().color(Color32::from_gray(230)));
                ui.add_space(6.0);
                ui.label(RichText::new("Suas câmeras, sempre à vista.").size(16.0).color(Color32::from_gray(150)));
                ui.add_space(22.0);
                let btn = egui::Button::new(RichText::new("+  Adicionar primeira câmera").size(18.0))
                    .corner_radius(20.0)
                    .min_size(egui::vec2(280.0, 46.0));
                clicked = ui.add(btn).clicked();
                ui.add_space(14.0);
                ui.label(
                    RichText::new("Antes, crie a conta da câmera no app Tapo → Configurações da câmera → Avançado → Conta da câmera")
                        .small()
                        .color(Color32::from_gray(120)),
                );
            });
        });
        if clicked {
            self.open_add_camera();
        }
    }

    fn show_notice(&mut self, ctx: &egui::Context) {
        let Some((text, since)) = &self.notice else { return };
        if since.elapsed() > NOTICE_TIME {
            self.notice = None;
            return;
        }
        egui::Area::new(egui::Id::unique("vyu-notice"))
            .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 64.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(60, 40, 20))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(ui, |ui| ui.label(RichText::new(text.as_str()).color(Color32::from_rgb(253, 230, 138))));
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    /// Registers/unregisters vyu in the Windows startup list.
    fn apply_autostart(&mut self) {}
}

fn digit_key(n: usize) -> Key {
    [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9][n - 1]
}

/// Three synthetic cameras laid out side by side, for `VYU_DEMO=1`.
fn add_demo_cameras(cfg: &mut Config) {
    let spots = [(0.02, 0.04, 0.55, 0.6), (0.6, 0.04, 0.38, 0.4), (0.6, 0.48, 0.38, 0.4)];
    for (i, (name, (x, y, w, h))) in ["Sala", "Garagem", "Quintal"].into_iter().zip(spots).enumerate() {
        let cam = Camera::new(name, "demo", "demo");
        cfg.active_mut().tiles.push(Tile { camera_id: cam.id, x, y, w, h, z: i as u32, muted: true, volume: 0.8 });
        cfg.cameras.push(cam);
    }
}

/// Adds a tile for `camera_id` centered on the canvas, above the others.
fn place_new_tile(cfg: &mut Config, camera_id: uuid::Uuid, canvas_size: (f32, f32)) {
    let bounds = Rect { x: 0.0, y: 0.0, w: canvas_size.0, h: canvas_size.1 };
    let layout = cfg.active_mut();
    if layout.tiles.iter().any(|t| t.camera_id == camera_id) {
        return;
    }
    let z = layout.tiles.iter().map(|t| t.z + 1).max().unwrap_or(0);
    let mut tile = Tile { camera_id, x: 0.0, y: 0.0, w: 0.0, h: 0.0, z, muted: true, volume: 0.8 };
    canvas::new_tile_rect(bounds).write_norm(&mut tile, canvas_size);
    layout.tiles.push(tile);
}

impl eframe::App for VyuApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.first_frame {
            self.first_frame = false;
            if self.cfg.settings.start_fullscreen {
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
            }
        }
        let size = ui.max_rect().size();
        self.canvas_size = (size.x, size.y);

        runtime::sync(&self.cfg, &mut self.rts, &self.mixer, &ctx, self.demo);
        self.handle_shortcuts(&ctx);

        if self.cfg.cameras.is_empty() {
            self.empty_state(ui);
        } else {
            let actions = tiles::show(ui, &mut self.cfg, &mut self.rts, &self.mixer, &mut self.canvas);
            if actions.changed {
                self.mark_dirty();
            }
            if let Some(id) = actions.edit {
                self.edit_camera(id);
            }
            if let Some(id) = actions.remove {
                self.remove_camera(id);
            }
        }

        // Floating bar: visible while the mouse moves, a dialog or a menu is open.
        let now = Instant::now();
        if ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_down()) {
            self.bar.poke(now);
        }
        let keep_open = self.dialog.is_open() || egui::Popup::is_any_open(&ctx);
        let visible = !self.cfg.cameras.is_empty() && self.bar.visible(now, keep_open);
        let opacity = ctx.animate_bool_with_time(egui::Id::unique("vyu-bar-fade"), visible, 0.25);
        if visible && !keep_open {
            ctx.request_repaint_after(self.bar.time_left(now));
        }
        let layout_name = self.cfg.active().name.clone();
        match bar::show(&ctx, opacity, &layout_name) {
            Some(bar::BarAction::AddCamera) => self.open_add_camera(),
            Some(bar::BarAction::Layouts) => self.dialog = Dialog::layouts(),
            Some(bar::BarAction::Settings) => self.dialog = Dialog::Settings,
            None => {}
        }

        let outcome = dialogs::show(&ctx, &mut self.dialog, &mut self.cfg);
        if outcome.changed {
            self.mark_dirty();
        }
        if let Some((id, is_new)) = outcome.saved_camera {
            runtime::restart(&mut self.rts, &self.mixer, id);
            if is_new {
                place_new_tile(&mut self.cfg, id, self.canvas_size);
            }
        }
        if let Some((id, show)) = outcome.toggle_tile {
            if show {
                place_new_tile(&mut self.cfg, id, self.canvas_size);
            } else {
                self.cfg.active_mut().tiles.retain(|t| t.camera_id != id);
            }
            self.mark_dirty();
        }
        if outcome.settings_changed {
            self.apply_autostart();
        }

        self.show_notice(&ctx);
        self.save_if_due(&ctx);
    }

    fn on_exit(&mut self) {
        if self.dirty_since.is_some() {
            self.save_now();
        }
    }
}
