# Authoritative server tick and input discipline

This document maps the server's real receive/tick/send path. It is not a proposed phase loop. Retail-compatible ordering already measured in `AGENTS.md` ("The tick, in order") and the cited research documents remains the constraint.

## CURRENT ORDER

### Transport boundary

1. `main.rs` drains at most `MAX_PACKETS_PER_FRAME` UDP datagrams in socket order.
2. `Server::handle_packet` handles connectionless traffic immediately. A valid connected message is fully decoded before any op commits; its packet receives one monotonic `packet_seq`.
3. Client commands are walked before that packet's move block, matching measured `SV_ExecuteClientMessage` ordering. Script-releasing `mr` commands are deferred to the tick's script-command boundary. A `kill` is stamped with the packet number and replays before that packet's usercmds.
4. `Server::tick` builds the authoritative frame and queues netchan output.
5. `main.rs` drains `take_outgoing()` and calls `send_to` in the queued order.

### Authoritative tick stages

| Stage | Current implementation | Ordering constraint |
|---|---|---|
| Deferred lifecycle/console | `tick`: `drain_console`, map vote, timeouts | A prior script frame's map/restart command executes before this level does more work. Drops happen before bot input and simulation. |
| Synthetic input | `step_bots` | Bots use a `BTreeMap` slot walk and receive packet numbers after already-received human packets. They do not bypass usercmd replay. |
| Frame clock | increment `sv_time_ms` by `FRAME_MS` | Usercmds are replayed on the new frame clock; script-releasing packet commands do not run on the previous frame's clock. |
| Packet replay | `replay_moves` | Globally lowest `packet_seq` first; a packet's `kill` precedes its cmds. Ties are deterministic by item kind then slot. Each cmd performs move, immediate weapon/melee/grenade effects, and touch/use in the measured order. |
| Missile/blast | `run_missiles`, radius conversion, `deliver_hits` | This tick's thrown missile gets its measured first flight; blast callbacks complete before script. |
| Deferred packet commands | drain `pending_script_commands` | Menu responses wake script on this frame's clock. |
| Script | `run_frame` | Notifies, thinks/waits, callbacks, queued spawns and script state remain in the measured implementation order. |
| Post-script authority | spawn/loadout, weapon/link/sim ops, vitals | Class selection remains a next-spawn action. Links re-anchor after this tick's touch passes. |
| End frame | slot-order follow, contents/stuck, `end_frame`, aim/look-at, turret pass | Slot order is intentional retail behavior, including lower/higher-slot follow asymmetry. It is not replaced with arrival order. |
| Reliable output | configstring changes, script commands, intermission scoreboard | Queues are drained before snapshots. |
| Snapshot/archive | `send_snapshots`, then frame retention/archive | Entities are built once, culled and encoded per client in stable client-slot order. |

The detailed measured field and callback order remains in `AGENTS.md`; combat evidence is in `docs/research/cod11-combat.md`, lifecycle evidence in `docs/research/cod11-map-cycle.md`, and follow/end-frame slot evidence in `docs/research/cod11-spectator-follow.md` and `cod11-player-clip.md`.

## RISKS

- A decoded usercmd clock could jump far ahead. The existing 1,000 ms pmove arrears bound limits work per cmd but, by itself, lets an attacker advance authoritative `commandTime` and movement ahead of the frame clock.
- Valid protocol fields can still request a weapon absent from the server table or not present in authoritative `weapons_held`.
- Parse failures, bad acknowledgements, contradictory stance bits, invalid class/loadout choices, impossible state actions, and packet/usercmd queue pressure were mostly silent or scattered debug lines. Repetition was not visible as one bounded signal.
- The receive loop has a global datagram budget and usercmd queues have bounds, but there was no per-client packet-rate signal. A noisy client could consume work while remaining below disconnect/timeout rules.
- Aggressive fire-rate or movement-distance heuristics would create false positives: the server already computes movement, weapon timing, ammo and fire events authoritatively. Client inspection or inferred intent would add no trustworthy evidence.

## SMALLEST CORRECTION

- `validation::Discipline` adds a saturating per-client strike counter. It logs the first and power-of-two repeats, never auto-kicks or bans, and exposes no client-inspection path.
- At move-block intake, cmds whose timestamp is beyond that block's own final cmd are skipped, matching measured `SV_UserMove`; this prevents a forged earlier entry from making the real final cmd stale. At replay, a future usercmd is capped to the measured retail `level.time + 200 ms` window. To preserve the project's documented connect-time skew, the fixed allowance for a tick is based on the greater of level time and that client's command clock at tick entry; it cannot compound across a burst. Hitching commands at or behind the current frame retain the measured 66 ms chop and 1,000 ms arrears behavior.
- Unknown or unowned usercmd weapon requests are replaced with the authoritative current weapon; movement in the same cmd remains intact. Class weapon edits are checked against the server weapon table, class indices remain range checked, and alive class selection stays deferred to spawn.
- Malformed/failed connected packets, out-of-range acknowledgements, per-tick packet pressure, bounded-queue overflow, contradictory stance input, invalid loadout/class actions, and `kill` outside live play produce strikes. Existing parse rejection, reliable-command flood protection, queue caps and retail packet replay order remain unchanged.
- No speculative fire-rate detector was added. Dead/spectator button meanings overlap follow and menu behavior; only weapon ownership/table state is treated as authoritative evidence. Movement position and velocity are never accepted from the client in the first place.

Synthetic tests cover a future timestamp, invalid/unowned weapons, representable contradictory stance input, impossible action state, saturation/no-ban behavior, and an ordinary movement command remaining byte-for-byte unchanged.
