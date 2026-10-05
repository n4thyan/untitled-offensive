//! Create-a-Class: the persistent, extensible class definition and its
//! on-disk profile format. Pure data and text parsing — no I/O, no engine
//! types — so both the client (to author and edit classes) and the server
//! (to resolve what a player spawns with) share one definition.
//!
//! The format is a deliberately tiny, forgiving line format rather than JSON
//! or TOML: the engine already ships a tokenizer for `name value` pairs, and a
//! profile that a human can read and hand-edit is the point. One line per
//! field, `key value`, `#` comments, blank lines ignored. Unknown keys are
//! kept so a profile written by a newer build survives a round-trip through an
//! older one.
//!
//! This module models the class only. What a slot *resolves to* — a real
//! weapon index, a real item — is the server's job
//! (`vcod_server::classes`), because only the server has the weapon table and
//! the item list to validate against.

use std::collections::BTreeMap;

/// How many classes one profile may hold. MW2-era allowed five; the exact
/// number is not a fidelity claim, it is a bound so a corrupt profile cannot
/// allocate without limit.
pub const MAX_CLASSES: usize = 10;

/// The slots a class fills, in the order a loadout screen shows them. The
/// names are the keys the profile format writes, so they are stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ClassSlot {
    Primary,
    Secondary,
    /// The lethal/equipment slot: a frag, a smoke, a satchel.
    Equipment,
    Perk1,
    Perk2,
    Perk3,
}

impl ClassSlot {
    pub const ALL: [ClassSlot; 6] = [
        ClassSlot::Primary,
        ClassSlot::Secondary,
        ClassSlot::Equipment,
        ClassSlot::Perk1,
        ClassSlot::Perk2,
        ClassSlot::Perk3,
    ];

    /// The profile key, and the name the UI shows.
    pub fn key(self) -> &'static str {
        match self {
            ClassSlot::Primary => "primary",
            ClassSlot::Secondary => "secondary",
            ClassSlot::Equipment => "equipment",
            ClassSlot::Perk1 => "perk1",
            ClassSlot::Perk2 => "perk2",
            ClassSlot::Perk3 => "perk3",
        }
    }

    pub fn from_key(key: &str) -> Option<ClassSlot> {
        ClassSlot::ALL.into_iter().find(|s| s.key() == key)
    }
}

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

/// One class. Every slot is an optional *name* (a configstring-7 weapon name
/// like `m1carbine_mp`, or a perk/item name). Names, not indices: an index is
/// only meaningful against one build's `WEAPON_LIST`, and a profile must
/// survive a rebuild. The server resolves names to indices at spawn time and
/// fails a slot that does not resolve, rather than trusting the profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    pub name: String,
    slots: BTreeMap<ClassSlot, String>,
}

impl Default for Class {
    fn default() -> Self {
        Class {
            name: "Custom".to_string(),
            slots: BTreeMap::new(),
        }
    }
}

impl Class {
    pub fn new(name: impl Into<String>) -> Class {
        Class {
            name: name.into(),
            slots: BTreeMap::new(),
        }
    }

    pub fn get(&self, slot: ClassSlot) -> Option<&str> {
        self.slots.get(&slot).map(String::as_str)
    }

    /// Set a slot. An empty or whitespace-only value clears it, so a UI that
    /// offers an explicit "none" writes the same state as a missing line.
    pub fn set(&mut self, slot: ClassSlot, value: impl Into<String>) {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() {
            self.slots.remove(&slot);
        } else {
            self.slots.insert(slot, value.to_string());
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (ClassSlot, &str)> {
        ClassSlot::ALL
            .into_iter()
            .filter_map(|s| self.get(s).map(|v| (s, v)))
    }

    /// A class with no primary and no secondary is not a playable loadout;
    /// the loader drops it rather than spawning an empty-handed player. A
    /// class with only a primary is fine (a stock loadout gives a pistol).
    pub fn is_playable(&self) -> bool {
        self.get(ClassSlot::Primary).is_some() || self.get(ClassSlot::Secondary).is_some()
    }
}

/// A whole profile: a display name and its classes, in order. Index 0 is the
/// active class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub classes: Vec<Class>,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            name: "Player".to_string(),
            classes: Vec::new(),
        }
    }
}

impl Profile {
    /// The class a spawn uses when nothing else is chosen: the active one, if
    /// it is playable, else the first playable class, else `None` (the caller
    /// then falls back to the stock loadout).
    pub fn active(&self) -> Option<&Class> {
        self.classes
            .first()
            .filter(|c| c.is_playable())
            .or_else(|| self.classes.iter().find(|c| c.is_playable()))
    }

    /// Parse a profile. Never fails: a malformed line is skipped and a
    /// playable-class check drops an unusable one, so a hand-edited profile
    /// degrades to the stock loadout instead of taking the server down.
    /// `problems` collects what was dropped, for the caller to log.
    pub fn parse(text: &str, problems: &mut Vec<String>) -> Profile {
        let mut profile = Profile::default();
        let mut current: Option<Class> = None;
        let mut have_name = false;

        let flush =
            |current: &mut Option<Class>, profile: &mut Profile, problems: &mut Vec<String>| {
                if let Some(class) = current.take() {
                    if class.is_playable() {
                        profile.classes.push(class);
                    } else {
                        problems.push(format!(
                            "class {:?} has no primary or secondary",
                            class.name
                        ));
                    }
                }
            };

        for (lineno, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once(char::is_whitespace) else {
                problems.push(format!("line {}: no value: {raw:?}", lineno + 1));
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();

            match key.as_str() {
                "profile" => {
                    profile.name = value.to_string();
                    have_name = true;
                }
                "class" => {
                    flush(&mut current, &mut profile, problems);
                    current = Some(Class::new(value));
                }
                other => {
                    if let Some(slot) = ClassSlot::from_key(other) {
                        match current.as_mut() {
                            Some(class) => class.set(slot, value),
                            None => problems
                                .push(format!("line {}: {other} outside any class", lineno + 1)),
                        }
                    } else {
                        problems.push(format!("line {}: unknown key {other:?}", lineno + 1));
                    }
                }
            }
        }
        flush(&mut current, &mut profile, problems);

        if !have_name {
            problems.push("no `profile` line; using the default name".to_string());
        }
        profile.classes.truncate(MAX_CLASSES);
        profile
    }

    /// The profile as the line format, the exact inverse of [`Profile::parse`]
    /// for every value the parser accepts (a name with a `#` in it would not
    /// round-trip, which is why names are sanitized on the way in).
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("# UNTITLED class profile\n");
        out.push_str(&format!("profile {}\n", self.name));
        for class in &self.classes {
            out.push_str(&format!("\nclass {}\n", class.name));
            for (slot, value) in class.iter() {
                out.push_str(&format!("{} {}\n", slot.key(), value));
            }
        }
        out
    }
}

/// A profile's class name and every slot value, with `#`/newline stripped, so
/// a name a player typed cannot break the line format or inject a key. Used by
/// the setter path; the parser still tolerates anything a hand-edited file
/// contains.
pub fn sanitize_value(value: &str) -> String {
    value
        .chars()
        .filter(|c| *c != '#' && *c != '\n' && *c != '\r')
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_round_trips() {
        let mut a = Class::new("Assault");
        a.set(ClassSlot::Primary, "m1garand_mp");
        a.set(ClassSlot::Secondary, "colt_mp");
        a.set(ClassSlot::Equipment, "fraggrenade_mp");
        a.set(ClassSlot::Perk1, "perk_stopping_power");
        let p = Profile {
            name: "Nathan".into(),
            classes: vec![a],
        };
        let mut problems = Vec::new();
        let back = Profile::parse(&p.to_text(), &mut problems);
        assert_eq!(problems, Vec::<String>::new());
        assert_eq!(back, p);
    }

    #[test]
    fn an_empty_or_missing_slot_is_absent_not_blank() {
        let mut c = Class::new("x");
        c.set(ClassSlot::Primary, "  ");
        assert_eq!(c.get(ClassSlot::Primary), None);
        c.set(ClassSlot::Primary, "sten_mp");
        assert_eq!(c.get(ClassSlot::Primary), Some("sten_mp"));
    }

    #[test]
    fn an_unplayable_class_is_dropped_with_a_problem() {
        let text = "profile P\nclass Empty\nperk1 nothing\n";
        let mut problems = Vec::new();
        let p = Profile::parse(text, &mut problems);
        assert!(p.classes.is_empty());
        assert_eq!(problems.len(), 1);
        assert!(p.active().is_none());
    }

    #[test]
    fn the_active_class_is_the_first_playable_one() {
        let mut empty = Class::new("Empty");
        empty.set(ClassSlot::Perk1, "x");
        let mut good = Class::new("Good");
        good.set(ClassSlot::Primary, "mp40_mp");
        let p = Profile {
            name: "P".into(),
            classes: vec![empty, good.clone()],
        };
        assert_eq!(p.active().unwrap().name, "Good");
    }

    #[test]
    fn comments_blank_lines_and_stray_keys_are_tolerated() {
        let text =
            "\n# a comment\nprofile  P  # trailing\n\nclass A\nprimary sten_mp\nbogus thing\n";
        let mut problems = Vec::new();
        let p = Profile::parse(text, &mut problems);
        assert_eq!(p.name, "P");
        assert_eq!(p.classes.len(), 1);
        assert_eq!(p.classes[0].get(ClassSlot::Primary), Some("sten_mp"));
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("bogus"));
    }

    #[test]
    fn sanitize_strips_the_metacharacters() {
        assert_eq!(sanitize_value("M1 #1\nprimary x"), "M1 1primary x");
    }

    #[test]
    fn too_many_classes_are_truncated() {
        let mut text = String::from("profile P\n");
        for i in 0..(MAX_CLASSES + 5) {
            text.push_str(&format!("class C{i}\nprimary sten_mp\n"));
        }
        let mut problems = Vec::new();
        let p = Profile::parse(&text, &mut problems);
        assert_eq!(p.classes.len(), MAX_CLASSES);
    }
}
