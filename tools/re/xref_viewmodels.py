#!/usr/bin/env python3
"""Cross-reference CoD1 weapon defs against available viewmodel xmodels/anims.

Finds viewmodels or anim sets that exist in the paks but no weapon def uses,
which would be a genuinely new weapon candidate (not a duplicate).
"""
import os, re, zipfile
from collections import defaultdict

GAME = r"C:\Program Files (x86)\Call of Duty\main"
CS7 = ("bar_mp bar_slow_mp bren_mp colt_mp enfield_mp fg42_mp fg42_semi_mp "
       "fraggrenade_mp kar98k_mp kar98k_sniper_mp luger_mp m1carbine_mp "
       "m1garand_mp mg42_bipod_duck_mp mg42_bipod_prone_mp mg42_bipod_stand_mp "
       "mk1britishfrag_mp mosin_nagant_mp mosin_nagant_sniper_mp mp40_mp mp44_mp "
       "mp44_semi_mp panzerfaust_mp ppsh_mp ppsh_semi_mp ptrs41_antitank_rifle_mp "
       "rgd-33russianfrag_mp springfield_mp sten_mp stielhandgranate_mp "
       "thompson_mp thompson_semi_mp").split()

z = zipfile.ZipFile(os.path.join(GAME, "pak0.pk3"))
names = {n.replace("\\", "/").lower(): n for n in z.namelist()}

def read_def(name, folder="mp"):
    key = f"weapons/{folder}/{name}".lower()
    if key not in names:
        return None
    return z.read(names[key]).decode("latin1")

# Every weapon def we can see (mp and sp), plus extras
all_defs = sorted(n for n in names if n.startswith("weapons/mp/") or n.startswith("weapons/sp/"))

used_models = set()
used_anims = set()
rows = []
for key in all_defs:
    txt = z.read(names[key]).decode("latin1")
    kv = dict(re.findall(r"\\([A-Za-z0-9_]+)\\([^\\]*)", txt))
    gm = kv.get("gunModel", "").strip()
    hm = kv.get("handModel", "").strip()
    used_models.add(gm.lower()); used_models.add(hm.lower())
    anims = {v.strip().lower() for k, v in kv.items() if k.lower().endswith("anim") and v.strip()}
    used_anims |= anims
    rows.append((key, gm, hm, kv.get("weaponClass", ""), kv.get("displayName", "")))

print("=== weapon defs ===")
for key, gm, hm, cls, dn in rows:
    print(f"  {key:42} class={cls:14} gun={gm:26} hands={hm:24} {dn}")

# available viewmodel xmodels
avail_models = sorted({n.split("/",1)[1] for n in names if n.startswith("xmodel/viewmodel") or n.startswith("xmodel/weapon")})
print("\n=== viewmodel/weapon xmodels available ===")
for m in avail_models:
    mark = "" if m.lower() in used_models else "   <-- UNUSED"
    print(f"  {m}{mark}")

# available viewmodel anim families
avail_anims = sorted({n.split("/",1)[1] for n in names if n.startswith("xanim/viewmodel")})
fams = defaultdict(list)
for a in avail_anims:
    m = re.match(r"viewmodel_([a-z0-9]+?)_", a)
    fams[m.group(1) if m else a].append(a)
print("\n=== viewmodel anim families available ===")
for fam in sorted(fams):
    any_used = any(x.lower() in used_anims for x in fams[fam])
    mark = "" if any_used else "   <-- UNUSED FAMILY"
    print(f"  {fam:20} ({len(fams[fam])} clips){mark}")

z.close()
