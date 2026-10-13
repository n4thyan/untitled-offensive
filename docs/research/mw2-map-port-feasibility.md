# MW2 (2009) map-geometry port feasibility — geometry-only

**Scope:** Can Call of Duty: Modern Warfare 2 (2009, IW4 engine) map *geometry* be
ported into this project's Quake III / CoD1 (`IBSP` version 59) engine?
**Method:** primary evidence — read the actual MW2 fastfiles shipped on this
machine, decompress one, and read the source of the open-source tools that handle
this format. **VERIFIED** marks measurements made here. **SOURCED** marks
facts read from a tool's source or documentation. **INFERRED** marks conclusions
drawn from that evidence.

---

## VERDICT

**Technically: yes, but it is not a BSP port — it is a second map-format loader,
and a converter is not a parse-and-rewrite.**

- MW2 ships **no `.bsp`/`.d3dbsp`/`.map` files at all**. All map geometry lives
  inside `.ff` **fastfiles** as several embedded asset structures
  (`clipMap_t`, `GfxWorld`, `ComWorld`, `MapEnts`, `GameWorldMp`, `PhysCollmap`,
  `FxWorld`). **VERIFIED** — 30,716 entries across the retail `main/*.iwd`
  archives are only `.wav` / `.iwi` / `.mp3` / `.cfg`; zero `.bsp`-family files,
  and no `IBSP`/`CoAGD` magic anywhere in a decompressed retail zone.
- The geometry is **a different BSP variant from Q3/CoD1 `IBSP`**, not a version
  bump of it. It is a pointer-linked, split collision/render runtime structure
  (CoD/IW4 `clipMap_t` + `GfxWorld`), not a flat lump directory. Plane and node
  records are different sizes and layouts. **VERIFIED** from OpenAssetTools'
  IW4 struct definitions.
- A converter is therefore **not a parse-and-rewrite**. Collision (brushes,
  planes) can be translated; **rendering geometry and visibility cannot be
  recovered losslessly** — MW2 stores cells/portals/AABB trees (a runtime
  visibility system), not a Q3-style BSP node/leaf/PVS lump, and the source
  `.map` that produced it is gone. You either consume MW2's own structures
  natively or **re-run BSP + vis/portal generation**.
- **Cheapest credible path: don't convert — load MW2 structures natively.** This
  is exactly what `iw4L` (a from-scratch Rust MW2 runtime, Apache-2.0) already
  does; its `fastfile_iw4` + `clipmap_iw4` crates read MW2 zones and build
  geometry/collision in a non-Quake engine. **SOURCED** (source read).
- **Cost:** roughly a second map pipeline (fastfile parse → clipMap/GfxWorld/
  ComWorld/MapEnts → world build). Comparable to the `fastfile_iw4` +
  `clipmap_iw4` + `asset_iw4` slice of iw4L, i.e. a multi-crate, multi-month
  effort for one engineer, **plus** a visibility decision (consume MW2 cells or
  regenerate).
- **Legal/technical blocker is the assets, not the code.** No open-source repo
  ships MW2 geometry; every tool requires an installed retail copy and reads the
  `.ff` at runtime or via the game's memory. Extracting geometry grants no
  rights, and a port still requires the retail game to be present.
- **Given the owner's stated position (no MW2 maps in the shipping product), the
  evidence supports NOT porting.** See "Decision inputs".

**Bottom line:** porting is *feasible as a research/experiment* (proven by iw4L),
but it is a large, asset-gated second loader with no lossless BSP converter, and
no existing tool does "MW2 geometry → Q3/CoD1 BSP". Not a drop-in.

---

## 1. What format do MW2 maps ship in?

**VERIFIED on this machine.** Retail install:
`C:/Program Files (x86)/Steam/steamapps/common/Call of Duty Modern Warfare 2`.

- Maps are `zone/english/<map>.ff` (+ `<map>_load.ff`), e.g. `mp_boneyard.ff`
  (62,472,213 bytes), `mp_terminal.ff`. Header bytes:
  `49 57 66 66 30 31 30 30 14 01 00 00 01 …` = magic `IWff0100`, then
  **version `0x114` (276) little-endian** (PC), then the Infinity Ward "authed
  chunks" stream (`IWffs100` at file offset `0x15`).
- Decompressing the signed/authed-chunk stream of `mp_boneyard.ff` yields a
  **107,895,934-byte zone**. (Note: for this file the zlib payload starts at
  `0x2015`, not `0x4015` as some notes assume — the chunk-group boundary differs
  on this build; **VERIFIED** by brute-forcing the offset and decompressing.)
- Zone header is **56 bytes (0x38)**, little-endian, 8 `blockSize` slots; the
  `XAssetList` region is at `0x28` and the asset-list pointer at `0x34` is the
  `0xFFFFFFFF` "insert block" placeholder — the game resolves the pool from the
  block tables, not a fixed offset. **VERIFIED.**
- **No `IBSP` (or `CoAGD`/`RBSP`/`FBSP`) magic exists anywhere in the
  decompressed zone. VERIFIED.**
- `main/*.iwd` (the pk3-equivalent archives) contain **30,716 entries, all
  `.wav` (18,393) / `.iwi` (12,075) / `.mp3` (247) / `.cfg` (1)** — i.e. audio
  and textures only. **Zero `.bsp`/`.d3dbsp`/`.map` entries. VERIFIED.**

**Geometry asset names (SOURCED — OpenAssetTools `IW4` zone entries):**
`clipMap_t` (collision + BSP/cell tree), `GfxWorld` (renderable surfaces,
vertices, cells, portals), `ComWorld` (primary lights), `MapEnts` (entity
string + binary model placements), `GameWorldMp` (MP script), `PhysCollmap`
(per-model collision), `FxWorld` (glass/destructibles), `GfxLightDef`.

So: **`.ff` fastfiles, geometry as embedded `clipMap_t`/`GfxWorld` assets.**

---

## 2. Is the geometry structurally the same as Quake III `IBSP`?

**No.** Same *lineage* (CoD derives from Q3), **different serialization and
layout**. **VERIFIED** against this project's own CoD1 notes
(`docs/research/bsp-ibsp59-format.md`) and OpenAssetTools' IW4 structs.

| Aspect | CoD1 / target (`IBSP` v59) | MW2 / IW4 (`clipMap_t`+`GfxWorld`) |
|---|---|---|
| Container | standalone `.bsp`, magic `IBSP` | no BSP file; serialized inside `.ff` |
| Directory | 33 × `(length, offset)` lump table | pointer-linked structs, zone blocks |
| Reference model | lump **offsets/indices** | **zone-relative 32-bit pointers** |
| Plane record | lump 2, **16 bytes** | `cplane_s`: normal[3] f32 + dist + `char type` + `char pad[3]` = **20 bytes** |
| Node | lump 20, **36 bytes** (`plane, children[2], mins[3], maxs[3]`) | `cNode_t`: `cplane_s* plane; int16 children[2]` = **16 bytes**, no bounds |
| Leaf | lump 21, **36 bytes** | `cLeaf_t` (`firstCollAabbIndex, collAabbCount, brushContents, terrainContents, Bounds, leafBrushNode`) — different fields |
| Collision | brushes (lump 4) + brushsides (lump 3) | `cbrush_t`/`cbrushside_t` **plus** a triangle-mesh partition system (`CollisionPartition`, `CollisionAabbTree`, `CollisionBorder`, `triEdgeIsWalkable`) |
| Render geometry | inside the same BSP (tri soups lump 6, draw verts lump 7) | **separate `GfxWorld`** (`GfxSurface`/`GfxWorldDraw`/`GfxWorldVertex`) |
| Visibility | BSP nodes/leafs + PVS lump 28 | `GfxWorldDpvsPlanes` cells + `GfxCell` portals + AABB trees (runtime form) |

**Key structural facts (VERIFIED from source):**
- `cbrushside_t` is **16 bytes**: `cplane_s* plane; uint16 materialNum; char firstAdjacentSideOffset; char edgeCount;` (OpenAssetTools `IW4_Assets.h`).
- The whole struct family is confirmed independently by the `iw4L` Rust loader,
  which documents **dual 32-bit-PC vs 64-bit-IW4MS layouts** for `clipMap_t`
  (`sz::CLIP_MAP`, `s.layout(..., 512)` etc.) and reads `cbrushside_t` as
  **16 bytes**, `cNode_t` as **16 bytes**, `cLeaf_t`, `cLeafBrushNode_s` 24 bytes.
- The engine serialises pointers that the loader has to resolve/fix up
  (`fixup_slot`, `asset_ptr_at`, `ptr_at`) — the format is a **zone image with
  pointer fix-ups**, not a self-contained flat file.

**Conclusion:** it is a CoD/IW BSP *derivative*, not the Q3 `IBSP` layout and not
CoAGD (no CoAGD magic present). A Q3/CoD1 BSP reader cannot read it.

---

## 3. What would a converter have to do?

Not a parse-and-rewrite. Concretely:

**Feasible to translate (parse-and-rewrite):**
- Planes: `cplane_s` (20 B) → Q3/CoD1 plane (16 B).
- Brushes: `cbrush_t`/`cbrushside_t` + `ClipMaterial` → CoD1 brushes
  (lump 4) + brushsides (lump 3) + materials (lump 0). Material
  name→surface/content flag mapping is mechanical.
- Entities: `MapEnts` entity string is already text and can be carried across.

**Not recoverable / must be regenerated:**
- **Renderable world geometry.** MW2 keeps it in `GfxWorld`
  (`GfxWorldDraw` vertex/index buffers, `GfxSurface` array, `srfTriangles_t`).
  You can reassemble triangles, but they are **not a BSP**: the Q3/CoD1 node/
  leaf tree (lumps 20/21) that the target renderer walks **does not exist** in
  MW2. OpenAssetTools states this directly for `clipMap_t`/`GfxWorld`:
  *"Compiled map output: a `.map` compiles into it and cannot be recovered."*
  **SOURCED.**
- **Visibility.** MW2's cells/portals/AABB trees (`GfxWorldDpvsPlanes`, `GfxCell`,
  `GfxWorldDpvsStatic`) are a runtime PVS, not Q3's node/leaf + PVS lump 28. To
  feed a Q3-style renderer you must either **(a)** write a renderer that consumes
  MW2's cell/portal/AABB structures directly, or **(b)** **re-run BSP build +
  vis + portal generation** (e.g. a CoD2-style `cod2map`-equivalent) on the
  reassembled triangles.
- **Lightmaps / light grid.** CoD1 uses 512×512 RGB pages in a BSP lump; MW2 uses
  `GfxLightmapArray` + a light grid (`GfxLightGrid`). Remapping is extra work,
  not a copy.

**Measured effort shape:** the closest existing artifact is `iw4L`'s
`fastfile_iw4` (zone parse, pointer fix-ups, asset linking) + `clipmap_iw4`
(collision mesh/BSP/cmodels) + `asset_iw4` (geometry IR) + `fastfile_iw4/load/
gfxworld.rs` (render geometry). That is **multiple crates of hand-reverse-engineered
loading**, built against the retail zones. **INFERRED** estimate: a
geometry-only native loader is on the order of months for one engineer; a true
BSP+vis **converter** is larger and still lossy for lighting/vis.

**Recommendation implied by the evidence:** if MW2 geometry were ever pursued,
**read MW2's structures natively** (as iw4L does) rather than converting to
`IBSP`; converting throws away the visibility/lighting data and forces a rebuild.

---

## 4. Existing open-source MW2 map loaders/converters

Searched GitHub, the CoD research wiki, zeroy wiki, iw4x/XLabs orgs, and the
tool lists. **No tool converts MW2 geometry to Q3/CoD1 `IBSP`.** Existing tools
fall into three classes:

| Tool | What it does with MW2 geometry | License |
|---|---|---|
| **`vladtrc/iw4L`** (candidate repo is a fork `mabaro/iw4L_modernWarfare2`) | **Loads MW2 `.ff` zones natively** and builds geometry/collision in a from-scratch Rust/bevy/wgpu engine (`fastfile_iw4`, `clipmap_iw4`). Proves full geometry portability into a non-Quake engine. | **Apache-2.0** |
| **`Laupetin/OpenAssetTools`** | Unlinker **dumps** `clipMap_t`, `GfxWorld`, `ComWorld`, `GameWorldMp`, `GameWorldSp`, `FxWorld` to **JSON** for IW4; Linker can rebuild. Explicitly **not lossless** back to `.map`. | GPLv3 |
| **`iw4x-x64/OAT`** (fork) | Same, **plus IW4MS** (Microsoft Store x64 release) with clipMap/GfxWorld **link** support. | GPLv3 |
| **`sheilan102/C2M`** (fork of Scobalula's Husky) | Extracts MW2 **map geometry to `.obj`** + entities + model placements, by reading the **running game's memory** (requires retail game running). | GPL |
| **`ZoneTool/zonetool`** | Fastfile linker for CoD4/MW/MW2/MW3; parses/reconstructs IW3/IW4/IW5 zones (incl. `ClipMap`/`GfxWorld` assets). | GPLv3 |
| **`RagdollPhysics/zonebuilder`**, `zonebuilders/zonebuilder` | IW4 fastfile generator/compiler (builds maps *into* `.ff`). | GPLv3 |
| **`iw4x/iw4-open-formats`** | Serializer/deserializer for IW4 asset formats. | GPLv3 |
| **`XLabsProject/iw4-zone-asset-finder`** | Finds which zone holds an asset; source builder. | (C#) |
| **`iw4x-community/iw3x-port`** + `iw4x/iw4x-map-porting-utility` | **Port CoD4 (IW3) → MW2 (IW4)** maps. Wrong direction (not MW2→Q3/CoD1). | GPLv3 |
| **`unrealloco/C2M`, `Mappie`, `KV Map Converter`, `Q3COD4BSP`** | Adjacent converters (some CoD→Source/OBJ, some Source/Q3→CoD4). None do MW2→Q3/CoD1. | varies |

**Nothing converts MW2 `clipMap_t`/`GfxWorld` to `IBSP` v59.** The only two that
preserve geometry faithfully are (a) a **native loader** (iw4L) and (b) a
**JSON dumper** (OAT), and OAT itself says the compiled form is not recoverable
as a map source.

**Directly relevant prior art:** `iw4L` is the candidate project named in the
brief and it is the strongest existence proof — a non-Quake engine reading MW2
geometry. It is also **Apache-2.0**, so its approach (not necessarily its code)
is reusable without copyleft.

---

## 5. Licensing reality

- **Tool licences are mostly GPL (copyleft):** OpenAssetTools, iw4x/iw4x-client,
  zonetool, ZoneBuilder, iw4-open-formats, C2M, iw3x-port — all **GPLv3/GPL**.
  Linking that code into a differently-licensed product forces GPL. (This
  project is itself **GPL-3.0** — `LICENSE`/`NOTICE` — so GPL tools are
  *licence-compatible* here; a permissively-licensed alternative for the loader
  approach is `iw4L`, **Apache-2.0**.)
- **Assets are the real blocker.** Every tool requires an **installed retail
  MW2**; the geometry ships only inside retail `.ff` files (**VERIFIED**: repos
  and IWDs contain no map files). OpenAssetTools states plainly: *"Extracting
  the contents of game files does not grant you any rights to them. All rights
  remain to their respective owners."* **SOURCED.** C2M: *"All BSP data
  extracted … is property of the developers."* **SOURCED.**
- **No `iw_15.dll`/`iw_16.dll` mechanism applies.** **VERIFIED** on this install:
  MW2 PC ships **`iw4mp.exe` (6.25 MB) and `iw4sp.exe` (4.48 MB)** with no
  `iw_*.dll` — geometry is loaded by the game's own code from the `.ff`. So a
  port does **not** get to lean on a shared engine DLL; it must re-implement the
  loader (or require the retail exe at runtime, as C2M does via process memory).
- **Conclusion:** any MW2 geometry port is **gated on the user owning and having
  installed retail MW2**, and cannot legally redistribute the geometry. A
  shipping product that "ports MW2 maps" would ship only the loader and rely on
  the user's retail install — exactly iw4L's model ("Point it at a copy of MW2
  you already own").

---

## 6. Decision inputs (for the owner)

1. **Not a drop-in.** MW2 geometry is not `IBSP`; supporting it means a second
   map pipeline (fastfile → clipMap/GfxWorld/ComWorld/MapEnts → world), not a
   BSP-version tweak in the existing loader.
2. **No converter exists and none can be lossless.** No tool does MW2→Q3/CoD1
   `IBSP`; visibility and lighting are in MW2-specific runtime structures and a
   Q3-style BSP would have to be **rebuilt** (BSP + vis + portals).
3. **The only proven path is native loading**, as demonstrated by `iw4L`
   (Apache-2.0) — consume MW2's own `clipMap_t`/`GfxWorld` cells/portals/AABBs.
4. **Asset-gated.** Requires retail MW2 present; geometry cannot be
   redistributed. Repos ship zero map data (**VERIFIED**).
5. **Given the owner already excludes MW2 maps from the shipping product**, the
   evidence says: keep this as a research curiosity; the cost/benefit does not
   justify a second loader, and there is no cheap converter to lean on.

---

## Appendix — evidence trail

- **Measured here:** retail MW2 `.ff` header (`IWff0100`, ver `0x114` LE),
  authed-chunk decompression to a 107,895,934-byte zone for `mp_boneyard.ff`,
  56-byte zone header, absence of any BSP magic, and the full `.iwd` entry
  census (30,716 entries, no `.bsp`/`.d3dbsp`/`.map`).
- **Read from source:** OpenAssetTools `IW4_ZoneCode.h` / `IW4_Assets.h`
  (`clipMap_t`, `cplane_s`, `cbrushside_t`, `cbrush_t`, `cNode_t`, `cLeaf_t`,
  `CollisionPartition`, `CollisionAabbTree`, `GfxWorld`, `GfxWorldDraw`,
  `GfxSurface`), `ZoneConstantsIW4.h` (version 276, auth-header layout);
  `iw4L` `crates/fastfile_iw4/src/load/{clipmap,gfxworld,world}.rs`,
  `crates/clipmap_iw4/src/{lib,mesh}.rs`.
- **Docs/wikis:** OpenAssetTools `SupportedAssetTypes.md` (IW4 + IW4MS),
  openassettools.dev zone-file reference, zeroy wiki "CoD4: d3dbsp", C2M README.
- **Not verified / caveats:** the exact `mp_boneyard.ff` authed-chunk start
  offset (`0x2015` here) differs from some published notes (`0x4015`); I did not
  enumerate individual `clipMap_t`/`GfxWorld` instance offsets in the zone
  (only the struct layouts and the asset-pool entries), and I did not build any
  tool or run a conversion. No retail file was modified.
