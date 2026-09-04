# Evidence — OBS-003g agenda context producers

## Scope and provenance

- Branch: continuation, no new branch cut.
- Base: `cf012e8` (`OBS-003f`).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-003g`),
  `plans/OBS-003G_AGENDA_CONTEXT.md`, LRR 8.18, and each agenda's own printed text.
- Historical Python: not inspected and not used as an acceptance oracle.
- Permission: P1 only. No network, external write, destructive action, or committed generated
  artifact.

## Changed paths

- `crates/ti4-engine/src/agenda_effects.rs`
- `plans/OBS-003G_AGENDA_CONTEXT.md`
- `plans/evidence/OBS-003G.md`
- `plans/EXECUTION_STATE.md`

## Result

`agenda_effects.rs`'s three registered producers (`choose_structure`, `ask_the_speaker`,
`resolve_with`'s own direct ask) attach typed `DecisionContext`.

## Tests and checks

| command | result |
|---|---|
| `cargo test -p ti4-engine --lib obs003g` | 1 passed |
| `cargo test -p ti4-engine --quiet` | 1,172 lib + 4 integration + 5 docs passed |
| `cargo test -p ti4-policy --lib -- --skip scored_games_stay_legal_and_deterministic_across_nested_windows` | 201 passed |
| `cargo test -p ti4-policy --lib bot::tests::scored_games_stay_legal_and_deterministic_across_nested_windows -- --exact --nocapture` | 1 passed; 102-game deterministic campaign, 267.34 s (266.91 s for `OBS-003f` — no regression) |
| `cargo test -p ti4-training --lib --quiet` | 133 passed |
| `cargo clippy -p ti4-engine --all-targets -- -D warnings` | passed |
| `cargo fmt -p ti4-engine -- --check` | passed |
| `git diff --check` | passed |

## Counterfactual and agreement evidence

- One test drives three real agendas (`defense_act`, `redistribution`, `seed_empire`) through
  `resolve_with` with a `Capturing`-wrapped `Scripted` decider and asserts all three subtypes are
  distinct.
- The `seed_empire` case additionally asserts the tiebreak's `actor` is the speaker, not either
  tied player — the fact 8.18 exists to state (the speaker breaks the tie, not a coin flip or the
  first-seated tied player).

## Decisions made and rationale

- **Two tiebreaks, kept distinct rather than merged.** `vote.rs`'s `vote_tiebreak` (already typed
  in `OBS-003e` slice 2, 8.19a) breaks a tie between agenda *outcomes*; `agenda_effects.rs`'s
  `agenda_elect_tiebreak` (8.18) is the speaker naming which *tied player* an agenda's own election
  names. Both are "the speaker breaks a tie" in casual description but are different rules
  answering different questions, and merging their subtypes would make one indistinguishable from
  the other to a reader of the typed context alone.
- **`choose_structure`'s only caller is Homeland Defense Act**, confirmed by reading rather than
  assumed, so `Content("defense_act")` is exact rather than a shared-primitive placeholder.

## Independent review

**OUTSTANDING.** Tier C. Not yet independently reviewed, alongside `OBS-008c2b`, the
production-discount bug fix, `OBS-008c3`, `OBS-003d`, `OBS-003e` slices 1–2, and `OBS-003f` — eight
packages now owed review.

## Non-goals retained

No features.rs change. No vocabulary generation or bundle republish. No agenda legality or
application change.
