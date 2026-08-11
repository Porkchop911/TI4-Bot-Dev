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
| M00 Oracle and baseline | Written | **Partial** — corpus imported and checksummed. No oracle exporter, no fixtures, no differential corpus. Correctness baseline was only collected, never run. Performance baseline disputed (see audit). |
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

1. **No oracle exporter exists.** `plans/M00-009_ORACLE_EXPORTER.md` was documented, never
   built. Until it is, no differential parity claim can be made, and M03-014, M04-015,
   M05-021, M06-018 and all of M12 are unimplementable. This is the single largest gap.
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
