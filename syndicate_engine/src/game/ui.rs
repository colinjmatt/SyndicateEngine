use crate::engine::{assets::AssetIndex, palette_decode::Rgb8};
use macroquad::prelude::*;

pub fn draw_hud(asset_index: &AssetIndex, selected: &str) {
    draw_rectangle(16.0, 16.0, 660.0, 264.0, Color::new(0.0, 0.0, 0.0, 0.62));
    draw_rectangle_lines(16.0, 16.0, 660.0, 264.0, 2.0, GREEN);
    draw_text(
        "SYNDICATEENGINE // CLEAN-ROOM PROTOTYPE",
        28.0,
        42.0,
        22.0,
        GREEN,
    );
    draw_text(
        &format!(
            "Assets: {} files from {}",
            asset_index.total_files(),
            asset_index.root_display()
        ),
        28.0,
        66.0,
        16.0,
        WHITE,
    );
    draw_text(
        &format!(
            "Maps:{} Missions:{} Palettes:{} Sprites:{} Sounds:{}",
            asset_index.maps(),
            asset_index.missions(),
            asset_index.palettes(),
            asset_index.sprites(),
            asset_index.sounds()
        ),
        28.0,
        88.0,
        16.0,
        WHITE,
    );
    draw_text(
        &format!("Sample map candidate: {}", asset_index.sample_map_name()),
        28.0,
        110.0,
        16.0,
        LIGHTGRAY,
    );
    draw_text(
        &format!("Selected agent: {selected}"),
        28.0,
        136.0,
        18.0,
        YELLOW,
    );
    draw_text(
        &format!("Decode: {}", asset_index.diagnostics().palette_status),
        28.0,
        160.0,
        15.0,
        SKYBLUE,
    );
    draw_text(
        &format!("Decode: {}", asset_index.diagnostics().tab_status),
        28.0,
        180.0,
        15.0,
        SKYBLUE,
    );
    draw_text(
        &format!("Sprite: {}", asset_index.diagnostics().sprite_status),
        28.0,
        204.0,
        15.0,
        SKYBLUE,
    );
    draw_text(
        &format!("Variant: {}", asset_index.diagnostics().tab_variant_status),
        28.0,
        224.0,
        15.0,
        SKYBLUE,
    );
    draw_text(
        "WASD/Arrows pan | Mouse wheel zoom | 1-4 select | Right-click command | Esc quit",
        28.0,
        268.0,
        15.0,
        GRAY,
    );

    draw_palette_preview(asset_index.diagnostics().palette_preview.as_slice());
}

fn draw_palette_preview(colors: &[Rgb8]) {
    if colors.is_empty() {
        draw_text("Palette preview unavailable", 28.0, 248.0, 15.0, DARKGRAY);
        return;
    }

    draw_text("Palette", 28.0, 248.0, 15.0, LIGHTGRAY);
    let swatch_size = 12.0;
    for (i, color) in colors.iter().enumerate() {
        draw_rectangle(
            92.0 + i as f32 * swatch_size,
            238.0,
            swatch_size,
            16.0,
            Color::from_rgba(color.r, color.g, color.b, 255),
        );
    }
    draw_rectangle_lines(
        92.0,
        238.0,
        swatch_size * colors.len() as f32,
        16.0,
        1.0,
        GRAY,
    );
}
