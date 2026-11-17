//! Editor state, menu bar, toolbar and panel layout.

use crate::assets::{self, Assets};
use crate::project;
use crate::settings::Settings;
use crate::tutorial::{self, Focus};
use crate::render::Renderer;
use crate::runtime::{log, InputState, LogLevel, LogLine, Runtime};
use crate::scene::{LightKind, Scene, Shape};
use crate::viewport::{EditorCamera, GizmoDrag, Tool};
use cryo_lang::{CryoError, Type, Value};
use eframe::egui::{self, Color32, Key, RichText};
use egui_dock::{DockArea, DockState, NodeIndex, Style};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const ACCENT: Color32 = Color32::from_rgb(38, 132, 255);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tab {
    Scene,
    Game,
    Script,
    Hierarchy,
    Inspector,
    Content,
    Console,
    Timeline,
}

pub struct Play {
    backup: Scene,
    backup_selected: Option<u64>,
    pub rt: Runtime,
    pub paused: bool,
}

#[derive(Default)]
pub struct ScriptEditor {
    pub path: Option<String>,
    pub text: String,
    pub saved: String,
    pub error: Option<CryoError>,
}

impl ScriptEditor {
    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }
}

pub struct PropCache {
    pub at: Instant,
    pub props: Result<Vec<(String, Option<Type>, Value)>, String>,
}

pub struct State {
    pub project_dir: PathBuf,
    pub scene_path: String,
    pub scene: Scene,
    pub selected: Option<u64>,
    pub renderer: Option<Arc<Mutex<Renderer>>>,
    pub render_error: Option<String>,
    pub cam: EditorCamera,
    pub tool: Tool,
    pub drag: Option<GizmoDrag>,
    pub play: Option<Play>,
    pub input: InputState,
    /// Gamepads, the controls of the project (with the player's changes) and how they resolve.
    pads: crate::input::Gamepads,
    pad: crate::input::Pad,
    resolver: crate::input::Resolver,
    pub bindings: Vec<crate::input::Binding>,
    bindings_for: PathBuf,
    /// What was pressed this frame before bindings (for the Controls menu).
    raw_pressed: std::collections::HashSet<String>,
    raw_held: std::collections::HashSet<String>,
    /// Controls menu: open, and which binding waits for a new key (index, gamepad?).
    show_controls: bool,
    rebinding: Option<(usize, bool)>,
    /// The game is holding the mouse pointer (first-person cameras).
    pub mouse_grabbed: bool,
    pub logs: Vec<LogLine>,
    undo: Vec<String>,
    redo: Vec<String>,
    last_json: String,
    saved_json: String,
    pub editor: ScriptEditor,
    pub browser_dir: String,
    pub prop_cache: HashMap<String, PropCache>,
    pub focus_tab: Option<Tab>,
    pub show_about: bool,
    /// Edit Mesh mode, when active.
    pub mesh_edit: Option<crate::meshmode::MeshSession>,
    pub timeline: crate::timeline::TimelineState,
    pub settings: Settings,
    /// Tutorial list window.
    pub show_tutorials: bool,
    /// Running tutorial: (id, step index).
    pub tutorial: Option<(&'static str, usize)>,
    /// The pause menu of an exported game is open.
    pub pause_menu: bool,
    /// Where each panel is on screen, so tutorials can point at it.
    pub tab_rects: HashMap<Tab, egui::Rect>,
    pub toolbar_rect: egui::Rect,
    pub menu_rect: egui::Rect,
    pub show_grid: bool,
    pub assets: Assets,
    /// Exported game: no editor, only the game screen.
    /// The frame of the video cutscene on screen: (file, frame number, picture).
    pub film_tex: Option<(String, usize, egui::TextureHandle)>,
    pub game_only: bool,
    /// The program started without the screen's sync (frame-rate limit "none", or a benchmark).
    pub vsync_off: bool,
}

impl State {
    pub fn info(&mut self, text: impl Into<String>) {
        log(&mut self.logs, LogLevel::Info, text);
    }
    pub fn error(&mut self, text: impl Into<String>) {
        log(&mut self.logs, LogLevel::Error, text);
        self.focus_tab = Some(Tab::Console);
    }

    pub fn playing(&self) -> bool {
        self.play.is_some()
    }

    pub fn scene_dirty(&self) -> bool {
        !self.playing() && self.last_json != self.saved_json
    }

    fn reset_history(&mut self) {
        self.last_json = self.scene.to_json();
        self.saved_json = self.last_json.clone();
        self.undo.clear();
        self.redo.clear();
    }

    pub fn load_scene(&mut self, rel: &str) {
        if self.playing() {
            return;
        }
        let path = self.project_dir.join(rel);
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| Scene::from_json(&s)) {
            Ok(scene) => {
                self.scene = scene;
                self.scene_path = rel.to_string();
                self.selected = None;
                self.reset_history();
                self.info(tr!("Scene opened: {rel}", "Cena aberta: {rel}"));
            }
            Err(e) => self.error(tr!("Could not open {rel}: {e}", "Não foi possível abrir {rel}: {e}")),
        }
    }

    pub fn save_scene(&mut self) {
        if self.playing() {
            self.info(tr!("Stop the game before saving the scene.", "Pare o jogo antes de salvar a cena."));
            return;
        }
        let json = self.scene.to_json();
        let path = self.project_dir.join(&self.scene_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        match std::fs::write(&path, &json) {
            Ok(()) => {
                self.saved_json = json.clone();
                self.last_json = json;
                self.info(tr!("Scene saved: {}", "Cena salva: {}", self.scene_path));
            }
            Err(e) => self.error(tr!("Error saving the scene: {e}", "Erro ao salvar a cena: {e}")),
        }
    }

    pub fn new_scene(&mut self) {
        if self.playing() {
            return;
        }
        let mut i = 1;
        let rel = loop {
            let rel = format!("scenes/Cena{i}.{}", project::SCENE_EXT);
            if !self.project_dir.join(&rel).exists() {
                break rel;
            }
            i += 1;
        };
        self.scene = Scene::default_scene();
        self.scene_path = rel;
        self.selected = None;
        self.reset_history();
        self.saved_json.clear(); // not on disk yet
        self.info(tr!("New scene created (save it with Ctrl+S).", "Nova cena criada (salve com Ctrl+S)."));
    }

    pub fn open_script(&mut self, rel: &str) {
        match std::fs::read_to_string(self.project_dir.join(rel)) {
            Ok(text) => {
                self.editor.error = cryo_lang::parse(&text).err();
                self.editor.saved = text.clone();
                self.editor.text = text;
                self.editor.path = Some(rel.to_string());
                self.focus_tab = Some(Tab::Script);
            }
            Err(e) => self.error(tr!("Could not open {rel}: {e}", "Não foi possível abrir {rel}: {e}")),
        }
    }

    pub fn save_script(&mut self) {
        let Some(rel) = self.editor.path.clone() else { return };
        match std::fs::write(self.project_dir.join(&rel), &self.editor.text) {
            Ok(()) => {
                self.editor.saved = self.editor.text.clone();
                self.prop_cache.remove(&rel);
                match &self.editor.error {
                    None => self.info(tr!("Script saved: {rel}", "Script salvo: {rel}")),
                    Some(e) => {
                        let msg = tr!("{rel} saved with an error, {e}", "{rel} salvo com erro, {e}");
                        log(&mut self.logs, LogLevel::Warn, msg);
                    }
                }
            }
            Err(e) => self.error(tr!("Error saving {rel}: {e}", "Erro ao salvar {rel}: {e}")),
        }
    }

    /// Imports external files into the project (models, textures, sounds).
    /// Converts a normal video into the project's `videos/` folder (needs ffmpeg).
    pub fn import_video(&mut self, src: &std::path::Path) -> Option<String> {
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
        let name: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        let rel = format!("videos/{name}.{}", crate::video::EXT);
        match crate::video::import(src, &self.project_dir.join(&rel)) {
            Ok((frames, seconds)) => {
                self.info(tr!(
                    "Video imported: {rel} ({frames} frames, {seconds:.1} s). Play it with a \"Play video\" event on the Timeline or video.play(\"{rel}\") in a script.",
                    "Vídeo importado: {rel} ({frames} quadros, {seconds:.1} s). Toque com um evento \"Tocar vídeo\" na Timeline ou video.play(\"{rel}\") em um script."
                ));
                self.browser_dir = "videos".into();
                Some(rel)
            }
            Err(e) => {
                self.error(tr!("Video: {e}", "Vídeo: {e}"));
                self.focus_tab = Some(Tab::Console);
                None
            }
        }
    }

    pub fn import_video_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title(tr!("Import a video cutscene", "Importar um vídeo de cutscene"))
            .add_filter(tr!("Videos", "Vídeos"), crate::video::SOURCE_EXTS)
            .pick_files();
        for src in picked.unwrap_or_default() {
            self.import_video(&src);
        }
    }

    /// A new cutscene, ready to edit: a clip that blocks the player, with its own camera
    /// starting at the editor's view, a fade from black and the cut back to the game camera.
    pub fn add_cutscene(&mut self) {
        use crate::anim::{Action, Clip, Ease, Marker, Property};
        let mut n = 1;
        while self.scene.animations.iter().any(|c| c.name == format!("Cutscene {n}")) {
            n += 1;
        }
        let game_cam = self.scene.entities.iter().find(|e| e.camera.is_some()).map(|e| e.id);
        let cam = self.scene.add_camera();
        let (pos, rot) = (self.cam.pos, glam::Vec3::new(self.cam.pitch.to_degrees(), self.cam.yaw.to_degrees(), 0.0));
        let ahead = self.cam.forward() * 4.0;
        if let Some(e) = self.scene.get_mut(cam) {
            e.name = format!("Cutscene {n} Camera");
            e.position = pos;
            e.rotation = rot;
        }
        let mut clip = Clip::new(format!("Cutscene {n}"));
        clip.duration = 6.0;
        clip.cutscene = true;
        let t = clip.track_mut(cam, Property::Position);
        t.set_key(0.0, pos.to_array());
        t.set_key(6.0, (pos + ahead).to_array());
        for k in &mut t.keys {
            k.ease = Ease::Smooth;
        }
        let r = clip.track_mut(cam, Property::Rotation);
        r.set_key(0.0, rot.to_array());
        r.set_key(6.0, rot.to_array());
        clip.markers.push(Marker { t: 0.0, action: Action::Camera { entity: cam } });
        clip.markers.push(Marker { t: 0.0, action: Action::Fade { to: 1.0, seconds: 0.0 } });
        clip.markers.push(Marker { t: 0.05, action: Action::Fade { to: 0.0, seconds: 1.0 } });
        clip.markers.push(Marker { t: 1.5, action: Action::Subtitle { text: tr!("Write the first line here.", "Escreva a primeira fala aqui."), seconds: 3.0 } });
        if let Some(back) = game_cam {
            clip.markers.push(Marker { t: 5.95, action: Action::Camera { entity: back } });
        }
        self.scene.animations.push(clip);
        self.timeline.clip = self.scene.animations.len() - 1;
        self.timeline.time = 0.0;
        self.selected = Some(cam);
        self.focus_tab = Some(Tab::Timeline);
        self.info(tr!(
            "Cutscene {n} created. On the Timeline: move the camera and press the key button to add shots, use + Event for cuts, subtitles, fades, sounds and videos. Start it with anim.play(\"Cutscene {n}\") or tick Autoplay.",
            "Cutscene {n} criada. Na Timeline: mova a câmera e use o botão de chave para criar tomadas, e + Evento para cortes, legendas, fades, sons e vídeos. Inicie com anim.play(\"Cutscene {n}\") ou marque Autoplay."
        ));
    }

    pub fn import_paths(&mut self, paths: &[PathBuf]) {
        for src in paths {
            // videos are converted, not copied
            let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if crate::video::SOURCE_EXTS.contains(&ext.as_str()) {
                self.import_video(src);
                continue;
            }
            match project::import_file(&self.project_dir, src) {
                Ok(rel) => {
                    self.assets.invalidate(&rel);
                    self.browser_dir = rel.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
                    let ext = assets::ext_of(&rel);
                    if assets::MODEL_EXTS.contains(&ext.as_str()) {
                        match self.assets.model(&rel) {
                            Some(m) => {
                                self.info(tr!("Model imported: {rel} ({} triangles)", "Modelo importado: {rel} ({} triângulos)", m.triangles));
                                if !self.playing() {
                                    self.add_model_to_scene(&rel);
                                }
                            }
                            None => self.focus_tab = Some(Tab::Console),
                        }
                    } else {
                        self.info(tr!("File imported: {rel}", "Arquivo importado: {rel}"));
                    }
                    if self.focus_tab.is_none() {
                        self.focus_tab = Some(Tab::Content);
                    }
                }
                Err(e) => self.error(tr!("Import: {e}", "Importação: {e}")),
            }
        }
    }

    pub fn import_dialog(&mut self) {
        let all: Vec<&str> = assets::MODEL_EXTS
            .iter()
            .chain(assets::IMAGE_EXTS)
            .chain(assets::SOUND_EXTS)
            .copied()
            .collect();
        let picked = rfd::FileDialog::new()
            .set_title(tr!("Import into the project", "Importar para o projeto"))
            .add_filter(tr!("Everything Cryo can import", "Tudo que a Cryo importa"), &all)
            .add_filter(tr!("3D models (.obj, .gltf, .glb)", "Modelos 3D (.obj, .gltf, .glb)"), assets::MODEL_EXTS)
            .add_filter(tr!("Images (.png, .jpg)", "Imagens (.png, .jpg)"), assets::IMAGE_EXTS)
            .add_filter(tr!("Sounds (.wav, .ogg, .mp3)", "Sons (.wav, .ogg, .mp3)"), assets::SOUND_EXTS)
            .pick_files();
        if let Some(paths) = picked {
            self.import_paths(&paths);
        }
    }

    /// Creates an entity with the model, at a reasonable size and in front of the camera.
    pub fn add_model_to_scene(&mut self, rel: &str) {
        let Some(model) = self.assets.model(rel) else {
            self.focus_tab = Some(Tab::Console);
            return;
        };
        let name = std::path::Path::new(rel).file_stem().and_then(|s| s.to_str()).unwrap_or("Model").to_string();
        let id = self.scene.add_mesh(Shape::Cube);
        let biggest = (model.half * 2.0).max_element();
        // huge or tiny models (different units) come in at ~2 m
        let scale = if !(0.05..=50.0).contains(&biggest) { 2.0 / biggest } else { 1.0 };
        if let Some(e) = self.scene.get_mut(id) {
            e.name = name;
            e.scale = glam::Vec3::splat(scale);
            let m = e.mesh.as_mut().unwrap();
            m.model = Some(rel.to_string());
            m.color = [1.0; 3];
            m.model_bounds = Some((model.center.to_array(), model.half.to_array()));
        }
        self.place_new(id);
        // rests the base of the model on the ground
        if let Some(e) = self.scene.get_mut(id) {
            e.position.y = (model.half.y - model.center.y) * scale;
        }
        if scale != 1.0 {
            self.info(tr!("The model was {biggest:.2} units across; its scale was set to {scale:.4}.", "O modelo tinha {biggest:.2} unidades; a escala foi ajustada para {scale:.4}."));
        }
    }

    pub fn export_dialog(&mut self) {
        self.export_with(false);
    }

    /// `other_system`: first asks for the engine program of the system to export for.
    pub fn export_with(&mut self, other_system: bool) {
        if self.scene_dirty() {
            self.save_scene();
        }
        let runtime = if other_system {
            let picked = rfd::FileDialog::new()
                .set_title(tr!(
                    "Choose the `cryo` program downloaded for the other system (cryo.exe, or cryo from the Linux or macOS download)",
                    "Escolha o programa `cryo` baixado para o outro sistema (cryo.exe, ou cryo do download de Linux ou macOS)"
                ))
                .pick_file();
            match picked {
                Some(p) => Some(p),
                None => return,
            }
        } else {
            None
        };
        let Some(dest) = rfd::FileDialog::new().set_title(tr!("Choose where to save the exported game", "Escolha onde salvar o jogo exportado")).pick_folder()
        else {
            return;
        };
        let title = self.project_dir.file_name().and_then(|n| n.to_str()).unwrap_or("Game").to_string();
        match project::export_game_with(&self.project_dir, &self.scene_path, &title, &dest, runtime.as_deref()) {
            Ok(out) => {
                self.info(tr!("Game exported: {}", "Jogo exportado: {}", out.display()));
                self.focus_tab = Some(Tab::Console);
            }
            Err(e) => self.error(e),
        }
    }

    /// Keeps the bounding box saved in the scene equal to the model on disk and reports loading errors.
    fn sync_assets(&mut self) {
        let paths: Vec<(u64, String)> = self
            .scene
            .entities
            .iter()
            .filter_map(|e| e.mesh.as_ref().and_then(|m| m.model.clone()).map(|p| (e.id, p)))
            .collect();
        for (id, path) in paths {
            let Some(model) = self.assets.model(&path) else { continue };
            let bounds = Some((model.center.to_array(), model.half.to_array()));
            if let Some(m) = self.scene.get_mut(id).and_then(|e| e.mesh.as_mut()) {
                if m.model_bounds != bounds {
                    m.model_bounds = bounds;
                }
            }
        }
        for e in std::mem::take(&mut self.assets.errors) {
            log(&mut self.logs, LogLevel::Error, e);
        }
    }

    /// Switches the editor to another project folder (an empty folder gets the example game).
    pub fn open_project(&mut self, dir: PathBuf) {
        self.stop_play();
        let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
        match project::ensure_project(&dir) {
            Ok(scene) => {
                self.project_dir = dir.clone();
                self.assets = Assets::new(dir.clone());
                self.editor = ScriptEditor::default();
                self.browser_dir.clear();
                self.prop_cache.clear();
                self.load_scene(&scene);
                self.settings.last_project = Some(dir.to_string_lossy().to_string());
                self.settings.save();
                self.info(tr!("Project opened: {}", "Projeto aberto: {}", dir.display()));
            }
            Err(e) => self.error(e),
        }
    }

    /// Creates the Ashen Keep example game in a folder chosen by the user and opens it.
    /// Creates the Outpost Kestrel example game in a folder chosen by the user and opens it.
    pub fn new_outpost_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title(tr!("Choose an empty folder for the Outpost Kestrel example", "Escolha uma pasta vazia para o exemplo Outpost Kestrel"))
            .pick_folder();
        let Some(dir) = picked else { return };
        match crate::outpost::create(&dir) {
            Ok(_) => self.open_project(dir),
            Err(e) => self.error(e),
        }
    }

    pub fn new_ashen_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title(tr!("Choose an empty folder for the Ashen Keep example", "Escolha uma pasta vazia para o exemplo Ashen Keep"))
            .pick_folder();
        let Some(dir) = picked else { return };
        match crate::ashen::create(&dir) {
            Ok(_) => self.open_project(dir),
            Err(e) => self.error(e),
        }
    }

    pub fn open_project_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title(tr!("Choose a project folder (an empty folder becomes a new project)", "Escolha a pasta do projeto (uma pasta vazia vira um projeto novo)"))
            .pick_folder();
        if let Some(dir) = picked {
            self.open_project(dir);
        }
    }

    pub fn set_language(&mut self, lang: crate::i18n::Lang) {
        crate::i18n::set_lang(lang);
        self.settings.language = lang.code().to_string();
        self.settings.save();
    }

    pub fn start_tutorial(&mut self, id: &'static str) {
        self.tutorial = Some((id, 0));
        self.show_tutorials = false;
    }

    fn end_tutorial(&mut self, finished: bool) {
        if let Some((id, _)) = self.tutorial.take() {
            if finished && !self.settings.tutorials_done.iter().any(|d| d == id) {
                self.settings.tutorials_done.push(id.to_string());
            }
            self.settings.tour_seen = true;
            self.settings.save();
        }
    }

    /// Tutorial list, the running tutorial's card and the highlight around what it talks about.
    /// The pause menu of an exported game: resume, graphics quality, full screen, quit.
    fn pause_ui(&mut self, ctx: &egui::Context) {
        if !self.pause_menu || !self.game_only {
            return;
        }
        let mut resume = false;
        egui::Window::new(tr!("Paused", "Pausado"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(260.0);
                if ui.add_sized([240.0, 30.0], egui::Button::new(tr!("Resume", "Continuar"))).clicked() {
                    resume = true;
                }
                ui.add_space(8.0);
                self.graphics_choice(ui);
                ui.add_space(6.0);
                self.fps_choice(ui);
                ui.add_space(6.0);
                self.volume_choice(ui);
                ui.add_space(6.0);
                if ui.add_sized([240.0, 26.0], egui::Button::new(tr!("Controls…", "Controles…"))).clicked() {
                    self.show_controls = true;
                }
                let mut full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                if ui.checkbox(&mut full, tr!("Full screen", "Tela cheia")).changed() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(full));
                }
                ui.add_space(8.0);
                if ui.add_sized([240.0, 30.0], egui::Button::new(tr!("Quit game", "Sair do jogo"))).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        self.controls_ui(ctx);
        if resume {
            self.pause_menu = false;
            if let Some(p) = &mut self.play {
                p.paused = false;
            }
        }
    }

    /// Volume sliders (all, music, effects), kept in the user's settings.
    pub fn volume_choice(&mut self, ui: &mut egui::Ui) {
        ui.label(tr!("Volume", "Volume"));
        let mut changed = false;
        egui::Grid::new("volumes").num_columns(2).show(ui, |ui| {
            for (name, v) in [
                (tr!("All", "Geral"), &mut self.settings.volume_master),
                (tr!("Music", "Música"), &mut self.settings.volume_music),
                (tr!("Effects", "Efeitos"), &mut self.settings.volume_effects),
            ] {
                ui.label(name);
                let mut pct = (v.unwrap_or(1.0) * 100.0).round();
                if ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix(" %").fixed_decimals(0)).changed() {
                    *v = Some(pct / 100.0);
                    changed = true;
                }
                ui.end_row();
            }
        });
        if changed {
            self.settings.save();
        }
    }

    /// The Controls window: what each control does and what to press for it; click a key to
    /// change it, then press the new key or gamepad button.
    pub fn controls_ui(&mut self, ctx: &egui::Context) {
        if !self.show_controls {
            return;
        }
        // in the editor nothing reads the keys for the game: read them here
        if self.rebinding.is_some() && !self.playing() {
            let mut keys: std::collections::HashSet<String> = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Key { key, pressed: true, repeat: false, .. } => Some(key.name().to_lowercase()),
                        egui::Event::PointerButton { button: egui::PointerButton::Secondary, pressed: true, .. } => Some("mouse_right".into()),
                        _ => None,
                    })
                    .collect()
            });
            keys.extend(self.pad.buttons.iter().map(|b| b.to_string()));
            self.raw_pressed = keys;
        }
        // waiting for a new key: the first one pressed of the right kind takes the place
        if let Some((i, pad)) = self.rebinding {
            let key = self.controls_key();
            let pick = self.raw_pressed.iter().find(|k| k.starts_with("pad_") == pad && k.as_str() != "escape").cloned();
            if self.raw_pressed.contains("escape") {
                self.rebinding = None;
            } else if let (Some(k), Some(b)) = (pick, self.bindings.get_mut(i)) {
                match b.inputs.iter().position(|x| x.starts_with("pad_") == pad) {
                    Some(at) => b.inputs[at] = k,
                    None => b.inputs.push(k),
                }
                let entry = self.settings.bindings.entry(key).or_default();
                entry.insert(b.name.clone(), b.inputs.clone());
                self.settings.save();
                self.rebinding = None;
            }
        }
        let mut open = true;
        let mut reset = false;
        egui::Window::new(tr!("Controls", "Controles"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                match &self.pad.name {
                    Some(n) => ui.label(RichText::new(tr!("Gamepad: {n}", "Controle: {n}")).small()),
                    None => ui.label(RichText::new(tr!("No gamepad connected.", "Nenhum controle conectado.")).small().weak()),
                };
                ui.add_space(4.0);
                egui::Grid::new("controls").num_columns(3).striped(true).show(ui, |ui| {
                    for (i, b) in self.bindings.iter().enumerate() {
                        ui.label(if b.label.is_empty() { b.name.clone() } else { b.label.clone() });
                        for pad in [false, true] {
                            let current = b.inputs.iter().find(|x| x.starts_with("pad_") == pad).map(|x| crate::input::label(x)).unwrap_or_else(|| "—".into());
                            let waiting = self.rebinding == Some((i, pad));
                            let text = if waiting { tr!("press…", "aperte…") } else { current };
                            if ui.add_sized([96.0, 22.0], egui::Button::new(text).selected(waiting)).clicked() {
                                self.rebinding = Some((i, pad));
                            }
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new(tr!("Click a key, then press the new key or button (Esc cancels).", "Clique numa tecla e aperte a nova tecla ou botão (Esc cancela).")).small().weak());
                if ui.button(tr!("Restore defaults", "Restaurar padrão")).clicked() {
                    reset = true;
                }
            });
        if reset {
            let key = self.controls_key();
            self.settings.bindings.remove(&key);
            self.settings.save();
            self.reload_bindings();
            self.rebinding = None;
        }
        if !open {
            self.show_controls = false;
            self.rebinding = None;
        }
    }

    /// Graphics quality, kept in the user's settings.
    pub fn graphics_choice(&mut self, ui: &mut egui::Ui) {
        ui.label(tr!("Graphics", "Gráficos"));
        ui.horizontal(|ui| {
            for (code, name) in [("low", tr!("Low", "Baixo")), ("medium", tr!("Medium", "Médio")), ("high", tr!("High", "Alto"))] {
                let on = self.settings.graphics == code || (code == "high" && self.settings.graphics.is_empty());
                if ui.selectable_label(on, name).clicked() {
                    self.settings.graphics = code.to_string();
                    self.settings.save();
                }
            }
        })
        .response
        .on_hover_text(tr!(
            "Low turns off post-processing and the shadows of lamps; Medium turns off reflections. For slower computers.",
            "Baixo desliga o pós-processamento e as sombras das luzes; Médio desliga os reflexos. Para computadores mais lentos."
        ));
    }

    /// Frame-rate limit of running games: 30, 60, 120 or none.
    pub fn fps_choice(&mut self, ui: &mut egui::Ui) {
        ui.label(tr!("Frame rate limit", "Limite de FPS"));
        ui.horizontal(|ui| {
            for (value, name) in [(30, "30".to_string()), (60, "60".to_string()), (120, "120".to_string()), (0, tr!("No limit", "Sem limite"))] {
                if ui.selectable_label(self.settings.fps() == value, name).clicked() {
                    self.settings.fps_limit = Some(value);
                    self.settings.save();
                }
            }
        })
        .response
        .on_hover_text(tr!(
            "How many frames per second a running game draws at most. A lower limit keeps the computer cooler and quieter.",
            "Quantos quadros por segundo um jogo rodando desenha no máximo. Um limite menor deixa o computador mais frio e silencioso."
        ));
        // the screen's own sync is chosen when the program starts
        let free = self.settings.fps() == 0;
        if free != self.vsync_off {
            ui.label(
                RichText::new(if free {
                    tr!("Restart to remove the screen's own limit too.", "Reinicie para tirar também o limite da própria tela.")
                } else {
                    tr!("Restart to sync with the screen again (no tearing).", "Reinicie para voltar a sincronizar com a tela (sem cortes na imagem).")
                })
                .small()
                .weak(),
            );
        } else if self.settings.fps() == 120 {
            ui.label(RichText::new(tr!("A 60 Hz screen still shows 60.", "Uma tela de 60 Hz continua mostrando 60.")).small().weak());
        }
    }

    fn tutorial_ui(&mut self, ctx: &egui::Context) {
        let tutorials = tutorial::all();

        if self.show_tutorials {
            let mut open = true;
            let mut start: Option<&'static str> = None;
            egui::Window::new(tr!("Tutorials", "Tutoriais"))
                .collapsible(false)
                .resizable(false)
                .default_width(430.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(tr!(
                            "Short guided tours. Each one points at the part of the editor it explains.",
                            "Tours guiados e curtos. Cada um aponta para a parte do editor que explica."
                        ))
                        .weak(),
                    );
                    ui.add_space(6.0);
                    for t in &tutorials {
                        let done = self.settings.tutorials_done.iter().any(|d| d == t.id);
                        ui.horizontal(|ui| {
                            let mark = if done { "✔" } else { "○" };
                            ui.label(RichText::new(mark).color(if done { Color32::from_rgb(110, 210, 130) } else { Color32::GRAY }));
                            ui.vertical(|ui| {
                                if ui.link(RichText::new(&t.title).strong()).clicked() {
                                    start = Some(t.id);
                                }
                                ui.label(
                                    RichText::new(format!("{} · {} {}", t.summary, t.steps.len(), tr!("steps", "passos")))
                                        .small()
                                        .weak(),
                                );
                            });
                        });
                        ui.add_space(4.0);
                    }
                });
            self.show_tutorials = open;
            if let Some(id) = start {
                self.start_tutorial(id);
            }
        }

        let Some((id, index)) = self.tutorial else { return };
        let Some(t) = tutorials.iter().find(|t| t.id == id) else {
            self.tutorial = None;
            return;
        };
        let index = index.min(t.steps.len() - 1);
        let step = &t.steps[index];

        // highlight the panel the step refers to
        let target = match step.focus {
            Focus::None => None,
            Focus::Tab(tab) => self.tab_rects.get(&tab).copied(),
            Focus::Toolbar => Some(self.toolbar_rect),
            Focus::MenuBar => Some(self.menu_rect),
        }
        .filter(|r| r.is_positive());
        if let Some(rect) = target {
            let pulse = (ctx.input(|i| i.time) * 3.0).sin() as f32 * 0.5 + 0.5;
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("tutorial_focus")));
            painter.rect_stroke(
                rect.shrink(2.0),
                4.0,
                egui::Stroke::new(2.0 + pulse * 2.0, ACCENT.gamma_multiply(0.6 + 0.4 * pulse)),
                egui::StrokeKind::Inside,
            );
            ctx.request_repaint();
        }

        let mut go: i32 = 0;
        let mut close = false;
        egui::Window::new(format!("🎓 {}", t.title))
            .id(egui::Id::new("tutorial_card"))
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .anchor(egui::Align2::RIGHT_BOTTOM, [-18.0, -40.0])
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(
                    RichText::new(tr!("Step {} of {}", "Passo {} de {}", index + 1, t.steps.len())).small().weak(),
                );
                ui.label(RichText::new(&step.title).heading().color(ACCENT));
                ui.add_space(4.0);
                ui.label(&step.body);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(index > 0, egui::Button::new(tr!("◀ Back", "◀ Voltar"))).clicked() {
                        go = -1;
                    }
                    let last = index + 1 == t.steps.len();
                    let next = if last { tr!("Finish ✔", "Concluir ✔") } else { tr!("Next ▶", "Próximo ▶") };
                    if ui.button(RichText::new(next).strong()).clicked() {
                        go = 1;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(tr!("Close", "Fechar")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        if close {
            self.end_tutorial(false);
        } else if go != 0 {
            let next = index as i32 + go;
            if next >= t.steps.len() as i32 {
                self.end_tutorial(true);
            } else {
                let next = next.max(0) as usize;
                self.tutorial = Some((id, next));
                // bring the panel the next step talks about to the front
                if let Focus::Tab(tab) = t.steps[next].focus {
                    self.focus_tab = Some(tab);
                }
            }
        }
    }

    /// Gives every Mesh collider the triangles of its model.
    fn fill_mesh_colliders(&mut self) {
        for e in &mut self.scene.entities {
            let (Some(p), Some(m)) = (&mut e.physics, &e.mesh) else { continue };
            p.mesh = None;
            if p.collider != crate::scene::ColliderShape::Mesh {
                continue;
            }
            p.terrain = None;
            let Some(model) = m.model.as_ref().and_then(|path| self.assets.model(path)) else { continue };
            if let Some(field) = &model.terrain {
                p.terrain = Some(field.clone());
                continue;
            }
            let mut tris = vec![];
            for s in &model.subs {
                let v = |i: u32| {
                    let o = i as usize * 8;
                    glam::Vec3::new(s.vertices[o], s.vertices[o + 1], s.vertices[o + 2])
                };
                for t in s.indices.chunks_exact(3) {
                    tris.push([v(t[0]), v(t[1]), v(t[2])]);
                }
            }
            if tris.len() > 60_000 {
                log(
                    &mut self.logs,
                    LogLevel::Warn,
                    tr!(
                        "{}: the model has {} triangles, too many for a Mesh collider (60000); using a box.",
                        "{}: o modelo tem {} triângulos, demais para um colisor de Malha (60000); usando caixa.",
                        e.name,
                        tris.len()
                    ),
                );
                continue;
            }
            p.mesh = Some(std::sync::Arc::new(tris));
        }
    }

    pub fn start_play(&mut self) {
        if self.playing() {
            return;
        }
        self.finish_mesh_edit(true);
        if self.editor.dirty() {
            self.save_script();
        }
        let backup = self.scene.clone();
        self.fill_mesh_colliders();
        self.info(tr!("▶ Game started", "▶ Jogo iniciado"));
        let rt = Runtime::start(&mut self.scene, &self.project_dir, &mut self.logs);
        if self.logs.iter().rev().take(20).any(|l| l.level == LogLevel::Error) {
            self.focus_tab = Some(Tab::Console);
        } else {
            self.focus_tab = Some(Tab::Game);
        }
        self.play = Some(Play { backup, backup_selected: self.selected, rt, paused: false });
        self.drag = None;
    }

    /// While playing: replaces the scene with another one of the project and starts it.
    /// Stop still brings back the scene the game started from.
    pub fn switch_scene(&mut self, rel: &str) {
        let scene = match std::fs::read_to_string(self.project_dir.join(rel)).map_err(|e| e.to_string()).and_then(|s| Scene::from_json(&s)) {
            Ok(s) => s,
            Err(e) => {
                self.error(tr!("Could not load {rel}: {e}", "Não foi possível carregar {rel}: {e}"));
                return;
            }
        };
        self.scene = scene;
        self.selected = None;
        if self.game_only {
            self.scene_path = rel.to_string();
        }
        self.fill_mesh_colliders();
        let rt = Runtime::start(&mut self.scene, &self.project_dir, &mut self.logs);
        if let Some(p) = &mut self.play {
            p.rt = rt;
        }
        self.info(tr!("Scene loaded: {rel}", "Cena carregada: {rel}"));
    }

    /// Saves the selected entity (and what is under it) as a prefab in `prefabs/`.
    pub fn save_prefab(&mut self) {
        let Some(id) = self.selected else { return };
        let Some(json) = self.scene.prefab_json(id) else { return };
        let name: String = self.scene.get(id).map(|e| e.name.clone()).unwrap_or_default().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        let rel = format!("prefabs/{}.{}", if name.is_empty() { "Prefab" } else { &name }, crate::scene::PREFAB_EXT);
        let path = self.project_dir.join(&rel);
        let result = path.parent().map(std::fs::create_dir_all).unwrap_or(Ok(())).and_then(|_| std::fs::write(&path, json));
        match result {
            Ok(()) => {
                self.browser_dir = "prefabs".into();
                self.info(tr!(
                    "Prefab saved: {rel}. Double-click it in Content to place a copy, or use scene.spawn_prefab(\"{rel}\", position) in a script.",
                    "Prefab salvo: {rel}. Dê dois cliques nele em Conteúdo para colocar uma cópia, ou use scene.spawn_prefab(\"{rel}\", posição) num script."
                ));
            }
            Err(e) => self.error(tr!("Could not save the prefab: {e}", "Não foi possível salvar o prefab: {e}")),
        }
    }

    /// Places a copy of a prefab where new things go.
    pub fn add_prefab_to_scene(&mut self, rel: &str) {
        let json = match std::fs::read_to_string(self.project_dir.join(rel)) {
            Ok(j) => j,
            Err(e) => {
                self.error(format!("{rel}: {e}"));
                return;
            }
        };
        match self.scene.instantiate(&json, None, None) {
            Ok(ids) => {
                let keep = self.scene.get(ids[0]).map(|e| e.position.y).unwrap_or(0.0);
                self.place_new(ids[0]);
                if let Some(e) = self.scene.get_mut(ids[0]) {
                    e.position.y = keep;
                }
            }
            Err(e) => self.error(format!("{rel}: {e}")),
        }
    }

    pub fn stop_play(&mut self) {
        if let Some(p) = self.play.take() {
            self.scene = p.backup;
            self.selected = p.backup_selected;
            self.info(tr!("■ Game stopped", "■ Jogo parado"));
            self.focus_tab = Some(Tab::Scene);
        }
    }

    pub fn undo(&mut self) {
        if self.playing() {
            return;
        }
        if let Some(prev) = self.undo.pop() {
            if let Ok(scene) = Scene::from_json(&prev) {
                self.redo.push(std::mem::replace(&mut self.last_json, prev));
                self.scene = scene;
                self.fix_selection();
            }
        }
    }

    pub fn redo(&mut self) {
        if self.playing() {
            return;
        }
        if let Some(next) = self.redo.pop() {
            if let Ok(scene) = Scene::from_json(&next) {
                self.undo.push(std::mem::replace(&mut self.last_json, next));
                self.scene = scene;
                self.fix_selection();
            }
        }
    }

    fn fix_selection(&mut self) {
        if let Some(id) = self.selected {
            if self.scene.get(id).is_none() {
                self.selected = None;
            }
        }
    }

    /// Records an undo step when the scene changes and no edit is in progress.
    fn track_history(&mut self, ctx: &egui::Context) {
        if self.playing() {
            return;
        }
        let busy = ctx.input(|i| i.pointer.any_down()) || ctx.egui_wants_keyboard_input();
        if busy {
            return;
        }
        let json = self.scene.to_json();
        if json != self.last_json {
            self.undo.push(std::mem::replace(&mut self.last_json, json));
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    pub fn delete_selected(&mut self) {
        if let Some(id) = self.selected.take() {
            self.scene.remove(id);
        }
    }

    pub fn duplicate_selected(&mut self) {
        if let Some(id) = self.selected {
            self.selected = self.scene.duplicate(id);
        }
    }

    /// Places a newly created object in front of the editor camera.
    fn place_new(&mut self, id: u64) {
        let mut p = self.cam.pos + self.cam.forward() * 8.0;
        p.y = p.y.max(0.5);
        let p = (p * 2.0).round() / 2.0;
        if let Some(e) = self.scene.get_mut(id) {
            if e.light.is_none() && e.camera.is_none() {
                e.position = p;
            }
        }
        self.selected = Some(id);
    }

    /// Writes the example rigged model into the project and places it in the scene.
    pub fn add_rig_example(&mut self) {
        let rel = "models/Stalk.gltf";
        let path = self.project_dir.join(rel);
        if !path.exists() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(&path, crate::skel::demo_rig_gltf()) {
                log(&mut self.logs, LogLevel::Error, format!("{rel}: {e}"));
                return;
            }
        }
        let id = self.scene.add_mesh(Shape::Cube);
        self.place_new(id);
        let bounds = self.assets.model(rel).map(|m| (m.center.to_array(), m.half.to_array()));
        if let Some(e) = self.scene.get_mut(id) {
            e.name = "Stalk".into();
            e.position.y = 0.0;
            if let Some(m) = &mut e.mesh {
                m.model = Some(rel.into());
                m.model_bounds = bounds;
                m.color = [1.0; 3];
                m.animation = "Wave".into();
            }
        }
    }

    /// Creates a ready-to-play first-person character at the editor camera's target.
    pub fn add_first_person(&mut self) {
        let script = "scripts/FirstPerson.cryo";
        let path = self.project_dir.join(script);
        if !path.exists() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(&path, crate::project::FIRST_PERSON) {
                log(&mut self.logs, LogLevel::Error, format!("{script}: {e}"));
            }
        }
        let body = self.scene.add_empty("First Person Player");
        self.place_new(body);
        let head = self.scene.add_camera();
        if let Some(e) = self.scene.get_mut(body) {
            e.scale = glam::Vec3::new(0.6, 1.8, 0.6);
            e.position.y += 0.9;
            e.tag = "player".into();
            let mut p = crate::scene::Physics::new(crate::scene::BodyKind::Dynamic, crate::scene::ColliderShape::Box);
            p.friction = 0.0;
            e.physics = Some(p);
            e.script = Some(crate::scene::ScriptRef { path: script.into(), props: Default::default() });
        }
        if let Some(e) = self.scene.get_mut(head) {
            e.name = "Head".into();
            e.parent = Some(body);
            // local space of the body: 0.4 x 1.8 m above its centre is eye height (1.62 m)
            e.position = glam::Vec3::new(0.0, 0.4, 0.0);
            e.rotation = glam::Vec3::ZERO;
            if let Some(c) = &mut e.camera {
                c.fov = 75.0;
                c.near = 0.05;
            }
        }
        self.selected = Some(body);
        log(
            &mut self.logs,
            LogLevel::Info,
            tr!(
                "First-person player created. Press Play: the mouse looks, WASD walks, Space jumps, Tab frees the mouse.",
                "Jogador em primeira pessoa criado. Aperte Play: o mouse olha, WASD anda, Espaço pula, Tab solta o mouse."
            ),
        );
    }

    /// A terrain with hills around a flat middle, ready to sculpt: its heightmap and settings go
    /// in `models/`, and it collides as a height field.
    pub fn add_terrain(&mut self) {
        let dir = self.project_dir.join("models");
        let mut name = "terrain".to_string();
        let mut n = 1;
        while dir.join(format!("{name}.terrain")).exists() {
            n += 1;
            name = format!("terrain{n}");
        }
        let file = match crate::terrain::write_default(&dir, &name) {
            Ok(f) => f,
            Err(e) => {
                log(&mut self.logs, LogLevel::Error, format!("terrain: {e}"));
                return;
            }
        };
        let rel = format!("models/{file}");
        let Some(model) = self.assets.model(&rel) else { return };
        let id = self.scene.add_mesh(Shape::Cube);
        if let Some(e) = self.scene.get_mut(id) {
            e.name = "Terrain".into();
            let m = e.mesh.as_mut().unwrap();
            m.model = Some(rel.clone());
            m.color = [1.0; 3];
            m.model_bounds = Some((model.center.to_array(), model.half.to_array()));
            e.physics = Some(crate::scene::Physics::new(crate::scene::BodyKind::Static, crate::scene::ColliderShape::Mesh));
        }
        self.selected = Some(id);
        log(
            &mut self.logs,
            LogLevel::Info,
            tr!(
                "Terrain created ({rel}). Paint its heightmap in any image editor (white is high) and change size, height and textures in the .terrain file.",
                "Terreno criado ({rel}). Pinte o mapa de altura em qualquer editor de imagem (branco é alto) e mude tamanho, altura e texturas no arquivo .terrain."
            ),
        );
    }

    pub fn create_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button(tr!("○  Empty", "○  Vazio")).clicked() {
            let id = self.scene.add_empty("Empty");
            self.place_new(id);
            ui.close();
        }
        ui.separator();
        for shape in Shape::ALL {
            if ui.button(format!("■  {}", shape_name(shape))).clicked() {
                let id = self.scene.add_mesh(shape);
                self.place_new(id);
                ui.close();
            }
        }
        if ui
            .button(tr!("✏  Editable mesh", "✏  Malha editável"))
            .on_hover_text(tr!("A cube you can model: extrude, inset, subdivide...", "Um cubo que você pode modelar: extrudar, inserir, subdividir..."))
            .clicked()
        {
            let id = self.scene.add_mesh(Shape::Cube);
            if let Some(e) = self.scene.get_mut(id) {
                e.name = "Mesh".into();
            }
            self.place_new(id);
            self.start_mesh_edit(id);
            ui.close();
        }
        ui.separator();
        if ui.button(tr!("☀  Directional light", "☀  Luz direcional")).clicked() {
            let id = self.scene.add_light(LightKind::Directional);
            self.place_new(id);
            ui.close();
        }
        if ui.button(tr!("💡  Point light", "💡  Luz pontual")).clicked() {
            let id = self.scene.add_light(LightKind::Point);
            self.place_new(id);
            ui.close();
        }
        if ui.button(tr!("🔦  Spotlight", "🔦  Holofote")).clicked() {
            let id = self.scene.add_light(LightKind::Spot);
            self.place_new(id);
            ui.close();
        }
        if ui.button(tr!("🎥  Camera", "🎥  Câmera")).clicked() {
            let id = self.scene.add_camera();
            self.place_new(id);
            ui.close();
        }
        if ui
            .button(tr!("■  Animated model (example)", "■  Modelo animado (exemplo)"))
            .on_hover_text(tr!(
                "A small rigged model with three clips (Wave, Curl, Nod), to try skeletal animation.",
                "Um pequeno modelo com esqueleto e três clipes (Wave, Curl, Nod), para testar animação de esqueleto."
            ))
            .clicked()
        {
            self.add_rig_example();
            ui.close();
        }
        if ui
            .button(tr!("🎥  First-person player", "🎥  Jogador em primeira pessoa"))
            .on_hover_text(tr!(
                "A body with physics, a camera at eye height and the FirstPerson script: mouse looks, WASD walks, Space jumps.",
                "Um corpo com física, uma câmera na altura dos olhos e o script FirstPerson: o mouse olha, WASD anda, Espaço pula."
            ))
            .clicked()
        {
            self.add_first_person();
            ui.close();
        }
        if ui
            .button(tr!("⛰  Terrain", "⛰  Terreno"))
            .on_hover_text(tr!(
                "Ground with hills from a heightmap image, with a collider that follows the slopes.",
                "Chão com morros a partir de uma imagem de altura, com colisor que segue as encostas."
            ))
            .clicked()
        {
            self.add_terrain();
            ui.close();
        }
        if ui
            .button(tr!("🎬  Cutscene", "🎬  Cutscene"))
            .on_hover_text(tr!(
                "A new cutscene on the Timeline: its own camera starting at this view, a fade in, a subtitle and the cut back to the game.",
                "Uma cutscene nova na Timeline: câmera própria começando nesta vista, fade de entrada, legenda e o corte de volta para o jogo."
            ))
            .clicked()
        {
            self.add_cutscene();
            ui.close();
        }
    }

    /// Holds or frees the mouse pointer, as the running game asked (`input.lock_mouse`).
    fn apply_mouse_lock(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.focused);
        let want = focused && self.play.as_ref().map(|p| !p.paused && p.rt.mouse_locked()).unwrap_or(false);
        if want == self.mouse_grabbed {
            return;
        }
        self.mouse_grabbed = want;
        // macOS can pin the pointer; elsewhere it is kept inside the window
        let grab = if !want {
            egui::CursorGrab::None
        } else if cfg!(target_os = "macos") {
            egui::CursorGrab::Locked
        } else {
            egui::CursorGrab::Confined
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::CursorGrab(grab));
        ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(!want));
    }

    /// Key under which the player's control changes for this project are kept.
    fn controls_key(&self) -> String {
        self.project_dir.file_name().and_then(|n| n.to_str()).unwrap_or("game").to_string()
    }

    fn reload_bindings(&mut self) {
        let key = self.controls_key();
        self.bindings = crate::input::load(&self.project_dir, self.settings.bindings.get(&key));
        self.bindings_for = self.project_dir.clone();
    }

    fn collect_input(&mut self, ctx: &egui::Context) {
        if self.bindings_for != self.project_dir {
            self.reload_bindings();
        }
        let prev_pad = std::mem::take(&mut self.pad);
        self.pad = self.pads.poll();
        let mut raw_pressed = std::collections::HashSet::new();
        let mut raw_released = std::collections::HashSet::new();
        self.input.mouse_dx = 0.0;
        self.input.mouse_dy = 0.0;
        self.input.scroll = 0.0;
        self.input.pad = [0.0; 6];
        if !self.playing() || ctx.egui_wants_keyboard_input() {
            self.input.down.clear();
            self.input.pressed.clear();
            self.input.released.clear();
            self.raw_held.clear();
            self.raw_pressed.clear();
            return;
        }
        let mut held = std::mem::take(&mut self.raw_held);
        ctx.input(|i| {
            for ev in &i.events {
                if let egui::Event::Key { key, pressed, repeat, .. } = ev {
                    let name = key.name().to_lowercase();
                    if *pressed && !*repeat {
                        raw_pressed.insert(name.clone());
                        held.insert(name);
                    } else if !*pressed {
                        raw_released.insert(name.clone());
                        held.remove(&name);
                    }
                }
            }
            for (name, button) in [
                ("mouse_left", egui::PointerButton::Primary),
                ("mouse_right", egui::PointerButton::Secondary),
                ("mouse_middle", egui::PointerButton::Middle),
            ] {
                let was = held.contains(name);
                let now = i.pointer.button_down(button);
                if now && !was {
                    raw_pressed.insert(name.into());
                    held.insert(name.into());
                } else if !now && was {
                    raw_released.insert(name.into());
                    held.remove(name);
                }
            }
            let mut d = i.pointer.delta();
            if self.mouse_grabbed {
                // the pointer is held in place: read the mouse itself
                let raw = i.events.iter().fold(egui::Vec2::ZERO, |acc, ev| match ev {
                    egui::Event::MouseMoved(m) => acc + *m,
                    _ => acc,
                });
                if raw != egui::Vec2::ZERO {
                    d = raw;
                }
            }
            // the right stick looks around like the mouse
            let dt = i.stable_dt.min(0.1);
            self.input.mouse_dx = d.x + self.pad.rx * 900.0 * dt;
            self.input.mouse_dy = d.y - self.pad.ry * 900.0 * dt;
            self.input.scroll = i.smooth_scroll_delta.y;
            if i.modifiers.shift {
                held.insert("shift".into());
            } else {
                held.remove("shift");
            }
            if !i.focused {
                held.clear();
            }
        });
        // gamepad buttons
        for b in &self.pad.buttons {
            held.insert(b.to_string());
            if !prev_pad.buttons.contains(b) {
                raw_pressed.insert(b.to_string());
            }
        }
        for b in &prev_pad.buttons {
            if !self.pad.buttons.contains(b) {
                raw_released.insert(b.to_string());
            }
        }
        let p = &self.pad;
        self.input.pad = [p.lx, p.ly, p.rx, p.ry, p.lt, p.rt];
        let (down, pressed, released) = self.resolver.resolve(&held, &raw_pressed, &raw_released, &self.bindings);
        self.input.down = down;
        self.input.pressed = pressed;
        self.input.released = released;
        self.raw_held = held;
        self.raw_pressed = raw_pressed;
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let (cmd, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));

        if cmd && pressed(Key::S) {
            if self.editor.dirty() {
                self.save_script();
            }
            if !self.playing() {
                self.save_scene();
            }
        }
        if cmd && pressed(Key::P) {
            if self.playing() {
                self.stop_play();
            } else {
                self.start_play();
            }
        }
        if self.playing() {
            if pressed(Key::Escape) || (self.game_only && self.input.pressed.contains("pad_start")) {
                if self.game_only {
                    // an exported game pauses and shows its menu instead of closing at once
                    self.pause_menu = !self.pause_menu;
                    if let Some(p) = &mut self.play {
                        p.paused = self.pause_menu;
                    }
                } else {
                    self.stop_play();
                }
            }
            return;
        }
        if typing {
            return;
        }
        // while editing a mesh, Undo and Delete belong to the mesh tools (see meshmode.rs)
        if self.mesh_edit.is_none() {
            if cmd && pressed(Key::Z) {
                if shift {
                    self.redo();
                } else {
                    self.undo();
                }
            }
            if cmd && pressed(Key::Y) {
                self.redo();
            }
            if cmd && pressed(Key::D) {
                self.duplicate_selected();
            }
            if pressed(Key::Delete) || pressed(Key::Backspace) {
                self.delete_selected();
            }
        }
        let flying = ctx.input(|i| i.pointer.secondary_down());
        if !cmd && !flying {
            if pressed(Key::Q) || pressed(Key::W) {
                self.tool = Tool::Move;
            }
            if pressed(Key::E) {
                self.tool = Tool::Rotate;
            }
            if pressed(Key::R) {
                self.tool = Tool::Scale;
            }
        }
    }
}

/// Shape name in the interface language (entity names stay in English).
pub fn shape_name(shape: Shape) -> String {
    match shape {
        Shape::Cube => tr!("Cube", "Cubo"),
        Shape::Sphere => tr!("Sphere", "Esfera"),
        Shape::Plane => tr!("Plane", "Plano"),
        Shape::Cylinder => tr!("Cylinder", "Cilindro"),
        Shape::Cone => tr!("Cone", "Cone"),
        Shape::Capsule => tr!("Capsule", "Cápsula"),
    }
}

pub struct App {
    dock: DockState<Tab>,
    st: State,
    frames: u32,
    fps_at: Instant,
    fps: f32,
    /// Graphics card and driver, as OpenGL reports them (for the benchmark report).
    gpu: String,
    bench: Option<Bench>,
    /// F3 in a game: frame rate and frame time in the corner.
    show_fps: bool,
    /// When the last frame of a running game started (for the frame-rate limit).
    last_frame: Instant,
}

/// Seconds the benchmark measures (`cryo <project> --bench[=seconds]`), after 3 s of warm-up.
pub static BENCH_SECONDS: std::sync::OnceLock<f32> = std::sync::OnceLock::new();

/// Measures how long frames take while the game runs, then writes a report and quits.
struct Bench {
    started: Instant,
    last: Option<Instant>,
    seconds: f32,
    /// Time between frames (everything: game, drawing, the graphics card), in ms.
    frames: Vec<f32>,
    /// Time the main thread spent on the game and on preparing the frame, in ms.
    cpu: Vec<f32>,
    pixels: [f32; 2],
}

impl Bench {
    const WARMUP: f32 = 3.0;

    fn report(&self, gpu: &str, st: &State) -> String {
        let mut f = self.frames.clone();
        f.sort_by(|a, b| a.total_cmp(b));
        let pct = |p: f32| if f.is_empty() { 0.0 } else { f[((f.len() - 1) as f32 * p).round() as usize] };
        let mean = f.iter().sum::<f32>() / f.len().max(1) as f32;
        let cpu = self.cpu.iter().sum::<f32>() / self.cpu.len().max(1) as f32;
        let tris: usize = st
            .scene
            .entities
            .iter()
            .filter_map(|e| e.mesh.as_ref().and_then(|m| m.model.as_ref()))
            .filter_map(|p| st.assets.cached_model(p))
            .map(|m| m.triangles)
            .sum();
        format!(
            "Cryo Engine {} benchmark\n\
             project: {}\n\
             system: {} {}\n\
             graphics card: {gpu}\n\
             resolution: {:.0} x {:.0}\n\
             graphics quality: {}\n\
             entities: {}, model triangles in the scene: {tris}\n\
             frames measured: {} over {:.1} s (after {:.0} s warm-up), vsync off\n\
             average: {:.1} fps ({mean:.2} ms per frame)\n\
             frame time: median {:.2} ms, 95% {:.2} ms, 99% {:.2} ms, worst {:.2} ms\n\
             1% low: {:.1} fps\n\
             main thread (game + preparing the frame): {cpu:.2} ms per frame\n",
            env!("CARGO_PKG_VERSION"),
            st.project_dir.display(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            self.pixels[0],
            self.pixels[1],
            if st.settings.graphics.is_empty() { "high" } else { &st.settings.graphics },
            st.scene.entities.len(),
            f.len(),
            f.iter().sum::<f32>() / 1000.0,
            Self::WARMUP,
            1000.0 / mean.max(1e-3),
            pct(0.5),
            pct(0.95),
            pct(0.99),
            f.last().copied().unwrap_or(0.0),
            1000.0 / pct(0.99).max(1e-3),
        )
    }
}

fn default_layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Scene, Tab::Game, Tab::Script]);
    let tree = dock.main_surface_mut();
    let [center, _right] = tree.split_right(NodeIndex::root(), 0.78, vec![Tab::Inspector]);
    let [center, _left] = tree.split_left(center, 0.19, vec![Tab::Hierarchy]);
    let [_center, _bottom] = tree.split_below(center, 0.70, vec![Tab::Content, Tab::Console, Tab::Timeline]);
    dock
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        project_dir: PathBuf,
        autoplay: bool,
        game: Option<project::GameConfig>,
        settings: Settings,
    ) -> Self {
        apply_theme(&cc.egui_ctx);
        let (renderer, render_error) = match cc.gl.as_ref() {
            Some(gl) => match Renderer::new(gl) {
                Ok(r) => (Some(Arc::new(Mutex::new(r))), None),
                Err(e) => (None, Some(e)),
            },
            None => (None, Some(tr!("OpenGL unavailable", "OpenGL indisponível").to_string())),
        };
        let project_dir = std::fs::canonicalize(&project_dir).unwrap_or_else(|_| {
            std::env::current_dir().map(|d| d.join(&project_dir)).unwrap_or(project_dir.clone())
        });

        let game_only = game.is_some();
        let mut st = State {
            assets: Assets::new(project_dir.clone()),
            film_tex: None,
            game_only,
            vsync_off: BENCH_SECONDS.get().is_some() || settings.fps() == 0,
            project_dir,
            scene_path: format!("scenes/Principal.{}", project::SCENE_EXT),
            scene: Scene::default_scene(),
            selected: None,
            renderer,
            render_error,
            cam: EditorCamera::default(),
            tool: Tool::Move,
            drag: None,
            play: None,
            input: InputState::default(),
            pads: crate::input::Gamepads::new(),
            pad: crate::input::Pad::default(),
            resolver: crate::input::Resolver::default(),
            bindings: vec![],
            bindings_for: PathBuf::new(),
            raw_pressed: Default::default(),
            raw_held: Default::default(),
            show_controls: false,
            rebinding: None,
            mouse_grabbed: false,
            pause_menu: false,
            logs: vec![],
            undo: vec![],
            redo: vec![],
            last_json: String::new(),
            saved_json: String::new(),
            editor: ScriptEditor::default(),
            browser_dir: String::new(),
            prop_cache: HashMap::new(),
            focus_tab: None,
            show_about: false,
            mesh_edit: None,
            timeline: Default::default(),
            settings,
            show_tutorials: false,
            tutorial: None,
            tab_rects: HashMap::new(),
            toolbar_rect: egui::Rect::NOTHING,
            menu_rect: egui::Rect::NOTHING,
            show_grid: true,
        };
        st.info(tr!("Cryo Engine {} — project: {}", "Cryo Engine {} — projeto: {}", env!("CARGO_PKG_VERSION"), st.project_dir.display()));
        let opened = match &game {
            Some(g) => Ok(g.scene.clone()),
            None => project::ensure_project(&st.project_dir),
        };
        match opened {
            Ok(rel) => st.load_scene(&rel),
            Err(e) => {
                st.error(e);
                st.reset_history();
            }
        }
        if let Some(e) = st.render_error.clone() {
            st.error(tr!("Renderer: {e}", "Renderizador: {e}"));
        }
        let scripts = project::list_files(&st.project_dir, project::SCRIPT_EXT);
        let preferred = scripts.iter().find(|s| s.ends_with("/Player.cryo")).or(scripts.first()).cloned();
        if let Some(first) = preferred {
            st.open_script(&first);
        }
        st.focus_tab = Some(Tab::Scene);
        // Useful for automated tests: CRYO_SELECT="Entity name"
        if let Ok(name) = std::env::var("CRYO_SELECT") {
            st.selected = st.scene.find_by_name(&name);
        }
        if std::env::var("CRYO_VIEW").as_deref() == Ok("top") {
            st.cam.view_from(glam::Vec3::Y, tr!("Top", "Topo"));
        }
        match std::env::var("CRYO_TAB").as_deref() {
            Ok("script") => st.focus_tab = Some(Tab::Script),
            Ok("timeline") => {
                st.focus_tab = Some(Tab::Timeline);
                st.timeline.time = 2.6;
            }
            _ => {}
        }
        // Also for automated tests: CRYO_EDIT_MESH=1 opens Edit Mesh on the selection and models a little.
        if std::env::var("CRYO_EDIT_MESH").is_ok() {
            if let Some(id) = st.selected {
                st.start_mesh_edit(id);
                st.mesh_demo();
            }
        }
        // CRYO_DEMO=rig adds the example rigged model, mid-animation (for screenshots)
        if std::env::var("CRYO_DEMO").as_deref() == Ok("rig") {
            st.add_rig_example();
            if let Some(e) = st.selected.and_then(|id| st.scene.get_mut(id)) {
                let centre = e.position + glam::Vec3::new(0.0, 1.5, 0.0);
                if let Some(m) = &mut e.mesh {
                    m.animation = "Curl".into();
                    m.anim.time = 0.8;
                }
                st.cam.frame(centre, 2.6);
            }
        }
        // first launch: start the welcome tour
        if !st.settings.tour_seen && !game_only && !autoplay && std::env::var("CRYO_NO_TOUR").is_err() {
            st.tutorial = Some(("welcome", 0));
        }
        if !game_only {
            st.settings.last_project = Some(st.project_dir.to_string_lossy().to_string());
            st.settings.save();
        }
        if autoplay || game_only {
            st.start_play();
        }
        let gpu = cc
            .gl
            .as_ref()
            .map(|gl| unsafe {
                use eframe::glow::HasContext;
                format!("{} ({})", gl.get_parameter_string(eframe::glow::RENDERER), gl.get_parameter_string(eframe::glow::VERSION))
            })
            .unwrap_or_default();
        let bench = BENCH_SECONDS.get().map(|&seconds| Bench {
            started: Instant::now(),
            last: None,
            seconds,
            frames: vec![],
            cpu: vec![],
            pixels: [0.0; 2],
        });
        App { dock: default_layout(), st, frames: 0, fps_at: Instant::now(), fps: 0.0, gpu, bench, show_fps: false, last_frame: Instant::now() }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let st = &mut self.st;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.label(RichText::new("❄ CRYO").strong().color(ACCENT));
            ui.separator();
            ui.menu_button(tr!("File", "Arquivo"), |ui| {
                if ui.button(tr!("Open or create project…", "Abrir ou criar projeto…")).clicked() {
                    ui.close();
                    st.open_project_dialog();
                }
                if ui
                    .button(tr!("New example: Ashen Keep (hack and slash)…", "Novo exemplo: Ashen Keep (hack and slash)…"))
                    .on_hover_text(tr!(
                        "A medieval action game with one complete level: combat, enemies, gates, a boss and a finishing quick time event.",
                        "Um jogo de ação medieval com uma fase completa: combate, inimigos, portões, um chefe e um quick time event de finalização."
                    ))
                    .clicked()
                {
                    ui.close();
                    st.new_ashen_dialog();
                }
                if ui
                    .button(tr!("New example: Outpost Kestrel (first-person shooter)…", "Novo exemplo: Outpost Kestrel (tiro em primeira pessoa)…"))
                    .on_hover_text(tr!(
                        "A first-person shooter with one complete level: a rifle, security drones, three sectors and a reactor to shut down.",
                        "Um jogo de tiro em primeira pessoa com uma fase completa: fuzil, drones de segurança, três setores e um reator para desligar."
                    ))
                    .clicked()
                {
                    ui.close();
                    st.new_outpost_dialog();
                }
                ui.separator();
                if ui.button(tr!("New scene", "Nova cena")).clicked() {
                    st.new_scene();
                    ui.close();
                }
                if ui.button(tr!("Save scene        Ctrl+S", "Salvar cena        Ctrl+S")).clicked() {
                    st.save_scene();
                    ui.close();
                }
                ui.separator();
                if ui.button(tr!("Import model, texture or sound…", "Importar modelo, textura ou som…")).clicked() {
                    ui.close();
                    st.import_dialog();
                }
                if ui
                    .button(tr!("Import video…", "Importar vídeo…"))
                    .on_hover_text(tr!(
                        "Converts a video (.mp4, .mov, .webm, .mkv, .avi, .gif) into a cutscene the game can play. GIFs work as they are; the other formats need ffmpeg installed on this computer.",
                        "Converte um vídeo (.mp4, .mov, .webm, .mkv, .avi, .gif) em uma cutscene que o jogo toca. GIFs funcionam direto; os outros formatos precisam do ffmpeg instalado neste computador."
                    ))
                    .clicked()
                {
                    ui.close();
                    st.import_video_dialog();
                }
                if ui
                    .button(tr!("Import video from a folder of pictures…", "Importar vídeo de uma pasta de imagens…"))
                    .on_hover_text(tr!(
                        "A folder of numbered PNG or JPEG frames (24 a second) and an optional sound.wav, as any video editor exports. Works without ffmpeg.",
                        "Uma pasta de quadros PNG ou JPEG numerados (24 por segundo) e um sound.wav opcional, como qualquer editor de vídeo exporta. Funciona sem ffmpeg."
                    ))
                    .clicked()
                {
                    ui.close();
                    if let Some(dir) = rfd::FileDialog::new().set_title(tr!("Folder of video frames", "Pasta com os quadros do vídeo")).pick_folder() {
                        st.import_video(&dir);
                    }
                }
                if ui.button(tr!("Export game…", "Exportar jogo…")).on_hover_text(tr!("Creates a folder with the game ready to run without the editor", "Gera uma pasta com o jogo pronto para rodar sem o editor")).clicked() {
                    ui.close();
                    st.export_dialog();
                }
                if ui
                    .button(tr!("Export for another system…", "Exportar para outro sistema…"))
                    .on_hover_text(tr!(
                        "Exports for Windows, macOS or Linux from here. You pick the `cryo` program of that system, from the Cryo Engine download page.",
                        "Exporta para Windows, macOS ou Linux a partir daqui. Você escolhe o programa `cryo` daquele sistema, da página de download da Cryo Engine."
                    ))
                    .clicked()
                {
                    ui.close();
                    st.export_with(true);
                }
                ui.menu_button(tr!("Open scene", "Abrir cena"), |ui| {
                    for rel in project::list_files(&st.project_dir, project::SCENE_EXT) {
                        if ui.button(&rel).clicked() {
                            st.load_scene(&rel);
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui.button(tr!("Quit", "Sair")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button(tr!("Edit", "Editar"), |ui| {
                if ui.button(tr!("Undo               Ctrl+Z", "Desfazer           Ctrl+Z")).clicked() {
                    st.undo();
                    ui.close();
                }
                if ui.button(tr!("Redo         Ctrl+Shift+Z", "Refazer      Ctrl+Shift+Z")).clicked() {
                    st.redo();
                    ui.close();
                }
                ui.separator();
                if ui.button(tr!("Duplicate          Ctrl+D", "Duplicar           Ctrl+D")).clicked() {
                    st.duplicate_selected();
                    ui.close();
                }
                if ui.button(tr!("Delete                Del", "Excluir               Del")).clicked() {
                    st.delete_selected();
                    ui.close();
                }
            });
            ui.menu_button(tr!("Object", "Objeto"), |ui| st.create_menu(ui));
            ui.menu_button(tr!("Window", "Janela"), |ui| {
                st.graphics_choice(ui);
                st.fps_choice(ui);
                st.volume_choice(ui);
                if ui.button(tr!("Controls and gamepad…", "Controles e gamepad…")).clicked() {
                    st.show_controls = true;
                    ui.close();
                }
                ui.separator();
                if ui.button(tr!("Reset default layout", "Restaurar layout padrão")).clicked() {
                    self.dock = default_layout();
                    ui.close();
                }
                ui.checkbox(&mut st.show_grid, tr!("Show grid", "Mostrar grade"));
            });
            ui.menu_button(tr!("Help", "Ajuda"), |ui| {
                if ui.button(tr!("Tutorials…                F1", "Tutoriais…                F1")).clicked() {
                    st.show_tutorials = true;
                    ui.close();
                }
                if ui.button(tr!("Welcome tour", "Tour de boas-vindas")).clicked() {
                    st.start_tutorial("welcome");
                    ui.close();
                }
                ui.hyperlink_to(
                    tr!("Online documentation", "Documentação online"),
                    "https://victor-477.github.io/Cryo-Gen/",
                );
                ui.separator();
                if ui.button(tr!("About Cryo Engine", "Sobre a Cryo Engine")).clicked() {
                    st.show_about = true;
                    ui.close();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button(format!("🌐 {}", crate::i18n::lang().name()), |ui| {
                    for lang in crate::i18n::Lang::ALL {
                        if ui.selectable_label(crate::i18n::lang() == lang, lang.name()).clicked() {
                            st.set_language(lang);
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("Language / Idioma");
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let st = &mut self.st;
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            for (tool, label, tip) in [
                (Tool::Move, tr!("Move", "Mover"), tr!("Move (W)", "Mover (W)")),
                (Tool::Rotate, tr!("Rotate", "Girar"), tr!("Rotate (E)", "Girar (E)")),
                (Tool::Scale, tr!("Scale", "Escala"), tr!("Scale (R)", "Escala (R)")),
            ] {
                if ui.selectable_label(st.tool == tool, label).on_hover_text(tip).clicked() {
                    st.tool = tool;
                }
            }
            ui.separator();
            ui.menu_button(tr!("+ Create", "+ Criar"), |ui| st.create_menu(ui));

            // Play / Pause / Stop in the centre, as in Unity and Unreal
            let w = ui.available_width();
            ui.add_space((w * 0.5 - 150.0).max(8.0));
            let playing = st.playing();
            let paused = st.play.as_ref().map(|p| p.paused).unwrap_or(false);
            let green = Color32::from_rgb(70, 200, 110);
            let play_text = RichText::new(tr!("▶ Play", "▶ Jogar")).color(if playing { Color32::WHITE } else { green }).strong();
            if ui.add(egui::Button::new(play_text).selected(playing)).on_hover_text("Ctrl+P").clicked() && !playing {
                st.start_play();
            }
            if ui.add_enabled(playing, egui::Button::new(tr!("⏸ Pause", "⏸ Pausar")).selected(paused)).clicked() {
                if let Some(p) = &mut st.play {
                    p.paused = !p.paused;
                    p.rt.audio.set_paused(p.paused);
                }
            }
            let stop = RichText::new(tr!("⏹ Stop", "⏹ Parar")).color(if playing { Color32::from_rgb(235, 90, 90) } else { Color32::GRAY });
            if ui.add_enabled(playing, egui::Button::new(stop)).on_hover_text("Esc").clicked() {
                st.stop_play();
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let st = &self.st;
        ui.horizontal(|ui| {
            let star = if st.scene_dirty() { " *" } else { "" };
            ui.label(RichText::new(format!("🎬 {}{star}", st.scene_path)).small());
            ui.separator();
            ui.label(RichText::new(tr!("{} entities", "{} entidades", st.scene.entities.len())).small());
            if st.playing() {
                ui.separator();
                ui.label(RichText::new(tr!("▶ PLAY MODE — changes are discarded when you stop", "▶ MODO JOGO — alterações serão descartadas ao parar")).small().color(ACCENT));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{:.0} fps", self.fps)).small());
                ui.separator();
                ui.label(RichText::new(st.project_dir.display().to_string()).small().weak());
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // frame-rate limit: a running game waits here until its next frame is due. With the
        // screen's sync on, the wait stops a little early and the sync does the exact timing.
        let limit = self.st.settings.fps();
        if limit > 0 && self.bench.is_none() && self.st.playing() {
            let period = std::time::Duration::from_secs_f64((if self.st.vsync_off { 1.0 } else { 0.9 }) / limit as f64);
            let due = self.last_frame + period;
            let now = Instant::now();
            if due > now {
                let wait = due - now;
                // sleep most of it, spin the last millisecond for accuracy
                if wait > std::time::Duration::from_millis(2) {
                    std::thread::sleep(wait - std::time::Duration::from_millis(1));
                }
                while Instant::now() < due {
                    std::hint::spin_loop();
                }
            }
        }
        self.last_frame = Instant::now();
        let frame_start = Instant::now();

        self.frames += 1;
        let el = self.fps_at.elapsed().as_secs_f32();
        if el >= 0.5 {
            self.fps = self.frames as f32 / el;
            self.frames = 0;
            self.fps_at = Instant::now();
        }

        // files dropped on the window are imported into the project
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !dropped.is_empty() && !self.st.game_only {
            self.st.import_paths(&dropped);
        }
        self.st.sync_assets();
        // leaving the edited object (or deleting it) ends Edit Mesh mode, keeping the work
        if let Some(entity) = self.st.mesh_edit.as_ref().map(|s| s.entity) {
            if self.st.selected != Some(entity) || self.st.scene.get(entity).is_none() {
                self.st.finish_mesh_edit(true);
            }
        }

        let frame_dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.st.timeline_tick(frame_dt);
        if self.st.timeline.previewing {
            ctx.request_repaint();
        }

        // --- simulation ---
        // outside Play, the selected rigged model previews its animation
        if self.st.play.is_none() {
            let dt = ctx.input(|i| i.stable_dt).min(0.1);
            if let Some(m) = self.st.selected.and_then(|id| self.st.scene.get_mut(id)).and_then(|e| e.mesh.as_mut()) {
                if m.model.is_some() && !m.animation.is_empty() {
                    m.advance_animation(dt);
                    ctx.request_repaint();
                }
            }
        }
        self.st.collect_input(&ctx);
        self.st.shortcuts(&ctx);
        self.st.apply_mouse_lock(&ctx);
        let mut next_scene = None;
        if let Some(play) = &mut self.st.play {
            if !play.paused {
                let dt = ctx.input(|i| i.stable_dt).min(0.1);
                // CRYO_DEMO=fight skips the opening and walks the hero forward (for screenshots)
                if std::env::var("CRYO_DEMO").as_deref() == Ok("fight") {
                    if play.rt.in_cutscene(&self.st.scene) {
                        play.rt.skip_cutscenes(&mut self.st.scene, &mut self.st.logs);
                    }
                    self.st.input.down.insert("w".into());
                    if play.rt.real_time > 2.2 && (play.rt.real_time * 2.0).fract() < 0.1 {
                        self.st.input.pressed.insert("j".into());
                    }
                }
                let st = &self.st.settings;
                play.rt.audio.set_mix(st.volume_master.unwrap_or(1.0), st.volume_music.unwrap_or(1.0), st.volume_effects.unwrap_or(1.0));
                play.rt.update(&mut self.st.scene, &self.st.input, dt, &mut self.st.logs);
                // scene.load(...) from a script: the next level replaces this one
                next_scene = play.rt.take_scene_request();
                // CRYO_DEMO=qte shows a quick time event shortly after Play (for screenshots)
                if play.rt.qte.is_none() && play.rt.real_time > 1.0 && play.rt.qte_result.is_none() {
                    if std::env::var("CRYO_DEMO").as_deref() == Ok("qte") {
                        let mut q = crate::qte::Qte::new("demo", crate::qte::QteKind::Press, vec!["e".into()], 20.0);
                        q.freeze = true;
                        play.rt.qte = Some(q);
                    }
                }
                if let Some(id) = self.st.selected {
                    if self.st.scene.get(id).is_none() {
                        self.st.selected = None;
                    }
                }
            }
            ctx.request_repaint();
        }
        if let Some(next) = next_scene {
            self.st.switch_scene(&next);
        }

        // --- exported game: only the game screen ---
        if self.st.game_only {
            egui::CentralPanel::no_frame().show(ui, |ui| self.st.game_view(ui));
            // the pause menu (Esc or the gamepad's Start) and its Controls window
            self.st.pause_ui(&ctx);
            if ctx.input(|i| i.key_pressed(Key::F3)) {
                self.show_fps = !self.show_fps;
            }
            if self.show_fps {
                let rect = ctx.content_rect();
                let text = format!("{:.0} fps  {:.1} ms", self.fps, 1000.0 / self.fps.max(0.1));
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("fps")));
                let pos = egui::pos2(rect.right() - 12.0, rect.top() + 10.0);
                let font = egui::FontId::monospace(15.0);
                painter.text(pos + egui::vec2(1.0, 1.0), egui::Align2::RIGHT_TOP, &text, font.clone(), Color32::BLACK);
                painter.text(pos, egui::Align2::RIGHT_TOP, &text, font, Color32::from_rgb(140, 255, 140));
            }
            self.bench_frame(&ctx, frame_start);
            return;
        }

        // --- interface ---
        let bar = egui::Frame::new().fill(Color32::from_rgb(21, 21, 23)).inner_margin(egui::Margin::symmetric(6, 3));
        self.st.menu_rect = egui::Panel::top("menu").frame(bar).show(ui, |ui| self.menu_bar(ui)).response.rect;
        let tb = egui::Frame::new().fill(Color32::from_rgb(36, 36, 39)).inner_margin(egui::Margin::symmetric(6, 5));
        self.st.toolbar_rect = egui::Panel::top("toolbar").frame(tb).show(ui, |ui| self.toolbar(ui)).response.rect;
        egui::Panel::bottom("status").frame(bar).show(ui, |ui| self.status_bar(ui));

        if let Some(tab) = self.st.focus_tab.take() {
            if let Some(path) = self.dock.find_tab(&tab) {
                let _ = self.dock.set_active_tab(path);
            }
        }

        egui::CentralPanel::no_frame().show(ui, |ui| {
            let mut style = Style::from_egui(ui.style());
            style.tab_bar.bg_fill = Color32::from_rgb(21, 21, 23);
            style.separator.width = 3.0;
            DockArea::new(&mut self.dock)
                .style(style)
                .show_close_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .show_inside(ui, &mut self.st);
        });

        if ctx.input(|i| i.key_pressed(Key::F1)) {
            self.st.show_tutorials = !self.st.show_tutorials;
        }
        self.st.tutorial_ui(&ctx);
        self.st.pause_ui(&ctx);
        if !self.st.game_only {
            self.st.controls_ui(&ctx);
        }

        if self.st.show_about {
            egui::Window::new(tr!("About Cryo Engine", "Sobre a Cryo Engine"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut self.st.show_about)
                .show(&ctx, |ui| {
                    ui.heading(RichText::new("❄ Cryo Engine").color(ACCENT));
                    ui.label(tr!("Version {}", "Versão {}", env!("CARGO_PKG_VERSION")));
                    ui.add_space(6.0);
                    ui.label(tr!("3D engine with a visual editor and scripting in the Cryo language.", "Engine 3D com editor visual e scripts na linguagem Cryo."));
                    ui.label(tr!("Import models (.obj, .gltf, .glb), textures and sounds by dragging", "Importe modelos (.obj, .gltf, .glb), texturas e sons arrastando"));
                    ui.label(tr!("the files onto the window. Camera controls: 🎥 Camera menu in the Scene tab.", "os arquivos para a janela. Controles da câmera: menu 🎥 Câmera na aba Cena."));
                });
        }

        self.st.track_history(&ctx);
    }
}

impl App {
    /// Records one frame of the benchmark; when it is over, writes the report and quits.
    fn bench_frame(&mut self, ctx: &egui::Context, frame_start: Instant) {
        let Some(b) = &mut self.bench else { return };
        let now = Instant::now();
        let warm = b.started.elapsed().as_secs_f32() > Bench::WARMUP;
        if let (Some(last), true) = (b.last, warm) {
            b.frames.push((frame_start - last).as_secs_f32() * 1000.0);
            b.cpu.push((now - frame_start).as_secs_f32() * 1000.0);
        }
        b.last = Some(frame_start);
        let r = ctx.content_rect();
        b.pixels = [r.width() * ctx.pixels_per_point(), r.height() * ctx.pixels_per_point()];
        ctx.request_repaint();
        if b.started.elapsed().as_secs_f32() < Bench::WARMUP + b.seconds {
            return;
        }
        let report = b.report(&self.gpu, &self.st);
        println!("{report}");
        let path = self.st.project_dir.join("bench-report.txt");
        if std::fs::write(&path, &report).is_ok() {
            println!("written to {}", path.display());
        }
        self.bench = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl egui_dock::TabViewer for State {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Scene => tr!("🌐 Scene", "🌐 Cena").into(),
            Tab::Game => tr!("🎮 Game", "🎮 Jogo").into(),
            Tab::Script => {
                let star = if self.editor.dirty() { " *" } else { "" };
                let name = self.editor.path.as_deref().and_then(|p| p.rsplit('/').next()).unwrap_or("Script");
                format!("📜 {name}{star}").into()
            }
            Tab::Hierarchy => tr!("☰ Hierarchy", "☰ Hierarquia").into(),
            Tab::Inspector => tr!("ℹ Inspector", "ℹ Inspetor").into(),
            Tab::Content => tr!("📁 Content", "📁 Conteúdo").into(),
            Tab::Timeline => tr!("🎬 Timeline", "🎬 Linha do tempo").into(),
            Tab::Console => {
                let errors = self.logs.iter().filter(|l| l.level == LogLevel::Error).count();
                if errors > 0 {
                    format!("🖥 Console ({errors})").into()
                } else {
                    "🖥 Console".into()
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        self.tab_rects.insert(*tab, ui.max_rect());
        match tab {
            Tab::Scene => self.scene_view(ui),
            Tab::Game => self.game_view(ui),
            Tab::Script => self.script_editor(ui),
            Tab::Hierarchy => self.hierarchy(ui),
            Tab::Inspector => self.inspector(ui),
            Tab::Content => self.content_browser(ui),
            Tab::Console => self.console(ui),
            Tab::Timeline => self.timeline_panel(ui),
        }
    }

    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        [false, false]
    }

    fn clear_background(&self, tab: &Tab) -> bool {
        !matches!(tab, Tab::Scene | Tab::Game)
    }
}

fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    let panel = Color32::from_rgb(36, 36, 39);
    v.panel_fill = panel;
    v.window_fill = Color32::from_rgb(30, 30, 33);
    v.extreme_bg_color = Color32::from_rgb(18, 18, 20);
    v.faint_bg_color = Color32::from_rgb(44, 44, 48);
    v.selection.bg_fill = Color32::from_rgb(28, 92, 180);
    v.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    v.widgets.noninteractive.bg_fill = panel;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(20, 20, 22));
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(200, 200, 204));
    v.widgets.inactive.bg_fill = Color32::from_rgb(56, 56, 61);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(52, 52, 57);
    v.widgets.hovered.bg_fill = Color32::from_rgb(72, 72, 79);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(68, 68, 75);
    v.widgets.active.bg_fill = Color32::from_rgb(28, 92, 180);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(28, 92, 180);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(7.0, 3.0);
    });
}
