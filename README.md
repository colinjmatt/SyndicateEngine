# SyndicateEngine

A clean-room, open-source, modern tactical isometric engine inspired by Bullfrog's 1993 **Syndicate**.

This project does **not** distribute copyrighted game data. Put your legally owned original files in `original_assets/`; that directory is ignored by git. The engine discovers those files locally and incrementally decodes their binary formats.

## Current prototype

- Native Rust + Macroquad desktop app, validated on Apple Silicon macOS.
- Isometric tactical city renderer with pan/zoom camera.
- Four controllable agents with selection and right-click movement orders.
- Asset indexer that scans `original_assets/` for maps, missions, palettes, sprites, and sounds.
- Early binary decoding modules for little-endian reads, VGA palettes, and `.TAB`/`.DAT` banks.
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

The report summarizes decoded palettes and `.TAB`/`.DAT` bank variant scores without copying copyrighted asset bytes into the repository.

## Roadmap

1. Decode Bullfrog `.TAB`/`.DAT` sprite banks into runtime textures.
2. Decode palette files and remap indexed art into RGBA textures.
3. Reverse-engineer `MAP*.DAT` city data into real tile layers.
4. Decode `MISS*.DAT` mission scripts, objectives, spawns, and trigger data.
5. Add tactical systems: weapons, civilians, vehicles, persuasion, destructibility, and AI.
6. Add modern UX: scalable UI, remappable controls, saves, accessibility, and mod packs.

## Legal stance

This is an independent clean-room engine project. It is not affiliated with, endorsed by, or sponsored by Bullfrog Productions or Electronic Arts. Original game assets are required from the user and are intentionally excluded from version control.