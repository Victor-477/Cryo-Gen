//! Project on disk: folders, example scene and file utilities.

use crate::scene::*;
use glam::Vec3;
use std::path::{Path, PathBuf};

pub const SCENE_EXT: &str = "cryoscene";
pub const SCRIPT_EXT: &str = "cryo";
pub const FIRST_PERSON: &str = include_str!("../template/scripts/FirstPerson.cryo");
pub const NEW_SCRIPT: &str = include_str!("../template/NewScript.cryo");

const TEMPLATE_SCRIPTS: &[(&str, &str)] = &[
    ("Player.cryo", include_str!("../template/scripts/Player.cryo")),
    ("Cap.cryo", include_str!("../template/scripts/Cap.cryo")),
    ("OrbitCamera.cryo", include_str!("../template/scripts/OrbitCamera.cryo")),
    ("Coin.cryo", include_str!("../template/scripts/Coin.cryo")),
    ("Shard.cryo", include_str!("../template/scripts/Shard.cryo")),
    ("Checkpoint.cryo", include_str!("../template/scripts/Checkpoint.cryo")),
    ("MovingPlatform.cryo", include_str!("../template/scripts/MovingPlatform.cryo")),
    ("BouncePad.cryo", include_str!("../template/scripts/BouncePad.cryo")),
    ("Spinner.cryo", include_str!("../template/scripts/Spinner.cryo")),
    ("Spiker.cryo", include_str!("../template/scripts/Spiker.cryo")),
    ("Hopper.cryo", include_str!("../template/scripts/Hopper.cryo")),
    ("HintZone.cryo", include_str!("../template/scripts/HintZone.cryo")),
    ("FollowCamera.cryo", include_str!("../template/scripts/FollowCamera.cryo")),
    ("FreeCamera.cryo", include_str!("../template/scripts/FreeCamera.cryo")),
    ("FirstPerson.cryo", FIRST_PERSON),
];

/// Example model (an 8-faced crystal) to demonstrate .obj import.
const CRYSTAL_OBJ: &str = "# Cryo Engine example crystal\n\
v 0 1 0\nv 0 -1 0\nv 0.5 0 0\nv 0 0 0.5\nv -0.5 0 0\nv 0 0 -0.5\n\
f 1 4 3\nf 1 5 4\nf 1 6 5\nf 1 3 6\nf 2 3 4\nf 2 4 5\nf 2 5 6\nf 2 6 3\n";

/// Makes sure the project folder exists. If it is empty, creates the example game.
/// Returns the (relative) path of the scene to open.
pub fn ensure_project(dir: &Path) -> Result<String, String> {
    let err = |e: std::io::Error| format!("could not prepare the project at {}: {e}", dir.display());
    std::fs::create_dir_all(dir.join("scripts")).map_err(err)?;
    std::fs::create_dir_all(dir.join("scenes")).map_err(err)?;

    if let Some(first) = list_files(dir, SCENE_EXT).into_iter().next() {
        return Ok(first);
    }
    for (name, src) in TEMPLATE_SCRIPTS {
        let p = dir.join("scripts").join(name);
        if !p.exists() {
            std::fs::write(p, src).map_err(err)?;
        }
    }
    // example assets: synthesised sounds, checker texture and an .obj model
    use crate::audio::synth_wav;
    use std::f32::consts::TAU;
    for sub in ["sounds", "textures", "models"] {
        std::fs::create_dir_all(dir.join(sub)).map_err(err)?;
    }
    let tone = |f: f32, t: f32| (t * f * TAU).sin();
    let sweep = |f0: f32, f1: f32, len: f32, t: f32| (TAU * (f0 * t + (f1 - f0) * t * t / (2.0 * len))).sin();
    let sounds: [(&str, Vec<u8>); 10] = [
        ("coin", synth_wav(0.35, |t| tone(if t < 0.08 { 988.0 } else { 1319.0 }, t) * (-t * 9.0).exp() * 0.6)),
        ("jump", synth_wav(0.18, |t| sweep(300.0, 900.0, 0.18, t) * (1.0 - t / 0.18) * 0.5)),
        ("throw", synth_wav(0.16, |t| sweep(900.0, 350.0, 0.16, t) * (1.0 - t / 0.16) * 0.4)),
        ("bounce", synth_wav(0.3, |t| sweep(220.0, 880.0, 0.3, t) * (-t * 6.0).exp() * 0.6)),
        ("stomp", synth_wav(0.2, |t| sweep(400.0, 90.0, 0.2, t) * (-t * 10.0).exp() * 0.7)),
        ("pound", synth_wav(0.3, |t| sweep(160.0, 40.0, 0.3, t) * (-t * 8.0).exp() * 0.9)),
        ("hurt", synth_wav(0.3, |t| (tone(180.0, t) + tone(187.0, t)) * (-t * 7.0).exp() * 0.4)),
        ("capture", synth_wav(0.45, |t| tone(440.0 + 660.0 * (t * 6.0).floor().min(2.0), t) * (-t * 4.0).exp() * 0.5)),
        (
            "shard",
            synth_wav(0.9, |t| {
                let notes = [523.0, 659.0, 784.0, 1047.0];
                let i = ((t / 0.12) as usize).min(3);
                tone(notes[i], t) * (-(t - i as f32 * 0.12).max(0.0) * 3.0).exp() * 0.5
            }),
        ),
        ("checkpoint", synth_wav(0.4, |t| tone(if t < 0.15 { 660.0 } else { 880.0 }, t) * (-t * 5.0).exp() * 0.5)),
    ];
    for (name, wav) in sounds {
        std::fs::write(dir.join(format!("sounds/{name}.wav")), wav).map_err(err)?;
    }
    let checker = image::RgbaImage::from_fn(256, 256, |x, y| {
        let on = ((x / 32) + (y / 32)) % 2 == 0;
        let v = if on { 240 } else { 205 };
        image::Rgba([v, v, v, 255])
    });
    checker.save(dir.join("textures/checker.png")).map_err(|e| format!("example texture: {e}"))?;
    std::fs::write(dir.join("models/crystal.obj"), CRYSTAL_OBJ).map_err(err)?;

    let rel = "scenes/Platformer.cryoscene".to_string();
    std::fs::write(dir.join(&rel), template_scene().to_json()).map_err(err)?;
    Ok(rel)
}

/// Lists (recursively) the files with the given extension, relative to the project.
pub fn list_files(dir: &Path, ext: &str) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, ext: &str, out: &mut Vec<String>, depth: usize) {
        if depth > 6 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(base, &p, ext, out, depth + 1);
            } else if p.extension().and_then(|e| e.to_str()) == Some(ext) {
                if let Ok(rel) = p.strip_prefix(base) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    let mut out = vec![];
    walk(dir, dir, ext, &mut out, 0);
    out.sort();
    out
}

pub struct DirEntry {
    pub name: String,
    pub rel: String,
    pub is_dir: bool,
}

pub fn list_dir(project: &Path, rel: &str) -> Vec<DirEntry> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(project.join(rel)) else { return out };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let child = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        out.push(DirEntry { name, rel: child, is_dir: entry.path().is_dir() });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

/// Turns any text into a valid identifier for a script name.
pub fn script_ident(name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if upper {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            upper = false;
        } else {
            upper = true;
        }
    }
    if out.is_empty() || out.chars().next().unwrap().is_numeric() {
        out.insert_str(0, "Script");
    }
    out
}

/// Creates a new script (never overwriting) and returns its relative path.
pub fn create_script(project: &Path, base: &str) -> Result<String, String> {
    let ident = script_ident(base);
    let mut name = ident.clone();
    let mut i = 2;
    let path = loop {
        let p: PathBuf = project.join("scripts").join(format!("{name}.{SCRIPT_EXT}"));
        if !p.exists() {
            break p;
        }
        name = format!("{ident}{i}");
        i += 1;
    };
    std::fs::create_dir_all(project.join("scripts")).map_err(|e| e.to_string())?;
    std::fs::write(&path, NEW_SCRIPT.replace("NAME", &name)).map_err(|e| e.to_string())?;
    Ok(format!("scripts/{name}.{SCRIPT_EXT}"))
}

/// Example game: "Frostpeak Isles", a 3D platformer.
///
/// Five Frost Shards are spread over a chain of floating islands. Each stretch teaches one
/// move: jumping, moving platforms, enemies, capturing a Hopper, wall jumps, the cap bounce
/// and bounce pads. All gameplay lives in the `.cryo` scripts; this function only places things.
pub fn template_scene() -> Scene {
    let mut s = Scene::empty();
    s.gravity = -24.0;
    s.ambient = [0.42, 0.46, 0.55];
    s.sky_top = [0.20, 0.42, 0.78];
    s.sky_horizon = [0.78, 0.88, 0.97];

    fn script(path: &str) -> Option<ScriptRef> {
        Some(ScriptRef { path: format!("scripts/{path}.cryo"), ..Default::default() })
    }
    fn script_with(path: &str, props: &[(&str, PropValue)]) -> Option<ScriptRef> {
        let mut r = ScriptRef { path: format!("scripts/{path}.cryo"), ..Default::default() };
        for (k, v) in props {
            r.props.insert(k.to_string(), v.clone());
        }
        Some(r)
    }
    /// A mesh entity; returns its id.
    fn mesh(s: &mut Scene, name: &str, shape: Shape, pos: Vec3, scale: Vec3, color: [f32; 3]) -> u64 {
        let id = s.add_mesh(shape);
        let e = s.get_mut(id).unwrap();
        e.name = name.into();
        e.position = pos;
        e.scale = scale;
        e.mesh.as_mut().unwrap().color = color;
        id
    }
    /// A static, solid box.
    fn block(s: &mut Scene, name: &str, pos: Vec3, scale: Vec3, color: [f32; 3]) -> u64 {
        let id = mesh(s, name, Shape::Cube, pos, scale, color);
        s.get_mut(id).unwrap().physics = Some(Physics::new(BodyKind::Static, ColliderShape::Box));
        id
    }
    fn child(s: &mut Scene, parent: u64, id: u64) -> u64 {
        s.get_mut(id).unwrap().parent = Some(parent);
        id
    }
    fn trigger(kind: BodyKind, shape: ColliderShape) -> Option<Physics> {
        let mut p = Physics::new(kind, shape);
        p.is_trigger = true;
        p.use_gravity = false;
        Some(p)
    }
    let v = Vec3::new;
    let grass = [0.42, 0.72, 0.45];
    let stone = [0.62, 0.64, 0.70];
    let wood = [0.72, 0.55, 0.38];
    let snow = [0.92, 0.95, 1.0];

    // ---------------------------------------------------------------- camera and lights
    let cam = s.add_camera();
    {
        let e = s.get_mut(cam).unwrap();
        e.position = v(0.0, 5.0, 13.0);
        e.rotation = v(-18.0, 0.0, 0.0);
        e.camera.as_mut().unwrap().fov = 62.0;
        e.script = script("OrbitCamera");
    }
    let sun = s.add_light(LightKind::Directional);
    {
        let e = s.get_mut(sun).unwrap();
        e.name = "Sun".into();
        e.position = v(10.0, 20.0, 10.0);
        e.rotation = v(-52.0, 40.0, 0.0);
        e.light.as_mut().unwrap().intensity = 1.15;
    }

    // ---------------------------------------------------------------- the hero
    // Root: collider + script, no mesh. `KipModel` cancels the root's scale and turns to face
    // the movement direction, so the collider box itself never rotates.
    let player = s.add_empty("Player");
    {
        let e = s.get_mut(player).unwrap();
        e.position = v(0.0, 0.85, 4.0);
        e.scale = v(0.8, 1.6, 0.8);
        let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Box);
        p.friction = 0.0;
        e.physics = Some(p);
        e.script = script("Player");
    }
    let model = s.add_empty("KipModel");
    child(&mut s, player, model);
    s.get_mut(model).unwrap().scale = v(1.25, 0.625, 1.25);
    let blue = [0.22, 0.55, 0.98];
    let red = [0.92, 0.22, 0.25];
    let id = mesh(&mut s, "KipBody", Shape::Capsule, v(0.0, -0.25, 0.0), v(0.72, 0.55, 0.72), blue);
    child(&mut s, model, id);
    let id = mesh(&mut s, "KipHead", Shape::Sphere, v(0.0, 0.42, 0.0), Vec3::splat(0.62), [1.0, 0.86, 0.72]);
    child(&mut s, model, id);
    for x in [-0.13, 0.13] {
        let id = mesh(&mut s, "KipEye", Shape::Sphere, v(x, 0.47, -0.27), Vec3::splat(0.11), [0.08, 0.08, 0.1]);
        child(&mut s, model, id);
    }
    let hat = s.add_empty("KipHat");
    child(&mut s, model, hat);
    s.get_mut(hat).unwrap().position = v(0.0, 0.70, 0.0);
    let id = mesh(&mut s, "HatBrim", Shape::Cylinder, v(0.0, 0.0, 0.0), v(0.80, 0.06, 0.80), red);
    child(&mut s, hat, id);
    let id = mesh(&mut s, "HatTop", Shape::Cylinder, v(0.0, 0.15, 0.0), v(0.50, 0.28, 0.50), red);
    child(&mut s, hat, id);

    // The thrown cap is a separate entity (hidden while Kip wears his hat).
    let cap = s.add_empty("Cap");
    {
        let e = s.get_mut(cap).unwrap();
        e.position = v(0.0, -50.0, 0.0);
        e.scale = v(1.4, 0.7, 1.4);
        e.physics = trigger(BodyKind::Dynamic, ColliderShape::Sphere);
        e.script = script("Cap");
    }
    let id = mesh(&mut s, "CapBrim", Shape::Cylinder, v(0.0, 0.0, 0.0), v(0.64, 0.1, 0.64), red);
    child(&mut s, cap, id);
    let id = mesh(&mut s, "CapTop", Shape::Cylinder, v(0.0, 0.25, 0.0), v(0.4, 0.45, 0.4), red);
    child(&mut s, cap, id);

    // ---------------------------------------------------------------- reusable pieces
    let coin = |s: &mut Scene, parent: u64, pos: Vec3| {
        let id = mesh(s, "Coin", Shape::Cylinder, pos, v(0.6, 0.07, 0.6), [1.0, 0.82, 0.18]);
        let e = s.get_mut(id).unwrap();
        e.parent = Some(parent);
        e.rotation = v(90.0, 0.0, 0.0);
        e.mesh.as_mut().unwrap().shininess = 1.0;
        e.physics = trigger(BodyKind::Static, ColliderShape::Sphere);
        e.script = script("Coin");
    };
    let shard = |s: &mut Scene, n: usize, pos: Vec3| {
        let id = mesh(s, &format!("Frost Shard {n}"), Shape::Cube, pos, Vec3::splat(0.9), [0.45, 0.95, 1.0]);
        let e = s.get_mut(id).unwrap();
        let m = e.mesh.as_mut().unwrap();
        m.model = Some("models/crystal.obj".into());
        m.model_bounds = Some(([0.0; 3], [0.5, 1.0, 0.5]));
        m.shininess = 1.0;
        e.physics = trigger(BodyKind::Static, ColliderShape::Sphere);
        e.script = script("Shard");
        // a soft glow so shards can be spotted from far away
        let l = s.add_light(LightKind::Point);
        let e = s.get_mut(l).unwrap();
        e.name = "Glow".into();
        e.parent = Some(id);
        e.position = Vec3::ZERO;
        let light = e.light.as_mut().unwrap();
        light.color = [0.4, 0.9, 1.0];
        light.intensity = 2.0;
        light.range = 6.0;
    };
    let checkpoint = |s: &mut Scene, pos: Vec3| {
        // a flat pad the player walks over; it turns green once activated
        let id = mesh(s, "Checkpoint", Shape::Cylinder, pos + v(0.0, 0.06, 0.0), v(2.4, 0.12, 2.4), [0.95, 0.35, 0.35]);
        let e = s.get_mut(id).unwrap();
        e.physics = trigger(BodyKind::Static, ColliderShape::Box);
        e.script = script("Checkpoint");
        let pole = mesh(s, "Pole", Shape::Cylinder, v(0.4, 10.0, 0.4), v(0.05, 20.0, 0.05), [0.9, 0.9, 0.95]);
        s.get_mut(pole).unwrap().parent = Some(id);
        let flag = mesh(s, "Flag", Shape::Cube, v(0.26, 17.0, 0.4), v(0.26, 5.0, 0.02), [0.3, 0.7, 1.0]);
        s.get_mut(flag).unwrap().parent = Some(id);
    };
    let hint = |s: &mut Scene, pos: Vec3, size: Vec3, text: &str| {
        let id = s.add_empty("Hint");
        let e = s.get_mut(id).unwrap();
        e.position = pos;
        e.scale = size;
        e.physics = trigger(BodyKind::Static, ColliderShape::Box);
        e.script = script_with("HintZone", &[("message", PropValue::Text(text.into()))]);
    };
    let tree = |s: &mut Scene, parent: u64, pos: Vec3, h: f32| {
        let t = mesh(s, "Trunk", Shape::Cylinder, pos + v(0.0, h * 0.2, 0.0), v(0.35, h * 0.4, 0.35), [0.45, 0.32, 0.22]);
        s.get_mut(t).unwrap().parent = Some(parent);
        let c = mesh(s, "Pine", Shape::Cone, pos + v(0.0, h * 0.7, 0.0), v(1.6, h * 0.8, 1.6), [0.20, 0.52, 0.36]);
        s.get_mut(c).unwrap().parent = Some(parent);
    };

    let coins = s.add_empty("Coins");
    let scenery = s.add_empty("Scenery");

    // sea far below (falling into it sends Kip back to the last checkpoint)
    let sea = mesh(&mut s, "Sea", Shape::Plane, v(0.0, -7.0, -45.0), v(500.0, 1.0, 500.0), [0.16, 0.42, 0.72]);
    {
        let m = s.get_mut(sea).unwrap().mesh.as_mut().unwrap();
        m.shininess = 1.0;
        m.cast_shadow = false;
    }

    // ---------------------------------------------------------------- 1. Start Isle
    let g = block(&mut s, "Start Isle", v(0.0, -1.0, 0.0), v(22.0, 2.0, 22.0), grass);
    s.get_mut(g).unwrap().mesh.as_mut().unwrap().texture = Some("textures/checker.png".into());
    checkpoint(&mut s, v(3.0, 0.0, 6.0));
    for i in 0..5 {
        coin(&mut s, coins, v(0.0, 0.8, 1.0 - i as f32 * 2.0));
    }
    tree(&mut s, scenery, v(8.0, 0.0, 7.0), 3.5);
    tree(&mut s, scenery, v(-8.5, 0.0, 8.0), 4.5);
    tree(&mut s, scenery, v(9.0, 0.0, -7.0), 3.0);
    // a step and a tall pillar: reach the top with a backflip or a triple jump
    block(&mut s, "Step", v(-7.0, 0.8, -2.5), v(3.0, 1.6, 3.0), stone);
    block(&mut s, "Pillar", v(-7.0, 1.75, -6.5), v(3.0, 3.5, 3.0), stone);
    shard(&mut s, 1, v(-7.0, 4.9, -6.5));
    hint(
        &mut s,
        v(-7.0, 1.0, 0.5),
        v(5.0, 2.0, 3.0),
        "Shift + Space: backflip.  Three jumps in a row while running: triple jump.",
    );
    hint(
        &mut s,
        v(0.0, 1.0, -9.0),
        v(8.0, 2.0, 3.0),
        "Shift + Space while running: long jump.  Shift in the air: ground pound.",
    );

    // ---------------------------------------------------------------- 2. stepping stones
    block(&mut s, "Stone 1", v(0.0, 0.5, -15.5), v(4.0, 1.0, 4.0), wood);
    block(&mut s, "Stone 2", v(3.5, 1.5, -21.0), v(4.0, 1.0, 4.0), wood);
    block(&mut s, "Stone 3", v(0.0, 2.5, -26.5), v(4.0, 1.0, 4.0), wood);
    coin(&mut s, coins, v(0.0, 1.8, -15.5));
    coin(&mut s, coins, v(3.5, 2.8, -21.0));
    coin(&mut s, coins, v(0.0, 3.8, -26.5));

    // ---------------------------------------------------------------- 3. moving platform
    let p = block(&mut s, "Moving Platform", v(0.0, 2.75, -32.0), v(4.0, 0.5, 4.0), [0.95, 0.75, 0.30]);
    s.get_mut(p).unwrap().script = script("MovingPlatform");
    for i in 0..3 {
        coin(&mut s, coins, v(0.0, 4.6, -34.0 - i as f32 * 2.5));
    }

    // ---------------------------------------------------------------- 4. Mid Isle
    let g = block(&mut s, "Mid Isle", v(0.0, 1.5, -54.0), v(24.0, 3.0, 20.0), grass);
    s.get_mut(g).unwrap().mesh.as_mut().unwrap().texture = Some("textures/checker.png".into());
    checkpoint(&mut s, v(-2.5, 3.0, -46.0));
    tree(&mut s, scenery, v(10.5, 3.0, -46.0), 4.0);
    tree(&mut s, scenery, v(-10.5, 3.0, -62.5), 3.5);
    for x in [-3.0, 3.0] {
        let id = mesh(&mut s, "Spiker", Shape::Sphere, v(x, 3.6, -50.0 - x), Vec3::splat(1.1), [0.75, 0.25, 0.55]);
        let e = s.get_mut(id).unwrap();
        let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Box);
        p.friction = 0.0;
        e.physics = Some(p);
        e.script = script("Spiker");
        let spike = mesh(&mut s, "Spike", Shape::Cone, v(0.0, 0.55, 0.0), v(0.5, 0.5, 0.5), [0.95, 0.9, 0.95]);
        child(&mut s, id, spike);
    }
    hint(
        &mut s,
        v(0.0, 4.0, -47.0),
        v(10.0, 2.0, 2.0),
        "Spikers: jump on their heads or hit them with your cap (F).",
    );
    // the Hopper and the tower only it can jump onto
    let hopper = mesh(&mut s, "Hopper", Shape::Sphere, v(5.0, 3.7, -53.0), v(1.3, 1.2, 1.3), [0.45, 0.85, 0.5]);
    {
        let e = s.get_mut(hopper).unwrap();
        let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Box);
        p.friction = 1.0;
        e.physics = Some(p);
        e.script = script("Hopper");
    }
    for x in [-0.2, 0.2] {
        let eye = mesh(&mut s, "HopperEye", Shape::Sphere, v(x, 0.25, -0.42), Vec3::splat(0.16), [0.05, 0.05, 0.08]);
        child(&mut s, hopper, eye);
    }
    block(&mut s, "Tower", v(8.0, 6.25, -60.0), v(6.0, 6.5, 6.0), stone);
    shard(&mut s, 2, v(8.0, 10.9, -60.0));
    hint(
        &mut s,
        v(5.0, 4.0, -53.0),
        v(7.0, 2.0, 5.0),
        "Throw your cap (F) at the Hopper to capture it. Its jump reaches the tower!",
    );
    // wall-jump shaft
    block(&mut s, "Wall A", v(-10.0, 8.0, -58.0), v(1.0, 10.0, 5.0), stone);
    block(&mut s, "Wall B", v(-6.0, 8.0, -58.0), v(1.0, 10.0, 5.0), stone);
    for i in 0..3 {
        coin(&mut s, coins, v(-8.0, 5.0 + i as f32 * 1.6, -58.0));
    }
    shard(&mut s, 3, v(-8.0, 10.5, -58.0));
    hint(
        &mut s,
        v(-8.0, 4.0, -54.0),
        v(4.0, 2.0, 2.5),
        "Jump against a wall, then press Space again to wall-jump. Climb between the walls!",
    );

    // ---------------------------------------------------------------- 5. cap gap
    hint(
        &mut s,
        v(0.0, 4.0, -62.5),
        v(8.0, 2.0, 2.5),
        "Too far to jump: throw the cap, keep F held, then jump onto it to bounce across.",
    );
    for i in 0..4 {
        let t = i as f32;
        coin(&mut s, coins, v(0.0, 4.2 + (1.5 - (t - 1.5).abs()) * 0.6, -66.0 - t * 2.2));
    }
    let g = block(&mut s, "Cap Isle", v(0.0, 1.5, -80.5), v(14.0, 3.0, 13.0), snow);
    s.get_mut(g).unwrap().mesh.as_mut().unwrap().texture = Some("textures/checker.png".into());
    checkpoint(&mut s, v(-4.0, 3.0, -76.5));
    shard(&mut s, 4, v(-3.0, 4.3, -82.0));
    tree(&mut s, scenery, v(-5.5, 3.0, -85.0), 3.5);

    // ---------------------------------------------------------------- 6. the summit
    let pad = block(&mut s, "Bounce Pad", v(4.0, 3.15, -84.0), v(2.4, 0.3, 2.4), [0.98, 0.45, 0.75]);
    s.get_mut(pad).unwrap().script = script("BouncePad");
    hint(&mut s, v(4.0, 4.0, -81.5), v(4.0, 2.0, 2.0), "Bounce pads launch you high. Aim for the summit!");
    block(&mut s, "Summit", v(4.0, 10.5, -93.0), v(9.0, 1.0, 9.0), snow);
    let bar = block(&mut s, "Spinner", v(4.0, 11.4, -93.0), v(8.0, 0.6, 0.6), [0.95, 0.35, 0.3]);
    s.get_mut(bar).unwrap().script = script("Spinner");
    block(&mut s, "Spinner Hub", v(4.0, 11.4, -93.0), v(0.9, 0.8, 0.9), stone);
    shard(&mut s, 5, v(4.0, 12.6, -96.2));
    tree(&mut s, scenery, v(7.5, 11.0, -96.5), 3.0);

    // ---------------------------------------------------------------- animations
    // "Intro": a cutscene. A second camera flies from the summit back to the start, then a
    // marker cuts to the gameplay camera and another one sends the player a hint.
    use crate::anim::{Action, Clip, Ease, Marker, Property};
    let intro_cam = s.add_camera();
    s.get_mut(intro_cam).unwrap().name = "Intro Camera".into();
    let shots = [
        (0.0, v(16.0, 17.0, -104.0), v(4.0, 11.0, -93.0)),
        (2.6, v(-14.0, 13.0, -70.0), v(0.0, 4.0, -56.0)),
        (5.0, v(9.0, 8.0, -18.0), v(0.0, 1.0, -6.0)),
        (7.0, v(0.0, 4.7, 12.3), v(0.0, 2.0, 4.0)),
    ];
    let mut intro = Clip::new("Intro");
    intro.duration = 7.0;
    intro.autoplay = true;
    intro.cutscene = true;
    for (t, pos, target) in shots {
        // let the entity work out the rotation that looks at the target
        let e = s.get_mut(intro_cam).unwrap();
        e.position = pos;
        e.look_at(target);
        let rot = e.rotation;
        intro.track_mut(intro_cam, Property::Position).set_key(t, pos.to_array());
        intro.track_mut(intro_cam, Property::Rotation).set_key(t, rot.to_array());
    }
    intro.markers.push(Marker { t: 0.0, action: Action::Camera { entity: intro_cam } });
    intro.markers.push(Marker { t: 7.0, action: Action::Camera { entity: cam } });
    intro.markers.push(Marker {
        t: 7.0,
        action: Action::Send {
            entity: player,
            event: "hint".into(),
            arg: "Find the five Frost Shards!  WASD: move   Space: jump   F: throw cap".into(),
        },
    });
    s.animations.push(intro);

    // "Victory": real-time animation started from Player.cryo with anim.play("Victory").
    // A beacon of light rises from the summit while a sound plays in sync.
    let beacon = mesh(&mut s, "Beacon", Shape::Cylinder, v(4.0, 11.0, -93.0), v(0.01, 0.01, 0.01), [0.5, 0.95, 1.0]);
    {
        let m = s.get_mut(beacon).unwrap().mesh.as_mut().unwrap();
        m.unlit = true;
        m.cast_shadow = false;
    }
    let mut victory = Clip::new("Victory");
    victory.duration = 3.0;
    let scale = victory.track_mut(beacon, Property::Scale);
    scale.set_key(0.0, [0.01, 0.01, 0.01]);
    scale.set_key(0.6, [2.4, 0.3, 2.4]);
    scale.set_key(3.0, [0.7, 120.0, 0.7]);
    scale.keys[0].ease = Ease::Linear;
    let colour = victory.track_mut(beacon, Property::Color);
    colour.set_key(0.0, [1.0, 1.0, 1.0]);
    colour.set_key(3.0, [0.4, 0.9, 1.0]);
    victory.markers.push(Marker { t: 0.0, action: Action::Sound { file: "sounds/shard.wav".into(), volume: 1.0 } });
    victory.markers.push(Marker { t: 0.9, action: Action::Sound { file: "sounds/checkpoint.wav".into(), volume: 0.8 } });
    s.animations.push(victory);

    s
}

/// Destination folder for each kind of imported file.
pub fn import_folder(ext: &str) -> Option<&'static str> {
    use crate::assets::{IMAGE_EXTS, MODEL_EXTS, SOUND_EXTS};
    if MODEL_EXTS.contains(&ext) {
        Some("models")
    } else if IMAGE_EXTS.contains(&ext) {
        Some("textures")
    } else if SOUND_EXTS.contains(&ext) {
        Some("sounds")
    } else if ext == SCRIPT_EXT {
        Some("scripts")
    } else {
        None
    }
}

/// Files a model references (materials, buffers, textures), relative to its folder.
fn model_dependencies(src: &Path) -> Vec<String> {
    let ext = crate::assets::ext_of(&src.to_string_lossy());
    let dir = src.parent().unwrap_or(Path::new("."));
    let mut out = vec![];
    match ext.as_str() {
        "obj" => {
            let Ok(text) = std::fs::read_to_string(src) else { return out };
            for line in text.lines() {
                if let Some(name) = line.trim().strip_prefix("mtllib ") {
                    let name = name.trim().replace('\\', "/");
                    if let Ok(mtl) = std::fs::read_to_string(dir.join(&name)) {
                        let base = Path::new(&name).parent().map(|p| p.to_path_buf()).unwrap_or_default();
                        for l in mtl.lines() {
                            let l = l.trim();
                            if l.starts_with("map_") {
                                if let Some(file) = l.split_whitespace().last() {
                                    out.push(base.join(file.replace('\\', "/")).to_string_lossy().to_string());
                                }
                            }
                        }
                    }
                    out.push(name);
                }
            }
        }
        "gltf" => {
            let Ok(text) = std::fs::read_to_string(src) else { return out };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return out };
            for key in ["buffers", "images"] {
                for item in json[key].as_array().into_iter().flatten() {
                    if let Some(uri) = item["uri"].as_str() {
                        if !uri.starts_with("data:") {
                            out.push(uri.replace("%20", " "));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// Copies an external file (and its dependencies) into the project.
/// Returns the relative path of the imported file.
pub fn import_file(project: &Path, src: &Path) -> Result<String, String> {
    let name = src.file_name().and_then(|n| n.to_str()).ok_or("invalid file name")?.to_string();
    let ext = crate::assets::ext_of(&name);
    let folder = import_folder(&ext).ok_or_else(|| {
        format!("`{name}`: unsupported type (models .obj/.gltf/.glb, images .png/.jpg, sounds .wav/.ogg/.mp3)")
    })?;
    let dest_dir = project.join(folder);
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(&name);
    // already inside the project? then there is nothing to copy
    let same = std::fs::canonicalize(src).ok() == std::fs::canonicalize(&dest).ok() && dest.exists();
    if !same {
        std::fs::copy(src, &dest).map_err(|e| format!("could not copy `{name}`: {e}"))?;
        let src_dir = src.parent().unwrap_or(Path::new("."));
        for dep in model_dependencies(src) {
            let rel = Path::new(&dep);
            // only dependencies inside the model's folder (no `..` or absolute paths)
            if rel.is_absolute() || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
                continue;
            }
            let to = dest_dir.join(rel);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::copy(src_dir.join(rel), to).ok();
        }
    }
    Ok(format!("{folder}/{name}"))
}

fn copy_dir(from: &Path, to: &Path, depth: usize) -> std::io::Result<()> {
    if depth > 12 {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let p = entry.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(&name), depth + 1)?;
        } else {
            std::fs::copy(&p, to.join(&name))?;
        }
    }
    Ok(())
}

/// Configuration written alongside the exported game.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct GameConfig {
    pub title: String,
    pub scene: String,
}

pub const GAME_CONFIG: &str = "cryo_game.json";

/// Exports the game to `dest`: a copy of the executable plus the `game/` folder with the project.
/// The executable, on finding that folder next to it, opens straight into the game (no editor).
#[cfg_attr(not(test), allow(dead_code))]
pub fn export_game(project: &Path, scene: &str, title: &str, dest: &Path) -> Result<PathBuf, String> {
    export_game_with(project, scene, title, dest, None)
}

/// The system an engine executable was built for, read from the first bytes of the file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TargetOs {
    Windows,
    MacOs,
    Linux,
}

impl TargetOs {
    pub fn current() -> TargetOs {
        if cfg!(windows) {
            TargetOs::Windows
        } else if cfg!(target_os = "macos") {
            TargetOs::MacOs
        } else {
            TargetOs::Linux
        }
    }

    pub fn of_file(path: &Path) -> Result<TargetOs, String> {
        use std::io::Read;
        let mut head = [0u8; 4];
        std::fs::File::open(path)
            .and_then(|mut f| f.read_exact(&mut head))
            .map_err(|e| format!("could not read `{}`: {e}", path.display()))?;
        match head {
            [b'M', b'Z', _, _] => Ok(TargetOs::Windows),
            [0x7F, b'E', b'L', b'F'] => Ok(TargetOs::Linux),
            [0xCF, 0xFA, 0xED, 0xFE] | [0xFE, 0xED, 0xFA, 0xCF] | [0xCA, 0xFE, 0xBA, 0xBE] => Ok(TargetOs::MacOs),
            _ => Err(format!(
                "`{}` is not a Cryo Engine executable (choose the `cryo` program from the download for that system)",
                path.display()
            )),
        }
    }
}

/// Exports the game. `runtime` is the engine executable to ship with it: `None` uses the one
/// running now; the `cryo` program downloaded for another system exports the game for that system.
pub fn export_game_with(
    project: &Path,
    scene: &str,
    title: &str,
    dest: &Path,
    runtime: Option<&Path>,
) -> Result<PathBuf, String> {
    let io = |e: std::io::Error| format!("export failed: {e}");
    let (exe, os) = match runtime {
        Some(path) => (path.to_path_buf(), TargetOs::of_file(path)?),
        None => (std::env::current_exe().map_err(io)?, TargetOs::current()),
    };
    let safe: String =
        title.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let safe = if safe.is_empty() { "Game".to_string() } else { safe };
    if dest.starts_with(project) {
        return Err("choose a folder outside the project to export to".into());
    }

    let (root, exe_dir, game_dir) = if os == TargetOs::MacOs {
        // macOS .app bundle
        let app = dest.join(format!("{safe}.app"));
        let contents = app.join("Contents");
        (app, contents.join("MacOS"), contents.join("Resources").join("game"))
    } else {
        let root = dest.join(&safe);
        (root.clone(), root.clone(), root.join("game"))
    };
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(io)?;
    }
    std::fs::create_dir_all(&exe_dir).map_err(io)?;
    let exe_name = if os == TargetOs::Windows { format!("{safe}.exe") } else { safe.clone() };
    std::fs::copy(&exe, exe_dir.join(&exe_name)).map_err(io)?;
    #[cfg(unix)]
    if os != TargetOs::Windows {
        // a runtime downloaded as a plain file may have lost its "executable" mark
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(exe_dir.join(&exe_name), std::fs::Permissions::from_mode(0o755));
    }
    copy_dir(project, &game_dir, 0).map_err(io)?;
    let config = GameConfig { title: title.to_string(), scene: scene.to_string() };
    std::fs::write(game_dir.join(GAME_CONFIG), serde_json::to_string_pretty(&config).unwrap_or_default())
        .map_err(io)?;
    if os == TargetOs::MacOs {
        let plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>CFBundleName</key><string>{title}</string>\n<key>CFBundleExecutable</key><string>{safe}</string>\n<key>CFBundleIdentifier</key><string>com.cryoengine.{safe}</string>\n<key>CFBundlePackageType</key><string>APPL</string>\n<key>CFBundleVersion</key><string>1.0</string>\n<key>NSHighResolutionCapable</key><true/>\n</dict></plist>\n"
        );
        std::fs::write(root.join("Contents").join("Info.plist"), plist).map_err(io)?;
    }
    Ok(root)
}

/// If this executable was exported as a game, returns the embedded `game/` folder.
pub fn bundled_game() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [dir.join("game"), dir.join("..").join("Resources").join("game")]
        .into_iter()
        .find(|p| p.join(GAME_CONFIG).is_file())
}

pub fn read_game_config(dir: &Path) -> Option<GameConfig> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(GAME_CONFIG)).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_obj_with_material_and_texture() {
        let base = std::env::temp_dir().join(format!("cryo_import_{}", std::process::id()));
        std::fs::remove_dir_all(&base).ok();
        let ext = base.join("outside/tex");
        std::fs::create_dir_all(&ext).unwrap();
        std::fs::write(base.join("outside/house.obj"), "mtllib house.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        std::fs::write(base.join("outside/house.mtl"), "newmtl a\nmap_Kd tex/wall.png\n").unwrap();
        std::fs::write(ext.join("wall.png"), "x").unwrap();
        let proj = base.join("proj");
        let rel = import_file(&proj, &base.join("outside/house.obj")).unwrap();
        assert_eq!(rel, "models/house.obj");
        assert!(proj.join("models/house.mtl").exists());
        assert!(proj.join("models/tex/wall.png").exists());
        assert!(import_file(&proj, &base.join("outside/house.mtl")).is_err());
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn exports_game_with_config() {
        let base = std::env::temp_dir().join(format!("cryo_export_{}", std::process::id()));
        std::fs::remove_dir_all(&base).ok();
        let proj = base.join("proj");
        let scene = ensure_project(&proj).unwrap();
        let out = export_game(&proj, &scene, "My Game", &base.join("out")).unwrap();
        let game = if cfg!(target_os = "macos") { out.join("Contents/Resources/game") } else { out.join("game") };
        assert_eq!(read_game_config(&game).unwrap().scene, scene);
        assert!(game.join("scripts/Player.cryo").exists());
        assert!(game.join("sounds/coin.wav").exists());
        assert!(export_game(&proj, &scene, "x", &proj.join("inside")).is_err());

        // with the engine program of another system, the game is laid out for that system
        let win = base.join("cryo.exe");
        std::fs::write(&win, b"MZ\x90\x00 not really a program").unwrap();
        let out = export_game_with(&proj, &scene, "My Game", &base.join("win"), Some(&win)).unwrap();
        assert!(out.join("My_Game.exe").is_file() && out.join("game/scripts/Player.cryo").is_file());
        let mac = base.join("cryo-mac");
        std::fs::write(&mac, [0xCF, 0xFA, 0xED, 0xFE, 1, 2, 3]).unwrap();
        let out = export_game_with(&proj, &scene, "My Game", &base.join("mac"), Some(&mac)).unwrap();
        assert!(out.ends_with("My_Game.app") && out.join("Contents/MacOS/My_Game").is_file());
        assert!(out.join("Contents/Info.plist").is_file() && out.join("Contents/Resources/game/sounds/coin.wav").is_file());
        let junk = base.join("notes.txt");
        std::fs::write(&junk, "hello").unwrap();
        assert!(export_game_with(&proj, &scene, "x", &base.join("junk"), Some(&junk)).is_err());
        std::fs::remove_dir_all(base).ok();
    }
}
