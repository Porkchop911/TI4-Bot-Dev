# M09-019b — Bounded profile with raw samples + feature inventory

## Package details

- **Milestone/package:** M09 / row 019, child b (parent spec: `plans/M09-019_POST_RULES_BASELINE_PROFILE.md`).
- **Dependencies:** M09-019a (accepted at `22a7fa7` on this branch), M00-012 microbenchmark protocol
  (`plans/evidence/M00-012{,a,b,c,d,e}.md`, fixed before any measurement).
- **Branch:** `wp/m09-019-post-rules-baseline-profile` (continues from the accepted M09-019a tip).
- **Permission class:** P2 — bounded profiler output. Runs simulations and profilers; writes raw
  reports to gitignored `out/profiles/`; commits plans/evidence only. No network, no new
  dependencies, no external state effects. The holdout pool and the r6 checkpoint are read-only
  inputs; their sha256 is asserted unchanged before and after the campaign (non-overwrite proof).

## Objective

Two deliverables, per the parent row:

1. **M00 protocol timing re-baseline** of engine / feature / model time on the post-rules tree.
   The pre-rules number (~450 µs/decision, MLP plan §2) was inferred from an aggregate training
   log, never profiled; this package produces the first real component breakdown under the fixed
   M00-012 protocol. No optimization is bundled — measurement code only.
2. **Feature inventory.** Catalog of the current feature families in `ti4-policy::features`
   (extractor → head mapping, factual vs hashed), committed as an evidence table with a pinning
   test so rows 021–023 can show diffs against it.

## Deliverable 1 — timing workloads and fixed parameters

All three workloads are single-worker (M00-012c: single-thread benchmarks use one worker), timed
with monotonic elapsed nanoseconds (`std::time::Instant`), fresh state per iteration, seeded from
the manifest constants below (never ambient entropy). The runner changes no power plan, priority,
or affinity setting.

| Constant | Value | Notes |
|---|---|---|
| `WARMUP_ITERATIONS` | 10 | M00-012a; each warmup must pass its semantic gate or the run is invalid |
| `TIMED_SAMPLES` | 30 | M00-012b |
| `IDLE_BEFORE_TIMING_MS` | 5000 | M00-012a five-second idle before timed samples begin |
| `W1_SEED_BASE` | 919_501 | W1 sample i uses seed `919_501 + i`, i = 0..40 (warmup then timed); distinct from the M08-021 812_xxx set and the M09-019a panel 919_001..=919_030 |
| `W2W3_FIXTURE_SEED` | 919_601 | game seed = tile seed, one fixed position for all W2/W3 iterations |
| `W1_HORIZON_STEPS` | 20_000 | safety bound with round cap 50; neither may bind — a complete game on this engine ends by objective-deck exhaustion at round 9, measured across all 40 seeds (see W1) |
| `REPLAY_STEP_BOUND` | 5_000 | safety bound for W2/W3 fixture replay; exceeding it fails the semantic gate |
| pool | `out/pools/full_np8_12_holdout.json` | sha256 prefix `aba33c81aa04cefb`; Validation role, same board process as the accepted M09-019a panel. The final-role pool is **not** used (parent non-goal). |
| checkpoint | `out/stage2_r6/final10000.json` | sha256 prefix `be792a2a207ced25` (`baseline::R6_CHECKPOINT_SHA_PREFIX`); loaded via `Champions::load_checkpoint_accepted`, which validates every champion profile. |
| seats / scope | p1..p6, `content_types::DEFAULT` (= FULL) | same as the accepted M09-019a panel. Note: `SourceSet::default()` is the *empty* EnumSet, not a scope — the runtime paths use the `DEFAULT` constant per its doc contract |
| build profile | **release** primary; debug reported alongside for reference | the pre-rules ~450 µs/decision context aggregate comes from release-mode training/simulation work (MLP plan §10), and future optimization targets release. M00-012a: the profile must match what acceptance gates use — the panel/baseline infrastructure runs `--release` |

### W1 — engine (single-core game class)

Each sample: fresh six-player game on a pool board (`pool.galaxy(content, sources, seed, homes)`),
all seats `SeededRandom::new(seed)` (pure engine + O(1) decisions; no policy cost in the timed
region), then `game.run(50, 20_000)` played to natural termination. The sample is the elapsed
nanoseconds of the run call; units of work are the resolved choices
(`game.table.log.records.len()`); the derived per-decision value divides by that count.

**Design note (measured before finalising):** every full game on this engine ends by
objective-deck exhaustion at round 9 regardless of play style — all 40 seeds in the manifest
complete there (`w1_ending_diagnostic` test). "Play one complete game" is therefore the workload's
natural shape; a fixed step budget would cut games off mid-shape, and per-decision normalisation
keeps samples comparable even when games differ in step count (M00-012b normaliser philosophy).

**Semantic gate (per sample):** `run(50, 20_000)` must return a **finished** state with no engine
error — i.e., the game completed. An engine error, round-cap termination, or hitting the safety
step bound is a shape mismatch and invalidates the run; a failed timed sample aborts the whole
campaign (M00-012b).

### W2 — feature extraction (policy scoring class)

Each sample: fresh replay from the fixture seed on the pool board with `SeededRandom` seats; step
until the first choice whose `decision_head(choice)` is `"production"` (a first-class head with a
large, payload-rich option set); capture `(step_index, choice)`. The timed region is exactly
`explicit_choice_features(&seen, &choice, &choice.player)` — the whole-choice extraction the live
inference path uses for explicit profiles. `Observed::new(&state, content, sources, None)` matches
the engine's own construction (galaxy always `None` in the live path).

**Semantic gate:** the captured position must be identical across every iteration (same step index
and same option id list — replay determinism is part of the gate); at least 3 legal options; total
feature entries across all option vectors > 0.

### W3 — model scoring (policy scoring class)

Same fixture and capture as W2. The timed region is exactly the live scoring path:
`profile.resolved_head(decision_head(choice))`, then `profile.score_vector(head, v)` per option into
a `BTreeMap<String, f64>`, then `inference::probabilities(&scores, temperature)` with the head's
temperature.

**Semantic gate:** every score finite; total probability mass within 1e-6 of 1.0.

### Statistics and variance (predeclared)

Computed over **all 30 raw samples per workload**, no outliers discarded (M00-012b): min, max,
mean, median, **sample standard deviation (n−1)**, p50, p95, p99. Mean and nearest-rank
percentiles reuse `ti4_sim::benchmark::Statistics::over` (rank = ceil(q·n/100), 1-based — p50 →
15th of 30, p95 → 29th, p99 → 30th); the profiler computes sample stdev explicitly because the
shared helper uses the population convention.

Variance thresholds are predeclared from M00-012e for the applicable workload classes (single-core
game; policy scoring): **stdev/mean ≤ 5%** and **(p95 − p50)/median ≤ 10%**. Disposition is fixed
in advance: if either threshold fails, one fresh 30-sample repeat run is performed; if either run
passes, both reports are retained and the result is marked `unstable` (cannot support a
performance gate); if both fail, `rejected_variance`. Runs are never combined and samples are
never cherry-picked. A speedup or comparison claim is out of scope for this package regardless —
this is a baseline re-measurement on one implementation.

### Report schema

One JSON report per retained run in a uniquely named, atomically published campaign directory
under gitignored `out/profiles/`, UTF-8, schema version `1.0.0`
(M00-012d fields): `benchmark_id`, `implementation: "rust"`, `oracle_commit: null`, `rust_commit`
(HEAD at run time), host block from the accepted protocol reader `benchmark::Host::observed()`
(`os`, `cpu` = PROCESSOR_IDENTIFIER with fallback, `logical_processors`, affinity inherited and
unchanged; full host fingerprint referenced to the M00-001 environment record in evidence),
workload block (`fixture_id`, seed(s), `workers: 1`, `semantic_gate: pass|fail`),
`warmup_iterations: 10`, retained `warmup_samples_ns[10]` and warmup units, raw
`samples_ns[30]`, `statistics_ns { count, mean, median, stdev, min,
max, p50, p95, p99 }`, `variance { stdev_pct, p95_minus_p50_pct, accepted }` (accepted per the
predeclared 5%/10% thresholds above), explicit final variance disposition, processor group,
actual process affinity, operator no-competitor assertion, and `captured_at_utc` (audit metadata,
excluded from canonical hash/equality). Reports are accumulated in memory and the complete
directory is renamed into place only after every semantic and input-integrity gate. The canonical
sha256 of each report and the full statistics summary are committed in evidence; raw samples stay
in `out/`.

### Runner

- `crates/ti4-sim/src/profile.rs`: protocol implementation (workload builders, sampling loop,
  statistics, report writer) plus ungated unit tests for the statistics/percentile/variance logic
  and report schema on synthetic data.
- The full campaign is a test gated by environment variable `TI4_M09_019B_PROFILE=1` (skips with a
  printed notice otherwise), so the default suite stays fast; evidence records the exact invocation.

## Deliverable 2 — feature inventory and pinning test

Evidence table in `plans/evidence/M09-019.md` cataloguing, for each feature family: name shape,
which extractor emits it (legacy hashed / explicit structured), factual vs hashed status, and the
head mapping rule. Structural facts pinned by a new test
`m09_019b_feature_inventory_is_pinned` in `crates/ti4-policy/src/features.rs` (**test module only**):

1. `FEATURE_PREFIXES` is exactly the 13 declared legacy families (closed list).
2. `learned::STAGE1_DECISION_HEADS` is exactly the 14 schema-4 heads (r6 champion vocabulary).
3. On a fixed inventory fixture (one option with bool + number + string + array payloads and a
   multi-token prompt), every legacy name stays inside the closed list **and** all 13 families are
   exercised by at least one emitted name — each table row is real, not aspirational.
4. On the same fixture through the explicit path: no `kind-faction:`/`option-faction:` names
   (faction crosses removed), no all-digit `option:{token}` names (board identities removed),
   `state_cross(&choice) == StateCross::None` and consequently no `state-kind:`/`state-option:`
   seat-fact names, and the explicit-only `prompt-kind:` family is present.

Rows 021–023 change feature families; any such change breaks one of these assertions until the
inventory table and this test are updated in the same package — that is the diff mechanism.

## Writable paths

- `crates/ti4-sim/src/profile.rs` (new timing module)
- `crates/ti4-sim/src/lib.rs` (+2 registration lines)
- `crates/ti4-policy/src/features.rs` (inventory pin plus debug-only closed-family enforcement;
  no release feature value/name change)
- `plans/M09-019b_BOUNDED_PROFILE_FEATURE_INVENTORY.md` (this file)
- `plans/evidence/M09-019.md` (append 019b section; historical text preserved)
- `plans/EXECUTION_STATE.md`

Any path not listed here requires a declared scope extension before the edit.

## Non-goals

- No optimization of engine, feature construction, or inference code — measurement only.
- No changes to `run.rs`, `baseline.rs`, or any production behavior in ti4-sim/ti4-policy.
- No new dependencies (Cargo.toml untouched).
- No use of final-role data; no archiving of checkpoints into Git (M09-020 owns durable fixtures,
  on its own branch).
- No paired Python comparison and no speedup claim — parity is not an acceptance criterion.
- No GPU/training workloads (row 024 territory); CPU single-core only.

## Acceptance criteria

1. `profile.rs` implements the M00 protocol exactly as fixed above; ungated unit tests pass.
2. The full campaign runs to completion with every semantic gate passing on all warmup and timed
   samples; raw reports written under `out/profiles/`; pool/checkpoint sha256 unchanged before vs
   after (non-overwrite proof recorded).
3. Variance verdict per the predeclared thresholds, honestly reported (pass / unstable /
   rejected_variance are all valid outcomes of a baseline measurement).
4. Evidence contains: full statistics tables for W1/W2/W3 with derived per-unit (per-decision for
   W1) and per-option values, host block, raw-report sha256s, non-overwrite proof, and the
   pre-rules aggregate
   (~450 µs/decision) quoted as context only — not a gate.
5. Inventory table committed in evidence; pinning test green; its assertions encode the exact
   family/head vocabulary so rows 021–023 diffs are visible.
6. Workspace suite green; clippy and rustfmt clean on touched crates (pre-existing warnings noted,
   not fixed).

## Review tier

**D — performance evidence.** This is the second of the two independent frontier reviews required
by row 019: one over M09-019a (baseline methodology + determinism, accepted at `22a7fa7`), this
one over M09-019b (timing protocol conformance + inventory accuracy). The parent row is not
accepted until both are resolved.

## Status

**Closed by operator decision (2026-08-24).** F-M09-019b-1..7 are implemented and the final
six-report release campaign was generated from clean exact commit `c2fb515` (`ae897f4`). All
semantic/integrity gates pass; both retained runs fail variance for each workload, so W1/W2/W3 are
correctly `rejected_variance`. Exact statistics/hashes are in the evidence file. Tier-D pass 2 was
performed independently (changes required); no written fresh recheck verdict exists in the
repository — per operator directive the row's review is treated as complete and this package is
closed on that basis, recorded as an operator decision rather than a reviewer acceptance (see
`plans/M09-019_OPEN_REVIEW_ITEMS.md`).
