# Stage-2 continuation plan (2026-08-15)

Branch `codex/stage1-parity-fixes`, HEAD `9fa7766` (P1-a4). Safepoint anchor: `66fd234`, tag
`safepoint/stage2-stall-baseline`. Oracle: `D:/Projects/ti4-engine @ 37061c5` (read-only, byte-untouched).

## Overarching goal

Make Rust Stage-2 training produce **measurable, promotable progress** — a promotion beyond the
bootstrap champion with paired table gain > 0 at n=32 panels under the validated gate protocol —
while keeping the Rust engine a faithful behavioral mirror of the Python oracle so that checkpoints
are transferable and every claim is testable by differential.

Two coupled sub-goals, kept strictly separate in reporting:

1. **Parity KPI** (means): T6 per-decision differential shows `max_score_gap = 0` and zero choice
   mismatches on the common prefix of every faction, with residuals only from documented open
   classes; trade/note/ac option-id sets mutually consistent where both engines offer them.
2. **Training KPI** (end): Rust reproduces oracle promotion behavior from an identical champion
   (boundary-by-boundary differential), then promotes its own lineage past the bootstrap with a
   real paired gain.

The zero-signal diagnosis stands unchanged (evidence §Conclusion): the stall is an
optimization/signal problem, not gate strictness; n≥32 panels and σ-based promotion are validated
as well calibrated. What has changed since: T4 showed a genuine upward trend under oracle-parity
settings (paired gains +0.125 → +0.391 across halves; 17/42 boundaries past the +0.30 margin, all
rejections clearance-veto driven with rotating factions), and T6 proved the earlier cross-engine
divergence was surface labels plus harness table artifacts — now being closed by Phase 1.

## Where we stand (verified against repository state)

| item | status | anchor |
|---|---|---|
| Stall diagnosis + instrumentation (T0–T2) | done, committed | `c7bcde6` |
| Panel decorrelation `--panel-step N` (opt-in, default bit-identical) | done, committed | `59a3c79` |
| T4 oracle-parity run (`--every 50 --accept-sigmas 0 --panel-step 32`, n=32) | ended u6700/8100 by operator decision; 43 boundaries, zero promotions, last boundary gain +0.490 | `out/stage2_t4_oracle_parity.json` |
| Python retest (control): own stage-1 champions → Stage-2 to u3550 | done; gate verdicts identical at 9/10 boundaries; sol@u3350 reproduced; xxcha@u3450 flip explained by non-run-reproducibility (wall-clock abandonment, parallel reduction order); accepted VP +3.52, mostly horizon reorientation jump in first 50 updates | `out/py_retest_stage2_pychamp.json` |
| T5 seed-stream alignment (Rust trains on Python's global stream: base 74_000_000, stride 10_000) + pilot +50 | done; Rust promoted u3100 matching Python; residual per-faction play divergence | `e71d0de` |
| T6 root cause: profile-table artifact (harness), not scoring | resolved; max_score_gap 0.000000 on common prefixes with correct tables | `fa5a26d`, `a84e18d` |
| Phase 1 surface alignment P1-a, P1-a2, P1-a3 (trade prompts/vocab/identity/pricing) | done | `8c628b6`, `54f16f2`, `cd863f9` |
| F3 correction + P1-a4 action-card trade shape (Arbiters) | done; pure surface expansion verified rust-vs-rust | `ca5333a`, `9fa7766` |

Standing protocol facts that every later step must respect:

- Python traces always with `--table learner_profiles`; Rust traces strip the 6-line preamble and
  use `--greedy-temperature 0.0001` (the silent-ignore `arg()` helper makes a wrong flag name an
  unflagged invalid run — verify metadata temperatures before any choice-level claim).
- F1: Rust rollouts deploy no leaders (`leaders::deploy` test-only; Python calls `_leaders_mod.arm`).
  Until fixed, hacan/xxcha comparable prefixes end at idx=1 (missing `component|leader|…agent`).
- F2: action-card hand composition diverges (Rust hands thin without the oracle's status-phase
  draw); full ac parity needs F2 + P1-a4 together. Verify exact oracle mechanism at spec time.
- Python pipeline is **not run-reproducible** — no experiment may pre-register exact-match rules
  against it; use verdict-agreement / distributional rules with an explicit drift band.

## Phase 1 — remaining surface alignment (mechanical, no frontier review needed)

Each item: its own branch + focused commit; failing test first; crate gates green; T6 re-run after
each landing to confirm the intended delta only (plus a rust-vs-rust diff for any option-id format
change). Default order is size/risk ascending so each package's effect is attributable and early
wins are visible.

| pkg | scope (from the recorded class table, evidence §Phase 1) | candidate sites | rationale / risk |
|---|---|---|---|
| **P1-b** — done | Payment prompts/labels (scoping corrected the class table: these are production-payment prompts, not auction bids): per-iteration prompt `"pay {owed} more {kind}"` + kind-suffixed exhaust labels at both Rust sites (`production.rs` free function ~212 and `ProductionWindow::Stage::Paying` ~1098) | `crates/ti4-engine/src/production.rs` | **Key finding:** prompt and label text are scoring features for the learned decider (features.rs tokenizes both), so this was feature-space alignment toward the Python-trained vocabulary, not cosmetics — rust-vs-rust shows intended choice movement at payment decisions with zero score-gap regression on common prefixes. Spec + findings F4–F7 in evidence §P1-b; mechanics scheduled as P1-g |
| **P1-d** — done | Reaction option ids + prompt identity: `reaction:{faction}:{EVENT}:after` (lowercase relation, faction id) vs `reaction:seatN:{EVENT}:After`; inner choice prompt/kind/labels + per-printed-card dedupe | `reactions.rs`, `timing.rs`, `wiring.rs` | implemented as spec'd; T6 all six factions max_score_gap=0, zero new divergence classes; rust-vs-rust p1b→p1d shows pure identity migration (one choice "diff" is the rename itself) with expected feature-space score shifts on the 60 renamed options. New findings **F8** (outer payload missing Python's `"cards"` list — needs a timing.rs `OptionPayload` API change) and **F9** (single-card windows: Python asks, Rust auto-picks; window-shape family of F5) recorded for a later package after P1-f |
| **P1-e** — done | Speaker choice + seat-id prompts → faction names: `"who becomes speaker"` with id/label = faction (`"{faction} becomes speaker"`) vs Rust seat ids under `"choose the new speaker"`; signal-jamming victim prompt `"…whose token goes into {system}"` + labels `"{faction}'s command token"` | `strategy_cards.rs`, `action_cards.rs` | implemented as spec'd; answers map back to seats via first-match-in-order lookup scoped to the presented candidates. T6 all six factions max_score_gap=0, zero new classes; 4/4 speaker decisions now oracle-format (count matches Python); only remaining seat-id surface is P1-c's `grant free Trade replenishment`. Rust-vs-rust p1d→p1e: first fork for l1z1x is the rename itself (speaker genuinely flipped hacan→jolnar) — expected feature-space movement per P1-b precedent. New findings **F10** (Rust never emits SPEAKER_CHANGED; Python does in 3 places), **F11** (agenda tie-break surface diverges: prompt/kind/ids + missing silence path), **F12** (jamming system option set: no adjacency/home-exclusion/galaxy dependency) |
| **P1-c** | Ground-commit + ready/retreat surface: `"commit ground forces in {sys}"` with ids `commit\|n\|planet` + `done_committing`; `"ready a planet"` wording; free-trade replenishment prompt/options (`"let another player replenish commodities"`, done/factions) vs Rust `land\|…`/decline and seat options | `invasion.rs`, `strategy_cards.rs`, replenishment sites | medium: option-id *format* changes → checkpoint feature buckets unaffected (weights are per-head, not per-id), but needs rust-vs-rust diff to prove intended delta only; touches combat/invasion paths |
| **P1-f** | Misc wording + loop structure: em-dash → `"--"` hyphen normalization across prompts; leadership purchase loop vs Rust `follow` gate (blind `decline/follow` secondary windows, incl. the no/yes replenishment secondaries seen at jolnar/l1z1x/letnev idx=1) | locate at spec time | largest Phase-1 item: changes *window shape* (inline choice in Python vs separate blind secondary in Rust), not just labels; do last so smaller packages' T6 deltas stay clean. Note: an earlier operator-facing message mislabeled this content as "P1-b"; the recorded class table governs and puts it here + in P1-c |
| **P1-g** | Payment mechanics (findings F4/F5/F6/F7 from P1-b scoping/tests): MC trade-good worth 2 vs Rust flat 1 (+`available()` ×1 undercount → legality-level); single-option auto-pick in Python vs always-ask in Rust; xxcha cross-source `exhaust\|{planet}\|{source}` options + affordability guard + PLANET_EXHAUSTED/BREAKTHROUGH_TRIGGERED emissions; F7 zero-worth planets offered as payment options (Python filters `worth > 0`) | `production.rs` both sites | behavioral (option set, values, window shape) — needs its own failing tests and rust-vs-rust trace diff; do after P1-f so text-only deltas stay clean first |

**Phase-1 exit criterion:** full-workspace gates green; T6 re-run (seed 83000001, rot 0, rounds 4,
greedy 0.0001, `--full-features`, correct Python table) shows comparable prefixes extended past
idx=1 for jolnar/l1z1x/letnev/sol; residuals only from F1 (hacan/xxcha leader components) and the
documented post-fork state cascade; P1-g findings (F4/F5/F6) remain as recorded residuals until that
package lands. Then a single consolidated evidence section + handover, compact.

**Findings backlog added by completed packages:** F8 and F9 (both from P1-d — reaction outer `"cards"`
payload; single-card ask asymmetry) are deferred to a later package after P1-f so window-shape changes
stay isolated. F4/F5/F6/F7 remain with P1-g as recorded. From P1-e: **F10** (SPEAKER_CHANGED never
emitted in Rust — Phase 2 event coverage), **F11** (agenda tie-break surface + silence path — beyond a
mechanical rename; zero T6 hits), **F12** (jamming system option set lacks Python's adjacency expansion,
home exclusion, and galaxy dependency — P1-g family).

## Phase 2 — game-flow alignment 

These change which legal actions exist and when windows fire → legality/timing territory → tier C/D
frontier-model review of the spec *before* implementation, then normal failing-tests-first flow.

1. **F1 — leader deployment in real games/rollouts** (Python `_leaders_mod.arm` at creation; Rust
   `leaders::deploy` exists but is test-only). Unlocks commander options and `"an"` notes for all
   factions' rollouts; expected to extend hacan/xxcha comparable prefixes past idx=1. Largest
   behavioral change of the phase — review first, differential-test against Python leader traces after.
2. **F2 — action-card hand composition** (status-phase draw mechanism; verify exact oracle source at
   spec time). Closes the ac alias-set divergence so P1-a4's shape is exercised in shared state.
3. **Reaction-window set alignment per T6b audit**: close 10 missing windows (UNIT_DESTROYED
   after/when, GROUND_FORCE_COMMITTED after, SUSTAIN_DAMAGE_USED after+when, GROUND_DICE_ROLLED
   after, RETREAT_ANNOUNCED after, defender COMBAT_ROUND_STARTED variant, HITS_ASSIGNING before ×2);
   remove or gate the 1 Rust-extra (PRODUCTION_USED "your units use PRODUCTION"); align 3–4 event
   names (SPACE_COMBAT_ENDED↔WON; INVASION_STARTED↔BEGAN ×2) with emit-site parity verification.
4. **WHEN/AFTER semantic fix** (strategy-card window: Python `Relation.WHEN` fires before completion,
   Rust `After` after resolution — different observable/mutable state at the reaction point).
   Highest-risk single line of Phase 2; gets its own review and its own differential evidence.

Ordering within Phase 2 is F1 → (F2 ∥ windows) → WHEN/AFTER: F1 first because it blocks the cleanest
remaining T6 prefixes and most changes rollout credit; F2 and window set can proceed in parallel on
disjoint files if review approves both; WHEN/AFTER last because it shifts reaction *timing*, which
interacts with everything above.

**Phase-2 exit criterion:** T6 differential residuals reduced to post-fork state cascade only; every
new window/leader decision score-equal (max_score_gap 0) where the common prefix reaches it; no
regression in any crate gate or training test.

## Phase 3 — decisive Stage-2 experiment and the progress push

1. **C1 — schema-compat + dry pilot.** Confirm `D:/Projects/ti4-engine/out/stage1_pg_six_to5000_20260810.json`
   loads in Rust (T5 pilot already proved this at +50 updates; re-verify post Phase 1/2 since rollout
   code changed). Run a short (+50) smoke to confirm no behavioral break from the alignment work.
2. **C2 — boundary-by-boundary differential run.** Rust `stage2_training.exe` *from that same Python
   stage-1 champion*, T4-equivalent settings (`--every 50 --accept-sigmas 0`, n=32 validation +
   confirmation, `--panel-step 32`, horizon 4, seed stream base 74_000_000 stride 10_000 per commit
   `e71d0de`), through at least u3550 to cover the Python retest span. Compare against
   `out/py_retest_stage2_pychamp.json` boundary by boundary. **Pre-registered rule (statistical, not
   exact — Python is non-run-reproducible):** agreement on promotion/rejection verdicts at every
   *stable* boundary (a boundary whose paired gain in the Python run was >0.15 away from its nearest
   veto/margin), plus no systematic drift in the candidate-gain distribution beyond what the xxcha@u3450
   flip quantifies as the non-reproducibility band. Pass ⇒ implementation gap closed; fail ⇒ escalate
   to frontier-model differential diagnosis (per T4's pre-registered failure branch) instead of tuning.
3. **C3 — Rust-native progress push.** Only after C2 passes (or with operator approval on partial):
   continue the Rust lineage beyond u3550 toward the original goal — a promotion past the bootstrap
   champion with paired gain > 0 at n=32 under `--accept-sigmas` per operator choice. T4's trend data
   (+0.125 → +0.391; 17/42 past margin) is the prior that this can succeed; monitor the rotating
   clearance-veto pattern specifically — if vetoes keep rotating factions on *fresh* panels, diagnose
   horizon-4 clearance noise vs residual rollout-behavior differences (feeds back into Phase-2
   residuals rather than gate tuning).
4. **Fallback levers, in order** (each its own small package with pre-registered decision rule):
   train-seeds 16 → 64 to cut gradient variance (~4× cost); then reward redesign (outcome-only credit
   over the full horizon) as last resort. `--rounds 8` remains deprioritized by operator ("plays too
   many rounds") and stays on file only.

## Standing verification protocol (every package, Phase 1 or 2)

1. Pre-implementation spec section in `plans/evidence/STAGE2-STALL-INVESTIGATION.md` (scope, oracle
   source citations, tests planned, permission class/bounds, out-of-scope list). Phase-2 specs also
   get the frontier review *before* code.
2. Failing test(s) first; smallest complete implementation; no speculative abstractions.
3. Gates: `cargo fmt -p <crate>` clean (let-chains), crate tests + doctests, 98 training tests,
   clippy zero warnings all targets on touched crates, workspace check.
4. T6 re-run with the full protocol checklist above; rust-vs-rust diff whenever option-id formats or
   window shapes change; vocabulary subset checks where new ids appear (strict regex for `ac*`).
5. Evidence section + EXECUTION_STATE handover rewrite; one focused commit per package; oracle repo
   integrity check after any Python invocation (`git -C D:/Projects/ti4-engine status --short`,
   with `PYTHONDONTWRITEBYTECODE=1` + `PYTHONPYCACHEPREFIX` into this repo, external cwd).
6. Compaction checkpoint at each package boundary using the handover format; never rely on memory
   across compaction.

## Decision points requiring operator approval

1. Phase-1 order confirmation (default b → d → e → c → f → g above). **Resolved:** b completed this
   session (operator's "work on plans/CONTINUATION_PLAN.md" treated as go-ahead for the default first
   package); next is d.
2. Whether C2 runs after **all** of Phase 2 or after F1 only (running earlier costs less but muddies
   the boundary comparison with known residual surface gaps; recommendation: at least F1 first, then
   decide on windows/F2 by cost vs clean-prefix value).
3. Exact C2 decision-rule thresholds before launch (pre-registration).
4. Any gate-semantics or pipeline change for C3 (e.g., the recorded post-T4 option to pre-filter the
   isolated fallback on the validation panel — semantics-preserving claim still needs explicit sign-off).

## Risk register

| risk | mitigation |
|---|---|
| State-cascade divergence masks a regression in T6 diffs | per-package rust-vs-rust diff proving intended delta only; score-gap check on full common prefix, not just counts |
| Trace-flag footgun (silent `arg()` ignore) invalidates choice-level claims | protocol checklist; verify metadata temperatures + table before any choice comparison |
| Python table mismatch masquerading as engine regression (`profiles` vs `learner_profiles`) | always `--table learner_profiles`; documented in evidence |
| Exact-match expectations against a non-reproducible pipeline | statistical pre-registered rules with explicit drift band (xxcha@u3450 flip as calibration) |
| Large-edit file corruption (learned twice this project) | `write` tool for large rewrites; validate syntax/bytes after every edit; CRLF byte-check on plans files if warnings appear |
| Oracle repo contamination | env guards + post-run integrity check, every time, no exceptions |
| Context exhaustion mid-package (happened once already) | package-boundary compaction checkpoints; durable state in evidence/handover so any session resumes from the required reading order |

## Rollback safety

Safepoint `66fd234` + tag remains the pre-Phase-1 anchor. Every P1-x and Phase-2 item is an
independent focused commit with its own evidence section → any one can be reverted in isolation
without disturbing the rest; surface changes (Phase 1) are revert-trivially safe, game-flow changes
(Phase 2) carry frontier review + differential evidence as their safety net.

## Definition of done (overall goal)

- Parity KPI: residuals in T6 limited to documented open classes with max_score_gap 0 and zero
  choice mismatches on all common prefixes; ac/note/pn id vocabularies mutually consistent where both
  engines offer them in shared state.
- Training KPI: C2 verdict agreement per the pre-registered rule, then ≥1 Rust-lineage promotion past
  the bootstrap champion with paired gain > 0 at n=32 — reported separately from parity results, as
  always.
