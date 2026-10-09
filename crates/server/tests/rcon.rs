use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};
use vcod_common::net::connectionless::build_oob;
use vcod_server::{Server, ServerConfig};

const A: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 32001);
const B: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 32002);

fn server(now: Instant) -> Server {
    Server::new(
        ServerConfig {
            map: "mp_carentan".into(),
            hostname: "alpha".into(),
            max_clients: 8,
            gametype: "dm".into(),
            test_entities: 0,
            trace: false,
            bots: 0,
            bots_shoot: false,
        },
        now,
    )
}

fn text(pkt: &[u8]) -> &str {
    std::str::from_utf8(&pkt[4..]).unwrap()
}

fn send(sv: &mut Server, from: SocketAddr, password: &str, command: &str, now: Instant) {
    sv.handle_packet(from, &build_oob(&format!("rcon {password} {command}")), now);
}

fn execute(
    sv: &mut Server,
    from: SocketAddr,
    password: &str,
    command: &str,
    now: Instant,
) -> String {
    send(sv, from, password, command, now);
    assert!(
        sv.take_outgoing().is_empty(),
        "authenticated RCON executes only on tick"
    );
    sv.tick(now);
    let out = sv.take_outgoing();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, from);
    text(&out[0].1).to_string()
}

#[test]
fn unset_or_empty_password_disables_rcon() {
    let now = Instant::now();
    let mut sv = server(now);
    send(&mut sv, A, "anything", "status", now);
    sv.tick(now);
    assert!(sv.take_outgoing().is_empty());

    sv.set_rcon_password("secret");
    sv.set_rcon_password("");
    send(&mut sv, A, "secret", "status", now);
    sv.tick(now);
    assert!(sv.take_outgoing().is_empty());
}

#[test]
fn authentication_happens_before_queueing_or_execution() {
    let now = Instant::now();
    let mut sv = server(now);
    sv.set_rcon_password("secret");

    send(&mut sv, A, "wrong", "set sv_hostname owned", now);
    let out = sv.take_outgoing();
    assert_eq!(out.len(), 1);
    assert!(text(&out[0].1).contains("Bad rconpassword"));

    let mut malformed_after_bad_auth = build_oob("rcon still-wrong ");
    malformed_after_bad_auth.push(0x80);
    sv.handle_packet(A, &malformed_after_bad_auth, now);
    let out = sv.take_outgoing();
    assert_eq!(out.len(), 1);
    assert!(text(&out[0].1).contains("Bad rconpassword"));

    let reply = execute(&mut sv, A, "secret", "get sv_hostname", now);
    assert!(reply.contains("sv_hostname = \"alpha\""));
}

#[test]
fn successful_requests_are_queued_until_the_tick_boundary() {
    let now = Instant::now();
    let mut sv = server(now);
    sv.set_rcon_password("secret");

    send(&mut sv, A, "secret", "set sv_hostname bravo", now);
    assert!(sv.take_outgoing().is_empty());
    sv.tick(now);
    assert!(text(&sv.take_outgoing()[0].1).contains("sv_hostname = \"bravo\""));

    let reply = execute(
        &mut sv,
        A,
        "secret",
        "serverinfo",
        now + Duration::from_millis(50),
    );
    assert!(reply.contains("hostname: bravo"));
}

#[test]
fn malformed_and_oversized_requests_are_rejected_without_execution() {
    let now = Instant::now();
    let mut sv = server(now);
    sv.set_rcon_password("secret");

    for packet in [
        build_oob("rcon"),
        build_oob("rcon secret"),
        build_oob("rcon secret \0status"),
        build_oob(&format!("rcon secret {}", "x".repeat(5000))),
    ] {
        sv.handle_packet(A, &packet, now);
    }
    sv.tick(now);
    assert!(sv.take_outgoing().len() <= 4);
    let reply = execute(&mut sv, B, "secret", "get sv_hostname", now);
    assert!(reply.contains("alpha"));
}

#[test]
fn per_source_packet_and_failed_auth_limits_do_not_block_another_source() {
    let now = Instant::now();
    let mut sv = server(now);
    sv.set_rcon_password("secret");

    for _ in 0..20 {
        send(&mut sv, A, "wrong", "status", now);
    }
    assert!(sv.take_outgoing().len() <= 5);

    let reply = execute(&mut sv, B, "secret", "version", now);
    assert!(reply.contains("vcod-server"));
}

#[test]
fn registry_exposes_safe_subset_and_omits_unsafe_commands() {
    let now = Instant::now();
    let mut sv = server(now);
    sv.set_rcon_password("secret");
    let reply = execute(&mut sv, A, "secret", "commands", now);
    for command in [
        "status",
        "players",
        "say",
        "tell",
        "kick",
        "map",
        "map_restart",
        "map_rotate",
        "gametype",
        "mapvote",
        "addbot",
        "botcount",
        "set",
        "get",
        "pause",
        "unpause",
        "dumpstats",
        "dumpprofile",
    ] {
        assert!(reply.contains(command), "missing {command}: {reply}");
    }
    for unsafe_command in ["ban", "mute", "exec"] {
        assert!(!reply.split_whitespace().any(|word| word == unsafe_command));
    }
}

#[test]
fn representative_information_mutation_and_error_commands_work() {
    let mut now = Instant::now();
    let mut sv = server(now);
    sv.set_rcon_password("secret");

    let help = execute(&mut sv, A, "secret", "help kick", now);
    assert!(help.contains("kick"));
    now += Duration::from_millis(50);
    assert!(execute(&mut sv, A, "secret", "players", now).contains("0 players"));
    now += Duration::from_millis(50);
    assert!(execute(&mut sv, A, "secret", "gametype tdm", now).contains("tdm"));
    now += Duration::from_millis(50);
    assert!(execute(&mut sv, A, "secret", "botcount 2", now).contains("botcount = 2"));
    now += Duration::from_millis(50);
    assert!(execute(&mut sv, A, "secret", "botsshoot 1", now).contains("botsshoot = 1"));
    now += Duration::from_millis(50);
    assert!(execute(&mut sv, A, "secret", "pause", now).contains("paused"));
    now += Duration::from_millis(50);
    assert!(execute(&mut sv, A, "secret", "unpause", now).contains("unpaused"));
    now += Duration::from_millis(50);
    assert!(execute(&mut sv, A, "secret", "no_such_command", now).contains("Unknown command"));
}

#[test]
fn response_output_is_bounded() {
    let now = Instant::now();
    let mut sv = server(now);
    sv.set_rcon_password("secret");
    let reply = execute(&mut sv, A, "secret", "commands", now);
    assert!(reply.len() <= 8192 + "print\n".len());
}

#[test]
fn local_console_uses_the_same_registry_and_mutation_path() {
    let now = Instant::now();
    let mut sv = server(now);
    sv.push_console("sv_hostname local-bravo");
    sv.tick(now);

    sv.set_rcon_password("secret");
    let reply = execute(
        &mut sv,
        A,
        "secret",
        "get sv_hostname",
        now + Duration::from_millis(50),
    );
    assert!(reply.contains("local-bravo"));
}

#[test]
fn passwords_are_never_returned_by_admin_commands() {
    let now = Instant::now();
    let mut sv = server(now);
    sv.set_rcon_password("rcon-secret");
    let reply = execute(&mut sv, A, "rcon-secret", "g_password join-secret", now);
    assert!(!reply.contains("join-secret"));
    assert!(!reply.contains("rcon-secret"));
    assert!(reply.contains("redacted"));
}

#[test]
fn rcon_bot_count_reserves_a_human_slot() {
    let now = Instant::now();
    let mut sv = server(now);
    sv.set_rcon_password("secret");
    let reply = execute(&mut sv, A, "secret", "botcount 8", now);
    assert!(reply.contains("botcount = 7"));
}
