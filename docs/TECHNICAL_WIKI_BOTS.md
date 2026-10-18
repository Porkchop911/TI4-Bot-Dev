---
type: Component
title: Bots, simulation and learning
description: Explain a bot answer and trace experiments from legal choice to evaluation.
timestamp: 2026-10-18
source_commit: 1f3556d1
confidence: high
resource: docs/TECHNICAL_WIKI_BOTS.md
citations:
  - crates/ti4-engine/src/choice.rs
  - crates/ti4-policy/src/bot.rs
  - crates/ti4-mlp/src/lib.rs
---

# Bots, simulation and learning

[Technical wiki](TECHNICAL_WIKI.md) · [Engine](TECHNICAL_WIKI_ENGINE.md) · [Interfaces](TECHNICAL_WIKI_INTERFACES.md)

Use this route to explain a choice, run a controlled comparison or inspect a learning path. Source scope: `wp/base-factions` at `1f3556d1`, inspected 2026-10-18.

| Path | Start here |
|---|---|
| Authored policy and factual inputs | [ti4-policy](../crates/ti4-policy/src/lib.rs), [bot scoring](../crates/ti4-policy/src/bot.rs), [features](../crates/ti4-policy/src/features.rs), [vocabulary](../crates/ti4-policy/src/vocabulary.rs) |
| Neural policy and learning code | [ti4-mlp](../crates/ti4-mlp/src/lib.rs), [MLP bot](../crates/ti4-mlp/src/bot.rs), [PPO](../crates/ti4-mlp/src/ppo.rs), [distillation](../crates/ti4-mlp/src/distill.rs) |
| Heuristic rule seat | [ti4-rule-bot](../crates/ti4-rule-bot/src/lib.rs); read its documented fallback before treating every answer as a rule choice |
| Games and experiment setup | [ti4-sim](../crates/ti4-sim/src/lib.rs), [run](../crates/ti4-sim/src/run.rs), [maps](../crates/ti4-sim/src/maps.rs), [batch](../crates/ti4-sim/src/batch.rs) |
| Training orchestration and math | [ti4-training](../crates/ti4-training/src/lib.rs), [rollout](../crates/ti4-training/src/rollout.rs), [reward](../crates/ti4-training/src/reward.rs), [tensor adapter](../crates/ti4-tensor/src/lib.rs) |

## Which bot answered?

| Path | How it chooses | Evidence to inspect |
|---|---|---|
| [ScoredBot](../crates/ti4-policy/src/bot.rs) | Authored components score options; a seeded RNG samples at a temperature. `choose_seeing` adds board facts for supported tactical choices. | Actual option scores, shortlist, temperature, seed and any remembered decision. Unknown kinds get an empty component set and remain legal. |
| [Learned policy](../crates/ti4-policy/src/inference.rs) | Uses a trained profile and [feature vocabulary](../crates/ti4-policy/src/features.rs). | Profile/checkpoint identity, emitted features, decision head, source set and scored options. |
| [MLP actor](../crates/ti4-mlp/src/lib.rs) | Batches legal options through a sparse-input trunk and head readout with faction residuals. | Bundle layout and weights, faction **identity** row, feature vector, head, logits, temperature and refusals. The source documents that a planned identity embedding is not wired into the input. |
| [Rule bot](../crates/ti4-rule-bot/src/lib.rs) | Uses a small set of board heuristics; uncovered choices keep the random bot's answer. | Whether `pick` returned an index or `None`, plus the seed and offered options. Its legal-answer property depends on choosing an offered index. |

The runner can construct different deciders. Start from the [runner's actual setup](../crates/ti4-sim/src/run.rs) or [server session](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/crates/ti4-server/src/session) before attributing an action to a policy. An option can be legal yet have no authored score, or have a vocabulary feature that was not emitted in this position. [Engine observations](../crates/ti4-engine/src/choice.rs) expose public facts and the acting seat's private facts; [model redaction](../crates/ti4-model/src/view.rs) defines hidden identities. The [MLP API boundary tests](../crates/ti4-mlp/tests/api_boundary.rs) instead check that inference without a position or with an invalid temperature fails rather than guessing.

## Reconstruct one surprising choice

### Design philosophy and mechanics

The four paths solve different problems. **ScoredBot** is an explainable, authored preference model: components attach reasons and values to supported legal options, then a shortlist and temperature turn scores into a seeded choice. It is useful when a human wants to inspect *why* an option ranked well. Its score is a preference, not a forecast of victory. A missing component for an option kind means zero authored argument, not engine illegality. Read [scoring](../crates/ti4-policy/src/scoring.rs) alongside [bot.rs](../crates/ti4-policy/src/bot.rs).

The **linear learned policy** keeps the engine's legal option set and projects each candidate into the [feature vocabulary](../crates/ti4-policy/src/vocabulary.rs). Per-head weights produce scores and a softmax produces probabilities. This makes a trained profile small and inspectable, but representation limits what it can learn: a named feature that is absent from the runtime vector cannot affect the action. The implementation explicitly avoids the authored score and filter paths; do not explain its action using ScoredBot component weights. See [inference](../crates/ti4-policy/src/inference.rs) and [profile](../crates/ti4-policy/src/learned.rs).

The **MLP actor** learns nonlinear interactions among sparse option features. It computes shared hidden activations, applies the selected decision head and faction residual row, and samples from temperature-scaled logits across offered options. A bundle and vocabulary must match. `MlpBot::seat()` supplies the private decider together with inference status, so a caller can reject a run that silently missed model inference; invalid temperature and empty legal sets are refused. The faction row is a residual in the actor, not an input identity embedding. See [actor](../crates/ti4-mlp/src/lib.rs), [seat boundary](../crates/ti4-mlp/src/bot.rs) and [bundle](../crates/ti4-mlp/src/bundle.rs).

The **rule bot** is a hand-built baseline organized around immediate objective opportunities, expansion, transport, production, tactical movement and resource preservation. In `wp/base-factions`, `choose` draws the seeded random choice first; `pick` may replace it with an offered index, and `None` keeps that random choice. Thus a completed game does not imply every decision had a strategic rule. Its documented aspiration is stronger, reproducible play through explicit heuristics, not an approximation to either learned policy. See [rule bot](../crates/ti4-rule-bot/src/lib.rs).

These are design descriptions from source, not a ranking. For measured outcomes and a reproducible comparison protocol, see [Bot benchmarks](TECHNICAL_WIKI_BENCHMARKS.md). The [strategy wiki bot guide](strategy-wiki/REPO_BOTS.md) adds the rule-bot branch's later planning layers and dated experiments.

1. Save the exact engine prompt, acting player, ordered option IDs and observation. The [choice API](../crates/ti4-engine/src/choice.rs) is the common input to all deciders.
2. Identify the selected path and configuration. For `ScoredBot`, inspect [`raw_score` and `seen_score`](../crates/ti4-policy/src/bot.rs); sampling means the highest score need not be selected. For the rule bot, inspect whether [`pick`](../crates/ti4-rule-bot/src/lib.rs) declined to advise. For a learner, capture the checkpoint and [bundle](../crates/ti4-mlp/src/bundle.rs).
3. Check intermediate features, head and option probabilities, then the submitted ID. A wrong answer can come from a feature, a score, a sample, or a mismatched offer; a legal ID by itself does not distinguish them. [Policy features](../crates/ti4-policy/src/features.rs), [MLP actor](../crates/ti4-mlp/src/lib.rs), and [engine validation](../crates/ti4-engine/src/choice.rs) establish these boundaries.
4. Replay the next game step to see the real consequence. Do not infer value from a single action label or an isolated probability; inspect [game continuation](../crates/ti4-engine/src/game.rs).

## From experiment to strength claim

[Simulation](../crates/ti4-sim/src/lib.rs) supplies game runs, maps, rotations, batches and reports. [Training](../crates/ti4-training/src/lib.rs) contains rollout, reward, gradient, stage loops and evaluation. Their module headers include historical stage notes; inspect the relevant implementation and dated evidence before declaring an area complete. [MLP PPO](../crates/ti4-mlp/src/ppo.rs) is a separate learned-actor path; identify which training path produced a checkpoint.

A reproducible comparison records **commit, content scope, map pool, seed set, roster, seat rotation, bot/checkpoint versions, temperature, game cutoff, failure handling and sample count**. Those are controls, not result metrics. Use [simulation configuration](../crates/ti4-sim/src/run.rs), [rotation](../crates/ti4-sim/src/rotation.rs), [evaluation](../crates/ti4-training/src/evaluation.rs), and [dated evidence](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/plans/evidence) to establish them. Compare the result with a baseline on the same setup before claiming an improvement. The [training pipeline](TRAINING_PIPELINE.md) and [operator guide](../plans/PPO_TRAINING_HOWTO.md) explain documented stages and launch details. Check the [compute queue](../plans/COMPUTE_QUEUE.md) before GPU or multi-minute CPU work.

An old benchmark or example checkpoint is not a current strength claim. The [advisor](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/crates/ti4-advisor/README.md) can evaluate a supplied legal choice with a checkpoint; its raw critic value is not a win probability. The [bot agent](https://github.com/Porkchop911/TI4-Bot-Dev/blob/1f3556d1e1007aad69d658ce3aa4024d141d90c0/crates/ti4-bot-agent/README.md) can submit the selected option to an online game. Both are deployment paths, not substitutes for a controlled tournament.

Publisher strategy advice is a hypothesis source. The [strategy vault route](strategy-wiki/SCHEMA.md) requires citations, setup conditions and separate engine evidence before a training target is accepted.
