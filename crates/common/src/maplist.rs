//! The playable-map registry: one canonical source of truth for which maps
//! exist, which are playable, and what a player should see them called.
//!
//! Everything that needs a map list reads it from here - the map selector, the
//! rotation, and end-of-match map voting - so the three cannot disagree about
//! what is playable. Before this, each would have discovered maps for itself
//! and a map could be in the rotation but not the selector.
//!
//! The inventory is *discovered* from the mounted paks, not hard-coded: a
//! stock map is playable because it is in the install, not because a switch
//! lists it. Only the display names and the denylist are authored, because
//! neither can be derived.

use crate::pk3::Pk3Fs;

/// Maps that exist but must not appear in a player-facing list.
///
/// `mp_toccoa` is the project's own dev map: its source, tooling and launcher
/// are intact and it stays reachable by naming it explicitly, but it is not a
/// shipping map and must not appear in the selector, the rotation or a vote.
const NOT_PLAYER_FACING: &[&str] = &["mp_toccoa"];

/// The gametypes the player-facing menu offers, in the order they are shown.
///
/// Only proven modes are here. `dm` is the mode every verification run used;
/// `tdm` is proven on the stock maps (both teams assigned, kills observed).
/// A mode whose script exists but has not been played through is deliberately
/// absent - offering it would promise something unverified.
pub const GAMETYPES: &[(&str, &str)] = &[("dm", "Deathmatch"), ("tdm", "Team Deathmatch")];

/// The most bots a player-facing match may ask for. The server takes a real
/// client slot per bot, so the ceiling is well under the wire's 64-client
/// field; this is a game-design bound, not a protocol one.
pub const MAX_BOTS: usize = 16;

/// The fewest bots a match may be started with.
pub const MIN_BOTS: usize = 0;

/// One map a player may choose: the canonical id and the friendly name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayableMap {
    /// The canonical id the server and the rotation use, e.g. `mp_carentan`.
    pub id: String,
    /// What the selector shows, e.g. `Carentan`.
    pub name: String,
}

/// Every playable map this install offers, in a stable order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapRegistry {
    maps: Vec<PlayableMap>,
}

impl MapRegistry {
    /// Discover the playable maps from the mounted paks.
    ///
    /// A map qualifies when it is a multiplayer map (`mp_*`) that the paks can
    /// actually resolve and that is not on [`NOT_PLAYER_FACING`]. The result is
    /// sorted by id, so the order is deterministic across runs and the selector
    /// does not reshuffle when an archive is added.
    pub fn discover(fs: &Pk3Fs) -> MapRegistry {
        let mut maps: Vec<PlayableMap> = fs
            .find_maps()
            .into_iter()
            .filter(|id| id.starts_with("mp_"))
            .filter(|id| !NOT_PLAYER_FACING.contains(&id.as_str()))
            .filter(|id| fs.resolve_map(id).is_some())
            .map(|id| PlayableMap {
                name: display_name(&id),
                id,
            })
            .collect();
        maps.sort_by(|a, b| a.id.cmp(&b.id));
        maps.dedup_by(|a, b| a.id == b.id);
        MapRegistry { maps }
    }

    /// An explicit registry, for tests and for a caller that already knows the
    /// list. Ids are not re-validated here; discovery is what validates.
    pub fn from_ids(ids: &[&str]) -> MapRegistry {
        let mut maps: Vec<PlayableMap> = ids
            .iter()
            .filter(|id| !NOT_PLAYER_FACING.contains(id))
            .map(|id| PlayableMap {
                name: display_name(id),
                id: (*id).to_string(),
            })
            .collect();
        maps.sort_by(|a, b| a.id.cmp(&b.id));
        maps.dedup_by(|a, b| a.id == b.id);
        MapRegistry { maps }
    }

    pub fn is_empty(&self) -> bool {
        self.maps.is_empty()
    }

    pub fn len(&self) -> usize {
        self.maps.len()
    }

    pub fn maps(&self) -> &[PlayableMap] {
        &self.maps
    }

    /// The map with this id, if it is playable.
    pub fn get(&self, id: &str) -> Option<&PlayableMap> {
        self.maps.iter().find(|m| m.id == id)
    }

    /// Whether `id` is a map a player may choose. The selector, the rotation
    /// and a vote all validate through here, so a hand-edited rotation cannot
    /// load a map the selector would not offer.
    pub fn is_playable(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// The display names, for a selector's choices.
    pub fn names(&self) -> Vec<String> {
        self.maps.iter().map(|m| m.name.clone()).collect()
    }

    /// The index of a map id in [`Self::names`], for a selector's value.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.maps.iter().position(|m| m.id == id)
    }

    /// The id at a selector index, clamped into range.
    pub fn id_at(&self, index: usize) -> Option<&str> {
        self.maps.get(index).map(|m| m.id.as_str())
    }

    /// Pick up to `count` other maps, excluding `current`, for a vote or a
    /// rotation step. Deterministic: the registry's own order, offset so the
    /// same map is not always first. Returns fewer than `count` when the
    /// registry is small, and never includes `current`.
    pub fn candidates(&self, current: &str, count: usize, offset: usize) -> Vec<&PlayableMap> {
        let others: Vec<&PlayableMap> = self.maps.iter().filter(|m| m.id != current).collect();
        if others.is_empty() {
            return Vec::new();
        }
        let start = offset % others.len();
        (0..count.min(others.len()))
            .map(|i| others[(start + i) % others.len()])
            .collect()
    }
}

/// A friendly name for a map id, from the authored table where one exists and
/// from the id itself otherwise, so a map added to the install still reads
/// sensibly rather than as a raw id.
fn display_name(id: &str) -> String {
    let stem = id.strip_prefix("mp_").unwrap_or(id);
    if let Some((_, name)) = KNOWN_NAMES.iter().find(|(k, _)| *k == stem) {
        return (*name).to_string();
    }
    // No authored name: title-case the id's words, so `mp_new_map` reads
    // `New Map` rather than being hidden or shown as `new_map`.
    stem.split(['_', '-'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The stock maps' friendly names, keyed by the id's stem. Only the ones whose
/// display name is not the mechanical title-case of the id need an entry; the
/// rest fall through to [`display_name`]'s derivation.
const KNOWN_NAMES: &[(&str, &str)] = &[
    ("carentan", "Carentan"),
    ("dawnville", "Dawnville"),
    ("brecourt", "Brecourt"),
    ("pavlov", "Pavlov"),
    ("rocket", "Rocket"),
    ("ship", "Ship"),
    ("harbor", "Harbor"),
    ("hurtgen", "Hurtgen"),
    ("stalingrad", "Stalingrad"),
    ("tigertown", "Tiger Town"),
    ("powcamp", "POW Camp"),
    ("bocage", "Bocage"),
    ("depot", "Depot"),
    ("railyard", "Railyard"),
    ("neuville", "Neuville"),
    ("chateau", "Chateau"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_display_name_comes_from_the_table_or_the_id() {
        assert_eq!(display_name("mp_carentan"), "Carentan");
        assert_eq!(display_name("mp_pavlov"), "Pavlov");
        // Not in the table: derived, not dropped.
        assert_eq!(display_name("mp_new_map"), "New Map");
        assert_eq!(display_name("mp_toccoa"), "Toccoa");
    }

    /// The dev map never appears in a player-facing list, whichever way the
    /// registry is built.
    #[test]
    fn the_dev_map_is_never_playable() {
        let r = MapRegistry::from_ids(&["mp_toccoa", "mp_carentan"]);
        assert!(!r.is_playable("mp_toccoa"));
        assert!(r.is_playable("mp_carentan"));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn ids_sort_and_dedupe() {
        let r = MapRegistry::from_ids(&["mp_pavlov", "mp_carentan", "mp_pavlov"]);
        let ids: Vec<&str> = r.maps().iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["mp_carentan", "mp_pavlov"]);
        assert_eq!(r.index_of("mp_pavlov"), Some(1));
        assert_eq!(r.id_at(0), Some("mp_carentan"));
        assert_eq!(r.id_at(9), None);
    }

    /// A vote's candidates never include the current map, so the map that was
    /// just played does not reappear as an option.
    #[test]
    fn vote_candidates_exclude_the_current_map() {
        let r = MapRegistry::from_ids(&["mp_carentan", "mp_pavlov", "mp_harbor", "mp_ship"]);
        let c = r.candidates("mp_carentan", 3, 0);
        assert_eq!(c.len(), 3);
        assert!(c.iter().all(|m| m.id != "mp_carentan"));
        // The offset rotates which map leads, so the same map is not always
        // the first option.
        let c2 = r.candidates("mp_carentan", 3, 1);
        assert_ne!(c[0].id, c2[0].id);
    }

    /// Asking for more candidates than exist returns what there is, not a
    /// panic and not a repeat.
    #[test]
    fn more_candidates_than_maps_returns_what_there_is() {
        let r = MapRegistry::from_ids(&["mp_carentan", "mp_pavlov"]);
        let c = r.candidates("mp_carentan", 5, 0);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].id, "mp_pavlov");
    }

    /// A registry of one map has no candidates at all - the caller falls back
    /// to a replay rather than being handed the current map.
    #[test]
    fn a_single_map_registry_has_no_candidates() {
        let r = MapRegistry::from_ids(&["mp_carentan"]);
        assert!(r.candidates("mp_carentan", 3, 0).is_empty());
    }

    #[test]
    fn the_offered_gametypes_are_the_proven_ones() {
        let ids: Vec<&str> = GAMETYPES.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&"dm"), "dm is the baseline");
        // A gametype with no label cannot be shown.
        assert!(GAMETYPES.iter().all(|(_, label)| !label.is_empty()));
    }

    #[test]
    fn the_bot_bounds_are_sane() {
        assert!(MIN_BOTS < MAX_BOTS);
        assert!(MAX_BOTS <= 16, "a player-facing match is not a stress test");
    }
}
