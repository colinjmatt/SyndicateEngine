use macroquad::prelude::*;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use syndicate_engine::{
    engine::{assets::AssetIndex, config::window_conf},
    game::world::WorldState,
};

#[macroquad::main(window_conf)]
async fn main() {
    let asset_index = AssetIndex::discover("../original_assets");
    let mut world = WorldState::new(asset_index).await;
    let visual_diagnostic_frame = std::env::var("SYNDICATE_VISUAL_DIAGNOSTIC_FRAMES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|frame| *frame > 0);
    let mut frame = 0_u32;
    let mut visual_diagnostic_written = false;

    loop {
        clear_background(Color::from_rgba(7, 10, 13, 255));
        world.update(get_frame_time());
        world.draw();
        frame = frame.saturating_add(1);
        if !visual_diagnostic_written
            && visual_diagnostic_frame.is_some_and(|capture_frame| frame >= capture_frame)
        {
            visual_diagnostic_written = true;
            if let Some(path) = visual_diagnostic_path(frame) {
                get_screen_data().export_png(path.to_string_lossy().as_ref());
                println!(
                    "[visual-diagnostic] wrote local-only screenshot {}; do not commit visual_diagnostics/",
                    path.display()
                );
            }
        }
        next_frame().await;
    }
}

fn visual_diagnostic_path(frame: u32) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let repo_root = if cwd
        .file_name()
        .is_some_and(|name| name == "syndicate_engine")
    {
        cwd.parent().map(PathBuf::from).unwrap_or(cwd)
    } else {
        cwd
    };
    let dir = repo_root.join("visual_diagnostics");
    if let Err(err) = fs::create_dir_all(&dir) {
        eprintln!(
            "[visual-diagnostic] could not create {}: {err}",
            dir.display()
        );
        return None;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    Some(dir.join(format!(
        "syndicate_engine-runtime-{stamp}-frame-{frame}.png"
    )))
}
