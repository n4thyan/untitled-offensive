//! End-of-match map voting, server-authoritative.
//!
//! The server picks the candidate maps, counts the votes and loads the winner;
//! a client only ever submits its own selection. Bots do not vote, a human gets
//! one vote, and a tie is broken deterministically so the same inputs always
//! produce the same map - a random tiebreak would make a match unreproducible
//! from its log.
//!
//! The vote is a thin layer over the rotation, not a second map system: the
//! winner is fed back through the ordinary `map <id>` console path, and the
//! candidate list comes from the same [`MapRegistry`] the selector and the
//! rotation read.

use std::collections::BTreeMap;

use crate::maplist::MapRegistry;

/// How long a vote window stays open, in server milliseconds.
pub const VOTE_WINDOW_MS: i32 = 15_000;

/// One ballot: the candidate maps offered, and who voted for which.
#[derive(Debug, Default)]
pub struct Vote {
    /// The candidate ids, in the order they are shown. Empty when no vote is
    /// running.
    candidates: Vec<String>,
    /// Client slot -> candidate index. One entry per human who has voted.
    votes: BTreeMap<usize, usize>,
    /// The server time the window opened, and when it closes.
    opened_ms: i32,
    deadline_ms: i32,
    /// The vote is over and its result has been taken.
    resolved: bool,
}

/// The outcome of a finished vote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Result {
    /// The map to load, and the tally that chose it.
    Map(String, Vec<(String, usize)>),
    /// Nobody voted: replay the current map.
    Replay,
}

impl Vote {
    /// Whether a vote is running.
    pub fn is_open(&self) -> bool {
        !self.candidates.is_empty() && !self.resolved
    }

    pub fn candidates(&self) -> &[String] {
        &self.candidates
    }

    pub fn deadline_ms(&self) -> i32 {
        self.deadline_ms
    }

    /// How many humans have voted.
    pub fn vote_count(&self) -> usize {
        self.votes.len()
    }

    /// Open a vote over `candidates` at `now_ms`. A list with fewer than two
    /// entries is not a vote - a "choice" between one map and itself is not a
    /// choice - so this leaves the vote closed and the caller falls back to a
    /// replay.
    pub fn open(&mut self, candidates: Vec<String>, now_ms: i32) {
        if candidates.len() < 2 {
            self.clear();
            return;
        }
        self.candidates = candidates;
        self.votes.clear();
        self.opened_ms = now_ms;
        self.deadline_ms = now_ms.wrapping_add(VOTE_WINDOW_MS);
        self.resolved = false;
    }

    /// Open a vote from a registry, excluding the current map. Returns whether
    /// a vote actually opened.
    pub fn open_from_registry(
        &mut self,
        registry: &MapRegistry,
        current: &str,
        count: usize,
        now_ms: i32,
    ) -> bool {
        // The offset rotates by the current map's own index, so a rotation
        // through the same map set does not offer the same three every time.
        let offset = registry.index_of(current).unwrap_or(0);
        let candidates: Vec<String> = registry
            .candidates(current, count, offset)
            .into_iter()
            .map(|m| m.id.clone())
            .collect();
        self.open(candidates, now_ms);
        self.is_open()
    }

    /// Open a vote from a registry, restricted to one playlist's pool. Same
    /// rules as [`Self::open_from_registry`], but the candidates come only
    /// from maps the playlist offers - a Quick Play vote never offers an IW
    /// map, and a Classic vote never offers a community one.
    pub fn open_from_playlist(
        &mut self,
        registry: &MapRegistry,
        playlist: &crate::maplist::Playlist,
        current: &str,
        count: usize,
        now_ms: i32,
    ) -> bool {
        let offset = registry.index_of(current).unwrap_or(0);
        let candidates: Vec<String> = registry
            .candidates_in_playlist(playlist, current, count, offset)
            .into_iter()
            .map(|m| m.id.clone())
            .collect();
        self.open(candidates, now_ms);
        self.is_open()
    }

    pub fn clear(&mut self) {
        *self = Vote::default();
    }

    /// Record a human's vote for a candidate index. One vote per slot; a
    /// second vote from the same slot replaces the first (changing your mind
    /// inside the window is allowed). An out-of-range index is refused.
    pub fn cast(&mut self, slot: usize, index: usize) -> bool {
        if !self.is_open() || index >= self.candidates.len() {
            return false;
        }
        self.votes.insert(slot, index);
        true
    }

    /// The candidate a slot voted for, if any.
    pub fn vote_of(&self, slot: usize) -> Option<usize> {
        self.votes.get(&slot).copied()
    }

    /// Whether `now_ms` is past the deadline.
    pub fn expired(&self, now_ms: i32) -> bool {
        self.is_open() && now_ms.wrapping_sub(self.deadline_ms) >= 0
    }

    /// The tally, in candidate order: `(id, votes)`.
    pub fn tally(&self) -> Vec<(String, usize)> {
        let mut counts = vec![0usize; self.candidates.len()];
        for &index in self.votes.values() {
            if let Some(c) = counts.get_mut(index) {
                *c += 1;
            }
        }
        self.candidates
            .iter()
            .cloned()
            .zip(counts)
            .collect::<Vec<_>>()
    }

    /// Resolve the vote: the winning map, or [`Result::Replay`] when nobody
    /// voted. Marks the vote resolved so it is only taken once.
    ///
    /// A tie is broken by the candidate order, which the server chose
    /// deterministically - so the result is a pure function of the ballots,
    /// not of wall-clock timing or hash iteration order.
    pub fn resolve(&mut self) -> Result {
        self.resolved = true;
        let tally = self.tally();
        if tally.iter().all(|(_, n)| *n == 0) {
            return Result::Replay;
        }
        let winner = tally
            .iter()
            .enumerate()
            .max_by_key(|(i, (_, n))| (*n, std::cmp::Reverse(*i)))
            .map(|(_, (id, _))| id.clone());
        match winner {
            Some(id) => Result::Map(id, tally),
            None => Result::Replay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates() -> Vec<String> {
        vec!["mp_pavlov".into(), "mp_harbor".into(), "mp_brecourt".into()]
    }

    #[test]
    fn a_vote_needs_at_least_two_candidates() {
        let mut v = Vote::default();
        v.open(vec!["mp_carentan".into()], 0);
        assert!(!v.is_open(), "one map is not a choice");
        v.open(candidates(), 0);
        assert!(v.is_open());
    }

    /// One vote per human, and changing your mind replaces it rather than
    /// stacking.
    #[test]
    fn one_vote_per_slot_and_a_change_replaces_it() {
        let mut v = Vote::default();
        v.open(candidates(), 0);
        assert!(v.cast(3, 0));
        assert!(v.cast(3, 1), "changing a vote is allowed");
        assert_eq!(v.vote_of(3), Some(1));
        assert_eq!(v.vote_count(), 1, "still one vote from that slot");
        assert_eq!(v.tally()[0].1, 0);
        assert_eq!(v.tally()[1].1, 1);
    }

    /// An out-of-range candidate index is refused, and a vote cast before the
    /// window opens or after it closes does nothing.
    #[test]
    fn an_invalid_or_late_vote_is_refused() {
        let mut v = Vote::default();
        assert!(!v.cast(1, 0), "no vote open");
        v.open(candidates(), 0);
        assert!(!v.cast(1, 9), "out of range");
        assert_eq!(v.vote_count(), 0);
        assert!(v.cast(1, 0));
        // Past the window.
        assert!(v.expired(VOTE_WINDOW_MS + 1));
    }

    /// Nobody voting is a replay, not the first candidate winning by default.
    #[test]
    fn no_votes_is_a_replay() {
        let mut v = Vote::default();
        v.open(candidates(), 0);
        assert_eq!(v.resolve(), Result::Replay);
    }

    /// The most-voted map wins.
    #[test]
    fn the_most_voted_map_wins() {
        let mut v = Vote::default();
        v.open(candidates(), 0);
        v.cast(1, 0);
        v.cast(2, 2);
        v.cast(3, 2);
        match v.resolve() {
            Result::Map(id, tally) => {
                assert_eq!(id, "mp_brecourt");
                assert_eq!(tally[2], ("mp_brecourt".to_string(), 2));
            }
            other => panic!("expected a map, got {other:?}"),
        }
    }

    /// A tie breaks by candidate order, deterministically: the same ballots
    /// always give the same map.
    #[test]
    fn a_tie_breaks_deterministically() {
        let result = || {
            let mut v = Vote::default();
            v.open(candidates(), 0);
            v.cast(1, 0);
            v.cast(2, 1);
            v.resolve()
        };
        let first = result();
        assert_eq!(first, result(), "the same ballots give the same winner");
        match first {
            Result::Map(id, _) => assert_eq!(id, "mp_pavlov", "the earlier candidate"),
            other => panic!("expected a map, got {other:?}"),
        }
    }

    /// A vote resolves once: a second resolve does not re-take it as open.
    #[test]
    fn a_resolved_vote_is_no_longer_open() {
        let mut v = Vote::default();
        v.open(candidates(), 0);
        v.cast(1, 0);
        v.resolve();
        assert!(!v.is_open());
        assert!(!v.cast(2, 1), "a closed vote takes no more ballots");
    }

    /// The registry path excludes the current map, so the map just played is
    /// not on the ballot.
    #[test]
    fn the_ballot_excludes_the_current_map() {
        let r = MapRegistry::from_ids(&["mp_carentan", "mp_pavlov", "mp_harbor", "mp_ship"]);
        let mut v = Vote::default();
        assert!(v.open_from_registry(&r, "mp_carentan", 3, 0));
        assert!(v.is_open());
        assert!(v.candidates().iter().all(|c| c != "mp_carentan"));
        assert_eq!(v.candidates().len(), 3);
    }

    /// A registry too small to offer a choice leaves the vote closed, and the
    /// caller replays.
    #[test]
    fn a_registry_too_small_leaves_the_vote_closed() {
        let r = MapRegistry::from_ids(&["mp_carentan"]);
        let mut v = Vote::default();
        assert!(!v.open_from_registry(&r, "mp_carentan", 3, 0));
        assert!(!v.is_open());
    }

    /// The tally is in candidate order and counts every slot exactly once.
    #[test]
    fn the_tally_is_in_candidate_order() {
        let mut v = Vote::default();
        v.open(candidates(), 0);
        v.cast(1, 1);
        v.cast(2, 1);
        v.cast(3, 0);
        assert_eq!(
            v.tally(),
            vec![
                ("mp_pavlov".to_string(), 1),
                ("mp_harbor".to_string(), 2),
                ("mp_brecourt".to_string(), 0),
            ]
        );
    }
}
