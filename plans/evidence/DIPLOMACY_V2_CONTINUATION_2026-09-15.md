# Diplomacy v2 continuation evidence — 2026-09-15

## Scope

Continuation of the uncommitted R/T/S/L work described in
`plans/DIPLOMACY_HANDOVER_2026-09-15.md`:

- make the contact option cap self-describing and remeasure it;
- address contacts by seat rather than faction;
- implement agenda vote assurances and their policy/reviewer surfaces;
- version the breaking v2 diplomacy/corpus observation formats explicitly;
- run the long soak, affected-crate tests, Clippy, and diplomacy-off simulation gate.

Permission class: P1. No network, external writes, destructive actions, fixture regeneration,
rebaseline, merge, or remote action. The historical Python repository was not inspected.

## Changes

- `MAX_SIGNAL_CANDIDATES` is an engine constant. The cost probe derives its maximum from
  `MAX_INITIAL_CANDIDATES + MAX_SIGNAL_CANDIDATES + 1` (43).
- Contact IDs are `component|diplomacy|seat|INDEX`; duplicate factions remain independently
  addressable. Engine resolution, policy counterparty lookup, and reviewer text share the index.
- `SignalStatement::WillVote { agenda, outcome }` is offered only with a concrete revealed agenda,
  resolves from `VotesRecorded`, and becomes `Honoured` only when the speaker's nonzero ballot
  records the named outcome. Otherwise it becomes `Broken`.
- Signal options expose bounded `diplomacy:signal-kind:*` and
  `diplomacy:signal-statement:*` features without raw agenda, system, or player identities.
- New output versions are `ti4-diplomacy-log-v2`, `ti4-offline-selfplay-v3`, and
  `seat-authorized-canonical-mlp-v3`. Readers retain the declared v1/v2 compatibility pairs.

## Verification

- `cargo test -p ti4-model`: 81 passed.
- `cargo test -p ti4-engine`: 1,331 unit tests, 1 content-ID integration test, 4 delivery-inventory
  tests, and 5 doctests passed.
- `cargo test -p ti4-policy`: 248 passed, including the long nested-window campaign.
- `cargo test -p ti4-bridge --lib`: 62 passed.
- `cargo test -p ti4-review`: 33 passed.
- `cargo test -p ti4-mlp --lib -- --skip bundle::`: 88 passed, 14 filtered.
- `cargo test -p ti4-mlp --example capture_offline_pilot`: 20 passed.
- `cargo test -p ti4-mlp --example offline_bc`: 6 passed.
- Strict Clippy passed for the changed engine, policy, and reviewer surfaces after allowing only the
  repository's recorded unrelated baseline lints.
- The opt-in cost probe passed. Diplomacy-off: 338 decisions, 275 steps, 30,343 state bytes.
  Diplomacy-on: 455 decisions, 382 steps, observed maximum 8 options, 73,764 state bytes, 137
  journal events / 28,413 journal bytes.
- Release soak, 60 seeds x 6 rounds: 6,036 offers, 3,370 counters, 3,001 accepts, 3,035 declines,
  2,701 settled deals, 3,834 signals, 3,273 signal outcomes, 195 transactions, 69 favours (14 agent,
  43 note loan, 12 skipped secondary). No refused step.
- `cargo test -p ti4-sim --release`: the behavior suite reproduced inside v38. Overall 51/52;
  `profile::tests::fixture_capture_is_deterministic` remains the documented environmental failure
  because this worktree has no map-pool fixture. No rebaseline was performed.

## Open findings

- `DiplomacyState::journal` remains append-only and unbounded. Truncation is not acceptable because
  it would make the authenticated deal export incomplete. The next package should move full journal
  ownership out of checkpointed `GameState` (or introduce an equivalent lossless sidecar) while
  preserving replay checks, reviewer events, and corpus export.
- `UseLeaderFor` still has typed fulfillment hooks only for the Hacan and L1Z1X agents.
- The S/L reviewer HTML has not yet received a manual browser rendering check.
- Paired greedy evaluation and diplomacy BC/PPO training have not run.
- Independent Tier-C review is still required before the uncommitted v2 legality/schema work is
  integrated.

