//! The end-of-match map-vote panel: the candidates the server offered, the
//! player's own vote, and the countdown to the deadline.
//!
//! The server owns the vote ([`vcod_common::mapvote`]); this only shows it and
//! sends back an index. It is an overlay during the intermission, not a modal
//! menu: the player keeps playing (or spectating) underneath it.

use super::font::{self, Font};
use super::HudQuad;

/// The panel's own state, rebuilt from the server's `mapvote` commands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VotePanel {
    /// The candidate map ids, in the order the server listed them. Empty when
    /// no vote is open.
    candidates: Vec<String>,
    /// The server-clock ms the vote closes at.
    deadline_ms: i32,
    /// The index this client voted for, if any.
    chosen: Option<usize>,
}

impl VotePanel {
    pub fn is_open(&self) -> bool {
        !self.candidates.is_empty()
    }

    pub fn candidates(&self) -> &[String] {
        &self.candidates
    }

    pub fn chosen(&self) -> Option<usize> {
        self.chosen
    }

    /// The server-clock ms the vote closes at. Used by the countdown and by
    /// tests; kept public so a caller can show a different clock format.
    #[allow(dead_code)]
    pub fn deadline_ms(&self) -> i32 {
        self.deadline_ms
    }

    /// Parse a `mapvote` serverCommand: `mapvote <deadline_ms> <candidate>...`,
    /// or `mapvote close`. Returns whether the command was the vote's.
    ///
    /// The candidates are map ids; the panel title-cases them for display via
    /// the registry when it has one, and falls back to the raw id otherwise.
    pub fn on_server_command(&mut self, tokens: &[String]) -> bool {
        if tokens.first().map(String::as_str) != Some("mapvote") {
            return false;
        }
        match tokens.get(1).map(String::as_str) {
            Some("close") | None => {
                self.candidates.clear();
                self.chosen = None;
                self.deadline_ms = 0;
            }
            Some(deadline) => {
                let deadline = deadline.parse::<i32>().unwrap_or(0);
                let candidates: Vec<String> = tokens[2..].to_vec();
                // A repeat of the same ballot keeps the player's own vote; a
                // different ballot (a new vote) clears it.
                if candidates != self.candidates {
                    self.chosen = None;
                }
                self.candidates = candidates;
                self.deadline_ms = deadline;
            }
        }
        true
    }

    /// Record the player's choice locally, so the panel can mark it. The
    /// authoritative record is the server's.
    pub fn choose(&mut self, index: usize) -> bool {
        if index >= self.candidates.len() {
            return false;
        }
        self.chosen = Some(index);
        true
    }

    /// Seconds left before the vote closes, from a server-clock reading.
    pub fn seconds_left(&self, server_time_ms: i32) -> i32 {
        if !self.is_open() {
            return 0;
        }
        ((self.deadline_ms.wrapping_sub(server_time_ms)) / 1000).max(0)
    }
}

/// Panel geometry, in the 480-line reference space the other screens use.
const PANEL_W: f32 = 448.0;
const ROW_GAP: f32 = 26.0;
const REF_H: f32 = 480.0;
const TITLE_Y: f32 = 16.0;
const ROWS_TOP: f32 = 56.0;
const ROW_X: f32 = 24.0;

const SELECTED: [f32; 4] = [1.0, 0.8, 0.2, 1.0];
const NORMAL: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const HEADING: [f32; 4] = [0.75, 0.75, 0.75, 1.0];

/// Draw the vote panel at the top-left. `name_of` maps a candidate id to a
/// friendly name (the registry's), or the id itself.
pub fn build(
    panel: &VotePanel,
    font: &Font,
    scale: f32,
    screen_h: f32,
    server_time_ms: i32,
    name_of: &dyn Fn(&str) -> String,
    out: &mut Vec<HudQuad>,
) {
    if !panel.is_open() {
        return;
    }
    let ps = screen_h / REF_H;
    let rows = panel.candidates().len();
    let panel_h = (ROWS_TOP + ROW_GAP * rows as f32 + 30.0) * ps;
    let w = PANEL_W * ps;
    let x = 16.0 * ps;
    let y = 16.0 * ps;
    let ts = scale * ps;

    out.push(HudQuad {
        verts: [[x, y], [x + w, y], [x + w, y + panel_h], [x, y + panel_h]],
        uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        rgba: [0.0, 0.0, 0.0, 0.72],
        texture: "ui_mp/assets/white".to_string(),
    });

    let secs = panel.seconds_left(server_time_ms);
    font::layout(
        font,
        &format!("MAP VOTE  -  {secs}s"),
        x + ROW_X * ps,
        y + TITLE_Y * ps,
        ts,
        NORMAL,
        out,
    );

    for (i, id) in panel.candidates().iter().enumerate() {
        let ry = y + (ROWS_TOP + ROW_GAP * i as f32) * ps;
        let mine = panel.chosen() == Some(i);
        let color = if mine { SELECTED } else { NORMAL };
        let mark = if mine { "> " } else { "  " };
        font::layout(
            font,
            &format!("{mark}{}.  {}", i + 1, name_of(id)),
            x + ROW_X * ps,
            ry,
            ts,
            color,
            out,
        );
    }
    font::layout(
        font,
        "Press 1-3 to vote",
        x + ROW_X * ps,
        y + (ROWS_TOP + ROW_GAP * rows as f32 + 6.0) * ps,
        ts * 0.82,
        HEADING,
        out,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn a_non_vote_command_is_not_claimed() {
        let mut p = VotePanel::default();
        assert!(!p.on_server_command(&cmd("b 1 0 0 2 10 50")));
        assert!(!p.is_open());
    }

    #[test]
    fn a_vote_command_opens_the_panel_with_its_candidates() {
        let mut p = VotePanel::default();
        assert!(p.on_server_command(&cmd("mapvote 15000 mp_pavlov mp_harbor mp_ship")));
        assert!(p.is_open());
        assert_eq!(p.candidates(), &["mp_pavlov", "mp_harbor", "mp_ship"]);
        assert_eq!(p.deadline_ms(), 15000);
    }

    #[test]
    fn close_clears_the_panel() {
        let mut p = VotePanel::default();
        p.on_server_command(&cmd("mapvote 15000 mp_pavlov mp_harbor"));
        assert!(p.is_open());
        assert!(p.on_server_command(&cmd("mapvote close")));
        assert!(!p.is_open());
        assert_eq!(p.chosen(), None);
    }

    #[test]
    fn choosing_marks_a_valid_index_and_refuses_an_invalid_one() {
        let mut p = VotePanel::default();
        p.on_server_command(&cmd("mapvote 15000 mp_pavlov mp_harbor"));
        assert!(p.choose(1));
        assert_eq!(p.chosen(), Some(1));
        assert!(!p.choose(9));
        assert_eq!(p.chosen(), Some(1), "an invalid choice does not clear the vote");
    }

    #[test]
    fn a_new_ballot_clears_the_previous_choice() {
        let mut p = VotePanel::default();
        p.on_server_command(&cmd("mapvote 15000 mp_pavlov mp_harbor"));
        p.choose(0);
        // A different ballot: a fresh vote, so the old choice does not carry.
        p.on_server_command(&cmd("mapvote 30000 mp_ship mp_brecourt"));
        assert_eq!(p.chosen(), None);
        assert_eq!(p.candidates(), &["mp_ship", "mp_brecourt"]);
    }

    #[test]
    fn a_repeat_of_the_same_ballot_keeps_the_choice() {
        let mut p = VotePanel::default();
        p.on_server_command(&cmd("mapvote 15000 mp_pavlov mp_harbor"));
        p.choose(1);
        p.on_server_command(&cmd("mapvote 14000 mp_pavlov mp_harbor"));
        assert_eq!(p.chosen(), Some(1), "the same ballot keeps the vote");
    }

    #[test]
    fn seconds_left_counts_down_and_never_goes_negative() {
        let mut p = VotePanel::default();
        p.on_server_command(&cmd("mapvote 15000 mp_pavlov mp_harbor"));
        assert_eq!(p.seconds_left(0), 15);
        assert_eq!(p.seconds_left(10000), 5);
        assert_eq!(p.seconds_left(20000), 0);
    }
}
