"""Package the compiled BSP and the map's GSC into the overlay pk3.

The vcod fork reads maps out of `Pk3Fs`, a directory of zip archives. So the
custom map ships the way retail maps do: a `.pk3` in an overlay directory that
is searched BEFORE the retail `main`. That means no engine change, no new
world format, and nothing written into the retail install.

A per-map `maps/MP/<name>.gsc` is required as well as the BSP: the server loads
the map's own script alongside the stock gametype, and a map with a BSP but no
script fails with `LoadError { msg: "script not found" }`. That script is
original project content (assets/scripts/), not retail text.

Generated binaries are not committed; `main-untitled/` is gitignored. The
source of truth is `layout.py` plus the files under `assets/`, and the whole
map rebuilds from those with two commands.
"""

import os
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))

BSP_SRC = os.path.join(HERE, "out.bsp")
SCRIPT_SRC = os.path.join(ROOT, "assets", "scripts", "mp_toccoa.gsc")
# The overlay directory the engine searches before the retail `main`.
MOD_DIR = os.path.join(ROOT, "main-untitled")
PK3_PATH = os.path.join(MOD_DIR, "untitled.pk3")


def main():
    missing = [p for p in (BSP_SRC, SCRIPT_SRC) if not os.path.exists(p)]
    if missing:
        for p in missing:
            print(f"missing {p}")
        raise SystemExit("run build_toccoa.py first (and keep assets/scripts/)")

    os.makedirs(MOD_DIR, exist_ok=True)
    if os.path.exists(PK3_PATH):
        os.remove(PK3_PATH)

    with zipfile.ZipFile(PK3_PATH, "w", zipfile.ZIP_DEFLATED) as z:
        # Stock CoD maps live at maps/MP/<name>.*; matching that means the
        # loader and the map-cycle logic find these with no special cases.
        z.write(BSP_SRC, "maps/MP/mp_toccoa.bsp")
        z.write(SCRIPT_SRC, "maps/MP/mp_toccoa.gsc")

    print(f"wrote {PK3_PATH} ({os.path.getsize(PK3_PATH)} bytes)")
    with zipfile.ZipFile(PK3_PATH) as z:
        for e in z.namelist():
            print(f"  {e}")


if __name__ == "__main__":
    main()