"""Assert the map's spawn points are usable, at build time.

Two layout bugs on the first playable mp_toccoa build were both invisible in
the BSP and only showed up as "bots stand still":

  * a spawn sat INSIDE the equipment shed's wall, so a bot spent a whole match
    wedged in geometry;
  * a crate stack overlapped a wall face for the same reason.

Both are cheap to catch here. A spawn is rejected if the eye-height point is
inside any brush, or if there is no floor within a step of its feet, so a
spawn cannot float or be buried. Run on every build; a failure is a layout bug,
not a warning to ignore.
"""

import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import layout as L  # noqa: E402

# A CoD player stands 56 units tall with the eye around 40 up.
FOOT_PROBE = 8.0
CEILING_PROBE = 56.0


def inside_any(brushes, p):
    for b in brushes:
        if all(b.mins[i] <= p[i] <= b.maxs[i] for i in range(3)):
            return b
    return None


def has_floor(brushes, x, y, z, reach=FOOT_PROBE * 3.0):
    """True when a solid surface lies just under the spawn's feet."""
    for b in brushes:
        if b.mins[0] - 1 <= x <= b.maxs[0] + 1 and b.mins[1] - 1 <= y <= b.maxs[1] + 1:
            if abs(b.maxs[2] - z) <= reach:
                return True
    return False


def clear_above(brushes, x, y, z, height=CEILING_PROBE):
    for b in brushes:
        if b.mins[0] <= x <= b.maxs[0] and b.mins[1] <= y <= b.maxs[1]:
            if b.mins[2] <= z + height and b.maxs[2] > z:
                return False
    return True


def main():
    w = L.make_world()
    brushes = w.brushes
    failures = []

    # Pull spawns out of the entity text: the layout stores them as blocks.
    import re
    pattern = re.compile(r'\{[^}]*"mp_deathmatch_spawn"[^}]*\}', re.S)
    count = 0
    for block in w.entities:
        if "mp_deathmatch_spawn" not in block:
            continue
        m = re.search(r'"origin"\s+"([-\d.]+)\s+([-\d.]+)\s+([-\d.]+)"', block)
        if not m:
            failures.append(f"spawn with unparseable origin: {block!r}")
            continue
        x, y, z = (float(m.group(1)), float(m.group(2)), float(m.group(3)))
        name = re.search(r'"targetname"\s+"([^"]+)"', block)
        label = name.group(1) if name else f"spawn@{x:.0f},{y:.0f}"
        count += 1

        body = inside_any(brushes, (x, y, z))
        if body:
            failures.append(
                f"{label} at ({x:.0f}, {y:.0f}, {z:.0f}) is INSIDE a "
                f"{body.material} brush {tuple(round(v) for v in body.mins)}"
                f"..{tuple(round(v) for v in body.maxs)}"
            )
            continue
        if not has_floor(brushes, x, y, z):
            failures.append(f"{label} at ({x:.0f}, {y:.0f}) has no floor under it")
        if not clear_above(brushes, x, y, z):
            failures.append(f"{label} at ({x:.0f}, {y:.0f}) has no headroom")

    print(f"checked {count} spawn points against {len(brushes)} brushes")
    if failures:
        print("\nFAILURES:")
        for f in failures:
            print("  -", f)
        return 1
    print("all spawn points are clear, floored and have headroom")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())