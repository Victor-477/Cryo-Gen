//! Simple, stable physics: gravity, oriented boxes (with rotation) and spheres.
//!
//! Works in world space, so parented entities also collide correctly.
//! Bodies marked "can rotate" also spin, roll and tip over; static bodies may use the
//! triangles of their model as the collider.

use crate::scene::{BodyKind, ColliderShape, Scene};
use glam::{Mat3, Mat4, Quat, Vec3};

#[derive(Clone)]
enum Col {
    Box { half: Vec3 },
    Sphere { r: f32 },
    /// Triangles in world space.
    Mesh { tris: Vec<[Vec3; 3]> },
}

struct Body {
    idx: usize,
    id: u64,
    /// Collider centre in world space.
    pos: Vec3,
    rot: Quat,
    /// The box is aligned with the world axes (fast, more stable path).
    aligned: bool,
    col: Col,
    dynamic: bool,
    trigger: bool,
    spin: bool,
    /// Converts a world-space displacement into the parent's space.
    parent_inv: Mat4,
}

fn gather(scene: &Scene) -> Vec<Body> {
    scene
        .entities
        .iter()
        .enumerate()
        .filter_map(|(idx, e)| {
            let p = e.physics.as_ref()?;
            if !scene.is_visible(e.id) {
                return None;
            }
            let world = scene.world_matrix(e.id);
            if world.determinant().abs() < 1e-9 {
                return None;
            }
            let (scale, rot, _) = world.to_scale_rotation_translation();
            let (center, base) = match &e.mesh {
                Some(m) => m.local_bounds(),
                None => (Vec3::ZERO, Vec3::splat(0.5)),
            };
            let half = base * scale.abs();
            let col = match (p.collider, &p.mesh) {
                (ColliderShape::Sphere, _) => Col::Sphere { r: half.max_element() },
                (ColliderShape::Mesh, Some(tris)) if p.kind == BodyKind::Static => Col::Mesh {
                    tris: tris.iter().map(|t| t.map(|v| world.transform_point3(v))).collect(),
                },
                _ => Col::Box { half },
            };
            let parent = scene.parent_matrix(e.id);
            Some(Body {
                idx,
                id: e.id,
                pos: world.transform_point3(center),
                rot,
                aligned: rot.abs_diff_eq(Quat::IDENTITY, 1e-4) || rot.abs_diff_eq(-Quat::IDENTITY, 1e-4),
                col,
                dynamic: p.kind == BodyKind::Dynamic,
                trigger: p.is_trigger,
                spin: p.can_rotate && p.kind == BodyKind::Dynamic && !p.is_trigger,
                parent_inv: if e.parent.is_some() { parent.inverse() } else { Mat4::IDENTITY },
            })
        })
        .collect()
}

fn sign(v: f32) -> f32 {
    if v >= 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// Axis-aligned boxes.
fn aabb(d: Vec3, ha: Vec3, hb: Vec3) -> Option<(Vec3, f32)> {
    let pen = ha + hb - d.abs();
    if pen.x <= 0.0 || pen.y <= 0.0 || pen.z <= 0.0 {
        return None;
    }
    if pen.x <= pen.y && pen.x <= pen.z {
        Some((Vec3::X * sign(d.x), pen.x))
    } else if pen.y <= pen.z {
        Some((Vec3::Y * sign(d.y), pen.y))
    } else {
        Some((Vec3::Z * sign(d.z), pen.z))
    }
}

/// Oriented boxes: separating axis theorem (15 axes).
fn obb(a: &Body, ha: Vec3, b: &Body, hb: Vec3) -> Option<(Vec3, f32)> {
    let ma = Mat3::from_quat(a.rot);
    let mb = Mat3::from_quat(b.rot);
    let ax = [ma.x_axis, ma.y_axis, ma.z_axis];
    let bx = [mb.x_axis, mb.y_axis, mb.z_axis];
    let d = a.pos - b.pos;
    let mut best: Option<(Vec3, f32)> = None;
    let mut test = |axis: Vec3, penalty: f32| -> bool {
        let len = axis.length();
        if len < 1e-5 {
            return true; // parallel axes: already covered by the face axes
        }
        let l = axis / len;
        let ra = ha.x * ax[0].dot(l).abs() + ha.y * ax[1].dot(l).abs() + ha.z * ax[2].dot(l).abs();
        let rb = hb.x * bx[0].dot(l).abs() + hb.y * bx[1].dot(l).abs() + hb.z * bx[2].dot(l).abs();
        let dist = d.dot(l);
        let overlap = ra + rb - dist.abs();
        if overlap <= 0.0 {
            return false;
        }
        // slight preference for face axes (more stable contacts)
        if best.map(|(_, o)| overlap * penalty < o).unwrap_or(true) {
            best = Some((l * sign(dist), overlap * penalty));
        }
        true
    };
    for i in 0..3 {
        if !test(ax[i], 1.0) || !test(bx[i], 1.0) {
            return None;
        }
    }
    for i in 0..3 {
        for j in 0..3 {
            if !test(ax[i].cross(bx[j]), 1.05) {
                return None;
            }
        }
    }
    best
}

/// Returns (normal pointing from B to A, depth) if they overlap.
fn overlap(a: &Body, b: &Body) -> Option<(Vec3, f32)> {
    match (&a.col, &b.col) {
        (Col::Mesh { .. }, Col::Mesh { .. }) => None,
        (Col::Mesh { tris }, _) => mesh_overlap(tris, b).map(|(n, d)| (-n, d)),
        (_, Col::Mesh { tris }) => mesh_overlap(tris, a),
        (Col::Box { half: ha }, Col::Box { half: hb }) => {
            let (ha, hb) = (*ha, *hb);
            if a.aligned && b.aligned {
                aabb(a.pos - b.pos, ha, hb)
            } else {
                obb(a, ha, b, hb)
            }
        }
        (Col::Sphere { r: ra }, Col::Sphere { r: rb }) => {
            let d = a.pos - b.pos;
            let dist = d.length();
            let pen = ra + rb - dist;
            if pen <= 0.0 {
                return None;
            }
            let n = if dist > 1e-6 { d / dist } else { Vec3::Y };
            Some((n, pen))
        }
        (Col::Sphere { r }, Col::Box { half }) => sphere_box(a.pos, *r, b, *half),
        (Col::Box { half }, Col::Sphere { r }) => sphere_box(b.pos, *r, a, *half).map(|(n, p)| (-n, p)),
    }
}

/// Deepest contact between a triangle mesh and a box or sphere.
/// The normal points from the mesh towards the body.
fn mesh_overlap(tris: &[[Vec3; 3]], body: &Body) -> Option<(Vec3, f32)> {
    let reach = match &body.col {
        Col::Sphere { r } => *r,
        Col::Box { half } => half.length(),
        Col::Mesh { .. } => return None,
    };
    let (lo, hi) = (body.pos - Vec3::splat(reach), body.pos + Vec3::splat(reach));
    let inv = body.rot.inverse();
    let mut best: Option<(Vec3, f32)> = None;
    for t in tris {
        let (tmin, tmax) = (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]));
        if tmin.cmpgt(hi).any() || tmax.cmplt(lo).any() {
            continue;
        }
        let hit = match &body.col {
            Col::Sphere { r } => sphere_triangle(body.pos, *r, t),
            Col::Box { half } => {
                let local = t.map(|v| inv * (v - body.pos));
                box_triangle(*half, &local).map(|(n, d)| (body.rot * n, d))
            }
            Col::Mesh { .. } => None,
        };
        if let Some((n, d)) = hit {
            if best.map(|(_, bd)| d > bd).unwrap_or(true) {
                best = Some((n, d));
            }
        }
    }
    best
}

fn sphere_triangle(c: Vec3, r: f32, t: &[Vec3; 3]) -> Option<(Vec3, f32)> {
    let p = closest_on_triangle(c, t);
    let d = c - p;
    let dist = d.length();
    if dist >= r {
        return None;
    }
    let n = if dist > 1e-5 { d / dist } else { (t[1] - t[0]).cross(t[2] - t[0]).normalize_or(Vec3::Y) };
    Some((n, r - dist))
}

/// Closest point of a triangle to `p` (Ericson, Real-Time Collision Detection).
fn closest_on_triangle(p: Vec3, t: &[Vec3; 3]) -> Vec3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Box centred at the origin against a triangle given in the box's space (separating axes).
/// Returns the direction that moves the box out, and how far.
fn box_triangle(half: Vec3, t: &[Vec3; 3]) -> Option<(Vec3, f32)> {
    let edges = [t[1] - t[0], t[2] - t[1], t[0] - t[2]];
    let mut axes: Vec<Vec3> = vec![edges[0].cross(edges[1]), Vec3::X, Vec3::Y, Vec3::Z];
    for e in edges {
        axes.extend([Vec3::X.cross(e), Vec3::Y.cross(e), Vec3::Z.cross(e)]);
    }
    let mut best: Option<(Vec3, f32)> = None;
    for (i, axis) in axes.into_iter().enumerate() {
        let len = axis.length();
        if len < 1e-6 {
            continue;
        }
        let l = axis / len;
        let p = [t[0].dot(l), t[1].dot(l), t[2].dot(l)];
        let (tmin, tmax) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
        let r = half.dot(l.abs());
        if tmin > r || tmax < -r {
            return None;
        }
        // the triangle sits on one side of the box centre: leave through the other
        let (dir, depth) = if tmin + tmax > 0.0 { (-l, r - tmin) } else { (l, tmax + r) };
        // slight preference for the triangle's own face (i == 0): steadier floors and walls
        let weight = if i == 0 { 1.0 } else { 1.05 };
        if best.map(|(_, d)| depth * weight < d).unwrap_or(true) {
            best = Some((dir, depth * weight));
        }
    }
    best
}

/// Where a body touches something that pushes it along `normal`.
fn support_point(b: &Body, normal: Vec3) -> Vec3 {
    match &b.col {
        Col::Sphere { r } => b.pos - normal * *r,
        Col::Box { half } => {
            let mut corners = [(0.0f32, Vec3::ZERO); 8];
            for (i, c) in corners.iter_mut().enumerate() {
                let s = Vec3::new(
                    if i & 1 == 0 { -1.0 } else { 1.0 },
                    if i & 2 == 0 { -1.0 } else { 1.0 },
                    if i & 4 == 0 { -1.0 } else { 1.0 },
                );
                let p = b.pos + b.rot * (s * *half);
                *c = (p.dot(normal), p);
            }
            let lowest = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
            let eps = 0.02 * half.max_element().max(0.05);
            let (mut sum, mut n) = (Vec3::ZERO, 0.0);
            for (d, p) in corners {
                if d - lowest <= eps {
                    sum += p;
                    n += 1.0;
                }
            }
            sum / n
        }
        Col::Mesh { .. } => b.pos,
    }
}

/// Inverse inertia of a body of mass 1, in world space.
fn inverse_inertia(b: &Body) -> Mat3 {
    let local = match &b.col {
        Col::Sphere { r } => Vec3::splat(0.4 * r * r),
        Col::Box { half } => {
            let s = *half * 2.0;
            Vec3::new(s.y * s.y + s.z * s.z, s.x * s.x + s.z * s.z, s.x * s.x + s.y * s.y) / 12.0
        }
        Col::Mesh { .. } => Vec3::ONE,
    };
    let r = Mat3::from_quat(b.rot);
    r * Mat3::from_diagonal(Vec3::ONE / local.max(Vec3::splat(1e-5))) * r.transpose()
}

/// Collision response for a body that can rotate: an impulse at the contact point changes
/// both its velocity and its spin. `other_v` is the velocity of what it hit.
fn spin_response(
    b: &Body,
    v: &mut Vec3,
    w: &mut Vec3,
    normal: Vec3,
    other_v: Vec3,
    bounce: f32,
    friction: f32,
) {
    let r = support_point(b, normal) - b.pos;
    let inv = inverse_inertia(b);
    let vp = *v + w.cross(r) - other_v;
    let vn = vp.dot(normal);
    if vn >= 0.0 {
        return;
    }
    let restitution = if vn.abs() > 1.0 { bounce } else { 0.0 };
    let k = 1.0 + normal.dot((inv * r.cross(normal)).cross(r));
    let jn = -(1.0 + restitution) * vn / k.max(1e-4);
    *v += normal * jn;
    *w += inv * r.cross(normal * jn);
    // friction acts against the sliding of the contact point; it is what makes things roll
    let vp = *v + w.cross(r) - other_v;
    let vt = vp - normal * vp.dot(normal);
    let speed = vt.length();
    if speed > 1e-4 {
        let t = vt / speed;
        let kt = 1.0 + t.dot((inv * r.cross(t)).cross(r));
        let jt = (speed / kt.max(1e-4)).min(friction * jn);
        *v -= t * jt;
        *w -= inv * r.cross(t * jt);
    }
}

/// Normal pointing from the box to the sphere.
fn sphere_box(sp: Vec3, r: f32, bx: &Body, half: Vec3) -> Option<(Vec3, f32)> {
    // works in the box's local space
    let inv = bx.rot.inverse();
    let local = inv * (sp - bx.pos);
    let closest = local.clamp(-half, half);
    let d = local - closest;
    let dist = d.length();
    if dist > 1e-6 {
        if dist >= r {
            return None;
        }
        Some((bx.rot * (d / dist), r - dist))
    } else {
        // sphere centre inside the box: exit through the nearest side
        let pen = half - local.abs();
        let (n, depth) = if pen.x <= pen.y && pen.x <= pen.z {
            (Vec3::X * sign(local.x), pen.x + r)
        } else if pen.y <= pen.z {
            (Vec3::Y * sign(local.y), pen.y + r)
        } else {
            (Vec3::Z * sign(local.z), pen.z + r)
        };
        Some((bx.rot * n, depth))
    }
}

/// The first solid collider a ray meets: (entity, distance, surface normal).
/// Triggers and the `ignore` entity are skipped; so is a box the ray starts inside.
pub fn raycast(scene: &Scene, origin: Vec3, dir: Vec3, max: f32, ignore: Option<u64>) -> Option<(u64, f32, Vec3)> {
    let d = dir.normalize_or_zero();
    if d == Vec3::ZERO {
        return None;
    }
    let mut best: Option<(u64, f32, Vec3)> = None;
    for b in gather(scene) {
        if b.trigger || Some(b.id) == ignore {
            continue;
        }
        let hit = match &b.col {
            Col::Sphere { r } => {
                let oc = origin - b.pos;
                let half_b = oc.dot(d);
                let disc = half_b * half_b - (oc.length_squared() - r * r);
                let t = -half_b - disc.max(0.0).sqrt();
                (disc >= 0.0 && t >= 0.0).then(|| (t, (origin + d * t - b.pos).normalize_or(Vec3::Y)))
            }
            Col::Box { half } => {
                let inv = b.rot.inverse();
                let (o, dl) = (inv * (origin - b.pos), inv * d);
                let (mut t0, mut t1, mut axis) = (f32::NEG_INFINITY, f32::INFINITY, 0);
                let mut miss = false;
                for i in 0..3 {
                    if dl[i].abs() < 1e-7 {
                        miss |= o[i].abs() > half[i];
                        continue;
                    }
                    let (a, c) = ((-half[i] - o[i]) / dl[i], (half[i] - o[i]) / dl[i]);
                    let (near, far) = (a.min(c), a.max(c));
                    if near > t0 {
                        t0 = near;
                        axis = i;
                    }
                    t1 = t1.min(far);
                }
                (!miss && t0 <= t1 && t0 >= 0.0).then(|| {
                    let mut n = Vec3::ZERO;
                    n[axis] = -sign(dl[axis]);
                    (t0, b.rot * n)
                })
            }
            Col::Mesh { tris } => {
                let mut nearest: Option<(f32, Vec3)> = None;
                for t in tris {
                    // Moller-Trumbore
                    let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
                    let p = d.cross(e2);
                    let det = e1.dot(p);
                    if det.abs() < 1e-8 {
                        continue;
                    }
                    let s = origin - t[0];
                    let u = s.dot(p) / det;
                    let q = s.cross(e1);
                    let v = d.dot(q) / det;
                    let dist = e2.dot(q) / det;
                    if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && dist >= 0.0 && nearest.map(|n| dist < n.0).unwrap_or(true) {
                        let n = e1.cross(e2).normalize_or(Vec3::Y);
                        nearest = Some((dist, if n.dot(d) > 0.0 { -n } else { n }));
                    }
                }
                nearest
            }
        };
        if let Some((t, n)) = hit {
            if t <= max && best.map(|h| t < h.1).unwrap_or(true) {
                best = Some((b.id, t, n));
            }
        }
    }
    best
}

/// Advances the simulation and returns the contacts found.
/// One touch between two entities. `normal` points from `b` towards `a`.
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub a: u64,
    pub b: u64,
    pub normal: Vec3,
}

pub fn step(scene: &mut Scene, dt: f32) -> Vec<Contact> {
    let mut contacts: Vec<Contact> = vec![];
    let dt = dt.min(1.0 / 20.0);
    const SUB: usize = 3;
    let h = dt / SUB as f32;
    let gravity = scene.gravity;

    for e in &mut scene.entities {
        if let Some(p) = &mut e.physics {
            p.grounded = false;
        }
    }

    for _ in 0..SUB {
        // integration
        for e in &mut scene.entities {
            let Some(p) = &mut e.physics else { continue };
            if p.kind != BodyKind::Dynamic {
                continue;
            }
            if p.use_gravity {
                p.velocity.y += gravity * h;
            }
            let step = p.velocity * h;
            e.position += match e.parent {
                None => step,
                Some(_) => Vec3::ZERO, // applied below, converted to the parent's space
            };
        }
        let moved: Vec<(usize, Vec3)> = scene
            .entities
            .iter()
            .enumerate()
            .filter(|(_, e)| e.parent.is_some())
            .filter_map(|(i, e)| {
                let p = e.physics.as_ref()?;
                (p.kind == BodyKind::Dynamic)
                    .then(|| (i, scene.parent_matrix(e.id).inverse().transform_vector3(p.velocity * h)))
            })
            .collect();
        for (i, d) in moved {
            scene.entities[i].position += d;
        }
        // spin
        let spinning: Vec<(u64, Vec3)> = scene
            .entities
            .iter_mut()
            .filter_map(|e| {
                let p = e.physics.as_mut()?;
                if !p.can_rotate || p.kind != BodyKind::Dynamic {
                    p.angular = Vec3::ZERO;
                    return None;
                }
                p.angular *= 1.0 - (0.4 * h).min(1.0); // air drag
                if p.angular.length_squared() > 60.0 * 60.0 {
                    p.angular = p.angular.normalize() * 60.0;
                }
                (p.angular.length_squared() > 1e-8).then_some((e.id, p.angular))
            })
            .collect();
        for (id, w) in spinning {
            let (_, rot, pos) = scene.world_matrix(id).to_scale_rotation_translation();
            scene.set_world_pose(id, pos, (Quat::from_scaled_axis(w * h) * rot).normalize());
        }

        // collisions
        let mut bodies = gather(scene);
        // broad phase: only moving bodies start a pair, and only bodies whose bounding
        // spheres touch go on to the exact test
        let reach: Vec<f32> = bodies
            .iter()
            .map(|b| match &b.col {
                Col::Box { half } => half.length(),
                Col::Sphere { r } => *r,
                Col::Mesh { .. } => f32::INFINITY,
            })
            .collect();
        let movers: Vec<usize> = (0..bodies.len()).filter(|i| bodies[*i].dynamic).collect();
        for &i in &movers {
            for j in 0..bodies.len() {
                if j == i || (bodies[j].dynamic && j < i) {
                    continue;
                }
                let far = reach[i] + reach[j];
                if far.is_finite() && bodies[i].pos.distance_squared(bodies[j].pos) > far * far {
                    continue;
                }
                let Some((n, depth)) = overlap(&bodies[i], &bodies[j]) else { continue };
                let pair = (bodies[i].id, bodies[j].id);
                if !contacts.iter().any(|c| (c.a, c.b) == pair) {
                    contacts.push(Contact { a: pair.0, b: pair.1, normal: n });
                }
                if bodies[i].trigger || bodies[j].trigger {
                    continue;
                }
                let (wi, wj) = match (bodies[i].dynamic, bodies[j].dynamic) {
                    (true, true) => (0.5, 0.5),
                    (true, false) => (1.0, 0.0),
                    _ => (0.0, 1.0),
                };
                let (di, dj) = (n * depth * wi, -n * depth * wj);
                bodies[i].pos += di;
                bodies[j].pos += dj;
                let (ii, ij) = (bodies[i].idx, bodies[j].idx);
                scene.entities[ii].position += bodies[i].parent_inv.transform_vector3(di);
                scene.entities[ij].position += bodies[j].parent_inv.transform_vector3(dj);

                let vi = scene.entities[ii].physics.as_ref().map(|p| p.velocity).unwrap_or_default();
                let vj = scene.entities[ij].physics.as_ref().map(|p| p.velocity).unwrap_or_default();
                let rel = (vi - vj).dot(n);
                let bounce = scene.entities[ii]
                    .physics
                    .as_ref()
                    .map(|p| p.bounciness)
                    .unwrap_or(0.0)
                    .max(scene.entities[ij].physics.as_ref().map(|p| p.bounciness).unwrap_or(0.0));
                let friction = scene.entities[ii]
                    .physics
                    .as_ref()
                    .map(|p| p.friction)
                    .unwrap_or(0.5)
                    .min(scene.entities[ij].physics.as_ref().map(|p| p.friction).unwrap_or(0.5));

                for (idx, w, normal, bi, other_v) in [(ii, wi, n, i, vj), (ij, wj, -n, j, vi)] {
                    if w == 0.0 {
                        continue;
                    }
                    let Some(p) = scene.entities[idx].physics.as_mut() else { continue };
                    if bodies[bi].spin {
                        let (mut v, mut spin) = (p.velocity, p.angular);
                        spin_response(&bodies[bi], &mut v, &mut spin, normal, other_v, bounce, friction);
                        // lying still on something: stop the last tremble
                        if normal.y > 0.6 {
                            p.grounded = true;
                            if spin.length_squared() < 0.02 && v.length_squared() < 0.02 {
                                spin = Vec3::ZERO;
                            } else {
                                spin *= 1.0 - (2.0 * h).min(1.0);
                            }
                        }
                        p.velocity = v;
                        p.angular = spin;
                        continue;
                    }
                    if rel < 0.0 {
                        // removes the approach velocity (with optional bounce)
                        let vn = p.velocity.dot(normal);
                        let restitution = if vn.abs() > 1.0 { bounce } else { 0.0 };
                        if vn < 0.0 {
                            p.velocity -= normal * vn * (1.0 + restitution);
                        }
                    }
                    if normal.y > 0.6 {
                        p.grounded = true;
                        // ground friction
                        let k = (1.0 - friction * 6.0 * h).clamp(0.0, 1.0);
                        p.velocity.x *= k;
                        p.velocity.z *= k;
                    }
                }
            }
        }
    }
    contacts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Physics, Shape};

    #[test]
    fn cube_falls_and_rests_on_ground() {
        let mut s = Scene::default_scene();
        let cube = s.find_by_name("Cube").unwrap();
        let e = s.get_mut(cube).unwrap();
        e.position.y = 5.0;
        e.physics = Some(Physics::new(BodyKind::Dynamic, ColliderShape::Box));
        for _ in 0..300 {
            step(&mut s, 1.0 / 60.0);
        }
        let e = s.get(cube).unwrap();
        assert!((e.position.y - 0.5).abs() < 0.02, "y = {}", e.position.y);
        assert!(e.physics.as_ref().unwrap().grounded);
    }

    #[test]
    fn sphere_rests_on_box() {
        let mut s = Scene::default_scene();
        let id = s.add_mesh(Shape::Sphere);
        let e = s.get_mut(id).unwrap();
        e.position = Vec3::new(3.0, 4.0, 0.0);
        e.physics = Some(Physics::new(BodyKind::Dynamic, ColliderShape::Sphere));
        for _ in 0..300 {
            step(&mut s, 1.0 / 60.0);
        }
        assert!((s.get(id).unwrap().position.y - 0.5).abs() < 0.02);
    }

    #[test]
    fn cube_rests_on_tilted_ramp() {
        // ramp rotated 30 degrees: the cube must rest on the sloped face, not pass through
        let mut s = Scene::empty();
        let ramp = s.add_mesh(Shape::Cube);
        let e = s.get_mut(ramp).unwrap();
        e.scale = Vec3::new(10.0, 1.0, 10.0);
        e.rotation = Vec3::new(0.0, 0.0, 30.0);
        e.physics = Some(Physics::new(BodyKind::Static, ColliderShape::Box));
        let cube = s.add_mesh(Shape::Cube);
        let e = s.get_mut(cube).unwrap();
        e.position = Vec3::new(0.0, 4.0, 0.0);
        let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Box);
        p.friction = 1.0;
        e.physics = Some(p);
        let mut touched = false;
        for _ in 0..120 {
            touched |= !step(&mut s, 1.0 / 60.0).is_empty();
        }
        assert!(touched);
        // the top face of the ramp passes through y = x·tan(30°) + 0.577; the cube must be above it
        let p = s.get(cube).unwrap().position;
        let surface = p.x * 30f32.to_radians().tan() + 0.577;
        assert!(p.y > surface + 0.3 && p.y < surface + 1.2, "pos = {p:?}, surface = {surface}");
    }

    #[test]
    fn child_of_moved_parent_collides_in_world_space() {
        let mut s = Scene::default_scene();
        let parent = s.add_empty("Parent");
        s.get_mut(parent).unwrap().position = Vec3::new(4.0, 10.0, 0.0);
        let id = s.add_mesh(Shape::Sphere);
        let e = s.get_mut(id).unwrap();
        e.parent = Some(parent);
        e.position = Vec3::new(0.0, -6.0, 0.0); // world: y = 4
        e.physics = Some(Physics::new(BodyKind::Dynamic, ColliderShape::Sphere));
        for _ in 0..300 {
            step(&mut s, 1.0 / 60.0);
        }
        let world_y = s.world_matrix(id).w_axis.y;
        assert!((world_y - 0.5).abs() < 0.03, "y = {world_y}");
    }

    #[test]
    fn ball_rolls_and_a_tilted_box_falls_flat() {
        let mut s = Scene::default_scene(); // has a static ground with its top at y = 0
        // a ball pushed sideways starts to roll: it spins around Z, clockwise seen from +Z
        let ball = s.add_mesh(Shape::Sphere);
        {
            let e = s.get_mut(ball).unwrap();
            e.position = Vec3::new(-6.0, 0.5, 4.0);
            let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Sphere);
            p.can_rotate = true;
            p.velocity = Vec3::new(4.0, 0.0, 0.0);
            e.physics = Some(p);
        }
        // a box dropped on its edge
        let cube = s.add_mesh(Shape::Cube);
        {
            let e = s.get_mut(cube).unwrap();
            e.position = Vec3::new(5.0, 2.0, 4.0);
            e.rotation = Vec3::new(0.0, 0.0, 35.0);
            let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Box);
            p.can_rotate = true;
            e.physics = Some(p);
        }
        for _ in 0..30 {
            step(&mut s, 1.0 / 60.0);
        }
        let p = s.get(ball).unwrap().physics.clone().unwrap();
        assert!(p.angular.z < -1.0, "the ball spins as it moves: {:?}", p.angular);
        assert!(p.velocity.x > 1.0 && p.velocity.x < 4.0, "friction turned part of the slide into spin: {:?}", p.velocity);
        // rolling without slipping: speed = spin x radius
        assert!((p.velocity.x + p.angular.z * 0.5).abs() < 0.6, "v = {:?}, w = {:?}", p.velocity, p.angular);

        for _ in 0..600 {
            step(&mut s, 1.0 / 60.0);
        }
        let e = s.get(cube).unwrap();
        let up = e.rotation_quat() * Vec3::Y;
        let flat = up.abs().max_element();
        assert!(flat > 0.995, "the box came to rest on one of its faces: up = {up:?}");
        assert!((e.position.y - 0.5).abs() < 0.05, "y = {}", e.position.y);
        let p = e.physics.as_ref().unwrap();
        assert!(p.angular.length() < 0.05 && p.velocity.length() < 0.2, "and it is at rest: {:?} {:?}", p.angular, p.velocity);
        assert!(e.position.is_finite() && e.position.distance(Vec3::new(5.0, 0.5, 4.0)) < 3.0);
    }

    #[test]
    fn mesh_collider_follows_the_triangles() {
        // a ramp made of two triangles, rising 1 m for every 2 m in +X, plus a wall at x = 6
        let ramp = vec![
            [Vec3::new(-4.0, -2.0, -3.0), Vec3::new(-4.0, -2.0, 3.0), Vec3::new(6.0, 3.0, 3.0)],
            [Vec3::new(-4.0, -2.0, -3.0), Vec3::new(6.0, 3.0, 3.0), Vec3::new(6.0, 3.0, -3.0)],
            [Vec3::new(6.0, 0.0, -3.0), Vec3::new(6.0, 0.0, 3.0), Vec3::new(6.0, 9.0, 3.0)],
            [Vec3::new(6.0, 0.0, -3.0), Vec3::new(6.0, 9.0, 3.0), Vec3::new(6.0, 9.0, -3.0)],
        ];
        let mut s = Scene::empty();
        let level = s.add_empty("Level");
        let mut p = Physics::new(BodyKind::Static, ColliderShape::Mesh);
        p.mesh = Some(std::sync::Arc::new(ramp));
        s.get_mut(level).unwrap().physics = Some(p);

        let ball = s.add_mesh(Shape::Sphere);
        let e = s.get_mut(ball).unwrap();
        e.position = Vec3::new(0.0, 4.0, 0.0);
        e.physics = Some(Physics::new(BodyKind::Dynamic, ColliderShape::Sphere));
        let cube = s.add_mesh(Shape::Cube);
        let e = s.get_mut(cube).unwrap();
        e.position = Vec3::new(3.0, 5.0, 1.0);
        let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Box);
        p.friction = 0.0;
        e.physics = Some(p);

        let height = |x: f32| (x + 4.0) * 0.5 - 2.0;
        let mut seen_normal = Vec3::ZERO;
        for _ in 0..70 {
            for c in step(&mut s, 1.0 / 60.0) {
                if c.a == ball || c.b == ball {
                    seen_normal = if c.a == ball { c.normal } else { -c.normal };
                }
            }
        }
        // the contact normal is the slope's, tilted towards -X
        assert!(seen_normal.y > 0.85 && seen_normal.x < -0.3, "{seen_normal:?}");
        for _ in 0..60 {
            step(&mut s, 1.0 / 60.0);
        }
        // both slid down the slope but never fell through it
        for id in [ball, cube] {
            let p = s.get(id).unwrap().position;
            assert!(p.x < -0.5, "slid downhill: {p:?}");
            if p.x > -4.0 {
                assert!(p.y > height(p.x) + 0.3 && p.y < height(p.x) + 1.3, "on the surface: {p:?}");
            }
        }
        // thrown at the wall, the ball stops there
        let e = s.get_mut(ball).unwrap();
        e.position = Vec3::new(4.0, 6.0, 0.0);
        e.physics.as_mut().unwrap().velocity = Vec3::new(20.0, 0.0, 0.0);
        e.physics.as_mut().unwrap().use_gravity = false;
        for _ in 0..30 {
            step(&mut s, 1.0 / 60.0);
        }
        let p = s.get(ball).unwrap().position;
        assert!(p.x < 5.55 && p.x > 5.0, "stopped by the wall: {p:?}");
    }

    /// Not a pass/fail test: prints how long a crowded scene takes to simulate.
    #[test]
    fn crowded_scene_timing() {
        let mut s = Scene::default_scene();
        for i in 0..400 {
            let id = s.add_mesh(Shape::Cube);
            let e = s.get_mut(id).unwrap();
            e.position = Vec3::new((i % 20) as f32 * 4.0 - 40.0, 0.5, (i / 20) as f32 * 4.0 - 40.0);
            e.rotation = Vec3::new(0.0, i as f32 * 7.0, 0.0);
            e.physics = Some(Physics::new(BodyKind::Static, ColliderShape::Box));
        }
        for i in 0..40 {
            let id = s.add_mesh(Shape::Sphere);
            let e = s.get_mut(id).unwrap();
            e.position = Vec3::new((i % 8) as f32 * 9.0 - 30.0, 3.0 + i as f32 * 0.1, (i / 8) as f32 * 9.0 - 20.0);
            e.physics = Some(Physics::new(BodyKind::Dynamic, ColliderShape::Sphere));
        }
        let t = std::time::Instant::now();
        for _ in 0..120 {
            step(&mut s, 1.0 / 60.0);
        }
        println!("PHYSICS 440 bodies: {:.2} ms per frame", t.elapsed().as_secs_f64() * 1000.0 / 120.0);
    }
}
