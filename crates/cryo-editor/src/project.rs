//! Projeto em disco: pastas, cena de exemplo e utilitários de arquivo.

use crate::scene::*;
use glam::Vec3;
use std::path::{Path, PathBuf};

pub const SCENE_EXT: &str = "cryoscene";
pub const SCRIPT_EXT: &str = "cryo";
pub const NEW_SCRIPT: &str = include_str!("../template/NovoScript.cryo");

const TEMPLATE_SCRIPTS: &[(&str, &str)] = &[
    ("Jogador.cryo", include_str!("../template/scripts/Jogador.cryo")),
    ("Moeda.cryo", include_str!("../template/scripts/Moeda.cryo")),
    ("CameraSegue.cryo", include_str!("../template/scripts/CameraSegue.cryo")),
    ("CameraOrbital.cryo", include_str!("../template/scripts/CameraOrbital.cryo")),
    ("CameraLivre.cryo", include_str!("../template/scripts/CameraLivre.cryo")),
];

/// Modelo de exemplo (um cristal de 8 faces) para demonstrar a importação de .obj.
const CRYSTAL_OBJ: &str = "# Cristal de exemplo da Cryo Engine\n\
v 0 1 0\nv 0 -1 0\nv 0.5 0 0\nv 0 0 0.5\nv -0.5 0 0\nv 0 0 -0.5\n\
f 1 4 3\nf 1 5 4\nf 1 6 5\nf 1 3 6\nf 2 3 4\nf 2 4 5\nf 2 5 6\nf 2 6 3\n";

/// Garante que a pasta do projeto existe. Se estiver vazia, cria o jogo de exemplo.
/// Devolve o caminho (relativo) da cena a abrir.
pub fn ensure_project(dir: &Path) -> Result<String, String> {
    let err = |e: std::io::Error| format!("não foi possível preparar o projeto em {}: {e}", dir.display());
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
    // recursos de exemplo: sons sintetizados, textura xadrez e um modelo .obj
    use crate::audio::synth_wav;
    use std::f32::consts::TAU;
    for sub in ["sounds", "textures", "models"] {
        std::fs::create_dir_all(dir.join(sub)).map_err(err)?;
    }
    let coin = synth_wav(0.35, |t| {
        let f = if t < 0.08 { 988.0 } else { 1319.0 };
        (t * f * TAU).sin() * (-t * 9.0).exp() * 0.6
    });
    std::fs::write(dir.join("sounds/moeda.wav"), coin).map_err(err)?;
    let jump = synth_wav(0.18, |t| (TAU * (300.0 * t + 900.0 * t * t)).sin() * (1.0 - t / 0.18) * 0.5);
    std::fs::write(dir.join("sounds/pulo.wav"), jump).map_err(err)?;
    let checker = image::RgbaImage::from_fn(256, 256, |x, y| {
        let on = ((x / 32) + (y / 32)) % 2 == 0;
        let v = if on { 235 } else { 150 };
        image::Rgba([v, v, v, 255])
    });
    checker.save(dir.join("textures/xadrez.png")).map_err(|e| format!("textura de exemplo: {e}"))?;
    std::fs::write(dir.join("models/cristal.obj"), CRYSTAL_OBJ).map_err(err)?;

    let rel = "scenes/Principal.cryoscene".to_string();
    std::fs::write(dir.join(&rel), template_scene().to_json()).map_err(err)?;
    Ok(rel)
}

/// Lista (recursivamente) os arquivos com a extensão dada, com caminho relativo ao projeto.
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

/// Transforma um texto qualquer em um identificador válido para nome de script.
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

/// Cria um script novo (sem sobrescrever) e devolve o caminho relativo.
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
    std::fs::write(&path, NEW_SCRIPT.replace("NOME", &name)).map_err(|e| e.to_string())?;
    Ok(format!("scripts/{name}.{SCRIPT_EXT}"))
}

/// Jogo de exemplo: colete as moedas.
pub fn template_scene() -> Scene {
    let mut s = Scene::empty();

    let cam = s.add_camera();
    {
        let e = s.get_mut(cam).unwrap();
        e.position = Vec3::new(0.0, 6.0, 11.0);
        e.rotation = Vec3::new(-25.0, 0.0, 0.0);
        e.script = Some(ScriptRef { path: "scripts/CameraSegue.cryo".into(), ..Default::default() });
    }

    s.add_light(LightKind::Directional);

    let lamp = s.add_light(LightKind::Point);
    {
        let e = s.get_mut(lamp).unwrap();
        e.position = Vec3::new(-5.0, 2.5, -4.0);
        let l = e.light.as_mut().unwrap();
        l.color = [0.35, 0.65, 1.0];
        l.intensity = 2.5;
        l.range = 9.0;
    }

    let solid = |s: &mut Scene, name: &str, pos: Vec3, scale: Vec3, color: [f32; 3]| {
        let id = s.add_mesh(Shape::Cube);
        let e = s.get_mut(id).unwrap();
        e.name = name.into();
        e.position = pos;
        e.scale = scale;
        e.mesh.as_mut().unwrap().color = color;
        e.physics = Some(Physics::new(BodyKind::Static, ColliderShape::Box));
        id
    };
    let ground = solid(&mut s, "Chão", Vec3::new(0.0, -0.25, 0.0), Vec3::new(24.0, 0.5, 24.0), [0.36, 0.50, 0.42]);
    s.get_mut(ground).unwrap().mesh.as_mut().unwrap().texture = Some("textures/xadrez.png".into());
    let ramp = solid(&mut s, "Rampa", Vec3::new(1.2, 0.0, -2.0), Vec3::new(4.0, 0.4, 3.0), [0.58, 0.48, 0.40]);
    s.get_mut(ramp).unwrap().rotation = Vec3::new(0.0, 0.0, 20.0);
    solid(&mut s, "Plataforma A", Vec3::new(4.0, 0.5, -2.0), Vec3::new(3.0, 1.0, 3.0), [0.55, 0.45, 0.38]);
    solid(&mut s, "Plataforma B", Vec3::new(7.5, 1.25, -2.0), Vec3::new(3.0, 2.5, 3.0), [0.60, 0.50, 0.42]);
    solid(&mut s, "Muro", Vec3::new(-5.0, 1.0, -6.0), Vec3::new(6.0, 2.0, 0.5), [0.50, 0.52, 0.58]);

    let player = s.add_mesh(Shape::Cube);
    {
        let e = s.get_mut(player).unwrap();
        e.name = "Jogador".into();
        e.position = Vec3::new(0.0, 1.0, 3.0);
        let m = e.mesh.as_mut().unwrap();
        m.color = [0.20, 0.55, 0.95];
        m.shininess = 0.6;
        let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Box);
        p.friction = 0.0;
        e.physics = Some(p);
        e.script = Some(ScriptRef { path: "scripts/Jogador.cryo".into(), ..Default::default() });
    }

    let ball = s.add_mesh(Shape::Sphere);
    {
        let e = s.get_mut(ball).unwrap();
        e.name = "Bola".into();
        e.position = Vec3::new(-3.0, 4.0, 1.0);
        let m = e.mesh.as_mut().unwrap();
        m.color = [0.92, 0.30, 0.25];
        m.shininess = 0.8;
        let mut p = Physics::new(BodyKind::Dynamic, ColliderShape::Sphere);
        p.bounciness = 0.75;
        e.physics = Some(p);
    }

    // modelo importado (.obj) com colisor
    let crystal = s.add_mesh(Shape::Cube);
    {
        let e = s.get_mut(crystal).unwrap();
        e.name = "Cristal".into();
        e.position = Vec3::new(-6.5, 1.5, -1.5);
        e.scale = Vec3::splat(1.5);
        let m = e.mesh.as_mut().unwrap();
        m.model = Some("models/cristal.obj".into());
        m.model_bounds = Some(([0.0; 3], [0.5, 1.0, 0.5]));
        m.color = [0.35, 0.95, 1.0];
        m.shininess = 1.0;
        e.physics = Some(Physics::new(BodyKind::Static, ColliderShape::Box));
    }

    let spot = s.add_light(LightKind::Spot);
    {
        let e = s.get_mut(spot).unwrap();
        e.position = Vec3::new(7.5, 7.0, -2.0);
        let l = e.light.as_mut().unwrap();
        l.color = [1.0, 0.85, 0.5];
        l.intensity = 3.0;
        l.angle = 28.0;
    }

    let coins = s.add_empty("Moedas");
    let spots = [
        Vec3::new(-3.0, 0.8, -2.0),
        Vec3::new(0.0, 0.8, -4.0),
        Vec3::new(4.0, 1.8, -2.0),
        Vec3::new(7.5, 3.3, -2.0),
        Vec3::new(-6.0, 0.8, 4.0),
    ];
    for p in spots {
        let id = s.add_mesh(Shape::Cylinder);
        let e = s.get_mut(id).unwrap();
        e.name = "Moeda".into();
        e.parent = Some(coins);
        e.position = p;
        e.rotation = Vec3::new(90.0, 0.0, 0.0);
        e.scale = Vec3::new(0.7, 0.08, 0.7);
        let m = e.mesh.as_mut().unwrap();
        m.color = [1.0, 0.80, 0.15];
        m.shininess = 1.0;
        let mut ph = Physics::new(BodyKind::Static, ColliderShape::Sphere);
        ph.is_trigger = true;
        e.physics = Some(ph);
        e.script = Some(ScriptRef { path: "scripts/Moeda.cryo".into(), ..Default::default() });
    }
    s
}

/// Pasta de destino para cada tipo de arquivo importado.
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

/// Arquivos que um modelo referencia (materiais, buffers, texturas), relativos à pasta dele.
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

/// Copia um arquivo externo (e suas dependências) para dentro do projeto.
/// Devolve o caminho relativo do arquivo importado.
pub fn import_file(project: &Path, src: &Path) -> Result<String, String> {
    let name = src.file_name().and_then(|n| n.to_str()).ok_or("nome de arquivo inválido")?.to_string();
    let ext = crate::assets::ext_of(&name);
    let folder = import_folder(&ext).ok_or_else(|| {
        format!("`{name}`: tipo não suportado (modelos .obj/.gltf/.glb, imagens .png/.jpg, sons .wav/.ogg/.mp3)")
    })?;
    let dest_dir = project.join(folder);
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(&name);
    // já está dentro do projeto? então não há o que copiar
    let same = std::fs::canonicalize(src).ok() == std::fs::canonicalize(&dest).ok() && dest.exists();
    if !same {
        std::fs::copy(src, &dest).map_err(|e| format!("não foi possível copiar `{name}`: {e}"))?;
        let src_dir = src.parent().unwrap_or(Path::new("."));
        for dep in model_dependencies(src) {
            let rel = Path::new(&dep);
            // só dependências dentro da pasta do modelo (nada de `..` ou caminhos absolutos)
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

/// Configuração gravada junto ao jogo exportado.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct GameConfig {
    pub title: String,
    pub scene: String,
}

pub const GAME_CONFIG: &str = "cryo_game.json";

/// Exporta o jogo para `dest`: uma cópia do executável mais a pasta `game/` com o projeto.
/// O executável, ao encontrar essa pasta ao lado, abre direto no jogo (sem o editor).
pub fn export_game(project: &Path, scene: &str, title: &str, dest: &Path) -> Result<PathBuf, String> {
    let io = |e: std::io::Error| format!("falha ao exportar: {e}");
    let exe = std::env::current_exe().map_err(io)?;
    let safe: String =
        title.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let safe = if safe.is_empty() { "Jogo".to_string() } else { safe };
    if dest.starts_with(project) {
        return Err("escolha uma pasta fora do projeto para exportar".into());
    }

    let (root, exe_dir, game_dir) = if cfg!(target_os = "macos") {
        // pacote .app do macOS
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
    let exe_name = if cfg!(windows) { format!("{safe}.exe") } else { safe.clone() };
    std::fs::copy(&exe, exe_dir.join(&exe_name)).map_err(io)?;
    copy_dir(project, &game_dir, 0).map_err(io)?;
    let config = GameConfig { title: title.to_string(), scene: scene.to_string() };
    std::fs::write(game_dir.join(GAME_CONFIG), serde_json::to_string_pretty(&config).unwrap_or_default())
        .map_err(io)?;
    if cfg!(target_os = "macos") {
        let plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>CFBundleName</key><string>{title}</string>\n<key>CFBundleExecutable</key><string>{safe}</string>\n<key>CFBundleIdentifier</key><string>com.cryoengine.{safe}</string>\n<key>CFBundlePackageType</key><string>APPL</string>\n<key>CFBundleVersion</key><string>1.0</string>\n<key>NSHighResolutionCapable</key><true/>\n</dict></plist>\n"
        );
        std::fs::write(root.join("Contents").join("Info.plist"), plist).map_err(io)?;
    }
    Ok(root)
}

/// Se este executável foi exportado como jogo, devolve a pasta `game/` embutida.
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
    fn importa_obj_com_material_e_textura() {
        let base = std::env::temp_dir().join(format!("cryo_import_{}", std::process::id()));
        std::fs::remove_dir_all(&base).ok();
        let ext = base.join("fora/tex");
        std::fs::create_dir_all(&ext).unwrap();
        std::fs::write(base.join("fora/casa.obj"), "mtllib casa.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        std::fs::write(base.join("fora/casa.mtl"), "newmtl a\nmap_Kd tex/parede.png\n").unwrap();
        std::fs::write(ext.join("parede.png"), "x").unwrap();
        let proj = base.join("proj");
        let rel = import_file(&proj, &base.join("fora/casa.obj")).unwrap();
        assert_eq!(rel, "models/casa.obj");
        assert!(proj.join("models/casa.mtl").exists());
        assert!(proj.join("models/tex/parede.png").exists());
        assert!(import_file(&proj, &base.join("fora/casa.mtl")).is_err());
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn exporta_jogo_com_configuracao() {
        let base = std::env::temp_dir().join(format!("cryo_export_{}", std::process::id()));
        std::fs::remove_dir_all(&base).ok();
        let proj = base.join("proj");
        let scene = ensure_project(&proj).unwrap();
        let out = export_game(&proj, &scene, "Meu Jogo", &base.join("saida")).unwrap();
        let game = if cfg!(target_os = "macos") { out.join("Contents/Resources/game") } else { out.join("game") };
        assert_eq!(read_game_config(&game).unwrap().scene, scene);
        assert!(game.join("scripts/Jogador.cryo").exists());
        assert!(game.join("sounds/moeda.wav").exists());
        assert!(export_game(&proj, &scene, "x", &proj.join("dentro")).is_err());
        std::fs::remove_dir_all(base).ok();
    }
}
