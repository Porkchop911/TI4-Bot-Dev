---
type: Component
title: Bot benchmarks and evidence
description: Keep gameplay strength, coverage, and inference speed comparable and reproducible.
timestamp: 2026-10-18
source_commit: f55a5b31; 592e5716 (code baseline only)
confidence: high
resource: docs/TECHNICAL_WIKI_BENCHMARKS.md
citations:
  - plans/evidence/HOLDOUT-BOTS-2026-10-13.md
  - plans/evidence/FOUR-V-SIX-MECHANISMS-2026-10-13.md
  - crates/ti4-infer/README.md
---

# Bot benchmarks and evidence

Evidence provenance: the October 13 holdout and mechanism reports were retained as untracked files in the `rule-bot` checkout and are included with this wiki publication. Commit `592e5716` identifies that checkout's code baseline, not a commit containing those reports. The report files below are the source for the quoted measurements.

[Technical wiki](TECHNICAL_WIKI.md) · [Bot designs](TECHNICAL_WIKI_BOTS.md) · [Strategy applications](strategy-wiki/MLP_APPLICATIONS.md)

**As of 2026-10-18:** no public, protocol-compatible TI4 autonomous-player leaderboard was located. [AsyncTI4's bot](https://github.com/AsyncTI4/TI4_map_generator_bot) manages human games; the [TI4 Kiosk rules bot](https://ti4-kiosk.com/) answers rules questions. Neither supplies an opponent benchmark for this engine. “Best in class” therefore has no defensible external ranking here. The strongest evidence available is the repository's controlled, dated comparisons. This page does not turn their round-four VP into a full-game win rate.

## Gameplay: matched six-player holdout

The [holdout report](../plans/evidence/HOLDOUT-BOTS-2026-10-13.md) compares the `rule-bot` checkout's **server-session smart bot** with checkpoint 16448's MLP on the **same 71 complete seed/map clusters across all six arms**, with the recorded roster, initial setup and mixed seating swaps. This is not an evaluation of `main`'s later standalone `ti4-rule-bot` crate. The metric is mean VP **per seat at the round-four cutoff**. Brackets are the report's nominal 95% t intervals; they are not estimates of full-game win probability.

| Diplomacy | Rule self-play | MLP self-play | Rule in mixed games | MLP in mixed games |
|---|---:|---:|---:|---:|
| Off | 3.347 [3.232, 3.463] | 2.254 [2.128, 2.379] | 3.465 [3.308, 3.621] | 1.840 [1.707, 1.974] |
| On | 2.786 [2.669, 2.904] | 3.601 [3.470, 3.732] | 3.207 [3.070, 3.343] | 3.749 [3.616, 3.882] |

Interpretation: the ordering changes with Diplomacy and roster. The campaign attempted 800 cutoff games, retained 764 usable games and 36 guard failures; its principal estimate uses complete cases only. The pool covered 100 unique maps, one seating orientation per seed, and no compatible four-player holdout pool. Those limits matter before using this as a deployment or promotion gate. Full setup, exclusions and raw-artifact provenance are in the [report](../plans/evidence/HOLDOUT-BOTS-2026-10-13.md).

The [controlled mechanism study](../plans/evidence/FOUR-V-SIX-MECHANISMS-2026-10-13.md) found that an earlier apparent four-versus-six reversal mixed Diplomacy settings. With Diplomacy **off**, on 21 matched core seeds, rule-minus-MLP was **2.1310 VP/seat (SE 0.2726)** at four players and **2.1746 (SE 0.1309)** at six; the paired difference in gaps was **−0.04365 (SE 0.28181)**. This study used a different setup from the holdout table, so its means must not be merged with those cells. It records expansion, force and scoring-opportunity differences as observed mechanisms, without proving that any one card preference caused the gap.

## Speed: local inference measurements, not playing strength

The [main branch inference README](../crates/ti4-infer/README.md) reports an i5-12600K local comparison on real decisions: native FMA **168 µs/decision**, approximate int8/VNNI **27.8 µs/decision**, and libtorch **405 µs/decision**. A rollout game with forward pass and critic measured **142 ms** for FMA, **31.5 ms** for int8, and **279 ms** for libtorch. These are performance observations from that host and workload, not portable throughput guarantees. The README reports that int8 changes the drawn option on about **2.6%** of decisions at temperature 0.001 in its parity sample; its faster number is not policy-equivalent to the FMA result. This inference crate exists on `main` at `f55a5b31`; it is absent from the `wp/base-factions` source snapshot used by most technical wiki pages.

## How to make the next claim stronger

Use the [main branch self-play protocol](RULE_BOT_OPTIMISATION.md) and [GameSpec](../crates/ti4-selfplay/src/spec.rs) to pin seed, map, factions, seat rotation, speaker, card set and rules version. Its smoke run has 10 games, golden benchmark 200, and candidate-versus-shipped A/B command 100 tables at both four and six players. These are **configured run sizes**, not results reported by this page. For a strength claim, retain complete-game termination and win-by-seat/faction, VP-source and decision-subtype summaries, paired per-game records, replay hashes, coverage and failures. Run baseline and candidate on the same spec, disclose changed code and checkpoint, and review uncertainty by seed/map rather than treating every seat as independent.

For the rule bot, subtype `smart` coverage says where a heuristic actually replaced random fallback; high win rate without that count can misattribute gains. For learned bots, capture inference status, vocabulary/bundle identity, temperature and invalid runs. For either, keep round-four VP, full-game win rate and inference latency as three separate claims. The [strategy wiki experiments](strategy-wiki/MLP_APPLICATIONS.md) document narrower Sol probes and their limits.
