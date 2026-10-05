#!/usr/bin/env python3
"""List files in the retail CoD1 paks matching a regex, or list all files whose
basename matches one of the 32 weapon names (to find their definition files)."""
import os, re, sys, zipfile

GAME = r"C:\Program Files (x86)\Call of Duty\main"
WEAPONS = ("bar_mp bar_slow_mp bren_mp colt_mp enfield_mp fg42_mp fg42_semi_mp "
           "fraggrenade_mp kar98k_mp kar98k_sniper_mp luger_mp m1carbine_mp "
           "m1garand_mp mg42_bipod_duck_mp mg42_bipod_prone_mp mg42_bipod_stand_mp "
           "mk1britishfrag_mp mosin_nagant_mp mosin_nagant_sniper_mp mp40_mp mp44_mp "
           "mp44_semi_mp panzerfaust_mp ppsh_mp ppsh_semi_mp ptrs41_antitank_rifle_mp "
           "rgd-33russianfrag_mp springfield_mp sten_mp stielhandgranate_mp "
           "thompson_mp thompson_semi_mp").split()

pat = re.compile(sys.argv[1], re.I) if len(sys.argv) > 1 else None

paks = sorted(f for f in os.listdir(GAME) if f.lower().endswith(".pk3"))
found = {}
for pak in paks:
    z = zipfile.ZipFile(os.path.join(GAME, pak))
    for name in z.namelist():
        n = name.replace("\\", "/")
        base = os.path.basename(n)
        if pat and pat.search(n):
            found.setdefault(n, pak)
        elif not pat and base in WEAPONS:
            found.setdefault(n, pak)
    z.close()

for n in sorted(found):
    print(f"[{found[n]}] {n}")
print(f"\n# {len(found)} matches")
