use crate::config::Tile;

/// Smallest tile size, in logical pixels.
pub const MIN_W: f32 = 120.0;
pub const MIN_H: f32 = 68.0;
/// Width of the grab band along each tile edge.
pub const EDGE: f32 = 6.0;
const ASPECT: f32 = 16.0 / 9.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn contains(&self, (px, py): (f32, f32)) -> bool {
        px >= self.x && px <= self.right() && py >= self.y && py <= self.bottom()
    }

    pub fn from_norm(t: &Tile, (cw, ch): (f32, f32)) -> Rect {
        Rect { x: t.x * cw, y: t.y * ch, w: t.w * cw, h: t.h * ch }
    }

    pub fn write_norm(&self, t: &mut Tile, (cw, ch): (f32, f32)) {
        if cw <= 0.0 || ch <= 0.0 {
            return;
        }
        t.x = self.x / cw;
        t.y = self.y / ch;
        t.w = self.w / cw;
        t.h = self.h / ch;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    Move,
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

impl Handle {
    pub fn moves_left(self) -> bool {
        matches!(self, Handle::W | Handle::NW | Handle::SW | Handle::Move)
    }
    pub fn moves_right(self) -> bool {
        matches!(self, Handle::E | Handle::NE | Handle::SE | Handle::Move)
    }
    pub fn moves_top(self) -> bool {
        matches!(self, Handle::N | Handle::NE | Handle::NW | Handle::Move)
    }
    pub fn moves_bottom(self) -> bool {
        matches!(self, Handle::S | Handle::SE | Handle::SW | Handle::Move)
    }
}

pub fn hit_handle(r: Rect, p: (f32, f32), edge: f32) -> Option<Handle> {
    if !r.contains(p) {
        return None;
    }
    let left = p.0 - r.x <= edge;
    let right = r.right() - p.0 <= edge;
    let top = p.1 - r.y <= edge;
    let bottom = r.bottom() - p.1 <= edge;
    Some(match (left, right, top, bottom) {
        (true, _, true, _) => Handle::NW,
        (_, true, true, _) => Handle::NE,
        (true, _, _, true) => Handle::SW,
        (_, true, _, true) => Handle::SE,
        (true, ..) => Handle::W,
        (_, true, ..) => Handle::E,
        (_, _, true, _) => Handle::N,
        (.., true) => Handle::S,
        _ => Handle::Move,
    })
}

/// Applies a pointer drag (total delta since the drag started) to `start`.
/// The edges opposite to the handle stay fixed; size never goes below the minimum.
pub fn apply_drag(start: Rect, h: Handle, (dx, dy): (f32, f32), lock_16_9: bool) -> Rect {
    if h == Handle::Move {
        return Rect { x: start.x + dx, y: start.y + dy, ..start };
    }
    let mut left = start.x;
    let mut right = start.right();
    let mut top = start.y;
    let mut bottom = start.bottom();
    if h.moves_left() {
        left = (left + dx).min(right - MIN_W);
    }
    if h.moves_right() {
        right = (right + dx).max(left + MIN_W);
    }
    if h.moves_top() {
        top = (top + dy).min(bottom - MIN_H);
    }
    if h.moves_bottom() {
        bottom = (bottom + dy).max(top + MIN_H);
    }
    let mut w = right - left;
    let mut hgt = bottom - top;
    if lock_16_9 {
        // Width drives height, except for pure vertical edges.
        if matches!(h, Handle::N | Handle::S) {
            w = (hgt * ASPECT).max(MIN_W);
            hgt = w / ASPECT;
        } else {
            hgt = (w / ASPECT).max(MIN_H);
            w = hgt * ASPECT;
        }
        if h.moves_left() {
            left = right - w;
        }
        if h.moves_top() {
            top = bottom - hgt;
        }
    }
    Rect { x: left, y: top, w, h: hgt }
}

/// Keeps a tile fully inside the canvas, shrinking it if it is larger.
pub fn clamp_into(r: Rect, b: Rect) -> Rect {
    let w = r.w.min(b.w);
    let h = r.h.min(b.h);
    let x = r.x.clamp(b.x, b.right() - w);
    let y = r.y.clamp(b.y, b.bottom() - h);
    Rect { x, y, w, h }
}

/// Where a newly added camera appears: centered, 40% of the width, 16:9.
pub fn new_tile_rect(b: Rect) -> Rect {
    let mut w = b.w * 0.4;
    let mut h = w / ASPECT;
    if h > b.h {
        h = b.h * 0.9;
        w = h * ASPECT;
    }
    Rect { x: b.x + (b.w - w) / 2.0, y: b.y + (b.h - h) / 2.0, w, h }
}

/// Puts `tiles[idx]` on top and renumbers z to 0..n (stable order otherwise).
pub fn bring_to_front(tiles: &mut [Tile], idx: usize) {
    let max = tiles.iter().map(|t| t.z).max().unwrap_or(0);
    tiles[idx].z = max + 1;
    let mut order: Vec<usize> = (0..tiles.len()).collect();
    order.sort_by_key(|&i| tiles[i].z);
    for (z, i) in order.into_iter().enumerate() {
        tiles[i].z = z as u32;
    }
}

/// Index of the highest-z rect containing `p`.
pub fn topmost_at(items: &[(Rect, u32)], p: (f32, f32)) -> Option<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, (r, _))| r.contains(p))
        .max_by_key(|(_, (_, z))| *z)
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn hit_corner_beats_edge() {
        let rect = r(100.0, 100.0, 200.0, 100.0);
        assert_eq!(hit_handle(rect, (299.0, 199.0), EDGE), Some(Handle::SE));
        assert_eq!(hit_handle(rect, (101.0, 101.0), EDGE), Some(Handle::NW));
        assert_eq!(hit_handle(rect, (200.0, 101.0), EDGE), Some(Handle::N));
        assert_eq!(hit_handle(rect, (298.0, 150.0), EDGE), Some(Handle::E));
    }

    #[test]
    fn hit_inside_is_move() {
        assert_eq!(hit_handle(r(100.0, 100.0, 200.0, 100.0), (200.0, 150.0), EDGE), Some(Handle::Move));
    }

    #[test]
    fn hit_outside_is_none() {
        assert_eq!(hit_handle(r(100.0, 100.0, 200.0, 100.0), (50.0, 50.0), EDGE), None);
    }

    #[test]
    fn drag_move_translates() {
        let out = apply_drag(r(10.0, 10.0, 200.0, 100.0), Handle::Move, (5.0, -3.0), false);
        assert_eq!(out, r(15.0, 7.0, 200.0, 100.0));
    }

    #[test]
    fn drag_se_grows() {
        let out = apply_drag(r(10.0, 10.0, 200.0, 100.0), Handle::SE, (50.0, 20.0), false);
        assert_eq!(out, r(10.0, 10.0, 250.0, 120.0));
    }

    #[test]
    fn drag_w_keeps_right_edge() {
        let start = r(100.0, 10.0, 200.0, 100.0);
        let out = apply_drag(start, Handle::W, (30.0, 0.0), false);
        assert_eq!(out.right(), start.right());
        assert_eq!(out.w, 170.0);
    }

    #[test]
    fn drag_respects_min_size() {
        let out = apply_drag(r(10.0, 10.0, 200.0, 100.0), Handle::SE, (-1000.0, -1000.0), false);
        assert_eq!((out.w, out.h), (MIN_W, MIN_H));
        let start = r(300.0, 300.0, 200.0, 100.0);
        let out = apply_drag(start, Handle::NW, (1000.0, 1000.0), false);
        assert_eq!((out.w, out.h), (MIN_W, MIN_H));
        assert_eq!((out.right(), out.bottom()), (start.right(), start.bottom()));
    }

    #[test]
    fn lock_16_9_on_corner() {
        let out = apply_drag(r(0.0, 0.0, 320.0, 180.0), Handle::SE, (160.0, 10.0), true);
        assert!(approx(out.w / out.h, 16.0 / 9.0), "{out:?}");
        assert!(approx(out.w, 480.0));
    }

    #[test]
    fn clamp_keeps_tile_inside() {
        let bounds = r(0.0, 0.0, 800.0, 600.0);
        let out = clamp_into(r(1200.0, -50.0, 200.0, 100.0), bounds);
        assert_eq!(out, r(600.0, 0.0, 200.0, 100.0));
        let out = clamp_into(r(10.0, 10.0, 1000.0, 900.0), bounds);
        assert_eq!(out, bounds);
    }

    #[test]
    fn norm_roundtrip() {
        let size = (1280.0, 720.0);
        let mut t = Tile { camera_id: Uuid::nil(), x: 0.1, y: 0.2, w: 0.3, h: 0.4, z: 0, muted: true, volume: 0.8 };
        let rect = Rect::from_norm(&t, size);
        assert!(approx(rect.x, 128.0) && approx(rect.h, 288.0));
        rect.write_norm(&mut t, size);
        assert!(approx(t.x, 0.1) && approx(t.y, 0.2) && approx(t.w, 0.3) && approx(t.h, 0.4));
    }

    #[test]
    fn new_tile_is_centered_16_9() {
        let out = new_tile_rect(r(0.0, 0.0, 1000.0, 800.0));
        assert!(approx(out.w, 400.0) && approx(out.h, 225.0));
        assert!(approx(out.x + out.w / 2.0, 500.0) && approx(out.y + out.h / 2.0, 400.0));
    }

    #[test]
    fn bring_to_front_reorders() {
        let mk = |z| Tile { camera_id: Uuid::new_v4(), x: 0.0, y: 0.0, w: 0.1, h: 0.1, z, muted: true, volume: 0.8 };
        let mut tiles = vec![mk(0), mk(5), mk(2)];
        bring_to_front(&mut tiles, 0);
        let zs: Vec<u32> = tiles.iter().map(|t| t.z).collect();
        assert_eq!(zs, vec![2, 1, 0]);
    }

    #[test]
    fn topmost_picks_highest_z() {
        let items = [(r(0.0, 0.0, 100.0, 100.0), 3), (r(50.0, 50.0, 100.0, 100.0), 7), (r(500.0, 0.0, 10.0, 10.0), 9)];
        assert_eq!(topmost_at(&items, (60.0, 60.0)), Some(1));
        assert_eq!(topmost_at(&items, (10.0, 10.0)), Some(0));
        assert_eq!(topmost_at(&items, (300.0, 300.0)), None);
    }
}
