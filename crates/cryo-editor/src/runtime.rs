//! Play mode: connects Cryo scripts to the scene (`self`, `input`, `scene`, `time` API).

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
    /// Mouse movement this frame (pixels) and scroll.
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
        other => Err(format!("`{what}` expects `vec3`, but got `{}`", other.type_name())),
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
        self.scene.get_mut(id).ok_or_else(|| "the entity no longer exists (it was destroyed)".to_string())
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
                        None => return Err("this entity has no mesh, so it has no `color`".into()),
                    },
                    "velocity" => to_v(e.physics.as_ref().map(|p| p.velocity).unwrap_or_default()),
                    "grounded" => Value::Bool(e.physics.as_ref().map(|p| p.grounded).unwrap_or(false)),
                    "trigger" => Value::Bool(e.physics.as_ref().map(|p| p.is_trigger).unwrap_or(false)),
                    "fov" => match &e.camera {
                        Some(c) => Value::Num(c.fov as f64),
                        None => return Err("`fov` only exists on entities with a Camera component".into()),
                    },
                    _ => return Err(format!("entity has no member `{name}`")),
                })
            }
            Value::Object(o) => match (&**o, name) {
                ("time", "dt") => Ok(Value::Num(self.dt)),
                ("time", "total") => Ok(Value::Num(self.time)),
                ("input", "mouse_dx") => Ok(Value::Num(self.input.mouse_dx as f64)),
                ("input", "mouse_dy") => Ok(Value::Num(self.input.mouse_dy as f64)),
                ("input", "scroll") => Ok(Value::Num(self.input.scroll as f64)),
                ("audio", "volume") => Ok(Value::Num(self.audio.volume as f64)),
                _ => Err(format!("`{o}` has no member `{name}`")),
            },
            other => Err(format!("`{}` has no member `{name}`", other.type_name())),
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
            return Err(format!("cannot set `{name}` here"));
        };
        let e = self.entity(*id)?;
        match name {
            "position" => e.position = from_v(&value, name)?,
            "rotation" => e.rotation = from_v(&value, name)?,
            "scale" => e.scale = from_v(&value, name)?,
            "name" => e.name = value.to_string(),
            "visible" => match value {
                Value::Bool(b) => e.visible = b,
                _ => return Err("`visible` expects `bool`".into()),
            },
            "color" => {
                let c = from_v(&value, name)?;
                match &mut e.mesh {
                    Some(m) => m.color = [c.x, c.y, c.z],
                    None => return Err("this entity has no mesh, so it has no `color`".into()),
                }
            }
            "velocity" => {
                let v = from_v(&value, name)?;
                match &mut e.physics {
                    Some(p) => p.velocity = v,
                    None => {
                        return Err(
                            "`velocity` needs the Physics component (add it in the Inspector or call add_physics)".into(),
                        )
                    }
                }
            }
            "fov" => match (&mut e.camera, value) {
                (Some(c), Value::Num(v)) => c.fov = (v as f32).clamp(5.0, 170.0),
                (None, _) => return Err("`fov` only exists on entities with a Camera component".into()),
                _ => return Err("`fov` expects `num`".into()),
            },
            "trigger" => match (&mut e.physics, value) {
                (Some(p), Value::Bool(b)) => p.is_trigger = b,
                (None, _) => return Err("`trigger` needs the Physics component".into()),
                _ => return Err("`trigger` expects `bool`".into()),
            },
            "forward" | "right" | "up" | "grounded" => {
                return Err(format!("`{name}` is read-only"))
            }
            _ => return Err(format!("entity has no member `{name}`")),
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
                _ => Err(format!("unknown function `{name}`")),
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
                            k => return Err(format!("invalid body type `{k}` (dynamic or static)")),
                        };
                        let shape = match &**shape {
                            "box" => ColliderShape::Box,
                            "sphere" => ColliderShape::Sphere,
                            s => return Err(format!("invalid collider `{s}` (box or sphere)")),
                        };
                        self.entity(id)?.physics = Some(Physics::new(kind, shape));
                    }
                    _ => return Err(format!("entity has no method `{name}` with these arguments")),
                }
                Ok(Value::Nil)
            }
            Some(Value::Object(o)) => match (&**o, name, args) {
                ("input", "axis", [Value::Text(n)]) => self
                    .input
                    .axis(n)
                    .map(Value::Num)
                    .ok_or_else(|| format!("unknown axis `{n}` (horizontal or vertical)")),
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
                        _ => return Err(format!("`audio.{name}` expects (file) or (file, volume)")),
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
                            "unknown shape `{shape}` (cube, sphere, plane, cylinder, cone, capsule)"
                        ));
                    };
                    let p = from_v(pos, "spawn")?;
                    if self.scene.entities.len() > 5000 {
                        return Err("reached the limit of 5000 entities in the scene".into());
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
                _ => Err(format!("`{o}` has no method `{name}` with these arguments")),
            },
            Some(other) => Err(format!("`{}` has no method `{name}`", other.type_name())),
        }
    }
}

struct Running {
    inst: Instance,
    name: String,
}

/// A running game session.
pub struct Runtime {
    instances: HashMap<u64, Running>,
    pub time: f64,
    pub hud: String,
    pub audio: Audio,
    queues: Queues,
}

impl Runtime {
    /// Compiles the scene's scripts, creates the instances and fires `start`.
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
                            log(logs, LogLevel::Error, format!("{}: could not read ({e})", sref.path));
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
                        "[{}] error in `{event}` ({who}), {e} — script disabled on this entity",
                        running.name
                    ),
                );
            }
        }
    }

    /// Delivers messages (`send`) and applies pending destroys.
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

        // `on collide(other, normal)`: the normal points from `other` towards the entity
        // receiving the event (straight up when it is standing on `other`).
        for c in physics::step(scene, dt) {
            let n = c.normal;
            self.dispatch(scene, input, dt as f64, logs, c.a, "collide", &[Value::Entity(c.b), to_v(n)]);
            self.dispatch(scene, input, dt as f64, logs, c.b, "collide", &[Value::Entity(c.a), to_v(-n)]);
        }
        self.flush(scene, input, dt as f64, logs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::ScriptRef;

    #[test]
    fn script_moves_entity_and_sends_message() {
        let dir = std::env::temp_dir().join(format!("cryo_test_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(
            dir.join("scripts/A.cryo"),
            "script A {\n prop speed: num = 1\n var n = 0\n\
             on start() { self.send(\"add\", 5) }\n\
             on add(q) { n += q \n hud(\"n=\" + n) }\n\
             on update(dt) { self.position.x += speed * dt \n let c = scene.find(\"Ground\") \n if c != nil { c.destroy() } }\n}",
        )
        .unwrap();
        let mut scene = Scene::default_scene();
        let cube = scene.find_by_name("Cube").unwrap();
        let mut props = std::collections::BTreeMap::new();
        props.insert("speed".to_string(), PropValue::Num(10.0));
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
        assert!(scene.find_by_name("Ground").is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    // ------------------------------------------------------------------
    // The example platformer, played by the test harness
    // ------------------------------------------------------------------

    struct Game {
        dir: std::path::PathBuf,
        scene: Scene,
        rt: Runtime,
        input: InputState,
        logs: Vec<LogLine>,
    }

    impl Game {
        fn new(tag: &str) -> Game {
            let dir = std::env::temp_dir().join(format!("cryo_platformer_{}_{tag}", std::process::id()));
            std::fs::remove_dir_all(&dir).ok();
            let rel = crate::project::ensure_project(&dir).unwrap();
            let mut scene = Scene::from_json(&std::fs::read_to_string(dir.join(rel)).unwrap()).unwrap();
            let mut logs = vec![];
            let rt = Runtime::start(&mut scene, &dir, &mut logs);
            let mut g = Game { dir, scene, rt, input: InputState::default(), logs };
            g.frames(30); // let everything settle on the ground
            g
        }
        fn frame(&mut self) {
            self.rt.update(&mut self.scene, &self.input, 1.0 / 60.0, &mut self.logs);
            self.input.pressed.clear();
            self.input.released.clear();
        }
        fn frames(&mut self, n: usize) {
            for _ in 0..n {
                self.frame();
            }
        }
        fn press(&mut self, key: &str) {
            self.input.pressed.insert(key.into());
            self.input.down.insert(key.into());
        }
        fn release(&mut self, key: &str) {
            self.input.released.insert(key.into());
            self.input.down.remove(key);
        }
        fn tap(&mut self, key: &str) {
            self.press(key);
            self.frame();
            self.release(key);
        }
        fn id(&self, name: &str) -> u64 {
            self.scene.find_by_name(name).unwrap_or_else(|| panic!("no entity named {name}"))
        }
        fn pos(&self, name: &str) -> Vec3 {
            self.scene.world_matrix(self.id(name)).w_axis.truncate()
        }
        fn grounded(&self, name: &str) -> bool {
            self.scene.get(self.id(name)).unwrap().physics.as_ref().unwrap().grounded
        }
        fn teleport(&mut self, name: &str, p: Vec3) {
            let id = self.id(name);
            let e = self.scene.get_mut(id).unwrap();
            e.position = p;
            if let Some(ph) = &mut e.physics {
                ph.velocity = Vec3::ZERO;
            }
        }
        /// Runs until the player is on the ground again; returns the highest point reached.
        fn until_landed(&mut self) -> f32 {
            let mut top = f32::MIN;
            for i in 0..600 {
                self.frame();
                top = top.max(self.pos("Player").y);
                if i > 5 && self.grounded("Player") {
                    break;
                }
            }
            top
        }
        fn finish(self) {
            let errs: Vec<_> =
                self.logs.iter().filter(|l| l.level == LogLevel::Error).map(|l| l.text.clone()).collect();
            assert!(errs.is_empty(), "{errs:?}");
            std::fs::remove_dir_all(self.dir).ok();
        }
    }

    #[test]
    fn platformer_boots_and_idles_without_errors() {
        let mut g = Game::new("boot");
        g.frames(240);
        assert!(g.grounded("Player"));
        assert!(g.rt.hud.contains("Shards 0/5"), "{}", g.rt.hud);
        assert!(g.rt.hud.contains("HP 3/3"), "{}", g.rt.hud);
        g.finish();
    }

    #[test]
    fn platformer_jumps_get_higher_in_a_chain() {
        let mut g = Game::new("jumps");
        let ground = g.pos("Player").y;
        // run diagonally across the island so three jumps fit, and so the chain counts
        g.teleport("Player", Vec3::new(-10.0, ground, 10.0));
        g.frames(10);
        g.press("w");
        g.press("d");
        g.frames(12);
        let mut peaks = vec![];
        for _ in 0..3 {
            g.press("space");
            peaks.push(g.until_landed() - ground);
            g.release("space");
            g.frame();
        }
        assert!(peaks[0] > 1.8 && peaks[0] < 2.4, "single jump: {peaks:?}");
        assert!(peaks[1] > peaks[0] + 0.4, "double jump: {peaks:?}");
        assert!(peaks[2] > 3.7, "triple jump: {peaks:?}");
        g.release("w");
        g.finish();
    }

    #[test]
    fn platformer_backflip_long_jump_and_ground_pound() {
        let mut g = Game::new("moves");
        let start = g.pos("Player");
        // backflip: crouch + jump while standing still
        g.press("shift");
        g.frames(3);
        g.press("space");
        let top = g.until_landed() - start.y;
        g.release("space");
        g.release("shift");
        assert!(top > 3.4, "backflip height {top}");

        // long jump: run, crouch + jump -> travels much further than a normal jump
        g.teleport("Player", start);
        g.frames(10);
        g.press("w");
        g.frames(40);
        let from = g.pos("Player").z;
        g.press("shift");
        g.press("space");
        g.frame();
        g.release("shift");
        g.until_landed();
        g.release("space");
        let dist = from - g.pos("Player").z;
        assert!(dist > 6.5, "long jump distance {dist}");
        g.release("w");

        // ground pound: slam down much faster than falling
        g.teleport("Player", start);
        g.frames(10);
        g.tap("space");
        g.frames(20);
        g.tap("shift");
        g.frames(18);
        let vy = g.scene.get(g.id("Player")).unwrap().physics.as_ref().unwrap().velocity.y;
        assert!(vy < -20.0 || g.grounded("Player"), "ground pound speed {vy}");
        g.until_landed();
        g.finish();
    }

    #[test]
    fn platformer_cap_flies_out_and_returns() {
        let mut g = Game::new("cap");
        let cap = g.id("Cap");
        assert!(!g.scene.get(cap).unwrap().visible);
        g.tap("f");
        g.frames(12);
        assert!(g.scene.get(cap).unwrap().visible, "cap should be flying");
        let out = (g.pos("Cap") - g.pos("Player")).length();
        assert!(out > 2.0, "cap distance {out}");
        g.frames(120);
        assert!(!g.scene.get(cap).unwrap().visible, "cap should be back home");
        // and it can be thrown again
        g.tap("f");
        g.frames(5);
        assert!(g.scene.get(cap).unwrap().visible);
        g.finish();
    }

    #[test]
    fn platformer_capture_the_hopper_and_super_jump() {
        let mut g = Game::new("capture");
        // stand facing the Hopper (the player starts facing -Z)
        let hopper = g.pos("Hopper");
        g.teleport("Player", hopper + Vec3::new(0.0, 0.2, 3.5));
        g.frames(20);
        g.tap("f");
        g.frames(40);
        let player = g.id("Player");
        assert!(!g.scene.get(player).unwrap().visible, "player should be inside the Hopper");
        let base = g.pos("Hopper").y;
        g.frames(30);
        g.tap("space");
        let mut top = 0.0f32;
        for _ in 0..90 {
            g.frame();
            top = top.max(g.pos("Hopper").y - base);
        }
        assert!(top > 6.8, "hopper jump {top}");
        g.tap("e");
        g.frames(3);
        assert!(g.scene.get(player).unwrap().visible, "player should be out again");
        g.frames(120);
        g.finish();
    }

    #[test]
    fn platformer_hopper_reaches_the_tower_shard() {
        let mut g = Game::new("tower");
        let hopper = g.pos("Hopper");
        g.teleport("Player", hopper + Vec3::new(0.0, 0.2, 3.5));
        g.frames(20);
        g.tap("f");
        g.frames(40);
        // hop to the foot of the tower, then jump on top while pushing forward and right
        g.teleport("Hopper", Vec3::new(8.0, 3.7, -55.5));
        g.frames(30);
        g.press("w");
        g.tap("space");
        g.frames(70);
        g.release("w");
        g.frames(80);
        assert!(g.pos("Hopper").y > 9.5, "hopper should stand on the tower: {:?} hud {}", g.pos("Hopper"), g.rt.hud);
        // leave the Hopper next to the shard and grab it
        g.tap("e");
        g.frames(5);
        if let Some(shard) = g.scene.find_by_name("Frost Shard 2") {
            let p = g.scene.get(shard).unwrap().position;
            g.teleport("Player", p);
            g.frames(5);
        }
        assert!(g.rt.hud.contains("Shards 1/5"), "{}", g.rt.hud);
        g.finish();
    }

    #[test]
    fn platformer_cap_bounce_crosses_the_big_gap() {
        let mut g = Game::new("gap");
        g.teleport("Player", Vec3::new(0.0, 3.85, -59.0));
        g.frames(20);
        // throw the cap from the edge and keep the button held so it hovers
        g.press("w");
        let mut thrown = false;
        let mut jumped = false;
        for _ in 0..400 {
            let z = g.pos("Player").z;
            if !thrown && z < -62.6 {
                g.press("f");
                thrown = true;
            }
            if !jumped && z < -63.6 {
                g.press("space");
                jumped = true;
            }
            g.frame();
            if jumped && g.grounded("Player") {
                break;
            }
            assert!(g.pos("Player").y > -4.0, "fell into the sea at {:?}", g.pos("Player"));
        }
        let p = g.pos("Player");
        assert!(p.z < -74.5 && p.y > 3.5, "should land on Cap Isle, got {p:?}");
        g.release("f");
        g.release("w");
        g.finish();
    }

    #[test]
    fn platformer_wall_jumps_climb_the_shaft() {
        let mut g = Game::new("walls");
        // between the two walls (inner faces at x = -9.5 and x = -6.5)
        g.teleport("Player", Vec3::new(-8.0, 3.85, -58.0));
        g.frames(20);
        let mut top = 0.0f32;
        let mut side = "a"; // push left first (towards Wall A at -X)
        g.press("space");
        g.frame();
        g.release("space");
        g.press(side);
        for _ in 0..600 {
            g.frame();
            let p = g.pos("Player");
            top = top.max(p.y);
            let x = p.x;
            // when touching a wall, jump off it and push towards the other one
            let touching = if side == "a" { x < -9.05 } else { x > -6.95 };
            if touching && !g.grounded("Player") {
                g.release(side);
                side = if side == "a" { "d" } else { "a" };
                g.press(side);
                g.tap("space");
            }
            if g.rt.hud.contains("Shards 1/5") {
                break;
            }
        }
        assert!(g.rt.hud.contains("Shards 1/5"), "highest point {top}, hud: {}", g.rt.hud);
        g.finish();
    }

    #[test]
    fn platformer_pickups_enemies_platforms_and_respawn() {
        let mut g = Game::new("misc");
        // coins
        let coin = g.scene.entities.iter().find(|e| e.name == "Coin").unwrap().id;
        let p = g.scene.world_matrix(coin).w_axis.truncate();
        g.teleport("Player", p);
        g.frames(3);
        assert!(g.rt.hud.contains("Coins 1"), "{}", g.rt.hud);
        assert!(g.scene.get(coin).is_none());

        // the moving platform carries the player along
        let plat = g.pos("Moving Platform");
        g.teleport("Player", plat + Vec3::new(0.0, 1.1, 0.0));
        g.frames(150);
        let d = g.pos("Player") - g.pos("Moving Platform");
        assert!(d.x.abs() < 1.0 && d.z.abs() < 1.0 && (d.y - 1.05).abs() < 0.2, "not carried: {d:?}");
        assert!((g.pos("Moving Platform").z - plat.z).abs() > 2.0, "platform did not move");

        // stomping a Spiker removes it and bounces the player; touching its side hurts
        let spikers = g.scene.entities.iter().filter(|e| e.name == "Spiker").count();
        let sp = g.pos("Spiker");
        g.teleport("Player", sp + Vec3::new(0.0, 2.2, 0.0));
        g.frames(40);
        assert_eq!(g.scene.entities.iter().filter(|e| e.name == "Spiker").count(), spikers - 1);
        assert!(g.rt.hud.contains("HP 3/3"), "{}", g.rt.hud);
        g.until_landed();
        let sp = g.pos("Spiker");
        g.teleport("Player", sp + Vec3::new(0.0, 0.25, 0.9));
        g.frames(10);
        assert!(g.rt.hud.contains("HP 2/3"), "{}", g.rt.hud);

        // checkpoint, then a fall into the sea brings the player back to it
        let cp = g.scene.entities.iter().filter(|e| e.name == "Checkpoint").nth(1).unwrap().position;
        g.teleport("Player", cp + Vec3::new(0.0, 0.9, 0.0));
        g.frames(30);
        g.teleport("Player", Vec3::new(30.0, 0.0, -30.0));
        g.frames(90);
        let back = g.pos("Player");
        assert!((back - cp).length() < 3.0, "respawned at {back:?}, checkpoint {cp:?}");

        // the bounce pad reaches the summit
        let pad = g.pos("Bounce Pad");
        g.teleport("Player", pad + Vec3::new(0.0, 1.2, 0.0));
        let mut top = 0.0f32;
        for _ in 0..120 {
            g.frame();
            top = top.max(g.pos("Player").y);
        }
        assert!(top > 12.2, "bounce pad height {top}");

        // every shard can be collected, which wins the game
        for n in 1..=5 {
            let p = g.pos(&format!("Frost Shard {n}"));
            g.teleport("Player", p);
            g.frames(3);
        }
        assert!(g.rt.hud.contains("Shards 5/5"), "{}", g.rt.hud);
        assert!(g.rt.hud.contains("Thanks for playing"), "{}", g.rt.hud);
        assert!(g.rt.audio.played > 5);
        g.finish();
    }
}
