# Execution state

This file is the durable resume point for autonomous agents. Update it before every context
compaction, package commit, handoff, or milestone transition.

It describes **the repository as measured**, not the plan. A milestone is complete when its
behaviour is implemented, tested, and reviewed — never because a document for it exists.
The previous version of this file claimed the migration was complete; see
[`AUDIT_2026-08-11_PLAN_VS_TREE.md`](AUDIT_2026-08-11_PLAN_VS_TREE.md) for what was
actually in the tree and how the two diverged.

## Current position

- Oracle repository: `D:\Projects\ti4-engine` (read-only)
- Oracle branch: `codex/fully-learned-policy`
- Oracle commit: `37061c511a4780d4c0719e0342533a498cd4b457` — verified clean
- Branch: `main`
- Planning: **M00–M13 documents written.** Implementation status is separate and below.
- Implementation: **M02 and M04 in progress.** Content, galaxy, state model, hidden views,
  setup, phases and turn order done. Movement, combat, production and legality are not.
- Last completed package: M05-004 — driving the tactical action
  (`plans/evidence/M05-004_TACTICAL_DRIVER.md`)
- Previous packages: the choice model (`plans/evidence/M03-001_TO_005_CHOICE_MODEL.md`);
  faction seating (`plans/evidence/M04-004_FACTION_SEATING.md`);
  state model, views, phases and turn order
  (`plans/evidence/M02-003_005_008_M04-003_006_007_STATE_AND_PHASES.md`); galaxy
  (`plans/evidence/M04-001_002_GALAXY.md`); content layer
  (`plans/evidence/M02-009_TO_012_CONTENT_LAYER.md`)
- Next dependency-ready package: space combat (M06), the largest remaining gap — ships of two
  players can currently share a system indefinitely.

## M04-005 package checkpoint (historical)

- Branch: `wp/m04-005-strategy-draft`, based on unmerged M04-003 package commit `8f97ffb`.
- Last completed package: M04-005 — generated strategy-card draft and atomic application
  (`plans/evidence/M04-005.md`).
- Next dependency-ready package: M04-008 — generic strategy primary. M04-005 through M04-007
  now provide generated drafting, phase progression, and turn order.
- `ti4-engine` now has 105 tests. The workspace has 295 passing tests: 121 `ti4-content`,
  105 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- M04-005 is committed cleanly as `Generate and apply strategy draft choices` at the current
  package-branch `HEAD`.
- Strategy choices are generated from current unclaimed cards, validated at the shared choice
  boundary, and applied atomically; action choices remain unimplemented.

## M04-008 package checkpoint (historical)

- Branch: `wp/m04-008-generic-strategy-primary`, based on M04-005 package commit `73ed98c`.
- Last completed package: M04-008 — structural strategic-action generation and exact-card
  exhaustion (`plans/evidence/M04-008.md`).
- Next dependency-ready package: M04-009 — generic strategy secondary.
- `ti4-engine` has 109 tests. The workspace has 299 passing tests: 121 `ti4-content`,
  109 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- One unused strategy card produces the compatible bare `strategic` action; a player with several
  cards receives a stable named action for each unused card. Applying an action validates before
  mutation and exhausts exactly its selected card.
- Card-specific primary effects and secondaries remain intentionally unimplemented. Normal actions,
  turn advancement, and phase completion are also outside this package.
- M04-008 is ready to commit after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## M04-009 package checkpoint (historical)

- Branch: `wp/m04-009-generic-strategy-secondary`, based on M04-008 package commit `7c27b47`.
- Last completed package: M04-009 — generic strategic-action secondary window
  (`plans/evidence/M04-009.md`).
- Next dependency-ready package: M04-010 — status phase structural flow.
- `ti4-engine` has 112 tests. The workspace has 302 passing tests: 121 `ti4-content`,
  112 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- `begin_strategic_action` opens a clock-wise follower window. Eligible followers may follow for
  one strategy token or decline; tokenless followers are recorded ineligible. The selected card
  exhausts only when that window completes.
- Content-specific primary and secondary effects, other eligibility gates, event emission, and
  persistent game-step ownership of the window remain intentionally unimplemented.
- M04-009 is ready to commit after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## M04-010 package checkpoint (historical)

- Branch: `wp/m04-010-status-phase`, based on M04-009 package commit `3475d01`.
- Last completed package: M04-010 — deterministic status-phase bookkeeping
  (`plans/evidence/M04-010.md`).
- Next dependency-ready package: M04-011 — agenda structural phase.
- `ti4-engine` has 116 tests. The workspace has 306 passing tests: 121 `ti4-content`,
  116 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- The status resolver reveals objectives, draws action cards in preserved initiative order,
  returns board tokens, readies/repairs state, and resets strategy-card/pass bookkeeping. An
  empty objective deck ends the game before later steps.
- Status scoring and the per-token allocation choice are intentionally unimplemented; no default
  allocation or automatic scoring is applied. M04-012 must own those generated decision windows
  before integrating status resolution into the phase driver.
- M04-010 is ready to commit after scoped formatting, focused and affected-crate tests, workspace
tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
are recorded in the package evidence; independent review remains owner-waived.

## M04-011 package checkpoint (historical)

- Branch: `wp/m04-011-agenda-structural`, based on M04-010 package commit `85a122e`.
- Last completed package: M04-011 — structural agenda reveal/order/ready bookkeeping
  (`plans/evidence/M04-011.md`).
- Next dependency-ready package: M04-012 — choice-window and generated-decision API.
- `ti4-engine` has 119 tests. The workspace has 309 passing tests: 121 `ti4-content`,
  119 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- `resolve_agenda_phase` atomically rejects illegal entry, reveals at most two agenda aliases,
  records speaker-clockwise voting order, and readies planets after its two slots (including an
  empty deck). Every agenda resolution is explicitly deferred; no vote, tie-break, or agenda effect
  is invented.
- Agenda resolution is deliberately not integrated into the phase driver. M04-012 owns the legal
  generated decision windows and safe integration alongside outstanding status choices.
- M04-011 committed after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## M04-012 package checkpoint (historical)

- Branch: `wp/m04-012-step-run`, based on M04-011 package commit `b6bef5b`.
- Last completed package: M04-012 — generated-choice game driver with bounded run metadata
  (`plans/evidence/M04-012.md`).
- Next dependency-ready package: M04-013 — random-legal bot.
- `ti4-engine` has 124 tests. The workspace has 314 passing tests: 121 `ti4-content`,
  124 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- `Game` now owns generated strategy/action choices, table decision recording, structural follower
  windows, phase/round progression, observable events, and a bounded `run` API. Legal-choice
  inspection is side-effect free; each decision step is separate from phase work.
- Required but unavailable status scoring/token-allocation and agenda voting/tie/effect choices
  stop at a typed `GameError` boundary. They are not replaced by guessed defaults or reported as
  a completed game; tactical/component/Fleet Logistics behavior remains outside this driver.
- M04-012 committed after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## M04-013 package checkpoint (historical)

- Branch: `wp/m04-013-random-legal-bot`, based on M04-012 package commit `d316107`.
- Last completed package: M04-013 — shared-stream seeded random-legal game constructor
  (`plans/evidence/M04-013.md`).
- Next dependency-ready package: M04-014 — generic completion suite.
- `ti4-engine` has 128 tests. The workspace has 318 passing tests: 121 `ti4-content`,
  128 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- `Game::with_seeded_random` applies one ChaCha8-backed `SeededRandom` default to every unseated
  player, preserving global decision order and the generated-choice validation boundary. Same
  native seed repeats its event/decision trace; different seeds select different legal traces.
- A random run reaches the explicit `StatusChoicesUnimplemented` boundary rather than hanging or
  pretending the absent status scoring/token choices completed. Python seed parity is intentionally
  not claimed because the native stream is ChaCha8, not Mersenne Twister.
- M04-013 committed after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## M04-014 package checkpoint (historical)

- Branch: `wp/m04-014-completion-suite`, based on M04-013 package commit `d509d87`.
- Last completed package: M04-014 — 100-seed native generic structural campaign
  (`plans/evidence/M04-014.md`).
- Next dependency-ready package: M04-015 — differential phase suite.
- `ti4-engine` has 130 tests. The workspace has 320 passing tests: 121 `ti4-content`,
  130 `ti4-engine`, 68 `ti4-model`, and 1 doc-test.
- Every one of 100 seeded two-to-six-player runs reaches the explicit status choice boundary within
  500 steps; no run silently finishes, deadlocks, or records an invented choice. Same-seed state,
  event, decision-log, and step-result snapshots match after every step.
- M04 does not yet have generic game completion. Status scoring/token allocation and agenda
  voting/ties/effects are still required decision windows. The campaign records this as a bounded
  failure rather than presenting an incomplete run as success.
- M04-014 committed after scoped formatting, focused and affected-crate tests, workspace
  tests, normal engine Clippy, and whitespace validation passed. Existing workspace lint warnings
  are recorded in the package evidence; independent review remains owner-waived.

## Current package checkpoint (authoritative)

- Branch: `wp/m00-014-integrity-guard`, based on M00-012 package commit `849496d`.
- Last completed package: M00-012 fixed benchmark protocol (`plans/evidence/M00-012.md`).
  M00-011 remains blocked by an oracle integrity failure. Active package: M00-014e guard tool.
- M00-008 fixture-selection and M00-009 design documents existed without code. M00-009b through g
  now provide deterministic public-state, redacted-view, choice, resolved-event, outcome, and
  structured-error components.
- Eighteen focused tests cover canonical state ordering, state byte stability, viewer-private identity
  preservation, opponent redaction, view byte stability, choice option ordering, payload
  canonicalization/refusal, event UID/cancellation/context, finished-outcome tie-breaking, and
  deterministic structured errors. Oracle HEAD remains
  `37061c511a4780d4c0719e0342533a498cd4b457` and its tree is clean.
- The stale M00-007a draft schema named fields absent from the pinned oracle. M00-009b records the
  actual-field reconciliation; it cannot yet be advertised as an exact shared Rust/Python schema.
- M04-015 remains blocked: bounded generated traces exist, but no approved selected generated
  corpus or Rust/Python cross-engine comparison exists.
- M00-009h is split before implementation: M00-009h1 wires and validates a deterministic,
  read-only initial-setup NDJSON stream; M00-009h2 completed its reproducibility campaign. This
  preserves the original acceptance requirement without pretending the still-unimplemented full
  causal event trace exists.
- M00-009i observes a bounded seeded scenario's generated choices, resolved events, final state,
  and dice history. M00-009j replays its captured option IDs and proves byte-identical bounded-game
  streams, including across the executable replay CLI. M00-010 is now blocked before generation:
  M00-008 contains no executable 100-scenario manifest (including the distinct three-/four-player
  definitions), and no approved artifact-retention policy exists for traces that may contain hidden
  card identities. See `plans/evidence/M00-010f.md`. No oracle paths are writable.
- M00-011 **is resolved as of 2026-08-12; the oracle guard passes again.** Its `--basetemp`
  override had been passed unquoted to a Bash-family shell, which stripped the backslashes, leaving
  the drive-relative path `D:Projectsti4-engine-rs...` that resolved inside the oracle — not a
  pytest path-reinterpretation. The stray tree was moved (not deleted) into the package's own
  gitignored `.tmp-m00-011/basetemp-recovered/`, and the oracle verified clean, at the pinned
  commit, with a pristine tracked tree. Future oracle runs must pass the override with forward
  slashes (`--basetemp=D:/Projects/ti4-engine-rs/.tmp-m00-011`), which no shell here mangles.
  The run's captured log also shows the **full oracle suite passed: 2,097 of 2,097 in 491.65 s** —
  recorded but not yet accepted as the baseline, which is the package owner's call. See
  `plans/evidence/M00-011.md`.
- M00-012 replaces the stale alternative-filled benchmark drafts with a fixed 10-warmup/30-sample,
  deterministic interleaving, non-mutating affinity, raw-sample schema, and variance-rejection
  protocol. M00-013's dependency on M00-011 is now discharged.
- M00-014e is **complete.** With the oracle clean, `tools/generate_oracle_manifest.py` produced
  `plans/oracle_integrity_manifest.json` — 238 files (`engine/`, `bridge/`, `tests/`, `data/`,
  `configs/`, `pyproject.toml`) at the pinned commit — and the guard verifies it in production
  (`oracle integrity verified: 238 files`, exit 0). Fail-closed was proven against the real oracle,
  not only fixtures: a zeroed digest is rejected with exit 2. Automatic pipeline integration is
  still a separate, unclaimed package. See `plans/evidence/M00-014e.md`.

## M04-016 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `c44e8cf`.
- Last completed package: M05-004 — driving the tactical action
  (`plans/evidence/M05-004_TACTICAL_DRIVER.md`).
- `ti4-engine` has 142 tests. The workspace has **332 passing tests**: 121 `ti4-content`,
  142 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. The build is warning-free.
- `TokenGain` asks once per token, so a player may split a grant between pools — the oracle's
  own rule, shared with Leadership, which is why it lives in `tokens.rs` and not in the status
  phase.
- The status phase is split into `resolve_before_token_gain` (81.2–81.4) and
  `resolve_after_token_gain` (81.6–81.8) so the 81.5 window sits where the rules put it.
  `resolve_status_phase` still runs both for callers with no decider, and a test pins that the
  halves compose to the whole.
- The old `StatusChoicesUnimplemented` covered two unrelated gaps. It is now
  `StatusScoringUnimplemented` and names only LRR 81.1, which is the single remaining obstacle
  to a generic game completing a round.
- Two pre-existing status tests used strategy-card ids that do not exist in the corpus
  (`leadership` rather than `pok1leadership`), so they silently tested seating order rather than
  initiative order. Fixed; no production code was wrong.

## M04-017 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `3a78709`.
- Last completed package: M04-017 — objective scoring
  (`plans/evidence/M04-017_OBJECTIVE_SCORING.md`).
- `ti4-engine` has 157 tests. The workspace has **347 passing tests**: 121 `ti4-content`,
  157 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- **A generic game now completes a whole round.** All 100 seeded two-to-six-player runs finish
  the round with no step refusing, where before every one stopped at an unimplemented boundary.
- Scoring's machinery is fully ported (61.8 once-per-game, 61.16 home control, 98.4a point cap,
  98.7/98.8 initiative tie-breaks, both-deck point lookup). The *requirement predicates* are a
  first tranche of 6 of the oracle's 32 — the planet-control family. The other 26 are
  unregistered and therefore unscoreable, which is the oracle's own design for a coverage gap,
  and `unregistered_objectives()` reports them.
- 81.1 runs before 81.2 because scoring can end the game.
- Two defects found and fixed during the package: resolving controlled planets per predicate was
  quadratic enough to stop the campaign terminating, and completing the status phase turned a
  previously-safe unbounded test loop into a hang. Both are recorded in the evidence.

## M04-018 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `0e2265a`.
- Last completed package: M04-018 — agenda voting (`plans/evidence/M04-018_AGENDA_VOTING.md`).
- `ti4-engine` has 174 tests. The workspace has **364 passing tests**: 121 `ti4-content`,
  174 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- **`AgendaChoicesUnimplemented` is gone.** The round loop contains no structural boundary:
  strategy, action, status and agenda all resolve through generated choices.
- `VoteWindow` is a resumable state machine (outcome, then a planet per vote, then the speaker),
  because this driver resolves one decision per step where the oracle uses nested loops.
- Encoded with tests: the speaker votes last (8.2ii), a planet casts its full influence (8.6a),
  an abstention is not a vote (8.14), a tie *or a silent table* goes to the speaker (8.19) and
  that decision is not a vote (8.19a), a passed law stays in play (8.20/8.21).
- Agenda *effects* are not applied. Every resolution emits `AGENDA_EFFECT_UNRESOLVED`, which is
  what the oracle does when no handler is registered. Laws are recorded but nothing reads them.
- The agenda corpus has **no `electType` field** — it is null on every card. Elections are read
  off the printed `target`, as the oracle does. Reading the absent field would have made every
  agenda a silent For/Against with nothing failing.

## M05-003 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `2be9a43`.
- Last completed package: M05-004 — driving the tactical action
  (`plans/evidence/M05-004_TACTICAL_DRIVER.md`).
- `ti4-engine` has 193 tests. The workspace has **383 passing tests**: 121 `ti4-content`,
  193 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- `engine/movement.py` ported in full: 58.4a–f, 11.1, 86.1, 59.1/59.1a/59.2, 41.1/41.3.
  Reachability is a breadth-first search, not a distance comparison, because gravity rifts make
  the budget path-dependent.
- **`Galaxy` adjacency is finally load-bearing.** It had existed unused since M04-001.
- `Board::for_player` reads *ships*, not units: a lone infantry is not a blockade.
- The test fixture took three attempts and the reasons are recorded in the evidence — a hex ring
  is itself a route (so blocking the centre only bites at move 2), and "two apart" does not mean
  "opposite". Both earlier versions passed while testing almost nothing about blockades.
- Nothing calls this yet: there is no tactical action, so movement is knowledge the engine
  cannot act on. That is M05-006.

## M05-006 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `a2fedaa`.
- Last completed package: M05-004 — driving the tactical action
  (`plans/evidence/M05-004_TACTICAL_DRIVER.md`).
- `ti4-engine` has 210 tests. The workspace has **400 passing tests**: 121 `ti4-content`,
  210 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- `CargoWindow` fills a hold under LRR 95, tracking candidates **by index, never by value**:
  units are plain data, two infantry compare equal, and an equality filter would silently make
  the second one unloadable while every step reported success.
- Ground forces loaded from a planet arrive in the destination's *space area*. Landing is
  invasion, a separate step; dropping them onto a planet would conquer it with nobody choosing.
- 41.2 rolls one die per rift *exited* — ending in a rift is safe. Nav Suite is honoured here as
  well as in the legality rules, and rolls no die at all, since a discarded die would still
  advance the seeded stream and desynchronise replay.
- 95.1b: a ship lost to a rift takes its cargo down with it.
- `MoveOutcome` names its passengers rather than counting them; a count cannot be acted on.
- **Nothing calls this yet.** There is no tactical action, so the pieces exist but the sequence
  does not. That is M05-001/002.

## M05-001/002 package checkpoint (historical)

- Branch: `wp/m00-014-integrity-guard`, continuing from `9381fb5`.
- Last completed package: M05-001/002 — activation and the movement step
  (`plans/evidence/M05-001_002_TACTICAL_ACTION.md`).
- `ti4-engine` has 225 tests. The workspace has **415 passing tests**: 121 `ti4-content`,
  225 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- 89.1b bars a system holding *your own* command token, and only your own — an opponent's is no
  obstacle, because activating a system they hold is how you attack it. Both directions tested.
- `activate` checks both refusals before mutating; `identical()` pins that a refused activation
  spends nothing.
- `movable` asks `MovementRules` rather than re-deriving legality. That join is the package:
  parking a destroyer on the only route makes the move disappear from the offered options with
  no code in `tactical` knowing why.
- One option per distinguishable move, not per hull, and damage stays in both the dedup key and
  the label.
- The one-ring fixture trap from M05-003 recurred here in a different module: "two systems away"
  can be two seats round the ring, by a route that never touches the centre. Recorded twice
  deliberately — the wrong version passed the eye test both times.
- **Nothing sequences these yet.** A driven game still cannot take a tactical action.

## M05-004 package checkpoint (authoritative)

- Branch: `wp/m00-014-integrity-guard`, continuing from `f25435b`.
- Last completed package: M05-004 — driving the tactical action
  (`plans/evidence/M05-004_TACTICAL_DRIVER.md`).
- `ti4-engine` has 235 tests. The workspace has **425 passing tests**: 121 `ti4-content`,
  235 `ti4-engine`, 68 `ti4-model`, and 1 doc-test. Build and engine Clippy are clean.
- A second objective-predicate tranche landed alongside: technology and structures, taking
  coverage from 6 of the oracle's 32 to 14.
- **A driven game can now take a tactical action**: activate, move ships one at a time, load
  each hold, roll the route's rifts, finish.
- The action is offered only when `Game` has a galaxy. Nothing else builds one, so no existing
  test or the 100-seed campaign changed behaviour; the option is appended rather than inserted,
  so a first-option table keeps taking the action it took before.
- The route is computed when the ship is selected and carried through loading, so rifts are
  rolled for the path that was legal when the move was offered.
- `with_seeded_random` seeds the `GameRng` too, so a replayed game rolls the same rifts.
- The action *completes* and emits `TACTICAL_STEPS_UNRESOLVED` rather than blocking. Combat,
  invasion and production are unimplemented, so **arriving in an enemy system has no
  consequence** — announced, not hidden.

## Implementation status

Measured, not claimed. "Scaffold" means the file compiles and has a plausible shape but its
behaviour is a placeholder.

| Crate | Status | Detail |
|---|---|---|
| `ti4-content` | **Implemented** | 28-category corpus loader, source scoping, TE id fallback, manifest cross-check, canonical digests, referential validation, unit catalogue, galaxy and adjacency, faction records and starting-fleet parsing. 121 tests. |
| `ti4-model` | **Implemented** | `id.rs`, `content_types.rs`, `hex.rs`, `state.rs` (45-field `Player`, 52-field `GameState`), `units.rs`, `view.rs` (redaction + leak check). 68 tests. |
| `ti4-engine` | **Partial** | Setup (all decks, two revealed public objectives, one secret per player), the four-phase state machine, the strategy draft (snake order), turn order by initiative, faction seating onto a board, the choice model (options, deciders, validation, decision log, replay), and the seeded RNG with dice. 101 tests. Nothing *generates* options yet, so no turn can be taken. Movement, combat, production, legality, the status phase and the agenda phase are absent — not stubbed. |
| `ti4-policy` | **Stub** | 5 × `todo!()` |
| `ti4-sim` | **Stub** | 6 × `todo!()` |
| `ti4-training` | **Stub** | 6 × `todo!()` |
| `ti4-bridge` | **Stub** | 5 × `todo!()` |
| `ti4-legacy` | **Stub** | 4 × `todo!()` |
| `ti4-cli` | **Stub** | Prints hardcoded version strings |
| `xtask` | **Stub** | Prints a version string |

### Milestone implementation

| Milestone | Planning | Implementation |
|---|---|---|
| M00 Oracle and baseline | Written | **Partial** — corpus imported and checksummed; deterministic public-state, redacted-view, choice, resolved-event, finished-outcome, and error projections are executable. No complete oracle exporter, generated fixtures, or differential corpus. Correctness baseline was only collected, never run. Performance baseline disputed (see audit). |
| M01 Repository bootstrap | Written | **Partial** — workspace, toolchain, lints, profiles exist. No CI, no coverage or mutation harness, no benchmark harness, no `benches/`. |
| M02 Content and model | Written | **In progress** — 001, 003, 005, 007, 008, 009–012 done. 002, 004, 006, 013–016 outstanding. |
| M03 Choice, timing, replay | Written | **Partial** — 001–006 done (choice, validation, deciders, decision log, pinned RNG with domain separation, dice). 007–016 outstanding. |
| M04 Game skeleton | Written | **Partial** — 001, 002, 003, 004, 006, 007 done. 005 (draft resolution), 008–016 outstanding. Setup now builds deterministic decks and deals setup cards. |
| M05 … M13 | Written | **Not started** |

## Repository state

- Working tree: clean after the M04-003 package commit on `wp/m04-003-deck-construction`
- Python oracle tree: clean, unmodified ✅
- Tests: **291 passing** (`cargo test --workspace`) — 121 `ti4-content`, 101 `ti4-engine`,
  68 `ti4-model`, 1 doc-test
- Integration tests: none. All tests are inline `#[cfg(test)]` modules.
- Content corpus: `crates/ti4-content/content/`, 29 files, 1,800 records, byte-identical to
  the oracle and checksummed in `CHECKSUMS.sha256`

## Open blockers and findings

1. **The oracle exporter is incomplete.** M00-009b/c/d/e/f/g provide tested state, redacted-view,
   choice, resolved-event, finished-outcome, and structured-error projections, but no CLI/NDJSON
   stream, fixture manifest, or complete reproducibility campaign exists. Until those are complete, no
   differential parity claim can be made, and M03-014, M04-015, M05-021, M06-018 and all of M12
   remain unimplementable. This is the single largest gap.
2. ~~No independent review of any code package.~~ Waived by the project owner
   (2026-08-11). Recorded here so the standard and the practice do not silently disagree.
3. **No CI.** M01-006/007/008/009 are marked complete but nothing runs on a push.
4. **Throughput gate is ~8× weaker than the master plan intends** — `M00-013a.md` labels a
   sequential measurement as 12-worker throughput. Changing a contractual gate needs
   authority; flagged, not corrected.
5. **`ti4-engine` behaviour is not oracle-derived.** Legality, movement, combat, and
   scoring are placeholders. They must be replaced against named oracle sources rather than
   extended.
6. ~~**`Galaxy` is not wired into the engine.**~~ Closed by M05-003: adjacency is now the basis
   of movement legality.
7. **The status phase is implemented except for scoring; the agenda phase except for voting.**
   A driven round now performs status steps 81.2–81.8 including the real 81.5 token choice, and
   stops at `StatusScoringUnimplemented` (81.1). The agenda phase reveals and orders, then stops
   at `AgendaChoicesUnimplemented`. Neither invents a default.

## Next actions

In dependency order. Each is one package under `PI_WORK_PACKAGE_STANDARD.md`.

This list was stale — it still named option generation and the status phase, both of which
shipped in M04-005/008/009/010/012. Rewritten against the tree as measured on 2026-08-12.

1. **M04-017 — objective scoring (LRR 81.1).** The last thing standing between the engine and
   a completed round. Needs the `objectives.scoreable` predicate registry (~40 requirements),
   `award`, the secret-objective window from `_score_secret`, and the 98.7 victory check. The
   oracle's design is that an objective with no registered predicate is simply unscoreable, so
   this can land as tranches of predicates without pretending to cover more than it does.
2. **M04-018 — agenda voting.** The remaining `AgendaChoicesUnimplemented` boundary: votes,
   tie-breaks, and law/directive effects.
3. **M05-003/006 — ship movement.** The first real use of `ti4-content::galaxy`: legality
   from adjacency and move value, then atomic application.
4. **M01-006 — CI**, so that the 332 tests actually gate a change. Now more valuable than it
   was: the integrity guard gives CI something meaningful to run before any oracle work.
5. **M00-010 — the fixture manifest**, still blocked on an executable 100-scenario definition
   and an artifact-retention policy for traces that may contain hidden card identities.

## Decisions in force

- Windows-first isolated Rust rewrite.
- The Python repository at `37061c5` is a read-only behavioural oracle.
- Public/semantic compatibility with translation layers where documented.
- Content is compiled into the binary; `ContentStore::from_dir` remains for regenerated or
  reduced corpora, and a test proves the two agree.
- Corpus files are committed byte-identical with SHA-256 checksums and `.gitattributes`
  pinning them against end-of-line translation.
- Independent review is waived for implementation packages by the project owner
  (2026-08-11). Evidence files record what was verified and by what test, not a reviewer.
- Scoped permissions per `SCOPED_PERMISSIONS.md`: packages default to P0/P1.

## Handover

```
Objective:
M02 and M04. Continue the Rust rewrite with M04-005 strategy-card draft resolution.
Oracle commit:
37061c511a4780d4c0719e0342533a498cd4b457 (codex/fully-learned-policy) — verified clean
Active milestone/package:
M04-003 deck construction complete; M04-005 strategy-card draft resolution next.
Status:
All six setup decks now derive from deterministic, source-scoped native RNG domains; setup
reveals two stage-I objectives and deals one secret to each player. `ti4-engine` remains
partial: no option generator can yet drive the existing draft state machine.
Working-tree state:
Clean after the M04-003 package commit (`Build deterministic setup decks`) on
`wp/m04-003-deck-construction`; exact HEAD is recorded at handoff.
Tests last run and exact results:
`cargo test --workspace` -> 121 `ti4-content`, 101 `ti4-engine`, 68 `ti4-model`, 1 doc-test
passed; 0 failed. `cargo clippy -p ti4-engine --lib` passed with pre-existing dependency and
workspace warnings. Scoped `rustfmt --check` passed.
Compatibility evidence:
`plans/evidence/M04-003.md`: source membership, stage ordering, fake-relic exclusion,
setup dealing, and deterministic domain-separated streams are covered. Native order is not
Python-order parity because native ChaCha8 is an approved intentional divergence; no
differential fixture exists.
Decisions made and rationale:
- Content compiled in via include_str!, with from_dir retained and proven equivalent
- Record counts cross-checked against manifest.json at load
- Unknown source tags are load errors, not silent filter misses
- ContentType taxonomy replaced: the previous list invented 14 categories and omitted 14
- Hex geometry lives in ti4-model (pure); Galaxy lives in ti4-content (needs records)
- 12 planets with no tileId are placed during play, modelled rather than allowlisted
- 2,866 lines of placeholder engine deleted rather than adapted; they modelled a game
  with no legality checks, distance always 1, and every unit's combat value 1
- GameState equality reproduces the oracle's compare=False fields, board included, with
  GameState::identical() added for a full structural comparison
- Galaxy::build now rejects a duplicate system id; silently keeping the last placement
  left placement and coords disagreeing and shifted every later tile round the spiral
- neutral_systems returns corpus order rather than a seeded shuffle; seeded map selection
  belongs with the simulation harness
- SeededRandom uses ChaCha8, not Python's Mersenne Twister, so the same seed plays a
  different legal game; reproducing an oracle game needs its decision log replayed
- GameRng splits by domain (SHA-256 of seed || domain name), so adding a die roll cannot
  reshuffle a deck and a seed-pinned test fails only for the reason it was testing
- Setup defaults to seed zero for backwards API compatibility; `start_game_seeded` exposes
  the seed without ambient randomness.
Open review findings or blockers:
Independent review remains owner-waived. No oracle exporter. No CI. Whole-workspace format
and strict-Clippy gates are pre-existingly blocked by untouched stubs, package metadata,
and lint debt; details are recorded in M04-003 evidence.
Next exact action:
Create `wp/m04-005-strategy-draft` from the M04 integration branch and inspect
`D:\Projects\ti4-engine\engine\game.py` strategy-option functions plus their tests.
Files to read first:
`plans/EXECUTION_STATE.md`, `plans/evidence/M04-003.md`, `plans/M04_GAME_SKELETON.md`,
and `D:\Projects\ti4-engine\engine\game.py` strategy-option functions.
```
