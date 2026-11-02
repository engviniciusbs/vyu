//! The egui application: owns the config, the camera runtimes and the audio.

mod runtime;
mod style;
mod tiles;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::audio::{AudioOutput, Mixer};
use crate::canvas::{self, Rect};
use crate::config::{Camera, Config, Tile, store};
use crate::stream::ffmpeg;

use runtime::Runtimes;
use tiles::CanvasState;

/// Wait this long after the last change before writing config.json.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

pub struct VyuApp {
    cfg: Config,
    cfg_path: PathBuf,
    demo: bool,
    rts: Runtimes,
    mixer: Arc<Mutex<Mixer>>,
    _audio: Option<AudioOutput>,
    canvas: CanvasState,
    dirty_since: Option<Instant>,
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

        let mixer = Arc::new(Mutex::new(Mixer::new()));
        let audio = match AudioOutput::start(mixer.clone()) {
            Ok(a) => Some(a),
            Err(e) => {
                tracing::warn!("sem áudio: {e}");
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
            dirty_since: None,
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
pub(crate) fn place_new_tile(cfg: &mut Config, camera_id: uuid::Uuid, canvas_size: (f32, f32)) {
    let bounds = Rect { x: 0.0, y: 0.0, w: canvas_size.0, h: canvas_size.1 };
    let layout = cfg.active_mut();
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
        runtime::sync(&self.cfg, &mut self.rts, &self.mixer, &ctx, self.demo);

        let actions = tiles::show(ui, &mut self.cfg, &mut self.rts, &self.mixer, &mut self.canvas);
        if actions.changed {
            self.mark_dirty();
        }
        if let Some(id) = actions.remove {
            runtime::restart(&mut self.rts, &self.mixer, id);
            self.cfg.remove_camera(id);
            let _ = crate::secrets::delete_password(id);
            self.mark_dirty();
        }
        self.save_if_due(&ctx);
    }

    fn on_exit(&mut self) {
        if self.dirty_since.is_some() {
            self.save_now();
        }
    }
}
