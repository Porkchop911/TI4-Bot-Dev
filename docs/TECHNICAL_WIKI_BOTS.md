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

The runner can construct different deciders. Start from the [runner's actual setup](../crates/ti4-sim/src/run.rs) or [server session](../crates/ti4-server/src/session) before attributing an action to a policy. An option can be legal yet have no authored score, or have a vocabulary feature that was not emitted in this position. [Engine observations](../crates/ti4-engine/src/choice.rs) expose public facts and the acting seat's private facts; [model redaction](../crates/ti4-model/src/view.rs) defines hidden identities. The [MLP API boundary tests](../crates/ti4-mlp/tests/api_boundary.rs) instead check that inference without a position or with an invalid temperature fails rather than guessing.

## Reconstruct one surprising choice

1. Save the exact engine prompt, acting player, ordered option IDs and observation. The [choice API](../crates/ti4-engine/src/choice.rs) is the common input to all deciders.
2. Identify the selected path and configuration. For `ScoredBot`, inspect [`raw_score` and `seen_score`](../crates/ti4-policy/src/bot.rs); sampling means the highest score need not be selected. For the rule bot, inspect whether [`pick`](../crates/ti4-rule-bot/src/lib.rs) declined to advise. For a learner, capture the checkpoint and [bundle](../crates/ti4-mlp/src/bundle.rs).
3. Check intermediate features, head and option probabilities, then the submitted ID. A wrong answer can come from a feature, a score, a sample, or a mismatched offer; a legal ID by itself does not distinguish them. [Policy features](../crates/ti4-policy/src/features.rs), [MLP actor](../crates/ti4-mlp/src/lib.rs), and [engine validation](../crates/ti4-engine/src/choice.rs) establish these boundaries.
4. Replay the next game step to see the real consequence. Do not infer value from a single action label or an isolated probability; inspect [game continuation](../crates/ti4-engine/src/game.rs).

## From experiment to strength claim

[Simulation](../crates/ti4-sim/src/lib.rs) supplies game runs, maps, rotations, batches and reports. [Training](../crates/ti4-training/src/lib.rs) contains rollout, reward, gradient, stage loops and evaluation. Their module headers include historical stage notes; inspect the relevant implementation and dated evidence before declaring an area complete. [MLP PPO](../crates/ti4-mlp/src/ppo.rs) is a separate learned-actor path; identify which training path produced a checkpoint.

A reproducible comparison records **commit, content scope, map pool, seed set, roster, seat rotation, bot/checkpoint versions, temperature, game cutoff, failure handling and sample count**. Those are controls, not result metrics. Use [simulation configuration](../crates/ti4-sim/src/run.rs), [rotation](../crates/ti4-sim/src/rotation.rs), [evaluation](../crates/ti4-training/src/evaluation.rs), and [dated evidence](../plans/evidence) to establish them. Compare the result with a baseline on the same setup before claiming an improvement. The [training pipeline](TRAINING_PIPELINE.md) and [operator guide](../plans/PPO_TRAINING_HOWTO.md) explain documented stages and launch details. Check the [compute queue](../plans/COMPUTE_QUEUE.md) before GPU or multi-minute CPU work.

An old benchmark or example checkpoint is not a current strength claim. The [advisor](../crates/ti4-advisor/README.md) can evaluate a supplied legal choice with a checkpoint; its raw critic value is not a win probability. The [bot agent](../crates/ti4-bot-agent/README.md) can submit the selected option to an online game. Both are deployment paths, not substitutes for a controlled tournament.

Publisher strategy advice is a hypothesis source. The [strategy vault route](../AGENTS.md#llm-wiki-workflow) requires citations, setup conditions and separate engine evidence before a training target is accepted.
