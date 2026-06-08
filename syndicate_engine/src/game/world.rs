use std::collections::BTreeSet;

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
            OriginalDebugAgentSpawn, OriginalDebugInteractionFocus, OriginalDebugInteractionIntent,
            OriginalDebugInteractionIntentStatus, OriginalDebugInteractionProbe,
            OriginalMissionObjectCandidate, OriginalMissionObjectKind, OriginalMissionScene,
            OriginalObjectRenderDecision, OriginalRuntimeRouteProbe, OriginalRuntimeRouteStatus,
            OriginalStaticRenderDecision, OriginalTilePoint,
        },
        mission_source::OriginalMissionSelection,
    },
    game::{
        agent::Agent,
        combat::{AttackResult, Combatant, resolve_attack},
        map::{TacticalMap, original_map_tile_world_top_left},
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
    original_cursor_tile: Option<OriginalTilePoint>,
    original_cursor_screen: Option<Vec2>,
    original_route_probe: Option<OriginalRuntimeRouteProbe>,
    original_interaction_probe: Option<OriginalDebugInteractionProbe>,
    original_navigation_debug_enabled: bool,
    original_debug_agents: Vec<OriginalDebugAgent>,
    selected_original_debug_agent: usize,
    original_control_runtime: OriginalMissionControlRuntime,
    original_control_trace: OriginalControlTrace,
}

const QUICK_SAVE_PATH: &str = "../saves/quicksave.json";
const ORIGINAL_DEBUG_AGENT_MAX_STEP_DT: f32 = 0.05;

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

#[derive(Debug, Clone)]
struct OriginalDebugAgent {
    slot: u8,
    record_index: u16,
    tile: OriginalTilePoint,
    route: Vec<OriginalTilePoint>,
    route_progress: f32,
    selected: bool,
    sprite_ready: bool,
    route_status: OriginalDebugAgentRouteStatus,
    direction: OriginalDebugAgentDirection,
    interaction_intent: Option<OriginalDebugInteractionIntent>,
    action_state: Option<OriginalDebugActionState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OriginalDebugAgentRouteStatus {
    Idle,
    Queued,
    Moving,
    Arrived,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OriginalDebugAgentDirection {
    South,
    SouthEast,
    East,
    NorthEast,
    North,
    NorthWest,
    West,
    SouthWest,
}

#[derive(Debug, Clone, Default)]
struct OriginalMissionControlRuntime {
    door_resolutions: usize,
    weapon_pickup_resolutions: usize,
    vehicle_entry_resolutions: usize,
    objective_contact_resolutions: usize,
    scenario_trigger_resolutions: usize,
    blocked_action_resolutions: usize,
    combat_probe_count: usize,
    last_result: Option<String>,
}

#[derive(Debug, Clone)]
struct OriginalControlTrace {
    enabled: bool,
    autopilot: bool,
    quit_after_frames: Option<u32>,
    frame: u32,
    elapsed: f32,
    next_emit_elapsed: f32,
    last_signature: String,
    smoke_queued: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct OriginalDebugActionState {
    status: OriginalDebugActionStatus,
    focus: OriginalDebugInteractionFocus,
    target_tile: Option<OriginalTilePoint>,
    route_nodes: usize,
    candidate_total: usize,
    elapsed: f32,
    emitted_resolution: bool,
    result_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OriginalDebugActionStatus {
    RouteQueued,
    Ready,
    Resolving,
    Resolved,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginalDebugActionResolution {
    agent_slot: u8,
    focus: OriginalDebugInteractionFocus,
    target_tile: Option<OriginalTilePoint>,
    result_label: String,
}

impl OriginalMissionControlRuntime {
    fn apply_resolution(&mut self, resolution: OriginalDebugActionResolution) {
        match resolution.focus {
            OriginalDebugInteractionFocus::DoorOpenCandidate
            | OriginalDebugInteractionFocus::LargeDoorCandidate => {
                self.door_resolutions += 1;
            }
            OriginalDebugInteractionFocus::WeaponPickupCandidate => {
                self.weapon_pickup_resolutions += 1;
            }
            OriginalDebugInteractionFocus::VehicleEntryCandidate => {
                self.vehicle_entry_resolutions += 1;
            }
            OriginalDebugInteractionFocus::ObjectiveTargetCandidate => {
                self.objective_contact_resolutions += 1;
            }
            OriginalDebugInteractionFocus::ScenarioTriggerCandidate => {
                self.scenario_trigger_resolutions += 1;
            }
            OriginalDebugInteractionFocus::None => {
                self.blocked_action_resolutions += 1;
            }
        }
        self.last_result = Some(format!(
            "agent {} {}; target {}",
            resolution.agent_slot + 1,
            resolution.result_label,
            resolution
                .target_tile
                .map(original_tile_short_label)
                .unwrap_or_else(|| "none".to_string())
        ));
    }

    fn record_combat_probe(&mut self, record_index: u16, distance: u16, in_range: bool) {
        self.combat_probe_count += 1;
        let range_label = if in_range { "in range" } else { "out of range" };
        self.last_result = Some(format!(
            "combat probe ped candidate {record_index} {range_label} at range {distance}; no hit state"
        ));
    }

    fn panel_label(&self) -> String {
        let last = self
            .last_result
            .as_deref()
            .unwrap_or("no local action result yet");
        format!(
            "control runtime local results door {} pickup {} vehicle {} objective {} scenario {} combat probes {}; {last}",
            self.door_resolutions,
            self.weapon_pickup_resolutions,
            self.vehicle_entry_resolutions,
            self.objective_contact_resolutions,
            self.scenario_trigger_resolutions,
            self.combat_probe_count
        )
    }
}

impl OriginalControlTrace {
    fn from_env() -> Self {
        let autopilot = env_flag("SYNDICATE_ORIGINAL_CONTROL_SMOKE");
        Self {
            enabled: autopilot || env_flag("SYNDICATE_ORIGINAL_CONTROL_TRACE"),
            autopilot,
            quit_after_frames: std::env::var("SYNDICATE_ORIGINAL_CONTROL_QUIT_FRAMES")
                .ok()
                .and_then(|value| value.parse().ok())
                .or_else(|| autopilot.then_some(240)),
            frame: 0,
            elapsed: 0.0,
            next_emit_elapsed: 0.0,
            last_signature: String::new(),
            smoke_queued: false,
        }
    }

    fn begin_frame(&mut self, real_dt: f32) -> bool {
        self.frame = self.frame.saturating_add(1);
        self.elapsed += real_dt.max(0.0);
        if self.autopilot && !self.smoke_queued {
            self.smoke_queued = true;
            return true;
        }
        false
    }

    fn should_emit(&mut self, signature: &str, force: bool) -> bool {
        if force || self.elapsed >= self.next_emit_elapsed || self.last_signature != signature {
            self.last_signature = signature.to_string();
            self.next_emit_elapsed = self.elapsed + 0.5;
            return true;
        }
        false
    }

    fn trace_line(&self, signature: &str) -> String {
        format!(
            "[original-control] frame {} t {:.2} {signature}",
            self.frame, self.elapsed
        )
    }

    fn should_quit(&self) -> bool {
        self.quit_after_frames
            .is_some_and(|quit_after_frames| self.frame >= quit_after_frames)
    }
}

impl OriginalDebugActionState {
    fn from_intent(intent: &OriginalDebugInteractionIntent) -> Self {
        let (status, result_label) = match intent.status {
            OriginalDebugInteractionIntentStatus::RouteQueued => (
                OriginalDebugActionStatus::RouteQueued,
                format!("{} action queued behind route", intent.focus.label()),
            ),
            OriginalDebugInteractionIntentStatus::ReadyAtTarget => (
                OriginalDebugActionStatus::Ready,
                format!("{} action ready at target", intent.focus.label()),
            ),
            _ => (
                OriginalDebugActionStatus::Blocked,
                format!(
                    "{} action blocked: {}",
                    intent.focus.label(),
                    intent.message
                ),
            ),
        };
        Self {
            status,
            focus: intent.focus,
            target_tile: intent.target_tile,
            route_nodes: intent.route_nodes,
            candidate_total: intent.candidate_total,
            elapsed: 0.0,
            emitted_resolution: false,
            result_label,
        }
    }

    fn mark_ready_after_route(&mut self, target_tile: OriginalTilePoint) {
        if self.status == OriginalDebugActionStatus::RouteQueued {
            self.status = OriginalDebugActionStatus::Ready;
            self.target_tile = Some(target_tile);
            self.route_nodes = 0;
            self.elapsed = 0.0;
            self.result_label = format!("{} action ready after route", self.focus.label());
        }
    }

    fn update(&mut self, real_dt: f32, agent_slot: u8) -> Option<OriginalDebugActionResolution> {
        match self.status {
            OriginalDebugActionStatus::Ready => {
                self.status = OriginalDebugActionStatus::Resolving;
                self.elapsed = 0.0;
                self.result_label =
                    format!("{} candidate action resolving locally", self.focus.label());
                None
            }
            OriginalDebugActionStatus::Resolving => {
                self.elapsed += real_dt.max(0.0);
                if self.elapsed >= 0.35 {
                    self.status = OriginalDebugActionStatus::Resolved;
                    self.result_label = self.resolved_label();
                    if !self.emitted_resolution {
                        self.emitted_resolution = true;
                        return Some(OriginalDebugActionResolution {
                            agent_slot,
                            focus: self.focus,
                            target_tile: self.target_tile,
                            result_label: self.result_label.clone(),
                        });
                    }
                }
                None
            }
            OriginalDebugActionStatus::Blocked if !self.emitted_resolution => {
                self.emitted_resolution = true;
                Some(OriginalDebugActionResolution {
                    agent_slot,
                    focus: OriginalDebugInteractionFocus::None,
                    target_tile: self.target_tile,
                    result_label: self.result_label.clone(),
                })
            }
            OriginalDebugActionStatus::RouteQueued
            | OriginalDebugActionStatus::Resolved
            | OriginalDebugActionStatus::Blocked => None,
        }
    }

    fn resolved_label(&self) -> String {
        match self.focus {
            OriginalDebugInteractionFocus::DoorOpenCandidate => {
                "door/open candidate resolved in local control state; blocker mutation still gated"
                    .to_string()
            }
            OriginalDebugInteractionFocus::LargeDoorCandidate => {
                "large-door candidate resolved in local control state; rail/blocker mutation still gated"
                    .to_string()
            }
            OriginalDebugInteractionFocus::WeaponPickupCandidate => {
                "weapon pickup candidate resolved in local control state; inventory mutation still gated"
                    .to_string()
            }
            OriginalDebugInteractionFocus::VehicleEntryCandidate => {
                "vehicle entry candidate resolved in local control state; passenger mutation still gated"
                    .to_string()
            }
            OriginalDebugInteractionFocus::ObjectiveTargetCandidate => {
                "objective target contacted in local control state; mission completion remains gated"
                    .to_string()
            }
            OriginalDebugInteractionFocus::ScenarioTriggerCandidate => {
                "scenario trigger candidate contacted in local control state; action chain remains gated"
                    .to_string()
            }
            OriginalDebugInteractionFocus::None => {
                "no candidate action resolved; gameplay state unchanged".to_string()
            }
        }
    }

    fn panel_label(&self) -> String {
        format!(
            "action {} {} c{} {}",
            self.focus.label(),
            self.status.label(),
            self.candidate_total,
            self.result_label
        )
    }
}

impl OriginalDebugActionStatus {
    fn label(self) -> &'static str {
        match self {
            Self::RouteQueued => "queued",
            Self::Ready => "ready",
            Self::Resolving => "resolving",
            Self::Resolved => "resolved",
            Self::Blocked => "blocked",
        }
    }
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
            Self::OriginalMissionSceneProbe => "original first-mission control".to_string(),
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
        let render_mode = initial_render_mode(
            original_map_loaded,
            original_mission_scene.is_some(),
            graphics_loaded,
        );
        let mut camera = if original_map_loaded {
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
        let original_debug_agents = original_mission_scene
            .as_ref()
            .map(original_debug_agents_from_scene)
            .unwrap_or_default();
        if render_mode == MapRenderMode::OriginalMissionSceneProbe {
            if let (Some(agent), Some(map_tiles), Some(graphics)) = (
                original_debug_agents.first(),
                original_map_tiles.as_ref(),
                original_graphics.as_ref(),
            ) {
                camera.offset = original_agent_focus_camera_offset(
                    map_tiles,
                    graphics,
                    agent.current_tile(),
                    camera.zoom,
                    vec2(screen_width() * 0.5, screen_height() * 0.56),
                );
                if let Some(view) = original_map_view.as_ref() {
                    view.clamp_camera(&mut camera);
                }
            }
        }
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
            original_cursor_tile: None,
            original_cursor_screen: None,
            original_route_probe: None,
            original_interaction_probe: None,
            original_navigation_debug_enabled: render_mode
                == MapRenderMode::OriginalMissionSceneProbe,
            original_debug_agents,
            selected_original_debug_agent: 0,
            original_control_runtime: OriginalMissionControlRuntime::default(),
            original_control_trace: OriginalControlTrace::from_env(),
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
        self.update_original_scene_cursor_probe();
        self.update_original_debug_agents(real_dt);
        self.update_original_control_trace(real_dt);
        let dt = self.sim_clock.advance_dt(real_dt);
        for (key, idx) in [
            (KeyCode::Key1, 0),
            (KeyCode::Key2, 1),
            (KeyCode::Key3, 2),
            (KeyCode::Key4, 3),
        ] {
            if is_key_pressed(key)
                && self.render_mode == MapRenderMode::OriginalMissionSceneProbe
                && self.original_navigation_debug_enabled
            {
                self.select_original_debug_agent(idx, extend_original_debug_selection());
            } else if is_key_pressed(key) && idx < self.agents.len() {
                self.select(idx);
            }
        }
        if is_key_pressed(KeyCode::E) {
            self.try_original_interaction_probe();
        }
        if is_key_pressed(KeyCode::O) {
            self.try_original_control_smoke_route("keyboard");
        }
        if is_key_pressed(KeyCode::T) {
            self.toggle_original_control_trace();
        }
        if is_mouse_button_pressed(MouseButton::Right) {
            if !self.try_original_route_probe_order() {
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
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            if self.render_mode == MapRenderMode::OriginalMissionSceneProbe
                && self.original_navigation_debug_enabled
            {
                if !self.try_select_original_debug_agent_at_cursor() {
                    self.try_original_combat_probe_at_cursor();
                }
            } else if !self.try_select_original_debug_agent_at_cursor() {
                self.try_attack_at_mouse();
            }
        }
        if is_key_pressed(KeyCode::F) {
            self.focus_camera_on_selected_original_agent();
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
            if self.render_mode != MapRenderMode::OriginalMissionSceneProbe {
                self.original_route_probe = None;
                self.original_interaction_probe = None;
                self.clear_original_debug_agent_routes();
            } else {
                self.original_navigation_debug_enabled = true;
                self.ensure_original_debug_agents();
            }
            self.combat_log = format!("View mode: {}", self.render_mode.label());
        }
        if is_key_pressed(KeyCode::G) {
            if self.render_mode == MapRenderMode::OriginalMissionSceneProbe {
                self.original_navigation_debug_enabled = !self.original_navigation_debug_enabled;
                self.ensure_original_debug_agents();
                self.clear_original_debug_agent_routes();
                self.original_interaction_probe = None;
                let state = if self.original_navigation_debug_enabled {
                    "enabled"
                } else {
                    "disabled"
                };
                self.combat_log = format!(
                    "Original mission control {state}; gated local agents only, demo gameplay still available"
                );
            } else {
                self.combat_log =
                    "Original mission control is available only in first-mission mode".to_string();
            }
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

    fn update_original_scene_cursor_probe(&mut self) {
        if self.render_mode != MapRenderMode::OriginalMissionSceneProbe {
            self.original_cursor_tile = None;
            self.original_cursor_screen = None;
            return;
        }

        let (Some(map_tiles), Some(graphics)) = (
            self.original_map_tiles.as_ref(),
            self.original_graphics.as_ref(),
        ) else {
            self.original_cursor_tile = None;
            self.original_cursor_screen = None;
            return;
        };

        let mouse = vec2(mouse_position().0, mouse_position().1);
        let preferred_tile = self
            .original_route_probe
            .as_ref()
            .and_then(|probe| probe.requested_goal_tile.or(probe.goal_tile))
            .or_else(|| {
                self.primary_original_debug_interaction_intent()
                    .and_then(|intent| intent.target_tile)
            });
        self.original_cursor_tile = self.map.pick_original_tile_at_screen_with_preferred(
            &self.camera,
            map_tiles,
            graphics,
            mouse,
            preferred_tile,
        );
        self.original_cursor_screen = self.original_cursor_tile.map(|tile| {
            self.map
                .original_tile_point_screen(&self.camera, map_tiles, graphics, tile)
        });
    }

    fn try_original_route_probe_order(&mut self) -> bool {
        if self.render_mode != MapRenderMode::OriginalMissionSceneProbe {
            return false;
        }

        let Some(goal) = self.original_cursor_tile else {
            self.combat_log =
                "Original route probe blocked: cursor is outside the candidate map".to_string();
            self.original_route_probe = None;
            self.clear_original_debug_agent_routes();
            return true;
        };
        if self.original_mission_scene.is_none() {
            self.combat_log =
                "Original route blocked: first-mission scene model unavailable".to_string();
            self.original_route_probe = None;
            self.clear_original_debug_agent_routes();
            return true;
        }

        if self.original_navigation_debug_enabled {
            self.ensure_original_debug_agents();
            let append_order = append_original_route_order();
            let selected_agents = self
                .selected_original_debug_agent_indices()
                .into_iter()
                .filter_map(|idx| {
                    self.original_debug_agents
                        .get(idx)
                        .map(|agent| (idx, agent.route_order_start_tile(append_order)))
                })
                .collect::<Vec<_>>();
            if selected_agents.is_empty() {
                self.combat_log =
                    "Original movement blocked: no movable original-control ped seed".to_string();
                self.original_route_probe = None;
                return true;
            }
            let route_probes = {
                let scene = self.original_mission_scene.as_ref().expect("checked above");
                selected_agents
                    .into_iter()
                    .map(|(idx, start)| {
                        (idx, scene.original_route_debug_probe_between(start, goal))
                    })
                    .collect::<Vec<_>>()
            };
            let mut ready = 0;
            let mut blocked = 0;
            let mut primary_label = None;
            for (idx, route_probe) in route_probes {
                if primary_label.is_none() || idx == self.selected_original_debug_agent {
                    primary_label = Some(route_probe.panel_label());
                    self.original_route_probe = Some(route_probe.clone());
                }
                if route_probe.status == OriginalRuntimeRouteStatus::CandidateRouteReady
                    && route_probe.path.len() > 1
                {
                    ready += 1;
                    if let Some(agent) = self.original_debug_agents.get_mut(idx) {
                        agent.clear_interaction_intent();
                        agent.assign_route_from_probe(&route_probe, append_order);
                    }
                } else {
                    blocked += 1;
                    if let Some(agent) = self.original_debug_agents.get_mut(idx) {
                        agent.clear_interaction_intent();
                        agent.block_route();
                    }
                }
            }
            let order_kind = if append_order { "queued" } else { "order" };
            self.combat_log = format!(
                "Original mission movement {order_kind}: selected {}, ready {}, blocked {}; {}; demo gameplay active",
                ready + blocked,
                ready,
                blocked,
                primary_label.unwrap_or_else(|| "no route probe result".to_string())
            );
        } else {
            let route_probe = self
                .original_mission_scene
                .as_ref()
                .expect("checked above")
                .original_route_probe_to_tile(goal);
            self.combat_log = route_probe.panel_label();
            self.clear_original_debug_agent_routes();
            self.original_route_probe = Some(route_probe);
        }
        true
    }

    fn try_original_interaction_probe(&mut self) -> bool {
        if self.render_mode != MapRenderMode::OriginalMissionSceneProbe {
            self.combat_log =
                "Original interaction/action control is available only in first-mission mode"
                    .to_string();
            return false;
        }
        if self.original_mission_scene.is_none() {
            self.combat_log =
                "Original interaction/action blocked: first-mission scene model unavailable"
                    .to_string();
            self.original_interaction_probe = None;
            return true;
        }
        self.ensure_original_debug_agents();
        let selected_agents = self
            .selected_original_debug_agent_indices()
            .into_iter()
            .filter_map(|idx| {
                self.original_debug_agents
                    .get(idx)
                    .map(|agent| (idx, agent.current_tile()))
            })
            .collect::<Vec<_>>();
        let agent_tile = selected_agents
            .iter()
            .find(|(idx, _)| *idx == self.selected_original_debug_agent)
            .or_else(|| selected_agents.first())
            .map(|(_, tile)| *tile);
        let target_tile = self.original_cursor_tile;
        let (probe, intents) = {
            let scene_model = self.original_mission_scene.as_ref().expect("checked above");
            let probe = scene_model.original_debug_interaction_probe_between(
                agent_tile,
                target_tile,
                self.original_navigation_debug_enabled,
            );
            let intents = selected_agents
                .iter()
                .map(|(idx, start)| {
                    (
                        *idx,
                        scene_model.original_debug_interaction_intent_between(
                            Some(*start),
                            target_tile,
                            self.original_navigation_debug_enabled,
                        ),
                    )
                })
                .collect::<Vec<_>>();
            (probe, intents)
        };
        self.original_interaction_probe = Some(probe.clone());
        if intents.is_empty() {
            self.combat_log = probe.panel_label();
            return true;
        }

        let mut queued = 0;
        let mut ready = 0;
        let mut blocked = 0;
        let mut primary_label = None;
        for (idx, intent) in intents {
            if primary_label.is_none() || idx == self.selected_original_debug_agent {
                primary_label = Some(intent.panel_label());
            }
            match intent.status {
                OriginalDebugInteractionIntentStatus::RouteQueued => queued += 1,
                OriginalDebugInteractionIntentStatus::ReadyAtTarget => ready += 1,
                _ => blocked += 1,
            }
            if let Some(agent) = self.original_debug_agents.get_mut(idx) {
                agent.assign_interaction_intent(intent);
            }
        }
        self.combat_log = format!(
            "Original mission interaction intents: selected {}, queued {}, ready {}, blocked {}; {}; demo gameplay active",
            queued + ready + blocked,
            queued,
            ready,
            blocked,
            primary_label.unwrap_or_else(|| probe.panel_label())
        );
        true
    }

    fn update_original_debug_agents(&mut self, real_dt: f32) {
        if !self.original_navigation_debug_enabled
            || self.render_mode != MapRenderMode::OriginalMissionSceneProbe
        {
            return;
        }
        self.ensure_original_debug_agents();
        let mut resolutions = Vec::new();
        let movement_dt = real_dt.clamp(0.0, ORIGINAL_DEBUG_AGENT_MAX_STEP_DT);
        for agent in &mut self.original_debug_agents {
            if let Some(resolution) = agent.update(movement_dt) {
                resolutions.push(resolution);
            }
        }
        for resolution in resolutions {
            self.original_control_runtime.apply_resolution(resolution);
        }
    }

    fn update_original_control_trace(&mut self, real_dt: f32) {
        let force_emit = self.original_control_trace.begin_frame(real_dt);
        if force_emit {
            self.try_original_control_smoke_route("autopilot");
        }
        if self.original_control_trace.enabled {
            let signature = self.original_control_trace_signature();
            if self
                .original_control_trace
                .should_emit(&signature, force_emit)
            {
                println!("{}", self.original_control_trace.trace_line(&signature));
            }
        }
        if self.original_control_trace.should_quit() {
            println!(
                "[original-control] smoke complete after {} frames; exiting",
                self.original_control_trace.frame
            );
            std::process::exit(0);
        }
    }

    fn toggle_original_control_trace(&mut self) {
        self.original_control_trace.enabled = !self.original_control_trace.enabled;
        self.original_control_trace.next_emit_elapsed = 0.0;
        let state = if self.original_control_trace.enabled {
            "enabled"
        } else {
            "disabled"
        };
        self.combat_log = format!("Original control console trace {state}; local diagnostics only");
    }

    fn try_original_control_smoke_route(&mut self, trigger: &str) -> bool {
        if self.render_mode != MapRenderMode::OriginalMissionSceneProbe {
            self.combat_log =
                "Original smoke route is available only in first-mission control mode".to_string();
            return false;
        }
        if self.original_mission_scene.is_none() {
            self.combat_log =
                "Original smoke route blocked: first-mission scene model unavailable".to_string();
            return true;
        }
        self.original_navigation_debug_enabled = true;
        self.ensure_original_debug_agents();
        let Some(idx) = self
            .selected_original_debug_agent_indices()
            .into_iter()
            .next()
            .or_else(|| (!self.original_debug_agents.is_empty()).then_some(0))
        else {
            self.combat_log =
                "Original smoke route blocked: no movable original-control ped seed".to_string();
            self.original_route_probe = None;
            return true;
        };
        self.select_original_debug_agent(idx, false);
        let start = self.original_debug_agents[idx].route_order_start_tile(false);
        let route_probe = self
            .original_mission_scene
            .as_ref()
            .expect("checked above")
            .original_control_smoke_route_from(start);
        self.original_route_probe = Some(route_probe.clone());
        if route_probe.status == OriginalRuntimeRouteStatus::CandidateRouteReady
            && route_probe.path.len() > 1
        {
            let route_len = route_probe.path.len();
            if let Some(agent) = self.original_debug_agents.get_mut(idx) {
                agent.clear_interaction_intent();
                agent.assign_route_from_probe(&route_probe, false);
            }
            self.combat_log = format!(
                "Original smoke route {trigger}: agent {} queued {} nodes; demo gameplay active",
                idx + 1,
                route_len
            );
        } else {
            if let Some(agent) = self.original_debug_agents.get_mut(idx) {
                agent.block_route();
            }
            self.combat_log = format!(
                "Original smoke route {trigger} blocked: {}; demo gameplay active",
                route_probe.panel_label()
            );
        }
        true
    }

    fn original_control_trace_signature(&self) -> String {
        let route = self
            .original_route_probe
            .as_ref()
            .map(|probe| format!("route={:?}/{}nodes", probe.status, probe.path.len()))
            .unwrap_or_else(|| "route=none".to_string());
        let agents = self
            .original_debug_agents
            .iter()
            .take(4)
            .map(|agent| {
                let tile = agent.current_tile();
                let selected = if agent.selected { "*" } else { "" };
                format!(
                    "a{}{} rec{} {} tile={},{},{} route={}/{}",
                    agent.slot + 1,
                    selected,
                    agent.record_index,
                    agent.route_status.label(),
                    tile.tile_x,
                    tile.tile_y,
                    tile.tile_z,
                    agent.route_progress.floor() as usize,
                    agent.route.len().saturating_sub(1)
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        format!(
            "mode={} control={} agents={} selected={} {route} {agents}",
            self.render_mode.label(),
            self.original_navigation_debug_enabled,
            self.original_debug_agents.len(),
            self.selected_original_debug_agent + 1,
        )
    }

    fn ensure_original_debug_agents(&mut self) {
        if self.original_debug_agents.is_empty() {
            if let Some(scene_model) = self.original_mission_scene.as_ref() {
                self.original_debug_agents = original_debug_agents_from_scene(scene_model);
            }
        }
        if self.selected_original_debug_agent >= self.original_debug_agents.len() {
            self.selected_original_debug_agent = 0;
        }
        self.ensure_original_debug_agent_selection();
    }

    fn select_original_debug_agent(&mut self, idx: usize, extend: bool) -> bool {
        self.ensure_original_debug_agents();
        if idx >= self.original_debug_agents.len() {
            return false;
        }
        self.selected_original_debug_agent = idx;
        if extend {
            if let Some(agent) = self.original_debug_agents.get_mut(idx) {
                agent.selected = !agent.selected;
            }
            if !self
                .original_debug_agents
                .iter()
                .any(|agent| agent.selected)
            {
                if let Some(agent) = self.original_debug_agents.get_mut(idx) {
                    agent.selected = true;
                }
            }
        } else {
            for (agent_idx, agent) in self.original_debug_agents.iter_mut().enumerate() {
                agent.selected = agent_idx == idx;
            }
        }
        let agent = &self.original_debug_agents[idx];
        let selected_count = self
            .original_debug_agents
            .iter()
            .filter(|agent| agent.selected)
            .count();
        self.combat_log = format!(
            "Selected original agent {}; selected set {}; original control is gated/local",
            agent.slot + 1,
            selected_count
        );
        true
    }

    fn ensure_original_debug_agent_selection(&mut self) {
        if self.original_debug_agents.is_empty() {
            return;
        }
        if !self
            .original_debug_agents
            .iter()
            .any(|agent| agent.selected)
        {
            if let Some(agent) = self
                .original_debug_agents
                .get_mut(self.selected_original_debug_agent)
            {
                agent.selected = true;
            }
        }
    }

    fn selected_original_debug_agent_indices(&self) -> Vec<usize> {
        self.original_debug_agents
            .iter()
            .enumerate()
            .filter_map(|(idx, agent)| agent.selected.then_some(idx))
            .collect()
    }

    fn clear_original_debug_agent_routes(&mut self) {
        for agent in &mut self.original_debug_agents {
            agent.clear_route();
            agent.clear_interaction_intent();
        }
    }

    fn try_select_original_debug_agent_at_cursor(&mut self) -> bool {
        if self.render_mode != MapRenderMode::OriginalMissionSceneProbe
            || !self.original_navigation_debug_enabled
        {
            return false;
        }
        let Some(cursor) = self.original_cursor_tile else {
            return false;
        };
        self.ensure_original_debug_agents();
        let Some((idx, _)) = self
            .original_debug_agents
            .iter()
            .enumerate()
            .filter_map(|(idx, agent)| {
                let tile = agent.current_tile();
                let xy = tile.tile_x.abs_diff(cursor.tile_x) + tile.tile_y.abs_diff(cursor.tile_y);
                let z = tile.tile_z.abs_diff(cursor.tile_z);
                (xy <= 1 && z <= 1).then_some((idx, xy + z))
            })
            .min_by_key(|(_, distance)| *distance)
        else {
            return false;
        };
        self.select_original_debug_agent(idx, extend_original_debug_selection())
    }

    fn controlled_original_ped_record_indices(&self) -> Vec<u16> {
        if self.render_mode != MapRenderMode::OriginalMissionSceneProbe
            || !self.original_navigation_debug_enabled
        {
            return Vec::new();
        }
        self.original_mission_scene
            .as_ref()
            .map(OriginalMissionScene::original_control_suppressed_ped_record_indices)
            .unwrap_or_else(|| {
                self.original_debug_agents
                    .iter()
                    .map(|agent| agent.record_index)
                    .collect()
            })
    }

    fn original_debug_agent_panel_label(&self) -> String {
        if !self.original_navigation_debug_enabled {
            return "original control gated by G; demo gameplay remains active".to_string();
        }
        let Some(agent) = self
            .original_debug_agents
            .get(self.selected_original_debug_agent)
        else {
            return "debug agents unavailable: no movable original-control ped seed".to_string();
        };
        let selected_count = self
            .original_debug_agents
            .iter()
            .filter(|agent| agent.selected)
            .count();
        let moving = self
            .original_debug_agents
            .iter()
            .filter(|agent| agent.route_status == OriginalDebugAgentRouteStatus::Moving)
            .count();
        let blocked = self
            .original_debug_agents
            .iter()
            .filter(|agent| agent.route_status == OriginalDebugAgentRouteStatus::Blocked)
            .count();
        let interaction_queued = self
            .original_debug_agents
            .iter()
            .filter(|agent| {
                agent.interaction_intent.as_ref().is_some_and(|intent| {
                    intent.status == OriginalDebugInteractionIntentStatus::RouteQueued
                })
            })
            .count();
        let interaction_ready = self
            .original_debug_agents
            .iter()
            .filter(|agent| {
                agent.interaction_intent.as_ref().is_some_and(|intent| {
                    intent.status == OriginalDebugInteractionIntentStatus::ReadyAtTarget
                })
            })
            .count();
        let primary_intent = agent
            .interaction_intent
            .as_ref()
            .map(|intent| format!("{} {}", intent.focus.label(), intent.status.label()))
            .unwrap_or_else(|| "none".to_string());
        let primary_action = agent
            .action_state
            .as_ref()
            .map(OriginalDebugActionState::panel_label)
            .unwrap_or_else(|| "action none".to_string());
        format!(
            "original agents {}; selected set {} primary {} at {},{},{}; route nodes {}; moving {} blocked {}; intents q/r {}/{} primary {}; {}; {}; demo gameplay available",
            self.original_debug_agents.len(),
            selected_count,
            agent.slot + 1,
            agent.current_tile().tile_x,
            agent.current_tile().tile_y,
            agent.current_tile().tile_z,
            agent.route.len(),
            moving,
            blocked,
            interaction_queued,
            interaction_ready,
            primary_intent,
            primary_action,
            agent.render_label()
        )
    }

    fn focus_camera_on_selected_original_agent(&mut self) -> bool {
        if self.render_mode != MapRenderMode::OriginalMissionSceneProbe
            || !self.original_navigation_debug_enabled
        {
            return false;
        }
        let (Some(map_tiles), Some(graphics), Some(agent)) = (
            self.original_map_tiles.as_ref(),
            self.original_graphics.as_ref(),
            self.original_debug_agents
                .get(self.selected_original_debug_agent),
        ) else {
            self.combat_log = "Original focus blocked: no selected original agent".to_string();
            return true;
        };
        let tile = agent.current_tile();
        let slot = agent.slot;
        self.camera.offset = original_agent_focus_camera_offset(
            map_tiles,
            graphics,
            tile,
            self.camera.zoom,
            vec2(screen_width() * 0.5, screen_height() * 0.56),
        );
        self.clamp_original_map_camera();
        self.combat_log = format!(
            "Focused camera on original agent {}; original control active",
            slot + 1
        );
        true
    }

    fn try_original_combat_probe_at_cursor(&mut self) -> bool {
        if self.render_mode != MapRenderMode::OriginalMissionSceneProbe
            || !self.original_navigation_debug_enabled
        {
            return false;
        }
        let Some(cursor) = self.original_cursor_tile else {
            self.combat_log = "Original combat probe blocked: cursor outside map".to_string();
            return true;
        };
        self.ensure_original_debug_agents();
        let Some(agent) = self
            .original_debug_agents
            .get(self.selected_original_debug_agent)
        else {
            self.combat_log =
                "Original combat probe blocked: no selected original agent".to_string();
            return true;
        };
        let Some(scene_model) = self.original_mission_scene.as_ref() else {
            self.combat_log = "Original combat probe blocked: scene model unavailable".to_string();
            return true;
        };
        let agent_tile = agent.current_tile();
        let squad_records = self
            .original_debug_agents
            .iter()
            .map(|agent| agent.record_index)
            .collect::<BTreeSet<_>>();
        let target = scene_model
            .objects
            .iter()
            .filter(|object| {
                object.kind == OriginalMissionObjectKind::Ped
                    && object.candidate_draw
                    && !squad_records.contains(&object.record_index)
            })
            .filter_map(|object| object.tile.map(|tile| (object.record_index, tile)))
            .filter(|(_, tile)| original_tile_near(*tile, cursor, 1, 1))
            .min_by_key(|(_, tile)| original_tile_distance(agent_tile, *tile));
        let Some((record_index, target_tile)) = target else {
            self.combat_log =
                "Original combat probe: no candidate non-squad ped at cursor".to_string();
            return true;
        };
        let distance = original_tile_distance(agent_tile, target_tile);
        let in_range = distance <= 8;
        self.original_control_runtime
            .record_combat_probe(record_index, distance, in_range);
        self.combat_log = if in_range {
            format!(
                "Original combat probe: ped candidate {record_index} in range {distance}; line/hit/death state blocked until weapon semantics are proven"
            )
        } else {
            format!(
                "Original combat probe: ped candidate {record_index} range {distance} exceeds provisional weapon range; no gameplay state changed"
            )
        };
        true
    }

    fn primary_original_debug_interaction_intent(&self) -> Option<&OriginalDebugInteractionIntent> {
        self.original_debug_agents
            .get(self.selected_original_debug_agent)
            .and_then(|agent| agent.interaction_intent.as_ref())
            .or_else(|| {
                self.original_debug_agents
                    .iter()
                    .find_map(|agent| agent.interaction_intent.as_ref())
            })
    }

    fn selected_agent_hud_name(&self) -> &str {
        if self.render_mode == MapRenderMode::OriginalMissionSceneProbe
            && self.original_navigation_debug_enabled
            && !self.original_debug_agents.is_empty()
        {
            "ORIGINAL AGENT"
        } else {
            self.agents[self.selected].name
        }
    }

    fn selected_agent_hud_order(&self) -> String {
        if self.render_mode == MapRenderMode::OriginalMissionSceneProbe
            && self.original_navigation_debug_enabled
            && !self.original_debug_agents.is_empty()
        {
            self.original_debug_agent_panel_label()
        } else {
            self.agents[self.selected].order_summary()
        }
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
                    let controlled_ped_record_indices =
                        self.controlled_original_ped_record_indices();
                    self.map.draw_original_mission_scene(
                        &self.camera,
                        map_tiles,
                        self.original_tile_types.as_ref(),
                        graphics,
                        scene_model,
                        object_graphics,
                        self.original_object_animation_frame(),
                        &controlled_ped_record_indices,
                    );
                    self.map.draw_original_route_probe_overlay(
                        &self.camera,
                        map_tiles,
                        graphics,
                        self.original_cursor_tile,
                        self.original_route_probe.as_ref(),
                        self.original_cursor_screen,
                    );
                    if let Some(intent) = self.primary_original_debug_interaction_intent() {
                        self.map.draw_original_debug_interaction_overlay(
                            &self.camera,
                            map_tiles,
                            graphics,
                            intent.target_tile,
                            intent.focus.label(),
                            intent.status == OriginalDebugInteractionIntentStatus::ReadyAtTarget,
                        );
                    }
                    if self.original_navigation_debug_enabled {
                        for agent in &self.original_debug_agents {
                            let object = agent
                                .sprite_ready
                                .then(|| scene_model.debug_agent_object(agent.record_index))
                                .flatten();
                            let directional_object = agent.render_object_candidate(object);
                            self.map.draw_original_debug_agent_marker(
                                &self.camera,
                                map_tiles,
                                graphics,
                                object_graphics,
                                directional_object.as_ref(),
                                agent.route_anchor_tile(),
                                &agent.route,
                                agent.route_progress,
                                agent.selected,
                                &agent.map_label(),
                                agent.animation_frame(self.original_object_animation_frame()),
                            );
                        }
                    }
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
            let original_debug_agent_label = self.original_debug_agent_panel_label();
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
                self.original_cursor_tile,
                self.original_route_probe.as_ref(),
                self.original_interaction_probe.as_ref(),
                self.original_navigation_debug_enabled,
                &original_debug_agent_label,
                &self.original_control_runtime.panel_label(),
            );
        }

        let hud_order = self.selected_agent_hud_order();
        ui::draw_hud(
            &self.assets,
            self.selected_agent_hud_name(),
            &hud_order,
            &self.combat_log,
            &format!(
                "{} | view {}",
                self.sim_clock.label(),
                self.render_mode.label()
            ),
        );
    }
}

impl OriginalDebugAgent {
    fn from_spawn(spawn: OriginalDebugAgentSpawn, selected: bool) -> Self {
        Self {
            slot: spawn.slot,
            record_index: spawn.record_index,
            tile: spawn.tile,
            route: Vec::new(),
            route_progress: 0.0,
            selected,
            sprite_ready: spawn.sprite_ready,
            route_status: OriginalDebugAgentRouteStatus::Idle,
            direction: OriginalDebugAgentDirection::South,
            interaction_intent: None,
            action_state: None,
        }
    }

    fn assign_route(&mut self, route: Vec<OriginalTilePoint>, append: bool) {
        let current_tile = self.current_tile();
        self.assign_route_from_current(route, append, current_tile);
    }

    fn assign_route_from_current(
        &mut self,
        route: Vec<OriginalTilePoint>,
        append: bool,
        current_tile: OriginalTilePoint,
    ) {
        let had_existing_route = !self.route.is_empty();
        let mut route = if append && !self.route.is_empty() {
            let mut appended = self.route.clone();
            appended.extend(route.into_iter().skip(1));
            appended
        } else {
            route
        };
        if route.first().is_none_or(|first| *first != current_tile) && !append {
            route.insert(0, current_tile);
        }
        if let Some(next) = route.get(1).copied() {
            self.direction = OriginalDebugAgentDirection::from_step(route[0], next);
        }
        let progress = if append && had_existing_route {
            self.route_progress
                .min(route.len().saturating_sub(1) as f32)
        } else {
            0.0
        };
        self.route = route;
        self.route_progress = progress;
        self.route_status = OriginalDebugAgentRouteStatus::Queued;
    }

    fn assign_route_from_probe(&mut self, route_probe: &OriginalRuntimeRouteProbe, append: bool) {
        if append {
            self.assign_route(route_probe.path.clone(), true);
            return;
        }
        let start_tile = route_probe
            .start_tile
            .or_else(|| route_probe.path.first().copied())
            .unwrap_or_else(|| self.current_tile());
        self.tile = start_tile;
        self.assign_route_from_current(route_probe.path.clone(), false, start_tile);
    }

    fn clear_route(&mut self) {
        self.route.clear();
        self.route_progress = 0.0;
        self.route_status = OriginalDebugAgentRouteStatus::Idle;
    }

    fn block_route(&mut self) {
        self.route.clear();
        self.route_progress = 0.0;
        self.route_status = OriginalDebugAgentRouteStatus::Blocked;
    }

    fn clear_interaction_intent(&mut self) {
        self.interaction_intent = None;
        self.action_state = None;
    }

    fn assign_interaction_intent(&mut self, intent: OriginalDebugInteractionIntent) {
        let action_state = OriginalDebugActionState::from_intent(&intent);
        match intent.status {
            OriginalDebugInteractionIntentStatus::RouteQueued if intent.route_path.len() > 1 => {
                if let Some(start_tile) = intent.route_path.first().copied() {
                    self.tile = start_tile;
                    self.assign_route_from_current(intent.route_path.clone(), false, start_tile);
                } else {
                    self.assign_route(intent.route_path.clone(), false);
                }
                self.interaction_intent = Some(intent);
                self.action_state = Some(action_state);
            }
            OriginalDebugInteractionIntentStatus::ReadyAtTarget => {
                self.route.clear();
                self.route_progress = 0.0;
                self.route_status = OriginalDebugAgentRouteStatus::Arrived;
                self.interaction_intent = Some(intent);
                self.action_state = Some(action_state);
            }
            _ => {
                self.block_route();
                self.interaction_intent = Some(intent);
                self.action_state = Some(action_state);
            }
        }
    }

    fn update(&mut self, real_dt: f32) -> Option<OriginalDebugActionResolution> {
        if self.route.len() >= 2 {
            self.route_status = OriginalDebugAgentRouteStatus::Moving;
            let previous_tile = self.current_tile();
            let max_progress = (self.route.len() - 1) as f32;
            self.route_progress = (self.route_progress + real_dt.max(0.0) * 4.0).min(max_progress);
            let next_tile = self.current_tile();
            if previous_tile != next_tile {
                self.direction = OriginalDebugAgentDirection::from_step(previous_tile, next_tile);
            }
            if self.route_progress >= max_progress {
                if let Some(last) = self.route.last().copied() {
                    self.tile = last;
                }
                self.route_status = OriginalDebugAgentRouteStatus::Arrived;
                if let Some(intent) = self.interaction_intent.as_mut() {
                    intent.mark_ready_after_route(self.tile);
                }
                if let Some(action) = self.action_state.as_mut() {
                    action.mark_ready_after_route(self.tile);
                }
            }
        }
        self.action_state
            .as_mut()
            .and_then(|action| action.update(real_dt, self.slot))
    }

    fn current_tile(&self) -> OriginalTilePoint {
        if self.route.is_empty() {
            return self.tile;
        }
        let index = self
            .route_progress
            .floor()
            .clamp(0.0, self.route.len().saturating_sub(1) as f32) as usize;
        self.route[index]
    }

    fn route_anchor_tile(&self) -> OriginalTilePoint {
        if self.route.is_empty() {
            self.tile
        } else {
            self.current_tile()
        }
    }

    fn route_order_start_tile(&self, append: bool) -> OriginalTilePoint {
        if append {
            self.route
                .last()
                .copied()
                .unwrap_or_else(|| self.current_tile())
        } else {
            self.current_tile()
        }
    }

    fn render_label(&self) -> &'static str {
        if self.sprite_ready {
            "sprite proof ready"
        } else {
            "marker-only sprite proof blocked"
        }
    }

    fn map_label(&self) -> String {
        let selected = if self.selected { "selected" } else { "debug" };
        format!(
            "{selected} agent {} {}{}",
            self.slot + 1,
            self.route_status.label(),
            self.interaction_intent
                .as_ref()
                .map(|intent| format!(" {}", intent.focus.label()))
                .unwrap_or_default()
        )
    }

    fn animation_frame(&self, global_frame: u16) -> u16 {
        let walk_phase = if self.route_status == OriginalDebugAgentRouteStatus::Moving {
            global_frame % 8
        } else {
            0
        };
        self.direction.frame_bias().saturating_add(walk_phase)
    }

    fn render_anchor_tile(&self) -> OriginalTilePoint {
        let mut tile = self.current_tile();
        tile.off_x = 0;
        tile.off_y = 0;
        tile.off_z = 0;
        tile
    }

    fn render_object_candidate(
        &self,
        object: Option<&OriginalMissionObjectCandidate>,
    ) -> Option<OriginalMissionObjectCandidate> {
        object.cloned().map(|mut object| {
            object.tile = Some(self.render_anchor_tile());
            object.orientation = Some(self.direction.orientation_byte());
            if self.route_status == OriginalDebugAgentRouteStatus::Moving {
                object.state = Some(0x10);
            }
            object
        })
    }
}

impl OriginalDebugAgentRouteStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Queued => "route queued",
            Self::Moving => "moving",
            Self::Arrived => "arrived",
            Self::Blocked => "blocked",
        }
    }
}

impl OriginalDebugAgentDirection {
    fn from_step(from: OriginalTilePoint, to: OriginalTilePoint) -> Self {
        let dx = to.tile_x as i32 - from.tile_x as i32;
        let dy = to.tile_y as i32 - from.tile_y as i32;
        match (dx.signum(), dy.signum()) {
            (0, 1) => Self::South,
            (1, 1) => Self::SouthEast,
            (1, 0) => Self::East,
            (1, -1) => Self::NorthEast,
            (0, -1) => Self::North,
            (-1, -1) => Self::NorthWest,
            (-1, 0) => Self::West,
            (-1, 1) => Self::SouthWest,
            _ => Self::South,
        }
    }

    fn orientation_byte(self) -> u8 {
        match self {
            Self::South => 0,
            Self::SouthEast => 32,
            Self::East => 64,
            Self::NorthEast => 96,
            Self::North => 128,
            Self::NorthWest => 160,
            Self::West => 192,
            Self::SouthWest => 224,
        }
    }

    fn frame_bias(self) -> u16 {
        (self.orientation_byte() / 32) as u16
    }
}

fn original_debug_agents_from_scene(scene_model: &OriginalMissionScene) -> Vec<OriginalDebugAgent> {
    scene_model
        .debug_agent_spawns()
        .into_iter()
        .enumerate()
        .map(|(idx, spawn)| OriginalDebugAgent::from_spawn(spawn, idx == 0))
        .collect()
}

fn original_agent_focus_camera_offset(
    map_tiles: &OriginalMapTiles,
    graphics: &RuntimeOriginalGraphics,
    tile: OriginalTilePoint,
    zoom: f32,
    screen_anchor: Vec2,
) -> Vec2 {
    let world = original_agent_focus_world_point(map_tiles, graphics, tile);
    screen_anchor - world * zoom
}

#[cfg(test)]
fn original_agent_focus_camera_offset_from_tile_size(
    map_tiles: &OriginalMapTiles,
    tile: OriginalTilePoint,
    zoom: f32,
    screen_anchor: Vec2,
    tile_width: f32,
    tile_height: f32,
) -> Vec2 {
    let world =
        original_agent_focus_world_point_from_tile_size(map_tiles, tile, tile_width, tile_height);
    screen_anchor - world * zoom
}

fn original_agent_focus_world_point(
    map_tiles: &OriginalMapTiles,
    graphics: &RuntimeOriginalGraphics,
    tile: OriginalTilePoint,
) -> Vec2 {
    original_agent_focus_world_point_from_tile_size(
        map_tiles,
        tile,
        graphics.bank().record_width as f32,
        graphics.bank().record_height as f32,
    )
}

fn original_agent_focus_world_point_from_tile_size(
    map_tiles: &OriginalMapTiles,
    tile: OriginalTilePoint,
    tile_width: f32,
    tile_height: f32,
) -> Vec2 {
    original_map_tile_world_top_left(
        map_tiles,
        tile.tile_x as f32,
        tile.tile_y as f32,
        tile.tile_z.saturating_add(1) as f32,
        tile_width,
        tile_height,
    ) + vec2(tile_width * 0.5, tile_height * 2.0 / 3.0)
}

fn initial_render_mode(
    original_map_loaded: bool,
    original_scene_loaded: bool,
    graphics_loaded: bool,
) -> MapRenderMode {
    if original_map_loaded && original_scene_loaded {
        MapRenderMode::OriginalMissionSceneProbe
    } else if original_map_loaded {
        MapRenderMode::OriginalMapTiles
    } else if graphics_loaded {
        MapRenderMode::OriginalGraphicsAtlas
    } else {
        MapRenderMode::DemoCity
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn compact_asset_label(label: &str) -> &str {
    label.rsplit('/').next().unwrap_or(label)
}

fn original_cursor_tile_panel_label(tile: Option<OriginalTilePoint>) -> String {
    tile.map(|tile| {
        format!(
            "cursor tile candidate {},{},{} off {},{}; local control target",
            tile.tile_x, tile.tile_y, tile.tile_z, tile.off_x, tile.off_y
        )
    })
    .unwrap_or_else(|| "cursor tile candidate unavailable".to_string())
}

fn extend_original_debug_selection() -> bool {
    is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
}

fn append_original_route_order() -> bool {
    extend_original_debug_selection()
}

fn original_tile_short_label(tile: OriginalTilePoint) -> String {
    format!("{},{},{}", tile.tile_x, tile.tile_y, tile.tile_z)
}

fn original_tile_distance(a: OriginalTilePoint, b: OriginalTilePoint) -> u16 {
    a.tile_x.abs_diff(b.tile_x) + a.tile_y.abs_diff(b.tile_y) + a.tile_z.abs_diff(b.tile_z)
}

fn original_tile_near(a: OriginalTilePoint, b: OriginalTilePoint, xy: u16, z: u16) -> bool {
    a.tile_x.abs_diff(b.tile_x) + a.tile_y.abs_diff(b.tile_y) <= xy
        && a.tile_z.abs_diff(b.tile_z) <= z
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
    original_cursor_tile: Option<OriginalTilePoint>,
    original_route_probe: Option<&OriginalRuntimeRouteProbe>,
    original_interaction_probe: Option<&OriginalDebugInteractionProbe>,
    original_navigation_debug_enabled: bool,
    original_debug_agent_label: &str,
    original_control_runtime_label: &str,
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
            MapRenderMode::OriginalMissionSceneProbe => "runtime original first-mission control",
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
                    &scene_model.spatial_probe.panel_label(),
                    x + 16.0,
                    y + 364.0,
                    11.0,
                    YELLOW,
                );
                draw_text(
                    &original_cursor_tile_panel_label(original_cursor_tile),
                    x + 16.0,
                    y + 382.0,
                    11.0,
                    GRAY,
                );
                let route_label = original_route_probe
                    .map(OriginalRuntimeRouteProbe::panel_label)
                    .unwrap_or_else(|| "route probe: right-click original map to test".to_string());
                draw_text(&route_label, x + 16.0, y + 404.0, 11.0, GRAY);
                let debug_gate = if original_navigation_debug_enabled {
                    format!(
                        "G original control ON | {}",
                        scene_model.navigation_debug_probe.panel_label()
                    )
                } else {
                    "G original control OFF | same-level route probe only".to_string()
                };
                draw_text(
                    &debug_gate,
                    x + 16.0,
                    y + 424.0,
                    10.5,
                    if original_navigation_debug_enabled {
                        SKYBLUE
                    } else {
                        GRAY
                    },
                );
                draw_text(
                    original_debug_agent_label,
                    x + 16.0,
                    y + 444.0,
                    10.5,
                    if original_navigation_debug_enabled {
                        SKYBLUE
                    } else {
                        GRAY
                    },
                );
                draw_text(
                    &format!(
                        "nav links {}, occupied {}, blockers {}; doors {} windows {}",
                        scene_model.navigation_probe.map_object_link_cells,
                        scene_model.navigation_probe.candidate_occupied_tiles,
                        scene_model.navigation_probe.static_blocking_candidates,
                        scene_model.navigation_probe.door_candidates,
                        scene_model.navigation_probe.window_candidates
                    ),
                    x + 16.0,
                    y + 464.0,
                    10.5,
                    GRAY,
                );
                draw_text(
                    &scene_model.interaction_probe.panel_label(),
                    x + 16.0,
                    y + 484.0,
                    10.5,
                    GRAY,
                );
                draw_text(
                    &scene_model.objective_debug_probe.panel_label(),
                    x + 16.0,
                    y + 504.0,
                    10.5,
                    GRAY,
                );
                let interaction_probe_label = original_interaction_probe
                    .map(OriginalDebugInteractionProbe::panel_label)
                    .unwrap_or_else(|| "E action: gated candidate buckets only".to_string());
                draw_text(
                    &interaction_probe_label,
                    x + 16.0,
                    y + 524.0,
                    10.5,
                    if original_navigation_debug_enabled {
                        SKYBLUE
                    } else {
                        GRAY
                    },
                );
                draw_text(
                    original_control_runtime_label,
                    x + 16.0,
                    y + 544.0,
                    10.5,
                    if original_navigation_debug_enabled {
                        SKYBLUE
                    } else {
                        GRAY
                    },
                );
                draw_text(
                    "original control is gated/local; demo grid remains available",
                    x + 16.0,
                    y + 564.0,
                    10.5,
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
                y + 594.0,
                12.0,
                YELLOW,
            );
            draw_text(
                "Gameplay/pathfinding remain on the demo tactical grid",
                x + 16.0,
                y + 612.0,
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
        MapRenderMode::OriginalMissionSceneProbe => 636.0,
        MapRenderMode::OriginalMapTiles => 292.0,
        MapRenderMode::BlockAddressability => 212.0,
        MapRenderMode::OriginalGraphicsMap | MapRenderMode::OriginalGraphicsAtlas => 204.0,
        MapRenderMode::CandidateField(_) => 180.0,
        _ => 156.0,
    }
}

fn map_panel_width(mode: MapRenderMode) -> f32 {
    match mode {
        MapRenderMode::OriginalMissionSceneProbe => 600.0,
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

#[cfg(test)]
mod tests {
    use super::{
        MapRenderMode, OriginalDebugActionStatus, OriginalDebugAgent, OriginalDebugAgentDirection,
        OriginalDebugAgentRouteStatus, OriginalDebugAgentSpawn, OriginalMissionControlRuntime,
        initial_render_mode, original_agent_focus_camera_offset_from_tile_size,
        original_agent_focus_world_point_from_tile_size,
    };
    use crate::engine::{
        map_tiles::OriginalMapTiles,
        mission_scene::{
            OriginalAnimationRefs, OriginalDebugInteractionFocus, OriginalDebugInteractionIntent,
            OriginalDebugInteractionIntentStatus, OriginalDrawStage,
            OriginalMissionObjectCandidate, OriginalMissionObjectKind, OriginalRouteTransitionKind,
            OriginalRuntimeRouteProbe, OriginalRuntimeRouteStatus, OriginalTilePoint,
        },
    };
    use macroquad::prelude::*;

    fn tile(tile_x: u16, tile_y: u16, tile_z: u16) -> OriginalTilePoint {
        OriginalTilePoint {
            tile_x,
            tile_y,
            tile_z,
            off_x: 128,
            off_y: 128,
            off_z: 0,
        }
    }

    #[test]
    fn debug_agent_moves_along_local_route_without_gameplay_state() {
        let mut agent = OriginalDebugAgent::from_spawn(
            OriginalDebugAgentSpawn {
                slot: 1,
                record_index: 7,
                tile: tile(4, 5, 0),
                sprite_ready: true,
            },
            true,
        );

        agent.assign_route(vec![tile(4, 5, 0), tile(5, 5, 0), tile(6, 6, 0)], false);
        agent.update(0.25);
        assert_eq!(agent.current_tile(), tile(5, 5, 0));
        assert_eq!(agent.route_status, OriginalDebugAgentRouteStatus::Moving);
        assert_eq!(agent.direction, OriginalDebugAgentDirection::East);
        assert!(agent.selected);
        assert_eq!(agent.render_label(), "sprite proof ready");

        agent.update(4.0);
        assert_eq!(agent.current_tile(), tile(6, 6, 0));
        assert_eq!(agent.route_status, OriginalDebugAgentRouteStatus::Arrived);
        agent.clear_route();
        assert_eq!(agent.current_tile(), tile(6, 6, 0));
    }

    #[test]
    fn debug_agent_route_probe_snaps_to_surface_without_spawn_z_stub() {
        let mut agent = OriginalDebugAgent::from_spawn(
            OriginalDebugAgentSpawn {
                slot: 0,
                record_index: 0,
                tile: tile(4, 5, 2),
                sprite_ready: true,
            },
            true,
        );
        let route_probe = OriginalRuntimeRouteProbe {
            status: OriginalRuntimeRouteStatus::CandidateRouteReady,
            start_tile: Some(tile(4, 5, 1)),
            goal_tile: Some(tile(6, 5, 1)),
            requested_goal_tile: Some(tile(6, 5, 1)),
            snap: None,
            transition_kind: OriginalRouteTransitionKind::SameLevelOnly,
            path: vec![tile(4, 5, 1), tile(5, 5, 1), tile(6, 5, 1)],
            message: "synthetic route ready".to_string(),
        };

        agent.assign_route_from_probe(&route_probe, false);

        assert_eq!(agent.current_tile(), tile(4, 5, 1));
        assert_eq!(agent.route.first().copied(), Some(tile(4, 5, 1)));
        assert!(!agent.route.contains(&tile(4, 5, 2)));
        assert_eq!(agent.route.len(), 3);
    }

    #[test]
    fn debug_agent_second_route_after_arrival_does_not_replay_previous_start() {
        let mut agent = OriginalDebugAgent::from_spawn(
            OriginalDebugAgentSpawn {
                slot: 0,
                record_index: 0,
                tile: tile(1, 1, 0),
                sprite_ready: true,
            },
            true,
        );
        agent.assign_route(vec![tile(1, 1, 0), tile(2, 1, 0), tile(3, 1, 0)], false);
        agent.update(4.0);
        assert_eq!(agent.current_tile(), tile(3, 1, 0));

        let route_probe = OriginalRuntimeRouteProbe {
            status: OriginalRuntimeRouteStatus::CandidateRouteReady,
            start_tile: Some(tile(3, 1, 0)),
            goal_tile: Some(tile(5, 1, 0)),
            requested_goal_tile: Some(tile(5, 1, 0)),
            snap: None,
            transition_kind: OriginalRouteTransitionKind::SameLevelOnly,
            path: vec![tile(3, 1, 0), tile(4, 1, 0), tile(5, 1, 0)],
            message: "synthetic second route ready".to_string(),
        };

        agent.assign_route_from_probe(&route_probe, false);

        assert_eq!(agent.current_tile(), tile(3, 1, 0));
        assert_eq!(agent.route.first().copied(), Some(tile(3, 1, 0)));
        assert!(!agent.route.contains(&tile(1, 1, 0)));
        assert_eq!(agent.route.len(), 3);
    }

    #[test]
    fn debug_agent_applies_directional_render_state_without_mutating_scene_object() {
        let mut agent = OriginalDebugAgent::from_spawn(
            OriginalDebugAgentSpawn {
                slot: 0,
                record_index: 0,
                tile: tile(4, 5, 0),
                sprite_ready: true,
            },
            true,
        );
        agent.assign_route(vec![tile(4, 5, 0), tile(5, 5, 0)], false);
        agent.update(0.1);
        let object = OriginalMissionObjectCandidate {
            kind: OriginalMissionObjectKind::Ped,
            record_index: 0,
            desc: Some(0x04),
            state: Some(0),
            type_value: Some(0),
            subtype_value: Some(0),
            orientation: Some(0),
            tile: Some(OriginalTilePoint {
                tile_x: 4,
                tile_y: 5,
                tile_z: 0,
                off_x: 180,
                off_y: 172,
                off_z: 0,
            }),
            queue_tile: Some(tile(4, 5, 0)),
            animation: OriginalAnimationRefs {
                base_anim: Some(0),
                current_anim: Some(0),
                current_frame: Some(0),
            },
            candidate_record: true,
            candidate_draw: true,
            draw_stage: Some(OriginalDrawStage::People),
        };

        let rendered = agent.render_object_candidate(Some(&object)).unwrap();

        assert_eq!(object.orientation, Some(0));
        assert_eq!(rendered.orientation, Some(64));
        assert_eq!(rendered.state, Some(0x10));
        assert_eq!(
            rendered.tile,
            Some(OriginalTilePoint {
                tile_x: 4,
                tile_y: 5,
                tile_z: 0,
                off_x: 0,
                off_y: 0,
                off_z: 0,
            })
        );
        assert_eq!(object.tile.unwrap().off_x, 180);
        assert!(agent.animation_frame(8) > 0);
    }

    #[test]
    fn debug_agent_promotes_interaction_intent_after_local_route() {
        let mut agent = OriginalDebugAgent::from_spawn(
            OriginalDebugAgentSpawn {
                slot: 0,
                record_index: 0,
                tile: tile(1, 1, 0),
                sprite_ready: false,
            },
            true,
        );
        let intent = OriginalDebugInteractionIntent {
            status: OriginalDebugInteractionIntentStatus::RouteQueued,
            focus: OriginalDebugInteractionFocus::WeaponPickupCandidate,
            agent_tile: Some(tile(1, 1, 0)),
            target_tile: Some(tile(3, 1, 0)),
            route_status: OriginalRuntimeRouteStatus::CandidateRouteReady,
            route_nodes: 3,
            route_path: vec![tile(1, 1, 0), tile(2, 1, 0), tile(3, 1, 0)],
            interaction_range: 1,
            candidate_total: 1,
            message: "synthetic debug interaction queued".to_string(),
        };

        agent.assign_interaction_intent(intent);
        assert_eq!(agent.route.len(), 3);
        assert_eq!(
            agent.interaction_intent.as_ref().unwrap().status,
            OriginalDebugInteractionIntentStatus::RouteQueued
        );

        agent.update(4.0);

        assert_eq!(agent.current_tile(), tile(3, 1, 0));
        assert_eq!(
            agent.interaction_intent.as_ref().unwrap().status,
            OriginalDebugInteractionIntentStatus::ReadyAtTarget
        );
        assert!(
            agent
                .interaction_intent
                .as_ref()
                .unwrap()
                .panel_label()
                .contains("candidate-only")
        );
        assert_eq!(agent.render_label(), "marker-only sprite proof blocked");
    }

    #[test]
    fn debug_agent_interaction_after_arrival_does_not_replay_previous_start() {
        let mut agent = OriginalDebugAgent::from_spawn(
            OriginalDebugAgentSpawn {
                slot: 0,
                record_index: 0,
                tile: tile(1, 1, 0),
                sprite_ready: false,
            },
            true,
        );
        agent.assign_route(vec![tile(1, 1, 0), tile(2, 1, 0), tile(3, 1, 0)], false);
        agent.update(4.0);
        let intent = OriginalDebugInteractionIntent {
            status: OriginalDebugInteractionIntentStatus::RouteQueued,
            focus: OriginalDebugInteractionFocus::DoorOpenCandidate,
            agent_tile: Some(tile(3, 1, 0)),
            target_tile: Some(tile(5, 1, 0)),
            route_status: OriginalRuntimeRouteStatus::CandidateRouteReady,
            route_nodes: 3,
            route_path: vec![tile(3, 1, 0), tile(4, 1, 0), tile(5, 1, 0)],
            interaction_range: 1,
            candidate_total: 1,
            message: "synthetic debug interaction queued".to_string(),
        };

        agent.assign_interaction_intent(intent);

        assert_eq!(agent.current_tile(), tile(3, 1, 0));
        assert_eq!(agent.route.first().copied(), Some(tile(3, 1, 0)));
        assert!(!agent.route.contains(&tile(1, 1, 0)));
        assert_eq!(agent.route.len(), 3);
    }

    #[test]
    fn original_agent_appends_shift_queued_routes_without_resetting_progress() {
        let mut agent = OriginalDebugAgent::from_spawn(
            OriginalDebugAgentSpawn {
                slot: 0,
                record_index: 0,
                tile: tile(1, 1, 0),
                sprite_ready: true,
            },
            true,
        );

        agent.assign_route(vec![tile(1, 1, 0), tile(2, 1, 0), tile(3, 1, 0)], false);
        agent.update(0.25);
        let progress = agent.route_progress;
        agent.assign_route(vec![tile(3, 1, 0), tile(4, 2, 0), tile(5, 3, 0)], true);

        assert!(agent.route_progress >= progress);
        assert_eq!(agent.route.len(), 5);
        assert_eq!(agent.route.last().copied(), Some(tile(5, 3, 0)));
        assert_eq!(agent.route_order_start_tile(true), tile(5, 3, 0));
    }

    #[test]
    fn original_agent_resolves_ready_interaction_as_local_action_state() {
        let mut agent = OriginalDebugAgent::from_spawn(
            OriginalDebugAgentSpawn {
                slot: 2,
                record_index: 2,
                tile: tile(4, 4, 0),
                sprite_ready: true,
            },
            true,
        );
        let intent = OriginalDebugInteractionIntent {
            status: OriginalDebugInteractionIntentStatus::ReadyAtTarget,
            focus: OriginalDebugInteractionFocus::ObjectiveTargetCandidate,
            agent_tile: Some(tile(4, 4, 0)),
            target_tile: Some(tile(5, 4, 0)),
            route_status: OriginalRuntimeRouteStatus::CandidateRouteReady,
            route_nodes: 0,
            route_path: Vec::new(),
            interaction_range: 1,
            candidate_total: 1,
            message: "synthetic objective target ready".to_string(),
        };

        agent.assign_interaction_intent(intent);
        assert_eq!(
            agent.action_state.as_ref().unwrap().status,
            OriginalDebugActionStatus::Ready
        );
        assert!(agent.update(0.1).is_none());
        let resolution = agent.update(0.4).expect("action should resolve locally");

        assert_eq!(resolution.agent_slot, 2);
        assert_eq!(
            resolution.focus,
            OriginalDebugInteractionFocus::ObjectiveTargetCandidate
        );
        assert!(
            resolution
                .result_label
                .contains("mission completion remains gated")
        );
        assert_eq!(
            agent.action_state.as_ref().unwrap().status,
            OriginalDebugActionStatus::Resolved
        );
    }

    #[test]
    fn original_control_runtime_tracks_local_action_and_combat_results() {
        let mut runtime = OriginalMissionControlRuntime::default();
        runtime.apply_resolution(super::OriginalDebugActionResolution {
            agent_slot: 0,
            focus: OriginalDebugInteractionFocus::WeaponPickupCandidate,
            target_tile: Some(tile(2, 2, 0)),
            result_label: "weapon pickup candidate resolved in local control state".to_string(),
        });
        runtime.record_combat_probe(9, 7, true);

        let label = runtime.panel_label();
        assert!(label.contains("pickup 1"));
        assert!(label.contains("combat probes 1"));
        assert!(label.contains("no hit state"));
        assert!(!label.contains("0x"));
        assert!(!label.contains("00 00"));
    }

    #[test]
    fn startup_prefers_original_control_when_scene_model_is_available() {
        assert_eq!(
            initial_render_mode(true, true, true),
            MapRenderMode::OriginalMissionSceneProbe
        );
        assert_eq!(
            initial_render_mode(true, false, true),
            MapRenderMode::OriginalMapTiles
        );
        assert_eq!(
            initial_render_mode(false, false, true),
            MapRenderMode::OriginalGraphicsAtlas
        );
        assert_eq!(
            initial_render_mode(false, false, false),
            MapRenderMode::DemoCity
        );
    }

    #[test]
    fn original_agent_start_camera_offset_places_agent_on_anchor() {
        let map_tiles = synthetic_original_map_tiles(128, 128, 12);
        let agent_tile = tile(91, 41, 2);
        let anchor = vec2(800.0, 520.0);
        let zoom = 0.82;

        let offset = original_agent_focus_camera_offset_from_tile_size(
            &map_tiles, agent_tile, zoom, anchor, 64.0, 48.0,
        );
        let world =
            original_agent_focus_world_point_from_tile_size(&map_tiles, agent_tile, 64.0, 48.0);
        let screen = world * zoom + offset;

        assert!((screen.x - anchor.x).abs() < 0.001);
        assert!((screen.y - anchor.y).abs() < 0.001);
    }

    fn synthetic_original_map_tiles(width: u32, depth: u32, height: u32) -> OriginalMapTiles {
        let column_count = (width * depth) as usize;
        let offset_table_bytes = column_count * 4;
        let stack = vec![0u8; height as usize];
        let mut decoded = Vec::new();
        decoded.extend_from_slice(&width.to_le_bytes());
        decoded.extend_from_slice(&depth.to_le_bytes());
        decoded.extend_from_slice(&height.to_le_bytes());
        for _ in 0..column_count {
            decoded.extend_from_slice(&(offset_table_bytes as u32).to_le_bytes());
        }
        decoded.extend_from_slice(&stack);
        OriginalMapTiles::from_decoded_bytes("synthetic/MAP01.DAT".to_string(), &decoded).unwrap()
    }
}
