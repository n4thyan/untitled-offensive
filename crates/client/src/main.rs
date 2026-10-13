mod audio;
mod camera;
mod entities;
mod fx;
mod hud;
mod hud_text;
mod loading;
mod local_server;
mod play;
mod probe;
mod quick_chat;
mod renderer;
mod sky;
mod viewmodel;

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use glam::Vec3;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use vcod_common::movetrace::MoveWorld;
use vcod_common::pk3::Pk3Fs;
use vcod_common::{bsp, collision, mesh, net, pmove, props, weapon, xmodel};

use camera::{FlyCamera, InputState};
use local_server::{DEFAULT_LOCAL_PORT, LocalServer, MatchConfig};
use renderer::{DynamicModelInstance, Renderer};

#[derive(Parser)]
#[command(about = "Call of Duty (2003) map viewer and client")]
struct Args {
    /// Map name, e.g. mp_pavlov
    map: Option<String>,
    /// List all maps found in the pk3 search path
    #[arg(long)]
    list: bool,
    /// Game install (holds main/). Defaults to $COD_DIR, else the executable's directory.
    #[arg(long, default_value_os_t = vcod_common::game_dir::default_game_dir())]
    game_dir: std::path::PathBuf,
    /// Pk3 subdirectory: main for CoD1, uo for United Offensive (untested)
    #[arg(long, default_value = "main")]
    mod_dir: String,
    /// A pk3 directory searched BEFORE --mod-dir. This is how original project
    /// content (custom maps) is layered over a retail install without the
    /// install being modified, copied, or duplicated into this repository.
    #[arg(long)]
    overlay_dir: Option<std::path::PathBuf>,
    /// Spawn as a soldier and walk (first person) instead of flying
    #[arg(long)]
    walk: bool,
    /// Start with the debug overlay visible (F3 toggles it at runtime)
    #[arg(long)]
    debug_overlay: bool,
    /// Headless: connect to a CoD server, dump the gamestate, then exit
    #[arg(long)]
    net_probe: Option<String>,
    /// Connect to a CoD server (ip:port) and join it through the stock team
    /// and weapon menus; spectator is one of the team menu's choices
    #[arg(long)]
    connect: Option<String>,
    /// Answer the stock team menu with this after --connect
    #[arg(long, requires = "connect")]
    team: Option<String>,
    /// Answer the stock weapon menu with this weapon file name (e.g. m1carbine_mp)
    #[arg(long, requires = "connect")]
    weapon: Option<String>,
    /// Overwrite the committed gamestate.bin fixture with the --net-probe capture.
    /// Off by default: the parser tests pin that file, and a capture from another
    /// map or a mid-round server is not a drop-in. Otherwise the probe writes to tmp/.
    #[arg(long)]
    save_fixture: bool,
    /// Overwrite the committed snapshots.bin fixture only; gamestate.bin stays pinned.
    #[arg(long)]
    save_snapshots: bool,
    /// Write the retail server's non-empty configstring table to
    /// crates/server/tests/fixtures/configstrings/<map>-<gametype>.txt, the
    /// fixture crates/server/tests/configstrings_ab.rs diffs against. Separate
    /// from --save-fixture because that pair pins the parser's byte-exact
    /// captures and this one pins the game module's table.
    #[arg(long)]
    save_configstrings: bool,
    /// Join a team through the stock menu and write the retail playerstate to
    /// crates/server/tests/fixtures/playerstate/<map>-<gametype>.txt, the
    /// fixture crates/server/tests/playerstate_ab.rs diffs against.
    #[arg(long)]
    save_playerstate: bool,
    /// Join a team, then hold each movement input in turn and write the
    /// retail playerstate settled under each to
    /// crates/server/tests/fixtures/playerstate/<map>-<gametype>-motion.txt.
    /// Separate from --save-playerstate because that one pins a standing
    /// player, which leaves every field the client predicts at zero.
    #[arg(long)]
    save_motion: bool,
    /// Join a team, then run a scripted weapon sequence -- single shot,
    /// sustained fire, reload, fire crouched and prone -- and write the retail
    /// playerstate to
    /// crates/server/tests/fixtures/playerstate/<map>-<gametype>-combat.txt.
    /// Separate from --save-motion because that one samples a settled state
    /// and a shot is a transient: this fixture carries every snapshot, since
    /// the playerstate event ring holds four slots and overwrites as it fires.
    /// Each firing step waits for weaponstate to read ready before it taps, and
    /// the reload holds the weapon file's reloadTime, so a weapon still busy
    /// from the step before cannot be recorded under a firing label. The
    /// sequence takes about 40 s, inside the default --probe-secs.
    #[arg(long)]
    save_combat: bool,
    /// Join a team, then run the sight script -- sight held, released, held
    /// through a reload, through a shot and while walking, plus a hip shot
    /// and a walk to kick the spread counter -- and write every snapshot's
    /// fWeaponPosFrac and aimSpreadScale to
    /// crates/server/tests/fixtures/playerstate/<map>-<gametype>-ads.txt.
    /// The same machine as --save-combat with another script; the two are
    /// separate fixtures because the ramp is a transient the combat steps
    /// never hold still for.
    #[arg(long)]
    save_ads: bool,
    /// Join a team, then run the grenade script -- one melee swing on the
    /// rifle, a switch to the frag, a cooked throw, a cook held past the pin,
    /// a cook cancelled by a weapon switch and a throw at the ground -- and
    /// write every snapshot's grenadeTimeLeft and weaponDelay, plus a !missile
    /// line per snapshot that had one on the wire, to
    /// crates/server/tests/fixtures/playerstate/<map>-<gametype>-grenade.txt.
    /// The same machine as --save-ads with another script; it stands still, so
    /// a gate replays it from the origin the header carries.
    #[arg(long)]
    save_grenade: bool,
    /// Join a team, raise the sight and tap eight shots down it standing
    /// still, printing every bullet-impact temp entity's origin beside the
    /// shooter's eye and view, which is what measures the sight sway
    /// (docs/research/cod11-combat.md, section 15). A measurement, not a
    /// fixture: the same machine as --save-ads, and it writes nothing.
    /// Use it with --probe-weapon kar98k_sniper_mp: adsSpread 0 makes the
    /// scoped shot deterministic.
    #[arg(long)]
    probe_sway: bool,
    /// The shooter half of the hit capture: join a team, walk toward the other
    /// player until the eye-to-eye trace through the map's collision is clear,
    /// then fire a single shot, a burst and until the target dies, and watch
    /// the corpse and the respawn. Writes
    /// crates/server/tests/fixtures/playerstate/<map>-<gametype>-hit-shooter.txt.
    /// Run it against a server that already has a --probe-target on it, on the
    /// other team. A run that never finds a line of sight says so in the
    /// fixture header and still records the shots, which then hit the map.
    #[arg(long)]
    save_hit: bool,
    /// The target half of the hit capture: join a team, stand still, kill
    /// itself 10 s in, press use 3 s after each death and record every
    /// snapshot that moves a damage or death field, plus the obituaries, the
    /// corpses and the scoreboards. Writes
    /// crates/server/tests/fixtures/playerstate/<map>-<gametype>-hit-target.txt.
    /// Separate from --save-hit because the two roles are two probes: this one
    /// is the victim, and its `kill` is what makes the death, the corpse and
    /// the respawn land whether or not the shooter ever gets a shot off.
    #[arg(long)]
    probe_target: bool,
    /// Join a team, stand still and record what the wire does across a map
    /// end and the rotation that follows: every gamestate with its serverId,
    /// every serverCommand with its reliable sequence, every out-of-band
    /// packet (retail announces a map change with `loadingnewmap` and sends no
    /// gamestate with it) and one trace line per snapshot that moved. Writes
    /// crates/server/tests/fixtures/netchan/<map>-<gametype>-mapchange.txt,
    /// named for the map the run started on. Give it --probe-secs enough to
    /// span the limit, the intermission and the next map's load. It sends
    /// `score` every 2 s, since retail answers the `b` scoreboard and never
    /// pushes one: every `b` in that fixture is an answer to this probe.
    #[arg(long)]
    save_mapchange: bool,
    /// The same recording across a round restart instead of a map change, as
    /// a pair: with --probe-target it is the half that kills itself 20 s in
    /// and ends the round, without it the half that walks up and only watches.
    /// Writes <map>-<gametype>-roundrestart-target.txt and
    /// -roundrestart-shooter.txt. A restart re-sends no gamestate, so what the
    /// shooter's weapon column shows either side of it is the pers[] carry the
    /// stock sd.gsc does by hand. Start the target first.
    #[arg(long)]
    save_roundrestart: bool,
    /// Join a team, walk the --probe-pvs route and write the entity trace to
    /// crates/server/tests/fixtures/entities/<map>-<gametype>.txt, the fixture
    /// crates/server/tests/entities_ab.rs diffs against. Separate from
    /// --probe-pvs because that one only prints: this one overwrites committed
    /// evidence. Each run appends its stations to the map's fixture, since one
    /// walk gets one random spawn; delete the file to start that map over.
    #[arg(long)]
    save_entities: bool,
    /// Join a team, then walk a route and print the snapshot entity list at
    /// each station: which entities the server sent, and every add and removal
    /// along the way with the position it happened at. Answers whether the
    /// server culls the entity list by where the client stands. Writes no
    /// fixture; the route is relative to the spawn, so its stations are
    /// positions in one run, not reproducible places.
    #[arg(long)]
    probe_pvs: bool,
    /// Walk the --probe-pvs route with the sight held and count, per second
    /// and for the run, what a slope does to the playerstate: snapshots off
    /// the ground, fWeaponPosFrac reversals, EV_STEP_VIEW (143) events with
    /// their parms and the mean speed. Writes no fixture; the same run
    /// against retail and against ours is the comparison. Pair it with
    /// --probe-cmd-ms 8 to send usercmds at a high-fps client's rate.
    #[arg(long)]
    probe_slope: bool,
    /// Join a team and walk at the map's trigger brushes, read out of the BSP,
    /// nearest unvisited first, steering round geometry with a look-ahead
    /// trace against the map's own collision. It presses use after every death
    /// so a minefield does not end the run, and kills itself when it wedges.
    /// The client writes no fixture: the evidence is the *server's*
    /// games_mp.log, logged by the client-probes/probe_trigger.gsc gametype,
    /// which is what crates/server/tests/triggers_ab.rs replays. Give it a
    /// long --probe-secs; most of a stock map's belt is walled off and each
    /// unreachable brush costs a leg.
    #[arg(long)]
    probe_triggers: bool,
    /// The S&D plant capture: joins the attackers, walks into bombzone_A,
    /// holds use for 2 s and releases (the abort), then holds use through a
    /// full plant with a forward walk sent 2 s in (what a link does to
    /// pmove), and stands still after. Writes
    /// crates/server/tests/fixtures/playerstate/<map>-sd-plant-attacker.txt.
    /// Pair with --probe-defuse on the other team, and run
    /// client-probes/probe_lookat as the gametype so the server's log carries
    /// the lookat fires. Retail evidence when taken against tools/run_probe.sh;
    /// a run against vcod-server overwrites it.
    #[arg(long, conflicts_with = "probe_defuse")]
    probe_plant: bool,
    /// The S&D defuse capture: joins the defenders, waits for the plant,
    /// walks to the bomb, sweeps the view across it with the aim alone (the
    /// lookat trigger's shape; a held use would let bomb_think finish the
    /// defuse mid-sweep), then aims true and holds use through the defuse. Writes <map>-sd-defuse-defender.txt.
    #[arg(long, conflicts_with = "probe_plant")]
    probe_defuse: bool,
    /// The item pickup capture: joins allies, waits for
    /// client-probes/probe_pickup's teleport onto the first fg42, stands on
    /// it, aims at it and takes it with the use key, takes the second one's
    /// ammo by touch after the probe's second teleport, then swaps the
    /// carbine for a panzerfaust and back. Writes
    /// crates/server/tests/fixtures/items/<map>-dm-pickup.txt.
    /// Retail evidence when taken against tools/run_probe.sh; a run against
    /// vcod-server overwrites it.
    #[arg(long)]
    save_pickup: bool,
    /// With `--net-probe`: mount mp_carentan's MG at (1712 1830 8), sweep it
    /// past both arcs, fire it, dismount, remount from a crouch, and try a
    /// mount from outside the arc; writes
    /// `crates/server/tests/fixtures/turret/<map>-dm-turret.txt`. Needs
    /// `client-probes/probe_turret` as the gametype with `probe_teleport 1`.
    #[arg(long)]
    save_turret: bool,
    /// With `--net-probe`: the player-clip walker. Waits for
    /// `client-probes/probe_bump`'s placement 200 units behind the
    /// `--probe-bump-target` client on mp_carentan, then walks into it
    /// head-on, at a 20-unit glance and in a jump once per stance it takes;
    /// writes `crates/server/tests/fixtures/playerstate/<map>-dm-bump-walker.txt`.
    /// A `--capture-tag` starting `overlap` runs the overlap script instead
    /// (stand, then walk through the gsc's `probe_overlap 1` setorigins) and
    /// writes `<map>-dm-bump-<tag>-walker.txt`. Retail evidence when taken
    /// against tools/run_probe.sh; a run against vcod-server overwrites it.
    #[arg(long, conflicts_with = "probe_bump_target")]
    save_bump: bool,
    /// With `--net-probe`: the player-clip target. Joins, stands where
    /// `client-probes/probe_bump` puts it and, once the walker stands on its
    /// mark, stands 35 s, crouches 25 s, stands 1.5 s, lies prone 25 s and
    /// stands. Writes no fixture; prints its playerstate while a push is on it.
    #[arg(long, conflicts_with = "save_bump")]
    probe_bump_target: bool,
    /// With `--net-probe`: stay a spectator and press the follow buttons
    /// (attack, attack, melee, the sight held 2 s, attack, 6 s apart from 8 s
    /// in), printing every snapshot whose clientNum, pm_type, pm_flags or
    /// eFlags moved. Writes no fixture.
    #[arg(long)]
    probe_follow: bool,
    /// With `--net-probe` and `--probe-team`: the killcam victim. Stands
    /// still, never sends `kill`, presses use 20 s after each death and once a
    /// second after that, and prints a `KILLCAM` line per snapshot from the
    /// death until 3 s after the respawn. Writes no fixture.
    #[arg(long)]
    probe_killcam: bool,
    /// With `--probe-killcam`: press use this many ms after the killcam starts
    /// (the first frame following another client) instead, to skip it.
    #[arg(long, requires = "probe_killcam")]
    probe_killcam_skip_ms: Option<u64>,
    /// Walk the --probe-slope route and write every usercmd sent and every
    /// snapshot's movement fields to
    /// crates/server/tests/fixtures/playerstate/<map>-<gametype>-slope-<ms>ms.txt,
    /// which playerstate_slope_ab.rs replays on our mover, origin for origin.
    /// Retail evidence when taken against tools/run_server.sh; a run against
    /// vcod-server overwrites it.
    #[arg(long)]
    save_slope: bool,
    /// With --save-slope or --probe-slope: a prone crawl instead of the
    /// route, from where `client-probes/probe_prone` puts the player on
    /// mp_carentan (`+set probe_teleport 1`, `probe_spot` street or mound),
    /// with the value the world yaw up the grade (90 on the street, 270 on
    /// the mound): prone, crawl up, down and across the grade, turn past the
    /// prone yaw cap, sweep the pitch past the prone pitch clamp, crawl
    /// looking down and downhill, stand. Tag the fixture with --capture-tag.
    #[arg(long, value_name = "UPHILL_YAW")]
    probe_prone: Option<f32>,
    /// Milliseconds between usercmds the probe sends. A retail client at 125
    /// fps sends one every 8 ms; the default is what every capture so far
    /// was taken with.
    #[arg(long, default_value_t = 16)]
    probe_cmd_ms: u64,
    /// Turns --save-hit into a measurement instead of a capture: the shooter
    /// taps once per entry of a static table of pitch offsets around the aim
    /// at the target's eye, echoing the offset on each !trace line, and writes
    /// no fixture. Pair those lines with the sHitLoc the server's own
    /// games_mp.log D; records carry and every vertical hit-location boundary
    /// falls out of the two. Give both probes a long --probe-secs: the sweep
    /// spends an offset only on a tap that had a live target to hit.
    #[arg(long, conflicts_with_all = ["probe_melee", "probe_grenade", "probe_grenade_death"])]
    probe_sweep: bool,
    /// Runs the hit pair's melee script instead of its bullet one: the shooter
    /// walks to within 40 units and taps the melee bit rather than the
    /// trigger. Both halves write <map>-<gametype>-melee-shooter.txt and
    /// -melee-target.txt, so pass it to the --probe-target half too or that
    /// half overwrites the committed bullet fixture.
    #[arg(long, conflicts_with_all = ["probe_sweep", "probe_grenade", "probe_grenade_death"])]
    probe_melee: bool,
    /// Runs the hit pair's grenade script: the shooter walks to within 300
    /// units, switches to the frag, cooks for a second, releases at the
    /// target's feet, watches the missile out and then throws a second one,
    /// barely cooked, at the ground beside it. Writes
    /// <map>-<gametype>-grenade-shooter.txt and -grenade-target.txt; pass it
    /// to the --probe-target half as well.
    #[arg(long, conflicts_with_all = ["probe_sweep", "probe_melee", "probe_grenade_death"])]
    probe_grenade: bool,
    /// The grenade script with a `kill` sent 500 ms into the cook, which is
    /// what puts the grenade a death drops on the wire. Writes
    /// <map>-<gametype>-grenade-death-shooter.txt and -grenade-death-target.txt.
    #[arg(long, conflicts_with_all = ["probe_sweep", "probe_melee", "probe_grenade"])]
    probe_grenade_death: bool,
    /// Suffix for the --save-entities and combat fixture names, so a capture
    /// taken under different conditions lands beside the plain one rather
    /// than on top of it: --capture-tag players writes
    /// <map>-<gametype>-players.txt. A tagged write refuses a path that
    /// already exists, since a tag can spell a committed fixture's name;
    /// --overwrite-fixture is how you mean it.
    #[arg(long)]
    capture_tag: Option<String>,
    /// Let a --capture-tag write replace a fixture that is already there.
    /// Without it the run fails rather than putting a capture taken against
    /// vcod's own server where the retail oracle lives.
    #[arg(long)]
    overwrite_fixture: bool,
    /// Which team --net-probe answers the stock team menu with: allies, axis,
    /// autoassign or spectator. On its own it makes the probe join and then
    /// report the roster once a second, which is how two probes on opposite
    /// teams measured what clientState.team carries.
    #[arg(long)]
    probe_team: Option<String>,
    /// What --net-probe answers the stock weapon menu with, instead of the
    /// nationality's default rifle: any weapon that menu allows, e.g.
    /// kar98k_sniper_mp on axis. A weapon the menu refuses reopens it and the
    /// probe never spawns.
    #[arg(long)]
    probe_weapon: Option<String>,
    /// Seconds --net-probe stays connected; an SD round boundary needs a few minutes
    #[arg(long, default_value_t = 65)]
    probe_secs: u64,
    /// Run without sound (also what happens when no output device opens).
    #[arg(long)]
    no_audio: bool,
    /// Master volume, 0.0 to 1.0.
    #[arg(long, default_value_t = 1.0)]
    volume: f32,
}

/// The map every mode reads: the renderer builds from it, entities resolve
/// submodels in it. `None` while `--connect` is between maps.
struct World {
    bsp: bsp::Bsp,
}

/// Where `--connect` is between connecting and drawing a map.
enum Phase {
    Connecting {
        since: Instant,
    },
    Loading {
        loader: loading::MapLoader,
    },
    /// Boxed to keep the variants a similar size.
    Live(Box<LivePhase>),
}

/// Everything `--connect` needs to draw a map.
struct LivePhase {
    world: collision::CollisionWorld,
    /// Configstring 7's weapons, for prediction.
    weapons: Vec<Option<weapon::WeaponDef>>,
    scene: entities::EntityScene,
    events: net::events::EventTracker,
    /// Our own ring's events already played off the prediction.
    predicted_events: play::events::PredictedEvents,
    clock: ServerClock,
    last_loop_snap: Option<u32>,
    /// Last frame's `entity_pos`, what prediction clips (docs/research/cod11-player-clip.md).
    drawn_pos: HashMap<u32, Vec3>,
}

fn live_phase(fs: &Pk3Fs, bsp: &bsp::Bsp, net: &net::NetClient<net::UdpTransport>) -> Phase {
    let world = collision::CollisionWorld::build(bsp, &props::collision_tris(fs, &bsp.entities));
    let gametype = net::info_value_for_key(net.configstring(0), "g_gametype").unwrap_or("");
    world.unlink_script_brushes(&bsp.entities, gametype);
    Phase::Live(Box::new(LivePhase {
        world,
        weapons: vcod_common::weapon_table::from_configstring(fs, net.configstring(7)),
        scene: entities::EntityScene::new(),
        events: net::events::EventTracker::new(),
        predicted_events: play::events::PredictedEvents::default(),
        clock: ServerClock::new(),
        last_loop_snap: None,
        drawn_pos: HashMap::new(),
    }))
}

enum Mode {
    /// The menu-only mode: no world, no server, no connection - just the
    /// client-owned screens. What Leave Match returns to, so a player is never
    /// stuck in a match they cannot exit.
    Menu {
        /// The client-owned menu screens.
        screens: Box<hud::screens::Screens>,
        /// The menu currently drawn.
        screen_menu: Option<hud::shell::Menu>,
    },
    Fly(FlyCamera),
    /// Position follows the interpolated playerstate. The client joins
    /// through the stock menus and plays or spectates as the snapshot says.
    Online {
        /// Boxed to keep the variants a similar size.
        net: Box<net::NetClient<net::UdpTransport>>,
        cam: FlyCamera,
        /// Boxed to keep the variants a similar size.
        input: Box<play::input::PlayInput>,
        clock: play::cmds::CmdClock,
        ring: play::cmds::CmdRing,
        /// Boxed to keep the variants a similar size.
        predictor: Box<play::predict::Predictor>,
        /// Boxed to keep the variants a similar size.
        view: Box<play::view::OnlineView>,
        phase: Phase,
        /// Boxed to keep the variants a similar size.
        join: Box<play::join::Join>,
        /// The one online modal presentation. Shell screens, server script
        /// menus and Create-a-Class replace one another rather than stacking.
        modal: Option<hud::modal::Modal>,
        /// The client-owned menu screens (main, play, pause, class picker).
        /// While one is up it owns input: gameplay keys are not built into a
        /// usercmd and the mouse does not turn the view.
        screens: Box<hud::screens::Screens>,

        /// The server's authoritative profile, assembled as its commands
        /// arrive. Create-a-Class and the picker read it, so neither can show
        /// a class the server does not have.
        profile: vcod_common::classes::wire::ProfileSync,
        /// The end-of-match map-vote panel, driven by the server's `mapvote`
        /// commands. Empty when no vote is open.
        vote: hud::vote::VotePanel,
    },
    Walk {
        /// Boxed to keep the variants a similar size.
        world: Box<collision::CollisionWorld>,
        /// Boxed to keep the variants a similar size.
        ps: Box<pmove::PlayerState>,
        input: pmove::PmInput,
        keys: WalkKeys,
        motion: viewmodel::ViewmodelMotion,
        /// `None` when the anims failed to load; the viewmodel then draws
        /// statically. Boxed to keep the variants a similar size.
        view_weapon: Option<Box<viewmodel::ViewWeapon>>,
        /// Minimal configstring table so weapon cues resolve through the same
        /// path as `--connect`: CS 7 carries [`WALK_LOADOUT`].
        configstrings: Vec<String>,
        /// Active index into [`WALK_LOADOUT`].
        weapon_slot: usize,
        /// Rounds behind the clip; refilled per weapon from its file.
        reserve: u32,
        /// Digit press latched until the next redraw loads that slot.
        switch_to: Option<usize>,
        /// Raw counts since the last frame, drained into the sway once per redraw.
        mouse_delta: (f32, f32),
        /// Press edges latched until the next redraw; `ads_held`/`fire_held` are level.
        fire_edge: bool,
        fire_held: bool,
        reload_edge: bool,
        ads_held: bool,
    },
}

/// Held movement keys, folded into `PmInput`'s float axes once per frame.
#[derive(Default)]
struct WalkKeys {
    w: bool,
    s: bool,
    a: bool,
    d: bool,
}

/// The walk-mode arsenal on number keys 1..=N: every retail archetype (semi
/// pistol, full-auto SMGs, auto rifle, big-clip bolt rifle) plus kar98k as
/// the baseline. Files carry `semiAuto`, `startAmmo`, `adsBobFactor`.
const WALK_LOADOUT: [&str; 6] = [
    "colt_mp",
    "thompson_mp",
    "mp40_mp",
    "mp44_mp",
    "enfield_mp",
    "kar98k_mp",
];

fn digit_slot(code: KeyCode) -> Option<usize> {
    Some(match code {
        KeyCode::Digit1 => 0,
        KeyCode::Digit2 => 1,
        KeyCode::Digit3 => 2,
        KeyCode::Digit4 => 3,
        KeyCode::Digit5 => 4,
        KeyCode::Digit6 => 5,
        _ => return None,
    })
}

/// The playable maps this install offers, for the menu's map selector.
///
/// Discovery is from the paks, plus the later-engine maps whose source data is
/// present locally: a native MW2 map is offered only when the user actually has
/// the install it reads from.
fn maplist_for(fs: &Pk3Fs) -> vcod_common::maplist::MapRegistry {
    let mut registry = vcod_common::maplist::MapRegistry::discover(fs);
    if let Some(install) = vcod_common::maplist::find_mw2_install() {
        registry.include_iw4(|id| vcod_common::maplist::iw4_map_available(&install, id));
    }
    registry
}

fn map_available_locally(fs: &Pk3Fs, map: &str) -> bool {
    fs.resolve_map(map).is_some()
        || vcod_common::maplist::find_mw2_install()
            .is_some_and(|install| vcod_common::maplist::iw4_map_available(&install, map))
}

/// The Create-a-Class profile directory a locally-spawned server reads. Prefers
/// the repository-relative path (the client is normally run from the repo root),
/// then one beside the executable.
fn find_classes_dir(exe_dir: &std::path::Path) -> std::path::PathBuf {
    let cwd = std::path::PathBuf::from("assets/classes");
    if cwd.is_dir() {
        return cwd;
    }
    let beside = exe_dir.join("assets/classes");
    if beside.is_dir() {
        return beside;
    }
    cwd
}

/// A fresh `--connect` mode: the state machine, the join handshake and the
/// client-owned screens, over a new net client. Factored out so a locally
/// spawned match and a command-line `--connect` build exactly the same mode.
fn online_mode(
    net: net::NetClient<net::UdpTransport>,
    fs: &Pk3Fs,
    team: Option<String>,
    weapon: Option<String>,
) -> Mode {
    Mode::Online {
        net: Box::new(net),
        cam: FlyCamera::new(Vec3::ZERO, 0.0),
        input: Box::default(),
        clock: play::cmds::CmdClock::default(),
        ring: play::cmds::CmdRing::default(),
        predictor: Box::default(),
        view: Box::default(),
        phase: Phase::Connecting {
            since: Instant::now(),
        },
        join: Box::new(play::join::Join::new(team, weapon)),
        modal: None,
        screens: Box::new(hud::screens::Screens::new(maplist_for(fs))),
        profile: vcod_common::classes::wire::ProfileSync::new(),
        vote: hud::vote::VotePanel::default(),
    }
}

/// Retail's `config_mp.cfg` binds for `--connect`; the mouse buttons and
/// wheel are mapped where their events arrive.
fn play_action(code: KeyCode) -> Option<play::input::Action> {
    use play::input::Action;
    Some(match code {
        KeyCode::KeyW => Action::Forward,
        KeyCode::KeyS => Action::Back,
        KeyCode::KeyA => Action::Left,
        KeyCode::KeyD => Action::Right,
        KeyCode::Space => Action::Jump,
        KeyCode::KeyC => Action::Crouch,
        KeyCode::ControlLeft | KeyCode::ControlRight => Action::Prone,
        // Sprint, a vcod addition - retail binds SHIFT to "+melee" and puts
        // its walk/run toggle on MOUSE2, which no capture ever sends.
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Action::Sprint,
        KeyCode::KeyF => Action::Use,
        KeyCode::KeyR => Action::Reload,
        KeyCode::KeyQ => Action::LeanLeft,
        KeyCode::KeyE => Action::LeanRight,
        // Melee moves off SHIFT to make room for sprint, so it gets V as well
        // as the Shift binding a player used to have.
        KeyCode::KeyV => Action::Melee,
        // `weaponslot primary`, `primaryb`, `pistol`, `grenade`. G is an
        // extra alias for the grenade slot; digit 4 and the wheel both still
        // reach it.
        KeyCode::KeyG => Action::Slot(4),
        KeyCode::Digit1 => Action::Slot(1),
        KeyCode::Digit2 => Action::Slot(2),
        KeyCode::Digit3 => Action::Slot(3),
        KeyCode::Digit4 => Action::Slot(4),
        _ => return None,
    })
}

impl WalkKeys {
    /// (forward, right) in -1..1, opposite keys cancel.
    fn axes(&self) -> (f32, f32) {
        let axis = |pos: bool, neg: bool| (pos as i32 - neg as i32) as f32;
        (axis(self.w, self.s), axis(self.d, self.a))
    }
}

/// Render this far behind the newest snapshot so a straddling pair is always
/// buffered; tolerates one dropped snapshot at 20 Hz.
const INTERP_DELAY_MS: i32 = 100;

/// Server now = newest snapshot time plus wall time since it was first seen,
/// so interpolation sweeps between 20 Hz snapshots instead of stepping. A new
/// snapshot re-anchors: continuous when on schedule, a snap after a long gap.
/// One per live map, so a new gamestate starts it afresh.
struct ServerClock {
    /// (newest snapshot server time, local ms it was first seen).
    anchor: Option<(i32, f64)>,
    /// The last time returned; a late snapshot re-anchors behind it.
    drawn: i32,
}

impl ServerClock {
    fn new() -> Self {
        Self {
            anchor: None,
            drawn: i32::MIN,
        }
    }

    /// Server time to interpolate at; `local_ms` is a monotonic wall clock.
    /// Never below a time already returned: the clock holds until a late
    /// snapshot's anchor catches up rather than stepping entities and the
    /// HUD's timers back.
    fn render_time(&mut self, local_ms: f64, newest: i32) -> i32 {
        match self.anchor {
            Some((t, _)) if t == newest => {}
            _ => self.anchor = Some((newest, local_ms)),
        }
        let (anchor_time, anchor_local) = self.anchor.unwrap();
        let server_now = anchor_time as f64 + (local_ms - anchor_local);
        self.drawn = self.drawn.max((server_now - INTERP_DELAY_MS as f64) as i32);
        self.drawn
    }
}

/// F3 overlay text, top line first. A free function because the caller holds
/// the renderer borrowed out of `App`.
#[allow(clippy::too_many_arguments)]
fn hud_lines(
    mode: &Mode,
    // The online entity scene; fly/walk have none.
    scene: Option<&entities::EntityScene>,
    stats: &hud_text::HudStats,
    // (build_instances, render, fx step+build_quads) ms
    cpu_ms: (f32, f32, f32),
    // (events drained, of those unrecognized)
    ev_counts: (u64, u64),
    // (particles, decals, lights)
    fx_counts: (usize, usize, usize),
    // (hud quads, hud build+upload ms, Hud::unknown); zeros outside Online
    hud_counts: (usize, f32, u64),
    audio: audio::AudioStats,
    r: &Renderer,
) -> Vec<String> {
    let fps = if stats.dt_smooth > 0.0 {
        1.0 / stats.dt_smooth
    } else {
        0.0
    };
    let (build_ms, render_ms, fx_ms) = cpu_ms;
    let mut lines = vec![
        format!(
            "fps {fps:5.1}  ({:5.2} ms, worst {:5.1})",
            stats.dt_smooth * 1000.0,
            stats.worst_ms
        ),
        format!("cpu ms: build {build_ms:5.2}  render {render_ms:5.2}"),
    ];
    let (fx_particles, fx_decals, fx_lights) = fx_counts;
    lines.push(format!(
        "fx p{fx_particles} d{fx_decals} l{fx_lights} {fx_ms:.2}ms"
    ));
    let (draws, insts, bones) = r.debug_counts();
    lines.push(format!("draw: world {draws}  inst {insts}  bones {bones}"));
    let vc = r.vis_counts();
    lines.push(format!(
        "vis: {} cells {}/{} soups {}/{} tris {}/{} props {}/{} occ {} {}  {:.2}ms",
        vc.mode.map_or("-".to_string(), |m| m.to_string()),
        vc.cells.0,
        vc.cells.1,
        vc.soups.0,
        vc.soups.1,
        vc.tris.0,
        vc.tris.1,
        vc.props.0,
        vc.props.1,
        vc.occluders,
        vc.portals_occluded,
        vc.gather_ms
    ));
    let (hud_quads, hud_ms, hud_unknown) = hud_counts;
    lines.push(format!("hud q{hud_quads} {hud_ms:.2}ms unk {hud_unknown}"));
    let (stage_draws, dropped, warns) = r.shader_stats();
    lines.push(format!(
        "shader: sky {}  stages {stage_draws}  dropped {dropped}  warns {warns}",
        r.sky_name().unwrap_or("none")
    ));
    lines.push(format!("fog: {}", r.fog_debug()));
    lines.push(format!(
        "audio v{} plays {} miss {} cull {} drop {} steal {} {:.2}ms",
        audio.voices,
        audio.plays,
        audio.misses,
        audio.culled,
        audio.drops,
        audio.steals,
        audio.step_ms
    ));
    let cam_line = |tag: &str, pos: Vec3, yaw: f32, pitch: f32| {
        format!(
            "{tag}: {:6.0} {:6.0} {:5.0}  yaw {:4.0} pitch {:3.0}",
            pos.x,
            pos.y,
            pos.z,
            yaw.to_degrees(),
            pitch.to_degrees()
        )
    };
    match mode {
        Mode::Menu { .. } => lines.push("menu: multiplayer (no match)".to_string()),
        Mode::Fly(cam) => lines.push(cam_line("fly", cam.pos, cam.yaw, cam.pitch)),
        Mode::Walk { ps, .. } => {
            lines.push(cam_line("walk", ps.origin, ps.yaw, ps.pitch));
            lines.push(format!(
                "vel {:4.0}  ground {}",
                ps.velocity.length(),
                ps.on_ground as u8
            ));
        }
        Mode::Online {
            net,
            cam,
            predictor,
            ..
        } => {
            lines.push(cam_line("online", cam.pos, cam.yaw, cam.pitch));
            let error = predictor.drawn_error();
            lines.push(format!(
                "pred {} miss {} err {:.1}",
                if error.is_some() { "on" } else { "off" },
                predictor.misses,
                error.unwrap_or(0.0)
            ));
            lines.push(format!(
                "net: {:?}  drops {}",
                net.state(),
                net.packet_drops()
            ));
            if let Some(snap) = net.snapshots().newest() {
                lines.push(format!(
                    "snap: ents {}  players {}  age {:3} ms",
                    snap.entities.len(),
                    snap.clients.len(),
                    net.snapshot_age().as_millis()
                ));
            }
            lines.push(format!(
                "interp miss/s {:4.1}  anim restarts/s {:4.1}",
                stats.misses_per_s, stats.restarts_per_s
            ));
            if let Some(scene) = scene {
                if scene.stats.pending_assemblies > 0 {
                    lines.push(format!(
                        "loading: {} assemblies pending",
                        scene.stats.pending_assemblies
                    ));
                }
            }
            if let Some((got, size)) = net.download_progress() {
                lines.push(format!("download: {got}/{size} bytes"));
            }
            let (ev_seen, ev_unknown) = ev_counts;
            lines.push(format!("ev {ev_seen} unk {ev_unknown}"));
        }
    }
    lines
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StartupKind {
    Frontend,
    LocalMap,
    Online,
}

fn startup_kind(map: Option<&str>, connected: bool) -> StartupKind {
    if connected {
        StartupKind::Online
    } else if map.is_some() {
        StartupKind::LocalMap
    } else {
        StartupKind::Frontend
    }
}

fn frontend_mode(registry: vcod_common::maplist::MapRegistry) -> Mode {
    let mut screens = Box::new(hud::screens::Screens::frontend(registry));
    screens.open(hud::screens::Screen::Main);
    let screen_menu = screens.menu();
    Mode::Menu {
        screens,
        screen_menu,
    }
}

/// Tear a match down to the worldless frontend.
///
/// This is the whole "leave match" contract in one place: every piece of
/// per-match state is dropped so the frontend renders over an empty view with
/// no live world, HUD or ambient behind it. It is a free function so the
/// teardown can be tested without a window or a GPU - the renderer half is
/// skipped when `renderer` is `None`, which is exactly the headless case.
fn teardown_to_frontend(
    fs: &Pk3Fs,
    world: &mut Option<World>,
    fx: &mut fx::sim::FxSystem,
    hud: &mut Option<hud::Hud>,
    audio: &mut audio::AudioSystem,
    renderer: Option<&mut Renderer>,
) -> Mode {
    if let Some(r) = renderer {
        r.unload_world();
        r.set_viewmodel(fs, &[]);
        r.set_fog(None, 0.0);
        r.set_fx_quads(fs, Vec::new());
        r.set_fx_lights(&[]);
        r.set_hud_quads(fs, Vec::new());
    }
    *world = None;
    fx.clear();
    if let Some(hud) = hud {
        hud.on_gamestate();
    }
    audio.set_ambient(fs, None);
    frontend_mode(maplist_for(fs))
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let dir = args.game_dir.join(&args.mod_dir);

    if let Some(addr) = &args.net_probe {
        // Sound cue resolution needs the game data, the wire-level prints do
        // not, so a failed open only costs the audio line.
        let fs = match Pk3Fs::open_layered(&dir, args.overlay_dir.as_deref()) {
            Ok(fs) => Some(fs),
            Err(e) => {
                log::warn!(
                    "cannot open game data in {} ({e:#}); the probe will not resolve sound aliases",
                    dir.display()
                );
                None
            }
        };
        use probe::ShooterScript;
        let script = match () {
            _ if args.probe_sweep => ShooterScript::Sweep,
            _ if args.probe_melee => ShooterScript::Melee,
            _ if args.probe_grenade => ShooterScript::Grenade,
            _ if args.probe_grenade_death => ShooterScript::GrenadeDeath,
            _ => ShooterScript::Hit,
        };
        return probe::probe(
            addr,
            probe::Save {
                fixture: args.save_fixture,
                snapshots: args.save_snapshots,
                configstrings: args.save_configstrings,
                playerstate: args.save_playerstate,
                motion: args.save_motion,
                slope: args.save_slope,
                prone: args.probe_prone,
                combat: args.save_combat,
                ads: args.save_ads,
                grenade: args.save_grenade,
                sway: args.probe_sway,
                entities: args.save_entities,
                hit: args.save_hit,
                target: args.probe_target,
                mapchange: args.save_mapchange,
                roundrestart: args.save_roundrestart,
                plant: args.probe_plant,
                defuse: args.probe_defuse,
                pickup: args.save_pickup,
                turret: args.save_turret,
                bump: args.save_bump,
                bump_target: args.probe_bump_target,
                follow: args.probe_follow,
                killcam: args.probe_killcam,
                killcam_skip_ms: args.probe_killcam_skip_ms,
            },
            args.capture_tag.clone(),
            args.overwrite_fixture,
            args.probe_pvs,
            args.probe_slope,
            args.probe_triggers,
            args.probe_cmd_ms,
            script,
            args.probe_team.as_deref(),
            args.probe_weapon.as_deref(),
            args.probe_secs,
            fs.as_ref(),
        );
    }

    let mut fs = Pk3Fs::open_layered(&dir, args.overlay_dir.as_deref())
        .with_context(|| format!("cannot open game data in {}", dir.display()))?;

    if args.list {
        for name in fs.find_maps() {
            println!("{name}");
        }
        return Ok(());
    }

    let net_client = match &args.connect {
        Some(addr) => Some(
            net::NetClient::connect(addr)
                .with_context(|| format!("cannot open a socket to {addr}"))?,
        ),
        None => None,
    };
    let startup = startup_kind(args.map.as_deref(), net_client.is_some());

    // Fly and walk need their map up front; online learns it from the
    // server inside the loop and loads through the same path as a map change.
    let local = if startup == StartupKind::LocalMap {
        let map = args.map.as_deref().expect("local-map startup has a map");
        // Try IW4 (MW2) maps first
        if let Some(bsp) = try_load_iw4_map(map, &mut fs)? {
            Some((map.to_string(), bsp))
        } else {
            let Some(path) = fs.resolve_map(map) else {
                let all = fs.find_maps();
                let needle = map.to_lowercase();
                let similar: Vec<String> = all
                    .iter()
                    .filter(|m| m.to_lowercase().contains(&needle))
                    .cloned()
                    .collect();
                let suggestions = if similar.is_empty() { all } else { similar };
                bail!("map '{map}' not found; similar: {}", suggestions.join(", "));
            };
            let data = fs
                .read(&path)
                .with_context(|| format!("cannot read {path}"))?;
            let bsp = bsp::parse(&data).with_context(|| format!("cannot parse {path}"))?;
            Some((map.to_string(), bsp))
        }
    } else {
        None
    };

    let (viewmodel, view_weapon) = if args.walk && net_client.is_none() {
        viewmodel::load_view_weapon(&fs, "kar98k_mp").unwrap_or_else(|| {
            log::warn!("no viewmodel; walking without one");
            (Vec::new(), None)
        })
    } else {
        (Vec::new(), None)
    };

    let (hud, localized) = if startup != StartupKind::LocalMap {
        let hud = match hud::Hud::new(&fs) {
            Ok(hud) => Some(hud),
            Err(e) => {
                log::warn!("hud: {e}, disabling the on-screen HUD");
                None
            }
        };
        (hud, vcod_common::localize::Localized::load(&fs))
    } else {
        (None, vcod_common::localize::Localized::default())
    };

    // Fly and walk have no aliases, but `.efx` spawns carry cues and need a listener.
    let mut audio = audio::AudioSystem::new(
        &fs,
        audio::AudioOpts {
            enabled: !args.no_audio,
            volume: args.volume,
        },
    );
    if !audio.enabled() {
        println!("audio: silent (no output device, or --no-audio)");
    }

    let (mode, world, title) = if let Some(net) = net_client {
        (
            online_mode(net, &fs, args.team.clone(), args.weapon.clone()),
            None,
            "CoD - UO — connecting".to_string(),
        )
    } else if startup == StartupKind::LocalMap {
        let (map, bsp) = local.expect("fly or walk without a local map");
        audio.on_gamestate(&map);
        // No server to send configstring 3; every stock MP map ships an
        // `ambient_<map>` alias (iw_sound.csv), unknown names just stay silent.
        let ambient = format!("ambient_{map}");
        audio.set_ambient(&fs, Some(&ambient));
        let mode = if args.walk {
            walk_mode(&map, &bsp, &fs, view_weapon)?
        } else {
            Mode::Fly(match bsp::find_spawn(&bsp.entities) {
                Some((origin, yaw)) => FlyCamera::new(Vec3::from(origin) + Vec3::Z * 60.0, yaw),
                None => {
                    let (min, max) = mesh::map_bounds(&bsp);
                    let center = (Vec3::from(min) + Vec3::from(max)) * 0.5;
                    log::warn!("no player spawn found, starting at the center of the map");
                    FlyCamera::new(center, 0.0)
                }
            })
        };
        (mode, Some(World { bsp }), format!("CoD - UO — {map}"))
    } else {
        (
            frontend_mode(maplist_for(&fs)),
            None,
            "CoD - Untitled Offensive".to_string(),
        )
    };

    if startup == StartupKind::Frontend {
        println!("frontend: arrows or mouse navigate, Enter/click selects, Esc backs out");
    } else {
        println!("click to capture mouse, Esc to release");
    }
    if startup == StartupKind::Frontend {
        // The frontend has no gameplay controls.
    } else if args.walk {
        println!("WASD move, Space jump, Ctrl crouch, Z prone, Q/E lean, Shift walk");
        println!("LMB fire, RMB aim, R reload, 1-6 weapons");
    } else if args.connect.is_some() {
        println!("WASD move, Space jump/stand, C crouch, Ctrl prone, Q/E lean, Shift melee, F use");
        println!("LMB fire, RMB aim, R reload, 1-4 weapon slots, wheel next/prev weapon");
        println!("M opens the script menu; 0-9 or arrows + Enter pick, Esc closes");
    } else {
        println!("WASD + Space/Ctrl fly, Shift boost, scroll changes speed");
    }

    fx::registry::init(&fs);

    // Where a locally-spawned dedicated server lives, and what it needs to see
    // the same maps and classes this client does.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let classes_dir = find_classes_dir(&exe_dir);

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        title,
        world,
        fs,
        game_dir: args.game_dir.clone(),
        mod_dir: args.mod_dir.clone(),
        connect_addr: args.connect.clone(),
        mode,
        viewmodel,
        input: InputState::default(),
        window: None,
        renderer: None,
        grabbed: false,
        cursor_pos: (0.0, 0.0),
        quit_requested: false,
        fx: fx::sim::FxSystem::new(),
        start: Instant::now(),
        last_frame: Instant::now(),
        debug_overlay: args.debug_overlay,
        cull_mode: renderer::CullMode::On,
        hud_stats: hud_text::HudStats::new(),
        interp_misses: 0,
        ev_seen: 0,
        ev_unknown: 0,
        build_ms: 0.0,
        render_ms: 0.0,
        fx_ms: 0.0,
        hud,
        hud_ms: 0.0,
        localized,
        menus: hud::menu::MenuCache::default(),
        audio,
        quick_chat: quick_chat::QuickChat::new(0x51ee),
        error: None,
        exe_dir,
        overlay_dir: args.overlay_dir.clone(),
        classes_dir,
        local_server: None,
    };
    event_loop.run_app(&mut app)?;
    match app.error.take() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// The camera's (yaw, pitch) in radians for our own playerstate: the raw cmd
/// angles plus `delta_angles`, which is the view the server builds
/// (docs/protocol-1.1.md, "View angles"). Wire pitch is down-positive, the
/// camera's up-positive. Pitch is drawn clamped at retail's limit; the cmd
/// still carries the raw angle.
fn own_view(raw: [i32; 3], delta: [i32; 3]) -> (f32, f32) {
    let short = |i: usize| raw[i].wrapping_add(delta[i]) as i16;
    let deg = |s: i16| s as f32 * 360.0 / 65536.0;
    let pitch = short(0).clamp(-PITCH_CLAMP_SHORT, PITCH_CLAMP_SHORT);
    (deg(short(1)).to_radians(), -deg(pitch).to_radians())
}

/// Retail's view pitch clamp, 87.9 degrees in short units
/// (docs/research/cod11-gsc-object-model.md, the defender fixture's
/// `viewangles[0]`).
const PITCH_CLAMP_SHORT: i16 = 16000;

/// Synthetic muzzle for playerState-ring fire events (`entity_num ==
/// u32::MAX` in `net::events`): the ridden body is `skip_num`, never drawn,
/// so it has no real muzzle. 20 forward, 4 right, 3 down of the camera,
/// aimed down the view forward. Same shape as `BuiltScene::muzzles` entries.
fn view_muzzle(cam_pos: Vec3, forward: Vec3, right: Vec3, up: Vec3) -> (Vec3, Vec3) {
    (cam_pos + forward * 20.0 + right * 4.0 - up * 3.0, forward)
}

/// Leave the live map and enter Loading for the server's new map: drop the
/// GPU world and per-map state, reset what a gamestate invalidates, and list
/// the paks the client is missing.
#[allow(clippy::too_many_arguments)]
fn start_loading(
    net: &net::NetClient<net::UdpTransport>,
    hud: &mut Option<hud::Hud>,
    audio: &mut audio::AudioSystem,
    fx: &mut fx::sim::FxSystem,
    world: &mut Option<World>,
    r: &mut Renderer,
    window: Option<&Window>,
    title: &mut String,
    game_dir: &std::path::Path,
    mod_dir: &str,
) -> Result<Phase> {
    let map = net::info_value_for_key(net.configstring(0), "mapname")
        .map(str::to_string)
        .context("the server sent no mapname in its serverinfo")?;
    r.unload_world();
    *world = None;
    if let Some(hud) = hud {
        hud.on_gamestate();
    }
    audio.on_gamestate(&map);
    fx.clear();
    *title = format!("CoD - UO — loading {map}");
    if let Some(window) = window {
        window.set_title(title);
    }
    // Same candidate order as the old blocking downloader; `MapLoader` caps it.
    let systeminfo = net.configstring(1).to_string();
    let candidates = net::download::candidates_for_map(&systeminfo, &map, mod_dir, |rel| {
        game_dir.join(rel).exists()
    })
    .into_iter()
    .filter_map(|name| {
        let rel = net::download::safe_rel_path(&name, mod_dir)?;
        Some((format!("{name}.pk3"), game_dir.join(rel)))
    })
    .collect();
    Ok(Phase::Loading {
        loader: loading::MapLoader::new(map, candidates),
    })
}

/// Reopens the pk3 search path after a download. Everything read off the old
/// one is read again or dropped to reload lazily, since a new pak may carry
/// files the old ones lacked or replace one under the same name. The GPU
/// half is `Renderer::reopen`, the viewmodel rig `OnlineView::reopen`.
#[allow(clippy::too_many_arguments)]
fn reopen_fs(
    mod_dir: &std::path::Path,
    fs: &mut Pk3Fs,
    localized: &mut vcod_common::localize::Localized,
    menus: &mut hud::menu::MenuCache,
    hud: &mut Option<hud::Hud>,
    audio: &mut audio::AudioSystem,
    fx: &mut fx::sim::FxSystem,
    quick_chat: &mut quick_chat::QuickChat,
) -> Result<()> {
    *fs = Pk3Fs::open(mod_dir)?;
    *localized = vcod_common::localize::Localized::load(fs);
    *menus = hud::menu::MenuCache::default();
    match hud {
        Some(h) => {
            if let Err(e) = h.reopen(fs) {
                log::warn!("hud: {e}, keeping the fonts already loaded");
            }
        }
        None => *hud = hud::Hud::new(fs).ok(),
    }
    audio.reopen(fs);
    fx.reopen();
    fx::registry::init(fs);
    quick_chat.reopen();
    Ok(())
}

/// Try to load an IW4 (MW2) map from the local install.
/// Returns Ok(None) if this is not an IW4 map or no install is found.
fn try_load_iw4_map(map: &str, fs: &mut Pk3Fs) -> Result<Option<vcod_common::bsp::Bsp>> {
    use vcod_common::maplist::{find_mw2_install, iw4_map_available};

    let install = match find_mw2_install() {
        Some(p) => p,
        None => return Ok(None),
    };

    if !iw4_map_available(&install, map) {
        return Ok(None);
    }

    fs.mount_dir(&install.join("main"), "iwd")
        .with_context(|| format!("cannot mount MW2 image archives from {}", install.display()))?;

    let Some(bsp) = iw4::load_map_from_install(&install, map)? else {
        return Ok(None);
    };
    let ff_path = install.join("zone/english").join(format!("{map}.ff"));

    log::info!(
        "iw4: loaded {} from {} ({} verts, {} indices, {} surfaces, {} planes, {} brushes, {} collision verts, {} entities)",
        map,
        ff_path.display(),
        bsp.verts.len(),
        bsp.indices.len(),
        bsp.soups.len(),
        bsp.planes.len(),
        bsp.brushes.len(),
        bsp.collision_verts.len(),
        bsp.entities.len()
    );

    Ok(Some(bsp))
}

/// Parse the map, upload it to the GPU and hand back the live phase.
#[allow(clippy::too_many_arguments)]
fn load_map(
    map: &str,
    net: &net::NetClient<net::UdpTransport>,
    fs: &mut Pk3Fs,
    audio: &mut audio::AudioSystem,
    world: &mut Option<World>,
    r: &mut Renderer,
    window: Option<&Window>,
    title: &mut String,
) -> Result<Phase> {
    let bsp = if let Some(bsp) = try_load_iw4_map(map, fs)? {
        bsp
    } else {
        let Some(path) = fs.resolve_map(map) else {
            bail!("map '{map}' did not resolve in the pk3 search path");
        };
        let data = fs
            .read(&path)
            .with_context(|| format!("cannot read {path}"))?;
        bsp::parse(&data).with_context(|| format!("cannot parse {path}"))?
    };
    // The ambient rides configstring 3 (docs/research/cod11-sound-system.md,
    // section 9); a fresh gamestate may have changed it.
    audio.set_ambient(fs, net::info_value_for_key(net.configstring(3), "n"));
    *title = format!("CoD - UO — {map}");
    if let Some(window) = window {
        window.set_title(title);
    }
    let phase = live_phase(fs, &bsp, net);
    r.load_world(&bsp, fs)?;
    *world = Some(World { bsp });
    Ok(phase)
}

/// The between-maps frame: the status text over an empty view, with chat
/// still flowing through the normal HUD build (no snapshot clients).
#[allow(clippy::too_many_arguments)]
fn loading_frame(
    r: &mut Renderer,
    fs: &Pk3Fs,
    localized: &vcod_common::localize::Localized,
    hud: &mut Option<hud::Hud>,
    now: f32,
    aspect: f32,
    cull: renderer::CullMode,
    configstrings: &[String],
    text: String,
) -> renderer::Frame {
    if let Some(hud) = hud {
        let (screen_w, screen_h) = r.screen_size();
        let no_clients = BTreeMap::new();
        let f = hud::HudFrame {
            modal: None,
            vote: None,
            registry: None,
            now,
            screen_w,
            screen_h,
            configstrings,
            clients: &no_clients,
            protocol: &net::protocol::PROTOCOL_V1,
            server_time: 0,
            snap_time: 0,
            fs,
            ps: None,
            predicted: None,
            local_player: false,
            weapons: &[],
            localized,
            view_yaw: 0.0,
            eye: [0.0; 3],
            fov: camera::DEFAULT_FOV_DEG,
            entity_origin: &|_| None,
        };
        let quads = hud.build(&f);
        r.set_hud_quads(fs, quads);
    }
    renderer::Frame {
        // Nothing is drawn, so any projection works.
        view_proj: camera::view_proj_from(
            Vec3::ZERO,
            0.0,
            0.0,
            0.0,
            camera::DEFAULT_FOV_DEG,
            aspect,
        ),
        eye: Vec3::ZERO,
        fwd: camera::basis(0.0, 0.0).0,
        time: now,
        cull,
        hud_lines: vec![text],
    }
}

/// `size == 0` means no block has named the pak's size yet.
fn loading_text(map: &str, progress: Option<loading::Progress>) -> String {
    match progress {
        None => format!("Loading {map}"),
        Some(p) => {
            let size_kb = if p.size == 0 {
                "?".to_string()
            } else {
                format!("{}", p.size as u64 / 1024)
            };
            format!(
                "Downloading {map}: pak {}/{}  {} / {} KB",
                p.pak,
                p.paks,
                p.received / 1024,
                size_kb
            )
        }
    }
}

fn walk_mode(
    map: &str,
    bsp: &bsp::Bsp,
    fs: &Pk3Fs,
    view_weapon: Option<Box<viewmodel::ViewWeapon>>,
) -> Result<Mode> {
    let Some((origin, yaw)) = bsp::find_spawn(&bsp.entities) else {
        bail!("map {map} has no player spawn; run without --walk to fly");
    };
    let world = collision::CollisionWorld::build(bsp, &props::collision_tris(fs, &bsp.entities));
    if world.brushes.is_empty() && world.tris.is_empty() {
        bail!("no collidable geometry in {map}; run without --walk to fly");
    }

    let mut ps = pmove::PlayerState::spawn(Vec3::from(origin) + Vec3::Z * 2.0, yaw);
    let drop = world.box_trace(
        ps.origin,
        ps.origin - Vec3::Z * 4096.0,
        ps.mins(),
        ps.maxs(),
    );
    if drop.startsolid {
        // CoD spawns anyway; pmove usually pushes out of solid in the first frames.
        log::warn!("spawn point is inside solid geometry, spawning there anyway");
    } else if drop.fraction < 1.0 {
        ps.origin = drop.endpos;
    } else {
        log::warn!("no ground within 4096 units below the spawn point");
    }

    let mut configstrings = vec![String::new(); 8];
    // `entityState.weapon` is 1-based into CS 7; walk mode carries the loadout.
    configstrings[7] = WALK_LOADOUT.join(" ");
    let start_slot = WALK_LOADOUT
        .iter()
        .position(|&w| w == "kar98k_mp")
        .unwrap_or(0);
    let reserve = view_weapon.as_ref().map_or(0, |w| w.def.start_ammo);

    Ok(Mode::Walk {
        world: Box::new(world),
        ps: Box::new(ps),
        input: pmove::PmInput::default(),
        keys: WalkKeys::default(),
        motion: viewmodel::ViewmodelMotion::new(),
        view_weapon,
        configstrings,
        weapon_slot: start_slot,
        reserve,
        switch_to: None,
        mouse_delta: (0.0, 0.0),
        fire_edge: false,
        fire_held: false,
        reload_edge: false,
        ads_held: false,
    })
}

struct App {
    title: String,
    world: Option<World>,
    fs: Pk3Fs,
    /// Where downloads land and the pk3 path reopens from.
    game_dir: std::path::PathBuf,
    mod_dir: String,
    /// `--connect` target, for the connecting screen text.
    connect_addr: Option<String>,
    mode: Mode,
    viewmodel: Vec<xmodel::XModel>,
    /// Fly-mode keys; walk keeps its own in `Mode::Walk`.
    input: InputState,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    grabbed: bool,
    /// Last window-space pointer position, used for menu clicks.
    cursor_pos: (f32, f32),
    /// Set by the menu action; the event handler exits so `App` is dropped.
    quit_requested: bool,
    fx: fx::sim::FxSystem,
    /// Origin of the clock effects are timed against.
    start: Instant,
    last_frame: Instant,
    debug_overlay: bool,
    cull_mode: renderer::CullMode,
    hud_stats: hud_text::HudStats,
    /// Online frames without a straddling snapshot pair. Cumulative; the
    /// overlay shows the rate.
    interp_misses: u64,
    /// Cumulative events drained, and those with an unrecognized code.
    ev_seen: u64,
    ev_unknown: u64,
    build_ms: f32,
    render_ms: f32,
    fx_ms: f32,
    hud: Option<hud::Hud>,
    hud_ms: f32,
    /// Menu labels; empty outside `--connect`.
    localized: vcod_common::localize::Localized,
    menus: hud::menu::MenuCache,
    audio: audio::AudioSystem,
    quick_chat: quick_chat::QuickChat,
    error: Option<anyhow::Error>,
    /// The directory holding `vcod.exe` and its sibling `vcod-server.exe`, so
    /// a local match can be spawned without a hard-coded path.
    exe_dir: std::path::PathBuf,
    /// The pk3 overlay the client resolved its own content from, passed to a
    /// locally-spawned server so it sees the same maps.
    overlay_dir: Option<std::path::PathBuf>,
    /// The Create-a-Class profile directory a locally-spawned server reads.
    classes_dir: std::path::PathBuf,
    /// The dedicated server this client started, if any. `None` means either
    /// no match, or a match the client did not own.
    local_server: Option<LocalServer>,
}

impl App {
    fn screen_captures_input(&self) -> bool {
        match &self.mode {
            Mode::Menu { .. } => true,
            Mode::Online { modal, .. } => modal.is_some(),
            _ => false,
        }
    }

    fn gameplay_input_active(&self) -> bool {
        match &self.mode {
            Mode::Fly(_) | Mode::Walk { .. } => true,
            Mode::Online { phase, modal, .. } => {
                online_gameplay_owns_input(matches!(phase, Phase::Live(_)), modal.is_some())
            }
            Mode::Menu { .. } => false,
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, err: anyhow::Error) {
        self.error = Some(err);
        event_loop.exit();
    }

    /// Whether Esc should open the pause menu: an online match with a live
    /// phase (a menu over the map), not the connecting or loading screens.
    fn pause_available(&self) -> bool {
        matches!(
            &self.mode,
            Mode::Online {
                phase: Phase::Live(_),
                ..
            }
        )
    }

    fn set_grab(&mut self, grabbed: bool) {
        let Some(window) = &self.window else { return };
        if grabbed {
            if window.set_cursor_grab(CursorGrabMode::Locked).is_err()
                && window.set_cursor_grab(CursorGrabMode::Confined).is_err()
            {
                log::warn!("this platform does not support grabbing the cursor");
                return;
            }
        } else if window.set_cursor_grab(CursorGrabMode::None).is_err() {
            return;
        }
        window.set_cursor_visible(!grabbed);
        self.grabbed = grabbed;
    }

    /// On grab release or focus loss, so no held key stays latched. Prone
    /// and the `--connect` stance stay, neither is a held key. The scoreboard drops, Tab is held too.
    fn clear_held_keys(&mut self) {
        match &mut self.mode {
            Mode::Menu { .. } => {}
            Mode::Fly(_) => self.input = InputState::default(),
            Mode::Online { input, .. } => input.release_all(),
            Mode::Walk {
                input,
                keys,
                fire_edge,
                fire_held,
                reload_edge,
                ads_held,
                ..
            } => {
                *keys = WalkKeys::default();
                input.jump = false;
                input.crouch = false;
                input.walk_slow = false;
                input.lean_left = false;
                input.lean_right = false;
                *fire_edge = false;
                *fire_held = false;
                *reload_edge = false;
                *ads_held = false;
            }
        }
        if let Some(hud) = &mut self.hud {
            hud.scoreboard.visible = false;
        }
    }

    /// The open script menu's keys, ahead of every other binding, and M to
    /// open the main menu when none is. False when the key is not the menu's.
    ///
    /// A client-owned screen ([`hud::screens`]) is checked first: while one is
    /// up it owns every key, so a menu keypress can never also reach a
    /// gameplay binding.
    fn menu_key(&mut self, code: KeyCode) -> bool {
        if self.screen_key(code) {
            return true;
        }
        let Mode::Online {
            net,
            join,
            modal,
            phase,
            profile,
            screens,
            vote,
            ..
        } = &mut self.mode
        else {
            return false;
        };
        // A modal owns every key, even one it does not bind. This prevents an
        // ignored menu key from becoming movement, fire, scoreboard or debug
        // input underneath it.
        if let Some(hud::modal::Modal::CreateAClass(cac)) = modal.as_mut() {
            match code {
                KeyCode::Escape | KeyCode::KeyB => *modal = None,
                KeyCode::ArrowLeft => cac.left(),
                KeyCode::ArrowRight => cac.right(),
                KeyCode::ArrowUp => cac.up(),
                KeyCode::ArrowDown => cac.down(),
                KeyCode::KeyW => cac.cycle_choice(-1),
                KeyCode::KeyS => cac.cycle_choice(1),
                KeyCode::KeyQ | KeyCode::PageUp => {
                    cac.switch_class(-1);
                }
                KeyCode::KeyE | KeyCode::PageDown => {
                    cac.switch_class(1);
                }
                KeyCode::Enter => {
                    // A cleared cell sends "-", so a slot the player emptied
                    // reaches the server as a clear rather than as a weapon
                    // literally named "-".
                    if let Some((class_index, slot, value)) = cac.commit() {
                        let cmd = format!("class {class_index} {slot} {value}");
                        log::info!("classes: {cmd}");
                        net.send_reliable(&cmd);
                    }
                }
                KeyCode::Backspace => {
                    if let Some((class_index, slot, _)) = cac.clear() {
                        let cmd = format!("class {class_index} {slot} -");
                        log::info!("classes: {cmd}");
                        net.send_reliable(&cmd);
                    }
                }
                KeyCode::Tab => {
                    net.send_reliable("saveclass");
                    log::info!("classes: saveclass");
                }
                _ => {}
            }
            return true;
        }
        // A pre-spawn script menu may be temporarily replaced by the pause
        // shell so Change Class can answer the retained Join request. Do not
        // call `join.close()` here: SelectClass rebuilds and answers that
        // weapon menu to release the authoritative spawn thread.
        if matches!(modal, Some(hud::modal::Modal::Script { .. }))
            && script_modal_shortcut(code) == Some(OnlineMenuShortcut::Pause)
        {
            screens.open(hud::screens::Screen::Pause);
            *modal = screens.menu().map(hud::modal::Modal::Shell);
            return true;
        }
        if let Some(hud::modal::Modal::Script { view, .. }) = modal.as_mut() {
            let response = match code {
                KeyCode::Escape => {
                    join.close();
                    *modal = None;
                    return true;
                }
                KeyCode::ArrowUp | KeyCode::KeyW => {
                    view.up();
                    return true;
                }
                KeyCode::ArrowDown | KeyCode::KeyS => {
                    view.down();
                    return true;
                }
                KeyCode::Enter | KeyCode::NumpadEnter => view.selected_response(),
                _ => digit_key(code).and_then(|key| view.response_for_key(key)),
            };
            if let Some(cmd) = response.and_then(|r| join.choose(r, net.server_id())) {
                net.send_reliable(&cmd);
            }
            return true;
        }
        // The map vote is an overlay, not a modal, so its digits are available
        // only after proving there is no modal owner.
        if vote.is_open() {
            if let Some(index) = vote_digit(code) {
                if vote.choose(index) {
                    log::info!("mapvote: voting {index}");
                    net.send_reliable(&format!("vote {index}"));
                }
                return true;
            }
        }
        match online_menu_shortcut(code, matches!(phase, Phase::Live(_)), modal.is_some()) {
            Some(OnlineMenuShortcut::Pause) => {
                screens.open(hud::screens::Screen::Pause);
                *modal = screens.menu().map(hud::modal::Modal::Shell);
                true
            }
            Some(OnlineMenuShortcut::CreateAClass) => {
                let mut editor =
                    hud::createaclass::CreateAClass::from_configstrings(net.configstrings());
                if let Some(profile) = profile.profile() {
                    editor.load(&profile, 0);
                }
                *modal = Some(hud::modal::Modal::CreateAClass(editor));
                true
            }
            None => false,
        }
    }
}

impl App {
    /// Route a key to the client-owned screens, when one is up. True when the
    /// key was consumed (or a screen was opened by it).
    ///
    /// The menu the key acts on is rebuilt from [`hud::screens::Screens`]
    /// first, so a stepped value is applied to the state that is actually
    /// held; it is then written back for the render loop to draw.
    ///
    /// Works in both [`Mode::Online`] and [`Mode::Menu`]: the screens own the
    /// key either way, and M opens the main menu when no screen is up.
    fn screen_key(&mut self, code: KeyCode) -> bool {
        let key = match code {
            KeyCode::ArrowUp | KeyCode::KeyW => hud::screens::MenuKey::Up,
            KeyCode::ArrowDown | KeyCode::KeyS => hud::screens::MenuKey::Down,
            KeyCode::ArrowLeft => hud::screens::MenuKey::Left,
            KeyCode::ArrowRight => hud::screens::MenuKey::Right,
            KeyCode::Enter | KeyCode::NumpadEnter => hud::screens::MenuKey::Activate,
            KeyCode::Escape => hud::screens::MenuKey::Back,
            _ => hud::screens::MenuKey::None,
        };
        // The action, taken while the mode is borrowed, run after.
        let mut action = None;
        let mut consumed = false;
        match &mut self.mode {
            Mode::Online { screens, modal, .. } => {
                if let Some(hud::modal::Modal::Shell(menu)) = modal.as_mut() {
                    // Reuse the menu already on screen so the cursor survives
                    // across keypresses; it is rebuilt only when a screen opens
                    // (and when the class list changes). Rebuilding per key
                    // would reset the selection to the first row on every press.
                    let (_, a) = screens.handle_key(menu, key);
                    action = a;
                    consumed = true;
                }
            }
            Mode::Menu {
                screens,
                screen_menu,
            } => {
                let mut menu = screen_menu
                    .take()
                    .unwrap_or_else(|| screens.menu().expect("the menu mode has a screen"));
                let (_, a) = screens.handle_key(&mut menu, key);
                *screen_menu = Some(menu);
                action = a;
                consumed = true;
            }
            _ => {}
        }
        if let Some(action) = action {
            self.run_screen_action(action);
        }
        consumed
    }

    fn screen_hover(&mut self, x: f32, y: f32) -> bool {
        let Some((screen_w, screen_h)) = self.renderer.as_ref().map(Renderer::screen_size) else {
            return false;
        };
        match &mut self.mode {
            Mode::Online { screens, modal, .. } => match modal.as_mut() {
                Some(hud::modal::Modal::Shell(menu)) => {
                    screens.handle_hover(menu, x, y, screen_w, screen_h)
                }
                Some(hud::modal::Modal::Script { view, .. }) => {
                    view.hover(x, y, screen_w, screen_h)
                }
                Some(hud::modal::Modal::CreateAClass(editor)) => {
                    editor.hover(x, y, screen_w, screen_h)
                }
                None => false,
            },
            Mode::Menu {
                screens,
                screen_menu,
            } => screen_menu
                .as_mut()
                .is_some_and(|menu| screens.handle_hover(menu, x, y, screen_w, screen_h)),
            _ => false,
        }
    }

    /// Handle a press while a client-owned screen is up. Returns true even
    /// when the pointer missed every row so a modal screen never clicks through.
    fn screen_click(&mut self) -> bool {
        let Some((screen_w, screen_h)) = self.renderer.as_ref().map(Renderer::screen_size) else {
            return false;
        };
        let (x, y) = self.cursor_pos;
        let mut action = None;
        let mut activate_modal = false;
        let consumed = match &mut self.mode {
            Mode::Online { screens, modal, .. } => {
                match modal.as_mut() {
                    Some(hud::modal::Modal::Shell(menu)) => {
                        action = screens.handle_click(menu, x, y, screen_w, screen_h);
                    }
                    Some(hud::modal::Modal::Script { view, .. }) => {
                        activate_modal = view.hover(x, y, screen_w, screen_h);
                    }
                    Some(hud::modal::Modal::CreateAClass(editor)) => {
                        if let Some(delta) =
                            editor.class_selector_direction(x, y, screen_w, screen_h)
                        {
                            editor.switch_class(delta);
                        } else {
                            activate_modal = editor.hover(x, y, screen_w, screen_h);
                        }
                    }
                    None => {}
                }
                modal.is_some()
            }
            Mode::Menu {
                screens,
                screen_menu,
            } => {
                if let Some(menu) = screen_menu.as_mut() {
                    action = screens.handle_click(menu, x, y, screen_w, screen_h);
                }
                true
            }
            _ => false,
        };
        if let Some(action) = action {
            self.run_screen_action(action);
        }
        if activate_modal {
            self.menu_key(KeyCode::Enter);
        }
        consumed
    }

    fn screen_wheel(&mut self, scroll: f32) -> bool {
        if scroll == 0.0 {
            return false;
        }
        let code = if scroll > 0.0 {
            KeyCode::ArrowUp
        } else {
            KeyCode::ArrowDown
        };
        self.menu_key(code)
    }

    /// Carry out a screen's action. Split out because it needs `&mut self`
    /// whole, while `screen_key` holds a borrow of the mode.
    fn run_screen_action(&mut self, action: hud::screens::Action) {
        use hud::screens::Action;
        match action {
            Action::Open(screen) => {
                self.screen_open(screen);
            }
            Action::Close => {
                self.screen_close();
                // Re-capture only when gameplay really owns input. A stock
                // script menu may still be open underneath the client-owned
                // screen (notably during pre-spawn class selection), and
                // Connecting/Loading never own gameplay input.
                let grab = self.gameplay_input_active();
                self.set_grab(grab);
            }
            Action::SelectClass(index) => {
                if let Mode::Online {
                    net,
                    join,
                    modal,
                    screens,
                    ..
                } = &mut self.mode
                {
                    // The server queues the class and applies it only through
                    // the normal spawn path. If the player is still parked on
                    // the stock pre-spawn weapon menu, answer that menu with
                    // its currently highlighted valid weapon after queuing the
                    // class. That releases the gametype's existing spawn
                    // thread, and the queued class replaces the stock loadout
                    // when that authoritative spawn is processed. Alive
                    // players have no weapon menu here, so their class still
                    // waits for the next legitimate respawn.
                    // The shell replaced (rather than covered) the stock menu.
                    // Rebuild that menu from Join only to obtain the response
                    // which releases the authoritative pre-spawn flow.
                    let mut script = None;
                    hud::menu::sync(
                        &mut script,
                        join,
                        &mut self.menus,
                        &self.fs,
                        &self.localized,
                        net.configstrings(),
                    );
                    let join_response = script
                        .as_ref()
                        .filter(|(name, _)| name.starts_with("weapon_"))
                        .and_then(|(_, view)| view.selected_response())
                        .map(str::to_string);

                    log::info!("classes: selectclass {index}");
                    net.send_reliable(&format!("selectclass {index}"));

                    if let Some(response) = join_response {
                        if let Some(cmd) = join.choose(&response, net.server_id()) {
                            log::info!(
                                "classes: releasing pre-spawn weapon menu with {response:?}"
                            );
                            net.send_reliable(&cmd);
                        }
                    }

                    screens.close();
                    *modal = None;
                }
                // If another stock menu is still waiting underneath the class
                // picker, leave the cursor released. Otherwise a live player
                // returns directly to gameplay.
                let grab = self.gameplay_input_active();
                self.set_grab(grab);
            }
            Action::OpenClassEditor => {
                // The assembled profile, taken before the mode is borrowed
                // mutably below.
                let profile = match &self.mode {
                    Mode::Online { profile, .. } => profile.profile(),
                    _ => None,
                };
                if let Mode::Online {
                    net,
                    modal,
                    screens,
                    ..
                } = &mut self.mode
                {
                    let mut editor =
                        hud::createaclass::CreateAClass::from_configstrings(net.configstrings());
                    // Open on the saved profile, not a blank grid: this is the
                    // round trip the screen was missing.
                    if let Some(profile) = &profile {
                        editor.load(profile, editor.class_index);
                    }
                    *modal = Some(hud::modal::Modal::CreateAClass(editor));
                    screens.close();
                }
            }
            Action::LeaveMatch => {
                self.leave_match();
            }
            Action::Quit => {
                self.quit_requested = true;
            }
            Action::StartMatch {
                playlist,
                map,
                gametype,
                bots,
                bot_combat,
            } => {
                // The settings come from the action, which the Play screen
                // built from the same held state these accessors expose.
                if let Mode::Online { screens, .. } = &self.mode {
                    debug_assert_eq!(screens.bot_count(), bots);
                    debug_assert_eq!(screens.bot_combat(), bot_combat);
                    debug_assert_eq!(screens.selected_map(), Some(map.as_str()));
                }
                self.start_match(MatchConfig {
                    map,
                    gametype,
                    bots,
                    bot_combat,
                });
                log::info!(
                    "menu: start match in playlist {playlist}, {bots} bots, bot_combat={bot_combat}"
                );
            }
            Action::Nothing => {}
        }
    }

    /// Open a screen on whichever mode is up.
    fn screen_open(&mut self, screen: hud::screens::Screen) {
        match &mut self.mode {
            Mode::Online { screens, modal, .. } => {
                screens.open(screen);
                *modal = screens.menu().map(hud::modal::Modal::Shell);
            }
            Mode::Menu {
                screens,
                screen_menu,
            } => {
                screens.open(screen);
                *screen_menu = screens.menu();
            }
            _ => {}
        }
    }

    /// Close the current screen on whichever mode is up.
    fn screen_close(&mut self) {
        match &mut self.mode {
            Mode::Online { screens, modal, .. } => {
                screens.close();
                *modal = None;
            }
            Mode::Menu {
                screens,
                screen_menu,
            } => {
                screens.close();
                *screen_menu = None;
            }
            _ => {}
        }
    }

    /// Start a match the client owns: spawn a dedicated server on a loopback
    /// port, then connect to it. Replaces the whole mode, so it works both from
    /// a running match and from the menu-only mode.
    fn start_match(&mut self, cfg: MatchConfig) {
        // A previous local server is stopped first: starting a second match
        // while one is running would leave the first orphaned.
        if let Some(mut old) = self.local_server.take() {
            old.stop();
        }

        let log_dir = std::path::PathBuf::from("dev-logs");
        let mut port = DEFAULT_LOCAL_PORT;
        let mut server = None;
        for _ in 0..4 {
            match LocalServer::spawn(
                &self.exe_dir,
                &self.game_dir,
                &self.mod_dir,
                self.overlay_dir.as_deref(),
                &self.classes_dir,
                &cfg,
                port,
                &log_dir,
            ) {
                Ok(s) => {
                    server = Some(s);
                    break;
                }
                Err(e) => {
                    log::warn!("menu: local server on udp/{port} failed: {e:#}");
                    port = local_server::next_port(port);
                }
            }
        }
        let Some(server) = server else {
            log::error!("menu: could not start a local match after several ports");
            self.notify("Could not start the server - see dev-logs/");
            return;
        };

        let addr = server.addr();
        match net::NetClient::connect(&addr) {
            Ok(net) => {
                let mode = online_mode(net, &self.fs, None, None);
                // Seed the new mode's registry with the maps this install
                // offers, including any later-engine ones.
                self.local_server = Some(server);
                self.connect_addr = Some(addr.clone());
                self.mode = mode;
                // Connecting/loading own the whole frame. Keep client-owned
                // screens closed here so the frontend cannot survive over the
                // loading screen or the live map. Esc may open Pause only once
                // the phase is actually Live.
                self.set_grab(false);
                if let Some(s) = &self.local_server {
                    log::info!(
                        "menu: connecting to the local match on {} (server log: {})",
                        addr,
                        s.log_path()
                            .map_or_else(|| "none".to_string(), |p| p.display().to_string())
                    );
                }
            }
            Err(e) => {
                let mut server = server;
                server.stop();
                log::error!("menu: cannot connect to the local server on {addr}: {e:#}");
                self.notify("Could not connect to the local match");
            }
        }
    }

    /// Leave the current match: disconnect, stop a server the client started,
    /// and return to the menu-only mode. A server the client did not start is
    /// left alone - it is not this client's to stop.
    ///
    /// This tears the match down rather than merely hiding it. Every piece of
    /// per-match state is dropped - the GPU world and sky, the collision BSP,
    /// the fog, fx, the HUD's match state, the ambient loop, held keys and
    /// mouse capture - so the frontend renders over an empty view with no live
    /// map behind it. Persistent state (the profile, the class list, settings)
    /// is deliberately not touched.
    fn leave_match(&mut self) {
        if let Mode::Online { net, .. } = &mut self.mode {
            net.disconnect();
        }
        if let Some(mut server) = self.local_server.take() {
            server.stop();
        } else if matches!(self.mode, Mode::Online { .. }) {
            log::info!("menu: left an external match (no local server to stop)");
        }
        self.connect_addr = None;

        // Tear the match down rather than merely hiding it: the frontend must
        // render over an empty view with no live map behind it.
        let mode = teardown_to_frontend(
            &self.fs,
            &mut self.world,
            &mut self.fx,
            &mut self.hud,
            &mut self.audio,
            self.renderer.as_mut(),
        );
        self.mode = mode;
        self.set_grab(false);
        log::info!("menu: returned to the multiplayer menu");
    }

    /// Put a short line in the F3 overlay / log when an action failed. Kept
    /// minimal: a modal error box is not worth building for two messages.
    fn notify(&mut self, msg: &str) {
        log::warn!("{msg}");
        println!("{msg}");
    }
}

/// The digit key that casts a map vote: 1-3 name the three candidates.
fn vote_digit(code: KeyCode) -> Option<usize> {
    Some(match code {
        KeyCode::Digit1 => 0,
        KeyCode::Digit2 => 1,
        KeyCode::Digit3 => 2,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OnlineMenuShortcut {
    Pause,
    CreateAClass,
}

fn online_gameplay_owns_input(live: bool, modal_open: bool) -> bool {
    live && !modal_open
}

/// Developer/menu shortcuts exist only in unobscured live gameplay. Keeping
/// this decision outside the event handler prevents Connecting and Loading
/// from acquiring a latent or visible modal owner.
fn online_menu_shortcut(
    code: KeyCode,
    live: bool,
    modal_open: bool,
) -> Option<OnlineMenuShortcut> {
    if !live || modal_open {
        return None;
    }
    match code {
        KeyCode::KeyM => Some(OnlineMenuShortcut::Pause),
        KeyCode::KeyB => Some(OnlineMenuShortcut::CreateAClass),
        _ => None,
    }
}

/// The only client-owned shortcut allowed to replace a stock script menu.
/// Join stays open underneath so pre-spawn Change Class can complete it.
fn script_modal_shortcut(code: KeyCode) -> Option<OnlineMenuShortcut> {
    (code == KeyCode::KeyM).then_some(OnlineMenuShortcut::Pause)
}

/// A menu `execKey` name for the digit row.
fn digit_key(code: KeyCode) -> Option<&'static str> {
    Some(match code {
        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        _ => return None,
    })
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(winit::dpi::LogicalSize::new(1600.0, 900.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                return self.fail(
                    event_loop,
                    anyhow::Error::new(e).context("cannot create a window"),
                );
            }
        };
        match Renderer::new(window.clone(), &self.fs) {
            Ok(mut r) => {
                if let Some(w) = &self.world {
                    if let Err(e) = r.load_world(&w.bsp, &self.fs) {
                        return self.fail(event_loop, e);
                    }
                }
                if !self.viewmodel.is_empty() {
                    r.set_viewmodel(&self.fs, &self.viewmodel);
                }
                self.renderer = Some(r);
            }
            Err(e) => return self.fail(event_loop, e),
        }
        window.request_redraw();
        self.window = Some(window);
        self.last_frame = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                // auto-repeat would retrigger the jump and walk's prone toggle
                if event.repeat {
                    return;
                }
                if pressed && self.menu_key(code) {
                    if self.screen_captures_input() {
                        self.set_grab(false);
                    } else {
                        self.set_grab(self.gameplay_input_active());
                    }
                    self.clear_held_keys();
                    if self.quit_requested {
                        event_loop.exit();
                    }
                    return;
                }
                if code == KeyCode::Escape && pressed {
                    // In a live match Esc opens the pause menu (a multiplayer
                    // menu overlay, not a simulation freeze). During connect or
                    // load it cancels through the same complete teardown as
                    // Leave Match, so there is no dead-end transition screen.
                    if matches!(
                        &self.mode,
                        Mode::Online {
                            phase: Phase::Connecting { .. } | Phase::Loading { .. },
                            ..
                        }
                    ) {
                        self.leave_match();
                        return;
                    }
                    if self.pause_available() {
                        if let Mode::Online { screens, modal, .. } = &mut self.mode {
                            screens.open(hud::screens::Screen::Pause);
                            *modal = screens.menu().map(hud::modal::Modal::Shell);
                        }
                        self.set_grab(false);
                        self.clear_held_keys();
                    } else {
                        self.set_grab(false);
                        self.clear_held_keys();
                    }
                    return;
                }
                if code == KeyCode::F3 && pressed {
                    self.debug_overlay = !self.debug_overlay;
                    return;
                }
                if code == KeyCode::F4 && pressed {
                    self.cull_mode = self.cull_mode.next();
                    return;
                }
                let grabbed = self.grabbed;
                let gameplay_input_active = self.gameplay_input_active();
                match &mut self.mode {
                    Mode::Menu { .. } => {}
                    Mode::Fly(_) => match code {
                        KeyCode::KeyW => self.input.forward = pressed,
                        KeyCode::KeyS => self.input.back = pressed,
                        KeyCode::KeyA => self.input.left = pressed,
                        KeyCode::KeyD => self.input.right = pressed,
                        KeyCode::Space => self.input.up = pressed,
                        KeyCode::ControlLeft => self.input.down = pressed,
                        KeyCode::ShiftLeft => self.input.boost = pressed,
                        _ => {}
                    },
                    Mode::Online { net, input, .. } => match code {
                        // The server never pushes scores: send `score` on the
                        // down edge, and every 2 s while held (see the redraw
                        // tick; docs/research/cod11-hud-protocol.md, section 4).
                        KeyCode::Tab if gameplay_input_active => {
                            if let Some(hud) = &mut self.hud {
                                hud.scoreboard.visible = pressed;
                                if pressed {
                                    let now = (Instant::now() - self.start).as_secs_f32();
                                    net.send_reliable("score");
                                    hud.scoreboard.mark_requested(now);
                                }
                            }
                        }
                        // A press counts only while the mouse is captured and
                        // no menu owns input; a release always passes so nothing
                        // stays held.
                        _ => {
                            if let Some(action) = play_action(code) {
                                if (gameplay_input_active && grabbed) || !pressed {
                                    input.key(action, pressed);
                                }
                            }
                        }
                    },
                    Mode::Walk {
                        input,
                        keys,
                        reload_edge,
                        switch_to,
                        ..
                    } => match code {
                        // weapon actions count only while the mouse is captured
                        KeyCode::KeyR if pressed && grabbed => *reload_edge = true,
                        KeyCode::KeyW => keys.w = pressed,
                        KeyCode::KeyS => keys.s = pressed,
                        KeyCode::KeyA => keys.a = pressed,
                        KeyCode::KeyD => keys.d = pressed,
                        KeyCode::Space => input.jump = pressed,
                        KeyCode::ControlLeft => input.crouch = pressed,
                        KeyCode::KeyZ if pressed => input.prone = !input.prone,
                        KeyCode::KeyQ => input.lean_left = pressed,
                        KeyCode::KeyE => input.lean_right = pressed,
                        KeyCode::ShiftLeft | KeyCode::ShiftRight => input.sprint = pressed,
                        _ => {
                            if pressed && grabbed {
                                if let Some(slot) = digit_slot(code) {
                                    *switch_to = Some(slot);
                                }
                            }
                        }
                    },
                }
            }
            WindowEvent::Focused(false) => {
                self.set_grab(false);
                self.clear_held_keys();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = (position.x as f32, position.y as f32);
                self.screen_hover(self.cursor_pos.0, self.cursor_pos.1);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                use winit::event::MouseButton;
                let pressed = state == ElementState::Pressed;
                if self.screen_captures_input() {
                    if button == MouseButton::Left && pressed {
                        self.screen_click();
                        if self.quit_requested {
                            event_loop.exit();
                        }
                    }
                    return;
                }
                // the click that captures the mouse must not also fire
                if button == MouseButton::Left && pressed && !self.grabbed {
                    if self.gameplay_input_active() {
                        self.set_grab(true);
                    }
                    return;
                }
                if !self.grabbed {
                    return;
                }
                match &mut self.mode {
                    Mode::Menu { .. } => {}
                    Mode::Walk {
                        fire_edge,
                        fire_held,
                        ads_held,
                        ..
                    } => match button {
                        MouseButton::Left if pressed => {
                            *fire_edge = true;
                            *fire_held = true;
                        }
                        MouseButton::Left => *fire_held = false,
                        MouseButton::Right => *ads_held = pressed,
                        _ => {}
                    },
                    Mode::Online { input, .. } => match button {
                        MouseButton::Left => input.key(play::input::Action::Attack, pressed),
                        MouseButton::Right => input.key(play::input::Action::Ads, pressed),
                        _ => {}
                    },
                    Mode::Fly(_) => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 60.0,
                };
                if self.screen_wheel(scroll) {
                    return;
                }
                let grabbed = self.grabbed;
                match &mut self.mode {
                    Mode::Fly(cam) => cam.adjust_speed(scroll),
                    // Retail binds MWHEELDOWN to weapnext, MWHEELUP to weapprev.
                    Mode::Online { input, modal, .. }
                        if grabbed && modal.is_none() && scroll != 0.0 =>
                    {
                        let action = if scroll < 0.0 {
                            play::input::Action::NextWeapon
                        } else {
                            play::input::Action::PrevWeapon
                        };
                        input.key(action, true);
                        input.key(action, false);
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;
                let elapsed = now - self.start;
                let time = elapsed.as_secs_f32();
                let local_ms = elapsed.as_secs_f64() * 1000.0;
                let cull = self.cull_mode;
                let Some(r) = &mut self.renderer else { return };
                let aspect = r.aspect();
                // A local match whose server exited on its own (a bad map, a
                // port clash) is reported here rather than left as a silent
                // connection failure; the net client then reports the drop.
                let server_died = self.local_server.as_mut().is_some_and(|s| !s.is_running());
                if server_died {
                    if let Some(s) = &self.local_server {
                        log::error!(
                            "menu: the local server on {} exited; check its log",
                            s.addr()
                        );
                    }
                    self.local_server = None;
                }
                // Set inside the online arm where `self` is borrowed out
                // field-by-field; acted on once the borrows end.
                let mut fatal: Option<anyhow::Error> = None;
                let (mut frame, vm) = match &mut self.mode {
                    Mode::Menu {
                        screens,
                        screen_menu,
                    } => {
                        // Menu-only: nothing is drawn but the menu over an
                        // empty view. No world, no server, no net client.
                        let _ = screens;
                        if let Some(hud) = &mut self.hud {
                            let (screen_w, screen_h) = r.screen_size();
                            let no_clients = BTreeMap::new();
                            let modal = screen_menu.as_ref().map(hud::modal::ModalRef::Shell);
                            let f = hud::HudFrame {
                                modal,
                                vote: None,
                                registry: None,
                                now: time,
                                screen_w,
                                screen_h,
                                configstrings: &[],
                                clients: &no_clients,
                                protocol: &net::protocol::PROTOCOL_V1,
                                server_time: 0,
                                snap_time: 0,
                                fs: &self.fs,
                                ps: None,
                                predicted: None,
                                local_player: false,
                                weapons: &[],
                                localized: &self.localized,
                                view_yaw: 0.0,
                                eye: [0.0; 3],
                                fov: camera::DEFAULT_FOV_DEG,
                                entity_origin: &|_| None,
                            };
                            let quads = hud.build(&f);
                            r.set_hud_quads(&self.fs, quads);
                        }
                        (
                            renderer::Frame {
                                // Nothing is drawn, so any projection works.
                                view_proj: camera::view_proj_from(
                                    Vec3::ZERO,
                                    0.0,
                                    0.0,
                                    0.0,
                                    camera::DEFAULT_FOV_DEG,
                                    aspect,
                                ),
                                eye: Vec3::ZERO,
                                fwd: camera::basis(0.0, 0.0).0,
                                time,
                                cull,
                                hud_lines: Vec::new(),
                            },
                            None,
                        )
                    }
                    Mode::Fly(cam) => {
                        cam.update(&self.input, dt);
                        let (cam_forward, cam_right, cam_up) = camera::basis(cam.yaw, cam.pitch);
                        // u32::MAX: no entity pinned to the camera.
                        self.audio
                            .set_listener(cam.pos, cam_forward, cam_right, cam_up, u32::MAX);
                        // no CollisionWorld in fly mode, so usePhysics fx don't collide
                        let fx_t0 = Instant::now();
                        self.fx.step(dt, time, None);
                        self.audio.step(&HashMap::new(), None);
                        r.set_fx_quads(
                            &self.fs,
                            self.fx.build_quads(cam.pos, cam_right, cam_up, time),
                        );
                        r.set_fx_lights(&self.fx.lights(cam.pos, time));
                        self.fx_ms = fx_t0.elapsed().as_secs_f32() * 1000.0;
                        (
                            renderer::Frame {
                                view_proj: cam.view_proj(aspect),
                                eye: cam.pos,
                                fwd: cam_forward,
                                time,
                                cull,
                                hud_lines: Vec::new(),
                            },
                            None,
                        )
                    }
                    Mode::Online {
                        net,
                        cam,
                        input,
                        clock: cmd_clock,
                        ring,
                        predictor,
                        view,
                        phase,
                        join,
                        modal,
                        screens,
                        profile,
                        vote: vote_panel,
                    } => {
                        let mut vm = None;
                        let events = net.pump();
                        let mut gamestate_ready = false;
                        for ev in &events {
                            if let Some(hud) = &mut self.hud {
                                hud.on_net_event(ev, time);
                            }
                            match ev {
                                net::NetEvent::Chat { text, .. } => {
                                    println!("{}", net::strip_colors(text))
                                }
                                net::NetEvent::Print(s) => println!("{s}"),
                                net::NetEvent::Dropped(why) => {
                                    fatal = Some(anyhow!("disconnected: {why}"))
                                }
                                // Map ambient; each round restart re-sends it
                                // with a new fade deadline, which is ignored.
                                net::NetEvent::ConfigstringChanged(3) => {
                                    self.audio.set_ambient(
                                        &self.fs,
                                        net::info_value_for_key(net.configstring(3), "n"),
                                    );
                                }
                                net::NetEvent::ConfigstringChanged(7) => {
                                    if let Phase::Live(live) = phase {
                                        live.weapons = vcod_common::weapon_table::from_configstring(
                                            &self.fs,
                                            net.configstring(7),
                                        );
                                    }
                                }
                                // `j/k/l` is quick chat; `s <idx>` is the announcer.
                                net::NetEvent::ServerCommand(ref tokens) => {
                                    // The server's profile sync. Claimed here so
                                    // it never reaches the join state machine,
                                    // which would only log it as unknown.
                                    if profile.on_command(tokens) {
                                        if let Some(p) = profile.profile() {
                                            screens.set_classes(Some(&p));
                                            if matches!(modal, Some(hud::modal::Modal::Shell(_))) {
                                                *modal =
                                                    screens.menu().map(hud::modal::Modal::Shell);
                                            }
                                        }
                                        continue;
                                    }
                                    if tokens.first().is_some_and(|t| t == "n") {
                                        join.on_restart();
                                    }
                                    // The end-of-match map vote, before the
                                    // join machine so its `mapvote` command is
                                    // never logged as unknown.
                                    if vote_panel.on_server_command(tokens) {
                                        continue;
                                    }
                                    for cmd in join.on_server_command(
                                        tokens,
                                        net.configstrings(),
                                        net.server_id(),
                                    ) {
                                        net.send_reliable(&cmd);
                                    }
                                    let newest = net.snapshots().newest();
                                    let protocol = &net::protocol::PROTOCOL_V1;
                                    let quick_chat = self.quick_chat.on_server_command(
                                        &self.fs,
                                        tokens,
                                        |num| {
                                            newest
                                                .and_then(|s| s.clients.get(&num))
                                                .map(|c| c.field_i32(protocol, "team"))
                                        },
                                    );
                                    if !quick_chat {
                                        self.audio.on_server_command(
                                            &self.fs,
                                            tokens,
                                            net.configstrings(),
                                        );
                                    }
                                }
                                net::NetEvent::GamestateReady => {
                                    join.on_gamestate();
                                    profile.reset();
                                    screens.set_classes(None);
                                    screens.close();
                                    *modal = None;
                                    *vote_panel = hud::vote::VotePanel::default();
                                    gamestate_ready = true;
                                }
                                _ => {}
                            }
                        }

                        let had_modal = modal.is_some();
                        // Script menus may own the presentation only during a
                        // live map and only while no client-owned modal is up.
                        // Join retains the server request while a shell/editor
                        // temporarily replaces its visual owner.
                        if matches!(phase, Phase::Live(_))
                            && (modal.is_none()
                                || matches!(modal, Some(hud::modal::Modal::Script { .. })))
                        {
                            let mut drawn = match modal.take() {
                                Some(hud::modal::Modal::Script { name, view }) => {
                                    Some((name, view))
                                }
                                None => None,
                                Some(other) => {
                                    *modal = Some(other);
                                    None
                                }
                            };
                            if modal.is_none() {
                                hud::menu::sync(
                                    &mut drawn,
                                    join,
                                    &mut self.menus,
                                    &self.fs,
                                    &self.localized,
                                    net.configstrings(),
                                );
                                *modal = drawn
                                    .map(|(name, view)| hud::modal::Modal::Script { name, view });
                                if modal.is_some() {
                                    screens.close();
                                }
                            }
                        }
                        if !had_modal && modal.is_some() {
                            input.release_all();
                            if let Some(hud) = &mut self.hud {
                                hud.scoreboard.visible = false;
                            }
                            if let Some(window) = &self.window {
                                let _ = window.set_cursor_grab(CursorGrabMode::None);
                                window.set_cursor_visible(true);
                            }
                            self.grabbed = false;
                        } else if had_modal && modal.is_none() {
                            let gameplay_owns_mouse = net.snapshots().newest().is_some_and(|snap| {
                                let pm_type = snap
                                    .ps
                                    .field_i32(&net::protocol::PROTOCOL_V1, "pm_type");
                                pmove::predict::predictable(pm_type)
                            });
                            if gameplay_owns_mouse {
                                if let Some(window) = &self.window {
                                    let grabbed = window
                                        .set_cursor_grab(CursorGrabMode::Locked)
                                        .or_else(|_| {
                                            window.set_cursor_grab(CursorGrabMode::Confined)
                                        })
                                        .is_ok();
                                    if grabbed {
                                        window.set_cursor_visible(false);
                                        self.grabbed = true;
                                    }
                                }
                            }
                        }

                        // One quick-chat line per second, played and shown
                        // like a chat line (retail queues text + alias).
                        if let Some(line) = self.quick_chat.drain(time) {
                            let name = net
                                .snapshots()
                                .newest()
                                .and_then(|s| s.clients.get(&line.client_num))
                                .map(|c| c.name(&net::protocol::PROTOCOL_V1))
                                .unwrap_or_else(|| format!("player {}", line.client_num));
                            println!("{}", net::strip_colors(&format!("{name}: {}", line.text)));
                            if let Some(hud) = &mut self.hud {
                                hud.chat
                                    .push(&format!("{name}: {}", line.text), false, time);
                            }
                            self.audio.play(&self.fs, line.cue);
                        }

                        // Re-send `score` every 2 s while Tab is held, as the
                        // stock client does (cod11-hud-protocol.md, section 4).
                        if let Some(hud) = &mut self.hud {
                            if hud.scoreboard.due(time) {
                                net.send_reliable("score");
                                hud.scoreboard.mark_requested(time);
                            }
                        }

                        if gamestate_ready {
                            cmd_clock.reset();
                            ring.clear();
                            predictor.reset();
                        }
                        // Only with the map up: the first cmd after a gamestate
                        // is what enters the client into the world
                        // (docs/protocol-1.1.md, "Entering the world").
                        if matches!(phase, Phase::Live(_))
                            && !gamestate_ready
                            && modal.is_none()
                            && net.state() == net::NetState::Active
                        {
                            let times = cmd_clock.due(net.server_clock_ms());
                            if !times.is_empty() {
                                let held = net.snapshots().newest().map_or_else(
                                    play::input::Held::default,
                                    |s| {
                                        play::input::Held::from_ps(
                                            &s.ps,
                                            &net::protocol::PROTOCOL_V1,
                                        )
                                    },
                                );
                                let new: Vec<_> =
                                    times.iter().map(|&t| input.build(t, &held)).collect();
                                net.send_cmds(&ring.packet(&new));
                            }
                        }

                        let progress = net.download_progress();
                        let frame = match phase {
                            Phase::Connecting { since } => {
                                if gamestate_ready {
                                    match start_loading(
                                        net,
                                        &mut self.hud,
                                        &mut self.audio,
                                        &mut self.fx,
                                        &mut self.world,
                                        r,
                                        self.window.as_deref(),
                                        &mut self.title,
                                        &self.game_dir,
                                        &self.mod_dir,
                                    ) {
                                        Ok(next) => *phase = next,
                                        Err(e) => fatal = Some(e),
                                    }
                                } else if since.elapsed() > Duration::from_secs(10) {
                                    fatal = Some(anyhow!("no gamestate from the server in 10 s"));
                                }
                                loading_frame(
                                    r,
                                    &self.fs,
                                    &self.localized,
                                    &mut self.hud,
                                    time,
                                    aspect,
                                    cull,
                                    net.configstrings(),
                                    format!(
                                        "Connecting to {}...",
                                        self.connect_addr.as_deref().unwrap_or("?")
                                    ),
                                )
                            }
                            Phase::Loading { loader } => {
                                let map = loader.map().to_string();
                                let map_resolves = map_available_locally(&self.fs, &map);
                                let mut waited = None;
                                // A Ready or Failed loader is never stepped again:
                                // Ready swaps the phase, Failed exits.
                                match loader.step(&events, progress, map_resolves, now) {
                                    loading::Action::BeginDownload { remote, dest } => {
                                        if let Err(e) = net.begin_download(&remote, &dest) {
                                            fatal = Some(e.context("cannot start the download"));
                                        }
                                    }
                                    loading::Action::Reopen => {
                                        match reopen_fs(
                                            &self.game_dir.join(&self.mod_dir),
                                            &mut self.fs,
                                            &mut self.localized,
                                            &mut self.menus,
                                            &mut self.hud,
                                            &mut self.audio,
                                            &mut self.fx,
                                            &mut self.quick_chat,
                                        ) {
                                            Ok(()) => {
                                                *modal = None;
                                                // The new paks may carry maps the
                                                // old ones lacked; the selector
                                                // must offer them.
                                                screens.set_registry(maplist_for(&self.fs));
                                                r.reopen(&self.fs);
                                                view.reopen();
                                            }
                                            Err(e) => fatal = Some(e),
                                        }
                                    }
                                    loading::Action::FinishDownloads => net.finish_downloads(),
                                    loading::Action::Ready => {
                                        match load_map(
                                            &map,
                                            net,
                                            &mut self.fs,
                                            &mut self.audio,
                                            &mut self.world,
                                            r,
                                            self.window.as_deref(),
                                            &mut self.title,
                                        ) {
                                            Ok(next) => *phase = next,
                                            Err(e) => fatal = Some(e),
                                        }
                                    }
                                    loading::Action::Failed(msg) => fatal = Some(anyhow!(msg)),
                                    loading::Action::Wait(p) => waited = p,
                                }
                                loading_frame(
                                    r,
                                    &self.fs,
                                    &self.localized,
                                    &mut self.hud,
                                    time,
                                    aspect,
                                    cull,
                                    net.configstrings(),
                                    loading_text(&map, waited),
                                )
                            }
                            Phase::Live(live) => {
                                if gamestate_ready {
                                    match start_loading(
                                        net,
                                        &mut self.hud,
                                        &mut self.audio,
                                        &mut self.fx,
                                        &mut self.world,
                                        r,
                                        self.window.as_deref(),
                                        &mut self.title,
                                        &self.game_dir,
                                        &self.mod_dir,
                                    ) {
                                        Ok(next) => *phase = next,
                                        Err(e) => fatal = Some(e),
                                    }
                                    let map = match phase {
                                        Phase::Loading { loader } => loader.map().to_string(),
                                        _ => String::new(),
                                    };
                                    loading_frame(
                                        r,
                                        &self.fs,
                                        &self.localized,
                                        &mut self.hud,
                                        time,
                                        aspect,
                                        cull,
                                        net.configstrings(),
                                        format!("Loading {map}"),
                                    )
                                } else {
                                    let LivePhase {
                                        world,
                                        weapons,
                                        scene,
                                        events,
                                        predicted_events,
                                        clock,
                                        last_loop_snap,
                                        drawn_pos,
                                    } = &mut **live;
                                    let bsp =
                                        &self.world.as_ref().expect("live phase has a map").bsp;
                                    let p = &net::protocol::PROTOCOL_V1;
                                    let client_num = net.gamestate().map_or(-1, |g| g.client_num);
                                    // While following, ps.clientNum is the followed
                                    // client's (docs/research/cod11-events-and-fx.md,
                                    // section 7); the camera sits inside that body, so
                                    // skip it instead of ours.
                                    let ps_client = net
                                        .snapshots()
                                        .newest()
                                        .map_or(-1, |s| s.ps.field_i32(p, "clientNum"));
                                    let following = ps_client >= 0 && ps_client != client_num;
                                    // Intermission (5) and dead (6, 7) take no view
                                    // from the cmd (Q3 `PM_UpdateViewAngles`), so
                                    // those draw the snapshot's view, as following does.
                                    let pm_type = net
                                        .snapshots()
                                        .newest()
                                        .map_or(0, |s| s.ps.field_i32(p, "pm_type"));
                                    let snapshot_view = following || (5..=7).contains(&pm_type);
                                    let skip_num = if following { ps_client } else { client_num };
                                    // Rebuilt every frame so absent entities (PVS churn) drop out.
                                    let mut instances: Vec<DynamicModelInstance> = Vec::new();
                                    // Per-shooter muzzle transforms for flashes and tracers.
                                    let mut muzzles: HashMap<u32, (Vec3, Vec3)> = HashMap::new();
                                    let mut weapon_flash: HashMap<i32, String> = HashMap::new();
                                    let mut entity_pos: HashMap<u32, Vec3> = HashMap::new();

                                    let render_time = net
                                        .snapshots()
                                        .newest()
                                        .map(|s| clock.render_time(local_ms, s.server_time));
                                    if let Some(render_time) = render_time {
                                        if let Some((a, b)) =
                                            net.snapshots().two_for_time(render_time)
                                        {
                                            // No straddling pair: one frame, held.
                                            if std::ptr::eq(a, b) {
                                                self.interp_misses += 1;
                                            }
                                            let oa = Vec3::from(a.ps.origin(p));
                                            let ob = Vec3::from(b.ps.origin(p));
                                            let f = ((render_time - a.server_time) as f32
                                                / (b.server_time - a.server_time).max(1) as f32)
                                                .clamp(0.0, 1.0);
                                            let t0 = Instant::now();
                                            let built = entities::build_instances(
                                                scene,
                                                (a, b, f),
                                                render_time,
                                                skip_num,
                                                net.configstrings(),
                                                &self.fs,
                                                bsp,
                                                r,
                                                p,
                                            );
                                            self.build_ms = t0.elapsed().as_secs_f32() * 1000.0;
                                            instances = built.instances;
                                            muzzles = built.muzzles;
                                            weapon_flash = built.weapon_flash;
                                            entity_pos = built.entity_pos;
                                            // Over 512 u is a teleport, not motion.
                                            let pos = if oa.distance(ob) > 512.0 {
                                                ob
                                            } else {
                                                oa.lerp(ob, f)
                                            };
                                            // viewHeightCurrent: eye offset above the feet origin.
                                            let vh = a.ps.field_f32(p, "viewHeightCurrent")
                                                * (1.0 - f)
                                                + b.ps.field_f32(p, "viewHeightCurrent") * f;
                                            cam.pos = pos + Vec3::Z * vh;
                                            if snapshot_view {
                                                let va_a = a.ps.viewangles(p);
                                                let va_b = b.ps.viewangles(p);
                                                cam.yaw = camera::lerp_angle(va_a[1], va_b[1], f)
                                                    .to_radians();
                                                cam.pitch =
                                                    -camera::lerp_angle(va_a[0], va_b[0], f)
                                                        .to_radians();
                                            }
                                        }
                                    }
                                    let predicted = if ps_client == client_num {
                                        net.snapshots().newest().and_then(|s| {
                                            let bodies = play::predict::solid_bodies(
                                                p,
                                                &s.entities,
                                                client_num as u32,
                                                drawn_pos,
                                            );
                                            predictor.predict(
                                                p, &s.ps, ring, world, &bodies, weapons, local_ms,
                                            )
                                        })
                                    } else {
                                        predictor.reset();
                                        None
                                    };
                                    drawn_pos.clone_from(&entity_pos);
                                    if let Some(v) = &predicted {
                                        cam.pos = v.origin + Vec3::Z * v.view_height;
                                    }
                                    // Prediction only describes *our* client, so the
                                    // rig comes from the snapshot whenever it does
                                    // not: while following (the killcam) the
                                    // snapshot's `clientNum` is the followed
                                    // client, so its `weapon` and
                                    // `viewmodelIndex` are the killer's, and the
                                    // same goes for the dead and intermission
                                    // pm_types that take no view from the cmd.
                                    //
                                    // Gating the whole rig on `!following` left the
                                    // killcam with the follower's camera and no
                                    // weapon at all - their POV moved, but nothing
                                    // was ever asked to draw the gun in it.
                                    // A local client that is dead is not a
                                    // first-person view of anybody: the camera
                                    // is on the corpse, and the snapshot's ps is
                                    // still ours, so reading a rig from it would
                                    // hang our own gun in the death view. Only
                                    // following has a *someone else's* ps to
                                    // read, and that is the killcam.
                                    let rig_ps = if following {
                                        net.snapshots()
                                            .newest()
                                            .map(|s| play::view::ViewPs::from_snapshot(p, &s.ps))
                                    } else if pmove::predict::predictable(pm_type) {
                                        net.snapshots().newest().map(|s| match &predicted {
                                            Some(v) => play::view::ViewPs::from_predicted(
                                                &v.pred,
                                                s.ps.field_i32(p, "viewmodelIndex"),
                                            ),
                                            None => play::view::ViewPs::from_snapshot(p, &s.ps),
                                        })
                                    } else {
                                        None
                                    };
                                    let view_ps = rig_ps;
                                    debug_assert!(
                                        !following || view_ps.is_some(),
                                        "the killcam has no playerstate to draw the killer's weapon from"
                                    );
                                    if let Some(ps) = &view_ps {
                                        if let Some(models) =
                                            view.sync_rig(&self.fs, net.configstrings(), ps)
                                        {
                                            r.set_viewmodel(&self.fs, &models);
                                            self.viewmodel = models;
                                        }
                                    }
                                    view.sprint(
                                        !following
                                            && ps_client == client_num
                                            && pmove::predict::predictable(pm_type)
                                            && input.is_down(play::input::Action::Sprint),
                                    );
                                    let (vm_draw, fov) =
                                        view.frame(weapons, view_ps.as_ref(), dt, local_ms);
                                    vm = vm_draw;
                                    if !snapshot_view {
                                        let delta = match &predicted {
                                            Some(v) => v.delta_angles,
                                            None => net.snapshots().newest().map_or([0; 3], |s| {
                                                net::DELTA_ANGLE_FIELDS
                                                    .map(|name| s.ps.field_i32(p, name))
                                            }),
                                        };
                                        (cam.yaw, cam.pitch) = own_view(input.raw_angles(), delta);
                                    }

                                    let (cam_forward, cam_right, cam_up) =
                                        camera::basis(cam.yaw, cam.pitch);
                                    // The ridden body is `skip_num`, never in `entity_pos`,
                                    // so the listener is told which entity rides the camera.
                                    self.audio.set_listener(
                                        cam.pos,
                                        cam_forward,
                                        cam_right,
                                        cam_up,
                                        audio::cues::ps_entity(ps_client),
                                    );

                                    let (muzzle_pos, muzzle_dir) = view
                                        .muzzle(cam.pos, (cam_forward, cam_right, cam_up))
                                        .unwrap_or_else(|| {
                                            view_muzzle(cam.pos, cam_forward, cam_right, cam_up)
                                        });
                                    muzzles.insert(u32::MAX, (muzzle_pos, muzzle_dir));
                                    // Bullet hits carry the shooter's number in
                                    // `other_entity_num`, and the body the camera rides
                                    // (ours, or the followed player's) is excluded from
                                    // the muzzle map, so key the view muzzle under it too
                                    // for the whizby; its tracer is `view_body`'s call.
                                    if let Ok(num) = u32::try_from(skip_num) {
                                        muzzles.insert(num, (muzzle_pos, muzzle_dir));
                                    }

                                    r.set_dynamic_models(&instances);

                                    // Step before this frame's events spawn, or the
                                    // [now-dt, now] integration would move particles born
                                    // at `now` (a tracer would start ahead of its muzzle).
                                    let fx_t0 = Instant::now();
                                    self.fx.step(dt, time, Some(&*world));

                                    let (screen_w, screen_h) = r.screen_size();

                                    let no_clients = BTreeMap::new();
                                    let newest = net.snapshots().newest();
                                    let local_player = ps_client == client_num
                                        && pmove::predict::predictable(pm_type);
                                    let entity_origin = |num: i32| {
                                        let num = u32::try_from(num).ok()?;
                                        entity_pos.get(&num).map(|v| v.to_array())
                                    };
                                    let hud_frame = hud::HudFrame {
                                        now: time,
                                        screen_w,
                                        screen_h,
                                        configstrings: net.configstrings(),
                                        clients: newest.map_or(&no_clients, |s| &s.clients),
                                        protocol: p,
                                        // The render clock, so hudelem tweens and
                                        // timers move between snapshots.
                                        server_time: render_time.unwrap_or(0),
                                        snap_time: render_time
                                            .and_then(|t| net.snapshots().two_for_time(t))
                                            .map_or(0, |(a, _)| a.server_time),
                                        fs: &self.fs,
                                        modal: modal.as_ref().map(hud::modal::Modal::as_ref),
                                        vote: vote_panel.is_open().then_some(vote_panel),
                                        registry: Some(screens.registry()),
                                        ps: newest.map(|s| &s.ps),
                                        predicted: predicted
                                            .as_ref()
                                            .filter(|_| local_player)
                                            .map(|v| &v.pred),
                                        local_player,
                                        weapons,
                                        localized: &self.localized,
                                        view_yaw: cam.yaw.to_degrees(),
                                        eye: cam.pos.to_array(),
                                        fov,
                                        entity_origin: &entity_origin,
                                    };

                                    // Events use the newest snapshot, not the interpolation
                                    // pair; they must not wait out the interp delay.
                                    if let Some(newest) = newest {
                                        let ctx = fx::registry::ResolveCtx {
                                            muzzles: &muzzles,
                                            weapon_flash: &weapon_flash,
                                            view_flash: vm.is_some().then_some(weapons.as_slice()),
                                            view_body: fx::registry::view_body(
                                                newest.ps.field_i32(p, "pm_flags"),
                                                ps_client,
                                            ),
                                        };
                                        // Our own ring plays off the prediction, and
                                        // the snapshot's copy of it is skipped. The
                                        // frame we die or reach the intermission still
                                        // skips the copies, then tracking stops.
                                        let own = ps_client == client_num;
                                        let predicting =
                                            own && pmove::predict::predictable(pm_type);
                                        let mut evs = Vec::new();
                                        if !own {
                                            predicted_events.stop();
                                        } else if let Some(v) =
                                            predicted.as_ref().filter(|_| predicting)
                                        {
                                            predicted_events.start_after(events.ps_sequence());
                                            let pred = &v.pred;
                                            evs.extend(
                                                predicted_events
                                                    .take_predicted(
                                                        pred.ring.seq,
                                                        pred.ring.events,
                                                        pred.ring.parms,
                                                    )
                                                    .into_iter()
                                                    .map(|(_, event, parm)| {
                                                        play::events::game_event(
                                                            pred, ps_client, event, parm,
                                                        )
                                                    }),
                                            );
                                        }
                                        for (seq, ev) in events.drain_seq(newest, p) {
                                            if own
                                                && seq.is_some_and(|s| {
                                                    !predicted_events.filter_snapshot(s, ev.event)
                                                })
                                            {
                                                continue;
                                            }
                                            evs.push(ev);
                                        }
                                        if !predicting {
                                            predicted_events.stop();
                                        }
                                        for ev in evs {
                                            self.ev_seen += 1;
                                            if let Some(hud) = &mut self.hud {
                                                hud.on_game_event(&ev, &hud_frame);
                                            }
                                            self.audio.on_game_event(
                                                &self.fs,
                                                &ev,
                                                net.configstrings(),
                                                &muzzles,
                                            );
                                            for r in fx::registry::resolve(&ev, &ctx) {
                                                match r {
                                                    fx::registry::Resolved::Spawn { path, at } => {
                                                        let sounds = self
                                                            .fx
                                                            .spawn(&self.fs, &path, at, time);
                                                        self.audio.play_fx(&self.fs, sounds);
                                                    }
                                                    fx::registry::Resolved::Tracer {
                                                        muzzle,
                                                        impact,
                                                        flesh,
                                                    } => {
                                                        self.fx.spawn_tracer(
                                                            muzzle, impact, flesh, time, dt,
                                                        );
                                                    }
                                                    fx::registry::Resolved::Known => {}
                                                    fx::registry::Resolved::Unknown => {
                                                        self.ev_unknown += 1;
                                                        log::debug!(
                                                            "unknown event {} parm {}",
                                                            ev.event,
                                                            ev.parm
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // `es.loopSound` (docs/research/cod11-sound-system.md,
                                    // section 9), reconciled once per snapshot. An entity
                                    // that left the snapshot is absent from the map, which
                                    // is what stops its loop.
                                    if let Some(newest) = newest {
                                        if *last_loop_snap != Some(newest.message_num) {
                                            *last_loop_snap = Some(newest.message_num);
                                            let loops: HashMap<u32, (i32, Vec3)> = newest
                                                .entities
                                                .iter()
                                                .filter_map(|(&num, e)| {
                                                    let idx = e.field_i32(p, "loopSound");
                                                    (idx != 0).then(|| {
                                                        (num, (idx, Vec3::from(e.origin(p))))
                                                    })
                                                })
                                                .collect();
                                            self.audio.set_loop_sounds(
                                                &self.fs,
                                                net.configstrings(),
                                                &loops,
                                            );
                                        }
                                    }

                                    // After the drain, so new voices get this frame's positions.
                                    self.audio.step(&entity_pos, Some(&*world));

                                    r.set_fx_quads(
                                        &self.fs,
                                        self.fx.build_quads(cam.pos, cam_right, cam_up, time),
                                    );
                                    r.set_fx_lights(&self.fx.lights(cam.pos, time));
                                    r.set_fog(
                                        net::FogParams::parse(
                                            net.configstring(net::protocol::CS_FOG_V1),
                                        ),
                                        time,
                                    );
                                    self.fx_ms = fx_t0.elapsed().as_secs_f32() * 1000.0;

                                    if let Some(hud) = &mut self.hud {
                                        let hud_t0 = Instant::now();
                                        let quads = hud.build(&hud_frame);
                                        r.set_hud_quads(&self.fs, quads);
                                        self.hud_ms = hud_t0.elapsed().as_secs_f32() * 1000.0;
                                    }

                                    renderer::Frame {
                                        view_proj: camera::view_proj_from(
                                            cam.pos, cam.yaw, cam.pitch, 0.0, fov, aspect,
                                        ),
                                        eye: cam.pos,
                                        fwd: cam_forward,
                                        time,
                                        cull,
                                        hud_lines: Vec::new(),
                                    }
                                }
                            }
                        };
                        (frame, vm)
                    }
                    Mode::Walk {
                        world,
                        ps,
                        input,
                        keys,
                        motion,
                        view_weapon,
                        configstrings,
                        weapon_slot,
                        reserve,
                        switch_to,
                        mouse_delta,
                        fire_edge,
                        fire_held,
                        reload_edge,
                        ads_held,
                    } => {
                        // Before this frame's fire event spawns, or the
                        // [now-dt, now] integration would move the new tracer
                        // ahead of its muzzle.
                        let fx_t0 = Instant::now();
                        self.fx.step(dt, time, Some(&*world));

                        if let Some(slot) = switch_to.take() {
                            if slot != *weapon_slot && slot < WALK_LOADOUT.len() {
                                let name = WALK_LOADOUT[slot];
                                match viewmodel::load_view_weapon(&self.fs, name) {
                                    Some((models, vw)) => {
                                        r.set_viewmodel(&self.fs, &models);
                                        *reserve = vw.as_ref().map_or(0, |w| w.def.start_ammo);
                                        self.viewmodel = models;
                                        *view_weapon = vw;
                                        *weapon_slot = slot;
                                    }
                                    None => log::warn!(
                                        "weapon {name} failed to load; keeping {}",
                                        WALK_LOADOUT[*weapon_slot]
                                    ),
                                }
                            }
                        }

                        (input.forward, input.right) = keys.axes();
                        let mw = MoveWorld::bare(world);
                        for ev in pmove::pmove(ps, input, &mw, dt, &[]) {
                            self.audio.on_game_event(
                                &self.fs,
                                &net::events::GameEvent {
                                    event: ev.event,
                                    parm: 0,
                                    // playerState-ring form: no entity, rides the listener
                                    entity_num: u32::MAX,
                                    client_num: -1,
                                    weapon: 0,
                                    surf_type: 0,
                                    pos: ps.origin.to_array(),
                                    dir: [0.0; 3],
                                    other_entity_num: u32::MAX,
                                    attacker_entity_num: -1,
                                },
                                configstrings,
                                &HashMap::new(),
                            );
                        }
                        input.jump = false; // Q3: a held jump doesn't autohop
                        let ground_speed = if ps.on_ground {
                            ps.velocity.truncate().length()
                        } else {
                            0.0
                        };

                        let v = ps.view();
                        let (eye_forward, eye_right, eye_up) = camera::basis(v.yaw, v.pitch);
                        self.audio
                            .set_listener(v.eye, eye_forward, eye_right, eye_up, u32::MAX);
                        let mut bone_sets = Vec::new();
                        let mut fov = camera::DEFAULT_FOV_DEG;
                        let mut damp = 1.0;
                        let mut sprint_anim = weapon::WeaponAnim::Idle;
                        if let Some(w) = view_weapon {
                            let out = w.state.update(
                                dt,
                                weapon::WeaponInput {
                                    fire: *fire_edge,
                                    fire_held: *fire_held,
                                    ads: *ads_held,
                                    reload: *reload_edge,
                                },
                            );
                            sprint_anim = out.anim;
                            w.pose_sight(out.ads_frac, *ads_held);
                            let clip = w
                                .anims
                                .get(&out.anim)
                                .or_else(|| w.anims.get(&weapon::WeaponAnim::Idle));
                            if let Some((anim, binding)) = clip {
                                let frame = anim.frame_pos(out.anim_time, out.looping);
                                w.pose.apply(anim, binding, frame);
                                // two models, hands then gun, in set_viewmodel order
                                bone_sets = (0..2)
                                    .map(|m| w.pose.skin_matrices(&w.skeleton, m))
                                    .collect();
                            }
                            fov = weapon::view_fov_x(
                                Some(&w.def),
                                out.ads_frac,
                                *ads_held,
                                false,
                                false,
                            );
                            damp = 1.0 + (w.def.ads_view_bob_mult - 1.0) * out.ads_frac;
                            damp *= 1.0 + (w.def.ads_bob_factor - 1.0) * out.ads_frac;
                            if let Some(cue) = out.cue {
                                if matches!(
                                    cue,
                                    weapon::WeaponCue::Reload | weapon::WeaponCue::ReloadFromEmpty
                                ) {
                                    let need = w.def.clip_size.saturating_sub(w.state.ammo());
                                    *reserve -= need.min(*reserve);
                                }
                                self.audio.on_game_event(
                                    &self.fs,
                                    &net::events::GameEvent {
                                        event: match cue {
                                            weapon::WeaponCue::Fire => fx::registry::EV_FIRE_WEAPON,
                                            weapon::WeaponCue::LastShot => {
                                                fx::registry::EV_FIRE_WEAPON_LASTSHOT
                                            }
                                            weapon::WeaponCue::Rechamber => {
                                                fx::registry::EV_RECHAMBER_WEAPON
                                            }
                                            weapon::WeaponCue::Reload => fx::registry::EV_RELOAD,
                                            weapon::WeaponCue::ReloadFromEmpty => {
                                                fx::registry::EV_RELOAD_FROM_EMPTY
                                            }
                                            weapon::WeaponCue::Raise => {
                                                fx::registry::EV_RAISE_WEAPON
                                            }
                                        },
                                        parm: 0,
                                        entity_num: u32::MAX,
                                        client_num: -1,
                                        // 1-based CS7 index of the active loadout slot
                                        weapon: *weapon_slot as i32 + 1,
                                        surf_type: 0,
                                        pos: ps.origin.to_array(),
                                        dir: [0.0; 3],
                                        other_entity_num: u32::MAX,
                                        attacker_entity_num: -1,
                                    },
                                    configstrings,
                                    &HashMap::new(),
                                );
                            }
                            if out.fired {
                                /// Q3/CoD bullet trace length.
                                const FIRE_RANGE: f32 = 8192.0;
                                // a point trace against solids only: playerclip-only
                                // geometry stops movement but not bullets
                                let tr = world.shot_trace(v.eye, v.eye + eye_forward * FIRE_RANGE);
                                // A muzzle inside solid returns no normal, which
                                // would build a NaN quad that never leaves the ring.
                                if tr.fraction < 1.0 && !tr.startsolid {
                                    // Same csv resolution as a server-driven hit:
                                    // surfType rides the surface flags' bits 20-24.
                                    let mut muzzles = HashMap::new();
                                    muzzles.insert(
                                        u32::MAX,
                                        (v.eye + eye_forward * 16.0 - Vec3::Z * 2.0, eye_forward),
                                    );
                                    let ctx = fx::registry::ResolveCtx {
                                        muzzles: &muzzles,
                                        weapon_flash: &HashMap::new(),
                                        view_flash: None,
                                        view_body: None,
                                    };
                                    let ev = net::events::GameEvent {
                                        event: fx::registry::EV_BULLET_HIT_SMALL,
                                        parm: net::events::dir_to_byte(tr.normal.to_array()),
                                        entity_num: u32::MAX,
                                        client_num: -1,
                                        weapon: *weapon_slot as i32 + 1,
                                        surf_type: collision::sound_material(tr.surface_flags),
                                        pos: tr.endpos.to_array(),
                                        dir: [0.0; 3],
                                        other_entity_num: u32::MAX,
                                        attacker_entity_num: -1,
                                    };
                                    for r in fx::registry::resolve(&ev, &ctx) {
                                        match r {
                                            fx::registry::Resolved::Spawn { path, at } => {
                                                let sounds =
                                                    self.fx.spawn(&self.fs, &path, at, time);
                                                self.audio.play_fx(&self.fs, sounds);
                                            }
                                            fx::registry::Resolved::Tracer {
                                                muzzle,
                                                impact,
                                                flesh,
                                            } => {
                                                self.fx
                                                    .spawn_tracer(muzzle, impact, flesh, time, dt);
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        *fire_edge = false;
                        *reload_edge = false;

                        motion.update_sprint(
                            dt,
                            viewmodel::SprintPoseInput {
                                requested: input.sprint,
                                moving: ground_speed > 1.0,
                                on_ground: ps.on_ground,
                                ads_frac: if *ads_held { 1.0 } else { 0.0 },
                                weapon_anim: sprint_anim,
                            },
                        );

                        motion.update(
                            dt,
                            ground_speed,
                            ps.on_ground,
                            mouse_delta.0,
                            mouse_delta.1,
                            damp,
                        );
                        *mouse_delta = (0.0, 0.0);
                        self.audio.step(&HashMap::new(), None);
                        r.set_fx_quads(
                            &self.fs,
                            self.fx.build_quads(v.eye, eye_right, eye_up, time),
                        );
                        r.set_fx_lights(&self.fx.lights(v.eye, time));
                        r.set_fog(
                            net::FogParams::parse(
                                configstrings
                                    .get(net::protocol::CS_FOG_V1)
                                    .map(String::as_str)
                                    .unwrap_or(""),
                            ),
                            time,
                        );
                        self.fx_ms = fx_t0.elapsed().as_secs_f32() * 1000.0;
                        (
                            renderer::Frame {
                                view_proj: camera::view_proj_from(
                                    v.eye, v.yaw, v.pitch, v.roll, fov, aspect,
                                ),
                                eye: v.eye,
                                fwd: camera::basis(v.yaw, v.pitch).0,
                                time,
                                cull,
                                hud_lines: vec![format!(
                                    "[{}] {} {} / {}",
                                    *weapon_slot + 1,
                                    WALK_LOADOUT[*weapon_slot],
                                    view_weapon.as_ref().map_or(0, |w| w.state.ammo()),
                                    reserve
                                )],
                            },
                            Some(renderer::VmDraw {
                                transform: motion.transform(),
                                fov_x: fov,
                                bone_sets,
                            }),
                        )
                    }
                };
                if let Some(err) = fatal {
                    self.fail(event_loop, err);
                    return;
                }
                // The scene lives in the online phase, so pull it back
                // out for the overlay after the mode's mutable borrows end.
                let scene = match &self.mode {
                    Mode::Online {
                        phase: Phase::Live(live),
                        ..
                    } => Some(&live.scene),
                    _ => None,
                };
                self.hud_stats.frame(
                    dt,
                    scene.map_or(0, |s| s.stats.anim_restarts),
                    self.interp_misses,
                );
                if self.debug_overlay {
                    frame.hud_lines = hud_lines(
                        &self.mode,
                        scene,
                        &self.hud_stats,
                        (self.build_ms, self.render_ms, self.fx_ms),
                        (self.ev_seen, self.ev_unknown),
                        self.fx.counts(),
                        (
                            r.hud_quad_count(),
                            self.hud_ms,
                            self.hud.as_ref().map_or(0, |h| h.unknown),
                        ),
                        self.audio.stats(),
                        r,
                    );
                }
                let t0 = Instant::now();
                r.render(frame, vm);
                self.render_ms = t0.elapsed().as_secs_f32() * 1000.0;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if !self.grabbed || self.screen_captures_input() {
                return;
            }
            let (dx, dy) = (dx as f32, dy as f32);
            match &mut self.mode {
                Mode::Menu { .. } => {}
                Mode::Fly(cam) => cam.mouse_delta(dx, dy),
                Mode::Online { input, view, .. } => {
                    input.mouse(dx, dy);
                    view.mouse(dx, dy);
                }
                Mode::Walk {
                    ps, mouse_delta, ..
                } => {
                    // same sensitivity and pitch clamp as FlyCamera::mouse_delta
                    const SENS: f32 = 0.003;
                    ps.yaw -= dx * SENS;
                    ps.pitch =
                        (ps.pitch - dy * SENS).clamp(-89.0f32.to_radians(), 89.0f32.to_radians());
                    // raw counts; the sway spring scales them
                    mouse_delta.0 += dx;
                    mouse_delta.1 += dy;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_menu_shortcuts_never_open_during_connect_load_or_another_modal() {
        assert_eq!(online_menu_shortcut(KeyCode::KeyM, false, false), None);
        assert_eq!(online_menu_shortcut(KeyCode::KeyB, false, false), None);
        assert_eq!(online_menu_shortcut(KeyCode::KeyM, true, true), None);
        assert_eq!(online_menu_shortcut(KeyCode::KeyB, true, true), None);
        assert_eq!(
            online_menu_shortcut(KeyCode::KeyM, true, false),
            Some(OnlineMenuShortcut::Pause)
        );
        assert_eq!(
            online_menu_shortcut(KeyCode::KeyB, true, false),
            Some(OnlineMenuShortcut::CreateAClass)
        );
        assert_eq!(
            script_modal_shortcut(KeyCode::KeyM),
            Some(OnlineMenuShortcut::Pause),
            "M must expose Change Class without closing the pre-spawn Join request"
        );
        assert_eq!(script_modal_shortcut(KeyCode::KeyB), None);
    }

    #[test]
    fn gameplay_input_requires_live_phase_without_a_modal_owner() {
        assert!(!online_gameplay_owns_input(false, false));
        assert!(!online_gameplay_owns_input(false, true));
        assert!(!online_gameplay_owns_input(true, true));
        assert!(online_gameplay_owns_input(true, false));
    }

    #[test]
    fn online_ui_has_exactly_one_modal_owner() {
        let shell =
            hud::shell::Menu::new("PAUSED", vec![hud::shell::Row::action("resume", "Resume")]);
        let mut modal = Some(hud::modal::Modal::Shell(shell));
        assert!(matches!(modal, Some(hud::modal::Modal::Shell(_))));

        let editor = hud::createaclass::CreateAClass::default();
        modal = Some(hud::modal::Modal::CreateAClass(editor));
        assert!(
            matches!(modal, Some(hud::modal::Modal::CreateAClass(_))),
            "opening another modal replaces rather than stacks owners"
        );
    }

    #[test]
    fn ordinary_no_map_launch_is_frontend_only() {
        assert_eq!(startup_kind(None, false), StartupKind::Frontend);
        let mode = frontend_mode(vcod_common::maplist::MapRegistry::from_ids(&[
            "mp_carentan",
        ]));
        match mode {
            Mode::Menu {
                screens,
                screen_menu,
            } => {
                assert_eq!(screens.current(), Some(hud::screens::Screen::Main));
                assert!(screen_menu.is_some());
            }
            _ => panic!("ordinary boot created gameplay state"),
        }
    }

    /// Leaving a match destroys the world and every per-match resource, so the
    /// frontend renders over an empty view. This is the lifecycle bug the QA
    /// recording showed: the menu drawn over a still-live map.
    #[test]
    fn leaving_a_match_destroys_the_world_and_returns_a_worldless_frontend() {
        let fs = Pk3Fs::empty();
        let mut world = Some(World {
            bsp: vcod_common::bsp::Bsp::empty(),
        });
        let mut fx = fx::sim::FxSystem::new();
        let mut hud: Option<hud::Hud> = None;
        let mut audio = audio::AudioSystem::new(
            &fs,
            audio::AudioOpts {
                enabled: false,
                volume: 1.0,
            },
        );

        let mode = teardown_to_frontend(&fs, &mut world, &mut fx, &mut hud, &mut audio, None);

        assert!(world.is_none(), "the collision world survived the teardown");
        match mode {
            Mode::Menu {
                screens,
                screen_menu,
            } => {
                assert_eq!(screens.current(), Some(hud::screens::Screen::Main));
                assert!(screen_menu.is_some(), "the frontend has no menu to draw");
            }
            _ => panic!("leaving a match did not return the frontend"),
        }
    }

    #[test]
    fn explicit_map_and_connect_still_select_gameplay_lifecycles() {
        assert_eq!(
            startup_kind(Some("mp_carentan"), false),
            StartupKind::LocalMap
        );
        assert_eq!(startup_kind(None, true), StartupKind::Online);
    }

    fn make_pk3(dir: &std::path::Path, file: &str, entries: &[(&str, &[u8])]) {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(std::fs::File::create(dir.join(file)).unwrap());
        for (name, content) in entries {
            z.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(content).unwrap();
        }
        z.finish().unwrap();
    }

    /// The state `reopen_fs` rebuilds, as it stands when a download lands:
    /// loaded off the old paks, the aliases filtered for the current map.
    struct Reopened {
        localized: vcod_common::localize::Localized,
        menus: hud::menu::MenuCache,
        hud: Option<hud::Hud>,
        audio: audio::AudioSystem,
        fx: fx::sim::FxSystem,
        quick_chat: quick_chat::QuickChat,
    }

    impl Reopened {
        fn load(fs: &Pk3Fs) -> Reopened {
            let mut audio = audio::AudioSystem::new(
                fs,
                audio::AudioOpts {
                    enabled: false,
                    volume: 1.0,
                },
            );
            audio.on_gamestate("mp_mod");
            Reopened {
                localized: vcod_common::localize::Localized::load(fs),
                menus: hud::menu::MenuCache::default(),
                hud: hud::Hud::new(fs).ok(),
                audio,
                fx: fx::sim::FxSystem::new(),
                quick_chat: quick_chat::QuickChat::new(1),
            }
        }

        fn reopen(&mut self, dir: &std::path::Path, fs: &mut Pk3Fs) {
            reopen_fs(
                dir,
                fs,
                &mut self.localized,
                &mut self.menus,
                &mut self.hud,
                &mut self.audio,
                &mut self.fx,
                &mut self.quick_chat,
            )
            .unwrap();
        }
    }

    /// A downloaded pak can carry the menus and strings the paks before it
    /// lacked; a miss cached against the old search path must not outlive it.
    #[test]
    fn reopen_reads_menus_and_strings_off_the_new_paks() {
        let dir = tempfile::tempdir().unwrap();
        make_pk3(dir.path(), "pak0.pk3", &[("readme.txt", b"")]);
        let mut fs = Pk3Fs::open(dir.path()).unwrap();
        let mut s = Reopened::load(&fs);
        assert!(s.menus.get(&fs, "team_mod").is_none());

        make_pk3(
            dir.path(),
            "zzz_mod.pk3",
            &[
                (
                    "ui_mp/scriptmenus/team_mod.menu",
                    br#"{ menuDef { name "team_mod"
      itemDef { name "a" visible 1 text "@MODMENU_ALLIES" action { scriptMenuResponse "allies"; } }
    } }"#,
                ),
                (
                    "localizedstrings/english/modmenu.str",
                    b"REFERENCE ALLIES\nLANG_ENGLISH \"Allies\"\n",
                ),
            ],
        );
        s.reopen(dir.path(), &mut fs);
        let menu = s.menus.get(&fs, "team_mod").expect("menu from the new pak");
        let v = hud::menu::view(menu, &s.localized, |_| None);
        assert_eq!(v.rows[0].label, "Allies");
    }

    /// The fonts, the sound aliases, the effect files and the impact table
    /// are read off the search path too, and a miss there must not outlive
    /// it either.
    #[test]
    fn reopen_reads_fonts_aliases_and_effects_off_the_new_paks() {
        use audio::cues::{Cue, Source};
        let dir = tempfile::tempdir().unwrap();
        make_pk3(dir.path(), "pak0.pk3", &[("readme.txt", b"")]);
        let mut fs = Pk3Fs::open(dir.path()).unwrap();
        let mut s = Reopened::load(&fs);
        assert!(s.hud.is_none(), "no fonts in the old paks");
        let shout = || Cue {
            alias: "mod_shout".to_string(),
            source: Source::Point(Vec3::ZERO),
            delay_s: 0.0,
        };
        s.audio.play(&fs, shout());
        assert_eq!(s.audio.stats().misses, 1);
        let at = fx::sim::SpawnAt::Point { pos: Vec3::ZERO };
        assert!(s.fx.spawn(&fs, "fx/mod/shout.efx", at, 0.0).is_empty());

        // `parse_font_dat` takes any file of the right length.
        let font = vec![0u8; 20552];
        make_pk3(
            dir.path(),
            "zzz_mod.pk3",
            &[
                ("fonts/fontImage_16.dat", &font),
                ("fonts/fontImage_24.dat", &font),
                (
                    "soundaliases/mod.csv",
                    b"name,file\nmod_shout,mod/shout.wav\n",
                ),
                (
                    "fx/mod/shout.efx",
                    b"Sound\n{\n\tsounds\n\t[\n\t\tmod_shout\n\t]\n}\n",
                ),
                (
                    "fx/iw_impacts.csv",
                    b"bullet_small_normal,concrete,fx/mod/hit.efx\n",
                ),
            ],
        );
        s.reopen(dir.path(), &mut fs);
        assert!(s.hud.is_some(), "fonts from the new pak");
        s.audio.play(&fs, shout());
        assert_eq!(s.audio.stats().misses, 1, "alias from the new pak");
        let sounds = s.fx.spawn(&fs, "fx/mod/shout.efx", at, 0.0);
        assert_eq!(sounds.len(), 1, "effect from the new pak");
        assert_eq!(sounds[0].alias, "mod_shout");

        let hit = net::events::GameEvent {
            event: fx::registry::EV_BULLET_HIT_SMALL,
            parm: 0,
            entity_num: 40,
            client_num: -1,
            weapon: 0,
            surf_type: 5, // concrete
            pos: [0.0; 3],
            dir: [0.0; 3],
            other_entity_num: 40,
            attacker_entity_num: -1,
        };
        let ctx = fx::registry::ResolveCtx {
            muzzles: &HashMap::new(),
            weapon_flash: &HashMap::new(),
            view_flash: None,
            view_body: None,
        };
        match fx::registry::resolve(&hit, &ctx).as_slice() {
            [fx::registry::Resolved::Spawn { path, .. }] => assert_eq!(path, "fx/mod/hit.efx"),
            other => panic!("impact table from the new pak: {other:?}"),
        }
    }

    /// Snapshots arrive at 20 Hz and the window redraws at 60 Hz, so render
    /// time must advance every frame, not per snapshot.
    #[test]
    fn interp_clock_advances_between_snapshots() {
        let mut clock = ServerClock::new();
        let t0 = clock.render_time(0.0, 10_000);
        let t1 = clock.render_time(16.0, 10_000);
        let t2 = clock.render_time(32.0, 10_000);
        let t3 = clock.render_time(48.0, 10_000);
        assert!(
            t0 < t1 && t1 < t2 && t2 < t3,
            "render time froze between snapshots: {t0} {t1} {t2} {t3}"
        );
    }

    #[test]
    fn own_view_adds_delta_angles_and_flips_pitch() {
        // 90 degrees of yaw from delta alone, 45 more from the mouse; wire
        // pitch 10 degrees down reads as the camera looking down.
        let quarter = 16384;
        let (yaw, pitch) = own_view([1820, quarter / 2, 0], [0, quarter, 0]);
        assert!(
            (yaw.to_degrees() - 135.0).abs() < 0.01,
            "yaw {}",
            yaw.to_degrees()
        );
        assert!(
            (pitch.to_degrees() + 10.0).abs() < 0.01,
            "pitch {}",
            pitch.to_degrees()
        );
        // A delta sent as the unsigned 16-bit value wraps like a signed one.
        let (yaw, _) = own_view([0, 0, 0], [0, 65536 - quarter, 0]);
        assert!((yaw.to_degrees() + 90.0).abs() < 0.01);
        // Past straight down draws at the clamp, either way.
        for raw_pitch in [quarter - 100, 20_000, -20_000] {
            let (_, pitch) = own_view([raw_pitch, 0, 0], [0; 3]);
            assert!(
                (pitch.to_degrees().abs() - 87.89).abs() < 0.01,
                "pitch {}",
                pitch.to_degrees()
            );
        }
    }

    #[test]
    fn view_muzzle_offsets_20_forward_4_right_3_down() {
        let cam_pos = Vec3::new(100.0, 200.0, 300.0);
        let forward = Vec3::X;
        let right = Vec3::Y;
        let up = Vec3::Z;
        let (pos, dir) = view_muzzle(cam_pos, forward, right, up);
        assert_eq!(pos, Vec3::new(120.0, 204.0, 297.0));
        assert_eq!(dir, forward);
    }

    /// A snapshot arriving on schedule must not lurch the render time by an
    /// interval.
    #[test]
    fn interp_clock_is_continuous_across_arrival() {
        let mut clock = ServerClock::new();
        clock.render_time(0.0, 10_000);
        let before = clock.render_time(48.0, 10_000);
        let after = clock.render_time(50.0, 10_050);
        assert!(
            (after - before).abs() <= 8,
            "render time jumped across the snapshot seam: {before} -> {after}"
        );
    }

    /// A snapshot arriving late re-anchors behind the time already drawn; the
    /// clock holds there instead of stepping back.
    #[test]
    fn interp_clock_never_runs_backwards() {
        let mut clock = ServerClock::new();
        clock.render_time(0.0, 10_000);
        let before = clock.render_time(70.0, 10_000);
        let after = clock.render_time(70.0, 10_050);
        let later = clock.render_time(100.0, 10_050);
        assert!(
            before <= after && after <= later,
            "render time ran backwards: {before} -> {after} -> {later}"
        );
    }

    /// A long gap (tab-out, map change) re-pegs instead of interpolating across it.
    #[test]
    fn interp_clock_repegs_on_large_jump() {
        let mut clock = ServerClock::new();
        clock.render_time(0.0, 10_000);
        clock.render_time(16.0, 10_000);
        let rt = clock.render_time(30_016.0, 40_000);
        assert!(
            (rt - (40_000 - INTERP_DELAY_MS)).abs() <= 4,
            "clock did not re-peg after a large jump: {rt}"
        );
    }
}
