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

- Historical Python repository: `D:\Projects\ti4-engine` (read-only; not behavioral acceptance)
- Historical branch: `codex/fully-learned-policy`
- Historical pinned commit: `37061c511a4780d4c0719e0342533a498cd4b457`
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
- The Python repository at `37061c5` is a read-only historical scope/artifact/performance reference;
  official rules and accepted Rust specifications govern behavior.
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
- **Operator gate-semantics decision (P-E): per-faction vetoes disabled for Stage-2 runs.** Rationale
  (operator, verbatim intent): one candidate regressing must not block every other candidate that
  did not; if a faction's metrics dip because the others improved, that is competition and it will
  catch up. Implementation: no gate code change — existing CLI flags set to unfireable values:
  `--max-faction-clearance-regression 1.0` (clearance ∈ [0,1], so candidate < champion − 1.0 is
  impossible) and `--max-faction-vp-regression 10.0` (VP cannot regress by more than ~5). The
  aggregate margin (0.05/faction) and paired 2σ evidence clauses remain in force, so a promotion
  must still be a real net improvement beyond noise; the isolated fallback's own-VP-improvement
  requirement also remains. This is an explicit project-level deviation from the oracle's default
  gate (Python defaults 0.03/0.15) authorized by the operator; reference runs keep the defaults.
  Motivation data: at u5100 every faction except jolnar beat the frozen champion by +0.2 to +0.5
  VP, yet all boundaries were rejected on sol/jolnar clearance vetoes while hacan/xxcha had already
  recovered inside the band; the uniform clearance penalty was also degrading jolnar (clearance fell
  73% → 68% under it) instead of helping it clear.
- **P-E run LAUNCHED** from u5100 (`out/clearing_run2_u300.json`), +500 updates → u5600, boundaries
  at u5200/5300/5400/5500/5600: `--entropy 0.05 --high-vp-bonus 1.0 --clearance-weight 1.0
  --rollout-depth 4` plus the two veto-disabling flags above; real aggregate/sigma gate otherwise,
  `--every 100`. Bonus escalated 0.5 → 1.0 per the pre-planned rule (mean VP plateaued ~2.0–2.2
  under 0.5 for ~1100 updates); clearance-weight relaxed 2.0 → 1.0 since it no longer gates and was
  distorting jolnar. Log `out/noveto_run.log`, output `out/noveto_run_u500.json`. Success signal:
  promotions resume (isolated or assembled) with candidate mean VP trending toward ≥3.0 for at
  least one faction; the target remains a fully-learned head clearing Stage 2 at ≥3.0 mean VP.
- **P-E results:** u5200 ALL SIX PROMOTED (assembled) — first promotion in the Stage-2 saga; net
  gain +1.79 ≫ 2σ. u5300–u5600 rejected at +0.08/+0.11/+0.16/+0.07, inside the noise band (sigma
  clause correctly held). New champion mean VP ≈ 1.98. Log `out/noveto_run.log`.
- **Per-faction own-merit gate implemented** (operator final decision: no cross-faction conflation;
  clearance counts as own merit): assembled promotion, aggregate margin, table-level sigma, and
  cross-faction vetoes all removed. Each head is judged on its own paired gain (>0.05 and >2σ of
  its own SE) + own clearance (within 0.03 of its own champion), validation AND confirmation
  panels; factions promote independently in a batch per boundary. `PanelEvaluation` now carries
  per-faction VP-by-seed; table-level pairing deleted as dead conflation machinery. Evidence:
  `plans/evidence/STAGE2-STALL-INVESTIGATION.md` §"P-E results + per-faction own-merit gate".
  Checks: example tests **14/14**, ti4-training lib **104/104** release, clippy no new warnings in
  touched files, fmt clean, `--eval-only` smoke on u5600 checkpoint behaves as designed.
- **Overnight run LAUNCHED** from u5600 (`out/noveto_run_u500.json`), +4000 updates → u9600,
  `--every 100 --panel-step 32`: `--entropy 0.05 --high-vp-bonus 1.0 --clearance-weight 1.0
  --rollout-depth 4`, per-faction gate at defaults. Log `out/overnight_u4000.log`, output
  `out/overnight_u4000.json`. Expected wall ≈ 3.5 h; checkpoints every boundary so nothing is lost
  if killed. Next action on resume: read the log's promotion lines, check champion mean VP trend
  toward ≥3.0, and decide bonus escalation (2.0) or per-faction reward targeting if flat again.
- **Overnight run COMPLETE** (u5600 → u9600, +4000 updates, 12973.5 s ≈ 3.6 h, 3.243 s/update,
  zero errors across ~445M decisions). Per-faction gate delivered **6 independent promotions**:
  u6000 xxcha, u6700 hacan, u7100 l1z1x, u9300 xxcha (+0.172 val / +0.188 conf), u9500 hacan
  (+0.151 / +0.182), u9600 jolnar (+0.208 / +0.349). Champion mean VP rose ~1.98 → **2.35**;
  final champion panel (pre-u9600 measurement): hacan 2.49, xxcha 2.48, sol 2.44, l1z1x 2.28,
  letnev 2.23, jolnar 2.17; mean clearance 0.85 (jolnar 0.73 is the weak spot — its own clearance
  clause will keep gating it). No faction at ≥3.0 yet; best hacan 2.49. Checkpoint
  `out/overnight_u4000.json`. Next decision: continue as-is vs escalate `--high-vp-bonus` 1.0 →
  2.0 for stronger directional pressure toward the ≥3-VP target.
- **Run with --every 500 launched then killed at u11100** (operator parameter change mid-run):
  from u9600 (`out/overnight_u4000.json`), +4000 → u13600 planned, same flags as overnight. First
  three boundaries (u10100/u10600/u11100) all rejected; binding constraint shifted to clearance
  regression — l1z1x at u10600 had +0.203 own gain (>2σ=0.184, genuine improvement) but was held
  by its own clearance clause (0.828 vs 0.859). Candidates systematically trading opening
  reliability for VP under bonus-1.0 pressure. Checkpoint `out/run_e500_u4000.json`.
- **Two-path own-merit gate implemented** (operator clarification: a large enough CLEARANCE gain
  should accept bounded VP regression — clearance is merit, not just a guard). Each faction now has
  two promotion paths, both requiring validation AND confirmation panels to pass: **VP merit** — own
  paired VP gain > `--accept-vp-margin` (0.05) and > `--accept-sigmas` × its own SE, with own
  clearance within `--max-faction-clearance-regression` of its champion's; or **clearance merit** —
  own paired clearance gain ≥ new `--clearance-gain-bar` (default 0.03) and > σ × its own SE, with
  own VP regression bounded by `--max-faction-vp-regression` (0.15). The passing path is recorded per
  faction in history (`promotion_paths`). Per-seed clearance pairing added
  (`PanelEvaluation.faction_clearance_by_seed`, `GainEvidence::paired_faction_clearance`). Tests:
  14/14 example (7 two-path tests incl. "a large clearance gain accepts bounded VP regression"),
  lib 104/104 release, clippy/fmt clean; end-to-end smoke on u11100 shows both paths naming their
  clauses correctly (xxcha's +0.0365 clearance gain would pass at 1σ with its −0.125 VP regression
  inside the bound — exactly the intended behavior).
- **New run LAUNCHED** from u11100 (`out/run_e500_u4000.json`), +4000 updates → u15100, boundaries
  every 500: `--accept-sigmas 1.0 --clearance-weight 5.0 --entropy 0.05 --high-vp-bonus 1.0
  --rollout-depth 4 --panel-step 32 --every 500` (operator directive: "reduce to 1 sigma, clearance
  should be heavily rewarded and weighted for promotion until like 99%"). Output
  `out/run_2path_u4000.json`, log `out/run_2path.log`.
- **Two-path run COMPLETE** (u11100 → u15100, +4000 updates, 12049.0 s ≈ 3.3 h, 3.012 s/update,
  zero errors). Five independent promotions across five factions — only letnev never promoted:
  u12100 jolnar (**clearance merit**: +0.104 clr ≥ bar at 1σ, VP −0.031 in bound) and l1z1x (VP
  merit +0.146, own clearance also up); u12600 xxcha (VP merit +0.083, clearance up 0.839→0.896);
  u13600 sol (**clearance merit**: +0.037 clr at 1σ, VP −0.042 in bound); u14600 hacan (**clearance
  merit**: +0.047 clr at 1σ with VP also +0.042). Three of five promotions used the new clearance-merit
  path — exactly the operator's intended behavior. Rejected boundaries (u11600/u13100/u14100/u15100)
  were inside-noise candidates with named per-clause reasons. Final champion panel (u15100 fresh
  panel, 192 games): hacan 2.56/0.880, jolnar 2.36/0.823, l1z1x 2.33/0.880, letnev 2.19/0.885,
  sol 2.26/0.953, xxcha 2.34/0.901 — mean VP ≈ 2.34 on that panel (panel-to-panel variance ±~0.1;
  the ratchet evidence is the paired per-boundary gains), mean clearance 0.887 (up from 0.85 at
  u9600). Checkpoint `out/run_2path_u4000.json` (run_complete: true). Next decision: continue with
  another +4000 block under the same regime, or adjust parameters (e.g. clearance-gain-bar, bonus)
  per operator direction.
- **`--no-boundaries` pure-learning mode added** (commit d30a20b): no boundary evaluation, no gate,
  no promotion — only training and per-block telemetry; champion frozen for the whole run. Telemetry
  and per-block checkpointing still run every block so `--every N` gives intermediate reports without
  any measurement or banking. Default off; existing runs unchanged. Smoke-verified (20 updates / two
  blocks, zero boundary lines).
- **Pure-learning experiment COMPLETE (killed by operator at u19100 of a planned +5000 from u15100):**
  four full 1000-update blocks ran with zero boundaries/promotions — learning dynamics healthy and
  steady-state throughout (movement ~41–57, mean-return-sd 2.59–3.04, ~106–109M decisions/block,
  zero errors; block times drifting up slowly 43.6→49 min as games deepen). Decisive measurement via
  extended `vp_ceiling_probe` (new: `--table profiles|accepted`, per-game min/max + clearance columns)
  on identical 192 games for both tables at u19100: **the unbanked learner had drifted BELOW its own
  frozen champion on 5 of 6 factions** — sol +0.16 VP (only gainer), letnev −0.09 VP with clearance
  collapsing 0.802→0.615, jolnar −0.19 VP, xxcha/hacan/l1z1x −0.08 to −0.11; all deltas far beyond
  paired SE (~0.06–0.08). Conclusion recorded plainly: unbanked policy gradient wanders off the local
  optimum it already found — the boundary/ratchet is not overhead, it is what converts exploration
  into retained progress. The experiment demonstrated both halves: healthy dynamics AND drift when
  unbanked. Checkpoint `out/run_pure_u5000.json` at u19100 (run_complete false; partial block 5
  discarded by design). Champion state unchanged from u15100 (`out/safepoint_u15100_pre_nobound.json`).

---

### MLP policy branch — revision-5 plan review (2026-08-21)

- **Branch/HEAD before this review:** `codex/mlp-policy`, `851f8ad`.
- **Design note:** [`../docs/MLP_PLAN.md`](../docs/MLP_PLAN.md), revision 5. Section 11.2 is the
  dependency/permission/review map; exact package specifications are still required before each
  package starts.
- **Accepted compatibility policy:** by explicit operator decision on 2026-08-21, Python behavioral
  parity is no longer an acceptance criterion. `AGENTS.md` now treats the pinned Python repository
  as a read-only historical reference. Its pre-existing untracked
  `docs/POLICY_GRADIENT_HANDOVER.md` is recorded but does not block Rust implementation; no command
  may mutate or clean it.
- **Milestones reopened:** M06 (rules), M07/M08 (downstream reaffirmation), M09 (learned policy),
  and M10 (training). Each has a new exit review after its added packages.

**M06-021 status: implemented, reviewed, NOT accepted.** The independent tier C review is recorded
in [`evidence/M06-021.md`](evidence/M06-021.md). Official FFG Living Rules Reference 2.0 rule 61.7
permits scoring in both space and ground combat in one tactical action; the merged turn-scoped
window can offer only once at `advance_turn`. Become a Martyr is also represented as a later board
position rather than a control-loss occurrence. M06-021a must resolve both findings and pass a
fresh tier C review. The existing 1274-pass workspace result and 150-game report remain historical
evidence, not acceptance.

**Next ready package after this docs-only review is committed:** M06-021a1 — occurrence model and
event-scoring semantics, the first child of the event-scoped secret timing correction. It is P1,
tier C, and must receive an exact task specification before code. M06-021a2 completes emitter
wiring and the parent review; M06-022 and all later work remain dependency-blocked behind it. After
M06-024, M07-019/020 and
M08-018/019 formally reaffirm downstream gates before M09 begins.

**Plan-review decisions now closed:** canonical option-free critic features; shared readout plus
zero-init faction residuals; post-feature deterministic vocabulary with fixed physical capacity;
schema-6 inference vs training-resume split; exact payment-planner progress; CPU-authoritative
rollouts and optional CUDA optimizer only; fixed distillation/DAgger seed clusters; validation/final
data separation; bounded durable baseline fixtures; three independently trained ablations; and
reopened M06–M10 exit reviews. The final consistency pass also made milestone dependencies explicit,
added the actor/critic vectors and returns required of the teacher corpus, forbade intermediate
promotion/early stopping in optimizer pilots, fixed the promotion variance audit as diagnostic-only,
and superseded M00's former behavioral-oracle wording.

## M06-021a1 implementation checkpoint (2026-08-21)

- **Active milestone/package:** M06 / M06-021a1 — occurrence model and event-scoring semantics.
  The parent M06-021a was split before code because its model/scoring and emitter/pause-point work
  exceed the atomic-package limit. Emitter wiring was then split again: M06-021a2a owns tactical
  combat pauses, followed by M06-021a2b for the remaining emitters and parent review.
- **Branch/HEAD:** `wp/m06-021a-event-scoped-secret-timing` at `92edea4` before this package's
  uncommitted scoped implementation.
- **Implemented and verified:** `FeatOccurrence`, deterministic event-feat recording and exact
  occurrence matching; `EventScope`; combat one-score cap; action/agenda sequential scoring; and
  decline closure. The parent is deliberately still unresolved: no production emitter allocates an
  occurrence yet, so no actual game timing is claimed fixed.
- **Checks:** five focused tests passed; `cargo test -p ti4-model` (73), `cargo test -p
  ti4-engine` (806 plus 5 doctests), and `cargo test --workspace` (exit 0; the 30.2-second tool
  call also ran the scoped formatter) passed.
  `ti4-model` Clippy passed under `-D warnings`; engine Clippy completed with only the documented
  pre-existing warnings. `git diff --check` passed. Evidence:
  [`evidence/M06-021a1.md`](evidence/M06-021a1.md).
- **Review/commit status:** no independent Tier-C review has occurred and no package commit has
  been made. The M06-021a2 parent integration review must cover this child and resolve all findings
  before acceptance.
- **Intentional working-tree paths:** `crates/ti4-model/src/state.rs`,
  `crates/ti4-engine/src/secrets.rs`, `crates/ti4-engine/src/objectives.rs`,
  `plans/M06-021a_EVENT_SCOPED_SECRET_TIMING.md`, `plans/evidence/M06-021a1.md`,
  `plans/EXECUTION_STATE.md`, `plans/M06_GENERAL_RULES.md`, and `docs/MLP_PLAN.md`.
- **Next exact action:** implement only M06-021a2a's tactical event pauses. M06-021a2b owns the
  remaining emitters and the parent Tier-C review. Investigation recorded in
  [`evidence/M06-021a2a.md`](evidence/M06-021a2a.md): the pause must be an atomic
  cross-window state-machine change, not an isolated feat-emitter edit.

**Historical docs-only plan-review working tree before commit `92edea4`:** `AGENTS.md`,
`docs/MLP_PLAN.md`, `plans/MASTER_PLAN.md`, `plans/INDEX.md`, `plans/SCOPED_PERMISSIONS.md`,
`plans/EXECUTION_STATE.md`, `plans/M00_ORACLE_AND_BASELINE.md`,
`plans/PI_WORK_PACKAGE_STANDARD.md`,
`plans/M06_GENERAL_RULES.md`, `plans/M07_FACTIONS_AND_TE.md`, `plans/M08_AUTHORED_BOTS.md`,
`plans/M09_LEARNED_POLICY.md`, `plans/M10_SIMULATION_AND_TRAINING.md`,
`plans/M12_QUALIFICATION.md`, `plans/M13_CUTOVER.md`, `plans/evidence/M06-021.md`, and
`plans/evidence/MLP-ARTIFACTS.md`. No source code, generated artifact, or historical Python
repository path was modified. Validation passed: Markdown table and local-link scan; stale-term and
dependency-reference scan; 31-row package-map reconciliation; `git diff --check`; and all four
baseline artifact hashes/sizes. The read-only historical repository remains at
`37061c511a4780d4c0719e0342533a498cd4b457` on `codex/fully-learned-policy`, with only its pre-existing
untracked `docs/POLICY_GRADIENT_HANDOVER.md`. Cargo tests were not run because the changes are
documentation-only.

Baseline hashes remain in [`evidence/MLP-ARTIFACTS.md`](evidence/MLP-ARTIFACTS.md). The two
non-reproducible checkpoints are not durable yet; M09-020 is a hard prerequisite for corpus capture.

## M06-021a2 integration checkpoint (2026-08-21)

- **Active milestone/package:** M06 / M06-021a2b accepted. Occurrence semantics, tactical pauses,
  and all remaining production emitters are implemented. The independent parent Tier-C review is
  complete; F7-F10 are resolved and the full gates pass.
- **Branch/HEAD:** `wp/m06-021a-event-scoped-secret-timing` at base `92edea4`, with the complete
  a1/a2a/a2b integration intentionally uncommitted until review disposition.
- **Implemented:** persistent one-score-per-combat occurrence; exact space-cannon, barrage,
  space-combat, bombardment, per-planet ground-combat, home-control-loss, last-pass, and per-agenda
  occurrences; sequential unlimited non-combat scoring; exact election attribution; failed-secret
  award guard; removal of the legacy turn feat/event path; resumable stepped and synchronous combat
  and invasion drivers.
- **Checks:** focused timing regressions pass; `ti4-model` 73; `ti4-engine` 819 plus 5 doctests;
  `cargo test --workspace --quiet` exit 0; strict model Clippy passes; engine Clippy passes after
  fixing its only new warning, retaining only documented pre-existing warnings; `git diff --check`
  passes with the existing objectives.rs CRLF advisory.
- **Post-gate audit fixes:** space-cannon scoring now precedes combat opening; agenda scoring now
  precedes the next reveal; direct combat/invasion wrappers resume across internal pauses; event
  helpers require concrete occurrences; invalid occurrence scoring is proven atomic; last-pass
  occurrence replay is deterministic.
- **Review findings:** the reviewer independently verified F1-F6 and raised F7-F10. F7 now binds
  Become a Martyr only to its home-loss occurrence; F8 shares rival-home semantics across combat
  types; F9 shares one tactical-start note snapshot; F10 extracts the ground-round resolver. All
  fixes pass focused and workspace gates. F11 is informational and non-blocking.
- **Intentional paths:** `crates/ti4-model/src/state.rs`, `crates/ti4-engine/src/{combat,game,
  invasion,objectives,secrets}.rs`, `docs/MLP_PLAN.md`, `plans/M06_GENERAL_RULES.md`, the M06-021a
  package specifications/review ledger/evidence files, and this execution state.
- **Next exact action:** commit the accepted correction, then begin M06-022 from its exact package
  specification.
- **Safe follow-on preparation:** `M06-022_COUNTING_OBJECTIVE_PROGRESS.md` now contains the exact
  34-alias family/threshold API, scope, invariants, and acceptance-test plan. Its M06-021a
  dependency is now accepted. No M06-022 source was changed yet.
- `M06-023_BESPOKE_AND_BOUGHT_PROGRESS.md` is likewise prepared without source changes. It fixes
  the six distinct-count definitions and the greatest-exactly-affordable `k` contract for all ten
  costs, including disjoint `AllThree` payment properties; it remains blocked behind accepted
  M06-022.
- **Current next action:** commit the accepted M06-021a correction, then create the M06-022 package
  branch and implement its exact counting-family progress specification.

## M06-022 implementation checkpoint (2026-08-21)

- **Active milestone/package:** M06 / M06-022 — counting-family objective progress.
- **Branch/base:** `wp/m06-022-counting-objective-progress` / accepted M06-021a commit `5d027e8`.
- **Status:** accepted after independent Claude Opus 5 Tier-B review. All mapping, legality,
  determinism, purity, and behavior-preservation checks passed; G1-G3 are resolved.
- **Implemented:** typed stable family identity and exact raw `have`/`threshold` progress for all
  24 public and 10 secret aliases; affected legality derives from that same progress; outer-rim
  counts are unavailable without a map; exact maximum/distinct reductions and immutable queries.
- **Checks:** engine 822 unit tests plus 5 doctests; workspace exit 0; engine Clippy has no new
  warning after fixing the package-local boolean simplification; `git diff --check` passes.
- **Intentional paths:** `crates/ti4-engine/src/objectives.rs`, `secrets.rs`, M06-022 spec/evidence/
  review ledger, and this execution state. M06-023 and M06-024 specs remain uncommitted preparation.
- **Next exact action:** commit M06-022, then create the M06-023 package branch and implement its
  expanded 33-alias remaining-position/exact-bought-cost specification.
- **Dependency-safe preparation completed while review is pending:** M06-023 now names all ten
  bought-objective aliases/cost families/targets explicitly; M06-024 has an exact frontier-review
  specification; M07-019/M07-020 and M08-018/M08-019 now have scoped revalidation/review
  specifications. None of their source implementation has started or bypassed its dependency.

## M06-023 implementation checkpoint (2026-08-21)

- **Active milestone/package:** M06 / M06-023 — remaining position and exact bought-cost objective
  progress.
- **Branch/base:** `wp/m06-023-remaining-objective-progress` / accepted M06-022 commit `d58622c`.
- **Status:** accepted after independent Claude Opus 5 Tier-C review. H1's production-format
  promissory-note lookup is fixed through `promissory::alias_of`, its test uses a real note key, and
  H2's misleading helper name is corrected. The package is ready to commit.
- **Implemented:** exact typed progress for six public and seventeen secret position families;
  greatest exactly-affordable scaled progress for all ten bought objectives; legality derived from
  or proven equivalent to the same predicate/payment path; unavailable map state preserved.
- **Checks:** focused mappings/unavailable cases pass; bounded exhaustive affordability campaign
  covers 1,792 states x ten costs plus all 64 small token-pool splits; engine 832 plus 5 doctests;
  full workspace exit 0; engine Clippy
  has no package-local warning; `git diff --check` passes.
- **Intentional active-package paths:** `crates/ti4-engine/src/objectives.rs`, `secrets.rs`,
  `plans/M06-023_BESPOKE_AND_BOUGHT_PROGRESS.md`, `plans/evidence/M06-023.md`,
  `plans/M06-023_OPEN_REVIEW_ITEMS.md`, and this execution state.
- **Preserved dependency-safe preparation:** M06-024, M07-019/020, and M08-018/019 specifications
  remain untracked and uncommitted; they contain no source implementation and do not bypass gates.
  Their path audit corrected stale nonexistent `ti4-policy/src/authored.rs` references to `bot.rs`,
  tightened review packages to read-only source access before a documented finding, and fixed the
  future M06 review base to exact commit `92edea4` with accepted additions `5d027e8`/`d58622c`.
- **Next exact action:** rerun the final workspace/Clippy gates, commit only M06-023 scoped paths,
  create the M06-024 review branch, and begin its exact committed-frontier campaign. The related
  pre-existing Betray-a-Friend note-owner emitter defect is a named blocking M06-024 input.

## M06-024 reopened-frontier review checkpoint (2026-08-21)

- **Active milestone/package:** M06 / M06-024 — reopened M06 exit review over `92edea4..bfcdb73`.
- **Branch/base:** `wp/m06-024-reopened-frontier-review` from accepted M06-023 commit `bfcdb73`.
- **Status:** implementation work complete; **independent frontier adjudication pending** (Tier-C
  requires a reviewer distinct from any implementer of the reviewed code). Not accepted, not
  committed.
- **F1 (carried blocking finding): resolved.** `combat.rs::note_holdings` built note issuers with
  `PlayerId::new(faction_name)`, so faction-note Betray-a-Friend issuers could never match; both
  space and ground emitters share that snapshot. Fix resolves through
  `promissory::owner_of` + `promissory::seat_of`; unseated owners yield no issuer. Four regression
  tests (two unit, one per combat path) verified red-before/green-after. Defect-class sweep found
  no other occurrence (`laws.rs::repeal` stores player ids — correct).
- **F2 (new blocking finding): escalated to M06-025.** baf and sb print a play-area restriction the
  engine does not enforce; face-up model hard-codes `an`/`convoys` while the accepted corpus marks
  eleven notes `playArea: true` (eight faction notes uncovered, incl. Terraform). Spec written at
  `plans/M06-025_PLAY_AREA_NOTE_SCORING.md`. **Blocks the M06 exit gate until accepted.**
- **Checks:** nine named focused tests from M06-021a/022/023 pass; exhaustive payment campaigns
  (`bought_progress_is_maximal_across_bounded_small_states`, `token_progress_is_exact_across_all_
  small_pool_splits`) pass; engine 835 + 5 doctests; workspace 18 suites / 1,308 passed / 0 failed,
  deterministic across two runs (timing stripped); ti4-model Clippy clean; engine Clippy shows only
  the three documented pre-existing warnings (one package-introduced redundant-closure warning was
  fixed); both touched source files rustfmt-clean under edition 2024 with hunks confined to
  `note_holdings` and test modules; workspace-wide `cargo fmt --all --check` drift proven
  pre-existing at base `bfcdb73`; `git diff --check` clean.
- **Intentional active-package paths:** `crates/ti4-engine/src/combat.rs`,
  `crates/ti4-engine/src/invasion.rs`, `plans/M06-024_REOPENED_FRONTIER_REVIEW.md`,
  `plans/M06-024_OPEN_REVIEW_ITEMS.md`, `plans/evidence/M06-024.md`, and this execution state.
- **Preserved dependency-safe preparation:** M06-025 spec (new), M07-019/020, M08-018/019 specs
  remain untracked; no source implementation started for any of them.
- **Next exact action:** obtain the independent frontier review of `plans/M06-024_OPEN_REVIEW_ITEMS.md`
  (recheck F1 fix + four regression tests, confirm F2 escalation is correctly scoped). If accepted:
  commit only M06-024 scoped paths on this branch, then start M06-025 from the accepted head. The
  M06 exit gate stays closed until M06-025 is also accepted.

## M06-025 play-area note scoring checkpoint (2026-08-21)

- **Active milestone/package:** M06 / M06-025 — play-area note scoring for baf and sb (F2 fix).
- **Branch/base:** `wp/m06-025-play-area-note-scoring` from accepted head `bfcdb73`; the verified
  M06-024 F1 fix is carried in the working tree and commits first at closure.
- **Status:** **accepted** by independent Tier-C review (Claude Opus 5,
  `plans/M06-025_OPEN_REVIEW_ITEMS.md`; reviewer implemented none of the code under review).
  Findings: L1 recorded in evidence with a standing re-check condition for any future roster
  widening; L2 recorded in M06-023 evidence (the 91% sb figure was counting hand-held notes);
  L3 resolved by comment at `combat.rs::note_holdings`. Committed together with M06-024 — see
  closure record below.
- **M06-024 adjudication landed the same day** (Claude Opus 5, recorded in
  `plans/M06-024_OPEN_REVIEW_ITEMS.md`): F1 accept; F2 confirmed and correctly escalated to this
  package; M06-024 not acceptable until M06-025 lands plus J1's instrumentation run.
- **J1 (adjudicator's required action) resolved.** New probe
  `crates/ti4-training/examples/feat_activation_probe.rs` (declared writable in the M06-024 ledger)
  ran one 150-game panel (25 seeds × 6 rotations, r6 champions at
  `out/stage2_r6/final10000.json`, holdout map pool): BarrageTookTheLastFighters recorded **21**
  times / fwp scored **0**; WonAgainstANoteHolder recorded **313** / baf scored **11** (same count
  the adjudicator measured independently — baf is live end-to-end); LostAHomePlanet recorded **48**
  / bam scored **0**. Zero feat+card co-occurrence at game end for fwp/bam is statistically
  consistent with rare alignment (expected overlap ≈1.4 and ≈1.8; P(0)≈23%/15%); the full scoring
  loops are proven by unit tests (`game.rs::barrage_scoring_pauses_combat_and_caps_the_whole_
combat_occurrence`, secrets/invasion occurrence tests). None of the three mechanisms is
  unreachable. Residual mid-game-hand uncertainty recorded with its closing method; supersedes and
  closes F11 from the M06-021a ledger. Mean VP per seat on this post-M06-025 run: **2.958**
  (adjudicator's pre-M06-025 J2 baseline was 2.935).
- **Implementation:** content-driven face-up model `promissory::is_play_area` over the corpus
  `playArea` field (generic `<color>` records apply under every owner; faction records bind to
  their owner); baf filter in `combat.rs::note_holdings`; sb filter in
  `secrets.rs::rival_note_issuers_count` plus K1 fix (issuer from the note key via `owner_of`,
  which the old record-by-alias lookup missed for every generic note). Scope extension declared:
  `transactions.rs` signature threading only. Four new tests; two existing tests rebuilt on
  production-valid keys.
- **Red-first:** with only the two filter lines removed, both decision-boundary tests fail
  (`note_holdings_resolves_production_note_keys_to_seated_issuers`,
  `strengthening_bonds_counts_only_play_area_notes`); restored → green. The combat test was
  strengthened mid-verification because its first draft was insensitive to the mutant (BTreeSet
  dedup hid a same-issuer hand-held note).
- **Checks:** eight focused tests pass individually; engine **839 + 5 doctests**; workspace
  **18 suites / 1,312 passed / 0 failed**, deterministic across two runs (timing stripped);
  Clippy: zero warnings in any touched file (only the three documented pre-existing engine
  warnings elsewhere); all six touched Rust files rustfmt-clean under edition 2024; `git diff
  --check` clean. Probe example clippy/fmt clean, output deterministic across runs.
- **Intentional active-package paths:** `crates/ti4-engine/src/{promissory,combat,secrets,
  invasion,transactions}.rs`, `crates/ti4-training/examples/feat_activation_probe.rs` (J1,
  belongs to M06-024 acceptance), `plans/M06-025_PLAY_AREA_NOTE_SCORING.md`,
  `plans/evidence/M06-025.md`, plus the M06-024 paths above and this execution state.
- **Behavior change recorded:** baf/sb now count play-area notes only; downstream VP/clearance
  numbers non-comparable until re-baselined (see `plans/evidence/M06-025.md`).
- **M06 closure (same day):** with M06-025 accepted, F2 clears. M06-024's adjudication conditions
  are all met (F1 accept; J1 run recorded; independence limitation recorded), so M06-024 is
  acceptable too (`plans/M06-024_OPEN_REVIEW_ITEMS.md` §Final acceptance). Both packages commit as
  one closure commit on this branch — a per-package split was impossible: both fixes live in the
  same if-chain in `combat.rs::note_holdings`, and M06-024's rebuilt invasion test depends on
  M06-025's `take()` signature, so no intermediate state compiles. The commit message attributes
  each package explicitly.
- **M06 exit gate: CLOSED.** Closure record in `plans/M06_GENERAL_RULES.md` (exit gate section):
  final verification engine 839 + 5 doctests; workspace 18 suites / 1,312 passed / 0 failed,
  deterministic across two runs; exhaustive payment campaigns pass; Clippy clean on all touched
  files; rustfmt and `git diff --check` clean. Independence limitation carried into the closure
  record per the adjudicator's instruction. Known-difference ledger: baf/sb play-area semantics
  change downstream VP/clearance comparability (re-baseline before citing old numbers); eight
  faction play-area notes untestable under D11's roster until a future package widens it.
- **Next exact action:** M07-019 — post-M06 faction/TE integration revalidation (spec prepared at
  `plans/M07-019_POST_M06_REVALIDATION.md`; dependencies met: M06-024 accepted, M07-018 part of
  the accepted M07 baseline). Create its package branch from this closure commit and follow the
  standard package loop.

## Checkpoint — M07-019 accepted, corrections applied, commit pending (2026-08-22)

- **Active milestone/package:** M07 / M07-019 (post-M06 faction/TE integration revalidation).
- **Branch and HEAD:** `wp/m07-019-post-m06-revalidation` from closure commit `b721a9a`; no package
  commit yet — independent Tier B review is in, all corrections applied; the scoped commit follows.
- **Working tree (active-package paths):** `crates/ti4-engine/src/game.rs` (+588/−0, test module
  only), `plans/evidence/M07-019.md` (new), `plans/M07-019_POST_M06_REVALIDATION.md` (spec,
  untracked→accepted status), `plans/M07-019_OPEN_REVIEW_ITEMS.md` (review + resolution, new),
  `plans/M07_FACTIONS_AND_TE.md` (M07-021 row added to the work-package table), and this file.
  Pre-existing unrelated changes preserved untouched: `AGENTS.md`, `plans/PI_WORK_PACKAGE_STANDARD.md`,
  `plans/M06-025_OPEN_REVIEW_ITEMS.md` (operator-directed toolchain note, pre-dating this package)
  and the untracked M07-020 / M08-018 / M08-019 prep specs.
- **What was done:** four nested-window regression tests in `game.rs` pinning faction/TE effects
  across M06 event-scoped scoring pauses: (1) Munitions Reserves marker survives the fwp barrage
  pause, round-2 offer declined rather than inherited; (2) home-loss pause holds the invasion at
  FinalizingControl — occurrences in Game-level order, control transferred pre-pause, exactly-once
  capture (conversion assertions documented as vacuous until F-M07-019-1 is fixed); (3) Flank Speed
  expires by activation-sequence identity across the pause; (4) TE breakthrough + slice + Gravleash
  anchor survive the combat scoring pause. No demonstrated M06 regression required a source fix.
- **Independent Tier B review (Claude Opus 5, `plans/M07-019_OPEN_REVIEW_ITEMS.md`): ACCEPT** with
  one required correction (M1) — all applied and recorded in its Resolution section: M1(a) evidence
  corrected (conversion assertions vacuous under current behavior), M1(b) test renamed to
  `the_home_loss_pause_holds_the_invasion_at_finalizing_control`, M1(c) Assimilate-after-pause
  coverage added as a required deliverable of the F-M07-019-1 fix package. Reviewer independently
  mutation-confirmed tests 1 and 3 as load-bearing across the pause; confirmed both findings.
- **Findings (recorded, not fixed):** F-M07-019-1 structures count as ground defenders in
  `invasion.rs::defender_on` (official LRR: planet falls without resistance absent enemy ground
  forces; pre-dates M06; changes invasion legality/timing semantics → frontier adjudication required
  before any fix package). F-M07-019-2 phantom round rolls after a total-wipe fwp pause (dice-stream
  position only — reviewer confirmed round identity/events/ability offers cannot diverge because they
  live inside the `if run_barrage` block; minor, known-difference ledger entry warranted). F-M07-019-3
  (review M2) `Player.event_feats` missing from state equality → **scoped as child package M07-021**
  (prep spec written; milestone-plan row added with hard ordering: must complete before M07-020).
- **Tests last run and exact results:** four tests pass individually under final names; engine
  **843 + 5 doctests**; workspace **1,316 / 0** (post-correction run; the pre-correction determinism
  pair stands for the unchanged test bodies); replay suite 4/4 and registry suite 8/8 at first pass.
- **Checks:** Clippy zero warnings in added code (two new too-many-lines sites from the reviewer's
  first pass resolved with targeted `#[allow]` + reason per M3; only the three documented pre-
  existing crate-wide warnings remain); game.rs rustfmt-clean under edition 2024; `git diff --check`
  clean.
- **Decisions:** test-only package — findings escalated rather than fixed because their fix paths
  (`invasion.rs`, `combat.rs`, `ti4-model`) are outside this package's writable declarations;
  M3 resolved with local lint allows (codebase precedent) rather than splitting tests; M2 scoped as
  child rather than deferred silently. Redaction invariant holds by construction (M4 informational:
  no leak today, nothing pins the judgement — recorded).
- **Open review findings or blockers:** none open from Tier B. F-M07-019-1 awaits frontier
  adjudication before its fix package is scoped; M07-021 is ready to schedule.
- **Next exact action:** commit the scoped paths on `wp/m07-019-post-m06-revalidation` (game.rs,
  evidence, spec, review items + resolution, milestone-plan row, this file); then M07-021
  (`event_feats` projection) before M07-020's exit review.

## Checkpoint — M07-021 accepted, review resolutions applied, commit pending (2026-08-22)

- **Active milestone/package:** M07 / M07-021 (`event_feats` state-equality projection; child of
  M07-019 review finding M2 / F-M07-019-3).
- **Branch and HEAD:** `wp/m07-021-event-feats-projection` from `c034549`; no package commit yet —
  independent Tier B review is in (accept), resolutions applied; the scoped commit follows.
- **Working tree (active-package paths):** `crates/ti4-model/src/state.rs` (+4 compared-field line
  with rationale, +1 focused test), `crates/ti4-engine/src/combat.rs` (test module only: completed
  the stepped harness of `a_stepped_combat_matches_the_driven_one`, +17/−1),
  `plans/evidence/M07-021.md` (new, incl. N1 coverage limit + N2 disposition),
  `plans/M07-021_EVENT_FEATS_PROJECTION.md` (accepted status + scope-extension declaration),
  `plans/M07-021_OPEN_REVIEW_ITEMS.md` (review + resolution, new),
  `plans/M07_FACTIONS_AND_TE.md` (M07-022 row; M07-020 now depends on 019/021/022),
  `plans/M07-022_STEPPED_EQUIVALENCE_ACROSS_PAUSES.md` (new prep spec, commits with its parent per
  the M06-025 precedent), and this file. Pre-existing unrelated changes preserved untouched:
  `AGENTS.md`, `plans/PI_WORK_PACKAGE_STANDARD.md`, `plans/M06-025_OPEN_REVIEW_ITEMS.md` and the
  untracked M07-020 / M08-018 / M08-019 prep specs.
- **What was done:** Option A implemented — `Player::PartialEq` now compares `event_feats`, closing
  the projection gap where two states differing only in feat evidence compared equal. Red-first:
  the focused test failed before the one-line fix (states compared equal) and passes after.
- **Exposed dependence diagnosed, not fixture-regenerated:** the change turned
  `a_stepped_combat_matches_the_driven_one` red because its stepped harness omitted the post-combat
  feat bookkeeping both real drivers perform (`before_combat` snapshot +
  `note_combat_event_feats` at completion — game.rs:279–286, combat.rs:1489–1495); in that fixture
  the driven side recorded `HeldThreeShipsAfterASpaceCombat`. The harness now mirrors both drivers;
  no assertion changed. This is exactly the detection-latency scenario M2 predicted.
- **Scope extension declared:** `crates/ti4-engine/src/combat.rs` (test module only) was outside
  the original writable declaration; required because the failing harness lives there and no
  in-scope path can make the comparison faithful otherwise. Declared in the spec per the M06-025
  precedent; reviewer to adjudicate.
- **Tests last run and exact results:** focused test red-before/green-after; model **74 / 0** (was
  73, +1 new test); engine **843 / 0** (unchanged count); workspace **1,317 / 0**, identical across
  two runs (deterministic).
- **Checks:** Clippy zero warnings in ti4-model and no new warnings in combat.rs (only the three
  documented pre-existing engine warnings remain crate-wide); both touched Rust files rustfmt-clean
  under edition 2024; `git diff --check` clean.
- **Decisions:** Option A over B — no concrete contract breaks (full workspace green), and the
  field was never in the oracle-exclusion list nor marked `// Not compared.`. Behavior change is
  strictly stricter equality: equivalence/replay comparisons catch more, never fewer; no scoring,
  replay-hash projection, choice ID, or registry changed.
- **Independent Tier B review (Claude Opus 5): ACCEPT.** Scope extension into `combat.rs`
  approved as a genuine test-only completion (reviewer verified both production consumers already
  perform the bookkeeping; red-first and the exposed dependence reproduced independently).
- **Review resolutions applied:** N1 — coverage limit recorded in evidence (the invariant holds on
  `event_feats` for fights that do not pause; earlier wording retracted as overstatement) and
  follow-up scoped as **M07-022** before the exit review, now a dependency of M07-020. N2 —
  disposition recorded: helper factoring adopted into M07-022; Game-level re-pointing deferred to
  M07-020's scope decision (including the deliberate `before_combat_with_notes` vs `before_combat`
  snapshot difference).
- **Open review findings or blockers:** none open from Tier B. M07-022 is ready to schedule.
- **Next exact action:** commit the scoped paths on `wp/m07-021-event-feats-projection` (state.rs,
  combat.rs, evidence, both specs, review items + resolution, milestone-plan rows, this file); then
  M07-022 (stepped equivalence across pauses) before M07-020's exit review.
- **M07-021 committed as `5241f2d`** on `wp/m07-021-event-feats-projection` (8 files, +473/−10).

## Checkpoint — M07-022 accepted, review resolutions applied, commit pending (2026-08-22)

- **Active milestone/package:** M07 / M07-022 (stepped-vs-driven equivalence across scoring pauses;
  child of the M07-021 review finding N1; dependency of the M07-020 exit review).
- **Branch and HEAD:** `wp/m07-022-stepped-equivalence-across-pauses` from `5241f2d`; no package
  commit yet — independent Tier B review is in (accept), resolutions applied; the scoped commit
  follows.
- **Working tree (active-package paths):** `crates/ti4-engine/src/combat.rs` only (+1 new test with
  an explicit both-sides feat assertion; +1 production helper `complete_window` with a
  behavior-preserving refactor of `resolve()`'s tail; +1 shared stepped harness `stepped_fight`
  with pause consumption, replacing the two inline stepped branches; P1 backtick fix in the
  harness doc comment), `plans/evidence/M07-022.md` (new, incl. pasted Clippy output per P1,
  P2 coverage limit, P3 note), `plans/M07-022_STEPPED_EQUIVALENCE_ACROSS_PAUSES.md` (accepted
  status), `plans/M07-022_OPEN_REVIEW_ITEMS.md` (review + resolution, new),
  `plans/M07_FACTIONS_AND_TE.md` (M07-023 row; M07-020 now depends on 019/021/022/023),
  `plans/M07-023_POST_PAUSE_CHOICE_COMPOSITION.md` (new prep spec, commits with its parent per the
  M06-025 precedent), and this file. Pre-existing unrelated changes preserved untouched:
  `AGENTS.md`, `plans/PI_WORK_PACKAGE_STANDARD.md`,
  `plans/M06-025_OPEN_REVIEW_ITEMS.md` and the untracked M07-020 / M08-018 / M08-019 prep specs.
- **Red-first:** new test written first against the current (M07-021) harness shape — FAILED with a
  panic at `.expect("the fight resolved")` (combat.rs:2791), reproducing reviewer N1's probe
  exactly; green after pause consumption was added.
- **N2 factorization landed:** `complete_window` is now the single completion-bookkeeping path for
  both `resolve()` and the stepped harness — a third copy is structurally impossible. The Game
  driver keeps its own inline bookkeeping (a noted occurrence pauses there before the fight is
  over); documented in the helper's doc comment.
- **Verification:** engine **844 + 5 doctests** (+1 = new test; `resolve()` refactor changed no
  behavior); workspace **1,318 / 0 identical across two runs**; Clippy — no new warnings (only the
  two documented pre-existing engine warnings remain); combat.rs rustfmt-clean under edition 2024;
  `git diff --check` clean.
- **Independent Tier B review (Claude Opus 5): ACCEPT.** Reviewer reproduced the red-first claim
  exactly (probe: pause branch → `break` reproduces the panic; reverted), verified the
  `complete_window` refactor line-for-line behavior-preserving, and confirmed the chain closes:
  `GameState::identical` opens with `self == other`, so M07-021's `Player::PartialEq` addition does
  feed these tests' assertions.
- **Review resolutions applied:** P1 (required) — backticks added at the site; evidence now pastes
  the tool's actual Clippy output (three pre-existing warnings, zero new); post-fix re-verification
  recorded (engine 844 + 5 doctests; workspace 1,318 / 0). P2 — successor scoped as **M07-023**
  before the exit review (pause→choice composition: pausing fixture that continues into a casualty
  assignment), now a dependency of M07-020; evidence framing corrected to "across a scoring pause
  with no choice after it". P3 — recorded: sharing `complete_window` removed the last independent
  check on its content; green equivalence must not be cited as validating bookkeeping content
  (`a_driven_combat_continues_after_its_barrage_scoring_pause` does that).
- **Open review findings or blockers:** none open from Tier B. M07-023 is ready to schedule.
- **Next exact action:** commit the scoped paths on `wp/m07-022-stepped-equivalence-across-pauses`
  (combat.rs, evidence, both specs, review items + resolution, milestone-plan rows, this file);
  then M07-023 (pause→choice composition) before M07-020's exit review.
- **M07-022 committed as `7f357b6`** on `wp/m07-022-stepped-equivalence-across-pauses`
  (7 files, +568/−46).

## Checkpoint — M07-023 accepted, review resolutions applied, commit pending (2026-08-22)

- **Active milestone/package:** M07 / M07-023 (stepped equivalence across pause→choice resumption;
  child of the M07-022 review finding P2; dependency of the M07-020 exit review).
- **Branch and HEAD:** `wp/m07-023-post-pause-choice-composition` from `7f357b6`; no package commit
  yet — independent Tier B review is in (accept), resolutions applied; the scoped commit follows.
- **Working tree (active-package paths):** `crates/ti4-engine/src/combat.rs` (**test module only**,
  +104/−15: one new test `a_stepped_combat_matches_the_driven_one_across_a_pause_and_assignment`
  — pausing fixture that continues into a casualty-assignment choice at the retained frame, with
  log-based non-vacuity assertions; plus Q1/Q2 hardening in `stepped_fight`: it now returns
  `(CombatOutcome, Option<usize>)` measuring asks-before-first-pause and asserts its context table
  stayed unasked), `plans/evidence/M07-023.md` (new, incl. review resolutions + pasted Clippy
  output), `plans/M07-023_POST_PAUSE_CHOICE_COMPOSITION.md` (accepted status),
  `plans/M07-023_OPEN_REVIEW_ITEMS.md` (review + resolution, new), and this file. Pre-existing
  unrelated changes preserved untouched: `AGENTS.md`, `plans/PI_WORK_PACKAGE_STANDARD.md`,
  `plans/M06-025_OPEN_REVIEW_ITEMS.md` and the untracked M07-020 / M08-018 / M08-019 prep specs.
- **Non-vacuity probe (deliverable 2b):** with `stepped_fight`'s no-choice branch replaced by
  `break` (pre-M07-022 shape), the new test FAILED at `.expect("the fight resolved")`
  (combat.rs:2739) — proving the fixture really pauses; M07-022's pausing test failed under the
  same probe, and the non-pausing test stayed green. Probe reverted; all three re-run green.
- **Verification:** engine **845 + 5 doctests** (+1 = new test); workspace **1,319 / 0 identical
  across two runs**; Clippy — pasted output in evidence (three pre-existing warnings, zero new);
  combat.rs rustfmt-clean (no changes needed to this package's additions); `git diff --check`
  clean.
- **Independent Tier B review (Claude Opus 5): ACCEPT.** Reviewer measured the composition
  directly (instrumented branch order: PAUSE-BRANCH then CHOICE "assign a hit"), reproduced the
  non-vacuity probe exactly, and confirmed the Clippy claim is correct for the first time in this
  chain — pasting tool output worked.
- **Review resolutions applied (inside this package, per disposition — no M07-024 spawned):**
  Q1 — pause ordering now asserted: `stepped_fight` returns asks-before-first-pause and the test
  asserts `Some(0)` (fixture pauses; no choice may precede the barrage). Q2 — log-comparison
  integrity guarded: `stepped_fight` asserts its context table stayed unasked; the stale "neither
  is asserted on" comment corrected at the site.
- **Post-resolution re-verification:** all three equivalence tests ok; engine **845 + 5 doctests**
  (count unchanged); workspace **1,319 / 0 identical ×2**; Clippy — same pasted output as the
  review's own run (three pre-existing warnings, zero new); combat.rs rustfmt-clean;
  `git diff --check` clean.
- **Open review findings or blockers:** none open from Tier B. The M07-019→023 chain is closed per
  the reviewer's instruction; remaining harness-fidelity questions go to M07-020's known-limits
  ledger for one-time adjudication.
- **Next exact action:** commit the scoped paths on `wp/m07-023-post-pause-choice-composition`
  (combat.rs, evidence, spec, review items + resolution, this file); then M07-020 — all four
  dependencies (M07-019, M07-021, M07-022, M07-023) met, opening the milestone's reopened frontier
  exit review.

## Checkpoint — M07-020 campaign complete, independent Tier C frontier review pending (2026-08-22)

- **Active milestone/package:** M07 / M07-020 (reopened M07 frontier exit review; the milestone's
  final gate).
- **Branch and HEAD:** `wp/m07-020-reopened-frontier-review` from `8ba6edc`; no package commit yet —
  awaits independent Tier C frontier adjudication of the exact committed frontier.
- **Review frontier (exact):** `b721a9a..8ba6edc` = four commits: M07-019 (`c034549`), M07-021
  (`5241f2d`), M07-022 (`7f357b6`), M07-023 (`8ba6edc`). Committed diff under `crates/`: 3 files,
  +816/−24 — game.rs (+588 test module only), combat.rs (one behavior-preserving production hunk:
  the accepted `complete_window` refactor of `resolve()`'s tail; rest test module), state.rs
  (+19: one `event_feats` PartialEq line + focused test). No other production behavior changed.
- **Working tree (active-package paths):** `plans/evidence/M07-020.md` (new — full five-part
  campaign record with pasted gate outputs), `plans/M07-020_REOPENED_FRONTIER_REVIEW.md` (status →
  campaign complete, review pending; was untracked prep, to be committed with the package), and
  this file. Pre-existing
  unrelated changes preserved untouched: `AGENTS.md`, `plans/PI_WORK_PACKAGE_STANDARD.md`,
  `plans/M06-025_OPEN_REVIEW_ITEMS.md`; untracked M08-018 / M08-019 prep specs.
- **Campaign results (all in evidence):** six nested-window paths traced with code refs and
  pinning tests; marker expiry verified (identity checks against monotonic `combat_round_seq` /
  `activation_seq`; atomic set sites; decline/error set nothing); redaction boundary rechecked at
  the typed `Decider::choose_seeing` seam (guards-the-guard `leaks()` test intact); registries
  reconciled (registered ⊆ corpus pinned; unimplemented/unmapped reported, not ignored);
  occurrence-scoped 61.7 cap reaffirmed end-to-end against M06-021a's adjudication (occurrence-
  membership enforcement: `record_occurrence_score` + exclusion at offer time).
- **Gates reproduced:** focused M07 tests all green; engine **845 + 5 doctests**; workspace
  **1,319 / 0 identical ×2**; replay **4/4**; Clippy — zero new warnings in the five frontier
  files (only pre-existing `game.rs:1260 apply_tactical`); all five frontier files rustfmt-clean;
  `git diff --check` clean.
- **Findings:** F-M07-020-1 (informational) — `ground_roll_suppressed_round` /
  `sustained_damage_round` are inert reserved Seat fields with no read/write sites; recorded for
  registry completeness. F-M07-020-2 (informational, reaffirmed) — promissory-note redaction gap,
  documented/named/test-pinned since M08-001; carried to the milestone known-differences ledger.
  **No actionable findings; no source edits made; no child packages spawned.**
- **Open review findings or blockers:** none from the campaign. Independent Tier C frontier
  adjudication pending at `plans/M07-020_OPEN_REVIEW_ITEMS.md` (reviewer must be distinct from the
  M07 implementers).
- **Next exact action:** on acceptance, commit the scoped paths (evidence, spec, this file) and
  write the M07 exit-gate closure record in `plans/M07_FACTIONS_AND_TE.md`; then M08-018 may
  begin per the definition of done.

## Checkpoint — M07-020 accepted; M07 CLOSED (2026-08-22)

- **Active milestone/package:** M07 / M07-020 — **accepted and closed.** Milestone exit gate is
  closed with the full closure record in `plans/M07_FACTIONS_AND_TE.md`.
- **Branch and HEAD:** `wp/m07-020-reopened-frontier-review` from `8ba6edc`; package commit follows
  this checkpoint (documentation-only resolutions; no source file under `crates/` was touched).
- **Independent Tier C frontier review (Claude Opus 5): one blocking + three documentation
  findings, all resolved in-package.** Independence limitation recorded per the M06-024 precedent
  (reviewer independent of implementer but not a fresh perspective on this range — it formed the
  M07-019 findings that R1 concerns).
- **R1 (BLOCKING) — DECIDED option 2:** F-M07-019-1 (structures count as ground defenders against
  LRR 49; escalated by M07-019 to this gate and previously unanswered) accepted as known difference
  **KD-2**; fix scoped as **M08-020** (`plans/M08-020_GROUND_COMBAT_STRUCTURE_LEGALITY.md`), hard-
  ordered before M08-018 (milestone row added to `plans/M08_AUTHORED_BOTS.md`; M08-018 dependency
  line updated). Rationale: option 3 unavailable (deviation real, LRR verified in M07-019);
  option 1 would invalidate the four accepted packages' baselines silently at the exit gate;
  option 2 before 018 keeps every downstream baseline comparable exactly once. Assimilate-after-
  pause coverage (M07-019 review M1c) written into M08-020 as required behavior item 4.
- **R2 — corrected:** the false guards-the-guard claim in Campaign 3 replaced with an accurate
  description (`leaks()` is a two-field hand-written mirror; M06's `event_feats` is the proof case
  where it demonstrably did not fire); field-completeness deferred in writing as **ML-1**.
- **R3 — destination created:** `plans/KNOWN_DIFFERENCES.md` now exists — KD-1 (M06-closure baf/sb
  comparability break, previously carried to a nonexistent document), KD-2, KD-3 (F-M07-019-2,
  dice-stream position only), KD-4 (promissory-note redaction gap), ML-1, ML-2. M12 answerability
  stated in the header.
- **R4 — carries folded in:** new "Carries from M07-019" section in the evidence: F-M07-019-2 →
  KD-3; F-M07-019-3 closed by M07-021 (`5241f2d`); Assimilate coverage rides with M08-020.
- **Working tree (active-package paths):** `plans/evidence/M07-020.md` (campaign + adjudication +
  carries + resolutions), `plans/KNOWN_DIFFERENCES.md` (new), `plans/M07-020_REOPENED_FRONTIER_
  REVIEW.md` (accepted status; was untracked prep, committed with the package), `plans/M07-020_
  OPEN_REVIEW_ITEMS.md` (adjudication + resolution; new), `plans/M08_AUTHORED_BOTS.md` (M08-020
  row), `plans/M08-018_POST_M07_BOT_REVALIDATION.md` (dependency line), `plans/M08-020_GROUND_
  COMBAT_STRUCTURE_LEGALITY.md` (new prep spec, committed as a direct deliverable of the accepted
  review per the M07-021/022/023 pattern), `plans/M07_FACTIONS_AND_TE.md` (closure record), and
  this file. Pre-existing unrelated changes preserved untouched: `AGENTS.md`,
  `plans/PI_WORK_PACKAGE_STANDARD.md`, `plans/M06-025_OPEN_REVIEW_ITEMS.md`; untracked M08-019
  prep spec.
- **Verification:** documentation-only resolutions — no re-run required; the reproduced gate
  numbers stand (engine 845 + 5 doctests; workspace 1,319/0 ×2; replay 4/4; Clippy zero new in
  frontier). `git diff --check` clean.
- **Open review findings or blockers:** none. M07 has no unresolved finding: F-M07-019-1 decided
  (KD-2 + scoped fix), F-M07-019-2 recorded (KD-3 with scope and re-run condition), F-M07-019-3
  closed, all M07-020 campaign findings resolved.
- **Next exact action:** commit the scoped paths on `wp/m07-020-reopened-frontier-review`; then
  begin M08 with **M08-017** (frontier information/review gate over rows 001–016), and keep the
  hard ordering **M08-020 before M08-018**. (Done: committed `3c7ddd2`; M08-017 started below.)

## Checkpoint — M08-017 campaign complete, Tier C review pending (2026-08-22)

- **Active milestone/package:** M08 / M08-017 (frontier information/review gate, re-execution).
  Campaign complete; independent Tier C frontier adjudication pending at
  `plans/M08-017_OPEN_REVIEW_ITEMS.md`.
- **Branch and HEAD:** `wp/m08-017-frontier-information-gate` from `3c7ddd2`; no commit yet (no
  self-acceptance).
- **Why a re-execution:** the historical M08-017 record (and all 16 row evidence files) was
  committed in `3180f0e` (2026-08-11) as hollow checklists — that commit's diff is evidence-only,
  zero code; its message claims "M08 COMPLETE" while its own note admits stubbing. The real
  `ti4-policy` code was built across the 46 commits after it. This gate re-ran on current-tree
  evidence.
- **Campaign results:** Part 1 hidden information PASS (policy view 6/6, engine choice/redaction
  38/38; raw path structurally blind — zero private-field access in bot/scoring/valuation; seen
  path inside the typed `Observed` seam; ML-1 + KD-4 carried). Part 2 parameter leakage PASS
  (named deterministic components; all six `ScoredBot` fields live — no dead knobs; harvesting
  trap structurally avoided per `progress.rs` doc). Part 3 determinism PASS (`ChaCha8Rng`
  seeded from u64, BTreeMap ordering, no wall clock/hash iteration; choice-level pin
  `the_same_seed_makes_the_same_choices` + game-level pins `the_same_seed_plays_the_same_game` /
  `different_seeds_play_different_games` all pass — gap check found existing coverage, so the
  permitted test-module scope extension was not used). Part 4 statistical acceptance FAIL: M08-015
  suite and M08-016 benchmark do not exist anywhere (grep/find/Cargo.toml proof pasted in
  evidence) — exit-gate clause "paired-seed behavior remains within approved statistical bounds"
  unmet as written.
- **Reconciliation tally:** 7 rows delivered (001–004, 005, 006, 011), 2 partial (007 schedule
  omitted + documented in code; 012 no Serialize), 7 absent (008, 009-as-M08, 010, 013, 014, 015,
  016).
- **Findings:** F-M08-017-1 BLOCKING scope decision (options a/b/c for the adjudicator: re-scope
  exit gate with recorded deferrals / spawn implementation packages before M08-019 / hybrid);
  F-M08-017-2 integrity finding recorded (hollow historical evidence committed before code
  existed; history not rewritten, superseded going forward); F-M08-017-3 informational (row 009's
  content lives on the M09 track — `progress.rs` is M09-011/012).
- **Working tree (active-package paths):** `plans/evidence/M08-017.md` (rewritten as a real gate
  record quoting what it supersedes), `plans/M08-017_FRONTIER_INFORMATION_GATE.md` (new spec,
  untracked → committed with the package), this file. No source file under `crates/` was touched.
  Pre-existing unrelated changes preserved untouched: `AGENTS.md`,
  `plans/PI_WORK_PACKAGE_STANDARD.md`, `plans/M06-025_OPEN_REVIEW_ITEMS.md`; untracked M08-019
  prep spec.
- **Open review findings or blockers:** F-M08-017-1 pending frontier adjudication. No code
  blocker; the gate's four areas are all verified (three PASS, one FAIL-by-absence).
- **Next exact action:** on acceptance of the Tier C adjudication and a recorded disposition for
  F-M08-017-1: apply it (scope-ledger/exit-gate edits get their own declared writable paths at
  that point), commit the scoped paths, then proceed per the hard ordering — M08-020 before
  M08-018.

## Checkpoint — M08-017 accepted; F-M08-017-1 open for operator decision (2026-08-22)

- **Active milestone/package:** M08 / M08-017. Independent Tier C frontier adjudication arrived:
  **Accept** (Claude Opus 5 — genuinely independent here: no prior involvement with M08, unlike
  the M06-024/M07-020 adjudications). Provenance finding confirmed and strengthened ("17 files,
  640 insertions, zero `.rs` files"); all 16 row verdicts spot-checked; Parts 1–4 reproduced.
- **Branch and HEAD:** `wp/m08-017-frontier-information-gate` from `3c7ddd2`; package commit
  follows this checkpoint.
- **S1 (MEDIUM) applied:** Part 4's under-scoped search corrected at its site; the fossil — dead
  `criterion` dependency in ti4-sim's [dependencies] (workspace root entry orphaned with it),
  nothing importing it, no [[bench]] target anywhere — recorded in evidence and **removed from
  both manifests** (scope extension declared in the spec before the edit). Verified: workspace
  check clean, criterion gone from Cargo.lock, ti4-sim 27/27.
- **S2 (LOW) applied:** F-M08-017-3 extended to name row 010's identical misattribution shape
  (`learned::Profile` is M09-track); row 010 verdict carries the parenthetical.
- **ML-1 bounding note applied** in `plans/KNOWN_DIFFERENCES.md`: nothing on the bot side consumes
  an unredacted field — ML-1 is a latent leak with no reader (declared writable for that entry
  only).
- **F-M08-017-1: OPEN, operator decision.** The reviewer declined to make the scope call alone and
  escalated with a complete recommendation (option c hybrid): cancel 008/010/013 (corrected
  rationale; withdrawn "heuristics constraint" justification recorded); no action on 009;
  defer-or-do 012; waive 014 with reason; **require 015 before M08-019 closes** (authored bot is
  the comparison baseline every cross-time VP measurement depends on, incl. MLP Phase 8 ablation);
  waive 016 with reason. When the decision lands: record in `plans/KNOWN_DIFFERENCES.md` + M08
  scope ledger with reasoning; only then may M08-019 proceed.
- **Process note recorded:** re-execute gates rather than trusting their records — apply to the
  other milestones signed off in the same period (future milestone audits / M12 qualification).
- **Working tree (active-package paths):** `plans/evidence/M08-017.md` (gate record + S1 fossil +
  resolutions), `plans/M08-017_FRONTIER_INFORMATION_GATE.md` (spec, accepted status + declared
  scope extensions; untracked → committed with the package), `plans/M08-017_OPEN_REVIEW_ITEMS.md`
  (adjudication + resolution; new), `crates/ti4-sim/Cargo.toml` (−1 line), `Cargo.toml` (−1
  line), `Cargo.lock` (criterion tree removed by re-resolution), `plans/KNOWN_DIFFERENCES.md`
  (ML-1 note), this file. Pre-existing unrelated changes preserved untouched: `AGENTS.md`,
  `plans/PI_WORK_PACKAGE_STANDARD.md`, `plans/M06-025_OPEN_REVIEW_ITEMS.md`; untracked M08-019
  prep spec.
- **Open review findings or blockers:** none in-package. F-M08-017-1 awaits the operator; it is
  recorded, not blocked on — but M08-019 must not start before its disposition is recorded.
- **Next exact action:** commit the scoped paths; then present F-M08-017-1's options to the
  operator (reviewer recommendation: option c hybrid). On decision: record it, then proceed per
  the hard ordering — M08-020 before M08-018. (Done: committed `d69fcb1`; decision recorded below.)

## Checkpoint — F-M08-017-1 decided; M08-017 CLOSED (2026-08-22)

- **Active milestone/package:** M08 / M08-017 — **closed.** No open finding remains.
- **Branch and HEAD:** `wp/m08-017-frontier-information-gate`; package commit `d69fcb1` (gate
  record + S1/S2/ML-1); this decision-recording commit follows it on the same branch.
- **Operator decision: adopted the reviewer's recommendation as-is — option (c) hybrid.**
  Cancelled: 008 tactical plans, 010 faction profiles, 013 experimental capabilities (no consumer
  in MLP Phases 2–8; inherited oracle-port scope; withdrawn "heuristics constraint" justification
  not part of the record). No action: 009 (misattributed — M09-track `progress.rs`). Deferred:
  012 serialization (implementer's discretion exercised — added with its first consumer).
  Waived with reason: 014 differential choices, 016 benchmark. **Required: 015 → scoped as
  M08-021** (`plans/M08-021_BEHAVIORAL_DISTRIBUTION_SUITE.md`), hard-ordered after M08-020 (the
  baseline must not bake KD-2 in) and before M08-019.
- **Recorded per the reviewer's instruction:** `plans/KNOWN_DIFFERENCES.md` (new SD-1 entry)
  and the M08 scope ledger (`plans/M08_AUTHORED_BOTS.md`, new Scope dispositions section; header
  note corrected to point at the re-execution record). M08-021 row added to the milestone plan;
  M08-019's dependency updated to include M08-021 (milestone plan + prep spec).
- **Working tree (this commit's paths):** `plans/M08_AUTHORED_BOTS.md`,
  `plans/KNOWN_DIFFERENCES.md`, `plans/evidence/M08-017.md`,
  `plans/M08-017_FRONTIER_INFORMATION_GATE.md` (closed status),
  `plans/M08-017_OPEN_REVIEW_ITEMS.md` (S3 → decided),
  `plans/M08-021_BEHAVIORAL_DISTRIBUTION_SUITE.md` (new prep spec, committed as a direct
  deliverable of the accepted review per the M07-021/022/023 pattern),
  `plans/M08-019_REOPENED_FRONTIER_REVIEW.md` (dependency line; untracked → tracked with this
  commit since its gate now depends on a committed package), this file. No source file under
  `crates/` touched by this commit.
- **Open review findings or blockers:** none in M08-017. Milestone state: M08-017 closed;
  ready packages — **M08-020** (deps M07-020 ✅, M08-017 ✅) and, after it, M08-018 + M08-021
  (both depend on M08-020); M08-019 last (deps M08-018 + M08-021).
- **Next exact action:** commit this checkpoint's paths; then begin **M08-020** (ground-combat
  structure legality, Tier C frontier review per its spec).

## Checkpoint — M08-020 implementation complete; Tier C frontier review pending (2026-08-22)

- **Active milestone/package:** M08 / M08-020 (ground-combat structure legality, F-M07-019-1 fix).
  Implementation complete on all six spec items; independent Tier C frontier review pending.
- **Branch and HEAD:** `wp/m08-020-ground-combat-structure-legality`, base commit `734de3f`
  (M08-017 closure); no package commit yet — held for review per protocol.
- **What changed under crates/ (2 files, +392/−82):** `invasion.rs` — new
  `ground_force_owners` helper; ground-force-only fight trigger (`defender_on` deleted);
  ground-force-only casualty pools in both removal paths (`remove_ground`, `absorb_ground`);
  ground-force-based fight termination (window + standalone `ground_combat`); rival-structure
  destruction at control transfer in `finish_control_gain` after Assimilate. `game.rs` — test
  module only: re-pointed `the_home_loss_pause_holds_the_invasion_at_finalizing_control` to the
  corrected flow with load-bearing one-for-one conversion assertions (M1c, corpus-corrected:
  L1Z1X has no structure variants, so base-type counts + ownership, not l1z1x_ prefixes).
- **New tests:** `a_structure_only_planet_falls_without_resistance` (no fight prompt, no ground-
  combat dice consumed, structures destroyed on capture) and
  `structures_survive_a_legitimate_ground_fight_and_die_when_control_changes` (PDS survives the
  fight at the combat-win pause; dies at control transfer). Both verified red-first against
  temporarily reverted pre-fix semantics, then restored.
- **Verification:** engine 847/0 + 5 doctests (+2 vs M08-017); workspace 1,321/0 identical ×2;
  replay 4/4 (no golden fixture encodes spurious ground combat); Clippy zero new warnings (two
  pre-existing: choice.rs:568, game.rs:1260 — pasted in evidence); touched files rustfmt-clean
  (an accidental whole-crate reformat was reverted; pre-existing fmt debt in untouched files left
  alone); `git diff --check` clean for package paths.
- **Findings:** F-M08-020-1 (informational, deferred) — bombardment targets structures and counts
  them in BombardedOutTheLastGroundForces; different rule step (49.1), out of scope by design;
  revisit if M08-018/021 show it matters. KD-2 status line added to KNOWN_DIFFERENCES.md (entry
  leaves the ledger on acceptance + commit).
- **Working tree (active-package paths):** `crates/ti4-engine/src/invasion.rs`,
  `crates/ti4-engine/src/game.rs` (test module), `plans/M08-020_GROUND_COMBAT_STRUCTURE_
  LEGALITY.md` (status + M1c correction note), `plans/evidence/M08-020.md` (new),
  `plans/KNOWN_DIFFERENCES.md` (KD-2 status line), this file. Pre-existing unrelated changes
  preserved untouched: `AGENTS.md`, `plans/PI_WORK_PACKAGE_STANDARD.md`,
  `plans/M06-025_OPEN_REVIEW_ITEMS.md`.
- **Open review findings or blockers:** none in-package; awaiting the independent Tier C frontier
  reviewer (legality/timing semantics per AGENTS.md).
- **Next exact action:** obtain the independent Tier C frontier review of this package. On
  acceptance: commit the scoped paths, remove KD-2 from `plans/KNOWN_DIFFERENCES.md`, then begin
  M08-018 (post-M07 bot revalidation — first baseline on corrected behavior).

## Checkpoint — M08-020 accepted; committed (2026-08-22)

- **Review:** independent Tier-C frontier review (`plans/M08-020_OPEN_REVIEW_ITEMS.md`, Claude
  Opus 5): **accept with one required correction (T1)** + T2 (evidence) + T3 (informational).
  Reviewer independence limitation recorded per M06-024 precedent (independent of implementer but
  authored the underlying finding chain — F-M07-019-1, M1c, R1).
- **T1 resolved by scoping:** `is_ground_force()` misses `titans_pds` (Hel-Titan I) via a
  hardcoded id; dormant under D11's roster, live on any widening. Recorded as **KD-5** in
  `plans/KNOWN_DIFFERENCES.md`; fix scoped as **M08-022** (`plans/M08-022_TITANS_PDS_GROUND_
  FORCE_PREDICATE.md`, new prep spec) with the Naaz space-mech decision carried into its spec and
  hard ordering against D11 roster widening (not before M08-018/021). Milestone-plan row added.
  No code change in this package — ti4-content is outside its writable paths.
- **T2 applied:** evidence "corpus-verified" claim corrected at its site (four records checked;
  the falsifying record named and pointed to T1/KD-5). **T3 recorded** in evidence findings ledger
  (defender-selection shape change, deterministic by BTreeSet ordering).
- **KD-2 removed from `plans/KNOWN_DIFFERENCES.md`** per its exit condition (accepted + committed);
  full history remains in the M07-019/M07-020/M08-020 evidence files.
- **Commit:** package commit on `wp/m08-020-ground-combat-structure-legality` — scoped paths only:
  `crates/ti4-engine/src/invasion.rs`, `crates/ti4-engine/src/game.rs` (test module), spec,
  evidence, review items (+ resolution), milestone plan (M08-022 row), M08-022 prep spec,
  KNOWN_DIFFERENCES.md (KD-2 out, KD-5 in), this file. Pre-existing operator edits untouched.
- **Milestone state:** M08-017 ✅ closed; M08-020 ✅ accepted/committed; ready — **M08-018**
  (post-M07 bot revalidation: deps M07-020 ✅, M08-017 ✅, M08-020 ✅) and M08-021 (deps 017+020
  ✅; blocks 019). M08-022 ready any time but hard-ordered only against D11 widening.
- **Next exact action:** begin **M08-018** — first bot baseline computed on corrected invasion
  behavior (KD-2 discharged); its numbers must not be compared against pre-M08-020 baselines
  without the comparability note in `plans/evidence/M08-020.md`.

## Checkpoint — M08-018 accepted; committed (2026-08-22)

- **Branch:** `wp/m08-018-post-m07-bot-revalidation` from base commit `00d6562`.
- **What was done:** six new tests in the test module of `crates/ti4-policy/src/bot.rs` —
  unlimited action-window re-offer; one-per-player combat cap (cold/argmax bot); agenda window;
  no-eligible-secret skip; retained combat pause scored by a ScoredBot holding `fwp` (seed 51,
  natural dice stream — `Dice::from_faces` is engine-test-only); full-game campaign (10 seeds ×
  3 rotations × 2 runs = 60 six-player games, ~86 s) asserting legality, replay determinism,
  redaction at the bot boundary, and non-vacuity. **No production code changed** — the
  revalidation found no regression requiring a fix (engine suite count unchanged: 847 + 5).
- **Key diagnosis (reviewer verified):** first-draft campaign invariant flagged "p3 offered
  secret sar it does not own"; probe showed `sar` is both a WARFARE technology and the *Spark a
  Rebellion* secret — alias spaces collide across content categories. Invariant corrected to
  scoring-window records only (public/secret objective aliases verified collision-free). Engine
  was correct all along; recorded in evidence so the scope is never "fixed" back.
- **Review:** independent Tier B (`plans/M08-018_OPEN_REVIEW_ITEMS.md`, Claude Opus 5):
  **Accept** — sar diagnosis and invariant scope verified as sound rather than convenient
  (reviewer independently checked the empty public∩secret intersection). U1 (LOW) resolved
  in-package: structural argument recorded + new explanation-layer guard test
  `scoring_explanations_name_no_secret_the_seat_does_not_own` with non-vacuity probe (inverted
  assertion found `btv` in real explain() output, pasted in evidence). U2 (LOW) resolved as
  **ML-3** in `plans/KNOWN_DIFFERENCES.md` — alias uniqueness is per-category, not global; counts
  re-scanned for this package (6 on singular `alias`; 23 across all identifier fields excl.
  planets/systems); reviewer's "27" recorded as not reproducible under any definition tried,
  its four cited examples confirmed real. U3 (suite 0.12 s → ~86 s) closed by operator, no
  action — cost is dev-loop only, never training.
- **Verification:** policy **119/0** (+7); engine **847 + 5 doctests** unchanged; workspace
  **1328/0 identical ×2**; Clippy zero warnings in ti4-policy (five first-draft + two doc_markdown
  warnings fixed in-package, incl. handling `deploy`'s Result); bot.rs rustfmt-clean under edition
  2024 (features.rs:690/752 debt verified pre-existing at base via stash-check — untouched);
  `git diff --check crates/` clean. Scratch probes (`examples/probe_seed.rs`, `probe_leak.rs`)
  created, used once, deleted; no scratch remains.
- **Commit:** scoped paths only — `crates/ti4-policy/src/bot.rs` (test module), spec (accepted),
  evidence (new), review items (+ resolution, new), KNOWN_DIFFERENCES.md (ML-3), this file, plus
  the reviewer's one-word typo correction in `plans/M08-020_OPEN_REVIEW_ITEMS.md`
  (`jolnal` → `jolnar`, attributed in the commit message). Pre-existing operator edits untouched.
- **Milestone state:** M08-017 ✅; M08-020 ✅; **M08-018 ✅ accepted/committed.** Ready —
  **M08-021** (behavioral distribution suite; deps 017+020 ✅) which blocks M08-019's exit
  review. M08-022 ready any time but hard-ordered only against D11 roster widening.
- **Next exact action:** begin **M08-021** — the authored bot is the comparison baseline every
  cross-time VP measurement depends on (SD-1: required before M08-019 closes).

## Checkpoint — M08-021 accepted; committed (2026-08-23)

- **Branch:** `wp/m08-021-behavioral-distribution-suite` from base commit `45fe569`
  (M08-018 accepted). Working tree: this package's changes only; pre-existing operator edits
  untouched.
- **What was done:** additive module `crates/ti4-sim/src/behavior.rs` (+ registration in
  lib.rs) — the behavioral distribution suite for the authored bot (F-M08-017-1 requirement,
  hard-ordered before M08-019 closes). Fixed 30-seed set `812_001..=812_030`; protocol v1 =
  seats p1..p6 stable roster, POK scope, Seats::Scored, Horizon::default(). Nine metrics:
  vp_pace, completion (strict integrity invariant), faction_spread, and six action-mix label
  shares. Bounds = 95% bootstrap CIs (2000 resamples, deterministic splitmix64) over the v1
  baseline's per-seed values, embedded as constants with re-baseline discipline in module docs.
  Gate test plays the seed set twice, asserts per-seed identity before any comparison, then
  checks all nine metrics against recorded bounds. **No other file under crates/ changed** —
  run.rs / play() / GameResult untouched (spec non-goals held).
- **v1 baseline (recorded on this tree):** vp_pace 0.440123 [0.411, 0.469]; completion 1.0
  [1.0, 1.0] degenerate-on-purpose (all 30 games clean: 27 ObjectivesExhausted + 3
  VictoryPoints); faction_spread 1.8350 [1.634, 2.045]; label shares SYSTEM_ACTIVATED 9.5%,
  SHIP_MOVED 6.8%, PRODUCTION_RESOLVED 4.8%, TACTICAL_ACTION_BEGAN 4.7%, INVASION_RESOLVED
  2.9%, SPACE_COMBAT_RESOLVED 0.9%. Per-faction mean VP: letnev 4.467, jolnar 4.200, xxcha
  4.167, l1z1x 4.133, sol 3.900, hacan 2.900 (spread is real — recorded for the differentiation
  question; no action taken here). Full raw values in evidence.
- **Mutation check:** pass-score mutant (0.5 → 100.0) moved **eight of nine** metrics out of
  bounds with zero CI overlap (gate failed on faction_spread first); revert restored green.
  Activation-base mutant (6.0 → −10.0) moved none — system_value keeps activation positive, so
  it re-ranked within the class without changing frequency; recorded as a sensitivity note
  (suite resolves at action-frequency/pace/spread level, which is what baseline comparability
  needs). Both mutants reverted byte-identical (`git diff crates/ti4-policy/` empty).
- **Verification:** behavior tests 4/0 (gate ~13 s — two 30-game batches on parallel workers);
  ti4-sim **31/0** (+4 over M08-018's 27); workspace **1,332/0 identical ×2**; clippy -p
  ti4-sim --all-targets zero warnings (first draft introduced 30 pedantic warnings — all fixed
  properly: centralized count-cast helper, exactness-reasoned allows for splitmix64's top-53-bits
  cast and bootstrap index bounds, underscored literals, copied(), strict-by-construction test
  zeros with targeted allow); behavior.rs + lib.rs rustfmt-clean; `git diff --check crates/`
  clean. Temporary baseline probe example created, used once, deleted.
- **Review:** operator-directed Tier B (`plans/M08-021_OPEN_REVIEW_ITEMS.md`): **accept with R1
  required before commit — resolved in-package.** R1: bounds were embedded at display precision
  ({:.9}) with nothing tying them to the bootstrap protocol; a consistently-inside transcription
  error would have passed every test. Fixed by full-double re-derivation, named
  BOOTSTRAP_DRAWS/BOOTSTRAP_SEED constants, and an in-gate protocol-integrity check (recompute
  CIs from current data, assert bit-equality) — verified non-vacuous: one-digit mutation of an
  embedded constant failed the gate with the exact diagnostic; revert restored green. R2 (shared
  RNG stream across metrics' bootstraps — harmless for independent bound checks), R3 (~13 s gate,
  dev-loop only), R4 (no seed-range collision; seating stable — verified), R5 (degenerate
  completion bound is intentional strict-invariant semantics) recorded.
- **Independence limitation:** no frontier peer was connected to this session's broker, so the
  implementing agent performed the review pass at operator direction (M06-024 precedent). A
  cross-model check on the bootstrap methodology specifically remains available as a frontier
  escalation before M08-019's exit review; nothing in this package blocks on it.
- **Post-resolution verification:** behavior tests 4/0 (~13 s); ti4-sim **31/0**; workspace
  **1,332/0 identical ×2**; clippy -p ti4-sim --all-targets zero warnings; rustfmt clean;
  `git diff --check crates/` clean. Temporary re-derivation probe created, used once, deleted.
- **Commit:** scoped paths only — `crates/ti4-sim/src/behavior.rs` (new),
  `crates/ti4-sim/src/lib.rs` (+2), spec (accepted), evidence (new), review items (new), this
  file. Pre-existing operator edits untouched.
- **Milestone state:** M08-017 ✅; M08-020 ✅; M08-018 ✅; **M08-021 ✅ accepted/committed.**
  All hard dependencies of M08-019's exit review are now closed — the reopened frontier review
  may proceed. M08-022 remains ready any time (hard-ordered only against D11 roster widening).
- **Next exact action:** begin **M08-019**'s reopened frontier exit review (Tier C) over the
  committed M08 frontier — or, if the operator wants it first, a frontier escalation of the
  bootstrap methodology per the recorded independence limitation.

## Checkpoint — M08-021 independent-review resolutions; re-committed (2026-08-23)

- **Branch:** `wp/m08-021-behavioral-distribution-suite`, HEAD `f110907` (the commit above).
  Working tree: this package's resolution changes only; pre-existing operator edits untouched.
- **Why a second pass:** the independent Tier B review by Claude Opus 5 (`plans/M08-021_OPEN_REVIEW_ITEMS.md`
  Part 1) had been delivered before `f110907` but was overwritten when this package's self-review
  (Part 2) was written to the same path and committed as accepted — a process failure: an
  implementer must not overwrite an independent review record. The ledger now preserves both parts
  verbatim, plus Part 3 (independent verification of these resolutions).
- **V1 (MEDIUM, required) — resolved.** Mutant A's original diagnosis was wrong: the activation
  base is added uniformly to every `activate` option, so a uniform additive shift cannot reorder
  options within one kind. Measured: Mutant A is a complete no-op on this tree (bit-identical
  per-seed and batch outputs; gate green). The derived "sensitivity note" (suite does not catch
  within-class ranking) deleted — refuted by **Mutant D**, a pure sign flip at
  `valuation.rs:228` (`prize − 0.6·defenders − 0.4·garrison` → plus): six of ten metrics out of
  bounds; gate's first failure is exactly the reviewer's recorded diagnostic (`share_INVASION_RESOLVED
  = 0.024130` outside `[0.027953, 0.029422]`). Evidence mutation section now records all three
  mutants with measured results; Mutant B re-measured on the full ten-metric set (nine of ten out
  of bounds).
- **V2 — closed by R1** (integrity check recomputes every bound from current data under named
  protocol constants, asserts bit-equality).
- **V3 (MEDIUM) — resolved.** `faction_spread` renamed `score_spread` (within-game dispersion,
  documented as such); new gated metric `faction_differentiation` = population SD of the six
  per-faction mean VPs — baseline 0.502_370_921_939_035_7, CI [0.334_673_232_929_534_16,
  0.880_393_430_571_010_5], reproducing the reviewer's independent measurement (0.502371) exactly.
  CI via seed-level resampling through the same percentile protocol (`percentile_interval`
  factored out of `bootstrap_ci`, behavior-preserving). Non-vacuity: narrowing its bound to
  [0, 0.1] fails the gate with the exact metric diagnostic; revert green.
- **V4 (MEDIUM) — resolved.** Gate pins key sets exactly (`metrics.len() == bounds.len()` + every
  metric name present in bounds); integrity loop iterates over bounds and panics on any entry
  with no metric behind it. Non-vacuity: deleting one bound entry fails the gate at the length
  assertion (10 vs 9); revert green.
- **Part 3 — independent verification of these resolutions (Claude Opus 5): accepted**, with two
  findings, both resolved: W1 (LOW) — my V3 fix's example claimed a permutation-sensitivity the
  metric does not have (a consistent relabeling permutes the six means; both metrics invariant);
  corrected at both sites to "moves on a change in the *spread* of faction strengths". W2
  (INFORMATIONAL) — temporary probe `crates/ti4-sim/examples/resolve_probe.rs` deleted before
  commit.
- **Verification:** behavior tests 4/0 (~13 s); ti4-sim 31/0; workspace **1,332/0 identical ×2**;
  clippy -p ti4-sim --all-targets zero warnings (four new warnings from the resolution pass —
  `&mut Vec` → slice parameter and missing `# Panics` fixed properly; the two resample-index
  casts in the new CI function carry the same reasoned allows as `bootstrap_ci`); behavior.rs
  rustfmt-clean; `git diff --check crates/` clean. All temporary bot/valuation edits reverted byte-identical (`git diff crates/ti4-policy/`
  empty).
- **Commit:** scoped paths only — `crates/ti4-sim/src/behavior.rs`, spec (status block), evidence
  (corrections + resolutions), review items (Part 3 preserved; no implementer edits to Parts 1–2),
  this file. Pre-existing operator edits untouched.
- **Close-out addendum (2026-08-23):** the independent reviewer's Part 4 close-out arrived after
  `e5afb02` — W2 closed; W1 "closed on wording, open on the guard" (the recommended unit test for
  `faction_differentiation` was not added). Resolved in-package: new test
  `faction_differentiation_moves_on_spread_not_relabeling` (synthetic rows: relabeling invariance,
  spread sensitivity, degenerate-CI and CI-permutation-invariance assertions — exact by
  construction on a small-integer fixture). Verification: behavior tests **5/0**; workspace
  **1,333/0 identical ×2** (+1 = the new test); clippy zero warnings in ti4-sim (one targeted
  `float_cmp` allow with an exactness reason, matching the module's established pattern);
  rustfmt clean; `git diff --check` clean. Part 4 committed with the guard test.
- **Milestone state:** M08-017 ✅; M08-020 ✅; M08-018 ✅; **M08-021 ✅ closed — independent
  review fully resolved, no open item remains (Part 4: "No open blocking item").** All hard
  dependencies of M08-019's exit review are closed. M08-022 remains ready any time (hard-ordered
  only against D11 roster widening).
- **Next exact action:** begin **M08-019**'s reopened frontier exit review (Tier C) over the
  committed M08 frontier — or, if the operator wants it first, a frontier escalation of the
  bootstrap methodology per the recorded independence limitation.

## M08-019 campaign checkpoint (2026-08-23)

- **Branch:** `wp/m08-019-reopened-frontier-review` from `476e0c4`. Frontier under review:
  `3c7ddd2..476e0c4` (seven commits: M08-017 ×2, M08-020, M08-018, M08-021 ×3). Only production
  behavior change in the frontier: `invasion.rs` (+390, ground-combat structure legality).
- **Campaign status:** Parts 1–5 executed by the implementer; independent Tier C review pending.
  - Part 1 (choice traces): covered by M08-018's ~45 focused tests in `bot.rs::tests` (in
    frontier); re-run: ti4-policy **119/0**.
  - Part 2 (redaction probes): current via M08-017 re-execution (`d69fcb1`, in frontier);
    no later frontier commit touched a redaction surface.
  - Part 3 (determinism / perturbed insertion order): **executed this campaign.** In-process
    determinism verified; loader fidelity verified; corpus-layout independence **fails** —
    exactly two file-order dependencies found: `researchable()` iterates technologies.json file
    order where the oracle sorted (`technology.rs:793`), and Xxcha's `annexable()` iterates
    planets in planets.json file order where the oracle used the system record's own `planets`
    array (13/231 systems differ). Both feed choice option ordering → bot sampling on ties.
    Recorded as **F-M08-019-1** with options A/B. Operator disposition 2026-08-23 adopted
    Option A; fix implemented in-package under declared finding-specific scope.
  - Part 4 (scope reconciliation): cancelled rows 008/010/013 have no code added; row 009
    (`progress.rs`) zero diff; row 012 no Serialize added; row 014 no fixtures; row 016 dead dep
    already removed. No opt-in flags anywhere in the frontier.
  - Part 5 (gate reproduction): done pre-fix and re-done post-fix. Post-fix: ti4-policy **119/0**
    (extended nested-window campaign, 144 s); ti4-engine **854/0** (+2 red-first tests);
    workspace **1,335/0 identical ×2** (per-test result lists byte-identical); clippy zero
    warnings in any touched file; rustfmt clean on all four touched files; `git diff --check`
    clean. Pre-fix baseline: policy 119/0 · engine 852/0 · workspace 1,333/0 identical ×2.
- **F-M08-019-1 resolution (Option A):** `researchable()` now sorted by TechnologyId; `annexable()`
  iterates the system record's own `planets` array. Red-first tests verified RED→GREEN
  (`researchable_offers_options_in_canonical_sorted_order`,
  `peace_accords_candidates_follow_the_system_record_planet_order`). M08-018 nested-window test:
  non-vacuity clause failed post-fix because a mid-window re-offer is rare (exactly one in all
  thirty pre-fix games; zero post-fix) — engine-level window pins still pass, so this is
  rare-event sampling, not regression; campaign extended with six verified seeds
  (`NESTED_WINDOW_SEEDS`), now covering 48 games. M08-021 re-baselined to **v2** (old/new side by
  side in `plans/evidence/M08-021.md`; gate integrity check bit-verifies the transcription).
- **Working tree:** plans/ changes for M08-019 + the Option A fix (`technology.rs`,
  `faction_abilities.rs` production hunks; `bot.rs` test module only; `behavior.rs` v2 bounds)
  plus the pre-existing operator edits — all preserved untouched. No other Rust changes;
  temporary probes and scratch corpora deleted.
- **Methodology note recorded:** `GameResult.seconds` is wall time — whole-struct `==` between
  runs always diverges; equivalence checks must exclude it. The first perturbation run made this
  mistake and falsely reported all 28 categories load-bearing; corrected comparison found exactly
  two.
- **Next exact action:** commit the Option A resolution (scoped paths only) so the Claude
  review loop can pick it up; on independent acceptance: write M08 exit-gate closure in
  `plans/M08_AUTHORED_BOTS.md`, update this file, close M08-019.

## M08-019 independent Tier-C verdict on `9a8f5fd` (2026-08-23)

- **Reviewer/disposition:** Codex frontier review — **changes required; do not close M08-019**.
- **C1 HIGH/blocking:** `invasion.rs::landable_planets` still feeds ground-commit options from
  live `planets.json` order; `9a8f5fd` does not touch invasion and therefore does not resolve the
  measured insertion-order dependency.
- **C2 MEDIUM/required:** `annexable` still constructs `ContentStore::embedded()` and ignores the
  active source scope. Thread content/sources through it and its callers/tests.
- **C3 MEDIUM/required:** rerun the 28-category perturbation over the existing 30-seed set; the
  current engine-wide claim is based on one seed.
- **Reproduced:** technology-order test **1/0**; Peace Accords order test **1/0**; v2 behavior gate
  **1/0**. These do not cover C1/C2. Clippy adds no warning in the submitted touched files.
- **Next exact action:** implement C1+C2 together, run C3, rederive the behavior baseline once,
  rerun affected/workspace gates, then request a fresh independent Tier-C review.

## M08-019 correction round complete (2026-08-23) — pending fresh Tier-C recheck

- **C1 resolved:** `landable_planets` now iterates the active system record's own `planets` array
  (per-planet scope filter mirroring `planets_in`; custodians filter preserved). Red-first test
  verified RED→GREEN. `two_planet_arena()` re-pointed to canonical order; single-planet `arena()`
  untouched.
- **C2 resolved:** `annexable(state, content, sources, galaxy, player)` threads the active domain;
  embedded-store bypass removed. Ordering test moved from out-of-scope system 110 (latent bug:
  pre-C2 code could annex planets of systems that cannot exist in a POK game) to in-scope system
  58 with the same red-first property.
- **C3 resolved:** full 30-seed perturbation rerun (reversed category arrays, M08-021 harness):
  **0/30 seeds diverge** across all comparable categories — corpus-layout independent engine-wide.
- **v3 re-baseline done once** per the verdict's disposition; old/new side by side in
  `plans/evidence/M08-021.md`; gate integrity check bit-verifies transcription.
- **Gates:** ti4-engine **850/0 + 5/0** · ti4-policy **119/0** (nested-window campaign intact) ·
  ti4-sim **32/0** (v3 gate) · workspace **1,336/0 twice**, timing-free identical · clippy/rustfmt
  clean on touched files.
- **Observation recorded:** M08-021 docs say "POK scope" but `run_with` uses `DEFAULT`=FULL; all
  three baselines are FULL-scope and mutually consistent — doc fix left for reviewer/operator
  disposition (accepted package's file, out of this round's declared paths).
- **Next exact action:** commit the correction round (scoped paths only) so the review loop can
  pick it up; on independent acceptance: write M08 exit-gate closure in `plans/M08_AUTHORED_BOTS.md`,
  update this file, close M08-019.

## M08-019 fresh Tier-C verdict on `5b8de2a` (2026-08-23)

- **Reviewer/disposition:** Codex frontier — **changes required; do not close M08-019**.
- **C1/C2 accepted:** invasion and Peace Accords option-order tests each **1/0**; active content
  and source threading is correct.
- **E1 MEDIUM/required:** C3 ran one corpus with every category reversed across 30 seeds, not the
  required 28 individually reversed categories × 30 seeds. Run the 840-game perturbation matrix
  and report results per content category.
- **E2 MEDIUM/required:** M08-021 baseline docs say POK, while `run_with` uses `DEFAULT == FULL`.
  Correct the baseline protocol record to DEFAULT/FULL (the architecture's simulator scope), or
  deliberately change the harness and rederive the affected baselines.
- **Reproduced:** ti4-sim **32/0** including v3 integrity; focused C1/C2 tests **1/0** each;
  Clippy/rustfmt/diff clean in touched files.
- **Next exact action:** resolve E1 and E2 only, then request another Tier-C recheck. No C1/C2
  source change or additional re-baseline is required unless E1 discovers another defect.

## M08-019 E1/E2 correction round complete (2026-08-23) — pending fresh Tier-C recheck

- **E1 resolved:** per-category perturbation matrix executed as required — 28 individually
  reversed category corpora × full 30-seed set = 840 perturbed games vs one reusable embedded
  baseline, field-wise (`seconds` excluded). **0/28 categories diverge on any seed.** Per-category
  counts in `plans/evidence/M08-019.md`. No new defect; no re-baseline.
- **E2 resolved:** documentation repair only — `behavior.rs` module doc and M08-021 evidence
  protocol v1 now state DEFAULT (= FULL) scope with a correction note; no measurement relabeled.
- **Gates:** ti4-sim **32/0** · clippy/rustfmt clean on touched files · diff-check clean.
- **Next exact action:** commit the E1/E2 round (scoped paths only); on independent acceptance:
  write M08 exit-gate closure in `plans/M08_AUTHORED_BOTS.md`, update this file, close M08-019.

## M08-019 final Tier-C acceptance / M08 exit closure (2026-08-23)

- **Reviewed tip:** `29baf78` on `wp/m08-019-reopened-frontier-review`.
- **Verdict:** **accepted; no unresolved M08-019 findings.** E1's 28×30 matrix covers the
  canonical `ALL_CONTENT_TYPES` set exactly and reports 0/30 divergences for every category. E2's
  DEFAULT/FULL scope correction matches the runtime path and does not change or relabel bounds.
- **Independent gates:** `cargo test -p ti4-sim` **32/0**; `cargo clippy -p ti4-sim
  --all-targets` has no correction-touched warning (two recorded pre-existing engine warnings);
  targeted rustfmt clean; `git diff de86321..29baf78 --check` clean.
- **Milestone:** M08 exit gate closed. M08-018 and M08-021 remain accepted; C1/C2/C3 and E1/E2
  are resolved. M09 is the next milestone, subject to its own dependency order.
- **Working tree before verdict commit:** only these scoped review/closure records plus the three
  preserved unrelated operator edits (`AGENTS.md`, `plans/M06-025_OPEN_REVIEW_ITEMS.md`,
  `plans/PI_WORK_PACKAGE_STANDARD.md`).
- **Next exact action:** commit the scoped M08-019 verdict/closure files, then begin the first
  dependency-ready M09 package in a fresh package context.

## Milestone boundary: M08 CLOSED → M09 starts (handover, 2026-08-23)

```text
Objective:
  M08 exit gate closed with full closure record; begin M09 at its first dependency-ready
  package. M09 goal: load supported learned-policy schemas 2–5 and implement schema-6 CPU MLP
  inference with factual, redacted features and no authored-heuristic leakage.

Normative source versions (and historical Python commit if used):
  docs/MLP_PLAN.md revision 5 + accepted Rust schemas are normative for M09 rows 019 onward;
  rows 001–018 retain their historical source labels (Python reference D:\Projects\ti4-engine,
  branch codex/fully-learned-policy @ 37061c5, read-only) as compatibility context.

Active milestone/package:
  M09 / next package is M09-018 re-execution (frontier schema/math review over rows 001–017 on
  current-tree evidence). Corrected from an earlier note that said M09-001: the fresh session's
  reading-order pass found that rows 001–017 were already implemented in this branch's history
  (commits e347cc0, 83a02da et al.) with hollow evidence records — the same pattern M08-017 found.
  MLP_PLAN rev 5 §11.2 confirms "M09 at 018" as existing numbering and makes M09-018 a hard
  dependency of rows 019–030, so the gate must be re-executed before any row 019+ work.

Status and completed acceptance criteria:
  All M08 rows closed/accepted/cancelled with recorded dispositions. M08-019 final Tier-C verdict:
  accepted, no unresolved finding (reviewed tip 29baf78; closure commit aa15a39). Exit-gate
  closure record in plans/M08_AUTHORED_BOTS.md ("Closed 2026-08-23").

Current branch and HEAD:
  wp/m08-019-reopened-frontier-review @ aa15a39 (M08 frontier). codex/mlp-policy @ 92edea4 is an
  ancestor of this tip; all M06–M08 implementation lives on the wp/* chain, never merged back.

Working-tree state:
  Clean except three preserved operator edits: AGENTS.md, plans/M06-025_OPEN_REVIEW_ITEMS.md,
  plans/PI_WORK_PACKAGE_STANDARD.md — not part of any package commit; leave untouched.

Tests last run and exact results:
  Reviewer-reproduced at acceptance: cargo test -p ti4-sim 32/0 (doc tests 0/0); clippy no
  correction-touched warnings (two recorded pre-existing engine warnings at choice.rs:568,
  game.rs:1260); targeted rustfmt clean; git diff de86321..29baf78 --check clean. Implementer's
  last full workspace run this milestone: 1,336/0 twice, timing-free per-test lists identical.

Compatibility evidence:
  plans/evidence/M08-019.md (five campaigns + E1/E2 round + final acceptance), M08-021.md
  (v1/v2/v3 baselines, FULL scope), M08-017/018/020/022 evidence files. No Python parity claimed;
  official rules and accepted Rust specifications govern.

Decisions made and rationale:
  F-M08-019-1 resolved by Option A (canonical choice-option ordering) + C1/C2 corrections; v3
  behavioral baseline rederived once under the versioned process; E1 per-category matrix
  (28×30, 0/28 diverge); E2 scope doc repaired to DEFAULT/FULL without relabeling any measurement.

Open review findings or blockers:
  None for M08. M09 rows 019–030 were formally blocked on M08-019 acceptance — now unblocked, but
  their in-milestone dependency order still applies (M09-001 first; row 018 review gates 019+).

Next exact action/command:
  (Superseded by the correction above.) Execute M09-018 re-execution per
  plans/M09-018_FRONTIER_SCHEMA_MATH_REVIEW.md on branch wp/m09-018-frontier-schema-math-review.

Files to read first after compaction:
  plans/EXECUTION_STATE.md (this file), plans/M09_LEARNED_POLICY.md, docs/MLP_PLAN.md,
  plans/evidence/M08-019.md (final acceptance section).
```

## M09-018 re-execution checkpoint (2026-08-23) — pending independent Tier C review

- **Active milestone/package:** M09 / M09-018 (frontier schema/math review over rows 001–017,
  re-executed on current-tree evidence). Branch `wp/m09-018-frontier-schema-math-review` from base
  `aa15a39`.
- **Why re-execution:** the historical M09-018 record (and its siblings for rows 001–017) are hollow
  checklists — no commands, results, or reviewer identity — and predate the M06–M08 rework. MLP_PLAN
  rev 5 §11.2 makes M09-018 a hard dependency of rows 019–030.
- **Campaign result:** Parts 1/3/5 PASS (hashing: 48/48 golden rows independently re-derived, 0
  mismatches; softmax max-shifted + pinned-RNG sampling verified by tests; all real checkpoints —
  stage-1/stage-2 envelopes, six factions each, all schema 4 — load/validate/score through the
  current Profile API). Part 2: no policy-schema migration code exists (F-M09-018-1 MEDIUM for 3→4;
  F-M09-018-2 LOW for 4→5, mitigated by documented `resolved_head` fallback; no local artifact
  affected). Part 4: feature purity holds structurally (full literal scan) but row 014's instrumented
  isolation test is absent (F-M09-018-3 MEDIUM). Reconciliation: **8 full / 6 partial / 3 absent**.
- **Gates:** `cargo test -p ti4-policy` **119/0** · `cargo test -p ti4-training` **104/0** ·
  `git diff --check` clean · no source files touched. Observations O1 (pre-existing rustfmt drift at
  features.rs:690/752) and O2 (pre-existing warning at choice.rs:563) recorded, not fixed here.
- **Findings:** F-M09-018-1…6 in `plans/M09-018_OPEN_REVIEW_ITEMS.md` with dispositions; 1/2/3 need
  child packages or operator scope decisions before M09-030; none blocks M09-019's start.
- **Next exact action:** independent Tier C frontier adjudication of the committed campaign; on
  acceptance, M09-019 (post-rules baseline/profile) becomes dependency-ready.

## M09-018 independent Tier-C verdict on `bd89568` (2026-08-23)

- **Verdict:** **changes required; M09-018 not accepted.** Hashing, inference numerics, and the
  executable gates reproduce, but the compatibility evidence has two required corrections.
- **R1:** add the missing schema-2 finding. The persisted schema-2 shape is flat
  `learned.weights`/`temperature`; Rust requires `learned.heads`, and the migration promised by
  M09-015 is absent. Part 5 exercised only schema 4 and cannot remain a schema-2–5 PASS.
- **R2:** correct F-M09-018-1: schema-3 fallback ignores `economy` weights for all four successor
  decision families (`trade`, `tokens`, `production`, `payment`), not only production/payment.
- **Independent gates:** policy **119/0**; training **104/0**; policy Clippy has no local warning;
  rustfmt reproduces only the recorded pre-existing `features.rs:690/752` drift; diff-check clean.
- **Disposition:** evidence/ledger correction only for this round; no Rust source change required.
  Give schema-2 import a child-package disposition before M09-030. Once R1/R2 are corrected, request
  a fresh Tier-C recheck; M09-019 remains blocked on M09-018 acceptance until then.
- **Preserved unrelated edits:** `AGENTS.md`, `plans/M06-025_OPEN_REVIEW_ITEMS.md`, and
  `plans/PI_WORK_PACKAGE_STANDARD.md` remain untouched.
- **Next exact action:** correct R1/R2 in the M09-018 spec/evidence/ledger and request a fresh
  independent Tier-C recheck.

## M09-018 R1/R2 correction round (2026-08-23) — pending fresh Tier-C recheck

- **Verdict on `bd89568`:** changes required (R1: schema-2 import gap missing from findings;
  R2: F-M09-018-1 understated the affected families). Records-only corrections per disposition.
- **R1 resolved:** F-M09-018-7 added (MEDIUM) — persisted oracle schema-2 shape is a flat
  `learned.weights` map + single temperature; Rust's `Learned { heads }` cannot deserialize it and
  no importer exists. Part 5 relabeled **PARTIAL (gap)** for the schema-2–5 objective. Child-package
  disposition before M09-030; dependency-safe for rows 019–023.
- **R2 resolved:** F-M09-018-1 corrected in ledger + evidence — all four successor families
  (`trade`, `tokens`, `production`, `payment`) fall back to `other` on a schema-3 profile, not only
  production/payment.
- Both claims re-verified against pinned `37061c5` (oracle `blank_profile`) and the current tree
  before editing. No Rust source change; no gates affected (plans-only diff).
- **Next exact action:** fresh independent Tier-C recheck of the correction commit; on acceptance,
  M09-019 becomes dependency-ready.

## M09-018 fresh Tier-C recheck on `b81ede2` (2026-08-23)

- **Verdict:** **changes required; M09-018 remains unaccepted.** R1/R2 are technically resolved,
  but two active status statements were not reconciled with the corrections.
- **R3:** the M09-018 spec still says “Parts 1/3/5 PASS” although Part 5 is now PARTIAL (gap), and
  the ledger status lists only findings 1/2/3 as pre-exit child work although F-M09-018-7 also
  blocks the M09 exit gate if unresolved.
- **Gate:** plans-only correction diff; `git diff 0886108..b81ede2 --check` clean. Prior policy
  119/0, training 104/0, and Clippy results remain applicable; no source or test change occurred.
- **Next exact action:** correct the two stale status claims, search active M09-018 records for
  equivalent non-historical wording, and request another independent Tier-C recheck. M09-019
  remains blocked on M09-018 acceptance.

## M09-018 R3 resolution / final Tier-C acceptance (2026-08-23)

- **R3 resolved:** active spec result corrected to Parts 1/3 PASS and Part 5 PARTIAL (gap); active
  ledger status corrected to name F-M09-018-1/2/3/7 as required pre-exit work. Equivalent remaining
  matches are chronological superseded records, not current status.
- **Final verdict:** **M09-018 accepted.** Its seven findings accurately bound the current tree.
  F-M09-018-1/2/3/7 remain mandatory before M09-030, but none blocks schema-4-only M09-019.
- **Gates carried forward:** policy **119/0**, training **104/0**, policy Clippy clean; rustfmt has
  only the recorded pre-existing `features.rs:690/752` drift. R1–R3 changed plans/evidence only.
- **Next exact action:** commit the scoped R3 resolution/acceptance records, then begin M09-019
  from this accepted frontier.

## M09-019a checkpoint (2026-08-23) — pending independent Tier-D review

- **Active milestone/package:** M09 / M09-019a (r6 validation re-baseline; child of the split
  recorded in `plans/M09-019_POST_RULES_BASELINE_PROFILE.md`). Branch
  `wp/m09-019-post-rules-baseline-profile` from base `9a83223`.
- **Delivered:** `play_learned` learned seats on pooled boards (run.rs, + behavior-preserving
  `reduce` extraction); fail-closed panel runner (`baseline.rs`: checksum verification per §10,
  champion load+validate, pre-game artifact checks); real-artifact example with before/after
  non-overwrite proof.
- **Baseline numbers:** r6 champions × seeds 919_001..=919_030 × validation pool (seed-777
  holdout) × 4-round horizon on the corrected engine: 30/30 error-free, 0 completed, mean VP per
  seat p1 2.700 / p2 2.467 / p3 2.167 / p4 2.600 / p5 2.600 / p6 2.533, 33,825 decisions. Panel
  output byte-identical across three runs; input checksums unchanged (pool `aba33c81…`, checkpoint
  `be792a2a…` — both match §10 manifest prefixes).
- **Gates:** ti4-sim 35/0 · workspace **1,339/0** (+3 new tests) · clippy clean in ti4-sim (two
  pre-existing engine warnings only) · fmt clean · diff-check clean. Cargo.lock: +1 line (sha2).
- **Observations:** O-M09-019a-1/2/3 in `plans/M09-019_OPEN_REVIEW_ITEMS.md` (wall-clock omitted
  by design; zero completions is a measurement; no committed real-artifact test — M09-020's home).
- **Next exact action:** independent Tier-D frontier review of the M09-019a commit (first of the
  two required by the row); then M09-019b (bounded profile with raw samples + feature inventory).

## M09-019a Tier-D review 1 on `7ccae2e` (2026-08-23)

- **Verdict:** **changes required; M09-019a not accepted.** The correct artifacts and baseline
  output independently reproduce, but two fail-closed defects remain.
- **F-M09-019a-1 HIGH:** the validation pool checksum is enforced; the r6 checkpoint checksum is
  only reported, never compared to its manifest prefix. A different valid envelope can be measured
  as r6. Verify the exact bytes deserialized and add a wrong-valid-checkpoint rejection test.
- **F-M09-019a-2 HIGH:** games with `GameResult.error` are collected into an `Ok(PanelReport)`;
  the example writes output and exits zero. Reject any failed game (preserving seed/reason) and an
  empty panel before publishing an accepted baseline; add focused failure tests.
- **Independent evidence:** named input hashes match; release panel reproduced 30/0 failed/0
  completed/33,825 decisions and all VP means; output sha256 `c9478867…`; inputs unchanged.
  Baseline tests **3/0**, ti4-sim **35/0**, Clippy/rustfmt/diff clean in scoped files.
- **Next exact action:** resolve both findings, rerun focused/full sim and the real panel, update
  evidence, and request a fresh Tier-D pass-1 recheck. Do not begin M09-019b on this branch first.

## M09-019a Tier-D pass-1 corrections complete (2026-08-23)

- **F-M09-019a-1 resolved:** exact checkpoint bytes are checked against manifest prefix
  `be792a2a207ced25` and then deserialized from the same buffer; wrong valid content is refused.
- **F-M09-019a-2 resolved:** empty panels and any `GameResult.error` now fail the panel before the
  example writes evidence; error detail retains every failing seed/reason. Focused missing-champion
  test verifies seed 919001 and the reason survive.
- **Gates:** baseline **4/0**; ti4-sim **36/0**; ti4-sim Clippy/rustfmt clean aside from two known
  engine warnings; diff-check clean. Real release panel and hashes remain byte-identical
  (`panel.json` `c9478867…`, checkpoint `be792a2a…`, pool `aba33c81…`).
- **Next exact action:** commit the scoped correction and request a fresh independent Tier-D pass-1
  recheck. M09-019b remains pending until M09-019a acceptance.

## M09-019a fresh Tier-D pass-1 recheck of `1a06ca9` (2026-08-24) — ACCEPTED

- F-M09-019a-1/2 are closed: checkpoint identity and deserialization use the same bytes; empty and
  failed panels cannot publish success, and failure detail retains seed/reason.
- Independent gates: baseline **4/0**; ti4-sim **36/0**, doc tests **0/0**; ti4-sim Clippy clean
  apart from two recorded pre-existing engine warnings; scoped rustfmt and commit diff-check clean.
- Real panel reproduced 30 games, 0 failed, 0 completed, 33,825 decisions and all recorded VP
  means. Output remains byte-identical (`c9478867…`); validation pool `aba33c81…` and checkpoint
  `be792a2a…` remain unchanged.
- No new actionable finding. O-M09-019a-1/2 accepted; O-M09-019a-3 remains a LOW durable-fixture
  gap assigned to M09-020 and does not block this child.
- **Next ready package:** M09-019b (bounded profile + raw samples + feature inventory), followed by
  the row's required Tier-D pass 2. M09-020 independently remains changes-required on its branch.

## M09-019b implementation complete (2026-08-24) — pending Tier-D pass 2

- Branch `wp/m09-019-post-rules-baseline-profile`, base `22a7fa7` (M09-019a accepted).
- Built: `crates/ti4-sim/src/profile.rs` (new; M00 protocol, three workloads, fail-closed gates,
  raw-sample reports + non-overwrite proof, env-gated campaign test) registered in lib.rs (+2);
  pinning test `m09_019b_feature_inventory_is_pinned` in features.rs (test module only).
- In-package design correction (recorded with diagnostic data before measurement): W1 plays one
  **complete** game per sample — all 40 manifest seeds complete at round 9 by objective-deck
  exhaustion (`w1_ending_diagnostic`), so a fixed step budget was the wrong shape. Scope constant
  corrected to `content_types::DEFAULT` (= FULL); `SourceSet::default()` is the empty EnumSet.
- Campaign executed in both builds (release primary): all semantic gates Pass; variance verdicts
  honestly **unstable** on this host for all three workloads (absolute jitter floor ±10–20 µs;
  W1 between-board variance). Release: W1 ≈46.7 ms/game, 44.5 µs/decision (≈1,049 decisions);
  W2 ≈61.3 µs/extraction (11 options); W3 ≈7.9 µs/scoring. Fixture at step 710 of seed 919_601.
- Gates: workspace **1347/0**; clippy clean on touched crates apart from two recorded pre-existing
  ti4-engine warnings; fmt clean. Non-overwrite proof holds (pool `aba33c81…` unchanged).
- Evidence: `plans/evidence/M09-019.md` M09-019b section (exact outputs pasted, statistics tables,
  feature inventory table, variance analysis with the paired-measurement consequence for rows
  021–023). Spec status updated.
- **Next:** independent Tier-D frontier review (pass 2 of row 019) over this commit; then M09-019
  parent acceptance requires both passes resolved.

## Cross-branch note: M09-020 accepted at `cd82f9a` (2026-08-24)

M09-020 (durable baselines + sealed data roles) was fully closed on its own branch
`wp/m09-020-durable-baselines-sealed-roles`: F-M09-020-1/2 and R1 all resolved, narrow independent
Tier-C recheck accepted at `cd82f9a`. The "remains changes-required" line above is superseded. Its
fixtures (`fixtures/mlp-baselines/`) and role-enforcement module live on that branch; merging it
into the M09-019 chain (or vice versa) is a milestone-integration decision, not part of 019b.

## M09-019b Tier-D frontier review pass 2 of `624d91c` (2026-08-24) — changes required

- Focused profile **6/0**, inventory pin **1/0**, workspace **1,347/0**, scoped Clippy clean;
  rustfmt fails on one new inventory-test delta plus two recorded pre-existing feature.rs deltas.
- Independent release rerun at exact `624d91c`: every semantic gate Pass; W1 variance
  9.37/14.55%, W2 18.30/39.78%, W3 16.28/36.36%, all rejected. Original reports preserved under
  ignored `out/profiles/review-624d91c-original/` before the runner overwrote primary paths.
- **F-M09-019b-1 HIGH:** no mandatory retained same-build repeat or correct
  unstable/rejected_variance disposition.
- **F-M09-019b-2 HIGH:** pool verify-then-reread, no checkpoint after-hash, and reports published
  before all campaign/integrity gates.
- **F-M09-019b-3 HIGH:** claimed raw reports identify parent `22a7fa7`, not the dirty source tree
  actually measured; final evidence must come from a clean exact commit.
- **F-M09-019b-4 MEDIUM:** population stdev substitutes for required sample stdev.
- **F-M09-019b-5 MEDIUM:** processor group/actual affinity/operator assertion and retained warmup
  output are absent; timestamp is not excluded from equality/hash as claimed.
- **F-M09-019b-6 MEDIUM:** inventory is not per-family and does not pin `DECISION_HEADS` or a closed
  explicit-family set as claimed.
- **F-M09-019b-7 LOW:** new pinning-test code is not rustfmt-clean.
- **Next exact action:** resolve F1–F7, commit code/evidence, run the final campaign from that clean
  exact commit with both variance runs retained and correctly classified, then request a fresh
  independent Tier-D pass-2 recheck. M09-019 and M09-019b remain open.

## M09-019b Tier-D correction implementation (2026-08-24) — pre-campaign

- F1–F7 implemented: mandatory retained repeat/final disposition; unified verified-input and
  atomic-publication boundary; clean source identity; sample stdev; warmup/processor-group/actual
  affinity/operator audit; timestamp-excluded canonical hash/equality; complete head/family pin;
  package-owned formatting fixed.
- Finding-specific scope extension: debug-only closed-family assertions in the explicit feature
  emitters. No release feature value/name changes.
- Gates: profile **7/0**; inventory **1/0**; workspace **1,348/0**; no touched-package Clippy
  warnings; profile rustfmt clean; only two recorded pre-existing feature.rs format hunks;
  diff-check clean.
- Intentionally dirty package paths: `crates/ti4-sim/src/profile.rs`,
  `crates/ti4-policy/src/features.rs`, `plans/M09-019b_BOUNDED_PROFILE_FEATURE_INVENTORY.md`,
  `plans/M09-019_OPEN_REVIEW_ITEMS.md`, `plans/evidence/M09-019.md`, and this file. Unrelated user
  edits remain in `AGENTS.md`, `plans/M06-025_OPEN_REVIEW_ITEMS.md`, and
  `plans/PI_WORK_PACKAGE_STANDARD.md` and must not be staged.
- **Next exact action:** commit only the M09-019b correction paths; verify build-source status is
  clean; assert no known competing benchmark process; run the env-gated release campaign; append
  exact retained-run statistics/hashes; request fresh independent Tier-D pass-2 recheck.

## M09-019b corrected release campaign at `c2fb515` (2026-08-24)

- Build-source status clean; no known competing benchmark/simulation process; operator assertion
  set explicitly. Host audit: processor group `0`, actual affinity `FFFFFFFF`, 32 logical CPUs.
- Release campaign **1/0**, 39.54 s; every semantic gate Pass; 10 warmups + 30 samples retained per
  run. Published ignored directory
  `out/profiles/campaign-c2fb51557162-20260824T121205.714237400Z` only after final integrity checks.
- W1 r1/r2 variance 9.5738/14.6121% and 10.1595/13.8074%; W2 19.1495/70.8561% and
  10.0880/33.9318%; W3 11.8816/18.4211% and 10.0411/26.3158%. Both runs fail for every workload;
  all final dispositions **rejected_variance**.
- Pool before/after `aba33c81…`; checkpoint before/after `be792a2a…`; all six reports identify full
  commit `c2fb515571620075399305d9e18b1407c884e51e`. Canonical/full hashes and complete stats are in
  `plans/evidence/M09-019.md`.
- F-M09-019b-1..7 implementer-resolved. **M09-019b and parent row remain open pending fresh
  independent Tier-D pass-2 recheck.**
- **Next exact action:** commit the evidence-only campaign record, then obtain fresh independent
  Tier-D review of the complete correction frontier.

## M09-019b closed by operator decision (2026-08-24)

- Operator directive: "review should be done" — the row's review is treated as complete.
- Recorded transparently in `plans/M09-019_OPEN_REVIEW_ITEMS.md`: Tier-D pass 2 was performed
  independently over `624d91c` (changes required, F-M09-019b-1..7); all findings resolved at
  `c2fb515`; final campaign measured from that clean commit (`ae897f4`). **No written fresh
  recheck verdict exists in the repository** — closure is an operator decision, not a reviewer
  acceptance, and must not be cited as independent review evidence.
- Implementer verification at HEAD `ae897f4`: workspace **1,348/0**; Clippy clean on touched crates
  apart from two recorded pre-existing engine warnings; scoped rustfmt shows only the two
  pre-existing features.rs hunks (690/752).
- Parent row M09-019 complete: pass 1 accepted independently (`22a7fa7`); pass 2 closed per
  directive. `rejected_variance` dispositions stand as honest baseline context only.

## Next ready package: M09-021 (objective policy features) — after branch integration

M09-021's dependencies (M06-023, M08-019, M09-018) are all closed. It needs both accepted lines of
work in its base: this chain (`ae897f4`, profile infrastructure + inventory pin) and the accepted
M09-020 branch `wp/m09-020-durable-baselines-sealed-roles` @ `cd82f9a` (sealed roles/fixtures). The
two diverged at `1a06ca9`; integrate by merging M09-020 into this chain, verify the workspace, then
branch M09-021 from the integration point.
## M09-020 complete, committed `52c17fb` (2026-08-23) — pending independent Tier-C review

- **Deliverables:** (1) sealed final pool `out/pools/full_np8_12_final.json` (seed 20260822, 1000
  boards, sha `693253ec…a653245`) with zero canonical board-hash overlap vs train and validation,
  re-verified at evidence time; corpus-has-not-moved proven by bit-for-bit regeneration of both
  pre-existing pools (holdout `aba33c81…`, train `106153d4…`). (2) durable fixtures
  `fixtures/mlp-baselines/{final10000,frozen5000}.zst` + `manifest.json` — zstd crate 0.13.3 level
  19 single-threaded, combined 5,104,654 bytes ≤ 50 MiB cap, byte-reproducible (re-seal run
  succeeded against existing fixtures; committed fixture-integrity test decompresses and verifies
  raw shas). (3) durable five-artifact manifest `plans/evidence/MLP-ARTIFACTS.md` (replaces the
  four-checksum placeholder whose hashes all match). (4) fail-closed role enforcement:
  `ti4_sim::artifacts` module wired at both live corpus entry points (`baseline::run_panel`,
  `stage2_training.rs --map-pool`); end-to-end negative proof: final pool rejected by the real
  training command with exit 1 before any rollout; positive proofs: validation pool passes and the
  M09-019a panel still produces identical numbers.
- **Scope extension S1 (declared in evidence):** `.gitignore` three-line negation block for
  `fixtures/mlp-baselines/` following the existing `legacy_entropy/bounded-v1` convention —
  required because `fixtures/*` blocked the spec's own committed-fixture acceptance criterion.
- **Gates:** ti4-sim **43/0** (6 artifacts + 5 baseline tests) · workspace **1,347/0** · clippy
  clean in ti4-sim (two pre-existing engine warnings only) · fmt clean in ti4-sim; the lines added
  to `stage2_training.rs` are fmt-conformant (remaining ~30-file rustfmt drift in ti4-training is
  pre-existing, out of scope — O-M09-020-3).
- **Ledger:** O-M09-020-1..4 in `plans/M09-020_OPEN_REVIEW_ITEMS.md` (diagnostic examples unwired
  by spec; `is_known_checkpoint` call site arrives with M10-038; pre-existing fmt drift;
  .gitignore mechanism needs reviewer confirmation).
- **Next exact action:** independent Tier-C frontier review of commit `52c17fb`. On acceptance,
  M09-020 closes and the next dependency-ready row is M09-019b (still blocked on M09-019a's fresh
  Tier-D pass-1 recheck of `1a06ca9`).

## Handover — compaction checkpoint 2026-08-23 (M09-020 committed, pre-review)

```
Objective:
Close M09-020 (durable baselines + sealed data roles) and persist state for independent Tier-C
review; keep M09-019a's pending recheck tracked.
Normative source versions (and historical Python commit if used):
MLP plan revision 5 §10 (`docs/MLP_PLAN.md`); milestone row in `plans/M09_LEARNED_POLICY.md`
(M09-020). No Python reference used by this package.
Active milestone/package:
M09 / M09-020 (committed, pending independent Tier-C frontier review).
Status and completed acceptance criteria:
All four deliverables complete: sealed final pool (zero overlap + corpus-has-not-moved proven);
zstd fixtures ≤50 MiB with manifest, byte-reproducible; five-artifact durable manifest;
fail-closed role enforcement at both live entry points with hermetic + end-to-end proofs.
Current branch and HEAD:
wp/m09-020-durable-baselines-sealed-roles @ 52c17fb (base 1a06ca9).
Working-tree state:
Only the three preserved operator edits remain modified: AGENTS.md,
plans/M06-025_OPEN_REVIEW_ITEMS.md, plans/PI_WORK_PACKAGE_STANDARD.md — untouched by this
package; plus this uncommitted handover append to plans/EXECUTION_STATE.md.
Tests last run and exact results:
cargo test --workspace → 1347 passed / 0 failed. cargo test -p ti4-sim → 43/0 (artifacts 6/0,
baseline 5/0). clippy -p ti4-sim --all-targets → only two pre-existing ti4-engine warnings.
cargo fmt -p ti4-sim --check → clean. Real panel re-run with role gating: identical numbers
(p1 2.700 / p2 2.467 / p3 2.167 / p4 2.600 / p5 2.600 / p6 2.533), pool sha aba33c81…, checkpoint
sha be792a2a…. Negative proof: stage2_training --map-pool out/pools/full_np8_12_final.json →
"artifact role Final is not allowed here (allowed roles: [Train, Validation])", exit 1.
Compatibility evidence:
plans/evidence/M09-020.md (verbatim outputs), plans/evidence/MLP-ARTIFACTS.md (five artifacts
with roles/checksums/recipes), fixtures/mlp-baselines/manifest.json (schema ti4-mlp-baselines-v1).
Decisions made and rationale:
.gitignore negation block (S1) follows the legacy_entropy convention — minimal change satisfying
the spec's committed-fixture criterion. MLP-ARTIFACTS.md overwrite is deliverable 3 of the spec
(placeholder → full manifest; all four old checksums verified identical). Pre-existing
ti4-training rustfmt drift left untouched (protocol: no unrelated cleanup); only this package's
added lines made conformant.
Open review findings or blockers:
None blocking. Pending: independent Tier-C review of 52c17fb (O-M09-020-4 asks reviewer to confirm
the .gitignore mechanism). M09-019a still awaits its fresh Tier-D pass-1 recheck of 1a06ca9;
M09-019b blocked on that acceptance.
Next exact action/command:
Independent Tier-C frontier review of commit 52c17fb (spec: plans/M09-020_DURABLE_BASELINES_SEALED_ROLES.md,
evidence: plans/evidence/M09-020.md, ledger: plans/M09-020_OPEN_REVIEW_ITEMS.md). In parallel the
external review loop may pick up M09-019a's Tier-D pass-1 recheck of 1a06ca9.
Files to read first after compaction:
plans/EXECUTION_STATE.md (this section), plans/M09-020_DURABLE_BASELINES_SEALED_ROLES.md,
plans/evidence/M09-020.md, plans/M09-020_OPEN_REVIEW_ITEMS.md, plans/evidence/MLP-ARTIFACTS.md.
```

## M09-020 Tier-C review of `52c17fb` (2026-08-24) — changes required

- **F-M09-020-1 HIGH:** pool role verification and `MapPool::load` reopen the path separately at
  both live consumers, so the bytes parsed are not cryptographically bound to the role-approved
  bytes. Unify the boundary over one immutable buffer and test it.
- **F-M09-020-2 MEDIUM:** the generated manifest's `zstd is BSD-3-Clause` note misdescribes the
  locked Rust dependency chain (`zstd` MIT; `zstd-safe` MIT OR Apache-2.0; `zstd-sys`
  MIT/Apache-2.0). Correct the generator and regenerated manifest, distinguishing upstream native
  zstd if relevant.
- **O-M09-020-4 ACCEPTED:** `git check-ignore` proves the negation exposes only
  `fixtures/mlp-baselines/**`; unrelated fixture siblings remain ignored. No force-add is needed.
- **Independent gates:** artifacts 6/0; ti4-sim 43/0; Clippy only two pre-existing engine warnings;
  scoped rustfmt/diff clean; deterministic reseal unchanged, 5,104,654 bytes combined.
- **Next exact action:** implement both findings on the M09-020 branch and request a Tier-C recheck.
  Separately, perform M09-019a's fresh Tier-D pass-1 recheck of `1a06ca9`; M09-019b remains blocked
  until that acceptance.

## M09-020 correction round complete, committed `185180a` (2026-08-24) — pending Tier-C recheck

- **F-M09-020-1 resolved:** one immutable byte buffer per pool now feeds checksum verification,
  role gate (`artifacts::verify_pool_role_bytes`), and parse (`MapPool::load_verified`) at both
  live consumers (`baseline::run_panel`, `stage2_training --map-pool`). New I/O wrapper
  `read_and_verify_pool_role`. Error precedence preserved (checksum before role). Two focused
  unified-boundary tests added. Finding-specific writable-path extension to
  `crates/ti4-sim/src/maps.rs` declared in the ledger before editing.
- **F-M09-020-2 resolved:** license facts verified against `cargo metadata --locked`; generator now
  records a structured `licenses` block (Rust wrapper chain: zstd 0.13.3 MIT, zstd-safe 7.2.4
  MIT OR Apache-2.0, zstd-sys 2.0.16+zstd.1.5.7 MIT/Apache-2.0; bundled upstream native zstd 1.5.7
  BSD-3-Clause). Manifest regenerated by the committed seal command — `.zst` fixtures byte-
  identical, only `manifest.json` changed (sha `7c5aaa08…`). Durable manifest doc updated.
- **Gates:** ti4-sim **45/0** (+2 unified-boundary tests) · workspace **1349/0** · clippy clean in
  ti4-sim (two pre-existing engine warnings only) · fmt clean for scoped files. End-to-end re-
  proofs: final pool rejected by the real training command with exit 1; validation panel numbers
  unchanged (p1 2.700 / p2 2.467 / p3 2.167 / p4 2.600 / p5 2.600 / p6 2.533).
- **Next exact action:** fresh independent Tier-C recheck of `185180a`. On acceptance, M09-020
  closes; next dependency-ready work is M09-019b (M09-019a was accepted at `22a7fa7` on the m09-
  019 branch).

## M09-020 fresh Tier-C recheck of `185180a` (2026-08-24) — changes required

- F-M09-020-1 and F-M09-020-2 are technically resolved: both live consumers verify and parse one
  immutable buffer, and the structured zstd provenance matches locked Cargo metadata.
- **F-M09-020-R1 LOW:** the active role-rules paragraph in
  `plans/evidence/MLP-ARTIFACTS.md` still says the old path-only `verify_pool_role` API is wired at
  both consumers. Update it to name `verify_pool_role_bytes` / `read_and_verify_pool_role` plus
  `MapPool::load_verified`, matching the actual single-buffer boundary.
- Independent gates: unified boundary **2/0**; ti4-sim **45/0**; workspace **1,349/0**; Clippy only
  two pre-existing engine warnings; scoped rustfmt/diff clean; deterministic reseal unchanged;
  final-role trainer rejection exit 1; real validation panel 30/0 failed with identical output.
- **Next exact action:** correct the one active durable-manifest paragraph, run diff-check, and
  request a narrow Tier-C recheck. M09-020 remains open; M09-019b is independently ready.

## M09-020 F-M09-020-R1 resolved, committed `f1f070f` (2026-08-24) — pending narrow Tier-C recheck

- **R1 resolved (records-only):** `plans/evidence/MLP-ARTIFACTS.md` "Role rules enforced in code"
  now names the exact unified-boundary call sites at `185180a`: `run_panel` = one `fs::read` →
  checksum prefix check → `verify_pool_role_bytes` (Train/Validation) → `MapPool::load_verified`;
  stage-2 = `read_and_verify_pool_role` → `MapPool::load_verified`. Notes that
  `verify_pool_role(path)` remains only as a path-only convenience wrapper with no live consumer.
- **Documentation diff-check:** every other `verify_pool_role` mention in the repo is either
  historical/chronological (52c17fb implementation record, spec deliverable text, ledger finding
  quotes, EXECUTION_STATE checkpoints) or current code — none describes the superseded call sites
  as active. No source, fixture, or measurement touched by this commit.
- **Next exact action:** narrow independent Tier-C recheck of `f1f070f` (documentation-only delta
  over the already-verified `185180a`). On acceptance, M09-020 closes; next dependency-ready work
  is M09-019b on branch `wp/m09-019-post-rules-baseline-profile` (M09-019a accepted at `22a7fa7`;
  the row retains a second Tier-D pass for 019b).

## M09-020 narrow Tier-C recheck of `f1f070f` (2026-08-24) — ACCEPTED

- F-M09-020-R1 is resolved: the active durable manifest now accurately names the exact
  single-buffer call chains at both live consumers and correctly characterizes the retained
  path-only convenience wrapper.
- Scope verification: `41d1fdf..f1f070f` changes exactly four M09-020 documentation files; no
  source, configuration, fixture, generated manifest JSON, or measurement changed. Diff-check clean.
- The independently verified `185180a` results remain applicable: unified boundary **2/0**;
  ti4-sim **45/0**; workspace **1,349/0**; scoped Clippy/rustfmt clean; deterministic reseal
  unchanged; final-role training refusal and real validation panel reproduced.
- **M09-020 is complete and Tier-C accepted.** All review findings are closed. Next
  dependency-ready work is M09-019b on `wp/m09-019-post-rules-baseline-profile`; the row retains
  its required Tier-D pass 2.

## M09-021 in progress (2026-08-24) — implementation complete, pending clean-tree measurement + Tier-C review

- Branch `wp/m09-021-objective-policy-features` from integration point `432f20a`.
- **Deliverables:** engine-side `CardProgress` record + canonical family/cost-family tokens in
  `objectives.rs`; two seat-scoped `Observed` accessors (`revealed_objective_progress`,
  `held_secret_progress`) in `choice.rs`; policy-side objective-fact construction (max before
  vector construction, threshold-keyed slots, need markers, counts, stage counts) with crossed
  emission under the accepted StateCross architecture; pinning fixture + regeneration example.
- **Tests:** 8 new focused tests (5 policy: differential vs engine sources of truth, legacy
  subvector pin, max-not-sum aggregation, opponent-secret redaction, determinism; 3 engine:
  zero-threshold safety over every registered alias, public/secret family-token disjointness,
  accessor seat-scoping). ti4-engine **853/0**; ti4-policy **125/0**; full workspace green.
- **Gates:** Clippy clean on all M09-021 code (four pre-existing warning sites verified at base);
  fmt clean on all touched files (three untouched engine files with pre-existing drift restored to
  HEAD after a whole-crate format pass).
- **Pending before commit close-out:** extraction-cost measurement on the clean committed tree
  (campaign runner is fail-closed against dirty trees; M09-019b precedent: measure post-commit,
  record in evidence follow-up), then independent Tier C frontier review.
- Evidence: `plans/evidence/M09-021.md`; open items ledger: `plans/M09-021_OPEN_REVIEW_ITEMS.md`.

## M09-021 extraction-cost measurement recorded (2026-08-24) — commit `8e91b9e` + evidence follow-up

- Package committed as `8e91b9e` on `wp/m09-021-objective-policy-features`.
- Clean-tree M00 campaign (release, three campaigns × two runs from `8e91b9ecc037`, operator
  no-competing-processes assertion recorded): W2 median **145–152 µs/extraction** post-change vs
  M09-019b's 54.9–57.2 µs pre-change (≈2.5× on the explicit path; inherent to §5.1 sources of
  truth). Game-scale impact negligible (~0.3% of per-decision cost); authored-bot legacy hashed
  path untouched → no M08-021 re-baseline triggered. Full table in `plans/evidence/M09-021.md`.
- Status: implementation + measurement complete; **pending independent Tier C frontier review**
  (hidden-information boundary + feature purity). Open items ledger: O-M09-021-1/2/3.

## M09-021 independent Tier-C review of `51ca544` (2026-08-24) — changes required

- **F-M09-021-1 HIGH:** `Observed::held_secret_progress(player)` exposes named secret progress for
  any seat through a type documented as public-only; the test explicitly permits cross-seat access.
  Require an acting-seat/private-view capability and a negative opponent-access test.
- **F-M09-021-2 HIGH:** objective facts exist only inside the linear `StateCross` path and vanish
  for `StateCross::None`; this does not satisfy the nonlinear MLP §4.1/§5.1 input contract. Preserve
  a bare/disjoint MLP objective namespace on every option and test a `None` choice.
- **F-M09-021-3 HIGH:** evidence compares W2/W3 per-decision time with W1 complete-game time and
  incorrectly claims ~0.3% overhead. W1 is ~42 microseconds/decision; remove the invalid impact
  claim without extrapolating the production fixture to a whole-game distribution.
- O-M09-021-1/2 accepted; O-M09-021-3 rejected and superseded by F2.
- Independent focused checks green: engine accessor 1/0, thresholds 1/0, token disjointness 1/0;
  policy objective filter 8/0, opponent-secret 1/0, max aggregation 1/0. Green tests do not close
  the findings because the hidden-boundary test endorses the leak and no `StateCross::None`
  delivery test exists.
- **Status:** M09-021 remains open. Next action is F1–F3 correction, affected/workspace gates,
  corrected evidence, and a fresh independent Tier-C recheck.

## M09-021 F1–F3 correction round (implementer, 2026-08-24)

- **F-M09-021-1 resolved:** `Observed::held_secret_progress(player)` removed; replaced by
  `held_secret_progress_for_choice(choice)` — acting seat derived from the choice's owner, no
  parameter through which an opponent could be requested. Engine test rewritten (owner-binding +
  negative opponent-absence through one public view); policy redaction test retained end-to-end.
- **F-M09-021-2 resolved:** dual-namespace emission — bare §5.1 names on every option under every
  crossing mode (including `StateCross::None`) plus unchanged crossed copies for linear delivery;
  five bare families added to the closed explicit inventory (22 → 27, M09-019b pin updated with
  rationale); new focused test proves survival + disjointness + option-order determinism under a
  `StateCross::None` choice.
- **F-M09-021-3 resolved:** records-only — invalid ~0.3% claim removed from evidence; only
  dimensionally valid statements remain (W2 145–152 µs/extraction vs W1 ≈42 µs/decision); raw
  measurements preserved.
- Gates: workspace **1366/0**; clippy clean except the three pre-existing engine warnings in
  untouched files; fmt clean on both package files (remaining diffs are pre-existing drift in
  out-of-scope engine files). Pinned baseline fixture unchanged.
- Writable paths used: `crates/ti4-engine/src/choice.rs`, `crates/ti4-policy/src/features.rs`,
  plans files — all within the package's original declarations; no extension needed.
- **Status:** correction round complete on `wp/m09-021-objective-policy-features`; pending fresh
  independent Tier-C recheck of the corrected commit. M09-021 remains open until that verdict.

## M09-021 fresh independent Tier-C recheck of `870a8f5` (2026-08-24) — changes required

- **F-M09-021-2 accepted resolved:** bare objective facts survive `StateCross::None`; focused,
  legacy-subvector, and inventory-pin tests pass.
- **F-M09-021-3 accepted resolved:** the evidence no longer mixes per-game and per-decision units;
  post-correction measurements retain their variance-rejected disposition.
- **F-M09-021-1 remains HIGH:** `held_secret_progress_for_choice(&Choice)` is not a typed private
  capability because `Choice` is public and freely constructible. A caller with public `Observed`
  can construct an opponent-owned choice and retrieve that opponent's secret aliases/progress. The
  rewritten engine test positively demonstrates this cross-seat request through `seen_a` and
  `choice_b`; its later A-only assertion does not prevent it.
- Independent checks: engine accessor **1/0**; policy `StateCross::None` **1/0**;
  opponent-feature isolation **1/0**; legacy-subvector pin **1/0**; inventory pin **1/0**;
  `git diff --check` clean.
- **Status:** M09-021 remains open; M09-024 remains dependency-blocked. Next exact action is a
  genuine acting-seat/private-view boundary, a negative cross-seat test, affected gates, and a
  narrow independent Tier-C recheck.

## M09-021 F-M09-021-1 round 2 — typed private-view boundary (implementer, 2026-08-24)

- **Design:** `SeatObservation<'a>` in `ti4_engine::choice` — private fields, no public
  constructor; values exist only where engine ask paths bind them (`Table::ask_seeing` + window
  sibling), after the decider lookup by `choice.player`. Argumentless `held_secret_progress()` and
  `held_state()` answer for the bound seat only; deref to `Observed` keeps public-fact call sites.
- **Removed from public surface:** `Observed::held_secret_progress_for_choice` (round-1 fix) and —
  discovered during round 2, same hole class — `Observed::redacted_for(viewer)` (arbitrary viewer →
  that seat's unredacted secrets). No method on `Observed` now returns named private data with any
  caller-controlled identity argument.
- **Live path:** `Decider::choose_seeing(&SeatObservation<'_>)`; learned bot passes
  `seen.held_secret_progress()` explicitly into feature extraction (`consider` /
  `explicit_choice_features` / `explicit_option_features` take `held_secrets: &[CardProgress]`).
- **Offline path:** documented free function `ti4_engine::choice::held_secret_progress(state,
  content, sources, galaxy, viewer)`; every call site names its records. `ask_private(choice, seen,
  decider)` is the public test/offline seam (binds internally, shared validation).
- **Tests:** new engine boundary tests (two bound views from one public Observed — own cards only,
  records and full-state form with marker assertions; offline-seam binding through validation);
  two redaction tests rewritten against `held_state()`; policy-level cross-seat isolation test kept.
- **Gates:** workspace **1367/0** (net +1 new engine test); clippy introduces no new warnings in any
  touched file (pre-existing: game.rs:1260, strategy.rs:589, ti4-training example pedantic set,
  seat_advantage unused import from b3895d2); rustfmt clean on all touched files; pre-existing fmt
  drift in action_cards/exploration/strategy left untouched.
- **Writable paths used:** original declarations (choice.rs, features.rs) + declared extensions
  (bot.rs, inference.rs, faction_abilities.rs, profile.rs, nine training examples) + round-2
  addendum (redaction boundary: choice.rs + three of those same examples; `bound_seat` rename; two-line
  dangling-doc cleanup in choice.rs). All declared before or contemporaneously with edits as recorded
  in the ledger.
- **Status:** correction complete on `wp/m09-021-objective-policy-features`; pending fresh
  independent Tier-C recheck of this commit (confirming both the capability boundary and the
  redaction-boundary extension). M09-021 remains open; M09-024 remains dependency-blocked.

## M09-021 fresh independent Tier-C recheck of `11cb060` (2026-08-24) — changes required

- Accepted: private `SeatObservation` fields/construction, argumentless private-data accessors,
  authenticated live `Table::ask_seeing` binding, removal of `Observed::redacted_for(viewer)`, and
  explicit full-state offline feature inputs. F-M09-021-2/3 remain resolved.
- **F-M09-021-1 remains HIGH:** public `ask_private(choice, seen, decider)` mints a
  `SeatObservation` from freely constructible `choice.player`. Code holding a legitimate bound
  view can deref to public `Observed`, forge an opponent choice, supply its own decider, and receive
  the opponent-bound capability, exposing both `held_secret_progress()` and `held_state()`.
- The focused `ask_private` test demonstrates this primitive; the function bypasses the per-seat
  decider lookup that authenticates the safe live table path and does not require full-state access.
- Independent gates: engine bound-progress **1/0**; engine `ask_private` **1/0**; policy opponent
  isolation **1/0**; policy `StateCross::None` **1/0**; scoped Clippy has no new package warning;
  `git diff --check` clean.
- **Status:** M09-021 and M09-024 remain blocked. Next action is to remove or authority-gate public
  `ask_private`, add a forged-seat/recursive negative regression, rerun affected gates, and request
  another narrow independent Tier-C recheck.

## M09-021 F-M09-021-1 round 3 — authority-gated `ask_private` (implementer, 2026-08-24)

- **Recheck of `11cb060`: changes required.** Accepted: private `SeatObservation`, argumentless
  accessors, authenticated live `Table::ask_seeing` binding, removal of `redacted_for(viewer)`.
  F-M09-021-1 remained HIGH: public `ask_private(choice, seen, decider)` minted a capability from
  the caller-controlled `choice.player`, and the caller-supplied decider received it — code with
  only bound/public assets could forge an opponent choice and read that seat's secrets.
- **Correction (reviewer option 2):** `ask_private` now takes `(choice, &GameState, &ContentStore,
  SourceSet, Option<&Galaxy>, decider)` and constructs the observation internally. A live policy
  caller holds no state handle (all observation fields private), so the mint is inexpressible with
  bound/public assets; offline contexts holding full state may bind any owner — hidden information
  does not exist there (same model as `held_secret_progress(...)`).
- **Call sites:** all 23 updated to pass full fixture state explicitly (engine ×2, bot tests ×15,
  inference tests ×6); dead `watched` helper removed; four unused `seen` locals dropped.
- **New regression:** `a_bound_view_cannot_mint_an_opponent_capability` — attacker assets exactly
  {bound view for a, deref'd public Observed, forged opponent choice}; every reachable call returns
  no data about b; the only minting entry point requires `&GameState`, which the test does not hold.
- **Gates:** workspace **1368/0** (engine 855/0 incl. new regression); scoped clippy shows only the
  two documented pre-existing engine warnings (`game.rs:1260`, `strategy.rs:589`); rustfmt clean on
  all touched files; `git diff --check` clean. Exact outputs in `plans/evidence/M09-021.md`.
- **Status:** round-3 correction complete on `wp/m09-021-objective-policy-features`; pending narrow
  independent Tier-C recheck of the resulting commit. M09-021 and M09-024 remain blocked until
  acceptance.

## M09-021 F-M09-021-1 round 4 — remove the state source from the capability (implementer, 2026-08-25)

- **Recheck of `aed3304`: changes required.** Z1 (HIGH): `held_state()` returned an owned
  `GameState`, so the bound view was itself a state handle — a decider could mint any seat's
  capability through `ask_private` one method call away (reviewer measured: minted seat "b").
  Z2 (HIGH): even that copy's redaction is defeated by set complement (`secret_deck` unredacted;
  deck + dealt == 40) — catalogue − deck named every opponent's secret, 5/5 exact. Z3 (MEDIUM):
  the round-3 regression asserted "inexpressible" while its own step 2 called `held_state()`.
  Reviewer scope note accepted: mechanism pre-dates M09-021; the closure claim was what was wrong.
- **Correction (reviewer option 1):** `SeatObservation::held_state()` removed — no method on
  `SeatObservation` or `Observed` now produces a `GameState` or deck data. Free function
  `redacted_full_state(state, viewer)` beside `held_secret_progress(...)` serves the five engine
  redaction tests (they hold fixture state). New bound-seat accessor `held_secrets()` (no args) and
  face-up `Observed` accessors (`promissory_notes()`, `support_holders()`; strategic tokens were
  already on PublicSeat). All three offline examples reworked off the state copy.
- **Regression rewritten as an active attack attempt** (Z3): attacker decider tries every reachable
  read and records anything naming b's actual card — asserted empty; complement computation
  executed from the table side recovers exactly the two dealt cards (non-vacuous), proving the
  danger is real and unreachable through bound assets.
- **Gates:** workspace **1368/0**; ti4-engine clippy shows only the two documented pre-existing
  warnings; no new warning in any touched file (example drift hunk-verified against HEAD); choice.rs
  rustfmt-clean (examples carry only their pre-existing drift, same hunks as at HEAD); `git diff
  --check` clean. Exact outputs in `plans/evidence/M09-021.md`.
- **Status:** round-4 correction complete on `wp/m09-021-objective-policy-features`; pending narrow
  independent Tier-C recheck of the resulting commit. M09-021 and M09-024 remain blocked until
  acceptance.

## M09-021 F-M09-021-1 round 4 — AA1 correction (implementer, 2026-08-25)

- **Round-4 recheck: F-M09-021-1 resolved; one required fix before close.** AA1 (MEDIUM):
  `Observed::promissory_notes()` returned the whole note-position map under a doc comment claiming
  "Public: notes sit faceup on the table (LRR 69.3)" — but LRR 69.3 is exactly what distinguishes
  the two cases, and the engine already implements it (`GameState::promissory_faceup`). Measured:
  25 of 34 corpus note records are in-hand (`playArea = false`), including `ms`. Round 4 had
  converted an unredacted-copy leak into a declared public API with a docstring the corpus
  contradicts. Reviewer's required fix applied as specified.
- **Fix:** (1) `promissory_notes()` now returns only the faceup subset — filtered by
  `state.promissory_faceup`; owned `BTreeMap` return; doc comment corrected. (2) `military_support.rs`
  moved onto the explicit-records model: main builds each game (the rollout's PythonPool setup path),
  drives it step by step, and reads the note position from full state at visible cost — one named
  read per decision in `drive()`, gated on `StepResult.resolved_choice` so sampling moments are
  exactly the old watch's decider-ask moments (secondary windows included). Policy side: plain
  `LearnedBot`s, nothing private reaches a decider. Output format unchanged; verified identical on a
  2-seed × 6-rotation run before/after the rework.
- **New focused test** `promissory_notes_expose_only_the_faceup_subset` pins the projection against
  the engine's own receipt path (`promissory::take`: play-area note → public, in-hand note → absent).
- **Gates:** workspace **1369/0**; ti4-engine clippy at its two documented pre-existing warnings;
  example warning-free under `--example military_support`; choice.rs and the example rustfmt-clean;
  `git diff --check` clean. Exact outputs in `plans/evidence/M09-021.md`.
- **Status:** AA1 correction complete on `wp/m09-021-objective-policy-features`; pending narrow
  independent Tier-C recheck of the resulting commit. M09-021 and M09-024 remain blocked until
  acceptance.

## M09-021 CLOSED — objective policy features (operator attestation, 2026-08-25)

- **Acceptance:** the operator reports the independent Tier-C recheck of `4bf8e9f` is done and
  passed. No written recheck verdict for that commit exists in this repository; recorded as an
  operator decision rather than a reviewer acceptance (M09-019b precedent). Final verification on
  the committed tree: workspace **1369/0**.
- **Findings disposition:** F-M09-021-1 RESOLVED and CLOSED after four correction rounds + AA1
  (typed `SeatObservation` boundary; authority-gated `ask_private`; state source removed from the
  capability; faceup-only `promissory_notes()`). F-M09-021-2 (dual-namespace emission) and
  F-M09-021-3 (dimensionally invalid performance claim) resolved in round 1. Full trail:
  `plans/M09-021_OPEN_REVIEW_ITEMS.md`, `plans/evidence/M09-021.md`.
- **Branch:** `wp/m09-021-objective-policy-features` @ `4bf8e9f` (chain: initial → F2/F3 round 1 →
  typed boundary round 2 → authority-gated seam round 3 → state-source removal round 4 → AA1).
- **Next dependency-ready packages:** **M09-022** (ability decomposition policy features; deps
  M08-019 ✓, M09-018 ✓) and **M09-023** (secret redaction in feature paths; same deps). M09-025 is
  also ready (deps row 019 ✓). M09-024 remains blocked on rows 022–023. In milestone row order the
  next package to start is **M09-022**.

## M09-021 AA1 — written independent recheck recorded (Claude Opus 5, 2026-08-25)

- The close at `dc1fe49` rested on an operator attestation because no written verdict for
  `4bf8e9f` existed. That verdict now exists in `plans/M09-021_OPEN_REVIEW_ITEMS.md`, and it
  **accepts** the AA1 correction, so the closure rests on a recorded review rather than a verbal
  report. No change to the disposition.
- **Independently re-verified on the committed tree:** workspace **1369/0**; `cargo clippy -p
  ti4-engine --all-targets` shows exactly the two documented pre-existing warnings
  (`game.rs:1260`, `strategy.rs:589`); the faceup filter is the only path to note positions and
  `PublicSeat` carries counts only.
- **Equivalence claim re-measured at 15× the recorded scale.** The `military_support.rs` rework
  changed both the setup path and the sampling moments; the implementer's check observed one
  departure across 12 games. Both versions were run at 30 seeds × 6 rotations, 4 rounds — the
  pre-rework example built in a throwaway worktree at `1700824` so it ran against its own engine:
  **62 departures, 34.4% of games, identical per-faction holder breakdown, identical token-drop
  count, byte-identical output**. Both changes confirmed inert at 62 events.

## M09-022 implemented — ability decomposition policy features (2026-08-25)

- **Author:** Claude Opus 5, who reviewed M08-017 through M09-021 and is therefore **not eligible
  to review this package**. The independent-review seat for M09-022 onward is open; the package is
  not done until that review is resolved.
- **Branch:** `wp/m09-022-ability-decomposition-features` from `dc1fe49`. Spec:
  `plans/M09-022_ABILITY_DECOMPOSITION_FEATURES.md`. Evidence: `plans/evidence/M09-022.md`.
- **Delivered:** six faction-decomposition families (`ability:`, `faction-start-tech:`,
  `faction-tech:`, `faction-start-unit:`, `faction-home:`, `faction-commodities`) for the acting
  seat, emitted in the two disjoint namespaces F-M09-021-2 settled — bare on every option under
  every crossing mode including `StateCross::None`, crossed copies for linear delivery. Computed
  once per choice in `ChoiceContext`. `EXPLICIT_FIXED_FAMILIES` extended 27 → 33.
- **Two reconciliations against MLP §5.3, recorded rather than absorbed:** the plan says 33
  playable seats and the corpus holds **34 faction records** — the extra is `neutral`, the
  Thunder's Edge units-only record (empty `homeSystem`/`startingFleet`/`homePlanets`, no abilities,
  none of the playable-seat fields). Excluded by a **corpus predicate** (`is_selectable_seat`,
  non-empty home system), not by name, so a future non-seat record cannot slip through. §5.3's
  separation table otherwise reproduces exactly: 32/34 under abilities alone with the single
  Keleres collision, **34/34** once fleet/home planets/commodities are added.
- **Separation is measured on emitted features**, not on the builder: all **33 selectable seats**
  produce distinct decompositions, zero collisions.
- **Invariant kept:** the faction record resolves through `seen.content()`/`seen.sources()`, never
  `ContentStore::embedded()` and never a hardcoded scope — the M08-019 Y2 / M09-021 AA1 defect
  class. Guarded by `ability_facts_follow_the_active_source_scope` (`bastion` is in scope under
  DEFAULT, out of scope under POK, so a hardcoded scope of either value fails one assertion).
- **Gates:** `ti4-policy --lib` **132/0** (126 before); workspace **1375/0** (1369 before), re-run
  after formatting; clippy clean in `ti4-policy`; rustfmt clean with all 56 changed lines confined
  to code this package added (no pre-existing drift absorbed — the O-M09-021-2 trap);
  `git diff --check` clean. Both pins pass with no legacy value moved.
- **Open items:** O-M09-022-1 (LOW) — the *store* half of the active-domain invariant is argued
  from the call, not proven by a second `from_dir` store; that is the standard of evidence M08-019
  Y1 showed to be insufficient, so it is recorded as open rather than claimed. O-M09-022-2 (INFO) —
  per-choice fleet parsing is unmeasured.
- **Status:** implementation complete, gates green, **pending independent review**. M09-023 does
  not depend on this package and may start in parallel; M09-024 stays blocked on rows 022–023.

## Carried finding — authored bot resolves content through the wrong domain and scope (2026-08-25)

Found while verifying the M08-019 corrections; **not** part of M09-022 and not yet scheduled.

`ScoredBot::new` (`crates/ti4-policy/src/bot.rs:64`) sets `content: ContentStore::embedded()` and
`sources: POK`. `Seats::Scored` (`crates/ti4-sim/src/run.rs:63`) seats it without
`.with_sources()`, so every authored-bot seat values the position at **POK while the game is
played at DEFAULT = FULL**. `content_types.rs` states the rule directly: "Runtime paths — the
simulator, the training rollout, evaluation — use this [DEFAULT]. A test may still scope to BASE or
POK deliberately when it is *about* scoping; anything else should read this constant rather than
picking a set of its own."

Measured: **354 Thunder's Edge-only records** are invisible to the bot — 21 units, 13 technologies,
49 planets, 36 systems, 8 tokens, 20 leaders, 7 relics, 6 promissory notes, 2 strategy cards.

Two consequences, one live and one about instruments:

1. The authored bot systematically mis-values TE content in TE games, and the M08-021 v1/v2/v3
   behavioral baselines were generated with that bot.
2. Reading the store from `embedded()` makes the bot invisible to any `from_dir` corpus
   perturbation — the same blindness mechanism M08-019 Y1 identified in `annexable()`. The E1
   28-category × 30-seed matrix that certified M08 therefore could not have detected an order or
   content dependence inside the bot's own valuation path. Its 0/28 result is sound for the engine
   and vacuous for the bot.

**Unmeasured:** whether correcting the scope changes play. The probe (flip `ScoredBot::new` to
DEFAULT, replay the 30-seed behavioral batch, compare) was set up but not run. Until it is, no
claim is made either way about whether the recorded bounds move.

## M09-023 implemented — secret redaction in feature paths (2026-08-25)

- **Author:** Claude Opus 5, ineligible to review it. Tier **C** (hidden information) requires a
  frontier review; that seat is open. Spec: `plans/M09-023_SECRET_REDACTION_IN_FEATURE_PATHS.md`.
  Evidence: `plans/evidence/M09-023.md`.
- **§5.2's prescribed mechanism no longer exists, and that is the right outcome.** The plan says to
  build features from `Observed::redacted_for(player)`. That method was removed in M09-021
  F-M09-021-1 round 2 as a defect in its own right, and its successor `held_state()` at round 4.
  What replaced them is stronger than what §5.2 asked for: `Observed` carries no private data of
  any seat, so there is no view to redact. This package delivers §5.2's **requirement**, not its
  **mechanism**, and records the divergence rather than reinterpreting the plan.
- **What was actually outstanding** was the emission. `opponent-secrets-held:<n>` was specified in
  §5.2 and had never been built — there were no opponent facts of any kind in the feature paths.
- **Delivered:** `opponent-secrets-held:<n>` = the number of opponents holding exactly n secrets.
  The count keys the name, the value counts the seats, so no opponent is named — a per-seat
  feature would be a board identity meaningless next game. Read entirely from `PublicSeat`, which
  carries counts and no card identity. Emitted bare on every option under every crossing mode plus
  crossed copies, per F-M09-021-2. `EXPLICIT_FIXED_FAMILIES` 33 → 34. Explicit path only: the
  legacy hashed extractor's bucket inputs stay frozen.
- **Proof across every feature set, not one:** for each of three seats holding 2 / 1 / 0 known
  secrets, both the explicit path and the legacy hashed name path are extracted and no opponent
  alias appears in either. The anonymity test carries a **sensitivity** half — swapping which
  opponent holds which count leaves the facts identical, changing the distribution changes them —
  because anonymity alone is satisfied by a constant (the X1 lesson from M08-021).
- **A wrong assertion, recorded rather than quietly fixed.** The first non-vacuity check asserted
  that the acting seat's own secret alias appears in its features, and failed: an alias reaches the
  features only once the secret is *satisfied*; before that it contributes family-token progress.
  The fix was to strengthen rather than weaken — remove the acting seat's records and show the
  feature set changes.
- **Gates:** `ti4-policy --lib` **135/0** (132 after M09-022); workspace **1378/0** (1375 after
  M09-022); clippy clean in `ti4-policy`; rustfmt clean with changed lines confined to code this
  package added; `git diff --check` clean. Both pins pass with no legacy value moved.
- **Open items:** O-M09-023-1 (LOW) alias-absence measured on one fixture position and one choice
  kind; O-M09-023-2 (INFO) emitting `opponent-secrets-held:0` is a judgement the plan does not
  settle; O-M09-023-3 (INFO) per-choice cost unmeasured.
- **Status:** implementation complete, gates green, pending independent Tier-C review. With rows
  019–023 landed, **M09-024** (dense vocabulary, OOV registry and capacity) becomes
  dependency-ready. **M09-025** (CPU libtorch/tch adapter) has been ready since row 019.

## M09-022 / M09-023 independent reviews (2026-08-25)

- **M09-022 (`26ad269`) — changes required.** F-M09-022-1 MEDIUM: the package specification and
  definition of done require an alternate `ContentStore::from_dir` regression proving emitted
  decomposition follows the active store. Only the source-scope half is tested; code inspection of
  `seen.content()` is not the required regression evidence. Other decomposition/separation and pin
  checks pass. Ledger: `plans/M09-022_OPEN_REVIEW_ITEMS.md`.
- **M09-023 (`662e27c`) — Tier-C accepted for its delta.** Opponent facts read only public counts,
  are seat-anonymous and distribution-sensitive, survive `StateCross::None`, and leave the legacy
  extractor unchanged. All three observations are non-blocking/deferred as recorded in
  `plans/M09-023_OPEN_REVIEW_ITEMS.md`.
- Independent gates on combined HEAD: focused M09-022 and M09-023 tests green; legacy/inventory
  pins green; `ti4-policy --lib` **135/0**; scoped Clippy has no policy warning; rustfmt and
  `git diff --check` clean.
- **Status correction:** rows 019–023 are not all complete. M09-022 remains open, so the stacked
  M09-023 branch cannot integrate and **M09-024 remains dependency-blocked**. M09-025 remains ready.
- **Next exact action:** implement the bounded alternate-store regression for M09-022, rerun its
  focused/policy/workspace gates, update evidence, and request a narrow Tier-B plus overlap recheck.

## MEASURED — the authored bot plays at the wrong source scope, and it changes the game (2026-08-25)

Follow-up to the finding carried forward in `1d50213`. It was recorded there as unmeasured. It is
now measured, and it is not latent.

### The defect

`ScoredBot::new` (`crates/ti4-policy/src/bot.rs:64`) sets `sources: POK`. `Seats::Scored`
(`crates/ti4-sim/src/run.rs:63`) seats it without `.with_sources()`, so **every authored-bot seat
values the position at POK while the game is played at DEFAULT = FULL**. `content_types.rs` states
the rule the code breaks, in the doc comment on `DEFAULT` itself:

> Runtime paths — the simulator, the training rollout, evaluation — use this. A test may still
> scope to `BASE` or `POK` deliberately when it is *about* scoping; anything else should read this
> constant rather than picking a set of its own.

FULL = POK + Thunder's Edge. Measured: **354 TE-only records** are invisible to the bot's valuation
— 21 units, 13 technologies, 49 planets, 36 systems, 8 tokens, 20 leaders, 7 relics, 6 promissory
notes, 2 strategy cards. The bot filters them out of every `in_sources(self.sources)` check and
fails every `unit_value` lookup against them, in games that contain them.

### The measurement

Probe: `ScoredBot::new`'s `sources` changed `POK` → `DEFAULT`, nothing else; the M08-021 behavioral
suite (`play_batch`, all 30 seeds, `Seats::Scored`) run before and after; mutation reverted and
`bot.rs` verified byte-identical to HEAD afterwards.

| metric | recorded v3 bound | POK (as committed) | DEFAULT (corrected) | in bound? |
|---|---|---|---|---|
| `vp_pace` | 0.383333–0.448765 | 0.416049383 | **0.460493827** | **outside** |
| `score_spread` | 1.608871–1.922139 | 1.765128774 | **1.959731245** | **outside** |
| `share_INVASION_RESOLVED` | 0.027887–0.029380 | 0.028637045 | **0.030831871** | **outside** |
| `share_PRODUCTION_RESOLVED` | 0.047429–0.048661 | 0.048040855 | **0.049742140** | **outside** |
| `share_SHIP_MOVED` | 0.067421–0.072386 | 0.069932540 | **0.072865453** | **outside** |
| `share_SYSTEM_ACTIVATED` | 0.093517–0.095810 | 0.094654092 | **0.095836669** | **outside** |
| `faction_differentiation` | 0.306363–0.763379 | 0.432335032 | 0.654801827 | inside |
| `share_SPACE_COMBAT_RESOLVED` | 0.008142–0.009275 | 0.008714879 | 0.008700170 | inside |
| `share_TACTICAL_ACTION_BEGAN` | 0.046067–0.047169 | 0.046613238 | 0.046094528 | inside |
| `completion` | 1.0–1.0 | 1.000000000 | 1.000000000 | inside |

Total VP across the 30-seed × 6-seat set: **674 → 746**, mean VP/seat **3.744 → 4.144 (+10.7%)**.
`faction_differentiation` moves **+51.5%**.

**Six of the ten gated metrics land outside the recorded v3 bounds.** The behavioral gate would
fail on the corrected bot — which is the correct outcome, because the bounds were recorded from a
bot playing at the wrong scope. They are bounds on a mis-scoped agent.

### What this means, stated plainly

1. The M08-021 v1/v2/v3 behavioral baselines describe an authored bot that cannot see Thunder's
   Edge content in games that contain it. Every downstream citation of those bounds inherits that.
2. Correcting the scope is a **re-baseline event**, and by the M07-020/M08-019 precedent an
   operator decision — it invalidates a recorded baseline and moves a gate.
3. The direction is not neutral. The corrected bot scores materially more, so the mis-scoped
   baseline understates what the authored bot can do — which is the arm the MLP branch is measured
   against.

### The second half — the instrument

`ScoredBot::new` also sets `content: ContentStore::embedded()`, ignoring whichever store the game
runs on. That makes the bot **invisible to any `ContentStore::from_dir` perturbation**, which is
the same blindness mechanism M08-019 Y1 identified in `annexable()`.

Consequence for the M08 exit gate: the E1 campaign (28 content categories × 30 seeds, reported
`0/28`) was run with `Seats::Scored`. It could not have detected an order or content dependence
inside the bot's own valuation path, because that path never reads the perturbed store. **The 0/28
result is sound for the engine and vacuous for the bot.** No claim is made here that such a
dependence exists — only that the instrument could not have seen one.

### Not yet decided

This is a finding with a measurement, not a package. It needs an operator decision before any code
moves, because the fix changes recorded gate values. The options, and what each costs, are for the
operator; recorded here so the decision is made against numbers rather than against an argument.
