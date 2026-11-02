use serde::{Deserialize, Serialize};
pub use uuid::Uuid;

pub const CONFIG_VERSION: u32 = 1;
pub const DEFAULT_LAYOUT: &str = "Padrão";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Quality {
    #[default]
    Auto,
    High,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Fit {
    /// Whole image visible, black bars if aspect differs.
    #[default]
    Contain,
    /// Fill the tile, cropping the edges.
    Cover,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub id: Uuid,
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub quality: Quality,
    #[serde(default)]
    pub fit: Fit,
}

fn default_port() -> u16 {
    554
}

impl Camera {
    pub fn new(name: &str, host: &str, username: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            host: host.into(),
            port: default_port(),
            username: username.into(),
            quality: Quality::default(),
            fit: Fit::default(),
        }
    }
}

/// A camera placed on the canvas. Position and size are normalized (0–1)
/// relative to the window, so layouts follow window resizes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub camera_id: Uuid,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub z: u32,
    #[serde(default = "default_muted")]
    pub muted: bool,
    #[serde(default = "default_volume")]
    pub volume: f32,
}

fn default_muted() -> bool {
    true
}

fn default_volume() -> f32 {
    0.8
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub name: String,
    #[serde(default)]
    pub tiles: Vec<Tile>,
}

impl Layout {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), tiles: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default)]
    pub start_fullscreen: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub cameras: Vec<Camera>,
    #[serde(default)]
    pub layouts: Vec<Layout>,
    #[serde(default)]
    pub active_layout: String,
    #[serde(default)]
    pub settings: Settings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            cameras: Vec::new(),
            layouts: vec![Layout::new(DEFAULT_LAYOUT)],
            active_layout: DEFAULT_LAYOUT.into(),
            settings: Settings::default(),
        }
    }
}

impl Config {
    fn active_index(&self) -> Option<usize> {
        self.layouts
            .iter()
            .position(|l| l.name == self.active_layout)
            .or(if self.layouts.is_empty() { None } else { Some(0) })
    }

    /// The active layout, falling back to the first one.
    pub fn active(&self) -> &Layout {
        static EMPTY: std::sync::OnceLock<Layout> = std::sync::OnceLock::new();
        match self.active_index() {
            Some(i) => &self.layouts[i],
            None => EMPTY.get_or_init(|| Layout::new(DEFAULT_LAYOUT)),
        }
    }

    /// Mutable active layout; creates the default layout if there is none.
    pub fn active_mut(&mut self) -> &mut Layout {
        let i = match self.active_index() {
            Some(i) => i,
            None => {
                self.layouts.push(Layout::new(DEFAULT_LAYOUT));
                0
            }
        };
        self.active_layout = self.layouts[i].name.clone();
        &mut self.layouts[i]
    }

    pub fn camera(&self, id: Uuid) -> Option<&Camera> {
        self.cameras.iter().find(|c| c.id == id)
    }

    /// Removes a camera and every tile that shows it, in all layouts.
    pub fn remove_camera(&mut self, id: Uuid) {
        self.cameras.retain(|c| c.id != id);
        for layout in &mut self.layouts {
            layout.tiles.retain(|t| t.camera_id != id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(camera_id: Uuid) -> Tile {
        Tile { camera_id, x: 0.1, y: 0.1, w: 0.4, h: 0.3, z: 0, muted: true, volume: 0.8 }
    }

    #[test]
    fn remove_camera_prunes_all_layouts() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut cfg = Config::default();
        cfg.cameras.push(Camera::new("A", "10.0.0.1", "u"));
        cfg.cameras[0].id = a;
        cfg.layouts[0].tiles = vec![tile(a), tile(b)];
        cfg.layouts.push(Layout { name: "Noite".into(), tiles: vec![tile(a)] });

        cfg.remove_camera(a);

        assert!(cfg.cameras.iter().all(|c| c.id != a));
        assert!(cfg.layouts.iter().flat_map(|l| &l.tiles).all(|t| t.camera_id != a));
        assert_eq!(cfg.layouts[0].tiles.len(), 1);
    }

    #[test]
    fn tile_defaults() {
        let id = Uuid::new_v4();
        let json = format!(r#"{{"camera_id":"{id}","x":0,"y":0,"w":0.5,"h":0.5,"z":0}}"#);
        let t: Tile = serde_json::from_str(&json).unwrap();
        assert!(t.muted);
        assert_eq!(t.volume, 0.8);
    }

    #[test]
    fn active_falls_back_to_first_layout() {
        let mut cfg = Config::default();
        cfg.active_layout = "inexistente".into();
        assert_eq!(cfg.active().name, "Padrão");
        cfg.layouts.clear();
        assert_eq!(cfg.active_mut().name, "Padrão");
        assert_eq!(cfg.layouts.len(), 1);
    }
}
