# Compute queue: who may use the GPU and heavy CPU, and what is waiting

Every agent (Claude, Codex, Pi, any session) reads this before starting GPU work or a multi-minute
CPU job, and updates it when it starts, pauses or finishes one. It is the single place compute
time is organised between agents.

## Current reservations

| Resource | Owner | Since | Rule |
|---|---|---|---|
| GPU (RTX 3090, 24 GB) | free | 2026-10-09 | Operator lifted the strata reservation 2026-10-09 ("gpu is free"). Check "Running now" before adding GPU work. |
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
