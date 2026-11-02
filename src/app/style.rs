//! Look & feel: Windows fonts (Segoe UI + symbol fallbacks) and a dark theme.

use std::sync::Arc;

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily};

/// (name, file in C:\Windows\Fonts). Missing files are skipped.
const FONTS: [(&str, &str); 3] = [
    ("Segoe UI", "segoeui.ttf"),
    ("Segoe UI Symbol", "seguisym.ttf"),
    ("Segoe UI Emoji", "seguiemj.ttf"),
];

pub fn apply(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let dir = std::env::var("WINDIR").map(|w| std::path::PathBuf::from(w).join("Fonts")).unwrap_or_default();
    let mut loaded = Vec::new();
    for (name, file) in FONTS {
        if let Ok(bytes) = std::fs::read(dir.join(file)) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
            loaded.push(name);
        }
    }
    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    if loaded.contains(&"Segoe UI") {
        prop.insert(0, "Segoe UI".into());
    }
    for fallback in ["Segoe UI Symbol", "Segoe UI Emoji"] {
        if loaded.contains(&fallback) {
            prop.push(fallback.into());
        }
    }
    ctx.set_fonts(fonts);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::BLACK;
    visuals.window_fill = Color32::from_rgb(22, 24, 28);
    visuals.window_corner_radius = 12.into();
    visuals.menu_corner_radius = 10.into();
    ctx.set_visuals(visuals);
}
