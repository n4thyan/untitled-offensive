# IW4 (MW2) material/texture chain — trace findings

Session: mp_rust diffuse texture vertical slice.
Status: **surface/material/image association resolved, integrated, and human-verified in runtime**.

## 2026-10-09 correction and implementation

The prior "X64 layout blocker" diagnosis was wrong. `X64` is only the loader's
name for the 8-byte-pointer wire variant; it does not imply a 64-bit game. The
existing serialized layouts are correct: `Material` is 120 bytes,
`MaterialTextureDef` is 16 bytes, and `GfxSurface` is 32 bytes with its material
asset pointer at `+16`.

The failed diagnostic confused XAsset pointer slots with temporary loaded
bodies. Material bodies use the TEMP block and may be overwritten after their
callback. Fresh nested references remain `Following`; reused references point
to another XAsset slot. The durable identity is the `slot` supplied to
`AssetLinkSink::loaded`/`alias`, not `latest_material.header` after zone load.

A callback-time probe established:

- 282 materials and 608 images observed;
- 271 materials with texture tables, 750 texture definitions total;
- all 750 non-water image references resolve to names;
- all 5911 surface `+16` references resolve to 76 distinct materials;
- semantic `2` is the diffuse/color image (for example
  `plastic_case_big_green_us_dirt_col`).

The runtime now captures image names by XAsset slot, resolves each material's
semantic-2 image, maps each surface's `+16` target to that image, and emits BSP
materials named `images/<gfx-image-name>`. MW2 `main/*.iwd` archives are mounted
read-only into `Pk3Fs`; IWI v8 DXT1/3/5 payloads decode directly to the existing
BC1/BC2/BC3 renderer image type without writing extracted DDS files.

Human runtime verification passed on 2026-10-09: Rust is immediately
recognizable, distinct ground/industrial/scaffolding textures are assigned to
the appropriate surfaces, UVs are broadly correct, the fullbright fallback is
readable, and geometry/collision/spawn remain intact. Some surfaces are still
blown out or unresolved; this is tracked as shader/lightmap polish rather than
a blocker for the playable material milestone. Runtime reported 45 textures
loaded across 46 batches, with only `images/,shadow` missing.

The IW4 vertex layout correction used for the playable fallback is: packed
color at `16..20`, diffuse UV at `20/24`, lightmap UV at `28/32`, packed
normal/tangent at `36..44`. Until real lightmaps are wired, neutral vertex color
`[128,128,128,255]`, white lightmap page 0 and `lm_uv=[0,0]` produce readable
fullbright output without changing the CoD1 renderer.

The older blocker analysis below is retained as investigation history but is
superseded by this correction.

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

## Second blocker: the Material struct layout is ALSO wrong for X64

Raw `u32` dump of the first material (`faction_128_arab`) at its loaded body
`block0off0` (x64, 8-byte pointers):

```
[0]=807459384 [1]=0 [2]=0 [3]=0 [4]=837974602 [5]=0 [6]=0 [7]=0
[8]=4294967295 [9]=4294967295 [10]=837974569 [11..39]=0
```

Reading:
- `+0` is the name pointer (it decodes to `faction_128_arab`), so the loader's
  `follow_name(p, 0)` is right.
- The only other plausible pointers are at `+16` and `+40` (both decode to
  block 1, ~301 MB apart in offset).
- `+32`/`+36` hold `0xFFFFFFFF, 0xFFFFFFFF` — a `-1` sentinel pair, not data.
- Everything from `+44` to `+160` is **zero**.

The loader allocates `s.layout(sz::MATERIAL, 120)` = **120 bytes** and reads
`texture_count` at `s.layout(72, 80)` = **80**, `technique_set` at 88,
`textures` at 96. Those offsets are all past the end of the real record, which
looks like a **~48-byte header**. So `texture_count` reads 0 (or garbage 255
when the "material" is actually a misread surface word), and the texture array
is never found. That is why 0 of 282 materials yield an image name.

This is independent of the GfxSurface blocker and must be fixed too: the
`Material` layout in `crates/iw4/src/load/material.rs` and
`asset_iw4::size::MATERIAL` were tuned against the x86 variant and do not
describe the X64 record.

## Next step (tomorrow, in order)

Fix the **Material** layout first — it is the cheaper of the two and everything
downstream depends on it.

1. **Determine the true x64 Material record.** From the dump the header is
   ~48 bytes: `+0` name, `+16` and `+40` pointers, `-1` sentinel at `+32`.
   Find which of `+16`/`+40` is the technique_set and which leads to the
   texture array, by following each and looking for a printable name (the
   current sweep found neither at `+0` or `+32` of the target, so the
   texture-def/image layout needs the same treatment as the surface).
2. Only then re-attempt the GfxSurface material pointer — with a known-good
   material body to match against, the surface field becomes findable by
   "does this word point at a real material record", which is a far stronger
   test than "does this word point at a printable string".
3. Render one: write the real `.iwi` DDS into an overlay pk3 as
   `images/<name>.dds` — the client's `load_material_image` already probes
   `.dds`, so one texture lights up with **no renderer change at all**. This is
   the smallest possible vertical slice and it deliberately reuses the whole
   existing CoD1 material path.

### Proven-good pieces to reuse (do not redo)

- **IWI → DDS is trivial and verified.** An `.iwi` is a 32-byte header
  (`IWi`, ver 8, then flags/usage/format at +4/+6/+8, `u16` w/h/d at +10/12/14,
  `u32[4]` per-picmip sizes at +16) followed by a **standard DXT mip chain**.
  Measured: `ch_metal_rusty_01_col` (fmt 11 = DXT1) 512x512, 32 + 174776 =
  174808 = file size exactly; `ch_factory_ruststain1` (fmt 13 = DXT5) 512x512,
  32 + 349552 = 349584 exactly. Decoded with a 128-byte DDS header in front,
  the payload scores mean adjacent `|dRGB|` of 6–9 versus 32–51 for a
  deliberately wrong offset — i.e. it is a real texture, not noise.
  Helper scripts: `%LOCALAPPDATA%/hermes/cache/scratch/iwi_probe.py` (decode to
  PNG) and `iwi_score.py` (coherence score vs wrong-offset control).
- `.iwi` files live in `main/*.iwd`, which are plain zips; `Pk3Fs` can index
  them, and it normalises `@` → `_` (exactly what MW2 image names need).
- **`tools/iwi_to_dds.py` already implements the conversion.** Verified: it
  emits a 128-byte DDS header (`DDS ` magic, flags `0x81007`, `mipMapCount` at
  offset 28, DXT1/DXT5 fourcc at 84) followed by the raw mip chain, which is
  exactly what `vcod_common::assets::parse_dds` reads. Usage:
  `python tools/iwi_to_dds.py in.iwi out.dds` or `--batch <dir> <out-dir>`.

### The GfxSurface side (blocked, secondary)

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
