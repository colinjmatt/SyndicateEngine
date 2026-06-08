//! Runtime-local first mission scene candidates from original GAME data.
//!
//! This module follows FreeSynd's mission-section layout closely enough to
//! build typed candidate records and a conservative draw queue. It deliberately
//! keeps rendering gated until animation/frame/sprite proof is sufficient.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::Path,
};

use crate::engine::{
    map_tiles::{OriginalMapTiles, OriginalTileTypes},
    mission_source::OriginalMissionSelection,
    original_sprites::{
        OriginalObjectFrameRefs, OriginalObjectSpriteRenderAssets, OriginalRenderObjectKind,
        OriginalStaticFrameRefs,
    },
    rnc::{RncBlock, RncError},
};

const GAME_MAP_OBJECT_OFFSET: usize = 6;
const GAME_MAP_OBJECT_BYTES: usize = 128 * 128 * 2;
const PEOPLE_OFFSET: usize = 32_776;
const CARS_OFFSET: usize = 56_328;
const STATICS_OFFSET: usize = 59_016;
const WEAPONS_OFFSET: usize = 71_016;
const SFX_OFFSET: usize = 89_448;
const SCENARIOS_OFFSET: usize = 97_128;
const OBJECTIVES_OFFSET: usize = 113_974;
const SCENARIO_RECORD_BYTES: usize = 8;
const SCENARIO_RECORD_COUNT: usize = 2_048;
const OBJECTIVE_RECORD_BYTES: usize = 14;
const OBJECTIVE_RECORD_COUNT: usize = 6;
const PEOPLE_RECORD_BYTES: usize = 92;
const VEHICLE_RECORD_BYTES: usize = 42;
const STATIC_RECORD_BYTES: usize = 30;
const WEAPON_RECORD_BYTES: usize = 36;
const OBJECT_OFFSET_PEOPLE_BASE: u16 = 0x0002;
const OBJECT_OFFSET_VEHICLES_BASE: u16 = 0x5c02;
const OBJECT_OFFSET_STATICS_BASE: u16 = 0x6682;
const OBJECT_OFFSET_WEAPONS_BASE: u16 = 0x9562;
const OBJECT_OFFSET_SFX_BASE: u16 = 0xdd62;

const ON_MAP_DESC: &[u8] = &[0x04];
const STATIC_DRAW_DESCS: &[u8] = &[0x04, 0x06, 0x07];
const SPRITE_TAB_ENTRY_BYTES: usize = 6;
const ROUTE_PROBE_SEARCH_RADIUS: u16 = 8;
const ROUTE_PROBE_DEBUG_SEARCH_RADIUS: u16 = 14;
const MAX_ROUTE_PROBE_PATH_NODES: usize = 512;
const MAX_ORIGINAL_DEBUG_AGENT_SPAWNS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalMissionScene {
    pub mission_label: String,
    pub mission_id: u16,
    pub map_id: u16,
    pub palette_id: u8,
    pub section_counts: Vec<OriginalMissionSceneSection>,
    pub objects: Vec<OriginalMissionObjectCandidate>,
    pub draw_queue: OriginalMissionDrawQueue,
    pub animation_support: OriginalAnimationCatalogSupport,
    pub sprite_support: OriginalSpriteBankSupport,
    pub static_render_proof: OriginalStaticRenderProof,
    pub ped_render_proof: OriginalObjectRenderProof,
    pub weapon_render_proof: OriginalObjectRenderProof,
    pub vehicle_render_proof: OriginalObjectRenderProof,
    pub spawn_probe: OriginalSpawnProbe,
    pub navigation_probe: OriginalNavigationProbe,
    pub spatial_probe: OriginalSpatialProbe,
    pub navigation_debug_probe: OriginalNavigationDebugProbe,
    pub interaction_probe: OriginalInteractionProbe,
    pub objective_debug_probe: OriginalObjectiveDebugProbe,
    spatial_model: Option<OriginalSpatialModel>,
    objective_model: OriginalObjectiveScenarioModel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalMissionSceneSection {
    pub label: &'static str,
    pub capacity: usize,
    pub non_zero_records: usize,
    pub candidate_records: usize,
    pub queued_records: usize,
    pub supported_animation_refs: usize,
    pub unsupported_animation_refs: usize,
    pub supported_frame_refs: usize,
    pub unsupported_frame_refs: usize,
    pub draw_stage: Option<OriginalDrawStage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalMissionObjectCandidate {
    pub kind: OriginalMissionObjectKind,
    pub record_index: u16,
    pub desc: Option<u8>,
    pub state: Option<u8>,
    pub type_value: Option<u8>,
    pub subtype_value: Option<u8>,
    pub orientation: Option<u8>,
    pub tile: Option<OriginalTilePoint>,
    pub queue_tile: Option<OriginalTilePoint>,
    pub animation: OriginalAnimationRefs,
    pub candidate_record: bool,
    pub candidate_draw: bool,
    pub draw_stage: Option<OriginalDrawStage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OriginalMissionObjectKind {
    Ped,
    Vehicle,
    Static,
    Weapon,
    Sfx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OriginalDrawStage {
    People,
    Vehicles,
    Weapons,
    Statics,
    Sfx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalTilePoint {
    pub tile_x: u16,
    pub tile_y: u16,
    pub tile_z: u16,
    pub off_x: u8,
    pub off_y: u8,
    pub off_z: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OriginalAnimationRefs {
    pub base_anim: Option<u16>,
    pub current_anim: Option<u16>,
    pub current_frame: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalMissionDrawQueue {
    entries: Vec<OriginalMissionDrawQueueEntry>,
    pub stage_counts: Vec<OriginalMissionDrawStageCount>,
    pub supported_animation_entries: usize,
    pub unsupported_animation_entries: usize,
    pub supported_frame_entries: usize,
    pub unsupported_frame_entries: usize,
    pub supported_sprite_entries: usize,
    pub unsupported_sprite_entries: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalMissionDrawQueueEntry {
    pub stage: OriginalDrawStage,
    pub kind: OriginalMissionObjectKind,
    pub record_index: u16,
    pub tile: OriginalTilePoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalMissionDrawStageCount {
    pub stage: OriginalDrawStage,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalAnimationCatalogSupport {
    pub source_labels: Vec<String>,
    pub element_records: usize,
    pub frame_records: usize,
    pub animation_records: usize,
    pub invalid_element_sprite_units: usize,
    pub invalid_element_links: usize,
    pub invalid_frame_links: usize,
    pub invalid_frame_element_links: usize,
    pub invalid_animation_starts: usize,
    pub referenced_animation_entries: usize,
    pub supported_animation_entries: usize,
    pub unsupported_animation_entries: usize,
    pub referenced_frame_entries: usize,
    pub supported_frame_entries: usize,
    pub unsupported_frame_entries: usize,
    pub referenced_sprite_entries: usize,
    pub supported_sprite_entries: usize,
    pub unsupported_sprite_entries: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalSpriteBankSupport {
    pub primary_label: Option<String>,
    pub primary_entry_count: usize,
    pub primary_valid_offset_entries: usize,
    pub sibling_bank_count: usize,
    pub total_candidate_entries: usize,
    pub total_valid_offset_entries: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalStaticRenderProof {
    pub category_label: &'static str,
    pub candidate_count: usize,
    pub supported_animation_count: usize,
    pub supported_frame_count: usize,
    pub supported_sprite_count: usize,
    pub runtime_frame_assembly_count: usize,
    pub runtime_renderable_static_count: usize,
    pub decision: OriginalStaticRenderDecision,
    pub blocker: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginalStaticRenderDecision {
    RuntimeRenderDisabled,
    RuntimeRenderReady,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalObjectRenderProof {
    pub category_label: &'static str,
    pub kind: OriginalMissionObjectKind,
    pub candidate_count: usize,
    pub runtime_frame_assembly_count: usize,
    pub runtime_renderable_count: usize,
    pub decision: OriginalObjectRenderDecision,
    pub blocker: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginalObjectRenderDecision {
    RuntimeRenderDisabled,
    RuntimeRenderReady,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalSpawnProbe {
    pub ped_spawn_candidates: usize,
    pub agent_candidates: usize,
    pub enemy_candidates: usize,
    pub trigger_scenario_candidates: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalNavigationProbe {
    pub map_object_link_cells: usize,
    pub unique_object_offsets: usize,
    pub candidate_occupied_tiles: usize,
    pub static_blocking_candidates: usize,
    pub door_candidates: usize,
    pub window_candidates: usize,
    pub vehicle_footprint_candidates: usize,
    pub ped_spawn_tile_candidates: usize,
    pub scenario_records: usize,
    pub scenario_tile_target_candidates: usize,
    pub bridge_status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalSpatialProbe {
    pub surface_candidate_tiles: usize,
    pub same_level_route_nodes: usize,
    pub same_level_edges_8dir: usize,
    pub diagonal_edges: usize,
    pub diagonal_blocked_edges: usize,
    pub slope_transition_edges: usize,
    pub road_route_nodes: usize,
    pub roof_route_nodes: usize,
    pub train_route_nodes: usize,
    pub door_patch_candidate_tiles: usize,
    pub safe_walk_candidate_nodes: usize,
    pub static_blocked_tiles: usize,
    pub static_footprint_tiles: usize,
    pub vehicle_blocked_tiles: usize,
    pub vehicle_footprint_tiles: usize,
    pub ped_occupied_tiles: usize,
    pub agent_spawn_groups: usize,
    pub enemy_spawn_groups: usize,
    pub route_seed_candidates: usize,
    pub proof_status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalNavigationDebugProbe {
    pub route_nodes: usize,
    pub same_level_edges: usize,
    pub diagonal_edges: usize,
    pub diagonal_blocked_edges: usize,
    pub slope_transition_edges: usize,
    pub road_route_nodes: usize,
    pub roof_route_nodes: usize,
    pub train_route_nodes: usize,
    pub door_patch_candidate_tiles: usize,
    pub static_footprint_tiles: usize,
    pub vehicle_footprint_tiles: usize,
    pub target_snap_radius: u16,
    pub decision: OriginalNavigationDebugDecision,
    pub guardrail: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginalNavigationDebugDecision {
    RuntimeDebugDisabled,
    RuntimeDebugReady,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalInteractionProbe {
    pub door_interaction_candidates: usize,
    pub opening_door_candidates: usize,
    pub large_door_candidates: usize,
    pub weapon_pickup_candidates: usize,
    pub vehicle_entry_candidates: usize,
    pub scenario_objective_buckets: usize,
    pub scenario_active_records: usize,
    pub scenario_action_buckets: usize,
    pub scenario_trigger_buckets: usize,
    pub scenario_tile_target_buckets: usize,
    pub scenario_object_target_buckets: usize,
    pub scenario_chain_start_peds: usize,
    pub scenario_chain_link_candidates: usize,
    pub scenario_loop_candidates: usize,
    pub scenario_invalid_next_candidates: usize,
    pub game_objective_records: usize,
    pub game_objective_supported_records: usize,
    pub game_objective_unknown_records: usize,
    pub objective_ped_target_buckets: usize,
    pub objective_vehicle_target_buckets: usize,
    pub objective_weapon_target_buckets: usize,
    pub objective_location_target_buckets: usize,
    pub objective_group_target_buckets: usize,
    pub objective_unresolved_target_buckets: usize,
    pub objective_success_condition_buckets: usize,
    pub objective_failure_condition_buckets: usize,
    pub miss_active_record_candidates: usize,
    pub miss_objective_buckets: usize,
    pub guardrail: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalDebugInteractionProbe {
    pub status: OriginalDebugInteractionStatus,
    pub agent_tile: Option<OriginalTilePoint>,
    pub target_tile: Option<OriginalTilePoint>,
    pub door_candidates: usize,
    pub opening_door_candidates: usize,
    pub large_door_candidates: usize,
    pub weapon_pickup_candidates: usize,
    pub vehicle_entry_candidates: usize,
    pub objective_target_candidates: usize,
    pub scenario_target_candidates: usize,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginalDebugInteractionStatus {
    DebugDisabled,
    MissingDebugAgent,
    MissingTarget,
    NoCandidateInteraction,
    CandidateInteractionReady,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalObjectiveDebugProbe {
    pub objective_records: usize,
    pub current_candidate_index: Option<u8>,
    pub current_candidate_kind: String,
    pub target_bucket: String,
    pub scenario_link_candidates: usize,
    pub success_condition_buckets: usize,
    pub failure_condition_buckets: usize,
    pub progress_status: OriginalObjectiveProgressStatus,
    pub guardrail: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginalObjectiveProgressStatus {
    NoCandidateObjective,
    CandidateOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalDebugInteractionIntent {
    pub status: OriginalDebugInteractionIntentStatus,
    pub focus: OriginalDebugInteractionFocus,
    pub agent_tile: Option<OriginalTilePoint>,
    pub target_tile: Option<OriginalTilePoint>,
    pub route_status: OriginalRuntimeRouteStatus,
    pub route_nodes: usize,
    pub route_path: Vec<OriginalTilePoint>,
    pub interaction_range: u16,
    pub candidate_total: usize,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginalDebugInteractionIntentStatus {
    DebugDisabled,
    MissingDebugAgent,
    MissingTarget,
    NoCandidateInteraction,
    RouteBlocked,
    RouteQueued,
    ReadyAtTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginalDebugInteractionFocus {
    None,
    DoorOpenCandidate,
    LargeDoorCandidate,
    WeaponPickupCandidate,
    VehicleEntryCandidate,
    ObjectiveTargetCandidate,
    ScenarioTriggerCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalDebugAgentSpawn {
    pub slot: u8,
    pub record_index: u16,
    pub tile: OriginalTilePoint,
    pub sprite_ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalRuntimeRouteProbe {
    pub status: OriginalRuntimeRouteStatus,
    pub start_tile: Option<OriginalTilePoint>,
    pub goal_tile: Option<OriginalTilePoint>,
    pub requested_goal_tile: Option<OriginalTilePoint>,
    pub snap: Option<OriginalRouteTargetSnap>,
    pub transition_kind: OriginalRouteTransitionKind,
    pub path: Vec<OriginalTilePoint>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginalRuntimeRouteStatus {
    SpatialModelUnavailable,
    MissingStart,
    GoalOutsideCandidateGraph,
    CandidateRouteReady,
    CandidateRouteBlocked,
    HeightTransitionsUnproven,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalRouteTargetSnap {
    pub xy_distance: u16,
    pub z_delta: u16,
    pub radius: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginalRouteTransitionKind {
    None,
    SameLevelOnly,
    CandidateSlopeHeight,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginalMissionSceneError {
    NoMissionCandidate,
    Decode(String),
}

#[derive(Debug, Clone, Copy)]
struct SceneSectionSpec {
    label: &'static str,
    kind: OriginalMissionObjectKind,
    start: usize,
    record_count: usize,
    record_size: usize,
    desc_offset: Option<usize>,
    state_offset: Option<usize>,
    active_descs: &'static [u8],
    type_offset: Option<usize>,
    subtype_offset: Option<usize>,
    orientation_offset: Option<usize>,
    position_offsets: Option<(usize, usize, usize)>,
    animation_offsets: Option<(usize, usize, usize)>,
    draw_stage: OriginalDrawStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AnimElement {
    sprite: u16,
    next_element: u16,
    sprite_unit_aligned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AnimFrame {
    first_element: u16,
    next_frame: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AnimationCatalog {
    source_labels: Vec<String>,
    elements: Vec<AnimElement>,
    frames: Vec<AnimFrame>,
    animations: Vec<u16>,
    invalid_element_sprite_units: usize,
    invalid_element_links: usize,
    invalid_frame_links: usize,
    invalid_frame_element_links: usize,
    invalid_animation_starts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginalMissionScriptProbe {
    active_record_candidates: usize,
    objective_bucket_candidates: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginalObjectiveScenarioModel {
    objectives: Vec<OriginalObjectiveCandidateRecord>,
    scenarios: Vec<OriginalScenarioCandidateRecord>,
    ped_scenario_start_candidates: usize,
    scenario_chain_link_candidates: usize,
    scenario_loop_candidates: usize,
    scenario_invalid_next_candidates: usize,
    miss_active_record_candidates: usize,
    miss_objective_buckets: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OriginalObjectiveCandidateRecord {
    record_index: u8,
    kind: OriginalObjectiveCandidateKind,
    target: OriginalObjectiveTarget,
    tile: Option<OriginalTilePoint>,
    success_buckets: u8,
    failure_buckets: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OriginalObjectiveCandidateKind {
    SubOrLocation,
    Persuade,
    Assassinate,
    Protect,
    TakeWeapon,
    EliminatePolice,
    EliminateAgents,
    DestroyVehicle,
    UseVehicle,
    Evacuate,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OriginalObjectiveTarget {
    None,
    Ped(u16),
    Vehicle(u16),
    Weapon(u16),
    Group,
    Location(OriginalTilePoint),
    UnresolvedOffset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OriginalScenarioCandidateRecord {
    record_index: u16,
    kind: OriginalScenarioCandidateKind,
    next_index: Option<u16>,
    object_target: Option<OriginalObjectOffsetTarget>,
    tile: Option<OriginalTilePoint>,
    invalid_next: bool,
    self_loop: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OriginalScenarioCandidateKind {
    WalkOrDrive,
    UseVehicle,
    Unknown,
    Escape,
    Trigger,
    Reset,
    TrainWait,
    ProtectedTargetReached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OriginalObjectOffsetTarget {
    Ped(u16),
    Vehicle(u16),
    Static(u16),
    Weapon(u16),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginalSpatialModel {
    route_nodes: BTreeSet<OriginalTileKey>,
    slope_nodes: BTreeMap<OriginalTileKey, CandidateSurfaceType>,
    same_level_edges: usize,
    diagonal_edges: usize,
    diagonal_blocked_edges: usize,
    slope_transition_edges: BTreeSet<(OriginalTileKey, OriginalTileKey)>,
    door_patch_candidate_tiles: BTreeSet<OriginalTileKey>,
    static_blocked_tiles: BTreeSet<OriginalTileKey>,
    static_footprint_tiles: BTreeSet<OriginalTileKey>,
    vehicle_blocked_tiles: BTreeSet<OriginalTileKey>,
    vehicle_footprint_tiles: BTreeSet<OriginalTileKey>,
    ped_occupied_tiles: BTreeSet<OriginalTileKey>,
    agent_spawn_tiles: Vec<OriginalTileKey>,
    enemy_spawn_tiles: Vec<OriginalTileKey>,
    surface_candidate_tiles: usize,
    safe_walk_candidate_nodes: usize,
    road_route_nodes: usize,
    roof_route_nodes: usize,
    train_route_nodes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct OriginalTileKey {
    x: u16,
    y: u16,
    z: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateSurfaceType {
    Empty,
    SlopeSn,
    SlopeNs,
    SlopeEw,
    SlopeWe,
    Ground,
    Road,
    HandrailLight,
    Roof,
    RoadPedCross,
    TrainStop,
    TrainPlatform,
    NonWalkable,
    Unknown,
}

impl OriginalMissionScene {
    pub fn from_root(
        root: impl AsRef<Path>,
        selection: &OriginalMissionSelection,
    ) -> Result<Self, OriginalMissionSceneError> {
        let root = root.as_ref();
        for relative in data_file_candidates(&format!("GAME{:02}.DAT", selection.mission_id)) {
            let path = root.join(&relative);
            let Ok(data) = fs::read(&path) else {
                continue;
            };
            let decoded = decode_maybe_rnc(&data)
                .map_err(|err| OriginalMissionSceneError::Decode(format!("{err:?}")))?;
            return Ok(Self::from_decoded_game_bytes(
                selection, relative, &decoded, root,
            ));
        }

        Err(OriginalMissionSceneError::NoMissionCandidate)
    }

    pub fn from_decoded_game_bytes(
        selection: &OriginalMissionSelection,
        mission_label: String,
        decoded: &[u8],
        root: &Path,
    ) -> Self {
        let objects = collect_candidate_objects(decoded);
        let sprite_support = OriginalSpriteBankSupport::from_root(root);
        let animation_catalog = AnimationCatalog::from_root(root);
        let object_render_assets = OriginalObjectSpriteRenderAssets::from_root_with_palette_id(
            root,
            Some(selection.palette_id),
        )
        .ok();
        let mission_script_probe =
            OriginalMissionScriptProbe::from_root(root, selection.mission_id);
        let map_tiles = OriginalMapTiles::from_root_for_map_id(root, selection.map_id).ok();
        let tile_types = OriginalTileTypes::from_root(root).ok();
        Self::from_parts(
            selection,
            mission_label,
            decoded,
            objects,
            sprite_support,
            animation_catalog,
            object_render_assets.as_ref(),
            mission_script_probe.as_ref(),
            map_tiles.as_ref(),
            tile_types.as_ref(),
        )
    }

    fn from_parts(
        selection: &OriginalMissionSelection,
        mission_label: String,
        decoded: &[u8],
        objects: Vec<OriginalMissionObjectCandidate>,
        sprite_support: OriginalSpriteBankSupport,
        animation_catalog: AnimationCatalog,
        object_render_assets: Option<&OriginalObjectSpriteRenderAssets>,
        mission_script_probe: Option<&OriginalMissionScriptProbe>,
        map_tiles: Option<&OriginalMapTiles>,
        tile_types: Option<&OriginalTileTypes>,
    ) -> Self {
        let animation_support = OriginalAnimationCatalogSupport::from_catalog(
            &animation_catalog,
            &objects,
            &sprite_support,
        );
        let section_counts = build_section_counts(&objects, &animation_support);
        let draw_queue =
            OriginalMissionDrawQueue::from_objects(&objects, &animation_catalog, &sprite_support);
        let static_render_proof = OriginalStaticRenderProof::from_scene_objects(
            &objects,
            &animation_catalog,
            &sprite_support,
            object_render_assets,
        );
        let ped_render_proof = OriginalObjectRenderProof::from_scene_objects(
            "candidate peds",
            OriginalMissionObjectKind::Ped,
            &objects,
            object_render_assets,
        );
        let weapon_render_proof = OriginalObjectRenderProof::from_scene_objects(
            "candidate weapons",
            OriginalMissionObjectKind::Weapon,
            &objects,
            object_render_assets,
        );
        let vehicle_render_proof = OriginalObjectRenderProof::from_scene_objects(
            "candidate vehicles",
            OriginalMissionObjectKind::Vehicle,
            &objects,
            object_render_assets,
        );
        let spawn_probe = OriginalSpawnProbe::from_objects_and_game_bytes(&objects, decoded);
        let navigation_probe = OriginalNavigationProbe::from_decoded_game_bytes(decoded, &objects);
        let spatial_model = map_tiles.zip(tile_types).map(|(map_tiles, tile_types)| {
            OriginalSpatialModel::from_map_and_objects(map_tiles, tile_types, &objects)
        });
        let spatial_probe = OriginalSpatialProbe::from_model(spatial_model.as_ref());
        let navigation_debug_probe =
            OriginalNavigationDebugProbe::from_model(spatial_model.as_ref());
        let objective_model =
            OriginalObjectiveScenarioModel::from_game_bytes(decoded, mission_script_probe);
        let interaction_probe = OriginalInteractionProbe::from_scene(&objects, &objective_model);
        let objective_debug_probe =
            OriginalObjectiveDebugProbe::from_model(&objective_model, &objects);

        Self {
            mission_label,
            mission_id: selection.mission_id,
            map_id: selection.map_id,
            palette_id: selection.palette_id,
            section_counts,
            objects,
            draw_queue,
            animation_support,
            sprite_support,
            static_render_proof,
            ped_render_proof,
            weapon_render_proof,
            vehicle_render_proof,
            spawn_probe,
            navigation_probe,
            spatial_probe,
            navigation_debug_probe,
            interaction_probe,
            objective_debug_probe,
            spatial_model,
            objective_model,
        }
    }

    pub fn object_summary_label(&self) -> String {
        let candidate_records: usize = self
            .section_counts
            .iter()
            .map(|section| section.candidate_records)
            .sum();
        let queued_records = self.draw_queue.entries.len();
        format!(
            "scene model candidate records {candidate_records}; queued {queued_records}; runtime-only"
        )
    }

    pub fn section_counts_panel_label(&self) -> String {
        let counts = self
            .section_counts
            .iter()
            .map(|section| format!("{} {}", section.short_label(), section.candidate_records))
            .collect::<Vec<_>>()
            .join(", ");
        format!("sections: {counts}")
    }

    pub fn section_counts_report_label(&self) -> String {
        self.section_counts
            .iter()
            .map(|section| {
                format!(
                    "{} {}/{} queued {}",
                    section.short_label(),
                    section.candidate_records,
                    section.capacity,
                    section.queued_records
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn draw_queue_health_label(&self) -> String {
        format!(
            "queue health {} entries; anim {}/{}; frame {}/{}; sprite {}/{}",
            self.draw_queue.entries.len(),
            self.draw_queue.supported_animation_entries,
            self.draw_queue.supported_animation_entries
                + self.draw_queue.unsupported_animation_entries,
            self.draw_queue.supported_frame_entries,
            self.draw_queue.supported_frame_entries + self.draw_queue.unsupported_frame_entries,
            self.draw_queue.supported_sprite_entries,
            self.draw_queue.supported_sprite_entries + self.draw_queue.unsupported_sprite_entries
        )
    }

    pub fn draw_stage_panel_label(&self) -> String {
        let counts = self
            .draw_queue
            .stage_counts
            .iter()
            .map(|count| format!("{} {}", count.stage.short_label(), count.count))
            .collect::<Vec<_>>()
            .join(", ");
        if counts.is_empty() {
            "draw stages: none queued".to_string()
        } else {
            format!("draw stages: {counts}")
        }
    }

    pub fn runtime_status_label(&self) -> String {
        match self.static_render_proof.decision {
            OriginalStaticRenderDecision::RuntimeRenderReady => format!(
                "First mission scene model loaded; runtime static rendering ready ({}/{} candidates)",
                self.static_render_proof.runtime_renderable_static_count,
                self.static_render_proof.candidate_count
            ),
            OriginalStaticRenderDecision::RuntimeRenderDisabled => format!(
                "First mission scene model loaded; static render disabled ({} candidates)",
                self.static_render_proof.candidate_count
            ),
        }
    }

    pub fn report_row(&self) -> String {
        format!(
            "| mission {} | map {} palette {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            self.mission_id,
            self.map_id,
            self.palette_id,
            self.section_counts_report_label(),
            self.draw_queue_health_label(),
            self.animation_support.report_label(),
            self.sprite_support.report_label(),
            self.static_render_proof.report_label(),
            self.object_render_report_label(),
            self.spatial_probe.report_label(),
            self.navigation_debug_probe.report_label(),
            self.interaction_objective_report_label(),
            self.navigation_probe.report_label()
        )
    }

    pub fn object_render_panel_label(&self) -> String {
        format!(
            "{} | {} | {}",
            self.ped_render_proof.panel_label(),
            self.vehicle_render_proof.panel_label(),
            self.weapon_render_proof.panel_label()
        )
    }

    pub fn object_render_report_label(&self) -> String {
        format!(
            "{}; {}; {}",
            self.ped_render_proof.report_label(),
            self.vehicle_render_proof.report_label(),
            self.weapon_render_proof.report_label()
        )
    }

    pub fn interaction_objective_report_label(&self) -> String {
        format!(
            "{}; {}; debug action resolution gate ready for local control labels only; no gameplay movement, door, inventory, vehicle, combat, or mission-completion mutation",
            self.interaction_probe.report_label(),
            self.objective_debug_probe.report_label()
        )
    }

    pub fn visible_candidate_count_in_bounds(
        &self,
        min_tile: (u16, u16),
        max_tile: (u16, u16),
    ) -> usize {
        self.draw_queue
            .entries
            .iter()
            .filter(|entry| {
                entry.tile.tile_x >= min_tile.0
                    && entry.tile.tile_x <= max_tile.0
                    && entry.tile.tile_y >= min_tile.1
                    && entry.tile.tile_y <= max_tile.1
            })
            .count()
    }

    pub fn original_route_probe_to_tile(
        &self,
        goal: OriginalTilePoint,
    ) -> OriginalRuntimeRouteProbe {
        let Some(model) = &self.spatial_model else {
            return OriginalRuntimeRouteProbe::unavailable();
        };
        model.route_probe_to_tile(goal, false)
    }

    pub fn original_route_debug_probe_to_tile(
        &self,
        goal: OriginalTilePoint,
    ) -> OriginalRuntimeRouteProbe {
        let Some(model) = &self.spatial_model else {
            return OriginalRuntimeRouteProbe::unavailable();
        };
        model.route_probe_to_tile(goal, true)
    }

    pub fn original_route_debug_probe_between(
        &self,
        start: OriginalTilePoint,
        goal: OriginalTilePoint,
    ) -> OriginalRuntimeRouteProbe {
        let Some(model) = &self.spatial_model else {
            return OriginalRuntimeRouteProbe::unavailable();
        };
        model.route_probe_between(start, goal, true)
    }

    pub fn original_control_smoke_route_from(
        &self,
        start: OriginalTilePoint,
    ) -> OriginalRuntimeRouteProbe {
        let Some(model) = &self.spatial_model else {
            return OriginalRuntimeRouteProbe::unavailable();
        };
        model.smoke_route_from(start)
    }

    pub fn debug_agent_spawns(&self) -> Vec<OriginalDebugAgentSpawn> {
        let strict_spawns = self
            .objects
            .iter()
            .filter(|object| is_player_agent_spawn_candidate(object))
            .filter_map(|object| {
                let tile = object.tile?;
                Some(OriginalDebugAgentSpawn {
                    slot: object.record_index.min(3) as u8,
                    record_index: object.record_index,
                    tile: self.original_control_surface_tile(tile),
                    sprite_ready: self.ped_render_proof.decision
                        == OriginalObjectRenderDecision::RuntimeRenderReady,
                })
            })
            .take(MAX_ORIGINAL_DEBUG_AGENT_SPAWNS)
            .collect::<Vec<_>>();
        if !strict_spawns.is_empty() {
            return strict_spawns;
        }

        self.objects
            .iter()
            .filter(|object| object.kind == OriginalMissionObjectKind::Ped && object.candidate_draw)
            .filter_map(|object| Some((object.record_index, object.tile?)))
            .take(MAX_ORIGINAL_DEBUG_AGENT_SPAWNS)
            .enumerate()
            .map(|(slot, (record_index, tile))| OriginalDebugAgentSpawn {
                slot: slot as u8,
                record_index,
                tile: self.original_control_surface_tile(tile),
                sprite_ready: self.ped_render_proof.decision
                    == OriginalObjectRenderDecision::RuntimeRenderReady,
            })
            .collect()
    }

    pub fn original_control_suppressed_ped_record_indices(&self) -> Vec<u16> {
        let mut record_indices = self
            .objects
            .iter()
            .filter(|object| object.kind == OriginalMissionObjectKind::Ped && object.candidate_draw)
            .map(|object| object.record_index)
            .collect::<Vec<_>>();
        record_indices.sort_unstable();
        record_indices.dedup();
        record_indices
    }

    fn original_control_surface_tile(&self, tile: OriginalTilePoint) -> OriginalTilePoint {
        self.spatial_model
            .as_ref()
            .and_then(|model| model.original_control_surface_tile(tile))
            .unwrap_or(tile)
    }

    pub fn debug_agent_object(&self, record_index: u16) -> Option<&OriginalMissionObjectCandidate> {
        self.objects.iter().find(|object| {
            object.kind == OriginalMissionObjectKind::Ped
                && object.record_index == record_index
                && object.candidate_draw
        })
    }

    pub fn original_debug_interaction_probe_between(
        &self,
        agent_tile: Option<OriginalTilePoint>,
        target_tile: Option<OriginalTilePoint>,
        debug_enabled: bool,
    ) -> OriginalDebugInteractionProbe {
        if !debug_enabled {
            return OriginalDebugInteractionProbe::debug_disabled();
        }
        let Some(agent_tile) = agent_tile else {
            return OriginalDebugInteractionProbe::missing_debug_agent(target_tile);
        };
        let Some(target_tile) = target_tile else {
            return OriginalDebugInteractionProbe::missing_target(agent_tile);
        };
        self.objective_model
            .debug_interaction_probe(agent_tile, target_tile, &self.objects)
    }

    pub fn original_debug_interaction_intent_between(
        &self,
        agent_tile: Option<OriginalTilePoint>,
        target_tile: Option<OriginalTilePoint>,
        debug_enabled: bool,
    ) -> OriginalDebugInteractionIntent {
        if !debug_enabled {
            return OriginalDebugInteractionIntent::debug_disabled();
        }
        let Some(agent_tile) = agent_tile else {
            return OriginalDebugInteractionIntent::missing_debug_agent(target_tile);
        };
        let Some(target_tile) = target_tile else {
            return OriginalDebugInteractionIntent::missing_target(agent_tile);
        };
        let probe =
            self.objective_model
                .debug_interaction_probe(agent_tile, target_tile, &self.objects);
        let route_probe = self
            .spatial_model
            .as_ref()
            .map(|model| model.route_probe_between(agent_tile, target_tile, true))
            .unwrap_or_else(OriginalRuntimeRouteProbe::unavailable);
        OriginalDebugInteractionIntent::from_probe(agent_tile, target_tile, probe, route_probe)
    }

    pub fn first_agent_spawn_tile(&self) -> Option<OriginalTilePoint> {
        self.debug_agent_spawns()
            .first()
            .map(|spawn| spawn.tile)
            .or_else(|| {
                self.spatial_model
                    .as_ref()
                    .and_then(OriginalSpatialModel::first_agent_spawn_tile)
            })
    }
}

impl OriginalMissionSceneSection {
    fn short_label(&self) -> &'static str {
        match self.label {
            "candidate people" => "peds",
            "candidate vehicles" => "vehicles",
            "candidate statics" => "statics",
            "candidate weapons" => "weapons",
            "candidate sfx" => "sfx",
            _ => "section",
        }
    }
}

impl OriginalMissionObjectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ped => "candidate ped",
            Self::Vehicle => "candidate vehicle",
            Self::Static => "candidate static",
            Self::Weapon => "candidate weapon",
            Self::Sfx => "candidate sfx",
        }
    }

    pub fn plural_label(self) -> &'static str {
        match self {
            Self::Ped => "peds",
            Self::Vehicle => "vehicles",
            Self::Static => "statics",
            Self::Weapon => "weapons",
            Self::Sfx => "sfx",
        }
    }
}

impl OriginalTilePoint {
    fn key(self) -> OriginalTileKey {
        OriginalTileKey {
            x: self.tile_x,
            y: self.tile_y,
            z: self.tile_z,
        }
    }
}

impl OriginalDrawStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::People => "people",
            Self::Vehicles => "vehicles",
            Self::Weapons => "weapons",
            Self::Statics => "statics",
            Self::Sfx => "sfx",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::People => "peds",
            Self::Vehicles => "vehicles",
            Self::Weapons => "weapons",
            Self::Statics => "statics",
            Self::Sfx => "sfx",
        }
    }

    fn order(self) -> u8 {
        match self {
            Self::People => 0,
            Self::Vehicles => 1,
            Self::Weapons => 2,
            Self::Statics => 3,
            Self::Sfx => 4,
        }
    }
}

impl OriginalMissionObjectCandidate {
    pub fn static_frame_refs(&self) -> OriginalStaticFrameRefs {
        OriginalStaticFrameRefs {
            base_anim: self.animation.base_anim,
            current_anim: self.animation.current_anim,
            current_frame: self.animation.current_frame,
            subtype: self.subtype_value,
            orientation: self.orientation,
        }
    }

    pub fn object_frame_refs(&self, animation_frame: u16) -> OriginalObjectFrameRefs {
        OriginalObjectFrameRefs {
            kind: match self.kind {
                OriginalMissionObjectKind::Ped => OriginalRenderObjectKind::Ped,
                OriginalMissionObjectKind::Vehicle => OriginalRenderObjectKind::Vehicle,
                OriginalMissionObjectKind::Static => OriginalRenderObjectKind::Static,
                OriginalMissionObjectKind::Weapon => OriginalRenderObjectKind::Weapon,
                OriginalMissionObjectKind::Sfx => OriginalRenderObjectKind::Static,
            },
            base_anim: self.animation.base_anim,
            current_anim: self.animation.current_anim,
            current_frame: self.animation.current_frame,
            subtype: self.subtype_value,
            orientation: self.orientation,
            state: self.state,
            animation_frame,
        }
    }

    fn animation_supported_by(&self, catalog: &AnimationCatalog) -> bool {
        self.animation
            .current_anim
            .is_some_and(|anim| catalog.animation_start(anim).is_some())
    }

    fn frame_supported_by(&self, catalog: &AnimationCatalog) -> bool {
        match (self.animation.current_anim, self.animation.current_frame) {
            (_, Some(frame)) if catalog.frame_index_supported(frame) => true,
            (Some(anim), Some(frame)) => catalog.frame_for_anim(anim, frame).is_some(),
            _ => false,
        }
    }

    fn sprite_supported_by(
        &self,
        catalog: &AnimationCatalog,
        sprite_support: &OriginalSpriteBankSupport,
    ) -> bool {
        match (self.animation.current_anim, self.animation.current_frame) {
            (anim, Some(frame)) => catalog
                .sprite_refs_for_frame_index(frame)
                .or_else(|| anim.and_then(|anim| catalog.sprite_refs_for_anim_frame(anim, frame)))
                .is_some_and(|sprites| {
                    !sprites.is_empty()
                        && sprites
                            .iter()
                            .all(|sprite| sprite_support.supports_sprite_index(*sprite))
                }),
            _ => false,
        }
    }
}

impl OriginalMissionDrawQueue {
    fn from_objects(
        objects: &[OriginalMissionObjectCandidate],
        catalog: &AnimationCatalog,
        sprite_support: &OriginalSpriteBankSupport,
    ) -> Self {
        let mut entries = objects
            .iter()
            .filter(|object| object.candidate_draw)
            .filter_map(|object| {
                Some(OriginalMissionDrawQueueEntry {
                    stage: object.draw_stage?,
                    kind: object.kind,
                    record_index: object.record_index,
                    tile: object.queue_tile?,
                })
            })
            .collect::<Vec<_>>();

        entries.sort_by_key(|entry| {
            (
                entry.tile.tile_x as u32 + entry.tile.tile_y as u32 + entry.tile.tile_z as u32,
                entry.tile.tile_z,
                entry.tile.tile_y,
                entry.tile.tile_x,
                entry.tile.off_x as u16 + entry.tile.off_y as u16,
                entry.tile.off_x,
                entry.tile.off_y,
                entry.stage.order(),
                entry.record_index,
            )
        });

        let mut stage_counts_by_stage = BTreeMap::<OriginalDrawStage, usize>::new();
        for entry in &entries {
            *stage_counts_by_stage.entry(entry.stage).or_default() += 1;
        }
        let stage_counts = stage_counts_by_stage
            .into_iter()
            .map(|(stage, count)| OriginalMissionDrawStageCount { stage, count })
            .collect();

        let mut supported_animation_entries = 0;
        let mut unsupported_animation_entries = 0;
        let mut supported_frame_entries = 0;
        let mut unsupported_frame_entries = 0;
        let mut supported_sprite_entries = 0;
        let mut unsupported_sprite_entries = 0;
        for object in objects.iter().filter(|object| object.candidate_draw) {
            if object.animation_supported_by(catalog) {
                supported_animation_entries += 1;
            } else {
                unsupported_animation_entries += 1;
            }
            if object.frame_supported_by(catalog) {
                supported_frame_entries += 1;
            } else {
                unsupported_frame_entries += 1;
            }
            if object.sprite_supported_by(catalog, sprite_support) {
                supported_sprite_entries += 1;
            } else {
                unsupported_sprite_entries += 1;
            }
        }

        Self {
            entries,
            stage_counts,
            supported_animation_entries,
            unsupported_animation_entries,
            supported_frame_entries,
            unsupported_frame_entries,
            supported_sprite_entries,
            unsupported_sprite_entries,
        }
    }

    pub fn entries(&self) -> &[OriginalMissionDrawQueueEntry] {
        &self.entries
    }

    pub fn total_candidates(&self) -> usize {
        self.entries.len()
    }
}

impl OriginalAnimationCatalogSupport {
    fn from_catalog(
        catalog: &AnimationCatalog,
        objects: &[OriginalMissionObjectCandidate],
        sprite_support: &OriginalSpriteBankSupport,
    ) -> Self {
        let mut referenced_animation_entries = 0;
        let mut supported_animation_entries = 0;
        let mut referenced_frame_entries = 0;
        let mut supported_frame_entries = 0;
        let mut referenced_sprite_entries = 0;
        let mut supported_sprite_entries = 0;

        for object in objects.iter().filter(|object| object.candidate_draw) {
            if object.animation.current_anim.is_some() {
                referenced_animation_entries += 1;
                if object.animation_supported_by(catalog) {
                    supported_animation_entries += 1;
                }
            }
            if let (Some(anim), Some(frame)) = (
                object.animation.current_anim,
                object.animation.current_frame,
            ) {
                referenced_frame_entries += 1;
                if object.frame_supported_by(catalog) {
                    supported_frame_entries += 1;
                }
                if let Some(sprites) = catalog
                    .sprite_refs_for_frame_index(frame)
                    .or_else(|| catalog.sprite_refs_for_anim_frame(anim, frame))
                {
                    referenced_sprite_entries += sprites.len();
                    supported_sprite_entries += sprites
                        .iter()
                        .filter(|sprite| sprite_support.supports_sprite_index(**sprite))
                        .count();
                }
            }
        }

        Self {
            source_labels: catalog.source_labels.clone(),
            element_records: catalog.elements.len(),
            frame_records: catalog.frames.len(),
            animation_records: catalog.animations.len(),
            invalid_element_sprite_units: catalog.invalid_element_sprite_units,
            invalid_element_links: catalog.invalid_element_links,
            invalid_frame_links: catalog.invalid_frame_links,
            invalid_frame_element_links: catalog.invalid_frame_element_links,
            invalid_animation_starts: catalog.invalid_animation_starts,
            referenced_animation_entries,
            supported_animation_entries,
            unsupported_animation_entries: referenced_animation_entries
                .saturating_sub(supported_animation_entries),
            referenced_frame_entries,
            supported_frame_entries,
            unsupported_frame_entries: referenced_frame_entries
                .saturating_sub(supported_frame_entries),
            referenced_sprite_entries,
            supported_sprite_entries,
            unsupported_sprite_entries: referenced_sprite_entries
                .saturating_sub(supported_sprite_entries),
        }
    }

    pub fn panel_label(&self) -> String {
        format!(
            "anim catalog {} anims, {} frames; refs {}/{}",
            self.animation_records,
            self.frame_records,
            self.supported_animation_entries,
            self.referenced_animation_entries
        )
    }

    pub fn report_label(&self) -> String {
        format!(
            "{}; frame refs {}/{}; sprite refs {}/{}; invalid links e{} f{} a{}",
            self.panel_label(),
            self.supported_frame_entries,
            self.referenced_frame_entries,
            self.supported_sprite_entries,
            self.referenced_sprite_entries,
            self.invalid_element_links,
            self.invalid_frame_links,
            self.invalid_animation_starts
        )
    }
}

impl OriginalSpriteBankSupport {
    fn from_root(root: &Path) -> Self {
        let mut primary = None;
        let mut sibling_bank_count = 0;
        let mut total_candidate_entries = 0;
        let mut total_valid_offset_entries = 0;

        for stem in ["HSPR-0", "HSPR-1", "HSPR-0-D", "HSPR-1-D"] {
            for prefix in ["SYNDICAT/DATA", "DATADISK/DATA"] {
                let tab_label = format!("{prefix}/{stem}.TAB");
                let dat_label = format!("{prefix}/{stem}.DAT");
                let tab_path = root.join(&tab_label);
                let dat_path = root.join(&dat_label);
                let (Some(tab), Some(dat)) = (
                    read_original_asset_bytes(&tab_path),
                    read_original_asset_bytes(&dat_path),
                ) else {
                    continue;
                };
                let summary = summarize_sprite_tab_bank(&tab_label, &tab, dat.len());
                if stem == "HSPR-0" && primary.is_none() {
                    primary = Some(summary.clone());
                } else {
                    sibling_bank_count += 1;
                }
                total_candidate_entries += summary.entry_count;
                total_valid_offset_entries += summary.valid_offset_entries;
            }
        }

        let primary_label = primary.as_ref().map(|summary| summary.label.clone());
        let primary_entry_count = primary.as_ref().map_or(0, |summary| summary.entry_count);
        let primary_valid_offset_entries = primary
            .as_ref()
            .map_or(0, |summary| summary.valid_offset_entries);

        Self {
            primary_label,
            primary_entry_count,
            primary_valid_offset_entries,
            sibling_bank_count,
            total_candidate_entries,
            total_valid_offset_entries,
        }
    }

    #[cfg(test)]
    fn from_primary_counts(entry_count: usize, valid_offset_entries: usize) -> Self {
        Self {
            primary_label: Some("synthetic HSPR-0".to_string()),
            primary_entry_count: entry_count,
            primary_valid_offset_entries: valid_offset_entries,
            sibling_bank_count: 0,
            total_candidate_entries: entry_count,
            total_valid_offset_entries: valid_offset_entries,
        }
    }

    fn supports_sprite_index(&self, sprite_index: u16) -> bool {
        (sprite_index as usize) < self.primary_entry_count
            && (sprite_index as usize) < self.primary_valid_offset_entries
    }

    pub fn panel_label(&self) -> String {
        if self.primary_entry_count == 0 {
            return "sprite bank support unavailable".to_string();
        }
        format!(
            "HSPR candidate entries {}/{} valid; siblings {}",
            self.primary_valid_offset_entries, self.primary_entry_count, self.sibling_bank_count
        )
    }

    pub fn report_label(&self) -> String {
        format!(
            "{}; runtime-only sprite catalog, no dimensions or previews",
            self.panel_label()
        )
    }
}

impl OriginalStaticRenderProof {
    fn from_scene_objects(
        objects: &[OriginalMissionObjectCandidate],
        catalog: &AnimationCatalog,
        sprite_support: &OriginalSpriteBankSupport,
        object_render_assets: Option<&OriginalObjectSpriteRenderAssets>,
    ) -> Self {
        let statics = objects
            .iter()
            .filter(|object| object.kind == OriginalMissionObjectKind::Static)
            .filter(|object| object.candidate_draw)
            .collect::<Vec<_>>();
        let candidate_count = statics.len();
        let supported_animation_count = statics
            .iter()
            .filter(|object| object.animation_supported_by(catalog))
            .count();
        let supported_frame_count = statics
            .iter()
            .filter(|object| object.frame_supported_by(catalog))
            .count();
        let supported_sprite_count = statics
            .iter()
            .filter(|object| object.sprite_supported_by(catalog, sprite_support))
            .count();
        let mut runtime_frame_assembly_count = 0;
        let mut runtime_renderable_static_count = 0;
        if let Some(assets) = object_render_assets {
            for object in &statics {
                let support = assets.static_frame_support(object.static_frame_refs());
                if support.assembled {
                    runtime_frame_assembly_count += 1;
                }
                if support.assembled && support.sprites_supported {
                    runtime_renderable_static_count += 1;
                }
            }
        }

        let decision = if runtime_renderable_static_count > 0 {
            OriginalStaticRenderDecision::RuntimeRenderReady
        } else {
            OriginalStaticRenderDecision::RuntimeRenderDisabled
        };
        let blocker = if candidate_count == 0 {
            "no candidate static records queued for runtime proof".to_string()
        } else if object_render_assets.is_none() {
            "runtime HSPR/ANI assets unavailable or failed strict bounds checks".to_string()
        } else if runtime_frame_assembly_count == 0 {
            "no candidate static frame could be assembled from guarded HELE/HFRA/HSTA chains"
                .to_string()
        } else if runtime_renderable_static_count == 0 {
            "assembled static frames reference unsupported HSPR sprites".to_string()
        } else if runtime_renderable_static_count < candidate_count {
            "partial runtime static render proof; unsupported or semantic statics remain candidate-only"
                .to_string()
        } else {
            "runtime static render proof ready for all queued static candidates".to_string()
        };

        let blocker = if decision == OriginalStaticRenderDecision::RuntimeRenderDisabled {
            blocker
        } else if supported_animation_count < candidate_count {
            "partial runtime static render proof; aggregate animation catalog refs remain incomplete"
                .to_string()
        } else if supported_frame_count < candidate_count {
            "partial runtime static render proof; aggregate frame refs remain incomplete"
                .to_string()
        } else if supported_sprite_count < candidate_count {
            "partial runtime static render proof; aggregate sprite-bank refs remain incomplete"
                .to_string()
        } else {
            blocker
        };

        Self {
            category_label: "candidate statics",
            candidate_count,
            supported_animation_count,
            supported_frame_count,
            supported_sprite_count,
            runtime_frame_assembly_count,
            runtime_renderable_static_count,
            decision,
            blocker,
        }
    }

    pub fn panel_label(&self) -> String {
        match self.decision {
            OriginalStaticRenderDecision::RuntimeRenderReady => format!(
                "static render ready: {}/{} candidates; frame proof {}; support {}/{}/{}",
                self.runtime_renderable_static_count,
                self.candidate_count,
                self.runtime_frame_assembly_count,
                self.supported_animation_count,
                self.supported_frame_count,
                self.supported_sprite_count
            ),
            OriginalStaticRenderDecision::RuntimeRenderDisabled => format!(
                "static render disabled: {}; {} candidates; frame proof {}; support {}/{}/{}",
                self.short_blocker_label(),
                self.candidate_count,
                self.runtime_frame_assembly_count,
                self.supported_animation_count,
                self.supported_frame_count,
                self.supported_sprite_count
            ),
        }
    }

    pub fn report_label(&self) -> String {
        match self.decision {
            OriginalStaticRenderDecision::RuntimeRenderReady => format!(
                "static render ready: {}/{} candidate statics; frame assembly {}/{}; {}; runtime-only, no previews, not proof of gameplay semantics",
                self.runtime_renderable_static_count,
                self.candidate_count,
                self.runtime_frame_assembly_count,
                self.candidate_count,
                self.blocker
            ),
            OriginalStaticRenderDecision::RuntimeRenderDisabled => format!(
                "static render disabled: {}; frame assembly {}/{}; runtime-only, not proof of decoded layout or semantics",
                self.blocker, self.runtime_frame_assembly_count, self.candidate_count
            ),
        }
    }

    fn short_blocker_label(&self) -> &'static str {
        if self.blocker.contains("HSPR/ANI") {
            "runtime sprite assets missing"
        } else if self.blocker.contains("assembled static frames") {
            "sprite refs incomplete"
        } else if self.blocker.contains("frame could be assembled") {
            "frame assembly missing"
        } else if self.blocker.contains("animation") {
            "animation refs incomplete"
        } else if self.blocker.contains("frame") {
            "frame refs incomplete"
        } else if self.blocker.contains("sprite-bank") {
            "sprite-bank refs incomplete"
        } else {
            "proof missing"
        }
    }
}

impl OriginalObjectRenderProof {
    fn from_scene_objects(
        category_label: &'static str,
        kind: OriginalMissionObjectKind,
        objects: &[OriginalMissionObjectCandidate],
        object_render_assets: Option<&OriginalObjectSpriteRenderAssets>,
    ) -> Self {
        let candidates = objects
            .iter()
            .filter(|object| object.kind == kind)
            .filter(|object| object.candidate_draw)
            .collect::<Vec<_>>();
        let candidate_count = candidates.len();
        let mut runtime_frame_assembly_count = 0;
        let mut runtime_renderable_count = 0;

        if let Some(assets) = object_render_assets {
            for object in &candidates {
                let support = assets.object_frame_support(object.object_frame_refs(0));
                if support.assembled {
                    runtime_frame_assembly_count += 1;
                }
                if support.assembled && support.sprites_supported {
                    runtime_renderable_count += 1;
                }
            }
        }

        let decision = if runtime_renderable_count > 0 {
            OriginalObjectRenderDecision::RuntimeRenderReady
        } else {
            OriginalObjectRenderDecision::RuntimeRenderDisabled
        };
        let blocker = if candidate_count == 0 {
            format!(
                "no queued {} records for runtime proof",
                kind.plural_label()
            )
        } else if object_render_assets.is_none() {
            "runtime HSPR/ANI assets unavailable or failed strict bounds checks".to_string()
        } else if runtime_frame_assembly_count == 0 {
            format!(
                "no candidate {} frame could be assembled from guarded animation chains",
                kind.label()
            )
        } else if runtime_renderable_count == 0 {
            format!(
                "assembled {} frames reference unsupported HSPR sprites",
                kind.plural_label()
            )
        } else if runtime_renderable_count < candidate_count {
            format!(
                "partial runtime {} render proof; unsupported candidates remain candidate-only",
                kind.plural_label()
            )
        } else {
            format!(
                "runtime {} render proof ready for all queued candidates",
                kind.plural_label()
            )
        };

        Self {
            category_label,
            kind,
            candidate_count,
            runtime_frame_assembly_count,
            runtime_renderable_count,
            decision,
            blocker,
        }
    }

    pub fn panel_label(&self) -> String {
        match self.decision {
            OriginalObjectRenderDecision::RuntimeRenderReady => format!(
                "{} ready {}/{}; frame proof {}",
                self.kind.plural_label(),
                self.runtime_renderable_count,
                self.candidate_count,
                self.runtime_frame_assembly_count
            ),
            OriginalObjectRenderDecision::RuntimeRenderDisabled => format!(
                "{} blocked {}; candidates {}; frame proof {}",
                self.kind.plural_label(),
                self.short_blocker_label(),
                self.candidate_count,
                self.runtime_frame_assembly_count
            ),
        }
    }

    pub fn report_label(&self) -> String {
        match self.decision {
            OriginalObjectRenderDecision::RuntimeRenderReady => format!(
                "{} render ready: {}/{} candidates; frame assembly {}/{}; {}; runtime-only, no previews, not proof of gameplay semantics",
                self.kind.plural_label(),
                self.runtime_renderable_count,
                self.candidate_count,
                self.runtime_frame_assembly_count,
                self.candidate_count,
                self.blocker
            ),
            OriginalObjectRenderDecision::RuntimeRenderDisabled => format!(
                "{} render disabled: {}; frame assembly {}/{}; runtime-only, not proof of decoded layout or semantics",
                self.kind.plural_label(),
                self.blocker,
                self.runtime_frame_assembly_count,
                self.candidate_count
            ),
        }
    }

    fn short_blocker_label(&self) -> &'static str {
        if self.candidate_count == 0 {
            "none queued"
        } else if self.blocker.contains("HSPR/ANI") {
            "runtime assets missing"
        } else if self.blocker.contains("unsupported HSPR") {
            "sprite refs incomplete"
        } else if self.blocker.contains("frame could be assembled") {
            "frame assembly missing"
        } else if self.blocker.contains("partial") {
            "partial proof"
        } else {
            "proof missing"
        }
    }
}

impl OriginalSpawnProbe {
    fn from_objects_and_game_bytes(
        objects: &[OriginalMissionObjectCandidate],
        decoded: &[u8],
    ) -> Self {
        let ped_spawn_candidates = objects
            .iter()
            .filter(|object| object.kind == OriginalMissionObjectKind::Ped && object.candidate_draw)
            .count();
        let agent_candidates = objects
            .iter()
            .filter(|object| is_player_agent_spawn_candidate(object))
            .count();
        let enemy_candidates = objects
            .iter()
            .filter(|object| is_enemy_ped_spawn_candidate(object))
            .count();
        let trigger_scenario_candidates = scenario_records(decoded)
            .filter(|record| record.get(7).copied() == Some(0x08))
            .count();

        Self {
            ped_spawn_candidates,
            agent_candidates,
            enemy_candidates,
            trigger_scenario_candidates,
        }
    }

    pub fn panel_label(&self) -> String {
        format!(
            "spawn candidates peds {}, agents {}, enemies {}, triggers {}",
            self.ped_spawn_candidates,
            self.agent_candidates,
            self.enemy_candidates,
            self.trigger_scenario_candidates
        )
    }
}

impl OriginalNavigationProbe {
    fn from_decoded_game_bytes(decoded: &[u8], objects: &[OriginalMissionObjectCandidate]) -> Self {
        let map_object_offsets = decoded
            .get(GAME_MAP_OBJECT_OFFSET..GAME_MAP_OBJECT_OFFSET + GAME_MAP_OBJECT_BYTES)
            .unwrap_or(&[]);
        let mut map_object_link_cells = 0;
        let mut unique_offsets = BTreeSet::new();
        for chunk in map_object_offsets.chunks_exact(2) {
            let offset = u16::from_le_bytes([chunk[0], chunk[1]]);
            if offset != 0 {
                map_object_link_cells += 1;
                unique_offsets.insert(offset);
            }
        }

        let mut occupied_tiles = BTreeSet::new();
        let mut static_blocking_candidates = 0;
        let mut door_candidates = 0;
        let mut window_candidates = 0;
        let mut vehicle_footprint_candidates = 0;
        let mut ped_spawn_tile_candidates = 0;
        for object in objects.iter().filter(|object| object.candidate_draw) {
            if let Some(tile) = object.tile {
                occupied_tiles.insert((tile.tile_x, tile.tile_y, tile.tile_z));
            }

            match object.kind {
                OriginalMissionObjectKind::Static => {
                    if is_door_static_candidate(object) {
                        door_candidates += 1;
                    } else if is_window_static_candidate(object) {
                        window_candidates += 1;
                    } else {
                        static_blocking_candidates += 1;
                    }
                }
                OriginalMissionObjectKind::Vehicle => {
                    vehicle_footprint_candidates += 1;
                }
                OriginalMissionObjectKind::Ped => {
                    ped_spawn_tile_candidates += 1;
                }
                OriginalMissionObjectKind::Weapon | OriginalMissionObjectKind::Sfx => {}
            }
        }

        let mut active_scenario_records = 0;
        let mut scenario_tile_target_candidates = 0;
        for record in scenario_records(decoded) {
            let scenario_type = record.get(7).copied().unwrap_or_default();
            if scenario_type == 0 {
                continue;
            }
            active_scenario_records += 1;
            if matches!(scenario_type, 0x01 | 0x02 | 0x08) {
                scenario_tile_target_candidates += 1;
            }
        }

        Self {
            map_object_link_cells,
            unique_object_offsets: unique_offsets.len(),
            candidate_occupied_tiles: occupied_tiles.len(),
            static_blocking_candidates,
            door_candidates,
            window_candidates,
            vehicle_footprint_candidates,
            ped_spawn_tile_candidates,
            scenario_records: active_scenario_records,
            scenario_tile_target_candidates,
            bridge_status: "navigation bridge candidate only; gameplay/pathfinding remains on demo grid",
        }
    }

    pub fn panel_label(&self) -> String {
        format!(
            "nav inputs links {}, occupied {}, blockers {}, doors {}, windows {}, vehicles {}; demo grid active",
            self.map_object_link_cells,
            self.candidate_occupied_tiles,
            self.static_blocking_candidates,
            self.door_candidates,
            self.window_candidates,
            self.vehicle_footprint_candidates
        )
    }

    pub fn report_label(&self) -> String {
        format!(
            "{}; ped spawn tiles {}; scenario targets {}; unique links {}; {}",
            self.panel_label(),
            self.ped_spawn_tile_candidates,
            self.scenario_tile_target_candidates,
            self.unique_object_offsets,
            self.bridge_status
        )
    }
}

impl OriginalSpatialProbe {
    fn from_model(model: Option<&OriginalSpatialModel>) -> Self {
        let Some(model) = model else {
            return Self {
                surface_candidate_tiles: 0,
                same_level_route_nodes: 0,
                same_level_edges_8dir: 0,
                diagonal_edges: 0,
                diagonal_blocked_edges: 0,
                slope_transition_edges: 0,
                road_route_nodes: 0,
                roof_route_nodes: 0,
                train_route_nodes: 0,
                door_patch_candidate_tiles: 0,
                safe_walk_candidate_nodes: 0,
                static_blocked_tiles: 0,
                static_footprint_tiles: 0,
                vehicle_blocked_tiles: 0,
                vehicle_footprint_tiles: 0,
                ped_occupied_tiles: 0,
                agent_spawn_groups: 0,
                enemy_spawn_groups: 0,
                route_seed_candidates: 0,
                proof_status: "spatial model unavailable; original navigation disabled",
            };
        };

        let route_seed_candidates = if model.agent_spawn_tiles.is_empty() {
            model.ped_occupied_tiles.len()
        } else {
            model.agent_spawn_tiles.len()
        }
        .min(model.route_nodes.len());

        Self {
            surface_candidate_tiles: model.surface_candidate_tiles,
            same_level_route_nodes: model.route_nodes.len(),
            same_level_edges_8dir: model.same_level_edges,
            diagonal_edges: model.diagonal_edges,
            diagonal_blocked_edges: model.diagonal_blocked_edges,
            slope_transition_edges: model.slope_transition_edges.len(),
            road_route_nodes: model.road_route_nodes,
            roof_route_nodes: model.roof_route_nodes,
            train_route_nodes: model.train_route_nodes,
            door_patch_candidate_tiles: model.door_patch_candidate_tiles.len(),
            safe_walk_candidate_nodes: model.safe_walk_candidate_nodes,
            static_blocked_tiles: model.static_blocked_tiles.len(),
            static_footprint_tiles: model.static_footprint_tiles.len(),
            vehicle_blocked_tiles: model.vehicle_blocked_tiles.len(),
            vehicle_footprint_tiles: model.vehicle_footprint_tiles.len(),
            ped_occupied_tiles: model.ped_occupied_tiles.len(),
            agent_spawn_groups: model.agent_spawn_tiles.len(),
            enemy_spawn_groups: model.enemy_spawn_tiles.len(),
            route_seed_candidates,
            proof_status: "candidate route graph with gated slope/door/footprint diagnostics; gameplay semantics remain unproven",
        }
    }

    pub fn panel_label(&self) -> String {
        format!(
            "spatial graph surfaces {}, route nodes {}, edges8 {}, diag {}/{}, slope {}, blockers s{} v{}, ped occ {}, agent seeds {}; gated",
            self.surface_candidate_tiles,
            self.same_level_route_nodes,
            self.same_level_edges_8dir,
            self.diagonal_edges,
            self.diagonal_blocked_edges,
            self.slope_transition_edges,
            self.static_blocked_tiles,
            self.vehicle_blocked_tiles,
            self.ped_occupied_tiles,
            self.agent_spawn_groups
        )
    }

    pub fn report_label(&self) -> String {
        format!(
            "{}; safe-walk candidates {}; road {} roof {} train {}; door patch tiles {}; footprints static {} vehicle {}; enemy groups {}; route seeds {}; {}; runtime-only, aggregate only, not proof of navigation semantics",
            self.panel_label(),
            self.safe_walk_candidate_nodes,
            self.road_route_nodes,
            self.roof_route_nodes,
            self.train_route_nodes,
            self.door_patch_candidate_tiles,
            self.static_footprint_tiles,
            self.vehicle_footprint_tiles,
            self.enemy_spawn_groups,
            self.route_seed_candidates,
            self.proof_status
        )
    }
}

impl OriginalNavigationDebugProbe {
    fn from_model(model: Option<&OriginalSpatialModel>) -> Self {
        let Some(model) = model else {
            return Self {
                route_nodes: 0,
                same_level_edges: 0,
                diagonal_edges: 0,
                diagonal_blocked_edges: 0,
                slope_transition_edges: 0,
                road_route_nodes: 0,
                roof_route_nodes: 0,
                train_route_nodes: 0,
                door_patch_candidate_tiles: 0,
                static_footprint_tiles: 0,
                vehicle_footprint_tiles: 0,
                target_snap_radius: 0,
                decision: OriginalNavigationDebugDecision::RuntimeDebugDisabled,
                guardrail: "debug navigation disabled: missing guarded spatial model; demo gameplay remains active",
            };
        };

        Self {
            route_nodes: model.route_nodes.len(),
            same_level_edges: model.same_level_edges,
            diagonal_edges: model.diagonal_edges,
            diagonal_blocked_edges: model.diagonal_blocked_edges,
            slope_transition_edges: model.slope_transition_edges.len(),
            road_route_nodes: model.road_route_nodes,
            roof_route_nodes: model.roof_route_nodes,
            train_route_nodes: model.train_route_nodes,
            door_patch_candidate_tiles: model.door_patch_candidate_tiles.len(),
            static_footprint_tiles: model.static_footprint_tiles.len(),
            vehicle_footprint_tiles: model.vehicle_footprint_tiles.len(),
            target_snap_radius: ROUTE_PROBE_DEBUG_SEARCH_RADIUS,
            decision: OriginalNavigationDebugDecision::RuntimeDebugReady,
            guardrail: "debug-only original navigation candidate; demo gameplay/pathfinding remains active",
        }
    }

    pub fn panel_label(&self) -> String {
        match self.decision {
            OriginalNavigationDebugDecision::RuntimeDebugDisabled => self.guardrail.to_string(),
            OriginalNavigationDebugDecision::RuntimeDebugReady => format!(
                "original control route graph ready: nodes {}, edges8 {} diag {}/{}, slope {}, snap {}; gated/local",
                self.route_nodes,
                self.same_level_edges,
                self.diagonal_edges,
                self.diagonal_blocked_edges,
                self.slope_transition_edges,
                self.target_snap_radius
            ),
        }
    }

    pub fn report_label(&self) -> String {
        format!(
            "{}; surfaces road {} roof {} train {}; door patch tiles {}; footprints static {} vehicle {}; {}; runtime-only aggregate, not proof of playable navigation",
            self.panel_label(),
            self.road_route_nodes,
            self.roof_route_nodes,
            self.train_route_nodes,
            self.door_patch_candidate_tiles,
            self.static_footprint_tiles,
            self.vehicle_footprint_tiles,
            self.guardrail
        )
    }
}

impl OriginalInteractionProbe {
    fn from_scene(
        objects: &[OriginalMissionObjectCandidate],
        model: &OriginalObjectiveScenarioModel,
    ) -> Self {
        let door_interaction_candidates = objects
            .iter()
            .filter(|object| object.candidate_draw && is_door_static_candidate(object))
            .count();
        let opening_door_candidates = objects
            .iter()
            .filter(|object| {
                object.candidate_draw
                    && object.kind == OriginalMissionObjectKind::Static
                    && matches!(object.subtype_value, Some(0x0e | 0x0f))
            })
            .count();
        let large_door_candidates = objects
            .iter()
            .filter(|object| object.candidate_draw && is_large_door_static_candidate(object))
            .count();
        let weapon_pickup_candidates = objects
            .iter()
            .filter(|object| {
                object.candidate_draw && object.kind == OriginalMissionObjectKind::Weapon
            })
            .count();
        let vehicle_entry_candidates = objects
            .iter()
            .filter(|object| {
                object.candidate_draw && object.kind == OriginalMissionObjectKind::Vehicle
            })
            .count();
        let scenario_objective_buckets = model
            .scenarios
            .iter()
            .filter(|record| {
                matches!(
                    record.kind,
                    OriginalScenarioCandidateKind::WalkOrDrive
                        | OriginalScenarioCandidateKind::UseVehicle
                        | OriginalScenarioCandidateKind::Trigger
                )
            })
            .count();
        let scenario_active_records = model.scenarios.len();
        let scenario_action_buckets = model
            .scenarios
            .iter()
            .filter(|record| record.kind.is_action_candidate())
            .count();
        let scenario_trigger_buckets = model
            .scenarios
            .iter()
            .filter(|record| record.kind == OriginalScenarioCandidateKind::Trigger)
            .count();
        let scenario_tile_target_buckets = model
            .scenarios
            .iter()
            .filter(|record| record.tile.is_some())
            .count();
        let scenario_object_target_buckets = model
            .scenarios
            .iter()
            .filter(|record| record.object_target.is_some())
            .count();
        let game_objective_records = model.objectives.len();
        let game_objective_supported_records = model
            .objectives
            .iter()
            .filter(|record| record.kind != OriginalObjectiveCandidateKind::Unknown)
            .count();
        let game_objective_unknown_records =
            game_objective_records.saturating_sub(game_objective_supported_records);
        let objective_ped_target_buckets = model
            .objectives
            .iter()
            .filter(|record| matches!(record.target, OriginalObjectiveTarget::Ped(_)))
            .count();
        let objective_vehicle_target_buckets = model
            .objectives
            .iter()
            .filter(|record| matches!(record.target, OriginalObjectiveTarget::Vehicle(_)))
            .count();
        let objective_weapon_target_buckets = model
            .objectives
            .iter()
            .filter(|record| matches!(record.target, OriginalObjectiveTarget::Weapon(_)))
            .count();
        let objective_location_target_buckets = model
            .objectives
            .iter()
            .filter(|record| matches!(record.target, OriginalObjectiveTarget::Location(_)))
            .count();
        let objective_group_target_buckets = model
            .objectives
            .iter()
            .filter(|record| matches!(record.target, OriginalObjectiveTarget::Group))
            .count();
        let objective_unresolved_target_buckets = model
            .objectives
            .iter()
            .filter(|record| matches!(record.target, OriginalObjectiveTarget::UnresolvedOffset))
            .count();
        let objective_success_condition_buckets = model
            .objectives
            .iter()
            .map(|record| record.success_buckets as usize)
            .sum();
        let objective_failure_condition_buckets = model
            .objectives
            .iter()
            .map(|record| record.failure_buckets as usize)
            .sum();

        Self {
            door_interaction_candidates,
            opening_door_candidates,
            large_door_candidates,
            weapon_pickup_candidates,
            vehicle_entry_candidates,
            scenario_objective_buckets,
            scenario_active_records,
            scenario_action_buckets,
            scenario_trigger_buckets,
            scenario_tile_target_buckets,
            scenario_object_target_buckets,
            scenario_chain_start_peds: model.ped_scenario_start_candidates,
            scenario_chain_link_candidates: model.scenario_chain_link_candidates,
            scenario_loop_candidates: model.scenario_loop_candidates,
            scenario_invalid_next_candidates: model.scenario_invalid_next_candidates,
            game_objective_records,
            game_objective_supported_records,
            game_objective_unknown_records,
            objective_ped_target_buckets,
            objective_vehicle_target_buckets,
            objective_weapon_target_buckets,
            objective_location_target_buckets,
            objective_group_target_buckets,
            objective_unresolved_target_buckets,
            objective_success_condition_buckets,
            objective_failure_condition_buckets,
            miss_active_record_candidates: model.miss_active_record_candidates,
            miss_objective_buckets: model.miss_objective_buckets,
            guardrail: "interaction buckets are candidate-only; no objective, pickup, door, AI, or vehicle semantics are active",
        }
    }

    pub fn panel_label(&self) -> String {
        format!(
            "interactions doors {} opening {} large {}; pickups {} vehicles {}; objectives GAME {}/{} unknown {}; scenarios {} trigger {} chains {}; MISS {}/{}; candidate-only",
            self.door_interaction_candidates,
            self.opening_door_candidates,
            self.large_door_candidates,
            self.weapon_pickup_candidates,
            self.vehicle_entry_candidates,
            self.game_objective_supported_records,
            self.game_objective_records,
            self.game_objective_unknown_records,
            self.scenario_active_records,
            self.scenario_trigger_buckets,
            self.scenario_chain_start_peds,
            self.miss_active_record_candidates,
            self.miss_objective_buckets
        )
    }

    pub fn report_label(&self) -> String {
        format!(
            "{}; objective targets ped {} vehicle {} weapon {} location {} group {} unresolved {}; success/failure buckets {}/{}; scenario actions {} tile targets {} object targets {} loops {} invalid-next {}; GAME scenario objective buckets {}; {}; runtime-only aggregate, not proof of interaction or mission semantics",
            self.panel_label(),
            self.objective_ped_target_buckets,
            self.objective_vehicle_target_buckets,
            self.objective_weapon_target_buckets,
            self.objective_location_target_buckets,
            self.objective_group_target_buckets,
            self.objective_unresolved_target_buckets,
            self.objective_success_condition_buckets,
            self.objective_failure_condition_buckets,
            self.scenario_action_buckets,
            self.scenario_tile_target_buckets,
            self.scenario_object_target_buckets,
            self.scenario_loop_candidates,
            self.scenario_invalid_next_candidates,
            self.scenario_objective_buckets,
            self.guardrail
        )
    }
}

impl OriginalDebugInteractionProbe {
    fn debug_disabled() -> Self {
        Self {
            status: OriginalDebugInteractionStatus::DebugDisabled,
            agent_tile: None,
            target_tile: None,
            door_candidates: 0,
            opening_door_candidates: 0,
            large_door_candidates: 0,
            weapon_pickup_candidates: 0,
            vehicle_entry_candidates: 0,
            objective_target_candidates: 0,
            scenario_target_candidates: 0,
            message: "interaction/action gate disabled by G; original control is local-only and demo gameplay remains active"
                .to_string(),
        }
    }

    fn missing_debug_agent(target_tile: Option<OriginalTilePoint>) -> Self {
        Self {
            status: OriginalDebugInteractionStatus::MissingDebugAgent,
            agent_tile: None,
            target_tile,
            door_candidates: 0,
            opening_door_candidates: 0,
            large_door_candidates: 0,
            weapon_pickup_candidates: 0,
            vehicle_entry_candidates: 0,
            objective_target_candidates: 0,
            scenario_target_candidates: 0,
            message: "interaction/action blocked: no selected original-agent marker".to_string(),
        }
    }

    fn missing_target(agent_tile: OriginalTilePoint) -> Self {
        Self {
            status: OriginalDebugInteractionStatus::MissingTarget,
            agent_tile: Some(agent_tile),
            target_tile: None,
            door_candidates: 0,
            opening_door_candidates: 0,
            large_door_candidates: 0,
            weapon_pickup_candidates: 0,
            vehicle_entry_candidates: 0,
            objective_target_candidates: 0,
            scenario_target_candidates: 0,
            message: "interaction/action blocked: cursor is outside the candidate map".to_string(),
        }
    }

    fn from_counts(
        agent_tile: OriginalTilePoint,
        target_tile: OriginalTilePoint,
        door_candidates: usize,
        opening_door_candidates: usize,
        large_door_candidates: usize,
        weapon_pickup_candidates: usize,
        vehicle_entry_candidates: usize,
        objective_target_candidates: usize,
        scenario_target_candidates: usize,
    ) -> Self {
        let total = door_candidates
            + weapon_pickup_candidates
            + vehicle_entry_candidates
            + objective_target_candidates
            + scenario_target_candidates;
        let status = if total == 0 {
            OriginalDebugInteractionStatus::NoCandidateInteraction
        } else {
            OriginalDebugInteractionStatus::CandidateInteractionReady
        };
        let message = if total == 0 {
            "interaction/action: no candidate door, pickup, vehicle, objective, or scenario bucket near cursor; candidate-only".to_string()
        } else {
            format!(
                "interaction/action: candidate buckets near cursor doors {} pickups {} vehicles {} objectives {} scenarios {}; local-control only",
                door_candidates,
                weapon_pickup_candidates,
                vehicle_entry_candidates,
                objective_target_candidates,
                scenario_target_candidates
            )
        };

        Self {
            status,
            agent_tile: Some(agent_tile),
            target_tile: Some(target_tile),
            door_candidates,
            opening_door_candidates,
            large_door_candidates,
            weapon_pickup_candidates,
            vehicle_entry_candidates,
            objective_target_candidates,
            scenario_target_candidates,
            message,
        }
    }

    pub fn panel_label(&self) -> String {
        match self.status {
            OriginalDebugInteractionStatus::DebugDisabled
            | OriginalDebugInteractionStatus::MissingDebugAgent
            | OriginalDebugInteractionStatus::MissingTarget => self.message.clone(),
            OriginalDebugInteractionStatus::NoCandidateInteraction
            | OriginalDebugInteractionStatus::CandidateInteractionReady => {
                let Some(target) = self.target_tile else {
                    return self.message.clone();
                };
                format!(
                    "{} at {},{},{}; opening {} large {}; candidate-only",
                    self.message,
                    target.tile_x,
                    target.tile_y,
                    target.tile_z,
                    self.opening_door_candidates,
                    self.large_door_candidates
                )
            }
        }
    }

    fn candidate_total(&self) -> usize {
        self.door_candidates
            + self.weapon_pickup_candidates
            + self.vehicle_entry_candidates
            + self.objective_target_candidates
            + self.scenario_target_candidates
    }
}

impl OriginalObjectiveDebugProbe {
    fn from_model(
        model: &OriginalObjectiveScenarioModel,
        objects: &[OriginalMissionObjectCandidate],
    ) -> Self {
        let Some(current) = model.objectives.first().copied() else {
            return Self {
                objective_records: 0,
                current_candidate_index: None,
                current_candidate_kind: "none".to_string(),
                target_bucket: "none".to_string(),
                scenario_link_candidates: 0,
                success_condition_buckets: 0,
                failure_condition_buckets: 0,
                progress_status: OriginalObjectiveProgressStatus::NoCandidateObjective,
                guardrail: "objective progress is candidate-only; mission completion remains inactive",
            };
        };
        Self {
            objective_records: model.objectives.len(),
            current_candidate_index: Some(current.record_index),
            current_candidate_kind: current.kind.label().to_string(),
            target_bucket: current.target.bucket_label().to_string(),
            scenario_link_candidates: model.scenario_links_for_objective(current, objects),
            success_condition_buckets: current.success_buckets as usize,
            failure_condition_buckets: current.failure_buckets as usize,
            progress_status: OriginalObjectiveProgressStatus::CandidateOnly,
            guardrail: "objective progress is candidate-only; mission completion remains inactive",
        }
    }

    pub fn panel_label(&self) -> String {
        match self.progress_status {
            OriginalObjectiveProgressStatus::NoCandidateObjective => {
                "objective debug: no GAME objective candidate; demo gameplay active".to_string()
            }
            OriginalObjectiveProgressStatus::CandidateOnly => format!(
                "objective debug: current {} target {}; scenario links {}; success/failure {}/{}; candidate-only",
                self.current_candidate_kind,
                self.target_bucket,
                self.scenario_link_candidates,
                self.success_condition_buckets,
                self.failure_condition_buckets
            ),
        }
    }

    pub fn report_label(&self) -> String {
        format!(
            "{}; records {}; {}; runtime-only aggregate, not proof of objective or mission-completion semantics",
            self.panel_label(),
            self.objective_records,
            self.guardrail
        )
    }
}

impl OriginalDebugInteractionIntent {
    fn debug_disabled() -> Self {
        Self::blocked(
            OriginalDebugInteractionIntentStatus::DebugDisabled,
            OriginalDebugInteractionFocus::None,
            None,
            None,
            OriginalRuntimeRouteStatus::SpatialModelUnavailable,
            0,
            0,
            "interaction intent gated by G; original control is local-only and demo gameplay remains active",
        )
    }

    fn missing_debug_agent(target_tile: Option<OriginalTilePoint>) -> Self {
        Self::blocked(
            OriginalDebugInteractionIntentStatus::MissingDebugAgent,
            OriginalDebugInteractionFocus::None,
            None,
            target_tile,
            OriginalRuntimeRouteStatus::MissingStart,
            0,
            0,
            "interaction intent blocked: no selected original-agent marker",
        )
    }

    fn missing_target(agent_tile: OriginalTilePoint) -> Self {
        Self::blocked(
            OriginalDebugInteractionIntentStatus::MissingTarget,
            OriginalDebugInteractionFocus::None,
            Some(agent_tile),
            None,
            OriginalRuntimeRouteStatus::GoalOutsideCandidateGraph,
            0,
            0,
            "interaction intent blocked: cursor is outside the candidate map",
        )
    }

    fn from_probe(
        agent_tile: OriginalTilePoint,
        target_tile: OriginalTilePoint,
        probe: OriginalDebugInteractionProbe,
        route_probe: OriginalRuntimeRouteProbe,
    ) -> Self {
        let focus = OriginalDebugInteractionFocus::from_probe(&probe);
        let candidate_total = probe.candidate_total();
        if focus == OriginalDebugInteractionFocus::None || candidate_total == 0 {
            return Self::blocked(
                OriginalDebugInteractionIntentStatus::NoCandidateInteraction,
                focus,
                Some(agent_tile),
                Some(target_tile),
                route_probe.status,
                0,
                candidate_total,
                "interaction intent blocked: no candidate interaction bucket near cursor",
            );
        }

        let interaction_range = focus.interaction_range();
        let in_range = tile_near(agent_tile, target_tile, interaction_range, 1);
        let route_ready = route_probe.status == OriginalRuntimeRouteStatus::CandidateRouteReady;
        if in_range {
            return Self {
                status: OriginalDebugInteractionIntentStatus::ReadyAtTarget,
                focus,
                agent_tile: Some(agent_tile),
                target_tile: Some(target_tile),
                route_status: route_probe.status,
                route_nodes: route_probe.path.len(),
                route_path: Vec::new(),
                interaction_range,
                candidate_total,
                message: format!(
                    "debug interaction ready: {} candidate at target; no gameplay state changed",
                    focus.label()
                ),
            };
        }

        if route_ready && route_probe.path.len() > 1 {
            return Self {
                status: OriginalDebugInteractionIntentStatus::RouteQueued,
                focus,
                agent_tile: Some(agent_tile),
                target_tile: Some(target_tile),
                route_status: route_probe.status,
                route_nodes: route_probe.path.len(),
                route_path: route_probe.path,
                interaction_range,
                candidate_total,
                message: format!(
                    "debug interaction queued: route to {} candidate; local intent only",
                    focus.label()
                ),
            };
        }

        Self::blocked(
            OriginalDebugInteractionIntentStatus::RouteBlocked,
            focus,
            Some(agent_tile),
            Some(target_tile),
            route_probe.status,
            route_probe.path.len(),
            candidate_total,
            "interaction intent blocked: route precondition is not proven",
        )
    }

    fn blocked(
        status: OriginalDebugInteractionIntentStatus,
        focus: OriginalDebugInteractionFocus,
        agent_tile: Option<OriginalTilePoint>,
        target_tile: Option<OriginalTilePoint>,
        route_status: OriginalRuntimeRouteStatus,
        route_nodes: usize,
        candidate_total: usize,
        message: &str,
    ) -> Self {
        Self {
            status,
            focus,
            agent_tile,
            target_tile,
            route_status,
            route_nodes,
            route_path: Vec::new(),
            interaction_range: focus.interaction_range(),
            candidate_total,
            message: message.to_string(),
        }
    }

    pub fn panel_label(&self) -> String {
        format!(
            "{}; focus {}; route {:?} nodes {}; range {}; candidate-only",
            self.message,
            self.focus.label(),
            self.route_status,
            self.route_nodes,
            self.interaction_range
        )
    }

    pub fn mark_ready_after_route(&mut self, agent_tile: OriginalTilePoint) {
        if self.status == OriginalDebugInteractionIntentStatus::RouteQueued {
            self.agent_tile = Some(agent_tile);
            self.route_path.clear();
            self.route_nodes = 0;
            self.status = OriginalDebugInteractionIntentStatus::ReadyAtTarget;
            self.message = format!(
                "debug interaction ready after route: {} candidate; no gameplay state changed",
                self.focus.label()
            );
        }
    }
}

impl OriginalDebugInteractionFocus {
    fn from_probe(probe: &OriginalDebugInteractionProbe) -> Self {
        if probe.opening_door_candidates > 0 {
            Self::DoorOpenCandidate
        } else if probe.large_door_candidates > 0 {
            Self::LargeDoorCandidate
        } else if probe.door_candidates > 0 {
            Self::DoorOpenCandidate
        } else if probe.weapon_pickup_candidates > 0 {
            Self::WeaponPickupCandidate
        } else if probe.vehicle_entry_candidates > 0 {
            Self::VehicleEntryCandidate
        } else if probe.objective_target_candidates > 0 {
            Self::ObjectiveTargetCandidate
        } else if probe.scenario_target_candidates > 0 {
            Self::ScenarioTriggerCandidate
        } else {
            Self::None
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::DoorOpenCandidate => "door/open",
            Self::LargeDoorCandidate => "large-door",
            Self::WeaponPickupCandidate => "weapon-pickup",
            Self::VehicleEntryCandidate => "vehicle-entry",
            Self::ObjectiveTargetCandidate => "objective-target",
            Self::ScenarioTriggerCandidate => "scenario-trigger",
        }
    }

    fn interaction_range(self) -> u16 {
        match self {
            Self::VehicleEntryCandidate => 2,
            Self::None => 0,
            _ => 1,
        }
    }
}

impl OriginalDebugInteractionIntentStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::DebugDisabled => "disabled",
            Self::MissingDebugAgent => "missing-agent",
            Self::MissingTarget => "missing-target",
            Self::NoCandidateInteraction => "no-candidate",
            Self::RouteBlocked => "route-blocked",
            Self::RouteQueued => "route-queued",
            Self::ReadyAtTarget => "ready",
        }
    }
}

impl OriginalRuntimeRouteProbe {
    fn unavailable() -> Self {
        Self {
            status: OriginalRuntimeRouteStatus::SpatialModelUnavailable,
            start_tile: None,
            goal_tile: None,
            requested_goal_tile: None,
            snap: None,
            transition_kind: OriginalRouteTransitionKind::None,
            path: Vec::new(),
            message:
                "original route probe unavailable: missing guarded spatial model; demo grid active"
                    .to_string(),
        }
    }

    fn missing_start(goal_tile: OriginalTilePoint) -> Self {
        Self {
            status: OriginalRuntimeRouteStatus::MissingStart,
            start_tile: None,
            goal_tile: Some(goal_tile),
            requested_goal_tile: Some(goal_tile),
            snap: None,
            transition_kind: OriginalRouteTransitionKind::None,
            path: Vec::new(),
            message:
                "original route probe blocked: no supported original-control route seed; demo grid active"
                    .to_string(),
        }
    }

    fn goal_outside(start_tile: OriginalTilePoint, requested_goal_tile: OriginalTilePoint) -> Self {
        Self {
            status: OriginalRuntimeRouteStatus::GoalOutsideCandidateGraph,
            start_tile: Some(start_tile),
            goal_tile: None,
            requested_goal_tile: Some(requested_goal_tile),
            snap: None,
            transition_kind: OriginalRouteTransitionKind::None,
            path: Vec::new(),
            message: "original route probe blocked: picked tile has no nearby candidate node"
                .to_string(),
        }
    }

    fn height_unproven(
        start_tile: OriginalTilePoint,
        goal_tile: OriginalTilePoint,
        requested_goal_tile: OriginalTilePoint,
        snap: OriginalRouteTargetSnap,
    ) -> Self {
        Self {
            status: OriginalRuntimeRouteStatus::HeightTransitionsUnproven,
            start_tile: Some(start_tile),
            goal_tile: Some(goal_tile),
            requested_goal_tile: Some(requested_goal_tile),
            snap: Some(snap),
            transition_kind: OriginalRouteTransitionKind::None,
            path: Vec::new(),
            message:
                "original route probe blocked: height/slope transition semantics are not proven"
                    .to_string(),
        }
    }

    fn blocked(
        start_tile: OriginalTilePoint,
        goal_tile: OriginalTilePoint,
        requested_goal_tile: OriginalTilePoint,
        snap: OriginalRouteTargetSnap,
        debug_enabled: bool,
    ) -> Self {
        let mode = if debug_enabled {
            "debug candidate graph"
        } else {
            "same-level candidate graph"
        };
        Self {
            status: OriginalRuntimeRouteStatus::CandidateRouteBlocked,
            start_tile: Some(start_tile),
            goal_tile: Some(goal_tile),
            requested_goal_tile: Some(requested_goal_tile),
            snap: Some(snap),
            transition_kind: OriginalRouteTransitionKind::None,
            path: Vec::new(),
            message: format!("original route probe found no {mode} path; demo grid remains active"),
        }
    }

    fn ready(
        start_tile: OriginalTilePoint,
        goal_tile: OriginalTilePoint,
        requested_goal_tile: OriginalTilePoint,
        snap: OriginalRouteTargetSnap,
        transition_kind: OriginalRouteTransitionKind,
        mut path: Vec<OriginalTilePoint>,
    ) -> Self {
        let truncated = path.len() > MAX_ROUTE_PROBE_PATH_NODES;
        if truncated {
            path.truncate(MAX_ROUTE_PROBE_PATH_NODES);
        }
        let suffix = if truncated { " (overlay capped)" } else { "" };
        let mode = match transition_kind {
            OriginalRouteTransitionKind::SameLevelOnly => "same-level candidate route",
            OriginalRouteTransitionKind::CandidateSlopeHeight => {
                "debug candidate route with slope/height edges"
            }
            OriginalRouteTransitionKind::None => "candidate route",
        };
        let snap_suffix = if snap.xy_distance == 0 && snap.z_delta == 0 {
            String::new()
        } else {
            format!(
                "; target snapped xy {} z {} within {}",
                snap.xy_distance, snap.z_delta, snap.radius
            )
        };
        Self {
            status: OriginalRuntimeRouteStatus::CandidateRouteReady,
            start_tile: Some(start_tile),
            goal_tile: Some(goal_tile),
            requested_goal_tile: Some(requested_goal_tile),
            snap: Some(snap),
            transition_kind,
            message: format!(
                "original {mode}: {} nodes{suffix}{snap_suffix}; demo gameplay still active",
                path.len(),
            ),
            path,
        }
    }

    pub fn panel_label(&self) -> String {
        match self.status {
            OriginalRuntimeRouteStatus::CandidateRouteReady => self.message.clone(),
            OriginalRuntimeRouteStatus::SpatialModelUnavailable
            | OriginalRuntimeRouteStatus::MissingStart
            | OriginalRuntimeRouteStatus::GoalOutsideCandidateGraph
            | OriginalRuntimeRouteStatus::CandidateRouteBlocked
            | OriginalRuntimeRouteStatus::HeightTransitionsUnproven => self.message.clone(),
        }
    }
}

impl OriginalSpatialModel {
    fn from_map_and_objects(
        map_tiles: &OriginalMapTiles,
        tile_types: &OriginalTileTypes,
        objects: &[OriginalMissionObjectCandidate],
    ) -> Self {
        let mut surface_candidate_tiles = 0;
        let mut route_nodes = BTreeSet::new();
        let mut slope_nodes = BTreeMap::new();
        let mut safe_walk_candidate_nodes = 0;

        for z in 0..map_tiles.height {
            for y in 0..map_tiles.depth {
                for x in 0..map_tiles.width {
                    let tile = surface_type_at(map_tiles, tile_types, x, y, z);
                    if tile.is_surface_candidate() {
                        surface_candidate_tiles += 1;
                    }
                    let upper = if z + 1 < map_tiles.height {
                        surface_type_at(map_tiles, tile_types, x, y, z + 1)
                    } else {
                        CandidateSurfaceType::Empty
                    };
                    if tile.is_walkable_candidate_with_upper(upper) {
                        let key = OriginalTileKey {
                            x: x as u16,
                            y: y as u16,
                            z: z as u16,
                        };
                        route_nodes.insert(key);
                        if tile.is_slope_candidate() {
                            slope_nodes.insert(key, tile);
                        }
                        if tile.is_safe_walk_candidate() {
                            safe_walk_candidate_nodes += 1;
                        }
                    }
                }
            }
        }

        let mut static_blocked_tiles = BTreeSet::new();
        let mut static_footprint_tiles = BTreeSet::new();
        let mut vehicle_blocked_tiles = BTreeSet::new();
        let mut vehicle_footprint_tiles = BTreeSet::new();
        let mut door_patch_candidate_tiles = BTreeSet::new();
        let mut ped_occupied_tiles = BTreeSet::new();
        let mut agent_spawn_tiles = Vec::new();
        let mut enemy_spawn_tiles = Vec::new();
        for object in objects.iter().filter(|object| object.candidate_draw) {
            let Some(tile) = object.tile else {
                continue;
            };
            let key = tile.key();
            match object.kind {
                OriginalMissionObjectKind::Static => {
                    if is_large_door_static_candidate(object) {
                        door_patch_candidate_tiles.extend(door_surface_patch_tiles(
                            key,
                            object.orientation,
                            map_tiles.width as u16,
                            map_tiles.depth as u16,
                        ));
                    }
                    if !is_door_static_candidate(object) && !is_window_static_candidate(object) {
                        for footprint in object_footprint_tiles(
                            key,
                            static_footprint_radius(object),
                            map_tiles.width as u16,
                            map_tiles.depth as u16,
                        ) {
                            static_blocked_tiles.insert(footprint);
                            static_footprint_tiles.insert(footprint);
                        }
                    }
                }
                OriginalMissionObjectKind::Vehicle => {
                    for footprint in oriented_vehicle_footprint_tiles(
                        key,
                        object.orientation,
                        map_tiles.width as u16,
                        map_tiles.depth as u16,
                    ) {
                        vehicle_blocked_tiles.insert(footprint);
                        vehicle_footprint_tiles.insert(footprint);
                    }
                }
                OriginalMissionObjectKind::Ped => {
                    ped_occupied_tiles.insert(key);
                    if is_player_agent_spawn_candidate(object) {
                        agent_spawn_tiles.push(key);
                    } else if is_enemy_ped_spawn_candidate(object) {
                        enemy_spawn_tiles.push(key);
                    }
                }
                OriginalMissionObjectKind::Weapon | OriginalMissionObjectKind::Sfx => {}
            }
        }

        route_nodes.retain(|key| {
            !static_blocked_tiles.contains(key) && !vehicle_blocked_tiles.contains(key)
        });
        slope_nodes.retain(|key, _| route_nodes.contains(key));

        let same_level_edges = count_same_level_edges(&route_nodes);
        let diagonal_edges = count_diagonal_edges(&route_nodes);
        let diagonal_blocked_edges = count_diagonal_blocked_edges(&route_nodes);
        let slope_transition_edges = build_slope_transition_edges(&route_nodes, &slope_nodes);
        let road_route_nodes = count_surface_nodes(
            map_tiles,
            tile_types,
            &route_nodes,
            CandidateSurfaceType::is_road_candidate,
        );
        let roof_route_nodes = count_surface_nodes(
            map_tiles,
            tile_types,
            &route_nodes,
            CandidateSurfaceType::is_roof_candidate,
        );
        let train_route_nodes = count_surface_nodes(
            map_tiles,
            tile_types,
            &route_nodes,
            CandidateSurfaceType::is_train_candidate,
        );

        Self {
            route_nodes,
            slope_nodes,
            same_level_edges,
            diagonal_edges,
            diagonal_blocked_edges,
            slope_transition_edges,
            door_patch_candidate_tiles,
            static_blocked_tiles,
            static_footprint_tiles,
            vehicle_blocked_tiles,
            vehicle_footprint_tiles,
            ped_occupied_tiles,
            agent_spawn_tiles,
            enemy_spawn_tiles,
            surface_candidate_tiles,
            safe_walk_candidate_nodes,
            road_route_nodes,
            roof_route_nodes,
            train_route_nodes,
        }
    }

    fn first_agent_spawn_tile(&self) -> Option<OriginalTilePoint> {
        self.agent_spawn_tiles
            .first()
            .or_else(|| self.ped_occupied_tiles.iter().next())
            .copied()
            .map(OriginalTileKey::to_tile_point)
    }

    fn original_control_surface_tile(&self, tile: OriginalTilePoint) -> Option<OriginalTilePoint> {
        self.nearest_route_node_same_z(tile.key(), tile.tile_z, ROUTE_PROBE_SEARCH_RADIUS)
            .or_else(|| self.nearest_route_node_any_z(tile.key(), ROUTE_PROBE_DEBUG_SEARCH_RADIUS))
            .map(OriginalTileKey::to_tile_point)
    }

    fn route_probe_to_tile(
        &self,
        goal: OriginalTilePoint,
        debug_enabled: bool,
    ) -> OriginalRuntimeRouteProbe {
        let Some(seed) = self
            .agent_spawn_tiles
            .first()
            .copied()
            .or_else(|| self.ped_occupied_tiles.iter().next().copied())
        else {
            return OriginalRuntimeRouteProbe::missing_start(goal);
        };

        let Some(start) = self.nearest_route_node_same_z(seed, seed.z, ROUTE_PROBE_SEARCH_RADIUS)
        else {
            return OriginalRuntimeRouteProbe::missing_start(goal);
        };
        self.route_probe_from_start(start, goal, debug_enabled)
    }

    fn route_probe_between(
        &self,
        start: OriginalTilePoint,
        goal: OriginalTilePoint,
        debug_enabled: bool,
    ) -> OriginalRuntimeRouteProbe {
        let start_key = start.key();
        let mut start =
            self.nearest_route_node_same_z(start_key, start.tile_z, ROUTE_PROBE_SEARCH_RADIUS);
        if start.is_none() && debug_enabled {
            start = self.nearest_route_node_any_z(start_key, ROUTE_PROBE_DEBUG_SEARCH_RADIUS)
        }
        let Some(start) = start else {
            return OriginalRuntimeRouteProbe::missing_start(goal);
        };
        self.route_probe_from_start(start, goal, debug_enabled)
    }

    fn smoke_route_from(&self, start: OriginalTilePoint) -> OriginalRuntimeRouteProbe {
        let Some(start) = self
            .nearest_route_node_same_z(start.key(), start.tile_z, ROUTE_PROBE_SEARCH_RADIUS)
            .or_else(|| {
                self.nearest_route_node_any_z(start.key(), ROUTE_PROBE_DEBUG_SEARCH_RADIUS)
            })
        else {
            return OriginalRuntimeRouteProbe::missing_start(start);
        };
        let Some((path, transition_kind)) = self.short_debug_route_from_start(start, 8, 512) else {
            return OriginalRuntimeRouteProbe::blocked(
                start.to_tile_point(),
                start.to_tile_point(),
                start.to_tile_point(),
                OriginalRouteTargetSnap {
                    xy_distance: 0,
                    z_delta: 0,
                    radius: ROUTE_PROBE_DEBUG_SEARCH_RADIUS,
                },
                true,
            );
        };
        let goal = path.last().copied().unwrap_or(start);
        OriginalRuntimeRouteProbe::ready(
            start.to_tile_point(),
            goal.to_tile_point(),
            goal.to_tile_point(),
            OriginalRouteTargetSnap {
                xy_distance: 0,
                z_delta: 0,
                radius: ROUTE_PROBE_DEBUG_SEARCH_RADIUS,
            },
            transition_kind,
            path.into_iter()
                .map(OriginalTileKey::to_tile_point)
                .collect(),
        )
    }

    fn route_probe_from_start(
        &self,
        start: OriginalTileKey,
        goal: OriginalTilePoint,
        debug_enabled: bool,
    ) -> OriginalRuntimeRouteProbe {
        let goal_key = goal.key();
        let search_radius = if debug_enabled {
            ROUTE_PROBE_DEBUG_SEARCH_RADIUS
        } else {
            ROUTE_PROBE_SEARCH_RADIUS
        };
        if !debug_enabled && goal_key.z != start.z {
            if let Some(node) = self.nearest_route_node_same_z(goal_key, goal_key.z, search_radius)
            {
                return OriginalRuntimeRouteProbe::height_unproven(
                    start.to_tile_point(),
                    node.to_tile_point(),
                    goal,
                    OriginalRouteTargetSnap {
                        xy_distance: node.manhattan_xy(goal_key),
                        z_delta: node.z.abs_diff(goal_key.z),
                        radius: search_radius,
                    },
                );
            }
        }
        let Some((goal_node, snap)) =
            self.nearest_route_node_for_mode(goal_key, start.z, search_radius, debug_enabled)
        else {
            if !debug_enabled {
                if let Some(node) = self.nearest_route_node_any_z(goal_key, search_radius) {
                    return OriginalRuntimeRouteProbe::height_unproven(
                        start.to_tile_point(),
                        node.to_tile_point(),
                        goal,
                        OriginalRouteTargetSnap {
                            xy_distance: node.manhattan_xy(goal_key),
                            z_delta: node.z.abs_diff(goal_key.z),
                            radius: search_radius,
                        },
                    );
                }
            }
            return OriginalRuntimeRouteProbe::goal_outside(start.to_tile_point(), goal);
        };

        if !debug_enabled && start.z != goal_node.z {
            return OriginalRuntimeRouteProbe::height_unproven(
                start.to_tile_point(),
                goal_node.to_tile_point(),
                goal,
                snap,
            );
        }

        let route = if debug_enabled {
            self.debug_route(start, goal_node)
        } else {
            self.same_level_route(start, goal_node)
                .map(|path| (path, OriginalRouteTransitionKind::SameLevelOnly))
        };

        match route {
            Some((path, transition_kind)) => OriginalRuntimeRouteProbe::ready(
                start.to_tile_point(),
                goal_node.to_tile_point(),
                goal,
                snap,
                transition_kind,
                path.into_iter()
                    .map(OriginalTileKey::to_tile_point)
                    .collect(),
            ),
            None if !debug_enabled && self.has_height_route_candidate(start, goal_node) => {
                OriginalRuntimeRouteProbe::height_unproven(
                    start.to_tile_point(),
                    goal_node.to_tile_point(),
                    goal,
                    snap,
                )
            }
            None => OriginalRuntimeRouteProbe::blocked(
                start.to_tile_point(),
                goal_node.to_tile_point(),
                goal,
                snap,
                debug_enabled,
            ),
        }
    }

    fn nearest_route_node_for_mode(
        &self,
        target: OriginalTileKey,
        start_z: u16,
        max_radius: u16,
        debug_enabled: bool,
    ) -> Option<(OriginalTileKey, OriginalRouteTargetSnap)> {
        let node = if debug_enabled {
            self.nearest_route_node_any_z(target, max_radius)
        } else {
            self.nearest_route_node_same_z(target, start_z, max_radius)
        }?;
        Some((
            node,
            OriginalRouteTargetSnap {
                xy_distance: node.manhattan_xy(target),
                z_delta: node.z.abs_diff(target.z),
                radius: max_radius,
            },
        ))
    }

    fn nearest_route_node_same_z(
        &self,
        target: OriginalTileKey,
        z: u16,
        max_radius: u16,
    ) -> Option<OriginalTileKey> {
        if target.z == z && self.route_nodes.contains(&target) {
            return Some(target);
        }

        let mut best = None;
        for node in self.route_nodes.iter().filter(|node| node.z == z) {
            let distance = node.manhattan_xy(target);
            if distance <= max_radius
                && best.is_none_or(|(best_distance, _)| distance < best_distance)
            {
                best = Some((distance, *node));
            }
        }
        best.map(|(_, node)| node)
    }

    fn nearest_route_node_any_z(
        &self,
        target: OriginalTileKey,
        max_radius: u16,
    ) -> Option<OriginalTileKey> {
        let mut best = None;
        for node in &self.route_nodes {
            let distance = node.manhattan_xyz(target);
            if distance <= max_radius
                && best.is_none_or(|(best_distance, _)| distance < best_distance)
            {
                best = Some((distance, *node));
            }
        }
        best.map(|(_, node)| node)
    }

    fn same_level_route(
        &self,
        start: OriginalTileKey,
        goal: OriginalTileKey,
    ) -> Option<Vec<OriginalTileKey>> {
        if start.z != goal.z
            || !self.route_nodes.contains(&start)
            || !self.route_nodes.contains(&goal)
        {
            return None;
        }

        let mut queue = VecDeque::from([start]);
        let mut previous = BTreeMap::<OriginalTileKey, Option<OriginalTileKey>>::new();
        previous.insert(start, None);

        while let Some(node) = queue.pop_front() {
            if node == goal {
                break;
            }
            for neighbor in same_level_neighbors(&self.route_nodes, node) {
                if previous.contains_key(&neighbor) {
                    continue;
                }
                previous.insert(neighbor, Some(node));
                queue.push_back(neighbor);
            }
        }

        if !previous.contains_key(&goal) {
            return None;
        }

        let mut path = Vec::new();
        let mut cursor = goal;
        path.push(cursor);
        while let Some(Some(parent)) = previous.get(&cursor).copied() {
            cursor = parent;
            path.push(cursor);
        }
        path.reverse();
        Some(path)
    }

    fn debug_route(
        &self,
        start: OriginalTileKey,
        goal: OriginalTileKey,
    ) -> Option<(Vec<OriginalTileKey>, OriginalRouteTransitionKind)> {
        if !self.route_nodes.contains(&start) || !self.route_nodes.contains(&goal) {
            return None;
        }

        let mut queue = VecDeque::from([start]);
        let mut previous = BTreeMap::<OriginalTileKey, Option<OriginalTileKey>>::new();
        previous.insert(start, None);

        while let Some(node) = queue.pop_front() {
            if node == goal {
                break;
            }
            for neighbor in self.debug_neighbors(node) {
                if previous.contains_key(&neighbor) {
                    continue;
                }
                previous.insert(neighbor, Some(node));
                queue.push_back(neighbor);
            }
        }

        if !previous.contains_key(&goal) {
            return None;
        }

        let mut path = Vec::new();
        let mut cursor = goal;
        path.push(cursor);
        let mut used_height_transition = false;
        while let Some(Some(parent)) = previous.get(&cursor).copied() {
            if parent.z != cursor.z {
                used_height_transition = true;
            }
            cursor = parent;
            path.push(cursor);
        }
        path.reverse();
        let transition_kind = if used_height_transition {
            OriginalRouteTransitionKind::CandidateSlopeHeight
        } else {
            OriginalRouteTransitionKind::SameLevelOnly
        };
        Some((path, transition_kind))
    }

    fn short_debug_route_from_start(
        &self,
        start: OriginalTileKey,
        min_steps: usize,
        max_visited: usize,
    ) -> Option<(Vec<OriginalTileKey>, OriginalRouteTransitionKind)> {
        if !self.route_nodes.contains(&start) {
            return None;
        }

        let mut queue = VecDeque::from([start]);
        let mut previous = BTreeMap::<OriginalTileKey, Option<OriginalTileKey>>::new();
        let mut depth = BTreeMap::<OriginalTileKey, usize>::new();
        previous.insert(start, None);
        depth.insert(start, 0);
        let mut best = start;

        while let Some(node) = queue.pop_front() {
            let node_depth = depth.get(&node).copied().unwrap_or(0);
            if node_depth >= min_steps {
                best = node;
                break;
            }
            if previous.len() >= max_visited {
                break;
            }
            for neighbor in self.debug_neighbors(node) {
                if previous.contains_key(&neighbor) {
                    continue;
                }
                previous.insert(neighbor, Some(node));
                depth.insert(neighbor, node_depth + 1);
                if depth.get(&best).copied().unwrap_or(0) < node_depth + 1 {
                    best = neighbor;
                }
                queue.push_back(neighbor);
            }
        }

        if best == start {
            return None;
        }
        let mut path = Vec::new();
        let mut cursor = best;
        path.push(cursor);
        let mut used_height_transition = false;
        while let Some(Some(parent)) = previous.get(&cursor).copied() {
            if parent.z != cursor.z {
                used_height_transition = true;
            }
            cursor = parent;
            path.push(cursor);
        }
        path.reverse();
        let transition_kind = if used_height_transition {
            OriginalRouteTransitionKind::CandidateSlopeHeight
        } else {
            OriginalRouteTransitionKind::SameLevelOnly
        };
        Some((path, transition_kind))
    }

    fn debug_neighbors(&self, node: OriginalTileKey) -> Vec<OriginalTileKey> {
        let mut neighbors = same_level_neighbors(&self.route_nodes, node);
        for neighbor in node.height_cardinal_neighbors() {
            let edge = ordered_edge(node, neighbor);
            if self.slope_transition_edges.contains(&edge) {
                neighbors.push(neighbor);
            }
        }
        neighbors
    }

    fn has_height_route_candidate(&self, start: OriginalTileKey, goal: OriginalTileKey) -> bool {
        start.z != goal.z && self.debug_route(start, goal).is_some()
    }
}

impl OriginalTileKey {
    fn to_tile_point(self) -> OriginalTilePoint {
        OriginalTilePoint {
            tile_x: self.x,
            tile_y: self.y,
            tile_z: self.z,
            off_x: 128,
            off_y: 128,
            off_z: 0,
        }
    }

    fn manhattan_xy(self, other: OriginalTileKey) -> u16 {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }

    fn manhattan_xyz(self, other: OriginalTileKey) -> u16 {
        self.manhattan_xy(other) + self.z.abs_diff(other.z)
    }

    fn offset(self, dx: i16, dy: i16, dz: i16) -> Option<OriginalTileKey> {
        let x = (self.x as i32 + dx as i32).try_into().ok()?;
        let y = (self.y as i32 + dy as i32).try_into().ok()?;
        let z = (self.z as i32 + dz as i32).try_into().ok()?;
        Some(OriginalTileKey { x, y, z })
    }

    fn cardinal_neighbors(self) -> impl Iterator<Item = OriginalTileKey> {
        [
            self.x.checked_sub(1).map(|x| OriginalTileKey { x, ..self }),
            self.x.checked_add(1).map(|x| OriginalTileKey { x, ..self }),
            self.y.checked_sub(1).map(|y| OriginalTileKey { y, ..self }),
            self.y.checked_add(1).map(|y| OriginalTileKey { y, ..self }),
        ]
        .into_iter()
        .flatten()
    }

    fn diagonal_neighbors(self) -> impl Iterator<Item = OriginalTileKey> {
        let xm = self.x.checked_sub(1);
        let xp = self.x.checked_add(1);
        let ym = self.y.checked_sub(1);
        let yp = self.y.checked_add(1);
        [
            xm.zip(ym).map(|(x, y)| OriginalTileKey { x, y, ..self }),
            xp.zip(ym).map(|(x, y)| OriginalTileKey { x, y, ..self }),
            xp.zip(yp).map(|(x, y)| OriginalTileKey { x, y, ..self }),
            xm.zip(yp).map(|(x, y)| OriginalTileKey { x, y, ..self }),
        ]
        .into_iter()
        .flatten()
    }

    fn height_cardinal_neighbors(self) -> impl Iterator<Item = OriginalTileKey> {
        let lower = self.z.checked_sub(1);
        let upper = self.z.checked_add(1);
        [
            lower.and_then(|z| {
                self.x
                    .checked_sub(1)
                    .map(|x| OriginalTileKey { x, z, ..self })
            }),
            lower.and_then(|z| {
                self.x
                    .checked_add(1)
                    .map(|x| OriginalTileKey { x, z, ..self })
            }),
            lower.and_then(|z| {
                self.y
                    .checked_sub(1)
                    .map(|y| OriginalTileKey { y, z, ..self })
            }),
            lower.and_then(|z| {
                self.y
                    .checked_add(1)
                    .map(|y| OriginalTileKey { y, z, ..self })
            }),
            upper.and_then(|z| {
                self.x
                    .checked_sub(1)
                    .map(|x| OriginalTileKey { x, z, ..self })
            }),
            upper.and_then(|z| {
                self.x
                    .checked_add(1)
                    .map(|x| OriginalTileKey { x, z, ..self })
            }),
            upper.and_then(|z| {
                self.y
                    .checked_sub(1)
                    .map(|y| OriginalTileKey { y, z, ..self })
            }),
            upper.and_then(|z| {
                self.y
                    .checked_add(1)
                    .map(|y| OriginalTileKey { y, z, ..self })
            }),
        ]
        .into_iter()
        .flatten()
    }
}

impl CandidateSurfaceType {
    fn from_tile(tile_index: u8, tile_type: u8) -> Self {
        match tile_index {
            0x80 | 0x81 => return Self::TrainPlatform,
            0x8f | 0x93 => return Self::Empty,
            _ => {}
        }

        match tile_type {
            0x00 => Self::Empty,
            0x01 => Self::SlopeSn,
            0x02 => Self::SlopeNs,
            0x03 => Self::SlopeEw,
            0x04 => Self::SlopeWe,
            0x05 => Self::Ground,
            0x0a => Self::NonWalkable,
            0x06..=0x09 | 0x0b | 0x0f => Self::Road,
            0x0c => Self::HandrailLight,
            0x0d => Self::Roof,
            0x0e => Self::RoadPedCross,
            0x10 => Self::TrainStop,
            _ => Self::Unknown,
        }
    }

    fn is_surface_candidate(self) -> bool {
        matches!(
            self,
            Self::SlopeSn
                | Self::SlopeNs
                | Self::SlopeEw
                | Self::SlopeWe
                | Self::Ground
                | Self::Road
                | Self::Roof
                | Self::RoadPedCross
                | Self::TrainPlatform
        )
    }

    fn is_walkable_candidate_with_upper(self, upper: CandidateSurfaceType) -> bool {
        self.is_surface_candidate() && matches!(upper, Self::Empty | Self::TrainStop)
    }

    fn is_safe_walk_candidate(self) -> bool {
        self.is_walkable_candidate_with_upper(Self::Empty) && !matches!(self, Self::Road)
    }

    fn is_slope_candidate(self) -> bool {
        matches!(
            self,
            Self::SlopeSn | Self::SlopeNs | Self::SlopeEw | Self::SlopeWe
        )
    }

    fn is_road_candidate(self) -> bool {
        matches!(self, Self::Road | Self::RoadPedCross)
    }

    fn is_roof_candidate(self) -> bool {
        matches!(self, Self::Roof)
    }

    fn is_train_candidate(self) -> bool {
        matches!(self, Self::TrainPlatform)
    }

    fn upper_step_delta(self) -> Option<(i16, i16)> {
        match self {
            Self::SlopeSn => Some((0, -1)),
            Self::SlopeNs => Some((0, 1)),
            Self::SlopeEw => Some((1, 0)),
            Self::SlopeWe => Some((-1, 0)),
            _ => None,
        }
    }
}

impl OriginalObjectiveScenarioModel {
    fn from_game_bytes(
        decoded: &[u8],
        mission_script_probe: Option<&OriginalMissionScriptProbe>,
    ) -> Self {
        let objectives = objective_records(decoded)
            .enumerate()
            .filter_map(|(record_index, record)| {
                OriginalObjectiveCandidateRecord::from_record(record_index as u8, record)
            })
            .collect::<Vec<_>>();
        let scenarios = scenario_records(decoded)
            .enumerate()
            .filter_map(|(record_index, record)| {
                OriginalScenarioCandidateRecord::from_record(record_index as u16, record)
            })
            .collect::<Vec<_>>();
        let ped_scenario_starts = ped_scenario_start_indices(decoded);
        let scenario_chain_link_candidates = scenarios
            .iter()
            .filter(|record| record.next_index.is_some())
            .count();
        let scenario_loop_candidates =
            count_scenario_loop_candidates(&scenarios, &ped_scenario_starts);
        let scenario_invalid_next_candidates = scenarios
            .iter()
            .filter(|record| record.invalid_next)
            .count();
        let miss_active_record_candidates = mission_script_probe
            .map(|probe| probe.active_record_candidates)
            .unwrap_or_default();
        let miss_objective_buckets = mission_script_probe
            .map(|probe| probe.objective_bucket_candidates)
            .unwrap_or_default();

        Self {
            objectives,
            scenarios,
            ped_scenario_start_candidates: ped_scenario_starts.len(),
            scenario_chain_link_candidates,
            scenario_loop_candidates,
            scenario_invalid_next_candidates,
            miss_active_record_candidates,
            miss_objective_buckets,
        }
    }

    fn debug_interaction_probe(
        &self,
        agent_tile: OriginalTilePoint,
        target_tile: OriginalTilePoint,
        objects: &[OriginalMissionObjectCandidate],
    ) -> OriginalDebugInteractionProbe {
        let nearby_objects = objects.iter().filter(|object| {
            object.candidate_draw
                && object
                    .tile
                    .is_some_and(|tile| tile_near(tile, target_tile, 1, 1))
        });
        let door_candidates = nearby_objects
            .clone()
            .filter(|object| is_door_static_candidate(object))
            .count();
        let opening_door_candidates = objects
            .iter()
            .filter(|object| {
                object.candidate_draw
                    && object
                        .tile
                        .is_some_and(|tile| tile_near(tile, target_tile, 1, 1))
                    && object.kind == OriginalMissionObjectKind::Static
                    && matches!(object.subtype_value, Some(0x0e | 0x0f))
            })
            .count();
        let large_door_candidates = objects
            .iter()
            .filter(|object| {
                object.candidate_draw
                    && object
                        .tile
                        .is_some_and(|tile| tile_near(tile, target_tile, 1, 1))
                    && is_large_door_static_candidate(object)
            })
            .count();
        let weapon_pickup_candidates = objects
            .iter()
            .filter(|object| {
                object.candidate_draw
                    && object.kind == OriginalMissionObjectKind::Weapon
                    && object
                        .tile
                        .is_some_and(|tile| tile_near(tile, target_tile, 1, 1))
            })
            .count();
        let vehicle_entry_candidates = objects
            .iter()
            .filter(|object| {
                object.candidate_draw
                    && object.kind == OriginalMissionObjectKind::Vehicle
                    && object
                        .tile
                        .is_some_and(|tile| tile_near(tile, target_tile, 2, 1))
            })
            .count();
        let objective_target_candidates = self
            .objectives
            .iter()
            .filter(|record| record.matches_tile(target_tile, objects))
            .count();
        let scenario_target_candidates = self
            .scenarios
            .iter()
            .filter(|record| record.matches_tile(target_tile, objects))
            .count();

        OriginalDebugInteractionProbe::from_counts(
            agent_tile,
            target_tile,
            door_candidates,
            opening_door_candidates,
            large_door_candidates,
            weapon_pickup_candidates,
            vehicle_entry_candidates,
            objective_target_candidates,
            scenario_target_candidates,
        )
    }

    fn scenario_links_for_objective(
        &self,
        objective: OriginalObjectiveCandidateRecord,
        objects: &[OriginalMissionObjectCandidate],
    ) -> usize {
        self.scenarios
            .iter()
            .filter(|scenario| {
                scenario
                    .object_target
                    .is_some_and(|target| objective.target.matches_object_target(target))
                    || objective
                        .target
                        .tile_from_objects(objects)
                        .is_some_and(|tile| {
                            scenario
                                .tile
                                .is_some_and(|scenario_tile| tile_near(tile, scenario_tile, 1, 1))
                        })
            })
            .count()
    }
}

impl OriginalObjectiveCandidateRecord {
    fn from_record(record_index: u8, record: &[u8]) -> Option<Self> {
        if record.iter().all(|byte| *byte == 0) {
            return None;
        }
        let type_value = read_le_u16(record, 0);
        let offset = read_le_u16(record, 2);
        let tile = objective_position_candidate(
            read_le_u16(record, 4),
            read_le_u16(record, 6),
            read_le_u16(record, 8),
        );
        let kind = OriginalObjectiveCandidateKind::from_type_value(type_value);
        let target = kind.target_from_record(offset, tile);
        let (success_buckets, failure_buckets) = kind.condition_bucket_counts();

        Some(Self {
            record_index,
            kind,
            target,
            tile,
            success_buckets,
            failure_buckets,
        })
    }

    fn matches_tile(
        &self,
        target_tile: OriginalTilePoint,
        objects: &[OriginalMissionObjectCandidate],
    ) -> bool {
        if self
            .tile
            .is_some_and(|tile| tile_near(tile, target_tile, 1, 1))
        {
            return true;
        }
        self.target
            .tile_from_objects(objects)
            .is_some_and(|tile| tile_near(tile, target_tile, 1, 1))
    }
}

impl OriginalObjectiveCandidateKind {
    fn from_type_value(type_value: u16) -> Self {
        match type_value {
            0x00 => Self::SubOrLocation,
            0x01 => Self::Persuade,
            0x02 => Self::Assassinate,
            0x03 => Self::Protect,
            0x05 => Self::TakeWeapon,
            0x0a => Self::EliminatePolice,
            0x0b => Self::EliminateAgents,
            0x0e => Self::DestroyVehicle,
            0x0f => Self::UseVehicle,
            0x10 => Self::Evacuate,
            _ => Self::Unknown,
        }
    }

    fn target_from_record(
        self,
        offset: u16,
        tile: Option<OriginalTilePoint>,
    ) -> OriginalObjectiveTarget {
        match self {
            Self::Persuade | Self::Assassinate | Self::Protect => {
                objective_object_target(offset, OriginalMissionObjectKind::Ped)
            }
            Self::TakeWeapon => objective_object_target(offset, OriginalMissionObjectKind::Weapon),
            Self::DestroyVehicle | Self::UseVehicle => {
                objective_object_target(offset, OriginalMissionObjectKind::Vehicle)
            }
            Self::EliminatePolice | Self::EliminateAgents => OriginalObjectiveTarget::Group,
            Self::Evacuate => tile
                .map(OriginalObjectiveTarget::Location)
                .unwrap_or(OriginalObjectiveTarget::None),
            Self::SubOrLocation => {
                tile.map(OriginalObjectiveTarget::Location)
                    .unwrap_or_else(|| {
                        if offset == 0 {
                            OriginalObjectiveTarget::None
                        } else {
                            OriginalObjectiveTarget::UnresolvedOffset
                        }
                    })
            }
            Self::Unknown => {
                if offset == 0 {
                    tile.map(OriginalObjectiveTarget::Location)
                        .unwrap_or(OriginalObjectiveTarget::None)
                } else {
                    OriginalObjectiveTarget::UnresolvedOffset
                }
            }
        }
    }

    fn condition_bucket_counts(self) -> (u8, u8) {
        match self {
            Self::SubOrLocation | Self::Unknown => (0, 0),
            Self::DestroyVehicle => (1, 1),
            _ => (1, 2),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SubOrLocation => "sub/location",
            Self::Persuade => "persuade",
            Self::Assassinate => "assassinate",
            Self::Protect => "protect",
            Self::TakeWeapon => "take-weapon",
            Self::EliminatePolice => "eliminate-police",
            Self::EliminateAgents => "eliminate-agents",
            Self::DestroyVehicle => "destroy-vehicle",
            Self::UseVehicle => "use-vehicle",
            Self::Evacuate => "evacuate",
            Self::Unknown => "unknown",
        }
    }
}

impl OriginalObjectiveTarget {
    fn tile_from_objects(
        self,
        objects: &[OriginalMissionObjectCandidate],
    ) -> Option<OriginalTilePoint> {
        let (kind, record_index) = match self {
            Self::Ped(record_index) => (OriginalMissionObjectKind::Ped, record_index),
            Self::Vehicle(record_index) => (OriginalMissionObjectKind::Vehicle, record_index),
            Self::Weapon(record_index) => (OriginalMissionObjectKind::Weapon, record_index),
            Self::Location(tile) => return Some(tile),
            Self::None | Self::Group | Self::UnresolvedOffset => return None,
        };
        objects
            .iter()
            .find(|object| object.kind == kind && object.record_index == record_index)
            .and_then(|object| object.tile)
    }

    fn bucket_label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Ped(_) => "ped",
            Self::Vehicle(_) => "vehicle",
            Self::Weapon(_) => "weapon",
            Self::Group => "group",
            Self::Location(_) => "location",
            Self::UnresolvedOffset => "unresolved-offset",
        }
    }

    fn matches_object_target(self, target: OriginalObjectOffsetTarget) -> bool {
        matches!(
            (self, target),
            (Self::Ped(a), OriginalObjectOffsetTarget::Ped(b))
                | (Self::Vehicle(a), OriginalObjectOffsetTarget::Vehicle(b))
                | (Self::Weapon(a), OriginalObjectOffsetTarget::Weapon(b)) if a == b
        )
    }
}

impl OriginalScenarioCandidateRecord {
    fn from_record(record_index: u16, record: &[u8]) -> Option<Self> {
        let scenario_type = record.get(7).copied().unwrap_or_default();
        if scenario_type == 0 {
            return None;
        }
        let next_raw = read_le_u16(record, 0);
        let next_index = scenario_offset_to_index(next_raw);
        let object_target = object_offset_target(read_le_u16(record, 2));
        let tile = scenario_tile_candidate(record);
        let invalid_next = next_raw != 0 && next_index.is_none();
        let self_loop = next_index == Some(record_index);

        Some(Self {
            record_index,
            kind: OriginalScenarioCandidateKind::from_type_value(scenario_type),
            next_index,
            object_target,
            tile,
            invalid_next,
            self_loop,
        })
    }

    fn matches_tile(
        &self,
        target_tile: OriginalTilePoint,
        objects: &[OriginalMissionObjectCandidate],
    ) -> bool {
        if self
            .tile
            .is_some_and(|tile| tile_near(tile, target_tile, 1, 1))
        {
            return true;
        }
        self.object_target
            .and_then(|target| target.tile_from_objects(objects))
            .is_some_and(|tile| tile_near(tile, target_tile, 1, 1))
    }
}

impl OriginalScenarioCandidateKind {
    fn from_type_value(type_value: u8) -> Self {
        match type_value {
            0x01 => Self::WalkOrDrive,
            0x02 => Self::UseVehicle,
            0x07 => Self::Escape,
            0x08 => Self::Trigger,
            0x09 => Self::Reset,
            0x0a => Self::TrainWait,
            0x0b => Self::ProtectedTargetReached,
            _ => Self::Unknown,
        }
    }

    fn is_action_candidate(self) -> bool {
        matches!(
            self,
            Self::WalkOrDrive
                | Self::UseVehicle
                | Self::Escape
                | Self::Reset
                | Self::TrainWait
                | Self::ProtectedTargetReached
        )
    }
}

impl OriginalObjectOffsetTarget {
    fn tile_from_objects(
        self,
        objects: &[OriginalMissionObjectCandidate],
    ) -> Option<OriginalTilePoint> {
        let (kind, record_index) = match self {
            Self::Ped(record_index) => (OriginalMissionObjectKind::Ped, record_index),
            Self::Vehicle(record_index) => (OriginalMissionObjectKind::Vehicle, record_index),
            Self::Static(record_index) => (OriginalMissionObjectKind::Static, record_index),
            Self::Weapon(record_index) => (OriginalMissionObjectKind::Weapon, record_index),
            Self::Unknown => return None,
        };
        objects
            .iter()
            .find(|object| object.kind == kind && object.record_index == record_index)
            .and_then(|object| object.tile)
    }
}

impl OriginalMissionScriptProbe {
    fn from_root(root: &Path, mission_id: u16) -> Option<Self> {
        for relative in data_file_candidates(&format!("MISS{mission_id:02}.DAT")) {
            let path = root.join(relative);
            let Some(decoded) = read_original_asset_bytes(&path) else {
                continue;
            };
            return Some(Self::from_bytes(&decoded));
        }
        None
    }

    fn from_bytes(decoded: &[u8]) -> Self {
        let active_record_candidates = decoded
            .chunks_exact(8)
            .filter(|record| record.iter().any(|byte| *byte != 0))
            .count();
        let objective_bucket_candidates = decoded
            .chunks_exact(8)
            .filter(|record| record.iter().filter(|byte| **byte != 0).count() >= 2)
            .count();
        Self {
            active_record_candidates,
            objective_bucket_candidates,
        }
    }
}

fn count_same_level_edges(route_nodes: &BTreeSet<OriginalTileKey>) -> usize {
    route_nodes
        .iter()
        .flat_map(|node| {
            same_level_neighbors(route_nodes, *node)
                .into_iter()
                .map(|neighbor| ordered_edge(*node, neighbor))
        })
        .collect::<BTreeSet<_>>()
        .len()
}

fn count_diagonal_edges(route_nodes: &BTreeSet<OriginalTileKey>) -> usize {
    route_nodes
        .iter()
        .flat_map(|node| {
            node.diagonal_neighbors()
                .filter(|neighbor| {
                    route_nodes.contains(neighbor)
                        && diagonal_corners_clear(route_nodes, *node, *neighbor)
                })
                .map(|neighbor| ordered_edge(*node, neighbor))
        })
        .collect::<BTreeSet<_>>()
        .len()
}

fn count_diagonal_blocked_edges(route_nodes: &BTreeSet<OriginalTileKey>) -> usize {
    route_nodes
        .iter()
        .flat_map(|node| {
            node.diagonal_neighbors()
                .filter(|neighbor| {
                    route_nodes.contains(neighbor)
                        && !diagonal_corners_clear(route_nodes, *node, *neighbor)
                })
                .map(|neighbor| ordered_edge(*node, neighbor))
        })
        .collect::<BTreeSet<_>>()
        .len()
}

fn build_slope_transition_edges(
    route_nodes: &BTreeSet<OriginalTileKey>,
    slope_nodes: &BTreeMap<OriginalTileKey, CandidateSurfaceType>,
) -> BTreeSet<(OriginalTileKey, OriginalTileKey)> {
    let mut edges = BTreeSet::new();
    for (slope_node, slope_type) in slope_nodes {
        let Some((dx, dy)) = slope_type.upper_step_delta() else {
            continue;
        };
        if let Some(upper) = slope_node.offset(dx, dy, 1) {
            if route_nodes.contains(&upper) {
                edges.insert(ordered_edge(*slope_node, upper));
            }
        }
        if let Some(lower) = slope_node.offset(-dx, -dy, -1) {
            if route_nodes.contains(&lower) {
                edges.insert(ordered_edge(*slope_node, lower));
            }
        }
    }
    edges
}

fn same_level_neighbors(
    route_nodes: &BTreeSet<OriginalTileKey>,
    node: OriginalTileKey,
) -> Vec<OriginalTileKey> {
    let mut neighbors = node
        .cardinal_neighbors()
        .filter(|neighbor| route_nodes.contains(neighbor))
        .collect::<Vec<_>>();
    neighbors.extend(node.diagonal_neighbors().filter(|neighbor| {
        route_nodes.contains(neighbor) && diagonal_corners_clear(route_nodes, node, *neighbor)
    }));
    neighbors
}

fn diagonal_corners_clear(
    route_nodes: &BTreeSet<OriginalTileKey>,
    from: OriginalTileKey,
    to: OriginalTileKey,
) -> bool {
    if from.z != to.z || from.x == to.x || from.y == to.y {
        return true;
    }
    let corner_x = OriginalTileKey {
        x: to.x,
        y: from.y,
        z: from.z,
    };
    let corner_y = OriginalTileKey {
        x: from.x,
        y: to.y,
        z: from.z,
    };
    route_nodes.contains(&corner_x) && route_nodes.contains(&corner_y)
}

fn count_surface_nodes(
    map_tiles: &OriginalMapTiles,
    tile_types: &OriginalTileTypes,
    route_nodes: &BTreeSet<OriginalTileKey>,
    predicate: fn(CandidateSurfaceType) -> bool,
) -> usize {
    route_nodes
        .iter()
        .filter(|node| {
            predicate(surface_type_at(
                map_tiles,
                tile_types,
                node.x as usize,
                node.y as usize,
                node.z as usize,
            ))
        })
        .count()
}

fn ordered_edge(a: OriginalTileKey, b: OriginalTileKey) -> (OriginalTileKey, OriginalTileKey) {
    if a <= b { (a, b) } else { (b, a) }
}

fn object_footprint_tiles(
    origin: OriginalTileKey,
    radius: u16,
    map_width: u16,
    map_depth: u16,
) -> Vec<OriginalTileKey> {
    let mut tiles = Vec::new();
    let radius = radius as i32;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let x = origin.x as i32 + dx;
            let y = origin.y as i32 + dy;
            if x >= 0 && y >= 0 && x < map_width as i32 && y < map_depth as i32 {
                tiles.push(OriginalTileKey {
                    x: x as u16,
                    y: y as u16,
                    z: origin.z,
                });
            }
        }
    }
    tiles
}

fn oriented_vehicle_footprint_tiles(
    origin: OriginalTileKey,
    orientation: Option<u8>,
    map_width: u16,
    map_depth: u16,
) -> Vec<OriginalTileKey> {
    let along_x = !static_orientation_is_ns(orientation);
    let mut offsets = Vec::new();
    for long in -1..=1 {
        for wide in 0..=1 {
            let (dx, dy) = if along_x { (long, wide) } else { (wide, long) };
            offsets.push((dx, dy));
        }
    }
    bounded_offset_tiles(origin, &offsets, map_width, map_depth)
}

fn door_surface_patch_tiles(
    origin: OriginalTileKey,
    orientation: Option<u8>,
    map_width: u16,
    map_depth: u16,
) -> Vec<OriginalTileKey> {
    let offsets: &[(i32, i32)] = if static_orientation_is_ns(orientation) {
        &[(-1, 0), (0, 0), (1, 0)]
    } else {
        &[(0, -1), (0, 0), (0, 1)]
    };
    bounded_offset_tiles(origin, offsets, map_width, map_depth)
}

fn bounded_offset_tiles(
    origin: OriginalTileKey,
    offsets: &[(i32, i32)],
    map_width: u16,
    map_depth: u16,
) -> Vec<OriginalTileKey> {
    offsets
        .iter()
        .filter_map(|(dx, dy)| {
            let x = origin.x as i32 + dx;
            let y = origin.y as i32 + dy;
            (x >= 0 && y >= 0 && x < map_width as i32 && y < map_depth as i32).then_some(
                OriginalTileKey {
                    x: x as u16,
                    y: y as u16,
                    z: origin.z,
                },
            )
        })
        .collect()
}

fn static_footprint_radius(object: &OriginalMissionObjectCandidate) -> u16 {
    if matches!(object.subtype_value, Some(0x01 | 0x02 | 0x03 | 0x08 | 0x0a)) {
        1
    } else {
        0
    }
}

fn static_orientation_is_ns(orientation: Option<u8>) -> bool {
    matches!(orientation, Some(0x00 | 0x80 | 0x7e | 0xfe) | None)
}

fn surface_type_at(
    map_tiles: &OriginalMapTiles,
    tile_types: &OriginalTileTypes,
    x: usize,
    y: usize,
    z: usize,
) -> CandidateSurfaceType {
    map_tiles
        .tile_at(x, y, z)
        .map(|tile_index| {
            CandidateSurfaceType::from_tile(tile_index, tile_types.tile_type(tile_index))
        })
        .unwrap_or(CandidateSurfaceType::Empty)
}

impl AnimationCatalog {
    fn from_root(root: &Path) -> Self {
        for prefix in ["SYNDICAT/DATA", "DATADISK/DATA"] {
            let labels = [
                format!("{prefix}/HELE-0.ANI"),
                format!("{prefix}/HFRA-0.ANI"),
                format!("{prefix}/HSTA-0.ANI"),
            ];
            let hele = read_original_asset_bytes(&root.join(&labels[0]));
            let hfra = read_original_asset_bytes(&root.join(&labels[1]));
            let hsta = read_original_asset_bytes(&root.join(&labels[2]));
            let (Some(hele), Some(hfra), Some(hsta)) = (hele, hfra, hsta) else {
                continue;
            };
            return Self::from_bytes(labels.to_vec(), &hele, &hfra, &hsta);
        }

        Self::empty()
    }

    fn from_bytes(source_labels: Vec<String>, hele: &[u8], hfra: &[u8], hsta: &[u8]) -> Self {
        let elements = hele
            .chunks_exact(10)
            .map(|record| {
                let sprite_units = read_le_u16(record, 0);
                AnimElement {
                    sprite: sprite_units / SPRITE_TAB_ENTRY_BYTES as u16,
                    next_element: read_le_u16(record, 8),
                    sprite_unit_aligned: sprite_units % SPRITE_TAB_ENTRY_BYTES as u16 == 0,
                }
            })
            .collect::<Vec<_>>();
        let frames = hfra
            .chunks_exact(8)
            .map(|record| AnimFrame {
                first_element: read_le_u16(record, 0),
                next_frame: read_le_u16(record, 6),
            })
            .collect::<Vec<_>>();
        let animations = hsta
            .chunks_exact(2)
            .map(|record| read_le_u16(record, 0))
            .collect::<Vec<_>>();

        let invalid_element_sprite_units = elements
            .iter()
            .filter(|element| !element.sprite_unit_aligned)
            .count();
        let invalid_element_links = elements
            .iter()
            .filter(|element| {
                element.next_element != 0 && element.next_element as usize >= elements.len()
            })
            .count();
        let invalid_frame_links = frames
            .iter()
            .filter(|frame| frame.next_frame as usize >= frames.len())
            .count();
        let invalid_frame_element_links = frames
            .iter()
            .filter(|frame| frame.first_element as usize >= elements.len())
            .count();
        let invalid_animation_starts = animations
            .iter()
            .filter(|frame| **frame as usize >= frames.len())
            .count();

        Self {
            source_labels,
            elements,
            frames,
            animations,
            invalid_element_sprite_units,
            invalid_element_links,
            invalid_frame_links,
            invalid_frame_element_links,
            invalid_animation_starts,
        }
    }

    fn empty() -> Self {
        Self {
            source_labels: Vec::new(),
            elements: Vec::new(),
            frames: Vec::new(),
            animations: Vec::new(),
            invalid_element_sprite_units: 0,
            invalid_element_links: 0,
            invalid_frame_links: 0,
            invalid_frame_element_links: 0,
            invalid_animation_starts: 0,
        }
    }

    fn animation_start(&self, anim_id: u16) -> Option<u16> {
        let frame = *self.animations.get(anim_id as usize)?;
        self.frames.get(frame as usize)?;
        Some(frame)
    }

    fn frame_for_anim(&self, anim_id: u16, frame_id: u16) -> Option<&AnimFrame> {
        let mut frame_index = self.animation_start(anim_id)?;
        let mut remaining = frame_id as usize;
        let mut visited = BTreeSet::new();
        loop {
            let frame = self.frames.get(frame_index as usize)?;
            if remaining == 0 {
                return Some(frame);
            }
            if !visited.insert(frame_index) || frame.next_frame as usize >= self.frames.len() {
                return None;
            }
            frame_index = frame.next_frame;
            remaining -= 1;
        }
    }

    fn frame_index_supported(&self, frame_index: u16) -> bool {
        self.frames.get(frame_index as usize).is_some_and(|frame| {
            (frame.first_element as usize) < self.elements.len()
                && (frame.next_frame as usize) < self.frames.len()
        })
    }

    fn sprite_refs_for_frame_index(&self, frame_index: u16) -> Option<Vec<u16>> {
        let frame = self.frames.get(frame_index as usize)?;
        self.sprite_refs_for_frame(frame)
    }

    fn sprite_refs_for_anim_frame(&self, anim_id: u16, frame_id: u16) -> Option<Vec<u16>> {
        let frame = self.frame_for_anim(anim_id, frame_id)?;
        self.sprite_refs_for_frame(frame)
    }

    fn sprite_refs_for_frame(&self, frame: &AnimFrame) -> Option<Vec<u16>> {
        let mut element_index = frame.first_element;
        let mut sprites = Vec::new();
        let mut visited = BTreeSet::new();

        loop {
            let element = self.elements.get(element_index as usize)?;
            sprites.push(element.sprite);
            if element.next_element == 0 {
                break;
            }
            if !visited.insert(element_index)
                || element.next_element as usize >= self.elements.len()
            {
                return None;
            }
            element_index = element.next_element;
        }

        Some(sprites)
    }
}

fn collect_candidate_objects(decoded: &[u8]) -> Vec<OriginalMissionObjectCandidate> {
    scene_section_specs()
        .into_iter()
        .flat_map(|spec| collect_section_objects(decoded, spec))
        .collect()
}

fn collect_section_objects(
    decoded: &[u8],
    spec: SceneSectionSpec,
) -> Vec<OriginalMissionObjectCandidate> {
    let available_records = decoded
        .get(spec.start..)
        .map(|tail| tail.len().min(spec.record_count * spec.record_size) / spec.record_size)
        .unwrap_or_default();
    let mut objects = Vec::new();

    for index in 0..available_records {
        let start = spec.start + index * spec.record_size;
        let Some(record) = decoded.get(start..start + spec.record_size) else {
            continue;
        };
        let non_zero = record.iter().any(|&byte| byte != 0);
        if !non_zero {
            continue;
        }

        let desc = spec
            .desc_offset
            .and_then(|offset| record.get(offset).copied());
        let candidate_record = match desc {
            Some(value) => value != 0,
            None => non_zero,
        };
        let candidate_draw = match desc {
            Some(value) => spec.active_descs.contains(&value),
            None => non_zero,
        };
        let tile = spec
            .position_offsets
            .and_then(|(x, y, z)| read_tile_point(record, x, y, z));
        let queue_tile = tile.map(|tile| {
            if spec.kind == OriginalMissionObjectKind::Vehicle {
                OriginalTilePoint {
                    tile_z: tile.tile_z.saturating_add(1),
                    ..tile
                }
            } else {
                tile
            }
        });
        let animation = spec
            .animation_offsets
            .map(|(base, frame, anim)| OriginalAnimationRefs {
                base_anim: read_record_u16(record, base),
                current_frame: read_record_u16(record, frame),
                current_anim: read_record_u16(record, anim),
            })
            .unwrap_or_default();

        objects.push(OriginalMissionObjectCandidate {
            kind: spec.kind,
            record_index: index as u16,
            desc,
            state: spec
                .state_offset
                .and_then(|offset| record.get(offset).copied()),
            type_value: spec
                .type_offset
                .and_then(|offset| record.get(offset).copied()),
            subtype_value: spec
                .subtype_offset
                .and_then(|offset| record.get(offset).copied()),
            orientation: spec
                .orientation_offset
                .and_then(|offset| record.get(offset).copied()),
            tile,
            queue_tile,
            animation,
            candidate_record,
            candidate_draw,
            draw_stage: Some(spec.draw_stage),
        });
    }

    objects
}

fn build_section_counts(
    objects: &[OriginalMissionObjectCandidate],
    support: &OriginalAnimationCatalogSupport,
) -> Vec<OriginalMissionSceneSection> {
    let _ = support;
    scene_section_specs()
        .into_iter()
        .map(|spec| {
            let section_objects = objects
                .iter()
                .filter(|object| object.kind == spec.kind)
                .collect::<Vec<_>>();
            let candidate_records = section_objects
                .iter()
                .filter(|object| object.candidate_record)
                .count();
            let queued_records = section_objects
                .iter()
                .filter(|object| object.candidate_draw)
                .count();
            let supported_animation_refs = section_objects
                .iter()
                .filter(|object| object.candidate_draw)
                .filter(|object| object.animation.current_anim.is_some())
                .count();
            let supported_frame_refs = section_objects
                .iter()
                .filter(|object| object.candidate_draw)
                .filter(|object| object.animation.current_frame.is_some())
                .count();

            OriginalMissionSceneSection {
                label: spec.label,
                capacity: spec.record_count,
                non_zero_records: section_objects.len(),
                candidate_records,
                queued_records,
                supported_animation_refs,
                unsupported_animation_refs: 0,
                supported_frame_refs,
                unsupported_frame_refs: 0,
                draw_stage: Some(spec.draw_stage),
            }
        })
        .collect()
}

fn scene_section_specs() -> Vec<SceneSectionSpec> {
    vec![
        SceneSectionSpec {
            label: "candidate people",
            kind: OriginalMissionObjectKind::Ped,
            start: PEOPLE_OFFSET,
            record_count: 256,
            record_size: 92,
            desc_offset: Some(10),
            state_offset: Some(11),
            active_descs: ON_MAP_DESC,
            type_offset: Some(28),
            subtype_offset: Some(25),
            orientation_offset: Some(26),
            position_offsets: Some((4, 6, 8)),
            animation_offsets: Some((14, 16, 18)),
            draw_stage: OriginalDrawStage::People,
        },
        SceneSectionSpec {
            label: "candidate vehicles",
            kind: OriginalMissionObjectKind::Vehicle,
            start: CARS_OFFSET,
            record_count: 64,
            record_size: 42,
            desc_offset: Some(10),
            state_offset: None,
            active_descs: ON_MAP_DESC,
            type_offset: Some(20),
            subtype_offset: Some(21),
            orientation_offset: Some(22),
            position_offsets: Some((4, 6, 8)),
            animation_offsets: Some((14, 16, 18)),
            draw_stage: OriginalDrawStage::Vehicles,
        },
        SceneSectionSpec {
            label: "candidate statics",
            kind: OriginalMissionObjectKind::Static,
            start: STATICS_OFFSET,
            record_count: 400,
            record_size: 30,
            desc_offset: Some(10),
            state_offset: None,
            active_descs: STATIC_DRAW_DESCS,
            type_offset: Some(24),
            subtype_offset: Some(25),
            orientation_offset: Some(26),
            position_offsets: Some((4, 6, 8)),
            animation_offsets: Some((14, 16, 18)),
            draw_stage: OriginalDrawStage::Statics,
        },
        SceneSectionSpec {
            label: "candidate weapons",
            kind: OriginalMissionObjectKind::Weapon,
            start: WEAPONS_OFFSET,
            record_count: 512,
            record_size: 36,
            desc_offset: Some(10),
            state_offset: None,
            active_descs: ON_MAP_DESC,
            type_offset: Some(24),
            subtype_offset: Some(25),
            orientation_offset: None,
            position_offsets: Some((4, 6, 8)),
            animation_offsets: Some((14, 16, 18)),
            draw_stage: OriginalDrawStage::Weapons,
        },
        SceneSectionSpec {
            label: "candidate sfx",
            kind: OriginalMissionObjectKind::Sfx,
            start: SFX_OFFSET,
            record_count: 256,
            record_size: 30,
            desc_offset: None,
            state_offset: None,
            active_descs: &[],
            type_offset: None,
            subtype_offset: None,
            orientation_offset: None,
            position_offsets: Some((4, 6, 8)),
            animation_offsets: Some((14, 16, 18)),
            draw_stage: OriginalDrawStage::Sfx,
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SpriteTabBankSummary {
    label: String,
    entry_count: usize,
    valid_offset_entries: usize,
}

fn summarize_sprite_tab_bank(label: &str, tab: &[u8], dat_len: usize) -> SpriteTabBankSummary {
    let entry_count = tab.len() / SPRITE_TAB_ENTRY_BYTES;
    let valid_offset_entries = tab
        .chunks_exact(SPRITE_TAB_ENTRY_BYTES)
        .filter(|entry| {
            let offset = u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]) as usize;
            offset <= dat_len
        })
        .count();

    SpriteTabBankSummary {
        label: label.to_string(),
        entry_count,
        valid_offset_entries,
    }
}

fn is_door_static_candidate(object: &OriginalMissionObjectCandidate) -> bool {
    matches!(object.type_value, Some(3 | 4))
        || matches!(object.subtype_value, Some(0x0c..=0x0f | 0x26))
}

fn is_large_door_static_candidate(object: &OriginalMissionObjectCandidate) -> bool {
    matches!(object.type_value, Some(4)) || matches!(object.subtype_value, Some(0x26))
}

fn is_window_static_candidate(object: &OriginalMissionObjectCandidate) -> bool {
    matches!(object.type_value, Some(6 | 7))
        || matches!(object.subtype_value, Some(0x12 | 0x13 | 0x15 | 0x20..=0x25))
}

fn is_player_agent_spawn_candidate(object: &OriginalMissionObjectCandidate) -> bool {
    object.kind == OriginalMissionObjectKind::Ped
        && object.candidate_draw
        && object.record_index < 4
}

fn is_enemy_ped_spawn_candidate(object: &OriginalMissionObjectCandidate) -> bool {
    object.kind == OriginalMissionObjectKind::Ped
        && object.candidate_draw
        && object.record_index >= 4
        && (matches!(object.type_value, Some(0x02 | 0x04 | 0x08 | 0x10))
            || matches!(object.subtype_value, Some(0x02 | 0x04 | 0x08 | 0x10)))
}

fn objective_records(decoded: &[u8]) -> std::slice::ChunksExact<'_, u8> {
    let tail = decoded.get(OBJECTIVES_OFFSET..).unwrap_or(&[]);
    let len = tail
        .len()
        .min(OBJECTIVE_RECORD_COUNT * OBJECTIVE_RECORD_BYTES);
    tail[..len].chunks_exact(OBJECTIVE_RECORD_BYTES)
}

fn scenario_records(decoded: &[u8]) -> std::slice::ChunksExact<'_, u8> {
    let tail = decoded.get(SCENARIOS_OFFSET..).unwrap_or(&[]);
    let len = tail
        .len()
        .min(SCENARIO_RECORD_COUNT * SCENARIO_RECORD_BYTES);
    tail[..len].chunks_exact(SCENARIO_RECORD_BYTES)
}

fn scenario_offset_to_index(offset: u16) -> Option<u16> {
    if offset == 0 || offset as usize % SCENARIO_RECORD_BYTES != 0 {
        return None;
    }
    let index = offset as usize / SCENARIO_RECORD_BYTES;
    (index < SCENARIO_RECORD_COUNT).then_some(index as u16)
}

fn scenario_tile_candidate(record: &[u8]) -> Option<OriginalTilePoint> {
    let tile_x = *record.get(4)?;
    let tile_y = *record.get(5)?;
    let tile_z = *record.get(6)?;
    if tile_x == 0 && tile_y == 0 && tile_z == 0 {
        return None;
    }
    Some(OriginalTilePoint {
        tile_x: (tile_x >> 1) as u16,
        tile_y: (tile_y >> 1) as u16,
        tile_z: tile_z as u16,
        off_x: (tile_x & 1) << 7,
        off_y: (tile_y & 1) << 7,
        off_z: 0,
    })
}

fn objective_position_candidate(x: u16, y: u16, z: u16) -> Option<OriginalTilePoint> {
    if x == 0 && y == 0 && z == 0 {
        return None;
    }
    if x < 128 && y < 128 {
        return Some(OriginalTilePoint {
            tile_x: x,
            tile_y: y,
            tile_z: z.min(15),
            off_x: 128,
            off_y: 128,
            off_z: 0,
        });
    }
    Some(OriginalTilePoint {
        tile_x: x >> 8,
        tile_y: y >> 8,
        tile_z: z >> 7,
        off_x: (x & 0x00ff) as u8,
        off_y: (y & 0x00ff) as u8,
        off_z: (z & 0x007f) as u8,
    })
}

fn objective_object_target(
    offset: u16,
    expected_kind: OriginalMissionObjectKind,
) -> OriginalObjectiveTarget {
    match object_offset_target(offset) {
        Some(OriginalObjectOffsetTarget::Ped(index))
            if expected_kind == OriginalMissionObjectKind::Ped =>
        {
            OriginalObjectiveTarget::Ped(index)
        }
        Some(OriginalObjectOffsetTarget::Vehicle(index))
            if expected_kind == OriginalMissionObjectKind::Vehicle =>
        {
            OriginalObjectiveTarget::Vehicle(index)
        }
        Some(OriginalObjectOffsetTarget::Weapon(index))
            if expected_kind == OriginalMissionObjectKind::Weapon =>
        {
            OriginalObjectiveTarget::Weapon(index)
        }
        None if offset == 0 => OriginalObjectiveTarget::None,
        _ => OriginalObjectiveTarget::UnresolvedOffset,
    }
}

fn object_offset_target(offset: u16) -> Option<OriginalObjectOffsetTarget> {
    if offset == 0 {
        return None;
    }
    if let Some(index) = strided_object_index(
        offset,
        OBJECT_OFFSET_PEOPLE_BASE,
        OBJECT_OFFSET_VEHICLES_BASE,
        PEOPLE_RECORD_BYTES,
    ) {
        return Some(OriginalObjectOffsetTarget::Ped(index));
    }
    if let Some(index) = strided_object_index(
        offset,
        OBJECT_OFFSET_VEHICLES_BASE,
        OBJECT_OFFSET_STATICS_BASE,
        VEHICLE_RECORD_BYTES,
    ) {
        return Some(OriginalObjectOffsetTarget::Vehicle(index));
    }
    if let Some(index) = strided_object_index(
        offset,
        OBJECT_OFFSET_STATICS_BASE,
        OBJECT_OFFSET_WEAPONS_BASE,
        STATIC_RECORD_BYTES,
    ) {
        return Some(OriginalObjectOffsetTarget::Static(index));
    }
    if let Some(index) = strided_object_index(
        offset,
        OBJECT_OFFSET_WEAPONS_BASE,
        OBJECT_OFFSET_SFX_BASE,
        WEAPON_RECORD_BYTES,
    ) {
        return Some(OriginalObjectOffsetTarget::Weapon(index));
    }
    Some(OriginalObjectOffsetTarget::Unknown)
}

fn strided_object_index(offset: u16, base: u16, end: u16, stride: usize) -> Option<u16> {
    if offset < base || offset >= end {
        return None;
    }
    let delta = (offset - base) as usize;
    (delta % stride == 0).then_some((delta / stride) as u16)
}

fn ped_scenario_start_indices(decoded: &[u8]) -> Vec<u16> {
    let tail = decoded.get(PEOPLE_OFFSET..).unwrap_or(&[]);
    let len = tail.len().min(256 * PEOPLE_RECORD_BYTES);
    tail[..len]
        .chunks_exact(PEOPLE_RECORD_BYTES)
        .filter_map(|record| scenario_offset_to_index(read_le_u16(record, 40)))
        .collect()
}

fn count_scenario_loop_candidates(
    scenarios: &[OriginalScenarioCandidateRecord],
    ped_scenario_starts: &[u16],
) -> usize {
    let by_index = scenarios
        .iter()
        .map(|record| (record.record_index, record))
        .collect::<BTreeMap<_, _>>();
    let direct_loops = scenarios.iter().filter(|record| record.self_loop).count();
    let chain_loops = ped_scenario_starts
        .iter()
        .filter(|start| {
            let mut cursor = **start;
            let mut visited = BTreeSet::new();
            while visited.insert(cursor) {
                let Some(record) = by_index.get(&cursor) else {
                    return false;
                };
                let Some(next) = record.next_index else {
                    return false;
                };
                cursor = next;
            }
            true
        })
        .count();
    direct_loops + chain_loops
}

fn tile_near(a: OriginalTilePoint, b: OriginalTilePoint, xy_radius: u16, z_radius: u16) -> bool {
    a.tile_x.abs_diff(b.tile_x) <= xy_radius
        && a.tile_y.abs_diff(b.tile_y) <= xy_radius
        && a.tile_z.abs_diff(b.tile_z) <= z_radius
}

pub fn format_mission_scene_report_rows(root: impl AsRef<Path>) -> Vec<String> {
    let root = root.as_ref();
    let Ok(selection) = OriginalMissionSelection::from_root(root) else {
        return Vec::new();
    };
    OriginalMissionScene::from_root(root, &selection)
        .map(|scene| vec![scene.report_row()])
        .unwrap_or_default()
}

fn data_file_candidates(file_name: &str) -> Vec<String> {
    ["SYNDICAT/DATA", "DATADISK/DATA"]
        .into_iter()
        .map(|prefix| format!("{prefix}/{file_name}"))
        .collect()
}

fn read_original_asset_bytes(path: &Path) -> Option<Vec<u8>> {
    let data = fs::read(path).ok()?;
    if RncBlock::parse(&data).is_some() {
        decode_maybe_rnc(&data).ok()
    } else {
        Some(data)
    }
}

fn read_tile_point(
    record: &[u8],
    x_offset: usize,
    y_offset: usize,
    z_offset: usize,
) -> Option<OriginalTilePoint> {
    let x = read_record_u16(record, x_offset)?;
    let y = read_record_u16(record, y_offset)?;
    let z = read_record_u16(record, z_offset)?;
    Some(OriginalTilePoint {
        tile_x: x >> 8,
        tile_y: y >> 8,
        tile_z: z >> 7,
        off_x: x as u8,
        off_y: y as u8,
        off_z: (z & 0x7f) as u8,
    })
}

fn read_record_u16(record: &[u8], offset: usize) -> Option<u16> {
    record
        .get(offset..offset + 2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn decode_maybe_rnc(data: &[u8]) -> Result<Vec<u8>, RncError> {
    if let Some(block) = RncBlock::parse(data) {
        block.decompress()
    } else {
        Ok(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        AnimationCatalog, CARS_OFFSET, OBJECT_OFFSET_PEOPLE_BASE, OBJECT_OFFSET_VEHICLES_BASE,
        OBJECTIVES_OFFSET, OriginalAnimationCatalogSupport, OriginalDebugInteractionFocus,
        OriginalDebugInteractionIntentStatus, OriginalDebugInteractionStatus, OriginalDrawStage,
        OriginalMissionObjectKind, OriginalMissionScene, OriginalMissionScriptProbe,
        OriginalMissionSelection, OriginalSpriteBankSupport, PEOPLE_OFFSET, SCENARIOS_OFFSET,
        STATICS_OFFSET, WEAPONS_OFFSET, collect_candidate_objects,
        format_mission_scene_report_rows, summarize_sprite_tab_bank,
    };
    use crate::engine::{
        map_tiles::{OriginalMapTiles, OriginalTileTypes},
        original_sprites::{
            OriginalAnimationBank, OriginalGameSpriteAtlas, OriginalObjectSpriteRenderAssets,
        },
        palette_decode::Palette,
    };

    fn selection() -> OriginalMissionSelection {
        OriginalMissionSelection {
            campaign_label: "synthetic campaign".to_string(),
            mission_id: 1,
            palette_id: 2,
            mission_label: "synthetic/GAME01.DAT".to_string(),
            map_id: 1,
            map_label: "SYNDICAT/DATA/MAP01.DAT".to_string(),
            palette_label: "SYNDICAT/DATA/HPAL02.DAT".to_string(),
            min_scroll_tile: (10, 10),
            max_scroll_tile: (30, 30),
            render_diagnostics: crate::engine::mission_source::OriginalMissionRenderDiagnostics {
                sections: Vec::new(),
            },
        }
    }

    #[test]
    fn parses_typed_candidate_records_and_freesynd_positions() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 16];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            12,
            13,
            2,
            7,
            8,
            9,
            0x04,
        );
        decoded[PEOPLE_OFFSET + 11] = 0x10;
        decoded[PEOPLE_OFFSET + 25] = 0x02;
        decoded[PEOPLE_OFFSET + 26] = 0x40;
        decoded[PEOPLE_OFFSET + 28] = 0x02;

        write_record(
            &mut decoded[CARS_OFFSET..CARS_OFFSET + 42],
            12,
            13,
            2,
            1,
            2,
            3,
            0x04,
        );

        write_record(
            &mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30],
            16,
            17,
            4,
            2,
            3,
            5,
            0x06,
        );
        decoded[STATICS_OFFSET + 25] = 0x16;

        write_record(
            &mut decoded[WEAPONS_OFFSET..WEAPONS_OFFSET + 36],
            21,
            22,
            0,
            4,
            5,
            6,
            0x04,
        );

        let objects = collect_candidate_objects(&decoded);
        let ped = objects
            .iter()
            .find(|object| object.kind == OriginalMissionObjectKind::Ped)
            .unwrap();
        let vehicle = objects
            .iter()
            .find(|object| object.kind == OriginalMissionObjectKind::Vehicle)
            .unwrap();
        let static_object = objects
            .iter()
            .find(|object| object.kind == OriginalMissionObjectKind::Static)
            .unwrap();

        assert_eq!(ped.tile.unwrap().tile_x, 12);
        assert_eq!(ped.tile.unwrap().off_x, 7);
        assert_eq!(ped.tile.unwrap().tile_z, 2);
        assert_eq!(ped.subtype_value, Some(0x02));
        assert_eq!(vehicle.queue_tile.unwrap().tile_z, 3);
        assert!(static_object.candidate_draw);
        assert_eq!(
            objects
                .iter()
                .filter(|object| object.candidate_draw)
                .count(),
            4
        );
    }

    #[test]
    fn builds_draw_queue_with_stage_counts_and_same_tile_order() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 16];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            10,
            10,
            1,
            2,
            4,
            0,
            0x04,
        );
        write_record(
            &mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30],
            10,
            10,
            1,
            9,
            9,
            0,
            0x04,
        );
        write_record(
            &mut decoded[STATICS_OFFSET + 30..STATICS_OFFSET + 60],
            10,
            10,
            1,
            8,
            9,
            0,
            0x04,
        );
        let mut objects = collect_candidate_objects(&decoded);
        let sprite_support = OriginalSpriteBankSupport::from_primary_counts(8, 8);
        let catalog = synthetic_catalog();
        for object in &mut objects {
            object.animation.current_anim = Some(0);
            object.animation.current_frame = Some(0);
        }
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            objects,
            sprite_support,
            catalog,
            None,
            None,
            None,
            None,
        );

        assert_eq!(scene.draw_queue.total_candidates(), 3);
        assert_eq!(
            scene.draw_queue.stage_counts[0].stage,
            OriginalDrawStage::People
        );
        assert_eq!(
            scene.draw_queue.stage_counts[1].stage,
            OriginalDrawStage::Statics
        );
        assert_eq!(
            scene.draw_queue.entries()[0].stage,
            OriginalDrawStage::People
        );
        assert_eq!(
            scene.draw_queue.entries()[1].stage,
            OriginalDrawStage::Statics
        );
        assert_eq!(scene.draw_queue.entries()[1].tile.off_x, 8);
        assert_eq!(scene.draw_queue.entries()[2].tile.off_x, 9);
        assert!(scene.draw_queue_health_label().contains("anim 3/3"));
    }

    #[test]
    fn validates_animation_and_sprite_support_without_exposing_bytes() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 16];
        write_record(
            &mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30],
            4,
            5,
            1,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 18..STATICS_OFFSET + 20].copy_from_slice(&0u16.to_le_bytes());
        decoded[STATICS_OFFSET + 16..STATICS_OFFSET + 18].copy_from_slice(&0u16.to_le_bytes());

        let objects = collect_candidate_objects(&decoded);
        let sprite_support = OriginalSpriteBankSupport::from_primary_counts(4, 4);
        let catalog = synthetic_catalog();
        let support =
            OriginalAnimationCatalogSupport::from_catalog(&catalog, &objects, &sprite_support);

        assert_eq!(support.supported_animation_entries, 1);
        assert_eq!(support.supported_frame_entries, 1);
        assert_eq!(support.supported_sprite_entries, 1);
        let label = support.report_label();
        assert!(label.contains("anim catalog"));
        assert!(!label.contains("00 00"));
        assert!(!label.contains("0x"));
    }

    #[test]
    fn static_render_path_is_guarded_until_sprite_decoder_proof_exists() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 16];
        write_record(
            &mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30],
            4,
            5,
            1,
            0,
            0,
            0,
            0x04,
        );
        let objects = collect_candidate_objects(&decoded);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            objects,
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            None,
            None,
        );

        assert_eq!(scene.static_render_proof.candidate_count, 1);
        assert_eq!(
            scene.static_render_proof.decision,
            super::OriginalStaticRenderDecision::RuntimeRenderDisabled
        );
        assert!(scene.static_render_proof.panel_label().contains("disabled"));
        assert!(
            scene
                .static_render_proof
                .report_label()
                .contains("runtime-only")
        );
        assert!(
            scene
                .static_render_proof
                .report_label()
                .contains("not proof")
        );
    }

    #[test]
    fn static_render_path_becomes_ready_with_guarded_sprite_and_frame_proof() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 16];
        write_record(
            &mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30],
            4,
            5,
            1,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 25] = 0x16;
        let objects = collect_candidate_objects(&decoded);
        let render_assets = synthetic_render_assets();
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            objects,
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            Some(&render_assets),
            None,
            None,
            None,
        );

        assert_eq!(
            scene.static_render_proof.decision,
            super::OriginalStaticRenderDecision::RuntimeRenderReady
        );
        assert_eq!(scene.static_render_proof.runtime_renderable_static_count, 1);
        assert!(
            scene
                .static_render_proof
                .report_label()
                .contains("runtime-only")
        );
        assert!(
            scene
                .static_render_proof
                .report_label()
                .contains("no previews")
        );
        assert!(!scene.static_render_proof.report_label().contains("00 00"));
    }

    #[test]
    fn object_render_proof_gates_peds_vehicles_and_weapons_without_bytes() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 16];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            4,
            5,
            1,
            0,
            0,
            0,
            0x04,
        );
        decoded[PEOPLE_OFFSET + 11] = 0x10;
        decoded[PEOPLE_OFFSET + 26] = 0;
        write_record(
            &mut decoded[CARS_OFFSET..CARS_OFFSET + 42],
            6,
            7,
            1,
            0,
            0,
            0,
            0x04,
        );
        write_record(
            &mut decoded[WEAPONS_OFFSET..WEAPONS_OFFSET + 36],
            8,
            9,
            1,
            0,
            0,
            0,
            0x04,
        );
        let render_assets = synthetic_render_assets();
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            Some(&render_assets),
            None,
            None,
            None,
        );

        assert_eq!(
            scene.ped_render_proof.decision,
            super::OriginalObjectRenderDecision::RuntimeRenderReady
        );
        assert_eq!(
            scene.vehicle_render_proof.decision,
            super::OriginalObjectRenderDecision::RuntimeRenderReady
        );
        assert_eq!(
            scene.weapon_render_proof.decision,
            super::OriginalObjectRenderDecision::RuntimeRenderReady
        );
        assert_eq!(scene.ped_render_proof.runtime_renderable_count, 1);
        assert!(scene.object_render_report_label().contains("runtime-only"));
        assert!(scene.object_render_report_label().contains("not proof"));
        assert!(!scene.object_render_report_label().contains("00 00"));
        assert!(!scene.object_render_report_label().contains("0x"));
    }

    #[test]
    fn reports_spawn_and_navigation_bridge_candidates_conservatively() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        decoded[6..8].copy_from_slice(&4u16.to_le_bytes());
        decoded[8..10].copy_from_slice(&8u16.to_le_bytes());
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            8,
            9,
            1,
            0,
            0,
            0,
            0x04,
        );
        decoded[PEOPLE_OFFSET + 28] = 0x02;
        write_record(
            &mut decoded[CARS_OFFSET..CARS_OFFSET + 42],
            10,
            11,
            1,
            0,
            0,
            0,
            0x04,
        );
        write_record(
            &mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30],
            12,
            13,
            1,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 25] = 0x0c;
        write_record(
            &mut decoded[STATICS_OFFSET + 30..STATICS_OFFSET + 60],
            14,
            15,
            1,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 30 + 25] = 0x12;
        write_record(
            &mut decoded[STATICS_OFFSET + 60..STATICS_OFFSET + 90],
            16,
            17,
            1,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 60 + 25] = 0x16;
        decoded[SCENARIOS_OFFSET + 7] = 0x08;

        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            None,
            None,
        );

        assert_eq!(scene.spawn_probe.agent_candidates, 1);
        assert_eq!(scene.spawn_probe.trigger_scenario_candidates, 1);
        assert_eq!(scene.navigation_probe.map_object_link_cells, 2);
        assert_eq!(scene.navigation_probe.candidate_occupied_tiles, 5);
        assert_eq!(scene.navigation_probe.door_candidates, 1);
        assert_eq!(scene.navigation_probe.window_candidates, 1);
        assert_eq!(scene.navigation_probe.static_blocking_candidates, 1);
        assert_eq!(scene.navigation_probe.vehicle_footprint_candidates, 1);
        assert_eq!(scene.navigation_probe.ped_spawn_tile_candidates, 1);
        assert!(
            scene
                .navigation_probe
                .panel_label()
                .contains("demo grid active")
        );
        assert!(
            scene
                .navigation_probe
                .report_label()
                .contains("candidate only")
        );
        assert!(
            scene
                .spatial_probe
                .report_label()
                .contains("spatial model unavailable")
        );
    }

    #[test]
    fn builds_guarded_spatial_model_and_same_level_route_probe_without_bytes() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            0,
            0,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[PEOPLE_OFFSET + 28] = 0x02;
        write_record(
            &mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30],
            1,
            0,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 25] = 0x16;
        write_record(
            &mut decoded[CARS_OFFSET..CARS_OFFSET + 42],
            3,
            3,
            0,
            0,
            0,
            0,
            0x04,
        );

        let map_tiles = synthetic_map_tiles(4, 4, 2, [1, 0]);
        let tile_types = synthetic_tile_types(&[(1, 0x05)]);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            Some(&map_tiles),
            Some(&tile_types),
        );

        assert_eq!(scene.spatial_probe.surface_candidate_tiles, 16);
        assert_eq!(scene.spatial_probe.safe_walk_candidate_nodes, 16);
        assert_eq!(scene.spatial_probe.static_blocked_tiles, 1);
        assert_eq!(scene.spatial_probe.static_footprint_tiles, 1);
        assert_eq!(scene.spatial_probe.vehicle_blocked_tiles, 2);
        assert_eq!(scene.spatial_probe.vehicle_footprint_tiles, 2);
        assert_eq!(scene.spatial_probe.agent_spawn_groups, 1);
        assert_eq!(scene.spatial_probe.same_level_route_nodes, 13);
        let route = scene.original_route_probe_to_tile(super::OriginalTilePoint {
            tile_x: 2,
            tile_y: 0,
            tile_z: 0,
            off_x: 128,
            off_y: 128,
            off_z: 0,
        });

        assert_eq!(
            route.status,
            super::OriginalRuntimeRouteStatus::CandidateRouteReady
        );
        assert!(route.path.len() >= 5);
        assert!(route.path.iter().all(|tile| tile.tile_z == 0));
        assert!(scene.spatial_probe.report_label().contains("runtime-only"));
        assert!(scene.spatial_probe.report_label().contains("not proof"));
        assert!(!scene.spatial_probe.report_label().contains("00 00"));
        assert!(!scene.spatial_probe.report_label().contains("0x"));
    }

    #[test]
    fn debug_agent_spawns_fall_back_to_rendered_ped_candidates() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        let ped_offset = PEOPLE_OFFSET + super::PEOPLE_RECORD_BYTES * 6;
        write_record(
            &mut decoded[ped_offset..ped_offset + super::PEOPLE_RECORD_BYTES],
            1,
            1,
            0,
            0,
            0,
            0,
            0x04,
        );
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            None,
            None,
        );

        let spawns = scene.debug_agent_spawns();

        assert_eq!(scene.spawn_probe.agent_candidates, 0);
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].slot, 0);
        assert_eq!(spawns[0].record_index, 6);
        assert_eq!(
            spawns[0].tile,
            super::OriginalTilePoint {
                tile_x: 1,
                tile_y: 1,
                tile_z: 0,
                off_x: 0,
                off_y: 0,
                off_z: 0,
            }
        );
    }

    #[test]
    fn original_control_suppression_hides_base_peds_while_overlay_owns_agents() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        for record_index in 0..6 {
            let offset = PEOPLE_OFFSET + super::PEOPLE_RECORD_BYTES * record_index;
            write_record(
                &mut decoded[offset..offset + super::PEOPLE_RECORD_BYTES],
                10 + record_index as u16,
                12,
                1,
                0,
                0,
                0,
                0x04,
            );
        }
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            None,
            None,
        );

        assert_eq!(scene.debug_agent_spawns().len(), 4);
        assert_eq!(
            scene.original_control_suppressed_ped_record_indices(),
            vec![0, 1, 2, 3, 4, 5]
        );
    }

    #[test]
    fn debug_agent_spawns_start_on_control_surface_without_visible_z_snap() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        for record_index in 0..4 {
            let offset = PEOPLE_OFFSET + super::PEOPLE_RECORD_BYTES * record_index;
            write_record(
                &mut decoded[offset..offset + super::PEOPLE_RECORD_BYTES],
                1 + record_index as u16,
                1,
                1,
                0,
                0,
                0,
                0x04,
            );
        }
        let map_tiles = synthetic_map_tiles(5, 5, 2, [1, 0]);
        let tile_types = synthetic_tile_types(&[(1, 0x05)]);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            Some(&map_tiles),
            Some(&tile_types),
        );

        let spawns = scene.debug_agent_spawns();

        assert_eq!(spawns.len(), 4);
        assert!(spawns.iter().all(|spawn| spawn.tile.tile_z == 0));
        assert_eq!(
            scene.first_agent_spawn_tile().map(|tile| tile.tile_z),
            Some(0)
        );
    }

    #[test]
    fn original_control_smoke_route_uses_fallback_ped_seed_without_bytes() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        let ped_offset = PEOPLE_OFFSET + super::PEOPLE_RECORD_BYTES * 6;
        write_record(
            &mut decoded[ped_offset..ped_offset + super::PEOPLE_RECORD_BYTES],
            0,
            0,
            0,
            0,
            0,
            0,
            0x04,
        );
        let map_tiles = synthetic_map_tiles(5, 5, 2, [1, 0]);
        let tile_types = synthetic_tile_types(&[(1, 0x05)]);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            Some(&map_tiles),
            Some(&tile_types),
        );
        let spawn = scene
            .debug_agent_spawns()
            .first()
            .copied()
            .expect("fallback debug ped spawn");

        let route = scene.original_control_smoke_route_from(spawn.tile);

        assert_eq!(
            route.status,
            super::OriginalRuntimeRouteStatus::CandidateRouteReady
        );
        assert!(route.path.len() > 1);
        assert!(route.panel_label().contains("demo gameplay"));
        assert!(!route.panel_label().contains("00 00"));
        assert!(!route.panel_label().contains("0x"));
    }

    #[test]
    fn manual_debug_route_snaps_start_from_agent_z_to_nearby_route_surface() {
        let decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        let map_tiles = synthetic_map_tiles(5, 5, 2, [1, 0]);
        let tile_types = synthetic_tile_types(&[(1, 0x05)]);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            Some(&map_tiles),
            Some(&tile_types),
        );

        let route = scene.original_route_debug_probe_between(
            super::OriginalTilePoint {
                tile_x: 0,
                tile_y: 0,
                tile_z: 1,
                off_x: 128,
                off_y: 128,
                off_z: 0,
            },
            super::OriginalTilePoint {
                tile_x: 3,
                tile_y: 0,
                tile_z: 0,
                off_x: 128,
                off_y: 128,
                off_z: 0,
            },
        );

        assert_eq!(
            route.status,
            super::OriginalRuntimeRouteStatus::CandidateRouteReady
        );
        assert_eq!(route.start_tile.map(|tile| tile.tile_z), Some(0));
        assert!(route.path.len() > 1);
        assert!(!route.panel_label().contains("00 00"));
        assert!(!route.panel_label().contains("0x"));
    }

    #[test]
    fn route_probe_blocks_unproven_height_transitions_conservatively() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            0,
            0,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[PEOPLE_OFFSET + 28] = 0x02;
        let mut stacks = vec![[0, 0]; 20];
        stacks[0] = [1, 0];
        stacks[15] = [0, 1];
        let map_tiles = synthetic_map_tiles_with_stacks(20, 1, 2, &stacks);
        let tile_types = synthetic_tile_types(&[(1, 0x05)]);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            Some(&map_tiles),
            Some(&tile_types),
        );

        let route = scene.original_route_probe_to_tile(super::OriginalTilePoint {
            tile_x: 15,
            tile_y: 0,
            tile_z: 1,
            off_x: 128,
            off_y: 128,
            off_z: 0,
        });

        assert_eq!(
            route.status,
            super::OriginalRuntimeRouteStatus::HeightTransitionsUnproven
        );
        assert!(route.message.contains("height/slope"));
    }

    #[test]
    fn debug_route_probe_can_use_candidate_slope_height_edges() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            0,
            0,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[PEOPLE_OFFSET + 28] = 0x02;

        let stacks = vec![[2, 0], [0, 1], [0, 1]];
        let map_tiles = synthetic_map_tiles_with_stacks(3, 1, 2, &stacks);
        let tile_types = synthetic_tile_types(&[(1, 0x05), (2, 0x03)]);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            Some(&map_tiles),
            Some(&tile_types),
        );
        let goal = super::OriginalTilePoint {
            tile_x: 2,
            tile_y: 0,
            tile_z: 1,
            off_x: 128,
            off_y: 128,
            off_z: 0,
        };

        let guarded = scene.original_route_probe_to_tile(goal);
        assert_eq!(
            guarded.status,
            super::OriginalRuntimeRouteStatus::HeightTransitionsUnproven
        );

        let debug_route = scene.original_route_debug_probe_to_tile(goal);
        assert_eq!(
            debug_route.status,
            super::OriginalRuntimeRouteStatus::CandidateRouteReady
        );
        assert_eq!(
            debug_route.transition_kind,
            super::OriginalRouteTransitionKind::CandidateSlopeHeight
        );
        assert!(debug_route.path.iter().any(|tile| tile.tile_z == 0));
        assert!(debug_route.path.iter().any(|tile| tile.tile_z == 1));
        assert!(debug_route.message.contains("debug"));
        assert!(scene.navigation_debug_probe.slope_transition_edges >= 1);
        assert!(
            scene
                .navigation_debug_probe
                .report_label()
                .contains("debug-only")
        );
        assert!(
            !scene
                .navigation_debug_probe
                .report_label()
                .contains("00 00")
        );
        assert!(!scene.navigation_debug_probe.report_label().contains("0x"));
    }

    #[test]
    fn navigation_debug_probe_counts_door_patches_and_footprints_without_bytes() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        write_record(
            &mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30],
            2,
            2,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 25] = 0x26;
        decoded[STATICS_OFFSET + 26] = 0x00;
        write_record(
            &mut decoded[STATICS_OFFSET + 30..STATICS_OFFSET + 60],
            1,
            1,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 30 + 25] = 0x01;
        write_record(
            &mut decoded[CARS_OFFSET..CARS_OFFSET + 42],
            4,
            4,
            0,
            0,
            0,
            0,
            0x04,
        );

        let map_tiles = synthetic_map_tiles(6, 6, 2, [1, 0]);
        let tile_types = synthetic_tile_types(&[(1, 0x05)]);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            Some(&map_tiles),
            Some(&tile_types),
        );

        assert_eq!(scene.spatial_probe.door_patch_candidate_tiles, 3);
        assert_eq!(scene.spatial_probe.static_footprint_tiles, 9);
        assert!(scene.spatial_probe.vehicle_footprint_tiles > 1);
        assert!(
            scene
                .navigation_debug_probe
                .report_label()
                .contains("footprints static")
        );
        assert!(
            scene
                .navigation_debug_probe
                .report_label()
                .contains("runtime-only aggregate")
        );
        assert!(
            !scene
                .navigation_debug_probe
                .report_label()
                .contains("00 00")
        );
        assert!(!scene.navigation_debug_probe.report_label().contains("0x"));
    }

    #[test]
    fn eight_direction_routes_respect_diagonal_corner_blockers() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            0,
            0,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[PEOPLE_OFFSET + 28] = 0x02;
        write_record(
            &mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30],
            1,
            0,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 25] = 0x16;
        write_record(
            &mut decoded[STATICS_OFFSET + 30..STATICS_OFFSET + 60],
            0,
            1,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 30 + 25] = 0x16;

        let map_tiles = synthetic_map_tiles(2, 2, 2, [1, 0]);
        let tile_types = synthetic_tile_types(&[(1, 0x05)]);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            Some(&map_tiles),
            Some(&tile_types),
        );

        assert_eq!(scene.spatial_probe.diagonal_edges, 0);
        assert!(scene.spatial_probe.diagonal_blocked_edges >= 1);
        let route = scene.original_route_debug_probe_between(
            super::OriginalTilePoint {
                tile_x: 0,
                tile_y: 0,
                tile_z: 0,
                off_x: 128,
                off_y: 128,
                off_z: 0,
            },
            super::OriginalTilePoint {
                tile_x: 1,
                tile_y: 1,
                tile_z: 0,
                off_x: 128,
                off_y: 128,
                off_z: 0,
            },
        );
        assert_eq!(
            route.status,
            super::OriginalRuntimeRouteStatus::CandidateRouteBlocked
        );
    }

    #[test]
    fn slope_orientation_blocks_wrong_direction_height_edges() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            0,
            0,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[PEOPLE_OFFSET + 28] = 0x02;

        let stacks = vec![[2, 0], [0, 1]];
        let map_tiles = synthetic_map_tiles_with_stacks(2, 1, 2, &stacks);
        let tile_types = synthetic_tile_types(&[(1, 0x05), (2, 0x01)]);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            Some(&map_tiles),
            Some(&tile_types),
        );

        let route = scene.original_route_debug_probe_to_tile(super::OriginalTilePoint {
            tile_x: 1,
            tile_y: 0,
            tile_z: 1,
            off_x: 128,
            off_y: 128,
            off_z: 0,
        });

        assert_eq!(
            route.status,
            super::OriginalRuntimeRouteStatus::CandidateRouteBlocked
        );
        assert_eq!(scene.navigation_debug_probe.slope_transition_edges, 0);
    }

    #[test]
    fn interaction_probe_summarizes_candidate_buckets_without_bytes() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 24];
        write_record(
            &mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30],
            2,
            2,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 25] = 0x0e;
        write_record(
            &mut decoded[STATICS_OFFSET + 30..STATICS_OFFSET + 60],
            3,
            2,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 30 + 25] = 0x26;
        write_record(
            &mut decoded[CARS_OFFSET..CARS_OFFSET + 42],
            4,
            2,
            0,
            0,
            0,
            0,
            0x04,
        );
        write_record(
            &mut decoded[WEAPONS_OFFSET..WEAPONS_OFFSET + 36],
            5,
            2,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[SCENARIOS_OFFSET + 7] = 0x02;
        let miss_probe = OriginalMissionScriptProbe::from_bytes(&[1, 2, 0, 0, 0, 0, 0, 0]);

        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            Some(&miss_probe),
            None,
            None,
        );

        assert_eq!(scene.interaction_probe.door_interaction_candidates, 2);
        assert_eq!(scene.interaction_probe.opening_door_candidates, 1);
        assert_eq!(scene.interaction_probe.large_door_candidates, 1);
        assert_eq!(scene.interaction_probe.weapon_pickup_candidates, 1);
        assert_eq!(scene.interaction_probe.vehicle_entry_candidates, 1);
        assert_eq!(scene.interaction_probe.scenario_objective_buckets, 1);
        assert_eq!(scene.interaction_probe.miss_active_record_candidates, 1);
        assert_eq!(scene.interaction_probe.miss_objective_buckets, 1);
        assert!(
            scene
                .interaction_probe
                .report_label()
                .contains("candidate-only")
        );
        assert!(!scene.interaction_probe.report_label().contains("00 00"));
        assert!(!scene.interaction_probe.report_label().contains("0x"));
    }

    #[test]
    fn objective_and_scenario_probe_parses_typed_buckets_without_bytes() {
        let mut decoded = vec![0u8; OBJECTIVES_OFFSET + 96];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            2,
            2,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[PEOPLE_OFFSET + 40..PEOPLE_OFFSET + 42].copy_from_slice(&8u16.to_le_bytes());
        write_record(
            &mut decoded[CARS_OFFSET..CARS_OFFSET + 42],
            4,
            4,
            0,
            0,
            0,
            0,
            0x04,
        );

        decoded[SCENARIOS_OFFSET..SCENARIOS_OFFSET + 2].copy_from_slice(&8u16.to_le_bytes());
        decoded[SCENARIOS_OFFSET + 4] = 6;
        decoded[SCENARIOS_OFFSET + 5] = 8;
        decoded[SCENARIOS_OFFSET + 7] = 0x08;
        decoded[SCENARIOS_OFFSET + 8..SCENARIOS_OFFSET + 10].copy_from_slice(&8u16.to_le_bytes());
        decoded[SCENARIOS_OFFSET + 10..SCENARIOS_OFFSET + 12]
            .copy_from_slice(&OBJECT_OFFSET_VEHICLES_BASE.to_le_bytes());
        decoded[SCENARIOS_OFFSET + 15] = 0x02;

        let objectives = OBJECTIVES_OFFSET;
        decoded[objectives..objectives + 2].copy_from_slice(&1u16.to_le_bytes());
        decoded[objectives + 2..objectives + 4]
            .copy_from_slice(&OBJECT_OFFSET_PEOPLE_BASE.to_le_bytes());
        decoded[objectives + 14..objectives + 16].copy_from_slice(&15u16.to_le_bytes());
        decoded[objectives + 16..objectives + 18]
            .copy_from_slice(&OBJECT_OFFSET_VEHICLES_BASE.to_le_bytes());
        decoded[objectives + 28..objectives + 30].copy_from_slice(&16u16.to_le_bytes());
        decoded[objectives + 32..objectives + 34].copy_from_slice(&(7u16 << 8).to_le_bytes());
        decoded[objectives + 34..objectives + 36].copy_from_slice(&(8u16 << 8).to_le_bytes());

        let miss_probe = OriginalMissionScriptProbe::from_bytes(&[1, 2, 0, 0, 0, 0, 0, 0]);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            Some(&miss_probe),
            None,
            None,
        );

        assert_eq!(scene.interaction_probe.game_objective_records, 3);
        assert_eq!(scene.interaction_probe.game_objective_supported_records, 3);
        assert_eq!(scene.interaction_probe.objective_ped_target_buckets, 1);
        assert_eq!(scene.interaction_probe.objective_vehicle_target_buckets, 1);
        assert_eq!(scene.interaction_probe.objective_location_target_buckets, 1);
        assert_eq!(scene.interaction_probe.scenario_active_records, 2);
        assert_eq!(scene.interaction_probe.scenario_trigger_buckets, 1);
        assert_eq!(scene.interaction_probe.scenario_action_buckets, 1);
        assert_eq!(scene.interaction_probe.scenario_tile_target_buckets, 1);
        assert_eq!(scene.interaction_probe.scenario_object_target_buckets, 1);
        assert_eq!(scene.interaction_probe.scenario_chain_start_peds, 1);
        assert!(scene.interaction_probe.scenario_loop_candidates >= 1);
        assert_eq!(scene.interaction_probe.miss_active_record_candidates, 1);
        assert!(
            scene
                .interaction_probe
                .report_label()
                .contains("candidate-only")
        );
        assert!(scene.interaction_probe.report_label().contains("not proof"));
        assert!(!scene.interaction_probe.report_label().contains("00 00"));
        assert!(!scene.interaction_probe.report_label().contains("0x"));
    }

    #[test]
    fn debug_interaction_probe_matches_clicked_buckets_without_gameplay_semantics() {
        let mut decoded = vec![0u8; OBJECTIVES_OFFSET + 48];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            2,
            2,
            0,
            0,
            0,
            0,
            0x04,
        );
        write_record(
            &mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30],
            3,
            2,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[STATICS_OFFSET + 25] = 0x0e;
        write_record(
            &mut decoded[CARS_OFFSET..CARS_OFFSET + 42],
            4,
            2,
            0,
            0,
            0,
            0,
            0x04,
        );
        write_record(
            &mut decoded[WEAPONS_OFFSET..WEAPONS_OFFSET + 36],
            3,
            3,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[SCENARIOS_OFFSET + 4] = 6;
        decoded[SCENARIOS_OFFSET + 5] = 4;
        decoded[SCENARIOS_OFFSET + 7] = 0x08;
        decoded[OBJECTIVES_OFFSET..OBJECTIVES_OFFSET + 2].copy_from_slice(&1u16.to_le_bytes());
        decoded[OBJECTIVES_OFFSET + 2..OBJECTIVES_OFFSET + 4]
            .copy_from_slice(&OBJECT_OFFSET_PEOPLE_BASE.to_le_bytes());

        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            None,
            None,
        );
        let agent = super::OriginalTilePoint {
            tile_x: 2,
            tile_y: 2,
            tile_z: 0,
            off_x: 128,
            off_y: 128,
            off_z: 0,
        };
        let target = super::OriginalTilePoint {
            tile_x: 3,
            tile_y: 2,
            tile_z: 0,
            off_x: 128,
            off_y: 128,
            off_z: 0,
        };
        let probe = scene.original_debug_interaction_probe_between(Some(agent), Some(target), true);

        assert_eq!(
            probe.status,
            OriginalDebugInteractionStatus::CandidateInteractionReady
        );
        assert_eq!(probe.door_candidates, 1);
        assert_eq!(probe.opening_door_candidates, 1);
        assert_eq!(probe.weapon_pickup_candidates, 1);
        assert_eq!(probe.vehicle_entry_candidates, 1);
        assert_eq!(probe.objective_target_candidates, 1);
        assert_eq!(probe.scenario_target_candidates, 1);
        assert!(probe.panel_label().contains("candidate-only"));
        assert!(!probe.panel_label().contains("00 00"));
        assert!(!probe.panel_label().contains("0x"));

        let disabled =
            scene.original_debug_interaction_probe_between(Some(agent), Some(target), false);
        assert_eq!(
            disabled.status,
            OriginalDebugInteractionStatus::DebugDisabled
        );
    }

    #[test]
    fn objective_debug_probe_links_current_target_to_scenario_candidates_without_bytes() {
        let mut decoded = vec![0u8; OBJECTIVES_OFFSET + 48];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            2,
            2,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[SCENARIOS_OFFSET + 2..SCENARIOS_OFFSET + 4]
            .copy_from_slice(&OBJECT_OFFSET_PEOPLE_BASE.to_le_bytes());
        decoded[SCENARIOS_OFFSET + 7] = 0x08;
        decoded[OBJECTIVES_OFFSET..OBJECTIVES_OFFSET + 2].copy_from_slice(&1u16.to_le_bytes());
        decoded[OBJECTIVES_OFFSET + 2..OBJECTIVES_OFFSET + 4]
            .copy_from_slice(&OBJECT_OFFSET_PEOPLE_BASE.to_le_bytes());

        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            None,
            None,
        );

        assert_eq!(scene.objective_debug_probe.current_candidate_index, Some(0));
        assert_eq!(
            scene.objective_debug_probe.current_candidate_kind,
            "persuade"
        );
        assert_eq!(scene.objective_debug_probe.target_bucket, "ped");
        assert_eq!(scene.objective_debug_probe.scenario_link_candidates, 1);
        assert!(
            scene
                .objective_debug_probe
                .panel_label()
                .contains("candidate-only")
        );
        assert!(
            scene
                .objective_debug_probe
                .report_label()
                .contains("not proof")
        );
        assert!(!scene.objective_debug_probe.report_label().contains("00 00"));
        assert!(!scene.objective_debug_probe.report_label().contains("0x"));
    }

    #[test]
    fn debug_interaction_intent_queues_route_and_blocks_unproven_targets() {
        let mut decoded = vec![0u8; OBJECTIVES_OFFSET + 48];
        write_record(
            &mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92],
            0,
            0,
            0,
            0,
            0,
            0,
            0x04,
        );
        decoded[PEOPLE_OFFSET + 28] = 0x02;
        write_record(
            &mut decoded[WEAPONS_OFFSET..WEAPONS_OFFSET + 36],
            2,
            0,
            0,
            0,
            0,
            0,
            0x04,
        );

        let map_tiles = synthetic_map_tiles(4, 1, 2, [1, 0]);
        let tile_types = synthetic_tile_types(&[(1, 0x05)]);
        let scene = OriginalMissionScene::from_parts(
            &selection(),
            "synthetic/GAME01.DAT".to_string(),
            &decoded,
            collect_candidate_objects(&decoded),
            OriginalSpriteBankSupport::from_primary_counts(4, 4),
            synthetic_catalog(),
            None,
            None,
            Some(&map_tiles),
            Some(&tile_types),
        );
        let agent = super::OriginalTilePoint {
            tile_x: 0,
            tile_y: 0,
            tile_z: 0,
            off_x: 128,
            off_y: 128,
            off_z: 0,
        };
        let weapon = super::OriginalTilePoint {
            tile_x: 2,
            tile_y: 0,
            tile_z: 0,
            off_x: 128,
            off_y: 128,
            off_z: 0,
        };

        let queued =
            scene.original_debug_interaction_intent_between(Some(agent), Some(weapon), true);
        assert_eq!(
            queued.status,
            OriginalDebugInteractionIntentStatus::RouteQueued
        );
        assert_eq!(
            queued.focus,
            OriginalDebugInteractionFocus::WeaponPickupCandidate
        );
        assert!(queued.route_path.len() > 1);
        assert!(queued.panel_label().contains("candidate-only"));
        assert!(!queued.panel_label().contains("00 00"));
        assert!(!queued.panel_label().contains("0x"));

        let ready =
            scene.original_debug_interaction_intent_between(Some(weapon), Some(weapon), true);
        assert_eq!(
            ready.status,
            OriginalDebugInteractionIntentStatus::ReadyAtTarget
        );

        let empty = scene.original_debug_interaction_intent_between(
            Some(agent),
            Some(super::OriginalTilePoint {
                tile_x: 0,
                tile_y: 0,
                tile_z: 0,
                off_x: 128,
                off_y: 128,
                off_z: 0,
            }),
            true,
        );
        assert_eq!(
            empty.status,
            OriginalDebugInteractionIntentStatus::NoCandidateInteraction
        );
    }

    #[test]
    fn summarizes_six_byte_sprite_tab_as_counts_only() {
        let mut tab = Vec::new();
        tab.extend_from_slice(&0u32.to_le_bytes());
        tab.extend_from_slice(&[16, 24]);
        tab.extend_from_slice(&20u32.to_le_bytes());
        tab.extend_from_slice(&[8, 12]);

        let summary = summarize_sprite_tab_bank("SYNDICAT/DATA/HSPR-0.TAB", &tab, 40);

        assert_eq!(summary.entry_count, 2);
        assert_eq!(summary.valid_offset_entries, 2);
        assert!(!format!("{summary:?}").contains("16x24"));
    }

    #[test]
    fn empty_report_rows_do_not_claim_scene_semantics() {
        let rows = format_mission_scene_report_rows(Path::new("definitely-not-an-asset-root"));
        assert!(rows.is_empty());
    }

    fn synthetic_catalog() -> AnimationCatalog {
        let mut hele = Vec::new();
        hele.extend_from_slice(&0u16.to_le_bytes());
        hele.extend_from_slice(&0i16.to_le_bytes());
        hele.extend_from_slice(&0i16.to_le_bytes());
        hele.extend_from_slice(&0u16.to_le_bytes());
        hele.extend_from_slice(&0u16.to_le_bytes());

        let mut hfra = Vec::new();
        hfra.extend_from_slice(&0u16.to_le_bytes());
        hfra.extend_from_slice(&[1, 1]);
        hfra.extend_from_slice(&0u16.to_le_bytes());
        hfra.extend_from_slice(&0u16.to_le_bytes());

        let hsta = 0u16.to_le_bytes();
        AnimationCatalog::from_bytes(vec!["synthetic".to_string()], &hele, &hfra, &hsta)
    }

    fn synthetic_render_assets() -> OriginalObjectSpriteRenderAssets {
        let palette = synthetic_palette();
        let mut tab = vec![0u8; 6];
        tab[4] = 8;
        tab[5] = 1;
        let dat = [0u8; 5];
        let sprite_atlas = OriginalGameSpriteAtlas::from_bytes(
            "synthetic/HSPR-0".to_string(),
            "synthetic/HPAL".to_string(),
            &tab,
            &dat,
            &palette,
        )
        .unwrap();

        let mut hele = Vec::new();
        hele.extend_from_slice(&0u16.to_le_bytes());
        hele.extend_from_slice(&0i16.to_le_bytes());
        hele.extend_from_slice(&0i16.to_le_bytes());
        hele.extend_from_slice(&0u16.to_le_bytes());
        hele.extend_from_slice(&0u16.to_le_bytes());

        let mut hfra = Vec::new();
        hfra.extend_from_slice(&0u16.to_le_bytes());
        hfra.extend_from_slice(&[1, 1]);
        hfra.extend_from_slice(&0x0100u16.to_le_bytes());
        hfra.extend_from_slice(&0u16.to_le_bytes());

        let hsta = 0u16.to_le_bytes();
        let animation_bank =
            OriginalAnimationBank::from_bytes(vec!["synthetic".to_string()], &hele, &hfra, &hsta)
                .unwrap();

        OriginalObjectSpriteRenderAssets {
            sprite_atlas,
            animation_bank,
        }
    }

    fn synthetic_palette() -> Palette {
        let mut data = vec![0u8; 768];
        for i in 0..256 {
            data[i * 3] = (i % 64) as u8;
            data[i * 3 + 1] = ((i * 2) % 64) as u8;
            data[i * 3 + 2] = ((i * 3) % 64) as u8;
        }
        Palette::decode_vga_6bit(&data).unwrap()
    }

    fn synthetic_map_tiles(
        width: u32,
        depth: u32,
        height: u32,
        stack: [u8; 2],
    ) -> OriginalMapTiles {
        let stacks = vec![stack; (width * depth) as usize];
        synthetic_map_tiles_with_stacks(width, depth, height, &stacks)
    }

    fn synthetic_map_tiles_with_stacks(
        width: u32,
        depth: u32,
        height: u32,
        stacks: &[[u8; 2]],
    ) -> OriginalMapTiles {
        assert_eq!(height, 2);
        assert_eq!(stacks.len(), (width * depth) as usize);
        let column_count = (width * depth) as usize;
        let offset_table_bytes = column_count * 4;
        let mut decoded = Vec::new();
        decoded.extend_from_slice(&width.to_le_bytes());
        decoded.extend_from_slice(&depth.to_le_bytes());
        decoded.extend_from_slice(&height.to_le_bytes());
        let mut stack_payload = Vec::new();
        for stack in stacks {
            let offset_from_byte_12 = (offset_table_bytes + stack_payload.len()) as u32;
            decoded.extend_from_slice(&offset_from_byte_12.to_le_bytes());
            stack_payload.extend_from_slice(stack);
        }
        decoded.extend_from_slice(&stack_payload);
        OriginalMapTiles::from_decoded_bytes("synthetic/MAP01.DAT".to_string(), &decoded).unwrap()
    }

    fn synthetic_tile_types(entries: &[(u8, u8)]) -> OriginalTileTypes {
        let mut decoded = vec![0u8; 256];
        for (tile_index, tile_type) in entries {
            decoded[*tile_index as usize] = *tile_type;
        }
        OriginalTileTypes::from_decoded_bytes("synthetic/COL01.DAT".to_string(), &decoded).unwrap()
    }

    fn write_record(
        record: &mut [u8],
        tile_x: u16,
        tile_y: u16,
        tile_z: u16,
        off_x: u8,
        off_y: u8,
        off_z: u8,
        desc: u8,
    ) {
        record[4..6].copy_from_slice(&(((tile_x << 8) | off_x as u16).to_le_bytes()));
        record[6..8].copy_from_slice(&(((tile_y << 8) | off_y as u16).to_le_bytes()));
        record[8..10].copy_from_slice(&(((tile_z << 7) | off_z as u16).to_le_bytes()));
        record[10] = desc;
        record[14..16].copy_from_slice(&0u16.to_le_bytes());
        record[16..18].copy_from_slice(&0u16.to_le_bytes());
        record[18..20].copy_from_slice(&0u16.to_le_bytes());
    }
}
