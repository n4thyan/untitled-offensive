//! A reusable menu shell: the state machine every vcod-owned screen shares,
//! so the main menu, the pause menu, the match-setup screen and the class
//! picker do not each reinvent navigation, selection and rendering.
//!
//! It is deliberately *not* the retail script-menu path (`hud/menu.rs`). A
//! script menu is one flat list the server opened and the client answers with
//! `mr`; these are client-owned screens with their own lifecycle (a pause menu
//! is not something a gametype script opens). What they *do* share is the
//! shape: a title, a list of rows, one selected, up/down to move, Enter to
//! activate, Esc to go back.
//!
//! A row is either a plain action, a left/right value selector (the map and
//! bot-count rows), or a divider. A row can be disabled, and a disabled row is
//! visibly dim and cannot be activated - a menu item that does nothing must
//! look like it does nothing rather than silently swallow the key.

use super::font::{self, Font};
use super::HudQuad;

/// What a row does when Enter is pressed, or what left/right changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A plain action, identified by [`Row::id`].
    Action,
    /// A value with `choices`, stepped by left/right. `value` indexes them.
    Selector { choices: Vec<String>, value: usize },
    /// A non-selectable heading or spacer.
    Divider,
}

/// One row of a menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// A stable identifier the caller matches on when a row activates. Empty
    /// for a divider.
    pub id: String,
    pub label: String,
    pub kind: RowKind,
    /// A row that is shown but cannot be selected.
    pub disabled: bool,
}

impl Row {
    /// A plain action row.
    pub fn action(id: impl Into<String>, label: impl Into<String>) -> Row {
        Row {
            id: id.into(),
            label: label.into(),
            kind: RowKind::Action,
            disabled: false,
        }
    }

    /// An action row that is shown dim and does nothing.
    pub fn disabled(id: impl Into<String>, label: impl Into<String>) -> Row {
        Row {
            disabled: true,
            ..Row::action(id, label)
        }
    }

    /// A left/right selector row.
    pub fn selector(id: impl Into<String>, label: impl Into<String>, choices: Vec<String>) -> Row {
        Row {
            id: id.into(),
            label: label.into(),
            kind: RowKind::Selector { choices, value: 0 },
            disabled: false,
        }
    }

    /// A heading/spacer row.
    pub fn divider(label: impl Into<String>) -> Row {
        Row {
            id: String::new(),
            label: label.into(),
            kind: RowKind::Divider,
            disabled: true,
        }
    }

    fn selectable(&self) -> bool {
        !self.disabled && self.kind != RowKind::Divider
    }

    /// The selected choice of a selector row.
    pub fn selected_choice(&self) -> Option<&str> {
        match &self.kind {
            RowKind::Selector { choices, value } => choices.get(*value).map(String::as_str),
            _ => None,
        }
    }
}

/// What a key did to the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The menu consumed the key and nothing else should happen.
    Consumed,
    /// The menu wants the caller to run this row's action.
    Activate(String),
    /// Esc: the caller closes or backs out.
    Close,
    /// The key was not the menu's.
    Ignored,
}

/// A whole menu screen: title, rows, and which row is selected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Menu {
    pub title: String,
    pub rows: Vec<Row>,
    pub selected: usize,
}

impl Menu {
    pub fn new(title: impl Into<String>, rows: Vec<Row>) -> Menu {
        let mut menu = Menu {
            title: title.into(),
            rows,
            selected: 0,
        };
        menu.select_first();
        menu
    }

    /// Put the cursor on the first selectable row, so a menu with a heading
    /// does not open with the heading selected.
    fn select_first(&mut self) {
        if let Some(i) = self.rows.iter().position(Row::selectable) {
            self.selected = i;
        }
    }

    /// Move the cursor to the next selectable row, wrapping. Dividers and
    /// disabled rows are skipped, so up/down never parks on a dead row.
    pub fn step(&mut self, delta: isize) {
        let n = self.rows.len();
        if n == 0 {
            return;
        }
        let mut i = self.selected;
        for _ in 0..n {
            i = (i as isize + delta).rem_euclid(n as isize) as usize;
            if self.rows[i].selectable() {
                self.selected = i;
                return;
            }
        }
    }

    pub fn up(&mut self) {
        self.step(-1);
    }

    pub fn down(&mut self) {
        self.step(1);
    }

    pub fn selected_row(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    /// Step the selected selector row's value. Returns the new value when the
    /// row is a selector and the value moved.
    pub fn step_value(&mut self, delta: isize) -> Option<usize> {
        let row = self.rows.get_mut(self.selected)?;
        let RowKind::Selector { choices, value } = &mut row.kind else {
            return None;
        };
        if choices.is_empty() {
            return None;
        }
        let n = choices.len() as isize;
        *value = (*value as isize + delta).rem_euclid(n) as usize;
        Some(*value)
    }

    /// The current value of the row with `id`, as a choice string.
    pub fn value_of(&self, id: &str) -> Option<&str> {
        self.rows
            .iter()
            .find(|r| r.id == id)
            .and_then(Row::selected_choice)
    }

    /// Replace the choices and value of a selector row, e.g. when a map list
    /// is rebuilt. The value is clamped to the new list.
    pub fn set_selector(&mut self, id: &str, choices: Vec<String>, value: usize) {
        let Some(row) = self.rows.iter_mut().find(|r| r.id == id) else {
            return;
        };
        let value = value.min(choices.len().saturating_sub(1));
        row.kind = RowKind::Selector { choices, value };
    }
}

/// Panel geometry, in the 480-line reference space the other screens use.
const PANEL_W: f32 = 448.0;
const PANEL_H: f32 = 288.0;
const REF_H: f32 = 480.0;
const TITLE_Y: f32 = 24.0;
const ROWS_TOP: f32 = 72.0;
const ROW_GAP: f32 = 26.0;
const ROW_X: f32 = 24.0;
const HINT_Y: f32 = 262.0;

const SELECTED: [f32; 4] = [1.0, 0.8, 0.2, 1.0];
const NORMAL: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const DIM: [f32; 4] = [0.5, 0.5, 0.5, 1.0];
const HEADING: [f32; 4] = [0.75, 0.75, 0.75, 1.0];

/// Draw the menu: a dark panel, the title, then one row each. A selector row
/// shows `< value >`; a disabled row is dim; the selected row is yellow and
/// carries a caret.
pub fn build(
    menu: &Menu,
    font: &Font,
    scale: f32,
    screen_w: f32,
    screen_h: f32,
    hint: &str,
    out: &mut Vec<HudQuad>,
) {
    let ps = screen_h / REF_H;
    let w = PANEL_W * ps;
    let h = PANEL_H * ps;
    let x = (screen_w - w) / 2.0;
    let y = (screen_h - h) / 2.0;
    let ts = scale * ps;

    // A dark backing plate, so the menu is legible over a bright map.
    out.push(HudQuad {
        verts: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        rgba: [0.0, 0.0, 0.0, 0.72],
        texture: "ui_mp/assets/white".to_string(),
    });

    font::layout(
        font,
        &menu.title,
        x + ROW_X * ps,
        y + TITLE_Y * ps,
        ts,
        NORMAL,
        out,
    );

    for (i, row) in menu.rows.iter().enumerate() {
        let ry = y + (ROWS_TOP + ROW_GAP * i as f32) * ps;
        let selected = i == menu.selected && row.selectable();
        let color = match (row.disabled, selected) {
            (_, true) => SELECTED,
            (true, false) if row.kind == RowKind::Divider => HEADING,
            (true, false) => DIM,
            (false, false) => NORMAL,
        };
        let text = match &row.kind {
            RowKind::Selector { .. } => match row.selected_choice() {
                Some(v) => format!("{}:  < {v} >", row.label),
                None => format!("{}:  < >", row.label),
            },
            _ => row.label.clone(),
        };
        // The caret marks the selection without relying on colour alone.
        let caret = if selected { "> " } else { "  " };
        font::layout(
            font,
            &format!("{caret}{text}"),
            x + ROW_X * ps,
            ry,
            ts,
            color,
            out,
        );
    }

    font::layout(
        font,
        hint,
        x + ROW_X * ps,
        y + HINT_Y * ps,
        ts * 0.82,
        HEADING,
        out,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Menu {
        Menu::new(
            "MULTIPLAYER",
            vec![
                Row::divider("HEADING"),
                Row::action("play", "Play"),
                Row::disabled("stats", "Stats"),
                Row::action("quit", "Quit"),
            ],
        )
    }

    /// A heading is skipped: the cursor opens on the first real row.
    #[test]
    fn the_cursor_starts_on_the_first_selectable_row() {
        assert_eq!(menu().selected_row().unwrap().id, "play");
    }

    /// A disabled row is never selected, so up/down cannot park on a dead item
    /// and Enter can never fire it.
    #[test]
    fn navigation_skips_disabled_rows_and_wraps() {
        let mut m = menu();
        m.down();
        assert_eq!(m.selected_row().unwrap().id, "quit", "skipped stats");
        m.down();
        assert_eq!(m.selected_row().unwrap().id, "play", "wrapped past quit");
        m.up();
        assert_eq!(m.selected_row().unwrap().id, "quit");
    }

    /// A selector row steps through its choices and wraps.
    #[test]
    fn a_selector_steps_and_wraps() {
        let mut m = Menu::new(
            "PLAY",
            vec![Row::selector(
                "map",
                "Map",
                vec!["Carentan".into(), "Pavlov".into(), "Harbor".into()],
            )],
        );
        assert_eq!(m.value_of("map"), Some("Carentan"));
        m.step_value(1);
        assert_eq!(m.value_of("map"), Some("Pavlov"));
        m.step_value(1);
        m.step_value(1);
        assert_eq!(m.value_of("map"), Some("Carentan"), "wrapped");
        m.step_value(-1);
        assert_eq!(m.value_of("map"), Some("Harbor"), "backwards wrap");
    }

    /// Stepping a value on a plain action row does nothing.
    #[test]
    fn stepping_a_value_on_an_action_row_does_nothing() {
        let mut m = Menu::new("x", vec![Row::action("a", "A")]);
        assert_eq!(m.step_value(1), None);
    }

    /// A selector whose list is replaced keeps a valid index.
    #[test]
    fn replacing_a_selector_clamps_the_value() {
        let mut m = Menu::new(
            "x",
            vec![Row::selector("map", "Map", vec!["a".into(), "b".into()])],
        );
        m.step_value(1);
        assert_eq!(m.value_of("map"), Some("b"));
        m.set_selector("map", vec!["only".into()], 1);
        assert_eq!(m.value_of("map"), Some("only"), "clamped into range");
    }

    /// A menu of only dividers has nothing to select and navigation is inert
    /// rather than panicking.
    #[test]
    fn a_menu_with_nothing_selectable_is_inert() {
        let mut m = Menu::new("x", vec![Row::divider("a"), Row::divider("b")]);
        m.down();
        m.up();
        assert!(m.selected_row().is_some());
        assert!(m.selected_row().is_none_or(|r| !r.selectable()));
    }
}
