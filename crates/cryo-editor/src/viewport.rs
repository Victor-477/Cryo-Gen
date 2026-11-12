//! 3D viewports: Scene tab (editor camera, selection, gizmos) and Game tab.

use crate::app::{State, ACCENT};
use crate::render::{ortho, RenderFrame};
use crate::scene::LightKind;
use eframe::egui::{self, Align2, Color32, FontId, Key, PointerButton, Pos2, Rect, Sense, Stroke, UiBuilder};
use eframe::egui_glow;
use glam::{EulerRot, Mat4, Quat, Vec3};
use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Move,
    Rotate,
    Scale,
}

pub struct GizmoDrag {
    pub axis: usize,
}

/// A camera pose: point of interest + angles + distance.
#[derive(Clone, Copy)]
pub struct Pose {
    pub pivot: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub ortho: bool,
}

struct Anim {
    from: Pose,
    to: Pose,
    t: f32,
}

/// Editor camera: free flight, orbit, orthographic views, bookmarks and smooth transitions.
pub struct EditorCamera {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// Distance to the point of interest (orbit pivot).
    pub dist: f32,
    /// Vertical field of view, in degrees.
    pub fov: f32,
    pub ortho: bool,
    /// Flight speed (m/s).
    pub speed: f32,
    /// Trackpad gestures: two fingers orbit, Shift pans, pinch zooms.
    pub trackpad: bool,
    /// Smooth acceleration while flying.
    pub smooth: bool,
    pub bookmarks: [Option<Pose>; 4],
    vel: Vec3,
    anim: Option<Anim>,
    /// Time left showing the speed notice.
    speed_flash: f32,
    pub view_name: String,
}

impl Default for EditorCamera {
    fn default() -> Self {
        let mut c = EditorCamera {
            pos: Vec3::ZERO,
            yaw: -0.6,
            pitch: -0.45,
            dist: 16.0,
            fov: 60.0,
            ortho: false,
            speed: 8.0,
            trackpad: cfg!(target_os = "macos"),
            smooth: true,
            bookmarks: [None; 4],
            vel: Vec3::ZERO,
            anim: None,
            speed_flash: 0.0,
            view_name: String::new(),
        };
        c.pos = Vec3::new(0.0, 0.5, 0.0) - c.forward() * c.dist;
        c
    }
}

impl EditorCamera {
    pub fn rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }
    pub fn forward(&self) -> Vec3 {
        self.rotation() * Vec3::NEG_Z
    }
    pub fn right(&self) -> Vec3 {
        self.rotation() * Vec3::X
    }
    pub fn up(&self) -> Vec3 {
        self.rotation() * Vec3::Y
    }
    pub fn pivot(&self) -> Vec3 {
        self.pos + self.forward() * self.dist
    }
    pub fn view(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.rotation(), self.pos).inverse()
    }
    fn half_height(&self) -> f32 {
        self.dist * (self.fov.to_radians() * 0.5).tan()
    }
    pub fn projection(&self, aspect: f32) -> Mat4 {
        if self.ortho {
            let hh = self.half_height();
            ortho(hh * aspect, hh, -2000.0, 2000.0)
        } else {
            perspective(self.fov.to_radians(), aspect, 0.05, 4000.0)
        }
    }

    pub fn pose(&self) -> Pose {
        Pose { pivot: self.pivot(), yaw: self.yaw, pitch: self.pitch, dist: self.dist, ortho: self.ortho }
    }
    pub fn set_pose(&mut self, p: Pose) {
        self.yaw = p.yaw;
        self.pitch = p.pitch;
        self.dist = p.dist;
        self.ortho = p.ortho;
        self.pos = p.pivot - self.forward() * self.dist;
    }
    /// Moves smoothly to another camera pose.
    pub fn animate_to(&mut self, to: Pose) {
        self.vel = Vec3::ZERO;
        self.anim = Some(Anim { from: self.pose(), to, t: 0.0 });
    }
    fn cancel_anim(&mut self) {
        self.anim = None;
    }

    fn look(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.005;
        self.pitch = (self.pitch - dy * 0.005).clamp(-FRAC_PI_2, FRAC_PI_2);
        self.view_name.clear();
    }
    fn orbit(&mut self, dx: f32, dy: f32) {
        let pivot = self.pivot();
        self.look(dx, dy);
        self.pos = pivot - self.forward() * self.dist;
    }
    /// Pans the camera in the screen plane; `per_px` = metres per pixel at the pivot distance.
    fn pan(&mut self, dx: f32, dy: f32, per_px: f32) {
        self.pos += (-self.right() * dx + self.up() * dy) * per_px;
    }
    /// Zooms in/out keeping the pivot (factor < 1 zooms in).
    fn zoom(&mut self, factor: f32) {
        let pivot = self.pivot();
        self.dist = (self.dist * factor).clamp(0.2, 2000.0);
        self.pos = pivot - self.forward() * self.dist;
    }

    /// Axis-aligned view: the camera looks along `-axis` (from the `+axis` side).
    pub fn view_from(&mut self, axis: Vec3, name: impl Into<String>) {
        let f = -axis;
        let pitch = f.y.clamp(-1.0, 1.0).asin();
        // top/bottom: aligned with the axes (X to the right)
        let yaw = if f.x.abs() + f.z.abs() < 1e-4 { 0.0 } else { (-f.x).atan2(-f.z) };
        self.animate_to(Pose { pivot: self.pivot(), yaw, pitch, dist: self.dist, ortho: true });
        self.view_name = name.into();
    }

    /// Frames a sphere (centre, radius) keeping the current direction.
    pub fn frame(&mut self, center: Vec3, radius: f32) {
        let dist = (radius / (self.fov.to_radians() * 0.5).tan().max(0.05) * 1.25 + 0.5).clamp(1.0, 1500.0);
        self.animate_to(Pose { pivot: center, yaw: self.yaw, pitch: self.pitch, dist, ortho: self.ortho });
    }

    fn update_anim(&mut self, dt: f32) -> bool {
        let Some(a) = &mut self.anim else { return false };
        a.t = (a.t + dt / 0.22).min(1.0);
        let k = a.t * a.t * (3.0 - 2.0 * a.t);
        let (from, to, done) = (a.from, a.to, a.t >= 1.0);
        // shortest angular path
        let mut dy = (to.yaw - from.yaw) % std::f32::consts::TAU;
        if dy > std::f32::consts::PI {
            dy -= std::f32::consts::TAU;
        } else if dy < -std::f32::consts::PI {
            dy += std::f32::consts::TAU;
        }
        self.set_pose(Pose {
            pivot: from.pivot.lerp(to.pivot, k),
            yaw: from.yaw + dy * k,
            pitch: from.pitch + (to.pitch - from.pitch) * k,
            dist: from.dist + (to.dist - from.dist) * k,
            // perspective during the transition; orthographic only on arrival
            ortho: if done { to.ortho } else { from.ortho && to.ortho },
        });
        if done {
            self.anim = None;
        }
        true
    }
}

/// Perspective projection, OpenGL convention (depth from -1 to 1).
pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols_array(&[
        f / aspect, 0.0, 0.0, 0.0,
        0.0, f, 0.0, 0.0,
        0.0, 0.0, (far + near) / (near - far), -1.0,
        0.0, 0.0, 2.0 * far * near / (near - far), 0.0,
    ])
}

pub(crate) fn project(vp: Mat4, rect: Rect, p: Vec3) -> Option<Pos2> {
    let clip = vp * p.extend(1.0);
    if clip.w <= 0.02 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    Some(Pos2::new(
        rect.center().x + ndc.x * rect.width() * 0.5,
        rect.center().y - ndc.y * rect.height() * 0.5,
    ))
}

fn screen_ray(vp: Mat4, rect: Rect, p: Pos2) -> (Vec3, Vec3) {
    let x = (p.x - rect.center().x) / (rect.width() * 0.5);
    let y = -(p.y - rect.center().y) / (rect.height() * 0.5);
    let inv = vp.inverse();
    let near = inv.project_point3(Vec3::new(x, y, -1.0));
    let far = inv.project_point3(Vec3::new(x, y, 1.0));
    (near, (far - near).normalize_or_zero())
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
    (p - (a + ab * t)).length()
}

/// Ray × axis-aligned box intersection (in local space). Returns the entry `t`.
/// Accepts negative `t` (needed in orthographic views, whose ray starts behind the scene).
fn ray_box(o: Vec3, d: Vec3, half: Vec3) -> Option<f32> {
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;
    for i in 0..3 {
        if d[i].abs() < 1e-8 {
            if o[i].abs() > half[i] {
                return None;
            }
        } else {
            let t1 = (-half[i] - o[i]) / d[i];
            let t2 = (half[i] - o[i]) / d[i];
            tmin = tmin.max(t1.min(t2));
            tmax = tmax.min(t1.max(t2));
        }
    }
    (tmax >= tmin).then_some(tmin)
}

const AXES: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];
const AXIS_COLORS: [Color32; 3] = [
    Color32::from_rgb(235, 70, 70),
    Color32::from_rgb(110, 210, 70),
    Color32::from_rgb(70, 130, 245),
];

impl State {
    /// Entity under the ray. `min_t` discards whatever is behind the camera (perspective).
    fn pick(&self, origin: Vec3, dir: Vec3, min_t: f32) -> Option<u64> {
        let mut best: Option<(f32, u64)> = None;
        for e in &self.scene.entities {
            if !self.scene.is_visible(e.id) {
                continue;
            }
            let world = self.scene.world_matrix(e.id);
            let t = match &e.mesh {
                Some(m) => {
                    if world.determinant().abs() < 1e-9 {
                        continue;
                    }
                    let inv = world.inverse();
                    let (center, half) = m.local_bounds();
                    // without normalising the direction, `t` stays valid in world space
                    ray_box(inv.transform_point3(origin) - center, inv.transform_vector3(dir), half)
                }
                None => {
                    // lights, cameras and empties: small selection sphere
                    let c = world.w_axis.truncate();
                    let oc = origin - c;
                    let b = oc.dot(dir);
                    let disc = b * b - (oc.length_squared() - 0.35 * 0.35);
                    (disc >= 0.0).then(|| -b - disc.sqrt())
                }
            };
            if let Some(t) = t.filter(|t| *t >= min_t) {
                if best.map(|(bt, _)| t < bt).unwrap_or(true) {
                    best = Some((t, e.id));
                }
            }
        }
        best.map(|(_, id)| id)
    }

    fn paint_3d(&self, ui: &egui::Ui, rect: Rect, frame: RenderFrame) {
        let Some(renderer) = self.renderer.clone() else {
            ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(20, 20, 22));
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                self.render_error.clone().unwrap_or(tr!("Renderer unavailable", "Renderizador indisponível")),
                FontId::proportional(14.0),
                Color32::LIGHT_RED,
            );
            return;
        };
        let frame = Arc::new(frame);
        let cb = egui_glow::CallbackFn::new(move |info, painter| {
            let vp = info.viewport_in_pixels();
            if let Ok(mut r) = renderer.lock() {
                r.paint(painter.gl(), &frame, [vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px]);
            }
        });
        ui.painter().add(egui::PaintCallback { rect, callback: Arc::new(cb) });
    }

    /// Approximate world-space centre and radius of an entity.
    fn entity_sphere(&self, id: u64) -> (Vec3, f32) {
        let world = self.scene.world_matrix(id);
        let (scale, _, pos) = world.to_scale_rotation_translation();
        match self.scene.get(id).and_then(|e| e.mesh.as_ref()) {
            Some(m) => {
                let (c, h) = m.local_bounds();
                (world.transform_point3(c), (h * scale.abs()).length().clamp(0.3, 500.0))
            }
            None => (pos, 1.0),
        }
    }

    pub fn focus_selected(&mut self) {
        match self.selected.filter(|id| self.scene.get(*id).is_some()) {
            Some(id) => {
                let (c, r) = self.entity_sphere(id);
                self.cam.frame(c, r);
            }
            None => self.frame_all(),
        }
    }

    pub fn frame_all(&mut self) {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for e in &self.scene.entities {
            let (c, r) = self.entity_sphere(e.id);
            let r = r.min(30.0);
            min = min.min(c - r);
            max = max.max(c + r);
        }
        if min.is_finite() && max.is_finite() {
            self.cam.frame((min + max) * 0.5, ((max - min) * 0.5).length().max(1.0));
        }
    }

    /// Moves the game camera (the selected one, or the main one) to the editor's current view.
    pub fn align_game_camera_to_view(&mut self) {
        let target = self
            .selected
            .filter(|id| self.scene.get(*id).map(|e| e.camera.is_some()).unwrap_or(false))
            .or(self.scene.main_camera());
        match target {
            Some(id) => {
                self.scene.set_world_pose(id, self.cam.pos, self.cam.rotation());
                let name = self.scene.get(id).map(|e| e.name.clone()).unwrap_or_default();
                self.info(tr!("Camera \"{name}\" aligned to the editor view.", "Câmera \"{name}\" alinhada à vista do editor."));
            }
            None => self.info(tr!("There is no camera in the scene to align.", "Não há câmera na cena para alinhar.")),
        }
    }

    /// Takes the editor view to the selected object's position/rotation.
    pub fn align_view_to_selected(&mut self) {
        let Some(id) = self.selected.filter(|id| self.scene.get(*id).is_some()) else { return };
        let (_, rot, pos) = self.scene.world_matrix(id).to_scale_rotation_translation();
        let (yaw, pitch, _) = rot.to_euler(EulerRot::YXZ);
        let dist = self.cam.dist;
        let forward = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * Vec3::NEG_Z;
        self.cam.animate_to(Pose { pivot: pos + forward * dist, yaw, pitch, dist, ortho: false });
        self.cam.view_name.clear();
    }

    fn camera_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(250.0);
        ui.label(egui::RichText::new(tr!("Projection", "Projeção")).strong());
        ui.horizontal(|ui| {
            if ui.selectable_label(!self.cam.ortho, tr!("Perspective", "Perspectiva")).clicked() {
                self.cam.ortho = false;
                self.cam.view_name.clear();
            }
            if ui.selectable_label(self.cam.ortho, tr!("Orthographic", "Ortográfica")).on_hover_text(tr!("O key", "Tecla O")).clicked() {
                self.cam.ortho = true;
            }
        });
        ui.add(egui::Slider::new(&mut self.cam.fov, 20.0..=110.0).suffix("°").text(tr!("Field of view", "Campo de visão")));

        ui.separator();
        ui.label(egui::RichText::new(tr!("Views", "Vistas")).strong());
        egui::Grid::new("vistas").num_columns(3).show(ui, |ui| {
            for (i, (label, axis)) in [
                (tr!("Front", "Frente"), Vec3::Z),
                (tr!("Right", "Direita"), Vec3::X),
                (tr!("Top", "Topo"), Vec3::Y),
                (tr!("Back", "Trás"), Vec3::NEG_Z),
                (tr!("Left", "Esquerda"), Vec3::NEG_X),
                (tr!("Bottom", "Base"), Vec3::NEG_Y),
            ]
            .into_iter()
            .enumerate()
            {
                if ui.button(&label).clicked() {
                    self.cam.view_from(axis, label);
                }
                if i == 2 {
                    ui.end_row();
                }
            }
        });

        ui.separator();
        ui.label(egui::RichText::new(tr!("Navigation", "Navegação")).strong());
        ui.add(egui::Slider::new(&mut self.cam.speed, 0.5..=100.0).logarithmic(true).suffix(" m/s").text(tr!("Speed", "Velocidade")))
            .on_hover_text(tr!("Also changes with the mouse wheel while you fly (right button)", "Também muda com a roda do mouse enquanto você voa (botão direito)"));
        ui.checkbox(&mut self.cam.smooth, tr!("Smooth movement", "Movimento suave"));
        ui.checkbox(&mut self.cam.trackpad, tr!("Trackpad gestures", "Gestos de trackpad"))
            .on_hover_text(tr!("Two fingers orbit · Shift + two fingers pan · pinch zooms", "Dois dedos orbitam · Shift + dois dedos desloca · pinça aproxima"));
        ui.checkbox(&mut self.show_grid, tr!("Show grid", "Mostrar grade"));

        ui.separator();
        if ui.button(tr!("Frame selection              F", "Enquadrar seleção            F")).clicked() {
            self.focus_selected();
            ui.close();
        }
        if ui.button(tr!("Frame all                 Home", "Enquadrar tudo            Home")).clicked() {
            self.frame_all();
            ui.close();
        }
        if ui.button(tr!("Align view to selected object", "Alinhar vista ao objeto selecionado")).clicked() {
            self.align_view_to_selected();
            ui.close();
        }
        if ui
            .button(tr!("Align game camera to view", "Alinhar câmera do jogo à vista"))
            .on_hover_text(tr!("Ctrl+Shift+F — puts the game camera exactly where you are looking", "Ctrl+Shift+F — posiciona a câmera do jogo exatamente onde você está olhando"))
            .clicked()
        {
            self.align_game_camera_to_view();
            ui.close();
        }

        ui.separator();
        ui.label(egui::RichText::new(tr!("Camera bookmarks", "Marcadores de câmera")).strong());
        ui.horizontal(|ui| {
            for i in 0..4 {
                let has = self.cam.bookmarks[i].is_some();
                let resp = ui
                    .add_enabled(has, egui::Button::new(tr!("Go {}", "Ir {}", i + 1)))
                    .on_hover_text(tr!("Key {}", "Tecla {}", i + 1));
                if resp.clicked() {
                    if let Some(p) = self.cam.bookmarks[i] {
                        self.cam.animate_to(p);
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            for i in 0..4 {
                if ui.small_button(tr!("Save {}", "Salvar {}", i + 1)).on_hover_text(format!("Ctrl+{}", i + 1)).clicked() {
                    self.cam.bookmarks[i] = Some(self.cam.pose());
                }
            }
        });
    }

    /// Editor camera navigation. Returns `true` while the user is flying.
    fn camera_input(&mut self, ui: &egui::Ui, resp: &egui::Response, rect: Rect) -> bool {
        let ctx = ui.ctx().clone();
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let (alt, shift, cmd) = ctx.input(|i| (i.modifiers.alt, i.modifiers.shift, i.modifiers.command));
        let typing = ctx.egui_wants_keyboard_input();
        let delta = resp.drag_delta();
        let per_px = 2.0 * self.cam.half_height() / rect.height().max(1.0);

        if self.cam.update_anim(dt) {
            ctx.request_repaint();
        }
        let dragging = resp.dragged_by(PointerButton::Secondary)
            || resp.dragged_by(PointerButton::Middle)
            || (resp.dragged_by(PointerButton::Primary) && alt);
        if dragging {
            self.cam.cancel_anim();
        }

        // --- dragging ---
        if resp.dragged_by(PointerButton::Secondary) {
            if alt {
                // Alt + right button: dolly in/out
                self.cam.zoom((-(delta.x - delta.y) * 0.006).exp());
            } else {
                self.cam.look(delta.x, delta.y);
            }
        }
        if resp.dragged_by(PointerButton::Middle) {
            self.cam.pan(delta.x, delta.y, per_px);
        }
        if resp.dragged_by(PointerButton::Primary) && alt {
            self.cam.orbit(delta.x, delta.y);
        }

        // --- flight (right button held) ---
        let flying = !alt
            && (resp.dragged_by(PointerButton::Secondary)
                || (resp.hovered() && ctx.input(|i| i.pointer.secondary_down())));
        let mut wish = Vec3::ZERO;
        if !typing && (flying || resp.hovered()) && !cmd {
            let (mv, arrows, fast) = ctx.input(|i| {
                let k = |key: Key| i.key_down(key) as i32 as f32;
                (
                    Vec3::new(k(Key::D) - k(Key::A), k(Key::E) - k(Key::Q), k(Key::W) - k(Key::S)),
                    Vec3::new(
                        k(Key::ArrowRight) - k(Key::ArrowLeft),
                        k(Key::PageUp) - k(Key::PageDown),
                        k(Key::ArrowUp) - k(Key::ArrowDown),
                    ),
                    i.modifiers.shift,
                )
            });
            // WASD only while flying (otherwise W/E/R switch tools); arrow keys always work
            let dir = if flying { mv + arrows } else { arrows };
            if dir != Vec3::ZERO {
                let d = dir.normalize();
                wish = (self.cam.right() * d.x + Vec3::Y * d.y + self.cam.forward() * d.z)
                    * self.cam.speed
                    * if fast { 3.0 } else { 1.0 };
                self.cam.cancel_anim();
            }
        }
        if self.cam.smooth {
            let k = 1.0 - (-14.0 * dt).exp();
            self.cam.vel += (wish - self.cam.vel) * k;
        } else {
            self.cam.vel = wish;
        }
        if self.cam.vel.length_squared() > 1e-5 {
            self.cam.pos += self.cam.vel * dt;
            ctx.request_repaint();
        } else {
            self.cam.vel = Vec3::ZERO;
        }

        // --- wheel / trackpad / pinch ---
        if resp.hovered() {
            let (scroll, pinch) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if pinch != 1.0 {
                self.cam.cancel_anim();
                self.cam.zoom(1.0 / pinch);
            }
            if scroll != egui::Vec2::ZERO {
                self.cam.cancel_anim();
                if flying {
                    // as in Unreal: the wheel adjusts flight speed
                    self.cam.speed = (self.cam.speed * (scroll.y * 0.004).exp()).clamp(0.5, 100.0);
                    self.cam.speed_flash = 1.2;
                } else if self.cam.trackpad {
                    if shift {
                        self.cam.pan(scroll.x, scroll.y, per_px);
                    } else {
                        self.cam.orbit(scroll.x * 0.7, scroll.y * 0.7);
                    }
                } else {
                    self.cam.zoom((-scroll.y * 0.0025).exp());
                }
            }
        }
        if self.cam.speed_flash > 0.0 {
            self.cam.speed_flash -= dt;
            ctx.request_repaint();
        }

        // --- shortcuts ---
        if resp.hovered() && !typing && !flying {
            let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));
            if pressed(Key::F) {
                if cmd && shift {
                    self.align_game_camera_to_view();
                } else if !cmd {
                    self.focus_selected();
                }
            }
            if pressed(Key::Home) {
                self.frame_all();
            }
            if pressed(Key::O) && !cmd {
                self.cam.ortho = !self.cam.ortho;
                if !self.cam.ortho {
                    self.cam.view_name.clear();
                }
            }
            for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4].into_iter().enumerate() {
                if pressed(key) {
                    if cmd {
                        self.cam.bookmarks[i] = Some(self.cam.pose());
                        self.info(tr!("Camera bookmark {} saved.", "Marcador de câmera {} salvo.", i + 1));
                    } else if let Some(p) = self.cam.bookmarks[i] {
                        self.cam.animate_to(p);
                    }
                }
            }
        }
        flying
    }

    pub fn scene_view(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        if rect.width() < 4.0 || rect.height() < 4.0 {
            return;
        }
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let ctx = ui.ctx().clone();
        let alt = ctx.input(|i| i.modifiers.alt);
        let delta = resp.drag_delta();

        self.camera_input(ui, &resp, rect);

        let aspect = rect.width() / rect.height();
        let view = self.cam.view();
        let proj = self.cam.projection(aspect);
        let vp = proj * view;
        let cam_rot_inv = self.cam.rotation().inverse();

        // ---------- orientation gizmo (top-right corner) ----------
        let widget_c = rect.right_top() + egui::vec2(-52.0, 56.0);
        let widget_r = 30.0;
        let pointer = resp.hover_pos().or(resp.interact_pointer_pos());
        let mut ends: Vec<(Vec3, Pos2, usize, bool)> = vec![];
        for (i, a) in AXES.iter().enumerate() {
            for positive in [true, false] {
                let dir = if positive { *a } else { -*a };
                let v = cam_rot_inv * dir;
                ends.push((v, widget_c + egui::vec2(v.x, -v.y) * widget_r, i, positive));
            }
        }
        ends.sort_by(|a, b| a.0.z.partial_cmp(&b.0.z).unwrap_or(std::cmp::Ordering::Equal));
        let widget_hover = pointer.and_then(|p| {
            ends.iter().rev().find(|(_, pos, _, positive)| (p - *pos).length() < if *positive { 10.0 } else { 7.0 }).map(|e| (e.2, e.3))
        });
        let over_widget = pointer.map(|p| (p - widget_c).length() < widget_r + 14.0).unwrap_or(false);
        if resp.clicked() {
            if let Some((i, positive)) = widget_hover {
                let names = [[tr!("Right", "Direita"), tr!("Left", "Esquerda")], [tr!("Top", "Topo"), tr!("Bottom", "Base")], [tr!("Front", "Frente"), tr!("Back", "Trás")]];
                let axis = if positive { AXES[i] } else { -AXES[i] };
                self.cam.view_from(axis, names[i][!positive as usize].clone());
            }
        }

        // ---------- gizmo ----------
        let mut handles: Option<(Vec3, f32, [Option<(Pos2, Pos2)>; 3])> = None;
        let mut hovered_axis: Option<usize> = None;
        // while editing a mesh the gizmo sits on the selected vertices/faces instead of the entity
        let editing = self.mesh_edit.is_some();
        let gizmo_origin = if editing {
            self.mesh_gizmo_origin()
        } else {
            self.selected.filter(|id| self.scene.get(*id).is_some()).map(|id| self.scene.world_matrix(id).w_axis.truncate())
        };
        if let Some(origin) = gizmo_origin {
            let scale_ref = if self.cam.ortho { self.cam.dist } else { (origin - self.cam.pos).length() };
            let size = scale_ref * (self.cam.fov.to_radians() * 0.5).tan() * 0.28;
            let mut segs = [None; 3];
            if let Some(o2) = project(vp, rect, origin) {
                for i in 0..3 {
                    if let Some(t2) = project(vp, rect, origin + AXES[i] * size) {
                        segs[i] = Some((o2, t2));
                    }
                }
            }
            if let (Some(p), None, false) = (pointer, &self.drag, over_widget) {
                let mut best = 10.0;
                for i in 0..3 {
                    if let Some((a, b)) = segs[i] {
                        // ignores the stretch next to the centre so the axes do not fight
                        let a2 = a + (b - a) * 0.15;
                        let d = dist_to_segment(p, a2, b);
                        if d < best {
                            best = d;
                            hovered_axis = Some(i);
                        }
                    }
                }
            }
            handles = Some((origin, size, segs));
        }
        if let Some(d) = &self.drag {
            hovered_axis = Some(d.axis);
        }

        if resp.drag_started_by(PointerButton::Primary) && !alt {
            self.drag = hovered_axis.map(|axis| GizmoDrag { axis });
        }
        if editing && resp.drag_started_by(PointerButton::Primary) && self.drag.is_some() {
            self.mesh_drag_start();
        }
        if editing && resp.dragged_by(PointerButton::Primary) {
            if let (Some(drag), Some((_, size, segs))) = (&self.drag, &handles) {
                if let Some((a, b)) = segs[drag.axis] {
                    let dir = b - a;
                    let len = dir.length();
                    if len > 1.0 {
                        let amount = delta.dot(dir / len) / len * size;
                        let (axis, size) = (AXES[drag.axis], *size);
                        self.mesh_drag(axis, amount, size);
                    }
                }
            }
        } else if resp.dragged_by(PointerButton::Primary) {
            if let (Some(drag), Some((_, size, segs)), Some(id)) = (&self.drag, &handles, self.selected) {
                if let Some((a, b)) = segs[drag.axis] {
                    let dir = b - a;
                    let len = dir.length();
                    if len > 1.0 {
                        let amount = delta.dot(dir / len) / len * size;
                        let axis = AXES[drag.axis];
                        let parent_inv = self.scene.parent_matrix(id).inverse();
                        if let Some(e) = self.scene.get_mut(id) {
                            match self.tool {
                                Tool::Move => e.position += parent_inv.transform_vector3(axis * amount),
                                Tool::Scale => {
                                    let s = &mut e.scale[drag.axis];
                                    *s = (*s + amount).max(0.01);
                                }
                                Tool::Rotate => e.rotation[drag.axis] += amount / size * 90.0,
                            }
                        }
                    }
                }
            }
        }
        if editing && resp.drag_stopped() {
            self.drag = None;
        }
        if resp.drag_stopped() {
            if let (Some(_), Some(id)) = (self.drag.take(), self.selected) {
                // rounds to avoid numeric noise in the saved scene
                if let Some(e) = self.scene.get_mut(id) {
                    let r = |v: Vec3| (v * 1000.0).round() / 1000.0;
                    e.position = r(e.position);
                    e.rotation = r(e.rotation);
                    e.scale = r(e.scale);
                }
            }
        }

        // ---------- click selection (double-click also frames) ----------
        if editing {
            if resp.clicked() && hovered_axis.is_none() && widget_hover.is_none() {
                if let Some(p) = resp.interact_pointer_pos() {
                    let (o, d) = screen_ray(vp, rect, p);
                    let add = ctx.input(|i| i.modifiers.shift || i.modifiers.command);
                    self.mesh_click(o, d, p, vp, rect, add);
                }
            }
            if resp.hovered() {
                self.mesh_edit_keys(&ctx);
            }
        } else if resp.clicked() && hovered_axis.is_none() && widget_hover.is_none() {
            if let Some(p) = resp.interact_pointer_pos() {
                let (o, d) = screen_ray(vp, rect, p);
                self.selected = self.pick(o, d, if self.cam.ortho { f32::NEG_INFINITY } else { 0.0 });
            }
        }
        if !editing && resp.double_clicked() && self.selected.is_some() && widget_hover.is_none() {
            self.focus_selected();
        }

        // ---------- 3D drawing ----------
        let mut frame = RenderFrame::from_scene(&self.scene, &mut self.assets, view, proj, self.selected);
        if self.cam.ortho {
            // in orthographic views the "camera" has no useful position: shadows around the pivot
            frame.shadow = frame.shadow.map(|(_, r)| (self.cam.pivot(), r.max(self.cam.dist)));
        }
        if self.show_grid {
            frame.add_grid(40);
        }
        for e in &self.scene.entities {
            let sel = self.selected == Some(e.id);
            let world = self.scene.world_matrix(e.id);
            if let Some(c) = &e.camera {
                let col = if sel { [1.0, 0.75, 0.2, 1.0] } else { [0.9, 0.9, 0.9, 0.55] };
                frame.add_frustum(world, c.fov, 16.0 / 9.0, 1.2, col);
            }
            if !sel {
                continue;
            }
            if let Some(l) = &e.light {
                if l.kind == LightKind::Spot {
                    frame.add_frustum(world, l.angle * 2.0, 1.0, l.range.min(6.0), [1.0, 0.9, 0.4, 0.8]);
                }
            }
            // box collider of the selected object (green), as in the well-known engines
            if let Some(p) = &e.physics {
                if p.collider == crate::scene::ColliderShape::Box {
                    let (c, h) = e.mesh.as_ref().map(|m| m.local_bounds()).unwrap_or((Vec3::ZERO, Vec3::splat(0.5)));
                    frame.add_box(world, c, h, [0.3, 1.0, 0.4, 0.9]);
                }
            }
        }
        self.mesh_edit_lines(&mut frame);
        self.paint_3d(ui, rect, frame);

        // ---------- 2D overlays ----------
        let painter = ui.painter_at(rect);
        self.mesh_edit_overlay(&painter, vp, rect);
        for e in &self.scene.entities {
            let icon = if e.light.is_some() {
                "☀"
            } else if e.camera.is_some() {
                "🎥"
            } else {
                continue;
            };
            if let Some(p) = project(vp, rect, self.scene.world_matrix(e.id).w_axis.truncate()) {
                let sel = self.selected == Some(e.id);
                painter.circle_filled(p, 12.0, Color32::from_black_alpha(130));
                painter.circle_stroke(
                    p,
                    12.0,
                    Stroke::new(1.5, if sel { Color32::from_rgb(255, 150, 20) } else { Color32::from_gray(160) }),
                );
                let color = match &e.light {
                    Some(l) => Color32::from_rgb(
                        (l.color[0] * 255.0) as u8,
                        (l.color[1] * 255.0) as u8,
                        (l.color[2] * 255.0) as u8,
                    ),
                    None => Color32::WHITE,
                };
                painter.text(p, Align2::CENTER_CENTER, icon, FontId::proportional(14.0), color);
            }
        }

        if let Some((_, _, segs)) = handles {
            for i in 0..3 {
                let Some((a, b)) = segs[i] else { continue };
                if (b - a).length() < 2.0 {
                    continue; // axis pointing at the camera (orthographic views)
                }
                let hot = hovered_axis == Some(i);
                let color = if hot { Color32::from_rgb(255, 225, 60) } else { AXIS_COLORS[i] };
                painter.line_segment([a, b], Stroke::new(if hot { 4.0 } else { 2.5 }, color));
                let dir = (b - a).normalized();
                let n = egui::vec2(-dir.y, dir.x);
                match self.tool {
                    Tool::Move => {
                        let tip = b + dir * 12.0;
                        painter.add(egui::Shape::convex_polygon(
                            vec![tip, b + n * 6.0, b - n * 6.0],
                            color,
                            Stroke::NONE,
                        ));
                    }
                    Tool::Scale => {
                        painter.rect_filled(Rect::from_center_size(b, egui::vec2(11.0, 11.0)), 1.0, color);
                    }
                    Tool::Rotate => {
                        painter.circle_stroke(b, 7.0, Stroke::new(2.5, color));
                    }
                }
            }
            if let Some((o2, _)) = segs.iter().flatten().next() {
                painter.circle_filled(*o2, 4.0, Color32::WHITE);
            }
        }

        // orientation gizmo
        painter.circle_filled(widget_c, widget_r + 13.0, Color32::from_black_alpha(if over_widget { 110 } else { 60 }));
        for (v, pos, i, positive) in &ends {
            let hot = widget_hover == Some((*i, *positive));
            let base = AXIS_COLORS[*i];
            let shade = 0.55 + 0.45 * ((v.z + 1.0) * 0.5);
            let col = if hot {
                Color32::WHITE
            } else {
                Color32::from_rgb(
                    (base.r() as f32 * shade) as u8,
                    (base.g() as f32 * shade) as u8,
                    (base.b() as f32 * shade) as u8,
                )
            };
            if *positive {
                painter.line_segment([widget_c, *pos], Stroke::new(2.0, col));
                painter.circle_filled(*pos, 9.0, col);
                painter.text(*pos, Align2::CENTER_CENTER, ["X", "Y", "Z"][*i], FontId::proportional(11.0), Color32::BLACK);
            } else {
                painter.circle_filled(*pos, 5.5, col.gamma_multiply(0.75));
            }
        }
        let proj_name = if self.cam.ortho { tr!("Orthographic", "Ortográfica") } else { tr!("Perspective", "Perspectiva") };
        let label = if self.cam.view_name.is_empty() {
            proj_name.to_string()
        } else {
            format!("{} · {proj_name}", self.cam.view_name)
        };
        painter.text(
            widget_c + egui::vec2(0.0, widget_r + 20.0),
            Align2::CENTER_TOP,
            label,
            FontId::proportional(11.5),
            Color32::from_white_alpha(190),
        );

        if self.cam.speed_flash > 0.0 {
            painter.text(
                rect.center_top() + egui::vec2(0.0, 40.0),
                Align2::CENTER_TOP,
                tr!("Camera speed: {:.1} m/s", "Velocidade da câmera: {:.1} m/s", self.cam.speed),
                FontId::proportional(15.0),
                Color32::WHITE,
            );
        }

        // hint line
        let hint = if self.playing() {
            tr!("Game running — see the Game tab", "Modo jogo em execução — veja a aba Jogo")
        } else if self.cam.trackpad {
            tr!("Two fingers: orbit  |  Shift + two fingers: pan  |  Pinch: zoom  |  Right button + WASD: fly  |  F: frame", "Dois dedos: orbitar  |  Shift + dois dedos: deslocar  |  Pinça: zoom  |  Botão direito + WASD: voar  |  F: enquadrar")
        } else {
            tr!("Right button + WASD: fly (wheel = speed)  |  Alt + drag: orbit  |  Middle button: pan  |  Wheel: zoom  |  F: frame", "Botão direito + WASD: voar (roda = velocidade)  |  Alt + arrastar: orbitar  |  Botão do meio: deslocar  |  Roda: zoom  |  F: enquadrar")
        };
        painter.text(
            rect.left_bottom() + egui::vec2(10.0, -8.0),
            Align2::LEFT_BOTTOM,
            hint,
            FontId::proportional(11.5),
            Color32::from_white_alpha(170),
        );
        if self.playing() {
            painter.rect_stroke(rect.shrink(1.0), 0.0, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
        }

        self.mesh_edit_toolbar(ui, rect);

        // camera menu (top-left corner)
        let bar = Rect::from_min_size(rect.left_top() + egui::vec2(8.0, 8.0), egui::vec2(260.0, 24.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.menu_button(tr!("🎥 Camera", "🎥 Câmera"), |ui| self.camera_menu(ui));
        child.label(
            egui::RichText::new(format!("{:.0} m/s", self.cam.speed)).small().color(Color32::from_white_alpha(170)),
        );
    }

    pub fn game_view(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        if rect.width() < 4.0 || rect.height() < 4.0 {
            return;
        }
        ui.allocate_rect(rect, Sense::click());
        let painter = ui.painter_at(rect);

        // a cutscene or a script may have switched to another camera
        let chosen = self
            .play
            .as_ref()
            .and_then(|p| p.rt.active_camera)
            .filter(|id| self.scene.get(*id).map(|e| e.camera.is_some()).unwrap_or(false));
        let Some(cam_id) = chosen.or(self.scene.main_camera()) else {
            painter.rect_filled(rect, 0.0, Color32::from_rgb(12, 12, 14));
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                tr!("No camera in the scene.\nCreate one in Object > Camera.", "Nenhuma câmera na cena.\nCrie uma em Objeto > Câmera."),
                FontId::proportional(15.0),
                Color32::from_gray(170),
            );
            return;
        };
        let cam = self.scene.get(cam_id).and_then(|e| e.camera.clone()).unwrap();
        let (_, rot, pos) = self.scene.world_matrix(cam_id).to_scale_rotation_translation();
        let view = Mat4::from_rotation_translation(rot, pos).inverse();
        let aspect = rect.width() / rect.height();
        let proj = perspective(
            cam.fov.clamp(5.0, 170.0).to_radians(),
            aspect,
            cam.near.max(0.01),
            cam.far.max(cam.near + 1.0),
        );
        let frame = RenderFrame::from_scene(&self.scene, &mut self.assets, view, proj, None);
        self.paint_3d(ui, rect, frame);

        match &self.play {
            Some(play) => {
                if !play.rt.hud.is_empty() && !play.rt.in_cutscene(&self.scene) {
                    let pos = rect.left_top() + egui::vec2(16.0, 14.0);
                    let font = FontId::proportional(20.0);
                    painter.text(pos + egui::vec2(1.5, 1.5), Align2::LEFT_TOP, &play.rt.hud, font.clone(), Color32::BLACK);
                    painter.text(pos, Align2::LEFT_TOP, &play.rt.hud, font, Color32::WHITE);
                }
                // meters: normal ones stacked on the left, boss bars ("!Name") wide at the bottom
                let mut y = rect.top() + 14.0 + if play.rt.hud.is_empty() { 0.0 } else { 30.0 * play.rt.hud.lines().count() as f32 };
                let mut boss_y = rect.bottom() - rect.height() * 0.13;
                let cinema = play.rt.in_cutscene(&self.scene);
                for (name, value, c) in play.rt.bars().iter().filter(|_| !cinema) {
                    let color = Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8);
                    let (r, label, align) = match name.strip_prefix('!') {
                        Some(label) => {
                            let w = (rect.width() * 0.5).min(620.0);
                            let r = Rect::from_min_size(Pos2::new(rect.center().x - w / 2.0, boss_y), egui::vec2(w, 12.0));
                            boss_y -= 40.0;
                            (r, label, Align2::CENTER_BOTTOM)
                        }
                        None => {
                            let r = Rect::from_min_size(Pos2::new(rect.left() + 16.0, y + 16.0), egui::vec2(230.0, 12.0));
                            y += 34.0;
                            (r, name.as_str(), Align2::LEFT_BOTTOM)
                        }
                    };
                    let anchor = if align == Align2::CENTER_BOTTOM { r.center_top() } else { r.left_top() } + egui::vec2(0.0, -2.0);
                    painter.text(anchor + egui::vec2(1.0, 1.0), align, label, FontId::proportional(13.0), Color32::BLACK);
                    painter.text(anchor, align, label, FontId::proportional(13.0), Color32::from_gray(235));
                    painter.rect_filled(r.expand(2.0), 3.0, Color32::from_black_alpha(190));
                    let mut fill = r;
                    fill.set_width(r.width() * value.clamp(0.0, 1.0));
                    painter.rect_filled(fill, 2.0, color);
                }
                if play.rt.crosshair() && !cinema {
                    let c = rect.center();
                    for (a, b) in [((-11.0, 0.0), (-4.0, 0.0)), ((4.0, 0.0), (11.0, 0.0)), ((0.0, -11.0), (0.0, -4.0)), ((0.0, 4.0), (0.0, 11.0))] {
                        let (p, q) = (c + egui::vec2(a.0, a.1), c + egui::vec2(b.0, b.1));
                        painter.line_segment([p, q], egui::Stroke::new(3.5, Color32::from_black_alpha(160)));
                        painter.line_segment([p, q], egui::Stroke::new(1.6, Color32::WHITE));
                    }
                    painter.circle_filled(c, 1.6, Color32::WHITE);
                }
                if let Some(text) = play.rt.banner() {
                    let pos = Pos2::new(rect.center().x, rect.top() + rect.height() * 0.36);
                    let size = (rect.width() * 0.055).clamp(24.0, 64.0);
                    painter.text(pos + egui::vec2(3.0, 3.0), Align2::CENTER_CENTER, text, FontId::proportional(size), Color32::from_black_alpha(220));
                    painter.text(pos, Align2::CENTER_CENTER, text, FontId::proportional(size), Color32::from_rgb(240, 225, 190));
                }
                if let Some(q) = &play.rt.qte {
                    draw_qte(&painter, rect, q);
                } else if let Some((success, at)) = play.rt.qte_result {
                    let age = (play.rt.real_time - at) as f32;
                    if age < 0.7 {
                        let alpha = ((1.0 - age / 0.7) * 255.0) as u8;
                        let (text, c) = if success {
                            (tr!("GOOD!", "BOA!"), Color32::from_rgba_unmultiplied(120, 235, 140, alpha))
                        } else {
                            (tr!("MISSED", "ERROU"), Color32::from_rgba_unmultiplied(255, 110, 110, alpha))
                        };
                        let pos = Pos2::new(rect.center().x, rect.top() + rect.height() * 0.68 - age * 30.0);
                        painter.text(pos + egui::vec2(2.0, 2.0), Align2::CENTER_CENTER, &text, FontId::proportional(30.0), Color32::from_black_alpha(alpha));
                        painter.text(pos, Align2::CENTER_CENTER, &text, FontId::proportional(30.0), c);
                    }
                }
                if play.rt.in_cutscene(&self.scene) {
                    // cinema bars
                    let bar = rect.height() * 0.09;
                    painter.rect_filled(Rect::from_min_max(rect.left_top(), Pos2::new(rect.right(), rect.top() + bar)), 0.0, Color32::BLACK);
                    painter.rect_filled(Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - bar), rect.right_bottom()), 0.0, Color32::BLACK);
                    // no skipping while a quick time event waits for an answer
                    if play.rt.qte.is_none() {
                    painter.text(
                        rect.right_bottom() + egui::vec2(-14.0, -bar * 0.5),
                        Align2::RIGHT_CENTER,
                        tr!("Enter: skip", "Enter: pular"),
                        FontId::proportional(13.0),
                        Color32::from_white_alpha(170),
                    );
                    }
                }
                if play.paused {
                    painter.text(
                        rect.center_top() + egui::vec2(0.0, 14.0),
                        Align2::CENTER_TOP,
                        tr!("⏸ PAUSED", "⏸ PAUSADO"),
                        FontId::proportional(16.0),
                        Color32::from_rgb(255, 210, 80),
                    );
                }
                // the exported game has no Console: show the last script error on screen
                if self.game_only {
                    if let Some(err) = self.logs.iter().rev().find(|l| l.level == crate::runtime::LogLevel::Error) {
                        painter.text(
                            rect.left_bottom() + egui::vec2(12.0, -10.0),
                            Align2::LEFT_BOTTOM,
                            &err.text,
                            FontId::proportional(13.0),
                            Color32::from_rgb(255, 120, 120),
                        );
                    }
                }
            }
            None => {
                painter.text(
                    rect.center_bottom() + egui::vec2(0.0, -14.0),
                    Align2::CENTER_BOTTOM,
                    tr!("Camera preview — click ▶ Play (Ctrl+P) to start", "Pré-visualização da câmera — clique em ▶ Jogar (Ctrl+P) para iniciar"),
                    FontId::proportional(13.0),
                    Color32::from_white_alpha(200),
                );
            }
        }
    }
}

/// The quick time event prompt: key cap, a ring that empties as time runs out and the progress.
fn draw_qte(painter: &egui::Painter, rect: Rect, q: &crate::qte::Qte) {
    use crate::qte::{key_label, QteKind};
    let center = Pos2::new(rect.center().x, rect.top() + rect.height() * 0.68);
    let left = q.time_left();
    let urgent = left < 0.3;
    let accent = if urgent { Color32::from_rgb(255, 110, 110) } else { Color32::from_rgb(120, 200, 255) };

    let cap = |center: Pos2, key: &str, size: f32, fill: Color32, text: Color32| {
        let label = key_label(key);
        let w = (size * 0.62 * label.chars().count().max(1) as f32).max(size);
        let r = Rect::from_center_size(center, egui::vec2(w + 14.0, size + 14.0));
        painter.rect_filled(r.translate(egui::vec2(0.0, 4.0)), 8.0, Color32::from_black_alpha(150));
        painter.rect_filled(r, 8.0, fill);
        painter.text(center, Align2::CENTER_CENTER, label, FontId::proportional(size * 0.72), text);
        r
    };

    if q.kind == QteKind::Sequence {
        let n = q.keys.len() as f32;
        let gap = 62.0;
        for (i, key) in q.keys.iter().enumerate() {
            let x = center.x + (i as f32 - (n - 1.0) / 2.0) * gap;
            let done = (i as f32) < q.progress;
            let current = i as f32 == q.progress.floor();
            let fill = if done {
                Color32::from_rgb(110, 210, 130)
            } else if current {
                Color32::WHITE
            } else {
                Color32::from_gray(110)
            };
            cap(Pos2::new(x, center.y), key, if current { 34.0 } else { 28.0 }, fill, Color32::from_gray(20));
        }
    } else {
        // time ring
        let radius = 46.0;
        painter.circle_filled(center, radius + 7.0, Color32::from_black_alpha(130));
        painter.circle_stroke(center, radius, egui::Stroke::new(5.0, Color32::from_white_alpha(40)));
        let steps = 48;
        let end = (steps as f32 * left).ceil() as usize;
        let pts: Vec<Pos2> = (0..=end)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 - (i as f32 / steps as f32).min(left) * std::f32::consts::TAU;
                center + egui::vec2(a.cos(), a.sin()) * radius
            })
            .collect();
        if pts.len() > 1 {
            painter.add(egui::Shape::line(pts, egui::Stroke::new(5.0, accent)));
        }
        cap(center, q.current_key(), 36.0, Color32::WHITE, Color32::from_gray(20));
    }

    // time bar for sequences, progress bar for mash and hold
    let bar = |y: f32, fraction: f32, color: Color32| {
        let w = 180.0;
        let r = Rect::from_min_size(Pos2::new(center.x - w / 2.0, y), egui::vec2(w, 8.0));
        painter.rect_filled(r, 4.0, Color32::from_black_alpha(160));
        let mut f = r;
        f.set_width(w * fraction.clamp(0.0, 1.0));
        painter.rect_filled(f, 4.0, color);
    };
    let hint = match q.kind {
        QteKind::Press => String::new(),
        QteKind::Mash => {
            bar(center.y + 64.0, q.fraction(), Color32::from_rgb(110, 210, 130));
            tr!("Press repeatedly!", "Aperte várias vezes!")
        }
        QteKind::Hold => {
            bar(center.y + 64.0, q.fraction(), Color32::from_rgb(110, 210, 130));
            tr!("Hold!", "Segure!")
        }
        QteKind::Sequence => {
            bar(center.y + 40.0, left, accent);
            String::new()
        }
    };
    if !hint.is_empty() {
        let pos = Pos2::new(center.x, center.y + 88.0);
        painter.text(pos + egui::vec2(1.0, 1.0), Align2::CENTER_CENTER, &hint, FontId::proportional(15.0), Color32::BLACK);
        painter.text(pos, Align2::CENTER_CENTER, &hint, FontId::proportional(15.0), Color32::WHITE);
    }
}
