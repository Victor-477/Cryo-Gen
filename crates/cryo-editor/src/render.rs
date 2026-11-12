//! OpenGL 3D renderer (through glow): primitives and imported models, textures,
//! directional/point/spot lights, shadows, sky, grid and selection outline.

use crate::assets::{Assets, ModelData, TextureData};
use crate::scene::{LightKind, Scene, Shape};
use eframe::glow::{self, HasContext};
use glam::{Mat3, Mat4, Vec3};
use std::collections::HashMap;
use std::sync::Arc;

pub const MAX_DIR: usize = 4;
pub const MAX_POINT: usize = 16;
/// Joints per skin that the vertex shader accepts; larger skeletons are skinned on the CPU.
pub const MAX_JOINTS: usize = 64;
const SHADOW_SIZE: i32 = 2048;

pub enum MeshRef {
    Shape(Shape),
    Model(Arc<ModelData>),
}

pub struct DrawItem {
    pub model: Mat4,
    pub mesh: MeshRef,
    pub color: [f32; 3],
    pub texture: Option<Arc<TextureData>>,
    pub shininess: f32,
    pub unlit: bool,
    pub selected: bool,
    pub cast_shadow: bool,
    /// Joint matrices per skin, when the model is animated by its skeleton.
    pub joints: Option<Arc<Vec<Vec<Mat4>>>>,
    /// Bounding sphere in world space, used to skip what the camera cannot see.
    pub center: Vec3,
    pub radius: f32,
    pub in_view: bool,
    pub in_shadow: bool,
}

/// Point light or spotlight.
pub struct LocalLight {
    pub pos: Vec3,
    pub color: [f32; 3],
    pub range: f32,
    /// Spotlight direction (ignored for point lights).
    pub dir: Vec3,
    /// Cosine of the cone's outer angle; -2 = point light (no cone).
    pub cos_outer: f32,
}

/// Everything needed to draw one frame (assembled on the UI thread).
pub struct RenderFrame {
    pub view: Mat4,
    pub proj: Mat4,
    pub cam_pos: Vec3,
    pub items: Vec<DrawItem>,
    /// (direction the light travels, colour * intensity)
    pub dir_lights: Vec<(Vec3, [f32; 3])>,
    pub locals: Vec<LocalLight>,
    pub ambient: [f32; 3],
    pub sky_top: [f32; 3],
    pub sky_horizon: [f32; 3],
    pub fog: f32,
    /// Lines: x, y, z, r, g, b, a per vertex.
    pub lines: Vec<f32>,
    /// Shadows from the first directional light: (centre of the covered area, radius).
    pub shadow: Option<(Vec3, f32)>,
}

impl RenderFrame {
    pub fn from_scene(scene: &Scene, assets: &mut Assets, view: Mat4, proj: Mat4, selected: Option<u64>) -> Self {
        let cam_pos = view.inverse().w_axis.truncate();
        let mut items = vec![];
        let mut dir_lights = vec![];
        let mut locals = vec![];
        let mut first_dir_shadows = None;
        for e in &scene.entities {
            if !scene.is_visible(e.id) {
                continue;
            }
            let world = scene.world_matrix(e.id);
            if let Some(m) = &e.mesh {
                let mut joints = None;
                let mesh = match &m.model {
                    Some(path) => assets.model(path).map(|base| {
                        joints = assets.pose_matrices(e.id, &base, m);
                        if joints.is_some() {
                            MeshRef::Model(base)
                        } else {
                            MeshRef::Model(assets.posed(e.id, &base, m))
                        }
                    }),
                    None => Some(MeshRef::Shape(m.shape)),
                };
                // model that failed to load: show a magenta cube instead
                let (mesh, color) = match mesh {
                    Some(mesh) => (mesh, m.color),
                    None => (MeshRef::Shape(Shape::Cube), [1.0, 0.0, 1.0]),
                };
                let (local_center, local_radius) = match &mesh {
                    // an animated model reaches beyond its rest pose
                    MeshRef::Model(md) => (md.center, md.half.length() * if md.skeleton.is_some() { 1.6 } else { 1.0 }),
                    MeshRef::Shape(shape) => (Vec3::ZERO, shape.half_extents().length()),
                };
                let stretch = world.x_axis.truncate().length().max(world.y_axis.truncate().length()).max(world.z_axis.truncate().length());
                items.push(DrawItem {
                    joints,
                    center: world.transform_point3(local_center),
                    radius: local_radius * stretch,
                    in_view: true,
                    in_shadow: true,
                    model: world,
                    mesh,
                    color,
                    texture: m.texture.as_ref().and_then(|t| assets.texture(t)),
                    shininess: m.shininess,
                    unlit: m.unlit,
                    selected: selected == Some(e.id),
                    cast_shadow: m.cast_shadow,
                });
            }
            if let Some(l) = &e.light {
                let c = [l.color[0] * l.intensity, l.color[1] * l.intensity, l.color[2] * l.intensity];
                let dir = world.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
                match l.kind {
                    LightKind::Directional if dir_lights.len() < MAX_DIR => {
                        if dir_lights.is_empty() {
                            first_dir_shadows = l.shadows.then_some(l.shadow_distance.clamp(8.0, 150.0));
                        }
                        dir_lights.push((dir, c));
                    }
                    LightKind::Point | LightKind::Spot => {
                        locals.push(LocalLight {
                            pos: world.w_axis.truncate(),
                            color: c,
                            range: l.range.max(0.01),
                            dir,
                            cos_outer: if l.kind == LightKind::Spot {
                                (l.angle.clamp(1.0, 89.0)).to_radians().cos()
                            } else {
                                -2.0
                            },
                        });
                    }
                    _ => {}
                }
            }
        }
        // shadow area centred a little ahead of the camera
        let forward = view.inverse().transform_vector3(Vec3::NEG_Z).normalize_or_zero();
        let shadow = first_dir_shadows.map(|reach| (cam_pos + forward * reach * 0.54, reach));
        // skip what is outside the camera's view, and shadow casters too far to matter
        let vp = proj * view;
        let rows = [vp.row(0), vp.row(1), vp.row(2), vp.row(3)];
        let planes = [rows[3] + rows[0], rows[3] - rows[0], rows[3] + rows[1], rows[3] - rows[1], rows[3] + rows[2], rows[3] - rows[2]];
        for item in &mut items {
            item.in_view = item.selected
                || planes.iter().all(|p| {
                    let n = p.truncate();
                    n.dot(item.center) + p.w >= -item.radius * n.length()
                });
            item.in_shadow = match shadow {
                Some((c, reach)) => item.cast_shadow && item.center.distance(c) < reach * 1.8 + item.radius,
                None => false,
            };
        }
        items.retain(|i| i.in_view || i.in_shadow);
        // more lights than the shader takes: keep the ones that matter most to this view
        if locals.len() > MAX_POINT {
            locals.sort_by(|a, b| {
                let score = |l: &LocalLight| (l.pos.distance(cam_pos) - l.range).max(0.0);
                score(a).total_cmp(&score(b))
            });
            locals.truncate(MAX_POINT);
        }
        RenderFrame {
            view,
            proj,
            cam_pos,
            items,
            dir_lights,
            locals,
            ambient: scene.ambient,
            sky_top: scene.sky_top,
            sky_horizon: scene.sky_horizon,
            fog: scene.fog,
            lines: vec![],
            shadow,
        }
    }

    pub fn line(&mut self, a: Vec3, b: Vec3, c: [f32; 4]) {
        self.lines.extend_from_slice(&[a.x, a.y, a.z, c[0], c[1], c[2], c[3]]);
        self.lines.extend_from_slice(&[b.x, b.y, b.z, c[0], c[1], c[2], c[3]]);
    }

    /// Editor grid on the XZ plane, with the X (red) and Z (blue) axes.
    pub fn add_grid(&mut self, half: i32) {
        let h = half as f32;
        for i in -half..=half {
            let f = i as f32;
            let strong = i % 10 == 0;
            let a = if strong { 0.38 } else { 0.16 };
            let c = [0.75, 0.75, 0.78, a];
            if i == 0 {
                self.line(Vec3::new(-h, 0.0, 0.0), Vec3::new(h, 0.0, 0.0), [0.90, 0.25, 0.25, 0.8]);
                self.line(Vec3::new(0.0, 0.0, -h), Vec3::new(0.0, 0.0, h), [0.25, 0.45, 0.95, 0.8]);
            } else {
                self.line(Vec3::new(-h, 0.0, f), Vec3::new(h, 0.0, f), c);
                self.line(Vec3::new(f, 0.0, -h), Vec3::new(f, 0.0, h), c);
            }
        }
    }

    /// Draws a camera's view volume or a spotlight's cone (in the editor).
    pub fn add_frustum(&mut self, world: Mat4, fov_deg: f32, aspect: f32, depth: f32, c: [f32; 4]) {
        let hh = (fov_deg.to_radians() * 0.5).tan() * depth;
        let hw = hh * aspect;
        let (_, r, t) = world.to_scale_rotation_translation();
        let corners = [
            Vec3::new(-hw, -hh, -depth),
            Vec3::new(hw, -hh, -depth),
            Vec3::new(hw, hh, -depth),
            Vec3::new(-hw, hh, -depth),
        ]
        .map(|p| t + r * p);
        for i in 0..4 {
            self.line(t, corners[i], c);
            self.line(corners[i], corners[(i + 1) % 4], c);
        }
    }

    /// Wireframe box (colliders, model bounds).
    pub fn add_box(&mut self, world: Mat4, center: Vec3, half: Vec3, c: [f32; 4]) {
        let p = |x: f32, y: f32, z: f32| world.transform_point3(center + half * Vec3::new(x, y, z));
        for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            self.line(p(-1.0, a, b), p(1.0, a, b), c);
            self.line(p(a, -1.0, b), p(a, 1.0, b), c);
            self.line(p(a, b, -1.0), p(a, b, 1.0), c);
        }
    }
}

/// View matrix looking along `dir` from `eye`.
pub fn look_to(eye: Vec3, dir: Vec3, up: Vec3) -> Mat4 {
    let f = dir.normalize_or_zero();
    let up = if f.cross(up).length_squared() < 1e-6 { Vec3::X } else { up };
    let r = f.cross(up).normalize();
    let u = r.cross(f);
    Mat4::from_cols_array(&[
        r.x, u.x, -f.x, 0.0,
        r.y, u.y, -f.y, 0.0,
        r.z, u.z, -f.z, 0.0,
        -r.dot(eye), -u.dot(eye), f.dot(eye), 1.0,
    ])
}

/// Symmetric orthographic projection, OpenGL convention.
pub fn ortho(half_w: f32, half_h: f32, near: f32, far: f32) -> Mat4 {
    Mat4::from_cols_array(&[
        1.0 / half_w, 0.0, 0.0, 0.0,
        0.0, 1.0 / half_h, 0.0, 0.0,
        0.0, 0.0, -2.0 / (far - near), 0.0,
        0.0, 0.0, -(far + near) / (far - near), 1.0,
    ])
}

struct GpuMesh {
    vao: glow::VertexArray,
    buffers: [glow::Buffer; 2],
    count: i32,
    color: [f32; 3],
    texture: Option<Arc<TextureData>>,
    /// Index of the skin whose joints move this mesh on the graphics card.
    skin: Option<usize>,
}

/// Tells the shader whether the next mesh follows a skeleton, and sends its joints.
unsafe fn bind_skin(
    gl: &glow::Context,
    flag: &Option<glow::UniformLocation>,
    mats: &Option<glow::UniformLocation>,
    part: &GpuMesh,
    joints: &Option<Arc<Vec<Vec<Mat4>>>>,
) {
    if let (Some(skin), Some(all)) = (part.skin, joints) {
        if let Some(list) = all.get(skin) {
            let flat: Vec<f32> = list.iter().take(MAX_JOINTS).flat_map(|m| m.to_cols_array()).collect();
            gl.uniform_matrix_4_f32_slice(mats.as_ref(), false, &flat);
            gl.uniform_1_i32(flag.as_ref(), 1);
            return;
        }
    }
    gl.uniform_1_i32(flag.as_ref(), 0);
}

pub struct Renderer {
    mesh_prog: glow::Program,
    depth_prog: glow::Program,
    line_prog: glow::Program,
    sky_prog: glow::Program,
    shapes: HashMap<Shape, GpuMesh>,
    models: HashMap<u64, Vec<GpuMesh>>,
    model_version: HashMap<u64, u64>,
    textures: HashMap<u64, glow::Texture>,
    line_vao: glow::VertexArray,
    line_vbo: glow::Buffer,
    empty_vao: glow::VertexArray,
    shadow_fbo: Option<glow::Framebuffer>,
    shadow_tex: Option<glow::Texture>,
    /// 1×1 white texture bound when a mesh has no texture (some drivers, notably on macOS,
    /// complain about a sampler with nothing bound even if it is never sampled).
    white_tex: Option<glow::Texture>,
    /// Frame counter and the frame each model was last drawn in, to free models no longer used
    /// (mesh editing creates a new one on every change).
    frame_no: u64,
    model_used: HashMap<u64, u64>,
}

const MESH_VS: &str = r#"
in vec3 a_pos;
in vec3 a_normal;
in vec2 a_uv;
in vec4 a_joints;
in vec4 a_weights;
uniform mat4 u_joints[64];
uniform int u_skinned;
mat4 skin_matrix() {
    if (u_skinned == 0) return mat4(1.0);
    return a_weights.x * u_joints[int(a_joints.x)] + a_weights.y * u_joints[int(a_joints.y)]
         + a_weights.z * u_joints[int(a_joints.z)] + a_weights.w * u_joints[int(a_joints.w)];
}
uniform mat4 u_view_proj;
uniform mat4 u_model;
uniform mat3 u_normal_mat;
uniform float u_outline;
out vec3 v_world;
out vec3 v_normal;
out vec2 v_uv;
void main() {
    mat4 skin = skin_matrix();
    vec4 w = u_model * (skin * vec4(a_pos, 1.0));
    vec3 local_n = mat3(skin) * a_normal;
    v_normal = u_normal_mat * local_n;
    v_uv = a_uv;
    vec4 clip = u_view_proj * w;
    if (u_outline > 0.0) {
        vec3 n = normalize(u_normal_mat * local_n);
        w.xyz += n * u_outline * max(clip.w, 0.1);
        clip = u_view_proj * w;
    }
    v_world = w.xyz;
    gl_Position = clip;
}
"#;

const MESH_FS: &str = r#"
in vec3 v_world;
in vec3 v_normal;
in vec2 v_uv;
out vec4 frag;
uniform vec3 u_color;
uniform vec3 u_cam;
uniform vec3 u_ambient;
uniform float u_fog;
uniform vec3 u_fog_color;
uniform int u_ndir;
uniform vec3 u_ldir[4];
uniform vec3 u_ldir_color[4];
uniform int u_npoints;
uniform vec3 u_ppos[16];
uniform vec3 u_pcolor[16];
uniform float u_prange[16];
uniform vec3 u_pdir[16];
uniform float u_pcone[16];
uniform float u_shine;
uniform int u_mode; // 0 = lit, 1 = unlit, 2 = flat colour (outline)
uniform sampler2D u_tex;
uniform int u_has_tex;
uniform sampler2DShadow u_shadow;
uniform mat4 u_light_vp;
uniform int u_has_shadow;

vec3 shade(vec3 n, vec3 v, vec3 l, vec3 lc, vec3 base) {
    float ndl = max(dot(n, l), 0.0);
    vec3 h = normalize(l + v);
    float spec = pow(max(dot(n, h), 0.0), mix(8.0, 96.0, u_shine)) * u_shine * step(0.0001, ndl);
    return lc * (base * ndl + vec3(spec));
}

float shadow_factor(vec3 n) {
    if (u_has_shadow == 0) return 1.0;
    vec4 lp = u_light_vp * vec4(v_world + n * 0.06, 1.0);
    vec3 p = lp.xyz / lp.w * 0.5 + 0.5;
    if (p.x <= 0.0 || p.x >= 1.0 || p.y <= 0.0 || p.y >= 1.0 || p.z >= 1.0) return 1.0;
    float texel = 1.0 / float(textureSize(u_shadow, 0).x);
    float sum = 0.0;
    for (int x = -1; x <= 1; x++)
        for (int y = -1; y <= 1; y++)
            sum += texture(u_shadow, vec3(p.xy + vec2(float(x), float(y)) * texel * 1.3, p.z - 0.0015));
    // softens the edge of the covered area
    vec2 e = abs(p.xy * 2.0 - 1.0);
    float fade = smoothstep(0.85, 1.0, max(e.x, e.y));
    return mix(sum / 9.0, 1.0, fade);
}

void main() {
    if (u_mode == 2) { frag = vec4(u_color, 1.0); return; }
    vec3 albedo = u_color;
    if (u_has_tex == 1) {
        vec4 t = texture(u_tex, v_uv);
        if (t.a < 0.3) discard;
        albedo *= t.rgb;
    }
    if (u_mode == 1) { frag = vec4(albedo, 1.0); return; }
    vec3 base = pow(albedo, vec3(2.2));
    vec3 n = normalize(v_normal);
    if (!gl_FrontFacing) n = -n;
    vec3 v = normalize(u_cam - v_world);
    vec3 amb = mix(u_ambient * 0.45, u_ambient, n.y * 0.5 + 0.5);
    vec3 col = base * amb;
    for (int i = 0; i < 4; i++) {
        if (i >= u_ndir) break;
        float s = i == 0 ? shadow_factor(n) : 1.0;
        col += shade(n, v, normalize(-u_ldir[i]), u_ldir_color[i], base) * s;
    }
    for (int i = 0; i < 16; i++) {
        if (i >= u_npoints) break;
        vec3 d = u_ppos[i] - v_world;
        float dist = length(d);
        vec3 l = d / max(dist, 0.0001);
        float att = clamp(1.0 - dist / u_prange[i], 0.0, 1.0);
        att *= att;
        if (u_pcone[i] > -1.5) {
            float c = dot(-l, u_pdir[i]);
            att *= smoothstep(u_pcone[i], mix(u_pcone[i], 1.0, 0.25), c);
        }
        col += shade(n, v, l, u_pcolor[i] * att, base);
    }
    col = col / (col + vec3(0.6)) * 1.6; // soft tone mapping
    col = pow(clamp(col, 0.0, 1.0), vec3(1.0 / 2.2));
    float fog_d = length(u_cam - v_world) * u_fog;
    col = mix(col, u_fog_color, 1.0 - exp(-fog_d * fog_d));
    frag = vec4(col, 1.0);
}
"#;

const DEPTH_VS: &str = r#"
in vec3 a_pos;
in vec4 a_joints;
in vec4 a_weights;
uniform mat4 u_joints[64];
uniform int u_skinned;
mat4 skin_matrix() {
    if (u_skinned == 0) return mat4(1.0);
    return a_weights.x * u_joints[int(a_joints.x)] + a_weights.y * u_joints[int(a_joints.y)]
         + a_weights.z * u_joints[int(a_joints.z)] + a_weights.w * u_joints[int(a_joints.w)];
}
uniform mat4 u_mvp;
void main() { gl_Position = u_mvp * (skin_matrix() * vec4(a_pos, 1.0)); }
"#;

const DEPTH_FS: &str = r#"
void main() {}
"#;

const LINE_VS: &str = r#"
in vec3 a_pos;
in vec4 a_color;
uniform mat4 u_view_proj;
out vec4 v_color;
void main() {
    v_color = a_color;
    gl_Position = u_view_proj * vec4(a_pos, 1.0);
}
"#;

const LINE_FS: &str = r#"
in vec4 v_color;
out vec4 frag;
void main() { frag = v_color; }
"#;

const SKY_VS: &str = r#"
uniform mat4 u_inv_view_proj;
out vec3 v_dir;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)) * 2.0 - 1.0;
    vec4 near = u_inv_view_proj * vec4(p, -1.0, 1.0);
    vec4 far = u_inv_view_proj * vec4(p, 1.0, 1.0);
    v_dir = far.xyz / far.w - near.xyz / near.w;
    gl_Position = vec4(p, 0.9999, 1.0);
}
"#;

const SKY_FS: &str = r#"
in vec3 v_dir;
out vec4 frag;
uniform vec3 u_top;
uniform vec3 u_horizon;
void main() {
    float y = normalize(v_dir).y;
    vec3 ground = u_horizon * 0.35;
    vec3 c = y >= 0.0 ? mix(u_horizon, u_top, pow(y, 0.55))
                      : mix(u_horizon, ground, pow(min(-y * 4.0, 1.0), 0.6));
    frag = vec4(c, 1.0);
}
"#;

fn bytes<T>(v: &[T]) -> &[u8] {
    // Safe for f32/u32: plain types, no padding.
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

unsafe fn compile(gl: &glow::Context, vs: &str, fs: &str, attribs: &[&str]) -> Result<glow::Program, String> {
    let header = "#version 150\n";
    let program = gl.create_program()?;
    let mut shaders = vec![];
    for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
        let sh = gl.create_shader(kind)?;
        gl.shader_source(sh, &format!("{header}{src}"));
        gl.compile_shader(sh);
        if !gl.get_shader_compile_status(sh) {
            return Err(format!("shader error: {}", gl.get_shader_info_log(sh)));
        }
        gl.attach_shader(program, sh);
        shaders.push(sh);
    }
    for (i, name) in attribs.iter().enumerate() {
        gl.bind_attrib_location(program, i as u32, name);
    }
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        return Err(format!("shader link error: {}", gl.get_program_info_log(program)));
    }
    for sh in shaders {
        gl.detach_shader(program, sh);
        gl.delete_shader(sh);
    }
    Ok(program)
}

unsafe fn upload_mesh(
    gl: &glow::Context,
    verts: &[f32],
    idx: &[u32],
) -> Result<(glow::VertexArray, [glow::Buffer; 2], i32), String> {
    let vao = gl.create_vertex_array()?;
    gl.bind_vertex_array(Some(vao));
    let vbo = gl.create_buffer()?;
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(verts), glow::STATIC_DRAW);
    let ebo = gl.create_buffer()?;
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
    gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytes(idx), glow::STATIC_DRAW);
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 32, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 32, 12);
    gl.enable_vertex_attrib_array(2);
    gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, 32, 24);
    Ok((vao, [vbo, ebo], idx.len() as i32))
}

/// Like `upload_mesh`, for vertices that carry joints and weights (16 floats each).
unsafe fn upload_skinned(
    gl: &glow::Context,
    verts: &[f32],
    idx: &[u32],
) -> Result<(glow::VertexArray, [glow::Buffer; 2], i32), String> {
    let vao = gl.create_vertex_array()?;
    gl.bind_vertex_array(Some(vao));
    let vbo = gl.create_buffer()?;
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(verts), glow::STATIC_DRAW);
    let ebo = gl.create_buffer()?;
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
    gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytes(idx), glow::STATIC_DRAW);
    for (i, (size, offset)) in [(3, 0), (3, 12), (2, 24), (4, 32), (4, 48)].into_iter().enumerate() {
        gl.enable_vertex_attrib_array(i as u32);
        gl.vertex_attrib_pointer_f32(i as u32, size, glow::FLOAT, false, 64, offset);
    }
    Ok((vao, [vbo, ebo], idx.len() as i32))
}

impl Renderer {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let mesh_prog = compile(gl, MESH_VS, MESH_FS, &["a_pos", "a_normal", "a_uv", "a_joints", "a_weights"])?;
            let depth_prog = compile(gl, DEPTH_VS, DEPTH_FS, &["a_pos", "a_normal", "a_uv", "a_joints", "a_weights"])?;
            let line_prog = compile(gl, LINE_VS, LINE_FS, &["a_pos", "a_color"])?;
            let sky_prog = compile(gl, SKY_VS, SKY_FS, &[])?;

            let mut shapes = HashMap::new();
            for shape in Shape::ALL {
                let (verts, idx) = build_mesh(shape);
                let (vao, buffers, count) = upload_mesh(gl, &verts, &idx)?;
                shapes.insert(shape, GpuMesh { vao, buffers, count, color: [1.0; 3], texture: None, skin: None });
            }

            let line_vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(line_vao));
            let line_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(line_vbo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 28, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, 28, 12);
            let empty_vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(None);

            // shadow map (if the card does not support it, the engine carries on without shadows)
            let (shadow_fbo, shadow_tex) = match Self::create_shadow_map(gl) {
                Ok((f, t)) => (Some(f), Some(t)),
                Err(_) => (None, None),
            };

            Ok(Renderer {
                mesh_prog,
                depth_prog,
                line_prog,
                sky_prog,
                shapes,
                models: HashMap::new(),
                model_version: HashMap::new(),
                textures: HashMap::new(),
                line_vao,
                line_vbo,
                empty_vao,
                shadow_fbo,
                shadow_tex,
                white_tex: None,
                frame_no: 0,
                model_used: HashMap::new(),
            })
        }
    }

    unsafe fn create_shadow_map(gl: &glow::Context) -> Result<(glow::Framebuffer, glow::Texture), String> {
        let prev = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
        let tex = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::DEPTH_COMPONENT24 as i32,
            SHADOW_SIZE,
            SHADOW_SIZE,
            0,
            glow::DEPTH_COMPONENT,
            glow::FLOAT,
            glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_MODE, glow::COMPARE_REF_TO_TEXTURE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_FUNC, glow::LEQUAL as i32);
        let fbo = gl.create_framebuffer()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(tex), 0);
        gl.draw_buffer(glow::NONE);
        gl.read_buffer(glow::NONE);
        let ok = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev);
        gl.bind_texture(glow::TEXTURE_2D, None);
        if ok {
            Ok((fbo, tex))
        } else {
            Err("incomplete shadow framebuffer".into())
        }
    }

    unsafe fn ensure_texture(&mut self, gl: &glow::Context, t: &Arc<TextureData>) -> Option<glow::Texture> {
        if let Some(tex) = self.textures.get(&t.id) {
            return Some(*tex);
        }
        if t.rgba.len() != (t.width as usize) * (t.height as usize) * 4 || t.width == 0 {
            return None;
        }
        let tex = gl.create_texture().ok()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            t.width as i32,
            t.height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&t.rgba)),
        );
        gl.generate_mipmap(glow::TEXTURE_2D);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        self.textures.insert(t.id, tex);
        Some(tex)
    }

    unsafe fn ensure_model(&mut self, gl: &glow::Context, m: &Arc<ModelData>) {
        if self.models.contains_key(&m.id) {
            if self.model_version.get(&m.id) == Some(&m.version) {
                return;
            }
            // an animated model moved: replace its vertices
            for part in self.models.remove(&m.id).unwrap_or_default() {
                gl.delete_vertex_array(part.vao);
                gl.delete_buffer(part.buffers[0]);
                gl.delete_buffer(part.buffers[1]);
            }
        }
        self.model_version.insert(m.id, m.version);
        let mut parts = vec![];
        for s in &m.subs {
            let uploaded = match (&s.skin, &m.skeleton) {
                (Some(skin), Some(_)) => {
                    // bind-pose vertices plus joints and weights: the shader poses them
                    let mut v = Vec::with_capacity(skin.positions.len() * 16);
                    for i in 0..skin.positions.len() {
                        v.extend_from_slice(&skin.positions[i].to_array());
                        v.extend_from_slice(&skin.normals[i].to_array());
                        v.extend_from_slice(&s.vertices[i * 8 + 6..i * 8 + 8]);
                        v.extend(skin.joints[i].map(|j| (j as usize).min(MAX_JOINTS - 1) as f32));
                        let w = skin.weights[i];
                        let total: f32 = w.iter().sum();
                        v.extend(if total > 1e-6 { w.map(|x| x / total) } else { [1.0, 0.0, 0.0, 0.0] });
                    }
                    upload_skinned(gl, &v, &s.indices).map(|r| (r, Some(skin.skin)))
                }
                _ => upload_mesh(gl, &s.vertices, &s.indices).map(|r| (r, None)),
            };
            if let Ok(((vao, buffers, count), skin)) = uploaded {
                parts.push(GpuMesh { vao, buffers, count, color: s.color, texture: s.texture.clone(), skin });
            }
        }
        self.models.insert(m.id, parts);
    }

    /// Draws the frame. `viewport` = [x, y, width, height] in pixels (already set up by egui).
    pub fn paint(&mut self, gl: &glow::Context, f: &RenderFrame, viewport: [i32; 4]) {
        let view_proj = f.proj * f.view;
        unsafe {
            // uploads to the GPU whatever has not been uploaded yet
            self.frame_no += 1;
            for item in &f.items {
                if let MeshRef::Model(m) = &item.mesh {
                    self.model_used.insert(m.id, self.frame_no);
                    self.ensure_model(gl, m);
                    for s in &m.subs {
                        if let Some(t) = &s.texture {
                            self.ensure_texture(gl, t);
                        }
                    }
                }
                if let Some(t) = &item.texture {
                    self.ensure_texture(gl, t);
                }
            }

            // every few seconds, free models that have not been drawn for a while
            if self.frame_no % 240 == 0 {
                let now = self.frame_no;
                let stale: Vec<u64> =
                    self.model_used.iter().filter(|(_, used)| now - **used > 600).map(|(id, _)| *id).collect();
                for id in stale {
                    self.model_used.remove(&id);
                    self.model_version.remove(&id);
                    for part in self.models.remove(&id).unwrap_or_default() {
                        gl.delete_vertex_array(part.vao);
                        gl.delete_buffer(part.buffers[0]);
                        gl.delete_buffer(part.buffers[1]);
                    }
                }
            }

            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.disable(glow::BLEND);
            gl.front_face(glow::CCW);

            // --- shadow pass ---
            let mut light_vp = Mat4::IDENTITY;
            let mut has_shadow = false;
            if let (Some((center, radius)), Some((dir, _)), Some(fbo)) = (f.shadow, f.dir_lights.first(), self.shadow_fbo) {
                let eye = center - *dir * radius * 2.0;
                light_vp = ortho(radius, radius, 0.1, radius * 4.0) * look_to(eye, *dir, Vec3::Y);
                let prev = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.viewport(0, 0, SHADOW_SIZE, SHADOW_SIZE);
                gl.disable(glow::SCISSOR_TEST);
                gl.clear_depth_f32(1.0);
                gl.clear(glow::DEPTH_BUFFER_BIT);
                gl.use_program(Some(self.depth_prog));
                let u_mvp = gl.get_uniform_location(self.depth_prog, "u_mvp");
                gl.disable(glow::CULL_FACE);
                gl.enable(glow::POLYGON_OFFSET_FILL);
                gl.polygon_offset(2.0, 4.0);
                let d_flag = gl.get_uniform_location(self.depth_prog, "u_skinned");
                let d_mats = gl.get_uniform_location(self.depth_prog, "u_joints[0]");
                gl.uniform_1_i32(d_flag.as_ref(), 0);
                for item in f.items.iter().filter(|i| i.cast_shadow && i.in_shadow) {
                    gl.uniform_matrix_4_f32_slice(u_mvp.as_ref(), false, &(light_vp * item.model).to_cols_array());
                    match &item.mesh {
                        MeshRef::Shape(s) => {
                            if let Some(m) = self.shapes.get(s) {
                                gl.bind_vertex_array(Some(m.vao));
                                gl.draw_elements(glow::TRIANGLES, m.count, glow::UNSIGNED_INT, 0);
                            }
                        }
                        MeshRef::Model(model) => {
                            for m in self.models.get(&model.id).into_iter().flatten() {
                                bind_skin(gl, &d_flag, &d_mats, m, &item.joints);
                                gl.bind_vertex_array(Some(m.vao));
                                gl.draw_elements(glow::TRIANGLES, m.count, glow::UNSIGNED_INT, 0);
                            }
                            gl.uniform_1_i32(d_flag.as_ref(), 0);
                        }
                    }
                }
                gl.disable(glow::POLYGON_OFFSET_FILL);
                gl.bind_framebuffer(glow::FRAMEBUFFER, prev);
                gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
                gl.enable(glow::SCISSOR_TEST);
                has_shadow = true;
            }

            gl.clear_depth_f32(1.0);
            gl.clear(glow::DEPTH_BUFFER_BIT);
            gl.disable(glow::CULL_FACE);
            let loc = |p, name: &str| gl.get_uniform_location(p, name);

            // --- sky ---
            gl.use_program(Some(self.sky_prog));
            gl.uniform_matrix_4_f32_slice(
                loc(self.sky_prog, "u_inv_view_proj").as_ref(),
                false,
                &view_proj.inverse().to_cols_array(),
            );
            gl.uniform_3_f32_slice(loc(self.sky_prog, "u_top").as_ref(), &f.sky_top);
            gl.uniform_3_f32_slice(loc(self.sky_prog, "u_horizon").as_ref(), &f.sky_horizon);
            gl.depth_mask(false);
            gl.bind_vertex_array(Some(self.empty_vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.depth_mask(true);

            // --- meshes ---
            let p = self.mesh_prog;
            gl.use_program(Some(p));
            gl.uniform_matrix_4_f32_slice(loc(p, "u_view_proj").as_ref(), false, &view_proj.to_cols_array());
            gl.uniform_3_f32_slice(loc(p, "u_cam").as_ref(), &f.cam_pos.to_array());
            gl.uniform_3_f32_slice(loc(p, "u_ambient").as_ref(), &f.ambient);
            gl.uniform_1_f32(loc(p, "u_fog").as_ref(), f.fog.clamp(0.0, 1.0));
            gl.uniform_3_f32_slice(loc(p, "u_fog_color").as_ref(), &f.sky_horizon);
            gl.uniform_1_i32(loc(p, "u_ndir").as_ref(), f.dir_lights.len() as i32);
            for (i, (dir, color)) in f.dir_lights.iter().enumerate() {
                gl.uniform_3_f32_slice(loc(p, &format!("u_ldir[{i}]")).as_ref(), &dir.to_array());
                gl.uniform_3_f32_slice(loc(p, &format!("u_ldir_color[{i}]")).as_ref(), color);
            }
            gl.uniform_1_i32(loc(p, "u_npoints").as_ref(), f.locals.len() as i32);
            for (i, l) in f.locals.iter().enumerate() {
                gl.uniform_3_f32_slice(loc(p, &format!("u_ppos[{i}]")).as_ref(), &l.pos.to_array());
                gl.uniform_3_f32_slice(loc(p, &format!("u_pcolor[{i}]")).as_ref(), &l.color);
                gl.uniform_1_f32(loc(p, &format!("u_prange[{i}]")).as_ref(), l.range);
                gl.uniform_3_f32_slice(loc(p, &format!("u_pdir[{i}]")).as_ref(), &l.dir.to_array());
                gl.uniform_1_f32(loc(p, &format!("u_pcone[{i}]")).as_ref(), l.cos_outer);
            }
            gl.uniform_1_i32(loc(p, "u_tex").as_ref(), 0);
            gl.uniform_1_i32(loc(p, "u_shadow").as_ref(), 1);
            gl.uniform_1_i32(loc(p, "u_has_shadow").as_ref(), has_shadow as i32);
            gl.uniform_matrix_4_f32_slice(loc(p, "u_light_vp").as_ref(), false, &light_vp.to_cols_array());
            // unit 1 always needs a valid depth texture
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, self.shadow_tex);
            gl.active_texture(glow::TEXTURE0);

            let u_model = loc(p, "u_model");
            let u_normal = loc(p, "u_normal_mat");
            let u_color = loc(p, "u_color");
            let u_shine = loc(p, "u_shine");
            let u_mode = loc(p, "u_mode");
            let u_outline = loc(p, "u_outline");
            let u_has_tex = loc(p, "u_has_tex");
            if self.white_tex.is_none() {
                if let Ok(t) = gl.create_texture() {
                    gl.bind_texture(glow::TEXTURE_2D, Some(t));
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA8 as i32,
                        1,
                        1,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(Some(&[255, 255, 255, 255])),
                    );
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
                    self.white_tex = Some(t);
                }
            }
            let white = self.white_tex;

            let u_flag = loc(p, "u_skinned");
            let u_mats = loc(p, "u_joints[0]");
            for item in f.items.iter().filter(|i| i.in_view) {
                let parts: &[GpuMesh] = match &item.mesh {
                    MeshRef::Shape(s) => self.shapes.get(s).map(std::slice::from_ref).unwrap_or(&[]),
                    MeshRef::Model(m) => self.models.get(&m.id).map(|v| v.as_slice()).unwrap_or(&[]),
                };
                gl.uniform_matrix_4_f32_slice(u_model.as_ref(), false, &item.model.to_cols_array());
                let nm = Mat3::from_mat4(item.model).inverse().transpose();
                gl.uniform_matrix_3_f32_slice(u_normal.as_ref(), false, &nm.to_cols_array());
                // negative scale flips face winding
                let flipped = item.model.determinant() < 0.0;
                // imported models do not always have consistently oriented faces
                let two_sided = matches!(item.mesh, MeshRef::Shape(Shape::Plane) | MeshRef::Model(_));

                if item.selected {
                    // orange outline: slightly larger inverted hull
                    gl.enable(glow::CULL_FACE);
                    gl.cull_face(if flipped { glow::BACK } else { glow::FRONT });
                    gl.uniform_1_i32(u_mode.as_ref(), 2);
                    gl.uniform_1_f32(u_outline.as_ref(), 0.005);
                    gl.uniform_3_f32(u_color.as_ref(), 1.0, 0.55, 0.05);
                    gl.depth_mask(false);
                    for m in parts {
                        bind_skin(gl, &u_flag, &u_mats, m, &item.joints);
                        gl.bind_vertex_array(Some(m.vao));
                        gl.draw_elements(glow::TRIANGLES, m.count, glow::UNSIGNED_INT, 0);
                    }
                    gl.depth_mask(true);
                }

                if two_sided {
                    gl.disable(glow::CULL_FACE);
                } else {
                    gl.enable(glow::CULL_FACE);
                    gl.cull_face(if flipped { glow::FRONT } else { glow::BACK });
                }
                gl.uniform_1_i32(u_mode.as_ref(), if item.unlit { 1 } else { 0 });
                gl.uniform_1_f32(u_outline.as_ref(), 0.0);
                gl.uniform_1_f32(u_shine.as_ref(), item.shininess.clamp(0.0, 1.0));
                for m in parts {
                    let c = [item.color[0] * m.color[0], item.color[1] * m.color[1], item.color[2] * m.color[2]];
                    gl.uniform_3_f32_slice(u_color.as_ref(), &c);
                    // the texture chosen in the Inspector takes priority over the model's
                    let tex = item
                        .texture
                        .as_ref()
                        .or(m.texture.as_ref())
                        .and_then(|t| self.textures.get(&t.id).copied());
                    gl.uniform_1_i32(u_has_tex.as_ref(), tex.is_some() as i32);
                    gl.bind_texture(glow::TEXTURE_2D, tex.or(white));
                    bind_skin(gl, &u_flag, &u_mats, m, &item.joints);
                    gl.bind_vertex_array(Some(m.vao));
                    gl.draw_elements(glow::TRIANGLES, m.count, glow::UNSIGNED_INT, 0);
                }
            }
            gl.disable(glow::CULL_FACE);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);

            // --- lines (grid, camera gizmos, colliders) ---
            if !f.lines.is_empty() {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                gl.depth_mask(false);
                gl.use_program(Some(self.line_prog));
                gl.uniform_matrix_4_f32_slice(
                    loc(self.line_prog, "u_view_proj").as_ref(),
                    false,
                    &view_proj.to_cols_array(),
                );
                gl.bind_vertex_array(Some(self.line_vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.line_vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(&f.lines), glow::DYNAMIC_DRAW);
                gl.draw_arrays(glow::LINES, 0, (f.lines.len() / 7) as i32);
                gl.depth_mask(true);
            }

            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
        }
    }
}

// ---------------------------------------------------------------------------
// Primitive mesh generation (position + normal + uv, u32 indices)
// ---------------------------------------------------------------------------

struct Builder {
    v: Vec<f32>,
    i: Vec<u32>,
}

impl Builder {
    fn vert(&mut self, p: Vec3, n: Vec3, u: f32, v: f32) -> u32 {
        self.v.extend_from_slice(&[p.x, p.y, p.z, n.x, n.y, n.z, u, v]);
        (self.v.len() / 8 - 1) as u32
    }
    /// Triangle whose winding is fixed automatically to face along the normal.
    fn tri(&mut self, a: u32, b: u32, c: u32) {
        let p = |i: u32| {
            let k = i as usize * 8;
            (
                Vec3::new(self.v[k], self.v[k + 1], self.v[k + 2]),
                Vec3::new(self.v[k + 3], self.v[k + 4], self.v[k + 5]),
            )
        };
        let (pa, na) = p(a);
        let (pb, nb) = p(b);
        let (pc, nc) = p(c);
        let face = (pb - pa).cross(pc - pa);
        if face.dot(na + nb + nc) >= 0.0 {
            self.i.extend_from_slice(&[a, b, c]);
        } else {
            self.i.extend_from_slice(&[a, c, b]);
        }
    }
    fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.tri(a, b, c);
        self.tri(a, c, d);
    }
}

const SEG: usize = 32;

fn ring(i: usize) -> (f32, f32) {
    let a = i as f32 / SEG as f32 * std::f32::consts::TAU;
    (a.cos(), a.sin())
}

fn build_mesh(shape: Shape) -> (Vec<f32>, Vec<u32>) {
    let mut b = Builder { v: vec![], i: vec![] };
    match shape {
        Shape::Cube => {
            for n in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z] {
                let u = if n.y.abs() > 0.5 { Vec3::X } else { Vec3::Y };
                let w = n.cross(u);
                let c = n * 0.5;
                let q = [
                    b.vert(c - u * 0.5 - w * 0.5, n, 0.0, 1.0),
                    b.vert(c + u * 0.5 - w * 0.5, n, 0.0, 0.0),
                    b.vert(c + u * 0.5 + w * 0.5, n, 1.0, 0.0),
                    b.vert(c - u * 0.5 + w * 0.5, n, 1.0, 1.0),
                ];
                b.quad(q[0], q[1], q[2], q[3]);
            }
        }
        Shape::Plane => {
            let q = [
                b.vert(Vec3::new(-0.5, 0.0, -0.5), Vec3::Y, 0.0, 0.0),
                b.vert(Vec3::new(0.5, 0.0, -0.5), Vec3::Y, 1.0, 0.0),
                b.vert(Vec3::new(0.5, 0.0, 0.5), Vec3::Y, 1.0, 1.0),
                b.vert(Vec3::new(-0.5, 0.0, 0.5), Vec3::Y, 0.0, 1.0),
            ];
            b.quad(q[0], q[1], q[2], q[3]);
        }
        Shape::Sphere => sphere_part(&mut b, 0.0, true, true),
        Shape::Capsule => {
            sphere_part(&mut b, 0.5, true, false);
            sphere_part(&mut b, -0.5, false, true);
            side(&mut b, 0.5, 0.5, -0.5, 0.5);
        }
        Shape::Cylinder => {
            side(&mut b, 0.5, 0.5, -0.5, 0.5);
            cap(&mut b, 0.5, Vec3::Y);
            cap(&mut b, -0.5, Vec3::NEG_Y);
        }
        Shape::Cone => {
            side(&mut b, 0.5, 0.0, -0.5, 0.5);
            cap(&mut b, -0.5, Vec3::NEG_Y);
        }
    }
    (b.v, b.i)
}

/// Sphere of radius 0.5 (or only the top/bottom hemisphere), offset in Y.
fn sphere_part(b: &mut Builder, y_off: f32, top: bool, bottom: bool) {
    let stacks = SEG / 2;
    let (s0, s1) = match (top, bottom) {
        (true, true) => (0, stacks),
        (true, false) => (0, stacks / 2),
        _ => (stacks / 2, stacks),
    };
    let mut rows: Vec<Vec<u32>> = vec![];
    for s in s0..=s1 {
        let phi = s as f32 / stacks as f32 * std::f32::consts::PI;
        let mut row = vec![];
        for i in 0..=SEG {
            let (c, sn) = ring(i);
            let n = Vec3::new(phi.sin() * c, phi.cos(), phi.sin() * sn);
            row.push(b.vert(n * 0.5 + Vec3::Y * y_off, n, 1.0 - i as f32 / SEG as f32, s as f32 / stacks as f32));
        }
        rows.push(row);
    }
    for r in 0..rows.len() - 1 {
        for i in 0..SEG {
            b.quad(rows[r][i], rows[r][i + 1], rows[r + 1][i + 1], rows[r + 1][i]);
        }
    }
}

/// Side of a truncated cone between y0 (radius r0) and y1 (radius r1).
fn side(b: &mut Builder, r0: f32, r1: f32, y0: f32, y1: f32) {
    let slope = (r0 - r1) / (y1 - y0);
    let mut lo = vec![];
    let mut hi = vec![];
    for i in 0..=SEG {
        let (c, s) = ring(i);
        let n = Vec3::new(c, slope, s).normalize();
        let u = 1.0 - i as f32 / SEG as f32;
        lo.push(b.vert(Vec3::new(c * r0, y0, s * r0), n, u, 1.0));
        hi.push(b.vert(Vec3::new(c * r1, y1, s * r1), n, u, 0.0));
    }
    for i in 0..SEG {
        if r1 > 0.0 {
            b.quad(lo[i], lo[i + 1], hi[i + 1], hi[i]);
        } else {
            b.tri(lo[i], lo[i + 1], hi[i]);
        }
    }
}

fn cap(b: &mut Builder, y: f32, n: Vec3) {
    let center = b.vert(Vec3::new(0.0, y, 0.0), n, 0.5, 0.5);
    let mut rim = vec![];
    for i in 0..=SEG {
        let (c, s) = ring(i);
        rim.push(b.vert(Vec3::new(c * 0.5, y, s * 0.5), n, 0.5 + c * 0.5, 0.5 + s * 0.5));
    }
    for i in 0..SEG {
        b.tri(center, rim[i], rim[i + 1]);
    }
}
