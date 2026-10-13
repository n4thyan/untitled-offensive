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

use super::HudQuad;
use super::font::{self, Font};

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
    /// Per-class drafts copied from the last authoritative profile. Keeping
    /// them separate means switching tabs cannot make an edit leak into a
    /// different class while the server echo is in flight.
    classes: Vec<Class>,
    /// Set when the profile has nothing to show yet.
    pub empty: bool,
    /// The last refusal the server sent, shown until the next edit.
    pub notice: Option<String>,
}

/// A row of the grid: the slots that sit side by side.
const COLUMNS: usize = 3;
impl CreateAClass {
    /// A fresh screen over the six slots in the shared class model.
    pub fn new(
        primaries: Vec<String>,
        secondaries: Vec<String>,
        equipment: Vec<String>,
        perks: Vec<String>,
    ) -> CreateAClass {
        let mut cells = Vec::new();
        for s in ClassSlot::ALL {
            let choices = match s {
                ClassSlot::Primary => primaries.clone(),
                ClassSlot::Secondary => secondaries.clone(),
                ClassSlot::Equipment => equipment.clone(),
                ClassSlot::Perk1 | ClassSlot::Perk2 | ClassSlot::Perk3 => perks.clone(),
            };
            cells.push(Cell::new(s, choices));
        }
        CreateAClass {
            cells,
            selected: 0,
            choice: 0,
            class_name: "Custom".to_string(),
            class_index: 0,
            classes: Vec::new(),
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

    /// Select the cell under a window-space pointer. Geometry is shared with
    /// the renderer so visible boxes and click targets cannot drift apart.
    pub fn hover(&mut self, x: f32, y: f32, screen_w: f32, screen_h: f32) -> bool {
        let ps = screen_h / REF_H;
        let panel_x = (screen_w - PANEL_W * ps) / 2.0;
        let panel_y = (screen_h - PANEL_H * ps) / 2.0;
        for i in 0..self.cells.len() {
            let col = i % COLUMNS;
            let row = i / COLUMNS;
            let left = panel_x + (CELL_X + (CELL_W + CELL_GAP_X) * col as f32) * ps;
            let top = panel_y + (CELL_Y + (CELL_H + CELL_GAP_Y) * row as f32) * ps;
            if x >= left && x < left + CELL_W * ps && y >= top && y < top + CELL_H * ps {
                if self.selected != i {
                    self.selected = i;
                    self.choice = 0;
                }
                return true;
            }
        }
        false
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
    pub fn commit(&mut self) -> Option<(usize, &'static str, String)> {
        let value = self.highlighted()?.to_string();
        let cell = self.cells.get_mut(self.selected)?;
        if cell.value.as_deref() == Some(value.as_str()) {
            return None;
        }
        cell.value = Some(value.clone());
        if let Some(class) = self.classes.get_mut(self.class_index) {
            class.set(cell.slot, value.clone());
        }
        self.notice = None;
        Some((self.class_index, cell.slot.key(), value))
    }

    /// Clear the selected slot.
    pub fn clear(&mut self) -> Option<(usize, &'static str, String)> {
        let cell = self.cells.get_mut(self.selected)?;
        if cell.value.take().is_none() {
            return None;
        }
        if let Some(class) = self.classes.get_mut(self.class_index) {
            class.set(cell.slot, "");
        }
        self.notice = None;
        Some((self.class_index, cell.slot.key(), String::new()))
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
        self.classes = profile.classes.clone();
        self.load_class(index);
    }

    fn load_class(&mut self, index: usize) {
        for cell in &mut self.cells {
            cell.value = None;
        }
        let Some(class) = self.classes.get(index) else {
            self.empty = true;
            return;
        };
        self.class_name = class.name.clone();
        self.class_index = index;
        self.empty = false;
        for cell in &mut self.cells {
            cell.value = class.get(cell.slot).map(str::to_string);
        }
        self.selected = self.selected.min(self.cells.len().saturating_sub(1));
        self.choice = 0;
    }

    /// Switch to another independently stored class, wrapping at both ends.
    pub fn switch_class(&mut self, delta: isize) -> bool {
        if self.classes.len() < 2 {
            return false;
        }
        let next =
            (self.class_index as isize + delta).rem_euclid(self.classes.len() as isize) as usize;
        self.load_class(next);
        true
    }

    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    /// Which visible class-selector arrow a pointer is over.
    pub fn class_selector_direction(
        &self,
        x: f32,
        y: f32,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<isize> {
        if self.classes.len() < 2 {
            return None;
        }
        let ps = screen_h / REF_H;
        let panel_x = (screen_w - PANEL_W * ps) / 2.0;
        let panel_y = (screen_h - PANEL_H * ps) / 2.0;
        let local_x = (x - panel_x) / ps;
        let local_y = (y - panel_y) / ps;
        if !(CLASS_Y - 14.0..=CLASS_Y + 14.0).contains(&local_y) {
            return None;
        }
        if (CLASS_X..=CLASS_X + 36.0).contains(&local_x) {
            Some(-1)
        } else if (PANEL_W - CLASS_X - 36.0..=PANEL_W - CLASS_X).contains(&local_x) {
            Some(1)
        } else {
            None
        }
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
const PANEL_H: f32 = 280.0;
const REF_H: f32 = 480.0;
const TITLE_Y: f32 = 24.0;
const CLASS_X: f32 = 24.0;
const CLASS_Y: f32 = 60.0;
const CELL_X: f32 = 24.0;
const CELL_Y: f32 = 92.0;
const CELL_W: f32 = 132.0;
const CELL_H: f32 = 52.0;
const CELL_GAP_X: f32 = 12.0;
const CELL_GAP_Y: f32 = 8.0;
const LABEL_SIZE: f32 = 0.7;
const VALUE_SIZE: f32 = 0.72;
const SELECTED: [f32; 4] = [1.0, 0.8, 0.2, 1.0];
const DIM: [f32; 4] = [0.55, 0.55, 0.55, 1.0];
const HINT_SIZE: f32 = 0.68;

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

    out.push(HudQuad {
        verts: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        rgba: [0.0, 0.0, 0.0, 0.78],
        texture: "white".to_string(),
    });

    font::layout(
        font,
        "CREATE A CLASS",
        x + CELL_X * ps,
        y + TITLE_Y * ps,
        ts,
        font::COLORS[7],
        out,
    );

    let class_heading = format!(
        "CLASS {} / {}   {}",
        view.class_index + 1,
        view.class_count().max(1),
        view.class_name
    );
    let class_scale = ts * 0.78;
    let heading_w = font::measure(font, &class_heading, class_scale);
    font::layout(
        font,
        &class_heading,
        x + (w - heading_w) / 2.0,
        y + CLASS_Y * ps,
        class_scale,
        SELECTED,
        out,
    );
    if view.class_count() > 1 {
        font::layout(
            font,
            "<",
            x + CLASS_X * ps,
            y + CLASS_Y * ps,
            class_scale,
            SELECTED,
            out,
        );
        font::layout(
            font,
            ">",
            x + (PANEL_W - CLASS_X - 10.0) * ps,
            y + CLASS_Y * ps,
            class_scale,
            SELECTED,
            out,
        );
    }

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
            texture: "white".to_string(),
        });

        font::layout(
            font,
            slot_label(cell.slot),
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
        (None, false) => "Q/E class  Arrows slot  W/S value  Enter apply".to_string(),
    };
    font::layout(
        font,
        &hint,
        x + CELL_X * ps,
        y + (PANEL_H - 44.0) * ps,
        ts * HINT_SIZE,
        font::COLORS[7],
        out,
    );
    font::layout(
        font,
        "BACKSPACE CLEAR   TAB SAVE   ESC BACK",
        x + CELL_X * ps,
        y + (PANEL_H - 24.0) * ps,
        ts * HINT_SIZE,
        DIM,
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

    /// The editor mirrors the shared class model exactly. Repeating a visual
    /// cell for one slot would make two controls overwrite the same value.
    #[test]
    fn the_grid_has_each_real_class_slot_exactly_once() {
        let s = screen();
        assert_eq!(s.cells.len(), ClassSlot::ALL.len());
        assert_eq!(
            s.cells.iter().map(|cell| cell.slot).collect::<Vec<_>>(),
            ClassSlot::ALL
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
        assert_eq!(s.commit(), Some((0, "primary", "thompson_mp".to_string())));
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

    #[test]
    fn pointer_hitboxes_follow_both_rendered_rows() {
        let mut s = screen();
        let ps = 720.0 / REF_H;
        let panel_x = (1280.0 - PANEL_W * ps) / 2.0;
        let panel_y = (720.0 - PANEL_H * ps) / 2.0;
        let x = panel_x + (CELL_X + 4.0) * ps;
        let y = panel_y + (CELL_Y + (CELL_H + CELL_GAP_Y) + 4.0) * ps;
        assert!(s.hover(x, y, 1280.0, 720.0));
        assert_eq!(s.selected, 3, "the second row is clickable at 720p");
    }

    #[test]
    fn perk_cells_use_the_three_distinct_model_slots() {
        assert_eq!(
            screen().cells[3..]
                .iter()
                .map(|cell| cell.slot)
                .collect::<Vec<_>>(),
            vec![ClassSlot::Perk1, ClassSlot::Perk2, ClassSlot::Perk3]
        );
    }

    #[test]
    fn edits_name_the_class_index_they_belong_to() {
        let mut profile = Profile {
            name: "vcod".into(),
            classes: vec![Class::new("Rifleman"), Class::new("Support")],
        };
        profile.classes[0].set(ClassSlot::Primary, "m1garand_mp");
        profile.classes[1].set(ClassSlot::Primary, "thompson_mp");
        let mut s = screen();
        s.load(&profile, 1);
        s.selected = 0;
        s.choice = 0;
        assert_eq!(
            s.commit(),
            Some((1, "primary", "m1garand_mp".to_string()))
        );
    }

    #[test]
    fn switching_classes_keeps_each_classes_draft_separate() {
        let mut first = Class::new("Rifleman");
        first.set(ClassSlot::Primary, "m1garand_mp");
        let mut second = Class::new("Support");
        second.set(ClassSlot::Primary, "thompson_mp");
        let profile = Profile {
            name: "vcod".into(),
            classes: vec![first, second],
        };
        let mut s = screen();
        s.load(&profile, 0);
        assert_eq!(s.cells[0].value.as_deref(), Some("m1garand_mp"));
        assert!(s.switch_class(1));
        assert_eq!(s.class_index, 1);
        assert_eq!(s.class_name, "Support");
        assert_eq!(s.cells[0].value.as_deref(), Some("thompson_mp"));
        assert!(s.switch_class(-1));
        assert_eq!(s.cells[0].value.as_deref(), Some("m1garand_mp"));
    }

    #[test]
    fn all_ten_class_positions_are_independently_reachable() {
        let classes = (0..vcod_common::classes::MAX_CLASSES)
            .map(|index| {
                let mut class = Class::new(format!("Class {}", index + 1));
                class.set(
                    ClassSlot::Primary,
                    if index % 2 == 0 {
                        "m1garand_mp"
                    } else {
                        "thompson_mp"
                    },
                );
                class
            })
            .collect();
        let mut s = screen();
        s.load(
            &Profile {
                name: "vcod".into(),
                classes,
            },
            0,
        );

        assert_eq!(s.class_count(), vcod_common::classes::MAX_CLASSES);
        for expected in 1..vcod_common::classes::MAX_CLASSES {
            assert!(s.switch_class(1));
            assert_eq!(s.class_index, expected);
            assert_eq!(s.class_name, format!("Class {}", expected + 1));
        }
        assert!(s.switch_class(1));
        assert_eq!(s.class_index, 0, "the tenth position wraps to the first");
    }

    #[test]
    fn class_selector_arrows_have_matching_mouse_targets() {
        let mut s = screen();
        s.load(
            &Profile {
                name: "vcod".into(),
                classes: vec![Class::new("One"), Class::new("Two")],
            },
            0,
        );
        let ps = 720.0 / REF_H;
        let panel_x = (1280.0 - PANEL_W * ps) / 2.0;
        let panel_y = (720.0 - PANEL_H * ps) / 2.0;
        let y = panel_y + CLASS_Y * ps;
        assert_eq!(
            s.class_selector_direction(panel_x + CLASS_X * ps, y, 1280.0, 720.0),
            Some(-1)
        );
        assert_eq!(
            s.class_selector_direction(
                panel_x + (PANEL_W - CLASS_X) * ps,
                y,
                1280.0,
                720.0
            ),
            Some(1)
        );
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
        assert_eq!(s.clear(), Some((0, "primary", String::new())));
        assert!(s.cells[0].value.is_none());
        assert_eq!(s.clear(), None, "clearing an empty cell changes nothing");
    }

    /// A draft with no primary and no secondary is not spawnable, matching
    /// `Class::is_playable`, and the screen says so.
    #[test]
    fn a_draft_with_no_weapon_is_not_playable() {
        let mut s = screen();
        assert!(!s.is_playable());
        s.selected = 1; // secondary
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
        s.selected = 2; // equipment
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
