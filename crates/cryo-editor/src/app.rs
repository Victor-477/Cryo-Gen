//! Estado do editor, barra de menus, barra de ferramentas e layout de painéis.

use crate::assets::{self, Assets};
use crate::project;
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
    pub show_grid: bool,
    pub assets: Assets,
    /// Jogo exportado: sem editor, só a tela do jogo.
    pub game_only: bool,
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
                self.info(format!("Cena aberta: {rel}"));
            }
            Err(e) => self.error(format!("Não foi possível abrir {rel}: {e}")),
        }
    }

    pub fn save_scene(&mut self) {
        if self.playing() {
            self.info("Pare o jogo antes de salvar a cena.");
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
                self.info(format!("Cena salva: {}", self.scene_path));
            }
            Err(e) => self.error(format!("Erro ao salvar a cena: {e}")),
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
        self.saved_json.clear(); // ainda não está em disco
        self.info("Nova cena criada (salve com Ctrl+S).");
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
            Err(e) => self.error(format!("Não foi possível abrir {rel}: {e}")),
        }
    }

    pub fn save_script(&mut self) {
        let Some(rel) = self.editor.path.clone() else { return };
        match std::fs::write(self.project_dir.join(&rel), &self.editor.text) {
            Ok(()) => {
                self.editor.saved = self.editor.text.clone();
                self.prop_cache.remove(&rel);
                match &self.editor.error {
                    None => self.info(format!("Script salvo: {rel}")),
                    Some(e) => {
                        let msg = format!("{rel} salvo com erro, {e}");
                        log(&mut self.logs, LogLevel::Warn, msg);
                    }
                }
            }
            Err(e) => self.error(format!("Erro ao salvar {rel}: {e}")),
        }
    }

    /// Importa arquivos externos para o projeto (modelos, texturas, sons).
    pub fn import_paths(&mut self, paths: &[PathBuf]) {
        for src in paths {
            match project::import_file(&self.project_dir, src) {
                Ok(rel) => {
                    self.assets.invalidate(&rel);
                    self.browser_dir = rel.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
                    let ext = assets::ext_of(&rel);
                    if assets::MODEL_EXTS.contains(&ext.as_str()) {
                        match self.assets.model(&rel) {
                            Some(m) => {
                                self.info(format!("Modelo importado: {rel} ({} triângulos)", m.triangles));
                                if !self.playing() {
                                    self.add_model_to_scene(&rel);
                                }
                            }
                            None => self.focus_tab = Some(Tab::Console),
                        }
                    } else {
                        self.info(format!("Arquivo importado: {rel}"));
                    }
                    if self.focus_tab.is_none() {
                        self.focus_tab = Some(Tab::Content);
                    }
                }
                Err(e) => self.error(format!("Importação: {e}")),
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
            .set_title("Importar para o projeto")
            .add_filter("Tudo que a Cryo importa", &all)
            .add_filter("Modelos 3D (.obj, .gltf, .glb)", assets::MODEL_EXTS)
            .add_filter("Imagens (.png, .jpg)", assets::IMAGE_EXTS)
            .add_filter("Sons (.wav, .ogg, .mp3)", assets::SOUND_EXTS)
            .pick_files();
        if let Some(paths) = picked {
            self.import_paths(&paths);
        }
    }

    /// Cria uma entidade com o modelo, já em tamanho razoável e à frente da câmera.
    pub fn add_model_to_scene(&mut self, rel: &str) {
        let Some(model) = self.assets.model(rel) else {
            self.focus_tab = Some(Tab::Console);
            return;
        };
        let name = std::path::Path::new(rel).file_stem().and_then(|s| s.to_str()).unwrap_or("Modelo").to_string();
        let id = self.scene.add_mesh(Shape::Cube);
        let biggest = (model.half * 2.0).max_element();
        // modelos gigantes ou minúsculos (unidades diferentes) entram com ~2 m
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
        // apoia a base do modelo no chão
        if let Some(e) = self.scene.get_mut(id) {
            e.position.y = (model.half.y - model.center.y) * scale;
        }
        if scale != 1.0 {
            self.info(format!("O modelo tinha {biggest:.2} unidades; a escala foi ajustada para {scale:.4}."));
        }
    }

    pub fn export_dialog(&mut self) {
        if self.scene_dirty() {
            self.save_scene();
        }
        let Some(dest) = rfd::FileDialog::new().set_title("Escolha onde salvar o jogo exportado").pick_folder()
        else {
            return;
        };
        let title = self.project_dir.file_name().and_then(|n| n.to_str()).unwrap_or("Jogo").to_string();
        match project::export_game(&self.project_dir, &self.scene_path, &title, &dest) {
            Ok(out) => {
                self.info(format!("Jogo exportado: {}", out.display()));
                self.focus_tab = Some(Tab::Console);
            }
            Err(e) => self.error(e),
        }
    }

    /// Mantém a caixa envolvente salva na cena igual à do modelo em disco e mostra erros de carregamento.
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

    pub fn start_play(&mut self) {
        if self.playing() {
            return;
        }
        if self.editor.dirty() {
            self.save_script();
        }
        let backup = self.scene.clone();
        self.info("▶ Jogo iniciado");
        let rt = Runtime::start(&mut self.scene, &self.project_dir, &mut self.logs);
        if self.logs.iter().rev().take(20).any(|l| l.level == LogLevel::Error) {
            self.focus_tab = Some(Tab::Console);
        } else {
            self.focus_tab = Some(Tab::Game);
        }
        self.play = Some(Play { backup, backup_selected: self.selected, rt, paused: false });
        self.drag = None;
    }

    pub fn stop_play(&mut self) {
        if let Some(p) = self.play.take() {
            self.scene = p.backup;
            self.selected = p.backup_selected;
            self.info("■ Jogo parado");
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

    /// Registra um passo de desfazer quando a cena muda e nenhuma edição está em andamento.
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

    /// Posiciona um objeto recém-criado à frente da câmera do editor.
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

    pub fn create_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button("○  Vazio").clicked() {
            let id = self.scene.add_empty("Vazio");
            self.place_new(id);
            ui.close();
        }
        ui.separator();
        for shape in Shape::ALL {
            if ui.button(format!("■  {}", shape.label())).clicked() {
                let id = self.scene.add_mesh(shape);
                self.place_new(id);
                ui.close();
            }
        }
        ui.separator();
        if ui.button("☀  Luz direcional").clicked() {
            let id = self.scene.add_light(LightKind::Directional);
            self.place_new(id);
            ui.close();
        }
        if ui.button("💡  Luz pontual").clicked() {
            let id = self.scene.add_light(LightKind::Point);
            self.place_new(id);
            ui.close();
        }
        if ui.button("🔦  Holofote").clicked() {
            let id = self.scene.add_light(LightKind::Spot);
            self.place_new(id);
            ui.close();
        }
        if ui.button("🎥  Câmera").clicked() {
            let id = self.scene.add_camera();
            self.place_new(id);
            ui.close();
        }
    }

    fn collect_input(&mut self, ctx: &egui::Context) {
        self.input.pressed.clear();
        self.input.released.clear();
        self.input.mouse_dx = 0.0;
        self.input.mouse_dy = 0.0;
        self.input.scroll = 0.0;
        if !self.playing() || ctx.egui_wants_keyboard_input() {
            self.input.down.clear();
            return;
        }
        ctx.input(|i| {
            for ev in &i.events {
                if let egui::Event::Key { key, pressed, repeat, .. } = ev {
                    let name = key.name().to_lowercase();
                    if *pressed && !*repeat {
                        self.input.pressed.insert(name.clone());
                        self.input.down.insert(name);
                    } else if !*pressed {
                        self.input.released.insert(name.clone());
                        self.input.down.remove(&name);
                    }
                }
            }
            for (name, button) in [
                ("mouse_left", egui::PointerButton::Primary),
                ("mouse_right", egui::PointerButton::Secondary),
                ("mouse_middle", egui::PointerButton::Middle),
            ] {
                let was = self.input.down.contains(name);
                let now = i.pointer.button_down(button);
                if now && !was {
                    self.input.pressed.insert(name.into());
                    self.input.down.insert(name.into());
                } else if !now && was {
                    self.input.released.insert(name.into());
                    self.input.down.remove(name);
                }
            }
            let d = i.pointer.delta();
            self.input.mouse_dx = d.x;
            self.input.mouse_dy = d.y;
            self.input.scroll = i.smooth_scroll_delta.y;
            if i.modifiers.shift {
                self.input.down.insert("shift".into());
            } else {
                self.input.down.remove("shift");
            }
            if !i.focused {
                self.input.down.clear();
            }
        });
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
            if pressed(Key::Escape) {
                if self.game_only {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    self.stop_play();
                }
            }
            return;
        }
        if typing {
            return;
        }
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

pub struct App {
    dock: DockState<Tab>,
    st: State,
    frames: u32,
    fps_at: Instant,
    fps: f32,
}

fn default_layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Scene, Tab::Game, Tab::Script]);
    let tree = dock.main_surface_mut();
    let [center, _right] = tree.split_right(NodeIndex::root(), 0.78, vec![Tab::Inspector]);
    let [center, _left] = tree.split_left(center, 0.19, vec![Tab::Hierarchy]);
    let [_center, _bottom] = tree.split_below(center, 0.70, vec![Tab::Content, Tab::Console]);
    dock
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        project_dir: PathBuf,
        autoplay: bool,
        game: Option<project::GameConfig>,
    ) -> Self {
        apply_theme(&cc.egui_ctx);
        let (renderer, render_error) = match cc.gl.as_ref() {
            Some(gl) => match Renderer::new(gl) {
                Ok(r) => (Some(Arc::new(Mutex::new(r))), None),
                Err(e) => (None, Some(e)),
            },
            None => (None, Some("OpenGL indisponível".to_string())),
        };
        let project_dir = std::fs::canonicalize(&project_dir).unwrap_or_else(|_| {
            std::env::current_dir().map(|d| d.join(&project_dir)).unwrap_or(project_dir.clone())
        });

        let game_only = game.is_some();
        let mut st = State {
            assets: Assets::new(project_dir.clone()),
            game_only,
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
            show_grid: true,
        };
        st.info(format!("Cryo Engine {} — projeto: {}", env!("CARGO_PKG_VERSION"), st.project_dir.display()));
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
            st.error(format!("Renderizador: {e}"));
        }
        if let Some(first) = project::list_files(&st.project_dir, project::SCRIPT_EXT).first().cloned() {
            st.open_script(&first);
        }
        st.focus_tab = Some(Tab::Scene);
        // Útil para testes automatizados: CRYO_SELECT="Nome da entidade"
        if let Ok(name) = std::env::var("CRYO_SELECT") {
            st.selected = st.scene.find_by_name(&name);
        }
        if std::env::var("CRYO_VIEW").as_deref() == Ok("top") {
            st.cam.view_from(glam::Vec3::Y, "Topo");
        }
        if std::env::var("CRYO_TAB").as_deref() == Ok("script") {
            st.focus_tab = Some(Tab::Script);
        }
        if autoplay || game_only {
            st.start_play();
        }
        App { dock: default_layout(), st, frames: 0, fps_at: Instant::now(), fps: 0.0 }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let st = &mut self.st;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.label(RichText::new("❄ CRYO").strong().color(ACCENT));
            ui.separator();
            ui.menu_button("Arquivo", |ui| {
                if ui.button("Nova cena").clicked() {
                    st.new_scene();
                    ui.close();
                }
                if ui.button("Salvar cena        Ctrl+S").clicked() {
                    st.save_scene();
                    ui.close();
                }
                ui.separator();
                if ui.button("Importar modelo, textura ou som…").clicked() {
                    ui.close();
                    st.import_dialog();
                }
                if ui.button("Exportar jogo…").on_hover_text("Gera uma pasta com o jogo pronto para rodar sem o editor").clicked() {
                    ui.close();
                    st.export_dialog();
                }
                ui.menu_button("Abrir cena", |ui| {
                    for rel in project::list_files(&st.project_dir, project::SCENE_EXT) {
                        if ui.button(&rel).clicked() {
                            st.load_scene(&rel);
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui.button("Sair").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Editar", |ui| {
                if ui.button("Desfazer           Ctrl+Z").clicked() {
                    st.undo();
                    ui.close();
                }
                if ui.button("Refazer      Ctrl+Shift+Z").clicked() {
                    st.redo();
                    ui.close();
                }
                ui.separator();
                if ui.button("Duplicar           Ctrl+D").clicked() {
                    st.duplicate_selected();
                    ui.close();
                }
                if ui.button("Excluir               Del").clicked() {
                    st.delete_selected();
                    ui.close();
                }
            });
            ui.menu_button("Objeto", |ui| st.create_menu(ui));
            ui.menu_button("Janela", |ui| {
                if ui.button("Restaurar layout padrão").clicked() {
                    self.dock = default_layout();
                    ui.close();
                }
                ui.checkbox(&mut st.show_grid, "Mostrar grade");
            });
            ui.menu_button("Ajuda", |ui| {
                if ui.button("Sobre a Cryo Engine").clicked() {
                    st.show_about = true;
                    ui.close();
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let st = &mut self.st;
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            for (tool, label, tip) in [
                (Tool::Move, "Mover", "Mover (W)"),
                (Tool::Rotate, "Girar", "Girar (E)"),
                (Tool::Scale, "Escala", "Escala (R)"),
            ] {
                if ui.selectable_label(st.tool == tool, label).on_hover_text(tip).clicked() {
                    st.tool = tool;
                }
            }
            ui.separator();
            ui.menu_button("+ Criar", |ui| st.create_menu(ui));

            // Play / Pause / Stop no centro, como na Unity e na Unreal
            let w = ui.available_width();
            ui.add_space((w * 0.5 - 150.0).max(8.0));
            let playing = st.playing();
            let paused = st.play.as_ref().map(|p| p.paused).unwrap_or(false);
            let green = Color32::from_rgb(70, 200, 110);
            let play_text = RichText::new("▶ Jogar").color(if playing { Color32::WHITE } else { green }).strong();
            if ui.add(egui::Button::new(play_text).selected(playing)).on_hover_text("Ctrl+P").clicked() && !playing {
                st.start_play();
            }
            if ui.add_enabled(playing, egui::Button::new("⏸ Pausar").selected(paused)).clicked() {
                if let Some(p) = &mut st.play {
                    p.paused = !p.paused;
                    p.rt.audio.set_paused(p.paused);
                }
            }
            let stop = RichText::new("⏹ Parar").color(if playing { Color32::from_rgb(235, 90, 90) } else { Color32::GRAY });
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
            ui.label(RichText::new(format!("{} entidades", st.scene.entities.len())).small());
            if st.playing() {
                ui.separator();
                ui.label(RichText::new("▶ MODO JOGO — alterações serão descartadas ao parar").small().color(ACCENT));
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

        self.frames += 1;
        let el = self.fps_at.elapsed().as_secs_f32();
        if el >= 0.5 {
            self.fps = self.frames as f32 / el;
            self.frames = 0;
            self.fps_at = Instant::now();
        }

        // arquivos arrastados para a janela são importados para o projeto
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !dropped.is_empty() && !self.st.game_only {
            self.st.import_paths(&dropped);
        }
        self.st.sync_assets();

        // --- simulação ---
        self.st.collect_input(&ctx);
        self.st.shortcuts(&ctx);
        if let Some(play) = &mut self.st.play {
            if !play.paused {
                let dt = ctx.input(|i| i.stable_dt).min(0.1);
                play.rt.update(&mut self.st.scene, &self.st.input, dt, &mut self.st.logs);
                if let Some(id) = self.st.selected {
                    if self.st.scene.get(id).is_none() {
                        self.st.selected = None;
                    }
                }
            }
            ctx.request_repaint();
        }

        // --- jogo exportado: só a tela do jogo ---
        if self.st.game_only {
            egui::CentralPanel::no_frame().show(ui, |ui| self.st.game_view(ui));
            return;
        }

        // --- interface ---
        let bar = egui::Frame::new().fill(Color32::from_rgb(21, 21, 23)).inner_margin(egui::Margin::symmetric(6, 3));
        egui::Panel::top("menu").frame(bar).show(ui, |ui| self.menu_bar(ui));
        let tb = egui::Frame::new().fill(Color32::from_rgb(36, 36, 39)).inner_margin(egui::Margin::symmetric(6, 5));
        egui::Panel::top("toolbar").frame(tb).show(ui, |ui| self.toolbar(ui));
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

        if self.st.show_about {
            egui::Window::new("Sobre a Cryo Engine")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut self.st.show_about)
                .show(&ctx, |ui| {
                    ui.heading(RichText::new("❄ Cryo Engine").color(ACCENT));
                    ui.label(format!("Versão {}", env!("CARGO_PKG_VERSION")));
                    ui.add_space(6.0);
                    ui.label("Engine 3D com editor visual e scripts na linguagem Cryo.");
                    ui.label("Importe modelos (.obj, .gltf, .glb), texturas e sons arrastando");
                    ui.label("os arquivos para a janela. Controles da câmera: menu 🎥 Câmera na aba Cena.");
                });
        }

        self.st.track_history(&ctx);
    }
}

impl egui_dock::TabViewer for State {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Scene => "🌐 Cena".into(),
            Tab::Game => "🎮 Jogo".into(),
            Tab::Script => {
                let star = if self.editor.dirty() { " *" } else { "" };
                let name = self.editor.path.as_deref().and_then(|p| p.rsplit('/').next()).unwrap_or("Script");
                format!("📜 {name}{star}").into()
            }
            Tab::Hierarchy => "☰ Hierarquia".into(),
            Tab::Inspector => "ℹ Inspetor".into(),
            Tab::Content => "📁 Conteúdo".into(),
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
        match tab {
            Tab::Scene => self.scene_view(ui),
            Tab::Game => self.game_view(ui),
            Tab::Script => self.script_editor(ui),
            Tab::Hierarchy => self.hierarchy(ui),
            Tab::Inspector => self.inspector(ui),
            Tab::Content => self.content_browser(ui),
            Tab::Console => self.console(ui),
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
