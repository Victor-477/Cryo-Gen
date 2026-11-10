//! Modelo de cena: entidades, componentes e serialização (.cryoscene).

use glam::{EulerRot, Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Shape {
    Cube,
    Sphere,
    Plane,
    Cylinder,
    Cone,
    Capsule,
}

impl Shape {
    pub const ALL: [Shape; 6] =
        [Shape::Cube, Shape::Sphere, Shape::Plane, Shape::Cylinder, Shape::Cone, Shape::Capsule];

    pub fn label(self) -> &'static str {
        match self {
            Shape::Cube => "Cubo",
            Shape::Sphere => "Esfera",
            Shape::Plane => "Plano",
            Shape::Cylinder => "Cilindro",
            Shape::Cone => "Cone",
            Shape::Capsule => "Cápsula",
        }
    }

    /// Nome usado nos scripts: `scene.spawn("cube", pos)`.
    pub fn from_script_name(s: &str) -> Option<Shape> {
        Some(match s.to_lowercase().as_str() {
            "cube" | "cubo" => Shape::Cube,
            "sphere" | "esfera" => Shape::Sphere,
            "plane" | "plano" => Shape::Plane,
            "cylinder" | "cilindro" => Shape::Cylinder,
            "cone" => Shape::Cone,
            "capsule" | "capsula" | "cápsula" => Shape::Capsule,
            _ => return None,
        })
    }

    /// Metade das dimensões do volume que envolve a malha, em espaço local.
    pub fn half_extents(self) -> Vec3 {
        match self {
            Shape::Plane => Vec3::new(0.5, 0.01, 0.5),
            Shape::Capsule => Vec3::new(0.5, 1.0, 0.5),
            _ => Vec3::splat(0.5),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshRenderer {
    pub shape: Shape,
    /// Modelo importado (.obj, .gltf, .glb), relativo ao projeto. Quando presente, substitui `shape`.
    #[serde(default)]
    pub model: Option<String>,
    /// Textura (.png, .jpg) aplicada sobre a cor.
    #[serde(default)]
    pub texture: Option<String>,
    pub color: [f32; 3],
    /// 0 = fosco, 1 = brilhante
    #[serde(default = "default_shine")]
    pub shininess: f32,
    /// Ignora a iluminação (bom para objetos "que brilham").
    #[serde(default)]
    pub unlit: bool,
    #[serde(default = "yes")]
    pub cast_shadow: bool,
    /// Caixa envolvente do modelo importado (centro, metade do tamanho), em espaço local.
    #[serde(default)]
    pub model_bounds: Option<([f32; 3], [f32; 3])>,
}

fn default_shine() -> f32 {
    0.3
}

impl MeshRenderer {
    pub fn primitive(shape: Shape) -> Self {
        MeshRenderer {
            shape,
            model: None,
            texture: None,
            color: [0.8, 0.8, 0.82],
            shininess: 0.3,
            unlit: false,
            cast_shadow: true,
            model_bounds: None,
        }
    }

    /// (centro, metade das dimensões) do volume que envolve a malha, em espaço local.
    pub fn local_bounds(&self) -> (Vec3, Vec3) {
        match (&self.model, &self.model_bounds) {
            (Some(_), Some((c, h))) => (Vec3::from_array(*c), Vec3::from_array(*h)),
            _ => (Vec3::ZERO, self.shape.half_extents()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum LightKind {
    Directional,
    Point,
    /// Holofote: cone de luz apontando para a frente (-Z) da entidade.
    Spot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Light {
    pub kind: LightKind,
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    /// Abertura do cone (graus) — só para holofotes.
    #[serde(default = "default_angle")]
    pub angle: f32,
    /// Projeta sombras (somente a primeira luz direcional).
    #[serde(default = "yes")]
    pub shadows: bool,
}

fn default_angle() -> f32 {
    35.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BodyKind {
    /// Não se move; outros corpos colidem com ele.
    Static,
    /// Movido pela física (gravidade, colisões).
    Dynamic,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ColliderShape {
    Box,
    Sphere,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Physics {
    pub kind: BodyKind,
    pub collider: ColliderShape,
    pub use_gravity: bool,
    pub bounciness: f32,
    pub friction: f32,
    /// Gatilho: detecta contato (evento `collide`) mas não bloqueia.
    pub is_trigger: bool,
    #[serde(skip)]
    pub velocity: Vec3,
    #[serde(skip)]
    pub grounded: bool,
}

impl Physics {
    pub fn new(kind: BodyKind, collider: ColliderShape) -> Self {
        Physics {
            kind,
            collider,
            use_gravity: true,
            bounciness: 0.0,
            friction: 0.5,
            is_trigger: false,
            velocity: Vec3::ZERO,
            grounded: false,
        }
    }
}

/// Valor de um `prop` de script salvo na cena.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PropValue {
    Num(f64),
    Bool(bool),
    Text(String),
    Vec3([f64; 3]),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ScriptRef {
    /// Caminho relativo à pasta do projeto, ex.: `scripts/Jogador.cryo`.
    pub path: String,
    /// Valores de `prop` alterados no Inspector.
    #[serde(default)]
    pub props: BTreeMap<String, PropValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub parent: Option<u64>,
    pub position: Vec3,
    /// Ângulos de Euler em graus (X, Y, Z).
    pub rotation: Vec3,
    pub scale: Vec3,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub mesh: Option<MeshRenderer>,
    #[serde(default)]
    pub light: Option<Light>,
    #[serde(default)]
    pub camera: Option<Camera>,
    #[serde(default)]
    pub physics: Option<Physics>,
    #[serde(default)]
    pub script: Option<ScriptRef>,
}

fn yes() -> bool {
    true
}

impl Entity {
    pub fn rotation_quat(&self) -> Quat {
        Quat::from_euler(
            EulerRot::YXZ,
            self.rotation.y.to_radians(),
            self.rotation.x.to_radians(),
            self.rotation.z.to_radians(),
        )
    }
    pub fn local_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation_quat(), self.position)
    }
    pub fn forward(&self) -> Vec3 {
        self.rotation_quat() * Vec3::NEG_Z
    }
    pub fn right(&self) -> Vec3 {
        self.rotation_quat() * Vec3::X
    }
    pub fn up(&self) -> Vec3 {
        self.rotation_quat() * Vec3::Y
    }
    /// Gira a entidade para que a frente (-Z) aponte para `target`.
    pub fn look_at(&mut self, target: Vec3) {
        let d = target - self.position;
        if d.length_squared() < 1e-10 {
            return;
        }
        let d = d.normalize();
        let yaw = (-d.x).atan2(-d.z);
        let pitch = d.y.clamp(-1.0, 1.0).asin();
        self.rotation = Vec3::new(pitch.to_degrees(), yaw.to_degrees(), 0.0);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub entities: Vec<Entity>,
    pub next_id: u64,
    pub ambient: [f32; 3],
    pub sky_top: [f32; 3],
    pub sky_horizon: [f32; 3],
    #[serde(default = "default_gravity")]
    pub gravity: f32,
}

fn default_gravity() -> f32 {
    -9.81
}

impl Scene {
    pub fn empty() -> Self {
        Scene {
            entities: vec![],
            next_id: 1,
            ambient: [0.30, 0.33, 0.40],
            sky_top: [0.16, 0.32, 0.58],
            sky_horizon: [0.66, 0.78, 0.90],
            gravity: default_gravity(),
        }
    }

    /// Cena inicial: chão, cubo, luz e câmera — como nas engines conhecidas.
    pub fn default_scene() -> Self {
        let mut s = Scene::empty();
        let cam = s.add_camera();
        let e = s.get_mut(cam).unwrap();
        e.position = Vec3::new(0.0, 3.0, 8.0);
        e.rotation = Vec3::new(-15.0, 0.0, 0.0);
        s.add_light(LightKind::Directional);
        let ground = s.add_mesh(Shape::Cube);
        let e = s.get_mut(ground).unwrap();
        e.name = "Chão".into();
        e.position = Vec3::new(0.0, -0.25, 0.0);
        e.scale = Vec3::new(20.0, 0.5, 20.0);
        e.mesh.as_mut().unwrap().color = [0.35, 0.42, 0.36];
        e.physics = Some(Physics::new(BodyKind::Static, ColliderShape::Box));
        let cube = s.add_mesh(Shape::Cube);
        s.get_mut(cube).unwrap().position = Vec3::new(0.0, 0.5, 0.0);
        s
    }

    pub fn get(&self, id: u64) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }
    pub fn find_by_name(&self, name: &str) -> Option<u64> {
        self.entities.iter().find(|e| e.name == name).map(|e| e.id)
    }

    fn unique_name(&self, base: &str) -> String {
        if !self.entities.iter().any(|e| e.name == base) {
            return base.to_string();
        }
        let mut i = 2;
        loop {
            let n = format!("{base} {i}");
            if !self.entities.iter().any(|e| e.name == n) {
                return n;
            }
            i += 1;
        }
    }

    pub fn add_empty(&mut self, name: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entities.push(Entity {
            id,
            name: self.unique_name(name),
            parent: None,
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
            visible: true,
            mesh: None,
            light: None,
            camera: None,
            physics: None,
            script: None,
        });
        id
    }

    pub fn add_mesh(&mut self, shape: Shape) -> u64 {
        let id = self.add_empty(shape.label());
        self.get_mut(id).unwrap().mesh = Some(MeshRenderer::primitive(shape));
        id
    }

    pub fn add_light(&mut self, kind: LightKind) -> u64 {
        let (name, pos, rot) = match kind {
            LightKind::Directional => {
                ("Luz Direcional", Vec3::new(3.0, 6.0, 3.0), Vec3::new(-50.0, 35.0, 0.0))
            }
            LightKind::Point => ("Luz Pontual", Vec3::new(0.0, 3.0, 0.0), Vec3::ZERO),
            LightKind::Spot => ("Holofote", Vec3::new(0.0, 5.0, 0.0), Vec3::new(-90.0, 0.0, 0.0)),
        };
        let id = self.add_empty(name);
        let e = self.get_mut(id).unwrap();
        e.position = pos;
        e.rotation = rot;
        e.light = Some(Light {
            kind,
            color: [1.0, 0.96, 0.9],
            intensity: if kind == LightKind::Directional { 1.0 } else { 2.0 },
            range: 12.0,
            angle: 35.0,
            shadows: true,
        });
        id
    }

    pub fn add_camera(&mut self) -> u64 {
        let id = self.add_empty("Câmera");
        self.get_mut(id).unwrap().camera = Some(Camera { fov: 60.0, near: 0.1, far: 500.0 });
        id
    }

    pub fn children(&self, id: Option<u64>) -> Vec<u64> {
        self.entities.iter().filter(|e| e.parent == id).map(|e| e.id).collect()
    }

    pub fn is_descendant(&self, id: u64, ancestor: u64) -> bool {
        let mut cur = self.get(id).and_then(|e| e.parent);
        let mut guard = 0;
        while let Some(p) = cur {
            if p == ancestor {
                return true;
            }
            cur = self.get(p).and_then(|e| e.parent);
            guard += 1;
            if guard > 1000 {
                break;
            }
        }
        false
    }

    /// Remove a entidade e todos os filhos.
    pub fn remove(&mut self, id: u64) {
        for c in self.children(Some(id)) {
            self.remove(c);
        }
        self.entities.retain(|e| e.id != id);
    }

    pub fn duplicate(&mut self, id: u64) -> Option<u64> {
        let src = self.get(id)?.clone();
        let new_id = self.dup_rec(&src, src.parent);
        let name = self.unique_name(&src.name);
        self.get_mut(new_id)?.name = name;
        Some(new_id)
    }

    fn dup_rec(&mut self, src: &Entity, parent: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let mut e = src.clone();
        e.id = id;
        e.parent = parent;
        self.entities.push(e);
        let kids: Vec<Entity> =
            self.entities.iter().filter(|e| e.parent == Some(src.id)).cloned().collect();
        for k in kids {
            self.dup_rec(&k, Some(id));
        }
        id
    }

    pub fn world_matrix(&self, id: u64) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        let mut cur = Some(id);
        let mut guard = 0;
        while let Some(i) = cur {
            let Some(e) = self.get(i) else { break };
            m = e.local_matrix() * m;
            cur = e.parent;
            guard += 1;
            if guard > 1000 {
                break;
            }
        }
        m
    }

    pub fn parent_matrix(&self, id: u64) -> Mat4 {
        match self.get(id).and_then(|e| e.parent) {
            Some(p) => self.world_matrix(p),
            None => Mat4::IDENTITY,
        }
    }

    /// Define posição e rotação no mundo (convertendo para o espaço do pai).
    pub fn set_world_pose(&mut self, id: u64, pos: Vec3, rot: Quat) {
        let parent = self.parent_matrix(id);
        if parent.determinant().abs() < 1e-9 {
            return;
        }
        let (_, prot, _) = parent.to_scale_rotation_translation();
        let local_pos = parent.inverse().transform_point3(pos);
        let local_rot = prot.inverse() * rot;
        let (y, x, z) = local_rot.to_euler(EulerRot::YXZ);
        if let Some(e) = self.get_mut(id) {
            e.position = local_pos;
            e.rotation = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
        }
    }

    pub fn is_visible(&self, id: u64) -> bool {
        let mut cur = Some(id);
        let mut guard = 0;
        while let Some(i) = cur {
            let Some(e) = self.get(i) else { return false };
            if !e.visible {
                return false;
            }
            cur = e.parent;
            guard += 1;
            if guard > 1000 {
                break;
            }
        }
        true
    }

    pub fn main_camera(&self) -> Option<u64> {
        self.entities.iter().find(|e| e.camera.is_some() && e.visible).map(|e| e.id)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Result<Scene, String> {
        serde_json::from_str(s).map_err(|e| format!("cena inválida: {e}"))
    }
}
