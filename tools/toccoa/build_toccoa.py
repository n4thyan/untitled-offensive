"""Build `mp_toccoa` — a CoD1-format IBSP 59 map — straight from Python.

No Radiant, no external tool: the engine already documents every lump it
reads (docs/research/bsp-ibsp59-format.md), so this emits that layout
directly. Geometry is axis-aligned boxes, which is all the brush primitive
set needs for a training-ground blockout.

Conventions, from the format doc:
  * Z is up, one unit is roughly an inch (a CoD player is 56 units tall).
  * The brush side order is 0=xmin 1=xmax 2=ymin 3=ymax 4=zmin 5=zmax.
  * Front faces wind clockwise seen from the normal side.
  * Contents: 1 = solid, and the renderer skips `trigger` materials.

The BSP lands in assets/maps/mp_toccoa.bsp; run-dev-match.bat compiles it
into a pk3 the engine already knows how to read.
"""

import math
import struct
import os

# --- geometry ----------------------------------------------------------------

class Brush:
    """An axis-aligned box, the only brush shape this blockout needs."""

    def __init__(self, mins, maxs, material):
        self.mins = tuple(float(v) for v in mins)
        self.maxs = tuple(float(v) for v in maxs)
        self.material = material

    def faces(self):
        """The six axial faces as (normal, plane_d, 4 corner points).

        Corners wind clockwise seen from outside, matching how the retail
        client culls: the renderer relies on that winding.
        """
        x0, y0, z0 = self.mins
        x1, y1, z1 = self.maxs
        d = lambda v: v
        return [
            # -x : viewed from -x looking +x, clockwise
            ((-1.0, 0.0, 0.0), d(x0),
             [(x0, y0, z1), (x0, y0, z0), (x0, y1, z0), (x0, y1, z1)]),
            # +x
            ((1.0, 0.0, 0.0), d(x1),
             [(x1, y0, z0), (x1, y0, z1), (x1, y1, z1), (x1, y1, z0)]),
            # -y
            ((0.0, -1.0, 0.0), d(y0),
             [(x0, y0, z1), (x1, y0, z1), (x1, y0, z0), (x0, y0, z0)]),
            # +y
            ((0.0, 1.0, 0.0), d(y1),
             [(x0, y1, z0), (x1, y1, z0), (x1, y1, z1), (x0, y1, z1)]),
            # -z (floor)
            ((0.0, 0.0, -1.0), d(z0),
             [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0)]),
            # +z (ceiling)
            ((0.0, 0.0, 1.0), d(z1),
             [(x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]),
        ]


class World:
    def __init__(self):
        self.brushes = []
        self.entities = []

    def box(self, mins, maxs, material):
        b = Brush(mins, maxs, material)
        self.brushes.append(b)
        return b

    def entity(self, block):
        self.entities.append(block)


def fmt(v):
    return f"{v:.4f}"


# --- the map -----------------------------------------------------------------
# Scale: CoD player is 56 units tall standing. A compact classic-CoD arena is
# roughly 3000-4500 units across. This is ~4000 x 3400, between Killhouse and
# Nuketown, which is the requested footprint.

W = 4000.0
H = 3400.0
FLOOR = 0.0
WALL = 128.0          # wall thickness
CEIL = 900.0          # open sky above

MAT = {
    # Every path here was verified to resolve against the retail paks by
    # indexing all 1271 texture stems (tools/toccoa/verify_materials.py). A
    # name that does not exist still renders -- as the default image -- so a
    # typo looks like an untextured box rather than an error, which is why
    # they are checked rather than guessed.
    "ground":    "textures/normandy/ground/dirt@earth_2",
    "mud":       "textures/normandy/ground/dirt@trench_goundalt1",
    "wood":      "textures/austria/walls/wood@barracksext_siding1",
    "wood2":     "textures/austria/walls/wood@barracksext_siding2slat",
    "metal":     "textures/industrial/metal@corrugatedsteel",
    "sandbag":   "textures/austria/walls/wood@barracksext_siding2mldy",
    "crate":     "textures/crates/wood@cratepile1",
    "concrete":  "textures/belgium/walls/concrete@bunkerint",
    "roof":      "textures/normandy/walls/wood@siding_gray",
    "turf":      "textures/normandy/ground/dirt@earthgrass_lpc",
    # Toccoa's own signage. These carry the theme directly: the obstacle
    # course, the grenade pit, the instructor tower and the cover drills are
    # all named on signs in the stock training level.
    "sign_course":   "textures/training/wood@sign_obstaclecourse",
    "sign_grenade":  "textures/training/wood@sign_grenadetraining",
    "sign_jump":     "textures/training/wood@sign_jumptraining",
    "sign_tower":    "textures/training/wood@sign_tower_e",
    "sign_barracks": "textures/training/wood@sign_barracks",
    "sign_cover":    "textures/training/wood@sign_covertocover",
    "sign_weapons":  "textures/training/wood@sign_weaponstraining",
    "hogwire":       "textures/training/metal_masked@hogwiredark",
    "target":        "textures/training/wood@testtarget",
}

# Material table. Index 0 must be a non-solid so it is safe as the default;
# the format doc notes retail's materials[0] is a clip_nosight.
MATERIALS = [
    "textures/common/clip_nosight",       # 0 - never drawn, never solid
    MAT["ground"], MAT["mud"], MAT["wood"], MAT["wood2"], MAT["metal"],
    MAT["sandbag"], MAT["crate"], MAT["concrete"], MAT["roof"], MAT["turf"],
    MAT["sign_course"], MAT["sign_grenade"], MAT["sign_jump"],
    MAT["sign_tower"], MAT["sign_barracks"], MAT["sign_cover"],
    MAT["sign_weapons"], MAT["hogwire"], MAT["target"],
]
MAT_INDEX = {name: i for i, name in enumerate(MATERIALS)}
# The layout speaks in short keys ("wood"), the BSP stores the full pk3 path
# the renderer resolves. Map the short keys onto the table.
MAT_INDEX.update({
    "sign_course": MAT_INDEX[MAT["sign_course"]],
    "sign_grenade": MAT_INDEX[MAT["sign_grenade"]],
    "sign_jump": MAT_INDEX[MAT["sign_jump"]],
    "sign_tower": MAT_INDEX[MAT["sign_tower"]],
    "sign_barracks": MAT_INDEX[MAT["sign_barracks"]],
    "sign_cover": MAT_INDEX[MAT["sign_cover"]],
    "sign_weapons": MAT_INDEX[MAT["sign_weapons"]],
    "hogwire": MAT_INDEX[MAT["hogwire"]],
    "target": MAT_INDEX[MAT["target"]],
    "ground": MAT_INDEX[MAT["ground"]],
    "mud": MAT_INDEX[MAT["mud"]],
    "wood": MAT_INDEX[MAT["wood"]],
    "wood2": MAT_INDEX[MAT["wood2"]],
    "metal": MAT_INDEX[MAT["metal"]],
    "sandbag": MAT_INDEX[MAT["sandbag"]],
    "crate": MAT_INDEX[MAT["crate"]],
    "concrete": MAT_INDEX[MAT["concrete"]],
    "roof": MAT_INDEX[MAT["roof"]],
    "turf": MAT_INDEX[MAT["turf"]],
})

# --- BSP emission ------------------------------------------------------------

def build(w, out_path):
    mats = list(MATERIALS)
    planes = []            # (nx, ny, nz, d)
    plane_index = {}

    def plane_id(n, d):
        key = (round(n[0], 5), round(n[1], 5), round(n[2], 5), round(d, 4))
        if key not in plane_index:
            plane_index[key] = len(planes)
            planes.append(key)
        return plane_index[key]

    sides = []             # (plane_or_dist_bits, material_index)
    brush_records = []     # (num_sides, material_index)
    first_side = 0

    for br in w.brushes:
        mi = MAT_INDEX[br.material]
        # Six axial sides carry an f32 bound instead of a plane index.
        bounds = [br.mins[0], br.maxs[0], br.mins[1], br.maxs[1],
                  br.mins[2], br.maxs[2]]
        for b in bounds:
            sides.append((struct.unpack("<I", struct.pack("<f", b))[0], mi))
        brush_records.append((6, mi))
        first_side += 6

    # --- draw geometry ------------------------------------------------------
    # One soup per material, because a soup names a single material and the
    # renderer drops a surface whose material it cannot resolve. A single soup
    # for the whole map would have to claim material 0, which is the
    # `clip_nosight` slot by convention -- and that renders as nothing, which is
    # exactly the "map has no drawable surfaces" the first build produced.
    #
    # A soup's draw indices are relative to its own first_vertex, not absolute.
    verts = []
    indices = []
    soups = []
    uv_scale = 1.0 / 32.0    # one texture tile per 32 units, CoD-ish
    NO_LIGHTMAP = 65535

    by_material = {}
    for br in w.brushes:
        by_material.setdefault(br.material, []).extend(br.faces())

    for material, faces in by_material.items():
        first_vertex = len(verts)
        first_index = len(indices)
        for (n, pd, corners) in faces:
            base = len(verts)
            for (cx, cy, cz) in corners:
                u = (cx + cy) * uv_scale
                v = cz * uv_scale
                verts.append((cx, cy, cz, u, v, u, v, n[0], n[1], n[2], 255))
            # Clockwise seen from outside, which is what the renderer culls on.
            #
            # Indices are RELATIVE to the soup's first_vertex, not absolute
            # (format doc, lump 6). Writing `base + i` here overflows every
            # soup after the first, because `base` is the running absolute
            # vertex count; subtracting first_vertex is what the format wants.
            r = base - first_vertex
            indices.extend([r + 0, r + 1, r + 2, r + 0, r + 2, r + 3])
        num_indices = len(indices) - first_index
        if num_indices == 0:
            continue
        soups.append((MAT_INDEX[material], NO_LIGHTMAP, first_vertex,
                      len(verts) - first_vertex, num_indices, first_index))

    # --- entities ----------------------------------------------------------
    # Retail maps put worldspawn first; readers and the gametype both assume it.
    ent_text = ""
    ordered = [b for b in w.entities if '"worldspawn"' in b] + \
              [b for b in w.entities if '"worldspawn"' not in b]
    for block in ordered:
        ent_text += block + "\n"

    # --- write the file ----------------------------------------------------
    LUMPS = 33

    def lump_bytes(index):
        if index == 0:      # materials
            out = b""
            for name in mats:
                nb = name.encode()[:63]
                out += nb + b"\x00" * (64 - len(nb))
                # content_flags drives collision, not looks: `CollisionWorld`
                # only keeps a brush whose material carries CONTENTS_SOLID
                # (0x1) or CONTENTS_PLAYERCLIP. A decorative value here renders
                # fine and then fails with "no collidable geometry", because
                # the map has nothing to stand on.
                #   0x1 SOLID       - normal world geometry, blocks and collides
                #   0x0 none        - the clip_nosight slot: not drawn, not solid
                #   0x4 TERRAIN     - floor slabs, treated as solid
                if name.startswith("textures/common/clip"):
                    contents = 0x0
                elif name in (MAT["ground"], MAT["mud"], MAT["turf"]):
                    contents = 0x4          # terrain
                else:
                    contents = 0x1          # solid
                out += struct.pack("<II", 0, contents)
            return out
        if index == 1:      # lightmaps: one all-black 512x512x3 page
            # Every soup below names NO_LIGHTMAP (65535), so this page is never
            # sampled: the client binds a white 1x1 instead and the surfaces
            # stay fullbright. The lump still has to be exactly one page so the
            # parser's `len % PAGE == 0` check passes. (An earlier comment here
            # claimed a black lightmap "would multiply the texture to zero" --
            # that is only true if a soup actually referenced page 0.)
            return b"\x00" * (512 * 512 * 3)
        if index == 2:      # planes
            return b"".join(struct.pack("<ffff", *p) for p in planes)
        if index == 3:      # brushsides
            return b"".join(struct.pack("<II", *s) for s in sides)
        if index == 4:      # brushes
            return b"".join(struct.pack("<HH", *b) for b in brush_records)
        if index == 6:      # triangle soups
            return b"".join(struct.pack("<HHIHHI", *sp) for sp in soups)
        if index == 7:      # draw verts
            out = b""
            for v in verts:
                out += struct.pack("<fffff", v[0], v[1], v[2], v[3], v[4])
                out += struct.pack("<ff", v[5], v[6])
                out += struct.pack("<fff", v[7], v[8], v[9])
                out += bytes([v[10], v[10], v[10], 255])
            return out
        if index == 8:      # draw indices
            return b"".join(struct.pack("<H", i) for i in indices)
        if index == 27:     # models: model 0 = world, owns everything
            mins = [min(b.mins[i] for b in w.brushes) for i in range(3)]
            maxs = [max(b.maxs[i] for b in w.brushes) for i in range(3)]
            return struct.pack("<ffffff", *mins, *maxs) + \
                   struct.pack("<IIIIII", 0, len(soups), 0, 0, 0, len(brush_records))
        if index == 29:     # entities
            return ent_text.encode("utf-8") + b"\x00"
        return b""

    blobs = [lump_bytes(i) for i in range(LUMPS)]

    header_size = 8 + LUMPS * 8
    offset = header_size
    placed = []
    for b in blobs:
        if len(b) == 0:
            placed.append((0, 0))
        else:
            placed.append((len(b), offset))
            offset += len(b)

    with open(out_path, "wb") as fh:
        fh.write(b"IBSP")
        fh.write(struct.pack("<I", 59))
        for (length, off) in placed:
            fh.write(struct.pack("<II", length, off))
        for b in blobs:
            if b:
                fh.write(b)

    return {
        "brushes": len(brush_records),
        "verts": len(verts),
        "tris": len(indices) // 3,
        "soups": len(soups),
        "materials": len(mats),
        "entities": len(w.entities),
        "bytes": os.path.getsize(out_path),
    }


if __name__ == "__main__":
    import layout
    stats = build(layout.make_world(), "out.bsp")
    print(stats)