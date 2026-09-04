# OBS-003d — tactical/combat context producers

## Package

- Milestone: Stage 2 complete decision contract; typed-context foundation.
- Dependencies: `OBS-003b–c`.
- Objective: populate typed `DecisionContext` for tactical, invasion, combat, transit, and
  placement producers, without changing legal sets or option IDs.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-003d`), LRR 89.1/89.2
  (tactical action/movement), 78.3/78.4/78.9/78.7/82 (combat: reroll, casualty, retreat, sustain),
  49/42 (invasion, ground combat), 27.2 (custodians), 95 (cargo), and the card text for Heart of the
  Ixth and Dunlain Reaper read from the embedded content store.
- Acceptance references: focused `obs003d` tests in `tactical.rs`, `combat.rs`, `invasion.rs`,
  `transit.rs`, and `game.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/tactical.rs`, `crates/ti4-engine/src/combat.rs`,
  `crates/ti4-engine/src/invasion.rs`, `crates/ti4-engine/src/transit.rs`,
  `crates/ti4-engine/src/game.rs`, this specification, package evidence, and
  `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only.

## Behavior

Thirteen producers across four files now attach a typed `DecisionContext` to the `Choice` they
build, naming the rule or content source and a stable subtype:

| producer | subtype | source |
|---|---|---|
| `tactical::activation_options` | `activate_system` | Rule 89.1 |
| `game.rs::tactical_choice` (wrapping `tactical::movement_options`) | `movement_step` | Rule 89.2 |
| `transit::CargoWindow::pending_choice` | `load_cargo` | Rule 95 |
| `combat::choose_casualty` | `assign_casualty` | Rule 78.4 |
| `combat::choose_reroll_dice` | `reroll_die` | Rule 78.3 |
| `combat::offer_sustain` | `sustain_damage` | Rule 82 |
| `combat::heart_ixth` | `heart_ixth_die_adjust` | Content("heartofixth") |
| `combat::CombatWindow::pending_choice` (`Announcing`) | `announce_retreat` | Rule 78.9 |
| `combat::CombatWindow::pending_choice` (`Retreating`) | `retreat_to` | Rule 78.7 |
| `combat::CombatWindow::pending_choice` (`Sustaining`) | `sustain_damage` | Rule 82 |
| `combat::CombatWindow::pending_choice` (`Assigning`) | `assign_casualty` | Rule 78.4 |
| `invasion::commit_ground_forces` / `committing_choice` | `commit_ground_forces` | Rule 49 |
| `invasion::absorb_ground` | `assign_ground_casualty` | Rule 42 |
| `invasion::dunlain_reaper` | `deploy_mech` | Content("dunlain_reaper") |
| `invasion::bombardment_target_question` (both call sites) | `bombardment_target` | Rule "Coexistence 7.2" |
| `invasion::InvasionWindow::pending_choice` (`ChoosingNextCombat`) | `start_next_ground_combat` | Rule "Coexistence" |
| `invasion::InvasionWindow::pending_choice` (`Custodians`) | `remove_custodians` | Rule 27.2 |
| `invasion::InvasionWindow::pending_choice` (`Fighting`) | `fight_ground_combat_round` | Rule 42 |

## Boundaries

- No option ID, label, legal set, or application-side behavior changes anywhere. Every existing
  test in the four files and their callers passed unmodified.
- `transit::CargoWindow` gained two fields (`phase`, `round`) set once at `for_ship` construction,
  matching how the window already fixes `player`/`origin` at open time; the bare `new()` used only
  by this file's own tests defaults them, since no real caller reaches it.
- `game.rs::AftermathWindow::enter_production`'s own typed context (`produce_unit`, `place_unit`)
  was already delivered by `OBS-008c1`/`c2a`/`c2b` and is untouched here.
- War Machine's own `Choice` (played through the generic `reactions.rs` action-card window, not a
  producer this package owns) is untouched.
- No feature exposure. `OBS-003d`'s row is explicitly the typed-context foundation; reading these
  contexts into policy features is `OBS-008a`/`008b`'s job, the way `OBS-008c` read the payment
  contexts `OBS-003a/003e` (partially) laid down.

## Tests and commands

- One focused `obs003d_*` test per representative producer (tactical activation, driven movement,
  cargo loading, combat retreat/casualty/sustain, invasion commit/custodians) proving the typed
  subtype, source, and — where the decision has one — target.
- Full engine suite (unit, integration, doc), policy suite plus the 102-game deterministic
  campaign, training suite, strict Clippy, `cargo fmt --check`, `git diff --check`.

## Definition of done

Every producer named above states a stable, machine-readable subtype and source rather than relying
on prompt text; legal sets, option IDs, and replay/serde behavior are unchanged; all checks and
independent Tier-C review pass; only package files are committed.
