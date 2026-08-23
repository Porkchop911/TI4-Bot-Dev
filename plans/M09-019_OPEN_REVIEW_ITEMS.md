# M09-019 open review items

Ledger for the post-rules baseline/profile row (children a/b). Findings are recorded here as they
arise; dispositions must be resolved before M09-019 closes and before M09-030.

## M09-019a observations (implementer, 2026-08-23) — pending Tier-D review

### O-M09-019a-1 — per-game records omit wall-clock by design — INFO

`panel.json` game records carry no `seconds` field. This is deliberate: the determinism proof
compares runs byte-for-byte, and a wall-clock field would make every comparison diverge (the
M08-019 `GameResult.seconds` trap). Timing evidence belongs to M09-019b under the M00 protocol,
where raw samples are preserved separately from semantic results.

### O-M09-019a-2 — zero completions at the 4-round horizon is a measurement, not a failure — INFO

All 30 baseline games ended `horizon_reached` with mean VP ≈ 2.2–2.7 per seat: the r6 champion
does not finish full games on the corrected engine within its training horizon. This is exactly
the kind of post-rules behavior §8 says must be measured before any MLP comparison. Consequence:
outcome-level (winner/completion) metrics have no spread at this horizon; if a later package needs
discriminative outcome evidence, it must choose and record a longer horizon — that is a scope
decision for M09-019b or its reviewer, not something to change silently here.

### O-M09-019a-3 — no committed test plays the real out/ artifacts — LOW (known gap)

The hermetic determinism test uses a synthetic six-slot pool; the real-artifact path is exercised
by the example binary and recorded in evidence, but no *committed test* loads `out/stage2_r6` or
the pools. This matches F-M09-018-4 (no committed real-artifact import test anywhere yet) and is
the natural home of M09-020's durable fixtures: once the baselines are archived into Git, a
committed test can verify them without depending on gitignored local data.

## Status

M09-019a: implementation complete, **pending independent Tier-D frontier review** (first of two).
No findings yet; observations above are recorded for the reviewer's disposition.
