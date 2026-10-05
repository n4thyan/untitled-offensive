//! Create-a-Class on the server: where profiles live, and what a class
//! resolves to against *this* build's weapon table.
//!
//! The split from [`vcod_common::classes`] is deliberate. The common half owns
//! the format and the data model, because both the client (authoring) and the
//! server (spawning) need to agree on it. This half owns the two things only
//! the server can do:
//!
//! * **Validation.** A class stores weapon *names*; the server is the only
//!   side that holds the weapon table, so it is the only side that can say
//!   whether a name is real. An unresolvable slot is dropped with a problem
//!   rather than trusted, so a profile edited for a different build — or
//!   hand-mangled — degrades to the stock loadout instead of spawning a
//!   player holding a weapon index that does not exist.
//! * **Authority.** The loadout is applied server-side from the server's own
//!   copy of the profile. A client cannot ask to spawn with a weapon the
//!   server will not hand it.
//!
//! Profiles are one `*.txt` file per name under a classes directory, parsed
//! with [`vcod_common::classes::Profile::parse`]. The file stem is the profile
//! key, so `nathan.txt` is the profile for a client named `nathan`. A client
//! with no file gets the embedded default, so a fresh install is playable
//! without anyone authoring anything.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vcod_common::classes::{Class, ClassSlot, Profile};

use crate::weapons::WeaponTable;

/// The default profile, compiled in so a server with no classes directory
/// still has something to spawn. `include_str!` keeps it in lockstep with the
/// shipped copy under `assets/classes/`.
pub const DEFAULT_PROFILE: &str = include_str!("../../../assets/classes/default.txt");

/// The perk names this build knows. Nothing consumes a perk yet — this is the
/// validation list a class is checked against so a typo is reported now and
/// the slot is already data-driven when perks land. Kept deliberately short:
/// three slots' worth of classic-CoD staples, no grind, no unlocks.
pub const KNOWN_PERKS: &[&str] = &[
    "perk_none",
    "perk_stopping_power",
    "perk_juggernaut",
    "perk_sleight_of_hand",
    "perk_steady_aim",
    "perk_bandolier",
    "perk_bomb_squad",
    "perk_flak_jacket",
    "perk_deep_impact",
    "perk_extended_mags",
    "perk_double_tap",
    "perk_iron_lungs",
];

/// A class resolved against the weapon table: the names that survived
/// validation, ready to hand the weapon builtins.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Loadout {
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub equipment: Option<String>,
    /// Kept in slot order; empty slots omitted.
    pub perks: Vec<String>,
    /// What was dropped and why, for the caller to log once.
    pub problems: Vec<String>,
}

impl Loadout {
    pub fn is_empty(&self) -> bool {
        self.primary.is_none() && self.secondary.is_none() && self.equipment.is_none()
    }
}

/// Resolve a class against a weapon table. Weapon slots are checked with
/// `is_weapon`; perk slots are checked against [`KNOWN_PERKS`]. A slot that
/// does not resolve is dropped and noted, never passed on.
pub fn resolve(class: &Class, is_weapon: &dyn Fn(&str) -> bool) -> Loadout {
    let mut out = Loadout::default();
    let mut weapon = |slot: ClassSlot| match class.get(slot) {
        None => None,
        Some(name) if is_weapon(name) => Some(name.to_string()),
        Some(name) => {
            out.problems
                .push(format!("{}: unknown weapon {name:?}", slot.key()));
            None
        }
    };
    out.primary = weapon(ClassSlot::Primary);
    out.secondary = weapon(ClassSlot::Secondary);
    out.equipment = weapon(ClassSlot::Equipment);

    for slot in [ClassSlot::Perk1, ClassSlot::Perk2, ClassSlot::Perk3] {
        let Some(name) = class.get(slot) else {
            continue;
        };
        if KNOWN_PERKS.contains(&name) {
            out.perks.push(name.to_string());
        } else {
            out.problems
                .push(format!("{}: unknown perk {name:?}", slot.key()));
        }
    }
    out
}

/// The server's profile store: every profile it knows, by name, plus which
/// class is active for each.
pub struct Classes {
    profiles: BTreeMap<String, Profile>,
    /// The active class index per profile name. Kept here rather than inside
    /// `Profile` because it is a per-server choice (which class a player is
    /// running right now), not part of the saved data.
    active: BTreeMap<String, usize>,
    /// The embedded default, used for a name with no file.
    default: Profile,
}

impl Default for Classes {
    fn default() -> Self {
        Classes::new(None)
    }
}

impl Classes {
    /// Load every `*.txt` under `dir` (when given), plus the embedded default.
    /// A directory that does not exist is normal, not an error: it means
    /// nobody has authored a profile yet and everyone gets the default.
    pub fn new(dir: Option<&Path>) -> Classes {
        let mut problems = Vec::new();
        let default = Profile::parse(DEFAULT_PROFILE, &mut problems);
        for p in &problems {
            log::warn!("classes: default profile: {p}");
        }
        let mut classes = Classes {
            profiles: BTreeMap::new(),
            active: BTreeMap::new(),
            default,
        };
        if let Some(dir) = dir {
            classes.load_dir(dir);
        }
        classes
    }

    /// Scan a directory of `*.txt` profiles. Sorted by filename so a load is
    /// deterministic; a file that fails to read is logged and skipped rather
    /// than taking the server down.
    pub fn load_dir(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                log::warn!("classes: cannot read {}: {e}", dir.display());
                return;
            }
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("txt")))
            .collect();
        files.sort();
        for path in files {
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    log::warn!("classes: cannot read {}: {e}", path.display());
                    continue;
                }
            };
            let mut problems = Vec::new();
            let profile = Profile::parse(&text, &mut problems);
            for p in &problems {
                log::warn!("classes: {}: {p}", path.display());
            }
            self.insert(stem, profile);
        }
    }

    /// Install a profile under a key. A profile whose name is blank is given
    /// the key as its display name, so the file stem is always usable.
    pub fn insert(&mut self, key: &str, mut profile: Profile) {
        if profile.name.trim().is_empty() {
            profile.name = key.to_string();
        }
        self.active.entry(key.to_ascii_lowercase()).or_insert(0);
        self.profiles.insert(key.to_ascii_lowercase(), profile);
    }

    /// The profile for a client name, or the embedded default. The lookup is
    /// case-insensitive: a client's name is display text, not an identifier.
    pub fn profile(&self, name: &str) -> &Profile {
        self.profiles
            .get(&name.to_ascii_lowercase())
            .unwrap_or(&self.default)
    }

    pub fn has_profile(&self, name: &str) -> bool {
        self.profiles.contains_key(&name.to_ascii_lowercase())
    }

    /// Which class a profile is running. 0 when unknown, and clamped to the
    /// profile's length so a stale index cannot panic a spawn.
    pub fn active_index(&self, name: &str) -> usize {
        let key = name.to_ascii_lowercase();
        let n = self
            .profiles
            .get(&key)
            .unwrap_or(&self.default)
            .classes
            .len();
        self.active
            .get(&key)
            .copied()
            .unwrap_or(0)
            .min(n.saturating_sub(1))
    }

    /// Select a class by index. Out-of-range is refused (false) rather than
    /// clamped silently, so a UI can report it.
    pub fn set_active(&mut self, name: &str, index: usize) -> bool {
        let key = name.to_ascii_lowercase();
        let n = self
            .profiles
            .get(&key)
            .unwrap_or(&self.default)
            .classes
            .len();
        if index >= n {
            return false;
        }
        self.active.insert(key, index);
        true
    }

    /// The class a client named `name` should spawn with, resolved. `None`
    /// when the profile has no playable class at all (then the stock script
    /// loadout stands).
    pub fn loadout_for(&self, name: &str, weapons: &WeaponTable) -> Option<Loadout> {
        let profile = self.profile(name);
        let index = self.active_index(name);
        let class = profile
            .classes
            .get(index)
            .filter(|c| c.is_playable())
            .or_else(|| profile.active())?;
        let is_weapon = |w: &str| {
            crate::configstrings::weapon_index(w).is_some_and(|i| weapons.get(i).is_some())
        };
        let out = resolve(class, &is_weapon);
        (!out.is_empty()).then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn class_with(pairs: &[(ClassSlot, &str)]) -> Class {
        let mut c = Class::new("T");
        for (s, v) in pairs {
            c.set(*s, *v);
        }
        c
    }

    /// A stub table that knows exactly the names handed to it.
    fn table(names: &[&str]) -> WeaponTable {
        let known: HashMap<String, ()> = names.iter().map(|n| (n.to_string(), ())).collect();
        let mut defs: Vec<Option<vcod_common::weapon::WeaponDef>> =
            vec![None; crate::items::NUM_ITEMS];
        for (i, name) in crate::configstrings::WEAPON_LIST.split(' ').enumerate() {
            if known.contains_key(name) {
                defs[i + 1] = Some(vcod_common::weapon::WeaponDef {
                    ammo_name: format!("{name}_ammo"),
                    clip_name: format!("{name}_clip"),
                    weapon_class: "rifle".into(),
                    ..Default::default()
                });
            }
        }
        WeaponTable::from_defs(defs)
    }

    #[test]
    fn a_valid_class_resolves_every_slot() {
        let c = class_with(&[
            (ClassSlot::Primary, "m1garand_mp"),
            (ClassSlot::Secondary, "colt_mp"),
            (ClassSlot::Equipment, "fraggrenade_mp"),
            (ClassSlot::Perk1, "perk_stopping_power"),
        ]);
        let t = table(&["m1garand_mp", "colt_mp", "fraggrenade_mp"]);
        let l = resolve(&c, &|w| {
            crate::configstrings::weapon_index(w).is_some_and(|i| t.get(i).is_some())
        });
        assert_eq!(l.primary.as_deref(), Some("m1garand_mp"));
        assert_eq!(l.secondary.as_deref(), Some("colt_mp"));
        assert_eq!(l.equipment.as_deref(), Some("fraggrenade_mp"));
        assert_eq!(l.perks, vec!["perk_stopping_power"]);
        assert!(l.problems.is_empty());
    }

    #[test]
    fn an_unknown_weapon_is_dropped_not_trusted() {
        let c = class_with(&[(ClassSlot::Primary, "ray_gun_mp")]);
        let t = table(&["m1garand_mp"]);
        let l = resolve(&c, &|w| {
            crate::configstrings::weapon_index(w).is_some_and(|i| t.get(i).is_some())
        });
        assert_eq!(l.primary, None);
        assert!(l.is_empty());
        assert_eq!(l.problems.len(), 1);
    }

    #[test]
    fn an_unknown_perk_is_reported_but_the_weapon_still_lands() {
        let c = class_with(&[
            (ClassSlot::Primary, "sten_mp"),
            (ClassSlot::Perk2, "perk_teleport"),
        ]);
        let t = table(&["sten_mp"]);
        let l = resolve(&c, &|w| {
            crate::configstrings::weapon_index(w).is_some_and(|i| t.get(i).is_some())
        });
        assert_eq!(l.primary.as_deref(), Some("sten_mp"));
        assert!(l.perks.is_empty());
        assert_eq!(l.problems.len(), 1);
    }

    #[test]
    fn the_default_profile_is_playable_and_weapon_names_are_real() {
        let t = table(&[
            "m1carbine_mp",
            "m1garand_mp",
            "thompson_mp",
            "colt_mp",
            "fraggrenade_mp",
        ]);
        let classes = Classes::new(None);
        let l = classes.loadout_for("anyone", &t).expect("default resolves");
        assert!(l.primary.is_some());
        assert!(l.secondary.is_some());
    }

    #[test]
    fn a_name_with_no_file_gets_the_default() {
        let classes = Classes::new(None);
        assert!(!classes.has_profile("stranger"));
        assert_eq!(classes.profile("stranger"), &classes.default);
        assert_eq!(classes.active_index("stranger"), 0);
    }

    #[test]
    fn a_profile_is_found_case_insensitively_and_selects_its_class() {
        let mut classes = Classes::new(None);
        let mut a = Class::new("First");
        a.set(ClassSlot::Primary, "m1garand_mp");
        let mut b = Class::new("Second");
        b.set(ClassSlot::Primary, "thompson_mp");
        classes.insert(
            "Nathan",
            Profile {
                name: "Nathan".into(),
                classes: vec![a, b],
            },
        );
        assert!(classes.has_profile("nathan"));
        assert_eq!(classes.active_index("NATHAN"), 0);
        assert!(classes.set_active("nathan", 1));
        assert_eq!(classes.active_index("nathan"), 1);
        assert!(!classes.set_active("nathan", 2), "out of range refused");
        let t = table(&["m1garand_mp", "thompson_mp"]);
        assert_eq!(
            classes
                .loadout_for("nathan", &t)
                .unwrap()
                .primary
                .as_deref(),
            Some("thompson_mp")
        );
    }

    #[test]
    fn a_profile_with_no_playable_class_resolves_to_none() {
        let mut classes = Classes::new(None);
        let mut empty = Class::new("Empty");
        empty.set(ClassSlot::Perk1, "perk_bomb_squad");
        classes.insert(
            "broken",
            Profile {
                name: "broken".into(),
                classes: vec![empty],
            },
        );
        let t = table(&["m1carbine_mp"]);
        assert!(classes.loadout_for("broken", &t).is_none());
    }

    #[test]
    fn a_classes_directory_is_scanned_and_a_bad_file_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("alpha.txt"),
            "profile Alpha\nclass A\nprimary sten_mp\n",
        )
        .unwrap();
        // A profile whose only class has no weapon: dropped, leaving no class.
        std::fs::write(
            dir.path().join("beta.txt"),
            "profile Beta\nclass B\nperk1 nope\n",
        )
        .unwrap();
        let classes = Classes::new(Some(dir.path()));
        assert!(classes.has_profile("alpha"));
        assert!(classes.has_profile("beta"));
        assert_eq!(classes.profile("alpha").classes.len(), 1);
        assert!(classes.profile("beta").classes.is_empty());
        let t = table(&["sten_mp"]);
        assert_eq!(
            classes.loadout_for("alpha", &t).unwrap().primary.as_deref(),
            Some("sten_mp")
        );
    }
}
