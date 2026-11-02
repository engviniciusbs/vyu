//! Floating bar at the top center: + Câmera · Layouts · Configurações.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Frame, RichText, Vec2};

/// The bar hides after this long without mouse movement.
pub const IDLE_HIDE: Duration = Duration::from_secs(2);

pub struct BarVisibility {
    last_activity: Instant,
}

impl BarVisibility {
    pub fn new(now: Instant) -> Self {
        Self { last_activity: now }
    }

    pub fn poke(&mut self, now: Instant) {
        self.last_activity = now;
    }

    /// `keep_open`: a dialog or menu is open.
    pub fn visible(&self, now: Instant, keep_open: bool) -> bool {
        keep_open || now.duration_since(self.last_activity) < IDLE_HIDE
    }

    pub fn time_left(&self, now: Instant) -> Duration {
        IDLE_HIDE.saturating_sub(now.duration_since(self.last_activity))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarAction {
    AddCamera,
    Layouts,
    Settings,
}

/// Draws the bar with the given opacity (0 = hidden). Returns the clicked action.
pub fn show(ctx: &egui::Context, opacity: f32, active_layout: &str) -> Option<BarAction> {
    if opacity <= 0.01 {
        return None;
    }
    let mut action = None;
    egui::Area::new(egui::Id::unique("vyu-bar"))
        .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 14.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_opacity(opacity);
            Frame::new()
                .fill(Color32::from_rgba_unmultiplied(28, 30, 36, 215))
                .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(25)))
                .corner_radius(22.0)
                .inner_margin(egui::Margin::symmetric(14, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 14.0;
                        let btn = |ui: &mut egui::Ui, text: &str, tip: &str| {
                            ui.add(egui::Button::new(RichText::new(text).size(15.0)).frame(false)).on_hover_text(tip).clicked()
                        };
                        if btn(ui, "+  Câmera", "Adicionar câmera (Ctrl+N)") {
                            action = Some(BarAction::AddCamera);
                        }
                        if btn(ui, &format!("▦  {active_layout}"), "Layouts (Ctrl+1…9 para trocar)") {
                            action = Some(BarAction::Layouts);
                        }
                        if btn(ui, "⚙", "Configurações") {
                            action = Some(BarAction::Settings);
                        }
                    });
                });
        });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_after_two_idle_seconds() {
        let t0 = Instant::now();
        let mut v = BarVisibility::new(t0);
        assert!(v.visible(t0 + Duration::from_millis(1900), false));
        assert!(!v.visible(t0 + Duration::from_millis(2100), false));
        v.poke(t0 + Duration::from_secs(3));
        assert!(v.visible(t0 + Duration::from_secs(4), false));
    }

    #[test]
    fn stays_while_something_is_open() {
        let t0 = Instant::now();
        let v = BarVisibility::new(t0);
        assert!(v.visible(t0 + Duration::from_secs(60), true));
    }
}
