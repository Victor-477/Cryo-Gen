//! Editor panels: Hierarchy, Inspector, Content, Console and Script Editor.

use crate::app::{PropCache, State, ACCENT};
use crate::highlight::highlight;
use crate::project::{self, SCENE_EXT, SCRIPT_EXT};
use crate::runtime::{value_to_prop, LogLevel};
use crate::scene::*;
use eframe::egui::{self, Color32, DragValue, RichText, ScrollArea, Sense};
use glam::Vec3;
use std::rc::Rc;
use std::time::Instant;

fn entity_icon(e: &Entity) -> &'static str {
    if e.camera.is_some() {
        "🎥"
    } else if e.light.is_some() {
        "☀"
    } else if e.mesh.is_some() {
        "■"
    } else {
        "○"
    }
}

fn vec3_row(ui: &mut egui::Ui, label: impl AsRef<str>, v: &mut Vec3, speed: f32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([62.0, 18.0], egui::Label::new(label.as_ref()).truncate());
        let colors = [
            Color32::from_rgb(235, 90, 90),
            Color32::from_rgb(120, 210, 80),
            Color32::from_rgb(90, 150, 250),
        ];
        let w = ((ui.available_width() - 3.0 * 22.0) / 3.0).clamp(36.0, 90.0);
        for (i, name) in ["X", "Y", "Z"].iter().enumerate() {
            ui.label(RichText::new(*name).color(colors[i]).strong());
            let mut f = v[i];
            if ui.add_sized([w, 18.0], DragValue::new(&mut f).speed(speed).max_decimals(3)).changed() {
                v[i] = f;
                changed = true;
            }
        }
    });
    changed
}

/// Colour button that only writes back when the user actually changes it
/// (egui's internal colour conversion introduces small rounding differences).
fn color_edit(ui: &mut egui::Ui, color: &mut [f32; 3]) -> egui::Response {
    let mut tmp = *color;
    let resp = ui.color_edit_button_rgb(&mut tmp);
    if resp.changed() {
        *color = tmp;
    }
    resp
}

fn row<R>(ui: &mut egui::Ui, label: impl AsRef<str>, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.add_sized([110.0, 18.0], egui::Label::new(label.as_ref()).truncate());
        add(ui)
    })
    .inner
}

/// Component header with a remove button. Returns `true` if the user removed it.
fn component(ui: &mut egui::Ui, id_key: &str, title: impl AsRef<str>, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let mut remove = false;
    ui.add_space(2.0);
    egui::Frame::new()
        .fill(Color32::from_rgb(44, 44, 48))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let id = ui.make_persistent_id(("comp", id_key));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    ui.label(RichText::new(title.as_ref()).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✖").on_hover_text(tr!("Remove component", "Remover componente")).clicked() {
                            remove = true;
                        }
                    });
                })
                .body(|ui| body(ui));
        });
    remove
}

impl State {
    // ------------------------------------------------------------------
    // Hierarchy
    // ------------------------------------------------------------------
    pub fn hierarchy(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.menu_button("+", |ui| self.create_menu(ui)).response.on_hover_text(tr!("Create object", "Criar objeto"));
            ui.label(RichText::new(self.scene_path.rsplit('/').next().unwrap_or("")).weak());
        });
        ui.separator();

        let mut reparent: Option<(u64, Option<u64>)> = None;
        let mut action: Option<(u64, &'static str)> = None;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut stack: Vec<(u64, usize)> =
                self.scene.children(None).into_iter().rev().map(|id| (id, 0)).collect();
            while let Some((id, depth)) = stack.pop() {
                for c in self.scene.children(Some(id)).into_iter().rev() {
                    stack.push((c, depth + 1));
                }
                let Some(e) = self.scene.get(id) else { continue };
                let selected = self.selected == Some(id);
                let mut text = RichText::new(format!("{} {}", entity_icon(e), e.name));
                if !self.scene.is_visible(id) {
                    text = text.weak().italics();
                }
                if e.script.is_some() {
                    text = text.color(if selected { Color32::WHITE } else { Color32::from_rgb(150, 205, 255) });
                }
                let resp = ui
                    .horizontal(|ui| {
                        ui.add_space(depth as f32 * 14.0 + 2.0);
                        ui.selectable_label(selected, text)
                    })
                    .inner;
                let resp = resp.interact(Sense::click_and_drag());
                if resp.clicked() {
                    self.selected = Some(id);
                }
                resp.dnd_set_drag_payload(id);
                if let Some(dragged) = resp.dnd_release_payload::<u64>() {
                    reparent = Some((*dragged, Some(id)));
                }
                resp.context_menu(|ui| {
                    for (label, act) in [
                        (tr!("Duplicate", "Duplicar"), "dup"),
                        (tr!("Create empty child", "Criar filho vazio"), "child"),
                        (tr!("Unparent", "Soltar do pai"), "unparent"),
                        (tr!("Delete", "Excluir"), "del"),
                    ] {
                        if ui.button(label).clicked() {
                            action = Some((id, act));
                            ui.close();
                        }
                    }
                });
            }
            // dropping on empty space = unparent
            let rest = ui.allocate_response(ui.available_size(), Sense::click());
            if rest.clicked() {
                self.selected = None;
            }
            if let Some(dragged) = rest.dnd_release_payload::<u64>() {
                reparent = Some((*dragged, None));
            }
        });

        if let Some((child, parent)) = reparent {
            self.set_parent(child, parent);
        }
        if let Some((id, act)) = action {
            match act {
                "dup" => self.selected = self.scene.duplicate(id),
                "child" => {
                    let c = self.scene.add_empty("Empty");
                    self.scene.get_mut(c).unwrap().parent = Some(id);
                    self.selected = Some(c);
                }
                "unparent" => self.set_parent(id, None),
                "del" => {
                    self.scene.remove(id);
                    if self.selected.map(|s| self.scene.get(s).is_none()).unwrap_or(false) {
                        self.selected = None;
                    }
                }
                _ => {}
            }
        }
    }

    /// Changes the parent while keeping the object's world position.
    fn set_parent(&mut self, child: u64, parent: Option<u64>) {
        if Some(child) == parent || self.scene.get(child).map(|e| e.parent) == Some(parent) {
            return;
        }
        if let Some(p) = parent {
            if self.scene.is_descendant(p, child) {
                return; // would create a cycle
            }
        }
        let world = self.scene.world_matrix(child);
        let parent_world = parent.map(|p| self.scene.world_matrix(p)).unwrap_or(glam::Mat4::IDENTITY);
        if parent_world.determinant().abs() < 1e-9 {
            return;
        }
        let local = parent_world.inverse() * world;
        let (scale, rot, pos) = local.to_scale_rotation_translation();
        let (y, x, z) = rot.to_euler(glam::EulerRot::YXZ);
        if let Some(e) = self.scene.get_mut(child) {
            e.parent = parent;
            e.position = pos;
            e.scale = scale;
            e.rotation = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
        }
    }

    // ------------------------------------------------------------------
    // Inspector
    // ------------------------------------------------------------------
    fn script_props(&mut self, rel: &str) -> Result<Vec<(String, Option<cryo_lang::Type>, cryo_lang::Value)>, String> {
        let fresh = self.prop_cache.get(rel).map(|c| c.at.elapsed().as_secs_f32() < 1.0).unwrap_or(false);
        if !fresh {
            let props = std::fs::read_to_string(self.project_dir.join(rel))
                .map_err(|e| tr!("file not found ({e})", "arquivo não encontrado ({e})"))
                .and_then(|src| cryo_lang::parse(&src).map_err(|e| e.to_string()))
                .map(|s| cryo_lang::default_props(&Rc::new(s)));
            self.prop_cache.insert(rel.to_string(), PropCache { at: Instant::now(), props });
        }
        self.prop_cache[rel].props.clone()
    }

    pub fn inspector(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let Some(id) = self.selected.filter(|id| self.scene.get(*id).is_some()) else {
                self.scene_settings(ui);
                return;
            };
            let scripts = project::list_files(&self.project_dir, SCRIPT_EXT);
            let script_path = self.scene.get(id).and_then(|e| e.script.as_ref().map(|s| s.path.clone()));
            let props = match &script_path {
                Some(p) if !p.is_empty() => Some(self.script_props(p)),
                _ => None,
            };
            let mut open_script: Option<String> = None;
            let mut new_script = false;
            let list_any = |exts: &[&str]| -> Vec<String> {
                let mut v: Vec<String> = exts.iter().flat_map(|x| project::list_files(&self.project_dir, x)).collect();
                v.sort();
                v
            };
            let models = list_any(crate::assets::MODEL_EXTS);
            let images = list_any(crate::assets::IMAGE_EXTS);
            let model_info = self
                .scene
                .get(id)
                .and_then(|e| e.mesh.as_ref())
                .and_then(|m| m.model.clone())
                .and_then(|p| self.assets.model(&p));
            let mut fit_size = false;

            let e = self.scene.get_mut(id).unwrap();
            ui.horizontal(|ui| {
                ui.checkbox(&mut e.visible, "").on_hover_text(tr!("Visible", "Visível"));
                ui.add(egui::TextEdit::singleline(&mut e.name).desired_width(f32::INFINITY));
            });
            ui.add_space(2.0);

            egui::Frame::new()
                .fill(Color32::from_rgb(44, 44, 48))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(tr!("Transform", "Transformação")).strong());
                    vec3_row(ui, tr!("Position", "Posição"), &mut e.position, 0.05);
                    vec3_row(ui, tr!("Rotation", "Rotação"), &mut e.rotation, 0.5);
                    vec3_row(ui, tr!("Scale", "Escala"), &mut e.scale, 0.02);
                });

            if let Some(m) = &mut e.mesh {
                let remove = component(ui, "mesh", tr!("■ Mesh", "■ Malha"), |ui| {
                    let file_name = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
                    row(ui, tr!("Shape", "Forma"), |ui| {
                        let shown = match &m.model {
                            Some(p) => file_name(p),
                            None => crate::app::shape_name(m.shape),
                        };
                        egui::ComboBox::from_id_salt("shape").selected_text(shown).show_ui(ui, |ui| {
                            for s in Shape::ALL {
                                if ui.selectable_label(m.model.is_none() && m.shape == s, crate::app::shape_name(s)).clicked() {
                                    m.shape = s;
                                    m.model = None;
                                    m.model_bounds = None;
                                }
                            }
                            if !models.is_empty() {
                                ui.separator();
                                ui.label(RichText::new(tr!("Imported models", "Modelos importados")).weak().small());
                            }
                            for f in &models {
                                if ui.selectable_label(m.model.as_ref() == Some(f), format!("◆ {f}")).clicked() {
                                    m.model = Some(f.clone());
                                    m.model_bounds = None;
                                }
                            }
                        });
                    });
                    if m.model.is_some() {
                        match &model_info {
                            Some(info) => {
                                let d = info.half * 2.0;
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(tr!("{} triangles · {:.2} × {:.2} × {:.2}", "{} triângulos · {:.2} × {:.2} × {:.2}",
                                            info.triangles, d.x, d.y, d.z
                                        ))
                                        .weak()
                                        .small(),
                                    );
                                    if ui.small_button(tr!("Fit size", "Ajustar tamanho")).on_hover_text(tr!("Scales the object so its largest dimension is 2 m", "Escala o objeto para ter 2 m na maior dimensão")).clicked() {
                                        fit_size = true;
                                    }
                                });
                            }
                            None => {
                                ui.colored_label(Color32::from_rgb(240, 110, 110), tr!("⚠ model failed to load (see the Console)", "⚠ modelo não carregou (veja o Console)"));
                            }
                        }
                    }
                    row(ui, tr!("Texture", "Textura"), |ui| {
                        let shown = m.texture.as_deref().map(file_name).unwrap_or_else(|| tr!("(none)", "(nenhuma)").into());
                        egui::ComboBox::from_id_salt("texture").selected_text(shown).show_ui(ui, |ui| {
                            if ui.selectable_label(m.texture.is_none(), tr!("(none)", "(nenhuma)")).clicked() {
                                m.texture = None;
                            }
                            for f in &images {
                                if ui.selectable_label(m.texture.as_ref() == Some(f), f).clicked() {
                                    m.texture = Some(f.clone());
                                }
                            }
                        });
                    });
                    row(ui, tr!("Colour", "Cor"), |ui| color_edit(ui, &mut m.color));
                    row(ui, tr!("Shininess", "Brilho"), |ui| ui.add(egui::Slider::new(&mut m.shininess, 0.0..=1.0)));
                    row(ui, tr!("Unlit", "Sem iluminação"), |ui| ui.checkbox(&mut m.unlit, ""));
                    row(ui, tr!("Casts shadow", "Projeta sombra"), |ui| ui.checkbox(&mut m.cast_shadow, ""));
                });
                if fit_size {
                    if let Some(info) = &model_info {
                        e.scale = Vec3::splat(2.0 / (info.half * 2.0).max_element().max(1e-6));
                    }
                }
                if remove {
                    e.mesh = None;
                }
            }

            if let Some(l) = &mut e.light {
                let remove = component(ui, "light", tr!("☀ Light", "☀ Luz"), |ui| {
                    row(ui, tr!("Type", "Tipo"), |ui| {
                        ui.selectable_value(&mut l.kind, LightKind::Directional, tr!("Directional", "Direcional"));
                        ui.selectable_value(&mut l.kind, LightKind::Point, tr!("Point", "Pontual"));
                        ui.selectable_value(&mut l.kind, LightKind::Spot, tr!("Spot", "Holofote"));
                    });
                    row(ui, tr!("Colour", "Cor"), |ui| color_edit(ui, &mut l.color));
                    row(ui, tr!("Intensity", "Intensidade"), |ui| ui.add(egui::Slider::new(&mut l.intensity, 0.0..=8.0)));
                    if l.kind != LightKind::Directional {
                        row(ui, tr!("Range", "Alcance"), |ui| ui.add(egui::Slider::new(&mut l.range, 0.5..=60.0)));
                    }
                    if l.kind == LightKind::Spot {
                        row(ui, tr!("Cone angle", "Abertura"), |ui| ui.add(egui::Slider::new(&mut l.angle, 5.0..=80.0).suffix("°")));
                    }
                    if l.kind == LightKind::Directional {
                        row(ui, tr!("Shadows", "Sombras"), |ui| {
                            ui.checkbox(&mut l.shadows, "").on_hover_text(tr!("Applies to the first directional light in the scene", "Vale para a primeira luz direcional da cena"))
                        });
                    }
                });
                if remove {
                    e.light = None;
                }
            }

            if let Some(c) = &mut e.camera {
                let remove = component(ui, "camera", tr!("🎥 Camera", "🎥 Câmera"), |ui| {
                    row(ui, tr!("Field of view", "Campo de visão"), |ui| ui.add(egui::Slider::new(&mut c.fov, 20.0..=120.0).suffix("°")));
                    row(ui, tr!("Near plane", "Plano próximo"), |ui| ui.add(DragValue::new(&mut c.near).speed(0.01).range(0.01..=10.0)));
                    row(ui, tr!("Far plane", "Plano distante"), |ui| ui.add(DragValue::new(&mut c.far).speed(1.0).range(10.0..=5000.0)));
                });
                if remove {
                    e.camera = None;
                }
            }

            if let Some(p) = &mut e.physics {
                let remove = component(ui, "physics", tr!("⚛ Physics", "⚛ Física"), |ui| {
                    row(ui, tr!("Body", "Corpo"), |ui| {
                        ui.selectable_value(&mut p.kind, BodyKind::Static, tr!("Static", "Estático"));
                        ui.selectable_value(&mut p.kind, BodyKind::Dynamic, tr!("Dynamic", "Dinâmico"));
                    });
                    row(ui, tr!("Collider", "Colisor"), |ui| {
                        ui.selectable_value(&mut p.collider, ColliderShape::Box, tr!("Box", "Caixa"));
                        ui.selectable_value(&mut p.collider, ColliderShape::Sphere, tr!("Sphere", "Esfera"));
                    });
                    row(ui, tr!("Trigger", "Gatilho"), |ui| {
                        ui.checkbox(&mut p.is_trigger, "").on_hover_text(tr!("Detects contact without blocking", "Detecta contato sem bloquear"))
                    });
                    if p.kind == BodyKind::Dynamic {
                        row(ui, tr!("Gravity", "Gravidade"), |ui| ui.checkbox(&mut p.use_gravity, ""));
                        row(ui, tr!("Bounciness", "Quique"), |ui| ui.add(egui::Slider::new(&mut p.bounciness, 0.0..=1.0)));
                        row(ui, tr!("Friction", "Atrito"), |ui| ui.add(egui::Slider::new(&mut p.friction, 0.0..=1.0)));
                    }
                });
                if remove {
                    e.physics = None;
                }
            }

            if let Some(s) = &mut e.script {
                let remove = component(ui, "script", tr!("📜 Cryo Script", "📜 Script Cryo"), |ui| {
                    row(ui, tr!("File", "Arquivo"), |ui| {
                        let none = tr!("(none)", "(nenhum)");
                        let shown = if s.path.is_empty() { none.as_str() } else { s.path.as_str() };
                        egui::ComboBox::from_id_salt("script_path")
                            .selected_text(shown.rsplit('/').next().unwrap_or(shown).to_string())
                            .show_ui(ui, |ui| {
                                for f in &scripts {
                                    if ui.selectable_label(&s.path == f, f).clicked() && &s.path != f {
                                        s.path = f.clone();
                                        s.props.clear();
                                    }
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        if !s.path.is_empty() && ui.button(tr!("✏ Edit", "✏ Editar")).clicked() {
                            open_script = Some(s.path.clone());
                        }
                        if ui.button(tr!("+ New script", "+ Novo script")).clicked() {
                            new_script = true;
                        }
                    });
                    match &props {
                        Some(Ok(list)) => {
                            if !list.is_empty() {
                                ui.separator();
                            }
                            for (name, _ty, default) in list {
                                let Some(default) = value_to_prop(default) else { continue };
                                let mut cur = match s.props.get(name) {
                                    // if the prop type changed in the script, fall back to the default
                                    Some(v) if std::mem::discriminant(v) == std::mem::discriminant(&default) => v.clone(),
                                    _ => default.clone(),
                                };
                                let overridden = cur != default;
                                let changed = match &mut cur {
                                    PropValue::Vec3(v) => {
                                        let mut g = Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32);
                                        let c = vec3_row(ui, name, &mut g, 0.05);
                                        *v = [g.x as f64, g.y as f64, g.z as f64];
                                        c
                                    }
                                    other => row(ui, name, |ui| match other {
                                        PropValue::Num(n) => ui.add(DragValue::new(n).speed(0.05).max_decimals(3)).changed(),
                                        PropValue::Bool(b) => ui.checkbox(b, "").changed(),
                                        PropValue::Text(t) => ui.text_edit_singleline(t).changed(),
                                        PropValue::Vec3(_) => false,
                                    }),
                                };
                                if changed {
                                    if cur == default {
                                        s.props.remove(name);
                                    } else {
                                        s.props.insert(name.clone(), cur);
                                    }
                                } else if overridden && !list.is_empty() {
                                    // keeps the custom value
                                }
                            }
                        }
                        Some(Err(err)) => {
                            ui.colored_label(Color32::from_rgb(240, 110, 110), format!("⚠ {err}"));
                        }
                        None => {
                            ui.label(RichText::new(tr!("Choose a .cryo file", "Escolha um arquivo .cryo")).weak());
                        }
                    }
                });
                if remove {
                    e.script = None;
                }
            }

            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.menu_button(tr!("+ Add component", "+ Adicionar componente"), |ui| {
                    if e.mesh.is_none() && ui.button(tr!("■ Mesh", "■ Malha")).clicked() {
                        e.mesh = Some(MeshRenderer::primitive(Shape::Cube));
                        ui.close();
                    }
                    if e.physics.is_none() && ui.button(tr!("⚛ Physics", "⚛ Física")).clicked() {
                        let col = match e.mesh.as_ref().map(|m| m.shape) {
                            Some(Shape::Sphere) => ColliderShape::Sphere,
                            _ => ColliderShape::Box,
                        };
                        e.physics = Some(Physics::new(BodyKind::Dynamic, col));
                        ui.close();
                    }
                    if e.script.is_none() && ui.button(tr!("📜 Cryo Script", "📜 Script Cryo")).clicked() {
                        e.script = Some(ScriptRef::default());
                        ui.close();
                    }
                    if e.light.is_none() && ui.button(tr!("☀ Light", "☀ Luz")).clicked() {
                        e.light = Some(Light {
                            kind: LightKind::Point,
                            color: [1.0; 3],
                            intensity: 1.5,
                            range: 10.0,
                            angle: 35.0,
                            shadows: true,
                        });
                        ui.close();
                    }
                    if e.camera.is_none() && ui.button(tr!("🎥 Camera", "🎥 Câmera")).clicked() {
                        e.camera = Some(Camera { fov: 60.0, near: 0.1, far: 500.0 });
                        ui.close();
                    }
                });
            });

            if new_script {
                let base = self.scene.get(id).map(|e| e.name.clone()).unwrap_or_default();
                match project::create_script(&self.project_dir, &base) {
                    Ok(rel) => {
                        if let Some(s) = self.scene.get_mut(id).and_then(|e| e.script.as_mut()) {
                            s.path = rel.clone();
                            s.props.clear();
                        }
                        open_script = Some(rel);
                    }
                    Err(e) => self.error(tr!("Could not create the script: {e}", "Não foi possível criar o script: {e}")),
                }
            }
            if let Some(rel) = open_script {
                self.open_script(&rel);
            }
        });
    }

    fn scene_settings(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(tr!("Scene settings", "Configurações da cena")).strong());
        ui.label(RichText::new(tr!("Select an object to see its components.", "Selecione um objeto para ver os componentes.")).weak().small());
        ui.add_space(6.0);
        row(ui, tr!("Ambient light", "Luz ambiente"), |ui| color_edit(ui, &mut self.scene.ambient));
        row(ui, tr!("Sky (top)", "Céu (topo)"), |ui| color_edit(ui, &mut self.scene.sky_top));
        row(ui, tr!("Sky (horizon)", "Céu (horizonte)"), |ui| color_edit(ui, &mut self.scene.sky_horizon));
        row(ui, tr!("Gravity", "Gravidade"), |ui| ui.add(DragValue::new(&mut self.scene.gravity).speed(0.05).suffix(" m/s²")));
    }

    // ------------------------------------------------------------------
    // Content browser
    // ------------------------------------------------------------------
    pub fn content_browser(&mut self, ui: &mut egui::Ui) {
        let mut go: Option<String> = None;
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.browser_dir.is_empty(), egui::Button::new("⬆")).clicked() {
                go = Some(match self.browser_dir.rsplit_once('/') {
                    Some((parent, _)) => parent.to_string(),
                    None => String::new(),
                });
            }
            if ui.button(tr!("⬇ Import", "⬇ Importar")).on_hover_text(tr!("Models (.obj, .gltf, .glb), textures (.png, .jpg) and sounds (.wav, .ogg, .mp3).\nYou can also drag files onto the window.", "Modelos (.obj, .gltf, .glb), texturas (.png, .jpg) e sons (.wav, .ogg, .mp3).\nTambém dá para arrastar os arquivos para a janela.")).clicked() {
                self.import_dialog();
            }
            if ui.button("+ Script").clicked() {
                match project::create_script(&self.project_dir, "NewScript") {
                    Ok(rel) => {
                        self.browser_dir = "scripts".into();
                        self.open_script(&rel);
                    }
                    Err(e) => self.error(tr!("Could not create the script: {e}", "Não foi possível criar o script: {e}")),
                }
            }
            ui.separator();
            if ui.link(tr!("Project", "Projeto")).clicked() {
                go = Some(String::new());
            }
            let mut acc = String::new();
            for part in self.browser_dir.clone().split('/').filter(|p| !p.is_empty()) {
                acc = if acc.is_empty() { part.to_string() } else { format!("{acc}/{part}") };
                ui.label("›");
                if ui.link(part).clicked() {
                    go = Some(acc.clone());
                }
            }
        });
        ui.separator();

        let entries = project::list_dir(&self.project_dir, &self.browser_dir);
        let mut open: Option<(String, bool)> = None;
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if entries.is_empty() {
                ui.label(RichText::new(tr!("Empty folder", "Pasta vazia")).weak());
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                for entry in &entries {
                    let ext = entry.name.rsplit('.').next().unwrap_or("");
                    let (icon, color) = if entry.is_dir {
                        ("📁", Color32::from_rgb(230, 190, 90))
                    } else if ext == SCRIPT_EXT {
                        ("📜", Color32::from_rgb(120, 200, 255))
                    } else if ext == SCENE_EXT {
                        ("🎬", Color32::from_rgb(160, 220, 140))
                    } else if crate::assets::MODEL_EXTS.contains(&ext) {
                        ("◆", Color32::from_rgb(120, 230, 240))
                    } else if crate::assets::IMAGE_EXTS.contains(&ext) {
                        ("🖼", Color32::from_rgb(230, 150, 220))
                    } else if crate::assets::SOUND_EXTS.contains(&ext) {
                        ("♫", Color32::from_rgb(250, 190, 110))
                    } else {
                        ("📄", Color32::GRAY)
                    };
                    let current = self.editor.path.as_deref() == Some(entry.rel.as_str())
                        || (self.scene_path == entry.rel);
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(92.0, 78.0), Sense::click());
                    let bg = if resp.hovered() {
                        Color32::from_rgb(62, 62, 68)
                    } else {
                        Color32::from_rgb(46, 46, 50)
                    };
                    ui.painter().rect_filled(rect, 5.0, bg);
                    if current {
                        ui.painter().rect_stroke(rect, 5.0, egui::Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside);
                    }
                    ui.painter().text(
                        rect.center_top() + egui::vec2(0.0, 8.0),
                        egui::Align2::CENTER_TOP,
                        icon,
                        egui::FontId::proportional(30.0),
                        color,
                    );
                    let mut name = entry.name.clone();
                    if name.chars().count() > 14 {
                        name = name.chars().take(13).collect::<String>() + "…";
                    }
                    ui.painter().text(
                        rect.center_bottom() + egui::vec2(0.0, -7.0),
                        egui::Align2::CENTER_BOTTOM,
                        name,
                        egui::FontId::proportional(11.5),
                        Color32::from_gray(215),
                    );
                    let tip = if crate::assets::MODEL_EXTS.contains(&ext) {
                        tr!("{}\nDouble-click: add to the scene", "{}\nDuplo clique: adicionar à cena", entry.rel)
                    } else if crate::assets::IMAGE_EXTS.contains(&ext) {
                        tr!("{}\nDouble-click: apply as a texture to the selected object", "{}\nDuplo clique: aplicar como textura no objeto selecionado", entry.rel)
                    } else {
                        entry.rel.clone()
                    };
                    let resp = resp.on_hover_text(tip);
                    if resp.double_clicked() || (resp.clicked() && entry.is_dir) {
                        open = Some((entry.rel.clone(), entry.is_dir));
                    }
                }
            });
        });

        if let Some((rel, is_dir)) = open {
            if is_dir {
                go = Some(rel);
            } else if rel.ends_with(&format!(".{SCRIPT_EXT}")) {
                self.open_script(&rel);
            } else if rel.ends_with(&format!(".{SCENE_EXT}")) {
                if self.playing() {
                    self.info(tr!("Stop the game before opening another scene.", "Pare o jogo antes de abrir outra cena."));
                } else {
                    self.load_scene(&rel);
                }
            } else {
                let ext = crate::assets::ext_of(&rel);
                if crate::assets::MODEL_EXTS.contains(&ext.as_str()) {
                    // double-click on a model: place it in the scene
                    if self.playing() {
                        self.info(tr!("Stop the game before adding objects to the scene.", "Pare o jogo antes de adicionar objetos à cena."));
                    } else {
                        self.add_model_to_scene(&rel);
                    }
                } else if crate::assets::IMAGE_EXTS.contains(&ext.as_str()) {
                    // double-click on an image: apply it as a texture to the selected object
                    let target = self.selected.and_then(|id| self.scene.get_mut(id)).and_then(|e| e.mesh.as_mut());
                    match target {
                        Some(m) => m.texture = Some(rel.clone()),
                        None => self.info(tr!("Select an object with a mesh to apply the texture.", "Selecione um objeto com malha para aplicar a textura.")),
                    }
                }
            }
        }
        if let Some(dir) = go {
            self.browser_dir = dir;
        }
    }

    // ------------------------------------------------------------------
    // Console
    // ------------------------------------------------------------------
    pub fn console(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(tr!("🗑 Clear", "🗑 Limpar")).clicked() {
                self.logs.clear();
            }
            let errors = self.logs.iter().filter(|l| l.level == LogLevel::Error).count();
            let warns = self.logs.iter().filter(|l| l.level == LogLevel::Warn).count();
            ui.label(RichText::new(tr!("{} messages", "{} mensagens", self.logs.len())).weak());
            if warns > 0 {
                ui.colored_label(Color32::from_rgb(240, 200, 90), format!("⚠ {warns}"));
            }
            if errors > 0 {
                ui.colored_label(Color32::from_rgb(240, 110, 110), format!("⛔ {errors}"));
            }
        });
        ui.separator();
        ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            for line in &self.logs {
                let (color, prefix) = match line.level {
                    LogLevel::Info => (Color32::from_gray(205), "  "),
                    LogLevel::Warn => (Color32::from_rgb(240, 200, 90), "⚠ "),
                    LogLevel::Error => (Color32::from_rgb(240, 110, 110), "⛔ "),
                };
                ui.label(RichText::new(format!("{prefix}{}", line.text)).monospace().color(color));
            }
        });
    }

    // ------------------------------------------------------------------
    // Script editor
    // ------------------------------------------------------------------
    pub fn script_editor(&mut self, ui: &mut egui::Ui) {
        let Some(path) = self.editor.path.clone() else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(tr!("Open a .cryo file from the Content tab (double-click)", "Abra um arquivo .cryo na aba Conteúdo (duplo clique)")).weak());
            });
            return;
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(&path).strong());
            if ui.add_enabled(self.editor.dirty(), egui::Button::new(tr!("💾 Save", "💾 Salvar"))).on_hover_text("Ctrl+S").clicked() {
                self.save_script();
            }
            match &self.editor.error {
                None => ui.colored_label(Color32::from_rgb(110, 210, 130), tr!("✔ no errors", "✔ sem erros")),
                Some(e) => ui.colored_label(Color32::from_rgb(240, 110, 110), format!("⛔ {e}")),
            };
        });
        ui.separator();

        let font = egui::FontId::monospace(13.5);
        let error_line = self.editor.error.as_ref().map(|e| e.line);
        ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                // line numbers
                let lines = self.editor.text.matches('\n').count() + 1;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add_space(2.0);
                    let mut job = egui::text::LayoutJob::default();
                    for n in 1..=lines {
                        let color = if Some(n) == error_line {
                            Color32::from_rgb(240, 110, 110)
                        } else {
                            Color32::from_gray(110)
                        };
                        job.append(
                            &format!("{n:>4}\n"),
                            0.0,
                            egui::TextFormat::simple(font.clone(), color),
                        );
                    }
                    ui.label(job);
                });

                let f = font.clone();
                let mut layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
                    let mut job = highlight(buf.as_str(), f.clone());
                    job.wrap.max_width = f32::INFINITY;
                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                };
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut self.editor.text)
                        .code_editor()
                        .font(font.clone())
                        .desired_width(f32::INFINITY)
                        .desired_rows(24)
                        .frame(egui::Frame::NONE)
                        .layouter(&mut layouter),
                );
                if resp.changed() {
                    self.editor.error = cryo_lang::parse(&self.editor.text).err();
                }
            });
        });
    }
}
