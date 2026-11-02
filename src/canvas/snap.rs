use super::geometry::{Handle, Rect};

/// An alignment line to draw while dragging (`vertical` → x = pos).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guide {
    pub vertical: bool,
    pub pos: f32,
}

/// Finds the closest target within `threshold` for any of the moving edges.
/// Returns (offset to apply, target position, which moving edge index).
fn best(moving: &[(usize, f32)], targets: &[f32], threshold: f32) -> Option<(f32, f32, usize)> {
    let mut found: Option<(f32, f32, usize)> = None;
    for &(edge, pos) in moving {
        for &t in targets {
            let d = t - pos;
            if d.abs() <= threshold && found.is_none_or(|(bd, _, _)| d.abs() < bd.abs()) {
                found = Some((d, t, edge));
            }
        }
    }
    found
}

/// Snaps the edges being dragged to other tiles' edges and the window edges.
pub fn snap(r: Rect, h: Handle, others: &[Rect], bounds: Rect, threshold: f32) -> (Rect, Vec<Guide>) {
    let mut out = r;
    let mut guides = Vec::new();

    let mut xs: Vec<f32> = vec![bounds.x, bounds.right()];
    let mut ys: Vec<f32> = vec![bounds.y, bounds.bottom()];
    for o in others {
        xs.extend([o.x, o.right()]);
        ys.extend([o.y, o.bottom()]);
    }

    // Edge index: 0 = left/top, 1 = right/bottom.
    let mut mx = Vec::new();
    if h.moves_left() {
        mx.push((0, r.x));
    }
    if h.moves_right() {
        mx.push((1, r.right()));
    }
    if let Some((d, t, edge)) = best(&mx, &xs, threshold) {
        match (h, edge) {
            (Handle::Move, _) => out.x += d,
            (_, 0) => {
                out.x += d;
                out.w -= d;
            }
            _ => out.w += d,
        }
        guides.push(Guide { vertical: true, pos: t });
    }

    let mut my = Vec::new();
    if h.moves_top() {
        my.push((0, r.y));
    }
    if h.moves_bottom() {
        my.push((1, r.bottom()));
    }
    if let Some((d, t, edge)) = best(&my, &ys, threshold) {
        match (h, edge) {
            (Handle::Move, _) => out.y += d,
            (_, 0) => {
                out.y += d;
                out.h -= d;
            }
            _ => out.h += d,
        }
        guides.push(Guide { vertical: false, pos: t });
    }

    (out, guides)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    const BOUNDS: Rect = Rect { x: 0.0, y: 0.0, w: 1000.0, h: 800.0 };

    #[test]
    fn snap_to_neighbor_edge() {
        let other = r(100.0, 100.0, 200.0, 100.0); // right edge at 300
        let moving = r(305.0, 400.0, 150.0, 90.0);
        let (out, guides) = snap(moving, Handle::Move, &[other], BOUNDS, 8.0);
        assert_eq!(out.x, 300.0);
        assert_eq!(out.w, 150.0);
        assert_eq!(guides, vec![Guide { vertical: true, pos: 300.0 }]);
    }

    #[test]
    fn no_snap_beyond_threshold() {
        let other = r(100.0, 100.0, 200.0, 100.0);
        let moving = r(309.0, 400.0, 150.0, 90.0);
        let (out, guides) = snap(moving, Handle::Move, &[other], BOUNDS, 8.0);
        assert_eq!(out, moving);
        assert!(guides.is_empty());
    }

    #[test]
    fn snap_to_window_edge() {
        let (out, guides) = snap(r(4.0, 795.0 - 90.0, 150.0, 90.0), Handle::Move, &[], BOUNDS, 8.0);
        assert_eq!(out.x, 0.0);
        assert_eq!(out.bottom(), 800.0);
        assert_eq!(guides.len(), 2);
    }

    #[test]
    fn resize_snaps_only_dragged_edge() {
        let other = r(500.0, 0.0, 100.0, 100.0); // left edge at 500
        let moving = r(100.0, 300.0, 396.0, 100.0); // right at 496
        let (out, _) = snap(moving, Handle::E, &[other], BOUNDS, 8.0);
        assert_eq!(out.x, 100.0);
        assert_eq!(out.right(), 500.0);
    }
}
