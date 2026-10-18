---
type: Invariant
title: Invariants and failure modes
description: Boundaries to preserve when changing choices, game state, projections, or learned models.
timestamp: 2026-10-18
source_commit: 1f3556d1
confidence: high
resource: docs/TECHNICAL_WIKI_INVARIANTS.md
citations:
  - crates/ti4-engine/src/choice.rs
  - crates/ti4-host/src/step_snapshot.rs
  - crates/ti4-view/src/projection.rs
---

# Invariants and failure modes

[Technical wiki](TECHNICAL_WIKI.md) · [Engine](TECHNICAL_WIKI_ENGINE.md) · [Bots](TECHNICAL_WIKI_BOTS.md) · [Interfaces](TECHNICAL_WIKI_INTERFACES.md) · [Tests](TECHNICAL_WIKI_TESTS.md)

Source scope: `wp/base-factions` at `1f3556d1`, inspected 2026-10-18. These are implementation contracts observed in code and tests, not a claim that all TI4 rules are complete.

## Choice ownership and legality

A [Choice](../crates/ti4-engine/src/choice.rs) belongs to one player and has an ordered list of option IDs. The engine's `validate` accepts an offered option, or a specifically permitted constructed free-form answer; it rejects an invented ID. A client or bot must submit the ID for the **current** pending choice, rather than reconstructing it from display text. The [replayer manual choice](../crates/ti4-replayer/src/control.rs) adds a fingerprint so an answer to an older prompt cannot be applied to a new one. [Protocol and session tests](../crates/ti4-server/tests/it/session_rejections.rs) are the online counterpart.

Failure pattern: a UI shows the right label but sends a stale ID, or a policy selects an ID from an earlier offer. Compare prompt, player, ordered IDs, nonce/version and submission at the boundary where the error first appears. [Choice](../crates/ti4-engine/src/choice.rs), [host protocol](../crates/ti4-host/src/protocol/mod.rs), and [server session](../crates/ti4-server/src/session) own different parts of that path.

## Visibility belongs to the viewer

[Model redaction](../crates/ti4-model/src/view.rs) replaces opponents' hidden card identities with placeholders while retaining public counts. [SeatObservation](../crates/ti4-engine/src/choice.rs) binds private facts to the acting seat. [View projection](../crates/ti4-view/src/projection.rs) builds a client view from redacted input, and [host state updates](../crates/ti4-host/src/state_update.rs) attach seat-only reaction settings and auto-resolved notes to that seat's message. A public option or public event can still reveal permitted information, so test the **whole viewer response** for the relevant role rather than inspecting one helper in isolation.

Failure pattern: a projection helper receives raw state and accidentally sends a hand identity to spectators or opponents. Start at [redaction tests](../crates/ti4-server/tests/it/projection_redaction.rs), then check player, spectator and actor responses separately. A bot's `choose_seeing` is a different boundary: inspect [SeatObservation](../crates/ti4-engine/src/choice.rs) and the specific bot's feature extraction. The [MLP API tests](../crates/ti4-mlp/tests/api_boundary.rs) cover refusal when the required position is absent.

## One seeded history must lead to one future

[Game](../crates/ti4-engine/src/game.rs) owns state, RNG, dice, events, timing and in-flight windows. A fork used for previews or batches must continue as the same game would from the same decision prefix. [StepSnapshot](../crates/ti4-host/src/step_snapshot.rs) is captured between steps, flushes deferred RNG synchronization, checks history generation and a log-prefix digest, and otherwise yields to full replay. The [fork equivalence test](../crates/ti4-server/tests/it/fork_equivalence.rs) compares the offers, log, state, events, timing and random behavior of live and forked paths.

Failure pattern: a newly added mutable field is copied in the live game but omitted from a snapshot, or a new random draw uses a fresh generator. Reproduce the same seed and decision prefix, then compare the **first** differing offer or event. A later final-state mismatch is a symptom, not the earliest cause. [Engine snapshots](../crates/ti4-engine/src/game.rs), [host snapshots](../crates/ti4-host/src/step_snapshot.rs), and [fork test](../crates/ti4-server/tests/it/fork_equivalence.rs) are the required reading route.

## Content scope and stable learned identities

A [ContentStore](../crates/ti4-content/src/loader.rs) resolves records under a source set. The [validator](../crates/ti4-content/src/validator.rs) distinguishes references that must be in scope from records that need only exist somewhere in the corpus, and documents known upstream gaps. A content ID existing in JSON does not guarantee it is available in a configured game.

The [MLP faction roster](../crates/ti4-mlp/src/lib.rs) is a pinned mapping from faction **identity** to model row, distinct from physical table seat. Reordering it would reinterpret stored weights. The [policy vocabulary](../crates/ti4-policy/src/vocabulary.rs), [head registry](../crates/ti4-policy/src/learned.rs), and [bundle](../crates/ti4-mlp/src/bundle.rs) are similar serialization boundaries. When changing any of them, identify the corresponding stored schema and migration or refusal behavior before training or loading a checkpoint. [MLP API tests](../crates/ti4-mlp/tests/api_boundary.rs) and [golden policy fixtures](../crates/ti4-policy/tests) are useful checks.

## Decisions about a change

| Change | Ask before editing | Focused evidence |
|---|---|---|
| New rule or option | Which source set, window and acting seat? What offer and resolver change? | [Engine choice](../crates/ti4-engine/src/choice.rs), [game step](../crates/ti4-engine/src/game.rs), affected rule module. |
| New persistent game field | Does it survive snapshot, fork, replay and serialization? Does it affect a public view? | [Game snapshot](../crates/ti4-engine/src/game.rs), [fork test](../crates/ti4-server/tests/it/fork_equivalence.rs), [view](../crates/ti4-view/src/projection.rs). |
| New client field | Which viewer may see it, and how is it versioned? | [Host protocol](../crates/ti4-host/src/protocol/mod.rs), [projection](../crates/ti4-view/src/projection.rs), [redaction test](../crates/ti4-server/tests/it/projection_redaction.rs). |
| New bot feature or head | Is it emitted for this option, stable in a bundle, and limited to the acting seat's observation? | [Features](../crates/ti4-policy/src/features.rs), [observation](../crates/ti4-engine/src/choice.rs), [MLP bundle](../crates/ti4-mlp/src/bundle.rs). |

The [test guide](TECHNICAL_WIKI_TESTS.md) gives a route from each failure class to a practical test. For official TI4 rule claims, use the [strategy wiki workflow](../AGENTS.md#llm-wiki-workflow) and retain its source citations.
