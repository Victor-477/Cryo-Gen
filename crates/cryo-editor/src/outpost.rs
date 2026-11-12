//! "Outpost Kestrel": the first-person shooter example. One level in three sectors: the yard,
//! the warehouse and the reactor room.
//!
//! Models and textures come from `tools/make_outpost_assets.py`; sounds are synthesised here.

use crate::scene::{BodyKind, ColliderShape, LightKind, Physics, PropValue, Scene, ScriptRef, Shape};
use glam::Vec3;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

#[path = "outpost_files.rs"]
mod files;

const SCRIPTS: &[(&str, &str)] = &[
    ("Trooper.cryo", include_str!("../template/outpost_scripts/Trooper.cryo")),
    ("Drone.cryo", include_str!("../template/outpost_scripts/Drone.cryo")),
    ("Door.cryo", include_str!("../template/outpost_scripts/Door.cryo")),
    ("Node.cryo", include_str!("../template/outpost_scripts/Node.cryo")),
    ("Pickup.cryo", include_str!("../template/outpost_scripts/Pickup.cryo")),
    ("Command.cryo", include_str!("../template/outpost_scripts/Command.cryo")),
];

pub const SCENE: &str = "scenes/OutpostKestrel.cryoscene";

/// Writes the whole game into `dir` (which must not already hold a project) and returns the scene path.
pub fn create(dir: &Path) -> Result<String, String> {
    let err = |e: std::io::Error| format!("could not create the example at {}: {e}", dir.display());
    if !crate::project::list_files(dir, crate::project::SCENE_EXT).is_empty() {
        return Err(format!("{} already holds a project; choose an empty folder for Outpost Kestrel", dir.display()));
    }
    for sub in ["scripts", "scenes", "models", "sounds", "textures"] {
        std::fs::create_dir_all(dir.join(sub)).map_err(err)?;
    }
    for (name, src) in SCRIPTS {
        std::fs::write(dir.join("scripts").join(name), src).map_err(err)?;
    }
    for (name, bytes) in files::FILES {
        std::fs::write(dir.join("models").join(name), bytes).map_err(err)?;
    }
    for (name, wav) in sounds() {
        std::fs::write(dir.join(format!("sounds/{name}.wav")), wav).map_err(err)?;
    }
    std::fs::write(dir.join(SCENE), build_scene(dir).to_json()).map_err(err)?;
    Ok(SCENE.to_string())
}

fn sounds() -> Vec<(&'static str, Vec<u8>)> {
    use crate::audio::synth_wav;
    use std::f32::consts::TAU;
    let tone = |f: f32, t: f32| (t * f * TAU).sin();
    let hiss = |t: f32| ((t * 91_733.0).sin() * 43_758.547).fract() * 2.0 - 1.0;
    let fade = |t: f32, d: f32| (1.0 - t / d).max(0.0);
    vec![
        ("shot", synth_wav(0.16, move |t| (hiss(t) * 0.8 + tone(140.0 - t * 500.0, t) * 0.6) * (-t * 34.0).exp())),
        ("bolt", synth_wav(0.3, move |t| tone(900.0 - t * 2200.0, t) * fade(t, 0.3) * 0.4 + hiss(t) * (-t * 30.0).exp() * 0.2)),
        ("boom", synth_wav(0.9, move |t| (hiss(t) * 0.7 + tone(55.0, t) * 0.8) * (-t * 5.0).exp())),
        ("reload", synth_wav(0.5, move |t| hiss(t) * (((t - 0.08).abs() < 0.02) as i32 + ((t - 0.36).abs() < 0.03) as i32) as f32 * 0.6)),
        ("pickup", synth_wav(0.3, move |t| tone(if t < 0.1 { 700.0 } else { 1050.0 }, t) * fade(t, 0.3) * 0.4)),
        ("hurt", synth_wav(0.25, move |t| (tone(160.0 - t * 200.0, t) + hiss(t) * 0.3) * (-t * 10.0).exp() * 0.6)),
        ("alert", synth_wav(0.4, move |t| tone(if (t * 10.0) as i32 % 2 == 0 { 1200.0 } else { 900.0 }, t) * fade(t, 0.4) * 0.3)),
        ("door", synth_wav(2.0, move |t| (hiss(t) * 0.2 + tone(70.0, t) * 0.5 + tone(140.0 + t * 30.0, t) * 0.15) * fade(t, 2.0))),
        ("win", synth_wav(2.2, move |t| {
            let notes = [261.63, 329.63, 392.0, 523.25];
            let i = ((t / 0.28) as usize).min(3);
            (tone(notes[i], t) + tone(notes[i] * 2.0, t) * 0.3) * fade(t, 2.2) * 0.35
        })),
        // a low machine hum with a slow pulse
        ("ambience", synth_wav(12.0, move |t| {
            let pulse = ((t / 3.0 * TAU).sin() * 0.3 + 0.7) * ((t % 1.5) * -1.2).exp().max(0.35);
            (tone(55.0, t) * 0.4 + tone(82.4, t) * 0.2 + tone(110.3, t) * 0.12 + hiss(t) * 0.02) * pulse * 0.6
        })),
    ]
}

/// Lays out the level. `dir` is where the models were written (their sizes are read from there).
pub fn build_scene(dir: &Path) -> Scene {
    let mut s = Scene::empty();
    // night: a cold moon outside, sodium lamps in the yard, emergency red by the reactor
    s.sky_top = [0.02, 0.03, 0.06];
    s.sky_horizon = [0.10, 0.13, 0.19];
    s.ambient = [0.20, 0.23, 0.30];
    s.fog = 0.014;
    s.relief = 1.6;
    s.gravity = -24.0;

    let mut bounds: HashMap<String, ([f32; 3], [f32; 3])> = HashMap::new();
    let v = Vec3::new;
    let script = |name: &str, props: &[(&str, PropValue)]| {
        let mut map = BTreeMap::new();
        for (k, val) in props {
            map.insert(k.to_string(), val.clone());
        }
        Some(ScriptRef { path: format!("scripts/{name}.cryo"), props: map })
    };
    let mut model = |s: &mut Scene, name: &str, file: &str, pos: Vec3, yaw: f32| -> u64 {
        let b = *bounds.entry(file.to_string()).or_insert_with(|| {
            crate::assets::load_model(&dir.join("models").join(file))
                .map(|m| (m.center.to_array(), m.half.to_array()))
                .unwrap_or(([0.0; 3], [0.5; 3]))
        });
        let id = s.add_mesh(Shape::Cube);
        let e = s.get_mut(id).unwrap();
        e.name = name.into();
        e.position = pos;
        e.rotation = v(0.0, yaw, 0.0);
        let m = e.mesh.as_mut().unwrap();
        m.model = Some(format!("models/{file}"));
        m.model_bounds = Some(b);
        m.color = [1.0; 3];
        m.shininess = 0.45;
        id
    };
    let solid = |s: &mut Scene, id: u64| {
        s.get_mut(id).unwrap().physics = Some(Physics::new(BodyKind::Static, ColliderShape::Box));
        id
    };
    let adopt = |s: &mut Scene, parent: u64, id: u64| {
        s.get_mut(id).unwrap().parent = Some(parent);
        id
    };
    let point = |s: &mut Scene, name: &str, pos: Vec3, color: [f32; 3], intensity: f32, range: f32| -> u64 {
        let id = s.add_light(LightKind::Point);
        let e = s.get_mut(id).unwrap();
        e.name = name.into();
        e.position = pos;
        let l = e.light.as_mut().unwrap();
        l.color = color;
        l.intensity = intensity;
        l.range = range;
        id
    };

    let moon = s.add_light(LightKind::Directional);
    {
        let e = s.get_mut(moon).unwrap();
        e.name = "Moon".into();
        e.position = v(-20.0, 40.0, 30.0);
        e.rotation = v(-48.0, -35.0, 0.0);
        let l = e.light.as_mut().unwrap();
        l.color = [0.55, 0.68, 1.0];
        l.intensity = 0.55;
        l.shadow_distance = 40.0;
    }

    // ------------------------------------------------------------ floors, roofs, walls
    let building = s.add_empty("Outpost");
    for (name, file, pos) in [
        ("Yard", "ground_yard.obj", v(0.0, 0.0, 24.0)),
        ("Warehouse Floor", "ground_hall.obj", v(0.0, 0.0, -14.0)),
        ("Reactor Floor", "ground_core.obj", v(0.0, 0.0, -44.0)),
        ("Warehouse Roof", "roof_hall.obj", v(0.0, 7.0, -14.0)),
        ("Reactor Roof", "roof_core.obj", v(0.0, 7.0, -44.0)),
    ] {
        let id = model(&mut s, name, file, pos, 0.0);
        if file.starts_with("ground") {
            // floors are worn smooth: they catch the lamps and the sky
            s.get_mut(id).unwrap().mesh.as_mut().unwrap().shininess = 0.8;
        }
        solid(&mut s, id);
        adopt(&mut s, building, id);
    }
    let mut wall = |s: &mut Scene, file: &str, x: f32, z: f32, yaw: f32| {
        let id = model(s, "Wall", file, v(x, 0.0, z), yaw);
        solid(s, id);
        adopt(s, building, id);
    };
    for x in [-24.0, -12.0, 0.0, 12.0, 24.0] {
        wall(&mut s, "fence_12.obj", x, 46.0, 0.0);
    }
    for z in [8.0, 20.0, 32.0, 44.0] {
        wall(&mut s, "fence_12.obj", -26.0, z, 90.0);
        wall(&mut s, "fence_12.obj", 26.0, z, 90.0);
    }
    for z in [2.0, -30.0] {
        wall(&mut s, "frame.obj", 0.0, z, 0.0);
        wall(&mut s, "wall_12.obj", -12.0, z, 0.0);
        wall(&mut s, "wall_12.obj", 12.0, z, 0.0);
    }
    wall(&mut s, "wall_12.obj", -24.0, 2.0, 0.0);
    wall(&mut s, "wall_12.obj", 24.0, 2.0, 0.0);
    for z in [-4.0, -16.0, -28.0] {
        wall(&mut s, "wall_12.obj", -16.0, z, 90.0);
        wall(&mut s, "wall_12.obj", 16.0, z, 90.0);
    }
    for z in [-36.0, -48.0, -60.0] {
        wall(&mut s, "wall_12.obj", -14.0, z, 90.0);
        wall(&mut s, "wall_12.obj", 14.0, z, 90.0);
    }
    for x in [-12.0, 0.0, 12.0] {
        wall(&mut s, "wall_12.obj", x, -58.0, 0.0);
    }
    for (name, z, area) in [("Yard Door", 2.0, "yard"), ("Reactor Door", -30.0, "hall")] {
        let id = model(&mut s, name, "door.obj", v(0.0, 0.0, z), 0.0);
        solid(&mut s, id);
        s.get_mut(id).unwrap().script = script("Door", &[("area", PropValue::Text(area.into()))]);
    }

    // ------------------------------------------------------------ cover, clutter, lights
    let props = s.add_empty("Props");
    let mut prop = |s: &mut Scene, name: &str, file: &str, x: f32, z: f32, yaw: f32| {
        let id = model(s, name, file, v(x, 0.0, z), yaw);
        solid(s, id);
        adopt(s, props, id);
    };
    for (file, x, z, yaw) in [
        ("container_blue.obj", -10.0, 31.0, 20.0), ("container_red.obj", 9.0, 27.0, -15.0), ("container_green.obj", -15.0, 14.0, 90.0),
        ("container_red.obj", 13.0, 12.0, 0.0), ("container_blue.obj", -9.0, -8.0, 90.0), ("container_green.obj", 9.0, -20.0, 0.0),
    ] {
        prop(&mut s, "Container", file, x, z, yaw);
    }
    for (x, z, yaw) in [
        (-3.0, 34.0, 10.0), (-1.6, 34.3, -5.0), (4.0, 18.0, 30.0), (-6.0, 20.0, 0.0), (18.0, 22.0, 45.0), (-20.0, 26.0, 15.0),
        (4.0, -6.0, 0.0), (5.4, -6.2, 12.0), (-3.0, -14.0, 80.0), (11.0, -10.0, 20.0), (-11.0, -22.0, 0.0), (2.0, -24.0, 35.0),
        (-9.0, -38.0, 0.0), (9.0, -38.0, 90.0), (-10.0, -52.0, 30.0), (10.0, -52.0, -30.0),
    ] {
        prop(&mut s, "Crate", "crate.obj", x, z, yaw);
    }
    for (x, z) in [(6.0, 33.0), (6.9, 33.4), (-18.0, 9.0), (20.0, 6.0), (-13.0, -4.0), (13.0, -26.0), (12.4, -25.0)] {
        prop(&mut s, "Barrel", "barrel.obj", x, z, 0.0);
    }
    for (x, z, yaw) in [(-20.0, 38.0, 200.0), (20.0, 38.0, 160.0), (-20.0, 10.0, -20.0), (20.0, 12.0, 20.0), (0.0, 24.0, 180.0)] {
        let id = model(&mut s, "Lamp Post", "lamp.obj", v(x, 0.0, z), yaw);
        adopt(&mut s, props, id);
        let a = yaw.to_radians();
        let l = point(&mut s, "Lamp", v(x + a.sin() * 1.1, 5.0, z - a.cos() * 1.1), [1.0, 0.72, 0.38], 3.4, 17.0);
        adopt(&mut s, props, l);
    }
    for (x, z) in [(-8.0, -6.0), (8.0, -6.0), (-8.0, -22.0), (8.0, -22.0), (0.0, -14.0)] {
        let l = point(&mut s, "Ceiling Light", v(x, 6.2, z), [0.8, 0.9, 1.0], 2.6, 15.0);
        adopt(&mut s, props, l);
    }
    for (x, z) in [(-9.0, -36.0), (9.0, -36.0), (-9.0, -53.0), (9.0, -53.0)] {
        let l = point(&mut s, "Emergency Light", v(x, 5.5, z), [1.0, 0.16, 0.1], 2.8, 14.0);
        adopt(&mut s, props, l);
    }
    let core = model(&mut s, "Reactor Core", "core.obj", v(0.0, 0.0, -44.0), 0.0);
    solid(&mut s, core);
    s.get_mut(core).unwrap().mesh.as_mut().unwrap().shininess = 0.8;
    point(&mut s, "Core Glow", v(0.0, 3.0, -41.0), [0.3, 0.9, 1.0], 3.2, 16.0);
    for (x, z) in [(6.5, -44.0), (-6.5, -44.0), (0.0, -51.0)] {
        let id = model(&mut s, "Coolant Node", "node.obj", v(x, 0.0, z), 0.0);
        solid(&mut s, id);
        let e = s.get_mut(id).unwrap();
        e.tag = "target".into();
        e.script = script("Node", &[]);
    }
    for (kind, x, z) in [("medkit", -4.0, 6.0), ("ammo", 4.0, 6.0), ("ammo", -12.0, -12.0), ("medkit", 12.0, -16.0), ("ammo", 3.0, -33.0), ("medkit", -3.0, -33.0)] {
        let id = model(&mut s, if kind == "ammo" { "Ammo" } else { "Medkit" }, &format!("{kind}.obj"), v(x, 0.3, z), 0.0);
        let mut p = Physics::new(BodyKind::Static, ColliderShape::Sphere);
        p.is_trigger = true;
        let e = s.get_mut(id).unwrap();
        e.scale = Vec3::splat(1.5);
        e.physics = Some(p);
        e.script = script("Pickup", &[("kind", PropValue::Text(kind.into()))]);
    }

    // ------------------------------------------------------------ drones
    let drones = s.add_empty("Drones");
    let num = PropValue::Num;
    let mut drone = |s: &mut Scene, heavy: bool, x: f32, z: f32, area: &str| {
        let id = model(s, if heavy { "Heavy Drone" } else { "Drone" }, if heavy { "drone_heavy.obj" } else { "drone.obj" }, v(x, if heavy { 3.0 } else { 2.5 }, z), 180.0);
        let mut props = vec![("area", PropValue::Text(area.into()))];
        if heavy {
            props.extend([("max_health", num(150.0)), ("damage", num(15.0)), ("speed", num(3.0)), ("fire_every", num(1.0)), ("keep", num(8.0))]);
        }
        let e = s.get_mut(id).unwrap();
        e.tag = "enemy".into();
        e.parent = Some(drones);
        e.physics = Some(Physics::new(BodyKind::Static, ColliderShape::Sphere));
        e.script = script("Drone", &props);
    };
    for (x, z) in [(-7.0, 22.0), (8.0, 19.0), (0.0, 12.0), (-17.0, 8.0), (16.0, 22.0)] {
        drone(&mut s, false, x, z, "yard");
    }
    for (heavy, x, z) in [(false, -6.0, -6.0), (false, 7.0, -12.0), (false, -10.0, -18.0), (false, 4.0, -26.0), (true, 12.0, -4.0), (true, -4.0, -24.0)] {
        drone(&mut s, heavy, x, z, "hall");
    }
    for (heavy, x, z) in [(false, -9.0, -42.0), (false, 9.0, -47.0), (false, 0.0, -55.0), (true, 8.0, -36.0), (true, -8.0, -54.0)] {
        drone(&mut s, heavy, x, z, "core");
    }

    // ------------------------------------------------------------ the player
    let trooper = s.add_empty("Trooper");
    {
        let e = s.get_mut(trooper).unwrap();
        e.position = v(0.0, 0.95, 41.0);
        e.scale = v(0.6, 1.8, 0.6);
        e.tag = "player".into();
        let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Box);
        p.friction = 0.0;
        e.physics = Some(p);
        e.script = script("Trooper", &[]);
    }
    // The head is not a child of the body (whose scale is uneven); the script carries it along.
    let head = s.add_camera();
    {
        let e = s.get_mut(head).unwrap();
        e.name = "Head".into();
        e.position = v(0.0, 1.67, 41.0);
        e.rotation = Vec3::ZERO;
        let c = e.camera.as_mut().unwrap();
        c.fov = 74.0;
        c.near = 0.04;
        c.far = 300.0;
    }
    let rifle = model(&mut s, "Rifle", "rifle.obj", v(0.19, -0.19, -0.36), 0.0);
    {
        let e = s.get_mut(rifle).unwrap();
        e.parent = Some(head);
        e.scale = Vec3::splat(0.6);
        let m = e.mesh.as_mut().unwrap();
        m.cast_shadow = false;
        m.shininess = 0.7;
    }
    let flash = point(&mut s, "Muzzle Flash", v(0.17, -0.1, -1.2), [1.0, 0.75, 0.4], 4.0, 9.0);
    s.get_mut(flash).unwrap().parent = Some(head);
    let command = s.add_empty("Command");
    s.get_mut(command).unwrap().script = script("Command", &[]);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{InputState, LogLevel, LogLine, Runtime};
    use cryo_lang::Value;

    struct Game {
        scene: Scene,
        rt: Runtime,
        logs: Vec<LogLine>,
        dir: std::path::PathBuf,
    }

    impl Game {
        fn start(tag: &str) -> Game {
            let dir = std::env::temp_dir().join(format!("cryo_outpost_{tag}_{}", std::process::id()));
            std::fs::remove_dir_all(&dir).ok();
            let rel = create(&dir).unwrap();
            let mut scene = Scene::from_json(&std::fs::read_to_string(dir.join(rel)).unwrap()).unwrap();
            let mut logs = vec![];
            let rt = Runtime::start(&mut scene, &dir, &mut logs);
            let mut g = Game { scene, rt, logs, dir };
            g.run(0.3, InputState::default());
            g
        }
        fn run(&mut self, seconds: f32, mut input: InputState) {
            for _ in 0..(seconds * 60.0).round().max(1.0) as usize {
                self.rt.update(&mut self.scene, &input, 1.0 / 60.0, &mut self.logs);
                input.pressed.clear();
                input.mouse_dx = 0.0;
                input.mouse_dy = 0.0;
            }
            let errs: Vec<_> = self.logs.iter().filter(|l| l.level == LogLevel::Error).map(|l| l.text.clone()).collect();
            assert!(errs.is_empty(), "{errs:?}");
        }
        fn held(keys: &[&str]) -> InputState {
            let mut i = InputState::default();
            i.down = keys.iter().map(|s| s.to_string()).collect();
            i
        }
        fn pos(&self, name: &str) -> Vec3 {
            self.scene.get(self.scene.find_by_name(name).unwrap()).unwrap().position
        }
        fn bar(&self, name: &str) -> f32 {
            self.rt.bars().iter().find(|b| b.0 == name).map(|b| b.1).unwrap()
        }
        fn foes(&self, area: &str) -> Vec<u64> {
            self.scene
                .entities
                .iter()
                .filter(|e| e.tag == "enemy")
                .filter(|e| e.script.as_ref().map(|s| s.props.get("area") == Some(&PropValue::Text(area.into()))).unwrap_or(false))
                .map(|e| e.id)
                .collect()
        }
        fn place(&mut self, x: f32, z: f32) {
            let id = self.scene.find_by_name("Trooper").unwrap();
            let e = self.scene.get_mut(id).unwrap();
            e.position.x = x;
            e.position.z = z;
        }
    }

    impl Drop for Game {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    #[test]
    fn models_load_and_the_level_boots() {
        let mut g = Game::start("boot");
        for (name, _) in files::FILES {
            if name.ends_with(".obj") {
                let m = crate::assets::load_model(&g.dir.join("models").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
                assert!(m.triangles > 0);
            }
        }
        assert!(create(&g.dir).is_err(), "never overwrites a project");
        assert_eq!(g.bar("Health"), 1.0);
        assert!(g.rt.hud.starts_with("Clear the yard\nAmmo: 30 / 120"), "{}", g.rt.hud);
        assert!(g.rt.crosshair() && g.rt.mouse_locked());
        assert_eq!((g.foes("yard").len(), g.foes("hall").len(), g.foes("core").len()), (5, 6, 5));
        // the camera rides on the trooper's shoulders and W walks north
        let z = g.pos("Trooper").z;
        g.run(1.0, Game::held(&["w"]));
        assert!(z - g.pos("Trooper").z > 4.5);
        assert!((g.pos("Head") - g.pos("Trooper") - Vec3::new(0.0, 0.72, 0.0)).length() < 0.2);
    }

    #[test]
    fn rifle_shoots_what_the_crosshair_is_on() {
        let mut g = Game::start("shoot");
        // stand 9 m south of a drone, behind nothing, and tilt the view up to it
        let target = g.foes("yard")[2];
        let d = g.scene.get(target).unwrap().position;
        g.place(d.x, d.z + 9.0);
        let eye_y = g.pos("Trooper").y + 0.72;
        let up = ((d.y - eye_y) / 9.0).atan().to_degrees();
        let mut aim = InputState::default();
        aim.mouse_dy = -up / 0.12;
        g.run(0.05, aim);
        let before = g.foes("yard").len();
        g.run(1.2, Game::held(&["mouse_left"]));
        assert!(g.scene.get(target).is_none(), "the drone in the sights is destroyed");
        assert_eq!(g.foes("yard").len(), before - 1);
        assert!(g.bar("Magazine") < 0.9, "firing spends the magazine");
        // empty the magazine: it reloads from the reserve by itself
        g.run(4.0, Game::held(&["mouse_left"]));
        g.run(2.0, InputState::default());
        assert!(g.rt.hud.contains("/ 90"), "one magazine came out of the reserve: {}", g.rt.hud);
        // a wall stops bullets: nothing in the warehouse was touched
        assert_eq!(g.foes("hall").len(), 6);
    }

    #[test]
    fn drones_hunt_and_the_trooper_respawns() {
        let mut g = Game::start("hunt");
        g.place(0.0, 26.0);
        g.run(6.0, InputState::default());
        let health = g.bar("Health");
        assert!(health < 1.0, "standing in the open gets the trooper shot");
        let id = g.scene.find_by_name("Trooper").unwrap();
        g.rt.send_for_test(id, "hit", vec![Value::Num(500.0), Value::Vec3([0.0; 3])]);
        g.run(0.5, InputState::default());
        assert_eq!(g.rt.banner(), Some("YOU WERE KILLED"));
        g.run(3.5, InputState::default());
        assert_eq!(g.bar("Health"), 1.0);
        assert!((g.pos("Trooper").z - 41.0).abs() < 1.5);
    }

    #[test]
    fn clearing_sectors_opens_doors_and_the_nodes_end_the_mission() {
        let mut g = Game::start("mission");
        let kill = |g: &mut Game, ids: Vec<u64>| {
            for id in ids {
                g.rt.send_for_test(id, "hit", vec![Value::Num(999.0), Value::Vec3([0.0; 3])]);
            }
        };
        let yard = g.foes("yard");
        kill(&mut g, yard);
        g.run(3.0, InputState::default());
        assert!(g.pos("Yard Door").y > 4.0 && g.pos("Reactor Door").y < 0.1);
        assert!(g.rt.hud.starts_with("Door open"), "{}", g.rt.hud);
        let hall = g.foes("hall");
        kill(&mut g, hall);
        g.run(3.0, InputState::default());
        assert!(g.pos("Reactor Door").y > 4.0);
        let nodes: Vec<u64> = g.scene.entities.iter().filter(|e| e.tag == "target").map(|e| e.id).collect();
        assert_eq!(nodes.len(), 3);
        kill(&mut g, nodes[..2].to_vec());
        g.run(0.5, InputState::default());
        assert!(g.rt.hud.starts_with("Destroy the coolant nodes: 1 left"), "{}", g.rt.hud);
        kill(&mut g, nodes[2..].to_vec());
        g.run(0.5, InputState::default());
        assert_eq!(g.rt.banner(), Some("OUTPOST SECURED"));
        assert!(g.rt.hud.starts_with("Reactor offline"));
    }
}
