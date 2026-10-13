//! The playable-map registry: one canonical source of truth for which maps
//! exist, which are playable, what a player should see them called, and who
//! made them.
//!
//! Everything that needs a map list reads it from here - the map selector, the
//! playlists, the rotation, and end-of-match map voting - so the four cannot
//! disagree about what is playable. Before this, each would have discovered
//! maps for itself and a map could be in the rotation but not the selector.
//!
//! The *inventory* is discovered from the mounted paks, not hard-coded: a
//! stock or community map is playable because it is in the install, not
//! because a switch lists it. What *is* authored is the metadata that cannot
//! be derived - the display name, the author credit, the description, the
//! source type and the verification flag - plus the denylist.

use std::path::Path;

use crate::pk3::Pk3Fs;

/// Maps that exist but must not appear in a player-facing list.
///
/// Re-add a map id here only while it is genuinely unplayable, and say what
/// is broken in this comment -- the list is the single gate the selector, the
/// rotation and a vote all read.
/// `mp_toccoa` remains an explicitly launchable internal development map, but
/// is not part of the player-facing alpha map set.
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

/// Where a map came from. The set is closed on purpose: a playlist filters on
/// it, and a new source is a deliberate addition, not an accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceType {
    /// A map that ships with retail Call of Duty (2003).
    StockCod1,
    /// A community-authored map in the CoD1 IBSP v59 format.
    CustomCod1,
    /// A map from a later Infinity Ward engine, loaded natively from its own
    /// data (MW2's `mp_rust` is the first).
    Iw4Native,
    /// Reserved for a future import path that is not yet implemented.
    FutureImport,
}

impl SourceType {
    /// The stable on-disk / wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceType::StockCod1 => "stock_cod1",
            SourceType::CustomCod1 => "custom_cod1",
            SourceType::Iw4Native => "iw4_native",
            SourceType::FutureImport => "future_import",
        }
    }

    /// Parse the stable name back.
    pub fn parse(s: &str) -> Option<SourceType> {
        [
            SourceType::StockCod1,
            SourceType::CustomCod1,
            SourceType::Iw4Native,
            SourceType::FutureImport,
        ]
        .into_iter()
        .find(|t| t.as_str() == s)
    }

    /// The "Source:" line the map-info panel shows. Credit for the map's
    /// origin, not a licence claim.
    pub fn label(self) -> &'static str {
        match self {
            SourceType::StockCod1 => "Official Call of Duty",
            SourceType::CustomCod1 => "CoD1 Community",
            SourceType::Iw4Native => "Modern Warfare 2",
            SourceType::FutureImport => "Imported",
        }
    }
}

/// One map a player may choose: the canonical id, the friendly name, and the
/// credit/metadata the UI and the vote read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayableMap {
    /// The canonical id the server and the rotation use, e.g. `mp_carentan`.
    pub id: String,
    /// What the selector shows, e.g. `Carentan`.
    pub name: String,
    /// A short original description, shown in the map-info panel. Empty when
    /// nothing has been authored.
    pub description: String,
    /// The map's author, or an empty string when no readme names one.
    pub author: String,
    /// Additional credits (co-authors, testers, texture sources).
    pub additional_credits: String,
    pub source_type: SourceType,
    /// The game the map shipped with, e.g. "Call of Duty".
    pub source_game: String,
    /// Where it came from, when known.
    pub source_url: String,
    /// Gametype ids the map's own readme documents support for.
    pub supported_gametypes: Vec<String>,
    /// Documented recommended player count, as text; empty when undocumented.
    pub recommended_players: String,
    pub tags: Vec<String>,
    /// Verified in a real runtime pass (world, textures, collision, spawn, DM,
    /// death, scoreboard, map change). A map that is not verified is kept out
    /// of every playlist that requires it.
    pub verified: bool,
    /// Free-form playlist tags from the metadata table, for future playlists.
    pub playlist_tags: Vec<String>,
}

impl PlayableMap {
    /// A minimal entry for a map with no authored metadata.
    fn bare(id: impl Into<String>, name: impl Into<String>, source_type: SourceType) -> Self {
        PlayableMap {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            author: String::new(),
            additional_credits: String::new(),
            source_type,
            source_game: String::new(),
            source_url: String::new(),
            supported_gametypes: Vec::new(),
            recommended_players: String::new(),
            tags: Vec::new(),
            verified: false,
            playlist_tags: Vec::new(),
        }
    }

    /// Whether the map carries a visible author credit.
    pub fn has_credit(&self) -> bool {
        !self.author.is_empty()
    }

    /// The label a map selector shows: the display name, plus a visible marker
    /// when the map has not been through a runtime verification pass. An
    /// available-but-unverified map is labelled rather than hidden, so a
    /// player can tell the two apart without the map vanishing from its own
    /// source playlist.
    pub fn playlist_label(&self) -> String {
        if self.verified {
            self.name.clone()
        } else {
            format!("{} [ALPHA / UNVERIFIED]", self.name)
        }
    }
}

/// The credit shown when a readme names no author. Never invented: a map with
/// an unreadable readme is credited to nobody rather than to a guess.
pub const UNKNOWN_AUTHOR: &str = "Unknown / unverified";

/// One authored metadata record, keyed by the canonical map id. Only the
/// fields that cannot be derived are here.
struct MapMeta {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    author: &'static str,
    additional_credits: &'static str,
    source_type: SourceType,
    source_game: &'static str,
    source_url: &'static str,
    gametypes: &'static [&'static str],
    recommended_players: &'static str,
    tags: &'static [&'static str],
    verified: bool,
    playlist_tags: &'static [&'static str],
}

/// A stock-map entry: name, region tag, and the proven gametypes. Every stock
/// map is verified - they are the baseline every run is measured against.
///
/// A macro rather than a `const fn`: `tags: &[region]` inside a function
/// borrows a local, which is not `'static`; the macro splices the literal in
/// so every field is a genuine static.
macro_rules! stock {
    ($id:literal, $name:literal, $region:literal, $gt:expr) => {
        MapMeta {
            id: $id,
            name: $name,
            description: "",
            author: "Infinity Ward",
            additional_credits: "",
            source_type: SourceType::StockCod1,
            source_game: "Call of Duty",
            source_url: "",
            gametypes: $gt,
            recommended_players: "",
            tags: &[$region],
            verified: true,
            playlist_tags: &["classic"],
        }
    };
}

/// The authored metadata table. A stock map's entry carries only its name and
/// source; a community map carries its credit. A map with no entry at all is
/// still playable if the paks resolve it - it is just unverified and
/// uncredited, and only appears where a playlist does not require verification.
const MAP_META: &[MapMeta] = &[
    // ---- Stock Call of Duty (2003) --------------------------------------
    stock!("mp_carentan", "Carentan", "Normandy", &["dm", "tdm"]),
    stock!("mp_dawnville", "Dawnville", "Normandy", &["dm", "tdm"]),
    stock!("mp_brecourt", "Brecourt", "Normandy", &["dm", "tdm"]),
    stock!("mp_pavlov", "Pavlov", "Stalingrad", &["dm", "tdm"]),
    stock!("mp_rocket", "Rocket", "Stalingrad", &["dm", "tdm"]),
    stock!("mp_ship", "Ship", "Atlantic", &["dm", "tdm"]),
    stock!("mp_harbor", "Harbor", "Atlantic", &["dm", "tdm"]),
    stock!("mp_hurtgen", "Hurtgen", "Hurtgen Forest", &["dm", "tdm"]),
    stock!("mp_stalingrad", "Stalingrad", "Stalingrad", &["dm", "tdm"]),
    stock!("mp_tigertown", "Tiger Town", "Normandy", &["dm", "tdm"]),
    stock!("mp_powcamp", "POW Camp", "Germany", &["dm", "tdm"]),
    stock!("mp_bocage", "Bocage", "Normandy", &["dm", "tdm"]),
    stock!("mp_depot", "Depot", "Germany", &["dm", "tdm"]),
    stock!("mp_railyard", "Railyard", "Germany", &["dm", "tdm"]),
    stock!("mp_neuville", "Neuville", "Normandy", &["dm", "tdm"]),
    stock!("mp_chateau", "Chateau", "France", &["dm", "tdm"]),
    // ---- Community Call of Duty 1 maps ----------------------------------
    MapMeta {
        id: "mp_stanjel",
        name: "Stanjel",
        description: "A tight Slovenian hill village of stone lanes, terraces and \
                      walled gardens, favouring close quarters and short sightlines.",
        author: "Roger Abrahamsson",
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community", "village", "close-quarters"],
        // Verified in a runtime pass (loads, renders, collision, DM).
        verified: true,
        playlist_tags: &["community_classics"],
    },
    MapMeta {
        id: "mp_merville",
        name: "Merville",
        description: "A community map built around a battery and its surrounding \
                      village, with open approaches and interior fighting.",
        author: UNKNOWN_AUTHOR,
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community"],
        verified: false,
        playlist_tags: &["community_classics"],
    },
    MapMeta {
        id: "mp_offensive",
        name: "Offensive",
        description: "A community infantry map of streets, rubble and covered \
                      advances.",
        author: UNKNOWN_AUTHOR,
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community"],
        verified: false,
        playlist_tags: &["community_classics"],
    },
    MapMeta {
        id: "mp_saint-lo",
        name: "Saint-Lo",
        description: "A community re-creation of the ruined Norman town, mixing \
                      open squares with close street fighting.",
        author: UNKNOWN_AUTHOR,
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community", "urban"],
        verified: false,
        playlist_tags: &["community_classics"],
    },
    MapMeta {
        id: "nuenen",
        name: "Nuenen",
        description: "A community map of a Dutch village, with a mix of open \
                      ground and built-up fighting.",
        author: UNKNOWN_AUTHOR,
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community", "village"],
        verified: false,
        playlist_tags: &["community_classics"],
    },
    MapMeta {
        id: "univermag",
        name: "Univermag",
        description: "A community map built around a department store and the \
                      streets around it, favouring interior engagements.",
        author: UNKNOWN_AUTHOR,
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community", "urban"],
        verified: false,
        playlist_tags: &["community_classics"],
    },
    MapMeta {
        id: "the_hunt",
        name: "The Hunt",
        description: "A community map of broken ground and cover, built for \
                      mid-range fighting.",
        author: UNKNOWN_AUTHOR,
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community"],
        verified: false,
        playlist_tags: &["community_classics"],
    },
    MapMeta {
        id: "entrenchments",
        name: "Entrenchments",
        description: "A community map of trench lines and dug-in positions, \
                      favouring cover-to-cover advances.",
        author: UNKNOWN_AUTHOR,
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community", "trenches"],
        verified: false,
        playlist_tags: &["community_classics"],
    },
    MapMeta {
        id: "pestilence",
        name: "Pestilence",
        description: "A community map of a bombed settlement, with rubble, \
                      cellars and narrow approaches.",
        author: UNKNOWN_AUTHOR,
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community"],
        verified: false,
        playlist_tags: &["community_classics"],
    },
    MapMeta {
        id: "mohsh_berlin_am",
        name: "Berlin (American)",
        description: "A community Berlin map, one of three nationality variants.",
        author: UNKNOWN_AUTHOR,
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community", "urban"],
        verified: false,
        playlist_tags: &["community_classics"],
    },
    MapMeta {
        id: "mohsh_berlin_bri",
        name: "Berlin (British)",
        description: "A community Berlin map, one of three nationality variants.",
        author: UNKNOWN_AUTHOR,
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community", "urban"],
        verified: false,
        playlist_tags: &["community_classics"],
    },
    MapMeta {
        id: "mohsh_berlin_rus",
        name: "Berlin (Russian)",
        description: "A community Berlin map, one of three nationality variants.",
        author: UNKNOWN_AUTHOR,
        additional_credits: "",
        source_type: SourceType::CustomCod1,
        source_game: "Call of Duty",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["community", "urban"],
        verified: false,
        playlist_tags: &["community_classics"],
    },
    // ---- Later-engine maps (loaded natively) ----------------------------
    MapMeta {
        id: "mp_rust",
        name: "Rust",
        description: "A compact industrial battlefield built around a central \
                      elevated structure, favouring fast close-range engagements \
                      and vertical movement.",
        // The map's creators, not this project's. Credited as the publisher/
        // developer of the source game; individual level designers are not
        // named in any shipped file.
        author: "Infinity Ward",
        additional_credits: "Published by Activision. Loaded from the local \
                             Call of Duty: Modern Warfare 2 install.",
        source_type: SourceType::Iw4Native,
        source_game: "Call of Duty: Modern Warfare 2",
        source_url: "",
        gametypes: &["dm", "tdm"],
        recommended_players: "",
        tags: &["iw", "industrial", "close-quarters"],
        // Geometry, spawn and collision are verified. Keep this false until
        // the complete normal multiplayer visual/lifecycle gate passes.
        verified: false,
        playlist_tags: &["iw_classics"],
    },
];

/// The authored metadata for a map id, if any.
fn meta_for(id: &str) -> Option<&'static MapMeta> {
    MAP_META.iter().find(|m| m.id == id)
}

/// How many authored metadata records exist. Exposed so a caller (and a test)
/// can reason about the table without duplicating its contents.
pub const MAP_META_COUNT: usize = MAP_META.len();

/// A friendly name for a map id, from the authored table where one exists and
/// from the id itself otherwise, so a map added to the install still reads
/// sensibly rather than as a raw id.
fn display_name(id: &str) -> String {
    if let Some(m) = meta_for(id) {
        return m.name.to_string();
    }
    let stem = id.strip_prefix("mp_").unwrap_or(id);
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

/// Build a [`PlayableMap`] for an id, filling the authored metadata where it
/// exists. `fallback_source` is what an unauthored map is classified as.
fn entry_for(id: &str, fallback_source: SourceType) -> PlayableMap {
    match meta_for(id) {
        Some(m) => PlayableMap {
            id: m.id.to_string(),
            name: m.name.to_string(),
            description: m.description.to_string(),
            author: m.author.to_string(),
            additional_credits: m.additional_credits.to_string(),
            source_type: m.source_type,
            source_game: m.source_game.to_string(),
            source_url: m.source_url.to_string(),
            supported_gametypes: m.gametypes.iter().map(|s| (*s).to_string()).collect(),
            recommended_players: m.recommended_players.to_string(),
            tags: m.tags.iter().map(|s| (*s).to_string()).collect(),
            verified: m.verified,
            playlist_tags: m.playlist_tags.iter().map(|s| (*s).to_string()).collect(),
        },
        None => PlayableMap::bare(id, display_name(id), fallback_source),
    }
}

/// A playlist: a named, data-driven map pool with its own gametypes, bot
/// default and vote setting. Rules live here, not inside a menu function, so
/// adding a playlist is a table entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Playlist {
    /// The stable id, e.g. `quick_play`.
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    /// Which map sources the pool draws from.
    pub sources: &'static [SourceType],
    /// The gametype ids this playlist offers, in order.
    pub gametypes: &'static [&'static str],
    /// The bot count the Play screen opens on for this playlist.
    pub default_bots: usize,
    pub map_vote_enabled: bool,
    /// Whether a map must be verified to appear in this playlist.
    ///
    /// This gates the *curated* pools (Quick Play, the friends release): a map
    /// that has not been through a runtime pass is kept out. A dedicated
    /// source playlist (`classic`, `community_classics`, `iw_classics`) sets
    /// this false so every *available* map from its own source is exposed,
    /// verified or not - an installed map is never silently dropped from its
    /// own playlist. Verification then shows up as the `[ALPHA / UNVERIFIED]`
    /// marker from [`PlayableMap::playlist_label`] rather than as removal.
    pub require_verified: bool,
}

/// The playlists, in the order the Play screen offers them. Only playlists
/// that have at least one verified map are populated by content; the rules
/// are fixed here regardless.
pub const PLAYLISTS: &[Playlist] = &[
    Playlist {
        id: "classic",
        display_name: "Classic",
        description: "Stock Call of Duty maps only.",
        sources: &[SourceType::StockCod1],
        gametypes: &["dm", "tdm"],
        default_bots: 8,
        map_vote_enabled: true,
        // Stock maps are the verified baseline, but the playlist still shows
        // every stock map that is installed rather than gating on the flag.
        require_verified: false,
    },
    Playlist {
        id: "community_classics",
        display_name: "Community Classics",
        description: "Call of Duty 1 community maps.",
        sources: &[SourceType::CustomCod1],
        gametypes: &["dm", "tdm"],
        default_bots: 8,
        map_vote_enabled: true,
        require_verified: false,
    },
    Playlist {
        id: "quick_play",
        display_name: "Quick Play",
        description: "Stock maps plus verified community maps.",
        sources: &[SourceType::StockCod1, SourceType::CustomCod1],
        gametypes: &["dm", "tdm"],
        default_bots: 8,
        map_vote_enabled: true,
        require_verified: true,
    },
    Playlist {
        id: "iw_classics",
        display_name: "IW Classics",
        description: "Maps from later Infinity Ward titles.",
        sources: &[SourceType::Iw4Native],
        gametypes: &["dm", "tdm"],
        default_bots: 8,
        map_vote_enabled: true,
        require_verified: false,
    },
];

/// The playlist with this id, if it exists.
pub fn playlist_by_id(id: &str) -> Option<&'static Playlist> {
    PLAYLISTS.iter().find(|p| p.id == id)
}

/// The maps from later-engine titles that this build knows how to load
/// natively, in the order they are offered. Each is only *playable* when its
/// source game's data is actually present on this machine; the registry is
/// told that by the caller rather than assuming it.
pub const IW4_KNOWN_MAPS: &[&str] = &["mp_rust"];

/// Every playable map this install offers, in a stable order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapRegistry {
    maps: Vec<PlayableMap>,
}

impl MapRegistry {
    /// Discover the playable maps from the mounted paks.
    ///
    /// A map qualifies when it is a multiplayer map (`mp_*`) that the paks can
    /// actually resolve and that is not on [`NOT_PLAYER_FACING`]. Later-engine
    /// maps are not in the paks; [`Self::include_iw4`] adds them when their
    /// source install is present. The result is sorted by id, so the order is
    /// deterministic across runs and the selector does not reshuffle when an
    /// archive is added.
    pub fn discover(fs: &Pk3Fs) -> MapRegistry {
        let mut maps: Vec<PlayableMap> = fs
            .find_maps()
            .into_iter()
            .filter(|id| id.starts_with("mp_"))
            .filter(|id| !NOT_PLAYER_FACING.contains(&id.as_str()))
            .filter(|id| fs.resolve_map(id).is_some())
            .map(|id| {
                // A map with no metadata entry is treated as a community map:
                // it came from a pak we do not recognise, not from retail.
                entry_for(&id, SourceType::CustomCod1)
            })
            .collect();
        maps.sort_by(|a, b| a.id.cmp(&b.id));
        maps.dedup_by(|a, b| a.id == b.id);
        MapRegistry { maps }
    }

    /// Add the later-engine maps whose source data is present locally.
    ///
    /// `available` answers whether a given known map's data is loadable (the
    /// caller checks the install; the registry does not read the disk). A map
    /// that is already present is left alone, so this is safe to call again
    /// after a download reopened the paks.
    pub fn include_iw4(&mut self, available: impl Fn(&str) -> bool) {
        for &id in IW4_KNOWN_MAPS {
            if !available(id) || self.maps.iter().any(|m| m.id == id) {
                continue;
            }
            self.maps.push(entry_for(id, SourceType::Iw4Native));
        }
        self.maps.sort_by(|a, b| a.id.cmp(&b.id));
        self.maps.dedup_by(|a, b| a.id == b.id);
    }

    /// An explicit registry, for tests and for a caller that already knows the
    /// list. Ids are not re-validated here; discovery is what validates.
    pub fn from_ids(ids: &[&str]) -> MapRegistry {
        let mut maps: Vec<PlayableMap> = ids
            .iter()
            .filter(|id| !NOT_PLAYER_FACING.contains(id))
            .map(|id| entry_for(id, SourceType::CustomCod1))
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

    /// The maps a playlist offers, in registry order. A playlist that
    /// requires verification drops a map that has not been through a real
    /// runtime pass, which is what keeps an untested map out of Quick Play.
    pub fn in_playlist(&self, playlist: &Playlist) -> Vec<&PlayableMap> {
        self.maps
            .iter()
            .filter(|m| playlist.sources.contains(&m.source_type))
            .filter(|m| !playlist.require_verified || m.verified)
            .collect()
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

    /// Vote candidates restricted to one playlist's pool. The current map is
    /// excluded, as in [`Self::candidates`], and the offset rotates the offer.
    pub fn candidates_in_playlist(
        &self,
        playlist: &Playlist,
        current: &str,
        count: usize,
        offset: usize,
    ) -> Vec<&PlayableMap> {
        let pool: Vec<&PlayableMap> = self
            .in_playlist(playlist)
            .into_iter()
            .filter(|m| m.id != current)
            .collect();
        if pool.is_empty() {
            return Vec::new();
        }
        let start = offset % pool.len();
        (0..count.min(pool.len()))
            .map(|i| pool[(start + i) % pool.len()])
            .collect()
    }
}

/// Where the local Modern Warfare 2 install lives, if it can be found. Read
/// only: the caller never writes into it. `None` means the later-engine maps
/// are not available on this machine.
pub fn find_mw2_install() -> Option<std::path::PathBuf> {
    const CANDIDATES: &[&str] = &[
        "C:/Program Files (x86)/Steam/steamapps/common/Call of Duty Modern Warfare 2",
        "C:/Program Files/Steam/steamapps/common/Call of Duty Modern Warfare 2",
        "D:/Steam/steamapps/common/Call of Duty Modern Warfare 2",
    ];
    for c in CANDIDATES {
        let p = Path::new(c);
        if p.join("zone").is_dir() && p.join("iw4mp.exe").is_file() {
            return Some(p.to_path_buf());
        }
    }
    None
}

/// Whether a known later-engine map's zone data is present under `install`.
pub fn iw4_map_available(install: &Path, id: &str) -> bool {
    install
        .join("zone")
        .join("english")
        .join(format!("{id}.ff"))
        .is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_display_name_comes_from_the_table_or_the_id() {
        assert_eq!(display_name("mp_carentan"), "Carentan");
        assert_eq!(display_name("mp_pavlov"), "Pavlov");
        assert_eq!(display_name("mp_stanjel"), "Stanjel");
        // Not in the table: derived, not dropped.
        assert_eq!(display_name("mp_new_map"), "New Map");
        assert_eq!(display_name("mp_toccoa"), "Toccoa");
    }

    /// The dev map stays launchable by id but is not offered by normal
    /// player-facing selection, rotation, or voting.
    #[test]
    fn the_dev_map_is_not_player_facing() {
        let r = MapRegistry::from_ids(&["mp_toccoa", "mp_carentan"]);
        assert!(!r.is_playable("mp_toccoa"));
        assert!(r.is_playable("mp_carentan"));
        assert_eq!(r.len(), 1);
    }

    /// The gate itself still works, so re-adding an id really does hide it.
    #[test]
    fn a_map_on_the_hidden_list_is_filtered_out() {
        // NOT_PLAYER_FACING is a const; assert the filter reads it by checking
        // the source-of-truth list is what from_ids consults.
        for id in NOT_PLAYER_FACING {
            let r = MapRegistry::from_ids(&[id, "mp_carentan"]);
            assert!(!r.is_playable(id), "{id} must stay hidden");
            assert!(r.is_playable("mp_carentan"));
        }
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

    /// A stock map is verified and carries the stock source; a community map
    /// is not stock, and an authored one carries its author.
    #[test]
    fn metadata_classifies_stock_and_community_maps() {
        let r = MapRegistry::from_ids(&["mp_carentan", "mp_stanjel", "mp_unknown_map"]);
        let carentan = r.get("mp_carentan").unwrap();
        assert_eq!(carentan.source_type, SourceType::StockCod1);
        assert!(carentan.verified);

        let stanjel = r.get("mp_stanjel").unwrap();
        assert_eq!(stanjel.source_type, SourceType::CustomCod1);
        assert_eq!(stanjel.author, "Roger Abrahamsson");
        assert!(stanjel.has_credit());

        // No entry: still playable, but uncredited and unverified.
        let unknown = r.get("mp_unknown_map").unwrap();
        assert_eq!(unknown.source_type, SourceType::CustomCod1);
        assert_eq!(unknown.author, "");
        assert!(!unknown.has_credit());
        assert!(!unknown.verified);
    }

    /// Dedicated source playlists expose every available map from their own
    /// source, verified or not; only the curated pool requires verification.
    #[test]
    fn dedicated_playlists_expose_available_maps_and_quick_play_requires_verification() {
        let r = MapRegistry::from_ids(&[
            "mp_carentan",
            "mp_stanjel",
            "mp_unverified_community",
            "mp_rust",
        ]);
        let classic = playlist_by_id("classic").unwrap();
        let community = playlist_by_id("community_classics").unwrap();
        let quick = playlist_by_id("quick_play").unwrap();
        let iw = playlist_by_id("iw_classics").unwrap();

        let ids = |p: &Playlist| -> Vec<String> {
            r.in_playlist(p).iter().map(|m| m.id.clone()).collect()
        };
        assert_eq!(ids(classic), vec!["mp_carentan"]);
        // The dedicated community playlist keeps its unverified maps visible:
        // an installed map is never silently removed from its own source pool.
        assert_eq!(
            ids(community),
            vec!["mp_stanjel", "mp_unverified_community"]
        );
        // Quick Play is the curated pool, so verification still gates it.
        assert_eq!(ids(quick), vec!["mp_carentan", "mp_stanjel"]);
        // IW Classics exposes the installed later-engine map while unverified.
        assert_eq!(ids(iw), vec!["mp_rust"]);
    }

    /// An unverified map is shown with a visible marker so a player can tell
    /// it apart from a verified one without it being hidden.
    #[test]
    fn an_unverified_map_carries_a_visible_alpha_marker() {
        let r = MapRegistry::from_ids(&["mp_rust", "mp_carentan"]);
        let rust = r.get("mp_rust").unwrap();
        assert!(!rust.verified);
        assert_eq!(rust.playlist_label(), "Rust [ALPHA / UNVERIFIED]");
        let carentan = r.get("mp_carentan").unwrap();
        assert!(carentan.verified);
        assert_eq!(carentan.playlist_label(), "Carentan");
    }

    /// The vote pool respects the playlist: no cross-playlist leakage.
    #[test]
    fn vote_candidates_respect_the_playlist() {
        let r = MapRegistry::from_ids(&["mp_carentan", "mp_stanjel", "mp_rust"]);
        let quick = playlist_by_id("quick_play").unwrap();
        let c = r.candidates_in_playlist(quick, "mp_carentan", 5, 0);
        assert!(c.iter().all(|m| m.source_type != SourceType::Iw4Native));
        assert!(c.iter().all(|m| m.id != "mp_carentan"));

        let classic = playlist_by_id("classic").unwrap();
        let c = r.candidates_in_playlist(classic, "mp_carentan", 5, 0);
        assert!(c.is_empty(), "the only stock map is the current one");
    }

    /// `include_iw4` adds only the maps the caller says are present, and does
    /// not duplicate one already in the registry.
    #[test]
    fn include_iw4_respects_availability_and_dedupes() {
        let mut r = MapRegistry::from_ids(&["mp_carentan"]);
        r.include_iw4(|_| false);
        assert!(r.get("mp_rust").is_none(), "not present, not added");

        r.include_iw4(|id| id == "mp_rust");
        assert!(r.get("mp_rust").is_some());
        let before = r.len();
        r.include_iw4(|_| true);
        assert_eq!(r.len(), before, "adding twice does not duplicate");
    }

    /// Every playlist id is unique and every playlist offers at least one
    /// gametype, so the Play screen can never open on an empty mode list.
    #[test]
    fn playlists_are_well_formed() {
        for (i, p) in PLAYLISTS.iter().enumerate() {
            assert!(!p.display_name.is_empty());
            assert!(!p.gametypes.is_empty(), "{}: no gametypes", p.id);
            assert!(
                PLAYLISTS[..i].iter().all(|q| q.id != p.id),
                "duplicate playlist id {}",
                p.id
            );
        }
        assert!(playlist_by_id("quick_play").is_some());
        assert!(playlist_by_id("nope").is_none());
    }

    /// The MW2 detector does not panic when nothing is installed, and the
    /// availability check reads a path under the install without touching it.
    #[test]
    fn the_mw2_probe_is_read_only_and_total() {
        let _ = find_mw2_install();
        assert!(!iw4_map_available(Path::new("Z:/nope"), "mp_rust"));
    }
}
