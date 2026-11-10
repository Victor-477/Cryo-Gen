//! Cryo Engine — editor 3D com scripts na linguagem Cryo.
//!
//! Uso: `cryo [pasta_do_projeto] [--play | --game]`

mod app;
mod assets;
mod audio;
mod highlight;
mod panels;
mod physics;
mod project;
mod render;
mod runtime;
mod scene;
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
                println!("Cryo Engine\n\nUso: cryo [pasta_do_projeto] [--play | --game]\n\n  --play   abre o projeto já em modo Play\n  --game   roda só o jogo, sem o editor");
                return Ok(());
            }
            other => project_dir = Some(PathBuf::from(other)),
        }
    }
    // Executável exportado: encontra a pasta `game/` ao lado e abre direto no jogo.
    let bundled = if project_dir.is_none() { project::bundled_game() } else { None };
    let game = match (&bundled, game_flag, &project_dir) {
        (Some(dir), _, _) => project::read_game_config(dir),
        (None, true, Some(dir)) => Some(project::read_game_config(dir).unwrap_or_else(|| project::GameConfig {
            title: dir.file_name().and_then(|n| n.to_str()).unwrap_or("Jogo").to_string(),
            scene: project::list_files(dir, project::SCENE_EXT).into_iter().next().unwrap_or_default(),
        })),
        _ => None,
    };
    let project_dir = bundled.or(project_dir).unwrap_or_else(|| PathBuf::from("ProjetoExemplo"));
    let title = match &game {
        Some(g) => g.title.clone(),
        None => "Cryo Engine".to_string(),
    };

    // Anti-aliasing (MSAA). Se a placa de vídeo não suportar, rode com CRYO_MSAA=0.
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
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, project_dir, autoplay, game)))),
    );
    if result.is_err() && msaa > 0 {
        eprintln!("Dica: se o erro for de configuração gráfica, tente novamente com CRYO_MSAA=0");
    }
    result
}
