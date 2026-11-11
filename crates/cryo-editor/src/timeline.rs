//! Timeline panel: create animation clips, set keyframes, place timed events and preview them.

use crate::anim::{read_property, Action, Clip, Ease, Marker, Property};
use crate::app::{State, ACCENT};
use crate::assets::SOUND_EXTS;
use crate::project;
use eframe::egui::{self, Align2, Color32, DragValue, FontId, Pos2, Rect, RichText, Sense, Stroke};

#[derive(Default)]
pub struct TimelineState {
    /// Index of the clip being edited.
    pub clip: usize,
    /// Playhead, in seconds.
    pub time: f32,
    /// Previewing in the editor (outside Play mode).
    pub previewing: bool,
    pub selected_key: Option<(usize, usize)>,
    pub selected_marker: Option<usize>,
}

pub fn property_name(p: Property) -> String {
    match p {
        Property::Position => tr!("Position", "Posição"),
        Property::Rotation => tr!("Rotation", "Rotação"),
        Property::Scale => tr!("Scale", "Escala"),
        Property::Color => tr!("Colour", "Cor"),
        Property::Fov => tr!("Field of view", "Campo de visão"),
    }
}

fn ease_name(e: Ease) -> String {
    match e {
        Ease::Linear => tr!("Linear", "Linear"),
        Ease::Smooth => tr!("Smooth", "Suave"),
        Ease::Step => tr!("Step (hold)", "Degrau (segura)"),
        Ease::EaseIn => tr!("Ease in (accelerate)", "Acelerar"),
        Ease::EaseOut => tr!("Ease out (brake)", "Frear"),
        Ease::Overshoot => tr!("Overshoot", "Passar e voltar"),
        Ease::Bounce => tr!("Bounce", "Quicar"),
    }
}

fn action_name(a: &Action) -> String {
    match a {
        Action::Send { .. } => tr!("Script event", "Evento de script"),
        Action::Sound { .. } => tr!("Sound", "Som"),
        Action::Camera { .. } => tr!("Camera cut", "Corte de câmera"),
        Action::Hud { .. } => tr!("Screen text", "Texto na tela"),
        Action::Broadcast { .. } => tr!("Event for everyone", "Evento para todos"),
        Action::Qte { .. } => tr!("Quick time event", "Quick time event"),
        Action::Play { .. } => tr!("Play animation", "Tocar animação"),
        Action::TimeScale { .. } => tr!("Game speed", "Velocidade do jogo"),
    }
}

fn action_color(a: &Action) -> Color32 {
    match a {
        Action::Send { .. } => Color32::from_rgb(120, 200, 255),
        Action::Sound { .. } => Color32::from_rgb(250, 190, 110),
        Action::Camera { .. } => Color32::from_rgb(190, 150, 255),
        Action::Hud { .. } => Color32::from_rgb(150, 230, 150),
        Action::Broadcast { .. } => Color32::from_rgb(90, 220, 220),
        Action::Qte { .. } => Color32::from_rgb(255, 120, 130),
        Action::Play { .. } => Color32::from_rgb(240, 230, 120),
        Action::TimeScale { .. } => Color32::from_rgb(200, 200, 210),
    }
}

fn diamond(painter: &egui::Painter, c: Pos2, r: f32, fill: Color32, selected: bool) {
    let pts = vec![c + egui::vec2(0.0, -r), c + egui::vec2(r, 0.0), c + egui::vec2(0.0, r), c + egui::vec2(-r, 0.0)];
    let stroke = if selected { Stroke::new(2.0, Color32::WHITE) } else { Stroke::new(1.0, Color32::from_black_alpha(160)) };
    painter.add(egui::Shape::convex_polygon(pts, fill, stroke));
}

impl State {
    fn entity_label(&self, id: u64) -> String {
        self.scene.get(id).map(|e| e.name.clone()).unwrap_or_else(|| tr!("(deleted)", "(excluído)"))
    }

    /// Combo box that picks an entity. `only_cameras` restricts the list to cameras.
    fn entity_combo(&self, ui: &mut egui::Ui, salt: &str, current: &mut u64, only_cameras: bool) {
        egui::ComboBox::from_id_salt(salt).selected_text(self.entity_label(*current)).width(150.0).show_ui(ui, |ui| {
            for e in &self.scene.entities {
                if only_cameras && e.camera.is_none() {
                    continue;
                }
                if !only_cameras && e.script.is_none() {
                    continue;
                }
                ui.selectable_value(current, e.id, format!("{}  #{}", e.name, e.id));
            }
        });
    }

    /// Moves the playhead; outside Play mode this also poses the scene at that instant.
    fn timeline_seek(&mut self, t: f32) {
        let Some(clip) = self.scene.animations.get(self.timeline.clip).cloned() else { return };
        self.timeline.time = t.clamp(0.0, clip.duration.max(0.01));
        if !self.playing() {
            clip.apply(&mut self.scene, self.timeline.time);
        }
    }

    /// Advances the editor preview (called every frame).
    pub fn timeline_tick(&mut self, dt: f32) {
        if self.playing() {
            self.timeline.previewing = false;
            // follow the clip while the game plays it
            if let Some(play) = &self.play {
                if let Some(p) = play.rt.playing.iter().find(|p| p.clip == self.timeline.clip) {
                    self.timeline.time = p.t;
                }
            }
            return;
        }
        if !self.timeline.previewing {
            return;
        }
        let Some(clip) = self.scene.animations.get(self.timeline.clip) else {
            self.timeline.previewing = false;
            return;
        };
        let (duration, looping) = (clip.duration.max(0.01), clip.looping);
        let mut t = self.timeline.time + dt;
        if t >= duration {
            if looping {
                t %= duration;
            } else {
                t = duration;
                self.timeline.previewing = false;
            }
        }
        self.timeline_seek(t);
    }

    pub fn timeline_panel(&mut self, ui: &mut egui::Ui) {
        if self.timeline.clip >= self.scene.animations.len() {
            self.timeline.clip = self.scene.animations.len().saturating_sub(1);
        }
        let playing_game = self.playing();

        // ------------------------------------------------------------ clip bar
        let mut new_clip = false;
        let mut delete_clip = false;
        ui.horizontal_wrapped(|ui| {
            let current = self.scene.animations.get(self.timeline.clip).map(|c| c.name.clone());
            egui::ComboBox::from_id_salt("clip")
                .selected_text(current.clone().unwrap_or_else(|| tr!("(no animation)", "(nenhuma animação)")))
                .show_ui(ui, |ui| {
                    for (i, c) in self.scene.animations.iter().enumerate() {
                        if ui.selectable_label(i == self.timeline.clip, &c.name).clicked() {
                            self.timeline.clip = i;
                            self.timeline.time = 0.0;
                            self.timeline.selected_key = None;
                            self.timeline.selected_marker = None;
                        }
                    }
                });
            if ui.add_enabled(!playing_game, egui::Button::new(tr!("+ New animation", "+ Nova animação"))).clicked() {
                new_clip = true;
            }
            if let Some(clip) = self.scene.animations.get_mut(self.timeline.clip) {
                ui.add_enabled_ui(!playing_game, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut clip.name).desired_width(120.0))
                        .on_hover_text(tr!("Name used by scripts: anim.play(\"Name\")", "Nome usado pelos scripts: anim.play(\"Nome\")"));
                    ui.label(tr!("Length", "Duração"));
                    ui.add(DragValue::new(&mut clip.duration).speed(0.05).range(0.1..=600.0).suffix(" s"));
                    ui.checkbox(&mut clip.looping, tr!("Loop", "Repetir"));
                    ui.checkbox(&mut clip.autoplay, tr!("Autoplay", "Tocar ao iniciar"))
                        .on_hover_text(tr!("Starts by itself when the game starts", "Começa sozinha quando o jogo inicia"));
                    ui.checkbox(&mut clip.cutscene, tr!("Cutscene", "Cutscene")).on_hover_text(tr!(
                        "Blocks the player's input and shows black bars while it plays (Enter skips)",
                        "Bloqueia os comandos do jogador e mostra faixas pretas enquanto toca (Enter pula)"
                    ));
                    if ui.small_button("🗑").on_hover_text(tr!("Delete this animation", "Excluir esta animação")).clicked() {
                        delete_clip = true;
                    }
                });
            }
        });
        if new_clip {
            let mut n = self.scene.animations.len() + 1;
            let mut name = format!("Animation {n}");
            while self.scene.animations.iter().any(|c| c.name == name) {
                n += 1;
                name = format!("Animation {n}");
            }
            self.scene.animations.push(Clip::new(name));
            self.timeline = TimelineState { clip: self.scene.animations.len() - 1, ..Default::default() };
        }
        if delete_clip && self.timeline.clip < self.scene.animations.len() {
            self.scene.animations.remove(self.timeline.clip);
            self.timeline = TimelineState::default();
        }
        if self.scene.animations.is_empty() {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(tr!(
                        "No animation in this scene yet. Create one, select an object, move the playhead and add keys.",
                        "Ainda não há animação nesta cena. Crie uma, selecione um objeto, mova o cursor de tempo e adicione chaves."
                    ))
                    .weak(),
                );
            });
            return;
        }
        let ci = self.timeline.clip;
        let duration = self.scene.animations[ci].duration.max(0.01);

        // ------------------------------------------------------------ transport and keying
        let selected = self.selected.filter(|id| self.scene.get(*id).is_some());
        let mut seek: Option<f32> = None;
        let mut add_key: Option<Property> = None;
        let mut add_marker: Option<Action> = None;
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!playing_game, |ui| {
                if ui.button("⏮").on_hover_text(tr!("Back to the start", "Voltar ao início")).clicked() {
                    seek = Some(0.0);
                    self.timeline.previewing = false;
                }
                let label = if self.timeline.previewing { tr!("⏸ Pause", "⏸ Pausar") } else { tr!("▶ Preview", "▶ Prévia") };
                if ui.button(label).on_hover_text(tr!("Plays the animation in the editor", "Toca a animação no editor")).clicked() {
                    if !self.timeline.previewing && self.timeline.time >= duration - 1e-3 {
                        seek = Some(0.0);
                    }
                    self.timeline.previewing = !self.timeline.previewing;
                }
            });
            ui.label(RichText::new(format!("{:.2} / {:.2} s", self.timeline.time, duration)).monospace());
            ui.separator();
            match selected {
                Some(id) => {
                    ui.label(tr!("Key for {}:", "Chave para {}:", self.entity_label(id)));
                    ui.add_enabled_ui(!playing_game, |ui| {
                        for p in Property::ALL {
                            if read_property(&self.scene, id, p).is_some()
                                && ui
                                    .button(format!("+ {}", property_name(p)))
                                    .on_hover_text(tr!(
                                        "Records the current value at the playhead",
                                        "Grava o valor atual na posição do cursor de tempo"
                                    ))
                                    .clicked()
                            {
                                add_key = Some(p);
                            }
                        }
                    });
                }
                None => {
                    ui.label(RichText::new(tr!("Select an object to add keys", "Selecione um objeto para adicionar chaves")).weak());
                }
            }
            ui.separator();
            ui.add_enabled_ui(!playing_game, |ui| {
                ui.menu_button(tr!("+ Event", "+ Evento"), |ui| {
                    let scripted = self.scene.entities.iter().find(|e| e.script.is_some()).map(|e| e.id).unwrap_or(0);
                    let camera = self.scene.entities.iter().find(|e| e.camera.is_some()).map(|e| e.id).unwrap_or(0);
                    let target = selected.filter(|id| self.scene.get(*id).map(|e| e.script.is_some()).unwrap_or(false)).unwrap_or(scripted);
                    let cam = selected.filter(|id| self.scene.get(*id).map(|e| e.camera.is_some()).unwrap_or(false)).unwrap_or(camera);
                    for action in [
                        Action::Send { entity: target, event: "my_event".into(), arg: String::new() },
                        Action::Sound { file: String::new(), volume: 1.0 },
                        Action::Camera { entity: cam },
                        Action::Hud { text: String::new() },
                        Action::Broadcast { event: "my_event".into(), arg: String::new() },
                        Action::Qte {
                            id: "qte".into(),
                            kind: crate::qte::QteKind::Press,
                            keys: "e".into(),
                            count: 10,
                            hold: 1.0,
                            seconds: 2.0,
                        },
                        Action::Play { clip: String::new() },
                        Action::TimeScale { scale: 0.3 },
                    ] {
                        if ui.button(action_name(&action)).clicked() {
                            add_marker = Some(action);
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(tr!(
                    "Something that happens at the exact instant of the playhead",
                    "Algo que acontece no instante exato do cursor de tempo"
                ));
            });
        });
        if let (Some(p), Some(id)) = (add_key, selected) {
            if let Some(value) = read_property(&self.scene, id, p) {
                let t = self.timeline.time;
                let clip = &mut self.scene.animations[ci];
                let key = clip.track_mut(id, p).set_key(t, value);
                let track = clip.tracks.iter().position(|tr| tr.entity == id && tr.property == p).unwrap_or(0);
                self.timeline.selected_key = Some((track, key));
                self.timeline.selected_marker = None;
            }
        }
        if let Some(action) = add_marker {
            let t = self.timeline.time;
            let clip = &mut self.scene.animations[ci];
            clip.markers.push(Marker { t, action });
            self.timeline.selected_marker = Some(clip.markers.len() - 1);
            self.timeline.selected_key = None;
        }
        ui.separator();

        // ------------------------------------------------------------ lanes
        let label_w = 190.0;
        let row_h = 20.0;
        let mut remove_track: Option<usize> = None;
        let mut key_moved: Option<(usize, usize, f32)> = None;
        let mut marker_moved: Option<(usize, f32)> = None;
        let mut resort = false;
        let clip = self.scene.animations[ci].clone();
        egui::ScrollArea::vertical().auto_shrink([false, false]).max_height((ui.available_height() - 40.0).max(60.0)).show(ui, |ui| {
            let width = ui.available_width();
            let lane_w = (width - label_w - 14.0).max(80.0);
            let x_of = |lane: Rect, t: f32| lane.left() + (t / duration).clamp(0.0, 1.0) * lane.width();

            // ruler (drag to scrub)
            let (ruler, ruler_resp) = ui.allocate_exact_size(egui::vec2(width, row_h), Sense::click_and_drag());
            let lane0 = Rect::from_min_size(ruler.left_top() + egui::vec2(label_w, 0.0), egui::vec2(lane_w, row_h));
            let painter = ui.painter().clone();
            painter.rect_filled(lane0, 2.0, Color32::from_rgb(30, 30, 34));
            let step = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0]
                .into_iter()
                .find(|s| lane_w / (duration / s) >= 46.0)
                .unwrap_or(60.0);
            let mut t = 0.0;
            while t <= duration + 1e-4 {
                let x = x_of(lane0, t);
                painter.line_segment([Pos2::new(x, lane0.bottom() - 6.0), Pos2::new(x, lane0.bottom())], Stroke::new(1.0, Color32::GRAY));
                painter.text(Pos2::new(x + 3.0, lane0.top() + 2.0), Align2::LEFT_TOP, format!("{t:.1}"), FontId::monospace(10.0), Color32::GRAY);
                t += step;
            }
            if let Some(p) = ruler_resp.interact_pointer_pos() {
                if (ruler_resp.dragged() || ruler_resp.clicked()) && !playing_game {
                    seek = Some(((p.x - lane0.left()) / lane0.width()).clamp(0.0, 1.0) * duration);
                    self.timeline.previewing = false;
                }
            }

            // events lane
            let (row, _) = ui.allocate_exact_size(egui::vec2(width, row_h), Sense::hover());
            let lane = Rect::from_min_size(row.left_top() + egui::vec2(label_w, 0.0), egui::vec2(lane_w, row_h));
            painter.text(row.left_center() + egui::vec2(4.0, 0.0), Align2::LEFT_CENTER, tr!("Events", "Eventos"), FontId::proportional(12.5), Color32::from_gray(210));
            painter.rect_filled(lane.shrink2(egui::vec2(0.0, 1.0)), 2.0, Color32::from_rgb(40, 38, 48));
            for (mi, m) in clip.markers.iter().enumerate() {
                let c = Pos2::new(x_of(lane, m.t), lane.center().y);
                let resp = ui
                    .interact(Rect::from_center_size(c, egui::vec2(14.0, row_h)), ui.id().with(("marker", mi)), Sense::click_and_drag())
                    .on_hover_text(format!("{} · {:.2} s", action_name(&m.action), m.t));
                let selected = self.timeline.selected_marker == Some(mi);
                painter.rect_filled(Rect::from_center_size(c, egui::vec2(8.0, 14.0)), 2.0, action_color(&m.action));
                if selected {
                    painter.rect_stroke(Rect::from_center_size(c, egui::vec2(10.0, 16.0)), 2.0, Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Outside);
                }
                if resp.clicked() || resp.drag_started() {
                    self.timeline.selected_marker = Some(mi);
                    self.timeline.selected_key = None;
                }
                if resp.dragged() && !playing_game {
                    marker_moved = Some((mi, resp.drag_delta().x / lane.width() * duration));
                }
            }

            // one lane per animated property
            for (ti, track) in clip.tracks.iter().enumerate() {
                let (row, _) = ui.allocate_exact_size(egui::vec2(width, row_h), Sense::hover());
                let lane = Rect::from_min_size(row.left_top() + egui::vec2(label_w, 0.0), egui::vec2(lane_w, row_h));
                let x_btn = Rect::from_min_size(row.left_top() + egui::vec2(2.0, 2.0), egui::vec2(16.0, 16.0));
                if ui.put(x_btn, egui::Button::new("✖").small().frame(false)).on_hover_text(tr!("Remove this track", "Remover esta trilha")).clicked() {
                    remove_track = Some(ti);
                }
                let label = format!("{} · {}", self.entity_label(track.entity), property_name(track.property));
                let missing = self.scene.get(track.entity).is_none();
                painter.with_clip_rect(Rect::from_min_max(row.left_top(), Pos2::new(lane.left() - 4.0, row.bottom()))).text(
                    row.left_center() + egui::vec2(22.0, 0.0),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(12.5),
                    if missing { Color32::from_rgb(240, 110, 110) } else { Color32::from_gray(210) },
                );
                painter.rect_filled(lane.shrink2(egui::vec2(0.0, 1.0)), 2.0, Color32::from_rgb(34, 36, 42));
                if let (Some(a), Some(b)) = (track.keys.first(), track.keys.last()) {
                    let y = lane.center().y;
                    painter.line_segment([Pos2::new(x_of(lane, a.t), y), Pos2::new(x_of(lane, b.t), y)], Stroke::new(2.0, Color32::from_gray(90)));
                }
                for (ki, key) in track.keys.iter().enumerate() {
                    let c = Pos2::new(x_of(lane, key.t), lane.center().y);
                    let resp = ui
                        .interact(Rect::from_center_size(c, egui::vec2(14.0, row_h)), ui.id().with(("key", ti, ki)), Sense::click_and_drag())
                        .on_hover_text(format!("{:.2} s", key.t));
                    let selected = self.timeline.selected_key == Some((ti, ki));
                    diamond(&painter, c, 6.0, if selected { ACCENT } else { Color32::from_rgb(230, 190, 90) }, selected);
                    if resp.clicked() || resp.drag_started() {
                        self.timeline.selected_key = Some((ti, ki));
                        self.timeline.selected_marker = None;
                    }
                    if resp.dragged() && !playing_game {
                        key_moved = Some((ti, ki, resp.drag_delta().x / lane.width() * duration));
                    }
                    if resp.drag_stopped() {
                        resort = true;
                    }
                    if resp.double_clicked() && !playing_game {
                        seek = Some(key.t);
                    }
                }
            }

            // playhead across all lanes
            let x = x_of(lane0, self.timeline.time);
            let bottom = ui.min_rect().bottom();
            painter.line_segment([Pos2::new(x, lane0.top()), Pos2::new(x, bottom)], Stroke::new(1.5, Color32::from_rgb(255, 90, 90)));
            painter.add(egui::Shape::convex_polygon(
                vec![Pos2::new(x - 5.0, lane0.top()), Pos2::new(x + 5.0, lane0.top()), Pos2::new(x, lane0.top() + 8.0)],
                Color32::from_rgb(255, 90, 90),
                Stroke::NONE,
            ));
        });

        if let Some(ti) = remove_track {
            self.scene.animations[ci].tracks.remove(ti);
            self.timeline.selected_key = None;
        }
        if let Some((ti, ki, dt)) = key_moved {
            if let Some(k) = self.scene.animations[ci].tracks.get_mut(ti).and_then(|t| t.keys.get_mut(ki)) {
                k.t = (k.t + dt).clamp(0.0, duration);
            }
        }
        if let Some((mi, dt)) = marker_moved {
            if let Some(m) = self.scene.animations[ci].markers.get_mut(mi) {
                m.t = (m.t + dt).clamp(0.0, duration);
            }
        }
        if resort {
            // keep keys in time order, and the selection on the same key
            if let Some((ti, ki)) = self.timeline.selected_key {
                if let Some(track) = self.scene.animations[ci].tracks.get_mut(ti) {
                    let t = track.keys.get(ki).map(|k| k.t);
                    track.sort();
                    self.timeline.selected_key = t.and_then(|t| track.keys.iter().position(|k| k.t == t)).map(|k| (ti, k));
                }
            }
        }
        if let Some(t) = seek {
            self.timeline_seek(t);
        }

        // ------------------------------------------------------------ details of the selection
        ui.separator();
        let sounds: Vec<String> = SOUND_EXTS.iter().flat_map(|x| project::list_files(&self.project_dir, x)).collect();
        if let Some((ti, ki)) = self.timeline.selected_key {
            let mut delete = false;
            let mut clip = std::mem::replace(&mut self.scene.animations[ci], Clip::new(""));
            if let Some(track) = clip.tracks.get_mut(ti) {
                let property = track.property;
                if let Some(key) = track.keys.get_mut(ki) {
                    ui.add_enabled_ui(!playing_game, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(property_name(property)).strong());
                            ui.label(tr!("Time", "Tempo"));
                            ui.add(DragValue::new(&mut key.t).speed(0.01).range(0.0..=duration).suffix(" s"));
                            ui.label(tr!("Value", "Valor"));
                            if property == Property::Color {
                                ui.color_edit_button_rgb(&mut key.value);
                            } else {
                                let n = if property == Property::Fov { 1 } else { 3 };
                                for i in 0..n {
                                    ui.add(DragValue::new(&mut key.value[i]).speed(0.05).max_decimals(3));
                                }
                            }
                            ui.label(tr!("Then", "Depois"));
                            egui::ComboBox::from_id_salt("ease").selected_text(ease_name(key.ease)).show_ui(ui, |ui| {
                                for e in Ease::ALL {
                                    ui.selectable_value(&mut key.ease, e, ease_name(e));
                                }
                            });
                            if ui.button(tr!("🗑 Delete key", "🗑 Excluir chave")).clicked() {
                                delete = true;
                            }
                        });
                    });
                }
                if delete {
                    track.keys.remove(ki);
                    self.timeline.selected_key = None;
                }
            }
            self.scene.animations[ci] = clip;
        } else if let Some(mi) = self.timeline.selected_marker {
            let mut delete = false;
            let clip_names: Vec<String> =
                self.scene.animations.iter().enumerate().filter(|(i, _)| *i != ci).map(|(_, c)| c.name.clone()).collect();
            let mut clip = std::mem::replace(&mut self.scene.animations[ci], Clip::new(""));
            if let Some(marker) = clip.markers.get_mut(mi) {
                ui.add_enabled_ui(!playing_game, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(action_name(&marker.action)).strong().color(action_color(&marker.action)));
                        ui.label(tr!("Time", "Tempo"));
                        ui.add(DragValue::new(&mut marker.t).speed(0.01).range(0.0..=duration).suffix(" s"));
                        match &mut marker.action {
                            Action::Send { entity, event, arg } => {
                                ui.label(tr!("Send to", "Enviar para"));
                                self.entity_combo(ui, "send_to", entity, false);
                                ui.label(tr!("event", "evento"));
                                ui.add(egui::TextEdit::singleline(event).desired_width(110.0))
                                    .on_hover_text(tr!("Runs `on <event>()` in that object's script", "Executa `on <evento>()` no script daquele objeto"));
                                ui.label(tr!("argument", "argumento"));
                                ui.add(egui::TextEdit::singleline(arg).desired_width(90.0));
                            }
                            Action::Sound { file, volume } => {
                                let shown = if file.is_empty() { tr!("(choose)", "(escolha)") } else { file.clone() };
                                egui::ComboBox::from_id_salt("sound").selected_text(shown).show_ui(ui, |ui| {
                                    for f in &sounds {
                                        ui.selectable_value(file, f.clone(), f);
                                    }
                                });
                                ui.label(tr!("Volume", "Volume"));
                                ui.add(DragValue::new(volume).speed(0.02).range(0.0..=2.0));
                            }
                            Action::Camera { entity } => {
                                ui.label(tr!("Switch to", "Trocar para"));
                                self.entity_combo(ui, "cam_to", entity, true);
                            }
                            Action::Hud { text } => {
                                ui.label(tr!("Text", "Texto"));
                                ui.add(egui::TextEdit::singleline(text).desired_width(260.0));
                            }
                            Action::Broadcast { event, arg } => {
                                ui.label(tr!("event", "evento"));
                                ui.add(egui::TextEdit::singleline(event).desired_width(110.0))
                                    .on_hover_text(tr!("Runs `on <event>()` in every script of the scene", "Executa `on <evento>()` em todos os scripts da cena"));
                                ui.label(tr!("argument", "argumento"));
                                ui.add(egui::TextEdit::singleline(arg).desired_width(90.0));
                            }
                            Action::Qte { id, kind, keys, count, hold, seconds } => {
                                use crate::qte::QteKind;
                                let kind_name = |k: QteKind| match k {
                                    QteKind::Press => tr!("Press once", "Apertar uma vez"),
                                    QteKind::Mash => tr!("Press many times", "Apertar várias vezes"),
                                    QteKind::Hold => tr!("Hold", "Segurar"),
                                    QteKind::Sequence => tr!("Sequence", "Sequência"),
                                };
                                ui.label(tr!("Name", "Nome"));
                                ui.add(egui::TextEdit::singleline(id).desired_width(80.0))
                                    .on_hover_text(tr!(
                                        "Scripts receive `on qte_success(name)` or `on qte_fail(name)`. The clip waits for the answer.",
                                        "Os scripts recebem `on qte_success(nome)` ou `on qte_fail(nome)`. O clipe espera a resposta."
                                    ));
                                egui::ComboBox::from_id_salt("qte_kind").selected_text(kind_name(*kind)).show_ui(ui, |ui| {
                                    for k in QteKind::ALL {
                                        ui.selectable_value(kind, k, kind_name(k));
                                    }
                                });
                                ui.label(if *kind == QteKind::Sequence { tr!("Keys", "Teclas") } else { tr!("Key", "Tecla") });
                                ui.add(egui::TextEdit::singleline(keys).desired_width(90.0))
                                    .on_hover_text(tr!(
                                        "Key names: e, space, enter, up, mouse_left... For a sequence, separate them with spaces: w a d",
                                        "Nomes das teclas: e, space, enter, up, mouse_left... Em uma sequência, separe com espaços: w a d"
                                    ));
                                match kind {
                                    QteKind::Mash => {
                                        ui.label(tr!("Presses", "Apertos"));
                                        ui.add(DragValue::new(count).range(1..=200));
                                    }
                                    QteKind::Hold => {
                                        ui.label(tr!("Hold for", "Segurar por"));
                                        ui.add(DragValue::new(hold).speed(0.05).range(0.1..=30.0).suffix(" s"));
                                    }
                                    _ => {}
                                }
                                ui.label(tr!("Time limit", "Tempo limite"));
                                ui.add(DragValue::new(seconds).speed(0.05).range(0.2..=60.0).suffix(" s"));
                            }
                            Action::Play { clip: target } => {
                                let shown = if target.is_empty() { tr!("(choose)", "(escolha)") } else { target.clone() };
                                egui::ComboBox::from_id_salt("play_clip").selected_text(shown).show_ui(ui, |ui| {
                                    for name in &clip_names {
                                        ui.selectable_value(target, name.clone(), name);
                                    }
                                });
                            }
                            Action::TimeScale { scale } => {
                                ui.label(tr!("Speed", "Velocidade"));
                                ui.add(DragValue::new(scale).speed(0.02).range(0.0..=4.0).suffix("x"))
                                    .on_hover_text(tr!("1 is normal, 0.3 is slow motion. Add another event to bring it back to 1.", "1 é normal, 0.3 é câmera lenta. Adicione outro evento para voltar a 1."));
                            }
                        }
                        if ui.button(tr!("🗑 Delete event", "🗑 Excluir evento")).clicked() {
                            delete = true;
                        }
                    });
                });
            }
            if delete && mi < clip.markers.len() {
                clip.markers.remove(mi);
                self.timeline.selected_marker = None;
            }
            self.scene.animations[ci] = clip;
        } else {
            ui.label(
                RichText::new(tr!(
                    "Drag the ruler to move through time. Click a key or an event to edit it; drag it to change its time.",
                    "Arraste a régua para percorrer o tempo. Clique em uma chave ou evento para editar; arraste para mudar o tempo."
                ))
                .weak()
                .small(),
            );
        }
    }
}
