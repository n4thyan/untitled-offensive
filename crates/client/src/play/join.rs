//! The stock team/weapon menu handshake as a pure state machine
//! (docs/research/cod11-hud-protocol.md, section 0.1). `JoinProbe`
//! (`crates/client/src/probe.rs`) is the headless reference this mirrors,
//! minus the socket.

use super::cvars::ClientCvars;

/// Configstrings 1180..1212 name the 32 script menus a `t <index>` opens.
const SCRIPT_MENUS: std::ops::Range<usize> = 1180..1212;

/// The script menu currently open, waiting on a `choose` or a `close`.
pub struct OpenMenu {
    pub index: i32,
    pub name: String,
}

/// Drives the handshake for one connection: which menu is open, and which
/// indices an auto-answer has already been sent for since the last
/// `on_gamestate`/`on_restart`.
pub struct Join {
    team: Option<String>,
    weapon: Option<String>,
    open: Option<OpenMenu>,
    /// Menu indices an auto-answer already went out for, so a menu the
    /// server refuses and reopens is offered to the user instead of
    /// answered again (`JoinProbe::answered`).
    auto_answered: Vec<i32>,
    pub cvars: ClientCvars,
}

/// `team_`/`weapon_` menu names auto-answer with the matching field, when
/// set; anything else has no scripted reply.
/// The response to send for a menu the client is joining through, or `None`
/// to leave it open for the player.
///
/// The server-info menu needs an answer even though it is not a team or
/// weapon choice. Every stock gametype opens it before the team menu and only
/// moves on when the response is `"close"` (`pak9`'s `dm.gsc` 242-258:
/// `self openMenu(game["menu_serverinfo"])` then
/// `if(menu == game["menu_serverinfo"] && response == "close") { ... self
/// openMenu(game["menu_team"]); }`). Leaving it unanswered parks the join on
/// its `waittill`, and the player sits in the spectator view of a live match
/// with no error anywhere -- so the auto-join has to answer it, exactly as
/// the server-side bots do.
///
/// This is keyed on the menu `t <idx>` actually opened, not on
/// `g_scriptMainMenu`, which names a different menu at this point (the stock
/// sequence sets that cvar to the team menu before opening server-info).
fn auto_reply<'a>(name: &str, team: Option<&'a str>, weapon: Option<&'a str>) -> Option<&'a str> {
    if name.starts_with("serverinfo_") {
        return Some("close");
    }
    if name.starts_with("team_") {
        return team;
    }
    if name.starts_with("weapon_") {
        return weapon;
    }
    None
}

impl Join {
    pub fn new(team: Option<String>, weapon: Option<String>) -> Self {
        Self {
            team,
            weapon,
            open: None,
            auto_answered: Vec::new(),
            cvars: ClientCvars::default(),
        }
    }

    /// `t <idx>` opens the script menu `configstrings[1180 + idx]` names,
    /// answered at once if it is a `team_`/`weapon_` menu covered by
    /// `Join::new`'s arguments and not already auto-answered since the last
    /// rearm; `u` closes whatever is open. Everything else, including `v`,
    /// which `self.cvars` already consumed, is ignored. Returns the
    /// reliable `mr` commands to send.
    pub fn on_server_command(
        &mut self,
        tokens: &[String],
        configstrings: &[String],
        server_id: i32,
    ) -> Vec<String> {
        self.cvars.on_server_command(tokens);
        match tokens.first().map(String::as_str) {
            Some("t") => {
                let Some(idx) = tokens.get(1).and_then(|t| t.parse::<i32>().ok()) else {
                    return Vec::new();
                };
                let Some(name) = usize::try_from(idx)
                    .ok()
                    .map(|i| SCRIPT_MENUS.start + i)
                    .filter(|cs| SCRIPT_MENUS.contains(cs))
                    .and_then(|cs| configstrings.get(cs))
                    .filter(|n| !n.is_empty())
                else {
                    log::warn!("t {idx}: no script menu at that configstring index");
                    return Vec::new();
                };
                if !self.auto_answered.contains(&idx) {
                    let reply = auto_reply(name, self.team.as_deref(), self.weapon.as_deref());
                    if let Some(reply) = reply {
                        self.auto_answered.push(idx);
                        self.open = None;
                        return vec![format!("mr {server_id} {idx} {reply}")];
                    }
                }
                self.open = Some(OpenMenu {
                    index: idx,
                    name: name.clone(),
                });
                Vec::new()
            }
            Some("u") => {
                self.close();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    pub fn open(&self) -> Option<&OpenMenu> {
        self.open.as_ref()
    }

    /// The `mr` for the open menu, closing it. `None` if nothing is open.
    pub fn choose(&mut self, response: &str, server_id: i32) -> Option<String> {
        let open = self.open.take()?;
        Some(format!("mr {server_id} {} {response}", open.index))
    }

    /// Esc. The stock gametypes ignore the `close` response (`dm.gsc:242`),
    /// so no command goes out.
    pub fn close(&mut self) {
        self.open = None;
    }


    /// A new gamestate reruns retail's `ClientConnect`, which puts the
    /// client back on the menus (`JoinProbe::rearm`).
    pub fn on_gamestate(&mut self) {
        self.rearm();
    }

    /// `n`: under `dm`, a round restart reopens the team menu under the
    /// index it last used; under `sd`, whose `pers[]` survives, nothing
    /// reopens and this is inert (`JoinProbe::reopen_menus`).
    pub fn on_restart(&mut self) {
        self.rearm();
    }

    fn rearm(&mut self) {
        self.open = None;
        self.auto_answered.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cs() -> Vec<String> {
        let mut cs = vec![String::new(); 1300];
        cs[1180] = "team_russiangerman".into();
        cs[1181] = "weapon_russian".into();
        cs
    }
    fn toks(s: &str) -> Vec<String> {
        s.split(' ').map(str::to_string).collect()
    }

    #[test]
    fn t_opens_the_menu_its_configstring_names() {
        let mut j = Join::new(None, None);
        assert!(j
            .on_server_command(&toks("v g_scriptMainMenu team_russiangerman"), &cs(), 7)
            .is_empty());
        assert!(j.on_server_command(&toks("t 0"), &cs(), 7).is_empty());
        let open = j.open().unwrap();
        assert_eq!((open.index, open.name.as_str()), (0, "team_russiangerman"));
        assert_eq!(j.choose("allies", 7).as_deref(), Some("mr 7 0 allies"));
        assert!(j.open().is_none());
    }

    #[test]
    fn u_closes() {
        let mut j = Join::new(None, None);
        j.on_server_command(&toks("t 0"), &cs(), 7);
        j.on_server_command(&toks("u"), &cs(), 7);
        assert!(j.open().is_none());
    }

    #[test]
    fn auto_answers_team_then_weapon() {
        let mut j = Join::new(Some("axis".into()), Some("kar98k_mp".into()));
        assert_eq!(
            j.on_server_command(&toks("t 0"), &cs(), 3),
            vec!["mr 3 0 axis"]
        );
        assert_eq!(
            j.on_server_command(&toks("t 1"), &cs(), 3),
            vec!["mr 3 1 kar98k_mp"]
        );
        assert!(j.open().is_none());
    }

    #[test]
    fn auto_weapon_answers_once_per_open() {
        let mut j = Join::new(None, Some("bogus_mp".into()));
        assert_eq!(j.on_server_command(&toks("t 1"), &cs(), 3).len(), 1);
        // The server refused and reopened the same menu: now the user picks.
        assert!(j.on_server_command(&toks("t 1"), &cs(), 3).is_empty());
        assert_eq!(j.open().unwrap().name, "weapon_russian");
    }

    #[test]
    fn rearm_answers_again() {
        let mut j = Join::new(Some("allies".into()), None);
        assert_eq!(j.on_server_command(&toks("t 0"), &cs(), 3).len(), 1);
        j.on_restart();
        assert_eq!(
            j.on_server_command(&toks("t 0"), &cs(), 4),
            vec!["mr 4 0 allies"]
        );
        j.on_gamestate();
        assert_eq!(
            j.on_server_command(&toks("t 0"), &cs(), 5),
            vec!["mr 5 0 allies"]
        );
    }

    #[test]
    fn a_t_naming_an_empty_slot_opens_nothing() {
        let mut j = Join::new(None, None);
        assert!(j.on_server_command(&toks("t 9"), &cs(), 3).is_empty());
        assert!(j.open().is_none());
    }

    /// Index 32 is past the 32 script-menu slots: cs 1212 is the first hint
    /// string, not a menu.
    #[test]
    fn a_t_past_the_script_menu_range_opens_nothing() {
        let mut cs = cs();
        cs[1212] = "team_hintstring".into();
        let mut j = Join::new(Some("allies".into()), None);
        assert!(j.on_server_command(&toks("t 32"), &cs, 3).is_empty());
        assert!(j.open().is_none());
    }

}
