//! Física simples e estável: gravidade, caixas orientadas (com rotação) e esferas.
//!
//! Funciona em espaço de mundo, então entidades com pai também colidem corretamente.
//! Limite conhecido: não há dinâmica de rotação — os corpos não tombam nem giram ao bater.

use crate::scene::{BodyKind, ColliderShape, Scene};
use glam::{Mat3, Mat4, Quat, Vec3};

#[derive(Clone, Copy)]
enum Col {
    Box { half: Vec3 },
    Sphere { r: f32 },
}

struct Body {
    idx: usize,
    id: u64,
    /// Centro do colisor no mundo.
    pos: Vec3,
    rot: Quat,
    /// A caixa está alinhada aos eixos do mundo (caminho rápido e mais estável).
    aligned: bool,
    col: Col,
    dynamic: bool,
    trigger: bool,
    /// Converte um deslocamento do mundo para o espaço do pai.
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
            let col = match p.collider {
                ColliderShape::Box => Col::Box { half },
                ColliderShape::Sphere => Col::Sphere { r: half.max_element() },
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

/// Caixas alinhadas aos eixos.
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

/// Caixas orientadas: teorema do eixo separador (15 eixos).
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
            return true; // eixos paralelos: já coberto pelos eixos das faces
        }
        let l = axis / len;
        let ra = ha.x * ax[0].dot(l).abs() + ha.y * ax[1].dot(l).abs() + ha.z * ax[2].dot(l).abs();
        let rb = hb.x * bx[0].dot(l).abs() + hb.y * bx[1].dot(l).abs() + hb.z * bx[2].dot(l).abs();
        let dist = d.dot(l);
        let overlap = ra + rb - dist.abs();
        if overlap <= 0.0 {
            return false;
        }
        // leve preferência pelos eixos das faces (contatos mais estáveis)
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

/// Retorna (normal apontando de B para A, profundidade) se houver sobreposição.
fn overlap(a: &Body, b: &Body) -> Option<(Vec3, f32)> {
    match (a.col, b.col) {
        (Col::Box { half: ha }, Col::Box { half: hb }) => {
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
        (Col::Sphere { r }, Col::Box { half }) => sphere_box(a.pos, r, b, half),
        (Col::Box { half }, Col::Sphere { r }) => sphere_box(b.pos, r, a, half).map(|(n, p)| (-n, p)),
    }
}

/// Normal apontando da caixa para a esfera.
fn sphere_box(sp: Vec3, r: f32, bx: &Body, half: Vec3) -> Option<(Vec3, f32)> {
    // trabalha no espaço local da caixa
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
        // centro da esfera dentro da caixa: sai pelo lado mais próximo
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

/// Avança a simulação e devolve os pares de entidades que se tocaram.
pub fn step(scene: &mut Scene, dt: f32) -> Vec<(u64, u64)> {
    let mut contacts: Vec<(u64, u64)> = vec![];
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
        // integração
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
                Some(_) => Vec3::ZERO, // aplicado abaixo, convertido para o espaço do pai
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

        // colisões
        let mut bodies = gather(scene);
        for i in 0..bodies.len() {
            for j in (i + 1)..bodies.len() {
                if !bodies[i].dynamic && !bodies[j].dynamic {
                    continue;
                }
                let Some((n, depth)) = overlap(&bodies[i], &bodies[j]) else { continue };
                let pair = (bodies[i].id, bodies[j].id);
                if !contacts.contains(&pair) {
                    contacts.push(pair);
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

                for (idx, w, normal) in [(ii, wi, n), (ij, wj, -n)] {
                    if w == 0.0 {
                        continue;
                    }
                    let Some(p) = scene.entities[idx].physics.as_mut() else { continue };
                    if rel < 0.0 {
                        // remove a velocidade de aproximação (com quique opcional)
                        let vn = p.velocity.dot(normal);
                        let restitution = if vn.abs() > 1.0 { bounce } else { 0.0 };
                        if vn < 0.0 {
                            p.velocity -= normal * vn * (1.0 + restitution);
                        }
                    }
                    if normal.y > 0.6 {
                        p.grounded = true;
                        // atrito no chão
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
    fn cubo_cai_e_para_no_chao() {
        let mut s = Scene::default_scene();
        let cube = s.find_by_name("Cubo").unwrap();
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
    fn esfera_para_sobre_caixa() {
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
    fn cubo_desliza_e_para_em_rampa_inclinada() {
        // rampa girada 30 graus: o cubo precisa parar sobre a face inclinada, não atravessar
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
        // a face de cima da rampa passa por y = x·tan(30°) + 0,577; o cubo tem de estar acima dela
        let p = s.get(cube).unwrap().position;
        let surface = p.x * 30f32.to_radians().tan() + 0.577;
        assert!(p.y > surface + 0.3 && p.y < surface + 1.2, "pos = {p:?}, superfície = {surface}");
    }

    #[test]
    fn filho_de_pai_transladado_colide_no_mundo() {
        let mut s = Scene::default_scene();
        let parent = s.add_empty("Pai");
        s.get_mut(parent).unwrap().position = Vec3::new(4.0, 10.0, 0.0);
        let id = s.add_mesh(Shape::Sphere);
        let e = s.get_mut(id).unwrap();
        e.parent = Some(parent);
        e.position = Vec3::new(0.0, -6.0, 0.0); // mundo: y = 4
        e.physics = Some(Physics::new(BodyKind::Dynamic, ColliderShape::Sphere));
        for _ in 0..300 {
            step(&mut s, 1.0 / 60.0);
        }
        let world_y = s.world_matrix(id).w_axis.y;
        assert!((world_y - 0.5).abs() < 0.03, "y = {world_y}");
    }
}
