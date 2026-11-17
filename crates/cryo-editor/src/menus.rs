//! Game menus: the main menu a game opens with, the pause menu, and the options screen.
//!
//! Everything is set per project in `menus.json` (edited in **Game > Menus…**): the title and
//! subtitle, a logo and a background picture, music, the buttons of each menu and what they do,
//! and the look (colours, sizes, where the menu sits on screen).
//!
//! Button actions: `start` (begin the game, or load `arg` as the first scene), `continue`
//! (the scene saved by `save.set("checkpoint", ...)`, only shown when there is one),
//! `options`, `controls`, `credits`, `resume`, `restart` (the current scene from its start),
//! `main_menu`, `quit`, `scene` (load the scene in `arg`) and `event` (sends `on menu(arg)` to
//! every script, for anything else).

use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Pos2, Rect, RichText, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "menus.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MenuButton {
    pub label: String,
    pub action: String,
    #[serde(default)]
    pub arg: String,
    /// Ask "Are you sure?" first (quit, restart, main menu...).
    #[serde(default)]
    pub confirm: bool,
    /// A line shown at the bottom while the button is highlighted.
    #[serde(default)]
    pub hint: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    /// "center", "left" or "right": where the buttons stand.
    pub align: String,
    /// Colours as [r, g, b, a], 0 to 255.
    pub accent: [u8; 4],
    pub text: [u8; 4],
    pub button: [u8; 4],
    pub button_hover: [u8; 4],
    /// Darkening laid over the game behind a menu.
    pub backdrop: [u8; 4],
    /// Background of the options, credits and custom screens.
    pub panel: [u8; 4],
    pub title_size: f32,
    pub button_size: f32,
    pub button_width: f32,
    pub button_height: f32,
    pub spacing: f32,
    /// Rounded corners of the buttons, in points.
    pub rounding: f32,
    /// Letters of the title spread apart (cinematic titles).
    pub title_spacing: bool,
    /// Font files of the project (.ttf, .otf) for the title and for everything else.
    pub title_font: String,
    pub text_font: String,
    /// Button labels in capitals, and a soft shadow under the texts.
    pub uppercase: bool,
    pub shadow: bool,
    /// Seconds a menu takes to fade in, and how far (points) the highlighted button slides.
    pub fade_in: f32,
    pub hover_slide: f32,
    /// Sounds of the project: moving between buttons, and choosing one.
    pub sound_move: String,
    pub sound_choose: String,
    /// Degrees a second the camera behind the main menu turns (a slow pan); 0 keeps it still.
    pub camera_drift: f32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            align: "center".into(),
            accent: [235, 205, 140, 255],
            text: [235, 235, 235, 255],
            button: [20, 20, 24, 200],
            button_hover: [70, 60, 40, 230],
            backdrop: [0, 0, 0, 150],
            panel: [16, 16, 20, 235],
            title_size: 64.0,
            button_size: 22.0,
            button_width: 300.0,
            button_height: 46.0,
            spacing: 10.0,
            rounding: 4.0,
            title_spacing: true,
            title_font: String::new(),
            text_font: String::new(),
            uppercase: false,
            shadow: true,
            fade_in: 0.35,
            hover_slide: 8.0,
            sound_move: String::new(),
            sound_choose: String::new(),
            camera_drift: 0.0,
        }
    }
}

/// A screen of the game's own: a title, some text and buttons (level select, game over,
/// difficulty...). Opened by a button with the action "screen" or by `menu.open(id)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CustomScreen {
    pub id: String,
    pub title: String,
    pub text: String,
    pub buttons: Vec<MenuButton>,
    /// Pauses the game while open (a game-over screen usually does).
    pub pauses: bool,
}

/// A setting of the game itself shown in Options (difficulty, subtitles, field of view...).
/// Scripts read it with `option("id")` and hear `on option_changed(id, value)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameOption {
    pub id: String,
    pub label: String,
    /// "toggle" (true/false), "slider" (a number from min to max) or "choice" (one of choices).
    pub kind: String,
    /// Starting value: 0/1 for a toggle, the number for a slider, the index for a choice.
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub choices: Vec<String>,
}

impl Default for GameOption {
    fn default() -> Self {
        GameOption { id: "option".into(), label: "Option".into(), kind: "toggle".into(), default: 0.0, min: 0.0, max: 1.0, step: 0.0, choices: vec![] }
    }
}

/// Which built-in parts the Options screen shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OptionsScreen {
    pub graphics: bool,
    pub frame_rate: bool,
    pub volume: bool,
    pub fullscreen: bool,
    pub controls: bool,
}

impl Default for OptionsScreen {
    fn default() -> Self {
        OptionsScreen { graphics: true, frame_rate: true, volume: true, fullscreen: true, controls: true }
    }
}

/// The screen shown while a level loads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Loading {
    pub enabled: bool,
    pub text: String,
    /// A picture of the project covering the screen.
    pub picture: String,
    /// One is shown at random each time.
    pub tips: Vec<String>,
    /// Shown at least this long, so the tip can be read.
    pub min_seconds: f32,
}

impl Default for Loading {
    fn default() -> Self {
        Loading { enabled: true, text: "Loading…".into(), picture: String::new(), tips: vec![], min_seconds: 1.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Menus {
    /// Show the main menu when the game starts (exported games; in the editor, when "Preview
    /// in Play" is on).
    pub main_enabled: bool,
    pub preview_in_editor: bool,
    pub title: String,
    pub subtitle: String,
    /// Pictures from the project (PNG/JPEG): a logo above the buttons, and a background that
    /// covers the screen (without one, the start scene is shown behind the menu).
    pub logo: String,
    pub background: String,
    /// A camera of the first scene (by name) to look through behind the main menu; empty: the
    /// game's own camera.
    pub camera: String,
    pub music: String,
    /// Small text in a corner (version, studio).
    pub footer: String,
    pub credits: String,
    pub main: Vec<MenuButton>,
    pub pause: Vec<MenuButton>,
    pub screens: Vec<CustomScreen>,
    pub options_screen: OptionsScreen,
    pub options: Vec<GameOption>,
    pub loading: Loading,
    pub style: Style,
}

fn b(label: &str, action: &str) -> MenuButton {
    MenuButton { label: label.into(), action: action.into(), arg: String::new(), confirm: false, hint: String::new() }
}

/// A button with an argument (a scene, a screen, an event).
pub fn button(label: &str, action: &str, arg: &str) -> MenuButton {
    MenuButton { arg: arg.into(), ..b(label, action) }
}

impl Default for Menus {
    fn default() -> Self {
        let mut quit = b("Quit", "quit");
        quit.confirm = true;
        let mut quit2 = quit.clone();
        quit2.label = "Quit".into();
        let mut to_main = b("Main menu", "main_menu");
        to_main.confirm = true;
        Menus {
            main_enabled: true,
            preview_in_editor: false,
            title: String::new(),
            subtitle: String::new(),
            logo: String::new(),
            background: String::new(),
            camera: String::new(),
            music: String::new(),
            footer: String::new(),
            credits: String::new(),
            main: vec![b("New game", "start"), b("Continue", "continue"), b("Options", "options"), b("Credits", "credits"), quit],
            pause: vec![b("Resume", "resume"), b("Options", "options"), b("Restart", "restart"), to_main, quit2],
            screens: vec![],
            options_screen: OptionsScreen::default(),
            options: vec![],
            loading: Loading::default(),
            style: Style::default(),
        }
    }
}

impl Menus {
    pub fn load(project: &Path) -> Menus {
        std::fs::read_to_string(project.join(FILE)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, project: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(project.join(FILE), json).map_err(|e| e.to_string())
    }

    pub fn screen_index(&self, id: &str) -> Option<usize> {
        self.screens.iter().position(|s| s.id == id)
    }

    /// The value of a game option for scripts: a toggle is true/false, a choice its text.
    pub fn option_value(&self, o: &GameOption, stored: Option<f64>) -> cryo_lang::Value {
        let v = stored.unwrap_or(o.default);
        match o.kind.as_str() {
            "toggle" => cryo_lang::Value::Bool(v >= 0.5),
            "choice" => cryo_lang::Value::text(o.choices.get(v.max(0.0) as usize).cloned().unwrap_or_default()),
            _ => cryo_lang::Value::Num(v.clamp(o.min.min(o.max), o.max.max(o.min))),
        }
    }
}

/// Every action a button can have, with its name for people.
pub fn actions() -> Vec<(&'static str, String)> {
    vec![
        ("start", tr!("Start the game", "Começar o jogo")),
        ("continue", tr!("Continue (saved checkpoint)", "Continuar (ponto salvo)")),
        ("options", tr!("Options", "Opções")),
        ("controls", tr!("Controls", "Controles")),
        ("credits", tr!("Credits", "Créditos")),
        ("screen", tr!("Open a screen of the game", "Abrir uma tela do jogo")),
        ("back", tr!("Back", "Voltar")),
        ("resume", tr!("Resume (pause menu)", "Voltar ao jogo (pausa)")),
        ("restart", tr!("Restart the level", "Reiniciar a fase")),
        ("main_menu", tr!("Back to the main menu", "Voltar ao menu inicial")),
        ("scene", tr!("Load a scene", "Carregar uma cena")),
        ("event", tr!("Send an event to scripts", "Enviar um evento aos scripts")),
        ("quit", tr!("Quit the game", "Sair do jogo")),
    ]
}

pub fn color(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// Which screen of the menus is open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Screen {
    Main,
    Pause,
    Options,
    Credits,
    /// One of the game's own screens (index in `Menus::screens`).
    Custom(usize),
}

/// Keyboard or gamepad navigation in a menu, this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Nav {
    None,
    Up,
    Down,
    Choose,
    Back,
}

/// What a menu shows above its buttons.
pub enum Header<'a> {
    /// The game's title, subtitle, logo and corner text (main menu).
    Title,
    /// A heading and some text (custom screens, "Are you sure?").
    Text(&'a str, &'a str),
    None,
}

/// How menus are drawn this frame: fonts, and seconds since the screen opened (fading in).
#[derive(Clone)]
pub struct Look {
    pub title_font: FontFamily,
    pub text_font: FontFamily,
    pub t: f32,
}

impl Default for Look {
    fn default() -> Self {
        Look { title_font: FontFamily::Proportional, text_font: FontFamily::Proportional, t: 10.0 }
    }
}

/// What happened in a menu this frame.
#[derive(Default)]
pub struct Outcome {
    pub chosen: Option<MenuButton>,
    /// The highlight moved to another button (for the "move" sound).
    pub moved: bool,
}

/// The scale of the menus for a screen this tall (designed for 900 points).
pub fn scale(rect: Rect) -> f32 {
    (rect.height() / 900.0).clamp(0.6, 2.0)
}

fn shadowed(painter: &egui::Painter, st: &Style, pos: Pos2, anchor: Align2, text: &str, font: FontId, col: Color32) {
    if st.shadow {
        let d = (font.size * 0.05).clamp(1.0, 4.0);
        painter.text(pos + Vec2::new(d, d * 1.3), anchor, text, font.clone(), Color32::from_black_alpha(150).gamma_multiply(col.a() as f32 / 255.0));
    }
    painter.text(pos, anchor, text, font, col);
}

/// Draws a menu over `rect` and says which button was chosen (mouse, or keyboard and
/// gamepad: up/down move, Enter/Space/A choose; `focus` keeps the highlighted one).
#[allow(clippy::too_many_arguments)]
pub fn draw(
    ui: &mut egui::Ui,
    rect: Rect,
    menus: &Menus,
    buttons: &[MenuButton],
    header: Header,
    textures: (&Option<egui::TextureHandle>, &Option<egui::TextureHandle>),
    focus: &mut usize,
    nav: Nav,
    look: &Look,
) -> Outcome {
    let st = &menus.style;
    let fade = if st.fade_in > 0.01 { (look.t / st.fade_in).clamp(0.0, 1.0) } else { 1.0 };
    ui.multiply_opacity(fade);
    let painter = ui.painter_at(rect);
    let (logo, background) = textures;
    let title_screen = matches!(header, Header::Title);
    match background {
        Some(tex) if title_screen => {
            // cover the screen, keeping the picture's shape
            let size = tex.size_vec2();
            let scale = (rect.width() / size.x).max(rect.height() / size.y);
            let r = Rect::from_center_size(rect.center(), size * scale);
            painter.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            painter.rect_filled(rect, 0.0, color([st.backdrop[0], st.backdrop[1], st.backdrop[2], st.backdrop[3] / 2]));
        }
        _ => {
            painter.rect_filled(rect, 0.0, color(st.backdrop));
        }
    }
    let k = scale(rect);
    let x = match st.align.as_str() {
        "left" => rect.left() + rect.width() * 0.08 + st.button_width * k * 0.5,
        "right" => rect.right() - rect.width() * 0.08 - st.button_width * k * 0.5,
        _ => rect.center().x,
    };
    let anchor = match st.align.as_str() {
        "left" => Align2::LEFT_CENTER,
        "right" => Align2::RIGHT_CENTER,
        _ => Align2::CENTER_CENTER,
    };
    let text_x = match st.align.as_str() {
        "left" => x - st.button_width * k * 0.5,
        "right" => x + st.button_width * k * 0.5,
        _ => x,
    };
    // the title floats up a little as it fades in
    let rise = (1.0 - fade) * 18.0 * k;
    let mut y = rect.top() + rect.height() * 0.2 + rise;
    let title_font = |size: f32| FontId::new(size, look.title_font.clone());
    let text_font = |size: f32| FontId::new(size, look.text_font.clone());
    match header {
        Header::Title => {
            if let Some(tex) = logo {
                let size = tex.size_vec2();
                let h = (rect.height() * 0.22).min(size.y * k);
                let w = size.x * h / size.y;
                let left = match st.align.as_str() {
                    "left" => text_x,
                    "right" => text_x - w,
                    _ => x - w / 2.0,
                };
                painter.image(tex.id(), Rect::from_min_size(Pos2::new(left, y - h / 2.0), Vec2::new(w, h)), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                y += h / 2.0 + 20.0 * k;
            }
            if !menus.title.is_empty() {
                let title = if st.title_spacing { menus.title.chars().map(|c| c.to_string()).collect::<Vec<_>>().join("\u{2009}") } else { menus.title.clone() };
                shadowed(&painter, st, Pos2::new(text_x, y), anchor, &title, title_font(st.title_size * k), color(st.accent));
                y += st.title_size * k * 0.75;
            }
            if !menus.subtitle.is_empty() {
                shadowed(&painter, st, Pos2::new(text_x, y), anchor, &menus.subtitle, text_font(st.button_size * k * 0.85), color(st.text).gamma_multiply(0.85));
            }
            if !menus.footer.is_empty() {
                painter.text(rect.right_bottom() - Vec2::new(14.0, 12.0), Align2::RIGHT_BOTTOM, &menus.footer, text_font(13.0 * k), color(st.text).gamma_multiply(0.55));
            }
        }
        Header::Text(title, text) => {
            // a panel in the middle for the heading and its text
            y = rect.top() + rect.height() * 0.3 + rise;
            if !title.is_empty() {
                shadowed(&painter, st, Pos2::new(text_x, y), anchor, title, title_font(st.title_size * 0.6 * k), color(st.accent));
                y += st.title_size * 0.6 * k * 0.8;
            }
            for line in text.lines() {
                shadowed(&painter, st, Pos2::new(text_x, y), anchor, line, text_font(st.button_size * 0.85 * k), color(st.text));
                y += st.button_size * 1.2 * k;
            }
        }
        Header::None => {}
    }
    // the buttons
    let n = buttons.len();
    let mut out = Outcome::default();
    if n == 0 {
        return out;
    }
    let before = *focus;
    *focus = (*focus).min(n - 1);
    match nav {
        Nav::Up => *focus = (*focus + n - 1) % n,
        Nav::Down => *focus = (*focus + 1) % n,
        _ => {}
    }
    let bh = st.button_height * k;
    let total = n as f32 * bh + (n - 1) as f32 * st.spacing * k;
    let top = match header {
        Header::Title => (rect.top() + rect.height() * 0.5).max(y + 30.0 * k).min(rect.bottom() - total - 40.0 * k),
        Header::Text(..) => (y + 24.0 * k).min(rect.bottom() - total - 40.0 * k),
        Header::None => rect.center().y - total / 2.0,
    };
    for (i, button) in buttons.iter().enumerate() {
        let id = ui.id().with(("menu_button", i));
        let base = Rect::from_center_size(Pos2::new(x, top + bh / 2.0 + i as f32 * (bh + st.spacing * k)), Vec2::new(st.button_width * k, bh));
        let resp = ui.interact(base, id, egui::Sense::click());
        if resp.hovered() && *focus != i {
            *focus = i;
        }
        let on = *focus == i;
        // the highlighted button slides a little (towards the middle when on the right)
        let dir = if st.align == "right" { -1.0 } else { 1.0 };
        let slide = ui.ctx().animate_value_with_time(id.with("slide"), if on { st.hover_slide * k * dir } else { 0.0 }, 0.12);
        let r = base.translate(Vec2::new(if st.align == "center" { 0.0 } else { slide }, 0.0));
        painter.rect_filled(r, st.rounding, color(if on { st.button_hover } else { st.button }));
        if on {
            painter.rect_stroke(r, st.rounding, Stroke::new(1.5, color(st.accent)), egui::StrokeKind::Inside);
        }
        let label_x = match st.align.as_str() {
            "left" => r.left() + 18.0 * k,
            "right" => r.right() - 18.0 * k,
            _ => r.center().x,
        };
        let label = if st.uppercase { button.label.to_uppercase() } else { button.label.clone() };
        shadowed(&painter, st, Pos2::new(label_x, r.center().y), anchor, &label, text_font(st.button_size * k), if on { color(st.accent) } else { color(st.text) });
        if resp.clicked() || (on && nav == Nav::Choose) {
            out.chosen = Some(button.clone());
        }
    }
    // the highlighted button's hint
    if let Some(b) = buttons.get(*focus) {
        if !b.hint.is_empty() {
            shadowed(&painter, st, Pos2::new(rect.center().x, rect.bottom() - 46.0 * k), Align2::CENTER_CENTER, &b.hint, text_font(st.button_size * 0.75 * k), color(st.text).gamma_multiply(0.8));
        }
    }
    out.moved = *focus != before.min(n - 1);
    out
}

/// Gives a Ui the look of the menus: colours, rounding and text sizes (options and credits).
pub fn style_ui(ui: &mut egui::Ui, st: &Style, k: f32, text_font: &FontFamily) {
    let v = ui.visuals_mut();
    v.override_text_color = Some(color(st.text));
    let opaque = |c: [u8; 4]| color([c[0], c[1], c[2], c[3].max(200)]);
    for (w, c) in [
        (&mut v.widgets.inactive, opaque(st.button)),
        (&mut v.widgets.hovered, opaque(st.button_hover)),
        (&mut v.widgets.active, opaque(st.button_hover)),
        (&mut v.widgets.open, opaque(st.button_hover)),
    ] {
        w.bg_fill = c;
        w.weak_bg_fill = c;
        w.corner_radius = egui::CornerRadius::same(st.rounding.clamp(0.0, 30.0) as u8);
        w.fg_stroke = Stroke::new(1.0, color(st.text));
    }
    // a faint outline so check boxes and sliders show on panels of the buttons' colour
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, color(st.text).gamma_multiply(0.35));
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, color(st.accent));
    v.widgets.active.bg_stroke = Stroke::new(1.0, color(st.accent));
    v.selection.bg_fill = color(st.accent).gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, color(st.accent));
    v.slider_trailing_fill = true;
    let style = ui.style_mut();
    for (ts, f) in style.text_styles.iter_mut() {
        f.size *= 1.15 * k;
        if !matches!(ts, egui::TextStyle::Monospace) {
            f.family = text_font.clone();
        }
    }
    style.spacing.item_spacing = Vec2::new(8.0, 7.0) * k;
    style.spacing.button_padding = Vec2::new(10.0, 5.0) * k;
    style.spacing.slider_width = 200.0 * k;
}

/// A panel in the middle of the screen in the menus' look, with a heading and a Back button.
/// Returns true when the player wants to go back.
pub fn panel(ui: &mut egui::Ui, rect: Rect, menus: &Menus, look: &Look, heading: &str, width: f32, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let st = &menus.style;
    let fade = if st.fade_in > 0.01 { (look.t / st.fade_in).clamp(0.0, 1.0) } else { 1.0 };
    ui.multiply_opacity(fade);
    ui.painter_at(rect).rect_filled(rect, 0.0, color(st.backdrop));
    let k = scale(rect);
    let mut back = false;
    let height = (rect.height() - 60.0 * k).max(200.0);
    let panel = Rect::from_center_size(rect.center(), Vec2::new((width * k).min(rect.width() - 40.0), height));
    ui.scope_builder(egui::UiBuilder::new().max_rect(panel), |ui| {
        style_ui(ui, st, k, &look.text_font);
        egui::Frame::new().fill(color(st.panel)).corner_radius(st.rounding.clamp(0.0, 30.0)).inner_margin(20.0 * k).stroke(Stroke::new(1.0, color(st.accent).gamma_multiply(0.6))).show(ui, |ui| {
            ui.set_width(panel.width() - 40.0 * k);
            ui.label(RichText::new(heading).font(FontId::new(st.button_size * 1.4 * k, look.title_font.clone())).color(color(st.accent)));
            ui.add_space(10.0 * k);
            egui::ScrollArea::vertical().max_height(height - 150.0 * k).show(ui, |ui| body(ui));
            ui.add_space(12.0 * k);
            let label = if st.uppercase { tr!("BACK", "VOLTAR") } else { tr!("Back", "Voltar") };
            if ui.add_sized([200.0 * k, 34.0 * k], egui::Button::new(label)).clicked() {
                back = true;
            }
        });
    });
    back
}

/// The credits screen. Returns true when the player wants to go back.
pub fn credits_panel(ui: &mut egui::Ui, rect: Rect, menus: &Menus, look: &Look) -> bool {
    let st = menus.style.clone();
    let k = scale(rect);
    let text = if menus.credits.is_empty() { tr!("Made with Cryo Engine.", "Feito com a Cryo Engine.") } else { menus.credits.clone() };
    panel(ui, rect, menus, look, &tr!("Credits", "Créditos"), 600.0, |ui| {
        ui.vertical_centered(|ui| {
            for line in text.lines() {
                ui.label(RichText::new(line).size(st.button_size * 0.85 * k));
            }
        });
    })
}

/// The loading screen: a picture, the text and a tip, with a small moving bar.
pub fn loading_screen(ui: &mut egui::Ui, rect: Rect, menus: &Menus, look: &Look, picture: &Option<egui::TextureHandle>, tip: &str, t: f32) {
    let st = &menus.style;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::BLACK);
    if let Some(tex) = picture {
        let size = tex.size_vec2();
        let s = (rect.width() / size.x).max(rect.height() / size.y);
        painter.image(tex.id(), Rect::from_center_size(rect.center(), size * s), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        painter.rect_filled(rect, 0.0, Color32::from_black_alpha(110));
    }
    let k = scale(rect);
    let font = |size: f32| FontId::new(size, look.text_font.clone());
    let at = rect.right_bottom() - Vec2::new(40.0 * k, 50.0 * k);
    shadowed(&painter, st, at, Align2::RIGHT_BOTTOM, &menus.loading.text, font(st.button_size * k), color(st.accent));
    // a bar sweeping under the text
    let w = 160.0 * k;
    let bar = Rect::from_min_size(at + Vec2::new(-w, 10.0 * k), Vec2::new(w, 3.0 * k));
    painter.rect_filled(bar, 1.0, color(st.text).gamma_multiply(0.2));
    let p = (t * 0.8).fract();
    let seg = Rect::from_min_size(Pos2::new(bar.left() + (w - w * 0.3) * p, bar.top()), Vec2::new(w * 0.3, bar.height()));
    painter.rect_filled(seg, 1.0, color(st.accent));
    if !tip.is_empty() {
        shadowed(&painter, st, rect.center_bottom() - Vec2::new(0.0, 110.0 * k), Align2::CENTER_CENTER, tip, font(st.button_size * 0.8 * k), color(st.text).gamma_multiply(0.9));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_load_with_defaults_and_keep_what_the_project_sets() {
        let dir = std::env::temp_dir().join(format!("cryo_menus_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let m = Menus::load(&dir);
        assert!(m.main_enabled && m.main.iter().any(|b| b.action == "start") && m.pause.iter().any(|b| b.action == "resume"));
        // an older or partial file keeps the defaults for what it does not say
        std::fs::write(dir.join(FILE), r#"{"title":"MY GAME","style":{"align":"left"}}"#).unwrap();
        let m = Menus::load(&dir);
        assert_eq!((m.title.as_str(), m.style.align.as_str(), m.style.title_size), ("MY GAME", "left", 64.0));
        assert_eq!(m.main.len(), 5);
        let mut m2 = m.clone();
        m2.main.push(button("Arena", "scene", "scenes/Arena.cryoscene"));
        m2.screens.push(CustomScreen { id: "levels".into(), title: "Levels".into(), buttons: vec![button("Back", "back", "")], ..Default::default() });
        m2.options.push(GameOption { id: "difficulty".into(), kind: "choice".into(), choices: vec!["Easy".into(), "Hard".into()], default: 1.0, ..Default::default() });
        m2.save(&dir).unwrap();
        let back = Menus::load(&dir);
        assert_eq!(back, m2);
        assert_eq!(back.screen_index("levels"), Some(0));
        // game options read as scripts see them
        let o = &back.options[0];
        assert_eq!(back.option_value(o, None), cryo_lang::Value::text("Hard"));
        assert_eq!(back.option_value(o, Some(0.0)), cryo_lang::Value::text("Easy"));
        let t = GameOption::default();
        assert_eq!(back.option_value(&t, Some(1.0)), cryo_lang::Value::Bool(true));
        std::fs::remove_dir_all(dir).ok();
    }
}
