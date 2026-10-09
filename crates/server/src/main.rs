//! `vcod-server`, a CoD 1.1 dedicated server in progress. Answers browsers,
//! accepts connections, sends the gamestate and uncompressed snapshots with
//! spectator flight.

use anyhow::{bail, Context, Result};
use clap::Parser;
use std::net::UdpSocket;
use std::time::{Duration, Instant};
use vcod_common::pk3::Pk3Fs;
use vcod_server::{Server, ServerConfig};

#[derive(Parser)]
#[command(about = "Call of Duty (2003) 1.1 dedicated server, in progress")]
struct Args {
    /// Map name, e.g. mp_carentan
    map: String,
    /// Game install; defaults to $COD_DIR, else the executable's directory
    #[arg(long, default_value_os_t = vcod_common::game_dir::default_game_dir())]
    game_dir: std::path::PathBuf,
    /// Pk3 subdirectory, main for CoD1 or uo for United Offensive
    #[arg(long, default_value = "main")]
    mod_dir: String,
    /// A pk3 directory searched BEFORE --mod-dir, so original project content
    /// (custom maps) layers over a retail install without modifying it.
    #[arg(long)]
    overlay_dir: Option<std::path::PathBuf>,
    /// UDP port
    #[arg(long, default_value_t = 28960)]
    port: u16,
    /// sv_hostname
    #[arg(long, default_value = "vcod")]
    hostname: String,
    /// sv_maxclients
    #[arg(long, default_value_t = 8)]
    max_clients: usize,
    /// g_gametype: the script under maps/mp/gametypes to run
    #[arg(long, default_value = "dm")]
    gametype: String,
    /// A gametype script from disk instead of the paks: overlaid at
    /// maps/mp/gametypes/<stem> and run as gametype <stem>. For the client
    /// probes in crates/gsc/tests/fixtures/semantics/client-probes/.
    #[arg(long)]
    gametype_script: Option<std::path::PathBuf>,
    /// Scripted entities that exercise the packet-entity wire path. 0 is off.
    #[arg(long, default_value_t = 0)]
    test_entities: usize,
    /// Debug bots in play. Each takes a real client slot; 0 is off.
    #[arg(long, default_value_t = 0)]
    bots: usize,
    /// Whether the bots fight. Without it they only wander.
    #[arg(long)]
    bots_shoot: bool,
    /// A cvar to set before the scripts load, retail's `+set name value`;
    /// repeatable, e.g. `--set scr_friendlyfire=1`.
    #[arg(long = "set", value_name = "NAME=VALUE")]
    set: Vec<String>,

    /// A directory of Create-a-Class `*.txt` profiles, one file per client
    /// name (the file stem is the name). Defaults to `assets/classes`.
    #[arg(long, default_value = "assets/classes")]
    classes_dir: std::path::PathBuf,

    /// Log one line per snapshot per client: send interval, the serverTime and
    /// commandTime a client predicts from, the usercmds consumed, and whether
    /// the frame went out as a delta.
    #[arg(long)]
    trace: bool,
}

/// `sv_fps 20`.
const FRAME: Duration = Duration::from_millis(50);

/// How far behind the schedule the loop still runs ticks back to back
/// before it resynchronises instead.
const MAX_CATCH_UP: Duration = Duration::from_secs(1);

/// Without a bound a flood keeps the socket readable, `tick` never runs and
/// the outbox is never flushed.
const MAX_PACKETS_PER_FRAME: usize = 256;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let mut args = Args::parse();
    if args.test_entities > vcod_server::world::MAX_TEST_ENTITIES {
        bail!(
            "--test-entities {} exceeds {}, the most that fits below ENTITYNUM_WORLD",
            args.test_entities,
            vcod_server::world::MAX_TEST_ENTITIES
        );
    }
    // Bots and the human share one slot pool, so a bot count that fills the
    // pool leaves no room for the player: `--bots 8` against the default
    // `--max-clients 8` filled every slot and the client was refused with
    // EXE_SERVERISFULL. Grow the pool by one rather than refusing, so the
    // recording setup works without a flag the player should not need.
    if args.bots >= args.max_clients {
        let grown = args.bots + 1;
        log::info!(
            "--bots {} fills --max-clients {}; growing the pool to {} so the player has a slot",
            args.bots,
            args.max_clients,
            grown
        );
        args.max_clients = grown.min(vcod_server::server::MAX_CLIENTS);
        if args.bots >= args.max_clients {
            bail!(
                "--bots {} exceeds --max-clients {} even after growing; each bot takes a real slot",
                args.bots,
                args.max_clients
            );
        }
    }
    let dir = args.game_dir.join(&args.mod_dir);
    let fs = std::rc::Rc::new(
        Pk3Fs::open_layered(&dir, args.overlay_dir.as_deref())
            .with_context(|| format!("opening game data in {}", dir.display()))?,
    );
    // A missing or corrupt map fails here rather than at first use. The same
    // seam handles native CoD1 BSPs and normalized IW4 fastfiles.
    let bsp = vcod_server::map_loader::load(&fs, &args.map)?;

    let overlay = match &args.gametype_script {
        Some(path) => {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .context("--gametype-script needs a file name")?
                .to_string();
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            Some((stem, text))
        }
        None => None,
    };
    let gametype = overlay
        .as_ref()
        .map_or(args.gametype.clone(), |(s, _)| s.clone());

    let sock = UdpSocket::bind(("0.0.0.0", args.port))
        .with_context(|| format!("binding udp/{}", args.port))?;
    sock.set_nonblocking(true)?;
    log::info!("vcod-server: {} on udp/{}", args.map, args.port);
    let mut server = Server::new(
        ServerConfig {
            map: args.map,
            hostname: args.hostname,
            max_clients: args.max_clients,
            gametype,
            test_entities: args.test_entities,
            bots: args.bots,
            bots_shoot: args.bots_shoot,
            trace: args.trace,
        },
        Instant::now(),
    );
    server.load_classes(&args.classes_dir);
    server.load_bsp(&bsp, Some(&fs));
    if let Some((stem, text)) = &overlay {
        server.overlay_script(&format!("maps/mp/gametypes/{stem}"), text);
    }
    for pair in &args.set {
        let Some((name, value)) = pair.split_once('=') else {
            bail!("--set takes NAME=VALUE, got {pair:?}");
        };
        server.set_cvar(name, value);
    }
    // A failed script load is fatal, not a warning. The configstring table is
    // mostly script output now, so a server that kept serving would hand
    // clients a gamestate with no team menu, no status icons and no shaders,
    // which reads as a protocol bug rather than a script failure.
    if let Err(e) = server.load_scripts(fs.clone()) {
        log::error!("loading the map and gametype scripts: {e:#}");
        std::process::exit(1);
    }
    let mut buf = vec![0u8; 65536];
    // A fixed schedule, not a sleep after each tick: `SV_Frame` runs one
    // game frame per `sv_fps` slice of wall time and catches up when a
    // frame overran, so `svs.time` tracks the wall clock. Sleeping the
    // remainder of each tick lets every overshoot accumulate, which read as
    // a serverTime 5-10% slow against a probe's wall clock under load.
    let mut next = Instant::now();
    loop {
        let now = Instant::now();
        for _ in 0..MAX_PACKETS_PER_FRAME {
            let Ok((n, from)) = sock.recv_from(&mut buf) else {
                break;
            };
            server.handle_packet(from, &buf[..n], now);
        }
        server.tick(now);
        // A level load that failed with the level already torn down: there is
        // no script to end the level and no table for a client to pull, which
        // is what retail's `Com_Error` ends the process for.
        if let Some(e) = server.take_fatal() {
            log::error!("{e:#}");
            std::process::exit(1);
        }
        for (to, pkt) in server.take_outgoing() {
            if let Err(e) = sock.send_to(&pkt, to) {
                log::debug!("send to {to}: {e}");
            }
        }
        next += FRAME;
        let after = Instant::now();
        if next > after {
            std::thread::sleep(next - after);
        } else if after - next > MAX_CATCH_UP {
            // A stall past the catch-up bound (a debugger, a suspend) is
            // dropped rather than replayed as a burst of ticks.
            next = after;
        }
    }
}
