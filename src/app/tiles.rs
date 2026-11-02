//! Draws camera tiles on the free-form canvas and handles drag/resize/focus.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui::{self, Align, Align2, Color32, CursorIcon, FontId, Layout, Pos2, Rect as ERect, Sense, Stroke, UiBuilder, Vec2};
use uuid::Uuid;

use super::runtime::{CameraRuntime, Runtimes};
use crate::audio::Mixer;
use crate::canvas::{self, Guide, Handle, Rect};
use crate::config::{Config, Fit, Quality};
use crate::stream::StreamVariant;
use crate::stream::session::{Command, SessionStatus};

const SNAP_PX: f32 = 8.0;
const BAR_H: f32 = 30.0;
const GUIDE_COLOR: Color32 = Color32::from_rgb(245, 158, 11);

struct Drag {
    idx: usize,
    handle: Handle,
    start: Rect,
    origin: Pos2,
}

#[derive(Default)]
pub struct CanvasState {
    drag: Option<Drag>,
    guides: Vec<Guide>,
    /// Camera shown over the whole window (double-click / ⛶).
    pub focused: Option<Uuid>,
    /// Tile whose hover controls are visible.
    controls_for: Option<Uuid>,
    /// Camera under the mouse pointer (for the `M` shortcut).
    pub hovered: Option<Uuid>,
}

#[derive(Default)]
pub struct TileActions {
    pub changed: bool,
    pub edit: Option<Uuid>,
    pub remove: Option<Uuid>,
}

/// Image rect inside `tile` and the texture UV for the given fit mode.
pub fn fit_image(tile: ERect, img_w: u32, img_h: u32, fit: Fit) -> (ERect, ERect) {
    let full_uv = ERect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    if img_w == 0 || img_h == 0 || tile.width() <= 0.0 || tile.height() <= 0.0 {
        return (tile, full_uv);
    }
    let img_aspect = img_w as f32 / img_h as f32;
    let tile_aspect = tile.width() / tile.height();
    match fit {
        Fit::Contain => {
            let size = if img_aspect > tile_aspect {
                Vec2::new(tile.width(), tile.width() / img_aspect)
            } else {
                Vec2::new(tile.height() * img_aspect, tile.height())
            };
            (ERect::from_center_size(tile.center(), size), full_uv)
        }
        Fit::Cover => {
            let uv = if img_aspect > tile_aspect {
                let w = tile_aspect / img_aspect;
                ERect::from_min_max(Pos2::new(0.5 - w / 2.0, 0.0), Pos2::new(0.5 + w / 2.0, 1.0))
            } else {
                let h = img_aspect / tile_aspect;
                ERect::from_min_max(Pos2::new(0.0, 0.5 - h / 2.0), Pos2::new(1.0, 0.5 + h / 2.0))
            };
            (tile, uv)
        }
    }
}

/// Overlay text for a session state (`None` while live).
pub fn status_text(s: &SessionStatus) -> Option<String> {
    match s {
        SessionStatus::Connecting => Some("Conectando…".into()),
        SessionStatus::Live { .. } | SessionStatus::Stopped => None,
        SessionStatus::AuthFailed => Some("Usuário ou senha incorretos".into()),
        SessionStatus::Reconnecting { attempt, retry_in, reason } => {
            Some(format!("{reason}\nReconectando (tentativa {attempt}, a cada {} s)…", retry_in.as_secs()))
        }
    }
}

fn to_screen(r: Rect, origin: Pos2) -> ERect {
    ERect::from_min_size(Pos2::new(origin.x + r.x, origin.y + r.y), Vec2::new(r.w, r.h))
}

fn cursor_for(h: Handle) -> CursorIcon {
    match h {
        Handle::Move => CursorIcon::Grab,
        Handle::N | Handle::S => CursorIcon::ResizeVertical,
        Handle::E | Handle::W => CursorIcon::ResizeHorizontal,
        Handle::NW | Handle::SE => CursorIcon::ResizeNwSe,
        Handle::NE | Handle::SW => CursorIcon::ResizeNeSw,
    }
}

/// Uploads the latest decoded frame to the GPU when it changed.
fn upload_frame(ctx: &egui::Context, id: Uuid, rt: &mut CameraRuntime) {
    let Some(handle) = &rt.handle else { return };
    let guard = handle.frame.lock().unwrap();
    let Some(frame) = guard.as_ref() else { return };
    if frame.seq == rt.last_seq {
        return;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([frame.width as usize, frame.height as usize], &frame.rgba);
    rt.last_seq = frame.seq;
    drop(guard);
    let opts = egui::TextureOptions::LINEAR;
    match &mut rt.texture {
        Some(t) => t.set(image, opts),
        None => rt.texture = Some(ctx.load_texture(format!("cam-{id}"), image, opts)),
    }
}

/// Picks stream1/stream2 for a tile of `height_px` physical pixels.
fn update_quality(rt: &mut CameraRuntime, quality: Quality, height_px: f32) {
    let Some(handle) = &rt.handle else { return };
    let desired = match quality {
        Quality::High => StreamVariant::High,
        Quality::Low => StreamVariant::Low,
        Quality::Auto => rt.auto.update(height_px, Instant::now()),
    };
    if desired != handle.variant() {
        handle.send(Command::SetVariant(desired));
    }
}

fn paint_tile(painter: &egui::Painter, rect: ERect, name: &str, fit: Fit, rt: Option<&CameraRuntime>, hovered: bool) {
    painter.rect_filled(rect, 4.0, Color32::from_gray(12));
    let mut overlay: Option<String> = None;
    let mut dim = false;
    if let Some(rt) = rt {
        if let Some(tex) = &rt.texture {
            let [w, h] = tex.size();
            let (draw, uv) = fit_image(rect, w as u32, h as u32, fit);
            painter.with_clip_rect(rect).image(tex.id(), draw, uv, Color32::WHITE);
        }
        if let Some(p) = &rt.problem {
            overlay = Some(p.clone());
            dim = true;
        } else if let Some(h) = &rt.handle {
            let status = h.status();
            dim = !matches!(status, SessionStatus::Live { .. });
            overlay = status_text(&status);
        }
    }
    if dim {
        painter.rect_filled(rect, 4.0, Color32::from_black_alpha(150));
    }
    if let Some(text) = overlay {
        painter.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(14.0), Color32::from_gray(220));
    }
    // Camera name with a soft shadow, top-left.
    let pos = rect.min + Vec2::new(10.0, 8.0);
    let font = FontId::proportional(13.0);
    painter.text(pos + Vec2::splat(1.0), Align2::LEFT_TOP, name.to_uppercase(), font.clone(), Color32::from_black_alpha(200));
    painter.text(pos, Align2::LEFT_TOP, name.to_uppercase(), font, Color32::WHITE);
    if hovered {
        painter.rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_white_alpha(60)), egui::StrokeKind::Inside);
    }
}

pub fn show(ui: &mut egui::Ui, cfg: &mut Config, rts: &mut Runtimes, mixer: &Arc<Mutex<Mixer>>, state: &mut CanvasState) -> TileActions {
    let mut actions = TileActions::default();
    let ctx = ui.ctx().clone();
    let canvas = ui.max_rect();
    let origin = canvas.min;
    let size = (canvas.width(), canvas.height());
    let bounds = Rect { x: 0.0, y: 0.0, w: size.0, h: size.1 };
    let ppp = ctx.pixels_per_point();

    for (id, rt) in rts.iter_mut() {
        upload_frame(&ctx, *id, rt);
    }

    let cameras = cfg.cameras.clone();
    let layout = cfg.active_mut();
    if state.focused.is_some_and(|f| !layout.tiles.iter().any(|t| t.camera_id == f)) {
        state.focused = None;
    }

    // Keep tiles inside the window (it may have shrunk or moved monitors).
    for t in layout.tiles.iter_mut() {
        let r = Rect::from_norm(t, size);
        let c = canvas::clamp_into(r, bounds);
        if c != r {
            c.write_norm(t, size);
        }
    }

    // Audio gains follow the tiles every frame.
    {
        let mut m = mixer.lock().unwrap();
        for t in &layout.tiles {
            m.set_gain(t.camera_id, t.volume, t.muted);
        }
    }

    let resp = ui.interact(canvas, ui.scope_id().with("vyu-canvas"), Sense::click_and_drag());
    let shift = ctx.input(|i| i.modifiers.shift);
    let alt = ctx.input(|i| i.modifiers.alt);
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    let rects: Vec<(Rect, u32)> = layout.tiles.iter().map(|t| (Rect::from_norm(t, size), t.z)).collect();
    let local = |p: Pos2| (p.x - origin.x, p.y - origin.y);

    if state.focused.is_none() {
        if resp.drag_started() {
            if let Some(o) = ctx.input(|i| i.pointer.press_origin()) {
                if let Some(idx) = canvas::topmost_at(&rects, local(o)) {
                    let handle = canvas::hit_handle(rects[idx].0, local(o), canvas::EDGE).unwrap_or(Handle::Move);
                    canvas::bring_to_front(&mut layout.tiles, idx);
                    state.drag = Some(Drag { idx, handle, start: rects[idx].0, origin: o });
                    actions.changed = true;
                }
            }
        }
        if let (Some(d), Some(p)) = (&state.drag, pointer) {
            if resp.dragged() {
                let delta = (p.x - d.origin.x, p.y - d.origin.y);
                let mut r = canvas::apply_drag(d.start, d.handle, delta, shift);
                state.guides.clear();
                if !alt {
                    let others: Vec<Rect> = rects.iter().enumerate().filter(|(i, _)| *i != d.idx).map(|(_, (r, _))| *r).collect();
                    let (snapped, guides) = canvas::snap(r, d.handle, &others, bounds, SNAP_PX);
                    r = snapped;
                    state.guides = guides;
                }
                let r = canvas::clamp_into(r, bounds);
                r.write_norm(&mut layout.tiles[d.idx], size);
                ctx.set_cursor_icon(if d.handle == Handle::Move { CursorIcon::Grabbing } else { cursor_for(d.handle) });
                actions.changed = true;
            }
        }
        if resp.drag_stopped() {
            state.drag = None;
            state.guides.clear();
        }
        if resp.clicked() {
            if let Some(idx) = pointer.and_then(|p| canvas::topmost_at(&rects, local(p))) {
                canvas::bring_to_front(&mut layout.tiles, idx);
                actions.changed = true;
            }
        }
        if state.drag.is_none() && resp.hovered() {
            if let Some(p) = pointer {
                if let Some(idx) = canvas::topmost_at(&rects, local(p)) {
                    if let Some(h) = canvas::hit_handle(rects[idx].0, local(p), canvas::EDGE) {
                        ctx.set_cursor_icon(cursor_for(h));
                    }
                }
            }
        }
    }
    if resp.double_clicked() {
        state.focused = match state.focused {
            Some(_) => None,
            None => pointer.and_then(|p| canvas::topmost_at(&rects, local(p))).map(|i| layout.tiles[i].camera_id),
        };
        state.drag = None;
    }

    // Paint in z order (or only the focused tile).
    let mut order: Vec<usize> = (0..layout.tiles.len()).collect();
    order.sort_by_key(|&i| layout.tiles[i].z);
    let hovered_idx = if state.focused.is_some() {
        layout.tiles.iter().position(|t| Some(t.camera_id) == state.focused)
    } else {
        pointer.filter(|p| canvas.contains(*p)).and_then(|p| canvas::topmost_at(&rects, local(p)))
    };
    state.hovered = hovered_idx.map(|i| layout.tiles[i].camera_id);
    let painter = ui.painter_at(canvas);
    let mut screen_rects = vec![ERect::NOTHING; layout.tiles.len()];
    for &i in &order {
        let t = &layout.tiles[i];
        if state.focused.is_some_and(|f| f != t.camera_id) {
            continue;
        }
        let rect = if state.focused.is_some() { canvas } else { to_screen(Rect::from_norm(t, size), origin) };
        screen_rects[i] = rect;
        let cam = cameras.iter().find(|c| c.id == t.camera_id);
        let name = cam.map_or("?", |c| c.name.as_str());
        let fit = cam.map_or(Fit::Contain, |c| c.fit);
        paint_tile(&painter, rect, name, fit, rts.get(&t.camera_id), hovered_idx == Some(i));
        if let (Some(cam), Some(rt)) = (cam, rts.get_mut(&t.camera_id)) {
            update_quality(rt, cam.quality, rect.height() * ppp);
        }
    }
    for g in &state.guides {
        let stroke = Stroke::new(1.0, GUIDE_COLOR);
        if g.vertical {
            let x = origin.x + g.pos;
            painter.line_segment([Pos2::new(x, canvas.top()), Pos2::new(x, canvas.bottom())], stroke);
        } else {
            let y = origin.y + g.pos;
            painter.line_segment([Pos2::new(canvas.left(), y), Pos2::new(canvas.right(), y)], stroke);
        }
    }

    // Hover controls (stay while a menu from them is open).
    if !egui::Popup::is_any_open(&ctx) {
        state.controls_for = if state.drag.is_some() { None } else { hovered_idx.map(|i| layout.tiles[i].camera_id) };
    }
    let Some(cid) = state.controls_for else { return actions };
    let Some(i) = layout.tiles.iter().position(|t| t.camera_id == cid) else { return actions };
    let rect = screen_rects[i];
    if rect == ERect::NOTHING {
        return actions;
    }

    // "Edit camera" button on auth / missing-password problems.
    let needs_edit = rts.get(&cid).is_some_and(|rt| {
        rt.problem.is_some() || rt.handle.as_ref().is_some_and(|h| h.status() == SessionStatus::AuthFailed)
    });
    if needs_edit {
        let btn = ERect::from_center_size(rect.center() + Vec2::new(0.0, 34.0), Vec2::new(130.0, 28.0));
        if ui.put(btn, egui::Button::new("Editar câmera")).clicked() {
            actions.edit = Some(cid);
        }
    }

    let has_audio = rts
        .get(&cid)
        .and_then(|rt| rt.handle.as_ref())
        .is_some_and(|h| matches!(h.status(), SessionStatus::Live { has_audio: true }));
    let bar = ERect::from_min_max(Pos2::new(rect.left() + 6.0, rect.bottom() - BAR_H - 6.0), Pos2::new(rect.right() - 6.0, rect.bottom() - 6.0));
    painter.rect_filled(bar, 8.0, Color32::from_black_alpha(170));
    let quality = cameras.iter().find(|c| c.id == cid).map(|c| (c.quality, c.fit));
    let mut new_quality = None;
    let mut new_fit = None;
    let tile = &mut layout.tiles[i];
    ui.scope_builder(UiBuilder::new().max_rect(bar.shrink2(Vec2::new(8.0, 2.0))).layout(Layout::left_to_right(Align::Center)), |ui| {
        let icon = if tile.muted || !has_audio { "🔇" } else { "🔊" };
        let mute = ui.add_enabled(has_audio, egui::Button::new(egui::RichText::new(icon).size(17.0)).frame(false));
        if mute.clicked() {
            tile.muted = !tile.muted;
            actions.changed = true;
        }
        mute.on_disabled_hover_text("Esta câmera não envia áudio");
        if has_audio {
            ui.spacing_mut().slider_width = 70.0;
            if ui.add(egui::Slider::new(&mut tile.volume, 0.0..=1.0).show_value(false)).changed() {
                tile.muted = false;
                actions.changed = true;
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button(egui::RichText::new("⋯").size(17.0), |ui| {
                if ui.button("Editar câmera").clicked() {
                    actions.edit = Some(cid);
                }
                if ui.button("Remover câmera").clicked() {
                    actions.remove = Some(cid);
                }
                ui.separator();
                if let Some((q, f)) = quality {
                    ui.label("Qualidade");
                    for (v, label) in [(Quality::Auto, "Automática"), (Quality::High, "Alta"), (Quality::Low, "Baixa")] {
                        if ui.radio(q == v, label).clicked() {
                            new_quality = Some(v);
                        }
                    }
                    ui.separator();
                    ui.label("Ajuste do vídeo");
                    for (v, label) in [(Fit::Contain, "Caber (faixas pretas)"), (Fit::Cover, "Preencher (corta as bordas)")] {
                        if ui.radio(f == v, label).clicked() {
                            new_fit = Some(v);
                        }
                    }
                }
            });
            let full = if state.focused.is_some() { "Sair da tela cheia" } else { "Tela cheia" };
            if ui.add(egui::Button::new(egui::RichText::new("⛶").size(17.0)).frame(false)).on_hover_text(full).clicked() {
                state.focused = if state.focused.is_some() { None } else { Some(cid) };
            }
        });
    });
    if let Some(cam) = cfg.cameras.iter_mut().find(|c| c.id == cid) {
        if let Some(q) = new_quality {
            cam.quality = q;
            actions.changed = true;
        }
        if let Some(f) = new_fit {
            cam.fit = f;
            actions.changed = true;
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn er(x: f32, y: f32, w: f32, h: f32) -> ERect {
        ERect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    #[test]
    fn contain_letterboxes_wide_tile() {
        // 16:9 image in a 400x400 tile → 400x225 centered vertically.
        let (draw, uv) = fit_image(er(0.0, 0.0, 400.0, 400.0), 1600, 900, Fit::Contain);
        assert_eq!(draw, er(0.0, 87.5, 400.0, 225.0));
        assert_eq!(uv, er(0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn cover_crops_sides() {
        // 16:9 image in a square tile → full tile, uv cropped horizontally.
        let (draw, uv) = fit_image(er(0.0, 0.0, 400.0, 400.0), 1600, 900, Fit::Cover);
        assert_eq!(draw, er(0.0, 0.0, 400.0, 400.0));
        let visible = 900.0 / 1600.0;
        assert!((uv.width() - visible).abs() < 1e-4);
        assert!((uv.center().x - 0.5).abs() < 1e-4);
        assert_eq!(uv.height(), 1.0);
    }

    #[test]
    fn status_messages() {
        assert_eq!(status_text(&SessionStatus::Connecting).as_deref(), Some("Conectando…"));
        assert_eq!(status_text(&SessionStatus::Live { has_audio: false }), None);
        assert_eq!(status_text(&SessionStatus::AuthFailed).as_deref(), Some("Usuário ou senha incorretos"));
        let r = SessionStatus::Reconnecting {
            attempt: 2,
            retry_in: std::time::Duration::from_secs(4),
            reason: "Câmera não encontrada em 10.0.0.9".into(),
        };
        assert_eq!(
            status_text(&r).as_deref(),
            Some("Câmera não encontrada em 10.0.0.9\nReconectando (tentativa 2, a cada 4 s)…")
        );
    }
}
