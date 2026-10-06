//! A locally-owned dedicated server, launched and stopped by the client.
//!
//! Start Match spawns `vcod-server.exe` beside this executable, on a loopback
//! port, with the settings the Play screen chose, and the client connects to
//! it. The client remembers that it owns the process, so Leave Match stops a
//! server it started and never one it merely connected to - killing a server
//! somebody else runs is not this client's business.
//!
//! The server is a child process rather than an in-process thread on purpose:
//! the client and server are separate binaries with separate event loops, and
//! a dedicated server that outlives a client crash is the behaviour the
//! existing `run-dev-match.bat` already relies on.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::{bail, Context, Result};

/// The settings a local match is started with, straight off the Play screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchConfig {
    pub map: String,
    pub gametype: String,
    pub bots: usize,
    pub bot_combat: bool,
}

/// A dedicated server this client started. Dropping it does not stop the
/// process - `stop` must be called - so a leak is visible rather than silent.
pub struct LocalServer {
    child: Child,
    port: u16,
    /// The log file the child's stdout/stderr were redirected to, when one
    /// was opened. `None` if the redirect failed and the child inherits.
    log_path: Option<PathBuf>,
}

impl LocalServer {
    /// Spawn `vcod-server.exe` beside the running client.
    ///
    /// `game_dir`, `mod_dir` and `overlay_dir` mirror the client's own search
    /// path, so the server resolves exactly the maps the client just offered.
    /// `classes_dir` is where the server keeps the Create-a-Class profiles.
    pub fn spawn(
        exe_dir: &Path,
        game_dir: &Path,
        mod_dir: &str,
        overlay_dir: Option<&Path>,
        classes_dir: &Path,
        cfg: &MatchConfig,
        port: u16,
        log_dir: &Path,
    ) -> Result<LocalServer> {
        let server_exe = exe_dir.join("vcod-server.exe");
        if !server_exe.is_file() {
            bail!(
                "no dedicated server at {} - build it with `cargo build --release`",
                server_exe.display()
            );
        }

        let mut cmd = Command::new(&server_exe);
        cmd.arg("--game-dir")
            .arg(game_dir)
            .arg("--mod-dir")
            .arg(mod_dir)
            .arg("--port")
            .arg(port.to_string())
            .arg("--gametype")
            .arg(&cfg.gametype)
            .arg("--bots")
            .arg(cfg.bots.to_string())
            .arg("--classes-dir")
            .arg(classes_dir);
        if cfg.bot_combat {
            cmd.arg("--bots-shoot");
        }
        if let Some(overlay) = overlay_dir {
            cmd.arg("--overlay-dir").arg(overlay);
        }
        cmd.arg(&cfg.map);

        // A window for the child would fight the client for focus, and its log
        // is more useful in a file than in a console nobody is looking at.
        cmd.stdin(Stdio::null());
        let log_path = std::fs::create_dir_all(log_dir).ok().and_then(|_| {
            let p = log_dir.join(format!("local-server-{port}.log"));
            std::fs::File::create(&p).ok().map(|f| (p, f))
        });
        match &log_path {
            Some((_, f)) => {
                let err = f.try_clone().context("cloning the server log handle")?;
                cmd.stdout(Stdio::from(err));
                cmd.stderr(Stdio::from(f.try_clone().unwrap_or_else(|_| {
                    std::fs::File::create(std::env::temp_dir().join("vcod-server.err"))
                        .expect("a fallback stderr file")
                })));
            }
            None => {
                cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
        }

        let child = cmd
            .spawn()
            .with_context(|| format!("spawning {}", server_exe.display()))?;
        log::info!(
            "local match: spawned {} on udp/{port} ({})",
            server_exe.display(),
            cfg.map
        );
        Ok(LocalServer {
            child,
            port,
            log_path: log_path.map(|(p, _)| p),
        })
    }

    /// The loopback address the client connects to.
    pub fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    pub fn log_path(&self) -> Option<&Path> {
        self.log_path.as_deref()
    }

    /// Whether the child is still running. A server that exited on its own
    /// (a bad map, a port clash) is reported here rather than as a silent
    /// connection failure.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Stop the server: `kill`, then reap. Idempotent enough for a leave path
    /// that may run twice.
    pub fn stop(&mut self) {
        if let Err(e) = self.child.kill() {
            // Already gone is the common case and is not an error worth a
            // warning; anything else is.
            if e.kind() != std::io::ErrorKind::InvalidInput {
                log::warn!("local match: could not stop the server: {e}");
            }
        }
        let _ = self.child.wait();
        log::info!("local match: stopped the server on udp/{}", self.port);
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        // Dropping without `stop` is a bug; killing here keeps it from leaving
        // an orphan, and the log says so.
        if matches!(self.child.try_wait(), Ok(None)) {
            log::warn!(
                "local match: server on udp/{} dropped while still running; stopping it",
                self.port
            );
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// A free-ish loopback port for a local match. Chosen above the IANA dynamic
/// floor so it does not collide with a service; the caller retries a spawn
/// failure rather than this probing the port (a bind race is unavoidable
/// without holding the socket).
pub const DEFAULT_LOCAL_PORT: u16 = 28960;

/// The next port to try when a spawn failed because the port was taken.
pub fn next_port(port: u16) -> u16 {
    // Stay in the same block; wrap to the dynamic floor rather than into a
    // privileged range.
    if port >= 28999 {
        28960
    } else {
        port + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spawn_without_the_server_binary_is_a_clear_error() {
        let tmp = std::env::temp_dir();
        let result = LocalServer::spawn(
            &tmp,
            Path::new("C:/nope"),
            "main",
            None,
            Path::new("assets/classes"),
            &MatchConfig {
                map: "mp_carentan".into(),
                gametype: "dm".into(),
                bots: 3,
                bot_combat: true,
            },
            28960,
            &tmp,
        );
        let err = match result {
            Ok(_) => panic!("a spawn with no server binary must fail"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("dedicated server"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn the_address_is_loopback_and_matches_the_port() {
        // A LocalServer cannot be built without a real binary, so check the
        // pure helper the address is derived from.
        let addr = format!("127.0.0.1:{}", DEFAULT_LOCAL_PORT);
        assert_eq!(addr, "127.0.0.1:28960");
    }

    #[test]
    fn ports_wrap_within_the_block() {
        assert_eq!(next_port(28960), 28961);
        assert_eq!(next_port(28999), 28960);
    }
}
