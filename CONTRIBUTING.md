# Contributing

## Development setup

1. Install Rust 1.90 or newer
2. Install Call of Duty (2003) and set `COD_DIR` or use `--game-dir`
3. Run `cargo build --release`

## Building

```bash
cargo build --release
```

For development:

```bash
cargo build
```

## Testing

```bash
cargo test
```

Tests that need retail game data check for `COD_DIR` and return early if not set.

## Running locally

```bash
# Start a dedicated server with bots
vcod-server mp_carentan --bots 4 --gametype dm

# Connect as a client
vcod --connect 127.0.0.1:28960
```

## Code style

- `cargo fmt` before committing
- `cargo clippy --workspace --all-targets -- -D warnings` must pass

## Commit messages

Use conventional commit format:

- `feat:` for new features
- `fix:` for bug fixes
- `docs:` for documentation
- `test:` for tests
- `chore:` for maintenance

## Legal

This is a fork of vcod, which is GPL-3.0-or-later. Contributions are
licensed under GPL-3.0-or-later. No proprietary Call of Duty assets
are ever contributed or distributed.
