//! Modo Play: liga os scripts Cryo à cena (API `self`, `input`, `scene`, `time`).

use crate::audio::Audio;
use crate::physics;
use crate::scene::{BodyKind, ColliderShape, Physics, PropValue, Scene, Shape};
use cryo_lang::{Host, Instance, Value};
use glam::Vec3;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone)]
pub struct LogLine {
    pub level: LogLevel,
    pub text: String,
}

pub fn log(logs: &mut Vec<LogLine>, level: LogLevel, text: impl Into<String>) {
    if logs.len() > 2000 {
        logs.drain(0..500);
    }
    logs.push(LogLine { level, text: text.into() });
}

#[derive(Default)]
pub struct InputState {
    pub down: HashSet<String>,
    pub pressed: HashSet<String>,
    pub released: HashSet<String>,
    /// Movimento do mouse neste quadro (pixels) e rolagem.
    pub mouse_dx: f32,
    pub mouse_dy: f32,
    pub scroll: f32,
}

impl InputState {
    fn axis(&self, name: &str) -> Option<f64> {
        let key = |k: &str| self.down.contains(k) as i32 as f64;
        Some(match name {
            "horizontal" => (key("d") + key("right")).min(1.0) - (key("a") + key("left")).min(1.0),
            "vertical" => (key("w") + key("up")).min(1.0) - (key("s") + key("down")).min(1.0),
            _ => return None,
        })
    }
}

pub fn prop_to_value(p: &PropValue) -> Value {
    match p {
        PropValue::Num(n) => Value::Num(*n),
        PropValue::Bool(b) => Value::Bool(*b),
        PropValue::Text(s) => Value::text(s),
        PropValue::Vec3(v) => Value::Vec3(*v),
    }
}

pub fn value_to_prop(v: &Value) -> Option<PropValue> {
    Some(match v {
        Value::Num(n) => PropValue::Num(*n),
        Value::Bool(b) => PropValue::Bool(*b),
        Value::Text(s) => PropValue::Text(s.to_string()),
        Value::Vec3(v) => PropValue::Vec3(*v),
        _ => return None,
    })
}

fn to_v(v: Vec3) -> Value {
    Value::Vec3([v.x as f64, v.y as f64, v.z as f64])
}

fn from_v(v: &Value, what: &str) -> Result<Vec3, String> {
    match v {
        Value::Vec3(a) => Ok(Vec3::new(a[0] as f32, a[1] as f32, a[2] as f32)),
        other => Err(format!("`{what}` espera `vec3`, mas recebeu `{}`", other.type_name())),
    }
}

struct Queues {
    destroy: Vec<u64>,
    messages: Vec<(u64, String, Vec<Value>)>,
}

struct EngineHost<'a> {
    scene: &'a mut Scene,
    me: u64,
    input: &'a InputState,
    time: f64,
    dt: f64,
    logs: &'a mut Vec<LogLine>,
    queues: &'a mut Queues,
    hud: &'a mut String,
    audio: &'a mut Audio,
    script_name: &'a str,
}

impl EngineHost<'_> {
    fn entity(&mut self, id: u64) -> Result<&mut crate::scene::Entity, String> {
        self.scene.get_mut(id).ok_or_else(|| "a entidade não existe mais (foi destruída)".to_string())
    }
}

impl Host for EngineHost<'_> {
    fn global(&mut self, name: &str) -> Option<Value> {
        match name {
            "self" => Some(Value::Entity(self.me)),
            "input" | "scene" | "time" | "audio" => Some(Value::Object(Rc::from(name))),
            _ => None,
        }
    }

    fn get_member(&mut self, target: &Value, name: &str) -> Result<Value, String> {
        match target {
            Value::Entity(id) => {
                let e = self.entity(*id)?;
                Ok(match name {
                    "name" => Value::text(&e.name),
                    "position" => to_v(e.position),
                    "rotation" => to_v(e.rotation),
                    "scale" => to_v(e.scale),
                    "visible" => Value::Bool(e.visible),
                    "forward" => to_v(e.forward()),
                    "right" => to_v(e.right()),
                    "up" => to_v(e.up()),
                    "color" => match &e.mesh {
                        Some(m) => Value::Vec3(m.color.map(|c| c as f64)),
                        None => return Err("esta entidade não tem malha, logo não tem `color`".into()),
                    },
                    "velocity" => to_v(e.physics.as_ref().map(|p| p.velocity).unwrap_or_default()),
                    "grounded" => Value::Bool(e.physics.as_ref().map(|p| p.grounded).unwrap_or(false)),
                    "fov" => match &e.camera {
                        Some(c) => Value::Num(c.fov as f64),
                        None => return Err("`fov` só existe em entidades com o componente Câmera".into()),
                    },
                    _ => return Err(format!("entidade não tem o membro `{name}`")),
                })
            }
            Value::Object(o) => match (&**o, name) {
                ("time", "dt") => Ok(Value::Num(self.dt)),
                ("time", "total") => Ok(Value::Num(self.time)),
                ("input", "mouse_dx") => Ok(Value::Num(self.input.mouse_dx as f64)),
                ("input", "mouse_dy") => Ok(Value::Num(self.input.mouse_dy as f64)),
                ("input", "scroll") => Ok(Value::Num(self.input.scroll as f64)),
                ("audio", "volume") => Ok(Value::Num(self.audio.volume as f64)),
                _ => Err(format!("`{o}` não tem o membro `{name}`")),
            },
            other => Err(format!("`{}` não tem o membro `{name}`", other.type_name())),
        }
    }

    fn set_member(&mut self, target: &Value, name: &str, value: Value) -> Result<(), String> {
        if let (Value::Object(o), "volume", Value::Num(v)) = (target, name, &value) {
            if &**o == "audio" {
                self.audio.volume = (*v as f32).clamp(0.0, 2.0);
                return Ok(());
            }
        }
        let Value::Entity(id) = target else {
            return Err(format!("não é possível alterar `{name}` aqui"));
        };
        let e = self.entity(*id)?;
        match name {
            "position" => e.position = from_v(&value, name)?,
            "rotation" => e.rotation = from_v(&value, name)?,
            "scale" => e.scale = from_v(&value, name)?,
            "name" => e.name = value.to_string(),
            "visible" => match value {
                Value::Bool(b) => e.visible = b,
                _ => return Err("`visible` espera `bool`".into()),
            },
            "color" => {
                let c = from_v(&value, name)?;
                match &mut e.mesh {
                    Some(m) => m.color = [c.x, c.y, c.z],
                    None => return Err("esta entidade não tem malha, logo não tem `color`".into()),
                }
            }
            "velocity" => {
                let v = from_v(&value, name)?;
                match &mut e.physics {
                    Some(p) => p.velocity = v,
                    None => {
                        return Err(
                            "`velocity` precisa do componente Física (adicione no Inspector ou use add_physics)".into(),
                        )
                    }
                }
            }
            "fov" => match (&mut e.camera, value) {
                (Some(c), Value::Num(v)) => c.fov = (v as f32).clamp(5.0, 170.0),
                (None, _) => return Err("`fov` só existe em entidades com o componente Câmera".into()),
                _ => return Err("`fov` espera `num`".into()),
            },
            "forward" | "right" | "up" | "grounded" => {
                return Err(format!("`{name}` é somente leitura"))
            }
            _ => return Err(format!("entidade não tem o membro `{name}`")),
        }
        Ok(())
    }

    fn call(&mut self, target: Option<&Value>, name: &str, args: &[Value]) -> Result<Value, String> {
        match target {
            None => match name {
                "log" => {
                    let parts: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                    log(self.logs, LogLevel::Info, format!("[{}] {}", self.script_name, parts.join(" ")));
                    Ok(Value::Nil)
                }
                "hud" => {
                    let parts: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                    *self.hud = parts.join(" ");
                    Ok(Value::Nil)
                }
                _ => Err(format!("função desconhecida `{name}`")),
            },
            Some(Value::Entity(id)) => {
                let id = *id;
                match (name, args) {
                    ("translate", [v]) => {
                        let d = from_v(v, name)?;
                        self.entity(id)?.position += d;
                    }
                    ("rotate", [v]) => {
                        let d = from_v(v, name)?;
                        self.entity(id)?.rotation += d;
                    }
                    ("look_at", [v]) => {
                        let t = from_v(v, name)?;
                        self.entity(id)?.look_at(t);
                    }
                    ("distance_to", [Value::Entity(o)]) => {
                        let a = self.entity(id)?.position;
                        let b = self.entity(*o)?.position;
                        return Ok(Value::Num(a.distance(b) as f64));
                    }
                    ("destroy", []) => self.queues.destroy.push(id),
                    ("exists", []) => return Ok(Value::Bool(self.scene.get(id).is_some())),
                    ("send", [Value::Text(event), rest @ ..]) => {
                        self.queues.messages.push((id, event.to_string(), rest.to_vec()));
                    }
                    ("add_physics", [Value::Text(kind), Value::Text(shape)]) => {
                        let kind = match &**kind {
                            "dynamic" => BodyKind::Dynamic,
                            "static" => BodyKind::Static,
                            k => return Err(format!("tipo de corpo `{k}` inválido (dynamic ou static)")),
                        };
                        let shape = match &**shape {
                            "box" => ColliderShape::Box,
                            "sphere" => ColliderShape::Sphere,
                            s => return Err(format!("colisor `{s}` inválido (box ou sphere)")),
                        };
                        self.entity(id)?.physics = Some(Physics::new(kind, shape));
                    }
                    _ => return Err(format!("entidade não tem o método `{name}` com esses argumentos")),
                }
                Ok(Value::Nil)
            }
            Some(Value::Object(o)) => match (&**o, name, args) {
                ("input", "axis", [Value::Text(n)]) => self
                    .input
                    .axis(n)
                    .map(Value::Num)
                    .ok_or_else(|| format!("eixo `{n}` desconhecido (horizontal ou vertical)")),
                ("input", "down", [Value::Text(k)]) => Ok(Value::Bool(self.input.down.contains(&**k))),
                ("input", "pressed", [Value::Text(k)]) => {
                    Ok(Value::Bool(self.input.pressed.contains(&**k)))
                }
                ("input", "released", [Value::Text(k)]) => {
                    Ok(Value::Bool(self.input.released.contains(&**k)))
                }
                ("audio", "play" | "music", [Value::Text(file), rest @ ..]) => {
                    let volume = match rest {
                        [] => 1.0,
                        [Value::Num(v)] => *v as f32,
                        _ => return Err(format!("`audio.{name}` espera (arquivo) ou (arquivo, volume)")),
                    };
                    let warn = if name == "play" {
                        self.audio.play(file, volume)?
                    } else {
                        self.audio.music(file, volume)?
                    };
                    if let Some(w) = warn {
                        log(self.logs, LogLevel::Warn, w);
                    }
                    Ok(Value::Nil)
                }
                ("audio", "stop_music", []) => {
                    self.audio.stop_music();
                    Ok(Value::Nil)
                }
                ("audio", "stop", []) => {
                    self.audio.stop_all();
                    Ok(Value::Nil)
                }
                ("scene", "find", [Value::Text(n)]) => {
                    Ok(self.scene.find_by_name(n).map(Value::Entity).unwrap_or(Value::Nil))
                }
                ("scene", "spawn", [Value::Text(shape), pos]) => {
                    let Some(shape) = Shape::from_script_name(shape) else {
                        return Err(format!(
                            "forma `{shape}` desconhecida (cube, sphere, plane, cylinder, cone, capsule)"
                        ));
                    };
                    let p = from_v(pos, "spawn")?;
                    if self.scene.entities.len() > 5000 {
                        return Err("limite de 5000 entidades na cena atingido".into());
                    }
                    let id = self.scene.add_mesh(shape);
                    self.scene.get_mut(id).unwrap().position = p;
                    Ok(Value::Entity(id))
                }
                ("scene", "destroy", [Value::Entity(id)]) => {
                    self.queues.destroy.push(*id);
                    Ok(Value::Nil)
                }
                ("scene", "all", []) => Ok(Value::list(
                    self.scene.entities.iter().map(|e| Value::Entity(e.id)).collect(),
                )),
                _ => Err(format!("`{o}` não tem o método `{name}` com esses argumentos")),
            },
            Some(other) => Err(format!("`{}` não tem o método `{name}`", other.type_name())),
        }
    }
}

struct Running {
    inst: Instance,
    name: String,
}

/// Uma sessão de jogo em execução.
pub struct Runtime {
    instances: HashMap<u64, Running>,
    pub time: f64,
    pub hud: String,
    pub audio: Audio,
    queues: Queues,
}

impl Runtime {
    /// Compila os scripts da cena, cria as instâncias e dispara `start`.
    pub fn start(scene: &mut Scene, project_dir: &Path, logs: &mut Vec<LogLine>) -> Runtime {
        let mut rt = Runtime {
            instances: HashMap::new(),
            time: 0.0,
            hud: String::new(),
            audio: Audio::new(project_dir.to_path_buf()),
            queues: Queues { destroy: vec![], messages: vec![] },
        };
        let input = InputState::default();
        let mut cache: HashMap<String, Option<Rc<cryo_lang::Script>>> = HashMap::new();
        let with_scripts: Vec<(u64, crate::scene::ScriptRef)> = scene
            .entities
            .iter()
            .filter_map(|e| e.script.clone().filter(|s| !s.path.is_empty()).map(|s| (e.id, s)))
            .collect();

        for (id, sref) in with_scripts {
            let script = cache
                .entry(sref.path.clone())
                .or_insert_with(|| {
                    let src = match std::fs::read_to_string(project_dir.join(&sref.path)) {
                        Ok(s) => s,
                        Err(e) => {
                            log(logs, LogLevel::Error, format!("{}: não foi possível ler ({e})", sref.path));
                            return None;
                        }
                    };
                    match cryo_lang::parse(&src) {
                        Ok(s) => Some(Rc::new(s)),
                        Err(e) => {
                            log(logs, LogLevel::Error, format!("{}: {e}", sref.path));
                            None
                        }
                    }
                })
                .clone();
            let Some(script) = script else { continue };
            let name = script.name.clone();
            let created = {
                let mut host = EngineHost {
                    scene,
                    me: id,
                    input: &input,
                    time: 0.0,
                    dt: 0.0,
                    logs,
                    queues: &mut rt.queues,
                    hud: &mut rt.hud,
                    audio: &mut rt.audio,
                    script_name: &name,
                };
                Instance::new(script, &mut host)
            };
            match created {
                Ok(mut inst) => {
                    for (k, v) in &sref.props {
                        if let Err(e) = inst.set_prop(k, prop_to_value(v)) {
                            log(logs, LogLevel::Warn, format!("{}: {e}", sref.path));
                        }
                    }
                    rt.instances.insert(id, Running { inst, name });
                }
                Err(e) => log(logs, LogLevel::Error, format!("{}: {e}", sref.path)),
            }
        }

        let ids: Vec<u64> = rt.instances.keys().copied().collect();
        for id in ids {
            rt.dispatch(scene, &input, 0.0, logs, id, "start", &[]);
        }
        rt.flush(scene, &input, 0.0, logs);
        rt
    }

    fn dispatch(
        &mut self,
        scene: &mut Scene,
        input: &InputState,
        dt: f64,
        logs: &mut Vec<LogLine>,
        id: u64,
        event: &str,
        args: &[Value],
    ) {
        if scene.get(id).is_none() {
            self.instances.remove(&id);
            return;
        }
        let Some(mut running) = self.instances.remove(&id) else { return };
        if !running.inst.has_handler(event) {
            self.instances.insert(id, running);
            return;
        }
        let result = {
            let mut host = EngineHost {
                scene,
                me: id,
                input,
                time: self.time,
                dt,
                logs,
                queues: &mut self.queues,
                hud: &mut self.hud,
                audio: &mut self.audio,
                script_name: &running.name,
            };
            running.inst.call_event(event, args, &mut host)
        };
        match result {
            Ok(_) => {
                self.instances.insert(id, running);
            }
            Err(e) => {
                let who = scene.get(id).map(|e| e.name.clone()).unwrap_or_default();
                log(
                    logs,
                    LogLevel::Error,
                    format!(
                        "[{}] erro em `{event}` ({who}), {e} — script desativado nesta entidade",
                        running.name
                    ),
                );
            }
        }
    }

    /// Entrega mensagens (`send`) e aplica destruições pendentes.
    fn flush(&mut self, scene: &mut Scene, input: &InputState, dt: f64, logs: &mut Vec<LogLine>) {
        for _ in 0..8 {
            if self.queues.messages.is_empty() {
                break;
            }
            let msgs = std::mem::take(&mut self.queues.messages);
            for (id, event, args) in msgs {
                self.dispatch(scene, input, dt, logs, id, &event, &args);
            }
        }
        self.queues.messages.clear();
        for id in std::mem::take(&mut self.queues.destroy) {
            scene.remove(id);
        }
        self.instances.retain(|id, _| scene.get(*id).is_some());
    }

    pub fn update(&mut self, scene: &mut Scene, input: &InputState, dt: f32, logs: &mut Vec<LogLine>) {
        let dt = dt.clamp(0.0, 0.1);
        self.time += dt as f64;

        let mut ids: Vec<u64> = self.instances.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            self.dispatch(scene, input, dt as f64, logs, id, "update", &[Value::Num(dt as f64)]);
        }
        self.flush(scene, input, dt as f64, logs);

        for (a, b) in physics::step(scene, dt) {
            self.dispatch(scene, input, dt as f64, logs, a, "collide", &[Value::Entity(b)]);
            self.dispatch(scene, input, dt as f64, logs, b, "collide", &[Value::Entity(a)]);
        }
        self.flush(scene, input, dt as f64, logs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::ScriptRef;

    #[test]
    fn script_move_entidade_e_envia_mensagem() {
        let dir = std::env::temp_dir().join(format!("cryo_test_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(
            dir.join("scripts/A.cryo"),
            "script A {\n prop vel: num = 1\n var n = 0\n\
             on start() { self.send(\"soma\", 5) }\n\
             on soma(q) { n += q \n hud(\"n=\" + n) }\n\
             on update(dt) { self.position.x += vel * dt \n let c = scene.find(\"Chão\") \n if c != nil { c.destroy() } }\n}",
        )
        .unwrap();
        let mut scene = Scene::default_scene();
        let cube = scene.find_by_name("Cubo").unwrap();
        let mut props = std::collections::BTreeMap::new();
        props.insert("vel".to_string(), PropValue::Num(10.0));
        scene.get_mut(cube).unwrap().script = Some(ScriptRef { path: "scripts/A.cryo".into(), props });
        let mut logs = vec![];
        let mut rt = Runtime::start(&mut scene, &dir, &mut logs);
        assert_eq!(rt.hud, "n=5");
        let input = InputState::default();
        for _ in 0..10 {
            rt.update(&mut scene, &input, 0.1, &mut logs);
        }
        let errs: Vec<_> = logs.iter().filter(|l| l.level == LogLevel::Error).map(|l| l.text.clone()).collect();
        assert!(errs.is_empty(), "{errs:?}");
        assert!((scene.get(cube).unwrap().position.x - 10.0).abs() < 1e-3);
        assert!(scene.find_by_name("Chão").is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    /// O jogo de exemplo inteiro roda sem erros: andar, pular e coletar uma moeda.
    #[test]
    fn jogo_de_exemplo_funciona() {
        let dir = std::env::temp_dir().join(format!("cryo_exemplo_{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let rel = crate::project::ensure_project(&dir).unwrap();
        let mut scene = Scene::from_json(&std::fs::read_to_string(dir.join(rel)).unwrap()).unwrap();
        let mut logs = vec![];
        let mut rt = Runtime::start(&mut scene, &dir, &mut logs);
        let player = scene.find_by_name("Jogador").unwrap();
        let total = scene.entities.len();

        let mut input = InputState::default();
        for _ in 0..60 {
            rt.update(&mut scene, &input, 1.0 / 60.0, &mut logs);
        }
        assert!(scene.get(player).unwrap().physics.as_ref().unwrap().grounded);

        // anda para a direita e pula
        let x0 = scene.get(player).unwrap().position.x;
        input.down.insert("d".into());
        input.pressed.insert("space".into());
        rt.update(&mut scene, &input, 1.0 / 60.0, &mut logs);
        input.pressed.clear();
        let mut max_y: f32 = 0.0;
        for _ in 0..30 {
            rt.update(&mut scene, &input, 1.0 / 60.0, &mut logs);
            max_y = max_y.max(scene.get(player).unwrap().position.y);
        }
        assert!(scene.get(player).unwrap().position.x > x0 + 2.0);
        assert!(max_y > 1.5, "pulo: {max_y}");

        // encosta em uma moeda
        input.down.clear();
        let coin = scene.entities.iter().find(|e| e.name == "Moeda").unwrap().position;
        scene.get_mut(player).unwrap().position = coin;
        for _ in 0..3 {
            rt.update(&mut scene, &input, 1.0 / 60.0, &mut logs);
        }
        assert_eq!(rt.hud, "Moedas: 1");
        assert_eq!(scene.entities.len(), total - 1);

        let errs: Vec<_> = logs.iter().filter(|l| l.level == LogLevel::Error).map(|l| l.text.clone()).collect();
        assert!(errs.is_empty(), "{errs:?}");
        // o pulo e a moeda dispararam sons (mesmo sem placa de som no ambiente de teste)
        assert!(rt.audio.played >= 2, "sons tocados: {}", rt.audio.played);
        std::fs::remove_dir_all(dir).ok();
    }
}
