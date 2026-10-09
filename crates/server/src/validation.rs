//! Conservative server-authoritative input discipline.
//!
//! This layer corrects only values the server can disprove from its own clock
//! or playerstate. It records bounded strikes for operator visibility; it does
//! not inspect clients, infer intent, kick, or ban.

use std::fmt;

use vcod_common::net::msg::{UserCmd, WBUTTON_CROUCH, WBUTTON_PRONE};

/// `ClientThink_real`'s measured upper usercmd clock window.
pub const USERCMD_FUTURE_MS: i32 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    FutureUsercmd,
    UsercmdPastBlockEnd,
    InvalidWeapon,
    UnownedWeapon,
    ContradictoryStance,
    MalformedPacket,
    PacketFlood,
    UsercmdFlood,
    InvalidLoadout,
    InvalidClass,
    ActionOutsidePlayingState,
    FireOutsidePlayingState,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Reason::FutureUsercmd => "usercmd clock is ahead of the authoritative window",
            Reason::UsercmdPastBlockEnd => "usercmd clock is past its move block's final cmd",
            Reason::InvalidWeapon => "usercmd names no server weapon",
            Reason::UnownedWeapon => "usercmd requests a weapon the player does not own",
            Reason::ContradictoryStance => "usercmd holds crouch and prone together",
            Reason::MalformedPacket => "connected packet is malformed or out of range",
            Reason::PacketFlood => "connected packet rate exceeded the frame budget",
            Reason::UsercmdFlood => "queued usercmds exceeded the bounded replay budget",
            Reason::InvalidLoadout => "loadout edit names an invalid value",
            Reason::InvalidClass => "class selection is out of range",
            Reason::ActionOutsidePlayingState => "action is impossible in the authoritative state",
            Reason::FireOutsidePlayingState => "fire is impossible in the authoritative state",
        };
        f.write_str(text)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CommandContext {
    pub level_time_ms: i32,
    /// Command clock at the start of this authoritative tick. A client that
    /// entered slightly ahead may progress, but cannot compound the allowance
    /// with every cmd in a burst.
    pub accepted_clock_ms: i32,
    pub current_weapon: u8,
    pub weapons_held: u64,
    /// Number of indexed weapon definitions. Index zero is the empty weapon.
    pub weapon_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Fire,
    Kill,
    SelectClass,
}

#[derive(Clone, Copy, Debug)]
pub struct ActionState {
    pub active: bool,
    pub alive: bool,
}

/// Per-client evidence counter. Saturation keeps hostile input from wrapping;
/// there is intentionally no enforcement escalation beyond log/strike mode.
#[derive(Clone, Debug, Default)]
pub struct Discipline {
    strikes: u16,
}

impl Discipline {
    pub fn strikes(&self) -> u16 {
        self.strikes
    }

    /// Records one signal and returns whether this occurrence should be logged.
    /// Power-of-two sampling keeps a persistent offender clear without flooding
    /// the server log.
    pub fn strike(&mut self, _reason: Reason) -> bool {
        self.strikes = self.strikes.saturating_add(1);
        self.strikes.is_power_of_two() || self.strikes == u16::MAX
    }

    pub fn should_log(&self, _reason: Reason) -> bool {
        self.strikes.is_power_of_two() || self.strikes == u16::MAX
    }

    /// Log/strike is the default and only policy: no automatic bans.
    pub fn should_ban(&self) -> bool {
        false
    }

    /// Bounds only values disproved by the server. The command remains usable,
    /// so valid movement in the same packet is not collateral damage.
    pub fn validate_usercmd(&mut self, cmd: &mut UserCmd, context: CommandContext) -> Vec<Reason> {
        let mut reasons = Vec::new();
        let latest = context
            .level_time_ms
            .max(context.accepted_clock_ms)
            .saturating_add(USERCMD_FUTURE_MS);
        if cmd.server_time > latest {
            cmd.server_time = latest;
            reasons.push(Reason::FutureUsercmd);
        }

        if cmd.weapon != 0 {
            if usize::from(cmd.weapon) >= context.weapon_count {
                cmd.weapon = context.current_weapon;
                reasons.push(Reason::InvalidWeapon);
            } else if cmd.weapon >= 64 || context.weapons_held & (1_u64 << cmd.weapon) == 0 {
                cmd.weapon = context.current_weapon;
                reasons.push(Reason::UnownedWeapon);
            }
        }

        if cmd.wbuttons & (WBUTTON_CROUCH | WBUTTON_PRONE) == WBUTTON_CROUCH | WBUTTON_PRONE {
            // Retail's input precedence remains untouched; this is evidence,
            // not an invented movement rule.
            reasons.push(Reason::ContradictoryStance);
        }

        for reason in &reasons {
            self.strike(*reason);
        }
        reasons
    }

    /// Retail's `SV_UserMove` skips an earlier entry whose clock is beyond the
    /// block's final cmd. Keeping it would process a forged jump and make the
    /// real final cmd stale.
    pub fn validate_usercmd_block(&mut self, mut cmds: Vec<UserCmd>) -> Vec<UserCmd> {
        let Some(last_time) = cmds.last().map(|cmd| cmd.server_time) else {
            return cmds;
        };
        let before = cmds.len();
        cmds.retain(|cmd| cmd.server_time <= last_time);
        if cmds.len() != before {
            self.strike(Reason::UsercmdPastBlockEnd);
        }
        cmds
    }

    /// State-action checks are deliberately narrow. Class selection is valid
    /// while alive because it is deferred to spawn. Fire/kill are impossible
    /// only when the authoritative state says the client is not live play.
    pub fn validate_action(&mut self, action: Action, state: ActionState) -> Option<Reason> {
        let reason = match action {
            Action::SelectClass => None,
            Action::Kill if !state.active || !state.alive => {
                Some(Reason::ActionOutsidePlayingState)
            }
            Action::Fire if !state.active || !state.alive => Some(Reason::FireOutsidePlayingState),
            Action::Kill | Action::Fire => None,
        };
        if let Some(reason) = reason {
            self.strike(reason);
        }
        reason
    }
}
