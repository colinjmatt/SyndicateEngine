# Reverse-engineering notes

These notes track observed asset-format behaviour from locally supplied original game data. They are not a specification yet; they are working notes for clean-room decoding.

## Palette data

- Candidate files include `COL01.DAT` and `HPALETTE.DAT` under `SYNDICAT/DATA` and `DATADISK/DATA`.
- The current decoder treats the first 768 bytes as a 256-colour VGA palette with 6-bit RGB channels.
- The HUD displays a 32-colour ramp sampled from the decoded palette.
- Current observed palette candidates begin with an `RNC` signature, meaning they are compressed/enveloped rather than direct palette payloads.
- RNC method-1 decompression is implemented with CRC-16/IBM verification for both packed payloads and unpacked output.
- `HPALETTE.DAT` decompresses to a 768-byte VGA palette candidate; `COL01.DAT` decompresses to a smaller 256-byte colour/index table and is tracked separately from full VGA palettes.

## RNC containers

- Header fields are big-endian for packed/unpacked lengths and CRCs; payload data begins after the 18-byte header.
- Method 1 uses LSB-first Huffman/LZ blocks and header byte 17 as the block count. Header byte 16 is retained as leeway/in-place metadata but is not needed for out-of-place decoding.
- The decoder verifies packed CRC before decompression and unpacked CRC before returning bytes. Method 2 remains unsupported until a clean fixture and implementation plan are available.

## MAP data

- All observed `MAP*.DAT` files are RNC method-1 containers and now decompress with packed/unpacked CRC verification.
- The first stable decoded structure is a `64 * 64 * 12 = 49152` byte primary cell section. The analyzer treats each 12-byte cell conservatively as three little-endian 32-bit words until the fields are named.
- Remaining decoded bytes form a variable tail. Observed tails are aligned to 12-byte records, suggesting additional map/object records, but those records are not semantically decoded yet.
- The generated report lists primary-cell uniqueness, empty-cell counts, and tail record counts as aggregate diagnostics only; it does not include asset bytes.
- The HUD can render an abstract 64x64 cell-signature preview for `MAP01.DAT`. Colours represent frequency-ranked exact 12-byte cell signatures, not decoded terrain types.
- The analyzer now also reports per-word ranges and candidate low-byte lanes (`b0`, `b4`, `b8`) for the three 32-bit words, plus top byte-value counts. These are aggregate field diagnostics only.
- A gated inferred-layer preview derives low-risk visual channels from dominant byte-lane baselines: word-0 low byte as a surface candidate, word-1 low byte as a detail candidate, word-2 low byte as a reference candidate, and the narrowest varying high byte as a height-like candidate. These labels are deliberately provisional and should not be treated as final terrain/building semantics.
- The report now adds evidence-backed field-correlation diagnostics across decoded MAP primary sections: global byte-lane distributions, neighbour-continuity percentages, common value transitions, repeated 2x2/block-like patterns, and conservative height-gradient checks. These diagnostics are intended to rank candidate fields; they are not claims of exact terrain/building semantics.
- A provisional `MapPrimarySubstrateCandidate` now copies the selected diagnostic byte lanes into non-gameplay substrate channels: `surface_index_candidate`, `detail_index_candidate`, `reference_candidate`, and `height_candidate`. Each channel records its selected lane, dominant baseline, unique count, continuity, repeated-block evidence, gentle-gradient evidence, and a low/medium/high heuristic confidence label. These confidence labels summarize evidence strength only and do not prove terrain, building, or object semantics.
- BLK-like graphics containers such as `MMAPBLK.DAT`, `HBLK01.DAT`, `MMAP.DAT`, and `MMAPOUT.DAT` are now inspected with non-reconstructable aggregate diagnostics: RNC status, decoded length, byte entropy/zero/unique summaries, plausible fixed-size indexed-pixel record counts, aggregate fixed-record layout probes, duplicate-record counts via checksums, per-record zero/unique/entropy min/median/max summaries, conservative leading/trailing table or remainder hints, and range-only correlations against MAP substrate candidate byte lanes. These rows can show that a MAP candidate range could address a block/tile candidate record count or that a container has very low aggregate entropy, but they do not prove terrain, building, object, minimap, mask, or render-layout semantics.
- The generated report also includes cross-container aggregate relation probes for BLK-like and TAB/DAT candidates. These compare decoded/plain length ratios, duplicate-name decoded length/hash status between `SYNDICAT/DATA` and `DATADISK/DATA`, layout-alignment support rankings across candidate dimensions, and TAB/DAT chunk-count compatibility against BLK-like fixed-record counts. These probes use only counts, lengths, entropy summaries, and non-reconstructable hash/status comparisons; they are intended to prioritize future investigation and do not establish render formats or semantic links.
- Press `M` in the prototype to cycle the main world view between the playable hand-authored demo city, the decoded `MAP01.DAT` signature preview, the provisional inferred-layer preview, and individual candidate-field explorer views for `surface_index_candidate`, `detail_index_candidate`, `reference_candidate`, and `height_candidate`. Gameplay/pathfinding still uses the demo tactical grid until the map fields are named.

## TAB/DAT banks

Observed `.TAB` files are not all the same shape:

- Some banks look compatible with 32-bit little-endian offsets into paired `.DAT` files.
- Some files have odd byte lengths or patterns that suggest packed records, 16-bit fields, flags, dimensions, or mixed metadata rather than a plain offset table.
- The engine now scores 16/24/32-bit little-endian interpretations by valid offset count, unique offset count, and monotonic adjacent pairs.
- Safely parsed 32-bit TAB/DAT archives now report aggregate-only chunk diagnostics: chunk count, min/median/max chunk size, common chunk-size buckets, chunk-size entropy, duplicate-offset/zero-length candidate counts, first/last offset sanity ranges, exact matches to fixed tile-byte candidates such as 64/256/512/1024/2048/4096 bytes, chunk-length progression candidates (equal-size runs, common adjacent length deltas, and repeated size-pattern counts), aggregate sprite chunk classifier counts, classifier distribution by chunk-size buckets, small/medium/large chunk count bands, zero/high-byte ratio min/median/max summaries by classifier kind, candidate leading-byte/header-shape frequency counts, and conservative candidate metadata-shape support counts/ranges for bounded leading dimension/offset interpretations. These summaries do not include chunk bytes or render previews.
- Cross-container probes compare TAB/DAT chunk-size distributions and chunk counts with BLK-like fixed-record candidates. The report also groups safely parsed TAB/DAT archives by conservative file-name families such as `HSPR`, `MSPR`, `MFNT`, `FONT`, and `SOUND`, then compares only aggregate classifier totals and candidate header-shape counts. These are compatibility rankings for future investigation only; they do not prove that a TAB/DAT bank is a sprite, tile, font, sound, UI, or map-support format.
- The generated report now includes dedicated TAB/sprite family aggregate-ranking and comparison sections for safely parsed TAB/DAT archives. The ranking section uses only parsed archive counts, total chunk counts, command-stream/raw/unknown classifier totals, bounded candidate metadata-shape support ratios, equal-size run and repeated size-pattern support, chunk-size entropy ranges, and overlapping common chunk-size buckets. The comparison section currently compares top sprite-like filename families such as `HSPR` and `MSPR` using aggregate ratio differences, progression-support differences, entropy ranges, and overlapping/distinct common chunk-size buckets. These rows are prioritization aids for future clean-room decoding and do not render sprites, expose chunk/header bytes, or prove metadata, graphics, font, sound, UI, or mission semantics.

Current conservative approach:

1. Use `TabArchive` only when a bank can be parsed safely into bounded chunks.
2. Use `TabVariantAnalysis` to report which offset width looks most plausible.
3. Use aggregate summaries and synthetic tests to rank candidate relationships without exposing reconstructable data.
4. Avoid rendering decoded sprites until chunk layout and per-chunk headers are better understood.

## Sprite chunks

`sprite_decode.rs` currently classifies safe chunks as:

- empty,
- likely raw indexed pixels,
- likely RLE/command stream,
- unknown.

This is diagnostic only. The next milestone is to inspect compatible chunks in `HSPR-1` and identify width/height/anchor metadata before attempting texture generation.

Current aggregate sprite-bank diagnostics intentionally stop at non-reconstructable metadata: chunk counts, size buckets, classifier distributions, zero/high-byte ratio summaries, candidate header-shape frequencies, and candidate metadata-shape support counts/ranges. Header-shape and metadata-shape labels are provisional aggregate patterns only; they should not be interpreted as decoded sprite dimensions, anchors, commands, or audio metadata without stronger cross-file evidence.