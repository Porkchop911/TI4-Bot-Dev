# M08-021 — independent Tier B review (open items)

## Reviewer identity and independence limitation

**Operator-directed review, 2026-08-23.** The operator instructed that the review be done; no
frontier-model peer was connected to this session's broker (`list_peers` empty at dispatch time),
so the implementing agent performed the review pass in a fresh adversarial posture over its own
work. **This is not an independent model review** — the M06-024 independence limitation applies
in full: findings below are as rigorous as this context allows, but they carry no cross-model
check on the bounds methodology itself. The escalation path (frontier review of the bootstrap
methodology or any re-baseline) remains open per the spec's tier description and should be taken
before M08-019's exit review if the operator wants a second perspective on the statistical
protocol specifically.

## Scope reviewed

`crates/ti4-sim/src/behavior.rs` (new), `crates/ti4-sim/src/lib.rs` (+2 lines), spec, evidence.
Verified: no other file under `crates/` changed; `run.rs`, `play()`, `GameResult` untouched;
bot.rs byte-identical to base after both mutation checks (`git diff crates/ti4-policy/` empty).

## Findings

### R1 — MEDIUM, required before commit: bounds embedded at display precision with no protocol tie

The v1 bounds were transcribed from a probe's `{:.9}` output (nine decimal places), not exact
doubles. Every v1 metric sits well inside its interval (margins ≥ 1e-4 except the exact
`completion`), so on this tree the rounding is harmless — but nothing in the committed code tied
the recorded constants to the documented bootstrap protocol: a transcription error in either the
values or the parameters would have passed every test, because the bounds check alone cannot see
a constant that was mistyped *consistently inside* the true interval.

**Resolution (applied):**
1. Bounds re-derived at full double precision (`{:?}`) from a fresh baseline run on this tree;
   embedded constants replaced with exact values (e.g., `vp_pace` hi: 0.469_135_802 →
   0.469_135_802_469_135_8).
2. Named protocol constants added: `BOOTSTRAP_DRAWS = 2000`, `BOOTSTRAP_SEED =
   0x9E37_79B9_7F4A_7C15` — changing either is now a documented re-baseline event.
3. The gate test gained an in-gate **protocol integrity check**: it recomputes every CI from the
   current batch's per-seed values under exactly those parameters and asserts bit-equality with
   the embedded constants. No extra game runs (bootstrap arithmetic is milliseconds against ~13 s
   of games).

**Non-vacuity verified:** one digit of an embedded constant mutated (`..._469_135_8` →
`..._469_135_7`) — gate FAILED with the exact diagnostic:
`recorded bound for vp_pace does not match the protocol recomputation: recorded (0.41111111111111115, 0.4691358024691357), recomputed (0.41111111111111115, 0.4691358024691358)`.
Revert restored green on the next run.

### R2 — LOW, recorded: shared RNG stream across metrics' bootstraps

All nine CIs are computed under the same `BOOTSTRAP_SEED`, so their Monte Carlo errors are
correlated (resample *sets* coincide across metrics). Harmless for this suite's use: each bound
is checked independently and each resample set is still an iid draw from that metric's empirical
distribution, so every individual interval is valid. Recorded so a future package doing joint
inference across metrics does not assume independent intervals without per-metric seeds.

### R3 — INFORMATIONAL: gate runtime

~13 s on this machine (two 30-game batches on parallel workers; the integrity check adds
milliseconds). Serial cost scales with seed count and core count — same class as M08-018's ~86 s
campaign, which the operator closed as dev-loop-only. Never in a training path.

### R4 — VERIFIED, no action: seed range and seating claims

- `grep -rn "812_0" crates/` (excluding behavior.rs): **no collision** with any other committed
  seed set (M08-018 used base 7_777; replay tests use small fixed seeds).
- Seating is stable as documented: `Table::seated(content, players, sources)` takes no seed —
  faction→seat assignment is identical across all thirty games and across versions. `run_with`
  preserves player order per worker and sorts results by seed before returning, so the gate's
  zip comparison (with its explicit same-seed-order assertion) is safe under any scheduling.

### R5 — VERIFIED, no action: degenerate completion bound

`[1.0, 1.0]` is intentional strict-invariant semantics ("every game ends cleanly"), not a
statistical interval; the non-degeneracy test correctly asserts finiteness + ordering (`lo <= hi`)
rather than strict inequality, with the degeneracy rule documented at the bound's site and in the
module docs. Any future error or horizon cutoff fails the gate — exactly the detection wanted.

## Verified sound (no finding)

- **splitmix64** matches the reference algorithm (γ increment; 0xBF58_476D_1CE4_E5B9 /
  0x94D0_49BB_1331_11EB multiplies; final xor-shift); normalization is top-53-bits ÷ 2⁵³ → [0,1)
  with an exactness-reasoned allow (top 53 bits fit exactly in an f64 mantissa).
- **Percentile indices** are symmetric: lo = stats[50], hi = stats[1949] of 2000 draws — 50
  resamples strictly below, 50 strictly above (absent ties), a proper central 95% interval.
- **Resample index safety:** draw ∈ [0,1) ⇒ product < n; truncating cast bounded by `% n`;
  allows carry construction-based reasons.
- **Determinism precondition** asserts same-seed order *before* any value comparison — a flaky
  bound cannot hide engine nondeterminism behind misaligned zips.
- **Per-game share averaging** (vs pooling all events) is the documented choice: each game weighs
  equally regardless of length; recorded in evidence with the v1 census.
- **Mutation check** (both mutants, full shifted-distribution table in evidence): pass-score
  mutant moved eight of nine metrics out of bounds with zero CI overlap — the suite detects real
  behavioral drift decisively; activation-base mutant moved none, correctly characterized as a
  within-class re-ranking that `system_value` absorbs. Sensitivity note (suite resolves at
  action-frequency/pace/spread level) is the right resolution for baseline comparability.

## Disposition

**Accept with R1 required before commit — R1 resolved in-package and verified non-vacuous.**
R2–R5 recorded as above. Independence limitation stands: if the operator wants a cross-model
check on the bootstrap methodology specifically, that is a frontier escalation available before
M08-019's exit review; nothing in this package blocks on it.
