//! Look & feel: system UI fonts (Segoe UI on Windows, Ubuntu/Noto/DejaVu on Linux)
//! with symbol fallbacks, and a dark theme.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily};

/// Main UI font: the first candidate that exists is used.
/// Symbol fallbacks: every one that exists is appended. Missing files are skipped.
struct FontPlan {
    main: Vec<PathBuf>,
    fallbacks: Vec<PathBuf>,
}

#[cfg(windows)]
fn font_plan() -> FontPlan {
    let dir = std::env::var("WINDIR").map(|w| PathBuf::from(w).join("Fonts")).unwrap_or_default();
    FontPlan { main: vec![dir.join("segoeui.ttf")], fallbacks: vec![dir.join("seguisym.ttf"), dir.join("seguiemj.ttf")] }
}

#[cfg(not(windows))]
fn font_plan() -> FontPlan {
    let p = |s: &str| PathBuf::from(s);
    FontPlan {
        main: vec![
            p("/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf"),
            p("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf"),
            p("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        ],
        fallbacks: vec![
            p("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
            p("/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf"),
            p("/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf"),
        ],
    }
}

pub fn apply(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let plan = font_plan();
    let mut load = |path: &PathBuf| -> Option<String> {
        let bytes = std::fs::read(path).ok()?;
        let name = path.file_stem()?.to_string_lossy().to_string();
        fonts.font_data.insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
        Some(name)
    };
    let main = plan.main.iter().find_map(&mut load);
    let fallbacks: Vec<String> = plan.fallbacks.iter().filter_map(&mut load).collect();
    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    if let Some(main) = main {
        prop.insert(0, main);
    }
    for fallback in fallbacks {
        if !prop.contains(&fallback) {
            prop.push(fallback);
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
