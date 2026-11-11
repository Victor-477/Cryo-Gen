//! Imported assets: 3D models (.obj, .gltf, .glb) and textures (.png, .jpg).

use glam::{Mat3, Mat4, Vec2, Vec3};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use crate::skel::{Channel, SkClip, Skeleton, Skin, SkinData, Target};
use std::sync::Arc;

pub const MODEL_EXTS: &[&str] = &["obj", "gltf", "glb"];
pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg"];
pub const SOUND_EXTS: &[&str] = &["wav", "ogg", "mp3"];

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn ext_of(path: &str) -> String {
    Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}

pub struct TextureData {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Part of a model with a single material.
pub struct SubMesh {
    /// Interleaved: position (3), normal (3), uv (2).
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    pub color: [f32; 3],
    pub texture: Option<Arc<TextureData>>,
    /// Joints and weights, when the sub-mesh follows a skeleton.
    pub skin: Option<Arc<SkinData>>,
}

pub struct ModelData {
    pub id: u64,
    /// Changes when the vertices change (animated models keep their id).
    pub version: u64,
    /// Joints and animation clips of a rigged glTF model.
    pub skeleton: Option<Arc<Skeleton>>,
    pub subs: Vec<SubMesh>,
    pub center: Vec3,
    pub half: Vec3,
    pub triangles: usize,
}

/// Cache of assets loaded from disk (CPU side; the GPU side is filled by the renderer).
pub struct Assets {
    pub project_dir: PathBuf,
    models: HashMap<String, Option<Arc<ModelData>>>,
    textures: HashMap<String, Option<Arc<TextureData>>>,
    /// Meshes being edited in the engine: shown instead of the file on disk.
    pub overrides: HashMap<String, Arc<ModelData>>,
    /// Loading errors not yet shown in the Console.
    pub errors: Vec<String>,
    /// Animated models in their current pose, per entity.
    posed: HashMap<u64, (PoseKey, Arc<ModelData>)>,
}

type PoseKey = (u64, Option<usize>, u32, Option<usize>, u32, u32);

impl Assets {
    pub fn new(project_dir: PathBuf) -> Self {
        Assets { project_dir, models: HashMap::new(), textures: HashMap::new(), overrides: HashMap::new(), errors: vec![], posed: HashMap::new() }
    }

    pub fn model(&mut self, rel: &str) -> Option<Arc<ModelData>> {
        if let Some(m) = self.overrides.get(rel) {
            return Some(m.clone());
        }
        if let Some(m) = self.models.get(rel) {
            return m.clone();
        }
        let loaded = match load_model(&self.project_dir.join(rel)) {
            Ok(m) => Some(Arc::new(m)),
            Err(e) => {
                self.errors.push(format!("{rel}: {e}"));
                None
            }
        };
        self.models.insert(rel.to_string(), loaded.clone());
        loaded
    }

    pub fn texture(&mut self, rel: &str) -> Option<Arc<TextureData>> {
        if let Some(t) = self.textures.get(rel) {
            return t.clone();
        }
        let loaded = match load_texture(&self.project_dir.join(rel)) {
            Ok(t) => Some(Arc::new(t)),
            Err(e) => {
                self.errors.push(format!("{rel}: {e}"));
                None
            }
        };
        self.textures.insert(rel.to_string(), loaded.clone());
        loaded
    }

    /// Names of the animation clips inside a model (empty when it has no skeleton).
    pub fn clip_names(&mut self, rel: &str) -> Vec<String> {
        self.model(rel)
            .and_then(|m| m.skeleton.as_ref().map(|s| s.clips.iter().map(|c| c.name.clone()).collect()))
            .unwrap_or_default()
    }

    /// The model in the pose asked by the entity's mesh component (skeletal animation).
    /// Models without a skeleton are returned as they are.
    pub fn posed(&mut self, entity: u64, base: &Arc<ModelData>, m: &crate::scene::MeshRenderer) -> Arc<ModelData> {
        let Some(sk) = &base.skeleton else { return base.clone() };
        let clip = sk.clip_index(&m.animation);
        let t = sk.clip_time(clip, m.anim.time, m.anim_loop);
        let fade = m.anim.fade.clamp(0.0, 1.0);
        let prev = if fade > 0.0 { sk.clip_index(&m.anim.prev) } else { None };
        let prev_t = sk.clip_time(prev, m.anim.prev_time, true);
        let key: PoseKey = (base.id, clip, t.to_bits(), prev, prev_t.to_bits(), fade.to_bits());
        let (id, version) = match self.posed.get(&entity) {
            Some((k, model)) if *k == key => return model.clone(),
            Some((_, model)) => (model.id, model.version + 1),
            None => (next_id(), 0),
        };
        let mut locals = sk.locals(clip, t);
        if fade > 0.0 {
            // cross-fade from the pose the model was in when the clip changed
            locals = Skeleton::blend(&locals, &sk.locals(prev, prev_t), fade);
        }
        let mats = sk.skin_matrices(&sk.globals(&locals));
        let subs = base
            .subs
            .iter()
            .map(|s| {
                let mut vertices = s.vertices.clone();
                if let Some(skin) = &s.skin {
                    skin.apply(&mats, &mut vertices);
                }
                SubMesh {
                    vertices,
                    indices: s.indices.clone(),
                    color: s.color,
                    texture: s.texture.clone(),
                    skin: s.skin.clone(),
                }
            })
            .collect();
        let model = Arc::new(ModelData {
            id,
            version,
            skeleton: base.skeleton.clone(),
            subs,
            center: base.center,
            half: base.half,
            triangles: base.triangles,
        });
        if self.posed.len() > 512 {
            self.posed.clear();
        }
        self.posed.insert(entity, (key, model.clone()));
        model
    }

    /// Forgets an asset so it is read from disk again (after re-importing).
    pub fn invalidate(&mut self, rel: &str) {
        self.models.remove(rel);
        self.textures.remove(rel);
    }
}

pub fn load_texture(path: &Path) -> Result<TextureData, String> {
    let img = image::open(path).map_err(|e| format!("invalid image ({e})"))?;
    let img = if img.width().max(img.height()) > 4096 {
        img.resize(4096, 4096, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    Ok(TextureData { id: next_id(), width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
}

pub fn load_model(path: &Path) -> Result<ModelData, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let (subs, skeleton) = match ext.as_str() {
        "obj" => (load_obj(path)?, None),
        "gltf" | "glb" => load_gltf(path)?,
        other => return Err(format!("format `.{other}` is not supported (use .obj, .gltf or .glb)")),
    };
    let mut model = finish(subs)?;
    if model.subs.iter().any(|s| s.skin.is_some()) {
        model.skeleton = skeleton.map(Arc::new);
    }
    Ok(model)
}

/// Reads the node tree, the skins and the animation clips of a glTF document.
fn load_skeleton(doc: &gltf::Document, buffers: &[gltf::buffer::Data]) -> Option<Skeleton> {
    if doc.skins().count() == 0 {
        return None;
    }
    let count = doc.nodes().count();
    let mut rest = vec![(Vec3::ZERO, glam::Quat::IDENTITY, Vec3::ONE); count];
    let mut parents = vec![None; count];
    for node in doc.nodes() {
        let (t, r, s) = node.transform().decomposed();
        rest[node.index()] = (Vec3::from_array(t), glam::Quat::from_array(r).normalize(), Vec3::from_array(s));
        for child in node.children() {
            parents[child.index()] = Some(node.index());
        }
    }
    // parents before children; a malformed file with a cycle simply leaves nodes out
    let mut order = vec![];
    let mut placed = vec![false; count];
    for _ in 0..count {
        let before = order.len();
        for i in 0..count {
            if !placed[i] && parents[i].map(|p| placed[p]).unwrap_or(true) {
                placed[i] = true;
                order.push(i);
            }
        }
        if order.len() == before {
            break;
        }
    }
    let skins = doc
        .skins()
        .map(|skin| {
            let joints: Vec<usize> = skin.joints().map(|j| j.index()).collect();
            let reader = skin.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let mut inverse_bind: Vec<Mat4> = reader
                .read_inverse_bind_matrices()
                .map(|it| it.map(|m| Mat4::from_cols_array_2d(&m)).collect())
                .unwrap_or_default();
            inverse_bind.resize(joints.len(), Mat4::IDENTITY);
            Skin { joints, inverse_bind }
        })
        .collect();
    let mut clips = vec![];
    for (i, anim) in doc.animations().enumerate() {
        let mut channels = vec![];
        let mut duration: f32 = 0.0;
        for ch in anim.channels() {
            use gltf::animation::util::ReadOutputs;
            let reader = ch.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let Some(times) = reader.read_inputs() else { continue };
            let times: Vec<f32> = times.collect();
            let (target, mut values): (Target, Vec<[f32; 4]>) = match reader.read_outputs() {
                Some(ReadOutputs::Translations(it)) => (Target::Translation, it.map(|v| [v[0], v[1], v[2], 0.0]).collect()),
                Some(ReadOutputs::Scales(it)) => (Target::Scale, it.map(|v| [v[0], v[1], v[2], 0.0]).collect()),
                Some(ReadOutputs::Rotations(it)) => (Target::Rotation, it.into_f32().collect()),
                _ => continue,
            };
            let interpolation = ch.sampler().interpolation();
            if interpolation == gltf::animation::Interpolation::CubicSpline {
                // (in-tangent, value, out-tangent) per key: keep the values
                values = values.chunks_exact(3).map(|c| c[1]).collect();
            }
            if times.is_empty() || values.len() < times.len() || times.iter().any(|t| !t.is_finite()) {
                continue;
            }
            duration = duration.max(*times.last().unwrap());
            channels.push(Channel {
                node: ch.target().node().index(),
                target,
                times,
                values,
                step: interpolation == gltf::animation::Interpolation::Step,
            });
        }
        if !channels.is_empty() {
            let name = anim.name().map(|n| n.to_string()).unwrap_or_else(|| format!("Animation {}", i + 1));
            clips.push(SkClip { name, duration, channels });
        }
    }
    Some(Skeleton { rest, parents, order, skins, clips })
}

fn finish(subs: Vec<SubMesh>) -> Result<ModelData, String> {
    let subs: Vec<SubMesh> = subs.into_iter().filter(|s| !s.indices.is_empty()).collect();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut triangles = 0;
    for s in &subs {
        triangles += s.indices.len() / 3;
        for v in s.vertices.chunks_exact(8) {
            let p = Vec3::new(v[0], v[1], v[2]);
            if !p.is_finite() {
                return Err("the model contains invalid coordinates".into());
            }
            min = min.min(p);
            max = max.max(p);
        }
    }
    if triangles == 0 {
        return Err("the model contains no triangles".into());
    }
    Ok(ModelData {
        id: next_id(),
        version: 0,
        skeleton: None,
        subs,
        center: (min + max) * 0.5,
        half: ((max - min) * 0.5).max(Vec3::splat(0.001)),
        triangles,
    })
}

fn push_vertex(out: &mut Vec<f32>, p: Vec3, n: Vec3, uv: Vec2) {
    out.extend_from_slice(&[p.x, p.y, p.z, n.x, n.y, n.z, uv.x, uv.y]);
}

// ---------------------------------------------------------------------------
// Wavefront OBJ (+ MTL)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Material {
    color: [f32; 3],
    texture: Option<String>,
}

fn load_mtl(path: &Path) -> HashMap<String, Material> {
    let mut out = HashMap::new();
    let Ok(src) = std::fs::read_to_string(path) else { return out };
    let mut current: Option<String> = None;
    for line in src.lines() {
        let line = line.trim();
        let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        match key {
            "newmtl" => {
                current = Some(rest.to_string());
                out.insert(rest.to_string(), Material { color: [0.8; 3], texture: None });
            }
            "Kd" => {
                let v: Vec<f32> = rest.split_whitespace().filter_map(|t| t.parse().ok()).collect();
                if let (Some(name), [r, g, b, ..]) = (&current, v.as_slice()) {
                    if let Some(m) = out.get_mut(name) {
                        m.color = [*r, *g, *b];
                    }
                }
            }
            "map_Kd" => {
                // the file name is the last item (options come first)
                if let (Some(name), Some(file)) = (&current, rest.split_whitespace().last()) {
                    if let Some(m) = out.get_mut(name) {
                        m.texture = Some(file.replace('\\', "/"));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn load_obj(path: &Path) -> Result<Vec<SubMesh>, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("could not read ({e})"))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut pos: Vec<Vec3> = vec![];
    let mut nor: Vec<Vec3> = vec![];
    let mut uvs: Vec<Vec2> = vec![];
    let mut materials: HashMap<String, Material> = HashMap::new();

    struct Group {
        material: String,
        vertices: Vec<f32>,
        indices: Vec<u32>,
        lookup: HashMap<(usize, usize, usize), u32>,
    }
    let mut groups: Vec<Group> = vec![];
    let mut current = 0usize;
    let new_group = |name: &str| Group {
        material: name.to_string(),
        vertices: vec![],
        indices: vec![],
        lookup: HashMap::new(),
    };
    groups.push(new_group(""));

    let resolve = |idx: i64, len: usize| -> Option<usize> {
        if idx > 0 && (idx as usize) <= len {
            Some(idx as usize - 1)
        } else if idx < 0 && (-idx) as usize <= len {
            Some(len - (-idx) as usize)
        } else {
            None
        }
    };

    for (ln, line) in src.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        let key = it.next().unwrap_or("");
        let nums = |it: std::str::SplitWhitespace| -> Vec<f32> { it.filter_map(|t| t.parse().ok()).collect() };
        match key {
            "v" => {
                let v = nums(it);
                if v.len() < 3 {
                    return Err(format!("line {}: incomplete vertex", ln + 1));
                }
                pos.push(Vec3::new(v[0], v[1], v[2]));
            }
            "vn" => {
                let v = nums(it);
                if v.len() >= 3 {
                    nor.push(Vec3::new(v[0], v[1], v[2]));
                }
            }
            "vt" => {
                let v = nums(it);
                if v.len() >= 2 {
                    uvs.push(Vec2::new(v[0], 1.0 - v[1]));
                } else if v.len() == 1 {
                    uvs.push(Vec2::new(v[0], 0.0));
                }
            }
            "mtllib" => {
                let name = line[6..].trim().replace('\\', "/");
                materials.extend(load_mtl(&dir.join(name)));
            }
            "usemtl" => {
                let name = line[6..].trim();
                current = match groups.iter().position(|g| g.material == name) {
                    Some(i) => i,
                    None => {
                        groups.push(new_group(name));
                        groups.len() - 1
                    }
                };
            }
            "f" => {
                let mut corners: Vec<(usize, Option<usize>, Option<usize>)> = vec![];
                for tok in it {
                    let mut parts = tok.split('/');
                    let p = parts.next().and_then(|t| t.parse::<i64>().ok()).and_then(|i| resolve(i, pos.len()));
                    let t = parts.next().and_then(|t| t.parse::<i64>().ok()).and_then(|i| resolve(i, uvs.len()));
                    let n = parts.next().and_then(|t| t.parse::<i64>().ok()).and_then(|i| resolve(i, nor.len()));
                    match p {
                        Some(p) => corners.push((p, t, n)),
                        None => return Err(format!("line {}: invalid vertex index", ln + 1)),
                    }
                }
                if corners.len() < 3 {
                    continue;
                }
                let face_n = (pos[corners[1].0] - pos[corners[0].0])
                    .cross(pos[corners[2].0] - pos[corners[0].0])
                    .normalize_or_zero();
                let g = &mut groups[current];
                let mut ids = Vec::with_capacity(corners.len());
                for (p, t, n) in &corners {
                    let id = match n {
                        // with its own normal: identical vertices are shared
                        Some(n) => {
                            let key = (*p, t.map(|t| t + 1).unwrap_or(0), *n + 1);
                            match g.lookup.get(&key) {
                                Some(id) => *id,
                                None => {
                                    let id = (g.vertices.len() / 8) as u32;
                                    let uv = t.map(|t| uvs[t]).unwrap_or(Vec2::ZERO);
                                    push_vertex(&mut g.vertices, pos[*p], nor[*n].normalize_or_zero(), uv);
                                    g.lookup.insert(key, id);
                                    id
                                }
                            }
                        }
                        // no normal: use the face normal
                        None => {
                            let id = (g.vertices.len() / 8) as u32;
                            let uv = t.map(|t| uvs[t]).unwrap_or(Vec2::ZERO);
                            push_vertex(&mut g.vertices, pos[*p], face_n, uv);
                            id
                        }
                    };
                    ids.push(id);
                }
                for i in 1..ids.len() - 1 {
                    g.indices.extend_from_slice(&[ids[0], ids[i], ids[i + 1]]);
                }
            }
            _ => {}
        }
    }

    let mut tex_cache: HashMap<String, Option<Arc<TextureData>>> = HashMap::new();
    Ok(groups
        .into_iter()
        .map(|g| {
            let mat = materials.get(&g.material);
            let texture = mat.and_then(|m| m.texture.clone()).and_then(|file| {
                tex_cache
                    .entry(file.clone())
                    .or_insert_with(|| load_texture(&dir.join(&file)).ok().map(Arc::new))
                    .clone()
            });
            // with a texture, the diffuse colour is usually just a neutral multiplier
            let color = match (mat, &texture) {
                (Some(_), Some(_)) => [1.0; 3],
                (Some(m), None) => m.color,
                (None, _) => [1.0; 3],
            };
            SubMesh { vertices: g.vertices, indices: g.indices, color, texture, skin: None }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// glTF 2.0 (.gltf / .glb)
// ---------------------------------------------------------------------------

fn load_gltf(path: &Path) -> Result<(Vec<SubMesh>, Option<Skeleton>), String> {
    let (doc, buffers, images) = gltf::import(path).map_err(|e| format!("invalid glTF ({e})"))?;
    let skeleton = load_skeleton(&doc, &buffers);
    let rest_mats: Vec<Vec<Mat4>> =
        skeleton.as_ref().map(|s| s.skin_matrices(&s.globals(&s.rest))).unwrap_or_default();
    let mut textures: HashMap<usize, Option<Arc<TextureData>>> = HashMap::new();
    let mut subs = vec![];

    let mut texture_for = |index: usize| -> Option<Arc<TextureData>> {
        textures
            .entry(index)
            .or_insert_with(|| {
                let img = images.get(index)?;
                use gltf::image::Format;
                let rgba: Vec<u8> = match img.format {
                    Format::R8G8B8A8 => img.pixels.clone(),
                    Format::R8G8B8 => img.pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
                    Format::R8 => img.pixels.iter().flat_map(|p| [*p, *p, *p, 255]).collect(),
                    Format::R8G8 => img.pixels.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
                    _ => return None,
                };
                Some(Arc::new(TextureData { id: next_id(), width: img.width, height: img.height, rgba }))
            })
            .clone()
    };

    // walks the node hierarchy accumulating transforms
    let mut stack: Vec<(gltf::Node, Mat4)> = vec![];
    let scenes: Vec<gltf::Scene> = match doc.default_scene() {
        Some(s) => vec![s],
        None => doc.scenes().collect(),
    };
    for scene in scenes {
        for node in scene.nodes() {
            stack.push((node, Mat4::IDENTITY));
        }
    }
    let mut guard = 0;
    while let Some((node, parent)) = stack.pop() {
        guard += 1;
        if guard > 100_000 {
            break;
        }
        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        for child in node.children() {
            stack.push((child, world));
        }
        let Some(mesh) = node.mesh() else { continue };
        let normal_mat = Mat3::from_mat4(world).inverse().transpose();
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let Some(positions) = reader.read_positions() else { continue };
            let positions: Vec<Vec3> = positions.map(|p| world.transform_point3(Vec3::from_array(p))).collect();
            let normals: Option<Vec<Vec3>> = reader
                .read_normals()
                .map(|it| it.map(|n| (normal_mat * Vec3::from_array(n)).normalize_or_zero()).collect());
            let uvs: Option<Vec<Vec2>> =
                reader.read_tex_coords(0).map(|t| t.into_f32().map(Vec2::from_array).collect());
            let indices: Vec<u32> = match reader.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };
            if indices.iter().any(|i| *i as usize >= positions.len()) {
                return Err("indices out of range in the model".into());
            }

            let pbr = prim.material().pbr_metallic_roughness();
            let c = pbr.base_color_factor();
            let texture = pbr.base_color_texture().and_then(|t| texture_for(t.texture().source().index()));

            // a skinned primitive follows its joints, not its own node
            if let (Some(skin), Some(joints), Some(weights)) =
                (node.skin(), reader.read_joints(0), reader.read_weights(0))
            {
                let raw: Vec<Vec3> = reader.read_positions().unwrap().map(Vec3::from_array).collect();
                let joints: Vec<[u16; 4]> = joints.into_u16().collect();
                let weights: Vec<[f32; 4]> = weights.into_f32().collect();
                if joints.len() == raw.len() && weights.len() == raw.len() && skin.index() < rest_mats.len() {
                    let mut raw_normals: Vec<Vec3> =
                        reader.read_normals().map(|it| it.map(Vec3::from_array).collect()).unwrap_or_default();
                    if raw_normals.len() != raw.len() {
                        raw_normals = vec![Vec3::ZERO; raw.len()];
                        for tri in indices.chunks_exact(3) {
                            let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                            let n = (raw[b] - raw[a]).cross(raw[c] - raw[a]);
                            for i in [a, b, c] {
                                raw_normals[i] += n;
                            }
                        }
                        for n in &mut raw_normals {
                            *n = n.normalize_or_zero();
                        }
                    }
                    let mut vertices = Vec::with_capacity(raw.len() * 8);
                    for (i, p) in raw.iter().enumerate() {
                        let uv = uvs.as_ref().and_then(|u| u.get(i)).copied().unwrap_or(Vec2::ZERO);
                        push_vertex(&mut vertices, *p, raw_normals[i], uv);
                    }
                    let data = SkinData { skin: skin.index(), positions: raw, normals: raw_normals, joints, weights };
                    data.apply(&rest_mats, &mut vertices);
                    subs.push(SubMesh {
                        vertices,
                        indices,
                        color: [c[0], c[1], c[2]],
                        texture,
                        skin: Some(Arc::new(data)),
                    });
                    continue;
                }
            }

            let mut vertices = Vec::with_capacity(indices.len() * 8);
            let mut out_idx = Vec::with_capacity(indices.len());
            match &normals {
                Some(normals) if normals.len() == positions.len() => {
                    for (i, p) in positions.iter().enumerate() {
                        let uv = uvs.as_ref().and_then(|u| u.get(i)).copied().unwrap_or(Vec2::ZERO);
                        push_vertex(&mut vertices, *p, normals[i], uv);
                    }
                    out_idx = indices;
                }
                _ => {
                    // no normals: compute one per face
                    for tri in indices.chunks_exact(3) {
                        let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                        let n = (positions[b] - positions[a]).cross(positions[c] - positions[a]).normalize_or_zero();
                        for i in [a, b, c] {
                            let uv = uvs.as_ref().and_then(|u| u.get(i)).copied().unwrap_or(Vec2::ZERO);
                            out_idx.push((vertices.len() / 8) as u32);
                            push_vertex(&mut vertices, positions[i], n, uv);
                        }
                    }
                }
            }
            // negative scale on the node flips triangle winding
            if world.determinant() < 0.0 {
                for tri in out_idx.chunks_exact_mut(3) {
                    tri.swap(1, 2);
                }
            }
            subs.push(SubMesh { vertices, indices: out_idx, color: [c[0], c[1], c[2]], texture, skin: None });
        }
    }
    Ok((subs, skeleton))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cryo_assets_{}_{name}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn obj_with_quad_material_and_negative_indices() {
        let d = tmp("obj");
        std::fs::write(d.join("m.mtl"), "newmtl red\nKd 1 0 0\n").unwrap();
        std::fs::write(
            d.join("q.obj"),
            "mtllib m.mtl\nv 0 0 0\nv 2 0 0\nv 2 0 -4\nv 0 0 -4\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n\
             usemtl red\nf 1/1 2/2 3/3 4/4\nf -4 -3 -2\n",
        )
        .unwrap();
        let m = load_model(&d.join("q.obj")).unwrap();
        assert_eq!(m.triangles, 3);
        assert_eq!(m.subs.len(), 1);
        assert_eq!(m.subs[0].color, [1.0, 0.0, 0.0]);
        assert!((m.center - Vec3::new(1.0, 0.0, -2.0)).length() < 1e-5);
        // the computed face normal points up
        assert!((m.subs[0].vertices[4] - 1.0).abs() < 1e-5);
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn gltf_with_transformed_node() {
        let d = tmp("gltf");
        // one triangle: 3 f32 positions (36 bytes) + 3 u16 indices (6 bytes)
        let mut bin: Vec<u8> = vec![];
        for v in [[0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for c in v {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        for i in [0u16, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        std::fs::write(d.join("t.bin"), &bin).unwrap();
        std::fs::write(
            d.join("t.gltf"),
            r#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],
            "nodes":[{"mesh":0,"translation":[10,0,0],"scale":[2,2,2]}],
            "materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0,1,0,1]}}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]}],
            "buffers":[{"uri":"t.bin","byteLength":42}],
            "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":6}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
                         {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}]}"#,
        )
        .unwrap();
        let m = load_model(&d.join("t.gltf")).unwrap();
        assert_eq!(m.triangles, 1);
        assert_eq!(m.subs[0].color, [0.0, 1.0, 0.0]);
        // the node transform was applied: x goes from 10 to 12
        assert!((m.center.x - 11.0).abs() < 1e-4, "{:?}", m.center);
        assert!((m.subs[0].vertices[5] - 1.0).abs() < 1e-5, "normal +Z");
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn rigged_gltf_bends_with_its_clips() {
        let path = tmp("rig").join("rig.gltf");
        std::fs::write(&path, crate::skel::demo_rig_gltf()).unwrap();
        let base = Arc::new(load_model(&path).unwrap());
        let sk = base.skeleton.clone().expect("the model has a skeleton");
        let names: Vec<&str> = sk.clips.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Wave", "Curl", "Nod"]);
        assert!((sk.clips[0].duration - 2.0).abs() < 1e-5);
        // rest pose: a straight stalk, 3 m tall
        assert!((base.center.y - 1.5).abs() < 1e-3 && (base.half.y - 1.5).abs() < 1e-3);

        let top = |m: &ModelData| {
            m.subs[0].vertices.chunks_exact(8).map(|v| Vec3::new(v[0], v[1], v[2])).fold(Vec3::ZERO, |a, p| if p.y > a.y { p } else { a })
        };
        let mut assets = Assets::new(std::env::temp_dir());
        let mut mesh = crate::scene::MeshRenderer::primitive(crate::scene::Shape::Cube);
        let rest = assets.posed(7, &base, &mesh);
        assert!(top(&rest).z.abs() < 0.2);

        // halfway through "Curl" every joint leans forward, so the tip moves far in +Z and comes down
        mesh.play_animation("Curl", false);
        mesh.anim.time = 1.0;
        let curled = assets.posed(7, &base, &mesh);
        let tip = top(&curled);
        assert!(tip.y < 2.6, "the stalk bent: {tip:?}");
        let far = curled.subs[0].vertices.chunks_exact(8).map(|v| v[2]).fold(0.0f32, f32::max);
        assert!(far > 1.2, "tip reaches forward: {far}");
        assert_eq!(curled.id, rest.id, "the same GPU model is reused");
        assert!(curled.version > rest.version);
        // same pose again: nothing is recomputed
        assert!(Arc::ptr_eq(&curled, &assets.posed(7, &base, &mesh)));
        // the base of the stalk is held by the first joint, which Curl leaves alone
        let low = curled.subs[0].vertices.chunks_exact(8).filter(|v| v[1] < 0.05).count();
        assert!(low >= 10);
        // the clip loops: 2 s later it is the same pose
        mesh.anim.time = 3.0;
        assert!((top(&assets.posed(7, &base, &mesh)) - tip).length() < 1e-3);
        // switching clips blends from the old pose
        mesh.play_animation("Wave", true);
        assert_eq!(mesh.anim.fade, 1.0);
        let blended = top(&assets.posed(7, &base, &mesh));
        assert!((blended - tip).length() < 1e-3, "at the start of the blend the pose has not jumped");
        mesh.advance_animation(0.3);
        assert_eq!(mesh.anim.fade, 0.0);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn invalid_file_becomes_error() {
        let d = tmp("bad");
        std::fs::write(d.join("x.obj"), "v 0 0 0\n").unwrap();
        assert!(load_model(&d.join("x.obj")).is_err());
        std::fs::write(d.join("x.glb"), "garbage").unwrap();
        assert!(load_model(&d.join("x.glb")).is_err());
        std::fs::remove_dir_all(d).ok();
    }
}
