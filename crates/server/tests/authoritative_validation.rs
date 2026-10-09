use vcod_common::net::msg::{UserCmd, NULL_USERCMD, WBUTTON_CROUCH, WBUTTON_PRONE};
use vcod_server::validation::{
    Action, ActionState, CommandContext, Discipline, Reason, USERCMD_FUTURE_MS,
};

fn active() -> CommandContext {
    CommandContext {
        level_time_ms: 10_000,
        accepted_clock_ms: 10_000,
        current_weapon: 10,
        weapons_held: 1 << 10,
        weapon_count: 32,
    }
}

#[test]
fn normal_commands_are_unchanged_and_do_not_strike() {
    let mut discipline = Discipline::default();
    let mut cmd = UserCmd {
        server_time: 10_050,
        weapon: 10,
        forward: 127,
        ..NULL_USERCMD
    };

    let reasons = discipline.validate_usercmd(&mut cmd, active());

    assert!(reasons.is_empty());
    assert_eq!(cmd.server_time, 10_050);
    assert_eq!(cmd.forward, 127);
    assert_eq!(cmd.weapon, 10);
    assert_eq!(discipline.strikes(), 0);
}

#[test]
fn an_impossible_future_timestamp_is_bounded_to_retails_window() {
    let mut discipline = Discipline::default();
    let mut cmd = UserCmd {
        server_time: 99_000,
        weapon: 10,
        forward: 127,
        ..NULL_USERCMD
    };

    let reasons = discipline.validate_usercmd(&mut cmd, active());

    assert_eq!(cmd.server_time, 10_000 + USERCMD_FUTURE_MS);
    assert_eq!(cmd.forward, 127, "only the impossible clock is corrected");
    assert_eq!(reasons, vec![Reason::FutureUsercmd]);
    assert_eq!(discipline.strikes(), 1);
}

#[test]
fn an_existing_connect_time_lead_can_progress_but_not_compound_in_a_burst() {
    let mut discipline = Discipline::default();
    let context = CommandContext {
        accepted_clock_ms: 12_000,
        ..active()
    };
    let mut ordinary = UserCmd {
        server_time: 12_050,
        weapon: 10,
        ..NULL_USERCMD
    };
    let mut burst_jump = UserCmd {
        server_time: 12_500,
        weapon: 10,
        ..NULL_USERCMD
    };

    assert!(discipline
        .validate_usercmd(&mut ordinary, context)
        .is_empty());
    assert_eq!(
        discipline.validate_usercmd(&mut burst_jump, context),
        vec![Reason::FutureUsercmd]
    );
    assert_eq!(burst_jump.server_time, 12_000 + USERCMD_FUTURE_MS);
}

#[test]
fn a_cmd_past_its_own_blocks_last_timestamp_is_removed_retail_style() {
    let mut discipline = Discipline::default();
    let cmds = [100, 500, 200]
        .into_iter()
        .map(|server_time| UserCmd {
            server_time,
            ..NULL_USERCMD
        })
        .collect();

    let kept = discipline.validate_usercmd_block(cmds);

    assert_eq!(
        kept.iter().map(|cmd| cmd.server_time).collect::<Vec<_>>(),
        vec![100, 200]
    );
    assert_eq!(discipline.strikes(), 1);
}

#[test]
fn an_unknown_or_unowned_weapon_is_replaced_by_authoritative_state() {
    let mut discipline = Discipline::default();
    let mut unknown = UserCmd {
        server_time: 10_050,
        weapon: 200,
        ..NULL_USERCMD
    };
    let mut unowned = UserCmd {
        server_time: 10_100,
        weapon: 11,
        ..NULL_USERCMD
    };

    assert_eq!(
        discipline.validate_usercmd(&mut unknown, active()),
        vec![Reason::InvalidWeapon]
    );
    assert_eq!(unknown.weapon, 10);
    assert_eq!(
        discipline.validate_usercmd(&mut unowned, active()),
        vec![Reason::UnownedWeapon]
    );
    assert_eq!(unowned.weapon, 10);
}

#[test]
fn representable_but_contradictory_stance_is_signalled_without_rewriting_movement() {
    let mut discipline = Discipline::default();
    let mut cmd = UserCmd {
        server_time: 10_050,
        weapon: 10,
        wbuttons: WBUTTON_CROUCH | WBUTTON_PRONE,
        right: -127,
        ..NULL_USERCMD
    };

    assert_eq!(
        discipline.validate_usercmd(&mut cmd, active()),
        vec![Reason::ContradictoryStance]
    );
    assert_eq!(cmd.wbuttons, WBUTTON_CROUCH | WBUTTON_PRONE);
    assert_eq!(cmd.right, -127);
}

#[test]
fn only_authoritatively_impossible_actions_strike() {
    let alive = ActionState {
        active: true,
        alive: true,
    };
    let dead = ActionState {
        active: true,
        alive: false,
    };
    let mut discipline = Discipline::default();

    assert!(discipline
        .validate_action(Action::SelectClass, alive)
        .is_none());
    assert_eq!(
        discipline.validate_action(Action::Fire, dead),
        Some(Reason::FireOutsidePlayingState)
    );
    assert_eq!(
        discipline.validate_action(Action::Kill, dead),
        Some(Reason::ActionOutsidePlayingState)
    );
}

#[test]
fn repeated_impossible_behavior_is_counted_but_never_bans() {
    let mut discipline = Discipline::default();
    for _ in 0..=u16::MAX {
        discipline.strike(Reason::MalformedPacket);
    }

    assert_eq!(discipline.strikes(), u16::MAX);
    assert!(!discipline.should_ban());
    assert!(discipline.should_log(Reason::MalformedPacket));
}
