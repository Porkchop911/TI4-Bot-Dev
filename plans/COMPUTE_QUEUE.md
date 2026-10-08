# Compute queue: who may use the GPU and heavy CPU, and what is waiting

Every agent (Claude, Codex, Pi, any session) reads this before starting GPU work or a multi-minute
CPU job, and updates it when it starts, pauses or finishes one. It is the single place compute
time is organised between agents.

## Current reservations

| Resource | Owner | Since | Rule |
|---|---|---|---|
| **GPU (RTX 3090, 24 GB)** | **`strata`** (operator's local LLM server, `D:\strataflashnext`) | 2026-10-09 | **Operator: "strata is busy with important stuff". No agent uses the GPU** (no `--device cuda`, no CUDA optimiser, no GPU inference) until the operator lifts this here. |
| RAM | `strata` holds ~43 GB | 2026-10-09 | Expect ~6 GB commit headroom: keep cargo at `-j4`..`-j8`, CPU rollouts at 8..16 threads, watch for allocation failures (infrastructure, not code). |
| CPU (16 physical cores) | free for agents | — | AGENTS.md: bounded parallelism for multi-minute jobs; one cargo coordinator at a time. |

Do not stop, unload or restart `strata` (or LM Studio) to free memory. Ask the operator.

## Waiting for the GPU (pinned)

| Job | Prepared by | Ready | Notes |
|---|---|---|---|
| Self-imitation pilot vs plain PPO control: two arms, 50 updates each, same start, seeds and settings; then greedy 600-seed eval of each | BF unification session (Claude), 2026-10-09 | Code in `crates/ti4-mlp/examples/ppo_update.rs` (`--self-imitation 0.2 --sil-margin 1 --sil-keep 4`); the two `.psd1` configs are not written yet -- see below | On CPU it is ~2-2.5 h per arm (optimiser ~150 s/update); acceptable to run on CPU only if the operator says so. |

Configs for the pinned pilot (write them when it is unpinned, from `scripts/pilot_newfactions_rows50.psd1`):
start `out/trade-teacher-wide-v8-20261008/checkpoint-20`, roster six, main-line rewards, 50 updates,
30 seeds per update, `--device cuda`; arm A adds `--self-imitation 0.2 --sil-margin 1 --sil-keep 4`;
evaluate each final checkpoint with `clearance_eval --roster six --seeds 600 --rounds 4
--temperature 0.001 --diplomacy` (holdout pool).

## Running now

| Job | Owner | Resource | Started |
|---|---|---|---|
| (none from Claude) | | | |

## Log

- 2026-10-09: GPU reserved for `strata` by the operator; self-imitation pilot pinned.
- 2026-10-09: engine no-progress guard started (CPU only); finished and committed (2809 engine tests, 40-game wide smoke clean).
- 2026-10-09: card effect-tag labelling started (no build, no GPU); draft finished (896 cards), awaiting operator spot-check.
- 2026-10-09: card-tag second review (Sonnet) ran and finished; 80 changes, 139 tags.
