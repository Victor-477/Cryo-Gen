//! In-editor 3D modelling: an editable polygon mesh and the operations of Edit Mesh mode
//! (extrude, inset, subdivide, delete, fill, merge, flip), saved to the project as `.obj`.

use crate::assets::{ModelData, SubMesh};
use crate::scene::Shape;
use glam::{Vec2, Vec3};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// Polygon mesh: shared vertex positions and faces with any number of corners.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditMesh {
    pub positions: Vec<Vec3>,
    pub faces: Vec<Vec<u32>>,
    /// Smooth shading (averaged normals) instead of flat faces.
    pub smooth: bool,
}

fn edge_key(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

impl EditMesh {
    // ------------------------------------------------------------------ queries

    pub fn face_normal(&self, f: usize) -> Vec3 {
        // Newell's method: works for any planar-ish polygon
        let face = &self.faces[f];
        let mut n = Vec3::ZERO;
        for i in 0..face.len() {
            let a = self.positions[face[i] as usize];
            let b = self.positions[face[(i + 1) % face.len()] as usize];
            n += Vec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
        }
        n.normalize_or_zero()
    }

    pub fn face_center(&self, f: usize) -> Vec3 {
        let face = &self.faces[f];
        face.iter().map(|i| self.positions[*i as usize]).sum::<Vec3>() / face.len().max(1) as f32
    }

    /// Unique edges as (vertex, vertex) pairs.
    pub fn edges(&self) -> Vec<(u32, u32)> {
        let mut set = BTreeSet::new();
        for face in &self.faces {
            for i in 0..face.len() {
                set.insert(edge_key(face[i], face[(i + 1) % face.len()]));
            }
        }
        set.into_iter().collect()
    }

    /// Vertices used by the given faces.
    pub fn face_vertices(&self, faces: &BTreeSet<usize>) -> BTreeSet<usize> {
        faces.iter().filter_map(|f| self.faces.get(*f)).flatten().map(|v| *v as usize).collect()
    }

    pub fn vertex_normals(&self) -> Vec<Vec3> {
        let mut out = vec![Vec3::ZERO; self.positions.len()];
        for f in 0..self.faces.len() {
            let n = self.face_normal(f);
            for v in &self.faces[f] {
                out[*v as usize] += n;
            }
        }
        out.into_iter().map(|n| n.normalize_or_zero()).collect()
    }

    /// Nearest face hit by a ray (in the mesh's local space): (face index, distance).
    pub fn pick_face(&self, origin: Vec3, dir: Vec3) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for (f, face) in self.faces.iter().enumerate() {
            for i in 1..face.len().saturating_sub(1) {
                let a = self.positions[face[0] as usize];
                let b = self.positions[face[i] as usize];
                let c = self.positions[face[i + 1] as usize];
                // Möller–Trumbore, both sides
                let (e1, e2) = (b - a, c - a);
                let p = dir.cross(e2);
                let det = e1.dot(p);
                if det.abs() < 1e-9 {
                    continue;
                }
                let s = origin - a;
                let u = s.dot(p) / det;
                let q = s.cross(e1);
                let v = dir.dot(q) / det;
                let t = e2.dot(q) / det;
                if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && best.map(|(_, bt)| t < bt).unwrap_or(true) {
                    best = Some((f, t));
                }
            }
        }
        best
    }

    // ------------------------------------------------------------------ operations
    // Each returns the new selection (faces or vertices, as documented).

    /// Extrudes the selected faces as one region along their average normal.
    /// Returns the faces that were moved out (the same indices, now the caps).
    pub fn extrude(&mut self, faces: &BTreeSet<usize>, distance: f32) -> BTreeSet<usize> {
        let faces: BTreeSet<usize> = faces.iter().copied().filter(|f| *f < self.faces.len()).collect();
        if faces.is_empty() {
            return faces;
        }
        let normal = faces.iter().map(|f| self.face_normal(*f)).sum::<Vec3>().normalize_or_zero();
        // directed boundary edges: edges used by exactly one selected face
        let mut count: HashMap<(u32, u32), usize> = HashMap::new();
        for f in &faces {
            let face = &self.faces[*f];
            for i in 0..face.len() {
                *count.entry(edge_key(face[i], face[(i + 1) % face.len()])).or_default() += 1;
            }
        }
        // duplicate every vertex of the region
        let mut copy: HashMap<u32, u32> = HashMap::new();
        for v in self.face_vertices(&faces) {
            copy.insert(v as u32, self.positions.len() as u32);
            self.positions.push(self.positions[v] + normal * distance);
        }
        let mut sides = vec![];
        for f in &faces {
            let face = self.faces[*f].clone();
            for i in 0..face.len() {
                let (a, b) = (face[i], face[(i + 1) % face.len()]);
                if count[&edge_key(a, b)] == 1 {
                    sides.push(vec![a, b, copy[&b], copy[&a]]);
                }
            }
            self.faces[*f] = face.iter().map(|v| copy[v]).collect();
        }
        self.faces.extend(sides);
        faces
    }

    /// Insets each selected face: a smaller face inside it plus a ring of quads.
    /// Returns the inner faces.
    pub fn inset(&mut self, faces: &BTreeSet<usize>, amount: f32) -> BTreeSet<usize> {
        let amount = amount.clamp(0.01, 0.95);
        let mut out = BTreeSet::new();
        let count = self.faces.len();
        for f in faces.iter().copied().filter(|f| *f < count) {
            let face = self.faces[f].clone();
            let center = self.face_center(f);
            let inner: Vec<u32> = face
                .iter()
                .map(|v| {
                    let p = self.positions[*v as usize];
                    self.positions.push(p.lerp(center, amount));
                    (self.positions.len() - 1) as u32
                })
                .collect();
            for i in 0..face.len() {
                let j = (i + 1) % face.len();
                self.faces.push(vec![face[i], face[j], inner[j], inner[i]]);
            }
            self.faces[f] = inner;
            out.insert(f);
        }
        out
    }

    /// Splits each selected face into quads (one per corner). Neighbouring faces receive the new
    /// edge midpoints so the surface stays closed. Returns the new faces.
    pub fn subdivide(&mut self, faces: &BTreeSet<usize>) -> BTreeSet<usize> {
        let selected: Vec<usize> = faces.iter().copied().filter(|f| *f < self.faces.len()).collect();
        let mut mids: HashMap<(u32, u32), u32> = HashMap::new();
        let mut out = BTreeSet::new();
        let mut new_faces: Vec<Vec<u32>> = vec![];
        for &f in &selected {
            let face = self.faces[f].clone();
            let n = face.len();
            let center = self.positions.len() as u32;
            self.positions.push(self.face_center(f));
            let mut mid = vec![];
            for i in 0..n {
                let (a, b) = (face[i], face[(i + 1) % n]);
                let m = *mids.entry(edge_key(a, b)).or_insert_with(|| {
                    self.positions.push((self.positions[a as usize] + self.positions[b as usize]) * 0.5);
                    (self.positions.len() - 1) as u32
                });
                mid.push(m);
            }
            for i in 0..n {
                let quad = vec![face[i], mid[i], center, mid[(i + n - 1) % n]];
                if i == 0 {
                    self.faces[f] = quad;
                    out.insert(f);
                } else {
                    new_faces.push(quad);
                }
            }
        }
        // give the midpoints to the faces that were not subdivided
        let skip: BTreeSet<usize> = selected.iter().copied().collect();
        for (f, face) in self.faces.iter_mut().enumerate() {
            if skip.contains(&f) {
                continue;
            }
            let mut patched = Vec::with_capacity(face.len());
            for i in 0..face.len() {
                let (a, b) = (face[i], face[(i + 1) % face.len()]);
                patched.push(a);
                if let Some(m) = mids.get(&edge_key(a, b)) {
                    patched.push(*m);
                }
            }
            *face = patched;
        }
        for q in new_faces {
            out.insert(self.faces.len());
            self.faces.push(q);
        }
        out
    }

    pub fn delete_faces(&mut self, faces: &BTreeSet<usize>) {
        let mut i = 0;
        self.faces.retain(|_| {
            i += 1;
            !faces.contains(&(i - 1))
        });
        self.remove_unused_vertices();
    }

    /// Removes vertices and every face that uses them.
    pub fn delete_vertices(&mut self, vertices: &BTreeSet<usize>) {
        self.faces.retain(|f| !f.iter().any(|v| vertices.contains(&(*v as usize))));
        self.remove_unused_vertices();
    }

    pub fn flip(&mut self, faces: &BTreeSet<usize>) {
        for f in faces {
            if let Some(face) = self.faces.get_mut(*f) {
                face.reverse();
            }
        }
    }

    /// Creates a face from three or more vertices, ordered around their centre.
    /// Returns the index of the new face.
    pub fn fill(&mut self, vertices: &BTreeSet<usize>) -> Option<usize> {
        let verts: Vec<usize> = vertices.iter().copied().filter(|v| *v < self.positions.len()).collect();
        if verts.len() < 3 {
            return None;
        }
        let center = verts.iter().map(|v| self.positions[*v]).sum::<Vec3>() / verts.len() as f32;
        // plane normal from the first non-degenerate triple
        let mut normal = Vec3::ZERO;
        'find: for i in 0..verts.len() {
            for j in (i + 1)..verts.len() {
                let n = (self.positions[verts[i]] - center).cross(self.positions[verts[j]] - center);
                if n.length_squared() > 1e-10 {
                    normal = n.normalize();
                    break 'find;
                }
            }
        }
        if normal == Vec3::ZERO {
            return None;
        }
        let x = (self.positions[verts[0]] - center).normalize_or_zero();
        let y = normal.cross(x);
        let mut ordered: Vec<(f32, usize)> = verts
            .iter()
            .map(|v| {
                let d = self.positions[*v] - center;
                (d.dot(y).atan2(d.dot(x)), *v)
            })
            .collect();
        ordered.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        self.faces.push(ordered.into_iter().map(|(_, v)| v as u32).collect());
        Some(self.faces.len() - 1)
    }

    /// Merges the vertices into one at their centre. Returns the surviving vertex.
    pub fn merge(&mut self, vertices: &BTreeSet<usize>) -> Option<usize> {
        let verts: Vec<usize> = vertices.iter().copied().filter(|v| *v < self.positions.len()).collect();
        if verts.len() < 2 {
            return None;
        }
        let center = verts.iter().map(|v| self.positions[*v]).sum::<Vec3>() / verts.len() as f32;
        let keep = verts[0] as u32;
        self.positions[keep as usize] = center;
        for face in &mut self.faces {
            for v in face.iter_mut() {
                if vertices.contains(&(*v as usize)) {
                    *v = keep;
                }
            }
            face.dedup();
            if face.len() > 1 && face.first() == face.last() {
                face.pop();
            }
        }
        self.faces.retain(|f| f.len() >= 3);
        let before = self.positions[keep as usize];
        self.remove_unused_vertices();
        self.positions.iter().position(|p| *p == before)
    }

    fn remove_unused_vertices(&mut self) {
        let mut used = vec![false; self.positions.len()];
        for f in &self.faces {
            for v in f {
                used[*v as usize] = true;
            }
        }
        let mut remap = vec![0u32; self.positions.len()];
        let mut kept = vec![];
        for (i, p) in self.positions.iter().enumerate() {
            if used[i] {
                remap[i] = kept.len() as u32;
                kept.push(*p);
            }
        }
        self.positions = kept;
        for f in &mut self.faces {
            for v in f.iter_mut() {
                *v = remap[*v as usize];
            }
        }
    }

    // ------------------------------------------------------------------ primitives

    /// Polygon version of a primitive, ready to edit (same size as the built-in shape).
    pub fn from_shape(shape: Shape) -> EditMesh {
        let mut m = EditMesh::default();
        let seg = 12usize;
        let ring = |m: &mut EditMesh, y: f32, r: f32| -> Vec<u32> {
            (0..seg)
                .map(|i| {
                    let a = i as f32 / seg as f32 * std::f32::consts::TAU;
                    m.positions.push(Vec3::new(a.cos() * r, y, -a.sin() * r));
                    (m.positions.len() - 1) as u32
                })
                .collect()
        };
        let bridge = |m: &mut EditMesh, lo: &[u32], hi: &[u32]| {
            for i in 0..lo.len() {
                let j = (i + 1) % lo.len();
                m.faces.push(vec![lo[i], lo[j], hi[j], hi[i]]);
            }
        };
        match shape {
            Shape::Cube => {
                for z in [-0.5, 0.5] {
                    for y in [-0.5, 0.5] {
                        for x in [-0.5, 0.5] {
                            m.positions.push(Vec3::new(x, y, z));
                        }
                    }
                }
                // vertex index = x + 2y + 4z
                m.faces = vec![
                    vec![4, 5, 7, 6], // +Z
                    vec![1, 0, 2, 3], // -Z
                    vec![5, 1, 3, 7], // +X
                    vec![0, 4, 6, 2], // -X
                    vec![6, 7, 3, 2], // +Y
                    vec![0, 1, 5, 4], // -Y
                ];
            }
            Shape::Plane => {
                m.positions = vec![
                    Vec3::new(-0.5, 0.0, 0.5),
                    Vec3::new(0.5, 0.0, 0.5),
                    Vec3::new(0.5, 0.0, -0.5),
                    Vec3::new(-0.5, 0.0, -0.5),
                ];
                m.faces = vec![vec![0, 1, 2, 3]];
            }
            Shape::Cylinder => {
                let lo = ring(&mut m, -0.5, 0.5);
                let hi = ring(&mut m, 0.5, 0.5);
                bridge(&mut m, &lo, &hi);
                m.faces.push(hi.clone());
                m.faces.push(lo.iter().rev().copied().collect());
            }
            Shape::Cone => {
                let lo = ring(&mut m, -0.5, 0.5);
                m.positions.push(Vec3::new(0.0, 0.5, 0.0));
                let tip = (m.positions.len() - 1) as u32;
                for i in 0..seg {
                    m.faces.push(vec![lo[i], lo[(i + 1) % seg], tip]);
                }
                m.faces.push(lo.iter().rev().copied().collect());
            }
            Shape::Sphere | Shape::Capsule => {
                // rings from bottom to top; the capsule stretches the two halves apart
                let stretch = if shape == Shape::Capsule { 0.5 } else { 0.0 };
                let stacks = 6usize;
                m.smooth = true;
                m.positions.push(Vec3::new(0.0, -0.5 - stretch, 0.0));
                let bottom = 0u32;
                let mut rings: Vec<Vec<u32>> = vec![];
                for s in 1..stacks {
                    let phi = -std::f32::consts::FRAC_PI_2 + s as f32 / stacks as f32 * std::f32::consts::PI;
                    let y = phi.sin() * 0.5;
                    let offset = if y < 0.0 { -stretch } else { stretch };
                    // the capsule needs two rings at the equator to form its straight side
                    if shape == Shape::Capsule && s == stacks / 2 {
                        rings.push(ring(&mut m, -stretch, 0.5));
                        rings.push(ring(&mut m, stretch, 0.5));
                    } else {
                        rings.push(ring(&mut m, y + offset, phi.cos() * 0.5));
                    }
                }
                m.positions.push(Vec3::new(0.0, 0.5 + stretch, 0.0));
                let top = (m.positions.len() - 1) as u32;
                for i in 0..seg {
                    let j = (i + 1) % seg;
                    m.faces.push(vec![bottom, rings[0][j], rings[0][i]]);
                    let last = rings.last().unwrap();
                    m.faces.push(vec![last[i], last[j], top]);
                }
                for r in 0..rings.len() - 1 {
                    let (lo, hi) = (rings[r].clone(), rings[r + 1].clone());
                    bridge(&mut m, &lo, &hi);
                }
            }
        }
        m
    }

    // ------------------------------------------------------------------ rendering and files

    /// Triangulated copy for the renderer (flat or smooth normals, box-projected UVs).
    pub fn to_model(&self, id: u64) -> ModelData {
        let smooth = self.vertex_normals();
        let mut vertices = vec![];
        let mut indices = vec![];
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for (f, face) in self.faces.iter().enumerate() {
            if face.len() < 3 {
                continue;
            }
            let n = self.face_normal(f);
            let base = (vertices.len() / 8) as u32;
            for v in face {
                let p = self.positions[*v as usize];
                let normal = if self.smooth { smooth[*v as usize] } else { n };
                let uv = box_uv(p, n);
                vertices.extend_from_slice(&[p.x, p.y, p.z, normal.x, normal.y, normal.z, uv.x, uv.y]);
                min = min.min(p);
                max = max.max(p);
            }
            for i in 1..face.len() as u32 - 1 {
                indices.extend_from_slice(&[base, base + i, base + i + 1]);
            }
        }
        if !min.is_finite() {
            min = Vec3::ZERO;
            max = Vec3::ZERO;
        }
        let triangles = indices.len() / 3;
        ModelData {
            id,
            version: 0,
            skeleton: None,
            lods: vec![],
            subs: vec![SubMesh { vertices, indices, color: [1.0; 3], texture: None, skin: None, normal: None, rough: -1.0, metal: 0.0, emissive: [0.0; 3], opacity: 1.0 }],
            center: (min + max) * 0.5,
            half: ((max - min) * 0.5).max(Vec3::splat(0.001)),
            triangles,
        }
    }

    pub fn to_obj(&self) -> String {
        let mut out = String::from("# Made with the Cryo Engine mesh editor\n");
        out.push_str(if self.smooth { "s 1\n" } else { "s off\n" });
        for p in &self.positions {
            out.push_str(&format!("v {} {} {}\n", p.x, p.y, p.z));
        }
        let normals = self.vertex_normals();
        if self.smooth {
            for n in &normals {
                out.push_str(&format!("vn {} {} {}\n", n.x, n.y, n.z));
            }
        }
        let mut vt = 0;
        for (f, face) in self.faces.iter().enumerate() {
            let n = self.face_normal(f);
            let mut line = String::from("f");
            for v in face {
                // OBJ stores V flipped compared to the engine
                let uv = box_uv(self.positions[*v as usize], n);
                out.push_str(&format!("vt {} {}\n", uv.x, 1.0 - uv.y));
                vt += 1;
                if self.smooth {
                    line.push_str(&format!(" {}/{}/{}", v + 1, vt, v + 1));
                } else {
                    line.push_str(&format!(" {}/{}", v + 1, vt));
                }
            }
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    /// Reads the polygons of an `.obj` file (positions and faces; materials are not edited).
    pub fn from_obj(src: &str) -> Result<EditMesh, String> {
        let mut m = EditMesh::default();
        for (ln, line) in src.lines().enumerate() {
            let mut it = line.split_whitespace();
            match it.next() {
                Some("v") => {
                    let v: Vec<f32> = it.filter_map(|t| t.parse().ok()).collect();
                    if v.len() < 3 {
                        return Err(format!("line {}: incomplete vertex", ln + 1));
                    }
                    m.positions.push(Vec3::new(v[0], v[1], v[2]));
                }
                Some("vn") => m.smooth = true,
                Some("s") => {
                    if matches!(it.next(), Some("off") | Some("0")) {
                        m.smooth = false;
                    }
                }
                Some("f") => {
                    let mut face = vec![];
                    for tok in it {
                        let i: i64 = tok.split('/').next().and_then(|t| t.parse().ok()).unwrap_or(0);
                        let len = m.positions.len() as i64;
                        let index = if i > 0 { i - 1 } else { len + i };
                        if index < 0 || index >= len {
                            return Err(format!("line {}: invalid vertex index", ln + 1));
                        }
                        face.push(index as u32);
                    }
                    face.dedup();
                    if face.len() >= 3 {
                        m.faces.push(face);
                    }
                }
                _ => {}
            }
        }
        if m.faces.is_empty() {
            return Err("the model contains no faces".into());
        }
        // a file with "s off" after normals keeps flat shading
        if src.lines().any(|l| l.trim() == "s off") {
            m.smooth = false;
        }
        Ok(m)
    }

    pub fn load(path: &Path) -> Result<EditMesh, String> {
        let src = std::fs::read_to_string(path).map_err(|e| format!("could not read ({e})"))?;
        EditMesh::from_obj(&src)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, self.to_obj()).map_err(|e| format!("could not save the model: {e}"))
    }
}

/// Texture coordinates by projecting along the face's dominant axis (1 unit = 1 tile).
fn box_uv(p: Vec3, n: Vec3) -> Vec2 {
    let a = n.abs();
    if a.x >= a.y && a.x >= a.z {
        Vec2::new(p.z + 0.5, 0.5 - p.y)
    } else if a.y >= a.z {
        Vec2::new(p.x + 0.5, p.z + 0.5)
    } else {
        Vec2::new(p.x + 0.5, 0.5 - p.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[usize]) -> BTreeSet<usize> {
        items.iter().copied().collect()
    }

    /// Closed surface: every edge is shared by exactly two faces, in opposite directions.
    fn is_closed(m: &EditMesh) -> bool {
        let mut directed: HashMap<(u32, u32), i32> = HashMap::new();
        for face in &m.faces {
            for i in 0..face.len() {
                *directed.entry((face[i], face[(i + 1) % face.len()])).or_default() += 1;
            }
        }
        directed.iter().all(|((a, b), n)| *n == 1 && directed.get(&(*b, *a)) == Some(&1))
    }

    /// Signed volume: positive when the faces point outwards.
    fn volume(m: &EditMesh) -> f32 {
        let mut v = 0.0;
        for face in &m.faces {
            for i in 1..face.len() - 1 {
                let (a, b, c) = (
                    m.positions[face[0] as usize],
                    m.positions[face[i] as usize],
                    m.positions[face[i + 1] as usize],
                );
                v += a.dot(b.cross(c)) / 6.0;
            }
        }
        v
    }

    #[test]
    fn primitives_are_closed_and_face_outwards() {
        for shape in [Shape::Cube, Shape::Cylinder, Shape::Cone, Shape::Sphere, Shape::Capsule] {
            let m = EditMesh::from_shape(shape);
            assert!(is_closed(&m), "{shape:?} is not closed");
            assert!(volume(&m) > 0.05, "{shape:?} volume {}", volume(&m));
        }
        assert!((volume(&EditMesh::from_shape(Shape::Cube)) - 1.0).abs() < 1e-5);
        let plane = EditMesh::from_shape(Shape::Plane);
        assert!(plane.face_normal(0).y > 0.99);
    }

    #[test]
    fn extrude_grows_the_cube_and_keeps_it_closed() {
        let mut m = EditMesh::from_shape(Shape::Cube);
        let top = (0..6).find(|f| m.face_normal(*f).y > 0.9).unwrap();
        let sel = m.extrude(&set(&[top]), 1.0);
        assert_eq!(sel, set(&[top]));
        assert_eq!(m.positions.len(), 12);
        assert_eq!(m.faces.len(), 10);
        assert!(is_closed(&m));
        assert!((volume(&m) - 2.0).abs() < 1e-4, "volume {}", volume(&m));
        assert!((m.face_center(top).y - 1.5).abs() < 1e-5);
    }

    #[test]
    fn extruding_two_adjacent_faces_moves_them_as_one_region() {
        let mut m = EditMesh::from_shape(Shape::Cube);
        m.subdivide(&set(&[0]));
        // two of the four quads that now make up the +Z side
        let quads: Vec<usize> = (0..m.faces.len()).filter(|f| m.face_normal(*f).z > 0.9).collect();
        assert_eq!(quads.len(), 4);
        m.extrude(&set(&quads[..2]), 0.5);
        assert!(is_closed(&m));
        assert!(volume(&m) > 1.2);
    }

    #[test]
    fn inset_subdivide_and_flip() {
        let mut m = EditMesh::from_shape(Shape::Cube);
        let inner = m.inset(&set(&[0]), 0.25);
        assert_eq!(inner.len(), 1);
        assert_eq!(m.faces.len(), 10);
        assert!(is_closed(&m));
        assert!((volume(&m) - 1.0).abs() < 1e-4);

        let new = m.subdivide(&set(&[1]));
        assert_eq!(new.len(), 4);
        assert!(is_closed(&m), "neighbours must receive the midpoints");
        assert!((volume(&m) - 1.0).abs() < 1e-4);

        let n = m.face_normal(2);
        m.flip(&set(&[2]));
        assert!(m.face_normal(2).dot(n) < -0.99);
    }

    #[test]
    fn delete_fill_and_merge() {
        let mut m = EditMesh::from_shape(Shape::Cube);
        let top = (0..6).find(|f| m.face_normal(*f).y > 0.9).unwrap();
        let ring: BTreeSet<usize> = m.faces[top].iter().map(|v| *v as usize).collect();
        m.delete_faces(&set(&[top]));
        assert_eq!(m.faces.len(), 5);
        assert_eq!(m.positions.len(), 8);
        assert!(!is_closed(&m));
        let f = m.fill(&ring).unwrap();
        assert_eq!(m.faces[f].len(), 4);
        // the new face may need flipping, but it closes the hole
        if !is_closed(&m) {
            m.flip(&set(&[f]));
        }
        assert!(is_closed(&m));

        // merging the four top vertices turns the cube into a pyramid
        let survivor = m.merge(&ring).unwrap();
        assert_eq!(m.positions.len(), 5);
        assert!((m.positions[survivor].y - 0.5).abs() < 1e-5);
        assert!(m.faces.iter().all(|f| f.len() >= 3));

        m.delete_vertices(&set(&[survivor]));
        assert_eq!(m.faces.len(), 1, "only the base is left");
    }

    #[test]
    fn obj_round_trip_and_model_conversion() {
        let mut m = EditMesh::from_shape(Shape::Cube);
        m.extrude(&set(&[0]), 0.5);
        let back = EditMesh::from_obj(&m.to_obj()).unwrap();
        assert_eq!(back.faces, m.faces);
        assert!(back.positions.iter().zip(&m.positions).all(|(a, b)| (*a - *b).length() < 1e-5));
        assert!(!back.smooth);

        let model = m.to_model(7);
        assert_eq!(model.triangles, 20);
        assert!((model.half.z - 0.75).abs() < 1e-5);
        // the engine's own .obj loader agrees with what the editor wrote
        let dir = std::env::temp_dir().join(format!("cryo_mesh_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        m.save(&dir.join("m.obj")).unwrap();
        let loaded = crate::assets::load_model(&dir.join("m.obj")).unwrap();
        assert_eq!(loaded.triangles, 20);
        std::fs::remove_dir_all(dir).ok();

        let mut smooth = EditMesh::from_shape(Shape::Sphere);
        smooth.smooth = true;
        assert!(EditMesh::from_obj(&smooth.to_obj()).unwrap().smooth);
    }

    #[test]
    fn picking_hits_the_nearest_face() {
        let m = EditMesh::from_shape(Shape::Cube);
        let (f, t) = m.pick_face(Vec3::new(0.0, 5.0, 0.0), Vec3::NEG_Y).unwrap();
        assert!(m.face_normal(f).y > 0.9);
        assert!((t - 4.5).abs() < 1e-4);
        assert!(m.pick_face(Vec3::new(3.0, 5.0, 0.0), Vec3::NEG_Y).is_none());
    }
}
