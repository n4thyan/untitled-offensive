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
There is not yet an MW2 path override. Rust is experimental and remains hidden
from verified playlists; a direct local viewer launch is `vcod.exe mp_rust
--game-dir "%COD_DIR%"`.

## Fast local bot match

From the package directory:

```bat
run-dev-match.bat mp_carentan dm 3
```

The script starts `vcod-server.exe` on UDP 28960, waits 12 seconds, then joins
with `vcod.exe`. It writes logs to `dev-logs`, reads retail data in place, and
does not write into the retail install. Run `stop-dev-match.bat` when finished.

Inside a connected match, press **M** for the multiplayer menu. **Play** exposes
the verified playlist, map, DM/TDM, bot count and bot-combat settings. **START
MATCH** starts the sibling `vcod-server.exe` on loopback and reconnects. A
client-owned local server is stopped by **Leave Match**. Starting `vcod.exe`
without a map or `--connect` is not yet a menu entry point.

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

**Pending integration.** This base does not expose an RCON password, protocol or
administrator command set. Do not expose the server on the assumption that it
can be remotely administered. This section must be replaced with tested commands
after the RCON branch is integrated.

## Known alpha limitations

- The normal entry path is a direct connection or map argument; menu-only startup
  is not wired yet.
- Stats/Leaderboards and Options are visible but disabled.
- Only DM and TDM are offered by player-facing playlists. `mp_rust` and most
  community-map metadata remain unverified and are hidden by playlist gates.
- Bots are debug opponents, not production AI.
- Mounted MG server simulation exists, but the client does not draw the gun.
- Some HUD pieces, mover prediction, other-player footsteps and several visual
  fidelity details remain incomplete. There is no mantling, matching CoD 1.1 MP.
- RCON and an in-game LAN/server browser are not integrated.
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
