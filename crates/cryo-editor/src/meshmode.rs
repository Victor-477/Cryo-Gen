//! Edit Mesh mode: the editor side of in-engine modelling (selection, gizmo, tools, saving).

use crate::app::State;
use crate::assets;
use crate::mesh_edit::EditMesh;
use crate::project;
use crate::render::RenderFrame;
use crate::viewport::Tool;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Stroke};
use glam::{Mat4, Quat, Vec3};
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
pub enum SelectMode {
    Vertex,
    Face,
}

/// One editing session on one entity's model.
pub struct MeshSession {
    pub entity: u64,
    /// Model file being edited, relative to the project.
    pub path: String,
    pub mesh: EditMesh,
    original: EditMesh,
    pub mode: SelectMode,
    pub selection: BTreeSet<usize>,
    undo: Vec<(EditMesh, BTreeSet<usize>, SelectMode)>,
}

impl MeshSession {
    /// Vertices affected by the gizmo: the selection itself, or the corners of the selected faces.
    fn active_vertices(&self) -> BTreeSet<usize> {
        match self.mode {
            SelectMode::Vertex => self.selection.iter().copied().filter(|v| *v < self.mesh.positions.len()).collect(),
            SelectMode::Face => self.mesh.face_vertices(&self.selection),
        }
    }

    fn centroid(&self) -> Option<Vec3> {
        let verts = self.active_vertices();
        if verts.is_empty() {
            return None;
        }
        Some(verts.iter().map(|v| self.mesh.positions[*v]).sum::<Vec3>() / verts.len() as f32)
    }

    fn checkpoint(&mut self) {
        self.undo.push((self.mesh.clone(), self.selection.clone(), self.mode));
        if self.undo.len() > 60 {
            self.undo.remove(0);
        }
    }
}

impl State {
    /// Starts editing the selected entity's mesh. A primitive is first turned into a model file.
    pub fn start_mesh_edit(&mut self, entity: u64) {
        if self.playing() {
            self.info(tr!("Stop the game before editing a mesh.", "Pare o jogo antes de editar uma malha."));
            return;
        }
        self.finish_mesh_edit(true);
        let Some(e) = self.scene.get(entity) else { return };
        let Some(m) = e.mesh.clone() else { return };
        let (path, mesh) = match &m.model {
            Some(path) => {
                if assets::ext_of(path) != "obj" {
                    self.error(tr!(
                        "Only .obj models can be edited in the engine ({} is not).",
                        "Só modelos .obj podem ser editados na engine ({} não é).",
                        path
                    ));
                    return;
                }
                match EditMesh::load(&self.project_dir.join(path)) {
                    Ok(mesh) => (path.clone(), mesh),
                    Err(err) => {
                        self.error(format!("{path}: {err}"));
                        return;
                    }
                }
            }
            None => {
                // primitive: write it out as an editable model named after the entity
                let mesh = EditMesh::from_shape(m.shape);
                let base = project::script_ident(&e.name);
                let mut name = base.clone();
                let mut i = 2;
                while self.project_dir.join(format!("models/{name}.obj")).exists() {
                    name = format!("{base}{i}");
                    i += 1;
                }
                let path = format!("models/{name}.obj");
                if let Err(err) = mesh.save(&self.project_dir.join(&path)) {
                    self.error(err);
                    return;
                }
                if let Some(mr) = self.scene.get_mut(entity).and_then(|e| e.mesh.as_mut()) {
                    mr.model = Some(path.clone());
                    mr.model_bounds = None;
                }
                (path, mesh)
            }
        };
        self.selected = Some(entity);
        self.mesh_edit = Some(MeshSession {
            entity,
            path,
            original: mesh.clone(),
            mesh,
            mode: SelectMode::Face,
            selection: BTreeSet::new(),
            undo: vec![],
        });
        self.refresh_mesh_preview();
        self.focus_tab = Some(crate::app::Tab::Scene);
    }

    /// Shows the mesh as it is being edited (without touching the file on disk).
    fn refresh_mesh_preview(&mut self) {
        if let Some(s) = &self.mesh_edit {
            let model = s.mesh.to_model(assets::next_id());
            self.assets.overrides.insert(s.path.clone(), Arc::new(model));
        }
    }

    /// Leaves Edit Mesh mode, saving the model (`save`) or going back to how it was.
    pub fn finish_mesh_edit(&mut self, save: bool) {
        let Some(s) = self.mesh_edit.take() else { return };
        self.assets.overrides.remove(&s.path);
        if save {
            if s.mesh.faces.is_empty() {
                self.error(tr!("The mesh has no faces, so it was not saved.", "A malha não tem faces, então não foi salva."));
            } else {
                match s.mesh.save(&self.project_dir.join(&s.path)) {
                    Ok(()) => self.info(tr!("Model saved: {}", "Modelo salvo: {}", s.path)),
                    Err(e) => self.error(e),
                }
            }
        } else {
            // the file may have been created by this session (from a primitive): keep it valid
            s.original.save(&self.project_dir.join(&s.path)).ok();
        }
        self.assets.invalidate(&s.path);
        self.drag = None;
    }

    pub fn mesh_undo(&mut self) {
        if let Some(s) = &mut self.mesh_edit {
            if let Some((mesh, selection, mode)) = s.undo.pop() {
                s.mesh = mesh;
                s.selection = selection;
                s.mode = mode;
            }
        }
        self.refresh_mesh_preview();
    }

    /// Runs one modelling tool on the current selection.
    pub fn mesh_op(&mut self, op: &str) {
        let Some(s) = &mut self.mesh_edit else { return };
        let needs_faces = matches!(op, "extrude" | "inset" | "subdivide" | "flip");
        if needs_faces && (s.mode != SelectMode::Face || s.selection.is_empty()) {
            self.info(tr!("Select one or more faces first.", "Selecione uma ou mais faces primeiro."));
            return;
        }
        if matches!(op, "fill" | "merge") && s.mode != SelectMode::Vertex {
            self.info(tr!("Switch to Vertices and select the vertices first.", "Mude para Vértices e selecione os vértices primeiro."));
            return;
        }
        s.checkpoint();
        let sel = s.selection.clone();
        match op {
            "extrude" => s.selection = s.mesh.extrude(&sel, 0.3),
            "inset" => s.selection = s.mesh.inset(&sel, 0.25),
            "subdivide" => s.selection = s.mesh.subdivide(&sel),
            "flip" => s.mesh.flip(&sel),
            "delete" => {
                match s.mode {
                    SelectMode::Face => s.mesh.delete_faces(&sel),
                    SelectMode::Vertex => s.mesh.delete_vertices(&sel),
                }
                s.selection.clear();
            }
            "fill" => {
                if s.mesh.fill(&sel).is_none() {
                    s.undo.pop();
                    self.info(tr!("Select at least three vertices to fill.", "Selecione pelo menos três vértices para preencher."));
                    return;
                }
            }
            "merge" => match s.mesh.merge(&sel) {
                Some(v) => s.selection = [v].into_iter().collect(),
                None => {
                    s.undo.pop();
                    self.info(tr!("Select at least two vertices to merge.", "Selecione pelo menos dois vértices para unir."));
                    return;
                }
            },
            "all" => {
                s.undo.pop();
                let n = if s.mode == SelectMode::Face { s.mesh.faces.len() } else { s.mesh.positions.len() };
                s.selection = if s.selection.len() == n { BTreeSet::new() } else { (0..n).collect() };
            }
            "smooth" => s.mesh.smooth = !s.mesh.smooth,
            _ => {
                s.undo.pop();
            }
        }
        self.refresh_mesh_preview();
    }

    /// Small scripted edit used by automated screenshots: extrude and inset the top face.
    pub fn mesh_demo(&mut self) {
        if let Some(s) = &mut self.mesh_edit {
            let top = (0..s.mesh.faces.len()).find(|f| s.mesh.face_normal(*f).y > 0.9);
            s.selection = top.into_iter().collect();
        }
        self.mesh_op("extrude");
        self.mesh_op("inset");
        self.mesh_op("extrude");
    }

    /// World-space position of the gizmo while editing (centre of the selection).
    pub fn mesh_gizmo_origin(&self) -> Option<Vec3> {
        let s = self.mesh_edit.as_ref()?;
        let c = s.centroid()?;
        Some(self.scene.world_matrix(s.entity).transform_point3(c))
    }

    /// The gizmo is about to be dragged: remember the mesh for undo.
    pub fn mesh_drag_start(&mut self) {
        if let Some(s) = &mut self.mesh_edit {
            s.checkpoint();
        }
    }

    /// Applies a gizmo drag to the selected vertices. `amount` is in world units along `axis`.
    pub fn mesh_drag(&mut self, axis: Vec3, amount: f32, size: f32) {
        let tool = self.tool;
        let Some(s) = &mut self.mesh_edit else { return };
        let world = self.scene.world_matrix(s.entity);
        if world.determinant().abs() < 1e-9 {
            return;
        }
        let inv = world.inverse();
        let verts = s.active_vertices();
        let Some(center) = s.centroid() else { return };
        let local_axis = inv.transform_vector3(axis).normalize_or_zero();
        match tool {
            Tool::Move => {
                let d = inv.transform_vector3(axis * amount);
                for v in verts {
                    s.mesh.positions[v] += d;
                }
            }
            Tool::Scale => {
                let factor = (1.0 + amount / size.max(1e-4)).max(0.05);
                for v in verts {
                    let rel = s.mesh.positions[v] - center;
                    s.mesh.positions[v] += local_axis * rel.dot(local_axis) * (factor - 1.0);
                }
            }
            Tool::Rotate => {
                let q = Quat::from_axis_angle(local_axis, (amount / size.max(1e-4) * 90.0).to_radians());
                for v in verts {
                    s.mesh.positions[v] = center + q * (s.mesh.positions[v] - center);
                }
            }
        }
        self.refresh_mesh_preview();
    }

    /// A click in the Scene while editing: picks a vertex or a face.
    pub fn mesh_click(&mut self, origin: Vec3, dir: Vec3, pointer: Pos2, vp: Mat4, rect: Rect, add: bool) {
        let Some(s) = &mut self.mesh_edit else { return };
        let world = self.scene.world_matrix(s.entity);
        let hit = match s.mode {
            SelectMode::Vertex => {
                let mut best: Option<(f32, usize)> = None;
                for (i, p) in s.mesh.positions.iter().enumerate() {
                    if let Some(sp) = crate::viewport::project(vp, rect, world.transform_point3(*p)) {
                        let d = (sp - pointer).length();
                        if d < 12.0 && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                            best = Some((d, i));
                        }
                    }
                }
                best.map(|(_, i)| i)
            }
            SelectMode::Face => {
                if world.determinant().abs() < 1e-9 {
                    None
                } else {
                    let inv = world.inverse();
                    s.mesh.pick_face(inv.transform_point3(origin), inv.transform_vector3(dir)).map(|(f, _)| f)
                }
            }
        };
        match (hit, add) {
            (Some(i), true) => {
                if !s.selection.remove(&i) {
                    s.selection.insert(i);
                }
            }
            (Some(i), false) => s.selection = [i].into_iter().collect(),
            (None, false) => s.selection.clear(),
            (None, true) => {}
        }
    }

    /// Wireframe of the mesh being edited (depth-tested lines).
    pub fn mesh_edit_lines(&self, frame: &mut RenderFrame) {
        let Some(s) = &self.mesh_edit else { return };
        let world = self.scene.world_matrix(s.entity);
        for (a, b) in s.mesh.edges() {
            frame.line(
                world.transform_point3(s.mesh.positions[a as usize]),
                world.transform_point3(s.mesh.positions[b as usize]),
                [0.05, 0.05, 0.08, 0.9],
            );
        }
    }

    /// Selection highlight and vertex dots, drawn over the 3D view.
    pub fn mesh_edit_overlay(&self, painter: &egui::Painter, vp: Mat4, rect: Rect) {
        let Some(s) = &self.mesh_edit else { return };
        let world = self.scene.world_matrix(s.entity);
        let to_screen = |p: Vec3| crate::viewport::project(vp, rect, world.transform_point3(p));
        let orange = Color32::from_rgb(255, 150, 20);
        if s.mode == SelectMode::Face {
            for f in &s.selection {
                let Some(face) = s.mesh.faces.get(*f) else { continue };
                let pts: Vec<Pos2> = face.iter().filter_map(|v| to_screen(s.mesh.positions[*v as usize])).collect();
                if pts.len() == face.len() && pts.len() >= 3 {
                    painter.add(egui::Shape::convex_polygon(
                        pts.clone(),
                        Color32::from_rgba_unmultiplied(255, 150, 20, 70),
                        Stroke::new(2.0, orange),
                    ));
                }
            }
        }
        if s.mode == SelectMode::Vertex {
            for (i, p) in s.mesh.positions.iter().enumerate() {
                if let Some(sp) = to_screen(*p) {
                    let sel = s.selection.contains(&i);
                    painter.circle_filled(sp, if sel { 5.0 } else { 3.5 }, if sel { orange } else { Color32::from_gray(20) });
                    painter.circle_stroke(sp, if sel { 5.0 } else { 3.5 }, Stroke::new(1.0, Color32::WHITE));
                }
            }
        }
    }

    /// Floating toolbar of Edit Mesh mode.
    pub fn mesh_edit_toolbar(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let Some(s) = &self.mesh_edit else { return };
        let (mode, smooth, can_undo) = (s.mode, s.mesh.smooth, !s.undo.is_empty());
        let stats = tr!(
            "{} vertices · {} faces · {} selected",
            "{} vértices · {} faces · {} selecionados",
            s.mesh.positions.len(),
            s.mesh.faces.len(),
            s.selection.len()
        );
        let bar = Rect::from_min_size(
            rect.left_top() + egui::vec2(8.0, 40.0),
            egui::vec2((rect.width() - 130.0).max(200.0), 64.0),
        );
        let mut op: Option<&'static str> = None;
        let mut set_mode: Option<SelectMode> = None;
        let mut finish: Option<bool> = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::top_down(egui::Align::Min)));
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(20, 20, 24, 225))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::same(6))
            .show(&mut child, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(tr!("EDIT MESH", "EDITAR MALHA")).strong().color(Color32::from_rgb(255, 170, 60)));
                    if ui.selectable_label(mode == SelectMode::Face, tr!("Faces", "Faces")).clicked() {
                        set_mode = Some(SelectMode::Face);
                    }
                    if ui.selectable_label(mode == SelectMode::Vertex, tr!("Vertices", "Vértices")).clicked() {
                        set_mode = Some(SelectMode::Vertex);
                    }
                    ui.separator();
                    for (id, label, tip) in [
                        ("extrude", tr!("Extrude", "Extrudar"), tr!("Pull the selected faces outwards (X)", "Puxa as faces selecionadas para fora (X)")),
                        ("inset", tr!("Inset", "Inserir"), tr!("A smaller face inside each selected face (I)", "Uma face menor dentro de cada face selecionada (I)")),
                        ("subdivide", tr!("Subdivide", "Subdividir"), tr!("Split the selected faces into quads (U)", "Divide as faces selecionadas em quadriláteros (U)")),
                        ("flip", tr!("Flip", "Inverter"), tr!("Turn the selected faces inside out (N)", "Vira as faces selecionadas do avesso (N)")),
                        ("fill", tr!("Fill", "Preencher"), tr!("New face from the selected vertices", "Nova face a partir dos vértices selecionados")),
                        ("merge", tr!("Merge", "Unir"), tr!("Join the selected vertices at their centre (M)", "Une os vértices selecionados no centro (M)")),
                        ("delete", tr!("Delete", "Excluir"), tr!("Remove the selection (Del)", "Remove a seleção (Del)")),
                        ("all", tr!("All", "Tudo"), tr!("Select everything, or nothing (A)", "Seleciona tudo, ou nada (A)")),
                    ] {
                        if ui.button(label).on_hover_text(tip).clicked() {
                            op = Some(id);
                        }
                    }
                    let mut sm = smooth;
                    if ui.checkbox(&mut sm, tr!("Smooth", "Suave")).on_hover_text(tr!("Smooth shading instead of flat faces", "Sombreamento suave em vez de faces planas")).changed() {
                        op = Some("smooth");
                    }
                    ui.separator();
                    if ui.add_enabled(can_undo, egui::Button::new(tr!("Undo", "Desfazer"))).on_hover_text("Ctrl+Z").clicked() {
                        op = Some("undo");
                    }
                    if ui.button(RichText::new(tr!("✔ Done", "✔ Concluir")).color(Color32::from_rgb(110, 210, 130))).clicked() {
                        finish = Some(true);
                    }
                    if ui.button(tr!("✖ Cancel", "✖ Cancelar")).clicked() {
                        finish = Some(false);
                    }
                });
                ui.label(RichText::new(stats).small().weak());
            });
        if let Some(m) = set_mode {
            if let Some(s) = &mut self.mesh_edit {
                if s.mode != m {
                    // carry the selection over: faces -> their vertices; vertices -> nothing
                    s.selection = if m == SelectMode::Vertex { s.mesh.face_vertices(&s.selection) } else { BTreeSet::new() };
                    s.mode = m;
                }
            }
        }
        match op {
            Some("undo") => self.mesh_undo(),
            Some(o) => self.mesh_op(o),
            None => {}
        }
        if let Some(save) = finish {
            self.finish_mesh_edit(save);
        }
    }

    /// Keyboard shortcuts of Edit Mesh mode. Returns `true` when the key was used.
    pub fn mesh_edit_keys(&mut self, ctx: &egui::Context) -> bool {
        if self.mesh_edit.is_none() || ctx.egui_wants_keyboard_input() {
            return false;
        }
        use egui::Key;
        let cmd = ctx.input(|i| i.modifiers.command);
        let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));
        if cmd && pressed(Key::Z) {
            self.mesh_undo();
            return true;
        }
        if cmd {
            return false;
        }
        for (key, op) in [
            (Key::X, "extrude"),
            (Key::I, "inset"),
            (Key::U, "subdivide"),
            (Key::N, "flip"),
            (Key::M, "merge"),
            (Key::A, "all"),
            (Key::Delete, "delete"),
            (Key::Backspace, "delete"),
        ] {
            if pressed(key) {
                self.mesh_op(op);
                return true;
            }
        }
        if pressed(Key::Escape) {
            self.finish_mesh_edit(true);
            return true;
        }
        false
    }
}
