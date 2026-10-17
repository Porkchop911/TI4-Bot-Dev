# BF manual-choice acceptance

## Scope

This is acceptance evidence for the bounded BF24 manual-choice presentation check. The intended
contract is that a parked offer carries the producer's prompt, option labels and IDs, and serialized
decision context intact; submitting one offered ID resumes the actual engine callback and changes
the matching target. The source producers currently in scope include the Ssruu copied round-agent
unit choice and copied L1Z1X planet choice. Ground combat sustain/casualty choices are optional for
this pass when a real driver fixture is available.

## Current test

`crates/ti4-replayer/tests/live.rs` adds two fixtures:

- `a_manual_ssruu_round_copy_parks_with_target_labels_and_resumes_on_the_chosen_unit` seats a
  Yssaril borrower, an exhausted Letnev source agent, and two target cruisers, arms the engine's
  real timing resolver, then emits `COMBAT_ROUND_STARTED` through `TimingContext` on a worker thread.
  A manual `ControlledDecider` parks the actual resolver ability choice and copied-unit target
  choice in the public `Gate`. The test submits the ability and second cruiser IDs from those offers,
  checks the pending choice's context subtype and readable unit labels, and verifies only the second
  cruiser gets the round bonus after the callback resumes. It also checks that Ssruu exhausts and the
  already exhausted source stays exhausted.
- `a_manual_ssruu_l1z1x_copy_preserves_planet_choice_and_replaces_that_planets_infantry` seats
  Yssaril, L1Z1X and Sol, prepares the active system with Sol infantry on two content planets, and
  emits `SYSTEM_ACTIVATED` through the armed resolver. It checks that the parked planet offer retains
  its context subtype, human-readable replacement labels and planet IDs, then submits the second
  planet's ID and verifies that only that planet's infantry becomes a Sol mech. The test also checks
  Ssruu and source-agent exhaustion semantics.

The test exercises a genuine resolver callback, `Table::ask`/`ask_seeing`, Gate parking, conversion
to `PendingManualChoice`, answer submission, and resumed effect. It does not claim a full
`LiveBranch`/`Game::step` tactical-round fixture: `LiveBranch` starts from `SimulationConfig`'s
checkpoint and map pool and exposes no caller-built `GameState` injection. The integration test keeps
the existing public boundary and uses engine fixtures plus the same public Gate/ControlledDecider
path rather than synthesizing `PendingManualChoice`.

## Validation status

This test was appended before this evidence file was created, so the intended spec-first order was
not followed; this note records that chronology. No Cargo command was run by this contributor, per
the task instruction that root owns the workspace build. Compilation and execution remain pending
root's coordinated test run. Ground-combat sustain/casualty producer paths are not covered: this
addition does not drive a full invasion combat window, so it does not claim acceptance for those
prompts.


Permission P1: writable only appended live.rs callback fixtures and this evidence; no public API/dependency/network/external process/folders/deletion. Independent root review confirms actual timing callback, manual Table parking, offer submission and resumed selected-unit/planet effects, with RAII Gate shutdown on assertion failure. Focused `cargo test -p ti4-replayer --test live a_manual_ssruu_ -q -j4 -- --test-threads=4` passes2,9filtered,0.05s, Cargoexit0 (target/bf-oct5-manual-callback.log). Absolute pinned libtorch and its DLL directory used. Root removes the one new unusedimport; full live test/lint pending. Dependencies (copied round module/shared activation) still uncommitted, so manual fixtures are not committed ahead of them. Existing lineupNone hunk belongs to other shared work and is excluded from this package. No wholeBF24 closure claimed.


After removing the new unusedimport, full affected `cargo test -p ti4-replayer --test live -q -j4 -- --test-threads=16`:11passed,0failed,2.60s,exit0 (target/bf-oct5-live-all.log). Both new actualcallback tests pass alongside existing live tests. Still no fullBF24/manualground/isolatedcommit claim.

## Ground-casualty callback follow-up — 2026-10-11

Luna added `a_manual_ground_combat_casualty_submits_the_selected_damaged_mech` in live.rs. Root reviewed the real producer and fixed the initial harness boundaries: Dice::from_faces is engine-unit-test-only, and public invasion::resolve assigns ground casualties automatically. The retained test instead uses public invasion::ground_combat, which actually builds assign_ground_casualty through Table::ask_seeing. It does not claim a retained Game/InvasionWindow manual path.

Seed 43 is pinned with the public dice implementation: attacker infantry rolls 8, defender damaged Hacan mech 1, defender infantry 10. Both sides hit; the real Gate parks the defender's two-option casualty choice. The test checks subtype, system/planet target, readable distinct labels, ground_casualty kind and unit/damage payloads. It submits the mech ID and verifies the damaged mech and attacking infantry are gone while the defender infantry survives. Its underlying fallback would choose the infantry, so the final state distinguishes the manual submission from a fallback. The Gate switches to Auto after submission; waiting is bounded and checks worker exit, with the existing shutdown guard releasing a parked worker on assertion failure.

Focused final test: **1 passed, 0 failed**, exit 0 (`out/logs/check-bf24-manual-ground-casualty-final-20261011.log`). After source cleanup, full live integration: **12 passed, 0 failed**, exit 0 (`out/logs/check-bf24-live-all-20261011.log`). Initial compile/fixture diagnosis failures remain in separate logs. Coordinator frontier review found and resolved moved IDs, an invalid post-submit mode-effect assumption, same-answer fallback and incorrect synchronous producer choice; no new production API or engine rule. Only the appended test is owned here; existing replayer/gui work is preserved.

BF24 remains open for borrowed_agent_planet, synergy_survivor/ground-effect subtype coverage and actual retained Game/manual ground paths, especially Naaz Maximum. This follow-up proves one genuine ground-casualty producer-to-Gate callback, not complete UI acceptance or browser rendering.
## Copied exploration and Absolute Synergy follow-up — 2026-10-11

Luna authored two more fixtures in live.rs; root independently reviewed the actual producers and final source. No production API or engine behavior changed.

- `a_manual_ssruu_copied_garv_and_gunn_explores_the_selected_planet`: real armed TURN_PASSED resolver, Yssaril borrower, exhausted Naaz source and ending Sol player. Two named ordinary planets share exactly one exploration trait; the ending player controls only those planets, and the deck holds two actual benign fragment cards. The real borrowed_agent_planet offer preserves planet IDs, kind and exact readable labels. Submitting the second planet explores it once, leaves the first unexplored, exhausts Ssruu and leaves the source exhausted.
- `a_manual_naaz_absolute_synergy_keeps_the_selected_damaged_form`: real armed BREAKTHROUGH_GAINED resolver with Naaz's breakthrough and four mechs in one system, split between space and a planet. The real synergy_survivor and synergy_survivor_state offers preserve exact IDs, kinds and labels, including deduplication of identical undamaged space forms. Submitting space and the damaged plain-form survivor produces exactly one damaged Maximum, with no remaining Naaz mechs in space or on planets. Agent status stays readied. This observes resumed final behavior; it does not independently observe the worker-owned game state while parked or prove retained Game reachability.

Both fixtures use bounded waits, early worker-finish detection and the existing shutdown guard. Root resolved fixture compilation errors (planet reference level; borrowing a unit from a temporary system view) and the expected deduplicated survivor count. Initial failure logs remain; no flaky retry claim.

Focused Garv/Gunn: **1 passed**, exit 0, out/logs/check-bf24-manual-garv-final-20261011.log. Focused synergy: **1 passed**, exit 0, out/logs/check-bf24-manual-synergy-20261011.log. After targeted formatting of only the new test span, full live integration: **14 passed, 0 failed**, exit 0, out/logs/check-bf24-live-all3-20261011.log (Cargo -j8, test threads 8; pinned CPU libTorch).

BF24 remains open for ground_effect_sustain, actual retained Game/manual ground paths and browser presentation. The latest full workspace result (4068 passed/1 failed/6 ignored) predates these two newest fixtures; its only failure remains the frozen reviewer golden.

## Ground-effect sustain follow-up — 2026-10-11

Luna traced the sole ground_effect_sustain producer to Saar's retained SAAR_DEORBIT_BARRAGE callback, then authored `a_manual_saar_deorbit_offers_naaz_maximum_ground_effect_sustain`. Root independently reviewed the registered ability, hit assignment and Maximum ground-force eligibility. The fixture emits the real effect event through fixtures::armed_resolver, with Saar and Naaz manually controlled and one undamaged Naaz Maximum on the target planet. Public RNG seed43 gives the one Deorbit die a hit. Saar's actual deorbit_hit offer selects that unit; Naaz then receives the actual Rule87 ground_effect_sustain offer with the serialized system/planet target. Submitting sustain retains the damaged Maximum; the decision log records the sustain, the resolver records GROUND_FORCE_SUSTAINED, and no GROUND_FORCE_DESTROYED event occurs. AlwaysDecline fallback would destroy it, so this distinguishes the manual answer.

Root corrected compilation boundaries: Table itself cannot cross threads, so only its typed decision log is returned; DecisionContext and Event fields use their typed APIs. Initial diagnostic failure logs remain. No production rule/API change. Targeted formatting only the appended fixture. Focused final: **1 passed**, exit0, out/logs/check-bf24-manual-deorbit-verified-20261011.log. Full live integration: **15 passed, 0 failed**, exit0, out/logs/check-bf24-live-all4-20261011.log, Cargo-j8/test-threads8, pinned CPU libTorch.

This covers the retained Saar callback and Naaz Maximum sustain-to-Gate path, bypassing the larger component-action offer, payment, target discovery and Game staged-event flush. Ordinary Game/InvasionWindow casualty assignment is still automatic and does not use this effect producer. Browser rendering and a full driven-game manual fixture remain open; no whole BF24 closure.

## Driven Game upgrade of the same sustain fixture

The later upgrade supersedes the direct-event reachability limit above, without duplicating the test. It starts a real Game in Action phase with Saar active and drives the actual generated Deorbit action, one-resource spend, legal planet target and trade-good payment through Gate submissions. A Saar cruiser in an asteroid-field fixture galaxy makes the target discoverable; Maximum has baseType mech and qualifies directly. The Game driver flushes the staged effect before presenting deorbit_hit and ground_effect_sustain. All six answers use actual parked offers; no injected effect event remains. The final state asserts the one trade good was paid, the breakthrough exhaustion mark exists, and the damaged Maximum survived, with the typed sustained event and recorded manual answer.

Public seed43 is supplied through GameState.rng_seed; Game::with_table uses it. The worker stays bounded to eight Game::step calls and rejects any StepResult error. Saar switches to Auto after its last effect selection and Naaz after sustain, permitting legitimate turn-close follow-ups to finish. It returns only state/log/events/results; Game/Table stay on the worker. Root reviewed this real producer route, fixed the initial fixture map mismatch (hub_from builds PoK tiles; selecting a DEFAULT-only TE asteroid was invalid), and kept DEFAULT unit/rule scope while selecting a PoK-compatible asteroid tile.

Focused upgraded test: **1 passed**, exit0, out/logs/check-bf24-game-deorbit-map-fixed-20261011.log. After formatting only this owned test span, full live integration: **15 passed, 0 failed**, exit0, out/logs/check-bf24-game-live-all-20261011.log. The earlier callback-only tests/results remain historical evidence, not the final implementation. Starting in Action phase excludes Strategy-phase setup, but component-action legality/payment/target discovery/staged Game delivery are now exercised. Ordinary invasion casualty/manual UI and browser rendering remain separate open gates; no whole BF24 closure.

## Retained ordinary invasion path audit — 2026-10-12

Source-only audit, root and independent Luna: InvasionWindow::resolve_ground_round calls remove_ground -> ground_hit_outcome_logged -> apply_ground_hit_with_origin, which applies the documented automatic sustain/cheapest-casualty rule. It does NOT call absorb_ground, the chooser used by standalone ground_combat. The earlier proposed retained-Game manual casualty test cannot pass under the current documented contract. No legality/timing/ground-hit behavior changed in this continuation. Enabling ordinary invasion casualty choices is a separate behavior change requiring a reconciled combat contract, retained-Game red test, logged sustain/destroy preservation, neutral handling and illegal-answer rollback evidence; callback coverage must not be presented as that implementation. Current live suite15passes as recorded in Saar evidence. The remaining presentation surface is the DESKTOP Rust GUI; historical browser wording above is imprecise.
