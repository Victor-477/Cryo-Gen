//! Path finding for characters: a walkability grid built from the solid colliders around the
//! start and the goal, A* over it, and a straightened path.

use crate::scene::{BodyKind, ColliderShape, Scene};
use glam::{Vec2, Vec3};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

const CELL: f32 = 0.5;
const MAX_SIDE: i32 = 240;

struct Grid {
    origin: Vec2,
    w: i32,
    h: i32,
    blocked: Vec<bool>,
}

impl Grid {
    fn cell(&self, p: Vec3) -> (i32, i32) {
        (((p.x - self.origin.x) / CELL).floor() as i32, ((p.z - self.origin.y) / CELL).floor() as i32)
    }
    fn centre(&self, c: (i32, i32), y: f32) -> Vec3 {
        Vec3::new(self.origin.x + (c.0 as f32 + 0.5) * CELL, y, self.origin.y + (c.1 as f32 + 0.5) * CELL)
    }
    fn free(&self, c: (i32, i32)) -> bool {
        c.0 >= 0 && c.1 >= 0 && c.0 < self.w && c.1 < self.h && !self.blocked[(c.1 * self.w + c.0) as usize]
    }
    /// A straight walk between two cells crosses no blocked cell.
    fn clear_line(&self, a: (i32, i32), b: (i32, i32)) -> bool {
        let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs()) * 2;
        (0..=steps).all(|i| {
            let t = if steps == 0 { 0.0 } else { i as f32 / steps as f32 };
            let x = a.0 as f32 + (b.0 - a.0) as f32 * t;
            let z = a.1 as f32 + (b.1 - a.1) as f32 * t;
            self.free((x.round() as i32, z.round() as i32))
        })
    }
}

/// Builds the grid: a static, solid collider blocks a cell when it overlaps the height of a
/// body standing at `y` (so floors do not block, walls and crates do). `radius` keeps the
/// path that far from obstacles.
fn build(scene: &Scene, from: Vec3, to: Vec3, radius: f32, ignore: &[u64]) -> Grid {
    let lo = from.min(to) - Vec3::splat(24.0);
    let hi = from.max(to) + Vec3::splat(24.0);
    let w = (((hi.x - lo.x) / CELL).ceil() as i32).clamp(1, MAX_SIDE);
    let h = (((hi.z - lo.z) / CELL).ceil() as i32).clamp(1, MAX_SIDE);
    // keep the window centred between start and goal when it had to be cut
    let mid = (from + to) * 0.5;
    let origin = Vec2::new(mid.x - w as f32 * CELL * 0.5, mid.z - h as f32 * CELL * 0.5);
    let mut g = Grid { origin, w, h, blocked: vec![false; (w * h) as usize] };
    let (band_lo, band_hi) = (from.y - 0.55, from.y + 1.0);
    for e in &scene.entities {
        let Some(p) = &e.physics else { continue };
        if p.kind != BodyKind::Static || p.is_trigger || ignore.contains(&e.id) || !scene.is_visible(e.id) {
            continue;
        }
        if p.collider == ColliderShape::Mesh {
            continue;
        }
        let world = scene.world_matrix(e.id);
        let (c, half) = match &e.mesh {
            Some(m) => m.local_bounds(),
            None => (Vec3::ZERO, Vec3::splat(0.5)),
        };
        let mut bmin = Vec3::splat(f32::INFINITY);
        let mut bmax = Vec3::splat(f32::NEG_INFINITY);
        for i in 0..8 {
            let s = Vec3::new(if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 });
            let q = world.transform_point3(c + half * s);
            bmin = bmin.min(q);
            bmax = bmax.max(q);
        }
        if bmax.y < band_lo || bmin.y > band_hi {
            continue;
        }
        let (a, b) = (g.cell(bmin - Vec3::splat(radius)), g.cell(bmax + Vec3::splat(radius)));
        for z in a.1.max(0)..=b.1.min(h - 1) {
            for x in a.0.max(0)..=b.0.min(w - 1) {
                g.blocked[(z * w + x) as usize] = true;
            }
        }
    }
    g
}

#[derive(PartialEq)]
struct Node(f32, (i32, i32));
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A walkable path from `from` to `to`, as corner points (the last one is `to` itself).
/// `None` when the goal cannot be reached.
pub fn find_path(scene: &Scene, from: Vec3, to: Vec3, radius: f32, ignore: &[u64]) -> Option<Vec<Vec3>> {
    let g = build(scene, from, to, radius, ignore);
    let start = g.cell(from);
    let mut goal = g.cell(to);
    if !g.free(goal) {
        // the goal is inside an obstacle (a target leaning on a wall): use the nearest free cell
        goal = (1..8)
            .flat_map(|r| (-r..=r).flat_map(move |dx| (-r..=r).map(move |dz| (goal.0 + dx, goal.1 + dz))))
            .find(|c| g.free(*c))?;
    }
    let index = |c: (i32, i32)| (c.1 * g.w + c.0) as usize;
    let inside = |c: (i32, i32)| c.0 >= 0 && c.1 >= 0 && c.0 < g.w && c.1 < g.h;
    if !inside(start) || !inside(goal) {
        return None;
    }
    let mut cost = vec![f32::INFINITY; (g.w * g.h) as usize];
    let mut came = vec![usize::MAX; (g.w * g.h) as usize];
    let mut open = BinaryHeap::new();
    cost[index(start)] = 0.0;
    let guess = |c: (i32, i32)| (((c.0 - goal.0) as f32).powi(2) + ((c.1 - goal.1) as f32).powi(2)).sqrt();
    open.push(Node(guess(start), start));
    let mut found = false;
    while let Some(Node(_, c)) = open.pop() {
        if c == goal {
            found = true;
            break;
        }
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let n = (c.0 + dx, c.1 + dz);
            // the start may be touching an obstacle; from there any neighbour is allowed
            if !g.free(n) && n != goal {
                continue;
            }
            if dx != 0 && dz != 0 && (!g.free((c.0 + dx, c.1)) || !g.free((c.0, c.1 + dz))) {
                continue; // no cutting corners
            }
            let step = if dx != 0 && dz != 0 { 1.414 } else { 1.0 };
            let new_cost = cost[index(c)] + step;
            if new_cost < cost[index(n)] {
                cost[index(n)] = new_cost;
                came[index(n)] = index(c);
                open.push(Node(new_cost + guess(n), n));
            }
        }
    }
    if !found {
        return None;
    }
    let mut cells = vec![goal];
    let mut at = index(goal);
    while came[at] != usize::MAX {
        at = came[at];
        cells.push(((at as i32) % g.w, (at as i32) / g.w));
    }
    cells.reverse();
    // string pulling: keep only the corners the path really needs
    let mut corners = vec![];
    let mut anchor = cells[0];
    for i in 1..cells.len() {
        if !g.clear_line(anchor, cells[i]) {
            anchor = cells[i - 1];
            corners.push(g.centre(anchor, from.y));
        }
    }
    corners.push(Vec3::new(to.x, from.y, to.z));
    Some(corners)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Physics, Shape};

    #[test]
    fn walks_around_a_wall_and_ignores_the_floor() {
        let mut s = Scene::default_scene(); // has a ground slab
        let wall = s.add_mesh(Shape::Cube);
        let e = s.get_mut(wall).unwrap();
        e.position = Vec3::new(0.0, 1.5, 0.0);
        e.scale = Vec3::new(10.0, 3.0, 1.0);
        e.physics = Some(Physics::new(BodyKind::Static, ColliderShape::Box));
        let from = Vec3::new(0.0, 0.9, 6.0);
        let to = Vec3::new(0.0, 0.9, -6.0);
        let path = find_path(&s, from, to, 0.4, &[]).expect("a path around the wall");
        assert!(path.len() >= 2, "{path:?}");
        // it goes past one end of the wall, never through it
        let widest = path.iter().map(|p| p.x.abs()).fold(0.0f32, f32::max);
        assert!(widest > 5.0, "{path:?}");
        for w in path.windows(2) {
            for k in 0..=20 {
                let p = w[0].lerp(w[1], k as f32 / 20.0);
                assert!(!(p.x.abs() < 5.3 && p.z.abs() < 0.8), "crosses the wall at {p:?}");
            }
        }
        assert!((path.last().unwrap().z + 6.0).abs() < 1e-4);
        // nothing in the way: a single straight segment
        let direct = find_path(&s, from, Vec3::new(3.0, 0.9, 9.0), 0.4, &[]).unwrap();
        assert_eq!(direct.len(), 1);
        // a closed box: no way in
        for (x, z, sx, sz) in [(0.0, -9.0, 4.0, 0.4), (0.0, -13.0, 4.0, 0.4), (-2.0, -11.0, 0.4, 4.0), (2.0, -11.0, 0.4, 4.0)] {
            let id = s.add_mesh(Shape::Cube);
            let e = s.get_mut(id).unwrap();
            e.position = Vec3::new(x, 1.5, z);
            e.scale = Vec3::new(sx, 3.0, sz);
            e.physics = Some(Physics::new(BodyKind::Static, ColliderShape::Box));
        }
        assert!(find_path(&s, from, Vec3::new(0.0, 0.9, -11.0), 0.3, &[]).is_none());
    }
}
