use crate::{
    engine::{
        assets::AssetIndex,
        block_decode::BlockIndexPlausibility,
        camera::CameraRig,
        iso::iso_to_grid,
        map_block_correlation::MapBlockCorrelationScene,
        map_catalog::MapDiagnosticSceneEntry,
        map_decode::MapCandidateField,
        map_scene::{MapDiagnosticScene, MapDiagnosticSceneLayer},
        map_tiles::{OriginalMapTiles, OriginalTileTypes},
        mission_scene::{
            OriginalMissionScene, OriginalObjectRenderDecision, OriginalStaticRenderDecision,
        },
        mission_source::OriginalMissionSelection,
    },
    game::{
        agent::Agent,
        combat::{AttackResult, Combatant, resolve_attack},
        map::TacticalMap,
        original_graphics::RuntimeOriginalGraphics,
        original_map_view::OriginalMapViewState,
        original_sprites::RuntimeOriginalObjectGraphics,
        pathfinding::{GridPos, find_path},
        save::{AgentSave, HostileSave, SaveGame, read_save, write_save},
        sim::SimClock,
        ui,
    },
};
use macroquad::prelude::*;

pub struct WorldState {
    assets: AssetIndex,
    camera: CameraRig,
    map: TacticalMap,
    agents: Vec<Agent>,
    hostiles: Vec<Combatant>,
    selected: usize,
    combat_log: String,
    sim_clock: SimClock,
    render_mode: MapRenderMode,
    selected_map_scene: usize,
    original_mission: Option<OriginalMissionSelection>,
    original_mission_scene: Option<OriginalMissionScene>,
    original_graphics: Option<RuntimeOriginalGraphics>,
    original_object_graphics: Option<RuntimeOriginalObjectGraphics>,
    original_object_animation_time: f32,
    original_map_tiles: Option<OriginalMapTiles>,
    original_tile_types: Option<OriginalTileTypes>,
    original_map_view: Option<OriginalMapViewState>,
}

const QUICK_SAVE_PATH: &str = "../saves/quicksave.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MapRenderMode {
    DemoCity,
    DecodedSignature,
    InferredLayer,
    CandidateField(MapCandidateField),
    BlockAddressability,
    OriginalMapTiles,
    OriginalMissionSceneProbe,
    OriginalGraphicsMap,
    OriginalGraphicsAtlas,
}

impl MapRenderMode {
    fn label(self) -> String {
        match self {
            Self::DemoCity => "demo city".to_string(),
            Self::DecodedSignature => "MAP signatures".to_string(),
            Self::InferredLayer => "MAP inferred layer".to_string(),
            Self::CandidateField(field) => format!("MAP {}", field.provisional_label()),
            Self::BlockAddressability => "MAP block addressability".to_string(),
            Self::OriginalMapTiles => "original mission map tiles".to_string(),
            Self::OriginalMissionSceneProbe => "original mission scene probe".to_string(),
            Self::OriginalGraphicsMap => "MAP original graphics candidate".to_string(),
            Self::OriginalGraphicsAtlas => "original graphics atlas".to_string(),
        }
    }

    fn diagnostic_layer(self) -> Option<MapDiagnosticSceneLayer> {
        match self {
            Self::DemoCity => None,
            Self::DecodedSignature => Some(MapDiagnosticSceneLayer::Signature),
            Self::InferredLayer => Some(MapDiagnosticSceneLayer::Inferred),
            Self::CandidateField(field) => Some(MapDiagnosticSceneLayer::CandidateField(field)),
            Self::BlockAddressability => Some(MapDiagnosticSceneLayer::BlockAddressability),
            Self::OriginalMapTiles
            | Self::OriginalMissionSceneProbe
            | Self::OriginalGraphicsMap
            | Self::OriginalGraphicsAtlas => None,
        }
    }
}

impl WorldState {
    pub fn new(assets: AssetIndex) -> Self {
        let original_mission = OriginalMissionSelection::from_root(assets.root_path()).ok();
        let original_mission_scene = original_mission.as_ref().and_then(|selection| {
            OriginalMissionScene::from_root(assets.root_path(), selection).ok()
        });
        let selected_map_id = original_mission
            .as_ref()
            .map(|selection| selection.map_id)
            .unwrap_or(1);
        let selected_palette_id = original_mission
            .as_ref()
            .map(|selection| selection.palette_id);
        let original_graphics = RuntimeOriginalGraphics::from_root_with_palette_id(
            assets.root_path(),
            selected_palette_id,
        );
        let original_object_graphics = RuntimeOriginalObjectGraphics::from_root_with_palette_id(
            assets.root_path(),
            selected_palette_id,
        );
        let original_map_tiles =
            OriginalMapTiles::from_root_for_map_id(assets.root_path(), selected_map_id).ok();
        let original_tile_types = OriginalTileTypes::from_root(assets.root_path()).ok();
        let original_map_view =
            if let (Some(map_tiles), Some(graphics)) = (&original_map_tiles, &original_graphics) {
                Some(OriginalMapViewState::from_runtime_assets(
                    map_tiles,
                    graphics,
                    original_mission.as_ref(),
                ))
            } else {
                None
            };
        let graphics_loaded = original_graphics.is_some();
        let original_map_loaded = graphics_loaded && original_map_tiles.is_some();
        let render_mode = if original_map_loaded {
            MapRenderMode::OriginalMapTiles
        } else if graphics_loaded {
            MapRenderMode::OriginalGraphicsAtlas
        } else {
            MapRenderMode::DemoCity
        };
        let camera = if original_map_loaded {
            original_map_view
                .as_ref()
                .zip(original_map_tiles.as_ref())
                .map(|(view, map_tiles)| {
                    view.starting_camera(map_tiles, original_tile_types.as_ref())
                })
                .unwrap_or_default()
        } else {
            CameraRig::default()
        };
        let combat_log = if original_map_loaded {
            original_mission_scene
                .as_ref()
                .map(OriginalMissionScene::runtime_status_label)
                .or_else(|| {
                    original_mission
                        .as_ref()
                        .map(OriginalMissionSelection::status_label)
                })
                .unwrap_or_else(|| "Runtime original mission map tile stacks loaded".to_string())
        } else if graphics_loaded {
            "Runtime original graphics loaded".to_string()
        } else {
            "No contact".to_string()
        };
        Self {
            assets,
            camera,
            map: TacticalMap::demo_city(),
            agents: Agent::squad(),
            hostiles: vec![
                Combatant::guard("EUROCORP-1", GridPos::new(15, 14)),
                Combatant::guard("EUROCORP-2", GridPos::new(23, 10)),
                Combatant::guard("POLICE", GridPos::new(8, 16)),
            ],
            selected: 0,
            combat_log,
            sim_clock: SimClock::default(),
            render_mode,
            selected_map_scene: 0,
            original_mission,
            original_mission_scene,
            original_graphics,
            original_object_graphics,
            original_object_animation_time: 0.0,
            original_map_tiles,
            original_tile_types,
            original_map_view,
        }
    }

    pub fn update(&mut self, real_dt: f32) {
        if is_key_pressed(KeyCode::Escape) {
            std::process::exit(0);
        }
        self.camera.update(real_dt);
        self.original_object_animation_time =
            (self.original_object_animation_time + real_dt.max(0.0)).rem_euclid(10_000.0);
        self.clamp_original_map_camera();
        self.update_render_controls();
        self.update_sim_controls();
        let dt = self.sim_clock.advance_dt(real_dt);
        for (key, idx) in [
            (KeyCode::Key1, 0),
            (KeyCode::Key2, 1),
            (KeyCode::Key3, 2),
            (KeyCode::Key4, 3),
        ] {
            if is_key_pressed(key) && idx < self.agents.len() {
                self.select(idx);
            }
        }
        if is_mouse_button_pressed(MouseButton::Right) {
            let mouse = vec2(mouse_position().0, mouse_position().1);
            let grid = iso_to_grid(self.camera.screen_to_world(mouse));
            let goal = GridPos::new(
                grid.x.round().clamp(0.0, self.map.width as f32 - 1.0) as i32,
                grid.y.round().clamp(0.0, self.map.height as f32 - 1.0) as i32,
            );
            let start = self.agents[self.selected].grid_pos();
            if let Some(path) = find_path(&self.map, start, goal) {
                self.agents[self.selected].set_path(path);
            } else {
                self.agents[self.selected].reject_order(goal);
            }
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            self.try_attack_at_mouse();
        }
        if is_key_pressed(KeyCode::F5) {
            match self.quick_save() {
                Ok(()) => self.combat_log = "Quick-saved tactical state".to_string(),
                Err(err) => self.combat_log = format!("Quick-save failed: {err}"),
            }
        }
        if is_key_pressed(KeyCode::F9) {
            match self.quick_load() {
                Ok(()) => self.combat_log = "Quick-loaded tactical state".to_string(),
                Err(err) => self.combat_log = format!("Quick-load failed: {err}"),
            }
        }
        for agent in &mut self.agents {
            agent.update(dt);
        }
        for hostile in &mut self.hostiles {
            hostile.tick(dt);
        }
    }

    fn update_render_controls(&mut self) {
        if is_key_pressed(KeyCode::N) {
            self.select_next_map_scene();
        }
        if is_key_pressed(KeyCode::P) {
            self.select_previous_map_scene();
        }
        if is_key_pressed(KeyCode::M) {
            if self.current_diagnostic_scene().is_none()
                && self.assets.diagnostics().map_preview.is_none()
                && self.original_map_tiles.is_none()
                && self.original_graphics.is_none()
            {
                self.combat_log = "MAP signature preview unavailable".to_string();
                return;
            }

            self.render_mode = self.next_render_mode();
            self.combat_log = format!("View mode: {}", self.render_mode.label());
        }
    }

    fn next_render_mode(&self) -> MapRenderMode {
        let inferred_available = self.current_diagnostic_scene().is_some()
            || self.assets.diagnostics().map_inferred_preview.is_some();
        match self.render_mode {
            MapRenderMode::DemoCity => MapRenderMode::DecodedSignature,
            MapRenderMode::DecodedSignature if inferred_available => MapRenderMode::InferredLayer,
            MapRenderMode::DecodedSignature => MapRenderMode::DemoCity,
            MapRenderMode::InferredLayer if inferred_available => {
                MapRenderMode::CandidateField(MapCandidateField::SurfaceIndex)
            }
            MapRenderMode::InferredLayer => MapRenderMode::DemoCity,
            MapRenderMode::CandidateField(MapCandidateField::SurfaceIndex)
                if inferred_available =>
            {
                MapRenderMode::CandidateField(MapCandidateField::DetailIndex)
            }
            MapRenderMode::CandidateField(MapCandidateField::DetailIndex) if inferred_available => {
                MapRenderMode::CandidateField(MapCandidateField::Reference)
            }
            MapRenderMode::CandidateField(MapCandidateField::Reference) if inferred_available => {
                MapRenderMode::CandidateField(MapCandidateField::Height)
            }
            MapRenderMode::CandidateField(MapCandidateField::Height)
                if self.current_block_correlation().is_some() =>
            {
                MapRenderMode::BlockAddressability
            }
            MapRenderMode::CandidateField(_) if self.original_map_tiles_ready() => {
                MapRenderMode::OriginalMapTiles
            }
            MapRenderMode::CandidateField(_) if self.original_graphics.is_some() => {
                MapRenderMode::OriginalGraphicsMap
            }
            MapRenderMode::CandidateField(_) => MapRenderMode::DemoCity,
            MapRenderMode::BlockAddressability if self.original_map_tiles_ready() => {
                MapRenderMode::OriginalMapTiles
            }
            MapRenderMode::BlockAddressability if self.original_graphics.is_some() => {
                MapRenderMode::OriginalGraphicsMap
            }
            MapRenderMode::BlockAddressability => MapRenderMode::DemoCity,
            MapRenderMode::OriginalMapTiles if self.original_mission_scene.is_some() => {
                MapRenderMode::OriginalMissionSceneProbe
            }
            MapRenderMode::OriginalMapTiles if inferred_available => {
                MapRenderMode::OriginalGraphicsMap
            }
            MapRenderMode::OriginalMapTiles => MapRenderMode::OriginalGraphicsAtlas,
            MapRenderMode::OriginalMissionSceneProbe => MapRenderMode::OriginalGraphicsMap,
            MapRenderMode::OriginalGraphicsMap => MapRenderMode::OriginalGraphicsAtlas,
            MapRenderMode::OriginalGraphicsAtlas => MapRenderMode::DemoCity,
        }
    }

    fn select_next_map_scene(&mut self) {
        let catalog = self.assets.map_scene_catalog();
        if catalog.len() < 2 {
            return;
        }
        self.selected_map_scene = catalog.next_index(self.selected_map_scene);
        self.ensure_render_mode_supported_by_selected_map();
        self.combat_log = format!("Decoded MAP: {}", self.current_map_panel_label());
    }

    fn select_previous_map_scene(&mut self) {
        let catalog = self.assets.map_scene_catalog();
        if catalog.len() < 2 {
            return;
        }
        self.selected_map_scene = catalog.previous_index(self.selected_map_scene);
        self.ensure_render_mode_supported_by_selected_map();
        self.combat_log = format!("Decoded MAP: {}", self.current_map_panel_label());
    }

    fn ensure_render_mode_supported_by_selected_map(&mut self) {
        if self.render_mode == MapRenderMode::BlockAddressability
            && self.current_block_correlation().is_none()
        {
            self.render_mode = MapRenderMode::InferredLayer;
        }
        if matches!(
            self.render_mode,
            MapRenderMode::OriginalMapTiles
                | MapRenderMode::OriginalMissionSceneProbe
                | MapRenderMode::OriginalGraphicsMap
                | MapRenderMode::OriginalGraphicsAtlas
        ) && self.original_graphics.is_none()
        {
            self.render_mode = MapRenderMode::InferredLayer;
        }
        if self.render_mode == MapRenderMode::OriginalMapTiles && self.original_map_tiles.is_none()
        {
            self.render_mode = MapRenderMode::OriginalGraphicsAtlas;
        }
        if self.render_mode == MapRenderMode::OriginalMissionSceneProbe
            && (self.original_map_tiles.is_none() || self.original_mission_scene.is_none())
        {
            self.render_mode = MapRenderMode::OriginalGraphicsMap;
        }
    }

    fn original_map_tiles_ready(&self) -> bool {
        self.original_graphics.is_some() && self.original_map_tiles.is_some()
    }

    fn original_object_animation_frame(&self) -> u16 {
        (self.original_object_animation_time * 6.0) as u16
    }

    fn original_scene_object_render_ready(scene_model: &OriginalMissionScene) -> bool {
        scene_model.static_render_proof.decision == OriginalStaticRenderDecision::RuntimeRenderReady
            || scene_model.ped_render_proof.decision
                == OriginalObjectRenderDecision::RuntimeRenderReady
            || scene_model.vehicle_render_proof.decision
                == OriginalObjectRenderDecision::RuntimeRenderReady
            || scene_model.weapon_render_proof.decision
                == OriginalObjectRenderDecision::RuntimeRenderReady
    }

    fn clamp_original_map_camera(&mut self) {
        if matches!(
            self.render_mode,
            MapRenderMode::OriginalMapTiles | MapRenderMode::OriginalMissionSceneProbe
        ) {
            if let Some(view) = self.original_map_view {
                view.clamp_camera(&mut self.camera);
            }
        }
    }

    fn current_map_entry_with_index(&self) -> Option<(usize, &MapDiagnosticSceneEntry)> {
        let catalog = self.assets.map_scene_catalog();
        if catalog.is_empty() {
            return None;
        }
        let index = self.selected_map_scene.min(catalog.len() - 1);
        catalog.entry(index).map(|entry| (index, entry))
    }

    fn current_diagnostic_scene(&self) -> Option<&MapDiagnosticScene> {
        self.current_map_entry_with_index()
            .map(|(_, entry)| &entry.scene)
            .or_else(|| self.assets.map_scene())
    }

    fn current_block_correlation(&self) -> Option<&MapBlockCorrelationScene> {
        self.current_map_entry_with_index()
            .and_then(|(_, entry)| entry.block_correlation.as_ref())
            .or_else(|| self.assets.map_block_correlation())
    }

    fn original_graphics_field(&self) -> MapCandidateField {
        self.current_block_correlation()
            .and_then(|correlation| correlation.selected_field())
            .unwrap_or(MapCandidateField::SurfaceIndex)
    }

    fn current_map_panel_label(&self) -> String {
        self.current_map_entry_with_index()
            .map(|(index, entry)| entry.panel_label(index, self.assets.map_scene_catalog().len()))
            .unwrap_or_else(|| "MAP01.DAT".to_string())
    }

    fn update_sim_controls(&mut self) {
        if is_key_pressed(KeyCode::Space) {
            self.sim_clock.toggle_pause();
        }
        if is_key_pressed(KeyCode::Period) {
            self.sim_clock.step_once();
        }
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
            self.sim_clock.faster();
        }
        if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
            self.sim_clock.slower();
        }
    }

    fn try_attack_at_mouse(&mut self) {
        let mouse = vec2(mouse_position().0, mouse_position().1);
        let grid = iso_to_grid(self.camera.screen_to_world(mouse));
        let clicked = GridPos::new(grid.x.round() as i32, grid.y.round() as i32);
        let Some(target_idx) = self
            .hostiles
            .iter()
            .position(|hostile| hostile.is_alive() && hostile.pos.manhattan(clicked) <= 1)
        else {
            self.combat_log = "No hostile at cursor".to_string();
            return;
        };

        let attacker_pos = self.agents[self.selected].grid_pos();
        let weapon = self.agents[self.selected].weapon;
        if !self.agents[self.selected].can_fire() {
            self.combat_log = format!("{} weapon cooling", self.agents[self.selected].name);
            return;
        }

        let target_name = self.hostiles[target_idx].name;
        let result = resolve_attack(attacker_pos, weapon, &mut self.hostiles[target_idx]);
        match result {
            AttackResult::Hit { remaining_hp } => {
                self.agents[self.selected].mark_fired_at(target_name);
                self.combat_log = format!("Hit {target_name}: {remaining_hp} HP remaining");
            }
            AttackResult::Eliminated => {
                self.agents[self.selected].mark_fired_at(target_name);
                self.combat_log = format!("{target_name} eliminated");
            }
            AttackResult::OutOfRange => {
                self.combat_log = format!("{target_name} out of range");
            }
            AttackResult::TargetAlreadyDown => {
                self.combat_log = format!("{target_name} already down");
            }
        }
    }

    fn select(&mut self, idx: usize) {
        self.selected = idx;
        for (i, agent) in self.agents.iter_mut().enumerate() {
            agent.selected = i == idx;
        }
    }

    fn quick_save(&self) -> anyhow::Result<()> {
        write_save(QUICK_SAVE_PATH, &self.to_save_game())
    }

    fn quick_load(&mut self) -> anyhow::Result<()> {
        let save = read_save(QUICK_SAVE_PATH)?;
        self.apply_save_game(save);
        Ok(())
    }

    fn to_save_game(&self) -> SaveGame {
        SaveGame {
            version: 1,
            selected_agent: self.selected,
            agents: self
                .agents
                .iter()
                .map(|agent| AgentSave {
                    name: agent.name.to_string(),
                    grid_x: agent.grid.x,
                    grid_y: agent.grid.y,
                    target_x: agent.target.x,
                    target_y: agent.target.y,
                    path: agent.path.clone(),
                })
                .collect(),
            hostiles: self
                .hostiles
                .iter()
                .map(|hostile| HostileSave {
                    name: hostile.name.to_string(),
                    pos: hostile.pos,
                    hp: hostile.hp,
                    cooldown: hostile.cooldown,
                })
                .collect(),
            combat_log: self.combat_log.clone(),
        }
    }

    fn apply_save_game(&mut self, save: SaveGame) {
        self.selected = save.selected_agent.min(self.agents.len().saturating_sub(1));
        for (agent, saved) in self.agents.iter_mut().zip(save.agents) {
            agent.grid = vec2(saved.grid_x, saved.grid_y);
            agent.target = vec2(saved.target_x, saved.target_y);
            agent.path = saved.path;
        }

        self.hostiles = save
            .hostiles
            .into_iter()
            .map(|saved| {
                let mut hostile = Combatant::guard(hostile_name(saved.name.as_str()), saved.pos);
                hostile.hp = saved.hp;
                hostile.cooldown = saved.cooldown;
                hostile
            })
            .collect();
        self.combat_log = save.combat_log;
        self.select(self.selected);
    }

    pub fn draw(&self) {
        match self.render_mode {
            MapRenderMode::DemoCity => self.map.draw(&self.camera),
            MapRenderMode::DecodedSignature => {
                if let Some(scene) = self.current_diagnostic_scene() {
                    self.map.draw_diagnostic_scene(
                        &self.camera,
                        scene,
                        MapDiagnosticSceneLayer::Signature,
                    );
                } else if let Some(preview) = self.assets.diagnostics().map_preview.as_ref() {
                    self.map.draw_signature_preview(&self.camera, preview);
                } else {
                    self.map.draw(&self.camera);
                }
            }
            MapRenderMode::InferredLayer => {
                if let Some(scene) = self.current_diagnostic_scene() {
                    self.map.draw_diagnostic_scene(
                        &self.camera,
                        scene,
                        MapDiagnosticSceneLayer::Inferred,
                    );
                } else if let Some(preview) =
                    self.assets.diagnostics().map_inferred_preview.as_ref()
                {
                    self.map.draw_inferred_layer_preview(&self.camera, preview);
                } else {
                    self.map.draw(&self.camera);
                }
            }
            MapRenderMode::CandidateField(field) => {
                if let Some(scene) = self.current_diagnostic_scene() {
                    self.map.draw_diagnostic_scene(
                        &self.camera,
                        scene,
                        MapDiagnosticSceneLayer::CandidateField(field),
                    );
                } else if let Some(substrate) =
                    self.assets.diagnostics().map_substrate_candidate.as_ref()
                {
                    self.map
                        .draw_candidate_field_preview(&self.camera, substrate, field);
                } else {
                    self.map.draw(&self.camera);
                }
            }
            MapRenderMode::BlockAddressability => {
                if let (Some(scene), Some(correlation)) = (
                    self.current_diagnostic_scene(),
                    self.current_block_correlation(),
                ) {
                    self.map
                        .draw_block_addressability_scene(&self.camera, scene, correlation);
                } else if let Some(scene) = self.current_diagnostic_scene() {
                    self.map.draw_diagnostic_scene(
                        &self.camera,
                        scene,
                        MapDiagnosticSceneLayer::Inferred,
                    );
                } else {
                    self.map.draw(&self.camera);
                }
            }
            MapRenderMode::OriginalGraphicsMap => {
                if let (Some(scene), Some(graphics)) = (
                    self.current_diagnostic_scene(),
                    self.original_graphics.as_ref(),
                ) {
                    self.map.draw_original_graphics_scene(
                        &self.camera,
                        scene,
                        self.original_graphics_field(),
                        graphics,
                    );
                } else {
                    self.map.draw(&self.camera);
                }
            }
            MapRenderMode::OriginalMapTiles => {
                if let (Some(map_tiles), Some(graphics)) = (
                    self.original_map_tiles.as_ref(),
                    self.original_graphics.as_ref(),
                ) {
                    self.map.draw_original_map_tiles(
                        &self.camera,
                        map_tiles,
                        self.original_tile_types.as_ref(),
                        graphics,
                    );
                }
            }
            MapRenderMode::OriginalMissionSceneProbe => {
                if let (Some(map_tiles), Some(graphics), Some(scene_model)) = (
                    self.original_map_tiles.as_ref(),
                    self.original_graphics.as_ref(),
                    self.original_mission_scene.as_ref(),
                ) {
                    let object_graphics = if Self::original_scene_object_render_ready(scene_model) {
                        self.original_object_graphics.as_ref()
                    } else {
                        None
                    };
                    self.map.draw_original_mission_scene(
                        &self.camera,
                        map_tiles,
                        self.original_tile_types.as_ref(),
                        graphics,
                        scene_model,
                        object_graphics,
                        self.original_object_animation_frame(),
                    );
                }
            }
            MapRenderMode::OriginalGraphicsAtlas => {
                if let Some(graphics) = self.original_graphics.as_ref() {
                    draw_original_graphics_atlas(graphics);
                }
            }
        }

        if self.render_mode == MapRenderMode::DemoCity {
            for agent in &self.agents {
                if agent.selected {
                    self.map.draw_path(&self.camera, &agent.path, agent.color);
                    if let Some(destination) = agent.destination() {
                        self.map.draw_marker(&self.camera, destination, agent.color);
                    }
                }
            }
            for agent in &self.agents {
                agent.draw(&self.camera);
            }
            for hostile in &self.hostiles {
                draw_hostile(&self.camera, hostile);
            }
            draw_minimap(&self.agents);
        } else {
            let map_label = if matches!(
                self.render_mode,
                MapRenderMode::OriginalMapTiles | MapRenderMode::OriginalMissionSceneProbe
            ) {
                self.original_mission
                    .as_ref()
                    .map(OriginalMissionSelection::panel_label)
                    .unwrap_or_else(|| self.current_map_panel_label())
            } else {
                self.current_map_panel_label()
            };
            draw_map_diagnostic_panel(
                self.current_diagnostic_scene(),
                self.current_block_correlation(),
                self.original_mission.as_ref(),
                self.original_mission_scene.as_ref(),
                self.original_map_view.as_ref(),
                self.original_graphics.as_ref(),
                self.original_object_graphics.as_ref(),
                self.original_map_tiles.as_ref(),
                self.original_tile_types.as_ref(),
                self.original_graphics_field(),
                self.render_mode,
                &map_label,
                &self.camera,
            );
        }

        ui::draw_hud(
            &self.assets,
            self.agents[self.selected].name,
            &self.agents[self.selected].order_summary(),
            &self.combat_log,
            &format!(
                "{} | view {}",
                self.sim_clock.label(),
                self.render_mode.label()
            ),
        );
    }
}

fn compact_asset_label(label: &str) -> &str {
    label.rsplit('/').next().unwrap_or(label)
}

fn hostile_name(name: &str) -> &'static str {
    match name {
        "EUROCORP-1" => "EUROCORP-1",
        "EUROCORP-2" => "EUROCORP-2",
        "POLICE" => "POLICE",
        _ => "HOSTILE",
    }
}

fn draw_hostile(camera: &CameraRig, hostile: &Combatant) {
    if !hostile.is_alive() {
        return;
    }
    let base = camera.world_to_screen(crate::engine::iso::grid_to_iso(
        hostile.pos.x as f32,
        hostile.pos.y as f32,
        0.0,
    ));
    let p = vec2(base.x, base.y - 18.0 * camera.zoom);
    draw_circle(p.x, p.y, 8.0 * camera.zoom, RED);
    draw_circle_lines(p.x, p.y, 11.0 * camera.zoom, 2.0, ORANGE);
    draw_text(
        hostile.name,
        p.x - 24.0,
        p.y - 16.0,
        13.0 * camera.zoom,
        ORANGE,
    );
    let hp_width = 34.0 * (hostile.hp as f32 / hostile.max_hp as f32);
    draw_rectangle(p.x - 17.0, p.y + 14.0, 34.0, 4.0, DARKGRAY);
    draw_rectangle(p.x - 17.0, p.y + 14.0, hp_width, 4.0, RED);
}

fn draw_minimap(agents: &[Agent]) {
    let x = screen_width() - 188.0;
    let y = 22.0;
    draw_rectangle(x, y, 166.0, 166.0, Color::new(0.0, 0.0, 0.0, 0.56));
    draw_rectangle_lines(x, y, 166.0, 166.0, 2.0, GREEN);
    draw_text("CITY GRID", x + 18.0, y + 24.0, 18.0, GREEN);
    for agent in agents {
        let px = x + 18.0 + agent.grid.x / 28.0 * 130.0;
        let py = y + 38.0 + agent.grid.y / 28.0 * 124.0;
        draw_circle(px, py, if agent.selected { 5.0 } else { 3.5 }, agent.color);
    }
}

fn draw_map_diagnostic_panel(
    scene: Option<&MapDiagnosticScene>,
    correlation: Option<&MapBlockCorrelationScene>,
    mission_selection: Option<&OriginalMissionSelection>,
    mission_scene: Option<&OriginalMissionScene>,
    original_map_view: Option<&OriginalMapViewState>,
    graphics: Option<&RuntimeOriginalGraphics>,
    object_graphics: Option<&RuntimeOriginalObjectGraphics>,
    map_tiles: Option<&OriginalMapTiles>,
    tile_types: Option<&OriginalTileTypes>,
    original_graphics_field: MapCandidateField,
    mode: MapRenderMode,
    map_label: &str,
    camera: &CameraRig,
) {
    let panel_width = map_panel_width(mode);
    let x = screen_width() - panel_width - 22.0;
    let y = 22.0;
    let panel_height = map_panel_height(mode);
    draw_rectangle(
        x,
        y,
        panel_width,
        panel_height,
        Color::new(0.0, 0.0, 0.0, 0.60),
    );
    draw_rectangle_lines(x, y, panel_width, panel_height, 2.0, SKYBLUE);
    draw_text("DECODED MAP DIAGNOSTIC", x + 16.0, y + 26.0, 18.0, SKYBLUE);
    draw_text(map_label, x + 16.0, y + 50.0, 14.0, WHITE);

    if let Some(scene) = scene {
        let layer = match mode {
            MapRenderMode::OriginalMapTiles => "runtime original MAP tile stacks",
            MapRenderMode::OriginalMissionSceneProbe => "runtime original mission scene model",
            MapRenderMode::OriginalGraphicsMap => "runtime original graphics candidate",
            MapRenderMode::OriginalGraphicsAtlas => "runtime original graphics atlas",
            _ => mode
                .diagnostic_layer()
                .map(|layer| layer.label())
                .unwrap_or("demo city"),
        };
        draw_text(
            &format!("{} | {layer}", mode.label()),
            x + 16.0,
            y + 74.0,
            14.0,
            LIGHTGRAY,
        );
        draw_text(
            &format!(
                "{}x{} cells | inferred {} | signatures {}",
                scene.width, scene.height, scene.visual_classes, scene.unique_signatures
            ),
            x + 16.0,
            y + 96.0,
            14.0,
            LIGHTGRAY,
        );
        if mode == MapRenderMode::BlockAddressability {
            if let Some(candidate) =
                correlation.and_then(|correlation| correlation.selected_candidate())
            {
                draw_text(
                    &format!(
                        "{} | {}",
                        candidate.field.provisional_label(),
                        block_plausibility_panel_label(candidate.plausibility)
                    ),
                    x + 16.0,
                    y + 120.0,
                    13.0,
                    LIGHTGRAY,
                );
                draw_text(
                    &format!(
                        "{}% addressable | {}/{} cells | {} out",
                        candidate.addressable_percent(),
                        candidate.addressable_cells,
                        candidate.total_cells,
                        candidate.out_of_range_cells
                    ),
                    x + 16.0,
                    y + 140.0,
                    13.0,
                    YELLOW,
                );
                draw_text(&candidate.container, x + 16.0, y + 160.0, 12.0, GRAY);
            } else {
                draw_text(
                    "Block addressability candidate unavailable",
                    x + 16.0,
                    y + 126.0,
                    13.0,
                    GRAY,
                );
            }
            draw_text(
                "Runtime-local aggregate; no decoded tile pixels",
                x + 16.0,
                y + 184.0,
                13.0,
                YELLOW,
            );
            draw_text(
                "Not proof of layout, walkability, objects, or semantics",
                x + 16.0,
                y + 202.0,
                12.0,
                GRAY,
            );
        } else if mode == MapRenderMode::OriginalMapTiles {
            if let (Some(map_tiles), Some(graphics)) = (map_tiles, graphics) {
                draw_text(
                    &format!(
                        "{}x{}x{} stacks | {} unique",
                        map_tiles.width, map_tiles.depth, map_tiles.height, map_tiles.unique_stacks
                    ),
                    x + 16.0,
                    y + 120.0,
                    13.0,
                    LIGHTGRAY,
                );
                draw_text(
                    &format!(
                        "max tile {} | HBLK {} {}x{} | palette {}",
                        map_tiles.max_tile_index,
                        graphics.bank().record_count,
                        graphics.bank().record_width,
                        graphics.bank().record_height,
                        compact_asset_label(&graphics.bank().palette_label)
                    ),
                    x + 16.0,
                    y + 140.0,
                    12.0,
                    YELLOW,
                );
                let source_label = tile_types
                    .map(|tile_types| {
                        format!("{} | {}", map_tiles.source_label, tile_types.source_label)
                    })
                    .unwrap_or_else(|| map_tiles.source_label.clone());
                draw_text(&source_label, x + 16.0, y + 160.0, 12.0, GRAY);
                if let Some(selection) = mission_selection {
                    draw_text(
                        &format!(
                            "mission {} map {} scroll {:?}->{:?}",
                            selection.mission_id,
                            selection.map_id,
                            selection.min_scroll_tile,
                            selection.max_scroll_tile
                        ),
                        x + 16.0,
                        y + 178.0,
                        12.0,
                        GRAY,
                    );
                    draw_text(
                        &selection.render_diagnostics.object_queue_panel_label(),
                        x + 16.0,
                        y + 196.0,
                        12.0,
                        GRAY,
                    );
                    draw_text(
                        selection
                            .render_diagnostics
                            .object_queue_order_panel_label(),
                        x + 16.0,
                        y + 214.0,
                        12.0,
                        GRAY,
                    );
                }
                if let Some(view) = original_map_view {
                    draw_text(
                        &view.scroll_summary_label(),
                        x + 16.0,
                        y + 232.0,
                        12.0,
                        GRAY,
                    );
                }
            }
            draw_text(
                "Runtime MAP tile placement; local pixels only",
                x + 16.0,
                y + 256.0,
                13.0,
                YELLOW,
            );
            draw_text(
                "No walkability, objects, mission, or entity semantics",
                x + 16.0,
                y + 274.0,
                12.0,
                GRAY,
            );
        } else if mode == MapRenderMode::OriginalMissionSceneProbe {
            if let Some(scene_model) = mission_scene {
                draw_text(
                    &scene_model.section_counts_panel_label(),
                    x + 16.0,
                    y + 120.0,
                    12.0,
                    LIGHTGRAY,
                );
                draw_text(
                    &scene_model.object_summary_label(),
                    x + 16.0,
                    y + 140.0,
                    12.0,
                    LIGHTGRAY,
                );
                draw_text(
                    &scene_model.draw_stage_panel_label(),
                    x + 16.0,
                    y + 160.0,
                    12.0,
                    LIGHTGRAY,
                );
                let visible_candidates =
                    visible_scene_candidate_total(scene_model, camera, map_tiles, graphics);
                draw_text(
                    &format!(
                        "viewport-visible candidates {}/{}",
                        visible_candidates,
                        scene_model.draw_queue.total_candidates()
                    ),
                    x + 16.0,
                    y + 180.0,
                    12.0,
                    YELLOW,
                );
                draw_text(
                    &scene_model.animation_support.panel_label(),
                    x + 16.0,
                    y + 200.0,
                    12.0,
                    GRAY,
                );
                draw_text(
                    &scene_model.sprite_support.panel_label(),
                    x + 16.0,
                    y + 220.0,
                    12.0,
                    GRAY,
                );
                draw_text(
                    &scene_model.static_render_proof.panel_label(),
                    x + 16.0,
                    y + 242.0,
                    11.0,
                    if scene_model.static_render_proof.decision
                        == OriginalStaticRenderDecision::RuntimeRenderReady
                    {
                        GREEN
                    } else {
                        ORANGE
                    },
                );
                draw_text(
                    &scene_model.ped_render_proof.panel_label(),
                    x + 16.0,
                    y + 264.0,
                    11.0,
                    if scene_model.ped_render_proof.decision
                        == OriginalObjectRenderDecision::RuntimeRenderReady
                    {
                        GREEN
                    } else {
                        GRAY
                    },
                );
                draw_text(
                    &scene_model.vehicle_render_proof.panel_label(),
                    x + 16.0,
                    y + 282.0,
                    11.0,
                    if scene_model.vehicle_render_proof.decision
                        == OriginalObjectRenderDecision::RuntimeRenderReady
                    {
                        GREEN
                    } else {
                        GRAY
                    },
                );
                draw_text(
                    &scene_model.weapon_render_proof.panel_label(),
                    x + 16.0,
                    y + 300.0,
                    11.0,
                    if scene_model.weapon_render_proof.decision
                        == OriginalObjectRenderDecision::RuntimeRenderReady
                    {
                        GREEN
                    } else {
                        GRAY
                    },
                );
                let static_runtime_label = if scene_model.static_render_proof.decision
                    == OriginalStaticRenderDecision::RuntimeRenderReady
                {
                    format!(
                        "map tiles rendered; statics rendered from local assets {}/{}",
                        scene_model
                            .static_render_proof
                            .runtime_renderable_static_count,
                        scene_model.static_render_proof.candidate_count
                    )
                } else {
                    "map tiles rendered; statics candidate-only/blocked".to_string()
                };
                draw_text(
                    &static_runtime_label,
                    x + 16.0,
                    y + 322.0,
                    11.0,
                    if object_graphics.is_some() {
                        YELLOW
                    } else {
                        GRAY
                    },
                );
                draw_text(
                    &scene_model.spawn_probe.panel_label(),
                    x + 16.0,
                    y + 344.0,
                    11.0,
                    GRAY,
                );
                draw_text(
                    &format!(
                        "nav links {}, occupied {}, blockers {}",
                        scene_model.navigation_probe.map_object_link_cells,
                        scene_model.navigation_probe.candidate_occupied_tiles,
                        scene_model.navigation_probe.static_blocking_candidates
                    ),
                    x + 16.0,
                    y + 364.0,
                    11.0,
                    GRAY,
                );
                draw_text(
                    &format!(
                        "doors {}, windows {}, vehicles {}; demo grid active",
                        scene_model.navigation_probe.door_candidates,
                        scene_model.navigation_probe.window_candidates,
                        scene_model.navigation_probe.vehicle_footprint_candidates
                    ),
                    x + 16.0,
                    y + 382.0,
                    11.0,
                    GRAY,
                );
                draw_text(
                    "gameplay navigation/occupancy remains candidate-only",
                    x + 16.0,
                    y + 404.0,
                    11.0,
                    GRAY,
                );
            } else {
                draw_text(
                    "first-mission scene model unavailable",
                    x + 16.0,
                    y + 126.0,
                    13.0,
                    GRAY,
                );
            }
            draw_text(
                "Map is rendered; objects are candidate-only unless proof passes",
                x + 16.0,
                y + 432.0,
                12.0,
                YELLOW,
            );
            draw_text(
                "Gameplay/pathfinding remain on the demo tactical grid",
                x + 16.0,
                y + 450.0,
                12.0,
                GRAY,
            );
        } else if mode == MapRenderMode::OriginalGraphicsMap {
            if let Some(graphics) = graphics {
                draw_text(
                    &format!(
                        "{} via {}",
                        original_graphics_field.provisional_label(),
                        graphics.bank().source_label
                    ),
                    x + 16.0,
                    y + 120.0,
                    12.0,
                    LIGHTGRAY,
                );
                draw_text(
                    &format!(
                        "{} {}x{} records | palette {}",
                        graphics.bank().record_count,
                        graphics.bank().record_width,
                        graphics.bank().record_height,
                        graphics.bank().palette_label
                    ),
                    x + 16.0,
                    y + 140.0,
                    12.0,
                    YELLOW,
                );
            }
            draw_text(
                "Runtime original pixels; candidate indexing only",
                x + 16.0,
                y + 164.0,
                13.0,
                YELLOW,
            );
            draw_text(
                "Not proof of terrain, objects, layout, or walkability",
                x + 16.0,
                y + 184.0,
                12.0,
                GRAY,
            );
        } else if mode == MapRenderMode::OriginalGraphicsAtlas {
            if let Some(graphics) = graphics {
                draw_text(
                    &format!(
                        "Atlas: {} {}x{} records",
                        graphics.bank().record_count,
                        graphics.bank().record_width,
                        graphics.bank().record_height
                    ),
                    x + 16.0,
                    y + 120.0,
                    13.0,
                    LIGHTGRAY,
                );
                draw_text(
                    &graphics.bank().source_label,
                    x + 16.0,
                    y + 140.0,
                    12.0,
                    YELLOW,
                );
            }
            draw_text(
                "Runtime-local atlas; previews are not written",
                x + 16.0,
                y + 164.0,
                13.0,
                YELLOW,
            );
            draw_text(
                "No decoded tile semantics claimed",
                x + 16.0,
                y + 184.0,
                12.0,
                GRAY,
            );
        } else if let MapRenderMode::CandidateField(field) = mode {
            if let Some(evidence) = scene.field_evidence_panel_label(field) {
                draw_text(&evidence, x + 16.0, y + 120.0, 13.0, LIGHTGRAY);
            }
            draw_text(
                "Runtime-local render; gameplay grid remains demo city",
                x + 16.0,
                y + 144.0,
                13.0,
                YELLOW,
            );
            draw_text(
                "No decoded walkability/object semantics claimed",
                x + 16.0,
                y + 164.0,
                13.0,
                GRAY,
            );
        } else {
            draw_text(
                "Runtime-local render; gameplay grid remains demo city",
                x + 16.0,
                y + 120.0,
                13.0,
                YELLOW,
            );
            draw_text(
                "No decoded walkability/object semantics claimed",
                x + 16.0,
                y + 140.0,
                13.0,
                GRAY,
            );
        }
    } else {
        draw_text(
            "MAP diagnostic scene unavailable",
            x + 16.0,
            y + 82.0,
            14.0,
            GRAY,
        );
    }
}

fn map_panel_height(mode: MapRenderMode) -> f32 {
    match mode {
        MapRenderMode::OriginalMissionSceneProbe => 472.0,
        MapRenderMode::OriginalMapTiles => 292.0,
        MapRenderMode::BlockAddressability => 212.0,
        MapRenderMode::OriginalGraphicsMap | MapRenderMode::OriginalGraphicsAtlas => 204.0,
        MapRenderMode::CandidateField(_) => 180.0,
        _ => 156.0,
    }
}

fn map_panel_width(mode: MapRenderMode) -> f32 {
    match mode {
        MapRenderMode::OriginalMissionSceneProbe => 520.0,
        _ => 370.0,
    }
}

fn visible_scene_candidate_total(
    scene: &OriginalMissionScene,
    camera: &CameraRig,
    map_tiles: Option<&OriginalMapTiles>,
    graphics: Option<&RuntimeOriginalGraphics>,
) -> usize {
    let (Some(map_tiles), Some(graphics)) = (map_tiles, graphics) else {
        return 0;
    };
    let tile_width = graphics.bank().record_width as f32;
    let tile_height = graphics.bank().record_height as f32;
    let margin = tile_width.max(tile_height) * camera.zoom * 2.0;

    scene
        .draw_queue
        .entries()
        .iter()
        .filter(|entry| {
            let top_left = crate::game::map::original_map_tile_world_top_left(
                map_tiles,
                entry.tile.tile_x as f32,
                entry.tile.tile_y as f32,
                entry.tile.tile_z as f32,
                tile_width,
                tile_height,
            );
            let screen = camera.world_to_screen(top_left);
            screen.x >= -margin
                && screen.y >= -margin
                && screen.x <= screen_width() + margin
                && screen.y <= screen_height() + margin
        })
        .count()
}

fn draw_original_graphics_atlas(graphics: &RuntimeOriginalGraphics) {
    let columns = if graphics.bank().record_count <= 256 {
        16
    } else {
        18
    };
    let rows = graphics.bank().record_count.div_ceil(columns).min(16);
    let base_tile_width = if graphics.bank().record_width >= 64 {
        32.0
    } else {
        26.0
    };
    let tile_size = vec2(
        base_tile_width,
        base_tile_width * graphics.bank().record_height as f32
            / graphics.bank().record_width.max(1) as f32,
    );
    let panel_width = columns as f32 * tile_size.x + 28.0;
    let panel_height = rows as f32 * tile_size.y + 92.0;
    let origin = vec2((screen_width() - panel_width - 40.0).max(900.0), 286.0);
    draw_rectangle(
        origin.x - 14.0,
        origin.y - 54.0,
        panel_width,
        panel_height,
        Color::new(0.0, 0.0, 0.0, 0.68),
    );
    draw_rectangle_lines(
        origin.x - 14.0,
        origin.y - 54.0,
        panel_width,
        panel_height,
        2.0,
        SKYBLUE,
    );
    draw_text(
        "RUNTIME ORIGINAL GRAPHICS ATLAS",
        origin.x,
        origin.y - 30.0,
        18.0,
        SKYBLUE,
    );
    draw_text(
        &format!(
            "{} | palette {}",
            graphics.bank().source_label,
            graphics.bank().palette_label
        ),
        origin.x,
        origin.y - 10.0,
        13.0,
        LIGHTGRAY,
    );
    graphics.draw_atlas_preview(origin, columns, rows, tile_size);
    draw_text(
        "Local asset pixels only; not saved to report/source/tests",
        origin.x,
        origin.y + rows as f32 * tile_size.y + 24.0,
        13.0,
        YELLOW,
    );
}

fn block_plausibility_panel_label(plausibility: BlockIndexPlausibility) -> &'static str {
    match plausibility {
        BlockIndexPlausibility::FitsRecordCount => "record-count fit",
        BlockIndexPlausibility::FitsByteRangeOnly => "byte-range only",
        BlockIndexPlausibility::OutOfRange => "out of range",
        BlockIndexPlausibility::Unknown => "unknown",
    }
}
