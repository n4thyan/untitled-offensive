//! The client-owned menu screens and their lifecycle: the multiplayer main
//! menu, the match-setup screen, the pause menu, and the class picker.
//!
//! Every screen is a [`shell::Menu`] plus the little state a screen needs
//! beyond it. There is deliberately no per-screen navigation code: moving,
//! selecting, stepping a value and skipping a disabled row all live in the
//! shell, so a new screen is a row list and an action handler, not another
//! renderer.
//!
//! ## Input ownership
//!
//! [`Screens`] is the single owner of "is a menu up". While [`Screens::current`]
//! is `Some`, gameplay input must not reach the player - the caller checks
//! [`Screens::captures_input`] and skips the usercmd build. That is what stops
//! a menu keypress from also firing the weapon, and it is checked in one place
//! rather than per binding.

use vcod_common::classes::Profile;
use vcod_common::maplist::{MapRegistry, GAMETYPES, MAX_BOTS, MIN_BOTS};

use super::shell::{Menu, Outcome, Row};

/// Which screen is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// The root multiplayer menu: Play, Create-a-Class, Stats, Options, Quit.
    Main,
    /// Map / gametype / bots / bot combat, and START MATCH.
    Play,
    /// Esc during a match: Resume, Change Class, Options, Leave Match.
    Pause,
    /// The pre-spawn / post-death class picker.
    ClassPicker,
}

/// An action a screen row asked for, handed back to the caller to perform. The
/// screen never does anything itself: it names the action, `main.rs` carries it
/// out, so the screens stay testable with no game around them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Open(Screen),
    /// Close the current screen with no other effect (Back / Resume).
    Close,
    /// Start a local match with these settings.
    StartMatch {
        map: String,
        gametype: String,
        bots: usize,
        bot_combat: bool,
    },
    /// Queue a class index for the next spawn.
    SelectClass(usize),
    /// Open the class editor (Create-a-Class).
    OpenClassEditor,
    /// Leave the match, back to the main menu.
    LeaveMatch,
    /// Quit the client.
    Quit,
    /// Nothing to do; a disabled row never produces this.
    Nothing,
}

/// The client's menu state: which screen is up, the maps and gametypes it may
/// offer, and the settings the Play screen is holding.
pub struct Screens {
    current: Option<Screen>,
    /// The map registry the Play screen and the picker read.
    registry: MapRegistry,
    /// The Play screen's held settings.
    map_index: usize,
    gametype: usize,
    bots: usize,
    bot_combat: bool,
    /// The class list the picker shows, refreshed from the profile.
    classes: Vec<String>,
}

/// How many bots the Play screen steps through, from MIN to MAX.
fn bot_choices() -> Vec<String> {
    (MIN_BOTS..=MAX_BOTS).map(|n| n.to_string()).collect()
}

fn gametype_choices() -> Vec<String> {
    GAMETYPES
        .iter()
        .map(|(_, label)| (*label).to_string())
        .collect()
}

fn on_off() -> Vec<String> {
    vec!["On".into(), "Off".into()]
}

impl Screens {
    /// A screens owner over a discovered map registry. The Play screen opens
    /// on the first playable map, or on nothing when the registry is empty.
    pub fn new(registry: MapRegistry) -> Screens {
        Screens {
            current: None,
            registry,
            map_index: 0,
            gametype: 0,
            // Eight bots is the recording default, and what the launcher is
            // used to; the pool growth makes room for the player.
            bots: 8.min(MAX_BOTS),
            bot_combat: true,
            classes: Vec::new(),
        }
    }

    /// The screen that is up. Used by the render loop to decide what to draw.
    #[allow(dead_code)]
    pub fn current(&self) -> Option<Screen> {
        self.current
    }

    /// Whether a menu owns input right now. The caller must not build a usercmd
    /// or act on a gameplay key while this is true.
    pub fn captures_input(&self) -> bool {
        self.current.is_some()
    }

    pub fn open(&mut self, screen: Screen) {
        self.current = Some(screen);
    }

    pub fn close(&mut self) {
        self.current = None;
    }

    /// The registry the selector reads. Exposed so a caller that needs the
    /// playable list (a rotation, a vote) does not build its own.
    #[allow(dead_code)]
    pub fn registry(&self) -> &MapRegistry {
        &self.registry
    }

    /// Replace the map registry, e.g. after a download reopened the paks, and
    /// clamp the Play screen's held map index so it still names a map.
    pub fn set_registry(&mut self, registry: MapRegistry) {
        self.registry = registry;
        self.map_index = self.map_index.min(self.registry.len().saturating_sub(1));
    }

    /// Refresh the class list the picker shows. Called when the profile sync
    /// delivers a new profile, so the picker is never stale.
    pub fn set_classes(&mut self, profile: Option<&Profile>) {
        self.classes = profile
            .map(|p| p.classes.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();
    }

    /// The class names the picker offers.
    #[allow(dead_code)]
    pub fn classes(&self) -> &[String] {
        &self.classes
    }

    /// The current map id the Play screen holds.
    pub fn selected_map(&self) -> Option<&str> {
        self.registry.id_at(self.map_index)
    }

    /// The current gametype id.
    pub fn selected_gametype(&self) -> &str {
        GAMETYPES
            .get(self.gametype)
            .map(|(id, _)| *id)
            .unwrap_or("dm")
    }

    pub fn bot_count(&self) -> usize {
        self.bots
    }

    pub fn bot_combat(&self) -> bool {
        self.bot_combat
    }

    /// Build the menu for the current screen. `None` when no screen is up.
    pub fn menu(&self) -> Option<Menu> {
        let screen = self.current?;
        Some(match screen {
            Screen::Main => Menu::new(
                "UNTITLED",
                vec![
                    Row::divider("MULTIPLAYER"),
                    Row::action("play", "Play"),
                    Row::action("createaclass", "Create-a-Class"),
                    Row::disabled("stats", "Stats / Leaderboards"),
                    Row::disabled("options", "Options"),
                    Row::action("quit", "Quit"),
                ],
            ),
            Screen::Play => {
                let maps = self.registry.names();
                let mut rows = vec![Row::divider("MATCH SETUP")];
                if maps.is_empty() {
                    rows.push(Row::disabled("map", "Map:  (none playable)"));
                } else {
                    rows.push(Row::selector("map", "Map", maps));
                }
                rows.push(Row::selector("gametype", "Gametype", gametype_choices()));
                rows.push(Row::selector("bots", "Bots", bot_choices()));
                rows.push(Row::selector("botcombat", "Bot Combat", on_off()));
                rows.push(Row::divider(""));
                rows.push(Row::action("start", "START MATCH"));
                rows.push(Row::action("back", "Back"));
                let mut menu = Menu::new("PLAY", rows);
                self.seed_play_values(&mut menu);
                menu
            }
            Screen::Pause => Menu::new(
                "PAUSED",
                vec![
                    Row::action("resume", "Resume"),
                    Row::action("changeclass", "Change Class"),
                    Row::disabled("options", "Options"),
                    Row::action("leave", "Leave Match"),
                ],
            ),
            Screen::ClassPicker => {
                let mut rows = vec![Row::divider("CHOOSE CLASS")];
                if self.classes.is_empty() {
                    rows.push(Row::disabled("none", "(no classes - use Create-a-Class)"));
                } else {
                    for (i, name) in self.classes.iter().enumerate() {
                        rows.push(Row::action(format!("class{i}"), name.clone()));
                    }
                }
                rows.push(Row::divider(""));
                rows.push(Row::action("back", "Back"));
                Menu::new("CHOOSE CLASS", rows)
            }
        })
    }

    /// Put the Play screen's held settings onto a freshly built menu, so the
    /// screen opens on the settings the player last chose rather than resetting.
    fn seed_play_values(&self, menu: &mut Menu) {
        menu.set_selector("map", self.registry.names(), self.map_index);
        menu.set_selector("gametype", gametype_choices(), self.gametype);
        menu.set_selector("bots", bot_choices(), self.bots.saturating_sub(MIN_BOTS));
        menu.set_selector("botcombat", on_off(), usize::from(!self.bot_combat));
    }

    /// Apply a key to the current screen's menu and return the action it asked
    /// for, if any.
    ///
    /// The screen's held settings are read back out of the menu on the way
    /// through, so a value the player stepped is remembered the next time the
    /// screen opens. The caller rebuilds the menu each frame, so state cannot
    /// live in the menu alone.
    pub fn handle_key(&mut self, menu: &mut Menu, key: MenuKey) -> (Outcome, Option<Action>) {
        match key {
            MenuKey::Up => {
                menu.up();
                (Outcome::Consumed, None)
            }
            MenuKey::Down => {
                menu.down();
                (Outcome::Consumed, None)
            }
            MenuKey::Left => {
                self.step_value(menu, -1);
                (Outcome::Consumed, None)
            }
            MenuKey::Right => {
                self.step_value(menu, 1);
                (Outcome::Consumed, None)
            }
            MenuKey::Activate => {
                let id = match menu.selected_row() {
                    Some(row) if !row.disabled => row.id.clone(),
                    _ => return (Outcome::Consumed, None),
                };
                (Outcome::Activate(id.clone()), Some(self.action_for(&id)))
            }
            MenuKey::Back => {
                // Esc backs out of a sub-screen to the main menu rather than
                // all the way out, so the player is never dropped straight
                // into gameplay from a settings screen.
                let action = match self.current {
                    Some(Screen::Play) | Some(Screen::ClassPicker) => Action::Open(Screen::Main),
                    Some(Screen::Pause) | Some(Screen::Main) => Action::Close,
                    None => Action::Nothing,
                };
                (Outcome::Close, Some(action))
            }
            MenuKey::None => (Outcome::Ignored, None),
        }
    }

    /// Step the selected selector and read the held settings back out.
    fn step_value(&mut self, menu: &mut Menu, delta: isize) {
        let Some(row) = menu.selected_row() else {
            return;
        };
        let id = row.id.clone();
        if menu.step_value(delta).is_none() {
            return;
        }
        match id.as_str() {
            "map" => {
                if let Some(v) = menu.value_of("map") {
                    if let Some(i) = self.registry.names().iter().position(|n| n == v) {
                        self.map_index = i;
                    }
                }
            }
            "gametype" => {
                if let Some(v) = menu.value_of("gametype") {
                    if let Some(i) = gametype_choices().iter().position(|n| n == v) {
                        self.gametype = i;
                    }
                }
            }
            "bots" => {
                if let Some(v) = menu.value_of("bots").and_then(|v| v.parse::<usize>().ok()) {
                    self.bots = v.clamp(MIN_BOTS, MAX_BOTS);
                }
            }
            "botcombat" => {
                self.bot_combat = menu.value_of("botcombat") == Some("On");
            }
            _ => {}
        }
    }

    /// The action a row id names on the current screen.
    fn action_for(&self, id: &str) -> Action {
        match id {
            "play" => Action::Open(Screen::Play),
            "createaclass" => Action::OpenClassEditor,
            "quit" => Action::Quit,
            "start" => match self.selected_map() {
                Some(map) => Action::StartMatch {
                    map: map.to_string(),
                    gametype: self.selected_gametype().to_string(),
                    bots: self.bots,
                    bot_combat: self.bot_combat,
                },
                // No playable map: the row is disabled, so this is a guard,
                // not a path a player can reach.
                None => Action::Nothing,
            },
            "back" => match self.current {
                Some(Screen::Play) | Some(Screen::ClassPicker) => Action::Open(Screen::Main),
                _ => Action::Close,
            },
            "resume" => Action::Close,
            "changeclass" => Action::Open(Screen::ClassPicker),
            "leave" => Action::LeaveMatch,
            other => match other.strip_prefix("class") {
                Some(n) => n
                    .parse::<usize>()
                    .map(Action::SelectClass)
                    .unwrap_or(Action::Nothing),
                None => Action::Nothing,
            },
        }
    }
}

/// The keys a screen understands, mapped from the physical key by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKey {
    Up,
    Down,
    Left,
    Right,
    Activate,
    Back,
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screens() -> Screens {
        Screens::new(MapRegistry::from_ids(&[
            "mp_carentan",
            "mp_pavlov",
            "mp_harbor",
        ]))
    }

    /// A screen plus the menu it is showing, the way the client holds them: the
    /// menu persists across keypresses so the cursor survives.
    struct Ui {
        s: Screens,
        menu: Menu,
    }

    impl Ui {
        fn new(screen: Screen) -> Ui {
            let mut s = screens();
            s.open(screen);
            let menu = s.menu().expect("a screen is up");
            Ui { s, menu }
        }

        /// Rebuild the menu from the screen, as the client does when a screen
        /// opens or the class list changes.
        fn refresh(&mut self) {
            self.menu = self.s.menu().expect("a screen is up");
        }

        fn key(&mut self, k: MenuKey) -> Option<Action> {
            self.s.handle_key(&mut self.menu, k).1
        }
    }

    /// No screen up means no input is captured: the caller keeps building
    /// usercmds.
    #[test]
    fn no_screen_captures_no_input() {
        let s = screens();
        assert!(!s.captures_input());
        assert!(s.menu().is_none());
    }

    /// The main menu's Play row opens the match-setup screen, and its
    /// Create-a-Class row asks for the editor.
    #[test]
    fn the_main_menu_routes_to_play_and_the_editor() {
        let mut ui = Ui::new(Screen::Main);
        assert_eq!(ui.key(MenuKey::Activate), Some(Action::Open(Screen::Play)));
        let mut ui = Ui::new(Screen::Main);
        ui.key(MenuKey::Down); // createaclass
        assert_eq!(ui.key(MenuKey::Activate), Some(Action::OpenClassEditor));
    }

    /// A disabled row cannot be activated: Stats and Options are shown but do
    /// nothing, and the cursor cannot even land on them.
    #[test]
    fn a_disabled_row_cannot_be_activated() {
        let mut ui = Ui::new(Screen::Main);
        // From Play, step down: createaclass, then quit (stats/options skipped).
        ui.key(MenuKey::Down);
        assert_eq!(ui.key(MenuKey::Activate), Some(Action::OpenClassEditor));
        let mut ui = Ui::new(Screen::Main);
        ui.key(MenuKey::Down);
        ui.key(MenuKey::Down);
        assert_eq!(ui.key(MenuKey::Activate), Some(Action::Quit));
    }

    /// START MATCH carries the settings the player stepped, not the defaults.
    #[test]
    fn start_match_carries_the_chosen_settings() {
        let mut ui = Ui::new(Screen::Play);
        // Map: the registry sorts by id, so the order is Carentan, Harbor,
        // Pavlov; step twice to reach Pavlov.
        ui.key(MenuKey::Right);
        ui.key(MenuKey::Right);
        // Bots: move to the bots row and step to 12.
        ui.key(MenuKey::Down);
        ui.key(MenuKey::Down);
        for _ in 0..4 {
            ui.key(MenuKey::Right);
        }
        // Bot combat: down, step to Off.
        ui.key(MenuKey::Down);
        ui.key(MenuKey::Right);
        assert_eq!(ui.s.selected_map(), Some("mp_pavlov"));
        assert_eq!(ui.s.bot_count(), 12);
        assert!(!ui.s.bot_combat());

        // Rebuild the menu (as the client does when a screen opens) and start:
        // the held settings must survive.
        ui.refresh();
        for _ in 0..4 {
            ui.key(MenuKey::Down);
        }
        assert_eq!(
            ui.key(MenuKey::Activate),
            Some(Action::StartMatch {
                map: "mp_pavlov".into(),
                gametype: "dm".into(),
                bots: 12,
                bot_combat: false,
            })
        );
    }

    /// The Play screen reopens on the settings last chosen.
    #[test]
    fn the_play_screen_remembers_its_settings() {
        let mut ui = Ui::new(Screen::Play);
        ui.key(MenuKey::Right); // map -> harbor
        ui.key(MenuKey::Right); // map -> pavlov
        ui.key(MenuKey::Down);
        ui.key(MenuKey::Down);
        ui.key(MenuKey::Right); // bots -> 9
        // Leave and come back.
        ui.s.close();
        ui.s.open(Screen::Play);
        ui.refresh();
        assert_eq!(ui.menu.value_of("map"), Some("Pavlov"));
        assert_eq!(ui.menu.value_of("bots"), Some("9"));
    }

    /// Esc from a sub-screen goes back to the main menu, not straight out.
    #[test]
    fn esc_backs_out_one_level() {
        let mut ui = Ui::new(Screen::Play);
        assert_eq!(ui.key(MenuKey::Back), Some(Action::Open(Screen::Main)));
        let mut ui = Ui::new(Screen::Main);
        assert_eq!(ui.key(MenuKey::Back), Some(Action::Close));
    }

    /// The class picker lists the profile's classes and selecting one names its
    /// index, which is what `selectclass` sends.
    #[test]
    fn the_class_picker_names_the_chosen_index() {
        let mut s = screens();
        s.set_classes(Some(&Profile {
            name: "vcod".into(),
            classes: vec![
                vcod_common::classes::Class::new("Rifleman"),
                vcod_common::classes::Class::new("Assault"),
                vcod_common::classes::Class::new("Support"),
            ],
        }));
        assert_eq!(s.classes().len(), 3);
        s.open(Screen::ClassPicker);
        let mut ui = Ui {
            menu: s.menu().unwrap(),
            s,
        };
        // The first class row is the first selectable after the heading.
        assert_eq!(ui.key(MenuKey::Activate), Some(Action::SelectClass(0)));
        // Down twice more lands on Support.
        ui.key(MenuKey::Down);
        ui.key(MenuKey::Down);
        assert_eq!(ui.key(MenuKey::Activate), Some(Action::SelectClass(2)));
    }

    /// A profile with no classes leaves the picker with nothing to choose and a
    /// disabled row, rather than an empty menu that silently does nothing.
    #[test]
    fn an_empty_profile_leaves_the_picker_inert() {
        let mut s = screens();
        s.set_classes(Some(&Profile::default()));
        s.open(Screen::ClassPicker);
        let menu = s.menu().unwrap();
        // Only Back is selectable.
        assert_eq!(menu.selected_row().unwrap().id, "back");
    }

    /// The pause menu offers Resume, Change Class and Leave Match; the
    /// not-yet-built Options is disabled.
    #[test]
    fn the_pause_menu_routes_resume_changeclass_and_leave() {
        let mut ui = Ui::new(Screen::Pause);
        assert_eq!(ui.key(MenuKey::Activate), Some(Action::Close));
        let mut ui = Ui::new(Screen::Pause);
        ui.key(MenuKey::Down);
        assert_eq!(
            ui.key(MenuKey::Activate),
            Some(Action::Open(Screen::ClassPicker)),
            "Change Class opens the picker"
        );
        let mut ui = Ui::new(Screen::Pause);
        ui.key(MenuKey::Down);
        ui.key(MenuKey::Down);
        assert_eq!(ui.key(MenuKey::Activate), Some(Action::LeaveMatch));
    }

    /// An empty registry disables the map row and START MATCH cannot be
    /// reached with a map that does not exist.
    #[test]
    fn an_empty_registry_disables_the_map_row() {
        let mut s = Screens::new(MapRegistry::from_ids(&[]));
        s.open(Screen::Play);
        let menu = s.menu().unwrap();
        assert!(menu.rows.iter().any(|r| r.id == "map" && r.disabled));
        assert_eq!(s.selected_map(), None);
    }
}
