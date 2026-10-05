//! Create-a-Class end to end on the stock `dm.gsc`: a client whose profile
//! names a class spawns holding that class's weapons, not the stock loadout,
//! and the playerstate proves it — the spawn weapon and the ammo arrays come
//! from the class. A client with no profile keeps the stock loadout.
//!
//! The client's name is its userinfo's `\name\vcod`, which is fixed in
//! `NetClient`, so the profile under test is always keyed `vcod`.
//!
//! Needs `COD_DIR`; without the paks it returns early.

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;
use vcod_common::net::protocol::PROTOCOL_V1;
use vcod_common::net::NetClient;

use common::{ClientEnd, Queues};

const MAP: &str = "mp_carentan";
const NAME: &str = "vcod";

/// The 1-based configstring-7 index of a stock weapon.
fn wi(name: &str) -> i32 {
    vcod_server::configstrings::weapon_index(name).unwrap() as i32
}

/// The ammo-array index a weapon's file names. Deliberately not the cs7
/// position: the two are different numbers (`m1carbine_mp` is cs7 12 and ammo
/// 11), and a test that conflates them passes for the wrong reason.
fn ammo_of(weapons: &vcod_server::weapons::WeaponTable, name: &str) -> usize {
    let def = weapons
        .get(wi(name) as usize)
        .unwrap_or_else(|| panic!("{name} is in the weapon table"));
    def.ammo_index
}

/// The clip-array index a weapon's file names.
fn clip_of(weapons: &vcod_server::weapons::WeaponTable, name: &str) -> i16 {
    let def = weapons
        .get(wi(name) as usize)
        .unwrap_or_else(|| panic!("{name} is in the weapon table"));
    def.clip_size as i16
}

/// A server on `mp_carentan` dm with `profiles` loaded (the first keyed to the
/// client's name), and one client joined through the stock menus answering
/// the weapon menu with `menu_weapon`. The join runs to its settle, so the
/// spawn's loadout has applied by the time it returns.
fn join_with(
    profiles: &[&str],
    menu_weapon: &str,
) -> Option<(
    vcod_server::Server,
    NetClient<ClientEnd>,
    std::rc::Rc<vcod_server::weapons::WeaponTable>,
)> {
    let fs = vcod_common::testing::game_fs()?;
    let bsp_path = fs.resolve_map(MAP).expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let fs = Rc::new(fs);

    // One `*.txt` per profile, the first keyed to the client's own name.
    let dir = tempfile::tempdir().unwrap();
    for (i, body) in profiles.iter().enumerate() {
        let key = if i == 0 { NAME } else { "other" };
        std::fs::write(dir.path().join(format!("{key}.txt")), body).unwrap();
    }

    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, "dm"), now);
    sv.load_classes(dir.path());
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(fs).expect("load the scripts");

    let q = Rc::new(RefCell::new(Queues::default()));
    let (mut cl, _join) = common::join(&mut sv, &q, &mut now, "allies", menu_weapon);
    let weapons = sv.weapon_table();
    for _ in 0..40 {
        now += std::time::Duration::from_millis(50);
        cl.send_frame(&vcod_common::net::msg::NULL_USERCMD);
        common::step(&mut sv, &q, &mut cl, now);
    }
    Some((sv, cl, weapons))
}

/// A profile whose single class is the three weapon slots.
fn profile_text(primary: &str, secondary: &str, equipment: &str) -> String {
    format!(
        "profile Test\nclass Test\nprimary {primary}\nsecondary {secondary}\nequipment {equipment}\n"
    )
}

/// A profile class replaces the stock spawn loadout: the menu answered
/// `m1carbine_mp`, the class names `thompson_mp`, and the player spawns with
/// the Thompson.
#[test]
fn a_profile_class_replaces_the_stock_spawn_loadout() {
    let Some((_sv, cl, weapons)) = join_with(
        &[&profile_text("thompson_mp", "colt_mp", "fraggrenade_mp")],
        "m1carbine_mp",
    ) else {
        eprintln!("skipping: no game data");
        return;
    };
    let snap = cl.snapshots().newest().expect("a snapshot").clone();
    assert_eq!(
        snap.ps.field_i32(&PROTOCOL_V1, "weapon"),
        wi("thompson_mp"),
        "the spawn weapon is the class primary, not the menu's carbine"
    );
    // The class's weapons are held with the class's own ammo, and the stock
    // carbine the gametype gave first was taken with its rounds. The ammo and
    // clip indexes come from the weapon files, not from the cs7 position: the
    // carbine's reserve lives at 11 and the garand's at 12, and a hardcoded
    // 10 here silently tested the wrong weapon.
    let t: &vcod_server::weapons::WeaponTable = &weapons;
    let thompson = ammo_of(t, "thompson_mp");
    let carbine = ammo_of(t, "m1carbine_mp");
    assert!(snap.ps.ammo(thompson) > 0, "thompson reserve");
    assert_eq!(
        snap.ps.clip(thompson),
        clip_of(t, "thompson_mp"),
        "and a full clip"
    );
    assert_eq!(
        snap.ps.ammo(carbine),
        0,
        "the carbine's stock reserve was cleared"
    );
    assert_eq!(snap.ps.clip(carbine), 0, "and its clip");
}

/// A client with no profile file keeps the stock loadout, so a fresh install
/// is playable with nothing authored.
#[test]
fn a_client_with_no_profile_keeps_the_stock_loadout() {
    let Some((_sv, cl, weapons)) = join_with(&[], "m1carbine_mp") else {
        eprintln!("skipping: no game data");
        return;
    };
    let snap = cl.snapshots().newest().expect("a snapshot").clone();
    assert_eq!(
        snap.ps.field_i32(&PROTOCOL_V1, "weapon"),
        wi("m1carbine_mp"),
        "the menu answer stands"
    );
    let carbine = ammo_of(&weapons, "m1carbine_mp");
    assert_eq!(snap.ps.ammo(carbine), 400, "the carbine's stock reserve");
    assert_eq!(snap.ps.clip(carbine), 15, "and its full magazine");
}

/// A class with an unknown weapon name fails safe: the slot is dropped, the
/// rest of the class still applies, and no spawn is broken.
#[test]
fn an_unknown_weapon_in_a_class_does_not_break_the_spawn() {
    let Some((_sv, cl, weapons)) = join_with(
        &[&profile_text("ray_gun_mp", "colt_mp", "fraggrenade_mp")],
        "m1carbine_mp",
    ) else {
        eprintln!("skipping: no game data");
        return;
    };
    let snap = cl.snapshots().newest().expect("a snapshot").clone();
    // The bad primary is dropped and the class's own primary is now absent, so
    // `apply_loadout` falls back to handing out the class's secondary and
    // equipment. `ps.weapon` follows the primary slot, which is empty, so the
    // spawn weapon is whatever the sim settled on — what matters is that the
    // spawn happened at all and the player is not empty-handed: the frag and
    // the colt are both live.
    assert_eq!(
        snap.ps.field_i32(&PROTOCOL_V1, "pm_type"),
        0,
        "the spawn completed: the bad weapon did not break it"
    );
    let colt = ammo_of(&weapons, "colt_mp");
    assert!(
        snap.ps.ammo(colt) > 0,
        "the class secondary was still given"
    );
    assert!(snap.ps.clip(colt) > 0, "with a magazine");
    assert_eq!(
        snap.ps.clip(ammo_of(&weapons, "fraggrenade_mp")),
        3,
        "and the frag"
    );
}

/// `selectclass` queues a class for the next spawn and does not swap the live
/// one: the exploit guard, proven through the real server rather than the
/// store's own unit test.
#[test]
fn selectclass_queues_a_class_without_swapping_the_live_one() {
    // A profile with two classes: the first is the stock spawn, the second a
    // different weapon the client will ask for mid-life.
    let two = "profile Test\nclass First\nprimary m1garand_mp\nsecondary colt_mp\n\
               class Second\nprimary thompson_mp\nsecondary colt_mp\n";
    let Some((mut sv, mut cl, weapons)) = join_with(&[two], "m1carbine_mp") else {
        eprintln!("skipping: no game data");
        return;
    };
    // Spawned on the first class.
    let snap = cl.snapshots().newest().expect("a snapshot").clone();
    assert_eq!(
        snap.ps.field_i32(&PROTOCOL_V1, "weapon"),
        wi("m1garand_mp"),
        "the first class is the active one"
    );

    // Ask for the second class, then step frames without dying. The live
    // weapon must not change: the choice is queued, not applied.
    cl.send_reliable("selectclass 1");
    let mut now = Instant::now();
    let q = Rc::new(RefCell::new(Queues::default()));
    for _ in 0..20 {
        now += std::time::Duration::from_millis(50);
        cl.send_frame(&vcod_common::net::msg::NULL_USERCMD);
        common::step(&mut sv, &q, &mut cl, now);
    }
    let snap = cl.snapshots().newest().expect("a snapshot").clone();
    assert_eq!(
        snap.ps.field_i32(&PROTOCOL_V1, "weapon"),
        wi("m1garand_mp"),
        "an alive player cannot swap loadouts by asking for another class"
    );
    let _ = weapons;
}
