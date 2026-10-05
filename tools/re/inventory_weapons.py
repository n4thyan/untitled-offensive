#!/usr/bin/env python3
"""Inventory weapon-related files across the retail CoD1 paks.

Read-only. Prints categorised listings so we can find weapons that exist as
assets but are not in the engine's WEAPON_LIST, and to see exactly what a port
would need (weapon script + xmodel + xanim + sounds + hud icon).
"""
import os
import re
import sys
import zipfile
from collections import defaultdict

GAME = r"C:\Program Files (x86)\Call of Duty\main"

# Categories by path fragment
CATS = [
    ("weapon scripts (mp)", re.compile(r"^weapons/mp/.*\.gsc$", re.I)),
    ("weapon scripts (sp)", re.compile(r"^weapons/sp/.*\.gsc$", re.I)),
    ("weapon scripts (other)", re.compile(r"^weapons/.*\.gsc$", re.I)),
    ("viewmodels (xmodel)", re.compile(r"^xmodel/.*viewmodel.*", re.I)),
    ("weapon xmodels", re.compile(r"^xmodel/.*(weapon|gun|rifle|smg|pistol|mg|launcher|sniper).*", re.I)),
    ("xanim", re.compile(r"^xanim/.*", re.I)),
    ("weapon sounds", re.compile(r"^sound/.*(weapon|gun|fire|reload|impact).*", re.I)),
    ("hud weapon icons", re.compile(r"^.*hud.*weapon.*", re.I)),
]


def main():
    paks = sorted(
        f for f in os.listdir(GAME) if f.lower().endswith(".pk3")
    )
    print(f"# {len(paks)} paks in {GAME}\n")

    all_weapon_scripts = set()
    cat_hits = defaultdict(set)
    weapon_like = set()

    for pak in paks:
        path = os.path.join(GAME, pak)
        try:
            z = zipfile.ZipFile(path)
        except Exception as e:
            print(f"!! {pak}: {e}", file=sys.stderr)
            continue
        for name in z.namelist():
            n = name.replace("\\", "/")
            low = n.lower()
            for label, rx in CATS:
                if rx.match(low):
                    cat_hits[label].add((n, pak))
            if low.startswith("weapons/mp/") and low.endswith(".gsc"):
                all_weapon_scripts.add(os.path.basename(low)[:-4])
            if re.search(r"(shotgun|trench|winchester|browning|flame|gew(e|ä)hr|g43|k43|svt|welrod|delisle|suppress|silenc)", low):
                weapon_like.add((n, pak))
        z.close()

    print("=" * 70)
    print("WEAPON SCRIPTS under weapons/mp/  (the MP loadout files)")
    print("=" * 70)
    for w in sorted(all_weapon_scripts):
        print("  ", w)

    for label, _ in CATS:
        hits = sorted(cat_hits[label])
        print()
        print("=" * 70)
        print(f"{label}  ({len(hits)})")
        print("=" * 70)
        for n, pak in hits[:400]:
            print(f"  [{pak}] {n}")

    print()
    print("=" * 70)
    print(f"SHOTGUN / FLAME / SUPPRESS / G43-LIKE MATCHES  ({len(weapon_like)})")
    print("=" * 70)
    for n, pak in sorted(weapon_like):
        print(f"  [{pak}] {n}")


if __name__ == "__main__":
    main()
