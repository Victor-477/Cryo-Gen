//! The game's HUD: what is drawn over the game while playing, made of elements the developer
//! places in Game > HUD (bars, texts, pictures, the skill bar, experience, crosshair,
//! minimap), saved in `hud.json`. Scripts feed it with `bar(...)`, `hud(...)` and
//! `hud.set("key", value)`; texts show values with `{key}`.

use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Pos2, Rect, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub const FILE: &str = "hud.json";

/// The kinds of elements, with their names in the editor.
pub fn kinds() -> Vec<(&'static str, String)> {
    vec![
        ("text", tr!("Text", "Texto")),
        ("bar", tr!("Bar (one value)", "Barra (um valor)")),
        ("bars", tr!("Script bars (stacked)", "Barras dos scripts (empilhadas)")),
        ("boss", tr!("Boss bars", "Barras de chefe")),
        ("image", tr!("Picture", "Imagem")),
        ("skills", tr!("Skill bar (RPG)", "Barra de habilidades (RPG)")),
        ("xp", tr!("Level and XP (RPG)", "Nível e XP (RPG)")),
        ("crosshair", tr!("Crosshair", "Mira")),
        ("minimap", tr!("Minimap", "Minimapa")),
        ("feed", tr!("Messages (loot, level up)", "Mensagens (itens, nível)")),
    ]
}

pub const ANCHORS: [&str; 9] = ["top_left", "top", "top_right", "left", "center", "right", "bottom_left", "bottom", "bottom_right"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Element {
    pub id: String,
    pub kind: String,
    /// Which point of the screen it hangs from, and how far from it (points on a screen 900
    /// high; they grow with the screen).
    pub anchor: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Fill (bars, crosshair, the player on the minimap) and background.
    pub color: [u8; 4],
    pub back: [u8; 4],
    pub text_color: [u8; 4],
    pub text_size: f32,
    /// "default", "title" or "text": the fonts of the game's menus.
    pub font: String,
    /// What it shows: the template of a text ("Potions: {potions}"), the value of a bar
    /// ("Health"), a picture's file, the entity the minimap follows.
    pub source: String,
    /// A label over bars.
    pub label: String,
    pub visible: bool,
    pub shadow: bool,
    pub rounding: f32,
    /// Minimap: how many metres from the centre to the edge.
    pub range: f32,
}

impl Default for Element {
    fn default() -> Self {
        Element {
            id: "element".into(),
            kind: "text".into(),
            anchor: "top_left".into(),
            x: 16.0,
            y: 16.0,
            w: 240.0,
            h: 14.0,
            color: [205, 50, 45, 255],
            back: [0, 0, 0, 170],
            text_color: [240, 240, 240, 255],
            text_size: 20.0,
            font: "default".into(),
            source: String::new(),
            label: String::new(),
            visible: true,
            shadow: true,
            rounding: 3.0,
            range: 40.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hud {
    pub elements: Vec<Element>,
}

impl Default for Hud {
    /// What games showed before the HUD could be edited: the script text at the top left,
    /// bars under it, boss bars at the bottom, the crosshair, and the RPG bars when used.
    fn default() -> Self {
        let e = |id: &str, kind: &str, anchor: &str, x: f32, y: f32, w: f32, h: f32| Element { id: id.into(), kind: kind.into(), anchor: anchor.into(), x, y, w, h, ..Default::default() };
        Hud {
            elements: vec![
                Element { source: "{hud}".into(), ..e("script_text", "text", "top_left", 16.0, 14.0, 0.0, 0.0) },
                e("bars", "bars", "top_left", 16.0, 0.0, 230.0, 12.0),
                e("boss", "boss", "bottom", 0.0, -110.0, 620.0, 12.0),
                Element { color: [255, 255, 255, 255], ..e("crosshair", "crosshair", "center", 0.0, 0.0, 22.0, 22.0) },
                e("skills", "skills", "bottom", 0.0, -18.0, 58.0, 58.0),
                Element { color: [200, 160, 70, 255], ..e("xp", "xp", "bottom", 0.0, -92.0, 360.0, 7.0) },
            ],
        }
    }
}

impl Hud {
    /// The project's HUD, or the classic one when it has none.
    pub fn load(project_dir: &Path) -> Hud {
        std::fs::read_to_string(project_dir.join(FILE)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, project_dir: &Path) -> Result<(), String> {
        std::fs::write(project_dir.join(FILE), serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| format!("{FILE}: {e}"))
    }

    pub fn images(&self) -> Vec<String> {
        self.elements.iter().filter(|e| e.kind == "image" && !e.source.is_empty()).map(|e| e.source.clone()).collect()
    }
}

/// What the HUD shows this frame.
pub struct Data<'a> {
    /// `hud("...")` from scripts.
    pub text: &'a str,
    /// `hud.set(key, value, max)` and `bar(name, fraction)`: value and maximum.
    pub values: &'a BTreeMap<String, (f64, f64)>,
    pub bars: &'a [(String, f32, [f32; 3])],
    /// `hud.message(text)` and what the RPG reports: (text, when, real seconds).
    pub feed: &'a [(String, f64)],
    pub now: f64,
    /// Elements hidden by scripts (`hud.show(id, false)`).
    pub hidden: &'a [String],
    pub crosshair: bool,
    pub rpg: Option<(&'a crate::rpg::Rpg, &'a crate::rpg::RpgState)>,
    pub scene: Option<&'a crate::scene::Scene>,
    pub images: &'a HashMap<String, egui::TextureId>,
    pub title_font: FontFamily,
    pub text_font: FontFamily,
    /// The skin of the RPG screens (colours of the menus).
    pub skin: Option<&'a crate::rpg::Skin>,
    /// In the editor's preview: show every element, with stand-in values.
    pub preview: bool,
}

fn col(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// The scale of the HUD for a screen this tall.
pub fn scale(rect: Rect) -> f32 {
    (rect.height() / 900.0).clamp(0.25, 3.0)
}

/// The anchor point on the screen, and how the element lines up with it.
fn anchor(rect: Rect, a: &str) -> (Pos2, Align2) {
    match a {
        "top" => (rect.center_top(), Align2::CENTER_TOP),
        "top_right" => (rect.right_top(), Align2::RIGHT_TOP),
        "left" => (rect.left_center(), Align2::LEFT_CENTER),
        "center" => (rect.center(), Align2::CENTER_CENTER),
        "right" => (rect.right_center(), Align2::RIGHT_CENTER),
        "bottom_left" => (rect.left_bottom(), Align2::LEFT_BOTTOM),
        "bottom" => (rect.center_bottom(), Align2::CENTER_BOTTOM),
        "bottom_right" => (rect.right_bottom(), Align2::RIGHT_BOTTOM),
        _ => (rect.left_top(), Align2::LEFT_TOP),
    }
}

/// Where an element of a given size goes on the screen.
pub fn place(rect: Rect, e: &Element, size: Vec2) -> Rect {
    let k = scale(rect);
    let (p, align) = anchor(rect, &e.anchor);
    align.anchor_size(p + Vec2::new(e.x, e.y) * k, size)
}

fn font(d: &Data, which: &str, size: f32) -> FontId {
    FontId::new(
        size,
        match which {
            "title" => d.title_font.clone(),
            "text" => d.text_font.clone(),
            _ => FontFamily::Proportional,
        },
    )
}

fn fmt(v: f64) -> String {
    if (v - v.round()).abs() < 1e-6 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

/// A text with `{key}` replaced by values: script values, `{hud}`, and the RPG's
/// `{level}`, `{xp}`, `{xp_next}`, `{points}`.
pub fn fill(template: &str, d: &Data) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('}') {
            Some(j) => {
                let key = &after[..j];
                out.push_str(&value_text(key, d));
                rest = &after[j + 1..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn value_text(key: &str, d: &Data) -> String {
    if key == "hud" {
        return if d.preview && d.text.is_empty() { tr!("(text from hud(...))", "(texto de hud(...))") } else { d.text.to_string() };
    }
    if let Some((rpg, st)) = d.rpg {
        match key {
            "level" => return st.level.to_string(),
            "xp" => return fmt(st.xp.floor()),
            "xp_next" => return fmt(st.xp_next(rpg)),
            "points" => return st.points.to_string(),
            _ => {}
        }
    }
    match d.values.get(key) {
        Some((v, _)) => fmt(*v),
        None if d.preview => "7".into(),
        None => String::new(),
    }
}

fn shadow_text(p: &egui::Painter, pos: Pos2, align: Align2, text: &str, f: FontId, c: Color32, shadow: bool) -> Rect {
    if shadow {
        let o = (f.size * 0.07).clamp(1.0, 3.0);
        p.text(pos + Vec2::splat(o), align, text, f.clone(), Color32::from_black_alpha((c.a() as f32 * 0.9) as u8));
    }
    p.text(pos, align, text, f, c)
}

fn bar_rect(p: &egui::Painter, r: Rect, frac: f32, fill: Color32, back: Color32, rounding: f32) {
    p.rect_filled(r.expand(2.0), rounding + 1.0, back);
    let mut f = r;
    f.set_width(r.width() * frac.clamp(0.0, 1.0));
    if f.width() > 0.5 {
        p.rect_filled(f, rounding, fill);
    }
}

/// Draws the HUD; returns each drawn element's rectangle (the editor uses them to pick).
pub fn draw(p: &egui::Painter, rect: Rect, hud: &Hud, d: &Data) -> Vec<Rect> {
    let k = scale(rect);
    // bars a "bar" element shows by name are not stacked again
    let bound: Vec<&str> = hud.elements.iter().filter(|e| e.kind == "bar").map(|e| e.source.as_str()).collect();
    let mut rects = vec![];
    for e in &hud.elements {
        let shown = d.preview || (e.visible && !d.hidden.iter().any(|h| *h == e.id));
        if !shown {
            rects.push(Rect::NOTHING);
            continue;
        }
        let ts = e.text_size * k;
        let r = match e.kind.as_str() {
            "text" => {
                let text = fill(&e.source, d);
                if text.is_empty() {
                    place(rect, e, Vec2::splat(4.0))
                } else {
                    let f = font(d, &e.font, ts);
                    let g = p.layout_no_wrap(text.clone(), f.clone(), Color32::WHITE);
                    let r = place(rect, e, g.size());
                    if e.back[3] > 0 && e.back != [0, 0, 0, 170] {
                        p.rect_filled(r.expand(6.0 * k), e.rounding, col(e.back));
                    }
                    let align = Align2([match e.anchor.as_str() {
                        "top_right" | "right" | "bottom_right" => egui::Align::Max,
                        "top" | "center" | "bottom" => egui::Align::Center,
                        _ => egui::Align::Min,
                    }, egui::Align::Min]);
                    let x = match align.x() {
                        egui::Align::Min => r.left(),
                        egui::Align::Center => r.center().x,
                        egui::Align::Max => r.right(),
                    };
                    shadow_text(p, Pos2::new(x, r.top()), align, &text, f, col(e.text_color), e.shadow);
                    r
                }
            }
            "bar" => {
                let (frac, color) = match d.bars.iter().find(|b| b.0 == e.source) {
                    Some((_, v, c)) if e.color == Element::default().color => (*v, Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)),
                    Some((_, v, _)) => (*v, col(e.color)),
                    None => match d.values.get(&e.source) {
                        Some((v, max)) => ((*v / max.max(1e-9)) as f32, col(e.color)),
                        None => (if d.preview { 0.7 } else { 0.0 }, col(e.color)),
                    },
                };
                let r = place(rect, e, Vec2::new(e.w, e.h) * k);
                if !e.label.is_empty() {
                    let f = font(d, &e.font, (ts * 0.8).max(9.0));
                    shadow_text(p, r.left_top() + Vec2::new(0.0, -2.0), Align2::LEFT_BOTTOM, &fill(&e.label, d), f, col(e.text_color), e.shadow);
                }
                if d.bars.iter().any(|b| b.0 == e.source) || d.values.contains_key(&e.source) || d.preview {
                    bar_rect(p, r, frac, color, col(e.back), e.rounding);
                }
                r
            }
            "bars" => {
                let list: Vec<(String, f32, [f32; 3])> = if d.preview && d.bars.is_empty() {
                    vec![("Health".into(), 0.8, [0.8, 0.16, 0.14]), ("Stamina".into(), 0.55, [0.3, 0.72, 0.36])]
                } else {
                    d.bars.iter().filter(|b| !b.0.starts_with('!') && !bound.contains(&b.0.as_str())).cloned().collect()
                };
                // under the script text when both hang from the top left
                let mut y0 = e.y;
                if e.anchor == "top_left" {
                    if let Some(t) = hud.elements.iter().find(|t| t.kind == "text" && t.source.contains("{hud}") && t.anchor == "top_left") {
                        let lines = if d.text.is_empty() { if d.preview { 1 } else { 0 } } else { d.text.lines().count() };
                        y0 = y0.max(t.y + lines as f32 * t.text_size * 1.5);
                    }
                }
                let step = e.h + 22.0;
                let total = Vec2::new(e.w, list.len().max(1) as f32 * step) * k;
                let all = place(rect, &Element { y: y0, ..e.clone() }, total);
                for (i, (name, v, c)) in list.iter().enumerate() {
                    let r = Rect::from_min_size(all.min + Vec2::new(0.0, (i as f32 * step + 16.0) * k), Vec2::new(e.w, e.h) * k);
                    let f = font(d, &e.font, (13.0 * k).max(9.0));
                    shadow_text(p, r.left_top() + Vec2::new(0.0, -2.0), Align2::LEFT_BOTTOM, name, f, col(e.text_color), e.shadow);
                    bar_rect(p, r, *v, Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8), col(e.back), e.rounding);
                }
                all
            }
            "boss" => {
                let list: Vec<(String, f32, [f32; 3])> = if d.preview && !d.bars.iter().any(|b| b.0.starts_with('!')) {
                    vec![("!Boss".into(), 0.6, [0.75, 0.3, 0.1])]
                } else {
                    d.bars.iter().filter(|b| b.0.starts_with('!')).cloned().collect()
                };
                let w = (e.w * k).min(rect.width() * 0.8);
                let mut all = place(rect, e, Vec2::new(w, e.h * k));
                let first = all;
                for (i, (name, v, c)) in list.iter().enumerate() {
                    let r = first.translate(Vec2::new(0.0, -(i as f32) * 40.0 * k));
                    let f = font(d, &e.font, (15.0 * k).max(9.0));
                    shadow_text(p, r.center_top() + Vec2::new(0.0, -3.0), Align2::CENTER_BOTTOM, name.trim_start_matches('!'), f, col(e.text_color), e.shadow);
                    bar_rect(p, r, *v, Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8), col(e.back), e.rounding);
                    all = all.union(r);
                }
                all
            }
            "image" => {
                let r = place(rect, e, Vec2::new(e.w, e.h) * k);
                match d.images.get(&e.source) {
                    Some(tex) => {
                        p.image(*tex, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::from_white_alpha(e.color[3]));
                    }
                    None if d.preview => {
                        p.rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_gray(150)), egui::StrokeKind::Inside);
                        p.text(r.center(), Align2::CENTER_CENTER, tr!("picture", "imagem"), FontId::proportional(11.0), Color32::from_gray(170));
                    }
                    None => {}
                }
                r
            }
            "skills" => match (d.rpg, d.skin) {
                (Some((rpg, st)), Some(skin)) if !st.bar.is_empty() => {
                    let size = e.w * k;
                    let n = st.bar.len() as f32;
                    let total = Vec2::new(n * size + (n - 1.0) * size * 0.12, size);
                    let r = place(rect, e, total);
                    crate::rpg::draw_skill_bar(p, r.center_bottom(), size, rpg, st, skin)
                }
                _ if d.preview => {
                    let size = e.w * k;
                    let r = place(rect, e, Vec2::new(4.0 * size + 3.0 * size * 0.12, size));
                    for i in 0..4 {
                        let s = Rect::from_min_size(r.min + Vec2::new(i as f32 * size * 1.12, 0.0), Vec2::splat(size));
                        p.rect_filled(s, 5.0, Color32::from_black_alpha(170));
                        p.text(s.left_top() + Vec2::new(4.0, 2.0), Align2::LEFT_TOP, (i + 1).to_string(), FontId::proportional(size * 0.24), Color32::WHITE);
                    }
                    r
                }
                _ => Rect::NOTHING,
            },
            "xp" => {
                let r = place(rect, e, Vec2::new(e.w, e.h) * k);
                match (d.rpg, d.skin) {
                    (Some((rpg, st)), Some(skin)) => crate::rpg::draw_xp_bar(p, r, rpg, st, skin, col(e.color), (ts * 0.8).max(9.0)),
                    _ if d.preview => {
                        bar_rect(p, r, 0.4, col(e.color), col(e.back), e.rounding);
                        p.text(r.center_top() + Vec2::new(0.0, -3.0), Align2::CENTER_BOTTOM, tr!("Level 3 · 120 / 250 XP", "Nível 3 · 120 / 250 XP"), font(d, &e.font, (13.0 * k).max(9.0)), col(e.text_color));
                    }
                    _ => {}
                }
                r
            }
            "crosshair" => {
                let r = place(rect, e, Vec2::new(e.w, e.h) * k);
                if d.crosshair || d.preview {
                    let c = r.center();
                    let s = r.width() / 2.0;
                    let g = s * 0.36;
                    for (a, b) in [((-s, 0.0), (-g, 0.0)), ((g, 0.0), (s, 0.0)), ((0.0, -s), (0.0, -g)), ((0.0, g), (0.0, s))] {
                        let (pa, pb) = (c + Vec2::new(a.0, a.1), c + Vec2::new(b.0, b.1));
                        p.line_segment([pa, pb], Stroke::new(3.5, Color32::from_black_alpha(160)));
                        p.line_segment([pa, pb], Stroke::new(1.6, col(e.color)));
                    }
                    p.circle_filled(c, 1.6, col(e.color));
                }
                r
            }
            "minimap" => {
                let r = place(rect, e, Vec2::new(e.w, e.w) * k);
                minimap(p, r, e, d);
                r
            }
            "feed" => {
                // the newest at the bottom; each line stays 4 seconds and fades
                let lines: Vec<(String, f32)> = if d.preview && d.feed.is_empty() {
                    vec![("+1 Healing Draught".into(), 1.0), (tr!("Level 3!", "Nível 3!"), 1.0)]
                } else {
                    d.feed.iter().filter_map(|(t, at)| {
                        let age = (d.now - at) as f32;
                        (age < 4.0).then(|| (t.clone(), (4.0 - age).clamp(0.0, 1.0)))
                    }).collect()
                };
                let f = font(d, &e.font, ts);
                let lh = ts * 1.35;
                let r = place(rect, e, Vec2::new(260.0 * k, lh * 5.0));
                let align = if e.anchor.ends_with("right") || e.anchor == "right" { Align2::RIGHT_BOTTOM } else if e.anchor == "top" || e.anchor == "bottom" || e.anchor == "center" { Align2::CENTER_BOTTOM } else { Align2::LEFT_BOTTOM };
                let x = match align.x() { egui::Align::Max => r.right(), egui::Align::Center => r.center().x, _ => r.left() };
                for (i, (t, a)) in lines.iter().rev().take(5).enumerate() {
                    let c = col(e.text_color).gamma_multiply(*a);
                    shadow_text(p, Pos2::new(x, r.bottom() - i as f32 * lh), align, t, f.clone(), c, e.shadow);
                }
                r
            }
            _ => Rect::NOTHING,
        };
        rects.push(r);
    }
    rects
}

/// A round map seen from above, north up: the followed entity in the middle, enemies red,
/// other tagged things in the element's colour.
fn minimap(p: &egui::Painter, r: Rect, e: &Element, d: &Data) {
    let c = r.center();
    let rad = r.width() / 2.0;
    p.circle_filled(c, rad, col(e.back));
    p.circle_stroke(c, rad, Stroke::new(1.5, col(e.text_color).gamma_multiply(0.6)));
    let Some(scene) = d.scene else {
        p.circle_filled(c, 4.0, col(e.color));
        return;
    };
    let me = if e.source.is_empty() {
        scene.entities.iter().find(|x| x.tag == "player").map(|x| x.id)
    } else {
        scene.find_by_name(&e.source)
    };
    let Some(me) = me else {
        if d.preview {
            p.circle_filled(c, 4.0, col(e.color));
        }
        return;
    };
    let center = scene.world_matrix(me).w_axis.truncate();
    let range = e.range.max(1.0);
    for x in &scene.entities {
        if x.id == me || !x.visible || x.tag.is_empty() {
            continue;
        }
        let dot = match x.tag.as_str() {
            "enemy" => Color32::from_rgb(230, 60, 50),
            "pickup" | "item" => Color32::from_rgb(240, 200, 80),
            "ally" | "npc" => Color32::from_rgb(90, 200, 120),
            _ => continue,
        };
        let w = scene.world_matrix(x.id).w_axis.truncate() - center;
        let v = Vec2::new(w.x, w.z) / range * rad;
        if v.length() < rad - 3.0 {
            p.circle_filled(c + v, 3.2, dot);
        }
    }
    // the followed entity, pointing where it faces
    let f = scene.get(me).map(|m| m.forward()).unwrap_or(glam::Vec3::NEG_Z);
    let dir = Vec2::new(f.x, f.z).normalized();
    let side = Vec2::new(-dir.y, dir.x);
    let tip = c + dir * 8.0;
    p.add(egui::Shape::convex_polygon(vec![tip, c - dir * 5.0 + side * 5.0, c - dir * 2.0, c - dir * 5.0 - side * 5.0], col(e.color), Stroke::new(1.0, Color32::BLACK)));
}

/// The form of one element in the HUD editor.
pub fn element_form(ui: &mut egui::Ui, e: &mut Element, images: &[String], entities: &[String], values: &[String]) {
    use eframe::egui::RichText;
    egui::Grid::new("hud_el").num_columns(2).spacing([8.0, 5.0]).show(ui, |ui| {
        ui.label("id");
        ui.text_edit_singleline(&mut e.id);
        ui.end_row();
        ui.label(tr!("Kind", "Tipo"));
        let kinds = kinds();
        egui::ComboBox::from_id_salt("hud_kind").width(220.0).selected_text(kinds.iter().find(|k| k.0 == e.kind).map(|k| k.1.clone()).unwrap_or(e.kind.clone())).show_ui(ui, |ui| {
            for (code, name) in &kinds {
                ui.selectable_value(&mut e.kind, code.to_string(), name);
            }
        });
        ui.end_row();
        ui.label(tr!("Shown", "Visível"));
        ui.checkbox(&mut e.visible, RichText::new(tr!("at the start (scripts: hud.show(id, true))", "no início (scripts: hud.show(id, true))")).small());
        ui.end_row();
        ui.label(tr!("Anchor", "Âncora"));
        egui::Grid::new("hud_anchor").spacing([2.0, 2.0]).show(ui, |ui| {
            for (i, a) in ANCHORS.iter().enumerate() {
                if ui.add_sized([26.0, 18.0], egui::Button::new("").selected(e.anchor == *a).min_size(egui::vec2(26.0, 18.0))).on_hover_text(*a).clicked() && e.anchor != *a {
                    e.anchor = a.to_string();
                    e.x = if a.ends_with("right") { -16.0 } else if a.ends_with("left") { 16.0 } else { 0.0 };
                    e.y = if a.starts_with("bottom") { -16.0 } else if a.starts_with("top") { 16.0 } else { 0.0 };
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        ui.end_row();
        ui.label(tr!("Position", "Posição"));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut e.x).prefix("x ").speed(1.0));
            ui.add(egui::DragValue::new(&mut e.y).prefix("y ").speed(1.0));
        });
        ui.end_row();
        match e.kind.as_str() {
            "text" => {
                ui.label(tr!("Text", "Texto"));
                ui.add(egui::TextEdit::multiline(&mut e.source).desired_rows(2).desired_width(220.0).hint_text("Potions: {potions}"));
                ui.end_row();
            }
            "bar" => {
                ui.label(tr!("Value", "Valor"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut e.source).desired_width(120.0).hint_text("Health"));
                    egui::ComboBox::from_id_salt("hud_vals").width(24.0).selected_text("").show_ui(ui, |ui| {
                        for v in values {
                            ui.selectable_value(&mut e.source, v.clone(), v);
                        }
                    });
                });
                ui.end_row();
                ui.label(tr!("Label", "Rótulo"));
                ui.text_edit_singleline(&mut e.label);
                ui.end_row();
            }
            "image" => {
                ui.label(tr!("Picture", "Imagem"));
                egui::ComboBox::from_id_salt("hud_img").width(220.0).selected_text(if e.source.is_empty() { tr!("(none)", "(nenhuma)") } else { e.source.clone() }).show_ui(ui, |ui| {
                    for f in images {
                        ui.selectable_value(&mut e.source, f.clone(), f);
                    }
                });
                ui.end_row();
            }
            "minimap" => {
                ui.label(tr!("Follows", "Segue"));
                egui::ComboBox::from_id_salt("hud_follow").width(220.0).selected_text(if e.source.is_empty() { tr!("(tagged \"player\")", "(tag \"player\")") } else { e.source.clone() }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut e.source, String::new(), tr!("(tagged \"player\")", "(tag \"player\")"));
                    for f in entities {
                        ui.selectable_value(&mut e.source, f.clone(), f);
                    }
                });
                ui.end_row();
                ui.label(tr!("Range (m)", "Alcance (m)"));
                ui.add(egui::Slider::new(&mut e.range, 5.0..=300.0));
                ui.end_row();
            }
            _ => {}
        }
        if !matches!(e.kind.as_str(), "text") {
            ui.label(tr!("Size", "Tamanho"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut e.w).prefix(if e.kind == "skills" { tr!("slot ", "espaço ") } else { "w ".into() }).speed(1.0).range(2.0..=1600.0));
                if !matches!(e.kind.as_str(), "skills" | "minimap") {
                    ui.add(egui::DragValue::new(&mut e.h).prefix("h ").speed(0.5).range(1.0..=900.0));
                }
            });
            ui.end_row();
        }
        if !matches!(e.kind.as_str(), "image" | "skills" | "crosshair") || e.kind == "crosshair" {
            ui.label(tr!("Colours", "Cores"));
            ui.horizontal(|ui| {
                for (c, tip) in [(&mut e.color, tr!("fill", "preenchimento")), (&mut e.back, tr!("background", "fundo")), (&mut e.text_color, tr!("text", "texto"))] {
                    let mut c32 = col(*c);
                    if ui.color_edit_button_srgba(&mut c32).on_hover_text(tip).changed() {
                        *c = c32.to_srgba_unmultiplied();
                    }
                }
            });
            ui.end_row();
        }
        if matches!(e.kind.as_str(), "text" | "bar" | "bars" | "boss" | "xp") {
            ui.label(tr!("Text size", "Tamanho do texto"));
            ui.add(egui::Slider::new(&mut e.text_size, 8.0..=72.0).fixed_decimals(0));
            ui.end_row();
            ui.label(tr!("Font", "Fonte"));
            ui.horizontal(|ui| {
                for (code, name) in [("default", tr!("Default", "Padrão")), ("title", tr!("Menu title", "Título do menu")), ("text", tr!("Menu text", "Texto do menu"))] {
                    ui.selectable_value(&mut e.font, code.to_string(), name);
                }
            });
            ui.end_row();
            ui.label(tr!("Shadow", "Sombra"));
            ui.checkbox(&mut e.shadow, "");
            ui.end_row();
        }
        if matches!(e.kind.as_str(), "bar" | "bars" | "boss" | "xp" | "text") {
            ui.label(tr!("Corners", "Cantos"));
            ui.add(egui::Slider::new(&mut e.rounding, 0.0..=12.0));
            ui.end_row();
        }
    });
}

/// The preview in the HUD editor: elements can be picked and dragged.
pub fn preview(ui: &mut egui::Ui, hud: &mut Hud, selected: &mut usize, d: &Data, background: Option<egui::TextureId>) -> egui::Response {
    let w = ui.available_width().max(260.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, w * 9.0 / 16.0), egui::Sense::click_and_drag());
    let p = ui.painter_at(rect);
    match background {
        Some(tex) => {
            p.image(tex, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::from_gray(200));
        }
        None => {
            p.rect_filled(rect, 0.0, Color32::from_rgb(52, 60, 72));
            p.rect_filled(Rect::from_min_max(Pos2::new(rect.left(), rect.center().y), rect.max), 0.0, Color32::from_rgb(58, 70, 50));
        }
    }
    let rects = draw(&p, rect, hud, d);
    let k = scale(rect);
    // pick with a click; drag to move
    if resp.drag_started() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            if let Some(i) = rects.iter().enumerate().rev().find(|(_, r)| r.expand(4.0).contains(pos)).map(|(i, _)| i) {
                *selected = i;
            }
        }
    }
    if resp.dragged() {
        if let Some(e) = hud.elements.get_mut(*selected) {
            let dlt = resp.drag_delta() / k;
            e.x = (e.x + dlt.x).round();
            e.y = (e.y + dlt.y).round();
        }
    }
    if let Some(r) = rects.get(*selected) {
        if r.is_positive() {
            p.rect_stroke(r.expand(3.0), 2.0, Stroke::new(1.5, Color32::from_rgb(255, 200, 60)), egui::StrokeKind::Outside);
        }
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_fill_in_values_and_elements_hang_from_their_anchor() {
        let mut values = BTreeMap::new();
        values.insert("potions".to_string(), (3.0, 5.0));
        let images = HashMap::new();
        let d = Data {
            text: "Goal",
            values: &values,
            bars: &[],
            feed: &[],
            now: 0.0,
            hidden: &[],
            crosshair: false,
            rpg: None,
            scene: None,
            images: &images,
            title_font: FontFamily::Proportional,
            text_font: FontFamily::Proportional,
            skin: None,
            preview: false,
        };
        assert_eq!(fill("{hud} / Potions: {potions} {nothing}{", &d), "Goal / Potions: 3 {");
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 900.0));
        let e = Element { anchor: "bottom_right".into(), x: -20.0, y: -10.0, ..Default::default() };
        let r = place(screen, &e, Vec2::new(100.0, 20.0));
        assert_eq!(r.max, Pos2::new(1580.0, 890.0));
        // twice the screen, twice the offsets
        let big = Rect::from_min_size(Pos2::ZERO, Vec2::new(3200.0, 1800.0));
        assert_eq!(place(big, &e, Vec2::new(100.0, 20.0)).max, Pos2::new(3160.0, 1780.0));
        // the classic HUD survives a round trip through the file format
        let h = Hud::default();
        let back: Hud = serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
        assert_eq!(h, back);
    }
}
