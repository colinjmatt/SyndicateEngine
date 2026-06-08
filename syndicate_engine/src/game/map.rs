use crate::engine::{
    block_texture::IndexedBlockGraphics,
    camera::CameraRig,
    iso::{draw_iso_tile, grid_to_iso},
    map_block_correlation::MapBlockCorrelationScene,
    map_decode::{
        MapCandidateField, MapInferredLayerPreview, MapPrimarySubstrateCandidate,
        MapSignaturePreview,
    },
    map_scene::{MapDiagnosticScene, MapDiagnosticSceneLayer},
    map_tiles::{OriginalMapTiles, OriginalTileTypes},
    mission_scene::{
        OriginalMissionObjectCandidate, OriginalMissionObjectKind, OriginalRuntimeRouteProbe,
        OriginalTilePoint,
    },
    palette,
};
use crate::game::original_graphics::RuntimeOriginalGraphics;
use crate::game::original_sprites::RuntimeOriginalObjectGraphics;
use crate::game::pathfinding::GridPos;
use macroquad::prelude::*;

const ORIGINAL_MAP_VIEWPORT_OVERSCAN_TILES: i32 = 14;
const ORIGINAL_MAP_OUT_OF_BOUNDS_GROUND_TILE: u8 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileKind {
    Road,
    Pavement,
    Roof,
    Water,
}

#[derive(Debug, Clone)]
pub struct TacticalMap {
    pub width: usize,
    pub height: usize,
    tiles: Vec<TileKind>,
}

impl TacticalMap {
    pub fn demo_city() -> Self {
        let width = 28;
        let height = 28;
        let mut tiles = vec![TileKind::Pavement; width * height];
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                tiles[idx] = if x == 13 || y == 14 || (x > 2 && x < 25 && y == 5) {
                    TileKind::Road
                } else if (x > 17 && y > 18) || (x < 4 && y > 20) {
                    TileKind::Water
                } else if (x + y) % 9 == 0
                    || (x > 6 && x < 11 && y > 7 && y < 12)
                    || (x > 16 && x < 22 && y > 8 && y < 13)
                {
                    TileKind::Roof
                } else {
                    TileKind::Pavement
                };
            }
        }
        Self {
            width,
            height,
            tiles,
        }
    }

    pub fn tile(&self, x: usize, y: usize) -> TileKind {
        self.tiles[y * self.width + x]
    }

    pub fn contains(&self, pos: GridPos) -> bool {
        pos.x >= 0 && pos.y >= 0 && (pos.x as usize) < self.width && (pos.y as usize) < self.height
    }

    pub fn tile_pos(&self, pos: GridPos) -> Option<TileKind> {
        self.contains(pos)
            .then(|| self.tile(pos.x as usize, pos.y as usize))
    }

    pub fn is_walkable_pos(&self, pos: GridPos) -> bool {
        self.tile_pos(pos)
            .is_some_and(|tile| !matches!(tile, TileKind::Water | TileKind::Roof))
    }

    pub fn is_road_pos(&self, pos: GridPos) -> bool {
        self.tile_pos(pos)
            .is_some_and(|tile| tile == TileKind::Road)
    }

    pub fn walkable_neighbors(&self, pos: GridPos) -> Vec<GridPos> {
        [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .map(|(dx, dy)| GridPos::new(pos.x + dx, pos.y + dy))
            .filter(|&next| self.is_walkable_pos(next))
            .collect()
    }

    pub fn draw_path(&self, camera: &CameraRig, path: &[GridPos], color: Color) {
        for window in path.windows(2) {
            let a =
                camera.world_to_screen(grid_to_iso(window[0].x as f32, window[0].y as f32, 0.05));
            let b =
                camera.world_to_screen(grid_to_iso(window[1].x as f32, window[1].y as f32, 0.05));
            draw_line(a.x, a.y, b.x, b.y, 3.0, color);
        }
    }

    pub fn draw_marker(&self, camera: &CameraRig, pos: GridPos, color: Color) {
        if !self.contains(pos) {
            return;
        }
        let center = camera.world_to_screen(grid_to_iso(pos.x as f32, pos.y as f32, 0.1));
        draw_circle_lines(center.x, center.y, 14.0 * camera.zoom, 3.0, color);
        draw_line(
            center.x - 18.0,
            center.y,
            center.x + 18.0,
            center.y,
            2.0,
            color,
        );
        draw_line(
            center.x,
            center.y - 10.0,
            center.x,
            center.y + 10.0,
            2.0,
            color,
        );
    }

    pub fn draw(&self, camera: &CameraRig) {
        for y in 0..self.height {
            for x in 0..self.width {
                let kind = self.tile(x, y);
                let z = if matches!(kind, TileKind::Roof) {
                    0.65
                } else {
                    0.0
                };
                let center = camera.world_to_screen(grid_to_iso(x as f32, y as f32, z));
                let color = match kind {
                    TileKind::Road => palette::ROAD,
                    TileKind::Pavement => palette::PAVEMENT,
                    TileKind::Roof => palette::ROOF,
                    TileKind::Water => palette::WATER,
                };
                draw_iso_tile(center, color, Color::new(0.02, 0.025, 0.03, 0.75));
            }
        }
    }

    pub fn draw_signature_preview(&self, camera: &CameraRig, preview: &MapSignaturePreview) {
        for y in 0..preview.height {
            for x in 0..preview.width {
                let class = preview.cell(x, y).unwrap_or(0);
                let center = camera.world_to_screen(grid_to_iso(x as f32, y as f32, 0.0));
                draw_iso_tile(
                    center,
                    signature_tile_color(class),
                    Color::new(0.01, 0.012, 0.016, 0.55),
                );
            }
        }
    }

    pub fn draw_inferred_layer_preview(
        &self,
        camera: &CameraRig,
        preview: &MapInferredLayerPreview,
    ) {
        for y in 0..preview.height {
            for x in 0..preview.width {
                let Some(cell) = preview.cell(x, y) else {
                    continue;
                };
                let z = cell.height_class as f32 * 0.06;
                let center = camera.world_to_screen(grid_to_iso(x as f32, y as f32, z));
                draw_iso_tile(
                    center,
                    inferred_tile_color(cell.visual_class, cell.height_class),
                    Color::new(0.01, 0.012, 0.016, 0.58),
                );
            }
        }
    }

    pub fn draw_candidate_field_preview(
        &self,
        camera: &CameraRig,
        substrate: &MapPrimarySubstrateCandidate,
        field: MapCandidateField,
    ) {
        let Some(evidence) = substrate.evidence_for(field) else {
            return;
        };
        let height_baseline = substrate
            .evidence_for(MapCandidateField::Height)
            .map(|evidence| evidence.baseline)
            .unwrap_or(evidence.baseline);

        for y in 0..substrate.height {
            for x in 0..substrate.width {
                let Some(value) = substrate.field_value(field, x, y) else {
                    continue;
                };
                let height_value = substrate
                    .field_value(MapCandidateField::Height, x, y)
                    .unwrap_or(height_baseline);
                let height_delta = height_value.abs_diff(height_baseline).min(15);
                let z = if field == MapCandidateField::Height {
                    value.abs_diff(evidence.baseline).min(15) as f32 * 0.065
                } else {
                    height_delta as f32 * 0.035
                };
                let center = camera.world_to_screen(grid_to_iso(x as f32, y as f32, z));
                draw_iso_tile(
                    center,
                    candidate_field_color(field, value, evidence.baseline, height_delta),
                    Color::new(0.01, 0.012, 0.016, 0.58),
                );
            }
        }
    }

    pub fn draw_diagnostic_scene(
        &self,
        camera: &CameraRig,
        scene: &MapDiagnosticScene,
        layer: MapDiagnosticSceneLayer,
    ) {
        for y in 0..scene.height {
            for x in 0..scene.width {
                let Some(cell) = scene.cell(x, y) else {
                    continue;
                };
                let height_delta = scene
                    .field_evidence(MapCandidateField::Height)
                    .map(|evidence| cell.height_candidate.abs_diff(evidence.baseline).min(15))
                    .unwrap_or(cell.height_class.min(15));
                let z = match layer {
                    MapDiagnosticSceneLayer::Inferred | MapDiagnosticSceneLayer::Signature => {
                        cell.height_class as f32 * 0.055
                    }
                    MapDiagnosticSceneLayer::CandidateField(MapCandidateField::Height) => {
                        height_delta as f32 * 0.065
                    }
                    MapDiagnosticSceneLayer::CandidateField(_)
                    | MapDiagnosticSceneLayer::BlockAddressability => height_delta as f32 * 0.035,
                };
                let center = camera.world_to_screen(grid_to_iso(x as f32, y as f32, z));
                let color = match layer {
                    MapDiagnosticSceneLayer::Inferred => {
                        inferred_tile_color(cell.visual_class, cell.height_class)
                    }
                    MapDiagnosticSceneLayer::Signature => {
                        signature_tile_color(cell.signature_class.unwrap_or(0))
                    }
                    MapDiagnosticSceneLayer::CandidateField(field) => scene
                        .field_evidence(field)
                        .map(|evidence| {
                            candidate_field_color(
                                field,
                                cell.field_value(field),
                                evidence.baseline,
                                height_delta,
                            )
                        })
                        .unwrap_or_else(|| inferred_tile_color(cell.visual_class, height_delta)),
                    MapDiagnosticSceneLayer::BlockAddressability => {
                        inferred_tile_color(cell.visual_class, height_delta)
                    }
                };
                draw_iso_tile(center, color, Color::new(0.01, 0.012, 0.016, 0.58));
            }
        }
    }

    pub fn draw_block_addressability_scene(
        &self,
        camera: &CameraRig,
        scene: &MapDiagnosticScene,
        correlation: &MapBlockCorrelationScene,
    ) {
        let Some(candidate) = correlation.selected_candidate() else {
            self.draw_diagnostic_scene(camera, scene, MapDiagnosticSceneLayer::Inferred);
            return;
        };
        let field = candidate.field;

        for y in 0..scene.height {
            for x in 0..scene.width {
                let Some(cell) = scene.cell(x, y) else {
                    continue;
                };
                let height_delta = scene
                    .field_evidence(MapCandidateField::Height)
                    .map(|evidence| cell.height_candidate.abs_diff(evidence.baseline).min(15))
                    .unwrap_or(cell.height_class.min(15));
                let z = height_delta as f32 * 0.038;
                let center = camera.world_to_screen(grid_to_iso(x as f32, y as f32, z));
                let value = cell.field_value(field);
                let color = block_addressability_color(
                    value,
                    candidate.baseline,
                    height_delta,
                    candidate.is_value_addressable(value),
                );
                draw_iso_tile(center, color, Color::new(0.01, 0.012, 0.016, 0.60));
            }
        }
    }

    pub fn draw_original_graphics_scene(
        &self,
        camera: &CameraRig,
        scene: &MapDiagnosticScene,
        field: MapCandidateField,
        graphics: &RuntimeOriginalGraphics,
    ) {
        let height_baseline = scene
            .field_evidence(MapCandidateField::Height)
            .map(|evidence| evidence.baseline)
            .unwrap_or_default();

        for y in 0..scene.height {
            for x in 0..scene.width {
                let Some(cell) = scene.cell(x, y) else {
                    continue;
                };
                let record_index = cell.field_value(field) as usize;
                if record_index >= graphics.bank().record_count {
                    continue;
                }

                let height_delta = cell.height_candidate.abs_diff(height_baseline).min(15);
                let z = height_delta as f32 * 0.040;
                let center = camera.world_to_screen(grid_to_iso(x as f32, y as f32, z));
                let size = vec2(
                    graphics.bank().record_width as f32 * camera.zoom * 0.42,
                    graphics.bank().record_height as f32 * camera.zoom * 0.42,
                );
                let top_left = vec2(center.x - size.x * 0.5, center.y - size.y * 0.68);
                if top_left.x > screen_width() + size.x
                    || top_left.y > screen_height() + size.y
                    || top_left.x + size.x < -size.x
                    || top_left.y + size.y < -size.y
                {
                    continue;
                }

                graphics.draw_record(record_index, top_left, size, WHITE);
            }
        }
    }

    pub fn draw_original_map_tiles(
        &self,
        camera: &CameraRig,
        map_tiles: &OriginalMapTiles,
        _tile_types: Option<&OriginalTileTypes>,
        graphics: &RuntimeOriginalGraphics,
    ) {
        let tile_size = vec2(
            graphics.bank().record_width as f32 * camera.zoom,
            graphics.bank().record_height as f32 * camera.zoom,
        );
        let viewport = OriginalMapViewport::from_camera(camera);
        let draw_plan = OriginalMapDrawPlan::for_viewport(
            map_tiles,
            &viewport,
            graphics.bank().record_width as f32,
            graphics.bank().record_height as f32,
        );

        for item in draw_plan.items() {
            let Some(tile_index) = original_map_tile_index(map_tiles, item.x, item.y, item.z)
            else {
                continue;
            };
            if tile_index as usize >= graphics.bank().record_count
                || !is_renderable_original_tile(tile_index, graphics.bank())
            {
                continue;
            }

            let top_left = camera.world_to_screen(item.world_top_left);
            if top_left.x > screen_width() + tile_size.x
                || top_left.y > screen_height() + tile_size.y
                || top_left.x + tile_size.x < -tile_size.x
                || top_left.y + tile_size.y < -tile_size.y
            {
                continue;
            }

            graphics.draw_record(tile_index as usize, top_left, tile_size, WHITE);
        }
    }

    pub fn draw_original_mission_scene(
        &self,
        camera: &CameraRig,
        map_tiles: &OriginalMapTiles,
        _tile_types: Option<&OriginalTileTypes>,
        graphics: &RuntimeOriginalGraphics,
        scene_model: &crate::engine::mission_scene::OriginalMissionScene,
        object_graphics: Option<&RuntimeOriginalObjectGraphics>,
        object_animation_frame: u16,
        controlled_ped_record_indices: &[u16],
    ) {
        let tile_size = vec2(
            graphics.bank().record_width as f32 * camera.zoom,
            graphics.bank().record_height as f32 * camera.zoom,
        );
        let viewport = OriginalMapViewport::from_camera(camera);
        let draw_plan = OriginalMapDrawPlan::for_viewport(
            map_tiles,
            &viewport,
            graphics.bank().record_width as f32,
            graphics.bank().record_height as f32,
        );

        for item in draw_plan.items() {
            let top_left = camera.world_to_screen(item.world_top_left);
            if top_left.x > screen_width() + tile_size.x
                || top_left.y > screen_height() + tile_size.y
                || top_left.x + tile_size.x < -tile_size.x
                || top_left.y + tile_size.y < -tile_size.y
            {
                continue;
            }

            if let Some(tile_index) = original_map_tile_index(map_tiles, item.x, item.y, item.z) {
                if (tile_index as usize) < graphics.bank().record_count
                    && is_renderable_original_tile(tile_index, graphics.bank())
                {
                    graphics.draw_record(tile_index as usize, top_left, tile_size, WHITE);
                }
            }

            if let Some(object_graphics) = object_graphics {
                if let Some(object_z) = item.z.checked_sub(1) {
                    for object in original_candidates_for_tile(
                        scene_model,
                        item.x as u16,
                        item.y as u16,
                        object_z as u16,
                    ) {
                        if original_object_hidden_by_controlled_agent(
                            object,
                            controlled_ped_record_indices,
                        ) {
                            continue;
                        }
                        object_graphics.draw_mission_object(
                            object,
                            top_left,
                            tile_size,
                            camera.zoom,
                            object_animation_frame,
                        );
                    }
                }
            }
        }
    }

    pub fn pick_original_tile_at_screen(
        &self,
        camera: &CameraRig,
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
        screen: Vec2,
    ) -> Option<OriginalTilePoint> {
        self.pick_original_tile_at_screen_with_preferred(camera, map_tiles, graphics, screen, None)
    }

    pub fn pick_original_tile_at_screen_with_preferred(
        &self,
        camera: &CameraRig,
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
        screen: Vec2,
        preferred: Option<OriginalTilePoint>,
    ) -> Option<OriginalTilePoint> {
        let world = camera.screen_to_world(screen);
        let tile_width = graphics.bank().record_width as f32;
        let tile_height = graphics.bank().record_height as f32;
        Self::pick_original_tile_at_world(
            map_tiles,
            graphics.bank(),
            world,
            preferred,
            14.0 / camera.zoom.max(0.01),
            tile_width,
            tile_height,
        )
    }

    pub fn original_tile_point_screen(
        &self,
        camera: &CameraRig,
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
        tile: OriginalTilePoint,
    ) -> Vec2 {
        original_tile_local_marker_screen(
            camera,
            map_tiles,
            tile,
            graphics.bank().record_width as f32,
            graphics.bank().record_height as f32,
        )
    }

    fn pick_original_tile_at_world(
        map_tiles: &OriginalMapTiles,
        graphics: &IndexedBlockGraphics,
        world: Vec2,
        preferred: Option<OriginalTilePoint>,
        preferred_radius_world: f32,
        tile_width: f32,
        tile_height: f32,
    ) -> Option<OriginalTilePoint> {
        if let Some(preferred) = preferred {
            let marker =
                original_tile_local_marker_world(map_tiles, preferred, tile_width, tile_height);
            if marker.distance(world) <= preferred_radius_world
                && original_tile_is_pickable(map_tiles, graphics, preferred)
            {
                return Some(preferred);
            }
        }

        for z in (0..map_tiles.height).rev() {
            let (tile_x, tile_y) =
                original_screen_to_tile_at_z(map_tiles, world, z, tile_width, tile_height);
            let x = tile_x.floor() as i32;
            let y = tile_y.floor() as i32;
            if x < 0 || y < 0 || (x as usize) >= map_tiles.width || (y as usize) >= map_tiles.depth
            {
                continue;
            }
            let Some(tile_index) = map_tiles.tile_at(x as usize, y as usize, z) else {
                continue;
            };
            if !is_renderable_original_tile(tile_index, graphics) {
                continue;
            }
            let off_x = ((tile_x - x as f32) * 255.0).round().clamp(0.0, 255.0) as u8;
            let off_y = ((tile_y - y as f32) * 255.0).round().clamp(0.0, 255.0) as u8;
            return Some(OriginalTilePoint {
                tile_x: x as u16,
                tile_y: y as u16,
                tile_z: z as u16,
                off_x,
                off_y,
                off_z: 0,
            });
        }
        None
    }

    pub fn draw_original_route_probe_overlay(
        &self,
        camera: &CameraRig,
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
        cursor_tile: Option<OriginalTilePoint>,
        route_probe: Option<&OriginalRuntimeRouteProbe>,
        cursor_screen: Option<Vec2>,
    ) {
        let tile_width = graphics.bank().record_width as f32;
        let tile_height = graphics.bank().record_height as f32;

        if let Some(route_probe) = route_probe {
            for pair in route_probe.path.windows(2) {
                let a = original_tile_marker_screen(
                    camera,
                    map_tiles,
                    pair[0],
                    tile_width,
                    tile_height,
                );
                let b = original_tile_marker_screen(
                    camera,
                    map_tiles,
                    pair[1],
                    tile_width,
                    tile_height,
                );
                draw_line(a.x, a.y, b.x, b.y, 2.0, Color::new(0.0, 0.9, 0.95, 0.82));
            }
            if let Some(start) = route_probe.start_tile {
                let p =
                    original_tile_marker_screen(camera, map_tiles, start, tile_width, tile_height);
                draw_circle_lines(p.x, p.y, 7.0, 2.0, GREEN);
            }
            if let Some(goal) = route_probe.goal_tile {
                let p =
                    original_tile_marker_screen(camera, map_tiles, goal, tile_width, tile_height);
                let color = if route_probe
                    .requested_goal_tile
                    .is_some_and(|requested| requested.key_tuple() != goal.key_tuple())
                {
                    SKYBLUE
                } else {
                    YELLOW
                };
                draw_circle_lines(p.x, p.y, 7.0, 2.0, color);
            }
            if let Some(requested) = route_probe.requested_goal_tile {
                let p = original_tile_local_marker_screen(
                    camera,
                    map_tiles,
                    requested,
                    tile_width,
                    tile_height,
                );
                draw_circle_lines(p.x, p.y, 10.0, 2.5, YELLOW);
                draw_circle(p.x, p.y, 2.5, YELLOW);
            }
        }

        if let Some(cursor_tile) = cursor_tile {
            let p = cursor_screen.unwrap_or_else(|| {
                original_tile_local_marker_screen(
                    camera,
                    map_tiles,
                    cursor_tile,
                    tile_width,
                    tile_height,
                )
            });
            draw_circle_lines(p.x, p.y, 9.0, 2.0, ORANGE);
            draw_circle(p.x, p.y, 2.5, ORANGE);
        }
    }

    pub fn draw_original_debug_agent_marker(
        &self,
        camera: &CameraRig,
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
        object_graphics: Option<&RuntimeOriginalObjectGraphics>,
        object: Option<&OriginalMissionObjectCandidate>,
        anchor_tile: OriginalTilePoint,
        route: &[OriginalTilePoint],
        progress: f32,
        selected: bool,
        label: &str,
        animation_frame: u16,
    ) {
        let tile_width = graphics.bank().record_width as f32;
        let tile_height = graphics.bank().record_height as f32;
        let tile_size = vec2(tile_width * camera.zoom, tile_height * camera.zoom);
        let (current, next, t) = original_route_progress_sample(anchor_tile, route, progress);
        let a = original_tile_marker_screen(camera, map_tiles, current, tile_width, tile_height);
        let b = original_tile_marker_screen(camera, map_tiles, next, tile_width, tile_height);
        let p = a.lerp(b, t);

        if selected {
            for pair in route.windows(2) {
                let from = original_tile_marker_screen(
                    camera,
                    map_tiles,
                    pair[0],
                    tile_width,
                    tile_height,
                );
                let to = original_tile_marker_screen(
                    camera,
                    map_tiles,
                    pair[1],
                    tile_width,
                    tile_height,
                );
                draw_line(
                    from.x,
                    from.y,
                    to.x,
                    to.y,
                    2.0,
                    Color::new(0.1, 0.95, 1.0, 0.72),
                );
            }
        }

        let mut drew_sprite = false;
        if let (Some(object_graphics), Some(object)) = (object_graphics, object) {
            let object_z = current.tile_z.saturating_add(1) as f32;
            let current_top_left = camera.world_to_screen(original_map_tile_world_top_left(
                map_tiles,
                current.tile_x as f32,
                current.tile_y as f32,
                object_z,
                tile_width,
                tile_height,
            ));
            let next_top_left = camera.world_to_screen(original_map_tile_world_top_left(
                map_tiles,
                next.tile_x as f32,
                next.tile_y as f32,
                next.tile_z.saturating_add(1) as f32,
                tile_width,
                tile_height,
            ));
            drew_sprite = object_graphics.draw_mission_object(
                object,
                current_top_left.lerp(next_top_left, t),
                tile_size,
                camera.zoom,
                animation_frame,
            );
        }

        let color = if selected {
            Color::new(0.0, 0.95, 1.0, 0.90)
        } else {
            Color::new(0.2, 0.75, 1.0, 0.54)
        };
        let radius = if selected { 12.0 } else { 8.0 };
        draw_circle_lines(p.x, p.y, radius, 2.0, color);
        if !drew_sprite {
            draw_circle(p.x, p.y, if selected { 4.5 } else { 3.0 }, color);
        } else if selected {
            draw_circle_lines(p.x, p.y, 17.0, 1.5, Color::new(1.0, 1.0, 0.2, 0.86));
        }
        if selected {
            draw_text(label, p.x + 10.0, p.y - 12.0, 12.0, color);
        }
    }

    pub fn draw_original_debug_interaction_overlay(
        &self,
        camera: &CameraRig,
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
        target_tile: Option<OriginalTilePoint>,
        label: &str,
        ready: bool,
    ) {
        let Some(target_tile) = target_tile else {
            return;
        };
        let tile_width = graphics.bank().record_width as f32;
        let tile_height = graphics.bank().record_height as f32;
        let p =
            original_tile_marker_screen(camera, map_tiles, target_tile, tile_width, tile_height);
        let color = if ready {
            Color::new(0.2, 1.0, 0.3, 0.86)
        } else {
            Color::new(1.0, 0.75, 0.1, 0.86)
        };
        draw_circle_lines(p.x, p.y, 15.0, 2.5, color);
        draw_circle_lines(p.x, p.y, 21.0, 1.5, color);
        draw_text(label, p.x + 12.0, p.y + 16.0, 12.0, color);
    }

    pub fn draw_original_combat_target_overlay(
        &self,
        camera: &CameraRig,
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
        target_tile: OriginalTilePoint,
        hp_label: &str,
        objective_complete: bool,
        defeated: bool,
    ) {
        let tile_width = graphics.bank().record_width as f32;
        let tile_height = graphics.bank().record_height as f32;
        let p =
            original_tile_marker_screen(camera, map_tiles, target_tile, tile_width, tile_height);
        let color = if objective_complete {
            Color::new(0.15, 1.0, 0.25, 0.92)
        } else if defeated {
            Color::new(0.70, 0.70, 0.75, 0.82)
        } else {
            Color::new(1.0, 0.12, 0.08, 0.92)
        };
        draw_circle_lines(p.x, p.y, 13.0, 2.0, color);
        draw_circle_lines(p.x, p.y, 19.0, 1.5, color);
        if defeated {
            draw_line(p.x - 10.0, p.y - 10.0, p.x + 10.0, p.y + 10.0, 2.0, color);
            draw_line(p.x + 10.0, p.y - 10.0, p.x - 10.0, p.y + 10.0, 2.0, color);
        } else {
            draw_circle(p.x, p.y, 3.0, color);
        }
        draw_text("TARGET", p.x + 12.0, p.y - 16.0, 12.0, color);
        draw_text(hp_label, p.x + 12.0, p.y - 2.0, 11.0, color);
    }

    pub fn draw_original_ped_candidate_overlay(
        &self,
        camera: &CameraRig,
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
        tile: OriginalTilePoint,
        label: &str,
        color: Color,
        defeated: bool,
    ) {
        let tile_width = graphics.bank().record_width as f32;
        let tile_height = graphics.bank().record_height as f32;
        let p = original_tile_marker_screen(camera, map_tiles, tile, tile_width, tile_height);
        draw_circle_lines(p.x, p.y, 10.0, 1.5, color);
        if defeated {
            draw_line(p.x - 7.0, p.y - 7.0, p.x + 7.0, p.y + 7.0, 1.8, color);
            draw_line(p.x + 7.0, p.y - 7.0, p.x - 7.0, p.y + 7.0, 1.8, color);
        } else {
            draw_circle(p.x, p.y, 2.0, color);
        }
        draw_text(label, p.x + 9.0, p.y + 11.0, 10.5, color);
    }

    pub fn draw_original_combat_feedback_overlay(
        &self,
        camera: &CameraRig,
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
        origins: &[OriginalTilePoint],
        target_tile: OriginalTilePoint,
        label: &str,
        color: Color,
        fade: f32,
    ) {
        if origins.is_empty() {
            return;
        }
        let tile_width = graphics.bank().record_width as f32;
        let tile_height = graphics.bank().record_height as f32;
        let alpha = fade.clamp(0.0, 1.0);
        let color = Color::new(color.r, color.g, color.b, color.a * alpha);
        let target =
            original_tile_marker_screen(camera, map_tiles, target_tile, tile_width, tile_height);
        for origin_tile in origins {
            let origin = original_tile_marker_screen(
                camera,
                map_tiles,
                *origin_tile,
                tile_width,
                tile_height,
            );
            draw_line(origin.x, origin.y, target.x, target.y, 2.0, color);
            draw_circle_lines(origin.x, origin.y, 7.0, 1.3, color);
        }
        let pulse = 15.0 + (1.0 - alpha) * 10.0;
        draw_circle_lines(target.x, target.y, pulse, 2.2, color);
        draw_text(label, target.x + 12.0, target.y + 18.0, 11.0, color);
    }
}

fn original_candidates_for_tile(
    scene_model: &crate::engine::mission_scene::OriginalMissionScene,
    tile_x: u16,
    tile_y: u16,
    tile_z: u16,
) -> Vec<&crate::engine::mission_scene::OriginalMissionObjectCandidate> {
    scene_model
        .draw_queue
        .entries()
        .iter()
        .filter(|entry| {
            entry.tile.tile_x == tile_x
                && entry.tile.tile_y == tile_y
                && entry.tile.tile_z == tile_z
        })
        .filter_map(|entry| {
            scene_model.objects.iter().find(|object| {
                object.kind == entry.kind
                    && object.record_index == entry.record_index
                    && object.candidate_draw
            })
        })
        .collect()
}

fn original_object_hidden_by_controlled_agent(
    object: &OriginalMissionObjectCandidate,
    controlled_ped_record_indices: &[u16],
) -> bool {
    object.kind == OriginalMissionObjectKind::Ped
        && controlled_ped_record_indices.contains(&object.record_index)
}

fn original_route_progress_sample(
    anchor_tile: OriginalTilePoint,
    route: &[OriginalTilePoint],
    progress: f32,
) -> (OriginalTilePoint, OriginalTilePoint, f32) {
    if route.is_empty() {
        return (anchor_tile, anchor_tile, 0.0);
    }
    let max_index = route.len().saturating_sub(1);
    let clamped = progress.clamp(0.0, max_index as f32);
    let index = clamped.floor() as usize;
    let t = clamped - index as f32;
    (
        route[index.min(max_index)],
        route[(index + 1).min(max_index)],
        t,
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OriginalMapViewport {
    pub origin: Vec2,
    pub size: Vec2,
    pub zoom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct OriginalMapDrawItem {
    x: i32,
    y: i32,
    z: usize,
    world_top_left: Vec2,
}

#[derive(Debug, Clone, PartialEq)]
struct OriginalMapDrawPlan {
    items: Vec<OriginalMapDrawItem>,
}

impl OriginalMapViewport {
    fn from_camera(camera: &CameraRig) -> Self {
        Self {
            origin: camera.screen_to_world(vec2(0.0, 0.0)),
            size: vec2(screen_width(), screen_height()) / camera.zoom,
            zoom: camera.zoom,
        }
    }
}

impl OriginalMapDrawPlan {
    fn for_viewport(
        map_tiles: &OriginalMapTiles,
        viewport: &OriginalMapViewport,
        tile_width: f32,
        tile_height: f32,
    ) -> Self {
        let start_tile =
            original_screen_to_tile(map_tiles, viewport.origin, tile_width, tile_height);
        let sw = start_tile.0.floor() as i32 - ORIGINAL_MAP_VIEWPORT_OVERSCAN_TILES;
        let sh = start_tile.1.floor() as i32 - ORIGINAL_MAP_VIEWPORT_OVERSCAN_TILES;
        let max_tz = map_tiles.height as i32 + 1;
        let step_x = tile_width * 0.5;
        let step_y = tile_height / 3.0;
        let chk = (viewport.size.x / step_x).ceil() as i32
            + 2
            + (viewport.size.y / step_y).ceil() as i32
            + max_tz * 2
            + ORIGINAL_MAP_VIEWPORT_OVERSCAN_TILES * 2;
        let shm = sh + chk;
        let chky = sh.max(0);
        let min_tile_y = chky - ORIGINAL_MAP_VIEWPORT_OVERSCAN_TILES;
        let max_tile_x = map_tiles.width as i32 + ORIGINAL_MAP_VIEWPORT_OVERSCAN_TILES;
        let zr = shm + max_tz + 1;
        let mut items = Vec::new();

        for inc in 0..zr {
            let ye = sh + inc;
            let ys = ye - max_tz - 2;
            let mut tile_z = max_tz + 1;
            for yb in ys..ye {
                if yb < 0 || yb < sh || yb >= shm {
                    tile_z -= 1;
                    continue;
                }

                let mut tile_y = yb;
                let mut tile_x = sw;
                while tile_y >= min_tile_y && tile_x < max_tile_x {
                    if tile_z >= 0
                        && (tile_z as usize) < map_tiles.height
                        && original_map_tile_index(map_tiles, tile_x, tile_y, tile_z as usize)
                            .is_some()
                    {
                        let world_top_left = original_map_tile_world_top_left(
                            map_tiles,
                            tile_x as f32,
                            tile_y as f32,
                            tile_z as f32,
                            tile_width,
                            tile_height,
                        );
                        if world_tile_intersects_viewport(
                            world_top_left,
                            vec2(tile_width, tile_height),
                            viewport,
                        ) {
                            items.push(OriginalMapDrawItem {
                                x: tile_x,
                                y: tile_y,
                                z: tile_z as usize,
                                world_top_left,
                            });
                        }
                    }

                    tile_x += 1;
                    tile_y -= 1;
                }
                tile_z -= 1;
            }
        }

        Self { items }
    }

    fn items(&self) -> &[OriginalMapDrawItem] {
        &self.items
    }
}

trait OriginalTilePointKey {
    fn key_tuple(self) -> (u16, u16, u16);
}

impl OriginalTilePointKey for OriginalTilePoint {
    fn key_tuple(self) -> (u16, u16, u16) {
        (self.tile_x, self.tile_y, self.tile_z)
    }
}

fn original_map_tile_index(map_tiles: &OriginalMapTiles, x: i32, y: i32, z: usize) -> Option<u8> {
    if x >= 0 && y >= 0 && (x as usize) < map_tiles.width && (y as usize) < map_tiles.depth {
        return map_tiles.tile_at(x as usize, y as usize, z);
    }

    if z < 2 {
        return Some(ORIGINAL_MAP_OUT_OF_BOUNDS_GROUND_TILE);
    }

    None
}

pub fn original_map_tile_world_top_left(
    map_tiles: &OriginalMapTiles,
    x: f32,
    y: f32,
    z: f32,
    tile_width: f32,
    tile_height: f32,
) -> Vec2 {
    let step_y = tile_height / 3.0;
    vec2(
        (map_tiles.width as f32 + (x - y)) * tile_width * 0.5,
        ((map_tiles.height as f32 + 1.0 + x + y) - (z - 1.0)) * step_y,
    )
}

fn original_screen_to_tile(
    map_tiles: &OriginalMapTiles,
    screen: Vec2,
    tile_width: f32,
    tile_height: f32,
) -> (f32, f32) {
    let x = screen.x - (map_tiles.width as f32 + 1.0) * tile_width * 0.5;
    let y = screen.y - (map_tiles.height as f32 + 2.0) * tile_height / 3.0;
    let dx = x / (tile_width * 0.5);
    let dy = y / (tile_height / 3.0);
    ((dx + dy) * 0.5, (dy - dx) * 0.5)
}

fn original_screen_to_tile_at_z(
    map_tiles: &OriginalMapTiles,
    screen: Vec2,
    tile_z: usize,
    tile_width: f32,
    tile_height: f32,
) -> (f32, f32) {
    let dx = screen.x / (tile_width * 0.5) - (map_tiles.width as f32 + 1.0);
    let dy = screen.y / (tile_height / 3.0) - (map_tiles.height as f32 + 3.0) + tile_z as f32;
    ((dx + dy) * 0.5, (dy - dx) * 0.5)
}

fn original_tile_marker_screen(
    camera: &CameraRig,
    map_tiles: &OriginalMapTiles,
    tile: OriginalTilePoint,
    tile_width: f32,
    tile_height: f32,
) -> Vec2 {
    let draw_z = tile.tile_z.saturating_add(1) as f32;
    let top_left = original_map_tile_world_top_left(
        map_tiles,
        tile.tile_x as f32,
        tile.tile_y as f32,
        draw_z,
        tile_width,
        tile_height,
    );
    camera.world_to_screen(top_left) + vec2(tile_width * 0.5, tile_height * 2.0 / 3.0) * camera.zoom
}

fn original_tile_local_marker_screen(
    camera: &CameraRig,
    map_tiles: &OriginalMapTiles,
    tile: OriginalTilePoint,
    tile_width: f32,
    tile_height: f32,
) -> Vec2 {
    camera.world_to_screen(original_tile_local_marker_world(
        map_tiles,
        tile,
        tile_width,
        tile_height,
    ))
}

fn original_tile_local_marker_world(
    map_tiles: &OriginalMapTiles,
    tile: OriginalTilePoint,
    tile_width: f32,
    tile_height: f32,
) -> Vec2 {
    let draw_z = tile.tile_z.saturating_add(1) as f32;
    let top_left = original_map_tile_world_top_left(
        map_tiles,
        tile.tile_x as f32,
        tile.tile_y as f32,
        draw_z,
        tile_width,
        tile_height,
    );
    let frac_x = tile.off_x as f32 / 255.0;
    let frac_y = tile.off_y as f32 / 255.0;
    let local = vec2(
        tile_width * 0.5 + (frac_x - frac_y) * tile_width * 0.5,
        tile_height * 2.0 / 3.0 + (frac_x + frac_y) * tile_height / 3.0,
    );
    top_left + local
}

fn world_tile_intersects_viewport(
    top_left: Vec2,
    size: Vec2,
    viewport: &OriginalMapViewport,
) -> bool {
    top_left.x <= viewport.origin.x + viewport.size.x + size.x
        && top_left.y <= viewport.origin.y + viewport.size.y + size.y
        && top_left.x + size.x >= viewport.origin.x - size.x
        && top_left.y + size.y >= viewport.origin.y - size.y
}

fn is_renderable_original_tile(tile_index: u8, graphics: &IndexedBlockGraphics) -> bool {
    if tile_index == 0 {
        return false;
    }

    graphics.record_has_visible_pixels(tile_index as usize)
}

fn original_tile_is_pickable(
    map_tiles: &OriginalMapTiles,
    graphics: &IndexedBlockGraphics,
    tile: OriginalTilePoint,
) -> bool {
    let x = tile.tile_x as usize;
    let y = tile.tile_y as usize;
    let z = tile.tile_z as usize;
    if x >= map_tiles.width || y >= map_tiles.depth || z >= map_tiles.height {
        return false;
    }
    map_tiles
        .tile_at(x, y, z)
        .is_some_and(|tile_index| is_renderable_original_tile(tile_index, graphics))
}

fn candidate_field_color(
    field: MapCandidateField,
    value: u8,
    baseline: u8,
    height_delta: u8,
) -> Color {
    if value == baseline {
        return brighten(
            Color::from_rgba(35, 41, 47, 255),
            height_delta as f32 * 0.02,
        );
    }

    let hue = value.wrapping_mul(37).wrapping_add(match field {
        MapCandidateField::SurfaceIndex => 11,
        MapCandidateField::DetailIndex => 67,
        MapCandidateField::Reference => 131,
        MapCandidateField::Height => 193,
    });
    let intensity = value.abs_diff(baseline) as f32 / 255.0;
    let base = match field {
        MapCandidateField::SurfaceIndex => Color::new(
            0.18 + intensity * 0.30,
            0.34 + hue as f32 / 255.0 * 0.42,
            0.24 + intensity * 0.24,
            1.0,
        ),
        MapCandidateField::DetailIndex => Color::new(
            0.35 + hue as f32 / 255.0 * 0.35,
            0.30 + intensity * 0.20,
            0.18 + intensity * 0.20,
            1.0,
        ),
        MapCandidateField::Reference => Color::new(
            0.22 + intensity * 0.16,
            0.34 + intensity * 0.22,
            0.42 + hue as f32 / 255.0 * 0.36,
            1.0,
        ),
        MapCandidateField::Height => Color::new(
            0.30 + intensity * 0.36,
            0.22 + height_delta as f32 * 0.025,
            0.42 + hue as f32 / 255.0 * 0.30,
            1.0,
        ),
    };
    brighten(base, height_delta as f32 * 0.025)
}

fn inferred_tile_color(class: u8, height_class: u8) -> Color {
    let base = match class {
        0 => Color::from_rgba(42, 50, 56, 255),
        1 => Color::from_rgba(82, 124, 89, 255),
        2 => Color::from_rgba(126, 116, 76, 255),
        3 => Color::from_rgba(77, 112, 152, 255),
        4 => Color::from_rgba(134, 102, 153, 255),
        5 => Color::from_rgba(165, 107, 70, 255),
        _ => Color::from_rgba(205, 205, 205, 255),
    };
    brighten(base, height_class as f32 * 0.035)
}

fn brighten(color: Color, amount: f32) -> Color {
    Color::new(
        (color.r + amount).min(1.0),
        (color.g + amount).min(1.0),
        (color.b + amount).min(1.0),
        color.a,
    )
}

fn block_addressability_color(
    value: u8,
    baseline: u8,
    height_delta: u8,
    addressable: Option<bool>,
) -> Color {
    let intensity = value.abs_diff(baseline) as f32 / 255.0;
    let base = match addressable {
        Some(true) if value == baseline => Color::from_rgba(43, 54, 56, 255),
        Some(true) => Color::new(0.16 + intensity * 0.22, 0.50, 0.42 + intensity * 0.22, 1.0),
        Some(false) => Color::new(0.60, 0.18 + intensity * 0.16, 0.16, 1.0),
        None => Color::from_rgba(82, 82, 88, 255),
    };
    brighten(base, height_delta as f32 * 0.025)
}

fn signature_tile_color(class: u8) -> Color {
    match class {
        0 => Color::from_rgba(18, 20, 27, 255),
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

#[cfg(test)]
mod tests {
    use super::{
        OriginalMapDrawPlan, OriginalMapViewport, TacticalMap, is_renderable_original_tile,
        original_map_tile_index, original_map_tile_world_top_left,
        original_object_hidden_by_controlled_agent, original_screen_to_tile,
        original_screen_to_tile_at_z, original_tile_local_marker_world,
    };
    use crate::engine::mission_scene::{
        OriginalAnimationRefs, OriginalDrawStage, OriginalMissionObjectCandidate,
        OriginalMissionObjectKind, OriginalTilePoint,
    };
    use crate::engine::{
        block_texture::IndexedBlockGraphics, map_tiles::OriginalMapTiles, palette_decode::Palette,
    };
    use macroquad::prelude::*;

    #[test]
    fn renders_tiles_by_runtime_pixel_visibility_not_col_type() {
        let graphics = synthetic_graphics_bank();

        assert!(!is_renderable_original_tile(0, &graphics));
        assert!(is_renderable_original_tile(1, &graphics));
        assert!(!is_renderable_original_tile(2, &graphics));
    }

    #[test]
    fn hides_base_scene_peds_that_are_controlled_debug_agents() {
        let controlled = [0, 3];
        let controlled_ped = synthetic_object(OriginalMissionObjectKind::Ped, 0);
        let other_ped = synthetic_object(OriginalMissionObjectKind::Ped, 4);
        let static_object = synthetic_object(OriginalMissionObjectKind::Static, 0);

        assert!(original_object_hidden_by_controlled_agent(
            &controlled_ped,
            &controlled
        ));
        assert!(!original_object_hidden_by_controlled_agent(
            &other_ped,
            &controlled
        ));
        assert!(!original_object_hidden_by_controlled_agent(
            &static_object,
            &controlled
        ));
    }

    #[test]
    fn original_projection_matches_freesynd_tile_step() {
        let map = synthetic_map_tiles(4, 4, 3);

        let ground = original_map_tile_world_top_left(&map, 1.0, 2.0, 0.0, 64.0, 48.0);
        let upper = original_map_tile_world_top_left(&map, 1.0, 2.0, 1.0, 64.0, 48.0);

        assert_eq!(ground.x, 96.0);
        assert_eq!(ground.y - upper.y, 16.0);
    }

    #[test]
    fn original_screen_to_tile_inverse_matches_renderer_basis() {
        let map = synthetic_map_tiles(8, 8, 3);
        let tile_width = 64.0;
        let tile_height = 48.0;
        let step_y = tile_height / 3.0;
        let screen = vec2(
            (map.width as f32 + 1.0 + 3.0 - 5.0) * tile_width * 0.5,
            (map.height as f32 + 2.0 + 3.0 + 5.0) * step_y,
        );

        let (x, y) = original_screen_to_tile(&map, screen, tile_width, tile_height);

        assert!((x - 3.0).abs() < 0.001);
        assert!((y - 5.0).abs() < 0.001);
    }

    #[test]
    fn original_z_aware_cursor_pick_inverse_matches_route_marker_basis() {
        let map = synthetic_map_tiles(8, 8, 4);
        let tile_width = 64.0;
        let tile_height = 48.0;
        let marker = original_map_tile_world_top_left(&map, 3.0, 5.0, 3.0, tile_width, tile_height)
            + vec2(tile_width * 0.5, tile_height * 2.0 / 3.0);

        let (x, y) = original_screen_to_tile_at_z(&map, marker, 2, tile_width, tile_height);

        assert!((x - 3.0).abs() < 0.001);
        assert!((y - 5.0).abs() < 0.001);
    }

    #[test]
    fn original_local_cursor_marker_roundtrips_fractional_pick() {
        let map = synthetic_map_tiles(8, 8, 4);
        let tile_width = 64.0;
        let tile_height = 48.0;
        let tile = OriginalTilePoint {
            tile_x: 3,
            tile_y: 5,
            tile_z: 2,
            off_x: 64,
            off_y: 191,
            off_z: 0,
        };

        let marker = original_tile_local_marker_world(&map, tile, tile_width, tile_height);
        let (x, y) = original_screen_to_tile_at_z(&map, marker, 2, tile_width, tile_height);

        assert!((x - (3.0 + 64.0 / 255.0)).abs() < 0.001);
        assert!((y - (5.0 + 191.0 / 255.0)).abs() < 0.001);
    }

    #[test]
    fn preferred_original_pick_keeps_existing_yellow_marker_tile() {
        let map = synthetic_map_tiles(8, 8, 4);
        let graphics = synthetic_hblk_sized_graphics_bank();
        let tile_width = 64.0;
        let tile_height = 48.0;
        let preferred = OriginalTilePoint {
            tile_x: 3,
            tile_y: 5,
            tile_z: 1,
            off_x: 128,
            off_y: 128,
            off_z: 0,
        };
        let marker = original_tile_local_marker_world(&map, preferred, tile_width, tile_height);

        let picked = TacticalMap::pick_original_tile_at_world(
            &map,
            &graphics,
            marker,
            Some(preferred),
            14.0,
            tile_width,
            tile_height,
        );

        assert_eq!(picked, Some(preferred));
    }

    #[test]
    fn viewport_draw_plan_keeps_only_map_or_low_z_fallbacks_without_asset_bytes() {
        let map = synthetic_map_tiles(5, 5, 3);
        let viewport = OriginalMapViewport {
            origin: vec2(0.0, 0.0),
            size: vec2(640.0, 480.0),
            zoom: 1.0,
        };

        let plan = OriginalMapDrawPlan::for_viewport(&map, &viewport, 64.0, 48.0);

        assert!(!plan.items().is_empty());
        assert!(plan.items().iter().all(|item| {
            let inside = item.x >= 0
                && item.y >= 0
                && (item.x as usize) < map.width
                && (item.y as usize) < map.depth;
            inside || item.z < 2
        }));
        assert!(plan.items().iter().all(|item| item.z < map.height));
    }

    #[test]
    fn out_of_bounds_low_z_uses_freesynd_ground_fallback_without_asset_bytes() {
        let map = synthetic_map_tiles(5, 5, 3);

        assert_eq!(original_map_tile_index(&map, 2, 2, 0), Some(1));
        assert_eq!(original_map_tile_index(&map, -1, 2, 0), Some(6));
        assert_eq!(original_map_tile_index(&map, 5, 2, 1), Some(6));
        assert_eq!(original_map_tile_index(&map, -1, 2, 2), None);
    }

    fn synthetic_graphics_bank() -> IndexedBlockGraphics {
        let palette = Palette::decode_vga_6bit(&vec![0u8; 768]).unwrap();
        let decoded = vec![0u8, 1, 0];
        IndexedBlockGraphics::from_fixed_records(
            "synthetic/HBLK01.DAT".to_string(),
            "synthetic/HPAL02.DAT".to_string(),
            1,
            1,
            0,
            &decoded,
            &palette,
        )
        .unwrap()
    }

    fn synthetic_hblk_sized_graphics_bank() -> IndexedBlockGraphics {
        let palette = Palette::decode_vga_6bit(&vec![0u8; 768]).unwrap();
        let mut decoded = vec![0u8; 64 * 48];
        decoded.extend(std::iter::repeat_n(1u8, 64 * 48));
        IndexedBlockGraphics::from_fixed_records(
            "synthetic/HBLK01.DAT".to_string(),
            "synthetic/HPAL02.DAT".to_string(),
            64,
            48,
            0,
            &decoded,
            &palette,
        )
        .unwrap()
    }

    fn synthetic_object(
        kind: OriginalMissionObjectKind,
        record_index: u16,
    ) -> OriginalMissionObjectCandidate {
        OriginalMissionObjectCandidate {
            kind,
            record_index,
            desc: Some(0x04),
            state: Some(0),
            type_value: Some(0),
            subtype_value: Some(0),
            orientation: Some(0),
            tile: Some(OriginalTilePoint {
                tile_x: 1,
                tile_y: 1,
                tile_z: 0,
                off_x: 128,
                off_y: 128,
                off_z: 0,
            }),
            queue_tile: None,
            animation: OriginalAnimationRefs::default(),
            candidate_record: true,
            candidate_draw: true,
            draw_stage: Some(match kind {
                OriginalMissionObjectKind::Ped => OriginalDrawStage::People,
                OriginalMissionObjectKind::Vehicle => OriginalDrawStage::Vehicles,
                OriginalMissionObjectKind::Static => OriginalDrawStage::Statics,
                OriginalMissionObjectKind::Weapon => OriginalDrawStage::Weapons,
                OriginalMissionObjectKind::Sfx => OriginalDrawStage::Sfx,
            }),
        }
    }

    fn synthetic_map_tiles(width: u32, depth: u32, height: u32) -> OriginalMapTiles {
        let column_count = (width * depth) as usize;
        let height = height as usize;
        let mut data = Vec::new();
        data.extend_from_slice(&width.to_le_bytes());
        data.extend_from_slice(&depth.to_le_bytes());
        data.extend_from_slice(&(height as u32).to_le_bytes());
        let offset_table_bytes = column_count * 4;
        let mut stack_payload = Vec::new();
        for _ in 0..column_count {
            let offset_from_byte_12 = (offset_table_bytes + stack_payload.len()) as u32;
            data.extend_from_slice(&offset_from_byte_12.to_le_bytes());
            stack_payload.extend(std::iter::repeat_n(1u8, height));
        }
        data.extend_from_slice(&stack_payload);

        OriginalMapTiles::from_decoded_bytes("synthetic/MAP01.DAT".to_string(), &data).unwrap()
    }
}
