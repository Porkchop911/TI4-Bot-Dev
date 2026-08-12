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
- Last completed package: M04-003 — deterministic deck construction and setup dealing
  (`plans/evidence/M04-003.md`)
- Previous packages: the choice model (`plans/evidence/M03-001_TO_005_CHOICE_MODEL.md`);
  faction seating (`plans/evidence/M04-004_FACTION_SEATING.md`);
  state model, views, phases and turn order
  (`plans/evidence/M02-003_005_008_M04-003_006_007_STATE_AND_PHASES.md`); galaxy
  (`plans/evidence/M04-001_002_GALAXY.md`); content layer
  (`plans/evidence/M02-009_TO_012_CONTENT_LAYER.md`)
- Next dependency-ready package: M04-005 — strategy-card draft resolution. Decks and
  setup dealing now complete the prerequisites for generated strategy choices.

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

- Branch: `wp/m00-009i-causal-export-gap`, based on M00-009h2 package commit `e4dfb2b`.
- Last completed package: M00-009i — bounded-game observation layer
  (`plans/evidence/M00-009i.md`), pending its focused package commit.
- M00-008 fixture-selection and M00-009 design documents existed without code. M00-009b through g
  now provide deterministic public-state, redacted-view, choice, resolved-event, outcome, and
  structured-error components.
- Twelve focused tests cover canonical state ordering, state byte stability, viewer-private identity
  preservation, opponent redaction, view byte stability, choice option ordering, payload
  canonicalization/refusal, event UID/cancellation/context, finished-outcome tie-breaking, and
  deterministic structured errors. Oracle HEAD remains
  `37061c511a4780d4c0719e0342533a498cd4b457` and its tree is clean.
- The stale M00-007a draft schema named fields absent from the pinned oracle. M00-009b records the
  actual-field reconciliation; it cannot yet be advertised as an exact shared Rust/Python schema.
- M04-015 remains blocked: the new setup-only CLI/NDJSON exporter has no selected generated corpus,
  complete causal trace, or cross-engine comparison.
- M00-009h is split before implementation: M00-009h1 wires and validates a deterministic,
  read-only initial-setup NDJSON stream; M00-009h2 completed its reproducibility campaign. This
  preserves the original acceptance requirement without pretending the still-unimplemented full
  causal event trace exists.
- M00-009i observes a bounded seeded scenario's generated choices, resolved events, final state,
  and dice history. Next ready package: script replay over that stream. No oracle paths are
  writable. The earlier campaign's five reproducible scratch outputs remain untracked only under
  ignored `.tmp-m00-009h2`.

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
6. **`Galaxy` is not wired into the engine.** Adjacency exists and is unused until movement
   is written.
7. **The status and agenda phases are boundaries, not implementations.** A caller driving a
   full round reaches `PhaseOutcome::StatusBegan` and finds nothing happens.

## Next actions

In dependency order. Each is one package under `PI_WORK_PACKAGE_STANDARD.md`.

1. **M04-005/012 — option generation.** The choice model exists but nothing fills it. Port
   the oracle's `Game._strategy_options` and `_action_options` so a seated game can take a
   turn.
2. **M05-003/006 — ship movement.** The first real use of `ti4-content::galaxy`: legality
   from adjacency and move value, then atomic application.
3. **M04-010 — the status phase.** `advance_phase` currently reaches `StatusBegan` and
   stops.
4. **M00-009 — build the oracle exporter.** Unblocks every differential deliverable.
5. **M01-006 — CI**, so that the 291 tests actually gate a change.

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
