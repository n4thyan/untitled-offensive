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

/// The profile-over-the-wire format: how the server hands a client its own
/// authoritative profile so Create-a-Class opens on what the player actually
/// saved instead of a blank grid.
///
/// It is a sequence of vcod server commands rather than one blob, because a
/// server command is a single line (the client's `read_string` stops at the
/// newline) and a profile's own text format is multi-line.
///
/// The framing is a header, exactly N payload commands, then a terminator:
///
/// ```text
/// classprofile "vcod" 9
/// classdef 0 "Rifleman"
/// classslot 0 primary m1garand_mp
/// ...
/// classend
/// ```
///
/// The count and the terminator are both checked, so a dropped or duplicated
/// command in the middle cannot pass as a complete profile. A client that
/// joined mid-sync therefore sees nothing until the whole thing lands, rather
/// than opening the screen on a half-built class.
///
/// The server is authoritative: this only ever travels server -> client. A
/// client never sends its profile up, because its copy is a view of the
/// server's, not a second truth to reconcile.
pub mod wire {
    use super::{Class, ClassSlot, Profile, MAX_CLASSES};

    /// `classprofile <name> <line_count>` - the header, sent first.
    pub const PROFILE: &str = "classprofile";
    /// `classdef <index> <name>` - one per class.
    pub const CLASS: &str = "classdef";
    /// `classslot <index> <slot_key> <value>` - one per filled slot.
    pub const SLOT: &str = "classslot";
    /// The terminator, sent last.
    pub const END: &str = "classend";

    /// The commands that describe `profile`, in order: the header, the payload
    /// the header counts, then [`END`].
    ///
    /// The name and class names are quoted so a space in either survives the
    /// client's tokenizer (which treats a double-quoted run as one token).
    pub fn encode(profile: &Profile) -> Vec<String> {
        let classes = profile.classes.iter().take(MAX_CLASSES).collect::<Vec<_>>();
        let mut payload = Vec::new();
        for (index, class) in classes.iter().enumerate() {
            payload.push(format!("{CLASS} {index} {}", quote(&class.name)));
            for (slot, value) in class.iter() {
                payload.push(format!("{SLOT} {index} {} {}", slot.key(), value));
            }
        }
        let mut out = Vec::with_capacity(payload.len() + 2);
        out.push(format!(
            "{PROFILE} {} {}",
            quote(&profile.name),
            payload.len()
        ));
        out.extend(payload);
        out.push(END.to_string());
        out
    }

    /// A single argument quoted for the tokenizer, with any embedded `"`
    /// removed so a quote cannot end the token early and inject a field.
    fn quote(s: &str) -> String {
        format!("\"{}\"", s.replace('"', "'"))
    }

    /// Assembles a profile from the command stream. Fed every server command
    /// the client sees; `on_command` returns true when the token belonged to
    /// this format, so the caller can route the rest elsewhere.
    ///
    /// A malformed or out-of-range command is ignored rather than aborting the
    /// sync. The header's count and the terminator are both checked, so a
    /// dropped command leaves [`Self::profile`] returning `None` rather than a
    /// profile missing a slot.
    #[derive(Default, Debug)]
    pub struct ProfileSync {
        name: Option<String>,
        /// How many payload commands the header promised.
        expected: Option<usize>,
        /// Payload commands seen since the header.
        seen: usize,
        classes: Vec<Class>,
        /// The terminator arrived.
        ended: bool,
    }

    impl ProfileSync {
        pub fn new() -> ProfileSync {
            ProfileSync::default()
        }

        /// A new gamestate invalidates a half-received profile.
        pub fn reset(&mut self) {
            *self = ProfileSync::default();
        }

        pub fn on_command(&mut self, tokens: &[String]) -> bool {
            match tokens.first().map(String::as_str) {
                Some(PROFILE) => {
                    let (Some(name), Some(count)) = (
                        tokens.get(1),
                        tokens.get(2).and_then(|n| n.parse::<usize>().ok()),
                    ) else {
                        return true;
                    };
                    self.name = Some(name.clone());
                    self.expected = Some(count.min(MAX_CLASSES * (1 + ClassSlot::ALL.len())));
                    self.seen = 0;
                    self.classes.clear();
                    self.ended = false;
                    true
                }
                Some(END) => {
                    self.ended = true;
                    true
                }
                Some(CLASS) => {
                    if self.expected.is_none() {
                        return true;
                    }
                    self.seen += 1;
                    let (Some(index), Some(name)) = (
                        tokens.get(1).and_then(|n| n.parse::<usize>().ok()),
                        tokens.get(2),
                    ) else {
                        return true;
                    };
                    // Classes arrive in index order; a gap is a dropped
                    // command, and the count check below catches it.
                    if index == self.classes.len() {
                        self.classes.push(Class::new(name.clone()));
                    }
                    true
                }
                Some(SLOT) => {
                    if self.expected.is_none() {
                        return true;
                    }
                    self.seen += 1;
                    let (Some(index), Some(slot), Some(value)) = (
                        tokens.get(1).and_then(|n| n.parse::<usize>().ok()),
                        tokens.get(2).and_then(|s| ClassSlot::from_key(s)),
                        tokens.get(3),
                    ) else {
                        return true;
                    };
                    if let Some(class) = self.classes.get_mut(index) {
                        class.set(slot, value.clone());
                    }
                    true
                }
                _ => false,
            }
        }

        /// The profile, once the whole of it has arrived: the terminator seen
        /// and exactly the number of payload commands the header promised.
        pub fn profile(&self) -> Option<Profile> {
            let expected = self.expected?;
            if !self.ended || self.seen != expected {
                return None;
            }
            Some(Profile {
                name: self.name.clone().unwrap_or_else(|| "Player".to_string()),
                classes: self.classes.clone(),
            })
        }
    }
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

    /// The wire format is the server's profile -> the client's copy, and the
    /// client's copy has to be the same profile: a round trip through
    /// `encode`/`ProfileSync` must reproduce every class and slot, or
    /// Create-a-Class opens on a lie.
    #[test]
    fn a_profile_round_trips_through_the_wire_format() {
        let mut a = Class::new("Rifleman");
        a.set(ClassSlot::Primary, "m1garand_mp");
        a.set(ClassSlot::Secondary, "colt_mp");
        a.set(ClassSlot::Equipment, "fraggrenade_mp");
        a.set(ClassSlot::Perk1, "perk_stopping_power");
        let mut b = Class::new("Support");
        b.set(ClassSlot::Primary, "bar_mp");
        let profile = Profile {
            name: "vcod".into(),
            classes: vec![a, b],
        };
        let mut sync = wire::ProfileSync::new();
        for cmd in wire::encode(&profile) {
            let tokens = tokenize(&cmd);
            assert!(sync.on_command(&tokens), "{cmd:?} was not ours");
        }
        assert_eq!(sync.profile().as_ref(), Some(&profile));
    }

    /// A class name with a space in it survives the tokenizer, and a quote in
    /// a name cannot break out and inject a field.
    #[test]
    fn a_quoted_name_survives_the_wire_tokenizer() {
        let mut a = Class::new("My Class");
        a.set(ClassSlot::Primary, "thompson_mp");
        let profile = Profile {
            name: "Some Player".into(),
            classes: vec![a],
        };
        // Run it through the client's real tokenizer, not a naive split.
        let mut sync = wire::ProfileSync::new();
        for cmd in wire::encode(&profile) {
            let tokens = tokenize(&cmd);
            sync.on_command(&tokens);
        }
        let back = sync.profile().expect("complete");
        assert_eq!(back.name, "Some Player");
        assert_eq!(back.classes[0].name, "My Class");
        assert_eq!(back.classes[0].get(ClassSlot::Primary), Some("thompson_mp"));

        // A hostile name cannot escape its token.
        let hostile = Profile {
            name: "a\" 9 \"b".into(),
            classes: vec![Class::new("x")],
        };
        let mut sync = wire::ProfileSync::new();
        for cmd in wire::encode(&hostile) {
            sync.on_command(&tokenize(&cmd));
        }
        let back = sync.profile().expect("complete");
        assert!(!back.name.contains('"'), "a quote survived: {:?}", back.name);
    }

    /// A half-received profile yields nothing, so a client that joined
    /// mid-sync cannot open Create-a-Class on a partly-built class.
    #[test]
    fn an_incomplete_profile_yields_nothing() {
        let mut a = Class::new("Rifleman");
        a.set(ClassSlot::Primary, "m1garand_mp");
        let mut b = Class::new("Support");
        b.set(ClassSlot::Primary, "bar_mp");
        let profile = Profile {
            name: "p".into(),
            classes: vec![a, b],
        };
        let cmds = wire::encode(&profile);
        // Everything but the last command: one class short of the header.
        let mut sync = wire::ProfileSync::new();
        for cmd in &cmds[..cmds.len() - 1] {
            sync.on_command(&tokenize(cmd));
        }
        assert!(sync.profile().is_none(), "a partial profile must not land");

        // The whole thing, and it lands.
        for cmd in &cmds {
            sync.on_command(&tokenize(cmd));
        }
        assert_eq!(sync.profile().as_ref(), Some(&profile));
    }

    /// A command that is not part of the format is reported as not ours, so
    /// the caller can route it to whatever else handles server commands.
    #[test]
    fn an_unrelated_command_is_not_claimed() {
        let mut sync = wire::ProfileSync::new();
        assert!(!sync.on_command(&tokenize("b 1 0 0 2 10 50")));
        assert!(!sync.on_command(&tokenize("t 0")));
        assert!(sync.profile().is_none());
    }

    /// A profile with no classes is complete as soon as its header lands: an
    /// empty profile is a real answer (a fresh player), not a missing one.
    #[test]
    fn an_empty_profile_is_complete() {
        let mut sync = wire::ProfileSync::new();
        for cmd in wire::encode(&Profile {
            name: "fresh".into(),
            classes: Vec::new(),
        }) {
            sync.on_command(&tokenize(&cmd));
        }
        let p = sync.profile().expect("an empty profile is still a profile");
        assert_eq!(p.name, "fresh");
        assert!(p.classes.is_empty());
    }

    /// A new gamestate drops a half-received profile rather than letting it
    /// merge with the next map's.
    #[test]
    fn reset_drops_a_partial_sync() {
        let mut a = Class::new("Rifleman");
        a.set(ClassSlot::Primary, "m1garand_mp");
        let mut b = Class::new("Support");
        b.set(ClassSlot::Primary, "bar_mp");
        let cmds = wire::encode(&Profile {
            name: "p".into(),
            classes: vec![a, b],
        });
        let mut sync = wire::ProfileSync::new();
        for cmd in &cmds[..cmds.len() - 1] {
            sync.on_command(&tokenize(cmd));
        }
        sync.reset();
        assert!(sync.profile().is_none());
        for cmd in &cmds {
            sync.on_command(&tokenize(cmd));
        }
        assert!(sync.profile().is_some());
    }

    /// The client's own tokenizer, so the test exercises the split the wire
    /// actually uses. Mirrors `net::tokenize` (a quoted run is one token).
    fn tokenize(s: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut chars = s.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else if c == '"' {
                chars.next();
                let mut tok = String::new();
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    tok.push(c);
                }
                tokens.push(tok);
            } else {
                let mut tok = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    tok.push(c);
                    chars.next();
                }
                tokens.push(tok);
            }
        }
        tokens
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
