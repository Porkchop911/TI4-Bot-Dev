# OBS-003g — agenda context producers

## Package

- Milestone: Stage 2 complete decision contract; typed-context foundation.
- Dependencies: `OBS-003b–c`.
- Objective: populate typed `DecisionContext` for the agenda-effect producers not already covered
  by `OBS-003e` slice 2's `vote.rs` work (outcomes/targets, votes, predictions, speaker, quash, and
  tiebreak decisions).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-003g`), LRR 8.18 (a tied
  election is the speaker's call), and Homeland Defense Act's/Colonial Redistribution's own printed
  text read from the embedded content store.
- Acceptance references: focused `obs003g` test in `agenda_effects.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/agenda_effects.rs`, this specification, package evidence,
  and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only.

## Behavior

Per the `OBS-002a` registry, `agenda_effects.rs` names three producers, all now typed:

- `choose_structure` (Homeland Defense Act's own PDS choice) — `Content("defense_act")`, subtype
  `defense_act_choose_pds`.
- `ask_the_speaker` (8.18's tied-election call, reached from Seed of an Empire and any other
  agenda whose tie is not otherwise broken) — `Rule("8.18")`, subtype `agenda_elect_tiebreak`.
- `resolve_with`'s own direct ask (Colonial Redistribution's settler choice) — `Content("redistribution")`,
  subtype `redistribution_choose_settler`.

Vote-casting, planet-exhaustion-to-vote, and the vote tiebreak are `vote.rs`'s own producers,
already typed in `OBS-003e` slice 2 (`cast_vote`, `vote_exhaust_planet`, `vote_tiebreak`) — a
different tiebreak from `agenda_elect_tiebreak` above: `vote.rs`'s is the speaker breaking a tie
*between outcomes* (8.19a); `ask_the_speaker`'s is the speaker naming *which tied player* an
agenda's own election names (8.18). Both are recorded so a later reader does not conflate them.

## Boundaries

- No option ID, label, legal set, or application-side behavior change. The existing suite in
  `agenda_effects.rs` passed unmodified.
- No features.rs change, matching every other `OBS-003` package: `OBS-008f` reads this into policy
  features later.

## Tests and commands

- One `obs003g_defense_act_and_redistribution_are_typed_distinctly` test covering all three
  subtypes this package adds, through real `resolve_with` calls with a `Capturing`-wrapped
  `Scripted` decider (`OBS-003e` slice 2's shared test helper), proving each is distinct and that
  the speaker-tiebreak's actor is genuinely the speaker (8.18).
- Full engine suite, policy suite plus the 102-game deterministic campaign, training suite, strict
  Clippy, `cargo fmt --check`, `git diff --check`.

## Definition of done

All three agenda-effect producers state a stable, machine-readable subtype and source; legal sets,
option IDs, and replay/serde behavior are unchanged; all checks and independent Tier-C review pass;
only package files are committed.
