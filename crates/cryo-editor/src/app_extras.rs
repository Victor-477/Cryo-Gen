//! The game's HUD and RPG screens while playing, and their editors (Game > HUD, Game > RPG).

use super::*;
use crate::rpg::{Act, Page, Rpg, Steps};

impl State {
    /// A picture of the project as a texture, loaded once.
    pub(super) fn picture(&mut self, ctx: &egui::Context, rel: &str) -> Option<egui::TextureId> {
        if rel.is_empty() {
            return None;
        }
        if !self.icon_tex.contains_key(rel) {
            let tex = crate::assets::load_texture(&self.project_dir.join(rel))
                .ok()
                .map(|t| ctx.load_texture(format!("pic:{rel}"), egui::ColorImage::from_rgba_unmultiplied([t.width as usize, t.height as usize], &t.rgba), egui::TextureOptions::LINEAR));
            self.icon_tex.insert(rel.to_string(), tex);
        }
        self.icon_tex.get(rel).and_then(|t| t.as_ref().map(|t| t.id()))
    }

    fn pictures(&mut self, ctx: &egui::Context, list: &[String]) -> HashMap<String, egui::TextureId> {
        list.iter().filter_map(|r| self.picture(ctx, r).map(|t| (r.clone(), t))).collect()
    }

    /// How the RPG screens look: the menus' colours and fonts, and the icons.
    fn skin_for(&mut self, ctx: &egui::Context, rpg: &Rpg) -> crate::rpg::Skin {
        if self.menus_for != self.project_dir {
            self.menus = crate::menus::Menus::load(&self.project_dir);
            self.menus_for = self.project_dir.clone();
        }
        let look = self.menu_look(ctx);
        let icons = self.pictures(ctx, &rpg.icons());
        let menus = self.menus.clone();
        crate::rpg::Skin::from_menus(&menus, &look, icons)
    }

    /// The HUD over the game (Game > HUD), fed by the scripts.
    pub fn draw_hud(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        let ctx = ui.ctx().clone();
        if self.hud_for != self.project_dir {
            self.hud = crate::hud::Hud::load(&self.project_dir);
            self.hud_for = self.project_dir.clone();
        }
        let Some(play) = &self.play else { return };
        let rpg = play.rt.rpg().map(|(d, _)| d.clone());
        let skin = rpg.as_ref().map(|r| self.skin_for(&ctx, r));
        let images = { let list = self.hud.images(); self.pictures(&ctx, &list) };
        let look = self.menu_look(&ctx);
        let Some(play) = &self.play else { return };
        let d = crate::hud::Data {
            text: &play.rt.hud,
            values: play.rt.hud_values(),
            bars: play.rt.bars(),
            feed: play.rt.hud_feed(),
            now: play.rt.real_time,
            hidden: play.rt.hud_hidden(),
            crosshair: play.rt.crosshair(),
            rpg: play.rt.rpg(),
            scene: Some(&self.scene),
            images: &images,
            title_font: look.title_font.clone(),
            text_font: look.text_font.clone(),
            skin: skin.as_ref(),
            preview: false,
        };
        crate::hud::draw(&ui.painter_at(rect), rect, &self.hud, &d);
    }

    /// Opens the inventory or the talent tree (pauses the game).
    pub fn open_rpg(&mut self, page: Page) -> bool {
        let Some(play) = &mut self.play else { return false };
        let Some((def, _)) = play.rt.rpg() else { return false };
        let sound = def.screens.sound_open.clone();
        if self.rpg_page.is_none() && !sound.is_empty() {
            let _ = play.rt.audio.play(&sound, 0.7);
        }
        play.paused = true;
        self.rpg_page = Some(page);
        self.rpg_pick = None;
        true
    }

    pub fn close_rpg(&mut self) {
        if self.rpg_page.take().is_some() && self.menu_screen.is_none() {
            if let Some(p) = &mut self.play {
                p.paused = false;
            }
        }
    }

    /// While playing: the keys that open and close the RPG screens. Returns true when it
    /// used Esc.
    pub(super) fn rpg_keys(&mut self) -> bool {
        let Some((inv, tal)) = self.play.as_ref().and_then(|p| p.rt.rpg()).map(|(d, _)| (d.screens.inventory_key.clone(), d.screens.talents_key.clone())) else {
            return false;
        };
        if self.menu_screen.is_some() || self.loading.is_some() || self.play.as_ref().map(|p| p.rt.in_cutscene(&self.scene)).unwrap_or(true) {
            return false;
        }
        let p = &self.raw_pressed;
        let toggle = |page: Page, key: &str| p.contains(key) || (page == Page::Inventory && p.contains("pad_select"));
        for (page, key) in [(Page::Inventory, inv.as_str()), (Page::Talents, tal.as_str())] {
            if !key.is_empty() && toggle(page, key) {
                if self.rpg_page == Some(page) {
                    self.close_rpg();
                } else {
                    self.open_rpg(page);
                }
                return false;
            }
        }
        if self.rpg_page.is_some() && (p.contains("escape") || p.contains("pad_b") || p.contains("pad_start")) {
            self.close_rpg();
            return true;
        }
        if self.rpg_page.is_some() && (p.contains("tab") || p.contains("pad_lb") || p.contains("pad_rb")) {
            let next = if self.rpg_page == Some(Page::Inventory) { Page::Talents } else { Page::Inventory };
            self.rpg_page = Some(next);
        }
        false
    }

    /// The inventory or talent tree over the game.
    pub(super) fn rpg_ui(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let ctx = ui.ctx().clone();
        let Some(page) = self.rpg_page else { return };
        let Some(def) = self.play.as_ref().and_then(|p| p.rt.rpg()).map(|(d, _)| d.clone()) else {
            self.rpg_page = None;
            return;
        };
        let skin = self.skin_for(&ctx, &def);
        let p = &self.raw_pressed;
        let has = |k: &[&str]| k.iter().any(|x| p.contains(*x));
        let steps = Steps {
            dx: has(&["right", "pad_right"]) as i32 - has(&["left", "pad_left"]) as i32,
            dy: has(&["down", "pad_down"]) as i32 - has(&["up", "pad_up"]) as i32,
            choose: has(&["enter", "pad_a"]),
        };
        let Some(play) = &mut self.play else { return };
        let st = play.rt.rpg().map(|(_, s)| s.clone()).unwrap_or_default();
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(rect).layer_id(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("rpg_screen"))));
        ui.set_clip_rect(rect);
        let acts = match page {
            Page::Inventory => crate::rpg::inventory_screen(&mut ui, rect, &def, &st, &skin, &mut self.rpg_focus, &mut self.rpg_pick, steps),
            Page::Talents => crate::rpg::talents_screen(&mut ui, rect, &def, &st, &skin, &mut self.rpg_focus, steps),
        };
        for a in acts {
            match a {
                Act::Close => self.close_rpg(),
                Act::Switch(pg) => {
                    self.rpg_page = Some(pg);
                    self.rpg_focus = 0;
                }
                other => {
                    if let Some(play) = &mut self.play {
                        play.rt.rpg_act(&other);
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------
    // Editors

    /// Game > HUD: place and style what is drawn over the game.
    pub fn hud_editor_ui(&mut self, ctx: &egui::Context) {
        if !self.show_hud_editor {
            return;
        }
        if self.hud_for != self.project_dir {
            self.hud = crate::hud::Hud::load(&self.project_dir);
            self.hud_for = self.project_dir.clone();
        }
        let before = self.hud.clone();
        let images = self.project_files(crate::assets::IMAGE_EXTS);
        let entities: Vec<String> = self.scene.entities.iter().map(|e| e.name.clone()).collect();
        let rpg = Rpg::load(&self.project_dir);
        let skin = rpg.enabled.then(|| self.skin_for(ctx, &rpg));
        let pics = { let list = self.hud.images(); self.pictures(ctx, &list) };
        let look = self.menu_look(ctx);
        let bg = self.menu_texture(ctx, 1).map(|t| t.id());
        // what the game would show: the values scripts set, the bars of the running game
        let sample_state = rpg.enabled.then(|| {
            let mut s = crate::rpg::RpgState::new(&rpg);
            s.level = 3;
            s.xp = rpg.xp_for(3) * 0.4;
            for (i, sk) in rpg.skills.iter().take(s.bar.len()).enumerate() {
                s.known.push(sk.id.clone());
                s.bar[i] = sk.id.clone();
            }
            if let Some(sk) = rpg.skills.first() {
                s.cooldowns.insert(sk.id.clone(), sk.cooldown * 0.5);
            }
            s
        });
        let values: Vec<String> = {
            let mut v: Vec<String> = self.play.as_ref().map(|p| p.rt.bars().iter().map(|b| b.0.clone()).chain(p.rt.hud_values().keys().cloned()).collect()).unwrap_or_default();
            v.extend(["Health", "Stamina", "Magazine"].map(String::from));
            v.sort();
            v.dedup();
            v
        };
        let mut sel = self.hud_sel;
        let mut open = true;
        let empty = std::collections::BTreeMap::new();
        egui::Window::new(tr!("Game HUD", "HUD do jogo")).open(&mut open).default_width(980.0).default_height(680.0).resizable(true).show(ctx, |ui| {
            ui.label(RichText::new(tr!(
                "What is drawn over the game. Drag the elements in the preview; scripts feed them with bar(\"Health\", 0.7), hud(\"text\") and hud.set(\"key\", value). Texts show values with {{key}}.",
                "O que aparece sobre o jogo. Arraste os elementos na prévia; scripts alimentam com bar(\"Health\", 0.7), hud(\"texto\") e hud.set(\"chave\", valor). Textos mostram valores com {{chave}}."
            )).small());
            ui.separator();
            ui.columns(2, |cols| {
                let ui = &mut cols[0];
                ui.horizontal_wrapped(|ui| {
                    for (i, e) in self.hud.elements.iter().enumerate() {
                        let name = if e.visible { e.id.clone() } else { format!("({})", e.id) };
                        if ui.selectable_label(sel == i, name).clicked() {
                            sel = i;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("hud_add").selected_text(tr!("+ Add", "+ Adicionar")).show_ui(ui, |ui| {
                        for (code, name) in crate::hud::kinds() {
                            if ui.selectable_label(false, name).clicked() {
                                let n = self.hud.elements.iter().filter(|e| e.kind == code).count() + 1;
                                let mut e = crate::hud::Element { id: format!("{code}{n}"), kind: code.into(), anchor: "center".into(), x: 0.0, y: 0.0, ..Default::default() };
                                match code {
                                    "text" => e.source = "Potions: {potions}".into(),
                                    "bar" => e.source = "Health".into(),
                                    "skills" => (e.w, e.h) = (58.0, 58.0),
                                    "minimap" => (e.anchor, e.x, e.y, e.w, e.back, e.color) = ("top_right".into(), -20.0, 20.0, 170.0, [10, 14, 18, 190], [255, 255, 255, 255]),
                                    "crosshair" => (e.w, e.h, e.color) = (22.0, 22.0, [255, 255, 255, 255]),
                                    "image" => (e.w, e.h) = (96.0, 96.0),
                                    _ => {}
                                }
                                self.hud.elements.push(e);
                                sel = self.hud.elements.len() - 1;
                            }
                        }
                    });
                    if let Some(e) = self.hud.elements.get(sel).cloned() {
                        if ui.button(tr!("Duplicate", "Duplicar")).clicked() {
                            self.hud.elements.push(crate::hud::Element { id: format!("{}_copy", e.id), x: e.x + 20.0, y: e.y + 20.0, ..e });
                            sel = self.hud.elements.len() - 1;
                        }
                        if ui.button("⬆").on_hover_text(tr!("Draw earlier (under)", "Desenhar antes (embaixo)")).clicked() && sel > 0 {
                            self.hud.elements.swap(sel, sel - 1);
                            sel -= 1;
                        }
                        if ui.button("🗑").clicked() {
                            self.hud.elements.remove(sel);
                            sel = sel.saturating_sub(1);
                        }
                    }
                    if ui.button(tr!("Classic HUD", "HUD clássica")).on_hover_text(tr!("Back to the default layout", "Volta ao layout padrão")).clicked() {
                        self.hud = crate::hud::Hud::default();
                        sel = 0;
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().id_salt("hud_form").max_height(470.0).show(ui, |ui| {
                    if let Some(e) = self.hud.elements.get_mut(sel) {
                        crate::hud::element_form(ui, e, &images, &entities, &values);
                    }
                });
                let ui = &mut cols[1];
                ui.label(RichText::new(tr!("Preview — click to pick, drag to move", "Prévia — clique para escolher, arraste para mover")).strong());
                let d = crate::hud::Data {
                    text: self.play.as_ref().map(|p| p.rt.hud.as_str()).unwrap_or(""),
                    values: self.play.as_ref().map(|p| p.rt.hud_values()).unwrap_or(&empty),
                    bars: self.play.as_ref().map(|p| p.rt.bars()).unwrap_or(&[]),
                    feed: &[],
                    now: 0.0,
                    hidden: &[],
                    crosshair: true,
                    rpg: sample_state.as_ref().map(|s| (&rpg, s)),
                    scene: Some(&self.scene),
                    images: &pics,
                    title_font: look.title_font.clone(),
                    text_font: look.text_font.clone(),
                    skin: skin.as_ref(),
                    preview: true,
                };
                crate::hud::preview(ui, &mut self.hud, &mut sel, &d, bg);
                ui.label(RichText::new(tr!("Saved in hud.json. Positions grow with the screen (designed for 900 points high).", "Salvo em hud.json. As posições crescem com a tela (pensadas para 900 pontos de altura).")).small().weak());
            });
        });
        self.hud_sel = sel;
        if self.hud != before {
            if let Err(e) = self.hud.save(&self.project_dir) {
                self.error(e);
            }
        }
        if !open {
            self.show_hud_editor = false;
        }
    }

    /// Game > RPG: items, skills, talents and levels.
    pub fn rpg_editor_ui(&mut self, ctx: &egui::Context) {
        if !self.show_rpg_editor {
            return;
        }
        if self.rpg_edit_for != self.project_dir {
            self.rpg_edit = Rpg::load(&self.project_dir);
            self.rpg_edit_for = self.project_dir.clone();
        }
        let before = self.rpg_edit.clone();
        let images = self.project_files(crate::assets::IMAGE_EXTS);
        let sounds = self.project_files(crate::assets::SOUND_EXTS);
        let skin = self.skin_for(ctx, &before);
        let mut tab = ctx.data(|d| d.get_temp::<u8>(egui::Id::new("rpg_tab"))).unwrap_or_else(|| std::env::var("CRYO_RPG_TAB").ok().and_then(|t| t.parse().ok()).unwrap_or(0));
        let mut sel = ctx.data(|d| d.get_temp::<usize>(egui::Id::new("rpg_sel")).unwrap_or(0));
        let mut open = true;
        egui::Window::new(tr!("RPG: items, skills and talents", "RPG: itens, habilidades e talentos")).open(&mut open).default_width(960.0).default_height(680.0).resizable(true).show(ctx, |ui| {
            let r = &mut self.rpg_edit;
            ui.horizontal(|ui| {
                ui.checkbox(&mut r.enabled, RichText::new(tr!("Use RPG systems in this game", "Usar sistemas de RPG neste jogo")).strong());
                ui.separator();
                for (i, name) in [tr!("Levels & keys", "Níveis e teclas"), tr!("Items", "Itens"), tr!("Skills", "Habilidades"), tr!("Talents", "Talentos")].into_iter().enumerate() {
                    if ui.selectable_label(tab == i as u8, name).clicked() {
                        tab = i as u8;
                        sel = 0;
                    }
                }
            });
            ui.label(RichText::new(tr!(
                "Scripts: inventory.add(\"potion\"), level.add_xp(25), level.stat(\"damage\"), skills.ready(\"whirlwind\"); events on level_up(n), on skill(id), on item_used(id), on talent_learned(id, rank).",
                "Scripts: inventory.add(\"potion\"), level.add_xp(25), level.stat(\"damage\"), skills.ready(\"whirlwind\"); eventos on level_up(n), on skill(id), on item_used(id), on talent_learned(id, rank)."
            )).small().weak());
            ui.separator();
            let pick = |ui: &mut egui::Ui, id: &str, value: &mut String, list: &[String]| {
                egui::ComboBox::from_id_salt(id.to_string()).width(200.0).selected_text(if value.is_empty() { tr!("(none)", "(nenhum)") } else { value.clone() }).show_ui(ui, |ui| {
                    ui.selectable_value(value, String::new(), tr!("(none)", "(nenhum)"));
                    for f in list {
                        ui.selectable_value(value, f.clone(), f);
                    }
                });
            };
            let color = |ui: &mut egui::Ui, c: &mut [u8; 3]| {
                let mut c32 = Color32::from_rgb(c[0], c[1], c[2]);
                if ui.color_edit_button_srgba(&mut c32).changed() {
                    *c = [c32.r(), c32.g(), c32.b()];
                }
            };
            let stats_ui = |ui: &mut egui::Ui, salt: &str, stats: &mut std::collections::BTreeMap<String, f64>| {
                let mut remove = None;
                let mut rename = None;
                for (k, v) in stats.iter_mut() {
                    ui.horizontal(|ui| {
                        let mut name = k.clone();
                        if ui.add(egui::TextEdit::singleline(&mut name).desired_width(110.0)).lost_focus() && name != *k && !name.is_empty() {
                            rename = Some((k.clone(), name));
                        }
                        ui.add(egui::DragValue::new(v).speed(0.05));
                        if ui.small_button("🗑").clicked() {
                            remove = Some(k.clone());
                        }
                    });
                }
                if let Some(k) = remove {
                    stats.remove(&k);
                }
                if let Some((a, b)) = rename {
                    if let Some(v) = stats.remove(&a) {
                        stats.insert(b, v);
                    }
                }
                ui.horizontal(|ui| {
                    for s in ["damage", "armor", "max_health", "speed"] {
                        if !stats.contains_key(s) && ui.small_button(format!("+ {s}")).clicked() {
                            stats.insert(s.into(), 1.0);
                        }
                    }
                    let _ = salt;
                });
            };
            let item_ids: Vec<String> = r.items.iter().map(|i| i.id.clone()).collect();
            let skill_ids: Vec<String> = r.skills.iter().map(|i| i.id.clone()).collect();
            let talent_ids: Vec<String> = r.talents.iter().map(|i| i.id.clone()).collect();
            match tab {
                0 => {
                    egui::Grid::new("rpg_lv").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                        let l = &mut r.levels;
                        ui.label(tr!("Highest level", "Nível máximo"));
                        ui.add(egui::DragValue::new(&mut l.max_level).range(1..=200));
                        ui.end_row();
                        ui.label(tr!("XP from level 1 to 2", "XP do nível 1 ao 2"));
                        ui.add(egui::DragValue::new(&mut l.xp_first).range(1.0..=1e7));
                        ui.end_row();
                        ui.label(tr!("Each level needs ×", "Cada nível pede ×"));
                        ui.add(egui::Slider::new(&mut l.growth, 1.0..=3.0));
                        ui.end_row();
                        ui.label(tr!("Talent points per level", "Pontos de talento por nível"));
                        ui.add(egui::DragValue::new(&mut l.points_per_level).range(0..=10));
                        ui.end_row();
                        ui.label(tr!("Each level adds", "Cada nível adiciona"));
                        ui.vertical(|ui| stats_ui(ui, "lv", &mut l.stats_per_level));
                        ui.end_row();
                        let s = &mut r.screens;
                        ui.label(tr!("Inventory key", "Tecla do inventário"));
                        ui.add(egui::TextEdit::singleline(&mut s.inventory_key).desired_width(60.0));
                        ui.end_row();
                        ui.label(tr!("Talents key", "Tecla dos talentos"));
                        ui.add(egui::TextEdit::singleline(&mut s.talents_key).desired_width(60.0));
                        ui.end_row();
                        ui.label(tr!("Bag slots", "Espaços da bolsa"));
                        ui.add(egui::DragValue::new(&mut s.slots).range(1..=120));
                        ui.end_row();
                        ui.label(tr!("Skill bar keys", "Teclas da barra"));
                        let mut keys = s.bar_keys.join(" ");
                        if ui.add(egui::TextEdit::singleline(&mut keys).desired_width(160.0).hint_text("1 2 3 4")).changed() {
                            s.bar_keys = keys.split_whitespace().map(String::from).take(10).collect();
                        }
                        ui.end_row();
                        for (name, v) in [(tr!("Sound: open", "Som: abrir"), &mut s.sound_open), (tr!("Sound: item", "Som: item"), &mut s.sound_item), (tr!("Sound: talent", "Som: talento"), &mut s.sound_talent), (tr!("Sound: level up", "Som: subir de nível"), &mut s.sound_level)] {
                            ui.label(name);
                            pick(ui, &format!("snd{}", v.len()), v, &sounds);
                            ui.end_row();
                        }
                        ui.label(tr!("Starts with", "Começa com"));
                        ui.vertical(|ui| {
                            let mut remove = None;
                            for (i, st) in r.start_items.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    pick(ui, &format!("start{i}"), &mut st.id, &item_ids);
                                    ui.add(egui::DragValue::new(&mut st.count).range(1..=999));
                                    if ui.small_button("🗑").clicked() {
                                        remove = Some(i);
                                    }
                                });
                            }
                            if let Some(i) = remove {
                                r.start_items.remove(i);
                            }
                            if ui.small_button(tr!("+ item", "+ item")).clicked() {
                                r.start_items.push(crate::rpg::Stack { id: item_ids.first().cloned().unwrap_or_default(), count: 1 });
                            }
                            let mut text = r.start_skills.join(", ");
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(tr!("skills:", "habilidades:")).small());
                                if ui.add(egui::TextEdit::singleline(&mut text).desired_width(200.0)).changed() {
                                    r.start_skills = text.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
                                }
                            });
                        });
                        ui.end_row();
                        ui.label(tr!("Talent columns", "Colunas de talentos"));
                        let mut text = r.branches.join(", ");
                        if ui.add(egui::TextEdit::singleline(&mut text).desired_width(260.0)).changed() {
                            r.branches = text.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
                        }
                        ui.end_row();
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new(tr!(
                        "XP needed per level: {}",
                        "XP por nível: {}",
                        (1..r.levels.max_level.min(8)).map(|l| r.xp_for(l).to_string()).collect::<Vec<_>>().join(", ")
                    )).small());
                }
                1 | 2 | 3 => {
                    ui.columns(2, |cols| {
                        let ui = &mut cols[0];
                        let names: Vec<String> = match tab {
                            1 => r.items.iter().map(|i| i.name.clone()).collect(),
                            2 => r.skills.iter().map(|i| i.name.clone()).collect(),
                            _ => r.talents.iter().map(|i| i.name.clone()).collect(),
                        };
                        egui::ScrollArea::vertical().id_salt("rpg_list").max_height(150.0).show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for (i, n) in names.iter().enumerate() {
                                    if ui.selectable_label(sel == i, n).clicked() {
                                        sel = i;
                                    }
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            if ui.button(tr!("+ New", "+ Novo")).clicked() {
                                match tab {
                                    1 => r.items.push(crate::rpg::ItemDef { id: format!("item{}", r.items.len() + 1), ..Default::default() }),
                                    2 => r.skills.push(crate::rpg::SkillDef { id: format!("skill{}", r.skills.len() + 1), ..Default::default() }),
                                    _ => r.talents.push(crate::rpg::Talent { id: format!("talent{}", r.talents.len() + 1), ..Default::default() }),
                                }
                                sel = names.len();
                            }
                            if !names.is_empty() && ui.button("🗑").clicked() {
                                match tab {
                                    1 => drop(r.items.remove(sel.min(r.items.len() - 1))),
                                    2 => drop(r.skills.remove(sel.min(r.skills.len() - 1))),
                                    _ => drop(r.talents.remove(sel.min(r.talents.len() - 1))),
                                }
                                sel = sel.saturating_sub(1);
                            }
                        });
                        ui.separator();
                        egui::ScrollArea::vertical().id_salt("rpg_form").max_height(420.0).show(ui, |ui| {
                            egui::Grid::new("rpg_f").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| match tab {
                                1 => {
                                    if let Some(it) = r.items.get_mut(sel) {
                                        ui.label("id");
                                        ui.text_edit_singleline(&mut it.id);
                                        ui.end_row();
                                        ui.label(tr!("Name", "Nome"));
                                        ui.text_edit_singleline(&mut it.name);
                                        ui.end_row();
                                        ui.label(tr!("Description", "Descrição"));
                                        ui.add(egui::TextEdit::multiline(&mut it.description).desired_rows(2));
                                        ui.end_row();
                                        ui.label(tr!("Icon", "Ícone"));
                                        ui.horizontal(|ui| {
                                            pick(ui, "it_icon", &mut it.icon, &images);
                                            color(ui, &mut it.color);
                                        });
                                        ui.end_row();
                                        ui.label(tr!("Worn as", "Vestido como"));
                                        ui.horizontal(|ui| {
                                            ui.selectable_value(&mut it.slot, String::new(), tr!("not worn", "não se veste"));
                                            for s in crate::rpg::EQUIP_SLOTS {
                                                ui.selectable_value(&mut it.slot, s.to_string(), crate::rpg::slot_name(s));
                                            }
                                        });
                                        ui.end_row();
                                        ui.label(tr!("Used up when used", "Gasta ao usar"));
                                        ui.checkbox(&mut it.consumable, "");
                                        ui.end_row();
                                        ui.label(tr!("Per slot", "Por espaço"));
                                        ui.add(egui::DragValue::new(&mut it.stack).range(1..=999));
                                        ui.end_row();
                                        ui.label(tr!("Stats when worn", "Atributos vestido"));
                                        ui.vertical(|ui| stats_ui(ui, "it", &mut it.stats));
                                        ui.end_row();
                                    }
                                }
                                2 => {
                                    if let Some(sk) = r.skills.get_mut(sel) {
                                        ui.label("id");
                                        ui.text_edit_singleline(&mut sk.id);
                                        ui.end_row();
                                        ui.label(tr!("Name", "Nome"));
                                        ui.text_edit_singleline(&mut sk.name);
                                        ui.end_row();
                                        ui.label(tr!("Description", "Descrição"));
                                        ui.add(egui::TextEdit::multiline(&mut sk.description).desired_rows(2));
                                        ui.end_row();
                                        ui.label(tr!("Icon", "Ícone"));
                                        ui.horizontal(|ui| {
                                            pick(ui, "sk_icon", &mut sk.icon, &images);
                                            color(ui, &mut sk.color);
                                        });
                                        ui.end_row();
                                        ui.label(tr!("Cooldown (s)", "Recarga (s)"));
                                        ui.add(egui::Slider::new(&mut sk.cooldown, 0.0..=120.0));
                                        ui.end_row();
                                        ui.label(tr!("Cost (shown)", "Custo (mostrado)"));
                                        ui.add(egui::TextEdit::singleline(&mut sk.cost).hint_text("30 stamina"));
                                        ui.end_row();
                                        ui.label("");
                                        ui.label(RichText::new(tr!("When its key is pressed and it is ready, every script gets on skill(\"{}\").", "Quando a tecla é apertada e ela está pronta, todos os scripts recebem on skill(\"{}\").", sk.id)).small().weak());
                                        ui.end_row();
                                    }
                                }
                                _ => {
                                    let n = r.branches.len().max(1);
                                    if let Some(t) = r.talents.get_mut(sel) {
                                        ui.label("id");
                                        ui.text_edit_singleline(&mut t.id);
                                        ui.end_row();
                                        ui.label(tr!("Name", "Nome"));
                                        ui.text_edit_singleline(&mut t.name);
                                        ui.end_row();
                                        ui.label(tr!("Description", "Descrição"));
                                        ui.add(egui::TextEdit::multiline(&mut t.description).desired_rows(2));
                                        ui.end_row();
                                        ui.label(tr!("Icon", "Ícone"));
                                        ui.horizontal(|ui| {
                                            pick(ui, "ta_icon", &mut t.icon, &images);
                                            color(ui, &mut t.color);
                                        });
                                        ui.end_row();
                                        ui.label(tr!("Column, row", "Coluna, linha"));
                                        ui.horizontal(|ui| {
                                            ui.add(egui::DragValue::new(&mut t.branch).range(0..=n - 1));
                                            ui.add(egui::DragValue::new(&mut t.tier).range(0..=12));
                                        });
                                        ui.end_row();
                                        ui.label(tr!("Ranks", "Níveis"));
                                        ui.add(egui::DragValue::new(&mut t.max_rank).range(1..=10));
                                        ui.end_row();
                                        ui.label(tr!("Player level needed", "Nível do jogador"));
                                        ui.add(egui::DragValue::new(&mut t.level).range(1..=200));
                                        ui.end_row();
                                        ui.label(tr!("Needs", "Requer"));
                                        ui.horizontal_wrapped(|ui| {
                                            for id in &talent_ids {
                                                if *id == t.id {
                                                    continue;
                                                }
                                                let mut on = t.requires.contains(id);
                                                if ui.checkbox(&mut on, id).changed() {
                                                    t.requires.retain(|x| x != id);
                                                    if on {
                                                        t.requires.push(id.clone());
                                                    }
                                                }
                                            }
                                        });
                                        ui.end_row();
                                        ui.label(tr!("Teaches skill", "Ensina habilidade"));
                                        pick(ui, "ta_skill", &mut t.skill, &skill_ids);
                                        ui.end_row();
                                        ui.label(tr!("Each rank adds", "Cada nível adiciona"));
                                        ui.vertical(|ui| stats_ui(ui, "ta", &mut t.stats));
                                        ui.end_row();
                                    }
                                }
                            });
                        });
                        // the preview of the tree or the bag
                        let ui = &mut cols[1];
                        let w = ui.available_width().max(260.0);
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, w * 0.66), egui::Sense::hover());
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                        child.set_clip_rect(rect);
                        let mut st = crate::rpg::RpgState::new(r);
                        st.level = 4;
                        st.points = 2;
                        if let Some(first) = r.talents.first() {
                            st.ranks.insert(first.id.clone(), 1);
                        }
                        for s in &r.skills {
                            st.learn_skill(r, &s.id);
                        }
                        let mut focus = sel;
                        let mut pick_none = None;
                        if tab == 3 {
                            crate::rpg::talents_screen(&mut child, rect, r, &st, &skin, &mut focus, Steps::default());
                        } else {
                            for id in item_ids.iter().take(st.bag.len()) {
                                if st.count(id) == 0 {
                                    st.add_item(r, id, 1);
                                }
                            }
                            crate::rpg::inventory_screen(&mut child, rect, r, &st, &skin, &mut focus, &mut pick_none, Steps::default());
                        }
                        ui.label(RichText::new(tr!("Preview with the menus' colours and fonts. Saved in rpg.json.", "Prévia com as cores e fontes dos menus. Salvo em rpg.json.")).small().weak());
                    });
                }
                _ => {}
            }
        });
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("rpg_tab"), tab);
            d.insert_temp(egui::Id::new("rpg_sel"), sel);
        });
        if self.rpg_edit != before {
            if let Err(e) = self.rpg_edit.save(&self.project_dir) {
                self.error(e);
            }
        }
        if !open {
            self.show_rpg_editor = false;
        }
    }
}
