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
- First-mission statics and any peds, vehicles, or ground weapons whose runtime proof gates pass can render from local original `HSPR-0`/`HELE`/`HFRA`/`HSTA` assets. The first-mission control view distinguishes original map tiles, rendered original statics, rendered/proven object candidates, selectable/multi-select original-agent markers, clicked candidate interaction buckets, queued original movement, local action-resolution state, candidate combat probes, and blocked navigation/gameplay semantics. Demo gameplay/pathfinding remain available on the hand-authored grid.
- Original-map camera startup and pan/zoom are constrained by the selected mission's scroll-tile bounds. The HUD can show first-mission scene queue health, viewport-visible candidate totals, animation/sprite support, static/object render readiness, spawn candidates, cursor tile candidates with local offsets, route target snap status, gated original-control status, candidate interaction/objective/scenario buckets, debug objective-progress labels, local action results, and navigation-probe counts without exposing local asset bytes or per-object dumps.
- HUD diagnostics showing original asset discovery and decode status.

## Run

```bash
cd syndicate_engine
cargo run --bin syndicate_engine
```

Controls:

- `WASD` / arrow keys: pan camera
- Mouse wheel: zoom
- `1`-`4`: select agents; in first-mission control mode, select candidate original-agent markers instead. Hold Shift to add/remove markers from the selected original-agent set.
- Right click: send selected agent to a tile in the demo city; in first-mission control mode, move selected original-agent markers along proven original route overlays. Hold Shift while right-clicking to append to the current original movement queue.
- Left click: attack in the demo city; in first-mission control mode, select a nearby original-agent marker or run a candidate combat/range probe against a clicked non-squad ped.
- `E`: in first-mission control mode, queue debug-gated interaction/action intents for selected original-agent markers at the current original cursor tile.
- `F`: focus the camera on the selected original-agent marker in first-mission control mode.
- `G`: in first-mission control mode, toggle gated original control on/off.
- `O`: in first-mission control mode, queue a local smoke-test route for the selected original-agent marker.
- `T`: toggle original-control console tracing for marker positions, route status, and control state.
- `M`: cycle between the runtime original mission-map tile render, first-mission control view, playable demo city, decoded `MAP*.DAT` diagnostic scene layers, aggregate block-addressability, runtime original-graphics candidate map, and runtime HBLK graphics atlas when local assets are available
- `N` / `P`: select the next or previous decoded MAP diagnostic scene
- `Esc`: quit

Local original-control smoke test:

```bash
cd syndicate_engine
SYNDICATE_ORIGINAL_CONTROL_SMOKE=1 cargo run --bin syndicate_engine
```

That runtime-only mode queues a candidate original route, prints aggregate marker/route status to stdout, and exits after a short run. Use `SYNDICATE_ORIGINAL_CONTROL_TRACE=1` for tracing without autopilot, or set `SYNDICATE_ORIGINAL_CONTROL_QUIT_FRAMES=480` to change the smoke-test duration.

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

Screenshots are written under `visual_diagnostics/`, which is ignored by git. Do not commit rendered original-asset previews.

## Roadmap

1. Decode Bullfrog `.TAB`/`.DAT` sprite banks into runtime textures.
2. Remap indexed art through decoded palettes into RGBA textures.
3. Prove the original sprite/frame renderer for statics, peds, weapons, vehicles, and sfx using runtime-local assets only.
4. Promote the gated first-mission control layer into a real original-navigation gameplay option as walkability, height transitions, door/window behavior, object occupancy, and spawn layers are proven.
5. Promote local action-resolution state into real objective, scenario, door, pickup, vehicle-entry, AI, combat, and mission-completion semantics as each behavior is proven.
6. Add tactical systems: weapons, civilians, vehicles, persuasion, destructibility, and AI.
7. Add modern UX: scalable UI, remappable controls, saves, accessibility, and mod packs.

## Legal stance

This is an independent clean-room engine project. It is not affiliated with, endorsed by, or sponsored by Bullfrog Productions or Electronic Arts. Original game assets are required from the user and are intentionally excluded from version control.
