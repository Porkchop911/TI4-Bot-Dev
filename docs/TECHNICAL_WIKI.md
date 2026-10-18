# Technical wiki

Source scope: `wp/base-factions` at `1f3556d1`, inspected 2026-10-18. This is a reading map for this checkout. Follow the linked source and dated evidence before claiming a behavior, qualification result or current run status. Other worktrees may differ.

## Start with the task

| Task | Reading route |
|---|---|
| Find where a rule offers or resolves a choice | [Engine and content](TECHNICAL_WIKI_ENGINE.md) |
| Explain a bot decision, simulate, train or evaluate | [Bots and learning](TECHNICAL_WIKI_BOTS.md) |
| Trace a browser update, native review, replay or bridge request | [Interfaces and hosting](TECHNICAL_WIKI_INTERFACES.md) |
| Check a design assumption before editing state, choices or views | [Invariants and failure modes](TECHNICAL_WIKI_INVARIANTS.md) |
| Pick a focused regression test and avoid an expensive unrelated run | [Tests and verification](TECHNICAL_WIKI_TESTS.md) |
| Find acceptance criteria or the status of a workstream | [Planning index](../plans/INDEX.md), then the named plan and its [evidence](../plans/evidence) |
| Find tests or operational commands | The owning crate's `tests/` and manifest; [web tests](../web/package.json), [web2 tests](../web2/package.json), or the relevant [operator guide](../plans/PPO_TRAINING_HOWTO.md) |

## Architecture at a glance

```text
content + model ──→ engine game, state and legal choices
                         ├──→ bots ──→ simulation ──→ training/evaluation ──→ checkpoint
                         └──→ viewer projection ──→ host protocol/session
                                                    ├──→ server ──→ online client
                                                    └──→ wasm ──→ local web2 client
```

Arrows show the useful reading direction, not the full Cargo dependency graph. [The manifest](../Cargo.toml) is the dependency authority; [engine choice](../crates/ti4-engine/src/choice.rs), [host](../crates/ti4-host/src/lib.rs), [server](../crates/ti4-server/src/lib.rs), and [Wasm](../crates/ti4-wasm/src/lib.rs) establish the main boundaries.

The [workspace manifest](../Cargo.toml) declares 22 Rust crates. [Model](../crates/ti4-model/src/lib.rs) defines game types; [content](../crates/ti4-content/src/lib.rs) loads and validates component records; [engine](../crates/ti4-engine/src/lib.rs) offers legal choices and applies answers. [Calc](../crates/ti4-calc/src/lib.rs) supplies combat odds without the training stack.

[Policy](../crates/ti4-policy/src/lib.rs), [MLP](../crates/ti4-mlp/src/lib.rs) and [rule bot](../crates/ti4-rule-bot/src/lib.rs) are distinct decision paths. [Simulation](../crates/ti4-sim/src/lib.rs), [training](../crates/ti4-training/src/lib.rs) and [tensor integration](../crates/ti4-tensor/src/lib.rs) surround them.

[View](../crates/ti4-view/src/lib.rs) projects client-facing state; [host](../crates/ti4-host/src/lib.rs) shares protocol and session logic between the [server](../crates/ti4-server/src/lib.rs) and [Wasm runtime](../crates/ti4-wasm/src/lib.rs). [Web](../web/README.md) and [web2](../web2/src/main.tsx) are separate client codebases in this checkout. [Advisor](../crates/ti4-advisor/src/lib.rs), [bot agent](../crates/ti4-bot-agent/src/lib.rs), [review](../crates/ti4-review/src/lib.rs), [replayer](../crates/ti4-replayer/src/lib.rs) and [bridge](../crates/ti4-bridge/src/lib.rs) are other interfaces.

[Legacy conversion](../crates/ti4-legacy/src/lib.rs) has its own crate. [ti4-cli](../crates/ti4-cli/src/main.rs) and [xtask](../crates/xtask/src/main.rs) are stubs in this checkout; find a command in its owning crate or script instead of assuming these dispatch it.

## Terms used across the pages

| Term | Meaning here |
|---|---|
| **Source set** | The selected component scope used by the [content store](../crates/ti4-content/src/loader.rs) and rules. A record may exist in the corpus but be outside a game. |
| **Choice / option** | An engine-owned question and one of its offered answers. The [option ID](../crates/ti4-engine/src/choice.rs) is what an actor submits; its label is presentation. |
| **Seat observation** | Public position plus facts private to the acting player, provided through [Decider::choose_seeing](../crates/ti4-engine/src/choice.rs). |
| **Projection** | A viewer-specific client representation derived from game state by [ti4-view](../crates/ti4-view/src/projection.rs). It is where visibility must be checked. |
| **Fork / replay** | A continuation from a copied game or recorded decision prefix. [Engine snapshots](../crates/ti4-engine/src/game.rs) and [host step snapshots](../crates/ti4-host/src/step_snapshot.rs) serve related but different paths. |
| **Profile / bundle** | Stored learned-policy parameters. [Linear policy profiles](../crates/ti4-policy/src/learned.rs) and [MLP bundles](../crates/ti4-mlp/src/bundle.rs) have different layouts. |

## Status and evidence

[EXECUTION_STATE.md](../plans/EXECUTION_STATE.md) is a handover document, and [INDEX.md](../plans/INDEX.md) routes to plans. The handover currently begins with a 2026-10-11 state, so verify it against Git and newer [evidence](../plans/evidence) before using it as live status. Source comments and examples can also describe an earlier stage. A module's presence says where to investigate, not that its feature is accepted.

For TI4 rules or strategy claims, follow the [repository wiki procedure](../AGENTS.md#llm-wiki-workflow) and the strategy vault named there. Keep publisher rules, corpus text, strategic advice and observed engine behavior separate.

## How to read a claim here

Each concept page links to the implementation or test that supports its claim. Read a link as evidence for this checkout at the commit above, not as a promise about another branch. When investigating a bug, keep four questions separate: **what the component record says**, **which choice the engine offered**, **what the decider or person submitted**, and **what the host displayed or persisted**. [Content loading](../crates/ti4-content/src/loader.rs), [choice validation](../crates/ti4-engine/src/choice.rs), and [client projection](../crates/ti4-view/src/projection.rs) sit at those different boundaries.

The page for each area has a short reading route, a concrete flow, and a failure-to-test map. Start with the route that matches the symptom; read [invariants](TECHNICAL_WIKI_INVARIANTS.md) before adding persistent game state or exposing data to a client. The [test guide](TECHNICAL_WIKI_TESTS.md) names actual test files and commands. If source and wiki disagree, fix the wiki and record the evidence in the [log](TECHNICAL_WIKI_LOG.md).

Maintenance: update the affected route when a responsibility or entry point changes, verify source links in this checkout, and append the [wiki log](TECHNICAL_WIKI_LOG.md). No generated output or external service is needed to read these pages.
