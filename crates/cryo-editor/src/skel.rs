//! Skeletal animation for imported glTF models: joints, skin weights and animation clips.
//!
//! Vertices are skinned on the CPU, so the renderer draws an animated model like any other.

use glam::{Mat3, Mat4, Quat, Vec3};

pub type Trs = (Vec3, Quat, Vec3);

#[derive(Clone, Copy, PartialEq)]
pub enum Target {
    Translation,
    Rotation,
    Scale,
}

pub struct Channel {
    pub node: usize,
    pub target: Target,
    pub times: Vec<f32>,
    /// xyz for translation and scale, xyzw for rotation.
    pub values: Vec<[f32; 4]>,
    pub step: bool,
}

impl Channel {
    fn sample(&self, t: f32) -> [f32; 4] {
        let n = self.times.len().min(self.values.len());
        if n == 0 {
            return [0.0, 0.0, 0.0, 1.0];
        }
        if t <= self.times[0] {
            return self.values[0];
        }
        if t >= self.times[n - 1] {
            return self.values[n - 1];
        }
        let i = self.times[..n].partition_point(|x| *x <= t).clamp(1, n - 1);
        let (t0, t1) = (self.times[i - 1], self.times[i]);
        let (a, b) = (self.values[i - 1], self.values[i]);
        if self.step || t1 <= t0 {
            return a;
        }
        let k = (t - t0) / (t1 - t0);
        if self.target == Target::Rotation {
            let q = Quat::from_array(a).normalize().slerp(Quat::from_array(b).normalize(), k);
            q.to_array()
        } else {
            [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, 0.0]
        }
    }
}

pub struct SkClip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<Channel>,
}

pub struct Skin {
    /// Node index of each joint.
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<Mat4>,
}

/// The node tree of a glTF file with its skins and animation clips.
pub struct Skeleton {
    /// Rest pose of every node.
    pub rest: Vec<Trs>,
    pub parents: Vec<Option<usize>>,
    /// Node indices with parents always before their children.
    pub order: Vec<usize>,
    pub skins: Vec<Skin>,
    pub clips: Vec<SkClip>,
}

impl Skeleton {
    pub fn clip_index(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|c| c.name == name)
    }

    /// Local transform of every node at time `t` of a clip (`None` gives the rest pose).
    pub fn locals(&self, clip: Option<usize>, t: f32) -> Vec<Trs> {
        let mut out = self.rest.clone();
        if let Some(clip) = clip.and_then(|c| self.clips.get(c)) {
            for ch in &clip.channels {
                let Some(node) = out.get_mut(ch.node) else { continue };
                let v = ch.sample(t);
                match ch.target {
                    Target::Translation => node.0 = Vec3::new(v[0], v[1], v[2]),
                    Target::Rotation => node.1 = Quat::from_array(v).normalize(),
                    Target::Scale => node.2 = Vec3::new(v[0], v[1], v[2]),
                }
            }
        }
        out
    }

    /// Mixes two poses: `w` = 0 gives `a`, 1 gives `b`.
    pub fn blend(a: &[Trs], b: &[Trs], w: f32) -> Vec<Trs> {
        a.iter().zip(b).map(|(a, b)| (a.0.lerp(b.0, w), a.1.slerp(b.1, w), a.2.lerp(b.2, w))).collect()
    }

    /// Model-space matrix of every node.
    pub fn globals(&self, locals: &[Trs]) -> Vec<Mat4> {
        let mut out = vec![Mat4::IDENTITY; locals.len()];
        for &i in &self.order {
            let (t, r, s) = locals[i];
            let local = Mat4::from_scale_rotation_translation(s, r, t);
            out[i] = match self.parents[i] {
                Some(p) => out[p] * local,
                None => local,
            };
        }
        out
    }

    /// The matrices that move each vertex from the bind pose to the given pose, per skin.
    pub fn skin_matrices(&self, globals: &[Mat4]) -> Vec<Vec<Mat4>> {
        self.skins
            .iter()
            .map(|s| {
                s.joints
                    .iter()
                    .zip(&s.inverse_bind)
                    .map(|(j, ibm)| globals.get(*j).copied().unwrap_or(Mat4::IDENTITY) * *ibm)
                    .collect()
            })
            .collect()
    }

    /// Time inside the clip for a clock that keeps running: wraps when looping, stops at the end otherwise.
    pub fn clip_time(&self, clip: Option<usize>, t: f32, looping: bool) -> f32 {
        let d = clip.and_then(|c| self.clips.get(c)).map(|c| c.duration).unwrap_or(0.0);
        if d <= 0.0 {
            0.0
        } else if looping {
            t.rem_euclid(d)
        } else {
            t.clamp(0.0, d)
        }
    }
}

/// Skin data of one sub-mesh, one entry per vertex.
pub struct SkinData {
    pub skin: usize,
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
}

impl SkinData {
    /// Writes the skinned positions and normals into interleaved vertices (8 floats each).
    pub fn apply(&self, mats: &[Vec<Mat4>], vertices: &mut [f32]) {
        let Some(mats) = mats.get(self.skin) else { return };
        for (i, v) in vertices.chunks_exact_mut(8).enumerate().take(self.positions.len()) {
            let (j, w) = (self.joints[i], self.weights[i]);
            let mut m = Mat4::ZERO;
            let mut total = 0.0;
            for k in 0..4 {
                if w[k] > 0.0 {
                    if let Some(jm) = mats.get(j[k] as usize) {
                        m += *jm * w[k];
                        total += w[k];
                    }
                }
            }
            if total < 1e-6 {
                m = Mat4::IDENTITY;
            } else if (total - 1.0).abs() > 1e-4 {
                m *= 1.0 / total;
            }
            let p = m.transform_point3(self.positions[i]);
            let n = (Mat3::from_mat4(m) * self.normals[i]).normalize_or_zero();
            v[0..3].copy_from_slice(&p.to_array());
            v[3..6].copy_from_slice(&n.to_array());
        }
    }
}

// ----------------------------------------------------------------------
// A small rigged model made in code: the example for skeletal animation
// ----------------------------------------------------------------------

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// A glTF file (text, self-contained) with a bendy stalk: 5 joints, smooth skin weights
/// and three clips: "Wave", "Curl" and "Nod".
pub fn demo_rig_gltf() -> String {
    const JOINTS: usize = 5;
    const RINGS: usize = 17;
    const SIDES: usize = 10;
    const HEIGHT: f32 = 3.0;
    let seg = HEIGHT / JOINTS as f32;

    let mut pos: Vec<[f32; 3]> = vec![];
    let mut nor: Vec<[f32; 3]> = vec![];
    let mut jnt: Vec<[u16; 4]> = vec![];
    let mut wgt: Vec<[f32; 4]> = vec![];
    let mut idx: Vec<u16> = vec![];
    for r in 0..RINGS {
        let y = HEIGHT * r as f32 / (RINGS - 1) as f32;
        // thinner towards the tip, with a bulb at the end
        let k = r as f32 / (RINGS - 1) as f32;
        let radius = 0.32 - 0.2 * k + if k > 0.86 { 0.22 * ((k - 0.86) / 0.14 * std::f32::consts::PI).sin() } else { 0.0 };
        // each ring follows the joint below it, easing into the next one
        let f = (y / seg - 0.5).clamp(0.0, (JOINTS - 1) as f32);
        let j0 = f.floor() as usize;
        let j1 = (j0 + 1).min(JOINTS - 1);
        let w1 = if j1 == j0 { 0.0 } else { f - j0 as f32 };
        for s in 0..SIDES {
            let a = s as f32 / SIDES as f32 * std::f32::consts::TAU;
            pos.push([a.cos() * radius, y, a.sin() * radius]);
            nor.push([a.cos(), 0.0, a.sin()]);
            jnt.push([j0 as u16, j1 as u16, 0, 0]);
            wgt.push([1.0 - w1, w1, 0.0, 0.0]);
        }
    }
    for r in 0..RINGS - 1 {
        for s in 0..SIDES {
            let a = (r * SIDES + s) as u16;
            let b = (r * SIDES + (s + 1) % SIDES) as u16;
            let c = a + SIDES as u16;
            let d = b + SIDES as u16;
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    // caps
    for (ring, y, up) in [(0usize, 0.0f32, false), (RINGS - 1, HEIGHT, true)] {
        let centre = pos.len() as u16;
        pos.push([0.0, y, 0.0]);
        nor.push([0.0, if up { 1.0 } else { -1.0 }, 0.0]);
        let j = if up { (JOINTS - 1) as u16 } else { 0 };
        jnt.push([j, 0, 0, 0]);
        wgt.push([1.0, 0.0, 0.0, 0.0]);
        for s in 0..SIDES {
            let a = (ring * SIDES + s) as u16;
            let b = (ring * SIDES + (s + 1) % SIDES) as u16;
            idx.extend_from_slice(&if up { [centre, b, a] } else { [centre, a, b] });
        }
    }

    // animation: rotations of every joint at 9 instants of a 2-second loop
    const FRAMES: usize = 9;
    let times: Vec<f32> = (0..FRAMES).map(|i| i as f32 * 0.25).collect();
    let clip = |f: &dyn Fn(usize, f32) -> Quat| -> Vec<[f32; 4]> {
        let mut out = vec![];
        for j in 0..JOINTS {
            for t in &times {
                out.push(f(j, *t / 2.0).to_array());
            }
        }
        out
    };
    let tau = std::f32::consts::TAU;
    let wave = clip(&|j, k| Quat::from_rotation_z(0.35 * (k * tau - j as f32 * 0.9).sin()));
    let curl = clip(&|j, k| Quat::from_rotation_x(if j == 0 { 0.0 } else { 0.55 * (0.5 - 0.5 * (k * tau).cos()) }));
    let nod = clip(&|j, k| if j == JOINTS - 1 { Quat::from_rotation_x(0.7 * (k * tau * 2.0).sin()) } else { Quat::IDENTITY });

    // one binary buffer; every block is a multiple of 4 bytes
    let mut bin: Vec<u8> = vec![];
    let mut views: Vec<(usize, usize)> = vec![];
    let mut block = |bytes: Vec<u8>, bin: &mut Vec<u8>| {
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        views.push((bin.len(), bytes.len()));
        bin.extend(bytes);
        views.len() - 1
    };
    let f32s = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    let v_pos = block(f32s(&pos.concat()), &mut bin);
    let v_nor = block(f32s(&nor.concat()), &mut bin);
    let v_jnt = block(jnt.concat().iter().flat_map(|x| x.to_le_bytes()).collect(), &mut bin);
    let v_wgt = block(f32s(&wgt.concat()), &mut bin);
    let v_idx = block(idx.iter().flat_map(|x| x.to_le_bytes()).collect(), &mut bin);
    let ibm: Vec<f32> =
        (0..JOINTS).flat_map(|j| Mat4::from_translation(Vec3::new(0.0, -(j as f32) * seg, 0.0)).to_cols_array()).collect();
    let v_ibm = block(f32s(&ibm), &mut bin);
    let v_time = block(f32s(&times), &mut bin);
    let v_clips: Vec<usize> = [&wave, &curl, &nod].iter().map(|c| block(f32s(&c.concat()), &mut bin)).collect();

    let n = pos.len();
    let max_r = pos.iter().fold(0.0f32, |m, p| m.max(p[0].abs()).max(p[2].abs()));
    let mut accessors = vec![
        serde_json::json!({"bufferView": v_pos, "componentType": 5126, "count": n, "type": "VEC3",
            "min": [-max_r, 0.0, -max_r], "max": [max_r, HEIGHT, max_r]}),
        serde_json::json!({"bufferView": v_nor, "componentType": 5126, "count": n, "type": "VEC3"}),
        serde_json::json!({"bufferView": v_jnt, "componentType": 5123, "count": n, "type": "VEC4"}),
        serde_json::json!({"bufferView": v_wgt, "componentType": 5126, "count": n, "type": "VEC4"}),
        serde_json::json!({"bufferView": v_idx, "componentType": 5123, "count": idx.len(), "type": "SCALAR"}),
        serde_json::json!({"bufferView": v_ibm, "componentType": 5126, "count": JOINTS, "type": "MAT4"}),
        serde_json::json!({"bufferView": v_time, "componentType": 5126, "count": FRAMES, "type": "SCALAR",
            "min": [0.0], "max": [2.0]}),
    ];
    let mut animations = vec![];
    for (ci, name) in ["Wave", "Curl", "Nod"].iter().enumerate() {
        let mut samplers = vec![];
        let mut channels = vec![];
        for j in 0..JOINTS {
            accessors.push(serde_json::json!({"bufferView": v_clips[ci], "byteOffset": j * FRAMES * 16,
                "componentType": 5126, "count": FRAMES, "type": "VEC4"}));
            samplers.push(serde_json::json!({"input": 6, "output": accessors.len() - 1, "interpolation": "LINEAR"}));
            channels.push(serde_json::json!({"sampler": j, "target": {"node": j + 1, "path": "rotation"}}));
        }
        animations.push(serde_json::json!({"name": name, "samplers": samplers, "channels": channels}));
    }
    // node 0 holds the mesh; nodes 1.. are the joints, each the child of the one before
    let mut nodes = vec![serde_json::json!({"name": "Stalk", "mesh": 0, "skin": 0})];
    for j in 0..JOINTS {
        let mut node = serde_json::json!({"name": format!("Joint{j}"), "translation": [0.0, if j == 0 { 0.0 } else { seg }, 0.0]});
        if j + 1 < JOINTS {
            node["children"] = serde_json::json!([j + 2]);
        }
        nodes.push(node);
    }
    let doc = serde_json::json!({
        "asset": {"version": "2.0", "generator": "Cryo Engine"},
        "scene": 0,
        "scenes": [{"nodes": [0, 1]}],
        "nodes": nodes,
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": 0, "NORMAL": 1, "JOINTS_0": 2, "WEIGHTS_0": 3},
            "indices": 4, "material": 0
        }]}],
        "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [0.35, 0.8, 0.55, 1.0]}}],
        "skins": [{"joints": (1..=JOINTS).collect::<Vec<usize>>(), "inverseBindMatrices": 5}],
        "animations": animations,
        "accessors": accessors,
        "bufferViews": views.iter().map(|(o, l)| serde_json::json!({"buffer": 0, "byteOffset": o, "byteLength": l})).collect::<Vec<_>>(),
        "buffers": [{"byteLength": bin.len(), "uri": format!("data:application/octet-stream;base64,{}", base64(&bin))}],
    });
    serde_json::to_string(&doc).unwrap()
}
