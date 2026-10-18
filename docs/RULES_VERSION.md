# Rules version (gating changed engine decisions)

A saved game is a log of decisions that the engine replays: after a restart, in the recovery
preflight of a deploy, on an offline resume. If a later build changes **what the engine offers or
does** at some point (an option that is no longer offered, a different order of events), a log that
answered the old offer no longer replays. `rules_version` is how such a change keeps old games
playable.

## What it is

`GameState::rules_version: u32` (crates/ti4-model/src/state.rs). Constants next to it:

- `RULES_VERSION` (now 15): what a **new** game plays. `GameState::new` sets it, so every way of
  creating a game (server, lobby, wasm host, selfplay, tests) gets the latest.
- `RULES_<CHANGE>` (now `RULES_INFILTRATE_FIX = 1`, `RULES_MENDOSA_SHIP = 2`, `RULES_SABOTAGE_TIMING = 3`, `RULES_LETNEV_FLEET_POOL = 4`, `RULES_ENIGMATIC_DEVICE = 5`, `RULES_TECH_PAYMENT_CHOICE = 6`, `RULES_BRIBERY_PER_PLAYER = 7`, `RULES_MUNITIONS_AFTER_ROLL = 8`, `RULES_TECH_RIDER = 9`, `RULES_ACTION_CARD_TARGETS = 10`, `RULES_RETREAT_HAZARDS = 11`, `RULES_COMMAND_TOKEN_POOL = 12`, `RULES_SELF_ASSEMBLY = 13`, `RULES_AGENDA_GRANT_REVEAL = 14`, `RULES_TECH_EFFECTS = 15`): the first version with one gated change.
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

Version 12 = Command token pool choice (`RULES_COMMAND_TOKEN_POOL`): a command token gained without a
named pool (52.4) is asked through `strategy_cards::gain_tokens` ("gain a command token into which
pool", one question per token). Applies to the exploration cards that gain tokens (Volatile Fuel
Source, the Entropic Field frontier rewards, the Keleres ship cards: all went to the strategy pool) and to
Leadership Rider (three tokens, went to the fleet pool). Version < 12: unchanged. Cards naming a pool
(Regulations, Rally, Hacan breakthrough, Ralnel/Naaz and others already asking) are untouched.

Version 13 = Self-Assembly Routines (`fb-warfare-mech-production`, `RULES_SELF_ASSEMBLY`):
`technology::self_assembly_after_production`, called where a `ProductionWindow` ends a use of
PRODUCTION that produced units (not an ability's production): a player holding a ready "sar" is
asked (`self_assembly_mech`, options = their controlled planets in that system, or decline); yes
exhausts the card and places 1 mech from reinforcements on that planet. Version < 13: owned and
inert, never asked.

Second ability (same version): `technology::timing_abilities` registers, for every seat, an
`after GROUND_FORCE_DESTROYED` reaction (condition: rules version >= 13, the event's player holds
"sar", the unit's base type is mech) that gains 1 trade good through `gain_trade_goods_via`
(announced as `TRADE_GOODS_GAINED`, source `self_assembly_routines`). One trade good per destroyed
mech (each destroyed mech is its own event; the card reads "after 1 of your mechs is destroyed").
It covers every path that announces `GROUND_FORCE_DESTROYED`: ground combat, bombardment, action
cards, agendas, unit abilities. Not covered: a mech lost as cargo when its ship is destroyed (no
ground-force event exists for cargo).

Version 14 = Grant Reallocation and agenda replacement (`fb-grant-allocation-agenda`,
`RULES_AGENDA_GRANT_REVEAL`; alpha feedback fb_1792301444_7526f5e2, game off_b1447d791342).

1. Research Grant Reallocation (`agenda_effects`, `grant_reallocation`): the elected player is asked
   `grant_reallocation_choose_technology` (any technology they do not own, a faction technology only
   if it is theirs), gains it (`technology::gain`, no prerequisites needed, nothing that fires on
   research) and removes one fleet token per prerequisite of it (none for a technology without
   prerequisites). Version < 14: no question, no technology, and one fleet token removed whatever the
   technology would have cost.
2. `Game::reveal_agenda`: an agenda revealed with no candidate to elect (Classified Document Leaks
   with no scored secret objective; Elect Law cards with no law in play; Elect Strategy Card, which
   the engine offers no candidates for) is discarded and the next card of the deck is revealed in its
   place ("discard this card and reveal another agenda from the top of the deck"). Version < 14: the
   card was dropped and the phase had one agenda fewer (the phase ended after one agenda).
3. Display: `AgendaRecord::discard_reason` (`no_choice` or `replaced`, left out of the JSON when
   unset). From version 14 a card discarded without a vote (the case above, Veto, Quash, Political
   Favor) is listed in `agenda_log` with `discarded`, no outcome and the reason, so the agenda
   toast and the toast history tell the table. `agendas_resolved_this_round` (bot) does not count
   such records.

Version 15 = technology effects (`tech-effect-fixes`, `RULES_TECH_EFFECTS`). Seven card effects that were missing:

1. Infantry II (`inf2`, `infantry_ii.rs`): after a generic `infantry2` is destroyed (`GROUND_FORCE_DESTROYED`), roll 1 die; 6 or more puts it on the card (a count in `faction_marks`, `inf2:card:<player>`; off the board); on the owner's `TURN_BEGAN` the units go to the first planet they control in the home system (no question), or wait; a player without the card loses what is on it.
2. Antimass Deflectors (`amd`): `combat::space_cannon_offense` asks 1 more of every SPACE CANNON die against the owner (the roll is recorded with the raised hit number, as Assail records its own).
3. Magen Defense Grid (`md`, the Codex 4 printing, `magen_defense.rs`): `SYSTEM_ACTIVATED` ("when") places 1 infantry per structure on its planet; `GROUND_COMBAT_STARTED` produces 1 hit that the card's owner assigns to one of the opponent's ground forces (question `magen_defense_hit` only when the opponent has several kinds; the target's owner decides on SUSTAIN DAMAGE).
4. Psychoarchaeology (`pa`): `technology::specialties` counts exhausted planets and `exhaust_specialties_for_research` exhausts nothing.
5. AI Development Algorithm (`aida`): a ready card ignores 1 prerequisite of a unit upgrade (`prerequisites_met_with`); research exhausts it only when the other holdings and waivers fall short.
6. Predictive Intelligence (`pi`): after a voter's planets and trade goods, `VoteWindow` asks `vote_predictive_intelligence` (3 additional votes); if taken and the backed outcome is not resolved, the card is exhausted when the vote closes.

Version < 15: none of these. The new questions only exist at version 15, so older logs replay unchanged; the timing abilities of items 1 and 3 are registered for every seat but their conditions are false below 15. Rule bot golden benches v1 to v5 are hash-identical with `RULES_VERSION` pinned to 11; at 15 the games differ (new effects, e.g. every Sol seat starts with Antimass Deflectors) and `baseline.json` needs a deliberate re-record.

Version 16 = faction effects (`faction-effect-fixes`, `RULES_FACTION_EFFECTS`). Six card effects that were missing or wrong:

1. Letnev breakthrough Gravleash Maneuvers (`letnevbt`, `combat::gravleash_maneuvers`): "before you roll dice during space combat, apply +X to the results of 1 of your ship's rolls, X = ship types you have in the combat". In every round `fleet_groups` splits one die off its group with the hit number lowered by X (minimum 1): the die the bonus helps most, ties to the first ship in board order (no question). Not for unit abilities.
2. Sol agent (`solagent`, Evelyn DeLouis): `borrowed_round_agents::owner_abilities` registers `leader:sol:solagent:GROUND_COMBAT_ROUND_STARTED:after` for each Sol seat; `Game`'s ground round emits the event while a readied Sol agent exists. Question `leader_ssruu_round_agent_unit` (the Ssruu copy's, any ground force in the system), the agent exhausts, `extra_die_for` adds the die for that round. From 16 every ground round ticks `combat_round_seq` (as space rounds do), and `roll_ground` also reads `extra_die_round`/`extra_die_unit` set by `leaders::dispatch_leader`.
3. Mentak Promise of Protection (`pop`): `promissory::take` no longer places it faceup; the holder plays it with `faction|mentak|pop` (`factions/mentak.rs`), the existing Pillage bar and return on activation apply once it is faceup.
4. Ral Nel commander (`ralnelcommander`): `leaders::commander_unlocked` reads `Feat::LastToPass`; `Game` runs the unlock check at the last pass.
5. Letnev commander (`letnevcommander`, Rear Admiral Farran): after a sustain the holder is asked `letnev_commander_trade_good` (gain / decline) instead of always being paid.
6. (Naaz fragment purge: already awarded the Shard of the Throne's point through `relics::gain`; no change.)

Version < 16: none of these. The ability slot of item 2 is only registered at 16, so older logs replay unchanged. Rule bot golden benches v1 to v4 are hash-identical with `RULES_VERSION` pinned to 11; v5's `baseline.json` needs a deliberate re-record only if its games reach these effects.

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
