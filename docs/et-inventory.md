# ET weapon/equipment inventory

Findings from inspecting the local Enemy Territory install against the CoD1
weapon list. This is the input to the "ET fills gaps" decision, and it settles
that decision: **ET contributes no importable weapons without a converter
project, and the few things it has that CoD1 lacks are deployables, not guns.**

## What is installed

```
C:\Program Files (x86)\Steam\steamapps\common\Wolfenstein Enemy Territory\
    et.exe, etded.exe, etmain\  (pak0, pak1, pak2, mp_bin)
```

3763 entries. Sound-heavy (1167), 961 models, 645 textures, 117 scripts,
114 maps.

## The blocker: no shared weapon format

| | Model format | Weapon definition |
|---|---|---|
| **Enemy Territory** | `.mdc` (165) — Quake II / RTCW lineage | `.weap` (40) — RTCW text format |
| **Call of Duty 1** | `.xmodel` + `.xmodelparts` — IW lineage | weapon `.gsc` + animscript |

They are different engines with different asset formats on both the mesh *and*
the weapon-behaviour side. An ET weapon cannot be referenced by a CoD1 map, put
in a CoD1 pk3, or loaded by this engine at all — there is no CoD1 `.weap`
reader and no `.mdc` loader.

Importing one ET weapon is therefore not a "drop a file in" task. It needs:

1. an `.mdc` → CoD1 `.xmodel` converter (skeleton, animation and material
   translation), and
2. a `.weap` → CoD1 weapon `.gsc` + animscript translation, including the
   fire/reload/ADS timing the CoD1 weapon model expects.

That is a real project on its own, and it is explicitly not this milestone.
The map was the milestone, and it is done.

## What ET actually has

40 `.weap` definitions:

```
adrenaline        akimbo_colt        akimbo_luger       akimbo_silenced_colt
akimbo_silenced_luger  ammopack     binocs             colt
dynamite          fg42               flamethrower       gpg40
grenade           k43                kar98              knife
landmine          luger              m1_garand          m1_garand_s
m7                mapmortar          medpack            mg42
mortar            mortar_set         mp40               panzerfaust
pineapple         pliers             satchel            satchel_det
silenced_colt     silenced_luger     smokegrenade       smokemarker
smoketrail        sten               syringe            thompson
```

Cross-referenced against CoD1's 32 (`WEAPON_LIST` in
`crates/server/src/configstrings.rs`, parsed from retail):

**Already covered by CoD1 — do not import duplicates:**

| ET | CoD1 equivalent |
|---|---|
| `colt`, `luger`, `silenced_colt`, `silenced_luger`, `akimbo_*` | `colt_mp`, `luger_mp` |
| `thompson` | `thompson_mp` |
| `mp40` | `mp40_mp` |
| `sten` | `sten_mp` |
| `kar98` | `kar98k_mp` |
| `m1_garand` | `m1garand_mp` |
| `fg42` | `fg42_mp` |
| `panzerfaust` | `panzerfaust_mp` |
| `mg42` | `mg42_bipod_duck_mp` / `_prone_mp` / `_stand_mp` |
| `grenade`, `pineapple` | `fraggrenade_mp`, `stielhandgranate_mp` |
| `m7` | `thompson_mp` (the M7 *is* a Thompson pattern) |
| `flamethrower` | *(CoD1 flamethrower exists in UO, not in base 1.1)* |
| `binocs` | `kar98k_sniper_mp`, `mosin_nagant_sniper_mp` |

Notably **CoD1 already has a bipod-deployable MG42** (three stance variants),
which is ET's signature weapon. There is no gap to fill there.

## Genuinely additive — but not firearms

What ET has that CoD1 base 1.1 does not is **deployables and equipment**, all
of them `.weap` definitions that would still need the converter above:

| ET item | What it is | CoD1 1.1 status |
|---|---|---|
| `mortar` / `mortar_set` | deployable 81mm mortar, crew-served | **absent** |
| `mapmortar` | secondary ammo/last-round-out mortar variant | absent |
| `landmine` | deployable anti-personnel mine | absent |
| `dynamite` / `satchel` | demo charges | absent |
| `pliers` | salvage/cut tool | absent |
| `binocs` | binocular spotter | absent (sniper variants serve this) |
| `ammopack`, `medpack` | deployable resupply | absent |
| `adrenaline`, `syringe` | throwable health items | absent |
| `smokegrenade` | screening smoke | CoD1 has smoke grenades |

## Conclusion and the rule to follow

**No ET firearm should be imported.** Every normal weapon ET offers is already
represented in CoD1, several of them better-integrated (the bipod MG42, the
sniper-scoped rifles). Importing an `.mdc` MP40 to replace CoD1's `.xmodel`
MP40 would be a downgrade and a converter project for no gameplay gain.

The standing rule this establishes:

> **CoD1/UO version first when the weapon already exists.
> ET fills gaps or supplies distinctive mechanics/content.**

The genuine ET gaps are all **deployable, crew-served or equipment** systems —
mortar, landmine, satchel charges, binoculars, resupply packs. Those are the
candidates if the mortar ever gets prototyped, and the mortar is the best of
them: a portable indirect-fire weapon is a distinct enough mechanic to justify
the converter, and it maps onto a class of gameplay (area denial, cross-map
pressure) that no CoD1 weapon fills.

When that happens it should be adapted to CoD conventions rather than keeping
ET balance — CoD lethality and fire rates, CoD respawn economics, CoD HUD
treatment — per the project's gameplay-first rule.

## Next step if this is picked up

Prototype **ET's deployable mortar** as the single pipeline proof:

1. Write an `.mdc` → `.xmodel` converter for one static weapon prop (the
   mortar tube), not a full skeletal character. A deployed emplacement is
   mostly a static model plus a muzzle, which is far more tractable.
2. Map `.weap` timing fields onto a CoD1 weapon `.gsc`.
3. Drive it from a spawnable entity rather than a player carry slot, since that
   is the mechanic being proved.

Deliberately a *deployable prop* first: it proves the format translation
without needing ET's animation or AI.