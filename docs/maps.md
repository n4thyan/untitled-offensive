# Custom map pipeline — `mp_toccoa`

How our own maps get built and loaded, and why it works the way it does.

## Short version

```bash
cd tools/toccoa
python validate_spawns.py     # assert spawns are usable
python build_toccoa.py        # layout.py -> out.bsp  (IBSP 59)
python package_pk3.py         # out.bsp + assets/scripts -> main-untitled/untitled.pk3
```

Then `run-dev-match.bat mp_toccoa` (which runs all three for you), or launch
the binaries by hand with `--overlay-dir`.

There is **no Radiant, no external tool and no engine change**. `build_toccoa.py`
emits the CoD1 BSP format directly from a layout description in Python.

## Why this route

The engine already reads CoD1 `.bsp` maps out of `Pk3Fs`, which is simply a
directory of zip archives. So the fastest sound route was to produce a real
CoD1-format BSP and ship it in a pk3 — the same shape retail maps arrive in.
The format is fully documented in `docs/research/bsp-ibsp59-format.md`, which
records byte layouts verified against retail `mp_pavlov.bsp`. Writing an
emitter against that spec turned out to be less work than porting or driving a
Radiant build, and it keeps the map reproducible from source with no external
dependency.

## The three files

| File | Role |
|---|---|
| `tools/toccoa/layout.py` | The map itself — brushes, materials, spawns, signage. This is the design document and the thing you edit. |
| `tools/toccoa/build_toccoa.py` | The BSP compiler. Axis-aligned brushes in, `IBSP` version 59 out. |
| `tools/toccoa/package_pk3.py` | Packs the BSP + the map's GSC into the overlay pk3. |

Plus two supporting pieces:

| File | Role |
|---|---|
| `assets/scripts/mp_toccoa.gsc` | Original per-map script: fog, ambient, factions, soldier models, radar image, retrieval-object keys. |
| `tools/toccoa/validate_spawns.py` | Build-time assertion that every spawn is in open floor with headroom. |

## Asset resolution

The engine resolves assets through a **layered pk3 search path**, added for
this map:

```
--overlay-dir  main-untitled/   ← our content, searched FIRST
--game-dir     C:/Program Files (x86)/Call of Duty   ← retail, read-only
                  └─ main/  (pak0 … pakb)
```

So `mp_toccoa` finds its own BSP and GSC in `main-untitled/untitled.pk3`, and
every texture, model, sound, menu and gametype script resolves from the retail
install. Consequences worth stating plainly:

- **The retail install is never modified.** No file is written into it, ever.
- **No proprietary asset enters this repository.** Textures are *referenced* by
  name, not copied. The `.gitignore` blocks every CoD asset extension, with an
  explicit negation so our own `.gsc` is still tracked.
- **Eventual public distribution is a packaging problem, not an engine
  problem**: a user with their own CoD install gets the map working the same
  way this launcher does.

Inside one pk3 the ordering rule matches retail: archives are sorted, later ones
win, so an overlay archive can override a file an earlier one provided.

## What the BSP writer emits

Axis-aligned boxes only — the primitive set a training-ground blockout needs.
Per `docs/research/bsp-ibsp59-format.md`:

| Lump | Contents |
|---|---|
| 0 | materials — 64-byte name + `surface_flags` + `content_flags` |
| 1 | lightmaps — one all-black 512×512×3 page (vertex-lit via `NO_LIGHTMAP`) |
| 2 | planes |
| 3 | brushsides — six axial sides carrying an f32 bound |
| 4 | brushes — `num_sides` + material |
| 6 | triangle soups — **one per material** |
| 7 | draw verts — `pos[3] uv[2] lm_uv[2] normal[3] rgba[4]`, 44 bytes |
| 8 | draw indices — `u16`, **relative to each soup's `first_vertex`** |
| 27 | models — model 0 owns the whole map |
| 29 | entities — `worldspawn` first, then spawns and intermission |

Two details that cost a build each, and are worth not rediscovering:

1. **One soup per material.** A soup names a single material, and the renderer
   drops a surface whose material it cannot resolve. A single map-wide soup has
   to claim material 0, which by convention is the `clip_nosight` slot — so it
   renders as nothing and the map reports `no drawable surfaces`.
2. **Indices are relative to `first_vertex`.** Writing absolute indices
   overflows every soup after the first; the parser catches it with
   `soup N has an index past its M vertices`.

`content_flags` drives **collision, not appearance**. A material must carry
`CONTENTS_SOLID (0x1)` or `CONTENTS_PLAYERCLIP (0x10000)` for its brush to
collide, and ground slabs use `CONTENTS_TERRAIN (0x4)`. A decorative value here
renders perfectly and then fails with `no collidable geometry`.

## A map also needs a script

A BSP alone is not a map to the server:

```
LoadError { path: "maps/mp/mp_toccoa", line: 0, msg: "script not found" }
```

Retail maps ship `maps/MP/<name>.gsc` beside the BSP, and the server loads it
alongside the stock gametype. `assets/scripts/mp_toccoa.gsc` is ours, written
to the same shape as a stock map script so the retail gametype drives the map
through exactly the path it drives `mp_carentan`: `setCullFog`, `ambientPlay`,
`maps\mp\_load::main()`, faction and soldier-model selection, and the
search-and-retrieve object keys.

## Validating a layout

`validate_spawns.py` exists because two of the first build's bugs were invisible
in the BSP and showed up only as "the bots stand still":

- a spawn sat **inside** the equipment shed's wall, so a bot spent a whole match
  wedged in geometry;
- a crate stack overlapped a wall face for the same reason.

It asserts, for every spawn, that the point is not inside any brush, that there
is a floor within a step below it, and that there is headroom above. It caught
**3 of 8** spawns on first run. Run it before every build; the launcher does.

## Bot navigation on this map

The bots are the map's playtest harness, and they found two real layout faults:

- The obstacle course's flanking walls were originally solid from y=300 to
  y=2600, sealing it into a 2300-unit corridor. Bots entered and could only
  leave by running past the end of a wall; they pressed against it for the whole
  match. Now built in pieces with four gaps per side, aligned with the hurdle
  spacing.
- The Team B compound's benches and sandbag wall blocked its only western exit,
  pinning a bot at x=3129 permanently.

Both are the kind of thing a static map compiles fine and only a moving bot
finds. The lesson is recorded in the comments at each site so the geometry is
not "tidied" back into a wall later.

## Adding a second custom map

1. Copy `layout.py`, give it a new `make_world()`, and a new map name.
2. Register any new materials in `build_toccoa.py`'s `MATERIALS` table — and
   **verify each name resolves** against the retail paks. A wrong name renders
   as the default image rather than erroring, so a typo looks like an
   untextured box. Index the paks' texture stems and check.
3. Copy `assets/scripts/mp_toccoa.gsc` for the per-map script.
4. Add a branch to `package_pk3.py` for the new entries.
5. Run `validate_spawns.py` before playing.

For anything beyond axis-aligned boxes — rotated brushes, curved geometry,
detail meshes — extend `build_toccoa.py`. The emitter is deliberately small and
readable so that is a contained job.