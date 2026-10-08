"""Index every entry in the retail CoD1 main/*.pk3 archives, and verify the
18 mp_toccoa material names resolve. Replaces the missing verify_materials.py
referenced by tools/toccoa/build_toccoa.py.

Usage: python tools/toccoa/verify_materials.py
"""
import os
import pickle
import sys
import zipfile

GAME_DIR = os.environ.get("COD_DIR", r"C:\Program Files (x86)\Call of Duty")
MAIN = os.path.join(GAME_DIR, "main")
CACHE = os.path.join(
    os.environ.get("LOCALAPPDATA", os.path.expanduser("~")),
    "hermes", "cache", "scratch", "cod1_index.pkl",
)

# The material table build_toccoa.py emits, in lump order.
MATERIALS = [
    "textures/common/clip_nosight",
    "textures/normandy/ground/dirt@earth_2",
    "textures/normandy/ground/dirt@trench_goundalt1",
    "textures/austria/walls/wood@barracksext_siding1",
    "textures/austria/walls/wood@barracksext_siding2slat",
    "textures/industrial/metal@corrugatedsteel",
    "textures/austria/walls/wood@barracksext_siding2mldy",
    "textures/crates/wood@cratepile1",
    "textures/belgium/walls/concrete@bunkerint",
    "textures/normandy/walls/wood@siding_gray",
    "textures/normandy/ground/dirt@earthgrass_lpc",
    "textures/training/wood@sign_obstaclecourse",
    "textures/training/wood@sign_grenadetraining",
    "textures/training/wood@sign_jumptraining",
    "textures/training/wood@sign_tower_e",
    "textures/training/wood@sign_barracks",
    "textures/training/wood@sign_covertocover",
    "textures/training/wood@sign_weaponstraining",
    "textures/training/metal_masked@hogwiredark",
    "textures/training/wood@testtarget",
]

IMAGE_EXTS = [".dds", ".tga", ".jpg", ".png"]


def build_index():
    idx = {}
    for f in sorted(os.listdir(MAIN)):
        if not f.lower().endswith(".pk3"):
            continue
        try:
            z = zipfile.ZipFile(os.path.join(MAIN, f))
        except Exception as e:
            print(f"  skip {f}: {e}")
            continue
        for n in z.namelist():
            idx.setdefault(n.lower(), f)
    return idx


def load_index():
    if os.path.exists(CACHE):
        try:
            return pickle.load(open(CACHE, "rb"))
        except Exception:
            pass
    idx = build_index()
    os.makedirs(os.path.dirname(CACHE), exist_ok=True)
    pickle.dump(idx, open(CACHE, "wb"))
    return idx


def main():
    idx = load_index()
    print(f"retail index: {len(idx)} entries "
          f"({sum(1 for k in idx if k.startswith('textures/'))} textures)")
    print(f"COD_DIR={GAME_DIR}\n")

    ok, missing = 0, []
    for m in MATERIALS:
        stem = m.lower()
        hits = []
        for ext in IMAGE_EXTS:
            k = stem + ext
            if k in idx:
                hits.append((k, idx[k]))
        # CoD1 also ships some names with the extension spelled in the pk3 as
        # the material path with no extension; check the bare path too.
        if stem in idx:
            hits.append((stem, idx[stem]))
        if hits:
            ok += 1
            print(f"  OK      {m}  ->  {hits[0][0]}  [{hits[0][1]}]")
        else:
            missing.append(m)
            print(f"  MISSING {m}")

    print(f"\n{ok}/{len(MATERIALS)} resolve, {len(missing)} missing")
    if missing:
        print("\nClosest candidates in the paks for each missing name:")
        for m in missing:
            tail = m.split("/")[-1].lower()
            # try the stem after '@' and the last path component
            probes = [tail, tail.split("@")[-1], tail.split("@")[0]]
            cands = set()
            for p in probes:
                if len(p) < 4:
                    continue
                for k in idx:
                    if k.startswith("textures/") and p in k:
                        cands.add(k)
                        if len(cands) > 6:
                            break
                if cands:
                    break
            for c in sorted(cands)[:6]:
                print(f"    {m}  ~  {c}")
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
