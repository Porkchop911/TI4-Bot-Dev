# Compute queue: who may use the GPU and heavy CPU, and what is waiting

Every agent (Claude, Codex, Pi, any session) reads this before starting GPU work or a multi-minute
CPU job, and updates it when it starts, pauses or finishes one. It is the single place compute
time is organised between agents.

## Current reservations

| Resource | Owner | Since | Rule |
|---|---|---|---|
| **GPU (RTX 3090, 24 GB)** | **blocked by the operator** | 2026-10-09 | **Operator: "gpu blocked". No agent uses the GPU** (no `--device cuda`, no CUDA optimiser, no GPU inference) until the operator lifts this here. |
| RAM | `strata` holds ~43 GB | 2026-10-09 | Expect ~6 GB commit headroom: keep cargo at `-j4`..`-j8`, CPU rollouts at 8..16 threads, watch for allocation failures (infrastructure, not code). |
| CPU (16 physical cores) | free for agents | — | AGENTS.md: bounded parallelism for multi-minute jobs; one cargo coordinator at a time. |

Do not stop, unload or restart `strata` (or LM Studio) to free memory. Ask the operator.

## Waiting for the GPU (pinned)

| Job | Prepared by | Ready | Notes |
|---|---|---|---|
| (none) | | | |

## Running now

| Job | Owner | Resource | Started |
|---|---|---|---|
| (none from Claude) | | | |

## Log

- 2026-10-09: GPU reserved for `strata` by the operator; self-imitation pilot pinned.
- 2026-10-09: engine no-progress guard started (CPU only); finished and committed (2809 engine tests, 40-game wide smoke clean).
- 2026-10-09: card effect-tag labelling started (no build, no GPU); draft finished (896 cards), awaiting operator spot-check.
- 2026-10-09: card-tag second review (Sonnet) ran and finished; 80 changes, 139 tags.
- 2026-10-09: operator: GPU free; self-imitation pilot unpinned and launched (both arms concurrently, commit 37305da5).
- 2026-10-09: self-imitation pilot finished (evidence plans/evidence/SIL-PILOT-2026-10-09.md); GPU free.
- 2026-10-09: operator blocked the GPU again ("gpu blocked"). Card-text feature work continues on CPU only.
