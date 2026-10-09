//! The map side of the server: collision for spectator flight and a spawn.

use glam::Vec3;
use std::{
    cell::Cell,
    collections::{BTreeMap, HashMap},
};
use vcod_common::net::msg::EntityState;
use vcod_common::net::protocol::{Protocol, ENTITYNUM_WORLD};
use vcod_common::net::trajectory::TR_LINEAR;
use vcod_common::{
    bsp,
    collision::{CollisionWorld, CONTENTS_SOLID},
    pmove,
};

pub struct World {
    pub collision: CollisionWorld,
    /// The BSP tree and PVS table, for culling each client's entity list.
    pub vis: bsp::Visibility,
    /// Feet origin and yaw degrees, from the ents lump's spawn classes.
    pub spawn: ([f32; 3], f32),
    pub(crate) spawn_safety: SpawnSafety,
}

const PLAYER_SPAWN_CLASSES: [&str; 5] = [
    "mp_deathmatch_spawn",
    "mp_teamdeathmatch_spawn",
    "mp_dm_spawn",
    "mp_tdm_spawn",
    "info_player_start",
];
const MAX_FLOOR_DROP: f32 = 128.0;
const MIN_FLOOR_NORMAL_Z: f32 = 0.7;
const HULL_CLEARANCE: f32 = 0.125;

#[derive(Debug)]
pub(crate) struct SpawnSafety {
    bounds: ([f32; 3], [f32; 3]),
    candidates: Vec<([f32; 3], f32)>,
    cursor: Cell<usize>,
    enabled: bool,
}

impl SpawnSafety {
    fn new(bounds: ([f32; 3], [f32; 3]), candidates: Vec<([f32; 3], f32)>) -> Self {
        SpawnSafety {
            bounds,
            candidates,
            cursor: Cell::new(0),
            enabled: true,
        }
    }

    pub(crate) fn disabled() -> Self {
        SpawnSafety {
            bounds: ([0.0; 3], [0.0; 3]),
            candidates: Vec::new(),
            cursor: Cell::new(0),
            enabled: false,
        }
    }

    fn select(
        &self,
        collision: &CollisionWorld,
        requested: ([f32; 3], f32),
    ) -> Option<([f32; 3], f32)> {
        if !self.enabled || spawn_candidate_is_safe(collision, self.bounds, requested.0) {
            return Some(requested);
        }
        let len = self.candidates.len();
        for offset in 0..len {
            let index = (self.cursor.get() + offset) % len;
            let candidate = self.candidates[index];
            if candidate.1.is_finite()
                && spawn_candidate_is_safe(collision, self.bounds, candidate.0)
            {
                self.cursor.set((index + 1) % len);
                return Some(candidate);
            }
        }
        None
    }
}

fn spawn_candidate_is_safe(
    collision: &CollisionWorld,
    bounds: ([f32; 3], [f32; 3]),
    origin: [f32; 3],
) -> bool {
    let origin = Vec3::from(origin);
    if !origin.is_finite() {
        return false;
    }
    let lo = Vec3::from(bounds.0);
    let hi = Vec3::from(bounds.1);
    let player = pmove::PlayerState::spawn(Vec3::ZERO, 0.0);
    let mins = player.mins();
    let maxs = player.maxs();
    if (origin + mins).cmplt(lo).any() || (origin + maxs).cmpgt(hi).any() {
        return false;
    }
    // Pmove leaves grounded feet one trace backoff (0.125 units) above the
    // floor. Test the same resting capsule so floor contact is not mistaken
    // for an embedded hull.
    let hull_origin = origin + Vec3::Z * HULL_CLEARANCE;
    let hull = collision.box_trace(hull_origin, hull_origin, mins, maxs);
    if hull.startsolid || hull.allsolid || hull.fraction < 1.0 {
        return false;
    }
    let centre = origin + (mins + maxs) * 0.5;
    if collision.point_contents(origin + Vec3::Z) & CONTENTS_SOLID != 0
        || collision.point_contents(centre) & CONTENTS_SOLID != 0
    {
        return false;
    }
    let floor = collision.box_trace(
        hull_origin,
        hull_origin - Vec3::Z * (MAX_FLOOR_DROP + HULL_CLEARANCE),
        mins,
        maxs,
    );
    !floor.startsolid
        && !floor.allsolid
        && floor.fraction < 1.0
        && floor.normal.z >= MIN_FLOOR_NORMAL_Z
}

fn player_spawn_candidates(entities: &str) -> Vec<([f32; 3], f32)> {
    let blocks = bsp::entity_blocks(entities);
    let mut out = Vec::new();
    for class in PLAYER_SPAWN_CLASSES {
        for block in &blocks {
            if block.get("classname").map(String::as_str) != Some(class) {
                continue;
            }
            let Some(origin) = block.get("origin").and_then(|v| bsp::parse_vec3(v)) else {
                continue;
            };
            let yaw = block
                .get("angles")
                .and_then(|a| a.split_whitespace().nth(1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            out.push((origin, yaw));
        }
    }
    out
}

/// The unit the engine grows a linked entity's box by on each axis before
/// taking its clusters. VERIFIED from `SV_LinkEntity` (cod_lnxded 0x8090bbe):
/// after `absmin`/`absmax` are the origin plus `r.mins`/`r.maxs`, each low
/// component has 1 subtracted and each high component 1 added. It is what
/// gives a script model, whose own box is a point, any clusters at all.
const LINK_EPSILON: f32 = 1.0;

/// Which of `entities` a client standing at `eye` is sent. Retail's rule is
/// the entity's clusters against the client's PVS row
/// (`docs/protocol-1.1.md`, "Which entities a client is sent"), collected from
/// the box its spawn function links it with, and this does the same. A point
/// lookup is not enough: a turret's origin sits inside the geometry it stands
/// on and reads as a solid leaf, so it would be visible from everywhere.
///
/// Retail also checks that the two areas connect. Every stock map's leafs
/// carry area -1 or 0 (`docs/research/bsp-ibsp59-format.md`), so there is
/// nothing to disconnect and no area test here.
pub fn visible_entities(
    vis: &bsp::Visibility,
    eye: [f32; 3],
    entities: &BTreeMap<u32, EntityState>,
    p: &Protocol,
) -> BTreeMap<u32, EntityState> {
    let from = vis.cluster_at(eye);
    entities
        .iter()
        .filter(|(_, e)| entity_visible(vis, from, e, p))
        .map(|(n, e)| (*n, e.clone()))
        .collect()
}

/// [`visible_entities`]' test for one entity, from the cluster `from`.
pub fn entity_visible(vis: &bsp::Visibility, from: i32, e: &EntityState, p: &Protocol) -> bool {
    let o = e.origin(p);
    let (mins, maxs) = crate::game::wire::link_box(e.field_i32(p, "eType"));
    let at = |b: [f32; 3], pad: f32| [o[0] + b[0] + pad, o[1] + b[1] + pad, o[2] + b[2] + pad];
    let clusters = vis.clusters_in_box(at(mins, -LINK_EPSILON), at(maxs, LINK_EPSILON));
    // An entity whose box touches no cluster at all is skipped, the way the
    // module's loop skips one with `numClusters == 0`.
    clusters.iter().any(|&c| vis.visible(from, c))
}

impl World {
    /// `fs` is the mounted paks, for the static props: a map's carts, crates
    /// and hay bales are xmodels rather than brushes, and retail collides
    /// against them. Without them a grenade rolls through a cart and a
    /// bullet passes through a barrel (`crates/server/tests/missile_ab.rs`,
    /// where the capture's own grenade comes to rest on one). `None` is the
    /// bare world every test that mounts no paks builds.
    pub fn from_bsp(b: &bsp::Bsp, fs: Option<&vcod_common::pk3::Pk3Fs>) -> Self {
        let props = fs.map_or_else(Vec::new, |fs| {
            vcod_common::props::collision_tris(fs, &b.entities)
        });
        let collision = CollisionWorld::build(b, &props);
        let spawn = bsp::find_spawn(&b.entities)
            // No spawn class in the ents: hover above the origin.
            .unwrap_or(([0.0, 0.0, 64.0], 0.0));
        let spawn_safety = b
            .models
            .first()
            .map_or_else(SpawnSafety::disabled, |model| {
                SpawnSafety::new(
                    (model.mins, model.maxs),
                    player_spawn_candidates(&b.entities),
                )
            });
        World {
            collision,
            vis: b.visibility(),
            spawn,
            spawn_safety,
        }
    }

    pub(crate) fn safe_player_spawn(&self, requested: ([f32; 3], f32)) -> Option<([f32; 3], f32)> {
        self.spawn_safety.select(&self.collision, requested)
    }
}

/// Scripted entities that exist only to drive the packet-entity wire path:
/// baselines, deltas, removals and trajectory evaluation. No gameplay meaning
/// (docs/design/2026-08-27-delta-compression-design.md).
pub struct TestEntities {
    count: usize,
    around: [f32; 3],
}

/// Entity numbers start at 32, clear of client slots for the default
/// `--max-clients` (8) and any run up to 32; CoD 1.1 allows up to 64
/// clients, so a server started with more than 32 collides a live slot
/// with a test entity number.
const FIRST_TEST_ENT: u32 = 32;
/// The last entity vanishes for 2 s out of every 8 s.
const CYCLE_MS: i32 = 8000;
const GONE_MS: i32 = 2000;
/// The highest `--test-entities` count that keeps every scripted entity
/// number below `ENTITYNUM_WORLD`; past this, the highest-numbered entity
/// collides with the reserved world slot or the packet-entities terminator.
pub const MAX_TEST_ENTITIES: usize = (ENTITYNUM_WORLD - FIRST_TEST_ENT) as usize;

impl TestEntities {
    pub fn new(count: usize, around: [f32; 3]) -> Self {
        TestEntities { count, around }
    }

    fn ent(&self, p: &Protocol, i: usize, server_time: i32) -> EntityState {
        let mut e = EntityState::null(p);
        // `number` takes part in EntityState equality; stamp it so a built
        // entity compares equal to the same entity read back off the wire.
        e.number = FIRST_TEST_ENT + i as u32;
        let seti = |e: &mut EntityState, name: &str, v: i32| {
            if let Some(idx) = EntityState::field_index(p, name) {
                e.fields[idx] = v;
            }
        };
        // pos.trBase and pos.trDelta are `bits: 0` fields: the wire value is
        // an f32 bit pattern, not a raw integer (fields_v1.rs).
        let setf = |e: &mut EntityState, name: &str, v: f32| {
            if let Some(idx) = EntityState::field_index(p, name) {
                e.fields[idx] = v.to_bits() as i32;
            }
        };
        // Entities sit 64 units apart along x and drift along y at 100
        // units/s; the trajectory restarts every lap so pos.trTime changes
        // on the wire without pos.trBase itself needing to move.
        let lap = 4000;
        let phase = server_time.rem_euclid(lap);
        seti(&mut e, "pos.trType", TR_LINEAR);
        seti(&mut e, "pos.trTime", server_time - phase);
        setf(&mut e, "pos.trBase[0]", self.around[0] + i as f32 * 64.0);
        setf(&mut e, "pos.trBase[1]", self.around[1]);
        setf(&mut e, "pos.trBase[2]", self.around[2]);
        setf(&mut e, "pos.trDelta[1]", 100.0);
        e
    }

    pub fn baselines(&self, p: &Protocol) -> HashMap<u32, EntityState> {
        (0..self.count)
            .map(|i| (FIRST_TEST_ENT + i as u32, self.ent(p, i, 0)))
            .collect()
    }

    pub fn at(&self, p: &Protocol, server_time: i32) -> BTreeMap<u32, EntityState> {
        let cycled_out = server_time.rem_euclid(CYCLE_MS) >= CYCLE_MS - GONE_MS;
        let last = self.count.saturating_sub(1);
        (0..self.count)
            .filter(|i| !(cycled_out && *i == last))
            .map(|i| (FIRST_TEST_ENT + i as u32, self.ent(p, i, server_time)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawns_at_a_map_spot_on_real_maps() {
        let Some(data) = vcod_common::testing::real_bsp() else {
            return;
        };
        let parsed = bsp::parse(&data).unwrap();
        let w = World::from_bsp(&parsed, None);
        // find_spawn picked a real, collision-safe spot, not bedrock.
        assert!(w.spawn.0[2] > -500.0, "spawn {:?}", w.spawn);
        assert_eq!(w.safe_player_spawn(w.spawn), Some(w.spawn));
    }

    #[test]
    fn every_stock_map_has_a_safe_player_spawn_fallback() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        for map in [
            "mp_carentan",
            "mp_dawnville",
            "mp_brecourt",
            "mp_pavlov",
            "mp_rocket",
            "mp_ship",
            "mp_harbor",
            "mp_hurtgen",
            "mp_stalingrad",
            "mp_tigertown",
            "mp_powcamp",
            "mp_bocage",
            "mp_depot",
            "mp_railyard",
            "mp_neuville",
            "mp_chateau",
        ] {
            let path = fs
                .resolve_map(map)
                .unwrap_or_else(|| panic!("missing {map}"));
            let data = fs.read(&path).unwrap_or_else(|| panic!("read {map}"));
            let parsed = bsp::parse(&data).unwrap_or_else(|e| panic!("parse {map}: {e}"));
            let world = World::from_bsp(&parsed, None);
            assert!(
                world.spawn_safety.candidates.iter().any(|candidate| {
                    candidate.1.is_finite()
                        && spawn_candidate_is_safe(
                            &world.collision,
                            world.spawn_safety.bounds,
                            candidate.0,
                        )
                }),
                "{map} has no collision-safe player spawn"
            );
        }
    }

    #[test]
    fn spawn_candidates_require_a_clear_supported_player_capsule_in_world() {
        let collision = vcod_common::collision::test_world(&[(
            glam::Vec3::new(80.0, -32.0, 0.0),
            glam::Vec3::new(120.0, 32.0, 96.0),
        )]);
        let bounds = ([-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 512.0]);

        assert!(spawn_candidate_is_safe(&collision, bounds, [0.0, 0.0, 0.0]));
        assert!(!spawn_candidate_is_safe(
            &collision,
            bounds,
            [100.0, 0.0, 0.0]
        ));
        assert!(!spawn_candidate_is_safe(
            &collision,
            bounds,
            [0.0, 0.0, -32.0]
        ));
        assert!(!spawn_candidate_is_safe(
            &collision,
            bounds,
            [0.0, 0.0, 200.0]
        ));
        assert!(!spawn_candidate_is_safe(
            &collision,
            bounds,
            [f32::NAN, 0.0, 0.0]
        ));
    }

    #[test]
    fn blocked_spawn_fallback_rotates_deterministically_over_map_entities() {
        let collision = vcod_common::collision::test_world(&[(
            glam::Vec3::new(80.0, -32.0, 0.0),
            glam::Vec3::new(120.0, 32.0, 96.0),
        )]);
        let bounds = ([-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 512.0]);
        let safety = SpawnSafety::new(
            bounds,
            vec![
                ([100.0, 0.0, 0.0], 10.0),
                ([0.0, 0.0, 0.0], 20.0),
                ([200.0, 0.0, 0.0], 30.0),
            ],
        );

        assert_eq!(
            safety.select(&collision, ([100.0, 0.0, 0.0], 90.0)),
            Some(([0.0, 0.0, 0.0], 20.0))
        );
        assert_eq!(
            safety.select(&collision, ([100.0, 0.0, 0.0], 90.0)),
            Some(([200.0, 0.0, 0.0], 30.0))
        );
    }

    #[test]
    fn a_clear_requested_spawn_is_preserved_and_no_safe_fallback_is_refused() {
        let collision = vcod_common::collision::test_world(&[(
            glam::Vec3::new(80.0, -32.0, 0.0),
            glam::Vec3::new(120.0, 32.0, 96.0),
        )]);
        let bounds = ([-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 512.0]);
        let requested = ([0.0, 0.0, 0.0], 47.0);
        let safety = SpawnSafety::new(bounds, vec![([100.0, 0.0, 0.0], 10.0)]);

        assert_eq!(safety.select(&collision, requested), Some(requested));
        assert_eq!(safety.select(&collision, ([100.0, 0.0, 0.0], 90.0)), None);
    }

    #[test]
    fn test_entities_move_and_one_cycles_out_and_back() {
        let p = &vcod_common::net::protocol::PROTOCOL_V1;
        let te = TestEntities::new(3, [0.0, 0.0, 64.0]);
        let time_idx = EntityState::field_index(p, "pos.trTime").unwrap();

        let a = te.at(p, 0);
        let b = te.at(p, 4000);
        assert_eq!(a.len(), 3, "all three present at t=0");
        // Motion is carried by trTime (the trajectory's start), not trBase,
        // which is fixed per entity; the client evaluates the interpolation.
        assert_ne!(
            a[&32].fields[time_idx], b[&32].fields[time_idx],
            "entity 32's trajectory restarted between t=0 and t=4000"
        );

        // The cycling entity is gone for a stretch and comes back.
        // 6500 % 8000 == 6500, inside the last 2s (>= 6000) of the 8s cycle.
        let gone = te.at(p, 6500);
        assert_eq!(gone.len(), 2, "one entity cycles out at t=6500");
        let back = te.at(p, 11000);
        assert_eq!(back.len(), 3, "and returns by t=11000");

        // Every entity has a baseline for the gamestate.
        let bl = te.baselines(p);
        assert_eq!(bl.len(), 3);
        assert!(bl.contains_key(&32));
    }

    #[test]
    fn max_test_entities_is_the_last_count_below_entitynum_world() {
        let p = &vcod_common::net::protocol::PROTOCOL_V1;

        let at_max = TestEntities::new(MAX_TEST_ENTITIES, [0.0, 0.0, 64.0]);
        let highest = *at_max.baselines(p).keys().max().unwrap();
        assert_eq!(
            highest,
            ENTITYNUM_WORLD - 1,
            "MAX_TEST_ENTITIES should use every number up to ENTITYNUM_WORLD - 1"
        );

        let one_over = TestEntities::new(MAX_TEST_ENTITIES + 1, [0.0, 0.0, 64.0]);
        let highest_over = *one_over.baselines(p).keys().max().unwrap();
        assert_eq!(
            highest_over, ENTITYNUM_WORLD,
            "one past MAX_TEST_ENTITIES reaches ENTITYNUM_WORLD, which is what \
             the --test-entities startup check rejects"
        );
    }
}
