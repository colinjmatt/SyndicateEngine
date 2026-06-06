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
    },
    game::{
        agent::Agent,
        combat::{AttackResult, Combatant, resolve_attack},
        map::TacticalMap,
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
}

const QUICK_SAVE_PATH: &str = "../saves/quicksave.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MapRenderMode {
    DemoCity,
    DecodedSignature,
    InferredLayer,
    CandidateField(MapCandidateField),
    BlockAddressability,
}

impl MapRenderMode {
    fn label(self) -> String {
        match self {
            Self::DemoCity => "demo city".to_string(),
            Self::DecodedSignature => "MAP signatures".to_string(),
            Self::InferredLayer => "MAP inferred layer".to_string(),
            Self::CandidateField(field) => format!("MAP {}", field.provisional_label()),
            Self::BlockAddressability => "MAP block addressability".to_string(),
        }
    }

    fn diagnostic_layer(self) -> Option<MapDiagnosticSceneLayer> {
        match self {
            Self::DemoCity => None,
            Self::DecodedSignature => Some(MapDiagnosticSceneLayer::Signature),
            Self::InferredLayer => Some(MapDiagnosticSceneLayer::Inferred),
            Self::CandidateField(field) => Some(MapDiagnosticSceneLayer::CandidateField(field)),
            Self::BlockAddressability => Some(MapDiagnosticSceneLayer::BlockAddressability),
        }
    }
}

impl WorldState {
    pub fn new(assets: AssetIndex) -> Self {
        Self {
            assets,
            camera: CameraRig::default(),
            map: TacticalMap::demo_city(),
            agents: Agent::squad(),
            hostiles: vec![
                Combatant::guard("EUROCORP-1", GridPos::new(15, 14)),
                Combatant::guard("EUROCORP-2", GridPos::new(23, 10)),
                Combatant::guard("POLICE", GridPos::new(8, 16)),
            ],
            selected: 0,
            combat_log: "No contact".to_string(),
            sim_clock: SimClock::default(),
            render_mode: MapRenderMode::DemoCity,
            selected_map_scene: 0,
        }
    }

    pub fn update(&mut self, real_dt: f32) {
        if is_key_pressed(KeyCode::Escape) {
            std::process::exit(0);
        }
        self.camera.update(real_dt);
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
            MapRenderMode::CandidateField(_) => MapRenderMode::DemoCity,
            MapRenderMode::BlockAddressability => MapRenderMode::DemoCity,
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
            let map_label = self.current_map_panel_label();
            draw_map_diagnostic_panel(
                self.current_diagnostic_scene(),
                self.current_block_correlation(),
                self.render_mode,
                &map_label,
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
    mode: MapRenderMode,
    map_label: &str,
) {
    let x = screen_width() - 392.0;
    let y = 22.0;
    let panel_height = if mode == MapRenderMode::BlockAddressability {
        212.0
    } else {
        156.0
    };
    draw_rectangle(x, y, 370.0, panel_height, Color::new(0.0, 0.0, 0.0, 0.60));
    draw_rectangle_lines(x, y, 370.0, panel_height, 2.0, SKYBLUE);
    draw_text("DECODED MAP DIAGNOSTIC", x + 16.0, y + 26.0, 18.0, SKYBLUE);
    draw_text(map_label, x + 16.0, y + 50.0, 14.0, WHITE);

    if let Some(scene) = scene {
        let layer = mode
            .diagnostic_layer()
            .map(|layer| layer.label())
            .unwrap_or("demo city");
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

fn block_plausibility_panel_label(plausibility: BlockIndexPlausibility) -> &'static str {
    match plausibility {
        BlockIndexPlausibility::FitsRecordCount => "record-count fit",
        BlockIndexPlausibility::FitsByteRangeOnly => "byte-range only",
        BlockIndexPlausibility::OutOfRange => "out of range",
        BlockIndexPlausibility::Unknown => "unknown",
    }
}
