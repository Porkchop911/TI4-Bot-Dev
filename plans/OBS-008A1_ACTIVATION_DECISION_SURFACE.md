# OBS-008a1 — activation decision surface

## Package

- Milestone: Stage 2 complete decision contract, first slice of `OBS-008a`
  (tactical continuation and options).
- Dependencies: `OBS-003d` (typed tactical context, `a5cfb29`/`6f058e8`), `OBS-006`
  (candidate-centred board state, `4880725`), `OBS-007a`/`007b` (preview contract and the
  deterministic foundation).
- Objective: deliver the typed *why* of a tactical decision (its stable subtype and option count)
  and the exact command-token consequence of activating a system to the acting policy, without
  changing any legal option set, option ID, label, or replay behaviour.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008a` row, static and
  counterfactual gates), `plans/OBS-003D_TACTICAL_COMBAT_CONTEXT.md`,
  `plans/OBS-007A_PREVIEW_CONTRACT.md`, `plans/OBS-007B_DETERMINISTIC_PREVIEW.md`, LRR 89.1.
- Acceptance references: focused `obs008a1` tests in `tactical.rs` and `ti4-policy/src/features.rs`,
  and the extended reserved-order block in `ti4-policy/src/vocabulary.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/tactical.rs`, `crates/ti4-policy/src/features.rs`,
  `crates/ti4-policy/src/vocabulary.rs`, `crates/ti4-policy/src/projection.rs`, this specification,
  package evidence, and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, ports, destructive actions, external-state changes: none.
- Generated artifacts: ordinary bounded Cargo output only; no committed bulk artifact.

## Inputs, outputs, and compatibility

- Input: a legal `activate_system` or `movement_step` question and its actor-bound state.
- Output: the same options, prompts, IDs, labels, and legal set, plus an exact `Preview` on each
  activation option and explicit policy facts under a new bounded `tactical` family.
- `ChoiceOption.preview` is runtime analysis, `#[serde(skip)]`. It does not touch option identity,
  payloads, legal sets, labels, replay scripts, or decision hashes (V1 or V2).
- No `DecisionContext` field is added or changed, so no context canonical form and no V2 decision
  hash moves. The slice only *reads* the context `OBS-003d` already attaches.
- New feature family `tactical` forces an append-only vocabulary registry bump
  `OOV_REGISTRY_VERSION` 6 → 7, `OOV_FAMILIES_V7` appending `tactical`, a new pinned fingerprint,
  and the one-version-back inference window moving to v6 (v5 now refused exactly as v4/v3/v2).

## Normative behavior

1. Every option of the `activate_system` choice carries `Preview::certain` with a single
   `TacticTokens` delta from the seat's current tactic pool to one less. This mirrors `activate()`,
   which spends exactly one tactic token unconditionally (LRR 89.1); the preview asserts only what
   the engine itself does.
2. The policy receives, under the `tactical` family and once per option (bare, uncrossed, in the
   same position `pay`/`production` decision facts already occupy):
   - `tactical:subtype:{subtype}` — `activate_system` or `movement_step`, read from
     `choice.context`, never from the prompt.
   - `tactical:option-count` — the number of legal options, clipped by `count_value`.
   - `tactical:optional` — `1.0` when `choice.context.optional`, absent otherwise.
   - `tactical:target-system` — `1.0` when `choice.context.target` names a system.
3. From an activation option's `Preview`:
   - `tactical:preview-known` plus `tactical:tactic-tokens-{before,after,change}` for a `Certain`
     outcome naming `TacticTokens`.
   - `tactical:preview-unknown` / `tactical:preview-unavailable` markers for the non-informative
     outcomes; no numeric consequence fact is ever emitted for them.
4. A missing preview emits nothing. A computed zero change is still distinguished from "no answer"
   by the `tactical:preview-known` marker.
5. `tactical` is classified `Transferable` in `projection.rs`: every fact is a bounded count, flag,
   or small-integer pool delta that means the same thing in any game.

## Non-goals

- Movement-step, load/cargo, landing, and ground-commitment consequence facts and previews; those
  are `OBS-008a2`/`a3`/`a4`.
- Any `DecisionContext` field change (e.g. setting `optional` on the movement context) — deferred
  so this slice cannot move a V2 decision hash.
- Combat and invasion option semantics (`OBS-008b`), prompt-free projection (`OBS-003i`),
  vocabulary publication (`OBS-011`), or any legal/mechanical tactical change.

## Tests and commands

- `tactical.rs`: an `obs008a1` test proving each activation option previews the exact
  `TacticTokens` fall by one, the preview agrees with actually calling `activate()` and
  re-measuring, and option IDs/labels/legal set are unchanged.
- `features.rs`: an `obs008a1` counterfactual test proving `tactical:subtype`,
  `tactical:option-count`, and exact `tactical:tactic-tokens-before/after/change` are present and
  move with the seat's tactic pool, that a missing preview fabricates no numeric fact, and that the
  facts survive the MLP projection (`admits("tactical:tactic-tokens-after")`).
- `vocabulary.rs`: extend `the_reserved_order_is_pinned_…` with the v6 → v7 append block and the
  pinned v7 fingerprint; move the one-version-back inference test to v6 and add a
  `version_five_remains_refused_for_inference` test.
- `cargo test -p ti4-engine --lib obs008a1`, `cargo test -p ti4-policy --lib obs008a1`,
  `cargo test -p ti4-policy --lib vocabulary`.
- Full engine suite (unit, integration, doc); full policy suite plus the 102-game deterministic
  campaign; training suite; strict all-target Clippy for engine and policy; targeted `cargo fmt`
  and `git diff --check`.

## Known traps

- Do not derive the tactic-token count from the prompt or a payload; read it from the seat and let
  `activate()` be the single source of the "-1".
- Do not encode a missing or non-informative preview as a numeric zero.
- Do not edit a frozen `OOV_FAMILIES_V*` list in place; append `OOV_FAMILIES_V7` and repin.
- Do not add `tactical` to `projection.rs` `FAMILY_ROLES` out of alphabetical order.

## Definition of done

Activation options expose their exact command-token consequence; the policy can tell a tactical
decision's kind and size from typed facts rather than prompt text; a missing preview stays missing;
old choices and V1/V2 replay hashes are unchanged; the registry bump is append-only and pinned;
focused and full checks pass; only scoped files are committed.
