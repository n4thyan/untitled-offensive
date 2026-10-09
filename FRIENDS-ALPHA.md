# UNTITLED friends alpha

This build is a private multiplayer alpha. It contains the UNTITLED client,
dedicated server, project scripts, configuration and documentation only. It does
**not** contain Call of Duty or Modern Warfare 2 files.

## Requirements

- Windows 10/11 x64.
- A legitimately purchased Call of Duty (2003) install with `main/pak0.pk3`.
  Patch 1.1 is the network target. A 1.5 install has compatible `pak0-4.pk3`
  assets, but joining retail servers is designed around protocol 1.1.
- A GPU/driver supported by wgpu and BC/DXT textures; the dedicated server is
  headless. Audio is optional.
- Optional: a legitimately purchased Steam install of Call of Duty: Modern
  Warfare 2 (2009) for the experimental native `mp_rust` loader. No MW2 data is
  copied into this package.

Do not send anyone your `main/*.pk3`, `zone/*.ff`, `.iwd`, `.iwi`, BSP, texture,
audio or model files. Each player supplies their own retail install.

## Install discovery and package layout

The client and server resolve CoD1 in this order:

1. `--game-dir PATH`
2. the `COD_DIR` environment variable
3. the directory containing the executable (useful only when the project
   executable is deliberately placed beside `CoDMP.exe`)
4. the current directory if the executable location cannot be determined

The supplied `run-dev-match.bat` also checks common CoD1 locations on `C:` and
`D:`. The most reliable setup is to set `COD_DIR` to the folder that contains
`main/pak0.pk3`:

```bat
set "COD_DIR=C:\Program Files (x86)\Call of Duty"
```

Project content can be layered without changing retail files:

```text
--overlay-dir PATH       optional directory of project-owned PK3 content
--classes-dir PATH       server-side Create-a-Class profiles
```

The friends package puts class profiles under `assets/classes`. The local-match
launcher passes that path explicitly. `--overlay-dir` is optional and is omitted
when `main-untitled` is not present.

MW2 discovery is currently read-only and checks these exact Steam locations:

- `C:\Program Files (x86)\Steam\steamapps\common\Call of Duty Modern Warfare 2`
- `C:\Program Files\Steam\steamapps\common\Call of Duty Modern Warfare 2`
- `D:\Steam\steamapps\common\Call of Duty Modern Warfare 2`

The install must contain `iw4mp.exe`, `zone`, and `zone/english/mp_rust.ff`.
There is not yet an MW2 path override. Rust uses the same frontend, dedicated
server, classes, bots and gameplay path as CoD1 maps, but remains hidden from
the friends playlist until invisible remote models and the full lifecycle gate
are fixed and verified.

## Fast local bot match

From the package directory:

```bat
run-dev-match.bat mp_carentan dm 3
```

The script starts `vcod-server.exe` on UDP 28960, waits 12 seconds, then joins
with `vcod.exe`. It writes logs to `dev-logs`, reads retail data in place, and
does not write into the retail install. Run `stop-dev-match.bat` when finished.

Starting `vcod.exe` with no map or `--connect` opens the frontend with no world
or server running behind it. **Play** exposes
the verified playlist, map, DM/TDM, bot count and bot-combat settings. **START
MATCH** starts the sibling `vcod-server.exe` on loopback and reconnects. A
client-owned local server is stopped by **Leave Match**. The menus accept both
mouse hover/click/wheel and Arrow keys/Enter/Escape.

## Dedicated host

The host supplies its own CoD1 install and runs:

```bat
vcod-server.exe --game-dir "C:\path\to\Call of Duty" --hostname "Friends Alpha" --port 28960 --gametype dm --max-clients 8 --bots 3 --bots-shoot --classes-dir "assets\classes" mp_carentan
```

The server binds all interfaces. Bots consume real client slots; when the bot
count fills `--max-clients`, the server grows the pool by one when possible so a
human can still join. Omit `--bots-shoot` for wandering-only debug bots. Use
`--set NAME=VALUE` for a supported game cvar, for example
`--set scr_friendlyfire=1`.

For LAN play, allow `vcod-server.exe` through Windows Firewall for the chosen
**UDP** port and give friends the host PC's LAN IPv4 address. Internet hosting
also requires forwarding that UDP port to the host. This alpha answers CoD 1.1
status/browser queries but does not provide NAT traversal or an in-game server
browser.

## Direct-IP / LAN join

```bat
vcod.exe --game-dir "C:\path\to\Call of Duty" --connect 192.168.1.50:28960
```

Optional automatic stock-menu choices:

```bat
vcod.exe --game-dir "C:\path\to\Call of Duty" --connect 192.168.1.50:28960 --team allies --weapon m1carbine_mp
```

`--team` accepts `allies`, `axis`, `autoassign` or `spectator`. A weapon rejected
by the server reopens the stock weapon menu. Custom maps/mod paks referenced by
a server may be downloaded into the joining player's selected CoD mod directory;
use only content you are licensed to receive.

## Menu and Create-a-Class

- **M**: open the multiplayer/script menu.
- **Arrow keys / Enter / Esc**: navigate, select and go back.
- Main menu: **Play**, **Create-a-Class**, disabled Stats/Options placeholders,
  and Quit.
- Pause menu: Resume, Change Class, disabled Options, Leave Match.
- Class selection is queued for the **next spawn**; it never changes an alive
  player's loadout mid-life.

Create-a-Class uses the server's authoritative profile. Files are one
`*.txt` profile per client name under the server's `--classes-dir`; a missing
profile receives the built-in default.

| Input | Create-a-Class action |
|---|---|
| B | Developer shortcut to open the editor while connected |
| Arrow keys | Move between slots |
| W / S | Cycle the selected slot's candidates |
| Enter | Apply the highlighted candidate |
| Backspace | Clear the slot |
| Tab | Save the profile on the server |
| Esc | Close the editor |

Perks are stored and validated but do not affect gameplay yet.

## Match controls

| Input | Action |
|---|---|
| WASD | Move |
| Mouse | Look |
| LMB / RMB | Fire / aim down sight |
| Space | Jump, or stand from crouch/prone |
| C / Ctrl | Crouch / prone |
| Q / E | Lean left / right |
| Shift | Sprint |
| V | Melee |
| G or 4 / wheel | Grenade slot / weapon selection |
| R | Reload |
| 1-4 | Weapon slots |
| F | Use, pick up, plant/defuse, or respawn |
| Tab | Scoreboard |
| Esc | Pause in a match; close a menu; otherwise release mouse |
| F3 / F4 | Debug overlay / culling mode |

Click the game window to capture the mouse.

## RCON

RCON is disabled unless the host passes a non-empty password. The server does
not enforce password complexity; the operator must choose a strong, unique
value:

```bat
vcod-server.exe --game-dir "C:\path\to\Call of Duty" --rcon-password "CHOOSE_A_STRONG_PASSWORD" mp_carentan
```

It uses the classic connectionless UDP request format: four `0xFF` bytes followed
by `rcon <password> <command>` on the game port. Use an id-tech-compatible
RCON client and start with `help`, `status` or `serverinfo`. Useful commands
include `say`, `tell`, `kick`, `map`, `map_restart`, `map_rotate`, `gametype`,
`mapvote`, `addbot`, `kickbot`, `kickbots`, `botcount`, `botsshoot`, `set`, `get`,
`pause`, `unpause`, `dumpstats` and `dumpprofile`.

RCON requests and output are bounded and rate-limited, authentication happens
before command parsing, and game mutations execute at the server tick boundary.
The password is never returned by commands or written to the game log. This
alpha intentionally omits persistent bans, mute state, arbitrary shell commands
and unrestricted config-file execution.

Classic RCON sends its password and commands over connectionless UDP without
encryption. Use it only on a trusted LAN or through a VPN, and firewall the game
port to trusted source addresses. Do not expose RCON directly to the Internet.

## Known alpha limitations

- Stats/Leaderboards and Options are visible but disabled.
- Only DM and TDM are offered by player-facing playlists. Rust and most
  community-map metadata remain unverified and hidden by playlist gates.
- Rust loads and bot combat is functional, but other player/bot models were
  invisible in the latest human playtest. They can still move, shoot and kill.
  Treat this as the first visual bug for the next session.
- Early server-authority logging can falsely report an unowned weapon during
  death/killcam or weapon-transition states. It is log-only and does not ban,
  but the ownership check should be gated to a live `PmType::Normal` player.
- Bots are debug opponents, not production AI.
- Mounted MG server simulation exists, but the client does not draw the gun.
- Some HUD pieces, mover prediction, other-player footsteps and several visual
  fidelity details remain incomplete. There is no mantling, matching CoD 1.1 MP.
- There is no in-game LAN/server browser; host and join by direct IP.
- A stale server already holding UDP 28960 can make a new host fail to bind;
  stop old `vcod-server.exe` processes and inspect `dev-logs/server.log`.

## Package verification (maintainers)

From a source checkout, the packager builds both release executables, creates a
clean guarded directory on `G:`, writes SHA-256 entries, scans for prohibited
retail extensions, and verifies the result:

```bat
python tools\package_friends_alpha.py
python tools\package_friends_alpha.py --check
```

The default output is `G:\untitled-builds\friends-alpha`. The script refuses to
clean a directory it did not create. The package intentionally includes only the
two project executables, these docs/licenses, project launch scripts and
`assets/classes/*.txt`.
