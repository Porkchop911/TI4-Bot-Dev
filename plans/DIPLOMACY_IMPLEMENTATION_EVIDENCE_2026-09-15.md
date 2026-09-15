# Structured Diplomacy Implementation Evidence — 2026-09-15

Branch: `codex/diplomacy-v1` in `diplomacy-worktree`, based on `5f2b2c6`.

No commit, merge, rebase, checkpoint migration, corpus publication, or baseline update was
performed. The separate dirty checkout and its engine fixes were not touched.

## Implemented contracts

- Match-scoped model state, bounded terminal history, directional relationships, monotonic IDs,
  legacy-promise migration, typed deals/promises/signals/events, and compatible JSON loading.
- Central relationship rules and decay, attack deduplication, typed promise settlement, explicit
  linked future payments, game-end expiry, and transaction-backed atomic immediate transfers.
- Resumable contact/offer/response/counter window with engine-stored candidates, two-counter cap,
  shared pair allowance, signals, and canonical decision contexts.
- Six candidate template identifiers, deterministic candidate and counter caps, relevance filters,
  immediate legality reuse, duplicate-obligation suppression, and factual bundle features.
- Public actor-relative and third-party relationship observations, signal counts, critic facts,
  v11 OOV family, and a dedicated 15th MLP head.
- Bundle schemas 9/10 with projection ABI 3, v10-to-v11 vocabulary/input migration, and trade-head
  warm start for the appended diplomacy readout.
- Append-only engine journal, denormalized `ti4-diplomacy-log-v1` export, self-play/observation v2
  capture schemas, authenticated diplomacy shards, v1 legacy reader path, and v2-aware validation.
- Simulation capability flag, capture telemetry/candidate gates, deterministic decision-ID replay,
  and a future adapter API that cannot receive mutable game state.

## Verification run

- `cargo check --workspace --all-targets`: pass. Only pre-existing example warnings reported.
- `cargo test -p ti4-model --lib`: 81 passed.
- `cargo test -p ti4-engine --lib`: 1,302 passed; decision-delivery inventory also passed.
- `cargo test -p ti4-policy --lib`: 247 passed.
- MLP legacy-head preservation/trade warm-start and vocabulary row-migration logit tests: pass.
- `cargo test -p ti4-bridge --lib`: 62 passed.
- One-round ordinary-decision-ID replay: identical decision log, diplomacy journal, and final state.
- `git diff --check`: pass.

Bundle/corpus tests that create temporary directories were not run, following the handover's
workspace rule. No real checkpoint or corpus was mutated.

## Fixed-seed cost probe

Two rounds, seed 117, six seeded-random seats, debug test build:

| Mode | Decisions | Engine steps | Max diplomacy options | Time | State bytes | Journal events | Journal bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| Disabled | 284 | 215 | 1 | 0.302 s | 29,454 | 0 | 2 |
| Enabled | 447 | 378 | 11 | 0.362 s | 55,159 | 109 | 17,907 |

This single-seed probe shows material decision/step inflation. It is evidence for a heterogeneous
pilot and contact-relevance tuning, not authority to change the one-pair-per-turn rule.

## Gates intentionally still external

- Explicit user approval before any commit, merge, baseline refresh, or broad self-play capture.
- Real 7→9 / 8→10 checkpoint migration and manifest/checksum validation on copied artifacts.
- Worker-count byte-determinism on a real v2 corpus and capture → validation → BC → inference smoke.
- Matched diplomacy-disabled/enabled/trained evaluation with seat/faction rotation.
- BC quality gate before admitting diplomacy samples to PPO.
