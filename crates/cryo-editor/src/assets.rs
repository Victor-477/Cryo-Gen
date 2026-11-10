//! Recursos importados: modelos 3D (.obj, .gltf, .glb) e texturas (.png, .jpg).

use glam::{Mat3, Mat4, Vec2, Vec3};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub const MODEL_EXTS: &[&str] = &["obj", "gltf", "glb"];
pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg"];
pub const SOUND_EXTS: &[&str] = &["wav", "ogg", "mp3"];

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
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

/// Parte de um modelo com um único material.
pub struct SubMesh {
    /// Intercalado: posição (3), normal (3), uv (2).
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    pub color: [f32; 3],
    pub texture: Option<Arc<TextureData>>,
}

pub struct ModelData {
    pub id: u64,
    pub subs: Vec<SubMesh>,
    pub center: Vec3,
    pub half: Vec3,
    pub triangles: usize,
}

/// Cache de recursos carregados do disco (lado da CPU; a GPU é preenchida pelo renderizador).
pub struct Assets {
    pub project_dir: PathBuf,
    models: HashMap<String, Option<Arc<ModelData>>>,
    textures: HashMap<String, Option<Arc<TextureData>>>,
    /// Erros de carregamento ainda não mostrados no Console.
    pub errors: Vec<String>,
}

impl Assets {
    pub fn new(project_dir: PathBuf) -> Self {
        Assets { project_dir, models: HashMap::new(), textures: HashMap::new(), errors: vec![] }
    }

    pub fn model(&mut self, rel: &str) -> Option<Arc<ModelData>> {
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

    /// Esquece um recurso para que seja relido do disco (após reimportar).
    pub fn invalidate(&mut self, rel: &str) {
        self.models.remove(rel);
        self.textures.remove(rel);
    }
}

pub fn load_texture(path: &Path) -> Result<TextureData, String> {
    let img = image::open(path).map_err(|e| format!("imagem inválida ({e})"))?;
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
    let subs = match ext.as_str() {
        "obj" => load_obj(path)?,
        "gltf" | "glb" => load_gltf(path)?,
        other => return Err(format!("formato `.{other}` não suportado (use .obj, .gltf ou .glb)")),
    };
    finish(subs)
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
                return Err("o modelo contém coordenadas inválidas".into());
            }
            min = min.min(p);
            max = max.max(p);
        }
    }
    if triangles == 0 {
        return Err("o modelo não contém triângulos".into());
    }
    Ok(ModelData {
        id: next_id(),
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
                // o nome do arquivo é o último item (opções vêm antes)
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
    let src = std::fs::read_to_string(path).map_err(|e| format!("não foi possível ler ({e})"))?;
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
                    return Err(format!("linha {}: vértice incompleto", ln + 1));
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
                        None => return Err(format!("linha {}: índice de vértice inválido", ln + 1)),
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
                        // com normal própria: vértices iguais são compartilhados
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
                        // sem normal: usa a normal da face
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
            // com textura, a cor difusa costuma ser só um multiplicador neutro
            let color = match (mat, &texture) {
                (Some(_), Some(_)) => [1.0; 3],
                (Some(m), None) => m.color,
                (None, _) => [1.0; 3],
            };
            SubMesh { vertices: g.vertices, indices: g.indices, color, texture }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// glTF 2.0 (.gltf / .glb)
// ---------------------------------------------------------------------------

fn load_gltf(path: &Path) -> Result<Vec<SubMesh>, String> {
    let (doc, buffers, images) = gltf::import(path).map_err(|e| format!("glTF inválido ({e})"))?;
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

    // percorre a hierarquia de nós acumulando as transformações
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
                return Err("índices fora do intervalo no modelo".into());
            }

            let pbr = prim.material().pbr_metallic_roughness();
            let c = pbr.base_color_factor();
            let texture = pbr.base_color_texture().and_then(|t| texture_for(t.texture().source().index()));

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
                    // sem normais: calcula uma por face
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
            // escala negativa no nó inverte a orientação dos triângulos
            if world.determinant() < 0.0 {
                for tri in out_idx.chunks_exact_mut(3) {
                    tri.swap(1, 2);
                }
            }
            subs.push(SubMesh { vertices, indices: out_idx, color: [c[0], c[1], c[2]], texture });
        }
    }
    Ok(subs)
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
    fn obj_com_quad_material_e_indices_negativos() {
        let d = tmp("obj");
        std::fs::write(d.join("m.mtl"), "newmtl vermelho\nKd 1 0 0\n").unwrap();
        std::fs::write(
            d.join("q.obj"),
            "mtllib m.mtl\nv 0 0 0\nv 2 0 0\nv 2 0 -4\nv 0 0 -4\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n\
             usemtl vermelho\nf 1/1 2/2 3/3 4/4\nf -4 -3 -2\n",
        )
        .unwrap();
        let m = load_model(&d.join("q.obj")).unwrap();
        assert_eq!(m.triangles, 3);
        assert_eq!(m.subs.len(), 1);
        assert_eq!(m.subs[0].color, [1.0, 0.0, 0.0]);
        assert!((m.center - Vec3::new(1.0, 0.0, -2.0)).length() < 1e-5);
        // normal calculada da face aponta para cima
        assert!((m.subs[0].vertices[4] - 1.0).abs() < 1e-5);
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn gltf_com_no_transformado() {
        let d = tmp("gltf");
        // um triângulo: 3 posições f32 (36 bytes) + 3 índices u16 (6 bytes)
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
        // a transformação do nó foi aplicada: x vai de 10 a 12
        assert!((m.center.x - 11.0).abs() < 1e-4, "{:?}", m.center);
        assert!((m.subs[0].vertices[5] - 1.0).abs() < 1e-5, "normal +Z");
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn arquivo_invalido_vira_erro() {
        let d = tmp("bad");
        std::fs::write(d.join("x.obj"), "v 0 0 0\n").unwrap();
        assert!(load_model(&d.join("x.obj")).is_err());
        std::fs::write(d.join("x.glb"), "lixo").unwrap();
        assert!(load_model(&d.join("x.glb")).is_err());
        std::fs::remove_dir_all(d).ok();
    }
}
