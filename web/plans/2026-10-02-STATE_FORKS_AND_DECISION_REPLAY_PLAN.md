# State forks, concurrent planning, and retroactive decision replay

**Status:** design and implementation plan; these features are not implemented by this document.

**Exploration baseline:** repository commit `3d592cb`, 2026-10-02.

**Deployment assumption:** one trusted group of roughly four friends, optionally with bots. Server-side Rust evaluation and several private engine runs are an appropriate starting point.

## 1. Recommendation and direct answers

Build a **server-owned draft/branch coordinator around one authoritative timeline**. Store plans as **semantic intents**, evaluate them in private Rust engine runs, and adopt their decisions only through fresh authoritative engine offers. Reuse the existing private batch replay and history replacement machinery.

The three features share this infrastructure, but need different orchestration:

| Feature                          | Branch purpose                                                                                           | How it becomes authoritative                                                                                                               |
| -------------------------------- | -------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Concurrent strategy selection    | Each participant privately composes their primary/secondary choices against a common announced action.   | Reveal the submitted summaries, obtain confirmation of that review revision, then resolve the choices in the engine's rules order.         |
| Next tactical action             | Privately compose activation, movement/cargo, and conditional later steps while the real game continues. | Revalidate at the actual action opportunity; the owner confirms; execute certified segments, pausing for reactions and uncertain outcomes. |
| Retroactive correction/insertion | Reconstruct the past, change a decision or workflow, and try to replay the subsequent intentions.        | Publish a fully validated replacement timeline, or keep a conflicted revision private until it is repaired.                                |

**Answers to the initial questions:**

1. **How does the engine think it is your turn?** Add an explicit, engine-owned hypothetical planning context and reuse the real tactical/strategy workflow producers. Tactical queries already accept an explicit player and hypothetical destination. A forecast can model an isolated action by that player without changing the live turn. Setting `state.active` inside the current live `Game` is insufficient: its open windows, timing priority, turn preparation, and duration counters still describe the original action.
2. **Is a decision failing to apply enough to detect conflicts?** It is a necessary last-line check, not a sufficient definition. A still-legal choice can have a changed target, cost, route, scope, or effect. Distinguish invalid choices, changed assumptions, changed consequences, and temporarily unvalidated continuations.
3. **How should the UI combine these features?** Use persistent workspaces: **Live**, **Strategy selection**, and **Next action**. The next-action draft may explicitly depend on the owner's strategy draft. Give each workspace its own projected board, choices, revisions, and submission destination. A live reaction interrupts attention without deleting the drafts.
4. **When can state updates be merged?** Background revalidation can run at certified observation/workflow boundaries. Adoption into live play requires the correct actual decision opportunity. A sabotage window can trigger a dependency warning, but it is not a valid place to inject a future tactical action or assume the pending card resolved.
5. **Are multiple full engine instances necessary?** Several independent private runs are useful, and the server already creates them for batches and replay. A branch should be durable intent data plus a reconstructible evaluation, rather than a permanently running ordinary `GameSession`. Cheap analytical queries can cover some planning steps. Complete engine checkpoints are an optimization, not a prerequisite for storing branches.

The central rule is:

> Rebase and replay intentions. Do not merge mutated board snapshots.

## Contents

- [Current architecture and useful foundations](#2-current-architecture-and-useful-foundations)
- [Terminology and invariants](#3-terminology-and-invariants)
- [Coordinator and engine responsibilities](#4-proposed-architecture)
- [Engine contracts and safe hypothetical execution](#5-engine-contracts-needed)
- [Drafts, identities, and dependencies](#6-data-model-and-identity)
- [Conflict detection and repair](#7-conflict-detection-and-repair)
- [Revalidation and adoption boundaries](#8-when-to-revalidate-rebase-and-adopt)
- [Concurrent strategy selection](#9-concurrent-strategy-selection)
- [Tactical planning and composition](#10-planning-the-next-tactical-action)
- [Retroactive correction and insertion](#11-retroactive-decision-changes-and-insertion)
- [Randomness and information](#12-randomness-information-and-replay-policy)
- [Protocol, persistence, and concurrency](#13-server-protocol-and-persistence)
- [UI design](#14-ui-design)
- [Additional problems](#15-additional-problems-and-design-decisions)
- [Implementation sequence](#16-implementation-sequence)
- [Verification and completion criteria](#17-verification-and-completion-criteria)
- [Code navigation](#18-code-navigation-and-related-plans)

## 2. Current architecture and useful foundations

These findings come from the implementation, not just older plan documents. Several engine module comments still describe formerly unimplemented mechanics; the current driver does run combat, invasion, production, and strategy effects.

### 2.1 Engine

| Current mechanism                                                                                                                                                                                  | Relevant code                                                                                                                                                             | Consequence for this work                                                                                                                    |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Every actor answers an engine-generated `Choice`; `Table` validates and records it.                                                                                                                | [`choice.rs`](../../crates/ti4-engine/src/choice.rs), `Choice`, `Decider`, `Table::ask_seeing`, `DecisionRecord`                                                          | The existing authority boundary can remain the execution boundary. Plans select fresh offered options rather than supplying state mutations. |
| `DecisionRecord` stores actor, prompt, chosen ID, offered IDs, and optional typed context. It does not store the selected option's complete semantic payload or resulting state.                   | `choice.rs`, `fingerprint.rs`                                                                                                                                             | Useful for exact replay, but insufficient as a durable, cross-state intent or consequence certificate.                                       |
| `DecisionContext` carries actor/source/subtype/phase/round/target/outstanding amounts.                                                                                                             | [`decision_context.rs`](../../crates/ti4-engine/src/decision_context.rs)                                                                                                  | A strong starting point for classification; it lacks a general action/workflow/decision-site identity.                                       |
| `Game` owns private continuation state in addition to `GameState`.                                                                                                                                 | [`game.rs`](../../crates/ti4-engine/src/game.rs), `Game` fields                                                                                                           | Windows, RNG, dice, event allocation, turn preparation, and failed-action state must be reconstructed or checkpointed together.              |
| The strategy owner resolves the primary before a clockwise follower window. A follower's effect can ask several nested questions in one `step()`.                                                  | `game.rs::apply_choice`, `step_secondary`; [`strategy.rs`](../../crates/ti4-engine/src/strategy.rs); [`strategy_cards.rs`](../../crates/ti4-engine/src/strategy_cards.rs) | Concurrent composition needs a new orchestration boundary; allowing several network answers to the current single offer does not solve it.   |
| `StrategySecondaryWindow` exposes one eligible follower at a time and charges the shared strategy-token cost before dispatching the effect. Leadership has a different purchase/eligibility shape. | `strategy.rs::next_choice`, `take_choice`, `secondary_eligible`                                                                                                           | Share eligibility and costs with detached participant planning. Do not reproduce these rules in the server.                                  |
| Hypothetical activation/reachability helpers already take a player explicitly.                                                                                                                     | [`tactical.rs`](../../crates/ti4-engine/src/tactical.rs), `activation_options`, `movable_into`                                                                            | Initial movement planning need not pretend to advance the whole live game to another player's turn.                                          |
| Movement and loading are resumable windows, but finishing loading can immediately sail and roll a gravity rift. Finishing movement can immediately enter cannon/combat/invasion/production.        | `game.rs::apply_tactical`, `sail`, `finish_tactical`; [`transit.rs`](../../crates/ti4-engine/src/transit.rs)                                                              | A planning boundary must exist before uncertain automatic work, not merely before the next `Choice`.                                         |
| Production has per-use capacity, credit, discounts, and payment/placement stages outside the board snapshot.                                                                                       | [`production.rs`](../../crates/ti4-engine/src/production.rs), `ProductionWindow`                                                                                          | Reopening a production window from a board between purchases would lose credit and remaining capacity.                                       |
| Timing is synchronous, nested, depth-first, and priority ordered. Some pending information exists in local Rust stack frames.                                                                      | [`timing.rs`](../../crates/ti4-engine/src/timing.rs), `Resolver::run_window_with_context`; [`reactions.rs`](../../crates/ti4-engine/src/reactions.rs)                     | Cloning structs alone cannot fork a worker suspended inside an arbitrary nested call.                                                        |
| Seeded randomness is domain separated, but individual outcomes and RNG positions are not persisted as a general game entropy journal.                                                              | [`rng.rs`](../../crates/ti4-engine/src/rng.rs), [`dice.rs`](../../crates/ti4-engine/src/dice.rs), `storage.rs`                                                            | Identical history replays deterministically; changed history can shift later randomness within a domain.                                     |
| Units are values, not entities with durable instance IDs. Some option IDs contain vector indexes.                                                                                                  | [`units.rs`](../../crates/ti4-model/src/units.rs), `tactical.rs::movement_options`, `transit.rs::pending_choice`                                                          | Plans should identify interchangeable unit classes/counts and relevant attributes, never reuse old vector indexes.                           |
| Runtime previews distinguish certain/chanced/unknown/unavailable outcomes, but `ChoiceOption.preview` is skipped in serialization.                                                                 | [`preview.rs`](../../crates/ti4-engine/src/preview.rs), `choice.rs`                                                                                                       | Reuse the conceptual contract; a browser planning response needs an explicit projected preview DTO.                                          |

### 2.2 Server and history

- [`session/worker.rs`](../../crates/ti4-server/src/session/worker.rs) owns one authoritative engine on a worker thread. `SessionShared.pending_decision` is a single `Option<PendingDecision>`.
- [`session/decider.rs`](../../crates/ti4-server/src/session/decider.rs) publishes the actor's offer and blocks on that actor's inbox. Other viewers receive a generic waiting status. This is inherently serialized authoritative input.
- `Table::on_observed_offer` lets the worker publish already accepted decisions and the position at a nested offer before an outer `step()` returns. It is an observation/persistence seam, not a complete checkpoint API.
- [`session/batch.rs`](../../crates/ti4-server/src/session/batch.rs) already reconstructs a private `Game`, verifies the recorded prefix, matches typed movement/payment/vote/production steps against fresh offers, and stops at the first unplanned choice. This is the most useful evaluator foundation.
- The batch evaluator has a 100-step plan limit and a 15-second deadline checked **between** engine steps. A single nested/long step is not interrupted by that deadline.
- [`session/registry.rs`](../../crates/ti4-server/src/session/registry.rs) provides per-game gates, atomic timeline replacement, idempotent batch receipts, and host-controlled undo/redo. Batch and history commits stop the old worker and start a replayed replacement.
- Internal autonomous bots and direct `GameSession` callers are not all controlled by the registry gate. Existing commit code checks the cursor again after stopping the worker; this is a race defense, not a general coordinator for groups and drafts.
- [`storage.rs`](../../crates/ti4-server/src/storage.rs) saves initial configuration and decision/event histories. `history.json` holds active and redo branches after replacement; it is atomically written and bounded to 64 MiB. Recovery prefers it over old append-only files.
- History currently has one active timeline and one redo future, rather than a persistent set of revision branches. Making a new decision after undo discards the redo future.
- Existing action IDs are inferred from the last `"action phase"` prompt and decision cursor. They are useful for log grouping but unstable under insertion and insufficient to locate a repeated historical workflow.
- `replay_session` replays chosen IDs and compares V1 decision hashes. V1 excludes typed context; batch prefix replay checks complete records including context. Neither is a complete engine-state/entropy proof.
- `GameState::PartialEq` deliberately excludes the board and other fields. New branch-equivalence checks must use full canonical state checksums and continuation/entropy evidence, not `assert_eq!(state_a, state_b)` alone.

### 2.3 UI and existing semantic planning

- [`web/src/protocol/client.ts`](../../web/src/protocol/client.ts) and [`useGameSession.ts`](../../web/src/hooks/useGameSession.ts) expose one live pending choice and one canonical snapshot. There is no branch-addressed input channel.
- [`GameShell.tsx`](../../web/src/components/GameShell.tsx) owns movement/production execution state and renders actor-only workflow components. Its movement plan resets on history generation changes; workflow-local drafts often reset on nonce changes.
- [`TacticalMovementOverlay.tsx`](../../web/src/components/TacticalMovementOverlay.tsx) already stages ships/cargo and submits a basket. Its `ExecutionPlan` describes execution of a current activation, not a persistent plan for a future action.
- [`TechnologyModal.tsx`](../../web/src/components/TechnologyModal.tsx) locally stages one or two technologies and uses a sequential submission pipeline. Primary research really has sequential prerequisite effects. The engine currently chooses a payment plan automatically in `strategy_cards.rs::paid_research`; the UI cannot promise an arbitrary research-payment basket until that choice is exposed by the engine.
- [`usePipelineRunner.ts`](../../web/src/hooks/usePipelineRunner.ts) stores JavaScript option predicates. Durable server plans need serializable data instead of predicates.
- [`ti4-policy/src/tactical_plan.rs`](../../crates/ti4-policy/src/tactical_plan.rs) already has semantic hull/load descriptions, a joint resource ledger, and an executor. It is useful design precedent, but is policy-layer code: its executor may drop unavailable cargo. Human draft execution should report that as a repair requiring confirmation. Keep shared legality/matching primitives in the engine or a lightweight common module rather than adding policy/ML dependencies to the server.
- The event log already supports a round/phase/action/stage hierarchy and complete visible history. Historical edit affordances can build on its real server cursors, with new stable identities.

## 3. Terminology and invariants

### 3.1 Terms that should remain distinct

| Term                    | Meaning                                                                                                                                        |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| Canonical timeline/head | The accepted engine history currently shared by the table.                                                                                     |
| Draft                   | The owner's editable intentions, assumptions, and dependencies. Exists independently of an engine instance.                                    |
| Evaluation branch       | A disposable private run/result derived from a canonical base and a specific draft revision.                                                   |
| Hypothesis              | An explicitly assumed fact used for forecasting, such as “my Technology selection is accepted” or “no intervening action changes these ships.” |
| Revalidation            | Check a draft against new facts; may produce conflicts without moving its accepted base.                                                       |
| Rebase                  | Rebuild the evaluation on a newer base and replay semantic intentions.                                                                         |
| Adoption/commit         | Apply certified decisions to the authoritative timeline at real offered sites.                                                                 |
| Revision branch         | A candidate replacement history reconstructed from an earlier canonical boundary.                                                              |
| Checkpoint              | A complete resumable engine continuation at a supported boundary; not a projected board.                                                       |
| Segment                 | An executable portion of a plan ending before an unplanned decision, unresolved effect, or other declared boundary.                            |
| Selection group         | A server coordination object for parallel draft submission, reveal, review, and finalization.                                                  |

### 3.2 Invariants

1. Exactly one canonical timeline exists for a game. Draft edits cannot spend its resources, draw its cards, or publish its events.
2. Every adopted answer selects exactly one freshly generated engine option for the correct actor and site.
3. A hypothetical turn is never proof that the player has a live action opportunity.
4. A branch is reproducible from its base, engine/content identity, hypotheses, and intent revisions. Cached state is disposable.
5. Revalidation never silently changes the owner's intention. Repairs that remove cargo, replace payment, change a destination, or add a cost produce a new draft revision.
6. Unknown is distinct from illegal. A pending reaction or future combat outcome can block validation without invalidating the plan.
7. Confirmation authorizes the reviewed content and assumptions, not any future plan that happens to have the same name.
8. Original history and an intentionally edited suffix have different replay contracts. Verify the unchanged prefix exactly; regenerate records for the edited suffix.
9. Projection occurs for each viewer and branch. The host's ability to publish a correction does not grant visibility into other players' hands or private branch diagnostics.
10. Persist a canonical change before publishing it. After an uncertain response, a durable operation receipt distinguishes committed, superseded, and uncommitted work.

## 4. Proposed architecture

```text
GameCoordinator (stable across worker replacements)
  |
  +-- canonical timeline + authoritative worker
  +-- draft store (owner-scoped intent documents)
  +-- selection groups (submit / reveal / review / finalize)
  +-- revision candidates (old prefix + edit + replayed suffix)
  +-- evaluation scheduler + cache
        |
        +-- private engine run for A's strategy draft
        +-- private engine run for B's strategy draft
        +-- private engine run for A's dependent tactical draft
        +-- private engine run for a historical revision

Every client receives:
  canonical projection
  + its own branch projections and diagnostics
  + explicitly revealed selection-group summaries
```

Put branch/group ownership outside `SessionShared`, since replacing a worker currently recreates the session. A registry-owned per-game coordinator is a reasonable first implementation. Its evaluator can share immutable content and initial configuration; mutable engine state, RNG, window progress, and timing usage belong to each run separately.

### Responsibilities

**Engine:** legal options, shared workflow execution, hypothetical-context construction, boundary/site metadata, consequence facts, stop reasons, and entropy/information gates.

**Server:** authentication, branch storage, revision CAS, scheduling, rebasing, group review, history alignment, durable commit, operation receipts, and viewer-specific projection.

**Browser:** stage/edit intentions, select a workspace, render projected consequences/conflicts, and confirm specific revisions. The browser need not contain or run the rules engine.

### Evaluation lifetime

Start with replay-backed, short-lived evaluations and a bounded job pool. For four players, recomputation and a handful of simultaneous instances are reasonable; actual full-game replay latency still needs measurement.

Store at most one primary next-action draft and one current strategy draft per player initially. Alternative plans can later be additional documents with inactive evaluations. Avoid rebuilding an engine on every quantity-button press: save a revision, coalesce rapid edits, and evaluate the newest requested revision.

Use the same evaluator interface for initial replay and later checkpoint acceleration. Do not create ordinary `GameSession` workers for private evaluation: their persistence, subscriber broadcasts, bot behavior, and indefinitely blocking human inboxes are live-play side effects.

## 5. Engine contracts needed

### 5.1 Explicit action, workflow, and decision sites

Extend producer-authored metadata with concepts such as:

```text
ActionScope: action type, owner, logical instance, start/end lifecycle
WorkflowScope: parent action, kind, participant/role, transaction instance
DecisionSite: workflow instance, local step/occurrence, actor, decision kind
BoundaryInfo: observation serial, timing path, supported planning/adoption operations
```

Examples: Technology primary research #1 versus #2; Leadership free-token allocation #2 versus purchased-token allocation #2; one ship's cargo hold versus the next hold; production use #1 versus another use in the same system.

`DecisionContext` already supplies much of the descriptive data. Add explicit context to generic follow/decline secondary offers as well: several of those currently lack it. Preserve the actual strategy-card alias and primary/follower role, including faction substitution.

An identity is not “same actor, same prompt, same target.” Repeated payments can have all three in common. A numeric event/activation counter also shifts after insertion. Give exact replay stable sites within a lineage, and give historical alignment structural anchors plus explicit old-to-new identity mappings.

Some metadata is private. A public boundary status should not name a held reaction card or list another player's legal choices. Expose operation capabilities and generic waiting reasons separately from the private engine timing path.

### 5.2 Capture a coherent observation boundary

Obtain a single immutable input bundle containing:

- canonical lineage/revision and accepted decision cursor;
- exact pending offer/site and observation serial;
- engine-owned state at that offer;
- static map/source configuration and dynamic topology facts;
- open action/workflow metadata and relevant uncertainty gates.

Read this under one synchronization boundary. Separate calls to `current_state()`, `decision_log()`, and `game_version()` can otherwise observe different moments.

The current `on_observed_offer` hook is a useful starting point. Extend the boundary seam to carry the actual `Choice` and producer scope. `Game::legal_options()` alone does not enumerate every nested question raised inside a synchronous effect. A decision cursor alone also misses automatic progress and multiple observations at the same cursor.

Replay to the named offer or pre-effect gate after verifying the accepted prefix, rather than stopping merely when `records.len() == cursor`. Record enough boundary identity to distinguish automatic progress at that cursor. No extra unrecorded human/bot decision may be accepted to reach the requested boundary.

An observation bundle may be sufficient for analytical planning. It must explicitly say whether it is sufficient to create a detached workflow or a complete continuation.

### 5.3 Separate evaluator modes

| Mode                       | Purpose                                                          | Allowed input behavior                                                                   |
| -------------------------- | ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Exact replay               | Reconstruct accepted history or an unchanged prefix.             | Require actor/site/choice record agreement; no fallback selections.                      |
| Hypothetical planning      | Evaluate an isolated future workflow under declared assumptions. | Actor/role comes from a server-authorized planning context; stop at uncertainty gates.   |
| Intent rebase              | Try the draft's semantics against a newer allowed base.          | Regenerate offered options; distinguish changed evidence from unavailable choices.       |
| Historical revision replay | Apply an edit, then align and retry later historical intentions. | Old suffix records become evidence/intents, not immutable expected offered lists.        |
| Canonical adoption         | Certify and publish a plan segment at a real opportunity.        | No synthetic turn, invented opponent answer, or planning-only state enters the timeline. |

Return an evaluation result with a **validated prefix**, per-intent status, safe projected consequences, first stopping boundary, remaining intents, and structured conflicts. Use distinct results for `needs_actor_input`, `waiting_for_live_window`, `chance_boundary`, `hidden_information_boundary`, `conflict`, `unsupported_context`, and `engine_failure`.

### 5.4 Hypothetical tactical context: forcing a turn safely

The useful question is “what could I do from these known facts when I get an action opportunity?”, not “can the current worker be tricked into accepting my action?”

Recommended implementation:

1. Introduce an engine-owned `PlanningScenario::NextTactical { actor, opportunity, assumptions }`.
2. Start from an eligible observation base. Reuse `activation_options` and `movable_into` for the initial menu and reachability.
3. Build an isolated tactical workflow for that actor. Its synthetic activation/turn scopes are explicitly planning-only. Use the same activation, cargo, movement, production, and cost helpers as real play.
4. The factory owns clearing/reconstructing applicable local workflow state. The server does not manually edit dozens of engine fields.
5. Preserve real ownership, token/resource availability, laws, map changes, duration restrictions, and actor-specific abilities. Model upcoming start-of-turn preparation as unresolved unless it is already known and supported.
6. Evaluate deterministic portions; mark pending start-of-turn effects, activation reactions, movement outcomes, combat, and acquisition-dependent production as conditional gates.
7. When the actual opportunity arrives, recreate the plan from the real engine site and validate again. Adopt only the resulting real decisions.

`begin_action_turn` only sets the active seat and increments `turn_seq`. The real `Game` additionally performs technology/promissory hooks, commander checks, `TURN_BEGAN` reactions, transaction handling, and timing lifecycle synchronization. Copying a mid-action `GameState` into a new `Game`, calling `begin_action_turn`, and continuing general play would lose the old continuation and guess these transitions.

Prospective scopes also need an explicit lifetime policy: a bonus tied to the current activation cannot automatically apply to the next activation, while a still-readied once-per-round ability can remain available. Allocate fresh planning scopes and evaluate expiry/usage through shared engine helpers; do not equate the next action with a guessed `turn_seq + 1`. Unknown intervening duration changes remain assumptions until the real opportunity arrives.

Use a forecast labeled **“If you acted from the current known position”** while opponents' intervening actions are unknown. Do not advance their turns with `FirstOption`, auto-pass them, or run bots to manufacture a supposedly exact future.

### 5.5 Detached strategy-participant context

Strategy followers are out-of-turn actors already. Their preview needs a **participant role**, not a fake active turn:

```text
StrategyParticipantContext {
  announced_action,
  primary_owner,
  actor,
  role: primary | secondary | substituted_primary,
  card_alias,
  shared_prerequisites,
  participant_workflow_scope
}
```

Extract/reuse a participant workflow that includes eligibility, follow/decline, token charging, card-specific choices, and faction follow-ups. The canonical driver and planning factory should invoke the same rules implementation. Directly calling `strategy_cards::secondary` without the outer eligibility/cost layer would skip costs; changing `active` to the follower would change timing priority and some ability semantics.

Where a primary changes shared facts, followers can compose provisional intent but exact preview waits for a fixed shared prerequisite. See the per-card matrix in section 9.

### 5.6 Stopping is control flow, not an illegal choice

The existing private batch evaluator marks completion and returns a `DeciderFailed` string at an unplanned offer. This is adequate as a limited discard-and-replay technique, but should not become a general resumable branch API.

There are real swallowing paths: `reactions.rs::slot` converts an inner choice error into `Ok(())`; several effects use `.ok()` or early-return behavior. A “pause” expressed as an ordinary choice error can therefore let an outer effect continue. The resulting `game.state` after the stack unwinds is not necessarily the state at the intended pause.

Two levels of work are useful:

- **Replay-only evaluator first:** capture the result at the exact boundary, latch a terminal evaluation stop, prevent any subsequent answer from being accepted, and discard the engine. Add typed stop/cancellation propagation to supported paths. A stopped snapshot is observational data, never a continuation to resume with `step()`.
- **Resumable engine later:** represent supported nested workflow/timing frames explicitly, so a yield preserves pending effects and local passed/resolved sets. Distinguish `AwaitingChoice`, `StoppedForEvaluation`, and actual engine errors. This makes true checkpoint/fork possible at those boundaries.

Add gates **before** dice/deck/effect execution where planning must stop. Waiting for the next actor choice is too late if a draw or roll happened automatically beforehand.

### 5.7 Checkpoint/fork tiers

| Tier                        | What can be reused                                                                          | Limitation                                                                                                   |
| --------------------------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| Analytical forecast         | Full known observation plus shared read-only engine queries.                                | No promise of an actual future turn or resolved reactions.                                                   |
| Replay-backed branch        | Initial setup, exact prefix, hypotheses, intents.                                           | Replays cost CPU; evaluation engines are disposable.                                                         |
| Quiescent checkpoint        | Complete engine core between supported workflow steps, with no unresolved Rust call frames. | Must include windows, RNG streams, dice, resolver usage, event sequence, map/source state, and driver flags. |
| Arbitrary nested checkpoint | Explicit continuation/timing frames and their pending effects.                              | Requires an engine control-flow refactor; not achieved by adding `Clone` to `GameState` or `Game`.           |

Separate an eventual `EngineCore` from deciders, channels, callbacks, storage, and subscribers. Immutable ability definitions can be shared; resolver usage/progress cannot. Keep replay from initialization as a correctness oracle for accelerated forks.

## 6. Data model and identity

### 6.1 Durable draft document

Illustrative schema, not an existing API:

```text
DraftDocument {
  schema_version,
  game_id, draft_id, owner,
  kind: strategy_participant | next_tactical | history_revision,
  revision,
  base: { lineage_id, canonical_revision, boundary_id, prefix_identity },
  parent_dependencies: [{ draft_id, revision, outcome_node }],
  opportunity: { round, actor, action_scope_or_next_slot },
  assumptions,
  intents: [IntentNode],
  acknowledged_changes,
  lifecycle
}

IntentNode {
  intent_id,
  workflow_anchor,
  kind,
  semantic_selection,
  prerequisites: [intent_id],
  execution_condition,
  prior_evidence
}
```

Store evaluations separately, addressed by `(draft_id, revision, base_boundary, parent_revisions, engine_identity)`. Every response repeats these identifiers; a late result cannot overwrite a newer draft evaluation.

A planning offer gets a **branch-local** offer ID/nonce. It is not the canonical pending-choice nonce. Bind commands to their destination explicitly.

### 6.2 Semantic selections

| Intent         | Persisted meaning                                                                                                                                     |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| Activate       | Destination system; ordinary/free activation role; relevant action opportunity.                                                                       |
| Move           | Plan-local hull slot, origin, unit type, damage/other relevant condition, count, permitted boost use, and route constraints where supported.          |
| Load           | Carrier's plan-local hull slot, actual pickup system, planet/space source, unit class/condition, and count.                                           |
| Research       | Technology alias, primary/secondary role and research ordinal; explicit optional second research and payment/discount intent if the engine offers it. |
| Gain tokens    | Workflow-scoped pool selections or an aggregate desired distribution compiled into legal allocation choices.                                          |
| Pay            | Transaction-scoped ordered planet/payment-face selections and trade goods; preserve credit and shared-budget semantics.                               |
| Produce/place  | Production-use anchor, requested unit quantities, destination placements, payment choices and discount/card intentions.                               |
| Play card      | Held card alias/copy semantics, legal printed timing site, target/effect selections, and prerequisites.                                               |
| Decline/finish | The specific workflow/site being declined or closed; not an unscoped `"decline"`/`"no"`.                                                              |

Resolve each intention to exactly one fresh offer. Equivalent interchangeable units may remap to a different vector index; non-equivalent substitutions need explicit repair.

Apply the freshly generated `ChoiceOption`, not a serialized historical option or a client-supplied payload. Where a route is currently chosen automatically by the engine, a requested route constraint must be checked against that route; it cannot invent a new path-selection capability.

Include all relevant attributes available in the rules: damage, galvanized status, upgrades, Gravity Drive, Ionian use, pickup location, and selected payment face. Current movement baskets do not encode every one of these. Cargo currently stores its actual pickup system internally, but its offered payload's `system` describes the ship origin; extend payloads and distinguish actual pickup systems in option deduplication before promising unambiguous en-route cargo planning. Likewise, expose relevant hull conditions before relying on them for semantic matching.

Do not introduce persistent IDs for every plastic unit merely to solve index churn. Use plan-local carrier slots for cargo relationships and engine value classes for interchangeable hulls. Add entity identity only if a mechanic truly needs it.

### 6.3 Identity/version layers

Keep separate:

- **Canonical lineage and revision:** where the active accepted history lives.
- **Observation boundary serial:** automatic progress can change the position without a new decision cursor.
- **Decision cursor:** counts accepted choices; useful for navigation within a lineage.
- **Logical action/workflow/site IDs:** identify the meaning of a decision.
- **Draft revision:** changes whenever the owner or an accepted repair changes intent.
- **Selection-group review revision:** changes whenever the revealed proposal set/review evidence changes.
- **Evaluation identity:** includes the exact base and parent revisions.
- **Operation/request ID:** durable idempotency for a mutation.
- **Offer nonce:** submission capability for one current offer in one workspace.

The existing `game_version` also changes when offers are published. It is not a sufficient semantic-state version for branch invalidation. Existing `history.generation` increments for batch replacement as well as rewinds; a changed generation must not automatically erase a persistent future draft.

Use exact prefix identity and lineage mapping to tell append-only advancement from a history rewrite. A branch whose parent was rewritten becomes stale and must be re-evaluated even if the cursor happens to be unchanged.

## 7. Conflict detection and repair

### 7.1 Four checks, not one

1. **Site alignment:** Is this the same actor, action, workflow, local decision, role, and timing occurrence?
2. **Legality:** Can the requested semantics resolve uniquely to a freshly offered option?
3. **Assumptions/preconditions:** Are the facts the plan depended on still true?
4. **Consequences/review evidence:** Does the result still match what the player reviewed closely enough to preserve confirmation?

For initial implementation, use conservative explicit assumptions and full evaluation at supported boundaries. Dependency/read-set tracking is an optimization; incomplete read sets must not be the sole reason to retain an authorization.

### 7.2 Conflict and uncertainty taxonomy

| Class                         | Example                                                                                                      | Result                                                                                                    |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| Equivalent remap              | A unit vector index changed; the same class/count of hull is available.                                      | Auto-rebase; no semantic repair.                                                                          |
| Recoverable resource mismatch | One of three selected cruisers was destroyed; two remain.                                                    | Preserve the requested three, show unavailable one, propose reducing/replacing it.                        |
| Dependency cascade            | The carrier died, invalidating its cargo and a later landing.                                                | Report one root problem with linked blocked children.                                                     |
| Changed cost/commitment       | A route now needs Ionian exhaustion; a research discount disappeared.                                        | Still potentially legal, but confirmation is invalid until the owner reviews the new cost.                |
| Changed public consequence    | The destination has a stronger enemy fleet, a law changed ship stats, or a planet changed controller.        | Soft conflict/review warning; may remain executable.                                                      |
| Hard root failure             | Destination was purged, actor has no legal action opportunity, or the last usable production source is gone. | Root/specific branch cannot execute as authored; retain it for retarget/replacement.                      |
| Workflow disappeared          | A recorded casualty/payment question no longer exists after a historical edit.                               | Stop suffix alignment; explicitly resolve whether the obligation vanished or the intent must be replaced. |
| New required question         | A correction introduces fleet-supply removal or a previously absent reaction.                                | Await that actor's input; do not invent an answer.                                                        |
| Chance/unknown future         | Gravity-rift survival, combat result, or future hidden draw.                                                 | Conditional/unvalidated continuation, not an illegal choice.                                              |
| Live timing window open       | Sabotage may cancel the card underlying a draft.                                                             | Defer exact rebase across that event; show its dependency as unresolved.                                  |
| Engine/replay failure         | The unchanged prefix no longer reconstructs under this build.                                                | Operational failure, separate from a player's plan conflict.                                              |

The example “all ships in the system died, so production is impossible” needs rule-specific evaluation: a surviving planetary dock can still produce, and a blockade blocks ships without necessarily blocking ground forces. The hard failure is loss of the relevant **usable production source/placement**, as computed by `production::capacity`, `producers`, and `placements`.

### 7.3 Evidence to retain

For each node, record a bounded explanation of what was reviewed: selected semantics, target, cost, resource reservations, route/boost use, stock/capacity, supported expected public effects, conditions, and relevant uncertainty gates.

Use two evidence layers:

- **Internal certificate:** full authoritative state/continuation identity and chosen-offer evidence needed for validation. Kept server-side.
- **Viewer review evidence:** observable consequences and assumptions appropriate to that actor. Drives the conflict UI and confirmation digest.

A whole-state diff catches too much noise; a missing option catches too little. Start with explicit feature-specific evidence for movement, research, token gain, payment, and production. Unknown consequence coverage should remain labeled partial.

An engine operation returning success/no error is not proof that the plan accomplished its intended effect. Some effects can no-op or produce fewer units than requested. Compare declared outputs with actual produced/placed counts, researched technologies, and token changes.

### 7.4 Repair behavior

- Keep the last authored plan and last reviewed projection while evaluating the new base.
- Return the longest validated prefix and preserve the blocked suffix.
- Continue checking independent nodes where possible; mark descendants of the first unknown gate as conditional rather than guessing their state.
- Offer specific repairs: remove unavailable hull, select replacement carrier, reduce cargo, change payment, change production site, or abandon an affected subplan.
- Accepting a repair increments the draft revision and invalidates any confirmation/dependent evaluation affected by it.
- A changed enemy fleet can require acknowledgment without editing the selected moves.
- Never silently send fewer units, spend another planet, choose a different technology, or finish movement merely because the requested next item disappeared.

For a small group, clarity is worth more than aggressively minimizing confirmation prompts. Auto-accept equivalent remaps and demonstrably irrelevant updates; ask again when cost, scope, public consequences, or declared assumptions change.

## 8. When to revalidate, rebase, and adopt

There are three different operations people may call “auto-merge”:

1. Refresh the live projection and mark dependencies dirty.
2. Automatically rebase a private evaluation and surface differences.
3. Execute a draft on the canonical engine.

Only the first two are background defaults. Executing a future tactical plan requires the actual offered opportunity and owner confirmation; executing a strategy draft requires the group review authorization.

### Boundary matrix

| Live situation                                                 | Background work                                                                   | Exact validation/adoption policy                                                                       |
| -------------------------------------------------------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Top-level action opportunity after start-of-turn work          | Rebase tactical draft on the real actor/opportunity.                              | Best adoption point for activation; validate actual choices.                                           |
| Strategy announcement's cancellation window open               | Compose provisional intents; note cancellation dependency.                        | Do not treat the announced action as guaranteed.                                                       |
| Announced strategy survives; shared prerequisite fixed         | Evaluate participant branches in the corresponding role.                          | Open/review selection group; later canonical execution follows rules order.                            |
| A movement/cargo workflow is open                              | Revalidate the remaining authored movement; keep already executed steps separate. | Adopt current actor's certified segment within this activation.                                        |
| Activation/movement card stack is open                         | Update visible facts and identify changed dependencies.                           | Unrelated future action remains deferred. A planned card is usable only at its own exact offered site. |
| A sabotage window is open                                      | Keep drafts editable; show “awaiting card resolution.”                            | No assumed decline, card resolution, or tactical injection. Resume evaluation after the event settles. |
| Loading completion is about to sail through a rift             | Validate chosen ship/load intent and known capacity.                              | Planning stops before the roll; live execution may perform the real roll, then revalidate later nodes. |
| Cannon/combat/invasion outcome unresolved                      | Refresh public losses and warnings as they become authoritative.                  | Post-outcome landing/production stays conditional; do not use seeded future outcomes as forecasts.     |
| Production entry reactions settled, before first build         | Evaluate production use, full budget, payment and placements.                     | Good production-segment adoption point; preserve the same workflow credit/discount scope.              |
| Mid-payment or mid-placement                                   | Re-evaluate only through the existing complete transaction continuation.          | Do not create a fresh window from board state; do not cross another actor's question.                  |
| Submitted decision in flight / incoherent publication boundary | Mark evaluation stale; keep editing intent.                                       | Wait for a coherent boundary; no canonical replacement based on the partial cache.                     |
| History changed                                                | Invalidate base lineage mappings and dependent certificates.                      | Reconstruct and rebase; old nonces and confirmations cannot execute.                                   |
| Finished game                                                  | Preserve browsing/edit candidates.                                                | Historical revision may reopen play if its fully validated replacement is nonterminal.                 |

### Rebase algorithm

```text
on canonical observation or parent-draft revision:
  mark affected evaluations dirty
  coalesce updates and select newest coherent allowed base
  reconstruct exact accepted prefix if needed
  replay declared parent outcomes/hypotheses
  build the engine-owned planning scenario
  replay semantic intent nodes until a conflict or uncertainty gate
  compare reviewed assumptions and visible effects
  publish result only if draft, base and parent revisions still match
```

At an unsupported boundary, retain the latest valid preview with a visible **“Not yet checked against the current window”** status. Avoid both a false conflict and a false green “valid” badge.

Background replay must not hold the canonical mutation gate. Capture an immutable base, compute outside the gate, and check identities at publication/adoption. If the game moves again, schedule the newest evaluation rather than repeatedly blocking live play to catch up.

## 9. Concurrent strategy selection

### 9.1 Preserve rules order while parallelizing human work

The engine can continue to resolve the primary and clockwise secondaries in its existing deterministic order. Parallelize **composition and review**, not unordered application of effects. This preserves replay behavior and avoids state-patch merging between participants.

The target includes the strategy owner: the owner can draft their primary while followers draft their secondaries after announcement. The simplest first slice opens parallel follower planning after the primary completes. Supporting primary/follower composition from announcement requires a new explicit `StrategyActionWindow`/announcement boundary before the currently inline primary effect.

Handle `STRATEGIC_ACTION_BEGAN` cancellation first for a fully valid group. Provisional drafts can start during that reaction window, but the UI must show that the action might be cancelled. If cancelled, keep useful intentions as inactive drafts rather than implying they were executed.

### 9.2 Group lifecycle

```text
announced / awaiting prerequisite
  -> selecting
  -> revealed(review_revision)
  -> awaiting reconfirmation
  -> finalizing
  -> completed
       or paused_for_live_input / needs_updated_review / cancelled
```

1. **Announce:** create a group bound to the action instance, card alias, primary owner, roster, and rules order. Freeze canonical advancement at a supported boundary while collecting required strategy choices.
2. **Compose:** each eligible participant can answer their own branch offers immediately, including follow/decline and the effect's multiple choices. Edits remain private until submission.
3. **Submit ready:** store that participant's immutable submitted draft revision. This does not yet research a technology or spend a token canonically.
4. **Reveal:** once every required participant has submitted or has an engine-proven no-choice status, publish allowlisted public summaries of the proposal set. Include the owner's primary choices when this stage is supported.
5. **Review:** each human confirms the exact group review revision after seeing the other players' submitted decisions. This is a separate input from “ready.”
6. **Revise:** any participant can revise while finalization has not started. Increment the group review revision, regenerate the candidate summaries, and invalidate prior confirmations. Previously revealed information remains known.
7. **Finalize:** atomically change the group to immutable finalizing state when the last valid confirmation arrives. Reject racing edits against that old revision.
8. **Resolve:** replay the confirmed intents in canonical rules order, with fresh eligibility, costs, and effects. Finish or pause as described below.

An ineligible player is represented explicitly and does not block the barrier. Eligibility must still be checked against shared prerequisite changes; a newly eligible or newly ineligible participant changes the review evidence/roster obligations.

The coordinator's ready/confirm messages are orchestration metadata, not invented engine `Choice`s. Engine replay logs only actual game decisions. Recovery restores a pending group from its durable coordinator record.

### 9.3 Review identity and repeated changes

Bind a confirmation to:

```text
(group_id, review_revision, submitted_draft_revisions,
 base_boundary, ordered_intent_digest, observable_review_digest)
```

A changed plan or relevant base/effect invalidates it. Pure UI rearrangement does not. Initially invalidate all group confirmations when the revealed proposal set changes; four participants make this simple and understandable. Avoid automatic fixed-point negotiation or silently choosing the last revision after a timeout.

Keep a clear list of “ready,” “reviewing,” and “confirmed this version.” A disconnected player can reconnect to the same draft and review revision. Presence does not imply consent, and absence does not auto-decline a secondary. Host-mediated takeover/manual resumption can use existing lobby identity mechanisms.

### 9.4 Finalization and live interruptions

A multi-player strategy action is not universally an all-or-nothing batch. Reactions, draws, faction follow-ups, and new required decisions can interrupt it.

- If private validation proves the entire reviewed group sequence reaches a supported end with no unplanned input, a multi-actor timeline transaction can commit it atomically. This needs a new group record; current `BatchRecord` is single-actor and cannot represent it.
- Otherwise, commit the longest authorized, certified segment and pause at the real live question/outcome. Never answer an unplanned reaction for another participant.
- Keep the group in finalizing/paused state. After live input settles, revalidate the remaining submitted intents. Any relevant changed cost or consequence requires updated review for the remaining work.
- Already committed segments are explicitly shown as committed. Revising those is a historical correction, not a hidden rollback of the group.

Expose this distinction in the UI. Concurrent selection is guaranteed; instantaneous simultaneous resolution of every possible effect is not. A group can reveal “Research Gravity Drive, assuming no cancelling effect” without asserting the result before its legal timing gates resolve.

### 9.5 Card-by-card planning capability

| Card/family                        | Parallel composition                                                                        | Dependency/uncertainty that affects finalization                                                                                                                            |
| ---------------------------------- | ------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Technology                         | Good pilot: primary research sequence and follower research intentions.                     | Second primary research depends on the first. Costs, discounts, substitutions and faction follow-ups must use actual rules; payment choices are currently partly automatic. |
| Leadership                         | Good next pilot: free/purchased token allocations, influence payment, and remaining credit. | Distinguish free allocation from each purchased token. Later fleet-supply/action availability can change with the pools.                                                    |
| Diplomacy                          | Followers can choose which own planets they intend to ready.                                | Primary system choice and global token placement are shared prerequisites. Do not treat follower previews from before them as exact.                                        |
| Construction                       | Stage target structure/placement intentions.                                                | Player-wide plastic stock, placement restrictions, coexistence, and TE primary production require fresh validation.                                                         |
| Trade                              | Stage replenish/decline and primary free-replenishment choices.                             | Primary choices determine participant cost/eligibility and change public commodities; faction effects can couple participants.                                              |
| Warfare                            | Stage recall/redistribution and home-production baskets.                                    | Token removal and pool changes affect later plans. TE Warfare's free tactical action delays its follower window until that action resolves.                                 |
| Politics                           | Stage participation and public primary choices where known.                                 | Actual drawn cards, hand-limit choices, and private agenda inspection are information gates; no draft preview of future identities.                                         |
| Imperial                           | Stage score/participate/draw intentions.                                                    | Scoring can end the game; secret draws and returns cannot all be selected in advance.                                                                                       |
| Faction/leader/relic modifications | Enable through explicit supported role/workflow adapters.                                   | A secondary may substitute the primary or add cross-player choices. Card name alone does not prove independence.                                                            |

Capability is per workflow stage, not one boolean “this card is simultaneous.” Unsupported stages remain normal live decisions while other participants retain their drafts.

### 9.6 Example: Technology plus future movement

```text
Live: B announces Technology.
A: drafts follow + Gravity Drive; marks ready.
C: drafts a different technology; marks ready.
B: drafts primary research #1 and optional #2; marks ready.
Everyone: sees the revealed proposal summaries and confirms review revision 7.
A meanwhile: next-action draft depends on A's submitted strategy revision 3.
            Its movement forecast includes Gravity Drive and reduced resources.
Canonical: resolves the strategy in normal rules order.
A's next-action draft: parent assumption is replaced with the real resulting facts.
                       Rebase shows clean / changed / blocked without losing intent.
```

Changing A's technology to Sarween Tools immediately invalidates the dependent Gravity Drive move forecast. A changed strategy proposal is still editable; the already-started canonical action is not.

## 10. Planning the next tactical action

### 10.1 Represent a workflow graph

```text
planned optional start/activation cards
  -> activation target
  -> ordered hull moves + per-hull cargo
  -> [activation/movement responses and chance gates]
  -> [space outcome]
  -> conditional landings
  -> [ground/exploration outcome]
  -> conditional production + payment + placement
```

Some intentions can be fully validated against current known facts; others are editable placeholders with conditions. A plan can be useful long before all of it is executable.

For example, stage a production basket now with **“produce here if my usable producer survives and this production opportunity occurs.”** It should not predict conquest, create a dock, or consume a future exploration bonus merely to make the basket legal. If the producer is already gone, report the present hard conflict.

Initial support should be activation + movement/cargo, then production in currently controlled/known production contexts. Extend conditional landing and postcombat production after the gate/dependency model is proven.

### 10.2 Reservations and competing own drafts

Within a composite forecast, account for:

- strategy token/resource/influence spending;
- one-time planet exhaustion and retained transaction credit;
- tactic token for activation;
- once-per-activation movement boosts and once-per-round card exhaustion;
- shared cargo stock, carrier capacity, and fleet supply;
- production capacity, resources, discounts, placements, and plastic supply.

The ledger describes **the owner's forecast**, not reservations against the canonical game or other players. Two alternative drafts can be individually feasible but mutually exclusive; show that relationship rather than blocking live transactions with a reservation system.

Move order matters. Gravity Drive use, cargo assignment, route changes, capacity left in origin systems, and rift losses can change later offers. Store an order where the engine cares about it. A basket's totals alone are not an execution script.

### 10.3 Parent dependencies

Use a small dependency graph rather than copying synthetic state from one branch into another:

```text
canonical base
  -> A's Technology participant draft revision 3
      -> A's next tactical draft revision 12
```

Evaluate the parent's supported outcome first, then the child's intents. The parent reference pins a revision and outcome node. A parent revision change marks the child dirty and invalidates affected review evidence. Prevent dependency cycles.

By default, another player's unpublished plan is not a dependency A can inspect. Explicitly revealed group decisions can be displayed as assumptions, with the group review revision attached. Never make a combined board by copying every player's independent final state over the canonical board.

Once a parent commits, re-evaluate the child against the actual canonical result and retire the hypothesis. If the actual parent differs, show the difference. A clean hypothetical parent result cannot certify a future child's live choices.

### 10.4 Execution when the turn arrives

1. Match the actual action opportunity, including additional-action/retained-turn cases. It is not enough for `state.active == owner` during a reaction or secondary.
2. Rebase against the actual post-start-of-turn boundary and require the planned action to be offered. Use actual opportunity eligibility: a passed player ordinarily has no turn, but a special granted action such as Puppets on a String can legitimately act while the passed flag remains set.
3. Show a final summary of changed targets, costs, stock, and conditions; require confirmation of the current draft/evaluation.
4. Commit activation at its offered site. It places a real token and can open other players' windows.
5. Pause for those live windows. Resume only after rechecking the remaining intentions against the same actual action/workflow.
6. Apply later certified segments, each ending at the first unplanned input or declared uncertainty gate.
7. Keep executed nodes immutable and remaining nodes editable. “Reset draft” does not undo already committed activation/movement.

For a completely deterministic, fully covered segment, reuse/extend the atomic batch path. For an action with live reactions, segmented execution is necessary. Do not promise that confirming a long tactical plan atomically commits the entire action.

Optional auto-execution at the next turn should be a later, explicit player preference bound to a certificate and assumptions. Initial behavior is **ready to confirm when the opportunity arrives**, which prevents a stale plan from announcing an unintended activation.

## 11. Retroactive decision changes and insertion

### 11.1 Why ordinary redo is insufficient

Current undo/redo reconstructs the original choices. If `A -> B -> C -> D` becomes `A -> B' -> C' -> D'`, the chosen semantics of C/D might survive, but their offered lists, contexts, costs, event cursors, and effects may differ. Strict equality against the old C/D records would reject many valid repairs; replaying only their old option IDs would accept some wrong ones.

Use two phases:

```text
unchanged prefix: exact historical replay and integrity check
edited suffix: semantic alignment, fresh legality, consequence comparison,
               new DecisionRecords/events and old-to-new mappings
```

Keep the old timeline available while building the candidate. The current active/redo split is not enough to preserve two different completed suffixes.

### 11.2 Correction workflow

1. Select a historical decision or a meaningful workflow in the log. Prefer editing the whole allocation/payment basket when several microchoices represent one decision by the player.
2. Bind the target to lineage + stable decision/workflow identity; record the original cursor as a locator, not its only identity.
3. Reconstruct the unchanged prefix from the saved initialization and actual map/source configuration.
4. Expose the historical offer/projected position to its legitimate actor and create a candidate revision. Use facts held at that time, not today's hand/resources.
5. Apply the replacement intent through fresh legal options at the historical site.
6. Try subsequent saved intentions in original order, scoped to their old actions/workflows and explicit prerequisites.
7. At the first unsupported alignment/new required input, pause the candidate and retain the remaining suffix. Other actors can repair their own historical decisions through actor-scoped candidate offers if supported.
8. Generate a summary: replayed unchanged intentions, equivalent remaps, changed public consequences, required repairs, obsolete decisions, and unvalidated remainder.
9. Once fully validated to the desired present boundary, the host publishes it using expected canonical head + candidate revision. The table's agreement remains table talk; no consensus-vote subsystem is needed.
10. Atomically replace the active timeline, allocate new suffix event identities, publish replacement projections, and invalidate/rebase live drafts referencing the old lineage.

Keep live play running while authoring a candidate if convenient. If the canonical head changes, mark the candidate outdated and extend/revalidate its tail before publication. A short explicit table pause can be offered during final review; never hold a mutex while waiting for people to repair decisions.

### 11.3 Aligning the suffix

For each remaining historical intent:

1. Find the expected structural action/workflow/actor site.
2. If the next required site is new or belongs to an unexpected actor, stop for input.
3. Match the historical semantic selection uniquely in the regenerated offer.
4. Apply and record a new decision, plus old-to-new provenance.
5. Compare public/actor-visible consequences and declared assumptions.

Generic IDs such as `no`, `yes`, `decline`, `fleet_tokens`, or `move|16|0` are particularly unsafe to consume positionally. A newly introduced “no” question could otherwise absorb a decline intended for a later strategy secondary.

Do not search arbitrarily forward until an old ID becomes legal, and do not silently skip missing questions. A disappeared obligation can be marked **obsolete** only when the engine's workflow transition demonstrates that it was discharged/removed; include this in revision review. Ambiguous repeated scopes require explicit repair.

Use persisted semantic evidence for new games. Where an older record lacks it, reconstruct the original offer in exact replay to derive the intent. Unsupported/private cases can fall back to manual revision of the affected action rather than guessing.

### 11.4 Leadership example

```text
Old: A puts a Leadership token into fleet; later A activates #22 and moves 3 ships.
Edit: A puts that token into tactic instead.
Replay: token allocation is legal; activation may remain legal.
        Fleet/capacity consequences and later removals now differ.
        If a new removal question is required, candidate pauses for A.
        The later movement can also remain offered but exceed the intended fleet allowance.
```

That is a conflict even if `gain_command_token -> tactic_tokens` and all three moves were individually offered. Replay must include automatic enforcement and new obligations. Editing a purchased token to another pool should preserve its original influence payment/credit unless those choices were explicitly part of the edit.

### 11.5 Inserting a decision is often editing an opportunity

Different cases require different treatment:

| Desired insertion                                     | Engine interpretation                                                                                                                              |
| ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Play a reaction where you previously declined         | Replace the decline at that exact timing site; record the card selection/targets and all new nested responses.                                     |
| Use an additional optional effect in an open workflow | Reopen the matching legal opportunity, not an arbitrary timestamp.                                                                                 |
| Play an `Action` card before an old tactical action   | The card usually consumes the action. The old tactical action cannot simply follow in the same turn unless an actual extra-action rule permits it. |
| Add a free transaction before an action               | Must be offered and obey adjacency/turn transaction limits; it may change later costs/ownership.                                                   |
| Play a card not held at the historical point          | Not a legal insertion even if it is held now.                                                                                                      |
| Insert Sabotage after the original card's effect      | Reconstruct and edit the original WHEN opportunity, before that effect resolves.                                                                   |

An inserted action can change turn order/opportunities for every subsequent action. This is why general insertion belongs after workflow identities, suffix alignment, and chance policy are working.

### 11.6 Review, audit, and receipts

- Preserve the old completed suffix as an archived revision, with `derived_from`/`replaces` relationships.
- Keep shared prefix IDs where possible; allocate fresh identities for regenerated suffix events and maintain explicit cursor mappings.
- Rebuild event facts/grouping from the actual new offers/results. Old public descriptions, batch ranges, and action IDs can be stale.
- Preserve historical batch/request receipts as active, undone, or superseded. A delayed retry must not execute again merely because its original batch was removed from the active branch.
- The publication actor can be the host; private repair choices still belong to their original decision actors. “Host review” shows public differences and generic private-input status, not other hands or selected hidden objectives.
- Undoing a correction means selecting the archived original revision, subject to the same head check. It is distinct from undoing one engine choice in the corrected timeline.

## 12. Randomness, information, and replay policy

This is a gameplay requirement, not primarily a compute problem. Even in a trusted hobby game, a planning UI should not accidentally show tomorrow's card or exact combat outcome.

### 12.1 Planning must not use authoritative future entropy as a preview

A private fork copied from the true seed/decks can determine a future roll or draw. Redacting the final board is insufficient:

- a draw can become a new card in the planner's own visible hand;
- future prompts/legal options can depend on that card;
- a casualty/production conflict can reveal whether a future fight succeeded;
- whether an opponent receives a reaction prompt can reveal held cards;
- returning a different result for repeated probes can become an information oracle.

Planning should stop before these effects or use explicit public analytical/conditional summaries. Combat odds can use the existing independent advisor/predictor approach with supported public facts; they are not exact outcomes from the game's RNG stream.

Do not auto-decline hidden reactions because the fork knows who has a card. Treat the relevant opportunity as a generic unresolved reaction gate. A player may author their own known reaction intent; it still needs the actual legal timing opportunity.

### 12.2 Projection gap to address deliberately

Current `projection::redacted_state` delegates to `view_for`/`redact_player`, which scrubs other players' hands but retains `GameState` facedown decks and `rng_seed`. Those raw states are attached to snapshots, updates, and actor choice messages. The BYOA plan explicitly deferred draw-deck scrubbing.

This deferral needs to be revisited before claiming information-safe planning. A branch-specific projection alone cannot preserve unknown outcomes if the ordinary client already receives the seed/deck order. Preserve the advisor's observed-feature input contract with unknown markers/counts or a sanitized observation DTO, and verify advisor/bot consumers against that shape. Keep full replay/checkpoint state internal.

Also distinguish **own holdings already known** from **new hypothetical own holdings**. Ordinary hand redaction is not the only visibility rule a planning projection needs.

### 12.3 Randomness after historical edits

The same seed gives the same stream, not necessarily the same later semantic outcomes. Inserting a combat/rift/reroll can consume extra dice and shift later rolls. Changing card draw order changes recipients even when the deck sequence is unchanged. Domain separation reduces unrelated shifts but does not preserve meaning within a domain.

Recommended staged policy:

1. **Historical correction pilot:** support edits with an unchanged relevant entropy/information trace. Replay automatic effects and compare their observable consequences; equal decision hashes alone do not establish this. Stop and report a changed/new chance or private-information obligation that is outside supported revision handling.
2. **General revision support:** add an internal entropy/outcome journal, keyed by logical occurrence and purpose rather than decision cursor or raw event number.
3. **Preserve compatible historical outcomes:** reuse already realized outcomes only when their occurrence and random experiment remain compatible. A combat with different dice count/type/targets is not automatically the same experiment.
4. **Handle new/incompatible occurrences explicitly:** define fresh-entropy or replay-from-affected-action behavior. Include changed consequences in revision review; never silently claim all later rolls were preserved.

The journal should describe domain, logical occurrence, parameters, outputs, and draw/card provenance. Decks are ordered shared objects: journal compatibility must preserve unique card ownership and actual deck transitions, not merely assign “the old draw” to each player independently.

Keep the chosen policy deterministic once a revision commits. Do not obtain a new favorable outcome by repeatedly evaluating an unchanged candidate. A new entropy occurrence should have stable candidate identity and an internal recorded result, while future private results remain concealed from draft projections.

Information already seen cannot be undone. Archive/review that an edit changes previously observed outcomes, and let the friends decide how to handle it through table talk. The engine should enforce coherent rules/history without pretending to erase players' memories.

## 13. Server protocol and persistence

### 13.1 Commands and projections

Use authenticated HTTP mutations, analogous to current baskets/history, and WebSocket projection updates. Proposed command families:

| Operation                        | Required identity/precondition                                                           |
| -------------------------------- | ---------------------------------------------------------------------------------------- |
| Create/read/edit draft           | Current player credential, owner, draft kind; edits use expected draft revision.         |
| Answer a branch offer            | Draft ID/revision + branch-local offer capability + semantic selection.                  |
| Request revalidation             | Draft revision and requested/latest supported base; result is asynchronous if necessary. |
| Mark strategy draft ready        | Group ID + draft revision; stores an immutable submitted proposal.                       |
| Confirm strategy review          | Group ID + review revision/digest.                                                       |
| Execute tactical segment         | Draft/evaluation identity + real canonical site/head + request ID.                       |
| Create/edit historical candidate | Original lineage/site plus candidate revision and actor-scoped edits.                    |
| Publish historical candidate     | Host credential, expected canonical head, candidate revision/certificate, request ID.    |

Possible routes are `/api/games/{game_id}/drafts`, `/selection-groups/{group_id}/...`, and `/revisions/{revision_id}/...`; exact URL naming should follow surrounding server conventions.

Provide a reconnect response with canonical projection, own drafts/evaluation status, current group review state, and operation receipt status. Keep private branch events separate from the public gameplay log.

Branch update messages use their own revision ordering. The current client reducer rejects messages by global `game_version`; blindly applying that rule to branch messages would discard valid results or clear a branch offer when another player receives a live choice. Validate both workspace identity and relevant canonical base.

Explicitly bump/coordinate protocol schemas: Rust DTOs and web decoders currently deny unknown fields in several places. Bots must understand new group/capability messages or use a documented serial-input adapter.

### 13.2 Durable storage

For the hobby deployment, retain file storage:

- authoritative timeline and committed mutation receipts in one atomic `history.json` generation;
- owner-scoped draft/group metadata in atomically replaced coordinator records;
- immutable archived timeline revisions written before an atomic active-head/reference update;
- evaluator outputs/checkpoints as disposable caches.

Publishing a strategy group can update canonical history and coordinator status across separate files. Make canonical receipts authoritative: after a crash, reconcile a stale “finalizing” coordinator record against committed group/segment receipts instead of executing the choices twice. Similarly, a draft marked executed only after response delivery must recover its actual status from the canonical receipt.

Archive files must be durable before `history.json` references them; a failed head write leaves the old timeline active. Bound draft size, archived revisions, and cache retention separately from the existing history limit. Preserve original saves when a migration fails.

The current persistence envelope pins content identity and package-version rules identity. Include a meaningful engine/replay schema/build identity for new certificates/checkpoints; `0.1.0` alone can span rule changes. Cache invalidation and exact replay failure must not be mistaken for player conflicts.

### 13.3 Concurrency and commit algorithm

Use short coordinator locks and optimistic evaluation:

1. Authenticate and capture immutable canonical/draft/group input under the per-game coordination boundary.
2. Run private replay/evaluation outside the canonical commit gate.
3. Reacquire the gate; check current credential, head lineage/revision, boundary/site, draft revision, parents, and review authorization.
4. Quiesce the canonical worker at a supported boundary. Internal bots must honor this mechanism too.
5. Recheck the authoritative prefix and in-flight state; reject a stale candidate rather than overwriting new decisions.
6. Persist the complete transaction/receipt once.
7. Install/start the replacement continuation and publish viewer-specific snapshots/status.
8. Recover from the durable new head if worker installation fails; report “committed, session unavailable” distinctly from “not committed.”

Do not hold the gate across a human inbox wait. The current registry intentionally releases it after reserving a choice so nested decisions can submit. A naive “hold the gate for the whole action” would deadlock that flow.

Unify direct submissions, internal bots, batch adoption, history edits, group finalization, and credential rotation under the same boundary protocol. A bot that races a rewrite should consume the new offer, not append an answer to the discarded timeline.

Make request IDs payload-bound. Reusing an ID with different semantics is a conflict; an identical retry returns its original receipt even if its timeline has since been superseded. Limit retries for a moving canonical head; surface “base advanced, review current evaluation” rather than silently changing what was confirmed.

### 13.4 Resource limits and responsiveness

The existing 4 KiB WebSocket input and 8 KiB HTTP body limits may be too small for a complete multi-stage plan. Choose explicit bounded branch/segment sizes, or use per-node edits rather than one unbounded document. Keep limits visible in protocol errors.

Use bounded evaluator jobs and cancellation tokens, retain only the newest scheduled revision, and check budgets inside supported nested loops. A timeout around a thread does not safely kill that thread. If an individual engine call cannot cooperatively stop, classify it unsupported for interactive planning or isolate the evaluator process; do not leave unlimited abandoned jobs running.

Suggested initial measurements: replay to late-game boundaries, a 50-intent basket, four strategy evaluations plus four dependent tactical drafts, and a historical suffix with combat. Set actual latency budgets from those results; do not assume cloning four games makes nested-step blocking disappear.

## 14. UI design

### 14.1 Workspaces and urgency

```text
[Live: B resolving Technology] [Technology: Ready / Review needed]
[Next action: #22 · depends on Technology · 2 conflicts]
```

- **Live:** canonical map/state/log and real currently required decisions.
- **Strategy selection:** the participant's private draft and the revealed review table.
- **Next action:** hypothetical map overlays, staged quantities, budget, dependencies, and unresolved gates.
- **Revision editor:** historical baseline/candidate comparison and replay progress, opened from the log.

A required live reaction gets the urgent banner/focus priority. Preserve the active planning workspace/draft and offer a clear return path after the reaction. Editing a next action must never send an option through `submitChoice` just because it is rendered using the same component.

Keep workspace status visible even when minimized. On narrow screens use one active board view with persistent workspace tabs/badges, rather than several stacked modal dialogs.

### 14.2 Reuse renderers with an explicit data/submission context

Introduce a workspace context containing:

```text
mode, workspace_id, projection_identity,
projected board/players, offer, draft revision,
capabilities, validation status, submission callback
```

Feed it to the existing activation, technology, movement, payment, and production renderers. Board hit targets, inspector details, quantity limits, selected options, and callbacks must all use the same workspace projection. The canonical header continues to show whose real turn it is even while the planning map displays a synthetic activation.

Move persistent intent state into a draft store above renderer lifetimes. Component unmounts, new live nonces, worker replacement, and history generation updates must not erase it. Keep transient focus/hover/minimize state local. Persist drafts server-side so refresh/reconnect recovers them without requiring local storage of projected private state.

### 14.3 Visual conflict model

Use player-facing language rather than raw engine errors:

- **Clean:** “Checked against current position; waiting for your action opportunity.”
- **Changed:** “#22 now contains two additional enemy cruisers. Your selected move is still legal.”
- **Repair needed:** “Carrier from #16 is no longer available; 2 infantry depend on it.”
- **Blocked root:** “No usable production source remains in #22.”
- **Conditional:** “Production will be checked after combat and production-entry effects.”
- **Deferred:** “This preview predates the open card window; awaiting resolution.”

Show a compact three-way comparison: **previously reviewed**, **new live facts**, and **rebased result**. On the board, use ghost/dashed planned units and arrows, unavailable-source markers, and affected-system outlines. Never render hypothetical units as canonical pieces.

Provide per-conflict repair controls and a change ledger (“remove 1 unavailable cruiser,” “use a different planet,” “keep the plan and acknowledge stronger opposition”). A repair stays visible until accepted; do not instantly clamp away vanished unit counts.

Separate validity from opportunity: a perfectly valid plan can still be waiting; an available live opportunity can still have a conflicted draft.

### 14.4 Strategy review table

Show participant, public proposed decision/effect, paid/conditional commitments appropriate for reveal, and readiness/confirmation status. Do not reveal full option lists, hands, or private follow-ups.

Buttons have distinct meanings:

- **Ready to reveal**: submit this draft revision.
- **Revise selection**: creates a changed proposal before finalization.
- **Confirm this version**: reconfirm after seeing the revealed decisions.
- **Resolve live follow-up**: answer an actual intervening offer during finalization.

Show “Proposal changed; review version 8” prominently when confirmations are invalidated. Announced/revealed proposals remain labeled proposals until their corresponding canonical effects commit.

### 14.5 Historical editor

Add **Edit decision/workflow** and **Insert at this opportunity** beside eligible historical entries, with actor/host capabilities respected. Display the historical board, original selection, proposed replacement, and replayed tail.

Group downstream results by action: “replayed,” “changed outcome,” “needs A's input,” “no longer applicable,” and “not checked yet.” The final publication screen states the original and candidate heads and changed public results. It does not ask every player to perform an in-game consent vote; the host applies the agreed table correction.

Keep history controls scoped: draft undo edits a draft; canonical Undo uses existing shared history; restoring an archived revision switches the canonical lineage. Do not overload one Undo button with all three meanings.

## 15. Additional problems and design decisions

| Problem                                                                | Proposed treatment                                                                                                                                                                                                                                                                  |
| ---------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| New legal alternatives appear without invalidating the selected one.   | Show an informational “new options available” hint where useful; do not force the player to change an otherwise equivalent plan.                                                                                                                                                    |
| A legal option's payload changes while its ID remains stable.          | Compare fresh semantic payload/cost/effect evidence. ID membership and V1 decision hash are not enough.                                                                                                                                                                             |
| A player gets an extra action or has their turn skipped.               | Match explicit action opportunities and scopes; retained turns are not necessarily new `turn_seq`s. Rebase/expire the draft rather than auto-run it at the wrong time.                                                                                                              |
| Same system has sequential productions with identical prompts.         | Use producer-issued production/payment workflow instances, preserving per-use credit and discounts.                                                                                                                                                                                 |
| Simultaneous plans overlap shared objects/decks.                       | Resolve confirmed intentions through canonical order; use shared prerequisite stages and information gates. Do not union independent effects.                                                                                                                                       |
| A player's strategy and tactical plans double-spend a planet or boost. | Compose them through parent dependencies and one forecast ledger; report contention.                                                                                                                                                                                                |
| Parent draft changes after child review.                               | Pin revisions; invalidate affected child certificates and show the changed assumption.                                                                                                                                                                                              |
| Other players' current action is only partly resolved.                 | Use the latest coherent permitted base; mark pending consequences unknown and refresh as they become real.                                                                                                                                                                          |
| Another player has an optional private response.                       | Report a generic reaction gate, independent of the hidden hand; do not infer exact future offer existence to the planner.                                                                                                                                                           |
| Bot support in a review barrier.                                       | Internal/external bots use participant draft/review APIs or a serial adapter. Bots may auto-confirm a specific revealed revision by policy, while humans still reconfirm. Replay prior bot decisions as saved intentions; do not rerun their policy throughout an unchanged suffix. |
| Two browser tabs edit one owner's draft.                               | Expected-draft-revision CAS; return the latest document and local edit conflict.                                                                                                                                                                                                    |
| Credential takeover during evaluation.                                 | Associate drafts with stable player identity, authenticate again at delivery/commit, and stop sending private updates to revoked credentials.                                                                                                                                       |
| Head advanced while a historical candidate was repaired.               | Revalidate the additional tail or require a new review; never truncate the new live choices silently.                                                                                                                                                                               |
| Save/build changes invalidate cached branches.                         | Invalidate caches; verify exact replay before re-evaluation. Distinguish this from gameplay conflicts.                                                                                                                                                                              |
| Corrections affect game victory or phase transitions.                  | Recompute terminal/phase state and regenerate events; stop aligned suffix when the candidate ends earlier or newly requires input.                                                                                                                                                  |
| Protocol reconnect delivers old results after a rewrite.               | Check lineage + workspace + revision, replace canonical events, and retain/rebase owner drafts explicitly.                                                                                                                                                                          |
| Draft conflict details disclose a hidden cause.                        | Project bounded, actor-authorized reasons; public/host status can say “requires private input” without card identity or contents.                                                                                                                                                   |
| Successful commit response is lost.                                    | Durable payload-bound operation receipts; retries identify active/undone/superseded results.                                                                                                                                                                                        |

### Alternatives and why the recommended combination fits

- **Browser-only drafts:** cheap and useful for rough staging, but cannot cover engine-dependent legality, persistence, cross-feature composition, and historical replay. Keep them as input buffers around server draft documents.
- **Patch/delta merging:** tempting because most choices spend a few resources or move a few units; unsafe for timing, decks, ownership, discounts, and derived effects. Use deltas for display/conflict evidence, never as the canonical merge primitive.
- **Fast-forward other players:** produces one invented future and can expose their private behavior. Suitable only for an explicitly separate analysis tool, not correctness validation of this feature.
- **Direct `active` override:** useful only inside an engine-owned, clearly synthetic planning scenario with proper scope/reset logic. Not a general bypass of turn legality.
- **Complete resumable engine refactor first:** valuable long term but unnecessary before replay-backed drafts and allowlisted shared workflows. Profile and expand checkpoint support after functional pilots.
- **General read/write-set or MVCC merger:** more complexity than this deployment needs; game rules also have semantic dependencies that ordinary field overlaps miss. Explicit workflow gates plus intent replay give clearer failures.
- **WASM engine plus projected-state synchronization:** would add build, content, transport, and private-state consistency work. Server evaluation already aligns with the current authority and the small-group deployment.

## 16. Implementation sequence

Each package should leave a usable narrow slice. Enable capabilities only for the workflow stages actually covered.

### P0 — evaluator/boundary correctness foundation

- Extract the common exact-prefix runner from batch/replay paths and make strict context/site/actor checks consistent.
- Add coherent boundary capture and explicit stop classifications; audit swallowed stop errors in the supported paths.
- Add action/workflow/site lifecycle identity and generic secondary context.
- Add pre-effect uncertainty gates for supported planning workflows.
- Define full state/continuation comparison and stronger replay/build identities.
- Revisit the raw deck/seed projection deferral and preserve advisor/bot compatibility with sanitized observations.
- Ensure canonical quiescence includes autonomous bots and does not hold locks across nested human waits.

**Exit:** a private supported workflow stops at the exact declared gate, publishes no live effects, and can be reconstructed at that boundary without silently accepting extra decisions.

### P1 — durable drafts and engine-owned planning scenarios

- Add owner-scoped draft documents, semantic nodes, revision CAS, evaluator results, and branch offers.
- Implement the detached strategy participant and initial tactical scenario adapters using shared rules.
- Add workspace-addressed protocol/client state and reconnect restoration.
- Implement conflict/result projection for movement, research, token allocation, and basic budgets.

**Exit:** an out-of-turn player can create/edit a persistent draft and obtain a correctly labeled conditional engine preview while live play continues.

### P2 — concurrent Technology followers, then primary composition

- Start with parallel secondaries after the ordinary primary completes.
- Add immutable ready submissions, reveal, review revisions, reconfirmation, and finalization.
- Add the announcement/primary workflow boundary to allow owner and followers to compose from announcement.
- Handle substitutions, optional second research, cancelled announcements, and unexpected follow-ups as explicit gates.
- Introduce group/segment records rather than pretending a multi-actor action is a single `BatchRecord`.

**Exit:** all relevant humans compose concurrently and confirm the revealed proposal version; actual decisions replay in rules order and pause cleanly for unplanned live input.

### P3 — tactical action planning and execution

- Add activation + ordered hull/cargo planning, precise stock/boost/route semantics, and repair UI.
- Rebase on supported live boundaries and distinguish waiting, stale, conditional, and invalid.
- Confirm at the actual action opportunity; reuse atomic baskets for certified segments.
- Preserve already executed nodes and pause/resume at activation/movement/card/chance gates.

**Exit:** a next-action draft survives state updates, refreshes, and a destroyed carrier; it neither silently drops cargo nor executes at a secondary/reaction opportunity.

### P4 — composition and conditional production

- Add strategy-parent dependencies, forecast reservations, and child invalidation.
- Add Leadership participant workflows, then supported Diplomacy/Trade/Construction stages.
- Add production/payment/placement intentions with real per-use credit and discounts.
- Add conditional postcombat/landing stages and replace committed parent assumptions with actual results.

**Exit:** a Technology/Leadership plan and a dependent next tactical action coexist; changed research/spending or loss of a production source produces a specific, repairable conflict.

### P5 — retroactive Leadership correction pilot

- Archive complete prior suffixes and introduce candidate revisions/lineage mappings.
- Edit historical token allocation workflows and semantically replay the suffix under the unchanged-entropy pilot policy.
- Stop for new obligations, actor/site ambiguity, changed consequences, or unsupported uncertainty.
- Add host publication, operation receipts, regenerate events/groups, and rebase current drafts afterward.

**Exit:** an agreed allocation correction preserves legal later intentions, surfaces affected fleet/action consequences, and replaces the timeline atomically with the original revision recoverable.

### P6 — reaction insertion and broader revisions

- Replace historical declines at exact legal card opportunities; reconstruct new nested responses.
- Add collaborative actor-scoped candidate repair and disappeared-obligation handling.
- Implement compatible occurrence alignment and the declared entropy/card-provenance policy.
- Extend to inserted action/free-transaction cases with actual turn-opportunity validation.

**Exit:** insertion cannot steal another decline, use a card not held then, bypass Sabotage timing, or leave both old and newly drawn copies in play.

### P7 — measured optimization and wider capabilities

- Measure replay/rebase/commit latency on late-game histories and four-player composite drafts.
- Add quiescent engine checkpoints if those measurements justify them.
- Expand supported timing continuations/card workflows and then arbitrary nested checkpoint support where useful.
- Add dependency invalidation indexes only after correctness with full supported re-evaluation is established.

**Exit:** optimizations produce the same complete continuation/records/outcomes as initialization replay and keep live play responsive under the measured hobby-game workload.

## 17. Verification and completion criteria

These are proposed implementation tests, not tests added or run for this documentation-only exploration.

### Engine and evaluator scenarios

| Scenario                                             | Required assertion                                                                                                       |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| Out-of-turn tactical forecast                        | Planning actor is correct; canonical active/window/state/entropy is unaffected; real legality still applies on adoption. |
| Synthetic turn during a real sabotage window         | No pending effect is discarded or assumed complete; forecast is deferred/conditional at the declared gate.               |
| Repeated identical prompts/workflows                 | No Leadership token, payment, hold, or production consumes an intent for a different occurrence.                         |
| Vector-index churn and damage/galvanize differences  | Equivalent unit remap succeeds; non-equivalent substitution is a conflict.                                               |
| En-route cargo and two carriers                      | Cargo source and carrier slot stay unambiguous; stock is not consumed twice.                                             |
| Shared boost contention                              | Two ships cannot both use the same per-activation boost in one forecast.                                                 |
| Carrier destroyed by new live state                  | Root unavailable-hull conflict blocks its cargo/landings; draft counts are retained until repaired.                      |
| Producer lost / surviving dock / blockade            | Distinguish total production loss from ship-only restrictions and continuing ground production.                          |
| Production credit and discount                       | Rebase/replay preserves combined-payment credit and discount scope, including mid-payment boundaries.                    |
| Choice ID still exists but cost/route changes        | Confirmation is invalidated even though fresh option membership succeeds.                                                |
| Planning crosses rift/combat/draw boundary           | No exact future roll/draw or downstream outcome-dependent legality is disclosed.                                         |
| Nested effect swallows a stop error                  | Evaluator still returns the captured terminal boundary and accepts no further answers; no resumable-state claim.         |
| Engine operation legally no-ops/produces fewer units | Intent outcome mismatch is visible, not reported as completed solely because no error occurred.                          |

### Server, groups, replay, and persistence

- Compare concurrent Technology/Leadership finalization with equivalent ordinary rules-order execution: fresh records, complete canonical state, automatic effects, next offer, and entropy evidence.
- Two participants revise/confirm concurrently; a stale confirmation cannot finalize a newer proposal set.
- Last confirmation races an edit; exactly one transition wins, and finalizing proposals are immutable.
- Eligibility changes after a primary/shared prerequisite are reflected in the required participant/review state.
- An unexpected reaction pauses finalization without answering it or losing remaining intentions; already committed segments remain accurately represented.
- Background rebase races a live human, internal bot, history edit, second batch, and credential takeover. No stale result adopts or leaks to the wrong recipient.
- Late-step mismatch, persistence failure, lost HTTP response, worker-start failure, and restart produce a complete old or new canonical transaction plus truthful receipts.
- A host corrects Leadership; unchanged prefix is exact, semantically valid suffix records are regenerated, new fleet removals pause the candidate, and old history is archived.
- Reaction insertion replaces the correct decline and opens actual new responses. An inserted `Action` card cannot preserve an incompatible same-turn tactical action.
- Chance-trace changes follow the chosen policy; compatible preserved outcomes and deck/card provenance remain coherent.
- Recovery handles canonical commit with stale group/draft metadata without double execution.
- New suffix event/action/batch ranges and old-to-new cursor mappings survive reconnect, undo, archived-revision restoration, and delayed request retries.
- Rules/content mismatch reports a replay/build incompatibility rather than a gameplay conflict.

### Browser and visibility scenarios

- Four isolated player views and a spectator: concurrent drafts stay owner-only until intended reveal; only permitted summaries become public.
- A player plans tactics while composing Technology; changing the parent updates the child budget/moves and explains the lost dependency.
- A live reaction interrupts planning focus while both drafts survive. All board inspection/selection/submission refers to the displayed workspace.
- Normal live nonce changes and batch worker replacements retain future drafts; actual history rewrites invalidate their certificates and trigger rebase.
- Refresh/reconnect restores own intentions and current ready/review state without showing hypothetical draws.
- Actor, opponent, spectator, and host revision projections contain no unknown deck order, RNG seed, future private holding, or hidden-cause conflict detail.
- Old asynchronous evaluations cannot overwrite the new workspace revision; stale/cancelled jobs never leave a green valid badge.
- Keyboard focus, minimized banners, narrow-screen workspace switching, repair controls, and specific confirmation labels remain usable.

### Relevant existing test surfaces

Extend the existing engine tactical/strategy/production/timing tests and server tests in `crates/ti4-server/tests/{history,session_deterministic_replay,session_recovery_replay,projection_redaction,crash_recovery,session_rejections}.rs`. Add dedicated branch/group/revision integration tests as the new APIs appear. Existing component/protocol tests and multi-context Playwright scenarios are the UI foundation.

Implementation checks should include focused engine/server suites, `cargo test -p ti4-server`, the relevant web tests/build, and multi-view browser scenarios. Broaden checks when shared replay/control-flow changes warrant it; planning-only documentation does not need runtime test execution.

### Overall done criteria

1. A strategy participant can compose immediately, see the other submitted decisions, and reconfirm the actual reviewed revision.
2. A future tactical draft persists, rebases against live changes, and reports illegal choices **and** meaningful changed assumptions/effects.
3. Concurrent strategy planning and next-action planning work together through explicit dependencies and correct shared budgets.
4. A historical correction replays later legal intentions, stops on real conflicts/new obligations, and publishes a coherent replacement with the original recoverable.
5. Insertion occurs only at genuine legal timing/action opportunities.
6. Speculation does not disclose future entropy or hidden/private behavior.
7. All canonical decisions, projections, history, receipts, and restart results agree on one authoritative lineage.

## 18. Code navigation and related plans

### Engine starting points

- [`crates/ti4-engine/src/choice.rs`](../../crates/ti4-engine/src/choice.rs): generated offers, actor observations, logs, nested offer hook.
- [`crates/ti4-engine/src/decision_context.rs`](../../crates/ti4-engine/src/decision_context.rs): typed source/subtype/target/outstanding constraints.
- [`crates/ti4-engine/src/game.rs`](../../crates/ti4-engine/src/game.rs): private continuations, `apply_choice`, `step_secondary`, `apply_tactical`, `sail`, `finish_action`, turn lifecycle.
- [`crates/ti4-engine/src/strategy.rs`](../../crates/ti4-engine/src/strategy.rs) and [`strategy_cards.rs`](../../crates/ti4-engine/src/strategy_cards.rs): participant ordering, eligibility/costs, inline primary/secondary effects.
- [`crates/ti4-engine/src/tactical.rs`](../../crates/ti4-engine/src/tactical.rs), [`transit.rs`](../../crates/ti4-engine/src/transit.rs), [`production.rs`](../../crates/ti4-engine/src/production.rs), [`payment.rs`](../../crates/ti4-engine/src/payment.rs): shared tactical legality and transaction state.
- [`crates/ti4-engine/src/timing.rs`](../../crates/ti4-engine/src/timing.rs), [`reactions.rs`](../../crates/ti4-engine/src/reactions.rs), [`rng.rs`](../../crates/ti4-engine/src/rng.rs), [`fingerprint.rs`](../../crates/ti4-engine/src/fingerprint.rs): timing continuations, information/entropy gates, and replay integrity.
- [`crates/ti4-model/src/state.rs`](../../crates/ti4-model/src/state.rs), [`units.rs`](../../crates/ti4-model/src/units.rs), [`view.rs`](../../crates/ti4-model/src/view.rs): state comparison, unit value semantics, and current visibility boundary.
- [`crates/ti4-policy/src/tactical_plan.rs`](../../crates/ti4-policy/src/tactical_plan.rs): semantic movement/cargo and joint-ledger precedent; separate reusable rules helpers from policy search/fallback behavior.

### Server and browser starting points

- [`crates/ti4-server/src/session/batch.rs`](../../crates/ti4-server/src/session/batch.rs), [`replay.rs`](../../crates/ti4-server/src/session/replay.rs), [`registry.rs`](../../crates/ti4-server/src/session/registry.rs): private evaluation and timeline adoption foundation.
- [`crates/ti4-server/src/session/worker.rs`](../../crates/ti4-server/src/session/worker.rs), [`decider.rs`](../../crates/ti4-server/src/session/decider.rs), [`mod.rs`](../../crates/ti4-server/src/session/mod.rs): single live offer, observed nested boundaries, quiescence/replacement.
- [`crates/ti4-server/src/storage.rs`](../../crates/ti4-server/src/storage.rs), [`projection.rs`](../../crates/ti4-server/src/projection.rs), [`protocol/server.rs`](../../crates/ti4-server/src/protocol/server.rs): durability, visibility, event/site/receipt metadata.
- [`web/src/protocol/client.ts`](../../web/src/protocol/client.ts), [`types.ts`](../../web/src/protocol/types.ts), [`decode.ts`](../../web/src/protocol/decode.ts), [`hooks/useGameSession.ts`](../../web/src/hooks/useGameSession.ts): branch-addressed ingress and independent revision ordering.
- [`web/src/components/GameShell.tsx`](../../web/src/components/GameShell.tsx), [`TacticalMovementOverlay.tsx`](../../web/src/components/TacticalMovementOverlay.tsx), [`TechnologyModal.tsx`](../../web/src/components/TechnologyModal.tsx), [`EventLog.tsx`](../../web/src/components/EventLog.tsx): workspace reuse, persistent draft state, review/conflict/history UI.
- [`crates/ti4-bot-agent/src/lib.rs`](../../crates/ti4-bot-agent/src/lib.rs): currently answers live pending messages; group/draft adaptation and sanitized observation compatibility.

### Related repository plans

- [`2026-09-26-GAME_ACTION_BATCHES_PLAN.md`](2026-09-26-GAME_ACTION_BATCHES_PLAN.md): atomic single-actor baskets, private replay, known limitations, and future workflow identities. Extend this foundation rather than introducing a competing batch executor.
- [`docs/code/WEB_GAME_HISTORY.md`](../../docs/code/WEB_GAME_HISTORY.md): initialization replay, RNG behavior, canonical replacement, undo/redo boundaries.
- [`2026-09-27-HIERARCHICAL_EVENT_LOG_PLAN.md`](2026-09-27-HIERARCHICAL_EVENT_LOG_PLAN.md): implemented log hierarchy and identity/navigation foundation.
- [`2026-09-24-DECISION_UI_CONSISTENCY_PLAN.md`](2026-09-24-DECISION_UI_CONSISTENCY_PLAN.md): shared decision frames and explicit staged-versus-committed behavior; workspaces extend that contract.
- [`2026-09-22-BYOA_STATELESS_ADVISOR_PLAN.md`](2026-09-22-BYOA_STATELESS_ADVISOR_PLAN.md): server/client/advisor separation and the prior facedown-deck projection deferral that must be reconciled with information-safe planning.
- [`plans/ACTIVATION_REWORK_PLAN_2026-09-17.md`](../../plans/ACTIVATION_REWORK_PLAN_2026-09-17.md): semantic tactical packages, joint reservations, and scoped execution precedent.

**Recommended next implementation task:** P0 with a tightly scoped Technology participant and activation/movement pilot. Prove coherent boundaries, safe stopping, semantic identities, and information projection before building a generic branch UI or optimizing full-engine cloning.
