# M11 — Tabletop Simulator bridge

## Goal

Replace the Python process side while retaining current TTS/Lua wire behavior and system-integrity principles.

## Work packages

| ID | Package | Depends | Python oracle | Deliverable and acceptance test |
|---|---|---|---|---|
| M11-001 | Bridge wire schemas | M02 | `bridge/server.py`, commands | Typed upload, poll, queue, log, command, outcome envelopes; golden JSON. |
| M11-002 | Loopback HTTP server | 001 | server endpoints | `/`, `/queue`, `/poll`, `/latest`, `/log`; loopback default and bounded request bodies. |
| M11-003 | Feature negotiation | 002 | `FEATURES`, `ensure.py` | Stale/missing-feature listener refusal and version diagnostics. |
| M11-004 | Command IDs/queue | 002 | `Bridge.queue/poll` | Monotonic/collision-safe IDs, FIFO batch semantics, restart tests. |
| M11-005 | Telemetry storage | 002 | upload/latest logic | Timestamped in-memory latest plus optional safe capture path; no traversal/overwrite. |
| M11-006 | Outcome/refusal parser | 001,004 | `bridge/outcomes.py` | Match logs to IDs, distinguish refusal/silence/success, never invent success. |
| M11-007 | Hex-summary decoder | M02 | `bridge/hexsummary.py` | Full grammar, typed board, malformed limits, Python golden corpus. |
| M11-008 | Hex-summary encoder | 007 | hexsummary encoder | Canonical encode and decode/encode properties. |
| M11-009 | Coordinate mapping | 007 | `bridge/mapping.py` | Table coordinates to engine hexes and inverse with lattice errors. |
| M11-010 | Telemetry importer | 007–009,M07 | `bridge/importer.py` | Public board to state, seating, tech, objectives, tokens; hidden state not invented. |
| M11-011 | Hand-log import | 006,010 | importer hand functions | Only explicit authorized logs update hidden hands; malformed names rejected. |
| M11-012 | State projection/audit | 010 | `audit.py`, `reconcile.py` | Engine projection versus observed totals/occupancy; report without correction. |
| M11-013 | Delta hypotheses | 007–012 | reconcile/explain | Activation, movement, landing, control hypotheses with ambiguity retained. |
| M11-014 | Command builders: movement | 001 | `commands.py` | Move/activate/control/land/claim/tactical JSON exact fixtures. |
| M11-015 | Command builders: economy/cards | 001 | `commands.py` | Tokens, cards, objectives, score, speaker, strategy, explore commands. |
| M11-016 | Engine event translators | 014,015,M07 | `bridge/perform.py` | Current event-to-command registry, economy reconciliation, exact ordering. |
| M11-017 | Human terminal seat | M03 | `bridge/person.py` | Choice display/parse, injected I/O, no terminal assumptions in core bridge. |
| M11-018 | Lua contract suite | 001–016 | `tts/*.lua`, bridge executor tests | Compile real Lua slices and exchange real messages with Rust server. |
| M11-019 | Save patch compatibility | 018 | `tools/patch_save.py` | Decide retain Python utility or port; output markers/order/compile checks remain compatible. |
| M11-020 | Adversarial bridge tests | 002–019 | bridge tests/handover traps | Oversize/malformed input, stale process, refused partial move, delayed telemetry, duplicate names. |
| M11-021 | Bridge soak | 002–020 | play/audit flows | Scripted long command/telemetry exchange has no lost IDs, hidden correction, or memory growth. |
| M11-022 | Frontier security review | 001–021 | — | Review loopback/auth assumptions, paths, resource limits, parser safety, refusal atomicity. |

## Exit gate

The existing Lua executor can communicate with Rust unchanged, all bridge fixtures pass, malformed
or stale peers are safely refused, and divergences remain visible rather than auto-corrected.

