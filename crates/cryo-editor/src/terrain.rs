//! Terrain: ground with hills and valleys, made from a height map (a grayscale image).
//!
//! A terrain is a small text file with the `.terrain` extension, used as an entity's model:
//!
//! ```text
//! heightmap = valley.png      # grayscale, 8 or 16 bits; white is high
//! size = 200 200              # width (X) and depth (Z) in metres, centred on the entity
//! height = 30                 # metres between black and white
//! base = -1                   # height of black
//! texture = tex_grass.jpg     # ground texture, repeated every `tile` metres
//! tile = 6
//! steep_texture = tex_cliff.jpg   # used where the slope is steeper than `steep` (optional)
//! steep_tile = 10
//! steep = 0.82                # 1 = flat, 0.7 is about 45 degrees
//! color = 1 1 1
//! roughness = 0.9
//! ```
//!
//! With a Mesh collider the terrain collides as a height field: fast, whatever its size, and
//! rays (`scene.raycast`) hit it too.

use crate::assets::{load_texture, SubMesh, TextureData};
use glam::{Vec2, Vec3};
use std::path::Path;
use std::sync::Arc;

/// Heights on a regular grid, in the terrain's own space (centred on the origin).
#[derive(Debug, PartialEq)]
pub struct Heightfield {
    pub cols: usize,
    pub rows: usize,
    pub size: Vec2,
    pub heights: Vec<f32>,
}

impl Heightfield {
    fn at(&self, c: usize, r: usize) -> f32 {
        self.heights[r.min(self.rows - 1) * self.cols + c.min(self.cols - 1)]
    }

    fn grid(&self, x: f32, z: f32) -> (f32, f32) {
        let gx = (x / self.size.x + 0.5) * (self.cols - 1) as f32;
        let gz = (z / self.size.y + 0.5) * (self.rows - 1) as f32;
        (gx, gz)
    }

    /// Whether (x, z) lies over the terrain.
    pub fn covers(&self, x: f32, z: f32) -> bool {
        x.abs() <= self.size.x * 0.5 && z.abs() <= self.size.y * 0.5
    }

    /// Ground height at (x, z), following the same two triangles per cell as the mesh.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let (gx, gz) = self.grid(x, z);
        let gx = gx.clamp(0.0, (self.cols - 1) as f32 - 1e-4);
        let gz = gz.clamp(0.0, (self.rows - 1) as f32 - 1e-4);
        let (c, r) = (gx.floor() as usize, gz.floor() as usize);
        let (fx, fz) = (gx - c as f32, gz - r as f32);
        let (h00, h10, h01, h11) = (self.at(c, r), self.at(c + 1, r), self.at(c, r + 1), self.at(c + 1, r + 1));
        if fx + fz <= 1.0 {
            h00 + (h10 - h00) * fx + (h01 - h00) * fz
        } else {
            h11 + (h01 - h11) * (1.0 - fx) + (h10 - h11) * (1.0 - fz)
        }
    }

    /// Upward surface normal at (x, z).
    pub fn normal(&self, x: f32, z: f32) -> Vec3 {
        let dx = self.size.x / (self.cols - 1) as f32;
        let dz = self.size.y / (self.rows - 1) as f32;
        let sx = self.height(x + dx * 0.5, z) - self.height(x - dx * 0.5, z);
        let sz = self.height(x, z + dz * 0.5) - self.height(x, z - dz * 0.5);
        Vec3::new(-sx / dx, 1.0, -sz / dz).normalize()
    }

    /// First hit of a ray (in the terrain's space) with the ground, as a distance along `dir`.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        let cell = (self.size.x / (self.cols - 1) as f32).min(self.size.y / (self.rows - 1) as f32);
        let step = (cell * 0.5).max(0.05);
        let above = |t: f32| {
            let p = origin + dir * t;
            p.y - self.height(p.x, p.z)
        };
        let mut t = 0.0;
        let mut prev = above(0.0);
        if prev < 0.0 {
            return None; // starts underground
        }
        while t < max {
            let next = (t + step).min(max);
            let p = origin + dir * next;
            let d = if self.covers(p.x, p.z) { above(next) } else { f32::INFINITY };
            if d <= 0.0 && prev > 0.0 {
                let (mut lo, mut hi) = (t, next);
                for _ in 0..20 {
                    let m = (lo + hi) * 0.5;
                    if above(m) > 0.0 { lo = m } else { hi = m }
                }
                return Some(hi);
            }
            prev = d;
            t = next;
        }
        None
    }
}

/// How the terrain looks.
struct Look {
    texture: Option<Arc<TextureData>>,
    normal: Option<Arc<TextureData>>,
    tile: f32,
    steep_texture: Option<Arc<TextureData>>,
    steep_normal: Option<Arc<TextureData>>,
    steep_tile: f32,
    steep: f32,
    color: [f32; 3],
    rough: f32,
}

fn texture_pair(dir: &Path, file: &str) -> (Option<Arc<TextureData>>, Option<Arc<TextureData>>) {
    if file.is_empty() {
        return (None, None);
    }
    let tex = load_texture(&dir.join(file)).ok().map(Arc::new);
    let stem = file.rsplit_once('.').map(|(s, e)| format!("{s}_n.{e}")).unwrap_or_default();
    let normal = load_texture(&dir.join(stem)).ok().map(Arc::new);
    (tex, normal)
}

/// Reads a `.terrain` file: the height field and the meshes that draw it.
pub fn load(path: &Path) -> Result<(Heightfield, Vec<SubMesh>), String> {
    read(path, true)
}

/// Only the height field (for collisions; no textures, no mesh).
pub fn load_field(path: &Path) -> Result<Heightfield, String> {
    read(path, false).map(|(f, _)| f)
}

fn read(path: &Path, with_mesh: bool) -> Result<(Heightfield, Vec<SubMesh>), String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("could not read ({e})"))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut heightmap = String::new();
    let (mut size, mut height, mut base) = (Vec2::splat(100.0), 10.0f32, 0.0f32);
    let (mut texture, mut steep_texture) = (String::new(), String::new());
    let (mut tile, mut steep_tile, mut steep) = (5.0f32, 8.0f32, 0.8f32);
    let (mut color, mut rough) = ([1.0f32; 3], 0.9f32);
    for line in src.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        let nums: Vec<f32> = value.split_whitespace().filter_map(|n| n.parse().ok()).collect();
        let first = nums.first().copied();
        match key.trim() {
            "heightmap" => heightmap = value.to_string(),
            "size" => {
                if let (Some(x), z) = (first, nums.get(1).copied()) {
                    size = Vec2::new(x, z.unwrap_or(x));
                }
            }
            "height" => height = first.unwrap_or(height),
            "base" => base = first.unwrap_or(base),
            "texture" => texture = value.to_string(),
            "tile" => tile = first.unwrap_or(tile).max(0.01),
            "steep_texture" => steep_texture = value.to_string(),
            "steep_tile" => steep_tile = first.unwrap_or(steep_tile).max(0.01),
            "steep" => steep = first.unwrap_or(steep),
            "color" if nums.len() >= 3 => color = [nums[0], nums[1], nums[2]],
            "roughness" => rough = first.unwrap_or(rough).clamp(0.02, 1.0),
            _ => {}
        }
    }
    if heightmap.is_empty() {
        return Err("the terrain names no heightmap".into());
    }
    let img = image::open(dir.join(&heightmap)).map_err(|e| format!("could not read the heightmap ({e})"))?;
    let gray = img.to_luma16();
    // at most 513 samples a side keeps the mesh drawable (about half a million triangles)
    let step = ((gray.width().max(gray.height()) as usize + 511) / 512).max(1);
    let cols = (gray.width() as usize - 1) / step + 1;
    let rows = (gray.height() as usize - 1) / step + 1;
    if cols < 2 || rows < 2 {
        return Err("the heightmap is too small".into());
    }
    let mut heights = Vec::with_capacity(cols * rows);
    for r in 0..rows {
        for c in 0..cols {
            let v = gray.get_pixel((c * step) as u32, (r * step) as u32).0[0] as f32 / 65535.0;
            heights.push(base + v * height);
        }
    }
    let field = Heightfield { cols, rows, size: Vec2::new(size.x.max(0.1), size.y.max(0.1)), heights };
    if !with_mesh {
        return Ok((field, vec![]));
    }
    let (texture, normal) = texture_pair(dir, &texture);
    let (steep_texture, steep_normal) = texture_pair(dir, &steep_texture);
    let look = Look { texture, normal, tile, steep_texture, steep_normal, steep_tile, steep, color, rough };
    let subs = mesh(&field, &look);
    Ok((field, subs))
}

/// One mesh; where the ground is steep the shader mixes in the second texture (when given).
fn mesh(f: &Heightfield, look: &Look) -> Vec<SubMesh> {
    let mut vertices = Vec::with_capacity(f.cols * f.rows * 8);
    for r in 0..f.rows {
        for c in 0..f.cols {
            let x = (c as f32 / (f.cols - 1) as f32 - 0.5) * f.size.x;
            let z = (r as f32 / (f.rows - 1) as f32 - 0.5) * f.size.y;
            let n = f.normal(x, z);
            vertices.extend_from_slice(&[x, f.at(c, r), z, n.x, n.y, n.z, x / look.tile, z / look.tile]);
        }
    }
    let mut indices = Vec::with_capacity((f.cols - 1) * (f.rows - 1) * 6);
    for r in 0..f.rows - 1 {
        for c in 0..f.cols - 1 {
            let i = |c: usize, r: usize| (r * f.cols + c) as u32;
            indices.extend_from_slice(&[i(c, r), i(c, r + 1), i(c + 1, r), i(c + 1, r + 1), i(c + 1, r), i(c, r + 1)]);
        }
    }
    let blend = look.steep_texture.as_ref().map(|t| {
        Arc::new(crate::assets::Blend {
            texture: Some(t.clone()),
            normal: look.steep_normal.clone(),
            steep: look.steep,
            uv_scale: look.tile / look.steep_tile,
        })
    });
    vec![SubMesh {
        vertices,
        indices,
        color: look.color,
        texture: look.texture.clone(),
        skin: None,
        normal: look.normal.clone(),
        rough: look.rough,
        metal: 0.0,
        emissive: [0.0; 3],
        opacity: 1.0,
        blend,
    }]
}

/// A starting terrain for a new project: rolling hills around a flat middle.
pub fn write_default(dir: &Path, name: &str) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let n = 257u32;
    let img = image::ImageBuffer::from_fn(n, n, |x, y| {
        let (u, v) = (x as f32 / (n - 1) as f32 - 0.5, y as f32 / (n - 1) as f32 - 0.5);
        let r = (u * u + v * v).sqrt();
        let hills = ((u * 9.0).sin() * (v * 7.0).cos() * 0.5 + 0.5) * 0.35 + ((u * 23.0 + 1.3).sin() * (v * 19.0 + 0.7).sin()) * 0.06;
        let rim = ((r - 0.18) / 0.3).clamp(0.0, 1.0);
        let h = (hills + 0.06) * rim * rim * (3.0 - 2.0 * rim);
        image::Luma([(h.clamp(0.0, 1.0) * 65535.0) as u16])
    });
    let png = format!("{name}_height.png");
    img.save(dir.join(&png)).map_err(|e| e.to_string())?;
    let file = format!("{name}.terrain");
    std::fs::write(
        dir.join(&file),
        format!(
            "# Cryo terrain: edit the numbers, or paint the heightmap in any image editor (white is high)\nheightmap = {png}\nsize = 160 160\nheight = 24\nbase = 0\ntexture =\ntile = 6\nsteep_texture =\nsteep = 0.8\ncolor = 0.42 0.5 0.32\nroughness = 0.9\n"
        ),
    )
    .map_err(|e| e.to_string())?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cryo_terrain_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn heights_normals_rays_and_the_default_terrain() {
        let d = tmp("t");
        // a ramp: height grows with x from 0 to 10 over 20 m
        let img = image::ImageBuffer::from_fn(65, 65, |x, _| image::Luma([(x as f32 / 64.0 * 65535.0) as u16]));
        img.save(d.join("ramp.png")).unwrap();
        std::fs::write(d.join("ramp.terrain"), "heightmap = ramp.png\nsize = 20 20\nheight = 10\n").unwrap();
        let (f, subs) = load(&d.join("ramp.terrain")).unwrap();
        assert!((f.height(-10.0, 0.0) - 0.0).abs() < 1e-3);
        assert!((f.height(0.0, 3.0) - 5.0).abs() < 1e-2);
        assert!((f.height(10.0, -5.0) - 10.0).abs() < 1e-2);
        let n = f.normal(0.0, 0.0);
        assert!(n.x < -0.4 && n.y > 0.85, "{n:?}");
        // a ray straight down from 20 m lands on the slope
        let t = f.raycast(Vec3::new(4.0, 20.0, 0.0), -Vec3::Y, 50.0).unwrap();
        assert!((20.0 - t - 7.0).abs() < 0.02, "{t}");
        // a ray along the slope, above it, never lands
        assert!(f.raycast(Vec3::new(-10.0, 1.0, 0.0), Vec3::new(1.0, 0.5, 0.0).normalize(), 30.0).is_none());
        let tris: usize = subs.iter().map(|s| s.indices.len() / 3).sum();
        assert_eq!(tris, 64 * 64 * 2);

        let file = write_default(&d, "hills").unwrap();
        let (f, _) = load(&d.join(file)).unwrap();
        assert!(f.height(0.0, 0.0).abs() < 0.05, "flat in the middle");
        assert!(f.heights.iter().cloned().fold(0.0f32, f32::max) > 3.0, "hills at the edges");
        std::fs::remove_dir_all(d).ok();
    }
}
