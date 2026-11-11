//! Play mode: connects Cryo scripts to the scene (`self`, `input`, `scene`, `time` API).

use crate::anim::{Action, Playing};
use crate::audio::Audio;
use crate::physics;
use crate::qte::{Outcome, Qte, QteKind};
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

/// A scheduled `send`: created by `after(...)` and `every(...)` in scripts.
struct Timer {
    at: f64,
    entity: u64,
    event: String,
    args: Vec<Value>,
    every: Option<f64>,
}

enum AnimCmd {
    Play(String),
    Stop(String),
}

/// What scripts ask the engine to do, applied between events, plus the state they share.
#[derive(Default)]
struct Queues {
    destroy: Vec<u64>,
    messages: Vec<(u64, String, Vec<Value>)>,
    /// `emit(...)`: events for every script in the scene.
    broadcast: Vec<(String, Vec<Value>)>,
    timers: Vec<Timer>,
    cancel: Vec<(u64, String)>,
    anim: Vec<AnimCmd>,
    camera: Option<u64>,
    /// A quick time event waiting to start, and a request to cancel the current one.
    qte: Option<Qte>,
    qte_cancel: bool,
    qte_active: bool,
    qte_progress: f32,
    /// `global.name`: values shared by every script.
    globals: HashMap<String, Value>,
    /// `time.scale`: 1 is normal speed, 0.25 is slow motion.
    time_scale: f64,
    /// `input.lock_mouse(true)`: the game keeps the mouse (first-person cameras).
    mouse_lock: bool,
}

fn qte_from_args(method: &str, args: &[Value], me: u64) -> Result<Qte, String> {
    // an optional last text names the event, so one script can tell several apart
    let (args, id) = match args {
        [rest @ .., Value::Text(id)] if rest.len() >= 2 => (rest, id.to_string()),
        _ => (args, method.to_string()),
    };
    let one = |k: &Rc<str>| vec![k.to_string()];
    let mut q = match (method, args) {
        ("press", [Value::Text(k), Value::Num(s)]) => Qte::new(&id, QteKind::Press, one(k), *s as f32),
        ("mash", [Value::Text(k), Value::Num(count), Value::Num(s)]) => {
            let mut q = Qte::new(&id, QteKind::Mash, one(k), *s as f32);
            q.count = count.round().clamp(1.0, 500.0) as u32;
            q
        }
        ("hold", [Value::Text(k), Value::Num(hold), Value::Num(s)]) => {
            let mut q = Qte::new(&id, QteKind::Hold, one(k), *s as f32);
            q.hold = (*hold as f32).max(0.05);
            q
        }
        ("sequence", [Value::List(keys), Value::Num(s)]) => {
            let keys: Vec<String> = keys.borrow().iter().map(|k| k.to_string()).collect();
            Qte::new(&id, QteKind::Sequence, keys, *s as f32)
        }
        _ => {
            return Err(match method {
                "press" => "`qte.press` expects (key, seconds)".to_string(),
                "mash" => "`qte.mash` expects (key, presses, seconds)".to_string(),
                "hold" => "`qte.hold` expects (key, hold_seconds, seconds)".to_string(),
                _ => "`qte.sequence` expects (list of keys, seconds)".to_string(),
            })
        }
    };
    if q.keys.is_empty() {
        return Err(format!("`qte.{method}` needs at least one key"));
    }
    q.owner = Some(me);
    Ok(q)
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
    playing: &'a [Playing],
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
            "input" | "scene" | "time" | "audio" | "anim" | "qte" | "global" => Some(Value::Object(Rc::from(name))),
            _ => None,
        }
    }

    fn get_member(&mut self, target: &Value, name: &str) -> Result<Value, String> {
        match target {
            Value::Entity(id) => {
                let e = self.entity(*id)?;
                Ok(match name {
                    "name" => Value::text(&e.name),
                    "tag" => Value::text(&e.tag),
                    "animation" => Value::text(e.mesh.as_ref().map(|m| m.animation.as_str()).unwrap_or("")),
                    "animation_time" => Value::Num(e.mesh.as_ref().map(|m| m.anim.time as f64).unwrap_or(0.0)),
                    "parent" => e.parent.map(Value::Entity).unwrap_or(Value::Nil),
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
                ("time", "scale") => Ok(Value::Num(self.queues.time_scale)),
                ("qte", "active") => Ok(Value::Bool(self.queues.qte_active || self.queues.qte.is_some())),
                ("qte", "progress") => Ok(Value::Num(self.queues.qte_progress as f64)),
                ("input", "mouse_locked") => Ok(Value::Bool(self.queues.mouse_lock)),
                // a global that was never set reads as nil
                ("global", _) => Ok(self.queues.globals.get(name).cloned().unwrap_or(Value::Nil)),
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
        if let Value::Object(o) = target {
            match (&**o, name, &value) {
                ("global", _, _) => {
                    if self.queues.globals.len() >= 10_000 && !self.queues.globals.contains_key(name) {
                        return Err("too many globals (10000)".into());
                    }
                    self.queues.globals.insert(name.to_string(), value);
                    return Ok(());
                }
                ("time", "scale", Value::Num(s)) => {
                    self.queues.time_scale = s.clamp(0.0, 10.0);
                    return Ok(());
                }
                _ => {}
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
            "tag" => e.tag = value.to_string(),
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
                // after(seconds, "event", args...) / every(seconds, "event", args...):
                // run `on event(...)` on this entity later, once or repeatedly
                "after" | "every" => match args {
                    [Value::Num(seconds), Value::Text(event), rest @ ..] => {
                        if name == "every" && *seconds < 0.02 {
                            return Err("`every` needs an interval of at least 0.02 seconds".into());
                        }
                        if self.queues.timers.len() > 5000 {
                            return Err("too many timers pending (5000)".into());
                        }
                        self.queues.timers.push(Timer {
                            at: self.time + seconds.max(0.0),
                            entity: self.me,
                            event: event.to_string(),
                            args: rest.to_vec(),
                            every: (name == "every").then_some(*seconds),
                        });
                        Ok(Value::Nil)
                    }
                    _ => Err(format!("`{name}` expects (seconds, \"event\", args...)")),
                },
                // emit("event", args...): runs `on event(...)` in every script of the scene
                "emit" => match args {
                    [Value::Text(event), rest @ ..] => {
                        if self.queues.broadcast.len() > 2000 {
                            return Err("too many events emitted in one frame (2000)".into());
                        }
                        self.queues.broadcast.push((event.to_string(), rest.to_vec()));
                        Ok(Value::Nil)
                    }
                    _ => Err("`emit` expects (\"event\", args...)".into()),
                },
                "cancel" => match args {
                    [Value::Text(event)] => {
                        self.queues.cancel.push((self.me, event.to_string()));
                        Ok(Value::Nil)
                    }
                    _ => Err("`cancel` expects (\"event\")".into()),
                },
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
                    // animate("Walk") or animate("Walk", speed): plays a clip of a rigged model,
                    // blending from the current pose; animate("") goes back to the rest pose
                    ("animate", [Value::Text(clip), rest @ ..]) => {
                        let speed = match rest {
                            [] => None,
                            [Value::Num(s)] => Some(*s as f32),
                            _ => return Err("`animate` expects (clip) or (clip, speed)".into()),
                        };
                        let Some(m) = &mut self.entity(id)?.mesh else {
                            return Err("`animate` needs an entity with a Mesh (a rigged model)".into());
                        };
                        m.play_animation(clip, true);
                        if let Some(s) = speed {
                            m.anim_speed = s.clamp(-10.0, 10.0);
                        }
                    }
                    ("child", [Value::Text(n)]) => {
                        let found = self
                            .scene
                            .children(Some(id))
                            .into_iter()
                            .find(|c| self.scene.get(*c).map(|e| e.name == **n).unwrap_or(false));
                        return Ok(found.map(Value::Entity).unwrap_or(Value::Nil));
                    }
                    ("children", []) => {
                        return Ok(Value::list(
                            self.scene.children(Some(id)).into_iter().map(Value::Entity).collect(),
                        ))
                    }
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
                ("input", "lock_mouse", [Value::Bool(on)]) => {
                    self.queues.mouse_lock = *on;
                    Ok(Value::Nil)
                }
                ("qte", "press" | "mash" | "hold" | "sequence", _) => {
                    self.queues.qte = Some(qte_from_args(name, args, self.me)?);
                    Ok(Value::Nil)
                }
                ("qte", "cancel", []) => {
                    self.queues.qte = None;
                    self.queues.qte_cancel = true;
                    Ok(Value::Nil)
                }
                ("scene", "tagged", [Value::Text(tag)]) => Ok(Value::list(
                    self.scene.entities.iter().filter(|e| e.tag == **tag).map(|e| Value::Entity(e.id)).collect(),
                )),
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
                ("scene", "set_camera", [Value::Entity(id)]) => {
                    if self.scene.get(*id).map(|e| e.camera.is_none()).unwrap_or(true) {
                        return Err("`scene.set_camera` expects an entity with a Camera component".into());
                    }
                    self.queues.camera = Some(*id);
                    Ok(Value::Nil)
                }
                ("anim", "play" | "stop", [Value::Text(clip)]) => {
                    if !self.scene.animations.iter().any(|c| c.name == **clip) {
                        return Err(format!("there is no animation named `{clip}` in this scene"));
                    }
                    self.queues.anim.push(if name == "play" {
                        AnimCmd::Play(clip.to_string())
                    } else {
                        AnimCmd::Stop(clip.to_string())
                    });
                    Ok(Value::Nil)
                }
                ("anim", "playing", [Value::Text(clip)]) => Ok(Value::Bool(self.playing.iter().any(|p| {
                    self.scene.animations.get(p.clip).map(|c| c.name == **clip).unwrap_or(false)
                }))),
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
    /// Clips currently playing.
    pub playing: Vec<Playing>,
    /// Camera chosen by a cutscene or a script (otherwise the first camera in the scene).
    pub active_camera: Option<u64>,
    /// The quick time event on screen, if any.
    pub qte: Option<Qte>,
    /// Result of the last quick time event and when it ended (real seconds), for the on-screen flash.
    pub qte_result: Option<(bool, f64)>,
    /// Seconds since Play started, never slowed down by `time.scale`.
    pub real_time: f64,
    /// Pairs of entities touching in the last physics step.
    contacts: HashSet<(u64, u64)>,
}

impl Runtime {
    /// Compiles the scene's scripts, creates the instances and fires `start`.
    pub fn start(scene: &mut Scene, project_dir: &Path, logs: &mut Vec<LogLine>) -> Runtime {
        let mut rt = Runtime {
            instances: HashMap::new(),
            time: 0.0,
            hud: String::new(),
            audio: Audio::new(project_dir.to_path_buf()),
            queues: Queues { time_scale: 1.0, ..Default::default() },
            playing: vec![],
            active_camera: None,
            qte: None,
            qte_result: None,
            real_time: 0.0,
            contacts: HashSet::new(),
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
                    playing: &rt.playing,
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

        for (i, clip) in scene.animations.iter().enumerate() {
            if clip.autoplay {
                rt.playing.push(Playing { clip: i, t: 0.0, fresh: true });
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
                playing: &self.playing,
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
            if !self.queues.broadcast.is_empty() {
                let mut ids: Vec<u64> = self.instances.keys().copied().collect();
                ids.sort_unstable();
                for (event, args) in std::mem::take(&mut self.queues.broadcast) {
                    for id in &ids {
                        self.queues.messages.push((*id, event.clone(), args.clone()));
                    }
                }
            }
            if self.queues.messages.is_empty() {
                break;
            }
            let msgs = std::mem::take(&mut self.queues.messages);
            for (id, event, args) in msgs {
                self.dispatch(scene, input, dt, logs, id, &event, &args);
            }
        }
        self.queues.messages.clear();
        self.queues.broadcast.clear();
        if std::mem::take(&mut self.queues.qte_cancel) {
            self.qte = None;
        }
        if let Some(q) = self.queues.qte.take() {
            self.qte = Some(q);
        }
        self.queues.qte_active = self.qte.is_some();
        self.queues.qte_progress = self.qte.as_ref().map(|q| q.fraction()).unwrap_or(0.0);
        for (entity, event) in std::mem::take(&mut self.queues.cancel) {
            self.queues.timers.retain(|t| !(t.entity == entity && t.event == event));
        }
        for cmd in std::mem::take(&mut self.queues.anim) {
            let (AnimCmd::Play(name) | AnimCmd::Stop(name)) = &cmd;
            let Some(index) = scene.animations.iter().position(|c| &c.name == name) else { continue };
            self.playing.retain(|p| p.clip != index);
            if matches!(cmd, AnimCmd::Play(_)) {
                self.playing.push(Playing { clip: index, t: 0.0, fresh: true });
            }
        }
        if let Some(cam) = self.queues.camera.take() {
            self.active_camera = Some(cam);
        }
        for id in std::mem::take(&mut self.queues.destroy) {
            scene.remove(id);
        }
        self.instances.retain(|id, _| scene.get(*id).is_some());
    }

    /// A cutscene is playing: the player's input is blocked.
    pub fn in_cutscene(&self, scene: &Scene) -> bool {
        self.playing.iter().any(|p| scene.animations.get(p.clip).map(|c| c.cutscene).unwrap_or(false))
    }

    fn run_action(&mut self, action: &Action, logs: &mut Vec<LogLine>) {
        match action {
            Action::Send { entity, event, arg } => {
                let args = if arg.is_empty() { vec![] } else { vec![Value::text(arg)] };
                self.queues.messages.push((*entity, event.clone(), args));
            }
            Action::Sound { file, volume } => match self.audio.play(file, *volume) {
                Ok(Some(warn)) => log(logs, LogLevel::Warn, warn),
                Ok(None) => {}
                Err(e) => log(logs, LogLevel::Error, e),
            },
            Action::Camera { entity } => self.active_camera = Some(*entity),
            Action::Hud { text } => self.hud = text.clone(),
            Action::Broadcast { event, arg } => {
                let args = if arg.is_empty() { vec![] } else { vec![Value::text(arg)] };
                self.queues.broadcast.push((event.clone(), args));
            }
            Action::Qte { id, kind, keys, count, hold, seconds } => {
                let keys: Vec<String> =
                    keys.split(|c: char| c == ',' || c.is_whitespace()).map(|k| k.to_string()).collect();
                let mut q = Qte::new(id, *kind, keys, *seconds);
                q.count = (*count).max(1);
                q.hold = hold.max(0.05);
                q.freeze = true;
                if q.keys.is_empty() {
                    log(logs, LogLevel::Warn, format!("quick time event `{id}` has no key; skipped"));
                } else {
                    self.queues.qte = Some(q);
                }
            }
            Action::Play { clip } => self.queues.anim.push(AnimCmd::Play(clip.clone())),
            Action::TimeScale { scale } => self.queues.time_scale = (*scale as f64).clamp(0.0, 10.0),
        }
    }

    /// The game asked to keep the mouse (`input.lock_mouse(true)`).
    pub fn mouse_locked(&self) -> bool {
        self.queues.mouse_lock
    }

    /// `time.scale` set by the game.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn time_scale(&self) -> f64 {
        self.queues.time_scale
    }

    /// Reads the player's answer to the quick time event on screen and reports the result.
    fn update_qte(&mut self, input: &InputState, real_dt: f32) {
        let Some(q) = &mut self.qte else { return };
        let outcome = q.update(input, real_dt);
        self.queues.qte_progress = q.fraction();
        if outcome == Outcome::Running {
            return;
        }
        let q = self.qte.take().unwrap();
        let success = outcome == Outcome::Success;
        let event = if success { "qte_success" } else { "qte_fail" };
        let args = vec![Value::text(&q.id)];
        match q.owner {
            Some(owner) => self.queues.messages.push((owner, event.into(), args)),
            None => self.queues.broadcast.push((event.into(), args)),
        }
        self.queues.qte_active = false;
        self.qte_result = Some((success, self.real_time));
    }

    /// Advances the playing clips: moves the animated entities and fires the markers crossed.
    /// `skip` jumps every cutscene straight to its end (still firing its remaining markers).
    fn advance_clips(&mut self, scene: &mut Scene, dt: f32, skip: bool, logs: &mut Vec<LogLine>) {
        let mut finished: Vec<String> = vec![];
        let mut actions: Vec<Action> = vec![];
        let mut playing = std::mem::take(&mut self.playing);
        playing.retain_mut(|p| {
            let Some(clip) = scene.animations.get(p.clip).cloned() else { return false };
            let duration = clip.duration.max(0.01);
            let from = p.t;
            let fresh = std::mem::replace(&mut p.fresh, false);
            let mut to = from + dt;
            if skip && clip.cutscene && !clip.looping {
                to = duration;
            }
            let mut keep = true;
            if to >= duration {
                actions.extend(clip.markers_between(from, duration, fresh).into_iter().map(|m| m.action.clone()));
                if clip.looping {
                    to %= duration;
                    actions.extend(clip.markers_between(0.0, to, true).into_iter().map(|m| m.action.clone()));
                } else {
                    to = duration;
                    keep = false;
                    finished.push(clip.name.clone());
                }
            } else {
                actions.extend(clip.markers_between(from, to, fresh).into_iter().map(|m| m.action.clone()));
            }
            p.t = to;
            clip.apply(scene, to);
            keep
        });
        // clips started by the actions below must not be lost
        playing.append(&mut self.playing);
        self.playing = playing;
        for a in &actions {
            self.run_action(a, logs);
        }
        // every script hears `on animation_end(name)`
        for name in finished {
            self.queues.broadcast.push(("animation_end".into(), vec![Value::text(&name)]));
        }
    }

    /// Ends every cutscene now (used by the Enter key and by tests).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn skip_cutscenes(&mut self, scene: &mut Scene, logs: &mut Vec<LogLine>) {
        self.advance_clips(scene, 0.0, true, logs);
        let input = InputState::default();
        self.flush(scene, &input, 0.0, logs);
    }

    pub fn update(&mut self, scene: &mut Scene, input: &InputState, dt: f32, logs: &mut Vec<LogLine>) {
        let real_dt = dt.clamp(0.0, 0.1);
        self.real_time += real_dt as f64;
        // quick time events read the real keyboard in real time, even in cutscenes and slow motion
        self.update_qte(input, real_dt);
        let frozen = self.qte.as_ref().map(|q| q.freeze).unwrap_or(false);

        let dt = real_dt * self.queues.time_scale as f32;
        self.time += dt as f64;

        // during a cutscene the scripts see no input; Enter skips it
        let no_input = InputState::default();
        let cutscene = self.in_cutscene(scene);
        let skip = cutscene && !frozen && input.pressed.contains("enter");
        let input = if cutscene { &no_input } else { input };

        // `on key_down(key)` and `on key_up(key)`
        let mut keys: Vec<&String> = input.pressed.iter().collect();
        keys.sort();
        for k in keys {
            self.queues.broadcast.push(("key_down".into(), vec![Value::text(k)]));
        }
        let mut keys: Vec<&String> = input.released.iter().collect();
        keys.sort();
        for k in keys {
            self.queues.broadcast.push(("key_up".into(), vec![Value::text(k)]));
        }

        // timers created with after(...) and every(...)
        let now = self.time;
        let mut due = vec![];
        self.queues.timers.retain_mut(|t| {
            if scene.get(t.entity).is_none() {
                return false; // its entity was destroyed
            }
            if t.at > now {
                return true;
            }
            due.push((t.entity, t.event.clone(), t.args.clone()));
            match t.every {
                Some(interval) => {
                    t.at += interval.max(0.02);
                    true
                }
                None => false,
            }
        });
        self.queues.messages.extend(due);

        // a cutscene waits while its quick time event is on screen
        if !frozen {
            self.advance_clips(scene, dt, skip, logs);
        }
        self.flush(scene, input, dt as f64, logs);

        let mut ids: Vec<u64> = self.instances.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            self.dispatch(scene, input, dt as f64, logs, id, "update", &[Value::Num(dt as f64)]);
        }
        self.flush(scene, input, dt as f64, logs);

        // skeletal animation of rigged models
        for e in &mut scene.entities {
            if let Some(m) = &mut e.mesh {
                m.advance_animation(dt);
            }
        }

        // `on collide(other, normal)`: the normal points from `other` towards the entity
        // receiving the event (straight up when it is standing on `other`).
        // `on touch_start(other)` fires once when two entities begin to touch, and
        // `on touch_end(other)` once when they separate.
        let mut touching = HashSet::new();
        for c in physics::step(scene, dt) {
            let n = c.normal;
            let pair = (c.a.min(c.b), c.a.max(c.b));
            if touching.insert(pair) && !self.contacts.contains(&pair) {
                self.dispatch(scene, input, dt as f64, logs, c.a, "touch_start", &[Value::Entity(c.b)]);
                self.dispatch(scene, input, dt as f64, logs, c.b, "touch_start", &[Value::Entity(c.a)]);
            }
            self.dispatch(scene, input, dt as f64, logs, c.a, "collide", &[Value::Entity(c.b), to_v(n)]);
            self.dispatch(scene, input, dt as f64, logs, c.b, "collide", &[Value::Entity(c.a), to_v(-n)]);
        }
        let ended: Vec<(u64, u64)> = self.contacts.difference(&touching).copied().collect();
        for (a, b) in ended {
            self.dispatch(scene, input, dt as f64, logs, a, "touch_end", &[Value::Entity(b)]);
            self.dispatch(scene, input, dt as f64, logs, b, "touch_end", &[Value::Entity(a)]);
        }
        self.contacts = touching;
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
            let mut rt = Runtime::start(&mut scene, &dir, &mut logs);
            // the level opens with a cutscene; these tests are about the gameplay after it
            rt.update(&mut scene, &InputState::default(), 1.0 / 60.0, &mut logs);
            assert!(rt.in_cutscene(&scene), "the intro cutscene should be playing");
            rt.skip_cutscenes(&mut scene, &mut logs);
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

    #[test]
    fn platformer_intro_cutscene_locks_input_and_cuts_cameras() {
        let dir = std::env::temp_dir().join(format!("cryo_platformer_{}_intro", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let rel = crate::project::ensure_project(&dir).unwrap();
        let mut scene = Scene::from_json(&std::fs::read_to_string(dir.join(rel)).unwrap()).unwrap();
        let mut logs = vec![];
        let mut rt = Runtime::start(&mut scene, &dir, &mut logs);
        let intro_cam = scene.find_by_name("Intro Camera").unwrap();
        let main_cam = scene.find_by_name("Camera").unwrap();
        let player = scene.find_by_name("Player").unwrap();

        // during the cutscene: the intro camera is active and moving, and the player ignores keys
        let mut input = InputState::default();
        input.down.insert("w".into());
        let start = scene.get(player).unwrap().position;
        let cam0 = scene.get(intro_cam).unwrap().position;
        for _ in 0..120 {
            rt.update(&mut scene, &input, 1.0 / 60.0, &mut logs);
        }
        assert!(rt.in_cutscene(&scene));
        assert_eq!(rt.active_camera, Some(intro_cam));
        assert!((scene.get(intro_cam).unwrap().position - cam0).length() > 5.0, "camera should be flying");
        let p = scene.get(player).unwrap().position;
        assert!((p.x - start.x).abs() < 0.05 && (p.z - start.z).abs() < 0.05, "input must be locked: {p:?}");

        // it ends by itself after 7 seconds: gameplay camera, hint delivered, input back
        for _ in 0..330 {
            rt.update(&mut scene, &input, 1.0 / 60.0, &mut logs);
        }
        assert!(!rt.in_cutscene(&scene));
        assert_eq!(rt.active_camera, Some(main_cam));
        assert!(rt.hud.contains("Find the five Frost Shards"), "{}", rt.hud);
        assert!(scene.get(player).unwrap().position.z < start.z - 0.5, "the player can move again");

        let errs: Vec<_> = logs.iter().filter(|l| l.level == LogLevel::Error).map(|l| l.text.clone()).collect();
        assert!(errs.is_empty(), "{errs:?}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn timers_clips_and_markers_stay_in_sync() {
        use crate::anim::{Action, Clip, Marker, Property};
        let dir = std::env::temp_dir().join(format!("cryo_sync_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(
            dir.join("scripts/T.cryo"),
            "script T {\n var log_text = \"\"\n var ticks = 0\n\
             on start() { after(0.5, \"late\", 7) \n every(0.2, \"tick\") \n anim.play(\"Slide\") }\n\
             on late(n) { log_text = log_text + \"late\" + n + \" \" }\n\
             on tick() { ticks += 1 \n if ticks == 3 { cancel(\"tick\") } }\n\
             on door(which) { log_text = log_text + \"door:\" + which + \"@\" + round(self.position.x) + \" \" }\n\
             on animation_end(name) { log_text = log_text + \"end:\" + name + \" \" }\n\
             on update(dt) { hud(log_text + \"ticks=\" + ticks + \" playing=\" + anim.playing(\"Slide\")) }\n}",
        )
        .unwrap();
        let mut scene = Scene::default_scene();
        let cube = scene.find_by_name("Cube").unwrap();
        scene.get_mut(cube).unwrap().script = Some(ScriptRef { path: "scripts/T.cryo".into(), ..Default::default() });
        let mut clip = Clip::new("Slide");
        clip.duration = 1.0;
        let track = clip.track_mut(cube, Property::Position);
        track.set_key(0.0, [0.0, 0.5, 0.0]);
        track.set_key(1.0, [10.0, 0.5, 0.0]);
        track.keys[0].ease = crate::anim::Ease::Linear;
        // the marker fires exactly when the cube is halfway
        clip.markers.push(Marker { t: 0.5, action: Action::Send { entity: cube, event: "door".into(), arg: "A".into() } });
        scene.animations.push(clip);

        let mut logs = vec![];
        let mut rt = Runtime::start(&mut scene, &dir, &mut logs);
        let input = InputState::default();
        for _ in 0..30 {
            rt.update(&mut scene, &input, 1.0 / 60.0, &mut logs);
        }
        assert!(rt.hud.contains("playing=true"), "{}", rt.hud);
        for _ in 0..60 {
            rt.update(&mut scene, &input, 1.0 / 60.0, &mut logs);
        }
        let errs: Vec<_> = logs.iter().filter(|l| l.level == LogLevel::Error).map(|l| l.text.clone()).collect();
        assert!(errs.is_empty(), "{errs:?}");
        assert!(rt.hud.contains("door:A@5 "), "marker in sync with the motion: {}", rt.hud);
        assert!(rt.hud.contains("late7 "), "{}", rt.hud);
        assert!(rt.hud.contains("end:Slide "), "{}", rt.hud);
        assert!(rt.hud.contains("ticks=3 "), "every() repeats until cancelled: {}", rt.hud);
        assert!(rt.hud.contains("playing=false"), "{}", rt.hud);
        assert!((scene.get(cube).unwrap().position.x - 10.0).abs() < 1e-3);
        std::fs::remove_dir_all(dir).ok();
    }

    fn keys(pressed: &[&str], down: &[&str]) -> InputState {
        let mut i = InputState::default();
        i.pressed = pressed.iter().map(|s| s.to_string()).collect();
        i.down = down.iter().chain(pressed).map(|s| s.to_string()).collect();
        i
    }

    fn no_errors(logs: &[LogLine]) {
        let errs: Vec<_> = logs.iter().filter(|l| l.level == LogLevel::Error).map(|l| l.text.clone()).collect();
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn events_reach_every_script_and_globals_are_shared() {
        let dir = std::env::temp_dir().join(format!("cryo_events_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(
            dir.join("scripts/Boss.cryo"),
            "script Boss {\n on start() { global.alarm = 0 }\n\
             on key_down(key) { if key == \"k\" { emit(\"alarm\", 3) } }\n\
             on update(dt) { hud(\"alarm=\" + global.alarm + \" enemies=\" + len(scene.tagged(\"enemy\")) + \" none=\" + global.never_set) }\n}",
        )
        .unwrap();
        std::fs::write(
            dir.join("scripts/Guard.cryo"),
            "script Guard {\n on alarm(level) { global.alarm += level \n self.tag = \"alert\" }\n}",
        )
        .unwrap();
        let mut scene = Scene::default_scene();
        let boss = scene.find_by_name("Cube").unwrap();
        scene.get_mut(boss).unwrap().script = Some(ScriptRef { path: "scripts/Boss.cryo".into(), ..Default::default() });
        let mut guards = vec![];
        for _ in 0..2 {
            let g = scene.add_empty("Guard");
            let e = scene.get_mut(g).unwrap();
            e.tag = "enemy".into();
            e.script = Some(ScriptRef { path: "scripts/Guard.cryo".into(), ..Default::default() });
            guards.push(g);
        }
        let mut logs = vec![];
        let mut rt = Runtime::start(&mut scene, &dir, &mut logs);
        rt.update(&mut scene, &InputState::default(), 0.016, &mut logs);
        assert_eq!(rt.hud, "alarm=0 enemies=2 none=nil");
        rt.update(&mut scene, &keys(&["k"], &[]), 0.016, &mut logs);
        rt.update(&mut scene, &InputState::default(), 0.016, &mut logs);
        no_errors(&logs);
        // both guards heard it, and changed their own tag
        assert_eq!(rt.hud, "alarm=6 enemies=0 none=nil");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn touch_events_fire_once_per_contact() {
        let dir = std::env::temp_dir().join(format!("cryo_touch_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(
            dir.join("scripts/Zone.cryo"),
            "script Zone {\n var inside = 0\n var enters = 0\n var exits = 0\n var frames = 0\n\
             on touch_start(other) { inside += 1 \n enters += 1 }\n\
             on touch_end(other) { inside -= 1 \n exits += 1 }\n\
             on collide(other) { frames += 1 }\n\
             on update(dt) { hud(\"inside=\" + inside + \" enters=\" + enters + \" exits=\" + exits + \" many=\" + (frames > 5)) }\n}",
        )
        .unwrap();
        let mut scene = Scene::empty();
        let zone = scene.add_mesh(Shape::Cube);
        {
            let e = scene.get_mut(zone).unwrap();
            e.scale = Vec3::splat(2.0);
            let mut p = Physics::new(BodyKind::Static, ColliderShape::Box);
            p.is_trigger = true;
            e.physics = Some(p);
            e.script = Some(ScriptRef { path: "scripts/Zone.cryo".into(), ..Default::default() });
        }
        let ball = scene.add_mesh(Shape::Sphere);
        {
            let e = scene.get_mut(ball).unwrap();
            e.position = Vec3::new(-5.0, 0.0, 0.0);
            let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Sphere);
            p.use_gravity = false;
            e.physics = Some(p);
        }
        let mut logs = vec![];
        let mut rt = Runtime::start(&mut scene, &dir, &mut logs);
        let input = InputState::default();
        // the ball crosses the zone from one side to the other
        for _ in 0..200 {
            scene.get_mut(ball).unwrap().position.x += 0.05;
            rt.update(&mut scene, &input, 1.0 / 60.0, &mut logs);
        }
        no_errors(&logs);
        assert_eq!(rt.hud, "inside=0 enters=1 exits=1 many=true");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn quick_time_events_from_scripts_and_cutscenes() {
        use crate::anim::{Action, Clip, Marker, Property};
        let dir = std::env::temp_dir().join(format!("cryo_qte_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(
            dir.join("scripts/Hero.cryo"),
            "script Hero {\n var out = \"\"\n\
             on dodge() { qte.press(\"e\", 1.0, \"dodge\") \n time.scale = 0.2 }\n\
             on door() { qte.mash(\"space\", 3, 2.0) }\n\
             on qte_success(id) { out = out + \"ok:\" + id + \" \" \n time.scale = 1 }\n\
             on qte_fail(id) { out = out + \"fail:\" + id + \" \" \n time.scale = 1 }\n\
             on update(dt) { hud(out + \"active=\" + qte.active) }\n}",
        )
        .unwrap();
        let mut scene = Scene::default_scene();
        let cube = scene.find_by_name("Cube").unwrap();
        scene.get_mut(cube).unwrap().script = Some(ScriptRef { path: "scripts/Hero.cryo".into(), ..Default::default() });
        // a cutscene with a QTE in the middle: the clip must wait for the answer
        let mut clip = Clip::new("Escape");
        clip.duration = 1.0;
        clip.cutscene = true;
        let track = clip.track_mut(cube, Property::Position);
        track.set_key(0.0, [0.0, 0.5, 0.0]);
        track.set_key(1.0, [10.0, 0.5, 0.0]);
        track.keys[0].ease = crate::anim::Ease::Linear;
        clip.markers.push(Marker {
            t: 0.5,
            action: Action::Qte { id: "jump".into(), kind: QteKind::Press, keys: "space".into(), count: 1, hold: 1.0, seconds: 3.0 },
        });
        scene.animations.push(clip);

        let mut logs = vec![];
        let mut rt = Runtime::start(&mut scene, &dir, &mut logs);
        let idle = InputState::default();
        let step = 1.0 / 60.0;

        // 1) from a script, answered in time; the game runs in slow motion meanwhile
        rt.queues.messages.push((cube, "dodge".into(), vec![]));
        rt.update(&mut scene, &idle, step, &mut logs);
        assert!(rt.qte.is_some());
        let before = rt.time;
        for _ in 0..10 {
            rt.update(&mut scene, &idle, step, &mut logs);
        }
        assert!((rt.time - before - 10.0 * step as f64 * 0.2).abs() < 1e-4, "time.scale slows the game clock");
        assert!(rt.hud.ends_with("active=true"), "{}", rt.hud);
        rt.update(&mut scene, &keys(&["e"], &[]), step, &mut logs);
        rt.update(&mut scene, &idle, step, &mut logs);
        assert_eq!(rt.hud, "ok:dodge active=false");
        assert_eq!(rt.time_scale(), 1.0);

        // 2) mashing too slowly fails; the default name is the kind of event
        rt.queues.messages.push((cube, "door".into(), vec![]));
        rt.update(&mut scene, &idle, step, &mut logs);
        rt.update(&mut scene, &keys(&["space"], &[]), step, &mut logs);
        for _ in 0..130 {
            rt.update(&mut scene, &idle, step, &mut logs);
        }
        assert_eq!(rt.hud, "ok:dodge fail:mash active=false");

        // 3) in a cutscene: the clip freezes at the marker until the player answers
        rt.queues.anim.push(AnimCmd::Play("Escape".into()));
        for _ in 0..120 {
            rt.update(&mut scene, &idle, step, &mut logs);
        }
        let x = scene.get(cube).unwrap().position.x;
        assert!(rt.qte.is_some() && rt.in_cutscene(&scene));
        assert!((x - 5.0).abs() < 0.3, "the cutscene waits at the QTE marker, x = {x}");
        // Enter does not skip while the prompt is up
        rt.update(&mut scene, &keys(&["enter"], &[]), step, &mut logs);
        assert!(rt.in_cutscene(&scene));
        rt.update(&mut scene, &keys(&["space"], &[]), step, &mut logs);
        for _ in 0..60 {
            rt.update(&mut scene, &idle, step, &mut logs);
        }
        no_errors(&logs);
        assert!(rt.hud.contains("ok:jump "), "{}", rt.hud);
        assert!((scene.get(cube).unwrap().position.x - 10.0).abs() < 1e-3, "the clip finished after the answer");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn first_person_controller_looks_walks_and_jumps() {
        let dir = std::env::temp_dir().join(format!("cryo_fps_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts/FirstPerson.cryo"), crate::project::FIRST_PERSON).unwrap();
        let mut scene = Scene::default_scene();
        let old_camera = scene.main_camera().unwrap();
        let body = scene.add_empty("First Person Player");
        {
            let e = scene.get_mut(body).unwrap();
            e.position = Vec3::new(3.0, 0.95, 3.0);
            e.scale = Vec3::new(0.6, 1.8, 0.6);
            let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Box);
            p.friction = 0.0;
            e.physics = Some(p);
            e.script = Some(ScriptRef { path: "scripts/FirstPerson.cryo".into(), ..Default::default() });
        }
        let head = scene.add_camera();
        {
            let e = scene.get_mut(head).unwrap();
            e.name = "Head".into();
            e.parent = Some(body);
            e.position = Vec3::new(0.0, 0.4, 0.0);
            e.rotation = Vec3::ZERO;
        }
        let mut logs = vec![];
        let mut rt = Runtime::start(&mut scene, &dir, &mut logs);
        let step = 1.0 / 60.0;
        for _ in 0..30 {
            rt.update(&mut scene, &InputState::default(), step, &mut logs);
        }
        assert!(rt.mouse_locked(), "the controller takes the mouse");
        assert_eq!(rt.active_camera, Some(head), "and the game looks through the head, not {old_camera}");
        assert!(scene.get(body).unwrap().physics.as_ref().unwrap().grounded);

        // mouse to the right and up: the head turns, the body does not
        let mut look = InputState::default();
        look.mouse_dx = 750.0; // 750 px * 0.12 = 90 degrees to the right
        look.mouse_dy = -100.0;
        rt.update(&mut scene, &look, step, &mut logs);
        let h = scene.get(head).unwrap().rotation;
        assert!((h.y + 90.0).abs() < 1e-3 && (h.x - 12.0).abs() < 1e-3, "{h:?}");
        assert_eq!(scene.get(body).unwrap().rotation, Vec3::ZERO);

        // W now walks towards +X (where the camera looks)
        let start = scene.get(body).unwrap().position;
        for _ in 0..60 {
            rt.update(&mut scene, &keys(&[], &["w"]), step, &mut logs);
        }
        let p = scene.get(body).unwrap().position;
        assert!(p.x - start.x > 4.0 && (p.z - start.z).abs() < 0.05, "walked {:?}", p - start);

        // Space jumps
        rt.update(&mut scene, &keys(&["space"], &[]), step, &mut logs);
        let mut top: f32 = 0.0;
        for _ in 0..90 {
            rt.update(&mut scene, &InputState::default(), step, &mut logs);
            top = top.max(scene.get(body).unwrap().position.y);
        }
        assert!(top - start.y > 1.0, "jump height {}", top - start.y);
        assert!((scene.get(body).unwrap().position.y - start.y).abs() < 0.05, "landed again");

        // Tab gives the mouse back
        rt.update(&mut scene, &keys(&["tab"], &[]), step, &mut logs);
        rt.update(&mut scene, &InputState::default(), step, &mut logs);
        assert!(!rt.mouse_locked());
        no_errors(&logs);
        std::fs::remove_dir_all(dir).ok();
    }
}
