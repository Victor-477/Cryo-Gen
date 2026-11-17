//! Role-playing systems a game can switch on: an inventory with equipment, levels and
//! experience, skills on a bar with cooldowns, and a talent tree.
//!
//! The game's data lives in `rpg.json` (Game > RPG in the editor): the items, skills and
//! talents that exist, how levels grow and which keys open the screens. What the player has
//! (level, bag, equipment, talent ranks, skills on the bar) is an [`RpgState`], kept by the
//! runtime while the game runs. Scripts reach it through `inventory`, `level`, `skills` and
//! `talents`, and hear about changes with events (`on level_up(level)`, `on skill(id)`...).

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub const FILE: &str = "rpg.json";
/// Where an item can be worn.
pub const EQUIP_SLOTS: [&str; 3] = ["weapon", "armor", "trinket"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemDef {
    pub id: String,
    pub name: String,
    pub description: String,
    /// A picture in the project (an icon); without one the item shows as its colour and initial.
    pub icon: String,
    pub color: [u8; 3],
    /// How many fit in one bag slot.
    pub stack: u32,
    /// "" for things that are carried or used; "weapon", "armor" or "trinket" to wear it.
    pub slot: String,
    /// Used up when used (a potion). Scripts hear `on item_used(id)`.
    pub consumable: bool,
    /// What wearing it adds: {"damage": 6, "armor": 4}.
    pub stats: BTreeMap<String, f64>,
}

impl Default for ItemDef {
    fn default() -> Self {
        ItemDef {
            id: "item".into(),
            name: "Item".into(),
            description: String::new(),
            icon: String::new(),
            color: [170, 150, 110],
            stack: 1,
            slot: String::new(),
            consumable: false,
            stats: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub color: [u8; 3],
    /// Seconds before it can be used again.
    pub cooldown: f32,
    /// Shown in the tooltip ("25 stamina"); the script decides what it really costs.
    pub cost: String,
}

impl Default for SkillDef {
    fn default() -> Self {
        SkillDef { id: "skill".into(), name: "Skill".into(), description: String::new(), icon: String::new(), color: [200, 120, 60], cooldown: 6.0, cost: String::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Talent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub color: [u8; 3],
    /// Which column of the tree (index in `branches`) and which row (0 at the top).
    pub branch: usize,
    pub tier: u32,
    pub max_rank: u32,
    /// Talents that need at least one rank first.
    pub requires: Vec<String>,
    /// The player's level needed.
    pub level: u32,
    /// What each rank adds: {"damage": 0.05}.
    pub stats: BTreeMap<String, f64>,
    /// A skill learned with the first rank.
    pub skill: String,
}

impl Default for Talent {
    fn default() -> Self {
        Talent {
            id: "talent".into(),
            name: "Talent".into(),
            description: String::new(),
            icon: String::new(),
            color: [120, 160, 220],
            branch: 0,
            tier: 0,
            max_rank: 1,
            requires: vec![],
            level: 1,
            stats: BTreeMap::new(),
            skill: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Leveling {
    pub max_level: u32,
    /// Experience from level 1 to 2; each next level needs `growth` times more.
    pub xp_first: f64,
    pub growth: f64,
    /// Talent points given at each new level.
    pub points_per_level: u32,
    /// What each level past the first adds: {"max_health": 10}.
    pub stats_per_level: BTreeMap<String, f64>,
}

impl Default for Leveling {
    fn default() -> Self {
        Leveling { max_level: 20, xp_first: 100.0, growth: 1.35, points_per_level: 1, stats_per_level: BTreeMap::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stack {
    pub id: String,
    pub count: u32,
}

impl Default for Stack {
    fn default() -> Self {
        Stack { id: String::new(), count: 1 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RpgScreens {
    /// Keys that open the inventory and the talent tree while playing.
    pub inventory_key: String,
    pub talents_key: String,
    /// Bag size.
    pub slots: u32,
    /// The skill bar: one key per slot.
    pub bar_keys: Vec<String>,
    pub sound_open: String,
    pub sound_item: String,
    pub sound_talent: String,
    pub sound_level: String,
}

impl Default for RpgScreens {
    fn default() -> Self {
        RpgScreens {
            inventory_key: "i".into(),
            talents_key: "k".into(),
            slots: 24,
            bar_keys: vec!["1".into(), "2".into(), "3".into(), "4".into()],
            sound_open: String::new(),
            sound_item: String::new(),
            sound_talent: String::new(),
            sound_level: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rpg {
    pub enabled: bool,
    pub items: Vec<ItemDef>,
    pub skills: Vec<SkillDef>,
    pub talents: Vec<Talent>,
    /// Names of the talent tree's columns.
    pub branches: Vec<String>,
    pub levels: Leveling,
    pub screens: RpgScreens,
    /// What a new game starts with.
    pub start_items: Vec<Stack>,
    pub start_skills: Vec<String>,
}

impl Default for Rpg {
    fn default() -> Self {
        Rpg {
            enabled: false,
            items: vec![],
            skills: vec![],
            talents: vec![],
            branches: vec!["Offense".into(), "Defense".into(), "Utility".into()],
            levels: Leveling::default(),
            screens: RpgScreens::default(),
            start_items: vec![],
            start_skills: vec![],
        }
    }
}

impl Rpg {
    pub fn load(project_dir: &Path) -> Rpg {
        std::fs::read_to_string(project_dir.join(FILE)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, project_dir: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(project_dir.join(FILE), text).map_err(|e| format!("{FILE}: {e}"))
    }

    pub fn item(&self, id: &str) -> Option<&ItemDef> {
        self.items.iter().find(|i| i.id == id)
    }
    pub fn skill(&self, id: &str) -> Option<&SkillDef> {
        self.skills.iter().find(|i| i.id == id)
    }
    pub fn talent(&self, id: &str) -> Option<&Talent> {
        self.talents.iter().find(|i| i.id == id)
    }

    /// Experience needed to go from `level` to the next.
    pub fn xp_for(&self, level: u32) -> f64 {
        (self.levels.xp_first.max(1.0) * self.levels.growth.max(1.0).powi(level.saturating_sub(1) as i32)).round()
    }

    /// Every icon the screens may show.
    pub fn icons(&self) -> Vec<String> {
        let mut v: Vec<String> = self.items.iter().map(|i| i.icon.clone()).chain(self.skills.iter().map(|s| s.icon.clone())).chain(self.talents.iter().map(|t| t.icon.clone())).filter(|i| !i.is_empty()).collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Something that happened to the player, told to every script as an event.
#[derive(Clone, Debug, PartialEq)]
pub enum RpgEvent {
    LevelUp(u32),
    ItemAdded(String, u32),
    ItemUsed(String),
    Equipped(String, String),
    Unequipped(String, String),
    TalentLearned(String, u32),
    SkillLearned(String),
    /// A skill on the bar was pressed and was ready (its cooldown has started).
    Skill(String),
}

impl RpgEvent {
    /// The script event and its arguments.
    pub fn event(&self) -> (&'static str, Vec<cryo_lang::Value>) {
        use cryo_lang::Value as V;
        match self {
            RpgEvent::LevelUp(l) => ("level_up", vec![V::Num(*l as f64)]),
            RpgEvent::ItemAdded(id, n) => ("item_added", vec![V::text(id), V::Num(*n as f64)]),
            RpgEvent::ItemUsed(id) => ("item_used", vec![V::text(id)]),
            RpgEvent::Equipped(slot, id) => ("equipped", vec![V::text(slot), V::text(id)]),
            RpgEvent::Unequipped(slot, id) => ("unequipped", vec![V::text(slot), V::text(id)]),
            RpgEvent::TalentLearned(id, r) => ("talent_learned", vec![V::text(id), V::Num(*r as f64)]),
            RpgEvent::SkillLearned(id) => ("skill_learned", vec![V::text(id)]),
            RpgEvent::Skill(id) => ("skill", vec![V::text(id)]),
        }
    }
}

/// What the player has: kept while the game runs, carried to the next level with
/// `scene.load`, and saved with `level.save()`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RpgState {
    pub level: u32,
    pub xp: f64,
    pub points: u32,
    pub bag: Vec<Option<Stack>>,
    pub equipped: BTreeMap<String, String>,
    pub ranks: BTreeMap<String, u32>,
    pub known: Vec<String>,
    /// Skill ids on the bar ("" for an empty slot).
    pub bar: Vec<String>,
    /// Seconds left before each skill is ready.
    pub cooldowns: BTreeMap<String, f32>,
}

impl RpgState {
    pub fn new(rpg: &Rpg) -> RpgState {
        let mut s = RpgState { level: 1, bag: vec![None; rpg.screens.slots.clamp(1, 120) as usize], bar: vec![String::new(); rpg.screens.bar_keys.len()], ..Default::default() };
        for st in &rpg.start_items {
            s.add_item(rpg, &st.id, st.count);
        }
        for id in &rpg.start_skills {
            s.learn_skill(rpg, id);
        }
        s
    }

    /// Experience still needed for the next level (0 at the top level).
    pub fn xp_next(&self, rpg: &Rpg) -> f64 {
        if self.level >= rpg.levels.max_level {
            0.0
        } else {
            rpg.xp_for(self.level)
        }
    }

    pub fn add_xp(&mut self, rpg: &Rpg, amount: f64) -> Vec<RpgEvent> {
        let mut out = vec![];
        if amount <= 0.0 || self.level >= rpg.levels.max_level {
            return out;
        }
        self.xp += amount;
        while self.level < rpg.levels.max_level && self.xp >= rpg.xp_for(self.level) {
            self.xp -= rpg.xp_for(self.level);
            self.level += 1;
            self.points += rpg.levels.points_per_level;
            out.push(RpgEvent::LevelUp(self.level));
        }
        if self.level >= rpg.levels.max_level {
            self.xp = 0.0;
        }
        out
    }

    pub fn count(&self, id: &str) -> u32 {
        self.bag.iter().flatten().filter(|s| s.id == id).map(|s| s.count).sum::<u32>() + self.equipped.values().filter(|e| *e == id).count() as u32
    }

    /// Puts items in the bag (filling stacks first); returns how many fit.
    pub fn add_item(&mut self, rpg: &Rpg, id: &str, count: u32) -> u32 {
        let stack = rpg.item(id).map(|i| i.stack.max(1)).unwrap_or(1);
        let mut left = count;
        for s in self.bag.iter_mut().flatten().filter(|s| s.id == id) {
            let room = stack.saturating_sub(s.count).min(left);
            s.count += room;
            left -= room;
        }
        for slot in self.bag.iter_mut() {
            if left == 0 {
                break;
            }
            if slot.is_none() {
                let n = left.min(stack);
                *slot = Some(Stack { id: id.to_string(), count: n });
                left -= n;
            }
        }
        count - left
    }

    /// Takes items out of the bag; returns how many there were to take.
    pub fn remove_item(&mut self, id: &str, count: u32) -> u32 {
        let mut left = count;
        for slot in self.bag.iter_mut().rev() {
            if left == 0 {
                break;
            }
            if let Some(s) = slot {
                if s.id == id {
                    let n = s.count.min(left);
                    s.count -= n;
                    left -= n;
                    if s.count == 0 {
                        *slot = None;
                    }
                }
            }
        }
        count - left
    }

    /// Uses what is in a bag slot: drinks a potion, wears a sword...
    pub fn use_slot(&mut self, rpg: &Rpg, index: usize) -> Vec<RpgEvent> {
        let Some(Some(stack)) = self.bag.get(index).cloned() else { return vec![] };
        let Some(def) = rpg.item(&stack.id) else { return vec![] };
        if !def.slot.is_empty() {
            return self.equip(rpg, &def.id);
        }
        if def.consumable {
            self.remove_item(&def.id, 1);
        }
        vec![RpgEvent::ItemUsed(def.id.clone())]
    }

    pub fn use_item(&mut self, rpg: &Rpg, id: &str) -> Vec<RpgEvent> {
        match self.bag.iter().position(|s| s.as_ref().map(|s| s.id == id).unwrap_or(false)) {
            Some(i) => self.use_slot(rpg, i),
            None => vec![],
        }
    }

    pub fn equip(&mut self, rpg: &Rpg, id: &str) -> Vec<RpgEvent> {
        let Some(def) = rpg.item(id) else { return vec![] };
        if def.slot.is_empty() || self.remove_item(id, 1) == 0 {
            return vec![];
        }
        let mut out = vec![];
        if let Some(old) = self.equipped.insert(def.slot.clone(), id.to_string()) {
            self.add_item(rpg, &old, 1);
            out.push(RpgEvent::Unequipped(def.slot.clone(), old));
        }
        out.push(RpgEvent::Equipped(def.slot.clone(), id.to_string()));
        out
    }

    pub fn unequip(&mut self, rpg: &Rpg, slot: &str) -> Vec<RpgEvent> {
        let Some(id) = self.equipped.get(slot).cloned() else { return vec![] };
        if self.bag.iter().all(|s| s.is_some()) {
            return vec![]; // no room in the bag
        }
        self.equipped.remove(slot);
        self.add_item(rpg, &id, 1);
        vec![RpgEvent::Unequipped(slot.to_string(), id)]
    }

    pub fn rank(&self, id: &str) -> u32 {
        self.ranks.get(id).copied().unwrap_or(0)
    }

    /// Why a talent cannot take one more rank now, if it cannot.
    pub fn blocked(&self, rpg: &Rpg, id: &str) -> Option<String> {
        let Some(t) = rpg.talent(id) else { return Some(tr!("unknown talent", "talento desconhecido")) };
        if self.rank(id) >= t.max_rank.max(1) {
            return Some(tr!("maxed", "no máximo"));
        }
        if self.level < t.level {
            return Some(tr!("needs level {}", "precisa do nível {}", t.level));
        }
        for r in &t.requires {
            if self.rank(r) == 0 {
                let name = rpg.talent(r).map(|t| t.name.clone()).unwrap_or(r.clone());
                return Some(tr!("needs {name}", "precisa de {name}"));
            }
        }
        if self.points == 0 {
            return Some(tr!("no points", "sem pontos"));
        }
        None
    }

    pub fn learn_talent(&mut self, rpg: &Rpg, id: &str) -> Vec<RpgEvent> {
        if self.blocked(rpg, id).is_some() {
            return vec![];
        }
        let t = rpg.talent(id).unwrap();
        self.points -= 1;
        let r = self.rank(id) + 1;
        self.ranks.insert(id.to_string(), r);
        let mut out = vec![RpgEvent::TalentLearned(id.to_string(), r)];
        if r == 1 && !t.skill.is_empty() {
            out.extend(self.learn_skill(rpg, &t.skill.clone()));
        }
        out
    }

    pub fn learn_skill(&mut self, rpg: &Rpg, id: &str) -> Vec<RpgEvent> {
        if rpg.skill(id).is_none() || self.known.iter().any(|k| k == id) {
            return vec![];
        }
        self.known.push(id.to_string());
        if !self.bar.iter().any(|b| b == id) {
            if let Some(free) = self.bar.iter_mut().find(|b| b.is_empty()) {
                *free = id.to_string();
            }
        }
        vec![RpgEvent::SkillLearned(id.to_string())]
    }

    /// Puts a known skill on a bar slot (moving it if it was elsewhere).
    pub fn assign(&mut self, slot: usize, id: &str) {
        if slot >= self.bar.len() || (!id.is_empty() && !self.known.iter().any(|k| k == id)) {
            return;
        }
        for b in self.bar.iter_mut() {
            if b == id {
                b.clear();
            }
        }
        self.bar[slot] = id.to_string();
    }

    pub fn ready(&self, id: &str) -> bool {
        self.known.iter().any(|k| k == id) && self.cooldowns.get(id).copied().unwrap_or(0.0) <= 0.0
    }

    /// Uses a skill if it is ready: starts its cooldown.
    pub fn use_skill(&mut self, rpg: &Rpg, id: &str) -> Option<RpgEvent> {
        if !self.ready(id) {
            return None;
        }
        let cd = rpg.skill(id).map(|s| s.cooldown).unwrap_or(0.0);
        if cd > 0.0 {
            self.cooldowns.insert(id.to_string(), cd);
        }
        Some(RpgEvent::Skill(id.to_string()))
    }

    pub fn press_slot(&mut self, rpg: &Rpg, slot: usize) -> Option<RpgEvent> {
        let id = self.bar.get(slot)?.clone();
        if id.is_empty() {
            return None;
        }
        self.use_skill(rpg, &id)
    }

    pub fn tick(&mut self, dt: f32) {
        for v in self.cooldowns.values_mut() {
            *v -= dt;
        }
        self.cooldowns.retain(|_, v| *v > 0.0);
    }

    /// A stat from levels, talents and what is worn: `stat("damage")`.
    pub fn stat(&self, rpg: &Rpg, name: &str) -> f64 {
        let mut v = rpg.levels.stats_per_level.get(name).copied().unwrap_or(0.0) * self.level.saturating_sub(1) as f64;
        for (id, r) in &self.ranks {
            if let Some(t) = rpg.talent(id) {
                v += t.stats.get(name).copied().unwrap_or(0.0) * *r as f64;
            }
        }
        for id in self.equipped.values() {
            if let Some(i) = rpg.item(id) {
                v += i.stats.get(name).copied().unwrap_or(0.0);
            }
        }
        v
    }

    /// All the stats anything gives, with their totals, for the character sheet.
    pub fn stats(&self, rpg: &Rpg) -> Vec<(String, f64)> {
        let mut names: Vec<String> = rpg.levels.stats_per_level.keys().cloned().collect();
        names.extend(rpg.talents.iter().flat_map(|t| t.stats.keys().cloned()));
        names.extend(rpg.items.iter().flat_map(|t| t.stats.keys().cloned()));
        names.sort();
        names.dedup();
        names.into_iter().map(|n| (n.clone(), self.stat(rpg, &n))).filter(|(_, v)| v.abs() > 1e-9).collect()
    }
}

// ---------------------------------------------------------------------------------------
// Drawing: the screens and the skill bar, in the colours of the game's menus.

/// What the screens need to look like the game: colours from the menus and fonts.
pub struct Skin {
    pub accent: Color32,
    pub text: Color32,
    pub panel: Color32,
    pub slot: Color32,
    pub title_font: egui::FontFamily,
    pub text_font: egui::FontFamily,
    pub icons: HashMap<String, egui::TextureId>,
}

impl Skin {
    pub fn from_menus(m: &crate::menus::Menus, look: &crate::menus::Look, icons: HashMap<String, egui::TextureId>) -> Skin {
        let c = crate::menus::color;
        Skin {
            accent: c(m.style.accent),
            text: c(m.style.text),
            panel: c([m.style.panel[0], m.style.panel[1], m.style.panel[2], m.style.panel[3].max(225)]),
            slot: c([m.style.button[0], m.style.button[1], m.style.button[2], 235]),
            title_font: look.title_font.clone(),
            text_font: look.text_font.clone(),
            icons,
        }
    }
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// An icon: the picture, or a tinted tile with the name's initial.
pub fn draw_icon(p: &egui::Painter, r: Rect, icon: &str, color: [u8; 3], name: &str, skin: &Skin) {
    match skin.icons.get(icon) {
        Some(tex) => {
            p.image(*tex, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            let c = rgb(color);
            p.rect_filled(r.shrink(r.width() * 0.08), r.width() * 0.12, c.gamma_multiply(0.55));
            p.rect_stroke(r.shrink(r.width() * 0.08), r.width() * 0.12, Stroke::new(1.5, c), egui::StrokeKind::Inside);
            let initial: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            p.text(r.center(), Align2::CENTER_CENTER, initial, FontId::new(r.height() * 0.5, skin.title_font.clone()), Color32::WHITE);
        }
    }
}

fn stat_label(name: &str) -> String {
    let mut s = name.replace('_', " ");
    if let Some(f) = s.get_mut(0..1) {
        f.make_ascii_uppercase();
    }
    s
}

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-6 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
}

fn stats_text(stats: &BTreeMap<String, f64>, per_rank: bool) -> String {
    stats
        .iter()
        .map(|(k, v)| format!("{}{} {}{}", if *v >= 0.0 { "+" } else { "" }, fmt_num(*v), stat_label(k), if per_rank { tr!(" per rank", " por nível") } else { String::new() }))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The keys and button names the bar shows.
fn key_name(k: &str) -> String {
    crate::input::label(k)
}

/// The skill bar: square slots with icons, key names and the cooldown sweeping down.
pub fn draw_skill_bar(p: &egui::Painter, center_bottom: Pos2, size: f32, rpg: &Rpg, st: &RpgState, skin: &Skin) -> Rect {
    let n = st.bar.len().max(1);
    let gap = size * 0.12;
    let w = n as f32 * size + (n - 1) as f32 * gap;
    let all = Rect::from_min_size(Pos2::new(center_bottom.x - w / 2.0, center_bottom.y - size), Vec2::new(w, size));
    for (i, id) in st.bar.iter().enumerate() {
        let r = Rect::from_min_size(all.min + Vec2::new(i as f32 * (size + gap), 0.0), Vec2::splat(size));
        p.rect_filled(r.expand(2.0), 6.0, Color32::from_black_alpha(170));
        p.rect_filled(r, 5.0, skin.slot);
        if let Some(s) = rpg.skill(id) {
            draw_icon(p, r.shrink(3.0), &s.icon, s.color, &s.name, skin);
            let left = st.cooldowns.get(id).copied().unwrap_or(0.0);
            if left > 0.0 && s.cooldown > 0.0 {
                let f = (left / s.cooldown).clamp(0.0, 1.0);
                let mut shade = r.shrink(3.0);
                shade.min.y = shade.max.y - shade.height() * f;
                p.rect_filled(shade, 3.0, Color32::from_black_alpha(170));
                p.text(r.center(), Align2::CENTER_CENTER, format!("{:.0}", left.ceil()), FontId::new(size * 0.38, skin.title_font.clone()), Color32::WHITE);
            } else {
                p.rect_stroke(r, 5.0, Stroke::new(1.5, skin.accent.gamma_multiply(0.8)), egui::StrokeKind::Inside);
            }
        }
        if let Some(k) = rpg.screens.bar_keys.get(i) {
            let kp = r.left_top() + Vec2::new(4.0, 2.0);
            p.text(kp + Vec2::new(1.0, 1.0), Align2::LEFT_TOP, key_name(k), FontId::new(size * 0.24, skin.text_font.clone()), Color32::BLACK);
            p.text(kp, Align2::LEFT_TOP, key_name(k), FontId::new(size * 0.24, skin.text_font.clone()), Color32::WHITE);
        }
    }
    all
}

/// The level and experience: a thin bar with "Level 3" over it.
pub fn draw_xp_bar(p: &egui::Painter, r: Rect, rpg: &Rpg, st: &RpgState, skin: &Skin, fill: Color32, text_size: f32) {
    let show_text = text_size > 0.0;
    let next = st.xp_next(rpg);
    let f = if next > 0.0 { (st.xp / next).clamp(0.0, 1.0) as f32 } else { 1.0 };
    p.rect_filled(r.expand(2.0), 3.0, Color32::from_black_alpha(180));
    let mut fr = r;
    fr.set_width(r.width() * f);
    p.rect_filled(fr, 2.0, fill);
    if show_text {
        let text = if next > 0.0 {
            tr!("Level {}  ·  {} / {} XP", "Nível {}  ·  {} / {} XP", st.level, fmt_num(st.xp.floor()), fmt_num(next))
        } else {
            tr!("Level {} (max)", "Nível {} (máximo)", st.level)
        };
        let tp = r.center_top() + Vec2::new(0.0, -3.0);
        let font = FontId::new(text_size, skin.text_font.clone());
        p.text(tp + Vec2::new(1.0, 1.0), Align2::CENTER_BOTTOM, &text, font.clone(), Color32::BLACK);
        p.text(tp, Align2::CENTER_BOTTOM, text, font, skin.text);
    }
    if st.points > 0 {
        let label = tr!("{} talent point(s)", "{} ponto(s) de talento", st.points);
        let font = FontId::new(text_size * 0.9, skin.text_font.clone());
        p.text(r.right_top() + Vec2::new(0.0, -3.0), Align2::RIGHT_BOTTOM, label, font, skin.accent);
    }
}

/// Which RPG screen is open in the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Inventory,
    Talents,
}

/// What the player did on a screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    UseSlot(usize),
    Unequip(String),
    Learn(String),
    Assign(usize, String),
    Switch(Page),
    Close,
}

/// Keyboard and gamepad steps for the screens.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Steps {
    pub dx: i32,
    pub dy: i32,
    pub choose: bool,
}

fn tooltip(ui: &egui::Ui, at: Rect, title: &str, sub: &str, body: &str, skin: &Skin) {
    let k = crate::menus::scale(ui.clip_rect()).min(ui.clip_rect().height() / 900.0 * 1.6).max(0.5);
    let p = ui.painter().clone().with_layer_id(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("rpg_tip")));
    let width = 260.0 * k;
    let font = FontId::new(14.0 * k, skin.text_font.clone());
    let galley = p.layout(body.to_string(), font, skin.text, width - 20.0 * k);
    let h = 52.0 * k + galley.size().y;
    let screen = ui.clip_rect();
    let mut pos = at.right_top() + Vec2::new(10.0 * k, 0.0);
    if pos.x + width > screen.right() {
        pos.x = at.left() - width - 10.0 * k;
    }
    pos.y = pos.y.min(screen.bottom() - h - 6.0);
    let r = Rect::from_min_size(pos, Vec2::new(width, h));
    p.rect_filled(r, 6.0, Color32::from_rgba_unmultiplied(12, 12, 14, 245));
    p.rect_stroke(r, 6.0, Stroke::new(1.0, skin.accent.gamma_multiply(0.7)), egui::StrokeKind::Inside);
    p.text(r.min + Vec2::new(10.0, 8.0) * k, Align2::LEFT_TOP, title, FontId::new(18.0 * k, skin.title_font.clone()), skin.accent);
    p.text(r.min + Vec2::new(10.0, 30.0) * k, Align2::LEFT_TOP, sub, FontId::new(12.0 * k, skin.text_font.clone()), skin.text.gamma_multiply(0.7));
    p.galley(r.min + Vec2::new(10.0, 46.0) * k, galley, skin.text);
}

fn header(ui: &mut egui::Ui, frame: Rect, page: Page, st: &RpgState, rpg: &Rpg, skin: &Skin, k: f32, out: &mut Vec<Act>) {
    let p = ui.painter().clone();
    let tabs = [(Page::Inventory, tr!("Inventory", "Inventário"), &rpg.screens.inventory_key), (Page::Talents, tr!("Talents", "Talentos"), &rpg.screens.talents_key)];
    let mut x = frame.left() + 24.0 * k;
    for (pg, name, key) in tabs {
        let text = format!("{name}  [{}]", key_name(key));
        let font = FontId::new(24.0 * k, skin.title_font.clone());
        let g = p.layout_no_wrap(text, font, if pg == page { skin.accent } else { skin.text.gamma_multiply(0.55) });
        let r = Rect::from_min_size(Pos2::new(x, frame.top() + 16.0 * k), g.size());
        let resp = ui.interact(r, ui.id().with(("rpg_tab", name.as_str())), egui::Sense::click());
        if resp.clicked() && pg != page {
            out.push(Act::Switch(pg));
        }
        p.galley(r.min, g, Color32::WHITE);
        if pg == page {
            p.line_segment([r.left_bottom() + Vec2::new(0.0, 3.0), r.right_bottom() + Vec2::new(0.0, 3.0)], Stroke::new(2.0, skin.accent));
        }
        x = r.right() + 28.0 * k;
    }
    // close
    let cr = Rect::from_min_size(Pos2::new(frame.right() - 40.0 * k, frame.top() + 14.0 * k), Vec2::splat(26.0 * k));
    let resp = ui.interact(cr, ui.id().with("rpg_close"), egui::Sense::click());
    let c = if resp.hovered() { skin.accent } else { skin.text };
    let d = 7.0 * k;
    p.line_segment([cr.center() - Vec2::splat(d), cr.center() + Vec2::splat(d)], Stroke::new(2.0, c));
    p.line_segment([cr.center() + Vec2::new(-d, d), cr.center() + Vec2::new(d, -d)], Stroke::new(2.0, c));
    if resp.clicked() {
        out.push(Act::Close);
    }
    // level line
    let next = st.xp_next(rpg);
    let line = if next > 0.0 {
        tr!("Level {}   {} / {} XP", "Nível {}   {} / {} XP", st.level, fmt_num(st.xp.floor()), fmt_num(next))
    } else {
        tr!("Level {} (max)", "Nível {} (máximo)", st.level)
    };
    p.text(Pos2::new(frame.right() - 60.0 * k, frame.top() + 26.0 * k), Align2::RIGHT_CENTER, line, FontId::new(15.0 * k, skin.text_font.clone()), skin.text);
}

/// The inventory: the bag, what is worn, the stats and the skill bar.
pub fn inventory_screen(ui: &mut egui::Ui, rect: Rect, rpg: &Rpg, st: &RpgState, skin: &Skin, focus: &mut usize, picked_skill: &mut Option<String>, steps: Steps) -> Vec<Act> {
    let mut out = vec![];
    let k = crate::menus::scale(rect);
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, Color32::from_black_alpha(150));
    let frame = Rect::from_center_size(rect.center(), Vec2::new((980.0 * k).min(rect.width() - 30.0), (620.0 * k).min(rect.height() - 30.0)));
    p.rect_filled(frame, 10.0 * k, skin.panel);
    p.rect_stroke(frame, 10.0 * k, Stroke::new(1.0, skin.accent.gamma_multiply(0.6)), egui::StrokeKind::Inside);
    header(ui, frame, Page::Inventory, st, rpg, skin, k, &mut out);
    // the bag
    let cols = 6usize;
    let size = 62.0 * k;
    let gap = 8.0 * k;
    let n = st.bag.len();
    if n > 0 {
        let rows = n.div_ceil(cols) as i32;
        // keyboard / gamepad: move the highlight around the grid, choose to use it
        let (c, r) = ((*focus % cols) as i32, (*focus / cols) as i32);
        let (c, r) = ((c + steps.dx).rem_euclid(cols as i32), (r + steps.dy).rem_euclid(rows));
        *focus = ((r as usize) * cols + c as usize).min(n - 1);
        if steps.choose {
            out.push(Act::UseSlot(*focus));
        }
    }
    let origin = frame.min + Vec2::new(24.0 * k, 70.0 * k);
    p.text(origin - Vec2::new(0.0, 8.0 * k), Align2::LEFT_BOTTOM, tr!("Bag — click to use or wear", "Bolsa — clique para usar ou vestir"), FontId::new(13.0 * k, skin.text_font.clone()), skin.text.gamma_multiply(0.7));
    for (i, slot) in st.bag.iter().enumerate() {
        let r = Rect::from_min_size(origin + Vec2::new((i % cols) as f32 * (size + gap), (i / cols) as f32 * (size + gap)), Vec2::splat(size));
        let resp = ui.interact(r, ui.id().with(("bag", i)), egui::Sense::click());
        if resp.hovered() {
            *focus = i;
        }
        let lit = *focus == i;
        p.rect_filled(r, 6.0 * k, skin.slot);
        p.rect_stroke(r, 6.0 * k, Stroke::new(if lit { 2.0 } else { 1.0 }, if lit { skin.accent } else { skin.text.gamma_multiply(0.18) }), egui::StrokeKind::Inside);
        if let Some(s) = slot {
            if let Some(def) = rpg.item(&s.id) {
                draw_icon(&p, r.shrink(5.0 * k), &def.icon, def.color, &def.name, skin);
                if s.count > 1 {
                    let at = r.right_bottom() - Vec2::new(5.0 * k, 3.0 * k);
                    p.text(at + Vec2::new(1.0, 1.0), Align2::RIGHT_BOTTOM, s.count.to_string(), FontId::new(15.0 * k, skin.text_font.clone()), Color32::BLACK);
                    p.text(at, Align2::RIGHT_BOTTOM, s.count.to_string(), FontId::new(15.0 * k, skin.text_font.clone()), Color32::WHITE);
                }
                if lit {
                    let kind = if !def.slot.is_empty() {
                        tr!("Wear: {}", "Vestir: {}", slot_name(&def.slot))
                    } else if def.consumable {
                        tr!("Use (consumed)", "Usar (gasta)")
                    } else {
                        tr!("Item", "Item")
                    };
                    let body = [def.description.clone(), stats_text(&def.stats, false)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n\n");
                    tooltip(ui, r, &def.name, &kind, &body, skin);
                }
            }
            if resp.clicked() {
                out.push(Act::UseSlot(i));
            }
        }
    }
    // what is worn and the stats
    let side = Pos2::new(origin.x + cols as f32 * (size + gap) + 30.0 * k, origin.y);
    p.text(side - Vec2::new(0.0, 8.0 * k), Align2::LEFT_BOTTOM, tr!("Equipment — click to take off", "Equipamento — clique para tirar"), FontId::new(13.0 * k, skin.text_font.clone()), skin.text.gamma_multiply(0.7));
    for (i, slot) in EQUIP_SLOTS.iter().enumerate() {
        let r = Rect::from_min_size(side + Vec2::new(0.0, i as f32 * (size + gap)), Vec2::splat(size));
        p.rect_filled(r, 6.0 * k, skin.slot);
        p.rect_stroke(r, 6.0 * k, Stroke::new(1.0, skin.accent.gamma_multiply(0.4)), egui::StrokeKind::Inside);
        p.text(r.right_center() + Vec2::new(12.0 * k, -9.0 * k), Align2::LEFT_CENTER, slot_name(slot), FontId::new(13.0 * k, skin.text_font.clone()), skin.text.gamma_multiply(0.7));
        let resp = ui.interact(r, ui.id().with(("worn", i)), egui::Sense::click());
        match st.equipped.get(*slot).and_then(|id| rpg.item(id)) {
            Some(def) => {
                draw_icon(&p, r.shrink(5.0 * k), &def.icon, def.color, &def.name, skin);
                p.text(r.right_center() + Vec2::new(12.0 * k, 9.0 * k), Align2::LEFT_CENTER, &def.name, FontId::new(15.0 * k, skin.text_font.clone()), skin.text);
                if resp.hovered() {
                    tooltip(ui, r, &def.name, &slot_name(slot), &[def.description.clone(), stats_text(&def.stats, false)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n\n"), skin);
                }
                if resp.clicked() {
                    out.push(Act::Unequip(slot.to_string()));
                }
            }
            None => {
                p.text(r.right_center() + Vec2::new(12.0 * k, 9.0 * k), Align2::LEFT_CENTER, tr!("(nothing)", "(nada)"), FontId::new(14.0 * k, skin.text_font.clone()), skin.text.gamma_multiply(0.4));
            }
        }
    }
    let mut y = side.y + 3.0 * (size + gap) + 16.0 * k;
    p.text(Pos2::new(side.x, y), Align2::LEFT_TOP, tr!("Stats", "Atributos"), FontId::new(18.0 * k, skin.title_font.clone()), skin.accent);
    y += 26.0 * k;
    let stats = st.stats(rpg);
    if stats.is_empty() {
        p.text(Pos2::new(side.x, y), Align2::LEFT_TOP, tr!("(nothing yet)", "(nada ainda)"), FontId::new(14.0 * k, skin.text_font.clone()), skin.text.gamma_multiply(0.5));
    }
    for (name, v) in stats {
        p.text(Pos2::new(side.x, y), Align2::LEFT_TOP, stat_label(&name), FontId::new(14.0 * k, skin.text_font.clone()), skin.text);
        p.text(Pos2::new(frame.right() - 24.0 * k, y), Align2::RIGHT_TOP, format!("{}{}", if v >= 0.0 { "+" } else { "" }, fmt_num(v)), FontId::new(14.0 * k, skin.text_font.clone()), skin.accent);
        y += 20.0 * k;
    }
    // the skill bar: pick a known skill, then a slot
    if !st.bar.is_empty() {
        let by = frame.bottom() - 96.0 * k;
        p.text(Pos2::new(origin.x, by - 8.0 * k), Align2::LEFT_BOTTOM, tr!("Skills — click a skill, then a slot of the bar", "Habilidades — clique numa habilidade e depois num espaço da barra"), FontId::new(13.0 * k, skin.text_font.clone()), skin.text.gamma_multiply(0.7));
        let s = 48.0 * k;
        for (i, id) in st.known.iter().enumerate() {
            let Some(def) = rpg.skill(id) else { continue };
            let r = Rect::from_min_size(Pos2::new(origin.x + i as f32 * (s + 6.0 * k), by), Vec2::splat(s));
            let resp = ui.interact(r, ui.id().with(("known", i)), egui::Sense::click());
            p.rect_filled(r, 5.0 * k, skin.slot);
            draw_icon(&p, r.shrink(3.0), &def.icon, def.color, &def.name, skin);
            let picked = picked_skill.as_deref() == Some(id.as_str());
            if picked || resp.hovered() {
                p.rect_stroke(r, 5.0 * k, Stroke::new(2.0, skin.accent), egui::StrokeKind::Inside);
            }
            if resp.hovered() {
                let sub = [tr!("Cooldown {}s", "Recarga {}s", fmt_num(def.cooldown as f64)), def.cost.clone()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
                tooltip(ui, r, &def.name, &sub, &def.description, skin);
            }
            if resp.clicked() {
                *picked_skill = if picked { None } else { Some(id.clone()) };
            }
        }
        if st.known.is_empty() {
            p.text(Pos2::new(origin.x, by + 14.0 * k), Align2::LEFT_TOP, tr!("No skills yet: learn them in the talent tree.", "Nenhuma habilidade ainda: aprenda na árvore de talentos."), FontId::new(14.0 * k, skin.text_font.clone()), skin.text.gamma_multiply(0.5));
        }
        let bar = draw_skill_bar(&p, Pos2::new(frame.right() - 24.0 * k - (st.bar.len() as f32 * 54.0 * k) / 2.0, by + s), s, rpg, st, skin);
        let gap = s * 0.12;
        for i in 0..st.bar.len() {
            let r = Rect::from_min_size(bar.min + Vec2::new(i as f32 * (s + gap), 0.0), Vec2::splat(s));
            let resp = ui.interact(r, ui.id().with(("barslot", i)), egui::Sense::click());
            if resp.hovered() && picked_skill.is_some() {
                p.rect_stroke(r, 5.0, Stroke::new(2.0, skin.accent), egui::StrokeKind::Inside);
            }
            if resp.clicked() {
                if let Some(id) = picked_skill.take() {
                    out.push(Act::Assign(i, id));
                }
            }
        }
    }
    out
}

pub fn slot_name(slot: &str) -> String {
    match slot {
        "weapon" => tr!("Weapon", "Arma"),
        "armor" => tr!("Armor", "Armadura"),
        "trinket" => tr!("Trinket", "Amuleto"),
        other => other.to_string(),
    }
}

/// The talent tree: a column per branch, rows by tier, lines to what each needs.
pub fn talents_screen(ui: &mut egui::Ui, rect: Rect, rpg: &Rpg, st: &RpgState, skin: &Skin, focus: &mut usize, steps: Steps) -> Vec<Act> {
    let mut out = vec![];
    let k = crate::menus::scale(rect);
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, Color32::from_black_alpha(150));
    let frame = Rect::from_center_size(rect.center(), Vec2::new((980.0 * k).min(rect.width() - 30.0), (620.0 * k).min(rect.height() - 30.0)));
    p.rect_filled(frame, 10.0 * k, skin.panel);
    p.rect_stroke(frame, 10.0 * k, Stroke::new(1.0, skin.accent.gamma_multiply(0.6)), egui::StrokeKind::Inside);
    header(ui, frame, Page::Talents, st, rpg, skin, k, &mut out);
    let pts = tr!("Points to spend: {}", "Pontos para gastar: {}", st.points);
    p.text(Pos2::new(frame.left() + 24.0 * k, frame.top() + 64.0 * k), Align2::LEFT_CENTER, pts, FontId::new(16.0 * k, skin.text_font.clone()), if st.points > 0 { skin.accent } else { skin.text.gamma_multiply(0.6) });
    let branches = rpg.branches.len().max(1);
    let tiers = rpg.talents.iter().map(|t| t.tier).max().unwrap_or(0) as usize + 1;
    let area = Rect::from_min_max(frame.min + Vec2::new(24.0 * k, 90.0 * k), frame.max - Vec2::new(24.0 * k, 24.0 * k));
    let col_w = area.width() / branches as f32;
    let row_h = (area.height() - 30.0 * k) / tiers.max(1) as f32;
    let node = (row_h * 0.62).min(64.0 * k).max(34.0);
    let pos_of = |t: &Talent| Pos2::new(area.left() + col_w * (t.branch.min(branches - 1) as f32 + 0.5), area.top() + 30.0 * k + row_h * (t.tier as f32 + 0.45));
    for (i, name) in rpg.branches.iter().enumerate() {
        let x = area.left() + col_w * (i as f32 + 0.5);
        p.text(Pos2::new(x, area.top() + 8.0 * k), Align2::CENTER_CENTER, name, FontId::new(19.0 * k, skin.title_font.clone()), skin.accent);
        if i > 0 {
            let lx = area.left() + col_w * i as f32;
            p.line_segment([Pos2::new(lx, area.top()), Pos2::new(lx, area.bottom())], Stroke::new(1.0, skin.text.gamma_multiply(0.12)));
        }
    }
    // the lines first, under the nodes
    for t in &rpg.talents {
        for r in &t.requires {
            if let Some(req) = rpg.talent(r) {
                let on = st.rank(r) > 0;
                p.line_segment([pos_of(req), pos_of(t)], Stroke::new(3.0 * k, if on { skin.accent.gamma_multiply(0.8) } else { skin.text.gamma_multiply(0.15) }));
            }
        }
    }
    // keyboard / gamepad: move between the talents in reading order
    let mut order: Vec<usize> = (0..rpg.talents.len()).collect();
    order.sort_by_key(|&i| (rpg.talents[i].tier, rpg.talents[i].branch));
    if !order.is_empty() {
        let at = order.iter().position(|&i| i == *focus).unwrap_or(0) as i32;
        let next = (at + steps.dx + steps.dy * branches as i32).rem_euclid(order.len() as i32);
        *focus = order[next as usize];
        if steps.choose {
            out.push(Act::Learn(rpg.talents[*focus].id.clone()));
        }
    }
    for (i, t) in rpg.talents.iter().enumerate() {
        let c = pos_of(t);
        let r = Rect::from_center_size(c, Vec2::splat(node));
        let resp = ui.interact(r, ui.id().with(("talent", i)), egui::Sense::click());
        if resp.hovered() {
            *focus = i;
        }
        let rank = st.rank(&t.id);
        let blocked = st.blocked(rpg, &t.id);
        let can = blocked.is_none();
        p.circle_filled(c, node * 0.56, Color32::from_black_alpha(200));
        p.rect_filled(r, node * 0.2, skin.slot);
        draw_icon(&p, r.shrink(4.0), &t.icon, t.color, &t.name, skin);
        if rank == 0 {
            p.rect_filled(r, node * 0.2, Color32::from_black_alpha(if can { 70 } else { 150 }));
        }
        let ring = if rank > 0 { skin.accent } else if can { skin.text.gamma_multiply(0.8) } else { skin.text.gamma_multiply(0.25) };
        p.rect_stroke(r, node * 0.2, Stroke::new(if *focus == i { 3.0 } else { 1.5 }, if *focus == i { Color32::WHITE } else { ring }), egui::StrokeKind::Outside);
        // rank badge
        let badge = format!("{rank}/{}", t.max_rank.max(1));
        let bp = r.right_bottom() + Vec2::new(-2.0, 2.0);
        let g = p.layout_no_wrap(badge, FontId::new(12.0 * k, skin.text_font.clone()), Color32::WHITE);
        let br = Rect::from_min_size(bp - g.size() - Vec2::new(4.0, 2.0), g.size() + Vec2::new(8.0, 4.0));
        p.rect_filled(br, 4.0, Color32::from_black_alpha(220));
        p.galley(br.min + Vec2::new(4.0, 2.0), g, if rank > 0 { skin.accent } else { Color32::WHITE });
        p.text(r.center_bottom() + Vec2::new(0.0, 6.0 * k), Align2::CENTER_TOP, &t.name, FontId::new(12.5 * k, skin.text_font.clone()), if rank > 0 { skin.accent } else { skin.text.gamma_multiply(0.85) });
        if *focus == i {
            let mut body = t.description.clone();
            let s = stats_text(&t.stats, t.max_rank > 1);
            if !s.is_empty() {
                body = [body, s].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n\n");
            }
            if let Some(sk) = rpg.skill(&t.skill) {
                body.push_str(&tr!("\n\nTeaches the skill {}.", "\n\nEnsina a habilidade {}.", sk.name));
            }
            body.push_str(&match &blocked {
                None => tr!("\n\nClick to learn.", "\n\nClique para aprender."),
                Some(why) => format!("\n\n({why})"),
            });
            tooltip(ui, r, &t.name, &tr!("Rank {} of {}", "Nível {} de {}", rank, t.max_rank.max(1)), &body, skin);
        }
        if resp.clicked() {
            out.push(Act::Learn(t.id.clone()));
        }
    }
    if rpg.talents.is_empty() {
        p.text(area.center(), Align2::CENTER_CENTER, tr!("This game has no talents.", "Este jogo não tem talentos."), FontId::new(16.0 * k, skin.text_font.clone()), skin.text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Rpg {
        let mut stats = BTreeMap::new();
        stats.insert("damage".to_string(), 5.0);
        let mut per_rank = BTreeMap::new();
        per_rank.insert("damage".to_string(), 0.1);
        Rpg {
            enabled: true,
            items: vec![
                ItemDef { id: "potion".into(), name: "Potion".into(), stack: 5, consumable: true, ..Default::default() },
                ItemDef { id: "sword".into(), name: "Sword".into(), slot: "weapon".into(), stats: stats.clone(), ..Default::default() },
                ItemDef { id: "axe".into(), name: "Axe".into(), slot: "weapon".into(), ..Default::default() },
            ],
            skills: vec![SkillDef { id: "spin".into(), name: "Spin".into(), cooldown: 2.0, ..Default::default() }],
            talents: vec![
                Talent { id: "might".into(), max_rank: 3, stats: per_rank, ..Default::default() },
                Talent { id: "whirl".into(), tier: 1, requires: vec!["might".into()], level: 2, skill: "spin".into(), ..Default::default() },
            ],
            screens: RpgScreens { slots: 4, ..Default::default() },
            start_items: vec![Stack { id: "potion".into(), count: 7 }],
            ..Default::default()
        }
    }

    #[test]
    fn bag_stacks_equipment_swaps_and_stats_add_up() {
        let rpg = sample();
        let mut s = RpgState::new(&rpg);
        // 7 potions in stacks of 5: two slots
        assert_eq!(s.count("potion"), 7);
        assert_eq!(s.bag.iter().flatten().count(), 2);
        assert_eq!(s.add_item(&rpg, "sword", 1), 1);
        // the bag has 4 slots: only one more axe fits
        assert_eq!(s.add_item(&rpg, "axe", 3), 1);
        // drinking uses one up
        assert_eq!(s.use_item(&rpg, "potion"), vec![RpgEvent::ItemUsed("potion".into())]);
        assert_eq!(s.count("potion"), 6);
        // wearing the sword gives its damage; wearing the axe puts the sword back in the bag
        let ev = s.use_item(&rpg, "sword");
        assert!(ev.contains(&RpgEvent::Equipped("weapon".into(), "sword".into())));
        assert_eq!(s.stat(&rpg, "damage"), 5.0);
        let ev = s.equip(&rpg, "axe");
        assert!(ev.contains(&RpgEvent::Unequipped("weapon".into(), "sword".into())));
        assert_eq!(s.stat(&rpg, "damage"), 0.0);
        assert_eq!(s.count("sword"), 1);
        assert_eq!(s.remove_item("potion", 100), 6);
    }

    #[test]
    fn levels_give_points_talents_need_them_and_teach_skills() {
        let rpg = sample();
        let mut s = RpgState::new(&rpg);
        assert_eq!(s.blocked(&rpg, "might"), Some(tr!("no points", "sem pontos")));
        // 100 xp to level 2, 135 more to level 3
        let ev = s.add_xp(&rpg, 250.0);
        assert_eq!(ev, vec![RpgEvent::LevelUp(2), RpgEvent::LevelUp(3)]);
        assert_eq!((s.level, s.points, s.xp), (3, 2, 15.0));
        // whirl needs might first
        assert!(s.blocked(&rpg, "whirl").is_some());
        assert!(s.learn_talent(&rpg, "whirl").is_empty());
        s.learn_talent(&rpg, "might");
        assert!((s.stat(&rpg, "damage") - 0.1).abs() < 1e-9);
        let ev = s.learn_talent(&rpg, "whirl");
        assert!(ev.contains(&RpgEvent::SkillLearned("spin".into())));
        assert_eq!(s.points, 0);
        // the skill went on the bar; pressing its key uses it once, then it cools down
        assert_eq!(s.bar[0], "spin");
        assert_eq!(s.press_slot(&rpg, 0), Some(RpgEvent::Skill("spin".into())));
        assert_eq!(s.press_slot(&rpg, 0), None);
        s.tick(2.1);
        assert!(s.ready("spin"));
        // moving it to another slot empties the first
        s.assign(2, "spin");
        assert_eq!((s.bar[0].as_str(), s.bar[2].as_str()), ("", "spin"));
    }
}
