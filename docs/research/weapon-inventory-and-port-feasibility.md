# The weapon inventory, and why nothing new was ported

Written 2026-10-05 at checkpoint `ad32f34`. This is the Phase 1/2 survey in
full: what the game has, what is missing, and why no new weapon was ported.
The conclusion is load-bearing for how the rest of the arsenal work is
sequenced, so it is written down rather than left in a session log.

**Bottom line: every WWII firearm that would be genuinely new to this game is
blocked on assets we do not have in a format this engine reads.** Create-a-Class
(Phase 4) is the highest-value work that is not blocked, and is therefore what
came next. Re-run the survey before assuming this still holds.

## 1. What the game has

Exactly the 32 names in configstring 7 (`crates/server/src/configstrings.rs`,
`WEAPON_LIST`), read from the CoD 1.1 + Underworld retail install. No weapon has
been added to the game; this is the whole arsenal.

| Class | Weapons |
|---|---|
| Pistol | `colt_mp`, `luger_mp` |
| SMG | `thompson_mp`/`_semi`, `mp40_mp`, `ppsh_mp`/`_semi`, `sten_mp`, `mp44_mp`/`_semi` |
| Rifle | `m1carbine_mp`, `m1garand_mp`, `kar98k_mp`, `enfield_mp`, `mosin_nagant_mp`, `fg42_mp`/`_semi`, `bar_mp`/`_slow_mp` |
| Sniper | `kar98k_sniper_mp`, `springfield_mp`, `mosin_nagant_sniper_mp` |
| MG | `mg42_bipod_duck_mp`, `mg42_bipod_prone_mp`, `mg42_bipod_stand_mp` |
| Launcher | `panzerfaust_mp`, `ptrs41_antitank_rifle_mp` |
| Grenade | `fraggrenade_mp`, `stielhandgranate_mp`, `mk1britishfrag_mp`, `rgd-33russianfrag_mp` |
| **Shotgun** | **none** |
| **Flamethrower** | **none** |

Counting `mg42_bipod_*` as one gun, that is roughly 24 distinct firearms, which
is the authentic CoD 1.1 roster. It is not thin for its era — the gaps are real
but narrow.

## 2. Gaps worth having

By gameplay value in a WWII shooter, in order:

1. **Shotgun** (Trench Gun, M1 Garand Trench Gun). The single biggest gap.
   Every era of CoD leans on one; it changes engagement range and pacing more
   than any rifle would. **Blocked** (§3).
2. **Flamethrower** (M2, M1919). Distinct role: area denial, no drop-off curves
   to tune. **Blocked** (§3).
3. **Gewehr 43** and **SVT-40**. High semi-auto / marksman rifles; ET has both.
   **Blocked** (§3) — the ET formats are unrelated to CoD 1's.
4. **Maxim / DP-28 / Browning .30 cal.** Light machine guns distinct from the
   deployable MG42. World models exist in the CoD paks with no def, no viewmodel
   and no animations. **Blocked** — dead assets, not weapons.
5. **Suppressed pistol.** `weapon_HiStandard` exists as a world mesh only: no
   viewmodel, no animation set. **Blocked.**
6. **More sniper variants.** Already covered — three rifles plus the FG42 cover
   the roles. *Not* worth doing; a differently-named carbine is not a new gun.

Item 6 is worth stating as a rule, because it is the obvious false economy:
`bar_slow_mp`, `mp44_semi_mp`, `stielhandgranate_mp` and friends are already in
the roster, and adding a re-skin of a firearm already represented is not content.

## 3. Why each is blocked

### CoD 1.1's own paks have nothing extra

`tools/re/inventory_weapons.py` walks all sixteen paks and categorises entries by
path; `tools/re/find_in_paks.py` regex-searches every pak;
`tools/re/xref_viewmodels.py` cross-references each weapon def against the
viewmodel xmodels and animation sets. Results:

- No shotgun, flamethrower, Gewehr 43, SVT-40 or Maxim **def** anywhere.
- `weapon_browning30cal`, `weapon_russian_maxim`: world/turret meshes, no def.
- `weapon_HiStandard`: world mesh only.
- **Every** viewmodel + animation family that does exist maps onto a weapon
  already in configstring 7. There is no orphaned gun hiding in the paks.

This was checked rather than assumed, because a hidden viewmodel would have been
the cheapest possible win.

### World at War has the guns, in a format this engine cannot read

Installed and owned by the user at `…/Steam/steamapps/common/Call of Duty World
at War`. It does contain the trench gun, the M1 flamethrower, the Gewehr 43 and
the SVT-40 — exactly the blocked list above.

But the assets are not pk3 files. `tools/re/probe_ff.py` confirms:

- `zone/english/common_mp.ff` opens with the `IWffu100` magic, and zlib from
  offset 12 inflates 39 MB to ~74.8 MB. So the data *is* readable and not
  encrypted.
- What is inside is a **linked zone** (`IWffu100` + a zone header), and the
  xmodels inside it are **version 16/18**.
- This engine's xmodel parser handles **version 14 only**
  (`docs/research/xmodel-v14-format.md`).

That is not a converter, it is a second asset pipeline: zone tree walking,
v16/v18 model and animation formats, and a different material model. Weeks of
work with no gameplay payoff until the very end, and it would put WaW-era
geometry conventions into a CoD 1.1 renderer. **Out of scope.** Same conclusion
for the `.iwd` archives, which are plain zips holding no weapon assets at all.

### Enemy Territory has the guns, in a third unrelated format

Installed at `…/Wolfenstein Enemy Territory`. It has `k43.weap` (the Gewehr 43),
`flamethrower.weap` and others — a genuinely useful inventory reference for
*what* a WWII gun's stats look like, and nothing more:

- Geometry is `.mdc`, a Quake-derived model format, unrelated to CoD 1's xmodel.
- Weapons are `.weap` key-value files in ET's own schema, not CoD's `weapons/mp/`
  defs.

A Gewehr 43 port is therefore: write an mdc-to-xmodel converter, write an anim
converter, author or find a viewmodel, hand-build a weapon def. Every one of
those is an open-ended task, and the result is one gun.

RTCW and the OpenMoHAA/MoHAA ecosystem are the same story: worth reading for
weapon state-machine and animation *ideas* (their code is GPL/BSD and readable),
worthless as an asset source.

## 4. Licensing

Kept strictly separate, per the project's standing rule:

- **Code** from ET:Legacy, RTCW, ioquake3 and friends is open source and can be
  read for patterns, and copied where the licence allows and the licence is
  recorded.
- **Game assets** are not open source because the engine is. Nothing from
  WaW, ET or RTCW is copied into this repository or into any pak. `.gitignore`
  blocks every CoD asset extension.
- What the game ships is read at runtime from the user's **own** retail
  installation, read-only, and never redistributed. That is how the existing
  32 weapons work and how any future one would have to work.

## 5. What this implies for sequencing

Phase 3 (port a weapon end to end) is blocked, so Phase 4 became the work:

1. **Create-a-Class** — uses only weapons already present, so it is unblocked,
   and it makes the existing roster matter more rather than adding to it.
2. **Scoreboard / stats** — needs no new content.
3. **Killstreaks** — rewards can be built from existing assets (a UAV-style
   recon reward is a script + an existing effect).
4. **Killcam** — pure engine work, no assets needed.

If the arsenal is ever to grow, the two realistic routes are (a) obtain
licence-clean v14 assets from a source that has them, or (b) build the WaW v16/v18
zone reader as its own project. Neither is a day. Route (a) is much cheaper and
should be tried first. **Do not spend a session rediscovering this.**

## Reproducing

```
python tools/re/inventory_weapons.py    # categorise every pak's contents
python tools/re/find_in_paks.py PATTERN # regex search across all paks
python tools/re/xref_viewmodels.py      # defs vs viewmodels + anims
python tools/re/probe_ff.py FILE.ff     # WaW fastfile header + inflate
```

All read `C:/Program Files (x86)/Call of Duty` (override with `COD_DIR`) and
write nothing outside the scratch directory.
