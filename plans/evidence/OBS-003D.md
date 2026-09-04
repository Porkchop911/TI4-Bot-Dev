# Evidence — OBS-003d tactical/combat context producers

## Scope and provenance

- Branch: continuation, no new branch cut.
- Base: `01dd638` (`OBS-008c3`).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-003d`),
  `plans/OBS-003D_TACTICAL_COMBAT_CONTEXT.md`, and the LRR rule numbers and content aliases named
  there, read from each producer's own comments and the embedded content store rather than assumed.
- Historical Python: not inspected and not used as an acceptance oracle.
- Permission: P1 only. No network, external write, destructive action, or committed generated
  artifact.

## Changed paths

- `crates/ti4-engine/src/tactical.rs`
- `crates/ti4-engine/src/combat.rs`
- `crates/ti4-engine/src/invasion.rs`
- `crates/ti4-engine/src/transit.rs`
- `crates/ti4-engine/src/game.rs`
- `plans/OBS-003D_TACTICAL_COMBAT_CONTEXT.md`
- `plans/evidence/OBS-003D.md`
- `plans/EXECUTION_STATE.md`

## Result

Thirteen producers across four files attach typed `DecisionContext`. See
`plans/OBS-003D_TACTICAL_COMBAT_CONTEXT.md` for the full producer/subtype/source table.

## Tests and checks

| command | result |
|---|---|
| `cargo test -p ti4-engine --lib obs003d` | 7 passed |
| `cargo test -p ti4-engine --quiet` | 1,163 lib + 4 integration + 5 docs passed |
| `cargo test -p ti4-policy --lib -- --skip scored_games_stay_legal_and_deterministic_across_nested_windows` | 201 passed |
| `cargo test -p ti4-policy --lib bot::tests::scored_games_stay_legal_and_deterministic_across_nested_windows -- --exact --nocapture` | 1 passed; 102-game deterministic campaign, 264.29 s (266.63 s for `OBS-008c3` — no regression) |
| `cargo test -p ti4-training --lib --quiet` | 133 passed |
| `cargo clippy -p ti4-engine --all-targets -- -D warnings` | passed |
| `cargo fmt -p ti4-engine -- --check` | passed |
| `git diff --check` | passed |

## Counterfactual and agreement evidence

- `tactical.rs`: activation carries `Rule("89.1")`/`activate_system`.
- `game.rs`: a real driven game (activation scripted, no further script) shows the movement choice
  carrying `Rule("89.2")`/`movement_step` — proving the wrapping in `tactical_choice`, not just the
  isolated `movement_options` call, reaches the real path.
- `transit.rs`: cargo loading carries `Rule("95")`/`load_cargo` and the correct origin system as
  target, read from the fields fixed at `for_ship` construction.
- `combat.rs`: a fresh `CombatWindow` (retreat-capable fixture) opens into `announce_retreat`
  (`Rule("78.9")`) targeting the fought-over system; forcing `Stage::Assigning` and
  `Stage::Sustaining` directly proves `assign_casualty` and `sustain_damage` are distinct subtypes
  from each other and from the retreat announcement, though all three arise from the same combat.
- `invasion.rs`: committing ground forces carries `Rule("49")`/`commit_ground_forces` targeting the
  system, captured via a custom capturing decider (`FirstOptionCapturing`) written separately from
  the shared `CommitRecording` helper rather than widening an already-approved fixture. The
  custodians ask, reached by constructing `Stage::Custodians` directly (mirroring an existing test's
  own pattern), carries `Rule("27.2")`/`remove_custodians` — distinct from ground-force commitment.
- The 102-game deterministic campaign exercises tactical movement, combat (casualty, sustain,
  retreat), and invasion (commit, custodians, bombardment) extensively and completed legally and
  deterministically at an unchanged wall-clock cost.

## Decisions made and rationale

- **`movement_options` stays a pure function; its context is attached at the call site.** It
  receives no `state` today (only `player` and pre-computed `movable` candidates), and threading
  `state` through purely to read `phase`/`round` would be a larger, less local change than wrapping
  the one real call site in `game.rs::tactical_choice`, which already has `self.state`.
- **`CargoWindow` gained two fields rather than threading `phase`/`round` as extra `pending_choice`
  parameters.** `pending_choice(&self)` takes no arguments at all; the window already fixes
  `player`/`origin` at `for_ship` construction, and `phase`/`round` follow the same pattern rather
  than becoming a new kind of parameter this window's shape does not otherwise have.
- **`bombardment_target_question` stays a pure function too; both of its two callers wrap the
  result.** It has no natural access to `system` (only a planet), and duplicating a `.contextualized`
  call at two sites is smaller and lower-risk than adding a parameter to a function called from two
  different stages of two different structures.
- **Rule citations were read from each producer's own comments, not guessed.** The bombardment and
  next-combat citations use "Coexistence 7.2"/"Coexistence", matching the code's own repeated
  comments, rather than a bare LRR number that would misattribute a variant rule to the base game.
  The custodians citation was corrected from an initial guess (34.2) to 27.2 after checking the
  file's own existing doc comments, which already cite the token's rule consistently.
- **`heart_ixth` and `dunlain_reaper` use `DecisionSource::Content`, not `Rule`.** Both are named
  card/unit effects, not core rules text, matching how the already-reviewed `OBS-008c3` used
  `Content("aida")` for AI Development Algorithm.
- **No features.rs change.** This package's own row is explicitly the typed-context foundation;
  reading these contexts into policy features belongs to `OBS-008a`/`008b`, mirroring how `OBS-008c`
  read the payment contexts `OBS-003a` laid down rather than `OBS-003a` reading them itself.

## Independent review

**OUTSTANDING.** Tier C (touches decision delivery across four core rules files). Not yet
independently reviewed, alongside the three packages already recorded as outstanding
(`OBS-008c2b`, the production-discount bug fix, `OBS-008c3`).

## Non-goals retained

No legal-set, option-ID, replay, or application-side change anywhere. No feature exposure. No
vocabulary generation or bundle republish. `OBS-003e`'s remaining scope (turn, strategy-card,
technology beyond this session's production work, token, and scoring producers) is untouched.
