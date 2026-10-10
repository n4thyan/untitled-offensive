# UNTITLED OFFENSIVE

An experimental classic Call of Duty sandbox built on [vcod](https://github.com/janost/vcod),
a Rust reimplementation of parts of Call of Duty (2003) patch 1.1.

This project started as an experiment with the CoD1 engine and has grown into
a sandbox for classic Call of Duty gameplay, maps and systems. It keeps
CoD1-style gameplay while expanding toward custom maps, Create-a-Class,
killcam, bots, sprint, dedicated server support, RCON and experimental
IW4 map loading.

## Current Highlights

- Worldless boot frontend with mouse/keyboard navigation
- Playlist system with CoD1 stock maps and community maps
- IW4/MW2 experimental native launcher (Rust world)
- Create-a-Class persistence with server-authoritative loadouts
- Killcam on death
- Dedicated server with authenticated RCON
- Sprint and mantle (experimental)

## Content / Map Support

| Content | Status |
| --- | --- |
| Call of Duty 1 stock maps | Supported |
| Community CoD1 maps | Supported |
| IW4 / MW2 maps | Experimental |
| Source 1 maps | Planned |
| Other engine adapters | Research |

## Current Status

This is Alpha software, actively in development. Features are implemented
incrementally and tested against retail game content.

## Requirements

- Windows x64
- Legitimate Call of Duty (2003) installation with `main/pak0.pk3`
- Optional: Modern Warfare 2 (2009) for experimental IW4 map content

This project does not redistribute proprietary game assets. You must provide
content from installations you legitimately own.

## Building

Prerequisites: Rust 1.90+

```bash
cargo build --release
```

This builds `target/release/vcod` (client) and `target/release/vcod-server`.

## Running

```bash
# Run a local match with a retail CoD1 install
run-dev-match.bat

# Or manually
vcod-server mp_carentan --bots 4 --gametype dm
vcod --connect 127.0.0.1:28960
```

See `FRIENDS-ALPHA.md` for detailed installation and package layout.

## Credits

UNTITLED OFFENSIVE is built on [vcod](https://github.com/janost/vcod) by janost.
vcod is a from-scratch Rust reimplementation of parts of Call of Duty (2003)
patch 1.1, including the engine, rendering, networking and scripting
groundwork. This project extends vcod toward a sandbox for classic Call of Duty
gameplay, maps and systems.

Additional credits and lineage are in the [vcod README](https://github.com/janost/vcod#lineage-and-inspiration)
and the original project's [NOTICE](NOTICE) file.

## Development

UNTITLED OFFENSIVE is developed with a mixture of manual work and AI-assisted
development. AI tools are used for things like codebase inspection, research,
debugging, implementation, testing and documentation. The direction of the
project is decided manually, and changes are checked against the actual game
and project before they are treated as working. Visual and gameplay changes
also get human testing because passing a test does not necessarily mean
something looks or plays correctly.

Reverse engineering and compatibility work is based on evidence from the
original games, existing research, runtime behaviour and the code, rather
than made-up assumptions.

## Legal

UNTITLED OFFENSIVE is not affiliated with, endorsed by, or connected to
Activision, Infinity Ward, Treyarch or any other Activision company.
Call of Duty is a trademark of Activision Publishing, Inc.

No proprietary Call of Duty assets are distributed here. Users must provide
content from legitimately owned installations.

GPL-3.0-or-later; see [LICENSE](LICENSE).