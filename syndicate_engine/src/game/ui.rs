use crate::engine::{assets::AssetIndex, palette_decode::Rgb8};
use macroquad::prelude::*;

pub fn draw_hud(asset_index: &AssetIndex, selected: &str, order: &str, combat: &str, sim: &str) {
    draw_rectangle(16.0, 16.0, 740.0, 326.0, Color::new(0.0, 0.0, 0.0, 0.62));
    draw_rectangle_lines(16.0, 16.0, 740.0, 326.0, 2.0, GREEN);
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
        &format!("Map: {}", asset_index.diagnostics().map_status),
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
    draw_text(&format!("Order: {order}"), 28.0, 156.0, 16.0, YELLOW);
    draw_text(&format!("Combat: {combat}"), 28.0, 176.0, 16.0, ORANGE);
    draw_text(&format!("Sim: {sim}"), 28.0, 196.0, 16.0, MAGENTA);
    draw_text(
        &format!("Decode: {}", asset_index.diagnostics().palette_status),
        28.0,
        220.0,
        15.0,
        SKYBLUE,
    );
    draw_text(
        &format!("Decode: {}", asset_index.diagnostics().tab_status),
        28.0,
        240.0,
        15.0,
        SKYBLUE,
    );
    draw_text(
        &format!("Sprite: {}", asset_index.diagnostics().sprite_status),
        28.0,
        264.0,
        15.0,
        SKYBLUE,
    );
    draw_text(
        &format!("Variant: {}", asset_index.diagnostics().tab_variant_status),
        28.0,
        284.0,
        15.0,
        SKYBLUE,
    );
    draw_text(
        "WASD pan | Wheel zoom | 1-4 select | RMB move | LMB attack | Space pause | . step | +/- speed | F5/F9 | Esc",
        28.0,
        330.0,
        15.0,
        GRAY,
    );

    draw_palette_preview(asset_index.diagnostics().palette_preview.as_slice());
}

fn draw_palette_preview(colors: &[Rgb8]) {
    if colors.is_empty() {
        draw_text("Palette preview unavailable", 28.0, 308.0, 15.0, DARKGRAY);
        return;
    }

    draw_text("Palette", 28.0, 308.0, 15.0, LIGHTGRAY);
    let swatch_size = 12.0;
    for (i, color) in colors.iter().enumerate() {
        draw_rectangle(
            92.0 + i as f32 * swatch_size,
            298.0,
            swatch_size,
            16.0,
            Color::from_rgba(color.r, color.g, color.b, 255),
        );
    }
    draw_rectangle_lines(
        92.0,
        298.0,
        swatch_size * colors.len() as f32,
        16.0,
        1.0,
        GRAY,
    );
}
