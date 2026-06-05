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

## TAB/DAT banks

Observed `.TAB` files are not all the same shape:

- Some banks look compatible with 32-bit little-endian offsets into paired `.DAT` files.
- Some files have odd byte lengths or patterns that suggest packed records, 16-bit fields, flags, dimensions, or mixed metadata rather than a plain offset table.
- The engine now scores 16/24/32-bit little-endian interpretations by valid offset count, unique offset count, and monotonic adjacent pairs.

Current conservative approach:

1. Use `TabArchive` only when a bank can be parsed safely into bounded chunks.
2. Use `TabVariantAnalysis` to report which offset width looks most plausible.
3. Avoid rendering decoded sprites until chunk layout and per-chunk headers are better understood.

## Sprite chunks

`sprite_decode.rs` currently classifies safe chunks as:

- empty,
- likely raw indexed pixels,
- likely RLE/command stream,
- unknown.

This is diagnostic only. The next milestone is to inspect compatible chunks in `HSPR-1` and identify width/height/anchor metadata before attempting texture generation.