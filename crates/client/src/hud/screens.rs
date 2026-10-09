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
//!
//! ## Playlists
//!
//! The Play screen is playlist-first: the playlist picks the map pool and the
//! gametypes, and the map selector is constrained to that pool. The rules live
//! in [`vcod_common::maplist::Playlist`], not here, so this file only reads
//! them.

use vcod_common::classes::Profile;
use vcod_common::maplist::{MapRegistry, PlayableMap, Playlist, MAX_BOTS, MIN_BOTS};
use vcod_common::maplist::{GAMETYPES, PLAYLISTS};

use super::shell::{Menu, Outcome, Row};

/// Which screen is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// The root multiplayer menu: Play, Create-a-Class, Options, Quit.
    Main,
    /// Playlist / map / gametype / bots / bot combat, and START MATCH.
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
        playlist: String,
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
    /// The Play screen's held settings. `map_index` indexes the *active
    /// playlist's* map list, not the whole registry.
    playlist: usize,
    map_index: usize,
    gametype: usize,
    bots: usize,
    bot_combat: bool,
    /// The class list the picker shows, refreshed from the profile.
    classes: Vec<String>,
    /// Whether Create-a-Class has a server-backed profile to edit.
    class_editor_available: bool,
    /// A root menu over gameplay may close; the standalone frontend may not.
    root_closable: bool,
}

/// How many bots the Play screen steps through, from MIN to MAX.
fn bot_choices() -> Vec<String> {
    (MIN_BOTS..=MAX_BOTS).map(|n| n.to_string()).collect()
}

fn playlist_choices() -> Vec<String> {
    PLAYLISTS
        .iter()
        .map(|p| p.display_name.to_string())
        .collect()
}

fn on_off() -> Vec<String> {
    vec!["On".into(), "Off".into()]
}

/// The gametype labels a playlist offers, in its own order.
fn gametype_choices(playlist: &Playlist) -> Vec<String> {
    playlist
        .gametypes
        .iter()
        .map(|id| {
            GAMETYPES
                .iter()
                .find(|(gid, _)| gid == id)
                .map(|(_, label)| (*label).to_string())
                .unwrap_or_else(|| (*id).to_string())
        })
        .collect()
}

/// The gametype id at an index into a playlist's gametypes.
fn gametype_id(playlist: &Playlist, index: usize) -> &'static str {
    playlist.gametypes.get(index).copied().unwrap_or("dm")
}

impl Screens {
    /// A screens owner over a discovered map registry. The Play screen opens
    /// on the first playlist that has maps, or on the first playlist.
    pub fn new(registry: MapRegistry) -> Screens {
        let mut s = Screens {
            current: None,
            registry,
            playlist: 0,
            map_index: 0,
            gametype: 0,
            // Eight bots is the recording default, and what the launcher is
            // used to; the pool growth makes room for the player.
            bots: 8.min(MAX_BOTS),
            bot_combat: true,
            classes: Vec::new(),
            class_editor_available: true,
            root_closable: true,
        };
        s.playlist = s.first_populated_playlist();
        s.bots = s.selected_playlist().default_bots.min(MAX_BOTS);
        s
    }

    /// Screens for a frontend with no world, connection, or server.
    pub fn frontend(registry: MapRegistry) -> Screens {
        let mut screens = Screens::new(registry);
        screens.class_editor_available = false;
        screens.root_closable = false;
        screens
    }

    /// The first playlist with a playable map, so the screen does not open on
    /// an empty pool when another one has maps. Falls back to the first.
    fn first_populated_playlist(&self) -> usize {
        PLAYLISTS
            .iter()
            .position(|p| !self.registry.in_playlist(p).is_empty())
            .unwrap_or(0)
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
        self.clamp_map_index();
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

    /// The active playlist.
    pub fn selected_playlist(&self) -> &'static Playlist {
        PLAYLISTS.get(self.playlist).unwrap_or(&PLAYLISTS[0])
    }

    /// The maps the active playlist offers, in registry order.
    pub fn playlist_maps(&self) -> Vec<&PlayableMap> {
        self.registry.in_playlist(self.selected_playlist())
    }

    /// The metadata for the map the Play screen currently holds.
    pub fn selected_map_meta(&self) -> Option<&PlayableMap> {
        self.playlist_maps().get(self.map_index).copied()
    }

    /// The current map id the Play screen holds.
    pub fn selected_map(&self) -> Option<&str> {
        self.selected_map_meta().map(|m| m.id.as_str())
    }

    /// The current gametype id.
    pub fn selected_gametype(&self) -> &str {
        gametype_id(self.selected_playlist(), self.gametype)
    }

    pub fn bot_count(&self) -> usize {
        self.bots
    }

    pub fn bot_combat(&self) -> bool {
        self.bot_combat
    }

    /// Keep the map index inside the active playlist's pool.
    fn clamp_map_index(&mut self) {
        let n = self.playlist_maps().len();
        self.map_index = self.map_index.min(n.saturating_sub(1));
        if n == 0 {
            self.map_index = 0;
        }
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
                    if self.class_editor_available {
                        Row::action("createaclass", "Create-a-Class")
                    } else {
                        Row::disabled("createaclass", "Create-a-Class (join a match)")
                    },
                    Row::disabled("options", "Options"),
                    Row::action("quit", "Quit"),
                ],
            ),
            Screen::Play => self.play_menu(),
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

    /// The match-setup screen: playlist first, then the map pool it defines,
    /// then the settings, then the map's own info panel and the actions.
    fn play_menu(&self) -> Menu {
        let playlist = self.selected_playlist();
        let maps: Vec<String> = self
            .playlist_maps()
            .iter()
            .map(|m| m.name.clone())
            .collect();
        let has_maps = !maps.is_empty();

        let mut rows = vec![Row::divider("MATCH SETUP")];
        rows.push(Row::selector("playlist", "Playlist", playlist_choices()));
        if !has_maps {
            rows.push(Row::disabled("map", "Map:  (none in this playlist)"));
        } else {
            rows.push(Row::selector("map", "Map", maps));
        }
        rows.push(Row::selector(
            "gametype",
            "Gametype",
            gametype_choices(playlist),
        ));
        rows.push(Row::selector("bots", "Bots", bot_choices()));
        rows.push(Row::selector("botcombat", "Bot Combat", on_off()));
        rows.push(Row::divider(""));

        // The map's own info, shown while the map row is on screen so the
        // community author is visible in the UI, not buried in a readme.
        rows.extend(self.map_info_rows());

        rows.push(Row::divider(""));
        if !has_maps {
            rows.push(Row::disabled("start", "START MATCH"));
        } else {
            rows.push(Row::action("start", "START MATCH"));
        }
        rows.push(Row::action("back", "Back"));
        let mut menu = Menu::new("PLAY", rows);
        self.seed_play_values(&mut menu);
        menu
    }

    /// The non-selectable info lines for the held map.
    fn map_info_rows(&self) -> Vec<Row> {
        let Some(map) = self.selected_map_meta() else {
            return Vec::new();
        };
        let mut rows = vec![Row::divider(format!("MAP: {}", map.name.to_uppercase()))];
        if !map.description.is_empty() {
            rows.push(Row::disabled("info", format!("  {}", map.description)));
        }
        rows.push(Row::disabled(
            "info",
            format!("  Author:  {}", credit_line(map)),
        ));
        rows.push(Row::disabled(
            "info",
            format!("  Source:  {}", map.source_type.label()),
        ));
        if !map.supported_gametypes.is_empty() {
            rows.push(Row::disabled(
                "info",
                format!("  Modes:  {}", map.supported_gametypes.join(", ")),
            ));
        }
        if !map.recommended_players.is_empty() {
            rows.push(Row::disabled(
                "info",
                format!("  Players:  {}", map.recommended_players),
            ));
        }
        rows
    }

    /// Put the Play screen's held settings onto a freshly built menu, so the
    /// screen opens on the settings the player last chose rather than resetting.
    fn seed_play_values(&self, menu: &mut Menu) {
        menu.set_selector("playlist", playlist_choices(), self.playlist);
        let maps: Vec<String> = self
            .playlist_maps()
            .iter()
            .map(|m| m.name.clone())
            .collect();
        menu.set_selector("map", maps, self.map_index);
        menu.set_selector(
            "gametype",
            gametype_choices(self.selected_playlist()),
            self.gametype,
        );
        menu.set_selector("bots", bot_choices(), self.bots.saturating_sub(MIN_BOTS));
        menu.set_selector("botcombat", on_off(), usize::from(!self.bot_combat));
    }

    /// Apply a key to the current screen's menu and return the action it asked
    /// for, if any.
    ///
    /// The screen's held settings are read back out of the menu on the way
    /// through, so a value the player stepped is remembered the next time the
    /// screen opens. The caller keeps the menu between input events, but the
    /// settings also survive the rebuild required by a screen transition.
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
                    Some(Screen::Pause) => Action::Close,
                    Some(Screen::Main) if self.root_closable => Action::Close,
                    Some(Screen::Main) | None => Action::Nothing,
                };
                (Outcome::Close, Some(action))
            }
            MenuKey::None => (Outcome::Ignored, None),
        }
    }

    /// Hover selection uses the shell's rendered hitboxes.
    pub fn handle_hover(
        &mut self,
        menu: &mut Menu,
        x: f32,
        y: f32,
        screen_w: f32,
        screen_h: f32,
    ) -> bool {
        menu.hover(x, y, screen_w, screen_h)
    }

    /// Click a row through the same action/value paths as keyboard input.
    pub fn handle_click(
        &mut self,
        menu: &mut Menu,
        x: f32,
        y: f32,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<Action> {
        let index = menu.hit_test(x, y, screen_w, screen_h)?;
        menu.selected = index;
        if matches!(
            menu.selected_row().map(|r| &r.kind),
            Some(super::shell::RowKind::Selector { .. })
        ) {
            let delta = if x < screen_w / 2.0 { -1 } else { 1 };
            self.step_value(menu, delta);
            None
        } else {
            self.handle_key(menu, MenuKey::Activate).1
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
            "playlist" => {
                if let Some(v) = menu.value_of("playlist") {
                    if let Some(i) = playlist_choices().iter().position(|n| n == v) {
                        self.playlist = i;
                        // The map pool changed under the cursor: reset the map
                        // and gametype to the new playlist's first entries, and
                        // re-seed the rows that depend on it.
                        self.map_index = 0;
                        self.gametype = 0;
                        self.bots = self.selected_playlist().default_bots.min(MAX_BOTS);
                        let maps: Vec<String> = self
                            .playlist_maps()
                            .iter()
                            .map(|m| m.name.clone())
                            .collect();
                        menu.set_selector("map", maps, self.map_index);
                        menu.set_selector(
                            "gametype",
                            gametype_choices(self.selected_playlist()),
                            self.gametype,
                        );
                        menu.set_selector("bots", bot_choices(), self.bots);
                    }
                }
            }
            "map" => {
                if let Some(v) = menu.value_of("map") {
                    let maps = self.playlist_maps();
                    if let Some(i) = maps.iter().position(|m| m.name == v) {
                        self.map_index = i;
                    }
                }
            }
            "gametype" => {
                if let Some(v) = menu.value_of("gametype") {
                    if let Some(i) = gametype_choices(self.selected_playlist())
                        .iter()
                        .position(|n| n == v)
                    {
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
                    playlist: self.selected_playlist().id.to_string(),
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

/// The credit line for a map: the author, plus additional credits when the
/// readme named any. A map with no author is shown as unverified rather than
/// silently credited to nobody.
fn credit_line(map: &PlayableMap) -> String {
    if !map.has_credit() {
        return vcod_common::maplist::UNKNOWN_AUTHOR.to_string();
    }
    if map.additional_credits.is_empty() {
        map.author.clone()
    } else {
        format!("{} ({})", map.author, map.additional_credits)
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
            "mp_stanjel",
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
            let action = self.s.handle_key(&mut self.menu, k).1;
            // The client rebuilds the drawn menu after an action that changed
            // the screen; mirror that so the cursor is where the new screen
            // put it.
            if matches!(action, Some(Action::Open(_)) | Some(Action::Close)) {
                self.refresh();
            }
            action
        }

        /// Move the cursor to the row with this id.
        fn select_row(&mut self, id: &str) {
            for _ in 0..self.menu.rows.len() {
                if self.menu.selected_row().is_some_and(|r| r.id == id) {
                    return;
                }
                self.menu.down();
            }
            panic!("no row {id}");
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
    /// The registry sorts by id, so the first populated playlist (Classic)
    /// offers Carentan, Harbor, Pavlov; step to Pavlov.
    #[test]
    fn start_match_carries_the_chosen_settings() {
        let mut ui = Ui::new(Screen::Play);
        assert_eq!(ui.s.selected_playlist().id, "classic");
        ui.select_row("map");
        ui.key(MenuKey::Right);
        ui.key(MenuKey::Right);
        ui.select_row("bots");
        for _ in 0..4 {
            ui.key(MenuKey::Right);
        }
        ui.select_row("botcombat");
        ui.key(MenuKey::Right);
        assert_eq!(ui.s.selected_map(), Some("mp_pavlov"));
        assert_eq!(ui.s.bot_count(), 12);
        assert!(!ui.s.bot_combat());

        // Rebuild the menu (as the client does when a screen opens) and start:
        // the held settings must survive.
        ui.refresh();
        ui.select_row("start");
        assert_eq!(
            ui.key(MenuKey::Activate),
            Some(Action::StartMatch {
                playlist: "classic".into(),
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
        ui.select_row("map");
        ui.key(MenuKey::Right); // map -> Harbor
        ui.key(MenuKey::Right); // map -> Pavlov
        ui.select_row("bots");
        ui.key(MenuKey::Right); // bots -> 9
                                // Leave and come back.
        ui.s.close();
        ui.s.open(Screen::Play);
        ui.refresh();
        assert_eq!(ui.menu.value_of("map"), Some("Pavlov"));
        assert_eq!(ui.menu.value_of("bots"), Some("9"));
    }

    /// Switching the playlist re-pools the map list and cannot leave the
    /// cursor on a map the new playlist does not offer.
    #[test]
    fn switching_playlist_re_pools_the_map_list() {
        let mut ui = Ui::new(Screen::Play);
        assert_eq!(ui.s.selected_playlist().id, "classic");
        ui.select_row("playlist");
        // Step left from Classic (the first) wraps to the last playlist.
        ui.key(MenuKey::Left);
        ui.refresh();
        assert_eq!(ui.s.selected_playlist().id, "iw_classics");
        // IW Classics has only the unverified Rust, so its pool is empty.
        assert!(ui.s.playlist_maps().is_empty());
        assert_eq!(ui.s.selected_map(), None);

        // Community Classics: only the verified community map.
        ui.s.playlist = 1;
        ui.refresh();
        assert_eq!(ui.s.selected_playlist().id, "community_classics");
        assert_eq!(ui.s.selected_map(), Some("mp_stanjel"));
        assert_eq!(ui.s.playlist_maps().len(), 1);

        // Classic: stock only, three maps.
        ui.s.playlist = 0;
        ui.refresh();
        assert_eq!(ui.s.selected_playlist().id, "classic");
        assert_eq!(ui.s.playlist_maps().len(), 3);
    }

    /// A playlist with no verified maps disables the map row and START MATCH
    /// cannot be reached with a map that does not exist.
    #[test]
    fn an_empty_playlist_disables_the_map_row() {
        let mut s = screens();
        s.open(Screen::Play);
        s.playlist = 3; // iw_classics: Rust is unverified
        let menu = s.menu().unwrap();
        assert!(menu.rows.iter().any(|r| r.id == "map" && r.disabled));
        assert_eq!(s.selected_map(), None);
    }

    /// The map info panel names the community author, so credit is visible in
    /// the UI and not only in a readme.
    #[test]
    fn the_map_info_panel_credits_the_author() {
        let mut s = screens();
        s.playlist = 1; // community_classics
        s.open(Screen::Play);
        let menu = s.menu().unwrap();
        let text: Vec<&str> = menu.rows.iter().map(|r| r.label.as_str()).collect();
        assert!(
            text.iter().any(|l| l.contains("Roger Abrahamsson")),
            "author line missing: {text:?}"
        );
        assert!(text.iter().any(|l| l.contains("CoD1 Community")));
        assert!(text.iter().any(|l| l.contains("MAP: STANJEL")));
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

    /// Every playlist yields a first gametype that the gametype table knows,
    /// so the Play screen can never show a mode with no id behind it.
    #[test]
    fn every_playlist_has_a_valid_gametype() {
        for p in PLAYLISTS {
            assert!(
                !gametype_id(p, 0).is_empty(),
                "{} has no first gametype",
                p.id
            );
            assert!(
                GAMETYPES.iter().any(|(id, _)| *id == gametype_id(p, 0)),
                "{}: unknown first gametype",
                p.id
            );
            assert_eq!(gametype_choices(p).len(), p.gametypes.len());
        }
    }

    /// Every playlist id resolves, and the helper is total on a bad id.
    #[test]
    fn playlist_lookup_is_total() {
        assert!(vcod_common::maplist::playlist_by_id("quick_play").is_some());
        assert!(vcod_common::maplist::playlist_by_id("nope").is_none());
    }

    #[test]
    fn frontend_disables_actions_that_require_a_live_server() {
        let mut s = Screens::frontend(MapRegistry::from_ids(&["mp_carentan"]));
        s.open(Screen::Main);
        let menu = s.menu().unwrap();
        assert!(menu.rows.iter().any(|r| r.id == "play" && !r.disabled));
        assert!(menu
            .rows
            .iter()
            .any(|r| r.id == "createaclass" && r.disabled));
        assert!(menu.rows.iter().any(|r| r.id == "options" && r.disabled));
        assert!(menu.rows.iter().any(|r| r.id == "quit" && !r.disabled));
        assert!(!menu.rows.iter().any(|r| r.id == "stats"));
    }

    #[test]
    fn mouse_hover_and_click_use_the_same_selection_and_action_path() {
        let mut ui = Ui::new(Screen::Main);
        let quit = ui.menu.rows.iter().position(|r| r.id == "quit").unwrap();
        let (x, y) = ui.menu.row_center(quit, 1600.0, 900.0).unwrap();
        assert!(ui.s.handle_hover(&mut ui.menu, x, y, 1600.0, 900.0));
        assert_eq!(ui.menu.selected_row().unwrap().id, "quit");
        assert_eq!(
            ui.s.handle_click(&mut ui.menu, x, y, 1600.0, 900.0),
            Some(Action::Quit)
        );
    }

    #[test]
    fn clicking_a_selector_half_steps_its_value() {
        let mut ui = Ui::new(Screen::Play);
        let map = ui.menu.rows.iter().position(|r| r.id == "map").unwrap();
        let (x, y) = ui.menu.row_center(map, 1600.0, 900.0).unwrap();
        assert_eq!(ui.menu.value_of("map"), Some("Carentan"));
        assert_eq!(
            ui.s.handle_click(&mut ui.menu, x + 100.0, y, 1600.0, 900.0),
            None
        );
        assert_eq!(ui.menu.value_of("map"), Some("Harbor"));
        assert_eq!(ui.s.selected_map(), Some("mp_harbor"));
    }

    #[test]
    fn empty_playlist_disables_start_match() {
        let mut s = screens();
        s.playlist = 3;
        s.open(Screen::Play);
        let menu = s.menu().unwrap();
        assert!(menu.rows.iter().any(|r| r.id == "start" && r.disabled));
    }

    #[test]
    fn back_from_the_root_menu_does_not_leave_a_blank_frontend() {
        let mut s = Screens::frontend(MapRegistry::from_ids(&["mp_carentan"]));
        s.open(Screen::Main);
        let mut menu = s.menu().unwrap();
        assert_eq!(
            s.handle_key(&mut menu, MenuKey::Back).1,
            Some(Action::Nothing)
        );
        assert_eq!(s.current(), Some(Screen::Main));
    }
}
