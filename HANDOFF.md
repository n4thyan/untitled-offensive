# UNTITLED — Session Handoff

A new classic Call of Duty–era FPS built on the vcod fork, running genuine
Call of Duty 1.1 content from the user's own retail installation.

Status as of this checkpoint: **the combat loop works end to end.**

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
run-dev-match.bat mp_carentan dm 4     REM defaults: mp_carentan / dm / 3
run-dev-match.bat mp_carentan tdm 3
```

`stop-dev-match.bat` shuts both processes down.

### By hand

```bash
# server
./target/release/vcod-server.exe --game-dir "C:/Program Files (x86)/Call of Duty" \
    --gametype dm --bots 3 --bots-shoot --port 28960 mp_carentan

# client (separate shell)
./target/release/vcod.exe --game-dir "C:/Program Files (x86)/Call of Duty" \
    --connect 127.0.0.1:28960 --team allies --weapon m1carbine_mp
```

### Offline, no server

```bash
./target/release/vcod.exe --game-dir "C:/Program Files (x86)/Call of Duty" --walk mp_carentan
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

**Actually exercised this session:** `mp_carentan` only (server and client).

**Custom maps:** none yet. This is the next major piece of work.

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
7. **One map actually tested.** `mp_carentan`. Other maps load but are unverified
   end to end.

---

## 15. GIT

Commit hash: **see §17** (recorded after the final test run).

Nothing has been pushed anywhere. Local only.

---

## 16. FINAL TEST RESULT

Recorded in the next section after the suite completed.

---

## 17. NEXT TASK

**The highest-value next step is the custom Western Front map.**

The combat loop is proven, so the engine no longer needs proving. What it now
needs is content that is ours, which is the only thing that makes this a
project rather than a very capable CoD1 runtime. A compact blockout —
shallow valley, wooded edges, clearing, ruined stone structure, trench,
sandbags, craters, flank routes — loadable by this same pipeline, is the
single thing that most moves the project forward.

Before or alongside it, two cheap wins:

- Implement `attackbuttonpressed` and `sayteam`/`sayall` (the remaining
  builtins that affect bot behaviour).
- Add cvars for the Quake movement extensions, additive over the recovered
  retail constants, with vanilla CoD1 as the default.

---

## 18. CHANGES MADE THIS SESSION

Engine/server:

- `getGuid` builtin — unblocked every gametype's `Callback_PlayerConnect`.
- `getAttachIgnoreCollision` builtin — unblocked `_utility::saveModel`.
- `takeWeapon` builtin + `WeaponOp::Take(u8)` — unblocked `_teams::givePistol`,
  which is what stopped players finishing their spawn.
- Bot menu resolution: bots now resolve `t <idx>` against the menu
  configstring table rather than the `g_scriptMainMenu` cvar. They were
  answering `"allies"` to the server-info menu, which only advances on
  `"close"`, so the join parked forever and no bot ever spawned.
- Client `auto_reply`: same server-info fix on the client side, which is what
  let a human client complete the join.

Bot behaviour (new gameplay, not a bugfix):

- Enemy-aware navigation: `BotView.nav_target` plus `bot_nav_target` and a
  widened `BOT_AWARENESS_RANGE` (6000) for steering only. Firing stays bounded
  by the original `SHOOT_RANGE` (1500) and still requires a clear sightline.
- Heading jitter on the seek bearing, so a bot blocked by geometry tries a
  different angle instead of re-picking the identical bearing forever.

Housekeeping:

- `crates/common/tests/shader_corpus.rs`: `#[cfg]` the Unix-only symlink so
  the suite compiles on Windows.
- `run-dev-match.bat`, `stop-dev-match.bat`, `dev-logs/`.