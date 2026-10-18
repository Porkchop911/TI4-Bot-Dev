---
type: Component
title: Battle and trade arenas
description: Explain isolated combat prediction, negotiation teaching, their evidence, and how facts reach the game policy.
timestamp: 2026-10-18
source_commit: a7975206
confidence: high
resource: docs/TECHNICAL_WIKI_ARENAS.md
citations:
  - crates/ti4-calc/src/arena.rs
  - crates/ti4-policy/src/battle.rs
  - crates/ti4-training/src/trade_arena.rs
  - crates/ti4-mlp/examples/trade_teacher.rs
---

# Battle and trade arenas

[Technical wiki](TECHNICAL_WIKI.md) · [Bot designs](TECHNICAL_WIKI_BOTS.md) · [Benchmark evidence](TECHNICAL_WIKI_BENCHMARKS.md) · [Strategy wiki bots](strategy-wiki/REPO_BOTS.md)

Source scope: `main` at `a7975206`, inspected 2026-10-18. Here an **arena** is a bounded learning or measurement task around one mechanic. It is not a separate TI4 ruleset or a player that independently wins a complete game. The battle arena learns what a fight is likely to cost; the trade arena and teacher focus on how to value and construct a negotiation. The full policy still decides whether a battle or deal serves its objective plan.

| Question | Battle arena | Trade arena |
|---|---|---|
| Unit of practice | One specified space or ground fight, repeated with controlled dice seeds | One negotiation opened in a real-game position |
| Main input | Fleets, upgrades, faction effects, damage and relevant public board context | Visible deal terms, participant state, offered legal negotiation options |
| Signal | Simulated win/loss/mutual outcome and expected survivors or losses | A hand-written value sheet; in the later teacher, planned option labels |
| What reaches live policy | Versioned battle predictions and tactical facts on legal action options | Deal-value facts on diplomacy options and a trained diplomacy-head preference |
| Main failure mode | A simulator or feature version mismatches the real fight | A locally attractive deal or step label fails to improve complete games |

## Battle arena: turn combat into usable facts

The [lean simulator](../crates/ti4-calc/src/arena.rs) takes two sides rather than constructing a whole `Game`. It resolves precombat guns, barrage, rolls, sustain and casualties under a declared casualty policy, then reports outcomes. This makes repeated labels practical. `ti4-training` [reexports it](../crates/ti4-training/src/lib.rs) and supplies the wide faction profiles. Its code states a boundary: when effects are off, compare against the engine under the same casualty policy; with effects on, the arena includes some effects that the engine did not yet model at its original implementation date. Agreement must therefore be demonstrated on a declared common scope, not assumed from a matching name.

The [battle predictor trainer](../crates/ti4-mlp/examples/battle_predictor.rs) samples bounded fleet pairs, labels each by repeated arena fights, and fits an outcome/survivor predictor. [Policy battle.rs](../crates/ti4-policy/src/battle.rs) owns the versioned input encoding and a plain-Rust forward pass for play. From a seat's observation it can attach odds, expected losses, unsupported flags and tactical-package facts to candidate actions; the MLP consumes those as option features. The predictor **does not move ships**. The policy chooses among legal activation, movement, cargo or retreat options using these facts alongside its other learned preferences.

Version 8 widened the space input to the seatable faction roster. Earlier predictors cannot silently read that encoding. Unsupported effects and contexts are refused or flagged instead of receiving plausible-looking odds. The version-8 ground network still covers the older six-faction setup. See [wide arena evidence](../plans/evidence/BF-22B-WIDE-ARENA.md) and [correctness review](../plans/evidence/BF-22B-REVIEW-2026-10-11.md). The review found and repaired combatant-context, repair-timing and unsupported-unit defects; retained predictor labels and evaluations predate those repairs, so they do **not** qualify the corrected implementation. A small corrected-source panel exists, but broad corrected-label qualification and wide ground coverage remain open in that evidence.

The [original predictor report](../plans/evidence/ARENA_002_PREDICTOR_2026-09-17.md) measured 0.43 percentage-point attacker-win mean absolute error on its held-out v1 positions. That is a **dated predictor fit metric**, not an engine-agreement guarantee for every rule or a game-strength result. It also records separate engine and ti4calc comparisons and the simulator's explicit scope. A tactical planner can be good at estimating a fight yet still spend a command token badly, leave home exposed, or miss an objective; use paired game evidence before claiming strategy improvement.

## Trade arena: put a value on a proposed deal

The [value sheet](../crates/ti4-policy/src/deal_value.rs) assigns each side an own net gain from received items minus the cost of items given, in trade-good units. It subtracts a share of the partner's gain; that share falls as one's own gain grows. Thus a beneficial two-way exchange can score well without treating every benefit to the other seat as an equal loss. The sheet prices trade goods, commodities, fragments, notes and other supported items using public position and visible terms. Some values are deliberate simplifying judgments: a Support for the Throne is 4.5 trade-good equivalents, a generic action card has a flat base value, and future promises start near zero. These are **policy heuristics**, not official exchange rates or guaranteed VP. The [arena adapter](../crates/ti4-training/src/trade_arena.rs) scores a settled `DealRevision` against the position when contact opened and converts trade-good score to the training return scale.

The [PPO trade arena](../crates/ti4-mlp/examples/trade_arena.rs) uses ordinary games as *vehicles* for realistic positions. It can force contact with a legal partner, lets policies play the negotiation, and records only the negotiation decisions. Each side's settled-deal value becomes the return for that side's recorded steps. Training is restricted to the diplomacy head and deal-value input rows; the first unrestricted run damaged other gameplay, according to the example's source commentary. It does not train the critic or ordinary game decisions. The approach has an exploration problem: a useful deal often requires several correct builder and response steps before any positive return appears.

The later [trade teacher](../crates/ti4-mlp/examples/trade_teacher.rs) addresses that sparse signal by planning a legal, mutually positive deal from what the policy can see and labeling each builder step. Some contacts deliberately carry selfish offers so the responder learns to decline as well as accept. The teacher trains only the diplomacy readout and value-fact rows, with a held-out game-seed slice for decision accuracy. This is **supervised imitation of the value sheet and planner**; it is not proof that the sheet captures long-term table politics or that a trained bot negotiates well against people.

The [wide-roster evidence](../plans/evidence/BF-22B-WIDE-ARENA.md) reports teacher held-out decision accuracy rising from **29.0% to 69.6%** on its collected examples. In its 20-seed × 6-rotation, four-round evaluation, wide-roster VP was **3.169** for predictor v8 and **3.171** after the teacher. That tiny descriptive difference is not a demonstrated gameplay gain; the report's later review also leaves corrected battle-label qualification open. The [original value-sheet plan](../plans/TRADE_ARENA_VALUE_SHEET_2026-09-22.md) records the operator's chosen assumptions and the earlier observation that Diplomacy can consume a large share of decisions. Its opening “nothing implemented” status is historical; the code above is the implementation authority.

## How to investigate or extend either arena

1. Name the mechanic and branch: `main` uses `ti4-calc::arena` reexported by training; older checkouts may have `ti4-training/src/battle_arena.rs`. Preserve the feature version, content scope, roster, and source commit.
2. Check the boundary: battle labels need simulator-versus-engine agreement under matching effects and timing; trade labels need legal options, seat-visible inputs and the value sheet's assumptions. Retain refusals and incomplete outcomes.
3. Measure the relevant result separately: predictor error, imitation accuracy, game VP/wins and runtime answer different questions. Use [the benchmark protocol](TECHNICAL_WIKI_BENCHMARKS.md) for paired full-game claims. Neither arena's local metric alone ranks a bot.
