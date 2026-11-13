//! Post-processing: the scene is drawn into an off-screen HDR image, then ambient
//! occlusion, screen-space reflections and bloom are added, and a filmic tone curve,
//! vignette and dithering produce the final picture.
//!
//! Turned on per scene (Scene settings > Bloom, Ambient occlusion, Reflections).
//! `CRYO_POST=0` turns it off everywhere, for very old graphics cards.

use eframe::glow::{self, HasContext};

pub const QUAD_VS: &str = r#"
out vec2 v_uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

/// Shared by the passes that read depth: rebuilds view-space positions and normals.
const VIEW_FN: &str = r#"
uniform sampler2D u_depth;
uniform mat4 u_inv_proj;
uniform vec2 u_texel;
vec3 view_pos(vec2 uv) {
    float d = texture(u_depth, uv).r;
    vec4 p = u_inv_proj * vec4(uv * 2.0 - 1.0, d * 2.0 - 1.0, 1.0);
    return p.xyz / p.w;
}
vec3 view_normal(vec2 uv, vec3 p) {
    // the neighbour with the smaller depth jump avoids smearing normals across edges
    vec3 r = view_pos(uv + vec2(u_texel.x, 0.0)) - p;
    vec3 l = p - view_pos(uv - vec2(u_texel.x, 0.0));
    vec3 u = view_pos(uv + vec2(0.0, u_texel.y)) - p;
    vec3 d = p - view_pos(uv - vec2(0.0, u_texel.y));
    vec3 dx = abs(r.z) < abs(l.z) ? r : l;
    vec3 dy = abs(u.z) < abs(d.z) ? u : d;
    vec3 n = normalize(cross(dx, dy));
    return dot(n, p) > 0.0 ? -n : n;
}
float hash(vec2 p) { return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }
"#;

/// Screen-space ambient occlusion: how much nearby geometry hides each point from the sky.
pub fn ao_fs() -> String {
    format!(
        "in vec2 v_uv;\nout vec4 frag;\n{VIEW_FN}{}",
        r#"
uniform mat4 u_proj;
uniform float u_radius;
void main() {
    if (texture(u_depth, v_uv).r >= 0.99999) { frag = vec4(1.0); return; }
    vec3 p = view_pos(v_uv);
    vec3 n = view_normal(v_uv, p);
    float a = hash(gl_FragCoord.xy) * 6.2831;
    vec3 rnd = vec3(cos(a), sin(a), 0.0);
    vec3 t = normalize(rnd - n * dot(rnd, n));
    vec3 b = cross(n, t);
    float radius = u_radius * clamp(-p.z * 0.08, 0.35, 1.0);
    float occ = 0.0;
    for (int i = 0; i < 14; i++) {
        float fi = float(i);
        vec3 k = vec3(hash(vec2(fi, 1.7)) * 2.0 - 1.0, hash(vec2(fi, 9.1)) * 2.0 - 1.0, hash(vec2(fi, 4.3)) * 0.9 + 0.1);
        float s = mix(0.12, 1.0, pow(fi / 14.0, 2.0));
        vec3 q = p + normalize(t * k.x + b * k.y + n * k.z) * s * radius;
        vec4 c = u_proj * vec4(q, 1.0);
        vec2 uv = c.xy / c.w * 0.5 + 0.5;
        if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0) continue;
        float sd = view_pos(uv).z;
        float range = smoothstep(0.0, 1.0, radius / max(abs(p.z - sd), 0.0001));
        occ += (sd >= q.z + 0.04 ? 1.0 : 0.0) * range;
    }
    frag = vec4(vec3(1.0 - occ / 14.0), 1.0);
}
"#
    )
}

/// Keeps what is brighter than white, at half resolution: the start of bloom.
pub const BRIGHT_FS: &str = r#"
in vec2 v_uv;
out vec4 frag;
uniform sampler2D u_src;
uniform vec2 u_texel;
void main() {
    vec3 c = vec3(0.0);
    for (int x = -1; x <= 1; x += 2)
        for (int y = -1; y <= 1; y += 2)
            c += texture(u_src, v_uv + vec2(float(x), float(y)) * u_texel * 0.5).rgb;
    c *= 0.25;
    float l = dot(c, vec3(0.2126, 0.7152, 0.0722));
    frag = vec4(c * max(l - 0.85, 0.0) / max(l, 0.0001), 1.0);
}
"#;

/// One direction of a gaussian blur.
pub const BLUR_FS: &str = r#"
in vec2 v_uv;
out vec4 frag;
uniform sampler2D u_src;
uniform vec2 u_step;
void main() {
    float w[5] = float[](0.227, 0.194, 0.122, 0.054, 0.016);
    vec3 c = texture(u_src, v_uv).rgb * w[0];
    for (int i = 1; i < 5; i++) {
        c += texture(u_src, v_uv + u_step * float(i)).rgb * w[i];
        c += texture(u_src, v_uv - u_step * float(i)).rgb * w[i];
    }
    frag = vec4(c, 1.0);
}
"#;

/// Puts it all together and writes the depth back, so editor lines still sit in the scene.
pub fn composite_fs() -> String {
    format!(
        "in vec2 v_uv;\nout vec4 frag;\n{VIEW_FN}{}",
        r#"
uniform sampler2D u_color;
uniform sampler2D u_ao;
uniform sampler2D u_bloom;
uniform mat4 u_proj;
uniform float u_ssao;
uniform float u_bloom_amount;
uniform float u_ssr;
void main() {
    vec4 src = texture(u_color, v_uv);
    vec3 col = src.rgb;
    float depth = texture(u_depth, v_uv).r;
    if (u_ssao > 0.0) {
        float ao = 0.0;
        for (int x = -2; x <= 1; x++)
            for (int y = -2; y <= 1; y++)
                ao += texture(u_ao, v_uv + (vec2(float(x), float(y)) + 0.5) * u_texel).r;
        col *= mix(1.0, ao / 16.0, clamp(u_ssao, 0.0, 1.5));
    }
    if (u_ssr > 0.0 && src.a > 0.02 && depth < 0.99999) {
        // march the reflected ray in view space and look up what the screen shows there
        vec3 p = view_pos(v_uv);
        vec3 n = view_normal(v_uv, p);
        vec3 r = reflect(normalize(p), n);
        float t = 0.2 + hash(gl_FragCoord.xy) * 0.2;
        vec2 hit_uv = vec2(-1.0);
        float travelled = 0.0;
        for (int i = 0; i < 40; i++) {
            vec3 q = p + r * t;
            vec4 c = u_proj * vec4(q, 1.0);
            if (c.w <= 0.0) break;
            vec2 uv = c.xy / c.w * 0.5 + 0.5;
            if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0) break;
            float sd = view_pos(uv).z;
            float gap = sd - q.z;
            if (gap > 0.0 && gap < 0.35 + t * 0.06) {
                // refine between the last two steps
                float lo = t - max(0.2, t * 0.12), hi = t;
                for (int k = 0; k < 5; k++) {
                    float m = (lo + hi) * 0.5;
                    vec3 qm = p + r * m;
                    vec4 cm = u_proj * vec4(qm, 1.0);
                    vec2 um = cm.xy / cm.w * 0.5 + 0.5;
                    if (view_pos(um).z - qm.z > 0.0) { hi = m; uv = um; } else { lo = m; }
                }
                hit_uv = uv;
                travelled = t;
                break;
            }
            t += max(0.2, t * 0.12);
        }
        if (hit_uv.x >= 0.0) {
            vec2 e = smoothstep(0.0, 0.12, hit_uv) * smoothstep(0.0, 0.12, 1.0 - hit_uv);
            float fade = e.x * e.y * (1.0 - smoothstep(18.0, 40.0, travelled));
            col = mix(col, texture(u_color, hit_uv).rgb, clamp(src.a * u_ssr * fade, 0.0, 1.0));
        }
    }
    col += texture(u_bloom, v_uv).rgb * u_bloom_amount;
    // filmic tone curve, then gamma
    col *= 1.45;
    col = clamp((col * (2.51 * col + 0.03)) / (col * (2.43 * col + 0.59) + 0.14), 0.0, 1.0);
    col = pow(col, vec3(1.0 / 2.2));
    // a soft vignette, and a trace of noise that hides colour banding in dark gradients
    vec2 c = v_uv - 0.5;
    col *= 1.0 - dot(c, c) * 0.55;
    col += (hash(gl_FragCoord.xy + 0.37) - 0.5) / 255.0;
    frag = vec4(col, 1.0);
    gl_FragDepth = depth;
}
"#
    )
}

/// Off-screen images for one viewport size.
pub struct Targets {
    pub w: i32,
    pub h: i32,
    /// Multisampled scene buffer (smooth edges); resolved into `scene_fbo`.
    pub msaa_fbo: Option<glow::Framebuffer>,
    msaa_bufs: Vec<glow::Renderbuffer>,
    pub scene_fbo: glow::Framebuffer,
    pub color: glow::Texture,
    pub depth: glow::Texture,
    pub ao_fbo: glow::Framebuffer,
    pub ao: glow::Texture,
    pub bloom_fbo: [glow::Framebuffer; 2],
    pub bloom: [glow::Texture; 2],
    pub last_used: u64,
}

unsafe fn texture(gl: &glow::Context, w: i32, h: i32, internal: u32, format: u32, ty: u32, linear: bool) -> Result<glow::Texture, String> {
    let t = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(t));
    gl.tex_image_2d(glow::TEXTURE_2D, 0, internal as i32, w, h, 0, format, ty, glow::PixelUnpackData::Slice(None));
    let filter = if linear { glow::LINEAR } else { glow::NEAREST } as i32;
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    Ok(t)
}

unsafe fn fbo(gl: &glow::Context, color: Option<glow::Texture>, depth: Option<glow::Texture>) -> Result<glow::Framebuffer, String> {
    let f = gl.create_framebuffer()?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
    if let Some(c) = color {
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(c), 0);
    }
    if let Some(d) = depth {
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(d), 0);
    }
    if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
        return Err("incomplete post-processing framebuffer".into());
    }
    Ok(f)
}

impl Targets {
    pub unsafe fn new(gl: &glow::Context, w: i32, h: i32, samples: i32) -> Result<Targets, String> {
        let prev = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
        let result = (|| {
            let color = texture(gl, w, h, glow::RGBA16F, glow::RGBA, glow::HALF_FLOAT, true)?;
            let depth = texture(gl, w, h, glow::DEPTH_COMPONENT24, glow::DEPTH_COMPONENT, glow::FLOAT, false)?;
            let scene_fbo = fbo(gl, Some(color), Some(depth))?;
            let ao = texture(gl, w, h, glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, true)?;
            let ao_fbo = fbo(gl, Some(ao), None)?;
            let (bw, bh) = ((w / 2).max(1), (h / 2).max(1));
            let b0 = texture(gl, bw, bh, glow::RGBA16F, glow::RGBA, glow::HALF_FLOAT, true)?;
            let b1 = texture(gl, bw, bh, glow::RGBA16F, glow::RGBA, glow::HALF_FLOAT, true)?;
            let bloom_fbo = [fbo(gl, Some(b0), None)?, fbo(gl, Some(b1), None)?];
            // multisampling, when the card has it
            let (mut msaa_fbo, mut msaa_bufs) = (None, vec![]);
            if samples > 1 {
                let c = gl.create_renderbuffer()?;
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(c));
                gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::RGBA16F, w, h);
                let d = gl.create_renderbuffer()?;
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(d));
                gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::DEPTH_COMPONENT24, w, h);
                let f = gl.create_framebuffer()?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(c));
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(d));
                if gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE {
                    msaa_fbo = Some(f);
                    msaa_bufs = vec![c, d];
                } else {
                    gl.delete_framebuffer(f);
                    gl.delete_renderbuffer(c);
                    gl.delete_renderbuffer(d);
                }
                gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            }
            Ok(Targets { w, h, msaa_fbo, msaa_bufs, scene_fbo, color, depth, ao_fbo, ao, bloom_fbo, bloom: [b0, b1], last_used: 0 })
        })();
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev);
        gl.bind_texture(glow::TEXTURE_2D, None);
        result
    }

    pub unsafe fn delete(self, gl: &glow::Context) {
        for f in [Some(self.scene_fbo), Some(self.ao_fbo), Some(self.bloom_fbo[0]), Some(self.bloom_fbo[1]), self.msaa_fbo].into_iter().flatten() {
            gl.delete_framebuffer(f);
        }
        for t in [self.color, self.depth, self.ao, self.bloom[0], self.bloom[1]] {
            gl.delete_texture(t);
        }
        for r in self.msaa_bufs {
            gl.delete_renderbuffer(r);
        }
    }
}

pub struct Post {
    pub ao_prog: glow::Program,
    pub bright_prog: glow::Program,
    pub blur_prog: glow::Program,
    pub composite_prog: glow::Program,
    pub targets: Vec<Targets>,
    pub samples: i32,
}

impl Post {
    /// The images for a viewport of this size, made on first use. Sizes not used for a
    /// while are freed (resizing a panel would otherwise pile them up).
    pub unsafe fn targets(&mut self, gl: &glow::Context, w: i32, h: i32, frame: u64) -> Option<usize> {
        if let Some(i) = self.targets.iter().position(|t| t.w == w && t.h == h) {
            self.targets[i].last_used = frame;
            return Some(i);
        }
        let mut i = 0;
        while i < self.targets.len() {
            if frame.saturating_sub(self.targets[i].last_used) > 120 {
                self.targets.swap_remove(i).delete(gl);
            } else {
                i += 1;
            }
        }
        let mut t = Targets::new(gl, w, h, self.samples).ok()?;
        t.last_used = frame;
        self.targets.push(t);
        Some(self.targets.len() - 1)
    }
}
