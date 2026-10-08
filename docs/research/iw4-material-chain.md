# IW4 (MW2) material/texture chain — trace findings

Session: short evening probe, mp_rust diffuse texture vertical slice.
Status: **blocked at the GfxSurface record layout**. No texture rendered yet.
Do not redo this trace; start from the open question at the bottom.

## What is already proven (do not re-derive)

- `probe.exe` loads `mp_rust.ff`: 480 assets, 235386 verts, 579864 indices,
  5911 surfaces, 3 lightmaps, 1 cell, 18 brush models.
- All 5911 GfxSurfaces carry a **non-null material pointer** (pointer kind
  "offset", zero nulls). The material reference is not the problem.
- The `Material` asset loader works: **282 materials** are captured with names,
  plus **608 images** with real names and dimensions (e.g.
  `plastic_case_big_green_us_dirt_col`, `me_cargocontainergreen_ext_roof_col`).
- Real image pixel data is **DXT-compressed in `main/*.iwd`**, NOT inline in the
  fastfile. Measured: `ch_metal_rusty_01_col.iwi` = format 11 (DXT1),
  512x512, 174808 bytes; `ch_factory_ruststain1.iwi` = format 13 (DXT5),
  512x512, 349584 bytes. `GfxImage.source_len == 0` for every image, so the
  loader records no inline source — the pixels must come from the `.iwi` entry
  in the iwd archives.
- The iwd files are **plain zips** with a ~30k-entry central directory. The
  client already has `Pk3Fs::open_layered(mod_dir, overlay)`; the game's `main/`
  directory can be passed as `mod_dir` (extension check accepts `.iwd`).
  `Pk3Fs` normalizes `@` → `_`, which is exactly what MW2 image names need.
- `asset_iw4::wavelet` exists but is **not needed** for mp_rust: these textures
  are format 11/13 (DXT), and `wavelet_check_header` only accepts 6..=10.

## The blocker: GfxSurface stride is NOT 24 on this asset

**`mp_rust.ff` is the X64 wire format** (`stream.wire_format() == X64`,
`pointer_bytes() == 8`). Every "24 bytes on x86" figure in
`references/iw4-geometry.md` is therefore describing the *wrong variant* for
this file; `stream.layout(24, 32)` returns **32**, and that is what the adapter
uses — which is why geometry works.

`diag_material` reads surfaces at 32-byte stride and still gets garbage:
`texture_count = 255`, material names decoding as `ZonePtr::Following`.

Raw dump of the first four records at 32-byte stride:

```
surf 0: 00 00 00 00 | 00 00 00 00 | 80 e4 30 00 | 00 00 00 00 | bd d5 84 31 | 00 00 00 00 | 00 01 00 01 | 00 00 00 00
surf 1: 00 00 00 00 | 00 00 00 00 | 80 e4 11 00 | 90 00 00 00 | bd d5 84 31 | 00 00 00 00 | 00 01 00 01 | 00 00 00 00
surf 2: 00 00 00 00 | 00 00 00 00 | 80 e4 02 00 | c3 00 00 00 | bd d5 84 31 | 00 00 00 00 | 00 03 00 01 | 00 00 00 00
surf 3: 00 00 00 00 | 00 00 00 00 | 80 e4 02 00 | c9 00 00 00 | bd d5 84 31 | 00 00 00 00 | 01 03 00 01 | 00 00 00 00
```

Reading:
- word 2 (`80 e4 30 00` = 0x30E480) and word 3 (`90 00 00 00` = 144,
  `c3` = 195, `c9` = 201) look like a **vertex count and a tri count**, not
  first_vertex/first_index. surf 0 has tri count 0 and word2 = 3204224, which
  is not a plausible vertex offset for a 235386-vertex map.
- word 4 is **identical across all four records** (`0x3184D5BD` =
  830789053). A constant per-surface pointer cannot be a material pointer.
  This is the strongest signal that the record boundary is wrong: consecutive
  surfaces must reference different materials.
- The tail `00 01 00 01` / `00 03 00 01` varies like small enum pairs.

Conclusion: the surfaces array pointer is correct (the values are coherent and
monotone in a counter), but **the element size and/or the field offsets differ
from the 24-byte layout the adapter was tuned against.** `asset_iw4::size`
declares `GFX_SURFACE = 24` (x86), matching `diag_bounds`'s comment, but that
number has never been validated against a material round-trip — only against
triangle-soup vertex counts.

Important: the *geometry* path is unaffected. `adapter::build_bsp` reads
`first_vertex` at offset 4, `vertex_count` at 8, `tri_count` at 10,
`first_index` at 12, and those offsets are human-verified correct (the map
renders and collision is excellent). So the geometry fields live at 4/8/10/12
within whatever the true record is; the material pointer is at 16 only
*according to the adapter's comment*, and that specific claim is now disproven.

## Also learned (smaller, still useful)

- Materials load into **block 0 (TEMP)**, surfaces live in **block 3
  (RUNTIME)**. There is no code path that fixes the surface's material pointer
  to point at the loaded material — `fixup_slot` is never called for
  `GfxSurface.material`. `ZonePtr::Following` in the runtime block means
  "payload follows", and nothing followed it. This is a second, independent
  defect that will need solving even after the stride is fixed.
- `Capture` must implement `AssetLinkSink` and be passed as the *same object*
  to both the `AssetSink` and the observed loader, otherwise nested material
  and image loads are invisible (the nested loader calls `links.loaded`, not
  `sink.load_asset`).

## Next step (tomorrow, in order)

1. **Find the true GfxSurface stride on X64.** The stride scan already run
   (candidate strides 20/24/28/32/36/40/44 crossed with material-pointer
   fields 12/16/20, over the first 400 surfaces, scoring on "pointer resolves
   to a printable name") produced **zero** combinations passing the bar. That
   means the material reference is **not a plain offset pointer stored inline
   in the surface record** on X64 — do not re-run the same scan.
   Next hypotheses, in order of likelihood:
   - The X64 surface record stores the material as a **32-bit encoded
     `ZonePtr`** (`ZonePtr::decode` semantics) rather than a full 8-byte
     pointer, i.e. the same `encode_offset` packing used in the wire table.
     Decode `u32` words at each offset with `ZonePtr::decode` instead of
     `stream.ptr_at`.
   - The surfaces array itself is reached through a **runtime-fixup slot** and
     the bytes read are the pre-fixup encoding, not the resolved pointer.
   - The record is 32 bytes but the material field is at an offset not in
     {12,16,20}; enumerate *all* 4-byte-aligned offsets 0..28.
2. Confirm the material pointer offset the same way.
3. Only then wire the material → texture table → image name chain.
4. Render one: add the resolved `.iwi` DDS payload to the overlay path the
   client already reads (`load_material_image` already probes `.dds`, so an
   overlay pk3 carrying `images/<name>.dds` would light up with **no renderer
   change at all**). That is the smallest possible vertical slice and it
   deliberately reuses the whole existing CoD1 material path.

## What NOT to do

- Do not re-run the winding experiment, do not touch collision, do not change
  `adapter::build_bsp`'s geometry offsets (4/8/10/12) — those are verified.
- Do not enable backface culling.
- Do not add a new renderer path before one texture is proven.
- Do not commit extracted retail `.iwi`/`.dds` bytes into the repository.
