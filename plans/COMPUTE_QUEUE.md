# Compute queue: who may use the GPU and heavy CPU, and what is waiting

Every agent (Claude, Codex, Pi, any session) reads this before starting GPU work or a multi-minute
CPU job, and updates it when it starts, pauses or finishes one. It is the single place compute
time is organised between agents.

## Current reservations

| Resource | Owner | Since | Rule |
|---|---|---|---|
| **GPU (RTX 3090, 24 GB)** | **free; Codex training stopped** | 2026-10-11 | Operator requested immediate stop. No automatic restart. |
| RAM | free (strata stopped by Claude with operator permission) | 2026-10-09 21:50 | strata was stopped, not restarted; the operator restarts it. |
| CPU (16 physical cores) | free; workspace checks completed | 2026-10-11 | AGENTS.md: bounded parallelism for multi-minute jobs; one cargo coordinator at a time. |

Do not stop, unload or restart `strata` (or LM Studio) to free memory. Ask the operator.

## Waiting for the GPU (pinned)

| Job | Prepared by | Ready | Notes |
|---|---|---|---|
| (no pinned GPU job; focus+GAE continuation remains stopped) | | | |

## Running now

| Job | Owner | Resource | Started |
|---|---|---|---|
| (none; r8 stopped by the operator's STOP at update 1280, 2026-10-21; checkpoint-1280 for all 32 factions) | | | |

## Log

- 2026-10-09: GPU reserved for `strata` by the operator; self-imitation pilot pinned.
- 2026-10-09: engine no-progress guard started (CPU only); finished and committed (2809 engine tests, 40-game wide smoke clean).
- 2026-10-09: card effect-tag labelling started (no build, no GPU); draft finished (896 cards), awaiting operator spot-check.
- 2026-10-09: card-tag second review (Sonnet) ran and finished; 80 changes, 139 tags.
- 2026-10-09: operator: GPU free; self-imitation pilot unpinned and launched (both arms concurrently, commit 37305da5).
- 2026-10-09: self-imitation pilot finished (evidence plans/evidence/SIL-PILOT-2026-10-09.md); GPU free.
- 2026-10-09: operator blocked the GPU again ("gpu blocked"). Card-text feature work continues on CPU only.
- 2026-10-09: card-tag features implemented and committed (c319b6e4, CPU only); pilot pinned for the GPU.
- 2026-10-09: GPU idle 20 min; operator: "you may work on queued things" and "you can unload strata". strata stopped, card-tag pilot launched (roster wide per operator; no noise-floor runs first).
- 2026-10-09: card-tag pilot training finished (T checkpoint-3584, C checkpoint-3560); GPU free; greedy evals running on CPU.
- 2026-10-09: card-tag pilot evals finished (plans/evidence/CARD-TAGS-PILOT-2026-10-09.md): T vs C -0.02 VP table, no card-tag benefit. GPU and CPU free; strata still stopped.
- 2026-10-09: operator "do 1 and 3": --faction-focus and --gae-lambda implemented (uncommitted); three-arm pilot launched from card-tag T checkpoint-3584.
- 2026-10-09: operator: 1000 updates before the greedy eval. 50-update pilot stopped at update 1 (run dirs kept); long runs launched (PIDs 42464, 22804, 37028).
- 2026-10-10: operator: one arm only. Three long arms stopped at ~update 10 (dirs kept); one run with --faction-focus 1 and --gae-lambda 0.95 launched (PID 37192).
- 2026-10-10 01:40: operator needs the GPU; focus+GAE run stopped at update 229 (no eval run). GPU handed to the operator.

- 2026-10-11: operator releases GPU and confirms no other sessions working. Codex prepares 800-update warm start from saved update 200 (checkpoint-16448), fresh seed base 1264300100 and output out/ppo-focusgae-FG-resume800-20261011. Existing Oct 9 binaries reused by SHA256; no rebuild of shared dirty source. Optimizer/focus history reset. Controller performs GPU smoke, bounds training at 8 hours, then paired greedy wide eval (600 seeds each, 16 CPU workers) against T checkpoint-3584. State: preparing; scripts/continue_focusgae_20261011.ps1 and plans/evidence/FOCUS-GAE-CONTINUATION-2026-10-11.md.


- 2026-10-11 11:01 CEST: corrected hidden controller launched PID31004; CUDA smoke child49344. Paired VP helper by Luna self-tests pass and reproduces prior pilot conclusions. Independent reviewer accepts controller and analysis; expected600seed/6rotation/6seat shape is enforced. Exact live phase/error/PID in run status.json; completion frees GPU automatically when trainer exits (queue row is a launch record, not live process evidence).


Operator requested immediate stop2026-10-11. Owned controller31004 stopped first,then trainer49308; both exits verified. All outputs retained. Latest saved:D:\Projects\ti4-engine-rs\out\ppo-focusgae-FG-resume800-20261011\checkpoints\checkpoint-92;M10-034 PPO, 1 update(s) from D:\Projects\ti4-engine-rs\out\ppo-focusgae-FG-wide-long1000-20261010\checkpoints\checkpoint-16448, roster Wide. No eval/automatic restart. Next:baseline and correctness reviews before further training.


- 2026-10-11: Codex BF25 fresh release original-18 pair sample completed: 153 pairs, 306 primary games, 153 replays, zero failures; at most16 workers, Cargo -j8. Raw log out/logs/check-bf25-original18-pairs-20261011.log. CPU freed; training remains stopped.

- 2026-10-11: Current CPU-libTorch workspace checks completed, 4068 passed/1 failed/6 ignored, exit 101; sole semantic_golden failure retained. Raw log out/logs/check-base-factions-workspace-bf25-20261011.log. CPU freed; manual fixture checks are short and coordinated, training remains stopped.
