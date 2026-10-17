# Rules version (gating changed engine decisions)

A saved game is a log of decisions that the engine replays: after a restart, in the recovery
preflight of a deploy, on an offline resume. If a later build changes **what the engine offers or
does** at some point (an option that is no longer offered, a different order of events), a log that
answered the old offer no longer replays. `rules_version` is how such a change keeps old games
playable.

## What it is

`GameState::rules_version: u32` (crates/ti4-model/src/state.rs). Constants next to it:

- `RULES_VERSION` (now 17): what a **new** game plays. `GameState::new` sets it, so every way of
  creating a game (server, lobby, wasm host, selfplay, tests) gets the latest.
- `RULES_<CHANGE>` (now `RULES_INFILTRATE_FIX = 1`, `RULES_MENDOSA_SHIP = 2`, `RULES_SABOTAGE_TIMING = 3`, `RULES_LETNEV_FLEET_POOL = 4`, `RULES_ENIGMATIC_DEVICE = 5`, `RULES_TECH_PAYMENT_CHOICE = 6`, `RULES_BRIBERY_PER_PLAYER = 7`, `RULES_MUNITIONS_AFTER_ROLL = 8`, `RULES_TECH_RIDER = 9`, `RULES_ACTION_CARD_TARGETS = 10`, `RULES_RETREAT_HAZARDS = 11`, `RULES_STATUS_GAIN = 12`, `RULES_TIMING_DOUBLE_PASS = 13`, `RULES_EXPLORE_BEFORE_CONTROL = 14`, `RULES_FLOATING_FACTORY_SPACE = 15`, `RULES_TE_MECATOL_LANDING = 16`, `RULES_TECH_PAYMENT_KEEPS_SPECIALTIES = 17`, `RULES_STATUS_REDISTRIBUTION_REFUSED = 17`): the first version with one gated change.
A state without the field is **version 0**, the legacy rules (what the alpha ran until 2026-10-16).

## How it is stored (the checksum rule)

`#[serde(default, skip_serializing_if = "is_zero_u32")]`: it is **not written while zero**. A legacy
`init.json` therefore serializes back to the exact bytes its envelope checksum covers, and the
`state_checksum` of its snapshots is unchanged. Never write a new field unconditionally into the
state, the init record or the envelope: that was the cause of the 2026-10-15 recovery regressions
(`action_turn`, `reaction_modes.json`; see commits f345235f, d284909c). A new game's `init.json`
records `"rules_version": 1` inside `initial_state`, and recovery reads it back from there (the
server replays from `init.json`'s `initial_state`, never from a fresh setup).

Wasm host (offline): `HostSave` (format 2, not bumped) and `Restart` carry `rules_version`
(left out for 0), and `Resume.rules_version`. `host_api::prepare` sets the state's version from the
resume (`None` = 0, legacy) and uses `RULES_VERSION` only for a start with no resume. So an
offline format 2 save written before the field resumes as legacy, and a new game's saves keep
theirs.

## Gated so far

Version 1 = the Infiltrate fix (`fix-infiltrate-offer`):

1. `action_cards::reaction_can_matter`: Infiltrate is offered only to the gaining player and only
   if a rival PDS or space dock can be replaced (`infiltrate_swaps`). Version 0: offered after
   every capture.
2. `InvasionWindow::finish_control_gain`: the previous holder's structures are destroyed after
   `PLANET_CONTROL_GAINED` (version 0: before it).
3. The Infiltrate effect and preview replace the rival's structures (version 0: the invader's own).

Version 2 = the Captain Mendosa fix (`fix-nightly-f2-mendosa`), `factions::saar`:

1. The boost mark is `seq|origin|type|damaged|count|bonus` and `agent_move_bonus` gives it to the
   first ship of that kind at the origin while no ship of the kind has left (version 0/1:
   `seq|origin|index|bonus`, matched to the list position, which shifts when an earlier ship leaves).
2. `agent_ships` offers equal ships (same type, damage, origin) once.

A game created at version 0 or 1 keeps playing the old behaviour.

Version 3 = Sabotage timing (`fix-sabotage-timing`, `RULES_SABOTAGE_TIMING`). Version 0/1/2: the
moment a player picked a card in a reaction window (e.g. "after you activate a system"), the card
was announced and every Sabotage holder was asked at once, before the other players of the same
window had declared anything. Version 3: **declaration round first**.

- In every reaction window except Sabotage's own (`ACTION_CARD_PLAYED`), choosing a card only
  *declares* it: the card is spent (hand to discard) and queued in `GameState::declared_card_plays`
  (not serialized while empty, so legacy checksums are byte-identical). The window keeps asking
  the seats in order, as before, until all have passed.
- Then, still inside the window and before the event resolves, the declared plays are announced
  in declaration order; each opens its own `ACTION_CARD_PLAYED` Sabotage window, offered to the
  holders in table order, never to the player themself and not for a Sabotage.
- Interpretation chosen (flag): "the declaration round" is one reaction window of one event; the
  Sabotage holder sees every declared card only as it is announced (it is not shown the whole
  list up front); a card declared earlier is not recalled if a later declaration makes it moot
  (e.g. two Public Disgraces); a card played in `ACTION_CARD_PLAYED` (Sabotage, Instinct Training)
  and component-action plays stay immediate. A Sabotage cancels only the one play it answers.

Version 4 = Letnev fleet pool (`fix-letnev-fleet-pool`, `RULES_LETNEV_FLEET_POOL`):

1. `tokens::TokenRedistribution::fleet_floor`: Letnev's redistribution (status phase, Warfare)
   also offers arrangements with fewer than two fleet tokens, down to none (the floor of two is a
   policy guard, not a rule; Armada allows pool + 2 ships per system). Version < 4: floor of two.

Version 5 = the Enigmatic Device researches (`RULES_ENIGMATIC_DEVICE`, branch
`enigmatic-device-tech-dialog`):

1. The relic (`relics::grant_chosen_technology`, only for `enigmaticdevice`) and both exploration
   cards (`exploration::enigmatic_device_technologies`) offer `technology::researchable_by_prerequisites`
   (not owned, own faction, not purged, prerequisites met) instead of every unowned technology.
   Waiver-only technologies (Inheritance Systems, faction waivers) are not offered: they charge a
   second price. The relic action is not offered when that list is empty.
2. The pick is researched (`technology::research_chosen`: specialties exhausted, unit upgrades,
   research consequences), not just inserted. Maw of Worlds (*gain*) is unchanged.
Version < 5 keeps the unfiltered list and the plain insert / `gain`.

Version 6 = `RULES_TECH_PAYMENT_CHOICE` (`tech-payment-choose-planets`): `strategy_cards::paid_research` (Technology secondary, and the primary's second
technology) asks the researcher the ordinary `pay_resources` question after the technology is
picked (`production::pay_seeing`). Version 0..5: the cheapest `payment::plans` entry is spent
automatically. Inheritance Systems' 2 resources (`technology::research`, no table) stay automatic.

Version 7 = Bribery asked once per voter (`fb-bribery-per-player`, `RULES_BRIBERY_PER_PLAYER`):

1. `Game::step_vote`: `VOTES_CAST` ("after you cast votes", "after the speaker votes": Bribery,
   Distinguished Councilor) fires once per voter, when they have named their outcome or abstained.
   Version 0/1: after every answer of the vote (the outcome, each planet exhausted, the trade
   goods), so a Bribery holder was asked again after every planet.

Version 8 = Munitions Reserves after the roll (`fb-munitions-timing`, `RULES_MUNITIONS_AFTER_ROLL`):

1. `faction_abilities::space_combat_round_started`: from version 8 it no longer asks Letnev for
   Munitions Reserves at the round's start (version < 8: asks blind, before any dice, and sets
   `munitions_round`, after which `combat::reroll_munitions_misses` redraws every miss).
2. `combat::reroll_window` (fleet rolls only): from version 8 it calls
   `faction_abilities::munitions_after_roll` once the fleet's dice are staged. The question
   (`munitions_reserves_reroll`, with `details.dice` and `details.misses`) offers `munitions`
   (pay 2, re-roll every missed die), `munitions_choose` (pay 2, then one `munitions_reroll_die`
   question per die) and decline; it is skipped with no miss or fewer than 2 trade goods.
   Re-rolled dice are recorded under the reason `munitions:<player>`.

Version 9 = Technology Rider and friends (`fb-technology-rider`, `RULES_TECH_RIDER`):

1. `action_cards::asked_rider_payoff`, reached from the vote close (`resolve_predictions_asking`):
   a correct Technology Rider researches 1 technology through the Technology primary's free
   research (`research_technology` question, source `ActionCard("tech_rider")`; prerequisites
   apply, nothing is paid). Version < 9: recorded, never performed.
2. Construction, Diplomacy and Warfare Riders ask for their planet (no space dock yet) or system
   when several qualify. Version < 9: paid only when exactly one choice existed, else skipped.

Not a change: Elder Qanoj (Xxcha commander) lets its owner vote after playing a rider, as printed.

Version 10 = action card targets (`fb-spacedock-card-targets`, `RULES_ACTION_CARD_TARGETS`):
`action_cards::is_playable` (so `available_actions` and `perform`) no longer offers a component
action ("Action" window) card that has no legal target: Reactor Meltdown (no space dock outside a
home system), Cripple Defenses, Lucky Shot, Plague, Uprising, Insubordination, Spy, Repeal Law,
Tactical Bombardment, Unstable Planet, Unexpected Action, Seize Artifact, Rise of a Messiah,
Frontline Deployment, War Effort, Scuttle (`has_legal_target`). Version < 10: always offered. Cards
with other conditions (Refit Troops, Ghost Ship, Fighter Conscription, the Pirate cards, Exploration
Probe, ...) are unchanged. Reaction windows are unchanged (Sabotage and bluffs stay).
Version 11 = Retreat hazards (`fb-retreat-asteroids`, `RULES_RETREAT_HAZARDS`):

1. `combat::retreat_may_enter`: retreat destinations (78.7c, Dark Energy Tap, Skilled Retreat) must
   pass `MovementRules::can_enter`: no asteroid field without Antimass Deflectors (11.1), no
   supernova (86.1) except faction exceptions. Nebulae stay allowed. Version < 11: no check.

Version 12 = the status-phase gain in the redistribution (cf5af3e0, `RULES_STATUS_GAIN`):

1. `Game::finish_status_phase`: each seat is asked one redistribution (81.5) over what it holds
   plus what it gains, capped by the reinforcements (20.4a), with `gained` in the details
   (`TokenRedistribution::with_gain`); `COMMAND_TOKEN_GAINED` is still emitted for each token.
   Version < 12: `Game::step_token_gain` asks the pool of each gained token
   (`gain_command_token`, `TokenGain::pending_choice`), then the redistribution gains nothing.

Version 13 = the second pass closes a window (niko 98fe8b1d, `RULES_TIMING_DOUBLE_PASS`):

1. `Resolver::run_window_with_context`: a seat that passes twice in a row closes the window, and a
   reaction slot opened and backed out of with no card played (the hand is unchanged) is a pass.
   Version < 13: every resolution clears the passes, and a backed-out slot is a resolution. The
   window with no state (`Resolver::emit`, tests only) has the new rule at every version.

Version 14 = exploration before control (niko, `RULES_EXPLORE_BEFORE_CONTROL`):

1. `InvasionWindow::advance_control`: a planet nobody controls is explored before control is
   established, and only while the invader still has a ground force there (35.1). Version < 14:
   explored after control was gained (`finish_control_gain`).

Version 15 = the Floating Factory in space (niko, `RULES_FLOATING_FACTORY_SPACE`):

1. `strategy_cards::construction_unit`: Saar's Floating Factory is placed by Construction and the
   Construction Rider in the space area. Version < 15: the generic space dock on the planet.

Version 16 = the Thunder's Edge Mecatol (niko, `RULES_TE_MECATOL_LANDING`):

1. `invasion::landable_planets`: `mrte` is not landable while the custodians token is on it, as
   `mr`. Version < 16: only `mr` is held back.

Version 17 = the research payment window and the refused redistribution (`mock-removal`, two
constants of one version):

1. `RULES_TECH_PAYMENT_KEEPS_SPECIALTIES`, `strategy_cards::paid_research` (Technology secondary,
   and the primary's second technology): the payment is asked inside the research's own payment
   window (`production::pay_in_window`; the Keleres agent is offered once, before the research
   question, not again by the payment). The planets whose specialties the chosen technology's
   prerequisites exhaust (`technology::specialties_spent_by_research`) are kept out of the payment,
   and a technology is offered only when it can be paid without them (`research_payable`). The
   `kept` of `details.payment` (`ResearchPaymentHints`) lists those planets, and is empty for an
   older game. Version 6..16: `production::pay_seeing` over every ready planet, every researchable
   technology offered (version 0..5: the cheapest plan, automatically).
2. `RULES_STATUS_REDISTRIBUTION_REFUSED`, `Game::finish_status_phase`: a status-phase
   redistribution (81.5) whose answer the engine refuses is an error of the step, and nothing is
   gained or announced for the seat. Version < 17: the refusal is passed over, and the gain goes
   into the tactic pool. Both versions keep the gain of `RULES_STATUS_GAIN` (version < 12: token
   by token).

## Adding the next gate

1. Raise `RULES_VERSION` in state.rs and add `pub const RULES_<NAME>: u32 = <new>;`.
2. Branch the changed decision on `state.rules_version >= RULES_<NAME>`; keep the old code path
   verbatim for lower versions (see `infiltrate_legacy`, `destroy_rival_structures` call order).
3. Test both: the new behaviour at `RULES_VERSION`, the old one with `state.rules_version = 0` (or
   the previous version), and a replay of a log recorded under the old version
   (`a_legacy_log_with_an_infiltrate_offer_replays_identically` in game.rs).
4. Run the recovery preflight (`deploy/public/deploy.sh --preflight-only --ref <head>`): every game
   that recovered before must still recover. Games in progress keep their stored version; only new
   games get the latest.

Do not use it for bug fixes that a replay cannot observe (a view, a label, a preview that is not
hashed), only for changes of offers, results or event order.
