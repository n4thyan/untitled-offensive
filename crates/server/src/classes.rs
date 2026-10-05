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

/// Re-exported: the perk list is shared, because the client offers exactly
/// the perks the server will accept.
pub use vcod_common::classes::KNOWN_PERKS;

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

/// A client-supplied profile name reduced to something safe to use as a
/// filename.
///
/// The name arrives from the network in the connect string, so it is
/// untrusted: `..\..\windows\system32` must not become a path. Anything
/// outside `[A-Za-z0-9_-]` becomes `_`, an empty or all-underscore name
/// falls back to `player`, and the result is length-capped so a long name
/// cannot exceed the filesystem's own limit.
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "player".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The server's profile store: every profile it knows, by name, plus which
/// class is active for each.
pub struct Classes {
    profiles: BTreeMap<String, Profile>,
    /// The active class index per profile name. Kept here rather than inside
    /// `Profile` because it is a per-server choice (which class a player is
    /// running right now), not part of the saved data.
    active: BTreeMap<String, usize>,
    /// A class chosen while alive, waiting for the next spawn. Draining this at
    /// spawn is what makes "change class" a next-life action rather than a
    /// live weapon swap.
    pending: BTreeMap<String, usize>,
    /// The embedded default, used for a name with no file.
    default: Profile,
    /// Where `save` writes, when the server was given a classes dir. Kept
    /// here rather than on the server so there is one answer to "where do
    /// profiles live", not two that can disagree.
    dir: Option<PathBuf>,
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
        let dir_owned = dir.map(Path::to_path_buf);
        let mut problems = Vec::new();
        let default = Profile::parse(DEFAULT_PROFILE, &mut problems);
        for p in &problems {
            log::warn!("classes: default profile: {p}");
        }
        let mut classes = Classes {
            profiles: BTreeMap::new(),
            active: BTreeMap::new(),
            pending: BTreeMap::new(),
            default,
            dir: dir_owned,
        };
        if let Some(dir) = dir {
            classes.load_dir(dir);
        }
        classes
    }

    /// Where [`Self::save`] writes, or `None` when the server was started
    /// without a classes dir.
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
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

    /// The embedded default profile, for a name with no file of its own.
    pub fn default_profile(&self) -> &Profile {
        &self.default
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

    /// Queue a class to be applied at the next spawn, and return whether the
    /// index names a real class.
    ///
    /// This is the alive-player path: a player who opens the class picker
    /// mid-life has their choice recorded here and it takes effect when
    /// `take_pending` is drained at the next spawn. It is deliberately
    /// separate from [`Self::set_active`], which applies immediately, so a
    /// live player cannot swap loadouts by picking a different class.
    pub fn queue_active(&mut self, name: &str, index: usize) -> bool {
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
        self.pending.insert(key, index);
        true
    }

    /// Apply the queued class, if any, and report whether the active class
    /// changed. Called at spawn, so a choice made while alive lands on the
    /// next life and not the current one.
    pub fn take_pending(&mut self, name: &str) -> bool {
        let key = name.to_ascii_lowercase();
        let Some(index) = self.pending.remove(&key) else {
            return false;
        };
        self.active.insert(key, index);
        true
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

    /// The profile for `name`, or a fresh empty one the caller can edit.
    ///
    /// A name with no file gets a real profile rather than a borrowed
    /// reference into the embedded default: Create-a-Class has to be able to
    /// build a profile from nothing, and handing back `&self.default` would
    /// quietly write one player's classes into everyone's fallback.
    pub fn profile_mut(&mut self, name: &str) -> &mut Profile {
        let key = name.to_ascii_lowercase();
        self.profiles.entry(key.clone()).or_insert_with(|| Profile {
            name: name.to_string(),
            classes: Vec::new(),
        })
    }

    /// Put a slot in the active class, creating the class if the profile has
    /// none yet. `false` when the value is rejected by `sanitize_value`.
    pub fn set_slot(&mut self, name: &str, slot: ClassSlot, value: &str) -> bool {
        let value = vcod_common::classes::sanitize_value(value);
        if value.is_empty() {
            return false;
        }
        // Slot edits always target the active class, so a player picking a
        // weapon is filling in the class they will spawn with. Read the index
        // before the mutable borrow of the profile.
        let index = self
            .active
            .get(&name.to_ascii_lowercase())
            .copied()
            .unwrap_or(0);
        let profile = self.profile_mut(name);
        if profile.classes.is_empty() {
            profile.classes.push(Class::new("Custom"));
        }
        let index = index.min(profile.classes.len().saturating_sub(1));
        if let Some(class) = profile.classes.get_mut(index) {
            class.set(slot, value);
            true
        } else {
            false
        }
    }

    /// Write a profile back to `<dir>/<key>.txt`, so an edit survives the
    /// server and the next launch reads it as an authored profile.
    ///
    /// Returns the path written. The name is sanitized to a filename: a
    /// profile is keyed by a client-supplied connect name, and that name is
    /// never allowed to reach the filesystem unfiltered.
    /// Write a profile into the dir the server was started with.
    pub fn save(&self, name: &str) -> std::io::Result<PathBuf> {
        let dir = self.dir().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no classes dir on this server",
            )
        })?;
        self.save_to(name, dir)
    }

    /// Write a profile into an explicit dir.
    pub fn save_to(&self, name: &str, dir: &Path) -> std::io::Result<PathBuf> {
        let key = sanitize_filename(name);
        let Some(profile) = self.profiles.get(&name.to_ascii_lowercase()) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no such profile",
            ));
        };
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{key}.txt"));
        std::fs::write(&path, profile.to_text())?;
        Ok(path)
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

    /// The class a client named `name` spawns with, resolved against
    /// `weapons`. `None` when the profile has no playable class, in which case
    /// the stock script loadout stands.
    ///
    /// `chosen` is the weapon the gametype's `spawnPlayer` just gave the
    /// player, from the stock weapon menu. A name with a profile of its own
    /// always wins — that is the point of Create-a-Class. A name with *no*
    /// profile gets the built-in default **only** when it has not already
    /// chosen a weapon for itself, because applying the default
    /// unconditionally overrode the weapon a player had just picked:
    /// `spawnPlayer` set the carbine and the default profile's first class
    /// replaced it with the garand one frame later. The default is a fallback
    /// for a fresh install, not a policy that outranks an explicit choice.
    ///
    /// Before this distinction existed, a client that answered the stock
    /// weapon menu with the carbine still spawned with the default profile's
    /// garand, which is indistinguishable from Create-a-Class not working.
    pub fn loadout_for(
        &self,
        name: &str,
        weapons: &WeaponTable,
        chosen: Option<usize>,
    ) -> Option<Loadout> {
        if !self.has_profile(name) && chosen.is_some() {
            return None;
        }
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
        let l = classes
            .loadout_for("anyone", &t, None)
            .expect("default resolves");
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

    /// The bug this guards: applying the built-in default to a player who had
    /// just picked a weapon at the stock weapon menu replaced their choice.
    /// `chosen` is that weapon, and a name with no profile of its own keeps it.
    #[test]
    fn a_profile_less_name_that_chose_a_weapon_keeps_it() {
        let classes = Classes::new(None);
        let t = table(&["m1carbine_mp", "m1garand_mp", "thompson_mp", "colt_mp"]);
        let carbine = crate::configstrings::weapon_index("m1carbine_mp").unwrap();
        assert_eq!(
            classes.loadout_for("stranger", &t, Some(carbine)),
            None,
            "the default does not outrank the weapon menu"
        );
        // Having chosen nothing, the same name still gets the default, so a
        // fresh install is playable with nothing authored.
        assert!(classes.loadout_for("stranger", &t, None).is_some());
    }

    /// A profile of its own always wins, chosen weapon or not: that is the
    /// whole point of Create-a-Class.
    #[test]
    fn a_profile_of_its_own_beats_the_weapon_menu_either_way() {
        let mut classes = Classes::new(None);
        let mut c = Class::new("Mine");
        c.set(ClassSlot::Primary, "thompson_mp");
        classes.insert(
            "nathan",
            Profile {
                name: "Nathan".into(),
                classes: vec![c],
            },
        );
        let t = table(&["m1carbine_mp", "thompson_mp"]);
        let carbine = crate::configstrings::weapon_index("m1carbine_mp").unwrap();
        for chosen in [None, Some(carbine)] {
            assert_eq!(
                classes
                    .loadout_for("nathan", &t, chosen)
                    .unwrap()
                    .primary
                    .as_deref(),
                Some("thompson_mp"),
                "the class wins with chosen={chosen:?}"
            );
        }
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
                .loadout_for("nathan", &t, None)
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
        assert!(classes.loadout_for("broken", &t, None).is_none());
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
            classes
                .loadout_for("alpha", &t, None)
                .unwrap()
                .primary
                .as_deref(),
            Some("sten_mp")
        );
    }

    /// A profile name arrives from the network, so it must not be able to
    /// choose a path. These are the shapes that matter: traversal, an
    /// absolute path, a device name, and an empty result.
    #[test]
    fn a_profile_name_cannot_choose_a_path() {
        for hostile in [
            "..\\..\\windows\\system32",
            "../../etc/passwd",
            "C:\\Windows\\System32",
            "/etc/shadow",
            "..",
            ".",
            "",
            "///",
            "con",
        ] {
            let out = sanitize_filename(hostile);
            assert!(
                out.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "{hostile:?} -> {out:?} kept a filesystem-significant character"
            );
            assert!(
                !out.contains(".."),
                "{hostile:?} -> {out:?} kept a traversal"
            );
            assert!(!out.is_empty(), "{hostile:?} produced an empty filename");
            assert!(out.len() <= 32, "{hostile:?} -> {out:?} is over the cap");
        }
        // And the ordinary case is untouched, or nobody's profile would save.
        assert_eq!(sanitize_filename("vcod"), "vcod");
        assert_eq!(sanitize_filename("Some Player"), "Some_Player");
    }

    /// Editing a slot creates the class and the profile if they are missing,
    /// and it always targets the class the player will spawn with.
    #[test]
    fn editing_a_slot_builds_a_profile_from_nothing() {
        let mut c = Classes::new(None);
        assert!(!c.has_profile("newbie"), "starts with no profile");
        assert!(c.set_slot("newbie", ClassSlot::Primary, "thompson_mp"));
        assert!(c.has_profile("newbie"));
        let p = c.profile("newbie");
        assert_eq!(p.classes.len(), 1);
        assert_eq!(p.classes[0].get(ClassSlot::Primary), Some("thompson_mp"));

        // A second edit lands in the same (active) class, not a new one.
        assert!(c.set_slot("newbie", ClassSlot::Perk1, "perk_sleight_of_hand"));
        assert_eq!(c.profile("newbie").classes.len(), 1);
        assert_eq!(
            c.profile("newbie").classes[0].get(ClassSlot::Perk1),
            Some("perk_sleight_of_hand")
        );

        // An empty value is refused rather than clearing the slot silently.
        assert!(!c.set_slot("newbie", ClassSlot::Primary, "   "));
        assert_eq!(
            c.profile("newbie").classes[0].get(ClassSlot::Primary),
            Some("thompson_mp")
        );
    }

    /// A class chosen while alive is queued, not applied: this is the exploit
    /// guard. `queue_active` must not change what the current life spawns
    /// with, and only a spawn's `take_pending` may.
    #[test]
    fn a_class_queued_while_alive_applies_only_at_the_next_spawn() {
        let mut c = Classes::new(None);
        let mut a = Class::new("First");
        a.set(ClassSlot::Primary, "m1garand_mp");
        let mut b = Class::new("Second");
        b.set(ClassSlot::Primary, "thompson_mp");
        c.insert(
            "nathan",
            Profile {
                name: "Nathan".into(),
                classes: vec![a, b],
            },
        );
        let t = table(&["m1garand_mp", "thompson_mp"]);
        assert_eq!(c.active_index("nathan"), 0);

        // The choice is queued, and the live class is untouched.
        assert!(c.queue_active("nathan", 1));
        assert_eq!(
            c.active_index("nathan"),
            0,
            "queueing must not swap the live loadout"
        );
        assert_eq!(
            c.loadout_for("nathan", &t, None).unwrap().primary.as_deref(),
            Some("m1garand_mp"),
            "the current life keeps its weapon"
        );

        // The spawn drains the queue and the new class applies.
        assert!(c.take_pending("nathan"));
        assert_eq!(c.active_index("nathan"), 1);
        assert_eq!(
            c.loadout_for("nathan", &t, None).unwrap().primary.as_deref(),
            Some("thompson_mp")
        );
        // Drained once: a second spawn does not re-apply anything.
        assert!(!c.take_pending("nathan"));
    }

    /// Queueing an index that names no class is refused, so a client cannot
    /// ask for a class the server does not have.
    #[test]
    fn queueing_an_out_of_range_class_is_refused() {
        let mut c = Classes::new(None);
        let mut a = Class::new("Only");
        a.set(ClassSlot::Primary, "m1garand_mp");
        c.insert(
            "nathan",
            Profile {
                name: "Nathan".into(),
                classes: vec![a],
            },
        );
        assert!(!c.queue_active("nathan", 1));
        assert!(!c.take_pending("nathan"), "nothing was queued");
        assert_eq!(c.active_index("nathan"), 0);
    }

    /// A name that only ever saw the embedded default still gets the default
    /// profile's classes, so `selectclass` on a fresh client names something
    /// real. This is what `send_profile` shows it, and the picker must agree.
    #[test]
    fn a_fresh_name_is_shown_the_default_profiles_classes() {
        let c = Classes::new(None);
        assert!(!c.has_profile("stranger"));
        let default = c.default_profile();
        assert!(!default.classes.is_empty(), "the default is playable");
        // The default's first class is what `profile()` resolves for a name
        // with no file, so the two views agree.
        assert_eq!(
            c.profile("stranger").classes.len(),
            default.classes.len()
        );
    }

    /// A saved profile reads back as the same authored profile, and the file
    /// lands under the classes dir under a sanitized name.
    #[test]
    fn a_saved_profile_round_trips() {
        let dir = std::env::temp_dir().join("vcod-class-save-test");
        let _ = std::fs::remove_dir_all(&dir);
        let mut c = Classes::new(Some(&dir));
        c.set_slot("round trip/../evil", ClassSlot::Primary, "m1garand_mp");
        c.set_slot("round trip/../evil", ClassSlot::Secondary, "colt_mp");
        let path = c.save("round trip/../evil").expect("save");
        assert!(path.starts_with(&dir), "{path:?} escaped {dir:?}");
        assert!(path.extension().is_some_and(|e| e == "txt"));
        let text = std::fs::read_to_string(&path).unwrap();
        let mut problems = Vec::new();
        let back = vcod_common::classes::Profile::parse(&text, &mut problems);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(back.classes.len(), 1);
        assert_eq!(back.classes[0].get(ClassSlot::Primary), Some("m1garand_mp"));
        assert_eq!(back.classes[0].get(ClassSlot::Secondary), Some("colt_mp"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
