"""The mp_toccoa layout: a live-fire training course at Camp Toccoa.

Three interconnecting routes between two protected spawn compounds, per the
design brief:

  Route A  obstacle course  - closest range, along the west side
  Route B  central yard     - the fast, dangerous middle, instructor tower
  Route C  firing/grenade    - medium-long rifle lanes, along the east side

Every route cross-connects into the middle, so a fight is not always frontal,
and neither spawn can see the other. Dimensions are in CoD units (a player is
56 tall); the arena is ~4000 x 3400, between Killhouse and Nuketown scale.
"""

import build_toccoa as B


# Materials mirrored from the builder so layout stays readable.
GROUND, MUD, WOOD, WOOD2, METAL = "ground", "mud", "wood", "wood2", "metal"
SANDBAG, CRATE, CONCRETE, ROOF, TURF = "sandbag", "crate", "concrete", "roof", "turf"

W = 4000.0        # x extent
H = 3400.0        # y extent
WT = 128.0        # wall thickness


def make_world():
    w = B.World()

    # ---------------------------------------------------------------- ground
    # One big slab under everything, plus patches so the surface is not a
    # single flat material and routes read differently underfoot.
    w.box((-WT, -WT, -64), (W + WT, H + WT, 0), GROUND)
    # muddy track down the middle of the obstacle course
    w.box((-WT, 1200, 0), (900, 1500, 8), MUD)
    # grass verge on the east flank
    w.box((3000, -WT, 0), (W + WT, H + WT, 8), TURF)

    # ------------------------------------------------------------ outer wall
    # A closed box with four walls; the interior is the arena. Ceiling is left
    # open so the map has a real sky.
    def wall(x0, y0, x1, y1, h=1400.0):
        w.box((x0, y0, 0), (x1, y1, h), CONCRETE)

    wall(-WT, -WT, 0, WT)                       # west
    wall(W, -WT, W + WT, WT)                   # east
    wall(-WT, -WT, WT, H)                      # south
    wall(-WT, H, W + WT, H + WT)               # north

    # ---------------------------------------------------------------- helpers
    def barrier(x0, y0, x1, y1, h=64.0, mat=WOOD):
        """A waist-high barrier: solid, blocks movement, does not block sight."""
        w.box((x0, y0, 0), (x1, y1, h), mat)

    def sandbag_wall(x0, y0, x1, y1, h=56.0):
        w.box((x0, y0, 0), (x1, y1, h), SANDBAG)

    def crate_stack(x, y, h=64.0, s=64.0):
        w.box((x, y, 0), (x + s, y + s, h), CRATE)

    # ==================================================== TEAM A SPAWN (south)
    # Training/barracks yard. Protected: a solid wall on the spawn side, so no
    # sightline into the middle until a player commits to a route.
    sx0, sy0, sx1, sy1 = 120.0, 120.0, 1180.0, 1180.0

    # spawn compound back wall (south), forcing players north to leave
    w.box((sx0, sy0, 0), (sx1, sy0 + WT, 1400), CONCRETE)
    # west side of the compound
    w.box((sx0, sy0, 0), (sx0 + WT, sy1, 1400), CONCRETE)

    # barracks hut inside the compound, giving cover at spawn
    bh_x0, bh_y0, bh_w, bh_d, bh_h = 220.0, 260.0, 620.0, 380.0, 320.0
    t = 64.0
    w.box((bh_x0, bh_y0, 0), (bh_x0 + bh_w, bh_y0 + t, bh_h), WOOD)        # south wall
    w.box((bh_x0, bh_y0, 0), (bh_x0 + t, bh_y0 + bh_d, bh_h), WOOD)        # west wall
    w.box((bh_x0 + bh_w - t, bh_y0, 0), (bh_x0 + bh_w, bh_y0 + 200, bh_h), WOOD)  # east wall, partial -> doorway
    w.box((bh_x0, bh_y0 + bh_d - t, 0), (bh_x0 + bh_w, bh_y0 + bh_d, bh_h), WOOD)  # north wall
    w.box((bh_x0, bh_y0, bh_h), (bh_x0 + bh_w, bh_y0 + bh_d, bh_h + 48), ROOF)     # roof

    # equipment crates and a sandbag lip at the spawn mouth
    crate_stack(900, 300)
    crate_stack(964, 300)
    crate_stack(900, 364)
    sandbag_wall(240, 1000, 900, 1064, 56)

    # two exits from the compound: a gap on the east side, and the north side
    # is open toward the obstacle course.
    # exit marker: the east wall of the compound is deliberately omitted
    # between y 700..900 so players can slip out east into the middle.

    # ==================================================== TEAM B SPAWN (north)
    # Equipment/firing yard. Equivalent value, not a visual mirror: more open,
    # firing benches rather than a barracks hut.
    bx0, by0, bx1, by1 = 2820.0, 2220.0, 3880.0, 3280.0

    w.box((bx0, by1 - WT, 0), (bx1, by1, 1400), CONCRETE)   # north back wall
    w.box((bx1 - WT, by0, 0), (bx1, by1, 1400), CONCRETE)   # east side

    # equipment shed
    sh_x0, sh_y0, sh_w, sh_d, sh_h = 3180.0, 2620.0, 520.0, 340.0, 300.0
    w.box((sh_x0, sh_y0, 0), (sh_x0 + sh_w, sh_y0 + t, sh_h), METAL)
    w.box((sh_x0 + sh_w - t, sh_y0, 0), (sh_x0 + sh_w, sh_y0 + 160, sh_h), METAL)
    w.box((sh_x0, sh_y0 + sh_d - t, 0), (sh_x0 + sh_w, sh_y0 + sh_d, sh_h), METAL)
    w.box((sh_x0, sh_y0, sh_h), (sh_x0 + sh_w, sh_y0 + sh_d, sh_h + 48), METAL)

    # firing benches + crates + sandbag position in the yard
    #
    # The benches and the sandbag wall sit on the compound's WEST approach,
    # which is the only way out toward the central yard. In the first build
    # they formed a solid barrier from x=2820 to x=3100, so a bot leaving
    # spawn walked straight into them and never got onto the map -- measured
    # pinned at x=3129 for the whole match. Spread along the north edge and
    # leave the whole southern approach clear instead.
    barrier(3000, 2500, 3200, 2564, 56, WOOD2)
    barrier(3000, 2900, 3200, 2964, 56, WOOD2)
    crate_stack(3400, 2900)
    crate_stack(3464, 2900)
    sandbag_wall(3300, 2600, 3364, 3000, 56)

    # The shed's north wall runs y=2896..2960, and a spawn at (3700, 2900)
    # therefore began life INSIDE solid geometry -- which is how a bot ended up
    # wedged there for an entire match while the other one idled in the open.
    # The spawn points below now sit in verified open floor, and
    # validate_spawns.py asserts it on every build rather than leaving the next
    # person to find this the hard way.
    crate_stack(2900, 3050)
    crate_stack(2964, 3050)

    # exits: the west side of the compound is open between x 2820..3000 toward
    # the firing route, and the south is open toward the central yard.

    # ================================================ ROUTE A - OBSTACLE COURSE
    # West/central-west strip: hurdles, low walls, logs, a crawl-under wire
    # section. Closest-range fighting.
    ax = 1180.0
    # zig-zag hurdles down the course
    barrier(ax, 400, ax + 420, 464, 56, WOOD)
    barrier(ax + 120, 900, ax + 540, 964, 56, WOOD)
    barrier(ax, 1400, ax + 420, 1464, 56, WOOD)
    barrier(ax + 120, 1900, ax + 540, 1964, 56, WOOD)
    barrier(ax, 2400, ax + 420, 2464, 56, WOOD)

    # Low walls flanking the course, making a lane rather than a wall of wood.
    #
    # Built in PIECES with deliberate gaps -- a gameplay requirement, not
    # decoration. The first pass ran both walls solid from y=300 to y=2600,
    # which turned the obstacle course into a sealed corridor: a bot that
    # entered could only leave by running past the end of a 2300-unit wall,
    # and with heading jitter it just pressed against the wall for the rest of
    # the match. Measured on the first playable build, both bots pinned within
    # 15 units of a wall face and no kill ever happened.
    #
    # Four gaps per side, aligned with the hurdle spacing, so each gap is a
    # real tactical exit and a place to break a sightline -- and so the middle
    # of the map has somewhere to shoot INTO from the course.
    def wall_with_gaps(x0, y0, x1, y1, gaps, h=120.0, mat=CONCRETE):
        cursor = y0
        for (ga, gb) in sorted(gaps):
            if ga > cursor:
                w.box((x0, cursor, 0), (x1, ga, h), mat)
            cursor = max(cursor, gb)
        if cursor < y1:
            w.box((x0, cursor, 0), (x1, y1, h), mat)

    course_gaps = [(464, 620), (1100, 1260), (1964, 2120), (2560, 2700)]
    wall_with_gaps(ax - 200, 300, ax, 2600, course_gaps)
    wall_with_gaps(ax + 600, 300, ax + 800, 2600,
                   [(a + 140, b + 140) for (a, b) in course_gaps])

    # The crawl-under wire sat at the far end of the course, behind the last
    # wall segment, so reaching it meant running the length of a corridor.
    # It belongs mid-course anyway, between hurdles 2 and 3, where a player is
    # already committed to close-range movement.
    w.box((ax + 150, 1180, 0), (ax + 650, 1216, 200), "hogwire")
    w.box((ax + 150, 1216, 0), (ax + 650, 1264, 200), "hogwire")
    sandbag_wall(ax + 150, 1264, ax + 650, 1300, 64)

    # The crawl-under HOg wire itself is placed mid-course above; the far
    # north-west corner instead gets an open turnaround so the course does not
    # terminate in a dead end.
    w.box((ax - 200, 2600, 0), (ax + 800, 2664, 120), CONCRETE)

    # sandbag chicane partway down
    sandbag_wall(ax + 420, 1150, ax + 560, 1400, 56)
    sandbag_wall(ax + 120, 1700, ax + 260, 1950, 56)

    # ================================================== ROUTE B - CENTRAL YARD
    # The fast, dangerous middle. Landmark: the instructor/observation tower.
    cx0, cx1 = 1500.0, 2700.0
    cy0, cy1 = 1100.0, 2300.0

    # low training barricades breaking sightlines through the yard
    barrier(cx0 + 100, cy0 + 120, cx0 + 700, cy0 + 184, 64, WOOD)
    barrier(cx1 - 700, cy1 - 184, cx1 - 100, cy1 - 120, 64, WOOD)
    barrier(cx0 + 300, cy0 + 700, cx0 + 364, cy0 + 1000, 64, WOOD2)

    # scattered crates for cover
    crate_stack(1900, 1500)
    crate_stack(1964, 1500)
    crate_stack(2200, 1900)

    # ---- instructor / observation tower -----------------------------------
    # Central landmark. Not a sniper nest: reachable by a ladder-like stack of
    # ledges, with four exposed faces and no full cover on top.
    tx, ty = 2050.0, 1600.0
    tw = 320.0                      # tower footprint
    leg = 48.0
    t_top = 620.0                   # platform height
    # four corner legs, leaving climbable gaps
    w.box((tx, ty, 0), (tx + leg, ty + tw, t_top), WOOD)
    w.box((tx + tw - leg, ty, 0), (tx + tw, ty + tw, t_top), WOOD)
    w.box((tx, ty, 0), (tx + tw, ty + leg, t_top), WOOD)
    w.box((tx, ty + tw - leg, 0), (tx + tw, ty + tw, t_top), WOOD)
    # platform (leaves a one-unit-wide slot at each corner for a climb line)
    w.box((tx, ty, t_top), (tx + tw, ty + tw, t_top + 48), WOOD2)
    # a low rail on two sides only: exposed, not a nest
    w.box((tx, ty + tw - leg, t_top + 48), (tx + tw, ty + tw, t_top + 140), WOOD2)
    # stair blocks to reach it, from the north-west
    w.box((tx - 180, ty + 60, 0), (tx, ty + 260, 180), WOOD)
    w.box((tx - 120, ty + 100, 180), (tx, ty + 300, 340), WOOD)
    w.box((tx - 60, ty + 140, 340), (tx, ty + 340, 520), WOOD)

    # ============================================== ROUTE C - FIRING / GRENADE
    # East strip: medium-to-long rifle lanes plus a small grenade structure.
    fx = 2700.0
    # firing bays: three recessed positions looking west into the map
    for i in range(3):
        by = 500.0 + i * 700.0
        w.box((fx + 400, by, 0), (fx + 464, by + 420, 200), CONCRETE)   # side wall
        w.box((fx + 400, by + 356, 0), (fx + 700, by + 420, 200), CONCRETE)  # front return
        barrier(fx + 500, by + 40, fx + 660, by + 104, 56, WOOD2)       # bench

    # grenade-training structure: a small roofed box with window openings
    gx, gy, gw, gd, gh = 3050.0, 2000.0, 520.0, 420.0, 340.0
    w.box((gx, gy, 0), (gx + gw, gy + t, gh), CONCRETE)               # south
    w.box((gx, gy, 0), (gx + t, gy + gd, gh), CONCRETE)               # west
    w.box((gx + gw - t, gy, 0), (gx + gw, gy + gd, gh), CONCRETE)     # east
    # north wall in two pieces, leaving a window band in the middle
    w.box((gx, gy + gd - t, 0), (gx + 160, gy + gd, gh), CONCRETE)
    w.box((gx + gw - 160, gy + gd - t, 0), (gx + gw, gy + gd, gh), CONCRETE)
    w.box((gx, gy + gd - t, gh - 90), (gx + gw, gy + gd, gh), CONCRETE)  # lintel over window
    w.box((gx, gy, gh), (gx + gw, gy + gd, gh + 48), CONCRETE)       # roof

    # demolition practice wall: a thin free-standing wall to shoot through gaps
    w.box((fx + 150, 1150, 0), (fx + 214, 1500, 220), CONCRETE)

    # ======================================================= CROSS-CONNECTIONS
    # Loops, so fights are not always frontal and players do not have to run
    # back through spawn to change lane.
    # obstacle course <-> central yard (south)
    barrier(ax + 800, 2000, cx0 + 100, 2064, 56, WOOD)
    # central yard <-> firing route (north)
    barrier(cx1 - 100, cy1 + 100, fx + 150, 2064 + 100, 56, WOOD2)
    # obstacle course <-> firing route, along the far north, behind both
    barrier(1300, 3000, 3600, 3064, 64, CONCRETE)

    # perimeter cross-links so no route is a dead end
    w.box((700, 2700, 0), (764, 3400, 140), CONCRETE)
    w.box((3200, 300, 0), (3264, 900, 140), CONCRETE)

    # ============================================================ SIGNAGE
    # Camp Toccoa's own training signs, from the stock `textures/training/`
    # set. These carry the theme directly -- the obstacle course, the grenade
    # pit, the jump drill and the instructor tower are each named on a sign in
    # the retail training level, so reusing them makes the blockout read as
    # Toccoa rather than as generic WWII boxes.
    #
    # Thin panels, waist-to-head height, facing into the space they name.
    def sign(x, y, z, mat, axis="y", size=128.0):
        t = 8.0
        if axis == "y":       # panel faces +/-y, thin in y
            w.box((x, y - t, z), (x + size * 2.0, y + t, z + size), mat)
        else:                 # thin in x
            w.box((x - t, y, z), (x + t, y + size * 2.0, z + size), mat)

    sign(1500, 300, 96, "sign_course")     # at the mouth of the obstacle course
    sign(2960, 1900, 96, "sign_grenade")   # by the grenade structure
    sign(1700, 1150, 96, "sign_jump")      # jump drill, central yard west
    sign(2380, 1560, 700, "sign_tower")    # on the instructor tower
    sign(300, 700, 96, "sign_barracks")    # in the team A compound
    sign(3120, 2450, 96, "sign_weapons")   # in the team B yard

    # A target board facing down the firing range, the long-sightline anchor.
    w.box((3450, 1500, 0), (3520, 1620, 300), "target")

    # ================================================================== SPAWNS
    # Team A: inside the south-west compound. Team B: inside the north-east.
    # Spawn origins sit ~16 units above the floor; the player origin is at the
    # feet. No spawn sees the other spawn: the compounds are diagonal corners
    # and walled on their outer sides.
    for (i, (x, y, yaw)) in enumerate([
        (420.0, 820.0, 0.0),
        (700.0, 860.0, 20.0),
        (980.0, 520.0, -15.0),
        (300.0, 940.0, 40.0),
    ]):
        w.entity(
            '{\n"classname" "mp_deathmatch_spawn"\n'
            f'"origin" "{x:.1f} {y:.1f} 16"\n'
            f'"angles" "0 {yaw:.1f} 0"\n'
            f'"targetname" "toccoa_spawn_a{i}"\n}}'
        )

    for (i, (x, y, yaw)) in enumerate([
        (2900.0, 2500.0, 180.0),
        (3500.0, 2560.0, 160.0),
        (3000.0, 2760.0, 200.0),
        (3250.0, 2350.0, 150.0),
    ]):
        w.entity(
            '{\n"classname" "mp_deathmatch_spawn"\n'
            f'"origin" "{x:.1f} {y:.1f} 16"\n'
            f'"angles" "0 {yaw:.1f} 0"\n'
            f'"targetname" "toccoa_spawn_b{i}"\n}}'
        )

    # intermission camera, as retail maps carry
    w.entity(
        '{\n"classname" "mp_deathmatch_intermission"\n'
        '"origin" "2000 1700 900"\n'
        '"angles" "0 180 0"\n}'
    )

    # worldspawn must be first
    w.entity('{\n"classname" "worldspawn"\n}')

    return w