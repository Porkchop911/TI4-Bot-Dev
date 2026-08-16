# Execution state

This file is the durable resume point for autonomous agents. Update it before every context
compaction, package commit, handoff, or milestone transition.

It describes **the repository as measured**, not the plan. A milestone is complete when its
behaviour is implemented, tested, and reviewed — never because a document for it exists.
The previous version of this file claimed the migration was complete; see
[`AUDIT_2026-08-11_PLAN_VS_TREE.md`](AUDIT_2026-08-11_PLAN_VS_TREE.md) for what was
actually in the tree and how the two diverged.

## Handover

See the compact handover at the end of this file, written before context compaction.

Read [`HANDOVER_COMPACT.md`](HANDOVER_COMPACT.md) for the full handover summary.

## Current position

- Oracle repository: `D:\Projects\ti4-engine` (read-only)
- Oracle branch: `codex/fully-learned-policy`
- Oracle commit: `37061c511a4780d4c0719e0342533a498cd4b457` — verified clean
- Branch: `wp/m06-003-structured-transactions` (thirteen packages, 2026-08-12)

### Codex Stage-1 parity repair checkpoint (2026-08-13)

- Active branch: `codex/stage1-parity-fixes`; the pre-existing local edit to
  `crates/ti4-training/examples/stage1_curve.rs` remains uncommitted and was not incorporated into
  this package.
- Implemented scope: collision-free schema-3/4/5 profiles; Python-style explicit option and board
  features; factual engine choice payloads; faction-keyed gradients; full seat rotations over
  shared maps; an exact high-level Python reference plan; and a comparison executable with
  representation, rotation, rollout, and solved-checkpoint gates.
- Semantic result: the repaired Rust run now learns useful opening motion immediately, but the
  Python solved checkpoint still fails transfer (96 seat-games/faction: Hacan 0.000, Jol-Nar
  0.000, Letnev 0.010 clearance). This is reported as a failed gate, not parity.
- Evidence: `docs/STAGE1_PARITY_COMPARISON.md` and
  `plans/evidence/CODEX-STAGE1-PARITY.md`. Final workspace verification is recorded there.
- Follow-up (2026-08-14): the driven strategy-card blocker is implemented for all eight cards and
  both active Thunder's Edge replacements. Primaries, accepted secondaries, shared/faction cost
  rules, Brilliant substitution, and TE Warfare's deferred free tactical action are wired. See
  `docs/STRATEGY_CARD_PARITY.md`.
- Map follow-up (2026-08-14): the Save-54 map-shape blocker is closed for parity/training runs.
  `ti4-sim::MapPool` reads and validates the exact Python JSON.GZ artifact; Stage-1 rotation,
  evaluation and training accept it with the Python `seed + 20,000,000` rule. On identical 32 pool
  selections, Python clearance is Hacan/Jol-Nar/Letnev `0.969/0.979/0.865`, while Rust is
  `0.312/0.292/0.010`. The remaining transfer failure is now isolated to game/choice execution,
  not map distribution. Evidence: `plans/evidence/CODEX-STAGE1-MAP-PARITY.md`.
- Solved-transfer follow-up (2026-08-14): the same-pool gate now passes at Hacan/Jol-Nar/Letnev
  `0.865/0.865/0.823` after propagating FULL sources, aligning technology identities, Gravity
  Drive, Transit Diodes, TE expeditions, Gravleash, faction production units, and the Python
  distance-zero `target:reachable` feature, then closing the implemented learned-window routing
  class. Stateful nested choices now receive `Observed`; Integrated Economy, learned Orbital Drop
  and Peace Accords targets, Psychoarchaeology, Chaos Mapping, Predictive Intelligence, and
  Bio-Stims are wired. This validates the imported policy representation and basic execution path,
  not complete engine parity. Unsupported content/event ordering remains documented in
  `docs/STAGE1_PARITY_COMPARISON.md`.
- Training optimization follow-up (2026-08-14): schema-4 profiles are shared through `Arc`, Rayon
  provides persistent work-stealing rollout execution, workers return deterministic sufficient
  statistics, and faction/head merges are parallel. The Stage-1 reference path measures about
  `0.091 s/update` versus `0.41` before this package and the historical optimized Python `0.556`.
  Stage 2 is wired to the same core with four-round rewards, six factions/rotations, the real
  Save-52 pool, blank or checkpoint bootstrap, faction evaluation, and atomic checksummed resume
  artifacts. See `docs/TRAINING_PIPELINE.md` and
  `plans/evidence/CODEX-TRAINING-PIPELINE-OPTIMIZATION.md`.

### Codex Stage-2 stall investigation (2026-08-14)

- Objective: explain why Stage-2 training has been flat since bootstrap (mode A — alive process,
  no promotions across ~1,100 updates) and test the candidate remedies against real artifacts.
  Failure modes B/C were ruled out by the operator before this package.
- Safepoint: commit `66fd234`, tag `safepoint/stage2-stall-baseline`; created before any new edit;
  rollback point for this investigation.
- T0 forensics on `out/stage2_*.json`: the champion is frozen at its bootstrap state across every
  recorded boundary; candidate aggregate gains over n=8 boundaries average ≈+0.4 with no trend;
  learner weights differ from the champion in ~80% of cells but |Δ| is small (mean ≈0.0014) — a
  random walk around bootstrap rather than drift.
- Telemetry correction: per-update weight movement scales exactly linearly with `learning_rate`
  (≈0.034/update at lr=0.03 vs ≈0.011–0.015/update at lr=0.01 in T2). The earlier "lr test
  inconclusive" reading was a block-length artifact — that run used `every=50` blocks, so raw
  totals looked half-sized.
- T1 eval-only at the frozen 5700 state, n=32 seeds × 6 rotations (`out/eval_t1_5700_n32.json`):
  paired gain **+0.188, SE 0.180** (2σ bar 0.361) and **no veto violations**. The repeated n=8
  "clearance regressions" were panel noise at 48 games/faction; the state genuinely is not
  promotable yet. Revised diagnosis: the stall is ≈zero optimizer drift, not gate miscalibration —
  the gate's refusal was correct at adequate resolution.
- Instrumentation in `crates/ti4-training/examples/stage2_training.rs` (gate boolean behavior
  preserved; 11/11 example tests pass): clause-level rejection reasons via
  `failed_stage_two_clauses`, console logging of every un-promoted boundary's rejecting clauses,
  an `--eval-only` mode with JSON sidecar, and a `--learning-rate` knob recorded in checkpoint
  arguments.
- T2 differential run complete (finished 2026-08-14 20:34 WEDT, 3309 s for 1000 updates): resume
  @4600 from `out/stage2_from_stage1.json`, lr 0.01 versus the baseline lineage's 0.03 at identical
  absolute update positions, n=32 boundaries every 100 → `out/stage2_test_lr001_n32.json` (log
  `out/logs/t2_lr001.log`). Every boundary gain is within ≈1.4σ of zero (mean −0.15); no promotions
  in either run; per-faction weight displacement from the shared champion scales with lr×updates
  (~0.36 ratio vs 0.30 step-budget ratio) — directionless exploration, no drift at either learning
  rate.
- **Final diagnosis (evidence-backed): the stall is a zero-signal optimization problem.** Four-
  round games compress outcomes into near-ties, so centered REINFORCE credit has ≈no directional
  information; entropy-regularized steps random-walk around bootstrap. Gate rejections are all
  correct; n=8 panels had been adding noise-driven vetoes and over-reading gains by +0.2–0.5 (fixed
  seed set reused at every boundary). Learning rate is not a lever; reward signal is.
- T3 Python-oracle parity audit (read-only, `D:/Projects/ti4-engine/out/stage2_pg_six_c_*.json`
  + trainer source): the oracle's Stage-2 **did** promote under its configuration
  (sol@u3350, xxcha@u3450, isolated path) — promotion-grade improvement emerges around u≈3350–
  3450 there. Full parity table: horizon, lr/entropy/clip, 96 seat-games/update batch, map pool,
  gate tolerances, reward math (golden-verified port), update law (line-level equivalent), weight
  movement magnitude, and full-decision trajectory capture are all equal. Rust is strictly
  *stricter* in three places: extra paired-σ clause (`--accept-sigmas 0` restores oracle gate),
  VP-only isolated-path improvement test (no clearance tiebreak), and eval cadence every 100 vs
  the oracle's 50. None of these explain zero drift; they must be matched for a fair parity run.
- Panel decorrelation implemented in `stage2_training.rs`: opt-in `--panel-step N` gives each
  boundary k a fresh disjoint seed block (`base + k·N`); default 0 keeps the historical fixed
  panel bit-for-bit. Per-boundary first seeds recorded in history entries (`validation_first_seed`,
  serde-defaulted) and checkpoint arguments; smoke run `out/smoke_panel_step.json` confirms
  boundary seeds 96000000/96000003/96000006. Two new unit tests (default unchanged; disjoint
  blocks).
- Checks: `cargo test -p ti4-training` 98/98 lib + 13/13 example; clippy clean; rustfmt applied;
  T2 binary built pre-fmt (cosmetic-only later edits).
- Evidence: `plans/evidence/STAGE2-STALL-INVESTIGATION.md`.
- T4 oracle-parity run: attempt 1 (PID 8752) killed at ~u4729 after its first two boundaries exposed
  a pairing-contract bug — with `--panel-step` on, fresh candidate panels share no source seeds with
  the champion's bootstrap measurement, so `GainEvidence::paired` degenerated to `samples=0,
  gain=+0.000` at every boundary and the margin clause could never fire (artifact preserved as
  `out/stage2_t4_attempt1_brokenpairing.json`, log
  `out/logs/t4_oracle_parity_attempt1_brokenpairing.log`). Fix in the loop: stepping mode now
  re-measures the incumbent on each boundary's fresh validation+confirmation panels before any paired
  comparison (default fixed-panel mode untouched). Relaunched ~22:05 WEDT as PID 18224 with the same
  pre-registered config (`--updates 3500 --every 50 --validation-seeds 32 --confirmation-seeds 32
  --accept-sigmas 0 --panel-step 32` + save52 pool), output `out/stage2_t4_oracle_parity.json`,
  log `out/logs/t4_oracle_parity.log`. Regression check on relaunch: first boundary must show
  `source seeds=32`, not 0.
- T4 decision rule (pre-registered): ≥1 promotion by u≈3500 = oracle parity achieved → the stall
  was gate strictness + budget, and the promoted table is promotable evidence. Zero promotions ⇒
  implementation-level game/feature divergence from the oracle → frontier-model differential
  diagnosis (do not tune hyperparameters further). `--rounds 8` deprioritized by operator;
  train-seeds=64 and reward re-examination remain fallbacks.
- T4 ETA ≈ 6–7 h (~04:00–05:00 WEDT): T2 training rate (≈3.28 s/update) plus 3 panels per boundary
  in stepping mode (incumbent validation + incumbent confirmation + candidate). Monitor via log tail /
  checkpoint history; first boundary at update 4650.
- T4 status ~00:10 WEDT: u6350/8100 (~78%), no promotions yet, all rejections clearance-veto driven
  (mostly sol trading round-1 openings for mid-game VP). Real positive drift has emerged under valid
  paired statistics (gains trending upward with updates; several +0.3–0.6 aggregate boundaries vetoed on
  per-faction clearance). Revised ETA ~02:30–03:00 WEDT.
- T4 CPU under-saturation diagnosis (operator question): **structural, not a bug.** Verified absent:
  sleeps/deadlines/locks in engine+training code; memory pressure (93 GB RAM, ~37 GB free); map-pool
  I/O (fully in-memory Vecs); checkpoint races (atomic tmp+rename — two concurrent `--eval-only` readers
  of the live file came up clean). Measured: a 192-game panel takes only ~4–6 s wall when cores are free,
  so training blocks are one short Rayon wave per update (~3 s, 96 tasks/32 threads) with barriers between
  updates and variable game-length tails => oscillating 25–78% of all cores; stepping-mode rejected
  boundaries run up to **10 panels** (incumbent x2 + candidate + isolated per-faction fallback x6, silent in
  the log except final clause lines) as a sequence of short waves with small serial gaps => the long
  low-CPU stretches seen in Task Manager. Measured pace ≈5.0 s/update incl. ~87 s/boundary matches u6350.
  Correctness is scheduling-independent; making eval faster would save <20% wall — not worth a mid-run
  rebuild/restart. Post-T4 option recorded: pre-filter the isolated fallback on the validation panel
  (gate semantics unchanged) to cut rejected-boundary cost ~3x for future runs.

- T4 ended by operator decision (~00:55 WEDT) at u6700/8100 after the operator judged the run a
  failure ("none of the VPs have moved"). Final state preserved in `out/stage2_t4_oracle_parity.json`
  (last boundary u6700, gain +0.490; 43 boundaries total, zero promotions). Run data: paired gains
  first half mean +0.125 -> second half +0.391; 17/42 boundaries cleared the +0.30 oracle margin; all
  rejections clearance-veto driven (rotating factions: sol early, letnev/l1z1x late). The operator's
  read on absolute VP levels is correct by construction (4-round horizon compresses everything to
  ~2.0 mean VP); the champion column only moves on promotion and there were none.
- Operator direction (supersedes the Rust-bootstrap T5 plan, which is withdrawn): re-run the PYTHON
  pipeline itself as a control retest — its own stage-1 champions through Stage-2 with the latest
  Python stage-2 settings; "basically no rust". Oracle repo stays read-only: trainer runs from an
  external cwd (PYTHONPATH + PYTHONDONTWRITEBYTECODE=1 + PYTHONPYCACHEPREFIX into this repo), all
  outputs redirected here; `git -C D:/Projects/ti4-engine status --short` verified unchanged after
  launch (one pre-existing untracked doc).
- Python retest launched ~01:25 WEDT, PID 65312 (+~30 worker processes):
  `python D:/Projects/ti4-engine/tools/train_stage1_policy_gradient.py --stage 2 --horizon 4
  --resume D:/Projects/ti4-engine/out/stage1_pg_six_to5000_20260810.json (u3050, schema-4 auto-migrated
  to 5 exactly as the original chain) --out out/py_retest_stage2_pychamp.json --updates 500` with the
  latest recorded segment settings verbatim: seed 74000000 (same panels + training stream as the
  original chain), train_seeds 16, validation/confirmation/audit seeds 32, eval_every 50, workers 30,
  lr 0.03, entropy 0.01, clip 1.0, vp/objective/secret weights 1.0/0.35/0.25, clear_bonus 22, r1_bonus
  3.0, r1_shaping 0.1, expansion 2.0, unit 1.0, accept_vp_margin 0.05, max faction clearance/vp/shortfall
  regression 0.03/0.15/0.10, shortfall margin 0.15, game_seconds 30, save52_e400_n8192 pool, six
  factions sol,letnev,xxcha,hacan,jolnar,l1z1x, progress_interval 8, surrogate snapshots on (into this
  repo). The original chain's three segments were each manually stopped early (+100/+100/+250 new
  updates); --updates 500 covers their full span (promotions landed at +300 sol@u3350 and +400
  xxcha@u3450) in one run to completion, ending u3550.
- Python retest COMPLETED 02:54 WEDT (~87 min wall, run_complete=True, u3550). Verdict: old system
  works and reproduces -- gate decisions identical at 9/10 boundaries (u3100 assembled all-six,
  u3350 isolated sol both reproduced); trajectories bit-identical through u3150 then small drift from
  u3200 (Python pipeline is not run-reproducible: wall-clock game_seconds abandonment + parallel
  reduction order), and the single flip is xxcha@u3450 -- original's swap passed all gate clauses
  comfortably (+0.474 vs 0.300 aggregate) while this run's drifted swap failed two (aggregate +0.016;
  sol VP veto -0.172). Start->final: total VP 8.042 -> 11.562 accepted (+3.52), learner 12.582
  (+4.54); per-faction table in plans/evidence/STAGE2-STALL-INVESTIGATION.md. Most of the gain is the
  horizon reorientation jump in the first 50 updates (stage-1 champion was horizon-1 trained).
- Next safe action: decide with the operator between (a) the decisive differential experiment -- run
  the Rust stage2_training.exe from this same Python stage-1 champion file (schema-compat check of
  `D:/Projects/ti4-engine/out/stage1_pg_six_to5000_20260810.json` against the Rust checkpoint format
  first) with T4-equivalent settings, comparing boundary-by-boundary; or (b) close out the
  investigation here and commit the retest artifacts + evidence. Note the comparability caveat: T4's
  resume point was already past its own reorientation jump, so its zero-promotion stretch is not
  directly comparable to Python's +300/+400 promotions from raw stage-1 champions.

### M08-005 tactical scoring checkpoint (2026-08-13)

- Active branch: `wp/m08-005b-tactical-scoring`, based on `8a72a4f`; the focused local package
  commit is `Score tactical activations from public board state` (see current Git HEAD).
- Completed scope: M08-005 tactical scoring is closed through M08-005b/d. `Observed` reports the
  active system and derives public movement reachability; `ScoredBot::choose_seeing` values
  systems, removes useless activations from its own shortlist, declines idle reinforcements, loads
  transport toward a prize, avoids surplus landings, and favors lift when troops are stranded.
  Planet-only stranded troops are correctly counted. Existing casualty/sustain/retreat rules
  remain in the shared dispatcher; plain `choose` remains the blind fallback.
- Next package: M08-006 economy/development scoring, followed by M08-007 objectives.
- Verification: `cargo fmt --all --check`; `cargo test -p ti4-policy` (40 passed); `cargo test -p
  ti4-engine` (703 unit + 5 doc tests passed); `cargo clippy -p ti4-policy -p ti4-engine
  --all-targets -- -D warnings`; and `git diff --check` all passed. Oracle integrity guard passed
  before and after. Focused mutation checks failed as intended and were restored.
- Simulator diagnostic: 24 scored games, 80 objective scores, top VP range 1–6; all still end
  `objectives_exhausted` in round 9. This is progress evidence only, not a parity or speed claim.
- Evidence: `plans/evidence/M08-005b.md`, `plans/evidence/M08-005c.md`,
  `plans/evidence/M08-005d.md`. Next safe action after committing is M08-006.

### M08-006a public development checkpoint (2026-08-13)

- Active branch: `wp/m08-006-economy-development`, based on `7fa7460`; focused local package
  commit follows this state update.
- Completed scope: observed research favours a missing colour path or a progressing unit-upgrade
  route from face-up cards; strategy-card choice uses printed public roles; observed token gain
  uses oracle `6 / 5 / 3` diminishing-return pool needs. Unknown or out-of-scope content retains
  the blind fallback.
- Verification: oracle integrity guard passed; `cargo fmt --all --check`; `cargo test -p
  ti4-policy` (45 passed); `cargo test -p ti4-engine` (703 unit + 5 doc tests passed); `cargo
  clippy -p ti4-policy -p ti4-engine --all-targets -- -D warnings`; and `git diff --check` all
  passed. A deliberately inverted colour-gap mutation failed its focused decision-boundary test
  and was restored.
- Evidence: `plans/evidence/M08-006a.md`. Next safe package: M08-006b strategy-secondary and
  payment/trade scoring, after checking which choice windows exist and whether their terms are
  publicly represented.

### M08-006b economy closeout checkpoint (2026-08-13)

- Active branch: `wp/m08-006-economy-development`; focused local package commit follows this
  state update.
- Completed scope: exact-fit payment preserves trade goods; legal Leadership token spends beat
  decline; generated trade offers value mutual Support, commodity conversion, exchange balance,
  and gifts. Termless transaction acceptance/counter and opening windows intentionally remain
  unscored, because their choices do not contain a deal to value.
- Verification: oracle integrity guard passed; `cargo fmt --all --check`; `cargo test -p
  ti4-policy` (48 passed); `cargo test -p ti4-engine` (703 unit + 5 doc tests passed); `cargo
  clippy -p ti4-policy -p ti4-engine --all-targets -- -D warnings`; and `git diff --check` all
  passed. A zeroed Support mutation failed the focused trade ranking and was restored.
- Evidence: `plans/evidence/M08-006b.md`. M08-006 is closed through M08-006a/b. Next safe
  package: M08-007 objective planning, beginning with the existing public objective/schedule
  facts and explicit private-secret boundary.

### M08-007a objective award checkpoint (2026-08-13)

- Active branch: `wp/m08-007-objective-planning`; package commit `0f1101e Prioritize printed
  objective awards`; handover checkpoint `1168f95`.
- Completed scope: legal scoring choices now use the exact source-scoped printed point value, so
  an offered two-point objective beats an offered one-point objective. The scorer reads no
  unoffered secret, objective deck, or hidden state; the score window remains the visibility and
  legality boundary.
- Verification: oracle integrity guard passed; `cargo fmt --all --check`; `cargo test -p
  ti4-policy` (49 passed); `cargo test -p ti4-engine` (703 unit + 5 doc tests passed); `cargo
  clippy -p ti4-policy -p ti4-engine --all-targets -- -D warnings`; and `git diff --check` all
  passed. A deliberately halved victory multiplier failed the exact component test and was
  restored.
- Evidence: `plans/evidence/M08-007a.md`. Next safe package: M08-007b public partial-progress
  demands and objective reserve facts, after identifying a compact public API that does not
  expose secret objectives or private exhaustions.

### M08-007b public technology-progress checkpoint (2026-08-13)

- Active branch: `wp/m08-007b-objective-progress`; focused package commit follows this state
  update.
- Revealed unscored public technology objectives now steer only legal research toward a visible
  colour pair or unit-upgrade threshold. No secret/objective-deck/private-resource state is read.
- Verification passed: oracle guard; format; 50 policy tests; 703 engine unit + 5 doc tests;
  clippy `-D warnings`; diff check. A zeroed pair component failed the focused decision boundary.
- Evidence: `plans/evidence/M08-007b.md`. Next: another M08-007 child for a distinct public goal
  family or a documented reservation observation API, after package scoping.

### M08-007c public planet-progress checkpoint (2026-08-13)

- Branch: `wp/m08-007c-planet-progress`; focused package commit follows this state update.
- Unscored revealed planet-control objectives add a named public valuation factor without reading
  secret/private state. Full affected-package checks pass; the 1.0-factor mutation fails the
  decision-boundary test and was restored.
- Evidence: `plans/evidence/M08-007c.md`. Next M08-007 package must scope a distinct public goal
  family or a safe reservation observation; do not expose private exhaustion as a shortcut.

### M08-007d public spend-capacity observation checkpoint (2026-08-13)

- Branch: `wp/m08-007d-public-reserves`; focused package commit follows this state update.
- `Observed::available_spend` now reports a player's public ready resources or influence through
  the authoritative production accounting, including face-up trade goods and excluding exhausted
  planets. The typed API returns only an aggregate: it exposes neither state nor exhaustion/card
  identities.
- Verification passed: oracle guard; focused boundary test; format; 704 engine unit + 5 doc tests;
  51 policy tests; clippy `-D warnings`; and diff check. A constant-zero mutation failed the
  ready-planet assertion and was restored.
- Evidence: `plans/evidence/M08-007d.md`. Next safe M08-007 child: use this aggregate only for
  revealed unscored public purchase-objective reservation; keep secret objectives and payment
  execution out of scope.

### M08-007e public purchase-objective reservation checkpoint (2026-08-13)

- Branch: `wp/m08-007e-public-purchase-reserves`; focused package commit follows this state
  update.
- Resource/influence payment options now carry additive `payment_kind` metadata. For a revealed,
  unscored, at-least-half-funded public single-kind purchase objective, observed payment scoring
  favors the smaller legal expenditure that preserves more public capacity. Payment legality,
  option IDs, and execution did not change.
- Verification passed: oracle guard; focused engine/policy boundaries; format; 705 engine unit +
  5 doc tests; 53 policy tests; clippy `-D warnings`; and diff check. Zeroing the reserve penalty
  selected the larger payment and was restored. An initial precise-cast lint was corrected before
  final verification.
- Evidence: `plans/evidence/M08-007e.md`. Next safe M08-007 child: public trade-good or token
  reserve facts, or another non-overlapping revealed public goal family; do not add secret or
  mixed-cost planning without a dedicated public fact model.

### M08-007f public trade-good objective reservation checkpoint (2026-08-13)

- Branch: `wp/m08-007f-public-trade-good-reserves`; focused package commit follows this state
  update.
- An offered trade-good payment now preserves the final public trade good for a revealed,
  unscored, at-least-half-funded Trade Routes or Centralize Trade objective. The reservation
  component has no effect on legality, IDs, execution, secrets, or negotiation.
- Verification passed: oracle guard; focused policy boundary; format; 54 policy tests; 705 engine
  unit + 5 doc tests (before the policy-only lint correction); final workspace-target clippy; and
  diff check. Zeroing the reserve penalty selected the trade good and was restored.
- Evidence: `plans/evidence/M08-007f.md`. Three atomic M08-007 children have completed since the
  last compaction checkpoint; write a fresh handover before the next package. Next safe scope:
  token reserve facts or another public goal family, not secret/mixed-cost/schedule planning.

### M10-020 atomic checkpoints checkpoint (2026-08-13)

- Branch: `wp/m08-007f-public-trade-good-reserves`; commit `bdb16b4`.
- Implemented `Checkpoint` struct (schema 1) matching the oracle's JSON checkpoint format.
- `Archive` with crash-safe atomic writes (temp file + rename), SHA-256 checksums, schema
  validation, and interrupted-temp detection.
- `Horizon` and `Run` now derive `Serialize`/`Deserialize`.
- 10 new tests: schema, round-trip, resume, mark_complete, not_found, interrupted_temp,
  deterministic checksum, changing checksum.
- Verification: 52 training tests + 986 workspace tests pass; clippy clean; format clean.
- Evidence: `plans/evidence/M10-020.md`.

### M10-008 parallel batch runner checkpoint (2026-08-13)

- Branch: `wp/m08-007f-public-trade-good-reserves`; commit `e743bff`.
- `play_batch()` divides seeds into chunks by `available_parallelism()`, spawns one thread per
  chunk, collects results sorted by seed.
- `train()` updated to use `play_batch` instead of sequential `play`.
- 4 new tests: batch count, seed ordering, determinism, empty input.
- Performance: 0.056 s/game single-threaded → estimated < 1 hour for 1M games across 32 cores.
- Verification: 56 training tests + 986 workspace tests pass; clippy clean; format clean.
- Evidence: `plans/evidence/M10-008.md`.

### M10-017 champion/learner promotion checkpoint (2026-08-13)

- Branch: `wp/m08-007f-public-trade-good-reserves`; commit `ed07139`.
- Replaced `promotion.rs` stub with full champion/learner separation implementation.
- `PanelMetrics`/`FactionMetrics` mirror oracle's `metrics()` output.
- `PromotionConfig` with configurable thresholds (shortfall_margin, regression allowances).
- `Promotion` struct with `acceptable_assembled()`, `is_better()`, `promote()`, `apply_promotion()`.
- `PromotionResult` with `promoted` factions, `accepted_kind` (Assembled/Isolated/None).
- Assembled path: all factions pass clearance + shortfall vetoes + aggregate gain.
- Isolated path: individual factions promoted when better AND veto still passes.
- 14 new tests covering all promotion paths, config defaults, serialization.
- Verification: 71 training tests + 987 workspace tests pass; clippy clean; format clean.
- Evidence: `plans/evidence/M10-017.md`.

### M10-018 learner/champion resume checkpoint (2026-08-13)

- Branch: `wp/m08-007f-public-trade-good-reserves`; commit `e359e7e`.
- Added `Archive::resume()` to restore training state from checkpoints.
- `ResumeState` struct holds champion, learner, history, telemetry, seeds, eval interval.
- Champion extracted from `checkpoint.accepted` (hard error if empty).
- Learner extracted from `checkpoint.profiles`, falls back to `accepted`.
- Validates champion/learner factions match.
- Computes `start_update` from max update in history.
- Seed ranges: validation at `seed + 9_000_000`, confirmation at `seed + 14_000_000` (oracle defaults).
- New `CheckpointError::ProfileValidation` variant for profile validation errors.
- 8 new tests covering resume, fallback, failed promotion, equivalence, validation.
- Verification: 77 training tests + 991 workspace tests pass; clippy clean; format clean.
- Evidence: `plans/evidence/M10-018.md`.

### Superseded timing-branch checkpoint
- Branch: `wp/m02-004-system-state`
- Active package: M02-004 system state; no implementation edits have started on this branch.
- Last completed package: M02-002 common schema envelope (`b063dcb`).
- M03-009 through M03-012 are complete on this resolver chain: deterministic ability registration,
  WHEN/resolution/AFTER windows, bounded depth-first nested emission, and typed once-per-trigger,
  turn, and round scopes. M03-013 is integrated: versioned SHA-256 hashes cover all replay-visible
  event and `DecisionRecord` fields. M03-014 is complete: a direct pinned-oracle fixture proves
  line-for-line public trace parity at the WHEN/resolution/AFTER ordering boundary. M03-015 is
  complete: 128-case generated registries verify termination under rule-consumed repeatable
  eligibility, exact frequency scopes, ineligibility, duplicate slots, trace determinism, and
  optional pass termination. The workspace has 121 content, 478 engine, and 68 model tests plus
  one doc-test, all passing. User waived Pi/external review on 2026-08-12 and authorized
  self-review; the waiver is recorded in each package evidence file rather than represented as
  independent review.
- M03-007's previous evidence claimed a completed translator without source, fixtures, or tests.
  The original oversized package is now split: M03-007a parses existing bounded oracle traces
  into explicit selected decisions and dice entropy, and M03-007b retains a generated, checksummed
  100-trace corpus (12,234,839 NDJSON bytes; four scenarios times seeds 0–24). M03-007c owns
  native semantic replay and remains blocked on generic-game parity. M03-016 cannot start until
  the parent package is complete.
- **Two agents are working this repository at once.** The M03 timing chain
  (M03-007a/b, M03-010 through M03-015) is held in `.worktrees/` by the other agent;
  `timing.rs` and `event.rs` belong to it. The packages below deliberately avoid both files.
- M03-009 ability registration is complete: `Resolver` owns deterministic `(event, relation)`
  registrations and persistent cannot rules, and ability callbacks have its concrete typed API.
  User waived Pi/external review on 2026-08-12 and authorized the agent's own invariant review.
- Five quality packages landed this session: `Dice::from_faces` (1), `unimplemented()` gaps
  for secrets/agenda_effects (2), wiring guard for five subsystems (3), runnable doc-examples
  on Table/Decider/ContentStore (4), and `plans/evidence/INDEX.md` separating 86 written
  evidence files from 345 placeholder stubs (5). See handover for details.

### Measured, 2026-08-12 evening

`cargo test --workspace`: **539 engine + 121 content + 68 model + 1 doc-test, 0 failed.**
Workspace clippy-clean under `-D warnings`; `cargo fmt --all --check` clean.

Registry coverage, from the ledger in `crates/ti4-engine/src/registry.rs`:

```
public objectives      40/40   (100%)
exploration cards      71/80   (89%)
secret objectives      27/40   (68%)
agenda effects         34/63   (54%)
relics                  5/17   (29%)
action cards            0/122  (0%)
```

**Those denominators are the corpus, not the oracle, and reading them as migration progress
overstates the gap badly.** Measured against the oracle at the pinned commit by comparing
registered aliases: public objectives 32, secrets 27, agendas 34, exploration 33, **action
cards 35**. Every registry except action cards is at or ahead of oracle parity. Action cards are
a genuine porting gap of 35 effects — the largest remaining — and M06-016 has now made them
playable, so the effects are the only thing missing.

The action-card figure was recorded here as 1 on 2026-08-12 and corrected the same evening. It
came from a pattern matching `@implements("alias")` only, and `action_cards.py` is the one oracle
module that also uses multi-argument decorators and `implements_every_copy`, which expands one
printed name to all four physical copies. Every other number in this list re-measured unchanged.

### Packages completed this session

1. **Transactions are negotiable** (94.1a) — the last unwired module.
2. **Public objectives complete at 40/40.** `Position` gained an optional galaxy so objectives
   about the shape of the board can be answered; without a map they report unmet.
3. **Secrets to 27/40**, which is oracle parity, including the two that are *bought*.
4. **Exploration 41 → 71/80.** `Resolving` gained the table, so a window resolving a "you may"
   card can ask the player instead of answering for them.
5. **Agendas 7 → 34/63**, oracle parity. `resolve_with` takes dice, a table and the map.
6. **Relics made reachable** through a component action, plus `relics::gain` as the one door.

Bugs found and fixed, each of which was silent:

- `Position::home_system` ignored the seat's own recorded home, inverting every requirement
  phrased "other than your home system".
- Dynamis Core read commodities *held* as the faction's commodity *value* — the card backwards.
- Relics drawn straight off the deck never scored the Shard of the Throne.
- Enforced Travel Ban read every wormhole system in the corpus, destroying garrisons in systems
  the game was never set up with.
- A leaked secret (Classified Document Leaks) was worth nothing to anybody, because `scoreable`
  looked only at the public registry while the requirement stays registered in `secrets`.

### Next actions, re-derived against the tree

1. **Content porting is done to oracle parity.** Further cards would be new design, not
   migration, and every remaining registry entry is blocked behind the reaction system.
2. The M03 timing chain is therefore the critical path for everything left, and it is held in
   `.worktrees/` by the other agent.
3. M00-013, the performance baseline, is still unrun.
4. `GameState` still does not record its source scope; `Game` holds it via `with_sources`, so a
   state loaded from disk and driven without it scores against the wrong catalogue.

- Planning: **M00–M13 documents written.** Implementation status is separate and below.
- Implementation: **M02 and M04 in progress.** Content, galaxy, state model, hidden views,
  setup, phases and turn order done. Movement, combat, production and legality are not.
- Last completed package: M06-001 — space combat
  (`plans/evidence/M06-001_SPACE_COMBAT.md`)
- Previous packages: the choice model (`plans/evidence/M03-001_TO_005_CHOICE_MODEL.md`);
  faction seating (`plans/evidence/M04-004_FACTION_SEATING.md`);
  state model, views, phases and turn order
  (`plans/evidence/M02-003_005_008_M04-003_006_007_STATE_AND_PHASES.md`); galaxy
  (`plans/evidence/M04-001_002_GALAXY.md`); content layer
  (`plans/evidence/M02-009_TO_012_CONTENT_LAYER.md`)
- Last completed package: **M05-010a — combat roll effects**
  (`plans/evidence/M05-010.md`). It applies the existing sequence-scoped morale, extra-die and
  Munitions markers at the deterministic space-combat boundary; 444 engine tests and the full
  workspace are green, and the oracle guard passes.
- M05-010b remains deferred: source registration and payment require the M06-016 reaction/event
  resolver and must not be invented as a direct action-card path.
- **M00-013 is blocked after a bounded smoke run.** The oracle is clean and executable, but its
  benchmark script cannot produce M00-012's raw monotonic paired report; the Rust benchmark is
  still `todo!()` and no semantic parity corpus qualifies a comparison. See
  `plans/evidence/M00-013.md`. No diagnostic number is accepted as a baseline.
- **M06-016 is blocked pending M03-008 through M03-012.** The required typed event/timing
  resolver does not exist; string event labels are not a safe substitute. See
  `plans/evidence/M06-016.md`.
- Last completed package: **M03-008 — typed event model** (`plans/evidence/M03-008.md`). It has
  trace-local numeric IDs, deterministic payload serialization, cancellation, and validated typed
  reads; 449 engine tests and the workspace suite pass.
- Last completed package: **M03-007b — bounded oracle trace corpus**
  (`plans/evidence/M03-007b.md`). Its manifest pins the oracle commit and validates every trace's
  size, checksum, provenance header, and safe resolved fixture path. The generator's source replay
  recreated every captured byte stream before retained artifacts were written.
- Next action: investigate M03-007c's native replay boundary against the current generic `Game`.
  M03-016 remains blocked on the unfinished M03-007 parent package and is not being bypassed.
- M03-007c investigation is now blocked rather than merely pending. The 100 source traces require
  Save 52/54 scenario/map construction, the full source state/event projection, compatible legal
  option IDs, and dice-entropy injection. The native `Game` has none of those compatibility
  surfaces: it accepts a prebuilt Rust state, uses its own ChaCha-derived dice stream, records
  `Vec<String>` events, and does not expose a source-equivalent canonical projection. Implementing
  a replay adapter would therefore either ignore source decisions/entropy or invent a parity claim.
  The next safe work is the dependency package(s) that establish generic native game parity, but
  strict milestone order currently places M04 after M03's unresolved exit gate. See
  `plans/evidence/M03-007c.md` for the exact blocking evidence.
- The owner authorized a recorded M03→M04 sequencing exception on 2026-08-12 to construct those
  prerequisites without faking M03-007c success. M04-015 is split: M04-015a now supplies the
  native canonical public-state projection in the executable source schema, with public-card
  redaction and deterministic bytes. M04-015b now owns a checked two-snapshot source-state intake
  boundary and validates all 100 retained traces. M04-015c1 now provides a strict field-path
  public-state comparator. M04-015c2a now reconstructs every retained initial snapshot into an
  intentionally public-only native `GameState`, proving its public projection is exact while
  rejecting held strategy cards whose initiative metadata is absent from the source schema.
  Opaque private-card placeholders make the state non-executable. M04-015c2b1 proves that the
  native driver consumes exactly the shared six-card strategy prefix then refuses the trace's first
  unsupported `component|expedition|secret` action; it neither skips nor accepts it. M04-015c2b2
  owns native executable scenario construction, full entropy/decision consumption, event
  projection, and selected cross-engine comparison. M03-007c remains blocked until those later
  compatibility surfaces prove semantic equality.

## M03-016b integration checkpoint (2026-08-12)

- Branch: `wp/m03-m06-timing-integration`.
- The M03 timing chain and `wp/m06-003-structured-transactions` now coexist in one local merge
  result. `game.rs` and `wiring.rs` merged automatically; their combined result retains the M06
  driver calls and M03's stateful timing ownership/wiring guard.
- `plans/EXECUTION_STATE.md` was the only textual conflict. Both historical branch checkpoints
  remain labelled as superseded above; the current package evidence and this checkpoint supersede
  their stale branch/next-action fields.
- No source-rule behavior was introduced as part of conflict resolution. The integration must pass
  all parent checks before it can be committed or used as the content/timing handoff branch.

## M03-016a package checkpoint (2026-08-12)

- Branch: `wp/m03-016a-stateful-timing`.
- Status: complete in the focused `Wire stateful timing through strategy selection` commit. This is
  an owner-authorized corrective child
  of the blocked M03-016 review, not a completion claim for M03-007c or the M03 exit gate.
- `Game` now owns a resolver and typed event sequence. A `TimingContext` gives stateful effects the
  live `GameState`, content/source scope, one game table, dice history, RNG, and nested allocator.
  The context-free resolver rejects stateful abilities rather than pretending they resolved.
- `STRATEGY_CARD_CHOSEN` now emits through the driver with source-matching player/card/goods facts;
  WHEN cancellation is atomic. Resolver frequency counters synchronize from `GameState`, and the
  wiring guard requires the driver call.
- Checks: 484 `ti4-engine`, 20 `ti4-legacy`, 72 `ti4-model`, and 1 doc-test passed; workspace
  strict Clippy, fmt, and diff check passed; oracle integrity guard verified 238 files before and
  after inspection. See `plans/evidence/M03-016a.md`.
- Remaining complaint scope: combat, invasion, production, technology, and card triggers require
  separately settled event vocabulary. Do not bulk-wrap existing string diagnostics as typed events.

## M04-005 package checkpoint (historical)

- Branch: `wp/m04-005-strategy-draft`, based on unmerged M04-003 package commit `8f97ffb`.
- Last completed package: M04-005 — generated strategy-card draft and atomic application
  (`plans/evidence/M04-005.md`).
- Next dependency-ready package: M04-008 — generic strategy primary. M04-005 through M04-007
  now provide generated drafting, phase progression, and turn order.
- `ti4-engine` now has 105 tests. The workspace has 295 passing tests: 121 `ti4-content`,
  105 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- M04-005 is committed cleanly as `Generate and apply strategy draft choices` at the current
  package-branch `HEAD`.
- Strategy choices are generated from current unclaimed cards, validated at the shared choice
  boundary, and applied atomically; action choices remain unimplemented.

## M04-008 package checkpoint (historical)

- Branch: `wp/m04-008-generic-strategy-primary`, based on M04-005 package commit `73ed98c`.
- Last completed package: M04-008 — structural strategic-action generation and exact-card
  exhaustion (`plans/evidence/M04-008.md`).
- Next dependency-ready package: M04-009 — generic strategy secondary.
- `ti4-engine` has 109 tests. The workspace has 299 passing tests: 121 `ti4-content`,
  109 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- One unused strategy card produces the compatible bare `strategic` action; a player with several
  cards receives a stable named action for each unused card. Applying an action validates before
  mutation and exhausts exactly its selected card.
- Card-specific primary effects and secondaries remain intentionally unimplemented. Normal actions,
  turn advancement, and phase completion are also outside this package.
- M04-008 is ready to commit after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## M04-009 package checkpoint (historical)

- Branch: `wp/m04-009-generic-strategy-secondary`, based on M04-008 package commit `7c27b47`.
- Last completed package: M04-009 — generic strategic-action secondary window
  (`plans/evidence/M04-009.md`).
- Next dependency-ready package: M04-010 — status phase structural flow.
- `ti4-engine` has 112 tests. The workspace has 302 passing tests: 121 `ti4-content`,
  112 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- `begin_strategic_action` opens a clock-wise follower window. Eligible followers may follow for
  one strategy token or decline; tokenless followers are recorded ineligible. The selected card
  exhausts only when that window completes.
- Content-specific primary and secondary effects, other eligibility gates, event emission, and
  persistent game-step ownership of the window remain intentionally unimplemented.
- M04-009 is ready to commit after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## M04-010 package checkpoint (historical)

- Branch: `wp/m04-010-status-phase`, based on M04-009 package commit `3475d01`.
- Last completed package: M04-010 — deterministic status-phase bookkeeping
  (`plans/evidence/M04-010.md`).
- Next dependency-ready package: M04-011 — agenda structural phase.
- `ti4-engine` has 116 tests. The workspace has 306 passing tests: 121 `ti4-content`,
  116 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- The status resolver reveals objectives, draws action cards in preserved initiative order,
  returns board tokens, readies/repairs state, and resets strategy-card/pass bookkeeping. An
  empty objective deck ends the game before later steps.
- Status scoring and the per-token allocation choice are intentionally unimplemented; no default
  allocation or automatic scoring is applied. M04-012 must own those generated decision windows
  before integrating status resolution into the phase driver.
- M04-010 is ready to commit after scoped formatting, focused and affected-crate tests, workspace
tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
are recorded in the package evidence; independent review remains owner-waived.

## M04-011 package checkpoint (historical)

- Branch: `wp/m04-011-agenda-structural`, based on M04-010 package commit `85a122e`.
- Last completed package: M04-011 — structural agenda reveal/order/ready bookkeeping
  (`plans/evidence/M04-011.md`).
- Next dependency-ready package: M04-012 — choice-window and generated-decision API.
- `ti4-engine` has 119 tests. The workspace has 309 passing tests: 121 `ti4-content`,
  119 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- `resolve_agenda_phase` atomically rejects illegal entry, reveals at most two agenda aliases,
  records speaker-clockwise voting order, and readies planets after its two slots (including an
  empty deck). Every agenda resolution is explicitly deferred; no vote, tie-break, or agenda effect
  is invented.
- Agenda resolution is deliberately not integrated into the phase driver. M04-012 owns the legal
  generated decision windows and safe integration alongside outstanding status choices.
- M04-011 committed after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## M04-012 package checkpoint (historical)

- Branch: `wp/m04-012-step-run`, based on M04-011 package commit `b6bef5b`.
- Last completed package: M04-012 — generated-choice game driver with bounded run metadata
  (`plans/evidence/M04-012.md`).
- Next dependency-ready package: M04-013 — random-legal bot.
- `ti4-engine` has 124 tests. The workspace has 314 passing tests: 121 `ti4-content`,
  124 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- `Game` now owns generated strategy/action choices, table decision recording, structural follower
  windows, phase/round progression, observable events, and a bounded `run` API. Legal-choice
  inspection is side-effect free; each decision step is separate from phase work.
- Required but unavailable status scoring/token-allocation and agenda voting/tie/effect choices
  stop at a typed `GameError` boundary. They are not replaced by guessed defaults or reported as
  a completed game; tactical/component/Fleet Logistics behavior remains outside this driver.
- M04-012 committed after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## M04-013 package checkpoint (historical)

- Branch: `wp/m04-013-random-legal-bot`, based on M04-012 package commit `d316107`.
- Last completed package: M04-013 — shared-stream seeded random-legal game constructor
  (`plans/evidence/M04-013.md`).
- Next dependency-ready package: M04-014 — generic completion suite.
- `ti4-engine` has 128 tests. The workspace has 318 passing tests: 121 `ti4-content`,
  128 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- `Game::with_seeded_random` applies one ChaCha8-backed `SeededRandom` default to every unseated
  player, preserving global decision order and the generated-choice validation boundary. Same
  native seed repeats its event/decision trace; different seeds select different legal traces.
- A random run reaches the explicit `StatusChoicesUnimplemented` boundary rather than hanging or
  pretending the absent status scoring/token choices completed. Python seed parity is intentionally
  not claimed because the native stream is ChaCha8, not Mersenne Twister.
- M04-013 committed after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## M04-014 package checkpoint (historical)

- Branch: `wp/m04-014-completion-suite`, based on M04-013 package commit `d509d87`.
- Last completed package: M04-014 — 100-seed native generic structural campaign
  (`plans/evidence/M04-014.md`).
- Next dependency-ready package: M04-015 — differential phase suite.
- `ti4-engine` has 130 tests. The workspace has 320 passing tests: 121 `ti4-content`,
  130 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- Every one of 100 seeded two-to-six-player runs reaches the explicit status choice boundary within
  500 steps; no run silently finishes, deadlocks, or records an invented choice. Same-seed state,
  event, decision-log, and step-result snapshots match after every step.
- M04 does not yet have generic game completion. Status scoring/token allocation and agenda
  voting/ties/effects are still required decision windows. The campaign records this as a bounded
  failure rather than presenting an incomplete run as success.
- M04-014 committed after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## Current package checkpoint (authoritative)

- Branch: `wp/m00-014-integrity-guard`, based on M00-012 package commit `849496d`.
- Last completed package: M00-012 fixed benchmark protocol (`plans/evidence/M00-012.md`).
  M00-011 remains blocked by an oracle integrity failure. Active package: M00-014e guard tool.
- M00-008 fixture-selection and M00-009 design documents existed without code. M00-009b through g
  now provide deterministic public-state, redacted-view, choice, resolved-event, outcome, and
  structured-error components.
- Eighteen focused tests cover canonical state ordering, state byte stability, viewer-private identity
  preservation, opponent redaction, view byte stability, choice option ordering, payload
  canonicalization/refusal, event UID/cancellation/context, finished-outcome tie-breaking, and
  deterministic structured errors. Oracle HEAD remains
  `37061c511a4780d4c0719e0342533a498cd4b457` and its tree is clean.
- The stale M00-007a draft schema named fields absent from the pinned oracle. M00-009b records the
  actual-field reconciliation; it cannot yet be advertised as an exact shared Rust/Python schema.
- M04-015 remains blocked: bounded generated traces exist, but no approved selected generated
  corpus or Rust/Python cross-engine comparison exists.
- M00-009h is split before implementation: M00-009h1 wires and validates a deterministic,
  read-only initial-setup NDJSON stream; M00-009h2 completed its reproducibility campaign. This
  preserves the original acceptance requirement without pretending the still-unimplemented full
  causal event trace exists.
- M00-009i observes a bounded seeded scenario's generated choices, resolved events, final state,
  and dice history. M00-009j replays its captured option IDs and proves byte-identical bounded-game
  streams, including across the executable replay CLI. M00-010 is now blocked before generation:
  M00-008 contains no executable 100-scenario manifest (including the distinct three-/four-player
  definitions), and no approved artifact-retention policy exists for traces that may contain hidden
  card identities. See `plans/evidence/M00-010f.md`. No oracle paths are writable.
- M00-011 **is resolved as of 2026-08-12; the oracle guard passes again.** Its `--basetemp`
  override had been passed unquoted to a Bash-family shell, which stripped the backslashes, leaving
  the drive-relative path `D:Projectsti4-engine-rs...` that resolved inside the oracle — not a
  pytest path-reinterpretation. The stray tree was moved (not deleted) into the package's own
  gitignored `.tmp-m00-011/basetemp-recovered/`, and the oracle verified clean, at the pinned
  commit, with a pristine tracked tree. Future oracle runs must pass the override with forward
  slashes (`--basetemp=D:/Projects/ti4-engine-rs/.tmp-m00-011`), which no shell here mangles.
  The run's captured log also shows the **full oracle suite passed: 2,097 of 2,097 in 491.65 s** —
  recorded but not yet accepted as the baseline, which is the package owner's call. See
  `plans/evidence/M00-011.md`.
- M00-012 replaces the stale alternative-filled benchmark drafts with a fixed 10-warmup/30-sample,
  deterministic interleaving, non-mutating affinity, raw-sample schema, and variance-rejection
  protocol. M00-013's dependency on M00-011 is now discharged.
- M00-014e is **complete.** With the oracle clean, `tools/generate_oracle_manifest.py` produced
  `plans/oracle_integrity_manifest.json` — 238 files (`engine/`, `bridge/`, `tests/`, `data/`,
  `configs/`, `pyproject.toml`) at the pinned commit — and the guard verifies it in production
  (`oracle integrity verified: 238 files`, exit 0). Fail-closed was proven against the real oracle,
  not only fixtures: a zeroed digest is rejected with exit 2. Automatic pipeline integration is
  still a separate, unclaimed package. See `plans/evidence/M00-014e.md`.

## M04-016 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `c44e8cf`.
- Last completed package: M06-001 — space combat
  (`plans/evidence/M06-001_SPACE_COMBAT.md`).
- `ti4-engine` has 142 tests. The workspace has **332 passing tests**: 121 `ti4-content`,
  142 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. The build is warning-free.
- `TokenGain` asks once per token, so a player may split a grant between pools — the oracle's
  own rule, shared with Leadership, which is why it lives in `tokens.rs` and not in the status
  phase.
- The status phase is split into `resolve_before_token_gain` (81.2–81.4) and
  `resolve_after_token_gain` (81.6–81.8) so the 81.5 window sits where the rules put it.
  `resolve_status_phase` still runs both for callers with no decider, and a test pins that the
  halves compose to the whole.
- The old `StatusChoicesUnimplemented` covered two unrelated gaps. It is now
  `StatusScoringUnimplemented` and names only LRR 81.1, which is the single remaining obstacle
  to a generic game completing a round.
- Two pre-existing status tests used strategy-card ids that do not exist in the corpus
  (`leadership` rather than `pok1leadership`), so they silently tested seating order rather than
  initiative order. Fixed; no production code was wrong.

## M04-017 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `3a78709`.
- Last completed package: M04-017 — objective scoring
  (`plans/evidence/M04-017_OBJECTIVE_SCORING.md`).
- `ti4-engine` has 157 tests. The workspace has **347 passing tests**: 121 `ti4-content`,
  157 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- **A generic game now completes a whole round.** All 100 seeded two-to-six-player runs finish
  the round with no step refusing, where before every one stopped at an unimplemented boundary.
- Scoring's machinery is fully ported (61.8 once-per-game, 61.16 home control, 98.4a point cap,
  98.7/98.8 initiative tie-breaks, both-deck point lookup). The *requirement predicates* are a
  first tranche of 6 of the oracle's 32 — the planet-control family. The other 26 are
  unregistered and therefore unscoreable, which is the oracle's own design for a coverage gap,
  and `unregistered_objectives()` reports them.
- 81.1 runs before 81.2 because scoring can end the game.
- Two defects found and fixed during the package: resolving controlled planets per predicate was
  quadratic enough to stop the campaign terminating, and completing the status phase turned a
  previously-safe unbounded test loop into a hang. Both are recorded in the evidence.

## M04-018 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `0e2265a`.
- Last completed package: M04-018 — agenda voting (`plans/evidence/M04-018_AGENDA_VOTING.md`).
- `ti4-engine` has 174 tests. The workspace has **364 passing tests**: 121 `ti4-content`,
  174 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- **`AgendaChoicesUnimplemented` is gone.** The round loop contains no structural boundary:
  strategy, action, status and agenda all resolve through generated choices.
- `VoteWindow` is a resumable state machine (outcome, then a planet per vote, then the speaker),
  because this driver resolves one decision per step where the oracle uses nested loops.
- Encoded with tests: the speaker votes last (8.2ii), a planet casts its full influence (8.6a),
  an abstention is not a vote (8.14), a tie *or a silent table* goes to the speaker (8.19) and
  that decision is not a vote (8.19a), a passed law stays in play (8.20/8.21).
- Agenda *effects* are not applied. Every resolution emits `AGENDA_EFFECT_UNRESOLVED`, which is
  what the oracle does when no handler is registered. Laws are recorded but nothing reads them.
- The agenda corpus has **no `electType` field** — it is null on every card. Elections are read
  off the printed `target`, as the oracle does. Reading the absent field would have made every
  agenda a silent For/Against with nothing failing.

## M05-003 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `2be9a43`.
- Last completed package: M06-001 — space combat
  (`plans/evidence/M06-001_SPACE_COMBAT.md`).
- `ti4-engine` has 193 tests. The workspace has **383 passing tests**: 121 `ti4-content`,
  193 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- `engine/movement.py` ported in full: 58.4a–f, 11.1, 86.1, 59.1/59.1a/59.2, 41.1/41.3.
  Reachability is a breadth-first search, not a distance comparison, because gravity rifts make
  the budget path-dependent.
- **`Galaxy` adjacency is finally load-bearing.** It had existed unused since M04-001.
- `Board::for_player` reads *ships*, not units: a lone infantry is not a blockade.
- The test fixture took three attempts and the reasons are recorded in the evidence — a hex ring
  is itself a route (so blocking the centre only bites at move 2), and "two apart" does not mean
  "opposite". Both earlier versions passed while testing almost nothing about blockades.
- Nothing calls this yet: there is no tactical action, so movement is knowledge the engine
  cannot act on. That is M05-006.

## M05-006 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `a2fedaa`.
- Last completed package: M06-001 — space combat
  (`plans/evidence/M06-001_SPACE_COMBAT.md`).
- `ti4-engine` has 210 tests. The workspace has **400 passing tests**: 121 `ti4-content`,
  210 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- `CargoWindow` fills a hold under LRR 95, tracking candidates **by index, never by value**:
  units are plain data, two infantry compare equal, and an equality filter would silently make
  the second one unloadable while every step reported success.
- Ground forces loaded from a planet arrive in the destination's *space area*. Landing is
  invasion, a separate step; dropping them onto a planet would conquer it with nobody choosing.
- 41.2 rolls one die per rift *exited* — ending in a rift is safe. Nav Suite is honoured here as
  well as in the legality rules, and rolls no die at all, since a discarded die would still
  advance the seeded stream and desynchronise replay.
- 95.1b: a ship lost to a rift takes its cargo down with it.
- `MoveOutcome` names its passengers rather than counting them; a count cannot be acted on.
- **Nothing calls this yet.** There is no tactical action, so the pieces exist but the sequence
  does not. That is M05-001/002.

## M05-001/002 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `9381fb5`.
- Last completed package: M05-001/002 — activation and the movement step
  (`plans/evidence/M05-001_002_TACTICAL_ACTION.md`).
- `ti4-engine` has 225 tests. The workspace has **415 passing tests**: 121 `ti4-content`,
  225 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- 89.1b bars a system holding *your own* command token, and only your own — an opponent's is no
  obstacle, because activating a system they hold is how you attack it. Both directions tested.
- `activate` checks both refusals before mutating; `identical()` pins that a refused activation
  spends nothing.
- `movable` asks `MovementRules` rather than re-deriving legality. That join is the package:
  parking a destroyer on the only route makes the move disappear from the offered options with
  no code in `tactical` knowing why.
- One option per distinguishable move, not per hull, and damage stays in both the dedup key and
  the label.
- The one-ring fixture trap from M05-003 recurred here in a different module: "two systems away"
  can be two seats round the ring, by a route that never touches the centre. Recorded twice
  deliberately — the wrong version passed the eye test both times.
- **Nothing sequences these yet.** A driven game still cannot take a tactical action.

## M05-004 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `f25435b`.
- Last completed package: M06-001 — space combat
  (`plans/evidence/M06-001_SPACE_COMBAT.md`).
- `ti4-engine` has 235 tests. The workspace has **425 passing tests**: 121 `ti4-content`,
  235 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- A second objective-predicate tranche landed alongside: technology and structures, taking
  coverage from 6 of the oracle's 32 to 14.
- **A driven game can now take a tactical action**: activate, move ships one at a time, load
  each hold, roll the route's rifts, finish.
- The action is offered only when `Game` has a galaxy. Nothing else builds one, so no existing
  test or the 100-seed campaign changed behaviour; the option is appended rather than inserted,
  so a first-option table keeps taking the action it took before.
- The route is computed when the ship is selected and carried through loading, so rifts are
  rolled for the path that was legal when the move was offered.
- `with_seeded_random` seeds the `GameRng` too, so a replayed game rolls the same rifts.
- The action *completes* and emits `TACTICAL_STEPS_UNRESOLVED` rather than blocking. Combat,
  invasion and production are unimplemented, so **arriving in an enemy system has no
  consequence** — announced, not hidden.

## M06-001 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `96a562e`.
- Last completed package: M05-004/012-015 — fleet limits and invasion
  (`plans/evidence/M05-004_012-015_FLEET_AND_INVASION.md`).
- `ti4-engine` has 255 tests. The workspace has **445 passing tests**: 121 `ti4-content`,
  255 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- 78.1, 78.5b/c, 78.5f, 78.6, 87.1 and 15.2a are implemented and tested. Hits are simultaneous;
  resolving sequentially would let casualties reduce return fire already earned.
- A divergence caught by its own test: the first version rolled one batch **per unit**, where
  the oracle groups by combat value. The draw count is part of what a seed reproduces, so
  rolling them apart would silently renumber every later draw.
- Casualty and sustain options are deduplicated for the reason the choice model documents: a
  sampling decider draws per option, so five fighters offered five times decided by count
  rather than by scoring.
- Choices are asked inline through a `Table`, as the oracle does, so **the step driver does not
  fight yet** — the position movement was in before its driver landed.
- Anti-fighter barrage (78.3/78.3a) is implemented and wired into round one; it is simultaneous
  and its hits fall only on fighters. Space cannon offense is implemented but **uncalled**, as it
  belongs to the tactical action's post-movement sequence.
- Not included: retreats, rerolls, combat modifiers, PDS II adjacency, ability suppression.

## M05-004/012-015 checkpoint (historical)

- **463 passing tests**: 121 `ti4-content`, 273 `ti4-engine`, 68 `ti4-model`, 1 doc-test.
  Zero warnings, engine Clippy clean. Oracle verified clean before and after.
- Fleet supply (37) and capacity (16); bombardment, landing, ground combat and planet control
  (49, 42). Five plan packages in one batch.
- A captured planet is taken **exhausted**; 49.5d leaves a wiped-out invasion's target with its
  previous holder; a war sun ignores Planetary Shield.
- Supply is enforced before capacity, since removing a carrier can strand its fighters.
- Working mode changed at the owner's request: batch several modules, one verify, one evidence
  file, one commit. Test density unchanged.
- **Nothing calls combat, invasion or fleet enforcement.** All three wait on the same wiring
  into the tactical action, which still emits `TACTICAL_STEPS_UNRESOLVED`.
- Package IDs in earlier evidence filenames drifted from the master plan and have not been
  renamed: `M05-004_TACTICAL_DRIVER` and `M06-001_SPACE_COMBAT` both sit in slots the plan
  assigns to other packages.

## Velocity refactor + tactical wiring (historical)

- **464 passing tests**: 121 `ti4-content`, 274 `ti4-engine`, 68 `ti4-model`, 1 doc-test.
  Zero rustc warnings; `ti4-engine` clippy-clean apart from two pre-existing `seating`/`setup`
  items.
- **The tactical action now fights.** `finish_tactical` runs capacity enforcement, space cannon
  offense, space combat, and — if the active player holds the space — invasion. Combat, invasion
  and fleet enforcement were all implemented and uncalled; this is the wiring that lights them.
- **Contract divergence, deliberate:** those steps resolve inside one `step()` rather than one
  decision per step, because they ask inline through the `Table`. Every decision is still
  generated, validated and logged; a caller just cannot inspect between two casualty
  assignments. Making them resumable is a follow-up. Leaving a whole subsystem dark was worse.
- Only `PRODUCTION_UNRESOLVED` remains announced at the end of the action.
- Lints narrowed: dropped clippy `nursery`, allowed `too_many_arguments`, `similar_names`,
  `many_single_char_names`. Across this project those produced no defect while costing a
  round-trip each; every real bug came from a test.
- `fixtures.rs` centralises the test helpers six modules had duplicated, and carries the
  one-ring geometry trap (`Hub::across`) that was rediscovered independently in two of them.
- **Not done from the agreed refactor:** the `Ctx` struct bundling content/sources/table/dice/rng
  (47 signatures), and the generic `Window` trait. The wiring above delivered the visible unblock
  first; both remain worthwhile and are cheaper now that lints and fixtures are settled.

## M05-016-019 checkpoint (authoritative)

- **477 passing tests**: 121 `ti4-content`, 287 `ti4-engine`, 68 `ti4-model`, 1 doc-test.
- **The tactical action is complete end to end**: activate, move, capacity, space cannon,
  combat, invasion, production. Nothing in it is announced as missing any more.
- 68.2's half-cost rule is implemented: fighters and infantry are produced two at a time for
  one resource. The first version charged `ceil` and yielded one, doubling the cost of the two
  commonest units in the game.
- Biggest known production gap: **no faction-specific hulls**, so every seat builds the generic
  unit. The oracle notes this flattens faction differentiation; worth an early package once
  factions land.
- Largest architectural debt: combat, invasion and production all resolve inside one `step()`,
  breaking the one-decision-per-step contract. The generic `Window` trait fixes it.

## M01-006 checkpoint

- **CI exists and the workspace is clippy-clean.** `.github/workflows/ci.yml` runs on Windows —
  the supported platform per `MASTER_PLAN` — with `RUSTFLAGS: -D warnings`, so a warning cannot
  reach `main`.
- Getting there took the workspace from **181 clippy warnings to 0**: `--fix` cleared 71, the
  `cargo` group was allowed (these crates are private, not published), `must_use_candidate` was
  allowed (a data model with hundreds of accessors gains nothing from it on each), and the
  remaining 27 were fixed by hand.
- CI also verifies the content corpus checksums. If the corpus drifts, every content-derived
  test is measuring something other than the pinned data.
- Every step was run locally before committing; a CI that is red on day one gets ignored.

## Window trait checkpoint (authoritative)

- **479 passing tests**: 121 `ti4-content`, 289 `ti4-engine`, 68 `ti4-model`, 1 doc-test.
  Workspace clippy-clean under `-D warnings`; CI gate verified locally.
- `choice::Window` names the shape the engine had hand-rolled five times. Completion is "no
  choice is owed" rather than a separate flag, so a window cannot claim to be finished while
  holding a question, or hold one after it is done.
- `Window::drive` runs a whole sequence against a `Table`, so converting a subsystem does not
  break callers that do not want to step it — production's 13 tests passed unchanged.
- `ProductionWindow` is the first conversion. `production_can_be_stepped_one_decision_at_a_time`
  asserts the game is a whole, inspectable state between every pair of decisions.
- **Invasion and combat are still inline.** That is the remaining two-thirds of the debt.

## Window conversion complete (authoritative)

- **481 passing tests**; workspace clippy-clean under `-D warnings`; CI gate verified locally.
- `choice::Window` + `Resolving` (corpus, source scope, dice, RNG). All three inline subsystems
  are converted, and `AftermathWindow` composes combat → invasion → production into one
  resumable sequence the driver steps.
- **The one-decision-per-step contract is restored.** A caller can inspect the game between two
  casualty assignments again, which the inline version made impossible.
- Combat's queue is what keeps 78.6 true under stepping: both sides' hits are computed and
  queued before either is absorbed, so a casualty cannot reduce return fire already earned.
- `AftermathWindow` carries a log the driver drains, rather than reaching for `Game::emit` —
  two event sinks could disagree, one cannot.
- Bugs found while converting, both by tests: the first invasion draft created a fresh RNG
  inside `resolve` (would have silently left the seeded stream), and combat's first `settle`
  returned on a stage that owed no decision, ending fights unresolved.

## M05-011 checkpoint (authoritative)

- **484 passing tests**; workspace clippy-clean under `-D warnings`.
- Retreat (78.4, 78.7) is in the combat window as two more stages: announcing before any dice,
  and leaving once the round's hits are absorbed.
- 78.4b — the defender announcing silences the attacker; 78.4c — a player with nowhere to go is
  not asked at all; 78.7b — one destination is not a decision, and what the fleet cannot carry
  is stranded and lost; 78.7d — a command token follows to the destination.
- Retreat needs a map, so `CombatWindow::with_galaxy` supplies one and the driver passes the
  game's. Without a map there is nowhere to retreat to and the stage settles silently.
- M05 now has only combat modifiers/rerolls (M05-010) left of its rules packages.

## M06-004/005 checkpoint (authoritative)

- **494 passing tests**; workspace clippy-clean under `-D warnings`.
- Technology prerequisites and research: colour tracks, prerequisite counting, planet
  specialties standing in for prerequisites (90.8), faction-locked technologies (90.11), and
  `grant` kept separate from `research` because gaining outright (90.5) is not researching.
- **Another corpus sentinel found by its own test.** `requirements` is written as the literal
  string `"null"` on some records, not an absent field. A test asserting every requirement
  letter maps to a track caught it; without that, `"null"` would have parsed as no
  prerequisites and made those technologies free. The sentinel is now handled explicitly so a
  future typo is still an error rather than a free technology.
- Not ported: prerequisite waivers (faction, law, AI Development Algorithm), unit-upgrade
  application, starting technology, and the ~2,800 remaining lines of `technology.py`.

## M06-006 checkpoint (authoritative)

- **503 passing tests**; workspace clippy-clean under `-D warnings`.
- Exploration: a planet explores into its own trait deck (35.2b), the frontier deck needs no
  planet (35.5), attachments attach, and a card with no handler is announced `Unresolved`
  rather than dropped — an unimplemented card must be visible as a gap.
- An attachment drawn from the frontier is *discarded* explicitly, since there is no planet to
  attach it to.
- Relic fragments (35.9): three of a trait buy a relic, and **frontier fragments substitute for
  any trait** — counted towards every other, and spent only after the matching ones, because a
  wildcard spent while a matching fragment was available is a wildcard wasted.
- Not ported: the per-card instant/token handlers (the oracle has ~20), relic effects, and the
  attachment value changes that feed `planet_value_now`.

## M06-002/003 checkpoint (authoritative)

- **513 passing tests**; workspace clippy-clean under `-D warnings`.
- Transactions: neighbours by shared or adjacent presence (60), and **21.5 — a commodity
  becomes a trade good the moment it changes hands**. That rule is the whole economy: a
  commodity is worthless to its owner and valuable to everyone else, which is what makes a deal
  worth making at all.
- Both sides are taken before either is given, so a deal cannot be funded with what it is about
  to receive. Pinned by a test where the proposer holds nothing and the partner is sending four.
- Wormholes need no special case: they are adjacency as far as `Galaxy` is concerned (60.2), so
  asking the galaxy is asking the right question.
- Not ported: promissory notes, action-card trades, Hacan's neighbour exemption, Trade Convoys,
  and the Keleres I.I.H.Q. reach.

## M06-010 checkpoint (authoritative)

- **524 passing tests**; workspace clippy-clean under `-D warnings`.
- Secret objectives: the 45.4 hand limit counting *scored* secrets (a player who has scored two
  may hold only one more, and one whose every secret is scored has nothing to return — the rules
  working, not a stuck state), 61.18 scoring, and timing classes.
- **Only status secrets are offered at status time.** Action and agenda secrets are scored at
  the event that satisfies them; offering them at status changes both their timing and whether
  the triggering fact still exists.
- Requirements are a first tranche of 2, with unregistered secrets unscoreable — the same design
  the objective registry documents.
- **The corpus corrected me again.** I registered "4 space docks" from memory; the printed card
  says 3, and the alias I guessed (`usc`) does not exist. Both found by a test asserting every
  registered alias is a real card.

## M06 batch checkpoint (authoritative)

- **548 passing tests**; workspace clippy-clean under `-D warnings`; CI gate verified locally.
- Six M06 packages landed in one stretch: secrets, leaders, action cards, the payment planner,
  on top of technology, exploration and transactions earlier.
- **The corpus corrected me twice more**, both caught by the guard test asserting every
  registered alias is a real card: the space-dock secret wants 3 docks, not 4, and the war-sun
  secret alias I guessed does not exist. That guard has now paid for itself in four registries.
- Action cards dedup by **printed name, not alias** — a card printed four times has four
  aliases, so keying on the alias offers the same card twice and makes a sampling decider
  likelier to discard whichever it holds two of.
- The payment planner enumerates *minimal* plans: a plan exhausting a planet it did not need is
  the same payment plus waste, and offering it biases a sampler towards overpaying.
- A missing `# Errors` doc slipped past into a commit and would have failed the new CI on its
  first push. Caught and fixed in the following commit — the gate works.

## M06-017 checkpoint (authoritative)

- **555 passing tests**; workspace clippy-clean under `-D warnings`.
- `registry::ledger` counts coverage per registry, so the gap is a number rather than an
  impression. Measured now (PoK scope):

```
public objectives      30/40   implemented (75%)
secret objectives      14/40   implemented (35%)
action cards            0/122  implemented (0%)
agenda effects          7/63   implemented (11%)
exploration cards      41/80   implemented (51%)
relics                  3/17   implemented (18%)
```

- This exists because the registry design — an unhandled card is *unavailable*, never silently
  free — is right but makes coverage invisible from outside: a game where nobody can score looks
  identical to one where nobody has met a requirement.
- `implemented_never_exceeds_total` guards a registered alias the corpus does not have, from the
  opposite side to the per-registry alias tests.

## Objective tranche 3 checkpoint (authoritative)

- **559 passing tests**; workspace clippy-clean under `-D warnings`.
- Objective coverage 14/40 → **19/40 (48%)**, and the ledger reports it without anyone
  retyping a number.
- Third tranche is the fleet-and-space family: armadas in a *single* system (a fleet spread
  across the board is not an armada, which is the whole point of the card), units in
  planetless systems, and planets carrying exploration attachments.
- `fighters_do_not_make_an_armada` pins that the card counts non-fighter ships — nine fighters
  in one system score nothing.

## Secret tranche 2 checkpoint (authoritative)

- **561 passing tests**; workspace clippy-clean under `-D warnings`.
- Secret objective coverage 2/40 → **10/40 (25%)**: dreadnought and PDS counts, ships in six
  systems, four planets of one trait, twelve combined resources or influence, and four
  technologies of one colour.
- `four_technologies_of_one_colour_is_not_four_technologies` is the one worth keeping: one
  technology in each of four tracks is the *opposite* of what the card asks for, and a naive
  count would have scored it.

## Purchase objectives checkpoint (authoritative)

- **565 passing tests**; workspace clippy-clean under `-D warnings`.
- Objective coverage 19/40 → **27/40 (68%)**. The eight bought objectives (61.10) are covered by
  their *price* rather than a predicate, and the ledger counts them — otherwise it would
  under-report by eight.
- **Affordable to offer, paid to take.** Being asked spends nothing; `award` charges. A
  predicate that spent as a side effect would bill a player for merely being offered the card,
  and an unaffordable purchase now returns `Unaffordable` with the state untouched.
- Token costs are taken strategy pool first, then fleet, then tactic. The oracle leaves the
  split to the player; this fixed order is a simplification and is recorded as one.
- The payment planner from M06-001 does the resource and influence purchases, which is the
  first caller to use it.

## Agenda effects checkpoint (authoritative)

- **574 passing tests**; workspace clippy-clean under `-D warnings`; zero failures across all
  four crates (counted explicitly, not inferred from passing lines).
- First tranche of agenda effects: Economic Equality, Mutiny, Seed of an Empire. Unregistered
  agendas still resolve their vote and announce the effect unresolved.
- Mutiny reads the **ballot**, not the outcome — who voted which way is the whole card.
- Economic Equality wipes before it pays, so on Against it is purely destructive.
- Seed of an Empire's tie is the **speaker's decision** (8.18), passed in as a callback. With no
  decider the point is simply not awarded, rather than handed to whoever sorts first.

## Laws checkpoint (authoritative)

- **586 passing tests**; workspace clippy-clean under `-D warnings`; zero failures.
- **`state.laws` was a list nothing read.** The engine could enact every law and enforced none.
  Four now bite: Fleet Regulations caps the fleet pool at four, Sanctions caps the action-card
  hand at three, Shared Research opens the nebulae, and repealing Public Censure takes its
  victory point back.
- That last one matters: a repeal that only deleted the entry would leave the point behind for
  good.
- `movement.rs` had a `nebulae_open` flag that had existed unused since M05-003 — Shared
  Research is the first thing that can set it, via `MovementRules::with_laws`.
- `laws::unimplemented` reports the rest, so the gap stays queryable.

## Relics checkpoint (authoritative)

- **595 passing tests**; workspace clippy-clean under `-D warnings`; zero failures.
- First relic tranche: Dynamis Core, Book of Latvinia, and the Circlet of the Void.
- Dynamis Core's two halves are applied together — its standing "+2 commodity value" is folded
  into the gain its action pays, so the halves cannot disagree about the number.
- **The Circlet is read where the roll happens**, not at the card. `MovementRules` gained a
  `rifts_ignored` flag beside the other modifiers, so the immunity cannot be honoured in the
  legality rules and forgotten in `transit` — which is precisely the mistake Nav Suite nearly
  made and that M05-006's evidence flagged.
- `relics::unimplemented` reports the other fourteen.

## Integration checkpoint (authoritative)

- **597 passing tests**; workspace clippy-clean under `-D warnings`; zero failures; oracle
  verified.
- Two modules were built and never called. Both are wired now:
  - **Leaders ready at 81.6.** An exhausted agent that never readies reads, after a round or
    two, as a player who has run out of agents.
  - **`check_unlocks` fires on scoring**, not at end of phase (51.7). A hero unlocked by a third
    objective must not wait for a status phase the game may never reach.
- ~~Nothing catches an unwired module.~~ `wiring.rs` does, as of the next commit.

## Wiring guard checkpoint (authoritative)

- **604 passing tests**; workspace clippy-clean under `-D warnings`; zero failures.
- `wiring.rs` closes the process hole this project kept falling into: five modules had arrived
  correct, fully tested and called by nothing, because **a unit test proves a module works,
  never that anything uses it**.
- Two kinds of check. Behavioural: drive a real game and assert each phase and the 81.5 token
  gain were reached. Structural: assert the driver still names each subsystem, and that the
  laws which bite are still consulted where they bite.
- **The guard was verified by breaking it**: removing `leaders::ready_all` from the status phase
  makes it fail with "81.6 no longer readies leaders", then passes again on restore. A guard
  nobody has seen fail is decoration.

## Exploration handlers checkpoint (authoritative)

- **610 passing tests**; workspace clippy-clean under `-D warnings`; zero failures.
- Exploration coverage 0/80 → **41/80 (51%)** — fragments and attachments always resolved; six
  instant handlers now do too (the three Entity cards, both Kelres cards, Derelict Vessel).
- **A real bug the new guard found immediately.** `pub mod exploration;` had been sitting under
  a stray `#[cfg(test)]` since the module was added: it compiled, its own tests passed because
  they run under `cfg(test)`, and **nothing outside tests could call it**. The module was
  effectively absent from the library for several commits.
- `only_test_support_modules_are_test_gated` now asserts `fixtures` is the only test-gated
  module. Verified by gating `laws` and watching it fail with `["fixtures", "laws"]`.
- That is the second guard in `wiring.rs` proven by breaking it, and the first one it caught was
  a mistake I had already shipped.

## Agenda effects wired (authoritative)

- **628 passing tests**; workspace clippy-clean under `-D warnings`; zero failures.
- `agenda_effects` was the **sixth** module here to arrive correct, tested and uncalled. It is
  now wired into `close_vote` and added to the `wiring.rs` guard list.
- Order matters and is now right: 8.20 enacts a passing law *before* the effect runs, so an
  effect that reads the laws in play sees the one just passed.
- Seed of an Empire's tie goes through the `Table` as a generated decision, like every other
  choice, rather than being decided inside the effect.
- Incentive Program now reveals by **stage**, not off the top. The deck is stage I then stage II
  in order, so the first version would have revealed the wrong stage for most of a game.

## Secrets wired into scoring (authoritative)

- **631 passing tests**; workspace clippy-clean under `-D warnings`; zero failures.
- The 81.1 window now offers **secrets alongside public objectives** (61.6). Closes the gap
  M04-017's evidence recorded as "no secret-objective window".
- A player with no public objective in reach may still have a secret in reach, so the window no
  longer stops at the public list — which is exactly what left satisfied secrets unscoreable.
- A scored secret leaves its owner's hand (61.18); a public objective does not. Which module
  owns the card decides which path the award takes.
- Guarded in `wiring.rs`, since this is precisely the kind of call that falls out silently.
- **Still unwired:** `technology` and `transactions`, each waiting on an action that does not
  exist yet (research, and a transaction window). `exploration` is wired: 35.1 fires on capture.

## Exploration on capture (authoritative)

- **636 passing tests**; workspace clippy-clean under `-D warnings`; zero failures.
- 35.1 now fires: taking a planet **nobody held** explores it; taking one off a rival does not.
  `establish_control` already carried the previous holder for exactly this, and nothing used it
  — a caller told only that control changed would explore every conquest and draw cards the
  rules do not give.
- Guarded in `wiring.rs`. Dropping that call would silently stop every exploration in the game
  while the invasion still looked correct.

## Strategy card abilities (authoritative)

- **641 passing tests**; workspace clippy-clean under `-D warnings`; zero failures.
- First tranche of strategy-card abilities: **Leadership** (52.2/52.3) and **Technology**
  (91.2/91.3). Both use machinery already here — token gain, the payment planner, and research
  — which is why they were the two to start with.
- This is the first caller of `technology::research` and the second of `payment::plans`, so two
  more previously-unwired modules now have a real path into play.
- Not implemented and recorded as such: Leadership's per-token pool choice (the gain goes to
  the strategy pool), and Technology's second research for six resources.
- **Still unwired:** `transactions`, which needs a transaction action that does not exist.

## Implementation status

Measured, not claimed. "Scaffold" means the file compiles and has a plausible shape but its
behaviour is a placeholder.

| Crate | Status | Detail |
|---|---|---|
| `ti4-content` | **Implemented** | 28-category corpus loader, source scoping, TE id fallback, manifest cross-check, canonical digests, referential validation, unit catalogue, galaxy and adjacency, faction records and starting-fleet parsing. 121 tests. |
| `ti4-model` | **Implemented** | `id.rs`, `content_types.rs`, `hex.rs`, `state.rs` (45-field `Player`, 52-field `GameState`), `units.rs`, `view.rs` (redaction + leak check). 68 tests. |
| `ti4-engine` | **Partial** | Setup (all decks, two revealed public objectives, one secret per player), the four-phase state machine, the strategy draft (snake order), turn order by initiative, faction seating onto a board, the choice model (options, deciders, validation, decision log, replay), and the seeded RNG with dice. 101 tests. Nothing *generates* options yet, so no turn can be taken. Movement, combat, production, legality, the status phase and the agenda phase are absent — not stubbed. |
| `ti4-policy` | **Stub** | 5 × `todo!()` |
| `ti4-sim` | **Stub** | 6 × `todo!()` |
| `ti4-training` | **Stub** | 6 × `todo!()` |
| `ti4-bridge` | **Stub** | 5 × `todo!()` |
| `ti4-legacy` | **Stub** | 4 × `todo!()` |
| `ti4-cli` | **Stub** | Prints hardcoded version strings |
| `xtask` | **Stub** | Prints a version string |

### Milestone implementation

| Milestone | Planning | Implementation |
|---|---|---|
| M00 Oracle and baseline | Written | **Partial** — corpus imported and checksummed; deterministic public-state, redacted-view, choice, resolved-event, finished-outcome, and error projections are executable. No complete oracle exporter, generated fixtures, or differential corpus. Correctness baseline was only collected, never run. Performance baseline disputed (see audit). |
| M01 Repository bootstrap | Written | **Partial** — workspace, toolchain, lints, profiles exist. No CI, no coverage or mutation harness, no benchmark harness, no `benches/`. |
| M02 Content and model | Written | **In progress** — 001–003, 005, 007, 008, 009–012 done. 004, 006, 013–016 outstanding. |
| M03 Choice, timing, replay | Written | **Partial** — 001–006 and 008–015 done (choice, validation, deciders, decision log, pinned RNG with domain separation, dice, event/timing resolver, frequency scopes, canonical hashes, direct timing differential, and generated timing properties). 007 remains blocked; 016 outstanding. |
| M04 Game skeleton | Written | **Partial** — 001, 002, 003, 004, 006, 007 done. 005 (draft resolution), 008–016 outstanding. Setup now builds deterministic decks and deals setup cards. |
| M05 … M13 | Written | **Not started** |

## Repository state

- Working tree: clean after the M04-003 package commit on `wp/m04-003-deck-construction`
- Python oracle tree: clean, unmodified ✅
- Tests: **291 passing** (`cargo test --workspace`) — 121 `ti4-content`, 101 `ti4-engine`,
  68 `ti4-model`, 1 doc-test
- Integration tests: none. All tests are inline `#[cfg(test)]` modules.
- Content corpus: `crates/ti4-content/content/`, 29 files, 1,800 records, byte-identical to
  the oracle and checksummed in `CHECKSUMS.sha256`

## Open blockers and findings

1. **The oracle exporter is incomplete.** M00-009b/c/d/e/f/g provide tested state, redacted-view,
   choice, resolved-event, finished-outcome, and structured-error projections, but no CLI/NDJSON
   stream, fixture manifest, or complete reproducibility campaign exists. Until those are complete, no
   differential parity claim can be made, and M03-014, M04-015, M05-021, M06-018 and all of M12
   remain unimplementable. This is the single largest gap.
2. ~~No independent review of any code package.~~ Waived by the project owner
   (2026-08-11). Recorded here so the standard and the practice do not silently disagree.
3. **No CI.** M01-006/007/008/009 are marked complete but nothing runs on a push.
4. **Throughput gate is ~8× weaker than the master plan intends** — `M00-013a.md` labels a
   sequential measurement as 12-worker throughput. Changing a contractual gate needs
   authority; flagged, not corrected.
5. **`ti4-engine` behaviour is not oracle-derived.** Legality, movement, combat, and
   scoring are placeholders. They must be replaced against named oracle sources rather than
   extended.
6. ~~**`Galaxy` is not wired into the engine.**~~ Closed by M05-003: adjacency is now the basis
   of movement legality.
7. **The status phase is implemented except for scoring; the agenda phase except for voting.**
   A driven round now performs status steps 81.2–81.8 including the real 81.5 token choice, and
   stops at `StatusScoringUnimplemented` (81.1). The agenda phase reveals and orders, then stops
   at `AgendaChoicesUnimplemented`. Neither invents a default.

## Next actions

Rewritten against the tree as measured on 2026-08-12. The previous list named packages that
had already shipped — it is worth re-deriving this rather than trusting it.

1. ~~The `Window` trait.~~ **Done.** Production, invasion and combat are resumable, and
   `AftermathWindow` composes them so the driver steps the whole post-movement sequence one
   decision at a time. The contract divergence introduced in `310d7f5` is closed.
2. ~~M01-006 — CI.~~ Done: `.github/workflows/ci.yml` runs fmt, clippy (deny), tests, docs and
   the corpus checksums on Windows.
3. **M05-010 — combat modifiers and rerolls**, the last tactical rule. (M05-011 retreat is
   done: announce before dice, defender first, the announcement silences the attacker, and
   what a retreating fleet cannot carry is stranded.)
4. **M00-013 — the performance baseline**, unblocked since the oracle was cleaned and the thing
   that validates the premise of the rewrite.
5. **M06 — general rules**: reactions (016), agenda effects/laws (014), relic effects, and the
   per-card handlers behind exploration and action cards. Done: payment planner (001),
   transactions (002/003), technology (004/005), exploration and fragments (006, part of 007),
   action cards (008), secrets (010) and leaders (015).

## Decisions in force

- Windows-first isolated Rust rewrite.
- The Python repository at `37061c5` is a read-only behavioural oracle.
- Public/semantic compatibility with translation layers where documented.
- Content is compiled into the binary; `ContentStore::from_dir` remains for regenerated or
  reduced corpora, and a test proves the two agree.
- Corpus files are committed byte-identical with SHA-256 checksums and `.gitattributes`
  pinning them against end-of-line translation.
- Independent review is waived for implementation packages by the project owner
  (2026-08-11). Evidence files record what was verified and by what test, not a reviewer.
- Scoped permissions per `SCOPED_PERMISSIONS.md`: packages default to P0/P1.




## Handover (P1-c complete)

- **Objective:** Phase 1 decision-surface alignment, sub-package P1-c — ground-commit, ready-planet
  and free-trade-replenishment surfaces aligned to oracle identity per `engine/invasion.py:253–324`,
  `engine/strategy.py:633–654`, `engine/strategy.py:206–246` (spec `out/p1c_spec.md`).
- **Active milestone/package:** Phase 1 / P1-c — COMPLETE. Next ready package: **P1-f** (misc wording +
  blind-secondary window shape, largest remaining Phase-1 item), then P1-g (behavioral payment/jamming),
  conditional P1-h gated on T6 tie-break hits post-P1-g.
- **Status:** D1 commit surface at both sites: prompt `"commit ground forces in {sys}"`, ids
  `commit|{n}|{planet}`, kind `commit` (`COMMIT_KIND`), label `land {type}[ (damaged)] on {planet}`,
  dedup key `(type, sustained_damage, planet)`, terminator `("done_committing","decline",...)`, plus the
  27.1 Mecatol Rex filter re-read each commit iteration (`mr` in system `"18"`). D2 ready: prompt
  `"ready which planet"`, labels `ready {planet}`, decline removed (forced choice per iteration).
  D3 replenishment: prompt `"let another player replenish commodities"`, faction-name options kind
  `replenish` label `{name} replenishes commodities`, terminator `("done","decline","nobody else
  replenishes")`, explicit generic exclusion, candidate-scoped first-match name→seat. D4 mechanical
  policy rename: bot.rs dispatch/parser/helpers land→commit (identical scores), learned.rs dead local
  arm + LOCAL_DIVERGENCES 13→12, two test fixtures. Six red-first tests (confirmed RED pre-implementation)
  all green; no other test changed.
- **Branch/HEAD:** `codex/stage1-parity-fixes` at P1-e commit `00d7415`; this package's changes
  staged for the focused commit: invasion.rs, strategy_cards.rs, bot.rs, learned.rs + plans updates.
- **Working-tree state:** modified (staged-for-commit): `crates/ti4-engine/src/invasion.rs`,
  `crates/ti4-engine/src/strategy_cards.rs`, `crates/ti4-policy/src/bot.rs`,
  `crates/ti4-policy/src/learned.rs`, `plans/evidence/STAGE2-STALL-INVESTIGATION.md` (§P1-c spec→complete),
  `plans/CONTINUATION_PLAN.md` (P1-c row done). Untracked out/: p1c trace artifacts. Oracle repo untouched.
- **Tests last run and exact results:** fmt clean; ti4-engine **782** lib + 5 doctests (+6 new);
  ti4-policy 102/102 (golden-heads routing with shrunk local-divergence ledger green); ti4-content
  126+1; ti4-training 98/98 release; clippy zero warnings both crates all targets; workspace check clean.
- **Compatibility evidence (T6 vs `out/py_ff_learn_83000001_p1a2.json`, 1868 py / 1147 rust):**
  all six factions max_score_gap=0.000000, choice_mismatches_within_common=0; first structural mismatch
  per faction = recorded classes only (hacan idx1 F1 leader component; other five idx1 blind secondary
  no/yes vocabulary + window shape). Surface-level diff beyond idx alignment: every same-event commit
  pair matches option sets AND labels (hacan 18/18, jolnar 2/2, l1z1x 5/5, letnev 3/3, xxcha 4/4); ready
  and replenish rows oracle-shaped on both sides with differences only from post-break game-state cascade.
  Legacy-form scan of the whole Rust trace: zero old prompts, zero kind-`land`; new surfaces 125 commit +
  10 ready + 33 replenish rows. Rust-vs-rust p1e→p1c: per-faction decision totals identical (no window-shape
  change); first fork in every faction is the rename itself (`land|…`→`commit|…`, or `decline`→`done` for
  jolnar's replenishment), downstream forks 3–27 all cascades — intended feature-space movement, zero forks
  before any renamed-surface decision.
- **Decisions made:** (1) shared helpers `landable_planets`/`commit_options` de-duplicate the two commit
  sites instead of copying oracle text twice. (2) 27.1 Mecatol exclusion treated as option-set identity
  (P1-c scope), re-read per iteration to match the oracle's mid-sequence token removal. (3) explicit
  `faction != "generic"` retain clause kept despite natural equivalence with commodity_limit=0 — spec parity
  + guard against future content. (4) no emissions implemented (verified inert or Phase-2-owned).
- **Open review findings or blockers:** none. No frontier review required: identity/option-set text only,
  no legality/timing/window-shape change; red-first tests + T6 + surface diff + rust-vs-rust fork analysis
  in evidence §P1-c.
- **Next exact action/command:** P1-f spec from the recorded class row (em-dash → `--` prompt normalization
  across prompts; leadership purchase loop vs Rust blind `decline/follow` secondaries incl. no/yes
  vocabulary) — investigate oracle + Rust sites read-only, write `out/p1f_spec.md`, red-first tests,
  implement, gates, T6 re-run vs the same Python artifact.
- **Files to read first after compaction:** this file (handover section); `plans/CONTINUATION_PLAN.md`
  Phase 1 table; evidence §P1-c (`plans/evidence/STAGE2-STALL-INVESTIGATION.md`, from line ~1244).


## Handover (P1-g complete)

- **Objective:** Phase 1 decision-surface alignment, sub-package P1-g — payment-loop mechanics
  (F5 lone-option auto-pick, F6 oracle payload identity, F7 zero-worth face filtering + the
  affordability guard), Xxcha *Archon's Gift* alternate payment faces (F2), Signal Jamming option-set
  parity (F12) per `engine/production.py` and `_jamming_systems`. Spec `out/p1g_spec.md`, split
  f5/f6/f7/f12/f-payload recorded pre-implementation.
- **Active milestone/package:** Phase 1 / P1-g — COMPLETE. Conditional P1-h: CHECKED AND SKIPPED per
  its own gate (post-P1-g T6 shows zero tie-break hits; F11 stays a documented known difference in the
  Phase-2 backlog). Next ready actions: item 2 of `plans/CONTINUATION_PLAN.md` "Path to a startable
  training run" — Phase-1 exit checkpoint (consolidated evidence + handover + compaction), then item 3,
  the C1 dry pilot.
- **Status:** lone payment options are consumed silently at settle (oracle `pay()` auto-pick) so Rust
  traces contain zero degenerate payment questions; payment option identity matches oracle
  (`kind`/`owed`/`source`/`worth`, cross-source `|{source}` id suffix only when the face is genuinely
  cross-source — planet ids containing '|' or single-segment decode safely); zero-worth faces are never
  offered and the affordability guard filters any face whose use would strand the bill; Archon's Gift
  offers its alternate faces with per-face values; `available()` sums max-face per planet. Jamming: pub
  `jamming_systems` (effective galaxy from the driver, ships-only reach, home exclusion via
  `is_home_system`, BTreeSet-sorted), eligibility in `is_playable`/`available_actions`, perform gate
  fizzles on the empty set. PLANET_EXHAUSTED/BREAKTHROUGH_TRIGGERED emissions remain deferred as a
  documented known difference (T6b-verified no bound window).
- **F14 (in-package regression found and fixed):** an early version of P1-g's settle loop had an empty
  `Stage::Done => {}` arm → infinite loop: any game with one production decline could never finish,
  so the multi-hour suite runs genuinely did not terminate (one process accumulated ~65 CPU-hours
  spinning a single instruction) — pathological by construction, not slow. Found by
  A/B-bisecting the P1-f worktree plus temporary instrumentation, fixed with `Stage::Done => break`,
  guarded by regression test `a_declined_production_window_settles_without_spinning`. Post-fix T6 trace
  regenerates **byte-identical** to `out/rust_ff_83000001_p1g.json` (zero behavioral effect on recorded
  evidence). No pre-existing test drove the decline→settle path, which is why all unit tests were green
  on the spinning code.
- **Branch/HEAD:** `codex/stage1-parity-fixes` at P1-f commit `6958935`; this package's changes are
  committed as the focused P1-g commit (this handover included).
- **Working-tree state (at commit):** modified: `crates/ti4-engine/src/{production,action_cards,
  strategy_cards,game}.rs`, `crates/ti4-policy/src/bot.rs` (payload key rename + fixtures),
  `crates/ti4-sim/src/run.rs` (fmt-only re-wrap carried over from P1-f's committed test),
  `plans/evidence/STAGE2-STALL-INVESTIGATION.md` (§P1-g: gates final, F14), `plans/CONTINUATION_PLAN.md`
  (path items 0–1 closed). Untracked out/: p1g spec/handover artifacts. Oracle repo untouched.
- **Tests last run and exact results:** fmt clean workspace-wide; clippy --all-targets zero warnings on
  ti4-engine/ti4-sim/ti4-policy; ti4-content 126, ti4-engine **793 lib + 5 doctests** (net +11 over P1-f:
  T-A lone-option auto-pick, T-B Archon's Gift faces/guard/payloads, T-C zero-worth never offered,
  T-D window settling a lone option in `Stage::Paying`, jamming set/eligibility/no-galaxy rewrites, F14
  regression test; two P1-b prompt tests flipped to assert auto-pick per f5), ti4-legacy 25, ti4-model
  72, ti4-policy 102, **ti4-sim 27/27 (release, wall 0.34 s)**, **ti4-training lib 98/98 (release, wall
  1.16 s)**.
- **Compatibility evidence:** T6 seed 83000001 rot 0 (rounds 4, greedy temp 0.0001, `--full-features`,
  pool save52_e400_n8192) vs unchanged Python artifact `out/py_ff_learn_83000001_p1a2.json` (1868):
  rust p1g = **1106 decisions**; all six factions **max_score_gap=0.000000, zero choice mismatches** on
  common prefixes; first structural mismatch per faction only the recorded F1 class at idx 1/2 (hacan
  leader component absent; sol `faction|orbital_drop` id). Payment questions: Rust p1g **91 with zero
  degenerate (<2 options)** vs P1-f's 106 with 15 and Python's 111 with 0. rust-vs-rust p1f→p1g: every
  first break is an intended delta; pre-fork score movement exactly the f-payload token rename
  (bucket-verified). Trace regenerated post-F14-fix: byte-identical to the recorded artifact.
- **Decisions made:** (1) lone-option auto-pick happens in settle rather than by filtering the option
  list — mirrors oracle `pay()` and keeps degenerate questions out of traces entirely. (2) payment id
  decode keys off the payload's `source` instead of re-splitting the id string, because planet ids may
  contain '|' or be single-segment. (3) zero-worth faces excluded at offer time AND re-checked by the
  affordability guard on every face (both match Python). (4) jamming scope limited to the system set +
  eligibility; the other ~17 per-card eligibility lambdas stay in the F13 backlog. (5) P1-h skipped per
  its recorded gate condition (zero tie-break hits post-P1-g).
- **Open review findings or blockers:** none for P1-g proper; F14 was self-discovered, fixed, and
  regression-tested inside this package. No frontier review required: payment/jamming surface identity
  only — no legality/timing/hidden-information change. Carried unchanged: F1 (leader-component gap)
  gates full-game alignment → Phase 2; F11 tie-break stays Phase-2 backlog; F13 card lambdas stay in the
  Phase-2/Phase-3 backlog with the `is_playable` comment as anchor.
- **Next exact action/command:** C1 dry pilot COMPLETED AND PASSED (2026-08-16; log
  `out/c1_dry_pilot.log`, wall 241 s): champion loaded at internal update u3050, +50 updates to u3100
  with **6,358,098 decisions, 0 errors, 0 zero-movement updates**; boundary panel (192 games/faction)
  clean, aggregate gain +2.229 (se 0.175; `--accept-sigmas 0` per C1 design — sanity signal only);
  checkpoint `out/stage2_p1g_dry_u50.json` structurally valid (`resumed_from` carries the champion
  sha256). Path items 0–3 of `plans/CONTINUATION_PLAN.md` all hold → **Rust is verified startable**.
  STOP POINT (plan decision point #3): a full T4-equivalent run (C2/C3: same seed stream, `--every 50
  --accept-sigmas 0`, n=32, `--panel-step 32`) now **requires operator approval of pre-registered
  thresholds before launch**; Phase 2 legality/timing work additionally requires a frontier review per
  AGENTS.md. Either way the next session resumes from this handover + the continuation plan.
- **Files to read first after compaction:** this file (handover section); `plans/CONTINUATION_PLAN.md`
  "Path to a startable training run" + Phase 1 table; evidence §P1-g (`plans/evidence/
  STAGE2-STALL-INVESTIGATION.md`, last sections, incl. F14).

## Status update: C2 real-gate run stopped; performance reviewed (2026-08-16)

- **C2** (operator-approved, real 2.0σ gate, `--panel-step 32`, +500 updates from u3050): 4
  boundaries before stop — u3100 **+PROMOTED all six factions** (val +2.349 / conf +1.990); u3150,
  u3200, u3250 rejected by per-faction clearance vetoes with no isolated fallback survivor. Every
  boundary showed a real paired gain (+0.98 to +2.35) — the zero-signal stall is gone; rejections are
  principled guardrails. Run ended after the u3250 checkpoint, exit code 1 with **no panic/error
  text** → consistent with external kill (operator "stop run"), no evidence of internal crash.
- **Performance review (spotty CPU — confirmed and quantified):** probe + 3-s sampler over a
  25-update run: utilization % of 32 cores min 3.1 / median 43.1 / mean 34.8 / max 89.8; learning
  phase ~60% (serial `apply()` gradient step between parallel rollout waves, stage1.rs:287); deep
  valleys at phase transitions; **rejected boundaries cost ~3× promoted ones** (~153–182 s each)
  because the isolated fallback runs up to six full validation panels — in C2 that was ~45% of wall
  time producing no promotion. Learning cost drifts +47% per chunk over 200 updates (+9.5% decisions
  → +33% per-decision) — cause not yet isolated. Full numbers: evidence §"C2 real-gate run +
  performance review". Recommendations R1–R4 (pipeline the loop, cheaper fallback, per-eval timing
  logs, drift probe) recorded there; none implemented pending operator decision.
- **New standing rule (operator):** every test/verification run must finish within a **10-minute
  timeout or it is considered failed/broken**; long training runs need explicit approval + monitored
  checkpoints. All subsequent verification work is designed to fit the budget (e.g., 25-update probe
  ≈ 4 min).
- **Next exact action:** operator decision on R1–R4 (which, if any, to implement) and whether to
  relaunch a full run after fixes; otherwise resume from this handover. No uncommitted changes.

## Status update: sustained-learning test launched (2026-08-16)

- **Operator task:** determine whether Stage-2 can *sustainably* get at least one faction to a
  3.0 average VP; fix what blocks it if not. Wall-time improvements approved as prerequisite.
- **VP ceiling probe** (new example `crates/ti4-training/examples/vp_ceiling_probe.rs`; u3100
  accepted champion, 192 games/faction): max game VP is only **4–5**; current means 1.35–1.73 with
  p50 of 1–2 and only ~8–20% of games reaching ≥3 (letnev/jolnar best at ~20%). A *mean* of 3.0
  therefore requires the median game to be a ≥3-VP game — reachable in principle, but a step change
  from current play, not an extrapolation. C2 candidates already hit 1.96 (jolnar) after just 50
  updates, so the early trend is steep; whether it continues toward 3.0 is exactly what this run
  tests.
- **Wall-time attempt reverted:** a `rayon::join` parallelization of the three boundary evaluations
  was implemented and determinism-verified (identical decisions/gains/promotion to C2) but measured
  **3× slower** on the boundary (174 s vs 55 s): concurrent CPU-bound tasks on a fixed pool only
  split threads, plus ~576 live game states thrash memory. Reverted; sequential evaluations are
  back. Real levers remain: fewer evaluations per rejected boundary (isolated-fallback pre-filter —
  semantic change, needs approval) or faster games (engine-level).
- **Sustained run launched:** `stage2_training` resuming from C2's checkpoint (learner u3250,
  accepted = u3100 champion), **+1000 updates → u4250**, `--every 50 --panel-step 32`, real 2.0σ
  gate, seed stream continues at base 74_000_000 stride 10_000 (next chunk u3250..u3300). Log
  `out/sustained_u1000.log`, checkpoint written every boundary. Success = some faction's accepted
  panel mean VP reaches ≥3.0 and holds; plateau well below after ~1000 updates = the finding, then
  diagnose (ceiling vs gate vs reward). ETA ~1.5–2 h; compact reports at boundaries.
- **Standing rule (operator, 2026-08-16):** never train or evaluate with an 8-round horizon unless
  specifically instructed — it wastes compute. All runs stay on the 4-round horizon.
- **Operator claim under investigation:** "the python version used to crack 5 vp at the last run
  before port to rust started." Exhaustive search of `D:/Projects/ti4-engine` (all .log files, all
  JSON checkpoints incl. subdirectories and audit/learner metrics, telemetry arrays, git history for
  deleted logs) found **no record of any faction averaging ≥2.5 VP**; the maximum anywhere is
  jolnar 2.34 at u3350 in `out/stage2_pg_six_c_20260810.log`. Single-game maxima of 5 VP do exist
  (Rust ceiling probe: jolnar/l1z1x max=5 on current profiles), so the memory may be a single-game
  result. Awaiting operator pointer to the specific full log if one exists elsewhere.
- **RESOLVED: the "python cracked 5 vp" claim (operator pointer: python branch / stats folder / HDD
  backup).** Found in `E:\ti4-engine\archive` (HDD backup of evolution-trainer runs, all with
  `horizon_round: 4`, i.e. directly comparable):
  - `stage2_blank_002`: **xxcha sustained ~4.5–4.6 avg VP** (4.57@g122, 4.54@g41, 4.53@g43,
    4.49@g124, 4.43@g130) — this is the "cracked 5 vp" memory; no run ever recorded ≥5.0 exactly.
  - `stage2_rich_001`: sol 4.25@g414, hacan 4.25@g363 — its manifest shows **`--vp-gate 5.0`**
    (the target the operator remembers).
  - `stage2_from_s1gen222_001`: letnev ~4.0–4.1 sustained g238–g258.
  - `stage2_blank_001`: letnev 2.71 (early, short run).
  **Critical distinction:** these are the *evolution* trainer (`tools/evolve_save54_three_player.py`,
  mutation + per-faction champion selection, seeds=12/gen), NOT the policy-gradient trainer that was
  ported to Rust. The pg trainer plateaus at ~2.0–2.3 avg VP in both Python and Rust on the same
  horizon. So: sustained ≥3 avg VP is *demonstrated achievable* on this horizon — by a different
  algorithm than the one currently implemented in Rust. Decision needed: keep pushing pg, port the
  evolution approach, or hybrid.

## Pivot: fixing the pg plateau (2026-08-16)

- **Operator direction:** the target stays a *fully learned, gradient-search* policy (the evolution
  archive results are only proof that ≥3 avg VP is reachable on this horizon by some policy class;
  they evolved hand-crafted heuristic weights, not learned heads). So: find and fix what caps the
  pg trainer at ~2.0–2.3.
- **Diagnosis from sustained-run telemetry (u3050..u3900):** `mean_return_std` stays healthy and
  slowly rises (~1.8→2.0) while per-head weight `movement` decays (~1.6→1.3). Return variance is not
  dying; the optimizer is taking smaller steps in a region where games still differ but VP does not
  improve — the signature of a **local optimum / flat basin**, not entropy collapse or reward death.
  The current policy already plays ≥3-VP games in ~8–20% of games (ceiling probe), so the feature
  space can express good play; it must be made consistent.
- **Sustained run stopped at u3900** (checkpointed cleanly; operator redirected priorities). Final
  state: jolnar accepted @u3550 (~2.1 VP), other five factions at u3100 heads; Rust pg dynamics
  matched Python's boundary-for-boundary through this point.
- **Experiment A launched:** `--entropy 0.05` (5× the reference 0.01, new CLI flag added to
  stage2_training.rs and recorded in checkpoint arguments), from u3900, +400 updates → u4300,
  `--every 100 --panel-step 32`, real gate. Log `out/exp_entropy05.log`. Success = candidate avg VP
  clearly exceeds the ~2.1–2.3 plateau (ideally a promotion ≥3). If it fails: next experiments are
  blank-start stage-2 (tests whether the Stage-1 prior is the blocker) and reward shaping toward
  high-VP games.
- **Experiment A (entropy 0.05 only) — VERDICT: no sustained break.** u3900→u4300, four boundaries,
  all rejected; candidate VP per boundary (sol/letnev/xxcha/hacan/jolnar/l1z1x):
  - u4000: 2.17 / 2.20 / 2.15 / 2.02 / **2.38** / 2.23
  - u4100: 1.92 / 2.15 / 2.14 / 2.04 / 2.27 / 2.03
  - u4200: 1.99 / 2.12 / 2.08 / 1.91 / 2.05 / 1.93
  - u4300: 1.92 / 2.08 / 2.00 / 1.85 / 2.10 / 1.81
  The u4000 spike regressed; no upward trend (slight downward drift). Exploration without
  directional pressure does not escape the basin — it wanders in it. Rejections were clearance
  vetoes (jolnar/sol), consistent with VP-for-clearance trades under flatter policies.
- **Implemented `--high-vp-bonus`** (default 0 = reference behavior preserved): terminal reward
  bonus paid when a seat finishes with ≥3 VP, credited at the final slot so every decision's return
  carries it exactly; field on `Reward` + `FactionPlan`, CLI flag recorded in checkpoint arguments,
  focused unit test (`the_high_vp_bonus_pays_only_when_the_seat_finishes_at_or_above_three`).
  Rationale: the reference reward pays per-VP via potential differences, so crossing 2→3 is one
  more unit of weight spread over ~7M decisions — weak credit for exactly the threshold that
  matters. Full `ti4-training --lib` release suite running before commit.
- **Experiment B (queued):** entropy 0.05 + high-vp-bonus 0.5, same u3900 start, +400 updates →
  u4300, `--every 100 --panel-step 32`, real gate — directly comparable to A. Success = sustained
  candidate VP above ~2.3 with an upward trend (ideally a promotion ≥3). If B also fails: next is
  blank-start stage-2 (tests whether the Stage-1 prior itself is the blocker) and/or bonus 1.0.
- **Operator mission (2026-08-16):** clear Stage 2 with learned heads; Rust must be ≥5× faster than a
  comparable Python process on the same workload; no prolonged CPU idle during runs; learning-algorithm
  experiments explicitly permitted. Standing rules unchanged (4-round horizon, 10-minute test budget for
  verification runs — long training runs are operator-approved monitored work).
- **Experiment B (entropy 0.05 + high-vp-bonus 0.5) — VERDICT: no break.** u3900→u4300, four boundaries,
  all rejected; candidate VP per boundary (sol/letnev/xxcha/hacan/jolnar/l1z1x):
  - u4000: 2.05 / **2.28** / 2.16 / 2.16 / 2.22 / 2.23
  - u4100: 1.88 / 2.10 / — / — / — / — (rejected)
  - u4200: 1.95 / 2.12 / — / — / — / — (rejected; jolnar clearance −0.036 veto)
  - u4300: 2.02 / 2.09 / — / — / — / — (rejected; xxcha clearance −0.073 veto)
- **Revised diagnosis (supersedes "plateau/basin"):** the learner is NOT stuck. Checkpoint audit history
  shows candidates beat the frozen champion by **+1.9 to +2.1 aggregate VP per boundary (~6–7σ paired)**;
  every rejection is a **per-faction clearance veto (0.03, raw panel means)**: the ≥3-VP bonus pressure makes
  the learner trade opening safety for mid-game VP (xxcha −0.073 clr for +0.51 VP at u4300). The gate blocks
  real Pareto moves; isolated fallback cannot save them because gains are interdependent across the table.
  Champion frozen at u3100/u3550 values while learner oscillates ~2.0–2.3 = equilibrium, not a basin.
- **Plan (out/stage2_clearance_plan.md):** P-A `--pipeline` staleness=1 training loop (fixes tail-idle,
  target ≥90% duty); P-B clearance-aware reward (`--clearance-floor/--clearance-weight`, default off) so the
  learner finds high-VP play inside the gate's 0.03 clearance band; P-C long clearing run with real gate.
  Gate semantics stay oracle-compatible (no veto changes).
- **Python comparable baseline RUNNING:** `out/py_baseline_launch.sh` — same champion u3050, seed base 74M
  stride 10k, 16 seeds × 6 rotations, horizon 4, save52 pool, workers=32, eval-every 50, 100 updates.
  First boundary u3100: **all six factions promoted** — matches Rust C2's first-boundary decision exactly
  (parity signal). CPU sampler logging to out/pybase_cpu_samples.txt for duty-cycle measurement.
- **Reachability proof in-protocol:** six-faction evolution run `save52_six_stage2_FINAL.json` reached
  hacan best_vp=4.83@g55, jolnar 3.96, letnev 3.71 (panel means, horizon 4) — ≥3 VP is reachable in the
  six-faction protocol by some policy class; pg must be steered there.

## Scheduling package + speed measurement (2026-08-16)

- **P-A/P-C scheduling implemented and measured** (evidence: `plans/evidence/STAGE2-SCHEDULING-WAVES.md`):
  - Root cause of spotty CPU: game-length variance leaves each update's fixed 96-game batch with
    stragglers; learning phase ran at **52% utilization** (16.7/32 cores) in the pure-learning probe.
  - `--rollout-depth D` (default 1 = reference): D consecutive updates' games roll out in one shared
    parallel wave before applying gradients (bounded staleness D-1; per-game results and apply order
    unchanged). **Depth=4: learning 357.3s → 293.0s (+18%), utilization 52% → 63%.** Depth=8 regresses
    (305.5s) — depth 4 is the recommended speed setting.
  - `--pipeline` (background-thread overlap, staleness 1): implemented but **measured worse on this
    machine** (652s vs 503s per 100 updates; memory pressure from ~192 live games). Kept behind its
    flag for other machines. Mutually exclusive with `--rollout-depth > 1` at the CLI.
  - New unit tests: wave == sequential per-group play (frozen profiles); empty-group handling.
    `ti4-training --lib` release **102/102**; clippy/fmt clean.
- **Hot-path attribution** (temporary instrumentation, removed): feature construction ≈ 5.5× scoring;
  policy side is ~32% of game time, engine ~68%. Policy micro-opts alone cannot reach 5x wall-time.
- **Python comparables measured** (same champion u3050, seed base 74M/stride 10k, save52 pool):
  - Python `workers=32`, train-seeds=16: **1034 s / 100 updates** (`out/py_baseline_u100.*`); u3100 all
    six promoted (matches Rust C2). Per-game ~1.73 core-s; avg 19/32 cores.
  - Python natural defaults `workers=1`, train-seeds=8: **2977 s / 25 updates** (`out/py_default_u25.*`);
    u3075 all six promoted (consistent dynamics). Per-game ~1.74 core-s single worker.
  - Rust reference depth=1: **503 s / 100 updates**; per-game ~0.62 core-s → **~2.8x per-game engine
    speedup** vs Python; wall-time ratio vs maxed-out Python currently 2.0-2.8x depending on whole-run
    average cores (Python sustains ~19, Rust reference ~14.3).
- **5x arithmetic recorded plainly:** total CPU for the standard workload ≈ 7,000 core-s; on 32 cores
  the wall-time floor is ~219 s = a ceiling of **~4.7x vs Python@w32 even at perfect utilization**.
  Reaching 5x against maxed-out Python also needs ~6-8% less total CPU work (boundary/I/O) or slightly
  faster games. Against Python's shipped default (`workers=1`, train-seeds=8), Rust is **>40x** on
  identical game counts. Which configuration defines "comparable" is an operator call; both numbers are
  in the evidence file.
- **Next:** (1) commit scheduling package; (2) implement P-B clearance-aware reward
  (`--clearance-weight`, default 0, final-slot penalty per uncleared opening — design in
  `out/pb_prep_notes.md`); (3) launch the long clearing run from u4300 with entropy 0.05 +
  high-vp-bonus 0.5 + clearance-weight ~1.0 + `--rollout-depth 4`, real gate, monitored checkpoints.

## Status (2026-08-16, after scheduling commit)

- Scheduling package committed: `0d0c30a` (`--rollout-depth`, `--pipeline`, group-wave rollout API,
  evidence `plans/evidence/STAGE2-SCHEDULING-WAVES.md`).
- P-B clearance-aware reward committed: `f168a26` (`--clearance-weight`, default 0 = reference;
  unit test `the_clearance_penalty_pays_only_when_the_opening_misses_the_bar`; lib suite 103/103).
- **P-C clearing run LAUNCHED** (operator-approved long run, monitored): from u4300 checkpoint
  (`out/exp_bonus05_entropy05_u400.json`), +600 updates → u4900, `--entropy 0.05 --high-vp-bonus 0.5
  --clearance-weight 1.0 --rollout-depth 4`, real gate, `--every 100 --panel-step 32`. Log
  `out/clearing_run.log`, CPU sampler `out/clearing_cpu.txt` (10 s cadence), output checkpoint
  `out/clearing_run_u600.json`. Expected ~50 min. Success signal: clearance vetoes stop firing and
  promotions resume with candidate VP trending up; target remains ≥3.0 mean VP for at least one
  faction on its panel. If flat after u4500: raise clearance-weight to 2.0 or high-vp-bonus to 1.0.
- **F15 (boundary-phase idle gap): diagnosed, fixed, verified.** Operator report: CPU idles ~10 s
  between spikes during runs. Instrumented probes (`out/dbg_boundary*.log`) proved each 192-game
  panel plays in ~3–4 s but a ~10–12 s serial gap followed every panel evaluation (growing to
  ~12.6 s within one boundary). Root cause: the trainer's `evaluate()` consumed full training
  rollouts — every decision's trajectory with all legal options' feature vectors plus per-decision
  progress snapshots — gigabytes per panel that it never reads, freed serially on the main thread
  at each drop. Fix: new evaluation-only rollout API (`play_rotated_batch_evaluation`,
  `play_rotated_save54_pool_batch_evaluation`) with bots built without `.recording()`; all prior
  public APIs unchanged (training still records). Parity unit test pins identical finals + empty
  trajectories. Result on the identical workload: boundary phase **~136 s → ~15 s**, learning
  block unchanged (~228–231 s), gate decisions unchanged (same clearance vetoes). Evidence:
  `plans/evidence/STAGE2-SCHEDULING-WAVES.md` §F15. Checks: ti4-training lib **104/104** release,
  clippy no new warnings vs HEAD, fmt clean.
- **P-C clearing run status:** killed at u4700 (checkpointed cleanly to
  `out/clearing_run_u600.json`) when the F15 idle-gap complaint was filed; boundaries u4400–u4700
  all rejected by clearance vetoes with VP flat ~2.0–2.3 — the reward-shape question (P-B/P-C) is
  still open and independent of F15. Resume from `out/clearing_run_u600.json` (+600 updates →
  u4900, same flags: `--entropy 0.05 --high-vp-bonus 0.5 --clearance-weight 1.0 --rollout-depth 4`,
  real gate, `--every 100 --panel-step 32`) now that the trainer no longer idles between panels.
