# UNTITLED — Session Handoff

A new classic Call of Duty–era FPS built on the vcod fork, running genuine
Call of Duty 1.1 content from the user's own retail installation, now with its
own first original map.

**End-of-day checkpoint: `mp_toccoa` is playable.** A graphical client joins,
spawns into the match with an M1 Carbine, and bots fight and die on the map.

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
| `python validate_spawns.py` | 8/8 clear, floored, headroom |
| `mp_toccoa` dm, 3 bots, 90 s | **4 kills, 7 damage events** |
| `mp_toccoa` tdm | **fails** — abort loop (above) |
| `mp_carentan` dm, 3 bots | boots clean, 0 aborts |
| `mp_carentan` tdm, 4 bots | boots clean, 1 kill, 0 errors, 0 aborts |

`cargo test -p vcod-server` is reported in §14.

## Git

- Checkpoint commit: see §14 (this session's commit).
- Working tree clean at that commit.
- **pushed: NO** — local only.
- **No proprietary data tracked.** `.gitignore` blocks every CoD asset extension
  (`*.pk3`, `*.bsp`, `*.xmodel`, `*.xanim`, `*.gsc`, `*.tga`, `*.dds`, `*.wav`)
  and the generated overlay, with an explicit negation so our own
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

**Fix TDM on `mp_toccoa`.** It is the one known-broken thing in an otherwise
working build, it is narrow, and everything else is already proven tonight. The
question to answer first: does the fix belong in `tools/toccoa/layout.py` (emit
the team spawn classes TDM expects) or in the server (accept DM spawns under TDM
the way retail does)? Compare a retail TDM map's entity list against ours to
decide.

Once TDM runs, the next candidates, in order:

1. **Quake movement extensions**, additive behind cvars with the recovered retail
   constants as the default. The obstacle course is purpose-built for it.
2. **Toccoa visual polish** — texture scale, sign placement, tower stairs. All
   three are unverified guesses waiting on a human eye.
3. **Bots pathfinding** instead of bearing + jitter, which is the map's biggest
   remaining gameplay weakness.
4. **Create-a-Class foundation** (no perks).

---

## 18. ET INVENTORY — investigated, conclusion: no firearms

Full findings in `docs/et-inventory.md`.

ET is installed (`...\Steam\steamapps\common\Wolfenstein Enemy Territory`,
3763 entries: 1167 sounds, 961 models, 645 textures, 117 scripts).

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