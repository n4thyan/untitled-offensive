# UNTITLED — Session Handoff

A new classic Call of Duty–era FPS built on the vcod fork, running genuine
Call of Duty 1.1 content from the user's own retail installation.

**Direction (2026-10-05): this is not a custom-map project any more.** The stock
CoD maps are the map pool; `mp_toccoa` stays as a dev-only map (its source,
tooling and launcher are intact, and it is in no rotation — it is reachable only
by naming it explicitly). The target is a WWII CoD base with a curated arsenal,
Create-a-Class, perks, a proper scoreboard, killstreaks and killcam.

**End-of-day checkpoint: Create-a-Class is in and loadout application is
unit-verified against the real retail weapon table.** The Phase 1/2 weapon
survey is done and its conclusion is that *no* new weapon is portable today —
see §0.1 and `docs/research/weapon-inventory-and-port-feasibility.md`.

---

# 0.1 — WHY NO NEW WEAPON WAS PORTED

The brief asked for one WWII weapon ported end to end. It was surveyed and is
not doable today, for asset reasons that were verified rather than assumed:

- The CoD 1.1 paks contain **no** shotgun, flamethrower, Gewehr 43, SVT-40 or
  Maxim *def*. Every viewmodel and animation family in the paks maps onto a
  weapon already in configstring 7 — there is no orphaned gun to adopt.
  `weapon_browning30cal`, `weapon_russian_maxim` and `weapon_HiStandard` exist
  as world meshes only: no def, no viewmodel, no animations.
- The user's owned **World at War** install *does* have all of them, but its
  assets live in `IWffu100` linked zones (readable: zlib from offset 12,
  39 MB → 74.8 MB) containing **xmodel v16/v18**. This engine's parser handles
  **v14 only**. That is a second asset pipeline, not a converter.
- The user's owned **Enemy Territory** install has `k43.weap` and
  `flamethrower.weap`, but ET's geometry is `.mdc` (Quake-derived) and its
  weapons are a different schema entirely.

So Phase 3 is blocked on assets and Phase 4 became the work. Full detail,
reproduction commands and the ranked gap list are in
`docs/research/weapon-inventory-and-port-feasibility.md`. **Do not re-survey
this**; if the arsenal is to grow, the cheap route is licence-clean v14 assets,
and the expensive one is writing the WaW v16/v18 zone reader as its own project.

The current arsenal, for the record: 2 pistols, 8 SMGs, 9 rifles, 3 snipers,
1 MG, 2 launchers, 4 grenades, and **no shotgun and no flamethrower**.

---

# 0. END OF DAY — PLAYABLE TONIGHT

```bat
cd C:\Users\natha\projects\untitled
run-dev-match.bat
```

That validates the 8 spawns, rebuilds `mp_toccoa` from `tools/toccoa/layout.py`,
starts a dedicated server with 3 bots, waits, and launches the client already
connected as **allies** with an **m1carbine_mp**. Click the window to capture the
mouse.

Optional arguments still work:

```bat
run-dev-match.bat mp_toccoa dm 3     REM the default
run-dev-match.bat mp_toccoa tdm 4
run-dev-match.bat mp_carentan dm 3
```

`stop-dev-match.bat` shuts both processes down. Logs land in `dev-logs\`.

## Verified on Toccoa

| | |
|---|---|
| client connected | **yes** |
| human answered the menus | **yes** — serverinfo → team → weapon, all three |
| human spawned | **yes** — `pm_type=Normal` at (3000, 2760, 8) |
| bots spawned | **yes** — all 3 reach `Normal` |
| movement | **yes** — bots traverse; human movement is client-side and unmodified |
| weapons | **yes** — m1carbine_mp (allies), kar98k_mp (axis) |
| damage | **yes** — `D;` events with real hit locations and damage values |
| **bot kill observed** | **yes — 4 kills / 7 damage events in 90 s** |
| human kill observed | no — the test client sits still; not driven by input here |
| death | **yes** — every kill runs `player_die` and the obituary callback |
| respawn | **yes** — kills are followed by fresh spawns in the same match |
| DM | **yes** |
| TDM | **no — see the bug below** |

### The "no kills on Toccoa" note from last session was wrong

It was an instrumentation gap, not a map fault. Last session's probes watched
for `Item:` lines, which are not spawn indicators. Reading the actual `K;`
obituary events shows Toccoa produces kills at a normal rate: **6 kills in ~45 s**
bots-only, **4 kills in 90 s** in the final regression run. No map change was
needed. The earlier "358 distinct bot positions / 113 samples with damage" figure
was real too — combat was working; it was just not being counted correctly.

## TDM on Toccoa is broken — a real, separate bug

`run-dev-match.bat mp_toccoa tdm 4` **does not work**: the server enters a
restart loop, logging `Aborting level - gametype is not supported` roughly 20
times a second, and no match starts.

What is established:

- **TDM itself is fine.** `mp_carentan` + tdm runs clean — both teams assigned,
  weapons chosen, **1 kill in 70 s**, 0 errors, 0 aborts.
- **DM on Toccoa is fine** (above).
- **It is Toccoa-specific**, and **not caused by our map script.** Replacing
  `assets/scripts/mp_toccoa.gsc` with a 9-line minimal script changed nothing —
  still 892 aborts in 45 s.
- So the cause is either the Toccoa BSP's entity list or our handling of it under
  a team gametype.

**Best current hypothesis:** the BSP only carries `mp_deathmatch_spawn` entities.
TDM needs team-specific spawns, and a retail TDM map declares them differently. The
open question is whether the fix belongs in `tools/toccoa/layout.py` (emit the
spawn classes TDM expects) or in the server (accept DM spawns under TDM the way
retail does). That was not resolved tonight and is deliberately left as the first
item for the next session.

Not a regression: DM on Toccoa worked before this session and still does.

## Map

- **Dimensions** ~4000 × 3400 units, open sky. 94 brushes, 1104 triangles.
- **Spawns** 8 (4 per team), all validated clear, floored and with headroom.
- **Geometry changed this session:** none. No tuning was required — the map was
  already producing kills once counted correctly. The three layout faults found
  yesterday (sealed obstacle corridor, Team B's blocked west exit, 3 buried
  spawns) remain fixed by that session's work.
- **Largest remaining layout weakness:** bots still navigate by bearing plus
  jitter rather than pathfinding, so on a map with more interior walls than
  Carentan some take long routes. Cross-route flanking is designed on paper but
  unverified in play.

## Visual checks for Nathan

My model has no image input and the vision tool is rate-limited, so **nothing
here is confirmed by eye.** Renderer statistics and server logs are the evidence.
Please check:

1. **Does `mp_toccoa` look like a place?** Textures resolve (18 loaded, 0 missing)
   and geometry renders, but nobody has seen it. Texture scale (`1/32`) is a guess.
2. **Are the training signs legible and correctly facing?** They are thin panels;
   orientation was never checked visually.
3. **Does the layout read as three routes?** The cross-connections were designed on
   paper; whether they enable flanking in play is unverified.
4. **Is the instructor tower reachable and appropriately exposed?** The stair
   blocks are a guess at climbability.
5. **Any obviously ugly or floating geometry?**

## Tests

| Suite | Result |
|---|---|
| `cargo fmt` | applied |
| `cargo build --release` | clean, no warnings |
| `cargo test --release -p vcod-server` | **514 passed, 0 failed** |
| `python validate_spawns.py` | 8/8 clear, floored, headroom |
| `mp_toccoa` dm, 3 bots, 90 s | **4 kills, 7 damage events** |
| `mp_toccoa` tdm | **fails** — abort loop (above) |
| `mp_carentan` dm, 3 bots | boots clean, 0 aborts |
| `mp_carentan` tdm, 4 bots | boots clean, 1 kill, 0 errors, 0 aborts |

`cargo test -p vcod-server` is reported in §14.

## Git

- Checkpoint commit: **`92e5689`** `feat: data-driven Create-a-Class, and the
  weapon survey that precedes it`, on top of `ad32f34`.
- Working tree clean at that commit.
- **pushed: NO** — local only.
- **No proprietary data tracked.** `.gitignore` blocks every CoD asset extension
  (`*.pk3`, `*.bsp`, `*.xmodel`, `*.xanim`, `*.gsc`, `*.tga`, `*.dds`, `*.wav`).
  The four new `tools/re/*.py` read the retail install and write only to the
  scratch directory.
  The generated overlay is also ignored, with an explicit negation so our own
  `assets/scripts/*.gsc` is versioned. Textures are referenced by name, never
  copied.

---

# 1. CURRENT STATE — what is actually playable

**Map:** `mp_toccoa` — a live-fire training course at Camp Toccoa.

### Built how

No Radiant, no external tool. `tools/toccoa/build_toccoa.py` emits CoD1's
`IBSP` version 59 directly from a layout description in Python, and
`package_pk3.py` packs the BSP plus the map's GSC into an overlay pk3.

```bash
cd tools/toccoa
python validate_spawns.py     # assert every spawn is usable
python build_toccoa.py        # layout.py -> out.bsp
python package_pk3.py         # -> main-untitled/untitled.pk3
```

`run-dev-match.bat mp_toccoa` runs all three automatically.

| | |
|---|---|
| Layout source | `tools/toccoa/layout.py` |
| BSP compiler | `tools/toccoa/build_toccoa.py` |
| Packager | `tools/toccoa/package_pk3.py` |
| Compiled output | `main-untitled/untitled.pk3` → `maps/MP/mp_toccoa.bsp` (generated, gitignored) |
| Map script | `assets/scripts/mp_toccoa.gsc` (original content) |
| Pipeline doc | `docs/maps.md` |

### Asset resolution

A layered pk3 search path, `--overlay-dir`, searched **before** the retail
`main`:

```
--overlay-dir  main-untitled/                              ours, first
--game-dir     C:/Program Files (x86)/Call of Duty   retail, read-only
```

`mp_toccoa` gets its BSP and GSC from ours; every texture, model, sound, menu
and gametype script resolves from retail. **The retail install is never
modified and no proprietary asset is copied into the repository** — textures
are referenced by name.

### Assets used

**CoD1** — 20 materials, every one verified to resolve against the paks:

| Role | Path |
|---|---|
| ground | `textures/normandy/ground/dirt@earth_2` |
| mud / trench | `textures/normandy/ground/dirt@trench_goundalt1` |
| turf | `textures/normandy/ground/dirt@earthgrass_lpc` |
| wood siding | `textures/austria/walls/wood@barracksext_siding1` / `_siding2slat` |
| sandbags | `textures/austria/walls/wood@barracksext_siding2mldy` |
| crates | `textures/crates/wood@cratepile1` |
| concrete | `textures/belgium/walls/concrete@bunkerint` |
| metal | `textures/industrial/metal@corrugatedsteel` |
| roof | `textures/normandy/walls/wood@siding_gray` |

**Toccoa's own training signage** — this is what makes the map read as a
training camp rather than generic boxes. The retail training level names each
drill on a sign, and all of these ship in `textures/training/`:

```
sign_obstaclecourse   sign_grenadetraining   sign_jumptraining
sign_tower_e          sign_barracks          sign_covertocover
sign_weaponstraining  metal_masked@hogwiredark (the HOG wire)
wood@testtarget                              (target board)
```

**ET assets used: none** — see §16.
**Project-owned:** the layout, the BSP compiler, `assets/scripts/mp_toccoa.gsc`.

### Dimensions and layout

Roughly **4000 × 3400 units** (a CoD player is 56 units tall), open to the sky —
between Killhouse and Nuketown scale, larger than Shipment.

Two protected spawn compounds, diagonal corners, 4 spawn points each:

- **Team A — barracks yard** (south-west): barracks hut with a doorway,
  equipment crates, sandbag lip, walled on its outer sides.
- **Team B — equipment/firing yard** (north-east): metal shed, firing benches,
  crates, sandbag position. Equivalent value, deliberately *not* a visual mirror.

Three interconnecting routes:

- **Route A — obstacle course** (west): five zig-zag hurdles, sandbag chicane,
  HOG-wire crawl-under, flanking walls in four pieces with four gaps per side.
- **Route B — central training yard** (middle): the **instructor/observation
  tower** — four corner legs, platform at 620, rail on two sides only, stair
  blocks on one approach. Deliberately not a sniper nest: exposed, limited
  cover, several counters. Low barricades and crates break sightlines.
- **Route C — firing / grenade course** (east): three recessed firing bays with
  benches, a roofed grenade structure with a window band, a demolition practice
  wall, a target board anchoring the long sightline.

**Cross-connections** create loops so fights are not always frontal: course ↔
central yard, central yard ↔ firing route, a far-north link behind both, and
perimeter cross-links.

### Tested

| | Result |
|---|---|
| `mp_toccoa` dm, 2 bots | loads; bots spawn `pm_type=Normal`, move, fight |
| `mp_toccoa` dm, 4 bots | **358 distinct bot positions**, **113 samples with damage taken**, health observed at 60 from real hits |
| `mp_toccoa` render | 18 batches, 3384 draw indices, **18 textures loaded, 0 missing**, collision present |
| `mp_carentan` regression | unchanged: 248 batches, 81 textures, 0 missing |
| `mp_toccoa` tdm | **not tested** |
| Client joining `mp_toccoa` | **not tested** — see §9 |

Renderer clean, collision correct (ground `CONTENTS_TERRAIN 0x4`, structures
`CONTENTS_SOLID 0x1`).

### Layout problems found

All three were invisible in the BSP and only showed up as bots standing still:

1. The obstacle course's flanking walls were originally solid y=300→2600,
   sealing it into a 2300-unit corridor. Bots entered and pressed against a wall
   for an entire match. Now built in pieces with four gaps per side.
2. The Team B benches and sandbag wall blocked its only western exit, pinning a
   bot at x=3129 permanently.
3. Three of eight spawns were buried in geometry. `validate_spawns.py` now
   asserts every spawn is in open floor with floor below and headroom above; it
   caught all three on first run and runs on every build.

**Still open:**

- **No kill observed on Toccoa.** Damage lands and bots traverse the map, but in
  the ~4-minute instrumented runs none died. `mp_carentan` kills reliably with
  the same code, so this is likely map geometry or LOS density rather than a
  regression — unproven, and the first thing to chase.
- Bots still navigate by bearing + jitter, no pathfinding; some take long routes.
- Spawn-safety is not measured the way the brief asks (deaths immediately after
  spawn, time to first contact, route usage). No instrumentation exists.

### Engine changes made for this

- **`Pk3Fs::open_layered(mod_dir, overlay)`** — searches an overlay pk3 directory
  before the base. Within a directory archives sort and later wins, matching
  retail. A missing overlay is normal, not an error.
- **`--overlay-dir`** on both `vcod.exe` and `vcod-server.exe`.
- **Two bot navigation fixes**, so bots traverse a map with interior walls:
  a bot re-picks its heading while still closing on a nav target rather than
  only every 20 ticks (which had them arrive and then stand still); and the
  anti-stuck jitter now applies *only when genuinely stalled* — averaged over a
  pick-per-tick bot it cancelled itself out and made bots circle their target
  instead of closing on it.

Full detail in `docs/maps.md`; ET analysis in `docs/et-inventory.md`.

---

## 1. CURRENT STATE — what is actually playable

Verified by running the binaries, not by reading code:

- A Windows release build compiles clean (`cargo build --release`, zero errors,
  zero warnings).
- A dedicated CoD1 server boots on a retail map with bots.
- Bots **spawn, take teams, pick weapons, move, shoot, take damage, die and
  respawn.**
- A human client connects to that server and auto-answers the join menus.
- `mp_carentan` renders with real CoD1 content: 248 batches, 335 058 draw
  indices, 81 textures (0 missing), 5 lightmap pages, 33 773 prop triangles,
  a real sky, and a first-person viewmodel.

### The verified kill, in detail

Instrumented run, 4 bots, `mp_carentan`, gametype `dm`. Bot 3's state over
time:

```
playing=true dead=false clip=5  enemy=None      spawning, crossing the map
playing=true dead=false clip=4  enemy=Some(...) acquired a contact
playing=true dead=false clip=3  enemy=Some(...) firing
playing=true dead=false clip=2
playing=true dead=false clip=1
playing=true dead=false clip=0                  magazine emptied
playing=false dead=true weapon=0 clip=-1        KILLED
playing=true dead=false clip=5                  RESPAWNED, new weapon
```

Shots fired and resolved (real weapon defs, real hit locations):

```
attacker=1 weapon=9  HIT victim=3 damage=96 hitloc=torso_lower
attacker=2 weapon=12 HIT victim=3 damage=40 hitloc=torso_upper
```

136 damage against 100 HP — lethal, and the kill landed through the stock CoD1
`CodeCallback_PlayerKilled` script path.

The debug instrumentation that produced this has been removed. Nothing above
is claimed from logs that no longer exist; it is what the server printed.

---

## 2. HOW TO RUN

### One-click development match

```
C:\Users\natha\projects\untitled\run-dev-match.bat
```

Starts the dedicated server, waits for it to load, then launches the client
connected to it. Logs land in `dev-logs\`.

```
run-dev-match.bat [MAP] [GAMETYPE] [BOTCOUNT]
run-dev-match.bat                        REM mp_toccoa, dm, 3 bots (default)
run-dev-match.bat mp_toccoa tdm 4
run-dev-match.bat mp_carentan dm 3
```

For `mp_toccoa` it first validates spawns and rebuilds the map from source, then
starts the server with `--overlay-dir` and launches the client. Logs land in
`dev-logs\`.

`stop-dev-match.bat` shuts both processes down.

### By hand

```bash
# server
./target/release/vcod-server.exe --game-dir "C:/Program Files (x86)/Call of Duty" \
    --overlay-dir main-untitled --gametype dm --bots 3 --bots-shoot --port 28960 mp_toccoa

# client (separate shell)
./target/release/vcod.exe --game-dir "C:/Program Files (x86)/Call of Duty" \
    --overlay-dir main-untitled --connect 127.0.0.1:28960 --team allies --weapon m1carbine_mp
```

### Offline, no server

```bash
./target/release/vcod.exe --game-dir "C:/Program Files (x86)/Call of Duty" \
    --overlay-dir main-untitled --walk mp_toccoa
```

---

## 3. CONTROLS

| Input | Action |
|---|---|
| Mouse | Look (click the window to capture, `Esc` to release) |
| `W` `A` `S` `D` | Move / strafe |
| `Space` | Jump (again to stand) |
| `C` | Crouch |
| `Ctrl` | Prone |
| `Q` / `E` | Lean |
| `Shift` | Walk |
| `F` | Use |
| `LMB` | Fire / melee |
| `RMB` | Aim down sights |
| `R` | Reload |
| `1`–`4` | Weapon slots |
| Wheel | Next / previous weapon |
| `M` | Script menu |
| `F3` | Debug overlay |

---

## 4. MAPS

**Retail CoD1 maps that load:** all 13 multiplayer maps are visible and load —
`mp_carentan`, `mp_ship`, `mp_bocage`, `mp_stalingrad`, `mp_tigertown`,
`mp_powcamp`, `mp_hurtgen`, `mp_pavlov`, `mp_rocket`, `mp_brecourt`,
`mp_depot`, `mp_dawnville`, `mp_carentan`, plus the single-player maps.

**Actually exercised:** `mp_carentan` (kill + respawn verified, server and client)
and **`mp_toccoa`** (server + bots; client not yet tested).

**Custom maps:** `mp_toccoa` — see §0 and `docs/maps.md`. Built from
`tools/toccoa/layout.py`; run-dev-match.bat compiles it on demand.

---

## 5. WEAPONS

Working: the full retail loadout, driven by the actual CoD1 weapon files.

Bots have been observed carrying **M1 Carbine** (allies, weapon index 12) and
**Kar98k** (axis, index 9), selecting them from the stock weapon menu.

Firing, reload, ADS, ammo and clip are implemented and were exercised by the
bots: clip counts decrement to zero, the bot reloads, and magazines refill on
respawn.

---

## 6. ENEMIES

Working: server-side bots that

- join through the stock CoD1 menus (team menu, weapon menu, server-info menu)
- spawn at real map spawn points
- move with real `pmove` usercmds
- acquire enemies by line-of-sight trace
- navigate toward contacts they cannot yet see
- fire real bullets that land real hits with real hit locations
- take damage, die, and respawn

**Placeholder / crude:** navigation is "walk at the bearing, with jitter".
There is no pathfinding, so a bot can still get pinned on geometry for a
while before its stall check and jitter work it loose. It does eventually, but
a bot may take a long route.

---

## 7. CoD FIDELITY

**Faithful (measured from retail, not invented):**

- Movement constants: gravity `800.0`, friction `PM_FRICTION 5.5`,
  `HEIGHT_CROUCH 50.0`, `VIEW_CROUCH 40.0` — recovered retail values.
- The whole GSC script pipeline runs the **unmodified stock CoD1 1.1
  gametype scripts** out of the retail paks. Spawns, weapons, death, respawn
  and scoring are retail logic, not reimplementations.
- The menu join sequence is answered through the real `openMenu` / `mr`
  protocol.
- Combat damage goes through the real `CodeCallback_PlayerHit` callback with
  real hit locations (`torso_lower`, `torso_upper`).

**Approximate:**

- Bot navigation (see above).
- Some CoD1 material effects are approximated — the client logs 177 shader
  library warnings for unsupported tokens (`unknown rgbGen`, `perlight`,
  `tcGen`, `animMap without frames`, `clampY approximated`). The map renders;
  these are visual niceties, not blockers.

---

## 8. QUAKE MOVEMENT

**Nothing implemented yet.** Movement today is straight recovered CoD1. This is
deliberately the next task after the custom map, and it must be additive —
the retail constants are the default and reference implementation, with the
Quake-style extensions behind cvars.

---

## 9. ENGINE DECISION

**vcod** — <https://github.com/janost/vcod>, forked at commit `e66775f`.

Chosen over iocod and OpenCoD1 because it was the only candidate that built
first try (4m27s, zero errors) *and* already had CoD1 movement, collision, a
viewmodel, HUD, a GSC VM, a CoD1.1 **dedicated server**, bots, and 23 verified
reverse-engineering documents on CoD1 wire formats and gameplay.

### Licensing

GPL-3.0-or-later, deliberately chosen. The upstream NOTICE records ports from
Quake III Arena (GPL-2+) and Return to Castle Wolfenstein multiplayer (GPL-3+
with additional terms), plus a triangle-strip decoder from `cod-asset-importer`
(GPL-3.0). All preserved. This is compatible with the id Tech / CoD-lineage
code we intend to borrow later.

---

## 10. REFERENCES USED

| Repo | Revision | Use |
|---|---|---|
| [janost/vcod](https://github.com/janost/vcod) | `e66775f780490967f4574b2e81c8572d383df6a1` | Engine base — whole project |
| [id-Software/Quake-III-Arena](https://github.com/id-Software/Quake-III-Arena) | via vcod NOTICE | Movement/netcode lineage (GPL) |
| [id-Software/RTCW-MP](https://github.com/id-Software/RTCW-MP) | via vcod NOTICE | pmove/collision ports (GPL) |
| [mauserzjeh/cod-asset-importer](https://github.com/mauserzjeh/cod-asset-importer) | via vcod NOTICE | XModel triangle-strip decoder (GPL-3.0) |

The retail install's own `.gsc` scripts were read directly from the paks to
establish the exact join sequence (`maps/MP/gametypes/dm.gsc`,
`_teams.gsc`, `maps/MP/_utility.gsc`).

---

## 11. PROPRIETARY DATA

Read-only source, never modified:

```
C:\Program Files (x86)\Call of Duty   (CoD 1.1, pak0–pakb)
```

`.gitignore` blocks `*.pk3`, `*.bsp`, `*.xmodel`, `*.xanim`, `*.gsc`, `*.tga`,
`*.dds`, `*.wav` and the install-path directories. No extracted asset is
written into the repository — content is read directly from the retail paks at
runtime. `dev-logs/` is gitignored too.

**Confirmed: no proprietary data is tracked by git.**

---

## 12. TESTS

- `cargo build --release` — clean, no warnings.
- `cargo fmt` — applied.
- `cargo test --release` — see §16 for the final result.
- One upstream test file (`crates/common/tests/shader_corpus.rs`) did not
  compile on Windows: it called `std::os::unix::fs::symlink` unconditionally.
  That is a pre-existing upstream bug, not a regression from this work. Fixed
  behind `#[cfg]`, using a plain file copy on Windows, so the suite runs here.

---

## 13. REMAINING MISSING GSC BUILTINS

Down from 11 at the start of this session to **7**. None of them block a
playable match:

```
addtestclient         allowspectateteam   attackbuttonpressed
iprintlnbold          pingplayer          sayall
sayteam
```

Resolved this session: `getGuid`, `getAttachIgnoreCollision`, `takeWeapon`
(plus the `WeaponOp::Take(u8)` playerstate path).

`attackbuttonpressed` and `sayteam`/`sayall` are the ones most worth
implementing next — the first two affect bot behaviour and team comms.

---

## 14. KNOWN BUGS / LIMITATIONS

1. **No visual confirmation.** The active model has no image input and the
   vision tool is rate-limited, so "it renders" is evidenced by renderer
   statistics and window enumeration, never by a screenshot.
2. **Client view not visually verified.** The client connects and answers the
   menus; the server confirms `client 3` joined. Whether the player is
   correctly posed in the world rather than the spectator camera has not been
   confirmed by eye.
3. **Bot navigation is crude** — bearing + jitter, no pathfinding.
4. **Bot accuracy is low.** They converge and land hits, but a 16-shot sample
   produced only 2 player hits. Fine for a combat loop; not good AI.
5. **`begin (spectator)` is misleading in logs.** It is logged before the
   menus resolve. Bots spawn correctly despite it. Cosmetic logging issue.
6. **Shader coverage is partial** — ~177 lib warnings for unsupported CoD1
   material tokens. Renderable, not exact.
7. **TDM on `mp_toccoa` aborts the level** in a restart loop — see §0. DM is
   fine, and TDM is fine on `mp_carentan`.
8. **No visual confirmation of the custom map.** See §19 — texture scale, sign
   placement and tower climbability are all unverified guesses.

---

## 15. GIT

Checkpoint commits:

```
ceb7a0c3a25d33ffb73cfa0e6264a31874bdb9a5   Add playable Toccoa training-ground map
7ed14c16bb79e82a324cb470cdeca246bb338da0   Playable CoD1 bot combat checkpoint
```

Working tree clean. **Nothing pushed anywhere — local only.**

---

## 16. TEST RESULTS

| Suite | Result |
|---|---|
| `cargo build --release` | clean, no warnings |
| `cargo fmt` | applied |
| `cargo test --release` (server) | **399 passed, 0 failed** |
| `cargo test --release` (common) | **635 passed, 2 failed** |
| `cargo test -p vcod-common --lib net::snapshot` (debug) | **18 passed, 0 failed** |
| `mp_carentan` dm, 4 bots | kill + respawn verified |
| `mp_carentan` tdm, 4 bots | boots clean, 0 errors, 0 missing builtins |
| `mp_toccoa` dm, 3 bots, 90 s | **4 kills, 7 damage events** |
| `python validate_spawns.py` | 8/8 spawns clear, floored, headroom |

The 2 release failures are **pre-existing upstream test-design issues, not
regressions**: both are `#[should_panic]` tests asserting on `debug_assert!`
(`snapshot.rs:1171` and `:1199`), which release builds strip. Both pass under
the debug profile, which is how the assertions are meant to be checked. Left
alone deliberately — "fixing" them would mean weakening a real guard to suit a
profile it was never written for.

---

## 17. NEXT SESSION

**Superseded 2026-10-05 — do not fix TDM on `mp_toccoa` today.** The direction
changed: `mp_toccoa` is a dev-only map and TDM on it is explicitly out of scope.
Its source, tooling and launcher are untouched; only its priority is.

### Where each phase stands

| Phase | Status |
|---|---|
| 1. Weapon inventory | **done** — full table in §0.1 and the research doc |
| 2. Port feasibility | **done** — every candidate blocked on assets; conclusion recorded |
| 3. Port one weapon | **blocked** on assets, not on effort. Do not retry without licence-clean v14 assets |
| 4. Create-a-Class | **core done and unit-verified**; not yet proven in a live match (see §16.1) |
| 5. Scoreboard / stats | not started |
| 6. Killstreaks | not started |
| 7. Killcam | not started (existing `crates/server/tests/killcam.rs` is the prior art) |

### The concrete next task

**Prove Create-a-Class in a running match, then commit that as the feature
checkpoint.** Specifically:

1. Start the server with `--classes-dir assets/classes` on a **stock** map
   (`mp_carentan dm`), drop a `vcod.txt` profile into that directory naming a
   Thompson class, and connect the real client. Confirm the player spawns
   holding the Thompson and not the carbine. This is the one thing the class
   system still needs — everything below it is covered by tests.
2. Then pick the class with a UI affordance. Right now a class is chosen by
   which file the player's name matches, which is server-driven and works, but
   a client-side menu is the real feature. `ScriptRuntime` already has
   `give_weapon_with_ammo` and the spawn hook; the missing piece is the menu and
   a way to save.
3. After that, **Phase 5 (scoreboard and player stats)** is the highest-value
   unblocked work, and it feeds killstreaks directly.

Phase 4's remaining scope, in order: client-side class menu and saving, then
perks doing something (the slots are parsed and validated but are inert), then
XP/unlocks — which must **not** be built yet.

---

## 16.1 — CREATE-A-CLASS: WHAT IS BUILT AND WHAT IS PROVEN

Built this session:

| File | What |
|---|---|
| `crates/common/src/classes.rs` | Profile/class data model and the line-based text parser. 7 unit tests |
| `crates/server/src/classes.rs` | Profile store, name-keyed lookup, weapon/perk validation against the live table, loadout resolution. 9 unit tests |
| `assets/classes/default.txt` | The embedded default profile: Rifleman, Assault, Support |
| `ScriptRuntime::apply_loadout` | Takes the stock loadout (`TakeAll`) then gives the class's weapons, primary as spawn weapon |
| `Server::load_classes` / `--classes-dir` | Profile directory wiring; a missing directory is not an error |

Design decisions, so they are not relitigated:

- **Server-authoritative.** A class names weapons by string; the server resolves
  each against its own weapon table and hands out only what resolves. A client
  cannot request a weapon the server will not give it.
- **Fails safe.** An unknown weapon is dropped and the slot is logged, not
  trusted and not fatal; the rest of the class still applies. This is what makes
  a profile written against a different build playable rather than broken.
- **Hand-editable line format, not JSON/TOML**, matching the engine's existing
  tokenizer style.
- **Applied after the spawn pass, before the weapon mirror** — so the spawn's
  playerstate reset cannot wipe the clips just written, and the frame's snapshot
  already carries the class.
- **Perks are validated but inert.** Names are checked against `KNOWN_PERKS`
  and unknown ones are reported. No perk effect is implemented. Deliberate.

Proven by tests:

- Profile parsing, case-insensitive lookup, per-name resolution — 9 server + 7
  common unit tests, all passing.
- **`apply_loadout` against the real retail weapon table**: the class's three
  weapons are held, the stock carbine is taken, its ammo array is cleared, the
  Thompson's own reserve and full clip are written, and the primary becomes the
  spawn weapon (`script.rs`,
  `a_loadout_replaces_the_stock_weapons_and_ammo`).

**PROVEN IN THE REAL GAME.** The human player launched
`run-dev-match.bat mp_carentan dm 3`, saw mp_carentan rendered in first
person with the class loadout applied — HUD `Thompson / Full_Auto`, ammo
`30 | 360` — and killed bots and was killed by them. Create-a-Class, the
menu join, spawn, combat and death/respawn all work in the actual client.
See §0.2 for the full record and the commits.

## 0.2 — PLAYABLE ON A STOCK MAP, WITH CREATE-A-CLASS

Confirmed **2026-10-05** by the human player, on screen:

- window `vcod — mp_carentan`, map visibly rendered, first-person view;
- HUD `Thompson / Full_Auto`, ammo `30 | 360`, health and match timer up;
- bots present; the player killed them and was killed by them;
- the M1 Carbine the weapon menu was answered with was **not** in hand.

So the whole chain works against the real client: connect, gamestate,
the 1.5 three-menu handshake, spawn, class loadout, combat, death,
respawn. `assets/classes/vcod.txt` is the profile; it is keyed to `vcod`,
the name `NetClient::send_connect` hardcodes, so it applies with no extra
wiring, and the Thompson is chosen because it is unmistakable next to the
carbine.

Launch it with:

```
run-dev-match.bat mp_carentan dm 3
```

### What the logs say that looks alarming but is not

`client 3 "vcod" begin (spectator)` is **not** a live status. It is emitted
once, unconditionally, at client entry, before the client has answered a
single menu, so it describes the starting state. It read as a live status
and sent us hunting a join bug in code that was already correct, while the
player was visibly in the map holding a class weapon. Reworded at
`f26409c` to `begin (starting as spectator)`. The live state is `pm_type`,
and the transitions out of spectator are logged where they happen.

## 0.3 — WHY MULTIPLAYER JOIN WAS BROKEN, AND WHY IT WAS NOT A UO MERGE

**Supersedes the "merged CoD1 + United Offensive install" story below, which
was wrong.** This install is stock CoD content: 16 canonical maps, no UO map
or content set. The old "514 passed" figure was recorded *without* `COD_DIR`
set, so the retail-dependent tests silently skipped and proved nothing.

The real cause was two related things:

1. **The gametype script is CoD 1.5's, not 1.1's.** pak5 carries 1.1's
   `dm.gsc`; pak8/pak9 carry 1.5's, and pak9 is last in the pak sort order
   so its file resolves. 1.5 opens a `serverinfo_<gametype>` menu *before*
   the team menu and blocks on `waittill("menuresponse")` until the client
   answers `"close"`. Answering nothing parks the join silently.
2. **Menu identity must come from the menu's own configstring**, at
   `1180 + index`, never from the `g_scriptMainMenu` cvar. Under 1.5 that
   cvar still names the *team* menu while the *serverinfo* menu is open,
   because a menu's index comes from the gametype's `precacheMenu` order.

Read straight out of the two retail scripts:

```
pak5 (1.1): 0 menu_team       1 menu_weapon_allies  2 menu_weapon_axis ...
pak9 (1.5): 0 menu_serverinfo 1 menu_team            2 menu_weapon_allies ...
```

`crates/client/src/play/join.rs` already did this correctly. The
integration harness in `crates/server/tests/common/mod.rs` did not, and
was fixed at `179c285`.

## 0.4 — THE FIXTURES THAT WERE ENCODING CoD 1.1 ASSETS

**The "Pk3Fs inflates model data wrongly" theory was disproven and should not
be revisited.** The evidence: `Pk3Fs` returns bytes whose CRC32 matches the
*resolving* archive's central-directory entry exactly — pak8's `0x938b319b`
for `xmodelparts/USAirborne3`, which is not pak0's `0x331e6b3b`. Both
archives match their own CRCs, and the `zip` crate verifies CRC on read, so
a bad inflate could not have passed silently. The byte differences that
started this were pak8's 1.5 model set against pak0's 1.1 one, hand-compared
without noticing they were different files.

CoD 1.5's paks ship replacements that override pak0, and six fixtures encoded
the 1.1 values. All corrected at `773dd20` against the real archives:

- `rifleBullet 1` for the Kar98k, Enfield and Mosin (pak0 says 0);
- `ui_allow_*` menu gates, where pak8 renames pak0's `scr_allow_*`;
- 1.5's `precacheMenu` puts `menu_serverinfo` at 1180;
- `g_gametype` must be set or the menu is literally named `serverinfo_`;
- `xmodelparts/USAirborne3`: `back_up` 5 not 4, `tag_weapon_right` 10 not 18,
  and a re-authored thigh box;
- two `should_panic` tests guard `debug_assert!` and cannot pass under
  `--release`, so they are `cfg_attr(not(debug_assertions), ignore)`.

**Current real-data result: 1039 passed, 1 failed.** The one failure is
`skeleton::tests::real_models_pose_sanely`, a first-person *viewmodel*
bind-pose offset (`[-9.008 1.987 7.821]` where identity is expected). It
fails identically at `ad32f34`, is pre-existing, and reads nothing in the
spawn, class, hit-registration or blast path. **Parked** unless it becomes
visibly wrong in play.

## 0.5 — WHAT THE NEXT SESSION SHOULD ACTUALLY DO

The foundation is done and verified in the real game. In priority order:

1. **Class selection UI.** `assets/classes/*.txt` is the proven backend; the
   first playable class is currently always the active one. Build the
   in-game `CREATE A CLASS` screen (Rifleman / Assault / Support / Custom 4-5),
   server-validated, persisting, affecting the next spawn. Do not rewrite the
   class architecture — add a selection path on top of it.
2. **Scoreboard.** `crates/client/src/hud/scoreboard.rs` (695 lines) already
   implements a retail-accurate Tab scoreboard off the `b` command, and the
   player has seen it working. What is missing is the **WaW/MW2 presentation**:
   it renders score / ping / time only. `score` and `deaths` are real
   `ClientState` wire fields (`game/fields.rs`, offsets 8416 and 8420);
   kills and assists are not, so they need a stats model behind it. Keep that
   model separate from rendering — it later feeds killstreaks, the post-match
   screen, and persistent stats.
3. Player-stat persistence, then the killstreak framework (3 Recon,
   4 Counter-Recon, 7 Attack Dogs), then killcam. Read
   `crates/server/tests/killcam.rs` before designing killcam; the player has
   already seen it run and reported it "looks a bit funny", so there is a real
   visual issue to look at.

Do not spend the session on Toccoa, custom maps, MW2 ports, ET/WaW asset
converters, weapon re-research, or the parked bind-pose test.

---

## 18. ET INVENTORY — investigated; the firearms are there, the format is not

Full findings in `docs/et-inventory.md`. **Superseded in part on
2026-10-05 — see §0.1.** The section below originally concluded ET held no
firearms worth having. That was wrong on one point and it matters: **ET does
have a Gewehr 43 (`k43.weap`) and a flamethrower (`flamethrower.weap`)**, both
genuinely absent from CoD 1.1, and both on the "should we have this" list in
§0.1. The *conclusion* stands anyway, for a different reason: ET's `.mdc`
geometry and `.weap` schema are unrelated to CoD 1.1's xmodel/weapon-def
pipeline, so neither is importable without writing a converter plus a
viewmodel, for one gun each.

Keeping this section because the format reasoning is sound and worth not
rediscovering. ET is installed (`...\Steam\steamapps\common\Wolfenstein
Enemy Territory`, 3763 entries: 1167 sounds, 961 models, 645 textures, 117
scripts).

**It shares no weapon format with CoD1.** ET weapons are `.mdc` meshes with
`.weap` text definitions (Quake II / RTCW lineage); CoD1 weapons are `.xmodel`
with weapon `.gsc` and animscripts (IW lineage). An ET weapon cannot be
referenced by a CoD1 map, put in a CoD1 pk3, or loaded by this engine at all.

Cross-referencing ET's 40 `.weap` definitions against CoD1's 32:

**Already covered by CoD1 — do not import duplicates.** `colt`, `luger`,
`silenced_*`, `akimbo_*` → `colt_mp`/`luger_mp`; `thompson`, `mp40`, `sten`,
`kar98`, `m1_garand`, `fg42`, `panzerfaust` all present; **`mg42` is covered as
a bipod-deployable with three stance variants** (`mg42_bipod_duck/prone/stand_mp`),
better integrated than ET's; grenades → `fraggrenade_mp`/`stielhandgranate_mp`;
`m7` is a Thompson pattern; `binocs` → the sniper-scoped rifles.

**Genuinely additive — but deployables and equipment, not firearms:**
`mortar`/`mortar_set`/`mapmortar`, `landmine`, `dynamite`/`satchel`, `pliers`,
`binocs`, `ammopack`, `medpack`, `adrenaline`, `syringe`, `smokegrenade`.

The rule this establishes, and which should stand:

> **CoD1/UO version first when the weapon already exists.
> ET fills gaps or supplies distinctive mechanics/content.**

**No ET weapon was imported.** Every ET firearm is already covered, several
better by CoD1, and importing one is a converter project (`.mdc` → `.xmodel` for
mesh and skeleton, `.weap` → CoD1 weapon `.gsc` + animscript for behaviour) for
zero gameplay gain.

If the mortar is ever prototyped, start from a **deployable prop** — mostly a
static model plus a muzzle — rather than a skeletal weapon. That keeps the work
to proving format translation without ET's animation or AI. It should be adapted
to CoD lethality, fire rates, respawn economics and HUD treatment rather than
keeping ET balance.

---

## 19. WHAT NATHAN MUST VISUALLY INSPECT

My model has no image input and the vision tool is rate-limited, so **nothing
here is confirmed by eye.** Renderer statistics and server logs are the
evidence. Please check:

1. **Does `mp_toccoa` look like a place?** Textures resolve and geometry renders,
   but nobody has seen it. Texture scale (`uv_scale = 1/32`) is a guess.
2. **Are the training signs legible and sensibly placed?** They are thin panels;
   orientation and facing were never checked visually.
3. **Does the layout read as three routes?** The cross-connections were designed
   on paper; whether they actually enable flanking in play is unverified.
4. **Is the instructor tower reachable and appropriately exposed?** The stair
   blocks are a guess at climbability.
5. **Can a player join and fight bots on Toccoa?** The client was verified
   joining a `mp_carentan` server, but **never tested against Toccoa**.

---

## 19b. STATE AS OF `52e9619`

The user has now played a live `mp_carentan` match and confirmed by eye:
Carentan renders first-person, the authored Thompson class applies
(30/360, Full_Auto), bots can be killed, the player can be killed, Tab
opens a scoreboard, and **the killcam shows the killer's POV and their
weapon**.

Six bugs came out of that play session, all of the same shape - a value
retail supplies that we were reading as absent:

- `scr_killcam` and `scr_forcerespawn` were both missing from the engine
  cvar table, so `dm.gsc`'s killcam branch was unreachable and every death
  respawned immediately. (There is no code path that showed a *wrong*
  killcam; there was no killcam.)
- `Localized::translate` required a leading `@`, but `setText`
  configstrings the reference bare, so every localized HUD element drew as
  its own key. `translate_binds` had the same bug independently, which is
  why the prompt took two commits to fix.
- The viewmodel rig was gated on `!following`, so the killcam had a camera
  and no weapon in it.

`docs/protocol-1.1.md` now carries a "vcod additions to the input bits"
section, because sprint is **new mechanics, not a port**: CoD 1 has one move
speed, the capture found no walk or sprint bit, and retail's own
`default_mp.cfg` binds `SHIFT "+melee"`. Sprint took `buttons` `0x04` (the
one bit the capture proved unused) and `SCALE_SPRINT = 1.5` is **chosen by
feel, not measured**. It is labelled that way in the source and the docs.

### Verified this session
- Workspace: **1789 passed, 0 failed.** The viewmodel bind-pose test that
  was parked for several checkpoints now passes as well.
- Not verified by eye: sprint, the V melee bind, the G grenade alias, and
  the Create-a-Class screen. All are unit-tested and built, but the user has
  not played them yet. Do not claim them as working in the game.

### What Create-a-Class still needs
The screen (`crates/client/src/hud/createaclass.rs`) and the server's edit
and save path (`Classes::set_slot` / `Classes::save`) both exist and are
tested. What is **not** wired is the round trip:

- the client has no copy of its own profile, because nothing sends one down,
  so opening the screen starts blank. `CreateAClass::load` and `to_class`
  are the marked landing points.
- pressing Enter logs the change (`classes: primary = thompson_mp`) but
  sends no command to the server, so an edit does not yet reach
  `set_slot`.
- no save key, and the pause menu and map selector are not built.

---

## 20. FILE INDEX

| Path | What |
|---|---|
| `tools/toccoa/layout.py` | the map — edit this |
| `tools/toccoa/build_toccoa.py` | BSP compiler |
| `tools/toccoa/package_pk3.py` | pk3 packager |
| `tools/toccoa/validate_spawns.py` | build-time spawn assertion |
| `assets/scripts/mp_toccoa.gsc` | original per-map script |
| `docs/maps.md` | custom map pipeline documentation |
| `docs/et-inventory.md` | ET inventory and the import decision |
| `run-dev-match.bat` | one-click match launcher (default: mp_toccoa) |
| `stop-dev-match.bat` | shut down server + client |
| `main-untitled/` | generated overlay (gitignored) |