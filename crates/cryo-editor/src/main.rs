//! Cryo Engine — 3D editor with scripting in the Cryo language.
//!
//! Usage: `cryo [project_folder] [--play | --game]`

// Release builds on Windows should not open a console window next to the editor.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[macro_use]
mod i18n;
mod anim;
mod app;
mod assets;
mod audio;
mod mesh_edit;
mod meshmode;
mod highlight;
mod panels;
mod physics;
mod project;
mod qte;
mod render;
mod runtime;
mod scene;
mod settings;
mod skel;
mod timeline;
mod tutorial;
mod viewport;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let mut project_dir: Option<PathBuf> = None;
    let mut autoplay = false;
    let mut game_flag = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--play" => autoplay = true,
            "--game" => game_flag = true,
            "-h" | "--help" => {
                println!("Cryo Engine\n\nUsage: cryo [project_folder] [--play | --game]\n\n  --play   open the project already in Play mode\n  --game   run only the game, without the editor");
                return Ok(());
            }
            other => project_dir = Some(PathBuf::from(other)),
        }
    }
    // Exported executable: finds the `game/` folder next to it and opens straight into the game.
    let bundled = if project_dir.is_none() { project::bundled_game() } else { None };
    let game = match (&bundled, game_flag, &project_dir) {
        (Some(dir), _, _) => project::read_game_config(dir),
        (None, true, Some(dir)) => Some(project::read_game_config(dir).unwrap_or_else(|| project::GameConfig {
            title: dir.file_name().and_then(|n| n.to_str()).unwrap_or("Game").to_string(),
            scene: project::list_files(dir, project::SCENE_EXT).into_iter().next().unwrap_or_default(),
        })),
        _ => None,
    };
    let settings = settings::Settings::load();
    i18n::set_lang(i18n::Lang::from_code(&settings.language));
    // No folder given: reopen the last project, or fall back to the example in the user's home.
    let project_dir = bundled.or(project_dir).unwrap_or_else(|| {
        settings
            .last_project
            .as_ref()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(settings::default_project_dir)
    });
    let title = match &game {
        Some(g) => g.title.clone(),
        None => "Cryo Engine".to_string(),
    };

    // Anti-aliasing (MSAA). If the graphics card does not support it, run with CRYO_MSAA=0.
    let msaa: u16 = std::env::var("CRYO_MSAA").ok().and_then(|v| v.parse().ok()).unwrap_or(4);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([1440.0, 880.0])
            .with_min_inner_size([900.0, 560.0]),
        depth_buffer: 24,
        multisampling: msaa,
        ..Default::default()
    };
    let result = eframe::run_native(
        "Cryo Engine",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, project_dir, autoplay, game, settings)))),
    );
    if result.is_err() && msaa > 0 {
        eprintln!("Hint: if this is a graphics configuration error, try again with CRYO_MSAA=0");
    }
    result
}
