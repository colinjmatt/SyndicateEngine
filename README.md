# SyndicateEngine

A clean-room, open-source, modern tactical isometric engine inspired by Bullfrog's 1993 **Syndicate**.

This project does **not** distribute copyrighted game data. Put your legally owned original files in `original_assets/`; that directory is ignored by git. The engine discovers those files locally and incrementally decodes their binary formats.

## Current prototype

- Native Rust + Macroquad desktop app, validated on Apple Silicon macOS.
- Isometric tactical city renderer with pan/zoom camera.
- Four controllable agents with selection and right-click movement orders.
- Asset indexer that scans `original_assets/` for maps, missions, palettes, sprites, and sounds.
- Early binary decoding modules for little-endian reads, RNC method-1 containers, VGA palettes, and `.TAB`/`.DAT` banks.
- Runtime-local decoded `MAP*.DAT` diagnostic scene catalog with inferred/candidate field views, aggregate block-addressability overlay, and runtime-only `HBLK01.DAT` map-tile graphics decoded from local assets. When local graphics are available, the app starts on the runtime graphics atlas; gameplay still uses the hand-authored demo grid.
- HUD diagnostics showing original asset discovery and decode status.

## Run

```bash
cd syndicate_engine
cargo run
```

Controls:

- `WASD` / arrow keys: pan camera
- Mouse wheel: zoom
- `1`-`4`: select agent
- Right click: send selected agent to a tile
- `M`: cycle between the playable demo city, decoded `MAP*.DAT` diagnostic scene layers, aggregate block-addressability, runtime original-graphics candidate map, and runtime HBLK graphics atlas when local assets are available
- `N` / `P`: select the next or previous decoded MAP diagnostic scene
- `Esc`: quit

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

## Roadmap

1. Decode Bullfrog `.TAB`/`.DAT` sprite banks into runtime textures.
2. Remap indexed art through decoded palettes into RGBA textures.
3. Reverse-engineer decompressed `MAP*.DAT` city data into real tile layers.
4. Decode `MISS*.DAT` mission scripts, objectives, spawns, and trigger data.
5. Add tactical systems: weapons, civilians, vehicles, persuasion, destructibility, and AI.
6. Add modern UX: scalable UI, remappable controls, saves, accessibility, and mod packs.

## Legal stance

This is an independent clean-room engine project. It is not affiliated with, endorsed by, or sponsored by Bullfrog Productions or Electronic Arts. Original game assets are required from the user and are intentionally excluded from version control.
