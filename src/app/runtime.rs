//! Live state per camera: its session, its texture and its quality picker.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use eframe::egui;
use uuid::Uuid;

use crate::audio::Mixer;
use crate::config::{Camera, Config, Quality};
use crate::secrets;
use crate::stream::StreamVariant;
use crate::stream::quality::AutoQuality;
use crate::stream::demo::DemoFactory;
use crate::stream::ffmpeg::FfmpegFactory;
use crate::stream::session::{Session, SessionHandle};

pub struct CameraRuntime {
    /// `None` when the session could not start (see `problem`).
    pub handle: Option<SessionHandle>,
    pub problem: Option<String>,
    pub texture: Option<egui::TextureHandle>,
    pub last_seq: u64,
    pub auto: AutoQuality,
}

pub type Runtimes = HashMap<Uuid, CameraRuntime>;

/// Cameras that should be streaming: the ones shown in the active layout.
pub fn wanted(cfg: &Config) -> HashSet<Uuid> {
    cfg.active()
        .tiles
        .iter()
        .map(|t| t.camera_id)
        .filter(|id| cfg.camera(*id).is_some())
        .collect()
}

pub fn initial_variant(q: Quality) -> StreamVariant {
    match q {
        Quality::High => StreamVariant::High,
        Quality::Auto | Quality::Low => StreamVariant::Low,
    }
}

/// Starts sessions for newly visible cameras and stops the ones no longer shown.
pub fn sync(cfg: &Config, rts: &mut Runtimes, mixer: &Arc<Mutex<Mixer>>, ctx: &egui::Context, demo: bool) {
    let wanted = wanted(cfg);
    rts.retain(|id, _| {
        let keep = wanted.contains(id);
        if !keep {
            mixer.lock().unwrap().remove_source(*id);
        }
        keep
    });
    for id in wanted {
        if rts.contains_key(&id) {
            continue;
        }
        let cam = cfg.camera(id).expect("wanted() only returns existing cameras");
        rts.insert(id, start(cam, cfg, mixer, ctx, demo));
    }
}

/// Stops a camera so the next `sync` restarts it (e.g. after editing it).
pub fn restart(rts: &mut Runtimes, mixer: &Arc<Mutex<Mixer>>, id: Uuid) {
    rts.remove(&id);
    mixer.lock().unwrap().remove_source(id);
}

fn start(cam: &Camera, cfg: &Config, mixer: &Arc<Mutex<Mixer>>, ctx: &egui::Context, demo: bool) -> CameraRuntime {
    let mut rt = CameraRuntime { handle: None, problem: None, texture: None, last_seq: 0, auto: AutoQuality::new() };
    let variant = initial_variant(cam.quality);
    let ctx = ctx.clone();
    let repaint = Box::new(move || ctx.request_repaint());
    if demo {
        let seed = cfg.cameras.iter().position(|c| c.id == cam.id).unwrap_or(0) as u32 + 1;
        let sink = mixer.lock().unwrap().add_source(cam.id);
        rt.handle = Some(Session::spawn(DemoFactory { seed }, variant, sink, repaint));
        return rt;
    }
    match secrets::get_password(cam.id) {
        Ok(Some(pass)) => {
            let factory = FfmpegFactory { host: cam.host.clone(), port: cam.port, user: cam.username.clone(), pass };
            let sink = mixer.lock().unwrap().add_source(cam.id);
            rt.handle = Some(Session::spawn(factory, variant, sink, repaint));
        }
        Ok(None) => rt.problem = Some("Senha não encontrada".into()),
        Err(e) => rt.problem = Some(e.to_string()),
    }
    rt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Layout, Tile};

    fn tile(id: Uuid) -> Tile {
        Tile { camera_id: id, x: 0.0, y: 0.0, w: 0.2, h: 0.2, z: 0, muted: true, volume: 0.8 }
    }

    #[test]
    fn wanted_is_active_layout_cameras_only() {
        let mut cfg = Config::default();
        let a = Camera::new("A", "h", "u");
        let b = Camera::new("B", "h", "u");
        let ghost = Uuid::new_v4(); // tile pointing at a deleted camera
        cfg.layouts[0].tiles = vec![tile(a.id), tile(a.id), tile(ghost)];
        cfg.layouts.push(Layout { name: "Outro".into(), tiles: vec![tile(b.id)] });
        cfg.cameras = vec![a.clone(), b];
        assert_eq!(wanted(&cfg), HashSet::from([a.id]));
    }

    #[test]
    fn initial_variant_follows_quality() {
        assert_eq!(initial_variant(Quality::Auto), StreamVariant::Low);
        assert_eq!(initial_variant(Quality::Low), StreamVariant::Low);
        assert_eq!(initial_variant(Quality::High), StreamVariant::High);
    }
}
