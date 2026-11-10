//! Viewports 3D: aba Cena (câmera do editor, seleção, gizmos) e aba Jogo.

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

/// Uma posição de câmera: ponto de interesse + ângulos + distância.
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

/// Câmera do editor: voo livre, órbita, vistas ortográficas, marcadores e transições suaves.
pub struct EditorCamera {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// Distância até o ponto de interesse (pivô da órbita).
    pub dist: f32,
    /// Campo de visão vertical, em graus.
    pub fov: f32,
    pub ortho: bool,
    /// Velocidade de voo (m/s).
    pub speed: f32,
    /// Gestos de trackpad: dois dedos orbitam, Shift desloca, pinça aproxima.
    pub trackpad: bool,
    /// Aceleração suave ao voar.
    pub smooth: bool,
    pub bookmarks: [Option<Pose>; 4],
    vel: Vec3,
    anim: Option<Anim>,
    /// Tempo restante mostrando o aviso de velocidade.
    speed_flash: f32,
    pub view_name: &'static str,
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
            view_name: "",
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
    /// Vai suavemente até outra posição de câmera.
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
        self.view_name = "";
    }
    fn orbit(&mut self, dx: f32, dy: f32) {
        let pivot = self.pivot();
        self.look(dx, dy);
        self.pos = pivot - self.forward() * self.dist;
    }
    /// Desloca a câmera no plano da tela; `per_px` = metros por pixel na distância do pivô.
    fn pan(&mut self, dx: f32, dy: f32, per_px: f32) {
        self.pos += (-self.right() * dx + self.up() * dy) * per_px;
    }
    /// Aproxima/afasta mantendo o pivô (fator < 1 aproxima).
    fn zoom(&mut self, factor: f32) {
        let pivot = self.pivot();
        self.dist = (self.dist * factor).clamp(0.2, 2000.0);
        self.pos = pivot - self.forward() * self.dist;
    }

    /// Vista alinhada a um eixo: a câmera olha na direção `-axis` (a partir do lado `+axis`).
    pub fn view_from(&mut self, axis: Vec3, name: &'static str) {
        let f = -axis;
        let pitch = f.y.clamp(-1.0, 1.0).asin();
        // topo/base: alinhado aos eixos (X para a direita)
        let yaw = if f.x.abs() + f.z.abs() < 1e-4 { 0.0 } else { (-f.x).atan2(-f.z) };
        self.animate_to(Pose { pivot: self.pivot(), yaw, pitch, dist: self.dist, ortho: true });
        self.view_name = name;
    }

    /// Enquadra uma esfera (centro, raio) mantendo a direção atual.
    pub fn frame(&mut self, center: Vec3, radius: f32) {
        let dist = (radius / (self.fov.to_radians() * 0.5).tan().max(0.05) * 1.25 + 0.5).clamp(1.0, 1500.0);
        self.animate_to(Pose { pivot: center, yaw: self.yaw, pitch: self.pitch, dist, ortho: self.ortho });
    }

    fn update_anim(&mut self, dt: f32) -> bool {
        let Some(a) = &mut self.anim else { return false };
        a.t = (a.t + dt / 0.22).min(1.0);
        let k = a.t * a.t * (3.0 - 2.0 * a.t);
        let (from, to, done) = (a.from, a.to, a.t >= 1.0);
        // menor caminho angular
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
            // perspectiva durante a transição; ortográfica só ao chegar
            ortho: if done { to.ortho } else { from.ortho && to.ortho },
        });
        if done {
            self.anim = None;
        }
        true
    }
}

/// Projeção em perspectiva no padrão OpenGL (profundidade de -1 a 1).
pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols_array(&[
        f / aspect, 0.0, 0.0, 0.0,
        0.0, f, 0.0, 0.0,
        0.0, 0.0, (far + near) / (near - far), -1.0,
        0.0, 0.0, 2.0 * far * near / (near - far), 0.0,
    ])
}

fn project(vp: Mat4, rect: Rect, p: Vec3) -> Option<Pos2> {
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

/// Interseção raio × caixa alinhada (em espaço local). Devolve o `t` de entrada.
/// Aceita `t` negativo (necessário na vista ortográfica, cujo raio nasce atrás da cena).
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
    /// Entidade sob o raio. `min_t` descarta o que está atrás da câmera (perspectiva).
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
                    // sem normalizar a direção, `t` continua valendo no espaço do mundo
                    ray_box(inv.transform_point3(origin) - center, inv.transform_vector3(dir), half)
                }
                None => {
                    // luzes, câmeras e vazios: pequena esfera de seleção
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
                self.render_error.as_deref().unwrap_or("Renderizador indisponível"),
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

    /// Centro e raio aproximados de uma entidade no mundo.
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

    /// Move a câmera do jogo (a selecionada, ou a principal) para a vista atual do editor.
    pub fn align_game_camera_to_view(&mut self) {
        let target = self
            .selected
            .filter(|id| self.scene.get(*id).map(|e| e.camera.is_some()).unwrap_or(false))
            .or(self.scene.main_camera());
        match target {
            Some(id) => {
                self.scene.set_world_pose(id, self.cam.pos, self.cam.rotation());
                let name = self.scene.get(id).map(|e| e.name.clone()).unwrap_or_default();
                self.info(format!("Câmera \"{name}\" alinhada à vista do editor."));
            }
            None => self.info("Não há câmera na cena para alinhar."),
        }
    }

    /// Leva a vista do editor para a posição/rotação do objeto selecionado.
    pub fn align_view_to_selected(&mut self) {
        let Some(id) = self.selected.filter(|id| self.scene.get(*id).is_some()) else { return };
        let (_, rot, pos) = self.scene.world_matrix(id).to_scale_rotation_translation();
        let (yaw, pitch, _) = rot.to_euler(EulerRot::YXZ);
        let dist = self.cam.dist;
        let forward = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * Vec3::NEG_Z;
        self.cam.animate_to(Pose { pivot: pos + forward * dist, yaw, pitch, dist, ortho: false });
        self.cam.view_name = "";
    }

    fn camera_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(250.0);
        ui.label(egui::RichText::new("Projeção").strong());
        ui.horizontal(|ui| {
            if ui.selectable_label(!self.cam.ortho, "Perspectiva").clicked() {
                self.cam.ortho = false;
                self.cam.view_name = "";
            }
            if ui.selectable_label(self.cam.ortho, "Ortográfica").on_hover_text("Tecla O").clicked() {
                self.cam.ortho = true;
            }
        });
        ui.add(egui::Slider::new(&mut self.cam.fov, 20.0..=110.0).suffix("°").text("Campo de visão"));

        ui.separator();
        ui.label(egui::RichText::new("Vistas").strong());
        egui::Grid::new("vistas").num_columns(3).show(ui, |ui| {
            for (i, (label, axis)) in [
                ("Frente", Vec3::Z),
                ("Direita", Vec3::X),
                ("Topo", Vec3::Y),
                ("Trás", Vec3::NEG_Z),
                ("Esquerda", Vec3::NEG_X),
                ("Base", Vec3::NEG_Y),
            ]
            .into_iter()
            .enumerate()
            {
                if ui.button(label).clicked() {
                    self.cam.view_from(axis, label);
                }
                if i == 2 {
                    ui.end_row();
                }
            }
        });

        ui.separator();
        ui.label(egui::RichText::new("Navegação").strong());
        ui.add(egui::Slider::new(&mut self.cam.speed, 0.5..=100.0).logarithmic(true).suffix(" m/s").text("Velocidade"))
            .on_hover_text("Também muda com a roda do mouse enquanto você voa (botão direito)");
        ui.checkbox(&mut self.cam.smooth, "Movimento suave");
        ui.checkbox(&mut self.cam.trackpad, "Gestos de trackpad")
            .on_hover_text("Dois dedos orbitam · Shift + dois dedos desloca · pinça aproxima");
        ui.checkbox(&mut self.show_grid, "Mostrar grade");

        ui.separator();
        if ui.button("Enquadrar seleção            F").clicked() {
            self.focus_selected();
            ui.close();
        }
        if ui.button("Enquadrar tudo            Home").clicked() {
            self.frame_all();
            ui.close();
        }
        if ui.button("Alinhar vista ao objeto selecionado").clicked() {
            self.align_view_to_selected();
            ui.close();
        }
        if ui
            .button("Alinhar câmera do jogo à vista")
            .on_hover_text("Ctrl+Shift+F — posiciona a câmera do jogo exatamente onde você está olhando")
            .clicked()
        {
            self.align_game_camera_to_view();
            ui.close();
        }

        ui.separator();
        ui.label(egui::RichText::new("Marcadores de câmera").strong());
        ui.horizontal(|ui| {
            for i in 0..4 {
                let has = self.cam.bookmarks[i].is_some();
                let resp = ui
                    .add_enabled(has, egui::Button::new(format!("Ir {}", i + 1)))
                    .on_hover_text(format!("Tecla {}", i + 1));
                if resp.clicked() {
                    if let Some(p) = self.cam.bookmarks[i] {
                        self.cam.animate_to(p);
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            for i in 0..4 {
                if ui.small_button(format!("Salvar {}", i + 1)).on_hover_text(format!("Ctrl+{}", i + 1)).clicked() {
                    self.cam.bookmarks[i] = Some(self.cam.pose());
                }
            }
        });
    }

    /// Navegação da câmera do editor. Devolve `true` enquanto o usuário está voando.
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

        // --- arrastar ---
        if resp.dragged_by(PointerButton::Secondary) {
            if alt {
                // Alt + botão direito: aproximar/afastar (dolly)
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

        // --- voo (botão direito segurado) ---
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
            // WASD só enquanto voa (fora do voo, W/E/R trocam a ferramenta); setas valem sempre
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

        // --- roda / trackpad / pinça ---
        if resp.hovered() {
            let (scroll, pinch) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if pinch != 1.0 {
                self.cam.cancel_anim();
                self.cam.zoom(1.0 / pinch);
            }
            if scroll != egui::Vec2::ZERO {
                self.cam.cancel_anim();
                if flying {
                    // como na Unreal: a roda ajusta a velocidade de voo
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

        // --- atalhos ---
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
                    self.cam.view_name = "";
                }
            }
            for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4].into_iter().enumerate() {
                if pressed(key) {
                    if cmd {
                        self.cam.bookmarks[i] = Some(self.cam.pose());
                        self.info(format!("Marcador de câmera {} salvo.", i + 1));
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

        // ---------- cubo de orientação (canto superior direito) ----------
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
                let names = [["Direita", "Esquerda"], ["Topo", "Base"], ["Frente", "Trás"]];
                let axis = if positive { AXES[i] } else { -AXES[i] };
                self.cam.view_from(axis, names[i][!positive as usize]);
            }
        }

        // ---------- gizmo ----------
        let mut handles: Option<(Vec3, f32, [Option<(Pos2, Pos2)>; 3])> = None;
        let mut hovered_axis: Option<usize> = None;
        if let Some(id) = self.selected.filter(|id| self.scene.get(*id).is_some()) {
            let origin = self.scene.world_matrix(id).w_axis.truncate();
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
                        // ignora o trecho colado no centro para não brigar entre eixos
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
        if resp.dragged_by(PointerButton::Primary) {
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
        if resp.drag_stopped() {
            if let (Some(_), Some(id)) = (self.drag.take(), self.selected) {
                // arredonda para evitar ruído numérico na cena salva
                if let Some(e) = self.scene.get_mut(id) {
                    let r = |v: Vec3| (v * 1000.0).round() / 1000.0;
                    e.position = r(e.position);
                    e.rotation = r(e.rotation);
                    e.scale = r(e.scale);
                }
            }
        }

        // ---------- seleção por clique (duplo clique também enquadra) ----------
        if resp.clicked() && hovered_axis.is_none() && widget_hover.is_none() {
            if let Some(p) = resp.interact_pointer_pos() {
                let (o, d) = screen_ray(vp, rect, p);
                self.selected = self.pick(o, d, if self.cam.ortho { f32::NEG_INFINITY } else { 0.0 });
            }
        }
        if resp.double_clicked() && self.selected.is_some() && widget_hover.is_none() {
            self.focus_selected();
        }

        // ---------- desenho 3D ----------
        let mut frame = RenderFrame::from_scene(&self.scene, &mut self.assets, view, proj, self.selected);
        if self.cam.ortho {
            // na vista ortográfica a "câmera" não tem posição útil: sombras em volta do pivô
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
            // colisor de caixa do objeto selecionado (verde), como nas engines conhecidas
            if let Some(p) = &e.physics {
                if p.collider == crate::scene::ColliderShape::Box {
                    let (c, h) = e.mesh.as_ref().map(|m| m.local_bounds()).unwrap_or((Vec3::ZERO, Vec3::splat(0.5)));
                    frame.add_box(world, c, h, [0.3, 1.0, 0.4, 0.9]);
                }
            }
        }
        self.paint_3d(ui, rect, frame);

        // ---------- sobreposições 2D ----------
        let painter = ui.painter_at(rect);
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
                    continue; // eixo apontando para a câmera (vistas ortográficas)
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

        // cubo de orientação
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
        let proj_name = if self.cam.ortho { "Ortográfica" } else { "Perspectiva" };
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
                format!("Velocidade da câmera: {:.1} m/s", self.cam.speed),
                FontId::proportional(15.0),
                Color32::WHITE,
            );
        }

        // legenda
        let hint = if self.playing() {
            "Modo jogo em execução — veja a aba Jogo"
        } else if self.cam.trackpad {
            "Dois dedos: orbitar  |  Shift + dois dedos: deslocar  |  Pinça: zoom  |  Botão direito + WASD: voar  |  F: enquadrar"
        } else {
            "Botão direito + WASD: voar (roda = velocidade)  |  Alt + arrastar: orbitar  |  Botão do meio: deslocar  |  Roda: zoom  |  F: enquadrar"
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

        // menu da câmera (canto superior esquerdo)
        let bar = Rect::from_min_size(rect.left_top() + egui::vec2(8.0, 8.0), egui::vec2(260.0, 24.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.menu_button("🎥 Câmera", |ui| self.camera_menu(ui));
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

        let Some(cam_id) = self.scene.main_camera() else {
            painter.rect_filled(rect, 0.0, Color32::from_rgb(12, 12, 14));
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Nenhuma câmera na cena.\nCrie uma em Objeto → Câmera.",
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
                if !play.rt.hud.is_empty() {
                    let pos = rect.left_top() + egui::vec2(16.0, 14.0);
                    let font = FontId::proportional(20.0);
                    painter.text(pos + egui::vec2(1.5, 1.5), Align2::LEFT_TOP, &play.rt.hud, font.clone(), Color32::BLACK);
                    painter.text(pos, Align2::LEFT_TOP, &play.rt.hud, font, Color32::WHITE);
                }
                if play.paused {
                    painter.text(
                        rect.center_top() + egui::vec2(0.0, 14.0),
                        Align2::CENTER_TOP,
                        "⏸ PAUSADO",
                        FontId::proportional(16.0),
                        Color32::from_rgb(255, 210, 80),
                    );
                }
                // no jogo exportado não há Console: mostra o último erro de script na tela
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
                    "Pré-visualização da câmera — clique em ▶ Jogar (Ctrl+P) para iniciar",
                    FontId::proportional(13.0),
                    Color32::from_white_alpha(200),
                );
            }
        }
    }
}
