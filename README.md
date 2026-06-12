# SyndicateEngine

A clean-room, open-source, modern tactical isometric engine inspired by Bullfrog's 1993 **Syndicate**.

This project does **not** distribute copyrighted game data. Put your legally owned original files in `original_assets/`; that directory is ignored by git. The engine discovers those files locally and incrementally decodes their binary formats.

## Current prototype

- Native Rust + Macroquad desktop app, validated on Apple Silicon macOS.
- Isometric tactical city renderer with pan/zoom camera.
- Four controllable agents with selection and right-click movement orders.
- Asset indexer that scans `original_assets/` for maps, missions, palettes, sprites, and sounds.
- Early binary decoding modules for little-endian reads, RNC method-1 containers, VGA palettes, and `.TAB`/`.DAT` banks.
- Runtime-local selected-mission `MAP##.DAT` tile-stack renderer using local `HBLK01.DAT` map tiles, the mission `HPAL##.DAT` palette, and `COL01.DAT` tile typing, plus a decoded `MAP*.DAT` diagnostic scene catalog with inferred/candidate field views and aggregate block-addressability overlays. When local map graphics are available, the app starts framed on the original mission compound render; gameplay still uses the hand-authored demo grid.
- Metadata-selected mission loading reads local `GAME##.DAT` map info for the selected campaign block, then chooses the corresponding `MAP##.DAT` and `HPAL##.DAT` at runtime.
- A runtime-local first-mission scene model now parses typed guarded candidates for people, vehicles, statics, weapons, sfx, animation/frame references, sprite-bank support, spawn probes, navigation bridge inputs, candidate occupied/blocking buckets, typed GAME objective records, scenario action/trigger chains, MISS aggregate buckets, a conservative object draw queue, and aggregate original-map spatial/route/debug-navigation probes.
- First-mission statics and any peds, vehicles, or ground weapons whose runtime proof gates pass can render from local original `HSPR-0`/`HELE`/`HFRA`/`HSTA` assets. The first-mission control view distinguishes original map tiles, rendered original statics, rendered/proven object candidates, primary-selected and multi-selected original-agent markers, non-squad ped role/alert-facing/pressure/fire/held/panic/flee/fled overlays, hover aim rings, clicked candidate interaction buckets, formation-spaced original movement with wider destination-tail reservations, fallback route targets, staggered squad starts, local spacing holds that reduce route-cell pileups, manual selected-agent/objective camera focus, cancellable/requeueable local orders, visible route-blocked door labels, closed-door threshold approach routes with explicit `E` prompts when an agent reaches the threshold, local door-open route gates that immediately retry the stored movement order and persist for later route checks, persistent local door/vehicle/objective/pickup overlays, scenario-chain-backed non-squad NPC movement where `GAME` action records provide route targets, limited local roam only for unproven civilian/unknown fallback cases, a mission-1 target-to-car set-piece that hides the boarded target ped, moves the runtime vehicle along a local scenario/fallback route, and marks a runtime-only mission failure if the target escapes, local controlled-agent vehicle passenger/exit state plus road-biased right-click drive orders that redraw the linked car and suppress the base vehicle record, direction-aware equipped-weapon glyphs for armed squad/NPC peds, local dropped-weapon candidates that can render through guarded original weapon frames and be picked up when source/drop proof exists, a gated local combat/objective target overlay with FreeSynd-reference weapon ranges/cooldowns from guarded GAME loadout hints or starter-pistol fallback, selectable local weapon and non-shooting equipment hints, local medikit/shield/scanner prototypes, conservative line-of-fire blockers, readable hit/range/block/cooldown/return-fire/volley/down/impact labels with projectile and down markers, provisional target HP/down/local mission-complete state, local agent HP/threat/down-test counters with active selection repair, repeated debug-gated hostile return-fire and route-gated hostile/civilian pressure markers that stop visibly when blocked, and explicit blockers for unsupported final inventory/traffic/final AI semantics. Demo gameplay/pathfinding remain available on the hand-authored grid.
- Original-map camera startup and pan/zoom are constrained by the selected mission's scroll-tile bounds. Normal first-mission control now hides the large decoder/prototype HUD and shows a left-side original-style tactical sidebar using FreeSynd-documented agent-selector and weapon-icon sprite ranges when local HSPR assets are present, with drawn fallbacks, a two-row selected-agent weapon/equipment grid, and HP/cooldown bars for objective, squad, selected-agent weapons, health/threat/down-test, route state, combat, action gates, command feedback, reset, controls, target-vehicle state, and runtime audio state; local down-test agents are kept out of active command selection while remaining visible as local state. The audio layer catalogs local original `SOUND-0/1` sample sets and `SYNGAME.XMI`/`INTRO.XMI` availability using FreeSynd's loader rules, converts supported local Creative VOC game samples to in-memory WAV buffers at runtime, maps local events to FreeSynd sample ordinals for pistol, Uzi, shotgun, laser, minigun, gauss, explosion, persuade, selected, door, time bomb, pickup/put-down, menu, mission-complete, and mission-failed classes where samples load, and exposes runtime mute/volume keys. XMI music sequencing, full mixer semantics, and final sound semantics remain gated with explicit blockers. `T` enables the detailed diagnostics/console trace. The detailed HUD can still show first-mission scene queue health, viewport-visible candidate totals, animation/sprite support, static/object render readiness, spawn candidates, cursor tile candidates with local offsets, route target snap status, route diagnostic alignment, gated original-control status, scenario plan/action buckets, candidate interaction/objective/scenario buckets, a local mission-state lifecycle, debug objective-progress labels, local action results, combat/objective status, route-blocked/opened door overlays, door threshold arrivals, formation spacing holds, dropped-weapon render/pickup proof, civilian panic/flee/fled markers, hostile pressure/held markers, NPC route/vehicle boarding/driving/audio counters, local down-test selection-repair counters, local reset state, automated verified local mission-complete/reset/interaction-gate playtest trace state, and navigation-probe counts without exposing local asset bytes or per-object dumps.
- HUD diagnostics showing original asset discovery and decode status.

## Run

```bash
cd syndicate_engine
cargo run --bin syndicate_engine
```

Controls:

- `WASD` / arrow keys: pan camera
- Mouse wheel: zoom
- `1`-`4`: select agents; in first-mission control mode, select candidate original-agent markers instead. Hold Shift to add/remove markers from the selected original-agent set. Local down-test agents stay visible but cannot join active command selection.
- Right click: send selected agent to a tile in the demo city; in first-mission control mode, move selected original-agent markers along proven original route overlays with formation fallback targets, staggered squad starts, and local door/dynamic route gates where possible. If a selected marker is boarded into a local vehicle, right click issues a road-biased vehicle drive order where route gates pass. Hold Shift while right-clicking to append to the current original movement queue.
- Left click: attack in the demo city; in first-mission control mode, select a nearby original-agent marker or have the selected original-agent set fire at the hovered current-objective/non-squad ped candidate using gated local combat state, runtime weapon/loadout hints, conservative blockers, objective-completion state, hostile alert/return-fire markers, local agent HP/down-test feedback, and temporary shot/status overlays.
- `E`: in first-mission control mode, queue debug-gated interaction/action intents for selected original-agent markers at the current original cursor tile; if the last route was blocked by a candidate door, selected agents can approach the closed-door threshold, the route shows an `OPEN WITH E` prompt, and `E` opens that runtime-only route gate locally before immediately retrying the stored movement goal. `E` can also resolve local dropped-weapon pickups, vehicle passenger entry/exit, and selected medikit/shield/scanner equipment use when proof gates pass, while final door animation/locks/inventory/driving/accessory/gameplay semantics remain gated.
- `C`: in first-mission control mode, cancel selected original-agent local routes/actions without resetting combat or objective state.
- `F`: focus the camera on the selected original-agent marker in first-mission control mode.
- `J`: focus the camera on the current local objective target in first-mission control mode.
- `G`: in first-mission control mode, toggle gated original control on/off.
- `O`: in first-mission control mode, queue a local smoke-test route for the selected original-agent marker.
- `Q`: in first-mission control mode, cycle the selected original-agent markers through supported local weapon/loadout hints.
- `V` / `Z` / `X`: toggle original audio mute, lower volume, or raise volume for runtime-loaded local samples.
- `R`: in first-mission control mode, reset the local playtest state, selected original-agent marker, local combat/objective feedback, and camera focus without mutating source GAME data.
- `T`: toggle original-control console tracing and the detailed first-mission diagnostics panel for marker positions, route status, and control state.
- `M`: cycle between the runtime original mission-map tile render, first-mission control view, playable demo city, decoded `MAP*.DAT` diagnostic scene layers, aggregate block-addressability, runtime original-graphics candidate map, and runtime HBLK graphics atlas when local assets are available
- `N` / `P`: select the next or previous decoded MAP diagnostic scene
- `Esc`: quit

Local original-control smoke test:

```bash
cd syndicate_engine
SYNDICATE_ORIGINAL_CONTROL_SMOKE=1 cargo run --bin syndicate_engine
```

That runtime-only mode queues a candidate original route, prints aggregate marker/route status to stdout, and exits after a short run. Use `SYNDICATE_ORIGINAL_CONTROL_TRACE=1` for tracing without autopilot, or set `SYNDICATE_ORIGINAL_CONTROL_QUIT_FRAMES=480` to change the smoke-test duration.

For a longer local route-and-fire playtest, run:

```bash
cd syndicate_engine
SYNDICATE_ORIGINAL_CONTROL_PLAYTEST=1 cargo run --bin syndicate_engine
```

This runtime-only mode selects active original-control agents, routes them toward the current candidate objective with formation spacing, fallback targets, and staggered starts, holds firing positions through weapon cooldowns, attempts gated local fire when the objective is in range/line, advances route-backed NPC movement and the target-to-car candidate state, records local audio-event hooks, prints movement/shot/hit/hostile/agent/NPC/vehicle/objective state to stdout, exits early when local mission-complete gates pass, and otherwise exits after the configured frame count. If the delayed target-car set-piece escapes before the objective is down, the local mission state becomes a runtime-only failure. Set `SYNDICATE_ORIGINAL_CONTROL_REQUIRE_COMPLETE=1` to make the playtest exit non-zero if the configured frame cap is reached before local mission-complete gates pass or a terminal local failure occurs; in that verified mode it also prints final shot/hit/hostile-pressure/held/civilian-flee/agent-down/selection-repair/interaction proof state, attempts a local door threshold/open/retry route-gate verification or precise blocker, resets the runtime-only local state, and prints the current interaction-gate proof label before exiting.

## Project layout

```text
syndicate_engine/src/engine/  reusable engine systems and asset decoders
syndicate_engine/src/game/    prototype gameplay, map, HUD, and world state
original_assets/              your local original game data, ignored by git
```

## Validation

```bash
make validate
```

Or run individual commands with `make fmt`, `make test`, `make build`, `make report`, and `make run`.

## Asset inspection report

Generate a headless reverse-engineering report from your local original assets:

```bash
cd syndicate_engine
cargo run --bin inspect_assets -- ../original_assets ../docs/generated/asset-report.md
```

The report summarizes verified RNC decompression, decoded palettes, and `.TAB`/`.DAT` bank variant scores without copying copyrighted asset bytes into the repository.

Preview the local TAB/sprite runtime-probe manifest without generating the full report:

```bash
cd syndicate_engine
cargo run --bin probe_manifest -- ../original_assets
```

Run the aggregate-only dry-run execution layer for the same local selectors:

```bash
cd syndicate_engine
cargo run --bin probe_manifest -- --execute ../original_assets
```

These commands print capped aggregate selector IDs, dry-run phases, support tiers, execution readiness, group/support counts, and stop conditions for local clean-room decoder probes. They do not print asset bytes, chunk data, previews, decoded dimensions, anchors, commands, audio, UI, or gameplay semantics.

## Local visual diagnostics

When the engine is running, capture a local screenshot for visual comparison:

```bash
scripts/capture_visual_diagnostic.sh
```

For repeatable in-engine captures during smoke runs, set `SYNDICATE_VISUAL_DIAGNOSTIC_FRAMES=<frame>`; the runtime writes a single PNG under `visual_diagnostics/` after drawing that frame. Screenshots are written under `visual_diagnostics/`, which is ignored by git. Do not commit rendered original-asset previews.

## Roadmap

1. Decode Bullfrog `.TAB`/`.DAT` sprite banks into runtime textures.
2. Remap indexed art through decoded palettes into RGBA textures.
3. Prove the original sprite/frame renderer for statics, peds, weapons, vehicles, and sfx using runtime-local assets only.
4. Promote the gated first-mission control layer into a real original-navigation gameplay option as walkability, height transitions, door/window behavior, object occupancy, and spawn layers are proven.
5. Replace remaining provisional local combat/objective state with proved FreeSynd-style blocker scans, burst/hit probability, scenario/objective mutation, AI, door, pickup, vehicle-entry, and final mission-completion semantics as each behavior is proven.
6. Finish tactical systems: complete original AI scripts, full vehicle driving/traffic, all weapons/accessories, persuasion, destructibility, XMI music playback plus complete sound mapping/mixer semantics, and fully decoded original sidebar controls beyond the current selector/icon sprite ranges.
7. Add modern UX: scalable UI, remappable controls, saves, accessibility, and mod packs.

## Legal stance

This is an independent clean-room engine project. It is not affiliated with, endorsed by, or sponsored by Bullfrog Productions or Electronic Arts. Original game assets are required from the user and are intentionally excluded from version control.
