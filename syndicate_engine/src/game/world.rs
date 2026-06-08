use std::collections::{BTreeMap, BTreeSet};

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
            OriginalCombatLineProbe, OriginalDebugAgentSpawn, OriginalDebugAgentWeaponHint,
            OriginalDebugAgentWeaponSource, OriginalDebugInteractionFocus,
            OriginalDebugInteractionIntent, OriginalDebugInteractionIntentStatus,
            OriginalDebugInteractionProbe, OriginalMissionObjectCandidate,
            OriginalMissionObjectKind, OriginalMissionScene, OriginalObjectRenderDecision,
            OriginalObjectiveRuntimeTarget, OriginalRuntimeRouteProbe, OriginalRuntimeRouteStatus,
            OriginalStaticRenderDecision, OriginalTilePoint, OriginalWeaponKind,
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
    original_combat_runtime: OriginalMissionCombatRuntime,
    original_combat_feedback: Option<OriginalCombatFeedback>,
    original_hover_target: Option<OriginalCombatTargetCandidate>,
    original_control_trace: OriginalControlTrace,
}

const QUICK_SAVE_PATH: &str = "../saves/quicksave.json";
const ORIGINAL_DEBUG_AGENT_MAX_STEP_DT: f32 = 0.05;
const ORIGINAL_CONTROL_SHOOT_REACTION_SECS: f32 = 0.20;
const ORIGINAL_CONTROL_TARGET_HP: i32 = 50;
const ORIGINAL_CONTROL_COMBAT_FEEDBACK_SECS: f32 = 0.58;
const ORIGINAL_CONTROL_AGENT_UNDER_FIRE_SECS: f32 = 0.90;
const ORIGINAL_CONTROL_HOSTILE_REACTION_DELAY_SECS: f32 = 0.35;
const ORIGINAL_CONTROL_HOSTILE_RELOAD_SECS: f32 = 1.25;
const ORIGINAL_COMBAT_TARGET_PICK_RADIUS: u16 = 2;

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
    weapon_cooldown: f32,
    weapons: Vec<OriginalCombatWeaponProfile>,
    selected_weapon_index: usize,
    under_fire_remaining: f32,
    local_threat_marks: u16,
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
    combat_hit_count: usize,
    last_result: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct OriginalMissionCombatRuntime {
    peds: BTreeMap<u16, OriginalCombatPedState>,
    objective_target: Option<OriginalObjectiveRuntimeTarget>,
    shots_fired: usize,
    hits: usize,
    defeated: usize,
    out_of_range: usize,
    blocked: usize,
    npc_reactions: usize,
    hostile_return_fire: usize,
    hostile_reaction_blocked: usize,
    objective_completed: bool,
    last_target: Option<OriginalCombatTargetCandidate>,
    hostile_reactions: BTreeMap<u16, OriginalHostileReactionState>,
    last_result: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct OriginalHostileReactionState {
    record_index: u16,
    role: OriginalCombatTargetRole,
    tile: OriginalTilePoint,
    next_fire_secs: f32,
    shots: usize,
    blocked: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginalHostileFireEvent {
    origin: OriginalTilePoint,
    target: OriginalTilePoint,
    target_agent_slot: u8,
    status: OriginalCombatShotStatus,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginalCombatPedState {
    record_index: u16,
    tile: OriginalTilePoint,
    hp: i32,
    max_hp: i32,
    objective_target: bool,
    defeated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OriginalCombatTargetCandidate {
    record_index: u16,
    tile: OriginalTilePoint,
    objective_target: bool,
    role: OriginalCombatTargetRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OriginalCombatTargetRole {
    Objective,
    Civilian,
    NpcAgent,
    Police,
    Guard,
    Criminal,
    Unknown,
}

impl OriginalCombatTargetRole {
    fn from_ped_object(object: &OriginalMissionObjectCandidate, objective_target: bool) -> Self {
        if objective_target {
            return Self::Objective;
        }
        let role_value = object
            .type_value
            .filter(|value| *value != 0)
            .or_else(|| object.subtype_value.filter(|value| *value != 0));
        match role_value {
            Some(0x01) => Self::Civilian,
            Some(0x02) => Self::NpcAgent,
            Some(0x04) => Self::Police,
            Some(0x08) => Self::Guard,
            Some(0x10) => Self::Criminal,
            _ => Self::Unknown,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Objective => "objective target",
            Self::Civilian => "civilian",
            Self::NpcAgent => "agent candidate",
            Self::Police => "police",
            Self::Guard => "guard",
            Self::Criminal => "criminal",
            Self::Unknown => "ped candidate",
        }
    }

    fn overlay_label(self) -> &'static str {
        match self {
            Self::Objective => "TARGET",
            Self::Civilian => "CIV",
            Self::NpcAgent => "NPC AGENT",
            Self::Police => "POLICE",
            Self::Guard => "GUARD",
            Self::Criminal => "CRIM",
            Self::Unknown => "PED",
        }
    }

    fn reaction_label(self) -> Option<&'static str> {
        match self {
            Self::NpcAgent | Self::Police | Self::Guard | Self::Criminal | Self::Objective => {
                Some(self.label())
            }
            Self::Civilian | Self::Unknown => None,
        }
    }
}

impl OriginalHostileReactionState {
    fn from_target(target: OriginalCombatTargetCandidate) -> Self {
        Self {
            record_index: target.record_index,
            role: target.role,
            tile: target.tile,
            next_fire_secs: ORIGINAL_CONTROL_HOSTILE_REACTION_DELAY_SECS,
            shots: 0,
            blocked: 0,
        }
    }

    fn label(&self) -> String {
        format!(
            "{} rec {} shots {} blocked {}",
            self.role.label(),
            self.record_index,
            self.shots,
            self.blocked
        )
    }
}

#[derive(Debug, Clone)]
struct OriginalCombatFeedback {
    origins: Vec<OriginalTilePoint>,
    target_tile: OriginalTilePoint,
    status: OriginalCombatShotStatus,
    remaining: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OriginalCombatShotStatus {
    Ready,
    NoWeapon,
    OutOfRange,
    Blocked,
    AlreadyDown,
    Cooling,
    HostileReturn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OriginalCombatShotCheck {
    status: OriginalCombatShotStatus,
    distance: u16,
    range: u16,
    blocker_label: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct OriginalCombatWeaponProfile {
    kind: OriginalWeaponKind,
    label: &'static str,
    source: OriginalDebugAgentWeaponSource,
    range_tiles: u16,
    local_damage: i32,
    cooldown_secs: f32,
}

impl OriginalCombatWeaponProfile {
    fn from_hint(hint: OriginalDebugAgentWeaponHint) -> Option<Self> {
        let kind = hint.kind?;
        let mut profile = Self::from_kind(kind)?;
        profile.source = hint.source;
        Some(profile)
    }

    fn from_kind(kind: OriginalWeaponKind) -> Option<Self> {
        let (label, range_world, local_damage, reload_ms) = match kind {
            OriginalWeaponKind::Pistol => ("Pistol", 1280, 2, 600),
            OriginalWeaponKind::GaussGun => ("Gauss gun", 5120, 64, 1500),
            OriginalWeaponKind::Shotgun => ("Shotgun", 1024, 12, 200),
            OriginalWeaponKind::Uzi => ("Uzi", 1792, 2, 100),
            OriginalWeaponKind::Minigun => ("Minigun", 2304, 10, 75),
            OriginalWeaponKind::Laser => ("Laser", 4096, 32, 200),
            OriginalWeaponKind::Flamer => ("Flamer", 1152, 8, 50),
            OriginalWeaponKind::LongRange => ("Long range", 6144, 2, 400),
            OriginalWeaponKind::Persuadatron
            | OriginalWeaponKind::Scanner
            | OriginalWeaponKind::MediKit
            | OriginalWeaponKind::TimeBomb
            | OriginalWeaponKind::AccessCard
            | OriginalWeaponKind::EnergyShield => return None,
        };
        Some(Self {
            kind,
            label,
            source: OriginalDebugAgentWeaponSource::NoSupportedWeapon,
            range_tiles: range_tiles_from_freesynd_world_range(range_world),
            local_damage,
            cooldown_secs: ORIGINAL_CONTROL_SHOOT_REACTION_SECS + reload_ms as f32 / 1000.0,
        })
    }

    fn panel_label(self) -> String {
        format!(
            "{} range {} dmg {} via {}",
            self.label,
            self.range_tiles,
            self.local_damage,
            self.source.label()
        )
    }
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

    fn record_combat_probe(
        &mut self,
        record_index: u16,
        distance: u16,
        status: OriginalCombatShotStatus,
    ) {
        self.combat_probe_count += 1;
        let status_label = match status {
            OriginalCombatShotStatus::Ready => "ready",
            OriginalCombatShotStatus::NoWeapon => "no supported weapon",
            OriginalCombatShotStatus::OutOfRange => "out of range",
            OriginalCombatShotStatus::Blocked => "blocked",
            OriginalCombatShotStatus::AlreadyDown => "already down",
            OriginalCombatShotStatus::Cooling => "cooldown",
            OriginalCombatShotStatus::HostileReturn => "hostile return",
        };
        self.last_result = Some(format!(
            "combat check ped candidate {record_index} {status_label} at range {distance}; gated local hit state"
        ));
    }

    fn record_combat_hit(&mut self, label: String) {
        self.combat_hit_count += 1;
        self.last_result = Some(label);
    }

    fn panel_label(&self) -> String {
        let last = self
            .last_result
            .as_deref()
            .unwrap_or("no local action result yet");
        format!(
            "control runtime local results door {} pickup {} vehicle {} objective {} scenario {} combat probes {} hits {}; {last}",
            self.door_resolutions,
            self.weapon_pickup_resolutions,
            self.vehicle_entry_resolutions,
            self.objective_contact_resolutions,
            self.scenario_trigger_resolutions,
            self.combat_probe_count,
            self.combat_hit_count
        )
    }
}

impl OriginalMissionCombatRuntime {
    fn from_scene(scene_model: Option<&OriginalMissionScene>) -> Self {
        let Some(scene_model) = scene_model else {
            return Self::default();
        };
        let objective_target = scene_model.current_objective_runtime_target();
        let mut runtime = Self {
            objective_target,
            ..Self::default()
        };
        if let Some(target) = objective_target {
            if target.target_kind == Some(OriginalMissionObjectKind::Ped)
                && let (Some(record_index), Some(tile)) =
                    (target.target_record_index, target.target_tile)
            {
                runtime.ensure_ped_state(record_index, tile, true);
            }
        }
        runtime
    }

    fn ensure_objective_target(
        &mut self,
        objective_target: Option<OriginalObjectiveRuntimeTarget>,
    ) {
        if self.objective_target.is_none() {
            self.objective_target = objective_target;
        }
        if let Some(target) = self.objective_target {
            if target.target_kind == Some(OriginalMissionObjectKind::Ped)
                && let (Some(record_index), Some(tile)) =
                    (target.target_record_index, target.target_tile)
            {
                self.ensure_ped_state(record_index, tile, true);
            }
        }
    }

    fn ensure_ped_state(
        &mut self,
        record_index: u16,
        tile: OriginalTilePoint,
        objective_target: bool,
    ) -> &mut OriginalCombatPedState {
        self.peds
            .entry(record_index)
            .and_modify(|state| {
                state.tile = tile;
                state.objective_target |= objective_target;
            })
            .or_insert(OriginalCombatPedState {
                record_index,
                tile,
                hp: ORIGINAL_CONTROL_TARGET_HP,
                max_hp: ORIGINAL_CONTROL_TARGET_HP,
                objective_target,
                defeated: false,
            })
    }

    fn ped_state(&self, record_index: u16) -> Option<&OriginalCombatPedState> {
        self.peds.get(&record_index)
    }

    fn objective_target_state(&self) -> Option<&OriginalCombatPedState> {
        let target = self.objective_target?;
        let record_index = target.target_record_index?;
        self.peds.get(&record_index)
    }

    fn objective_target_record_index(&self) -> Option<u16> {
        self.objective_target?.target_record_index
    }

    fn objective_target_overlay(&self) -> Option<(OriginalTilePoint, String, bool, bool)> {
        let state = self.objective_target_state()?;
        let hp_label = if state.defeated {
            "down".to_string()
        } else {
            format!("HP {}/{}", state.hp, state.max_hp)
        };
        Some((
            state.tile,
            hp_label,
            self.objective_completed,
            state.defeated,
        ))
    }

    fn combat_target_overlay(&self) -> Option<(OriginalTilePoint, String, bool, bool)> {
        if let Some(overlay) = self.objective_target_overlay() {
            return Some(overlay);
        }
        let target = self.last_target?;
        let state = self.peds.get(&target.record_index)?;
        let hp_label = if state.defeated {
            format!("{} down", target.role.label())
        } else {
            format!("{} HP {}/{}", target.role.label(), state.hp, state.max_hp)
        };
        Some((state.tile, hp_label, false, state.defeated))
    }

    fn mark_target_candidate(&mut self, target: OriginalCombatTargetCandidate) {
        self.last_target = Some(target);
        self.ensure_ped_state(target.record_index, target.tile, target.objective_target);
        self.last_result = Some(format!(
            "target {} ped candidate {}; local combat only",
            target.role.label(),
            target.record_index
        ));
    }

    fn apply_hit(
        &mut self,
        target: OriginalCombatTargetCandidate,
        damage: i32,
    ) -> OriginalCombatAttackResult {
        let (defeated_now, remaining_hp, objective_target) = {
            let state =
                self.ensure_ped_state(target.record_index, target.tile, target.objective_target);
            if state.defeated {
                return OriginalCombatAttackResult::AlreadyDown;
            }
            state.hp = (state.hp - damage.max(0)).max(0);
            if state.hp == 0 {
                state.defeated = true;
            }
            (state.defeated, state.hp, state.objective_target)
        };
        self.shots_fired += 1;
        self.hits += 1;
        if defeated_now {
            self.defeated += 1;
            if objective_target
                && self
                    .objective_target
                    .is_some_and(|objective| objective.objective_kind_label == "assassinate")
            {
                self.objective_completed = true;
            }
            OriginalCombatAttackResult::Defeated {
                objective_completed: self.objective_completed && objective_target,
            }
        } else {
            OriginalCombatAttackResult::Hit { remaining_hp }
        }
    }

    fn record_out_of_range(
        &mut self,
        target: OriginalCombatTargetCandidate,
        distance: u16,
        range: u16,
    ) {
        self.ensure_ped_state(target.record_index, target.tile, target.objective_target);
        self.out_of_range += 1;
        self.last_result = Some(format!(
            "{} ped candidate {} out of range {distance}/{}; local combat state unchanged",
            target.role.label(),
            target.record_index,
            range
        ));
    }

    fn record_blocked(&mut self, target: OriginalCombatTargetCandidate, reason: &str) {
        self.ensure_ped_state(target.record_index, target.tile, target.objective_target);
        self.blocked += 1;
        self.last_result = Some(format!(
            "{} ped candidate {} blocked: {reason}; local combat state unchanged",
            target.role.label(),
            target.record_index
        ));
    }

    fn record_result(
        &mut self,
        target: OriginalCombatTargetCandidate,
        result: OriginalCombatAttackResult,
    ) -> String {
        let label = match result {
            OriginalCombatAttackResult::Hit { remaining_hp } => format!(
                "original combat: {} ped candidate {} hit; {remaining_hp} HP remaining; local combat only",
                target.role.label(),
                target.record_index
            ),
            OriginalCombatAttackResult::Defeated {
                objective_completed: true,
            } => format!(
                "original combat: objective target ped candidate {} defeated; local objective complete",
                target.record_index
            ),
            OriginalCombatAttackResult::Defeated {
                objective_completed: false,
            } => format!(
                "original combat: {} ped candidate {} defeated; local combat only",
                target.role.label(),
                target.record_index
            ),
            OriginalCombatAttackResult::AlreadyDown => format!(
                "original combat: {} ped candidate {} already down; local state unchanged",
                target.role.label(),
                target.record_index
            ),
        };
        self.last_result = Some(label.clone());
        label
    }

    fn record_npc_reaction(&mut self, target: OriginalCombatTargetCandidate) -> Option<String> {
        let role = target.role.reaction_label()?;
        if self
            .peds
            .get(&target.record_index)
            .is_some_and(|state| state.defeated)
        {
            return None;
        }
        self.npc_reactions += 1;
        self.hostile_reactions
            .entry(target.record_index)
            .and_modify(|reaction| {
                reaction.tile = target.tile;
                reaction.next_fire_secs = reaction
                    .next_fire_secs
                    .min(ORIGINAL_CONTROL_HOSTILE_REACTION_DELAY_SECS);
            })
            .or_insert_with(|| OriginalHostileReactionState::from_target(target));
        let label = format!(
            "{} ped candidate {} alerted locally; return-fire remains debug-gated",
            role, target.record_index
        );
        self.last_result = Some(label.clone());
        Some(label)
    }

    fn update_hostile_reactions(
        &mut self,
        real_dt: f32,
        agents: &[OriginalDebugAgent],
        scene_model: &OriginalMissionScene,
    ) -> Vec<OriginalHostileFireEvent> {
        if agents.is_empty() || self.hostile_reactions.is_empty() {
            return Vec::new();
        }
        let pistol = OriginalCombatWeaponProfile::from_kind(OriginalWeaponKind::Pistol)
            .expect("pistol profile");
        let mut events = Vec::new();
        let mut remove = Vec::new();
        let keys = self.hostile_reactions.keys().copied().collect::<Vec<_>>();
        for key in keys {
            if self.peds.get(&key).is_some_and(|state| state.defeated) {
                remove.push(key);
                continue;
            }
            let Some(reaction) = self.hostile_reactions.get_mut(&key) else {
                continue;
            };
            reaction.next_fire_secs -= real_dt.max(0.0);
            if reaction.next_fire_secs > 0.0 {
                continue;
            }
            let Some((target_agent_slot, target_tile)) = agents
                .iter()
                .map(|agent| {
                    (
                        agent.slot,
                        agent.current_tile(),
                        original_tile_distance(reaction.tile, agent.current_tile()),
                    )
                })
                .min_by_key(|(_, _, distance)| *distance)
                .map(|(slot, tile, _)| (slot, tile))
            else {
                continue;
            };
            let line_probe =
                scene_model.original_combat_line_probe_between(reaction.tile, target_tile);
            let check =
                original_hostile_return_fire_check(reaction.tile, target_tile, pistol, &line_probe);
            match check.status {
                OriginalCombatShotStatus::Ready => {
                    reaction.shots += 1;
                    reaction.next_fire_secs = ORIGINAL_CONTROL_HOSTILE_RELOAD_SECS;
                    self.hostile_return_fire += 1;
                    let label = format!(
                        "{} ped candidate {} returned fire at agent {}; local threat marker only",
                        reaction.role.label(),
                        reaction.record_index,
                        target_agent_slot + 1
                    );
                    self.last_result = Some(label.clone());
                    events.push(OriginalHostileFireEvent {
                        origin: reaction.tile,
                        target: target_tile,
                        target_agent_slot,
                        status: OriginalCombatShotStatus::HostileReturn,
                        label,
                    });
                }
                OriginalCombatShotStatus::OutOfRange | OriginalCombatShotStatus::Blocked => {
                    reaction.blocked += 1;
                    reaction.next_fire_secs = ORIGINAL_CONTROL_HOSTILE_RELOAD_SECS * 0.75;
                    self.hostile_reaction_blocked += 1;
                    self.last_result = Some(format!(
                        "{} ped candidate {} reaction blocked by {}; local AI remains gated",
                        reaction.role.label(),
                        reaction.record_index,
                        check.blocker_label
                    ));
                }
                _ => {
                    reaction.next_fire_secs = ORIGINAL_CONTROL_HOSTILE_RELOAD_SECS;
                }
            }
        }
        for key in remove {
            self.hostile_reactions.remove(&key);
        }
        events
    }

    fn panel_label(&self) -> String {
        let objective = self
            .objective_target
            .map(|target| {
                format!(
                    "{} target {} rec {}",
                    target.objective_kind_label,
                    target.target_bucket_label,
                    target
                        .target_record_index
                        .map(|idx| idx.to_string())
                        .unwrap_or_else(|| "none".to_string())
                )
            })
            .unwrap_or_else(|| "objective target none".to_string());
        let hp = self
            .objective_target_state()
            .map(|target| {
                if target.defeated {
                    "down".to_string()
                } else {
                    format!("{}/{}", target.hp, target.max_hp)
                }
            })
            .unwrap_or_else(|| "unknown".to_string());
        let progress = if self.objective_completed {
            "objective local-complete"
        } else {
            "objective pending"
        };
        let hostile = if self.hostile_reactions.is_empty() {
            "hostile reactions none".to_string()
        } else {
            let first = self
                .hostile_reactions
                .values()
                .next()
                .map(OriginalHostileReactionState::label)
                .unwrap_or_else(|| "hostile reactions active".to_string());
            format!("hostiles {} active; {first}", self.hostile_reactions.len())
        };
        let last = self
            .last_result
            .as_deref()
            .unwrap_or("no local combat result yet");
        format!(
            "combat local {objective} hp {hp}; shots {} hits {} down {} oor {} blocked {} react {} return {} rb {}; {progress}; {hostile}; {last}",
            self.shots_fired,
            self.hits,
            self.defeated,
            self.out_of_range,
            self.blocked,
            self.npc_reactions,
            self.hostile_return_fire,
            self.hostile_reaction_blocked
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OriginalCombatAttackResult {
    Hit { remaining_hp: i32 },
    Defeated { objective_completed: bool },
    AlreadyDown,
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

impl OriginalCombatFeedback {
    fn new(
        origins: Vec<OriginalTilePoint>,
        target_tile: OriginalTilePoint,
        status: OriginalCombatShotStatus,
    ) -> Self {
        Self {
            origins,
            target_tile,
            status,
            remaining: ORIGINAL_CONTROL_COMBAT_FEEDBACK_SECS,
        }
    }

    fn update(&mut self, real_dt: f32) {
        self.remaining = (self.remaining - real_dt.max(0.0)).max(0.0);
    }

    fn is_alive(&self) -> bool {
        self.remaining > 0.0
    }

    fn fade(&self) -> f32 {
        (self.remaining / ORIGINAL_CONTROL_COMBAT_FEEDBACK_SECS).clamp(0.0, 1.0)
    }

    fn color(&self) -> Color {
        match self.status {
            OriginalCombatShotStatus::Ready => Color::new(0.0, 0.95, 1.0, 0.90),
            OriginalCombatShotStatus::NoWeapon => Color::new(0.55, 0.55, 0.62, 0.76),
            OriginalCombatShotStatus::OutOfRange => Color::new(1.0, 0.65, 0.05, 0.82),
            OriginalCombatShotStatus::Blocked => Color::new(1.0, 0.15, 0.10, 0.82),
            OriginalCombatShotStatus::AlreadyDown => Color::new(0.70, 0.70, 0.75, 0.76),
            OriginalCombatShotStatus::Cooling => Color::new(0.95, 0.85, 0.20, 0.76),
            OriginalCombatShotStatus::HostileReturn => Color::new(1.0, 0.05, 0.18, 0.88),
        }
    }

    fn label(&self) -> &'static str {
        match self.status {
            OriginalCombatShotStatus::Ready => "SHOT",
            OriginalCombatShotStatus::NoWeapon => "NO WEAPON",
            OriginalCombatShotStatus::OutOfRange => "RANGE",
            OriginalCombatShotStatus::Blocked => "BLOCKED",
            OriginalCombatShotStatus::AlreadyDown => "DOWN",
            OriginalCombatShotStatus::Cooling => "COOLDOWN",
            OriginalCombatShotStatus::HostileReturn => "RETURN",
        }
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
        let original_combat_runtime =
            OriginalMissionCombatRuntime::from_scene(original_mission_scene.as_ref());
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
            original_combat_runtime,
            original_combat_feedback: None,
            original_hover_target: None,
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
        self.update_original_hostile_reactions(real_dt);
        self.update_original_combat_feedback(real_dt);
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
        if is_key_pressed(KeyCode::Q) {
            self.try_cycle_original_debug_agent_weapons();
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
            self.original_hover_target = None;
            return;
        }

        let (Some(map_tiles), Some(graphics)) = (
            self.original_map_tiles.as_ref(),
            self.original_graphics.as_ref(),
        ) else {
            self.original_cursor_tile = None;
            self.original_cursor_screen = None;
            self.original_hover_target = None;
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
        self.original_hover_target = self.original_cursor_tile.and_then(|cursor| {
            self.original_mission_scene
                .as_ref()
                .and_then(|scene_model| {
                    let objective_target = scene_model.current_objective_runtime_target();
                    self.original_combat_target_at_cursor(scene_model, cursor, objective_target)
                })
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

    fn update_original_hostile_reactions(&mut self, real_dt: f32) {
        if !self.original_navigation_debug_enabled
            || self.render_mode != MapRenderMode::OriginalMissionSceneProbe
        {
            return;
        }
        let Some(scene_model) = self.original_mission_scene.as_ref() else {
            return;
        };
        let events = self.original_combat_runtime.update_hostile_reactions(
            real_dt,
            &self.original_debug_agents,
            scene_model,
        );
        for event in events {
            if let Some(agent) = self
                .original_debug_agents
                .iter_mut()
                .find(|agent| agent.slot == event.target_agent_slot)
            {
                agent.mark_under_fire();
            }
            self.original_combat_feedback = Some(OriginalCombatFeedback::new(
                vec![event.origin],
                event.target,
                event.status,
            ));
            self.combat_log = event.label;
        }
    }

    fn update_original_combat_feedback(&mut self, real_dt: f32) {
        if let Some(feedback) = &mut self.original_combat_feedback {
            feedback.update(real_dt);
            if !feedback.is_alive() {
                self.original_combat_feedback = None;
            }
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

    fn try_cycle_original_debug_agent_weapons(&mut self) -> bool {
        if self.render_mode != MapRenderMode::OriginalMissionSceneProbe
            || !self.original_navigation_debug_enabled
        {
            return false;
        }
        self.ensure_original_debug_agents();
        let selected_agents = self.selected_original_debug_agent_indices();
        if selected_agents.is_empty() {
            self.combat_log =
                "Original weapon cycle blocked: no selected original agent".to_string();
            return true;
        }

        let mut cycled = 0;
        let mut blocked = 0;
        let mut primary_label = None;
        for idx in selected_agents {
            let Some(agent) = self.original_debug_agents.get_mut(idx) else {
                continue;
            };
            match agent.cycle_weapon() {
                Some(weapon) => {
                    cycled += 1;
                    if primary_label.is_none() || idx == self.selected_original_debug_agent {
                        primary_label = Some(format!(
                            "agent {} selected {} via {}",
                            agent.slot + 1,
                            weapon.label,
                            weapon.source.label()
                        ));
                    }
                }
                None => {
                    blocked += 1;
                    if primary_label.is_none() || idx == self.selected_original_debug_agent {
                        primary_label = Some(format!(
                            "agent {} has no supported weapon; inventory semantics blocked",
                            agent.slot + 1
                        ));
                    }
                }
            }
        }
        self.combat_log = format!(
            "Original weapon selection: cycled {} blocked {}; {}; local control only",
            cycled,
            blocked,
            primary_label.unwrap_or_else(|| "no selected weapon changed".to_string())
        );
        true
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
                let weapon = agent
                    .selected_weapon()
                    .map(|weapon| weapon.label)
                    .unwrap_or("unarmed");
                format!(
                    "a{}{} rec{} {} {} tile={},{},{} route={}/{}",
                    agent.slot + 1,
                    selected,
                    agent.record_index,
                    agent.route_status.label(),
                    weapon,
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

    fn draw_original_ped_candidate_role_overlays(
        &self,
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
        scene_model: &OriginalMissionScene,
        controlled_ped_record_indices: &[u16],
    ) {
        let objective_target_record = self.original_combat_runtime.objective_target_record_index();
        for object in scene_model.objects.iter().filter(|object| {
            object.kind == OriginalMissionObjectKind::Ped
                && object.candidate_draw
                && !controlled_ped_record_indices.contains(&object.record_index)
        }) {
            let Some(tile) = object.tile else {
                continue;
            };
            let is_target = objective_target_record == Some(object.record_index);
            let defeated = self
                .original_combat_runtime
                .ped_state(object.record_index)
                .is_some_and(|state| state.defeated);
            let (label, color) = original_ped_candidate_role_style(object, is_target, defeated);
            self.map.draw_original_ped_candidate_overlay(
                &self.camera,
                map_tiles,
                graphics,
                tile,
                label,
                color,
                defeated,
            );
        }
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
            "original agents {}; selected set {} primary {} at {},{},{}; weapon {}; route nodes {}; moving {} blocked {}; intents q/r {}/{} primary {}; {}; {}; demo gameplay available",
            self.original_debug_agents.len(),
            selected_count,
            agent.slot + 1,
            agent.current_tile().tile_x,
            agent.current_tile().tile_y,
            agent.current_tile().tile_z,
            agent.weapon_status_label(),
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
        let selected_agents = self.selected_original_debug_agent_indices();
        if selected_agents.is_empty() {
            self.combat_log =
                "Original combat probe blocked: no selected original agent".to_string();
            return true;
        }
        let Some(scene_model) = self.original_mission_scene.as_ref() else {
            self.combat_log = "Original combat probe blocked: scene model unavailable".to_string();
            return true;
        };
        let objective_target = scene_model.current_objective_runtime_target();
        self.original_combat_runtime
            .ensure_objective_target(objective_target);
        let Some(target) =
            self.original_combat_target_at_cursor(scene_model, cursor, objective_target)
        else {
            self.combat_log =
                "Original combat probe: no candidate non-squad ped at cursor".to_string();
            return true;
        };
        self.original_combat_runtime.mark_target_candidate(target);

        let mut fired = 0;
        let mut out_of_range = 0;
        let mut blocked = 0;
        let mut cooling = 0;
        let mut already_down = 0;
        let mut primary_label = None;
        let mut feedback_origins = Vec::new();
        let mut feedback_status = None;
        for idx in selected_agents.iter().copied() {
            let Some(agent) = self.original_debug_agents.get(idx) else {
                continue;
            };
            let agent_tile = agent.current_tile();
            let agent_slot = agent.slot;
            let agent_can_fire = agent.can_fire();
            let weapon = agent.selected_weapon();
            let line_probe =
                scene_model.original_combat_line_probe_between(agent_tile, target.tile);
            let check = original_combat_shot_check(
                agent_tile,
                target.tile,
                self.original_combat_runtime.ped_state(target.record_index),
                agent_can_fire,
                weapon,
                &line_probe,
            );
            self.original_control_runtime.record_combat_probe(
                target.record_index,
                check.distance,
                check.status,
            );
            if check.status == OriginalCombatShotStatus::Ready || feedback_origins.is_empty() {
                feedback_origins.push(agent_tile);
                feedback_status = Some(check.status);
            }
            match check.status {
                OriginalCombatShotStatus::Ready => {
                    let Some(weapon) = weapon else {
                        continue;
                    };
                    if let Some(agent) = self.original_debug_agents.get_mut(idx) {
                        agent.mark_fired(weapon.cooldown_secs);
                    }
                    let result = self
                        .original_combat_runtime
                        .apply_hit(target, weapon.local_damage);
                    let label = self.original_combat_runtime.record_result(target, result);
                    let reaction = self.original_combat_runtime.record_npc_reaction(target);
                    self.original_control_runtime
                        .record_combat_hit(label.clone());
                    primary_label = Some(format!(
                        "agent {} fired {}; {label}; {}; range {}/{}",
                        agent_slot + 1,
                        weapon.label,
                        reaction.unwrap_or_else(|| "no local NPC reaction".to_string()),
                        check.distance,
                        check.range
                    ));
                    fired += 1;
                }
                OriginalCombatShotStatus::NoWeapon => {
                    primary_label.get_or_insert_with(|| {
                        format!("agent {} has no supported combat weapon", agent_slot + 1)
                    });
                    blocked += 1;
                }
                OriginalCombatShotStatus::OutOfRange => {
                    self.original_combat_runtime.record_out_of_range(
                        target,
                        check.distance,
                        check.range,
                    );
                    primary_label.get_or_insert_with(|| {
                        format!(
                            "agent {} out of range {}/{}",
                            agent_slot + 1,
                            check.distance,
                            check.range
                        )
                    });
                    out_of_range += 1;
                }
                OriginalCombatShotStatus::Blocked => {
                    self.original_combat_runtime
                        .record_blocked(target, check.blocker_label);
                    primary_label.get_or_insert_with(|| {
                        format!(
                            "agent {} blocked by {}; {}",
                            agent_slot + 1,
                            check.blocker_label,
                            line_probe.panel_label()
                        )
                    });
                    blocked += 1;
                }
                OriginalCombatShotStatus::AlreadyDown => {
                    let label = self
                        .original_combat_runtime
                        .record_result(target, OriginalCombatAttackResult::AlreadyDown);
                    primary_label.get_or_insert(label);
                    already_down += 1;
                }
                OriginalCombatShotStatus::Cooling => {
                    primary_label
                        .get_or_insert_with(|| format!("agent {} weapon cooling", agent_slot + 1));
                    cooling += 1;
                }
                OriginalCombatShotStatus::HostileReturn => {
                    primary_label.get_or_insert_with(|| {
                        "hostile return-fire is resolved by the local reaction loop".to_string()
                    });
                }
            }
        }
        self.combat_log = format!(
            "Original combat local: selected {} fired {} cooldown {} out {} blocked {} down {}; {}; full blocker/AI/mission semantics gated",
            selected_agents.len(),
            fired,
            cooling,
            out_of_range,
            blocked,
            already_down,
            primary_label.unwrap_or_else(|| "no selected agent could fire".to_string())
        );
        if !feedback_origins.is_empty() {
            self.original_combat_feedback = Some(OriginalCombatFeedback::new(
                feedback_origins,
                target.tile,
                if fired > 0 {
                    OriginalCombatShotStatus::Ready
                } else {
                    feedback_status.unwrap_or(OriginalCombatShotStatus::Blocked)
                },
            ));
        }
        true
    }

    fn original_combat_target_at_cursor(
        &self,
        scene_model: &OriginalMissionScene,
        cursor: OriginalTilePoint,
        objective_target: Option<OriginalObjectiveRuntimeTarget>,
    ) -> Option<OriginalCombatTargetCandidate> {
        if let Some(target) = objective_target {
            if target.target_kind == Some(OriginalMissionObjectKind::Ped)
                && let (Some(record_index), Some(tile)) =
                    (target.target_record_index, target.target_tile)
                && original_tile_near(tile, cursor, ORIGINAL_COMBAT_TARGET_PICK_RADIUS, 1)
            {
                return Some(OriginalCombatTargetCandidate {
                    record_index,
                    tile,
                    objective_target: true,
                    role: OriginalCombatTargetRole::Objective,
                });
            }
        }

        let squad_records = self
            .original_debug_agents
            .iter()
            .map(|agent| agent.record_index)
            .collect::<BTreeSet<_>>();
        scene_model
            .objects
            .iter()
            .filter(|object| {
                object.kind == OriginalMissionObjectKind::Ped
                    && object.candidate_draw
                    && !squad_records.contains(&object.record_index)
            })
            .filter_map(|object| {
                let tile = object.tile?;
                let objective_target = objective_target.is_some_and(|target| {
                    target.target_kind == Some(OriginalMissionObjectKind::Ped)
                        && target.target_record_index == Some(object.record_index)
                });
                Some((
                    object.record_index,
                    tile,
                    OriginalCombatTargetRole::from_ped_object(object, objective_target),
                    objective_target,
                ))
            })
            .filter(|(record_index, tile, _, _)| {
                original_tile_near(*tile, cursor, ORIGINAL_COMBAT_TARGET_PICK_RADIUS, 1)
                    && !self
                        .original_combat_runtime
                        .ped_state(*record_index)
                        .is_some_and(|state| state.defeated)
            })
            .min_by_key(|(_, tile, _, _)| original_tile_distance(cursor, *tile))
            .map(
                |(record_index, tile, role, objective_target)| OriginalCombatTargetCandidate {
                    record_index,
                    tile,
                    objective_target,
                    role,
                },
            )
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
                    self.draw_original_ped_candidate_role_overlays(
                        map_tiles,
                        graphics,
                        scene_model,
                        &controlled_ped_record_indices,
                    );
                    if let Some((target_tile, hp_label, objective_complete, defeated)) =
                        self.original_combat_runtime.combat_target_overlay()
                    {
                        self.map.draw_original_combat_target_overlay(
                            &self.camera,
                            map_tiles,
                            graphics,
                            target_tile,
                            &hp_label,
                            objective_complete,
                            defeated,
                        );
                    }
                    if let Some(target) = self.original_hover_target {
                        let label = format!("AIM {}", target.role.overlay_label());
                        self.map.draw_original_combat_hover_overlay(
                            &self.camera,
                            map_tiles,
                            graphics,
                            target.tile,
                            &label,
                            target.role.reaction_label().is_some(),
                        );
                    }
                    if let Some(feedback) = &self.original_combat_feedback {
                        self.map.draw_original_combat_feedback_overlay(
                            &self.camera,
                            map_tiles,
                            graphics,
                            &feedback.origins,
                            feedback.target_tile,
                            feedback.label(),
                            feedback.color(),
                            feedback.fade(),
                        );
                    }
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
                                agent.is_under_fire(),
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
                &self.original_combat_runtime.panel_label(),
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
    #[cfg(test)]
    fn from_spawn(spawn: OriginalDebugAgentSpawn, selected: bool) -> Self {
        Self::from_spawn_with_weapons(
            spawn,
            selected,
            vec![OriginalDebugAgentWeaponHint::player_fallback_pistol()],
        )
    }

    fn from_spawn_with_weapons(
        spawn: OriginalDebugAgentSpawn,
        selected: bool,
        weapon_hints: Vec<OriginalDebugAgentWeaponHint>,
    ) -> Self {
        let weapons = weapon_hints
            .into_iter()
            .filter_map(OriginalCombatWeaponProfile::from_hint)
            .collect::<Vec<_>>();
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
            weapon_cooldown: 0.0,
            weapons,
            selected_weapon_index: 0,
            under_fire_remaining: 0.0,
            local_threat_marks: 0,
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
        self.weapon_cooldown = (self.weapon_cooldown - real_dt.max(0.0)).max(0.0);
        self.under_fire_remaining = (self.under_fire_remaining - real_dt.max(0.0)).max(0.0);
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

    fn can_fire(&self) -> bool {
        self.weapon_cooldown <= 0.0
    }

    fn mark_fired(&mut self, cooldown_secs: f32) {
        self.weapon_cooldown = cooldown_secs.max(0.05);
    }

    fn mark_under_fire(&mut self) {
        self.under_fire_remaining = ORIGINAL_CONTROL_AGENT_UNDER_FIRE_SECS;
        self.local_threat_marks = self.local_threat_marks.saturating_add(1);
    }

    fn is_under_fire(&self) -> bool {
        self.under_fire_remaining > 0.0
    }

    fn selected_weapon(&self) -> Option<OriginalCombatWeaponProfile> {
        self.weapons
            .get(self.selected_weapon_index)
            .copied()
            .or_else(|| self.weapons.first().copied())
    }

    fn cycle_weapon(&mut self) -> Option<OriginalCombatWeaponProfile> {
        if self.weapons.is_empty() {
            return None;
        }
        self.selected_weapon_index = (self.selected_weapon_index + 1) % self.weapons.len();
        self.selected_weapon()
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

    fn weapon_label(&self) -> String {
        self.selected_weapon()
            .map(OriginalCombatWeaponProfile::panel_label)
            .map(|label| {
                if self.weapons.len() > 1 {
                    format!(
                        "{label} [{}/{}]",
                        self.selected_weapon_index + 1,
                        self.weapons.len()
                    )
                } else {
                    label
                }
            })
            .unwrap_or_else(|| "no supported weapon; inventory semantics blocked".to_string())
    }

    fn weapon_status_label(&self) -> String {
        let cooldown = if self.weapon_cooldown > 0.0 {
            format!("cooldown {:.1}s", self.weapon_cooldown)
        } else {
            "ready".to_string()
        };
        let threat = if self.local_threat_marks > 0 {
            format!("; local threat marks {}", self.local_threat_marks)
        } else {
            String::new()
        };
        format!("{}; {cooldown}{threat}", self.weapon_label())
    }

    fn map_label(&self) -> String {
        let selected = if self.selected { "selected" } else { "debug" };
        let weapon = self
            .selected_weapon()
            .map(|weapon| weapon.label)
            .unwrap_or("unarmed");
        format!(
            "{selected} agent {} {} {}{}{}",
            self.slot + 1,
            self.route_status.label(),
            weapon,
            if self.is_under_fire() {
                " UNDER FIRE"
            } else {
                ""
            },
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
        .map(|(idx, spawn)| {
            let weapon_hints = scene_model.debug_agent_weapon_hints(spawn.record_index);
            OriginalDebugAgent::from_spawn_with_weapons(spawn, idx == 0, weapon_hints)
        })
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

fn range_tiles_from_freesynd_world_range(range_world: u16) -> u16 {
    range_world.div_ceil(256).max(1)
}

fn original_tile_near(a: OriginalTilePoint, b: OriginalTilePoint, xy: u16, z: u16) -> bool {
    a.tile_x.abs_diff(b.tile_x) + a.tile_y.abs_diff(b.tile_y) <= xy
        && a.tile_z.abs_diff(b.tile_z) <= z
}

fn original_combat_shot_check(
    agent_tile: OriginalTilePoint,
    target_tile: OriginalTilePoint,
    target_state: Option<&OriginalCombatPedState>,
    agent_can_fire: bool,
    weapon: Option<OriginalCombatWeaponProfile>,
    line_probe: &OriginalCombatLineProbe,
) -> OriginalCombatShotCheck {
    let distance = original_tile_distance(agent_tile, target_tile);
    let range = weapon.map(|weapon| weapon.range_tiles).unwrap_or_default();
    let (status, blocker_label) = if target_state.is_some_and(|state| state.defeated) {
        (OriginalCombatShotStatus::AlreadyDown, "target already down")
    } else if weapon.is_none() {
        (OriginalCombatShotStatus::NoWeapon, "no supported weapon")
    } else if !agent_can_fire {
        (OriginalCombatShotStatus::Cooling, "weapon cooling")
    } else if distance > range {
        (OriginalCombatShotStatus::OutOfRange, "out of range")
    } else if agent_tile.tile_z.abs_diff(target_tile.tile_z) > 1 {
        (
            OriginalCombatShotStatus::Blocked,
            "unproven height transition",
        )
    } else if !line_probe.is_clear() {
        (OriginalCombatShotStatus::Blocked, line_probe.blocker_label)
    } else {
        (OriginalCombatShotStatus::Ready, "candidate line clear")
    };
    OriginalCombatShotCheck {
        status,
        distance,
        range,
        blocker_label,
    }
}

fn original_hostile_return_fire_check(
    hostile_tile: OriginalTilePoint,
    agent_tile: OriginalTilePoint,
    weapon: OriginalCombatWeaponProfile,
    line_probe: &OriginalCombatLineProbe,
) -> OriginalCombatShotCheck {
    original_combat_shot_check(
        hostile_tile,
        agent_tile,
        None,
        true,
        Some(weapon),
        line_probe,
    )
}

fn original_ped_candidate_role_style(
    object: &OriginalMissionObjectCandidate,
    objective_target: bool,
    defeated: bool,
) -> (&'static str, Color) {
    if defeated {
        return ("DOWN", Color::new(0.70, 0.70, 0.75, 0.76));
    }
    if objective_target {
        return ("TARGET", Color::new(1.0, 0.10, 0.06, 0.90));
    }
    let role_value = object
        .type_value
        .filter(|value| *value != 0)
        .or_else(|| object.subtype_value.filter(|value| *value != 0));
    match role_value {
        Some(0x01) => ("CIV", Color::new(0.72, 0.78, 0.82, 0.66)),
        Some(0x02) => ("NPC AGENT", Color::new(1.0, 0.62, 0.05, 0.78)),
        Some(0x04) => ("POLICE", Color::new(0.25, 0.70, 1.0, 0.78)),
        Some(0x08) => ("GUARD", Color::new(1.0, 0.78, 0.08, 0.78)),
        Some(0x10) => ("CRIM", Color::new(1.0, 0.28, 0.16, 0.78)),
        _ => ("NPC", Color::new(0.72, 0.78, 0.82, 0.60)),
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
    original_combat_runtime_label: &str,
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
                    original_combat_runtime_label,
                    x + 16.0,
                    y + 564.0,
                    10.5,
                    if original_navigation_debug_enabled {
                        ORANGE
                    } else {
                        GRAY
                    },
                );
                draw_text(
                    "original control is gated/local; demo grid remains available",
                    x + 16.0,
                    y + 584.0,
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
                y + 606.0,
                12.0,
                YELLOW,
            );
            draw_text(
                "Gameplay/pathfinding remain on the demo tactical grid",
                x + 16.0,
                y + 624.0,
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
        MapRenderMode::OriginalMissionSceneProbe => 648.0,
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
        MapRenderMode, OriginalCombatAttackResult, OriginalCombatFeedback, OriginalCombatPedState,
        OriginalCombatShotStatus, OriginalCombatTargetCandidate, OriginalCombatTargetRole,
        OriginalCombatWeaponProfile, OriginalDebugActionStatus, OriginalDebugAgent,
        OriginalDebugAgentDirection, OriginalDebugAgentRouteStatus, OriginalDebugAgentSpawn,
        OriginalMissionCombatRuntime, OriginalMissionControlRuntime, initial_render_mode,
        original_agent_focus_camera_offset_from_tile_size,
        original_agent_focus_world_point_from_tile_size, original_combat_shot_check,
        original_hostile_return_fire_check, original_ped_candidate_role_style,
        range_tiles_from_freesynd_world_range,
    };
    use crate::engine::{
        map_tiles::OriginalMapTiles,
        mission_scene::{
            OriginalAnimationRefs, OriginalCombatLineProbe, OriginalCombatLineStatus,
            OriginalDebugAgentWeaponHint, OriginalDebugAgentWeaponSource,
            OriginalDebugInteractionFocus, OriginalDebugInteractionIntent,
            OriginalDebugInteractionIntentStatus, OriginalDrawStage,
            OriginalMissionObjectCandidate, OriginalMissionObjectKind,
            OriginalObjectiveRuntimeTarget, OriginalRouteTransitionKind, OriginalRuntimeRouteProbe,
            OriginalRuntimeRouteStatus, OriginalTilePoint, OriginalWeaponKind,
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

    fn clear_line() -> OriginalCombatLineProbe {
        OriginalCombatLineProbe {
            status: OriginalCombatLineStatus::CandidateClear,
            checked_tiles: 0,
            blocker_tile: None,
            blocker_label: "candidate line clear",
        }
    }

    fn ped_object(type_value: u8, subtype_value: u8) -> OriginalMissionObjectCandidate {
        OriginalMissionObjectCandidate {
            kind: OriginalMissionObjectKind::Ped,
            record_index: 9,
            desc: Some(0x04),
            state: Some(0),
            type_value: Some(type_value),
            subtype_value: Some(subtype_value),
            orientation: Some(0),
            tile: Some(tile(4, 5, 0)),
            queue_tile: Some(tile(4, 5, 0)),
            animation: OriginalAnimationRefs {
                base_anim: Some(0),
                current_anim: Some(0),
                current_frame: Some(0),
            },
            candidate_record: true,
            candidate_draw: true,
            draw_stage: Some(OriginalDrawStage::People),
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
        runtime.record_combat_probe(9, 7, OriginalCombatShotStatus::Ready);

        let label = runtime.panel_label();
        assert!(label.contains("pickup 1"));
        assert!(label.contains("combat probes 1"));
        assert!(label.contains("gated local hit state"));
        assert!(!label.contains("0x"));
        assert!(!label.contains("00 00"));
    }

    #[test]
    fn original_combat_runtime_completes_assassinate_target_locally() {
        let target_tile = tile(8, 9, 0);
        let laser = OriginalCombatWeaponProfile::from_kind(OriginalWeaponKind::Laser).unwrap();
        let objective = OriginalObjectiveRuntimeTarget {
            objective_index: 0,
            objective_kind_label: "assassinate",
            target_bucket_label: "ped",
            target_kind: Some(OriginalMissionObjectKind::Ped),
            target_record_index: Some(12),
            target_tile: Some(target_tile),
        };
        let mut runtime = OriginalMissionCombatRuntime::default();
        runtime.ensure_objective_target(Some(objective));
        let candidate = OriginalCombatTargetCandidate {
            record_index: 12,
            tile: target_tile,
            objective_target: true,
            role: OriginalCombatTargetRole::Objective,
        };

        assert_eq!(
            runtime.apply_hit(candidate, laser.local_damage),
            OriginalCombatAttackResult::Hit { remaining_hp: 18 }
        );
        assert_eq!(
            runtime.apply_hit(candidate, laser.local_damage),
            OriginalCombatAttackResult::Defeated {
                objective_completed: true
            }
        );

        let overlay = runtime.objective_target_overlay().expect("target overlay");
        assert_eq!(overlay.0, target_tile);
        assert_eq!(overlay.1, "down");
        assert!(overlay.2);
        assert!(overlay.3);
        assert!(runtime.panel_label().contains("objective local-complete"));
        assert!(!runtime.panel_label().contains("0x"));
        assert!(!runtime.panel_label().contains("00 00"));
    }

    #[test]
    fn original_combat_runtime_tracks_target_overlay_and_npc_reaction_candidate() {
        let target_tile = tile(6, 7, 0);
        let mut runtime = OriginalMissionCombatRuntime::default();
        let candidate = OriginalCombatTargetCandidate {
            record_index: 9,
            tile: target_tile,
            objective_target: false,
            role: OriginalCombatTargetRole::Guard,
        };

        runtime.mark_target_candidate(candidate);
        let overlay = runtime.combat_target_overlay().expect("target overlay");
        assert_eq!(overlay.0, target_tile);
        assert!(overlay.1.contains("guard"));
        let reaction = runtime
            .record_npc_reaction(candidate)
            .expect("hostile reaction");

        assert!(reaction.contains("alerted locally"));
        assert!(runtime.panel_label().contains("react 1"));
        assert!(runtime.panel_label().contains("hostiles 1 active"));
        assert!(!runtime.panel_label().contains("0x"));
        assert!(!runtime.panel_label().contains("00 00"));
    }

    #[test]
    fn original_hostile_return_fire_check_uses_shared_line_and_range_gates() {
        let guard = tile(5, 5, 0);
        let agent = tile(8, 5, 0);
        let pistol = OriginalCombatWeaponProfile::from_kind(OriginalWeaponKind::Pistol).unwrap();
        let clear = clear_line();
        let ready = original_hostile_return_fire_check(guard, agent, pistol, &clear);

        assert_eq!(ready.status, OriginalCombatShotStatus::Ready);
        assert_eq!(
            original_hostile_return_fire_check(guard, tile(20, 5, 0), pistol, &clear).status,
            OriginalCombatShotStatus::OutOfRange
        );

        let blocked_line = OriginalCombatLineProbe {
            status: OriginalCombatLineStatus::BlockedByPedOccupancy,
            checked_tiles: 1,
            blocker_tile: Some(tile(6, 5, 0)),
            blocker_label: "ped occupancy candidate",
        };
        let blocked = original_hostile_return_fire_check(guard, agent, pistol, &blocked_line);

        assert_eq!(blocked.status, OriginalCombatShotStatus::Blocked);
        assert_eq!(blocked.blocker_label, "ped occupancy candidate");
    }

    #[test]
    fn original_combat_shot_check_gates_cooldown_range_height_and_down_state() {
        let start = tile(4, 4, 0);
        let target = tile(8, 4, 0);
        let pistol = OriginalCombatWeaponProfile::from_kind(OriginalWeaponKind::Pistol).unwrap();
        let clear = clear_line();
        assert_eq!(
            original_combat_shot_check(start, target, None, true, Some(pistol), &clear).status,
            OriginalCombatShotStatus::Ready
        );
        assert_eq!(
            original_combat_shot_check(start, target, None, false, Some(pistol), &clear).status,
            OriginalCombatShotStatus::Cooling
        );
        assert_eq!(
            original_combat_shot_check(start, tile(20, 4, 0), None, true, Some(pistol), &clear)
                .status,
            OriginalCombatShotStatus::OutOfRange
        );
        assert_eq!(
            original_combat_shot_check(start, tile(5, 4, 3), None, true, Some(pistol), &clear)
                .status,
            OriginalCombatShotStatus::Blocked
        );
        assert_eq!(
            original_combat_shot_check(start, target, None, true, None, &clear).status,
            OriginalCombatShotStatus::NoWeapon
        );
        let blocked_line = OriginalCombatLineProbe {
            status: OriginalCombatLineStatus::BlockedByStaticFootprint,
            checked_tiles: 1,
            blocker_tile: Some(tile(6, 4, 0)),
            blocker_label: "static footprint candidate",
        };
        let blocked =
            original_combat_shot_check(start, target, None, true, Some(pistol), &blocked_line);
        assert_eq!(blocked.status, OriginalCombatShotStatus::Blocked);
        assert_eq!(blocked.blocker_label, "static footprint candidate");
        let defeated = OriginalCombatPedState {
            record_index: 9,
            tile: target,
            hp: 0,
            max_hp: 50,
            objective_target: false,
            defeated: true,
        };
        assert_eq!(
            original_combat_shot_check(start, target, Some(&defeated), true, None, &clear).status,
            OriginalCombatShotStatus::AlreadyDown
        );
    }

    #[test]
    fn original_combat_weapon_profiles_follow_freesynd_reference_ranges() {
        let pistol = OriginalCombatWeaponProfile::from_kind(OriginalWeaponKind::Pistol).unwrap();
        let uzi = OriginalCombatWeaponProfile::from_kind(OriginalWeaponKind::Uzi).unwrap();
        let laser = OriginalCombatWeaponProfile::from_kind(OriginalWeaponKind::Laser).unwrap();

        assert_eq!(range_tiles_from_freesynd_world_range(1280), 5);
        assert_eq!(pistol.range_tiles, 5);
        assert_eq!(uzi.range_tiles, 7);
        assert_eq!(laser.range_tiles, 16);
        assert_eq!(pistol.local_damage, 2);
        assert_eq!(laser.local_damage, 32);
        assert!(pistol.cooldown_secs > uzi.cooldown_secs);
        assert!(OriginalCombatWeaponProfile::from_kind(OriginalWeaponKind::Scanner).is_none());
    }

    #[test]
    fn original_ped_candidate_role_style_distinguishes_target_and_npc_agent() {
        assert_eq!(
            original_ped_candidate_role_style(&ped_object(0x01, 0), false, false).0,
            "CIV"
        );
        assert_eq!(
            original_ped_candidate_role_style(&ped_object(0x02, 0), false, false).0,
            "NPC AGENT"
        );
        assert_eq!(
            original_ped_candidate_role_style(&ped_object(0x08, 0), false, false).0,
            "GUARD"
        );
        assert_eq!(
            original_ped_candidate_role_style(&ped_object(0, 0x10), false, false).0,
            "CRIM"
        );
        assert_eq!(
            original_ped_candidate_role_style(&ped_object(0x02, 0), true, false).0,
            "TARGET"
        );
        assert_eq!(
            original_ped_candidate_role_style(&ped_object(0x02, 0), true, true).0,
            "DOWN"
        );
    }

    #[test]
    fn original_combat_feedback_fades_and_labels_status() {
        let mut feedback = OriginalCombatFeedback::new(
            vec![tile(1, 1, 0)],
            tile(2, 2, 0),
            OriginalCombatShotStatus::Ready,
        );
        assert!(feedback.is_alive());
        assert_eq!(feedback.label(), "SHOT");
        assert!(feedback.fade() > 0.99);
        feedback.update(super::ORIGINAL_CONTROL_COMBAT_FEEDBACK_SECS + 0.01);
        assert!(!feedback.is_alive());
        assert_eq!(feedback.fade(), 0.0);

        let blocked = OriginalCombatFeedback::new(
            vec![tile(1, 1, 0)],
            tile(2, 2, 0),
            OriginalCombatShotStatus::Blocked,
        );
        assert_eq!(blocked.label(), "BLOCKED");
        let hostile_return = OriginalCombatFeedback::new(
            vec![tile(2, 2, 0)],
            tile(1, 1, 0),
            OriginalCombatShotStatus::HostileReturn,
        );
        assert_eq!(hostile_return.label(), "RETURN");
    }

    #[test]
    fn original_debug_agent_weapon_cooldown_ticks_after_firing() {
        let mut agent = OriginalDebugAgent::from_spawn(
            OriginalDebugAgentSpawn {
                slot: 0,
                record_index: 0,
                tile: tile(1, 1, 0),
                sprite_ready: true,
            },
            true,
        );
        assert!(agent.can_fire());
        let pistol = OriginalCombatWeaponProfile::from_kind(OriginalWeaponKind::Pistol).unwrap();
        agent.mark_fired(pistol.cooldown_secs);
        assert!(!agent.can_fire());
        agent.mark_under_fire();
        assert!(agent.is_under_fire());
        assert!(agent.weapon_status_label().contains("local threat marks 1"));
        agent.update(
            pistol
                .cooldown_secs
                .max(super::ORIGINAL_CONTROL_AGENT_UNDER_FIRE_SECS)
                + 0.01,
        );
        assert!(agent.can_fire());
        assert!(!agent.is_under_fire());
    }

    #[test]
    fn original_debug_agent_cycles_local_weapon_hints() {
        let mut agent = OriginalDebugAgent::from_spawn_with_weapons(
            OriginalDebugAgentSpawn {
                slot: 0,
                record_index: 0,
                tile: tile(1, 1, 0),
                sprite_ready: true,
            },
            true,
            vec![
                OriginalDebugAgentWeaponHint {
                    kind: Some(OriginalWeaponKind::Uzi),
                    source: OriginalDebugAgentWeaponSource::EquipmentOffset,
                    weapon_record_index: Some(3),
                },
                OriginalDebugAgentWeaponHint::player_fallback_pistol(),
            ],
        );

        assert_eq!(
            agent.selected_weapon().unwrap().kind,
            OriginalWeaponKind::Uzi
        );
        assert!(agent.weapon_label().contains("GAME equipment"));
        assert!(agent.weapon_label().contains("[1/2]"));
        assert_eq!(
            agent.cycle_weapon().unwrap().kind,
            OriginalWeaponKind::Pistol
        );
        assert!(agent.weapon_label().contains("starter pistol fallback"));
        assert_eq!(agent.cycle_weapon().unwrap().kind, OriginalWeaponKind::Uzi);
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
