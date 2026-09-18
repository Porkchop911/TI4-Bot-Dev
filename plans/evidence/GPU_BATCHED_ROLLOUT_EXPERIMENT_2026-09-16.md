# Live batched GPU rollout: stop recommendation

## Decision

Do not use the experimental GPU rollout backend for training on the measured setup.
Two CPU updates averaged 36.344 s; two GPU updates averaged 82.983 s: GPU took
2.283 times as long, or approximately 56.2% fewer updates per hour. Keep parallel
CPU inference and CUDA optimization. No further GPU tuning campaign is justified
by these results. This is a bounded diagnostic, not a formal performance gate.

The earlier replay smoke compared GPU batching against single-thread CPU scoring.
It did not measure the existing parallel CPU rollout pool. These live results
supersede that extrapolation for this implementation.

## Scope and provenance

The user explicitly requested a Terra agent, minimal context, and actual PPO
update measurements. The delegated agent implemented an opt-in GPU owner thread,
bounded request queue, partial-batch flush, frozen inference weights and per-seat
sampling/recording. The CPU backend remains the default. The user subsequently
asked to evaluate whether continuing this work was worthwhile before proceeding.

Assigned experiment: RTX 3090; checkpoint 212544; training pool
`out/pools/full_np8_12_train.json`; diplomacy enabled; temperature 2.5; four game
rounds; one update per independent invocation; four PPO epochs, minibatches of
4096; no checkpoints. Both backends use CUDA optimization. Logs directly confirm
96 games, 32 workers and matching seed/rotation panels (seeds 650000000–650000015).

The agent reached its usage limit before writing its final command manifest.
The complete executed argument vectors, exact reward flags, tested executable
hash, process exit codes and hardware-isolation checks were not durably handed
over. Do not treat the assigned configuration as independently verified for
every flag. Four complete update/game diagnostic logs remain available and were
independently read and hashed by the parent agent. No replacement runs were
started after the stop decision.

At delegation the branch was `codex/diplomacy-v1`, HEAD
`85d01517a196315a5062b706199513c7570f15c0`, with existing dirty work. Other work
subsequently committed the experimental implementation in `ae3980f`; the parent
observed HEAD `740dc82` and a clean tree before adding this report. These commits
were not made by this task. Current HEAD is not a claimed benchmark binary ID.

Permissions: user-authorized P1 source/evidence and bounded P2 local measurements;
existing workspace and target directory only. No new folders, remote writes,
checkpoint promotion, or historical Python access. No owned trainer/build process
was present at final parent inspection.

## Measurements

All values are seconds. Each row contains one full 96-game update. Run order by
log timestamps was GPU1, CPU1, GPU2, CPU2.

| Run | Update wall | Rollout | Freeze | Optimize | Recorded decisions | Raw options |
|---|---:|---:|---:|---:|---:|---:|
| CPU1 | 36.415 | 22.084 | 4.902 | 7.752 | 232599 | 1296505 |
| CPU2 | 36.272 | 22.033 | 4.939 | 7.610 | 232599 | 1296505 |
| GPU32-1 | 82.508 | 68.111 | 5.142 | 7.543 | 232870 | 1297326 |
| GPU32-2 | 83.458 | 68.947 | 5.148 | 7.657 | 232695 | 1296643 |

The logged driver elapsed times were respectively 36.625, 36.492, 82.752 and
83.648 s. These are in-process elapsed readings, not externally measured process
lifetimes. Update wall includes diagnostic hashing and reporting; it is not just
the printed rollout-plus-optimizer subtotal. Two repetitions establish neither a
confidence interval nor broad hardware generality, but the observed difference is
far larger than their within-backend spread.

GPU1 averaged 25.82 requests per batch (maximum 32), with 1.836 ms average and
101.111 ms maximum queue time. GPU service scoring alone consumed 34.664 s and
packing another 0.662 s; the entire CPU rollout took 22.084 s. Thus merely removing
the configured flush wait cannot make this measured implementation beat CPU.
GPU2 similarly averaged 25.78 requests per batch and spent 34.533 s scoring.
The logs do not isolate kernel time from transfers, host work and scheduling;
they do not prove which optimization would resolve the service bottleneck.

## Semantics and checks

CPU repetitions have identical whole-batch digests and all 96 state/event/choice
digests match. Compared with CPU1, GPU1 matches 92/96 final states, 94/96 event
logs and 92/96 complete per-game choice-digest maps; GPU2 matches 93/96, 95/96 and
93/96 respectively. GPU repetitions also have different batch digests. Do not
claim deterministic or trajectory-equivalent execution. Decision count differs
by at most 0.117% from CPU; this small workload difference cannot explain the
observed factor of 2.28, but exact-workload parity has not passed.

Parent code review identified and the implementer corrected: resetting flush
timeout on each arrival instead of a fixed deadline; extra f64 softmax arithmetic
differences; incomplete nonfinite rejection; omitted packing-time accounting;
and a scheduling-sensitive batch-count test. The agent reported 3/3 focused
service tests passing (CPU actor/critic agreement at T=2.5, singleton partial
flush, nonfinite rejection). Parent inspected the fixes. Full affected-crate and
final lint qualification was not completed before the agent stopped.

GPU bot stage timing is not directly comparable to CPU: its Forward includes
queueing plus actor and critic, and its CriticForward marker follows record
assembly. Use whole-update/rollout timing and separate service statistics.

## Raw evidence

Files reside in the existing ignored `target/` directory. SHA-256:

| File | SHA-256 |
|---|---|
| gpu-batched-ppo-cpu-r1-20260916.jsonl | 0a93f0dd931b48634452c93d5bdadd06f734caf7a06d3dbef498b0ea0c2480ff |
| gpu-batched-ppo-cpu-r2-20260916.jsonl | e01089fc330c6c96bf181ef95c06bb200ebcde01bf5dc2ac00eaefb037b4deae |
| gpu-batched-ppo-gpu-b32-r1-20260916.jsonl | 02cdfdadd6a44dcd35a768e03b515f4a4d27110657d78b2ec697638f5f5c92a8 |
| gpu-batched-ppo-gpu-b32-r2-20260916.jsonl | c1beccef106ebc23f6c87518a6d62120db829e14f67bd12156a95fbab4d619ab |

## Better next experiment: isolated combat learning

An arena is plausible because `ti4-engine/src/combat.rs` already exposes
`combat::resolve` and a retained `CombatWindow`, with shared fixture builders.
The policy test `the_bot_answers_a_retained_combat_window_after_its_scoring_pause`
shows the window driven through real validated choices. Use that engine, not a
second hand-written combat simulator. The synchronous resolver lacks an outer
game timing/scoring context; begin with an explicitly bounded ordinary-fleet
subset, and use the retained game/window path for later timing-dependent cases.

The useful first target is prediction of win/draw/loss and surviving ships under
a declared fixed casualty/retreat policy. Repeated independent dice seeds produce
probability targets; a single lucky outcome is not a probability. Split held-out
fleet configurations before generating repetitions, rather than splitting seeds
of the same configurations across train and test. Include damage, unit upgrades
and relevant public modifiers when expanding scope. Hidden opposing information
must not enter actor inputs, even when the simulator knows it.

Before training, audit that the actual actor projection can distinguish fleet
compositions. The inspected activation family includes enemy ship counts and
reachable hull/capacity counts; the combat family supplies decision subtype and
local preview facts. These do not by themselves establish a sufficient two-fleet
representation. Check the complete projected vector and vocabulary, not just raw
simulator state. Training an outcome predictor cannot fix indistinguishable inputs.

Proposed bounded sequence, not implemented by this report:

1. Generate a small engine-backed ordinary-fleet panel; measure resolved fights/s,
   failure count, label uncertainty and deterministic seeded replay.
2. Train an auxiliary outcome predictor and test calibration/error on held-out
   fleet combinations against a simple fleet-strength baseline. Keep the full-game
   VP critic's target unchanged.
3. Only if prediction improves, expose the learned representation to attack/fleet
   selection and compare full-game learning curves at equal total compute, including
   arena generation/pretraining cost. Keep every game and evaluation at four rounds.

Arena learning is a hypothesis about reaching better policies with fewer full
games. It has not been shown to reduce PPO seconds per update or improve round-4
VP. A combat-only choice head also does not automatically teach earlier activation,
movement or production decisions. Those transfer links are explicit acceptance
criteria, not assumed benefits.
