# OBS-008b5 — bombardment target surface

## Package

- Milestone: Stage 2 complete decision contract, fifth slice of `OBS-008b`.
- Dependencies: `OBS-008b2` (`4b9c383`, the `combat` family), `OBS-005` (`4b69835`, opponent slots),
  `OBS-003d` (the `bombardment_target` typed context).
- Objective: give the bombardment-target ask (Coexistence 7.2) board context, and — the load-
  bearing fix — stop its option identity from reaching the policy as a raw, literal seat id, the
  one representation-doctrine violation this audit found: the option's whole id *is* an opposing
  seat's `PlayerId`, and nothing suppressed it from the generic `option:` token pipeline.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (representation rule 3,
  "Represent opponents in deterministic actor-relative slots, not player IDs"), `plans/
  OBS-005_RELATIONAL_PUBLIC_TABLE_STATE.md`, Coexistence 7/7.1/7.2.
- Acceptance references: focused `obs008b5` tests in `invasion.rs` and
  `ti4-policy/src/features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/invasion.rs`, `crates/ti4-policy/src/features.rs`, this
  specification, package evidence, and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, ports, destructive actions, external-state changes: none.
- Generated artifacts: ordinary bounded Cargo output only.

## Behavior

1. `bombardment_target_question` gains a `system` parameter and attaches `system`/`planet` payload
   to every option — board context this decision previously carried none of at all. Both call
   sites (`apply_bombard_plan`, `InvasionWindow::pending_choice`'s `ChoosingBombardment` stage)
   pass their own system.
2. **The identity fix.** `explicit_option_features_with`'s `dropped`-token computation, which
   already strips a board-identity argument out of a composite `verb|argument` option id, now has
   a second case: when the canonical kind is `bombardment_target`, the option's *entire* id is
   dropped (not just an argument slice), because the whole id is the targeted seat's raw
   `PlayerId` rather than a verb with a board reference buried in it. This removes the raw id from
   both the id- and label-derived token sets (`dropped` is applied uniformly to both).
3. A new `bombardment_target_features` reads `choice.context.subtype == "bombardment_target"`,
   parses the option id back into a `PlayerId`, finds its position in
   `seen.opponent_slots(player)` (OBS-005), and emits `combat:target-slot-{index}` — a bounded,
   relationship-ranked fact that means the same thing across games, replacing the raw identity
   the option id can no longer surface as a token.
4. No new feature family: `combat:target-slot-*` reuses the family `OBS-008b2` already
   established, and the `system`/`planet` payload reaches the policy through the pre-existing
   generic `option-system:*`/`payload:planet:*` pipelines. No vocabulary registry change.

## Invariants and boundaries

- No option ID, legal set, or application change; the option id is unchanged (still the raw
  `PlayerId` string) because the *engine* legitimately needs it to route the answer — only the
  *feature* pipeline is taught not to surface it.
- No `DecisionContext` field is added or changed.
- This fix is scoped to `bombardment_target`. A broader audit of every producer whose option id or
  label might carry a raw player identity (agenda "elect a player" outcomes, and others found by a
  grep across roughly twenty engine files during scoping) is out of this package's scope and is
  recorded as a residual for a dedicated pass, not silently fixed everywhere here.

## Tests and commands

- `invasion.rs`: an `obs008b5` test proving every bombardment-target option carries the correct
  `system`/`planet` payload, while the option id remains the targeted seat (needed for routing).
- `features.rs`: an `obs008b5` test proving, on a fixture where the targeted seat is the actor's
  own combat counterpart (slot 0), that `combat:target-slot-0` is present, that no `option:b`-style
  raw-identity token is ever emitted, and that the `system` payload still reaches the policy
  through `option-system:*`.
- `cargo test -p ti4-engine --lib obs008b5`, `cargo test -p ti4-policy --lib obs008b5`,
  `cargo test -p ti4-engine --lib invasion::`.
- Full engine suite (unit, integration, doc); full policy suite plus the 102-game deterministic
  campaign; training suite; strict all-target Clippy for engine and policy; targeted `cargo fmt`
  and `git diff --check`.

## Known traps

- Do not drop only an argument slice of the id for this kind; the whole id is the identity here,
  unlike `commit|{index}|{planet}`-shaped ids.
- Do not leave the label untouched: `dropped` filters the combined id+label token set, so fixing
  only `option.id`'s own tokenization would still leak the seat through `"{player}'s units"`.
- Do not generalize this fix to every kind whose id might be an identity; it is scoped to the one
  producer this audit found and confirmed.

## Definition of done

Every bombardment-target option carries board context and no longer leaks the targeted seat's raw
identity as a literal feature token; the policy reads the targeted seat as an OBS-005 opponent
slot instead; option identity, legal sets, and V1/V2 replay hashes are unchanged; no vocabulary
change; focused and full checks pass; only scoped files are committed.
