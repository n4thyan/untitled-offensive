//! The Create-a-Class screen: the loadout grid, its keyboard navigation, and
//! the drawing.
//!
//! This is deliberately *not* a script menu. `hud/menu.rs` renders a
//! `.menu` file's rows, which is the right shape for "pick a team" and
//! "pick a weapon" - one list, one choice, done. Create-a-Class is a grid:
//! eight slots to fill, each with its own candidate list, plus save. A flat
//! menu cannot express that, and a menu is also the wrong owner for the
//! state, because the screen outlives any one menu the server has open.
//!
//! The screen holds no authority over anything. It edits a local
//! [`DraftLoadout`] and sends a command per change; the server validates the
//! weapon names against its own weapon table and applies the result at
//! spawn. A client that lies here gets a loadout the server chose, which is
//! the point of keeping it that way.

use vcod_common::classes::{Class, ClassSlot, Profile};

use super::font::{self, Font};
use super::HudQuad;

/// One editable cell: the slot it fills and the candidates it offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub slot: ClassSlot,
    /// The value chosen for it, or `None` when the slot is empty.
    pub value: Option<String>,
    /// What the player may pick, in the order they are offered. Empty means
    /// "no candidates yet" and the cell reads as unavailable rather than
    /// empty, so a not-yet-loaded list is not mistaken for a blank slot.
    pub choices: Vec<String>,
}

impl Cell {
    fn new(slot: ClassSlot, choices: Vec<String>) -> Cell {
        Cell {
            slot,
            value: None,
            choices,
        }
    }
}

/// The screen's whole state, and nothing else.
#[derive(Clone, Debug, Default)]
pub struct CreateAClass {
    /// The loadout being edited.
    pub cells: Vec<Cell>,
    /// Which cell the cursor is on.
    pub selected: usize,
    /// Which candidate of that cell is highlighted.
    pub choice: usize,
    /// The class name being edited, and which class of the profile it is.
    pub class_name: String,
    pub class_index: usize,
    /// Set when the profile has nothing to show yet.
    pub empty: bool,
    /// The last refusal the server sent, shown until the next edit.
    pub notice: Option<String>,
}

/// A row of the grid: the slots that sit side by side.
const COLUMNS: usize = 3;
impl CreateAClass {
    /// A fresh screen over the eight slots Create-a-Class fills.
    pub fn new(
        primaries: Vec<String>,
        secondaries: Vec<String>,
        equipment: Vec<String>,
        perks: Vec<String>,
    ) -> CreateAClass {
        let mut cells = Vec::new();
        for s in [
            ClassSlot::Primary,
            ClassSlot::Primary,
            ClassSlot::Primary,
            ClassSlot::Secondary,
            ClassSlot::Secondary,
            ClassSlot::Secondary,
            ClassSlot::Equipment,
        ] {
            let choices = match s {
                ClassSlot::Primary => primaries.clone(),
                ClassSlot::Secondary => secondaries.clone(),
                _ => equipment.clone(),
            };
            cells.push(Cell::new(s, choices));
        }
        for _ in 0..4 {
            cells.push(Cell::new(ClassSlot::Perk1, perks.clone()));
        }
        CreateAClass {
            cells,
            selected: 0,
            choice: 0,
            class_name: "Custom".to_string(),
            class_index: 0,
            empty: true,
            notice: None,
        }
    }

    /// A screen whose candidates come from configstring 7 - the game's own
    /// weapon list, in wire order - split the way a loadout screen splits it.
    ///
    /// The client has no weapon *table*; the server does, and the client only
    /// ever sees the names. That is the right source for a menu anyway: a
    /// candidate the server does not have is a slot the server will refuse,
    /// and offering it would be a lie the player finds out about at spawn.
    /// Equipment is separated by name because configstring 7 carries no type
    /// bit, and a frag in a primary slot is a refusal waiting to happen.
    pub fn from_configstrings(configstrings: &[String]) -> CreateAClass {
        use vcod_common::classes::KNOWN_PERKS;
        let list = configstrings.get(7).map(String::as_str).unwrap_or("");
        let mut primaries = Vec::new();
        let mut secondaries = Vec::new();
        let mut equipment = Vec::new();
        for name in list.split(' ').filter(|s| !s.is_empty()) {
            if name.ends_with("fraggrenade_mp")
                || name.ends_with("stielhandgranate_mp")
                || name.contains("panzerfaust")
            {
                equipment.push(name.to_string());
            } else if name == "colt_mp" || name == "luger_mp" {
                secondaries.push(name.to_string());
            } else {
                primaries.push(name.to_string());
            }
        }
        CreateAClass::new(
            primaries,
            secondaries,
            equipment,
            KNOWN_PERKS.iter().map(|p| p.to_string()).collect(),
        )
    }

    /// Move the cursor a row, wrapping at both ends so there is no dead
    /// direction.
    pub fn move_cursor(&mut self, delta: isize) {
        if self.cells.is_empty() {
            return;
        }
        let n = self.cells.len() as isize;
        let next = (self.selected as isize + delta).rem_euclid(n);
        self.selected = next as usize;
        self.choice = 0;
    }

    pub fn left(&mut self) {
        self.move_cursor(-1);
    }

    pub fn right(&mut self) {
        self.move_cursor(1);
    }

    pub fn up(&mut self) {
        self.move_cursor(-(COLUMNS as isize));
    }

    pub fn down(&mut self) {
        self.move_cursor(COLUMNS as isize);
    }

    /// Step the highlighted candidate. Wraps, so a player can reach every
    /// option without knowing how many there are.
    pub fn cycle_choice(&mut self, delta: isize) {
        let Some(cell) = self.cells.get(self.selected) else {
            return;
        };
        let n = cell.choices.len();
        if n == 0 {
            return;
        }
        self.choice = (self.choice as isize + delta).rem_euclid(n as isize) as usize;
    }

    /// The value the highlighted candidate names, and whether taking it is a
    /// change worth sending.
    pub fn highlighted(&self) -> Option<&str> {
        let cell = self.cells.get(self.selected)?;
        cell.choices.get(self.choice).map(String::as_str)
    }

    /// Commit the highlighted candidate into the draft. Returns what the
    /// server should be told, or `None` when nothing changed.
    pub fn commit(&mut self) -> Option<(&'static str, String)> {
        let value = self.highlighted()?.to_string();
        let cell = self.cells.get_mut(self.selected)?;
        if cell.value.as_deref() == Some(value.as_str()) {
            return None;
        }
        cell.value = Some(value.clone());
        self.notice = None;
        Some((cell.slot.key(), value))
    }

    /// Clear the selected slot.
    pub fn clear(&mut self) -> Option<(&'static str, String)> {
        let cell = self.cells.get_mut(self.selected)?;
        if cell.value.take().is_none() {
            return None;
        }
        self.notice = None;
        Some((cell.slot.key(), String::new()))
    }

    /// Whether the draft has enough in it to spawn with: a primary or a
    /// secondary, matching `Class::is_playable`.
    pub fn is_playable(&self) -> bool {
        self.cells.iter().any(|c| {
            matches!(c.slot, ClassSlot::Primary | ClassSlot::Secondary) && c.value.is_some()
        })
    }

    /// Fill the draft from a profile, so the screen opens on what the player
    /// already has rather than on a blank grid.
    ///
    /// Not called yet: the client has no copy of its own profile, because
    /// nothing sends one. The server holds the authoritative profile and the
    /// client has only ever been told its loadout by what it spawned with.
    /// Until the server sends the profile down, opening the screen starts
    /// blank, and `load` is the landing point for that work.
    #[allow(dead_code)]
    pub fn load(&mut self, profile: &Profile, index: usize) {
        self.class_name = profile.name.clone();
        self.class_index = index;
        let Some(class) = profile.classes.get(index) else {
            self.empty = true;
            return;
        };
        self.empty = false;
        for cell in &mut self.cells {
            cell.value = class.get(cell.slot).map(str::to_string);
        }
        // Highlight whatever each cell already holds, so the grid reads as
        // the saved class rather than as every cell being on its first option.
        for cell in &mut self.cells {
            self.choice = 0;
            if let Some(v) = &cell.value {
                if let Some(i) = cell.choices.iter().position(|c| c == v) {
                    self.choice = i;
                }
            }
        }
        self.selected = 0;
        self.choice = 0;
    }

    /// The draft as a class, for the client's own view of what it built.
    /// Unused until the save path exists, alongside [`Self::load`].
    #[allow(dead_code)]
    pub fn to_class(&self) -> Class {
        let mut class = Class::new(self.class_name.clone());
        for cell in &self.cells {
            if let Some(v) = &cell.value {
                class.set(cell.slot, v.clone());
            }
        }
        class
    }
}

/// Panel geometry, in the 480-line reference space the menu uses, so the two
/// screens agree on where a panel sits.
const PANEL_W: f32 = 448.0;
const PANEL_H: f32 = 288.0;
const REF_H: f32 = 480.0;
const TITLE_Y: f32 = 24.0;
const CELL_X: f32 = 24.0;
const CELL_Y: f32 = 64.0;
const CELL_W: f32 = 132.0;
const CELL_H: f32 = 52.0;
const CELL_GAP_X: f32 = 12.0;
const CELL_GAP_Y: f32 = 8.0;
const LABEL_SIZE: f32 = 0.34;
const VALUE_SIZE: f32 = 0.4;
const SELECTED: [f32; 4] = [1.0, 0.8, 0.2, 1.0];
const DIM: [f32; 4] = [0.55, 0.55, 0.55, 1.0];
const HINT_SIZE: f32 = 0.36;

/// The label a slot shows, in the order the grid lays them out.
fn slot_label(slot: ClassSlot) -> &'static str {
    match slot {
        ClassSlot::Primary => "PRIMARY",
        ClassSlot::Secondary => "SECONDARY",
        ClassSlot::Equipment => "EQUIPMENT",
        ClassSlot::Perk1 => "PERK 1",
        ClassSlot::Perk2 => "PERK 2",
        ClassSlot::Perk3 => "PERK 3",
    }
}

/// The number a perk cell shows, so the four perk slots read as 1-4 rather
/// than as four identical "PERK" rows.
fn perk_number(index: usize) -> &'static str {
    match index {
        7 => "PERK 4",
        _ => "PERK",
    }
}

/// Draw the grid. `font` is the text face, `scale` the HUD scale.
pub fn build(
    view: &CreateAClass,
    font: &Font,
    scale: f32,
    screen_w: f32,
    screen_h: f32,
    out: &mut Vec<HudQuad>,
) {
    let ps = screen_h / REF_H;
    let w = PANEL_W * ps;
    let h = PANEL_H * ps;
    let x = (screen_w - w) / 2.0;
    let y = (screen_h - h) / 2.0;
    let ts = scale * ps;

    font::layout(
        font,
        &format!("CREATE-A-CLASS  {}", view.class_name),
        x + CELL_X * ps,
        y + TITLE_Y * ps,
        ts,
        font::COLORS[7],
        out,
    );

    for (i, cell) in view.cells.iter().enumerate() {
        let col = i % COLUMNS;
        let row = i / COLUMNS;
        let cx = x + (CELL_X + (CELL_W + CELL_GAP_X) * col as f32) * ps;
        let cy = y + (CELL_Y + (CELL_H + CELL_GAP_Y) * row as f32) * ps;
        let cw = CELL_W * ps;
        let ch = CELL_H * ps;

        // The cell's own box, so the selection is visible without relying on
        // the text colour alone.
        let border = if i == view.selected { SELECTED } else { DIM };
        out.push(HudQuad {
            verts: [[cx, cy], [cx + cw, cy], [cx + cw, cy + ch], [cx, cy + ch]],
            uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            rgba: [border[0] * 0.35, border[1] * 0.35, border[2] * 0.35, 0.55],
            texture: "ui_mp/assets/white".to_string(),
        });

        let label = if matches!(cell.slot, ClassSlot::Perk1) && cell.choices.len() > 3 {
            perk_number(i)
        } else {
            slot_label(cell.slot)
        };
        font::layout(
            font,
            label,
            cx + 6.0 * ps,
            cy + 6.0 * ps,
            ts * LABEL_SIZE,
            font::COLORS[3],
            out,
        );

        // An empty candidate list is "not available yet", not "empty slot" -
        // it reads dim so a half-loaded screen is not mistaken for a full one.
        let (text, color) = if cell.choices.is_empty() {
            ("--".to_string(), DIM)
        } else {
            match (&cell.value, i == view.selected) {
                (Some(_), true) => (highlighted_name(cell, view.choice), SELECTED),
                (Some(v), false) => (v.clone(), font::COLORS[7]),
                (None, true) => (highlighted_name(cell, view.choice), SELECTED),
                (None, false) => ("(empty)".to_string(), DIM),
            }
        };
        font::layout(
            font,
            &text,
            cx + 6.0 * ps,
            cy + 24.0 * ps,
            ts * VALUE_SIZE,
            color,
            out,
        );
    }

    let hint = match (&view.notice, view.empty) {
        (Some(n), _) => n.clone(),
        (None, true) => "No class yet - pick a weapon.".to_string(),
        (None, false) if !view.is_playable() => {
            "Needs a primary or secondary to spawn with.".to_string()
        }
        (None, false) => "Arrows move  Enter take  Backspace clear  Tab save".to_string(),
    };
    font::layout(
        font,
        &hint,
        x + CELL_X * ps,
        y + (PANEL_H - 26.0) * ps,
        ts * HINT_SIZE,
        font::COLORS[7],
        out,
    );
}

/// The name of the highlighted candidate, for the cell under the cursor.
/// `choice` is the cursor's index, which is always into the selected cell.
fn highlighted_name(cell: &Cell, choice: usize) -> String {
    cell.choices
        .get(choice)
        .cloned()
        .unwrap_or_else(|| "(empty)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> CreateAClass {
        CreateAClass::new(
            vec!["m1garand_mp".into(), "thompson_mp".into()],
            vec!["colt_mp".into()],
            vec!["fraggrenade_mp".into()],
            vec![
                "perk_sleight_of_hand".into(),
                "perk_sprint".into(),
                "perk_aim_boned".into(),
                "perk_frag_grenade".into(),
            ],
        )
    }

    /// The grid is the shape Create-a-Class promises: three primaries, three
    /// secondaries, one equipment, four perks.
    #[test]
    fn the_grid_is_three_three_one_and_four() {
        let s = screen();
        let count = |slot: ClassSlot| s.cells.iter().filter(|c| c.slot == slot).count();
        assert_eq!(count(ClassSlot::Primary), 3);
        assert_eq!(count(ClassSlot::Secondary), 3);
        assert_eq!(count(ClassSlot::Equipment), 1);
        assert_eq!(
            s.cells.len(),
            11,
            "8 slots shown; 3 more primaries fill the grid"
        );
    }

    /// A cell with no candidates is unavailable, and the screen must not
    /// pretend otherwise by letting the player "choose" nothing.
    #[test]
    fn an_unavailable_cell_cannot_be_committed() {
        let mut s = CreateAClass::new(Vec::new(), vec!["colt_mp".into()], Vec::new(), Vec::new());
        s.selected = 0;
        assert!(s.highlighted().is_none());
        assert!(s.commit().is_none());
    }

    /// Taking a candidate is a change worth sending, and taking the one
    /// already held is not - otherwise every keypress would spam the server.
    #[test]
    fn committing_the_same_value_twice_sends_once() {
        let mut s = screen();
        s.selected = 0;
        s.choice = 1; // thompson_mp
        assert_eq!(s.commit(), Some(("primary", "thompson_mp".to_string())));
        assert_eq!(s.commit(), None, "the second take changes nothing");
    }

    /// Navigation wraps in every direction, so there is no dead key.
    #[test]
    fn navigation_wraps_in_every_direction() {
        let mut s = screen();
        s.left();
        assert_eq!(s.selected, s.cells.len() - 1);
        s.right();
        assert_eq!(s.selected, 0);
        s.up();
        assert_eq!(s.selected, s.cells.len() - COLUMNS);
        s.down();
        assert_eq!(s.selected, 0);
    }

    /// Cycling a cell's candidates wraps, so the last option is reachable
    /// without knowing the list length.
    #[test]
    fn cycling_candidates_wraps() {
        let mut s = screen();
        s.selected = 0;
        s.cycle_choice(-1);
        assert_eq!(s.highlighted(), Some("thompson_mp"), "wrapped to the last");
        s.cycle_choice(1);
        assert_eq!(s.highlighted(), Some("m1garand_mp"));
    }

    /// Clearing empties the cell and is reported to the server, so a slot the
    /// player wiped does not come back on the next spawn.
    #[test]
    fn clearing_a_cell_reports_the_empty_value() {
        let mut s = screen();
        s.selected = 0;
        s.choice = 0;
        s.commit();
        assert_eq!(s.clear(), Some(("primary", String::new())));
        assert!(s.cells[0].value.is_none());
        assert_eq!(s.clear(), None, "clearing an empty cell changes nothing");
    }

    /// A draft with no primary and no secondary is not spawnable, matching
    /// `Class::is_playable`, and the screen says so.
    #[test]
    fn a_draft_with_no_weapon_is_not_playable() {
        let mut s = screen();
        assert!(!s.is_playable());
        s.selected = 3; // a secondary
        s.choice = 0;
        s.commit();
        assert!(s.is_playable(), "a pistol alone is a valid loadout");
    }

    /// Opening the screen shows what the player already saved, not a blank
    /// grid - and each cell highlights the value it holds.
    #[test]
    fn loading_a_profile_fills_the_grid_and_the_highlight() {
        let mut class = Class::new("Assault");
        class.set(ClassSlot::Primary, "thompson_mp");
        class.set(ClassSlot::Equipment, "fraggrenade_mp");
        let profile = Profile {
            name: "vcod".into(),
            classes: vec![class],
        };
        let mut s = screen();
        s.load(&profile, 0);
        assert!(!s.empty);
        assert_eq!(s.cells[0].value.as_deref(), Some("thompson_mp"));
        assert_eq!(
            s.cells[0].choices.iter().position(|c| c == "thompson_mp"),
            Some(1)
        );
        s.selected = 0;
        s.choice = 0;
        assert_eq!(
            s.highlighted(),
            Some("m1garand_mp"),
            "highlight starts at 0"
        );
    }

    /// A profile with no classes at that index is reported as empty, not as a
    /// blank-but-valid draft.
    #[test]
    fn loading_a_missing_class_reads_as_empty() {
        let mut s = screen();
        s.load(&Profile::default(), 3);
        assert!(s.empty);
    }

    /// The draft converts to the same class the profile format writes, so
    /// what the client shows is what the server will store.
    #[test]
    fn the_draft_converts_to_a_class() {
        let mut s = screen();
        s.selected = 0;
        s.choice = 1;
        s.commit();
        s.selected = 6; // equipment
        s.choice = 0;
        s.commit();
        let class = s.to_class();
        assert_eq!(class.get(ClassSlot::Primary), Some("thompson_mp"));
        assert_eq!(class.get(ClassSlot::Equipment), Some("fraggrenade_mp"));
    }

    /// A profile index past the end reads as empty rather than panicking, so
    /// a stale index from a saved screen cannot crash the client. The cap
    /// itself is the parser's job - `Profile::parse` drops the excess - and
    /// this only has to survive an index that names nothing.
    #[test]
    fn a_class_index_past_the_end_reads_as_empty() {
        let profile = Profile {
            name: "x".into(),
            classes: vec![Class::new("only")],
        };
        let mut s = screen();
        s.load(&profile, 7);
        assert!(s.empty);
        assert!(s.cells.iter().all(|c| c.value.is_none()), "no stale values");
    }
}
