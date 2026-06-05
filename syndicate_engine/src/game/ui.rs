use crate::engine::{
    assets::AssetIndex,
    map_decode::{MapInferredLayerPreview, MapSignaturePreview},
    palette_decode::Rgb8,
};
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
        "WASD pan | Wheel zoom | 1-4 select | RMB move | LMB attack | M map views | Space pause | . step | +/- speed | F5/F9 | Esc",
        28.0,
        330.0,
        15.0,
        GRAY,
    );

    draw_palette_preview(asset_index.diagnostics().palette_preview.as_slice());
    draw_map_previews(
        asset_index.diagnostics().map_preview.as_ref(),
        asset_index.diagnostics().map_inferred_preview.as_ref(),
    );
}

fn draw_map_previews(
    signature: Option<&MapSignaturePreview>,
    inferred: Option<&MapInferredLayerPreview>,
) {
    let scale = 1.65;
    let signature_origin = vec2(506.0, 118.0);
    let inferred_origin = vec2(632.0, 118.0);
    draw_signature_minimap(signature_origin, scale, signature);
    draw_inferred_minimap(inferred_origin, scale, inferred);
}

fn draw_signature_minimap(origin: Vec2, scale: f32, preview: Option<&MapSignaturePreview>) {
    draw_text(
        "MAP01 signatures",
        origin.x,
        origin.y - 8.0,
        13.0,
        LIGHTGRAY,
    );

    let Some(preview) = preview else {
        draw_text("unavailable", origin.x, origin.y + 18.0, 13.0, DARKGRAY);
        return;
    };

    for y in 0..preview.height {
        for x in 0..preview.width {
            let class = preview.cell(x, y).unwrap_or(0);
            draw_rectangle(
                origin.x + x as f32 * scale,
                origin.y + y as f32 * scale,
                scale,
                scale,
                signature_color(class),
            );
        }
    }

    draw_rectangle_lines(
        origin.x,
        origin.y,
        preview.width as f32 * scale,
        preview.height as f32 * scale,
        1.0,
        GRAY,
    );
    draw_text(
        &format!(
            "{} classes, top {}%",
            preview.visual_classes,
            preview.dominant_coverage_percent()
        ),
        origin.x,
        origin.y + preview.height as f32 * scale + 14.0,
        13.0,
        LIGHTGRAY,
    );
}

fn draw_inferred_minimap(origin: Vec2, scale: f32, preview: Option<&MapInferredLayerPreview>) {
    draw_text("MAP01 inferred", origin.x, origin.y - 8.0, 13.0, LIGHTGRAY);

    let Some(preview) = preview else {
        draw_text("unavailable", origin.x, origin.y + 18.0, 13.0, DARKGRAY);
        return;
    };

    for y in 0..preview.height {
        for x in 0..preview.width {
            let Some(cell) = preview.cell(x, y) else {
                continue;
            };
            draw_rectangle(
                origin.x + x as f32 * scale,
                origin.y + y as f32 * scale,
                scale,
                scale,
                inferred_color(cell.visual_class, cell.height_class),
            );
        }
    }

    draw_rectangle_lines(
        origin.x,
        origin.y,
        preview.width as f32 * scale,
        preview.height as f32 * scale,
        1.0,
        GRAY,
    );
    draw_text(
        &format!(
            "{} classes, h b{}",
            preview.visual_classes, preview.height_lane
        ),
        origin.x,
        origin.y + preview.height as f32 * scale + 14.0,
        13.0,
        LIGHTGRAY,
    );
}

fn inferred_color(class: u8, height_class: u8) -> Color {
    let base = match class {
        0 => Color::from_rgba(42, 50, 56, 255),
        1 => Color::from_rgba(82, 124, 89, 255),
        2 => Color::from_rgba(126, 116, 76, 255),
        3 => Color::from_rgba(77, 112, 152, 255),
        4 => Color::from_rgba(134, 102, 153, 255),
        5 => Color::from_rgba(165, 107, 70, 255),
        _ => Color::from_rgba(205, 205, 205, 255),
    };
    Color::new(
        (base.r + height_class as f32 * 0.035).min(1.0),
        (base.g + height_class as f32 * 0.035).min(1.0),
        (base.b + height_class as f32 * 0.035).min(1.0),
        base.a,
    )
}

fn signature_color(class: u8) -> Color {
    match class {
        0 => Color::from_rgba(20, 23, 31, 255),
        1 => Color::from_rgba(58, 105, 147, 255),
        2 => Color::from_rgba(74, 137, 92, 255),
        3 => Color::from_rgba(157, 126, 62, 255),
        4 => Color::from_rgba(130, 86, 156, 255),
        5 => Color::from_rgba(158, 79, 80, 255),
        6 => Color::from_rgba(64, 150, 150, 255),
        7 => Color::from_rgba(180, 180, 92, 255),
        8 => Color::from_rgba(99, 105, 190, 255),
        9 => Color::from_rgba(190, 120, 70, 255),
        10 => Color::from_rgba(120, 170, 105, 255),
        11 => Color::from_rgba(170, 105, 150, 255),
        12 => Color::from_rgba(95, 145, 190, 255),
        13 => Color::from_rgba(190, 95, 120, 255),
        14 => Color::from_rgba(130, 150, 80, 255),
        _ => Color::from_rgba(205, 205, 205, 255),
    }
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
