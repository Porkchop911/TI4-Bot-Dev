# R01 — Interactive learned-game reviewer

## Authorization and relationship to the migration

R01 is an operator-authorized optional add-on. It does not alter the M00–M13 milestone order,
training semantics, or TTS behavior. On 2026-08-28 the operator explicitly waived the independent
review and atomic-package cadence requirements for **R01 only** and directed one complete
implementation pass. Formatting, tests, lints, deterministic behavior, bounded execution, honest
failure reporting, evidence, and scoped permissions remain required.

This plan supersedes the earlier artifact-first viewer plan. The earlier implementation rendered a
hand-authored `ReviewBundle` but could not start or control a real simulation. That is not the
requested product and must not be presented as one.

## Product contract

`cargo run -p ti4-review` opens a native Windows application. Its primary workflow is:

```text
Choose checkpoint bundle/profile JSON + choose map pool + seed + rotation
    -> load a real six-faction learned-policy game at its starting table
    -> step or run the live engine
    -> inspect omniscient board, holdings, and learned decisions
    -> autosave/reopen review history and optionally export replay-only HTML
```

There is no required command-line input, browser extension, local server, hand-authored game JSON,
or mock/example game. A headless CLI remains available for automated verification and export.

## Locked requirements from the operator interview

### Inputs and setup

- Native file-picker buttons select either a schema-6 MLP checkpoint bundle's `manifest.json` or
  `slots.json`, or a legacy JSON checkpoint/profile table, plus the JSON/JSON.GZ map pool. Selecting
  `slots.json` resolves and verifies the complete sibling bundle; it is never treated as weights.
- The seed is an unsigned 64-bit integer. Rotation is one of the six standard cyclic seat rotations.
- For legacy linear checkpoints, a selector chooses current `learner_profiles`/`profiles` or
  `accepted` champion profiles. Schema-6 MLP bundles have one immutable actor and ignore that
  legacy-only selector. Missing/malformed bundle components, checksum or shape mismatches,
  hashed/non-explicit legacy profiles, non-finite values, and incomplete six-faction tables fail
  before setup.
- The standard lineup is `sol`, `letnev`, `xxcha`, `hacan`, `jolnar`, and `l1z1x` under `FULL`
  content. The map uses the training path's `seed + 20,000,000` tile-seed rule.
- A new session opens at the post-deployment starting table before the first engine step.

### Live controls and exact boundaries

The live session is driven through `Game::step()`, the simulator's smallest stable transition.
Every attempted engine step produces a history frame, including automatic transitions and failures.

- **Step:** exactly one `Game::step()` call.
- **Next decision:** advance until one generated policy choice resolves.
- **Next action:** advance from the current point until one complete top-level action/pass resolves
  and the next top-level action choice (or a phase/end boundary) is reached.
- **Run N:** run a user-selected number of steps, decisions, or complete actions.
- **End round:** stop at the next round boundary.
- **End game:** run until natural completion or the declared safety limit.
- **Stop:** interrupt continuous execution at the next engine-step boundary.

Continuous commands run in bounded UI batches so Stop remains responsive. A hard per-command step
limit prevents a stalled game from appearing complete. Limit exhaustion, engine errors, user stops,
and unfinished sessions are visibly distinct from natural completion.

### Omniscient review surface

This version is explicitly a referee/debug view, not a public or seat-redacted artifact. It shows:

- the full hex board, systems, planets, ownership, exhaustion, command/frontier tokens, and units;
- all players' score, economy, command tokens, strategy cards, technologies, objectives, action
  cards, relics, leaders, notes, and other engine-visible holdings;
- round, phase, active seat/system, pending window, engine events, and terminal/error state;
- for every learned decision: acting seat/faction, prompt, requested/resolved head, temperature,
  every legal option and label, chosen option, raw score, probability, and projected feature
  values. Linear profiles also expose exact weights and value×weight contributions; nonlinear MLP
  inputs are labelled nonlinear rather than inventing a fixed per-feature weight.

The main layout is board center, player panels left, decision/details right, timeline and controls
below, and inputs/session actions above. Systems, planets, players, and history entries are
selectable for detail.

The board uses one fixed high-contrast color per physical seat, stable across faction rotations and
history. A thick colored hex edge means that seat exclusively controls the system's space area by
ships; planet ownership is independent and fills each planet circle with its controller's color.
Planets are placed inside their system and label name, resources/influence, traits, technology
specialties, and legendary status. Space and planet units are aggregated by owner/base class and
drawn with distinct geometric silhouettes; counts, sustained damage, and galvanize state remain
visible. Player sheets use the same color and grouped visual cards for economy, command pools,
strategy cards, controlled planets, board units, technologies, scored/secret objectives, action
cards, relics/fragments, leaders, plots, and breakthroughs. A legend explains all abbreviations.

### History, persistence, and export

- Previous/next/timeline navigation is view-only. It never rewinds or branches the live engine.
- A bounded autosave is updated during and after commands. `Save As` writes a portable omniscient
  review session; `Open Review` reopens it for view-only inspection without rerunning the game.
- A replay file contains normalized presentation snapshots and decision diagnostics, not a live
  resumable `Game`; reopening cannot continue simulation.
- `Export HTML` writes a self-contained, read-only replay with no server or external assets.
- Writes use an adjacent temporary file and replacement discipline. Checkpoint and map inputs are
  read-only and their hashes are recorded in the session.

## Architecture

```text
native egui launcher
  -> strict checkpoint/map loaders
  -> ti4-training interactive setup boundary
  -> ti4-engine::Game::step
  -> omniscient snapshot + policy-decision capture after every step
  -> native renderer / portable session JSON / replay-only HTML
```

`ti4-training` exposes only the already-existing deterministic seating/deployment construction as a
small `setup_game_with_deciders` boundary. It does not change rollout behavior. `ti4-review` owns
control cadence, capture, persistence, and presentation.

## Scope and permissions

Permission class: P2 (P1 source/plan work plus crates.io dependency resolution and bounded review
artifacts).

- Writable: `Cargo.toml`, `Cargo.lock`, `crates/ti4-review/**`, the minimal additive setup boundary
  and tests in `crates/ti4-training/src/rollout.rs`, `plans/R01_REVIEW_VIEWER.md`,
  `plans/EXECUTION_STATE.md`, and `plans/evidence/R01-IMPLEMENTATION.md`.
- Read-only external paths: none. The historical Python repository is not used.
- Network: crates.io metadata/downloads only for the pinned native GUI/file-dialog dependencies.
- Processes/ports: bounded Cargo build/test processes; no server and no port.
- Generated artifacts: ignored `out/reviews/`, maximum 512 MiB per session or HTML export. Test
  artifacts use task-specific temporary directories.
- Destructive actions: replacement/removal of exact adjacent temporary or backup save files only.
- External-state changes: none. No push, deployment, TTS mutation, or live service.

## Safety and bounds

- Checkpoint and map pool are validated completely before creating a game. An MLP bundle verifies
  its manifest, inventory, checksums, vocabulary, tensor shapes, runtime, heads, and faction roster.
  Inputs are never edited.
- All numeric checkpoint parameters used for inference must be finite. Every required faction must
  resolve to an explicit supported profile.
- A command may attempt at most 2,000,000 engine steps; configurable `Run N` is capped at 1,000,000.
- A session holds at most 1,000,001 frames and refuses serialization above 512 MiB. HTML export
  refuses output above 512 MiB. Bounds are reported, never silently truncated as success.
- Every frame records its exact engine-step index, round, phase, active seat, choice-resolution flag,
  action boundary, completion flag, error, state snapshot, map placement, new events, and optional
  decision detail.
- Same checkpoint bytes, map bytes, seed, rotation, table selection, and command sequence must
  produce identical semantic session content (excluding the save path).

## Acceptance

1. Launch with no arguments opens the native app and exposes both input-picker buttons.
2. A real schema-6 MLP bundle (selected through either `manifest.json` or `slots.json`) and map pool
   create the six-player starting table without executing the first choice; a legacy profile
   checkpoint remains supported.
3. Step, next-decision, next-action, Run N for all three units, end-round, end-game, and Stop obey
   their declared boundaries on the real `Game::step()` path.
4. A learned decision capture contains all legal options, chosen option, scores, probabilities, and
   non-vacuous feature/contribution detail where available.
5. Board and omniscient player projections agree with the underlying state at initial setup and
   after representative movement/economy/scoring changes. Seat colors, exclusive space-control
   edges, independently owned planet fills, planet metadata, geometric unit classes, and illustrated
   player-sheet groups remain consistent in the native view and HTML export.
6. Engine failure, command bound, and unfinished/user-stopped sessions remain visibly incomplete.
7. Save/reopen preserves every recorded frame and decision; reopened history is view-only.
8. HTML export is self-contained, replay-only, and includes board, players, timeline, and decision
   diagnostics without external requests.
9. Malformed/missing checkpoint tables, missing factions, invalid profiles, bad pools, bad saved
   sessions, oversized artifacts, and failed atomic writes fail clearly.
10. `cargo fmt --check`, focused `ti4-review` and affected `ti4-training` tests, Clippy with warnings
    denied for touched crates, and a real headless smoke simulation pass.

## Definition of done

The native application launches from `cargo run -p ti4-review`, starts a real learned-policy game
from button-selected inputs at the initial table, implements every locked control, exposes the
omniscient and policy details above, saves/reopens history, exports HTML, passes the acceptance
checks, and records honest evidence. No independent R01 review is required by operator waiver.
