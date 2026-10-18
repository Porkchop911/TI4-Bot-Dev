//! The Naalu Collective (`naalu`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope; the per-item record is
//! `plans/evidence/BF-naalu.md`.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Telepathic: "At the end of the strategy phase: Place the Naalu \"0\" token on your strategy
//!   card; you are first in initiative order."
//! * Foresight: "After another player moves ships into a system that contains 1 or more of your
//!   ships: You may place 1 token from your strategy pool in an adjacent system that does not
//!   contain another player's ships; move your ships from the active system into that system."
//! * Neuroglaive (`ng`): "After another player activates a system that contains 1 or more of your
//!   ships, that player removes 1 token from their fleet pool and returns it to their
//!   reinforcements."
//! * Hybrid Crystal Fighter II (`hcf2`): "This unit may move without being transported. Fighters
//!   in excess of your ships' capacity count as 1/2 of a ship against your fleet pool."
//! * Matriarch (flagship): "During an invasion in this system, you may commit fighters to planets
//!   as if they were ground forces. After combat, return those units to the space area."
//! * Iconoclast (`naalu_mech_te`, the Thunder's Edge printing `ti4_content::units::faction_unit`
//!   deals at `DEFAULT`): "DEPLOY: When another player gains a relic, place 1 mech on any planet
//!   you control."
//! * Gift of Prescience (`gift`): "At the end of the Strategy Phase: Place this card faceup in
//!   your play area and place the Naalu '0' token on your strategy card, you are the first in
//!   initiative order. The Naalu player cannot use their Telepathic faction ability during this
//!   game round. Return this card to the Naalu player at the end of the status phase."
//! * Z'eu (`naaluagent-te`, the Thunder's Edge agent `leaders::for_faction` deals at `DEFAULT`):
//!   "After any player's command token is placed in a system: You may exhaust this card to return
//!   that token to that player's reinforcements."
//! * M'aban (`naalucommander`): "You may look at your neighbors' hands of promissory notes and the
//!   top and bottom card of the agenda deck." Unlock: "Have ground forces in or adjacent to the
//!   Mecatol Rex system." (the unlock is shared code, `leaders::commander_unlocked`).
//! * The Oracle (`naaluhero`): "At the end of the status phase: You may force each other player to
//!   give you 1 promissory note from their hand. If you do, purge this card."
//! * Mindsieve (`naalubt`): "When you would resolve the secondary ability of another player's
//!   strategy card, you may give them a promissory note to resolve it without spending a command
//!   token."
//!
//! Every Naalu asset is claimed in [`MODULE`]; routes and tests are in `plans/evidence/BF-naalu.md`
//! and `BF-NAALU-TOKEN-PLACEMENTS.md` (Z'eu reacts to every `COMMAND_TOKEN_PLACED` the game emits
//! or stages).

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_content::units::catalogue;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus, TokenPool};
use ti4_model::units::Unit;

use super::hooks_combat::CombatHooks;
use super::hooks_ground::GroundHooks;
use super::hooks_strategy::{SecondaryWaiver, StrategyHooks};
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::phrases::{Phrase, Subject, Worded};
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// The faction alias; also the faction name in promissory note ids (`gift:naalu`).
const FACTION: &str = "naalu";
const GIFT: &str = "gift:naalu";
const AGENT: &str = "naaluagent-te";
const HERO: &str = "naaluhero";
/// `faction_marks` key: the round in which Gift of Prescience was played (and Telepathic is off).
const GIFT_MARK: &str = "naalu:gift_round";

/// What this faction implements. Only what is whole and tested: the rest is in the module docs.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &["telepathic", "foresight"],
    technologies: &["ng", "hcf2"],
    units: &[
        "naalu_fighter",
        "naalu_fighter2",
        "naalu_flagship",
        "naalu_mech_te",
    ],
    promissory: &["gift"],
    leaders: &[HERO, "naalucommander", AGENT],
    breakthroughs: &["naalubt"],
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        combat: CombatHooks {
            fighter_fleet_weight_halves: Some(fighter_fleet_weight_halves),
            ..CombatHooks::NONE
        },
        ground: GroundHooks {
            temporary_space_commit_candidates: Some(matriarch_fighters),
            temporary_ground_force: Some(matriarch_temporary_ground_force),
            ..GroundHooks::NONE
        },
        strategy: StrategyHooks {
            strategy_phase_ended: Some(strategy_phase_ended),
            secondary_waivers: Some(secondary_waivers),
            secondary_waived: Some(secondary_waived),
            ..StrategyHooks::NONE
        },
        cards: super::hooks_cards::CardHooks {
            may_view_hand: Some(may_view_promissory_hand),
            ..super::hooks_cards::CardHooks::NONE
        },
        ..Hooks::NONE
    },
};

/// M'aban: the live permission follows the recipient's commander rights and neighbour status.
fn may_view_promissory_hand(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    galaxy: Option<&Galaxy>,
    viewer: &PlayerId,
    owner: &PlayerId,
    kind: super::hooks_cards::RevealKind,
) -> bool {
    if kind != super::hooks_cards::RevealKind::PromissoryNotes || !commander_has_peek(state, viewer)
    {
        return false;
    }
    galaxy.is_some_and(|galaxy| crate::transactions::are_neighbours(state, galaxy, viewer, owner))
}

/// Whether the bound Naalu seat currently owns its unlocked M'aban commander.
pub(crate) fn commander_has_peek(state: &GameState, player: &PlayerId) -> bool {
    crate::promissory::has_commander_ability(state, player, "naalucommander")
}

// -- small readers -------------------------------------------------------------------------------

fn is_naalu(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

/// Owns the technology, or the Nekro's Valefar Assimilator carries its text.
fn has_technology(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::has_technology_text(state, player, alias)
}

fn leader_status(state: &GameState, player: &PlayerId, leader: &str) -> Option<LeaderStatus> {
    crate::leaders::status(state, player, &LeaderId::new(leader))
}

fn decision(state: &GameState, player: &PlayerId, card: &str, subtype: &str) -> DecisionContext {
    DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility(card.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

/// The player's ships (not carried ground forces) standing in a system's space area.
fn ships_in(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<Unit> {
    let types = catalogue(content, sources);
    state
        .ships_of(player, system)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(ti4_content::units::UnitType::is_ship)
        })
        .cloned()
        .collect()
}

/// The promissory notes a player holds in hand: not faceup in a play area.
fn hand_notes(state: &GameState, player: &PlayerId) -> Vec<String> {
    crate::promissory::held_by(state, player)
        .into_iter()
        .filter(|note| !state.promissory_faceup.contains(note))
        .collect()
}

// -- Telepathic and Gift of Prescience -----------------------------------------------------------

/// Whether Gift of Prescience was played this game round (Telepathic is then off).
fn gift_played_this_round(state: &GameState) -> bool {
    state.faction_marks.get(GIFT_MARK).map(String::as_str) == Some(state.round.to_string().as_str())
}

/// The seat that holds the Naalu player's Gift of Prescience, if a different faction holds it and
/// the note is not already played.
fn gift_holder(state: &GameState) -> Option<PlayerId> {
    let holder = state.promissory_notes.get(GIFT)?;
    if is_naalu(state, holder) || crate::promissory::seat_of(state, FACTION).is_none() {
        return None;
    }
    Some(holder.clone())
}

/// Telepathic: "Place the Naalu '0' token on your strategy card; you are first in initiative
/// order." Mandatory, so decided here without a question. The token is the Naalu player's only
/// while Gift of Prescience has not been played this round (it then sits on the holder's card).
fn strategy_phase_ended(state: &mut GameState) {
    if gift_played_this_round(state) {
        return;
    }
    let naalu: Vec<PlayerId> = state
        .players
        .iter()
        .filter(|seat| seat.faction.as_str() == FACTION)
        .map(|seat| seat.id.clone())
        .collect();
    for player in naalu {
        crate::strategy::set_initiative_override(state, &player, 0);
    }
}

/// Gift of Prescience, played by its holder at the end of the strategy phase (typed
/// `STRATEGY_PHASE_ENDED`, which `game.rs` emits before initiative is read).
fn gift(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    let ready = |state: &GameState, who: &PlayerId| {
        !gift_played_this_round(state) && gift_holder(state).as_ref() == Some(who)
    };
    Ability::stateful(
        format!("promissory:{owner_name}:gift:STRATEGY_PHASE_ENDED:after"),
        seat.clone(),
        "STRATEGY_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            if !ready(context.state, &owner) {
                return Ok(());
            }
            crate::strategy::set_initiative_override(context.state, &owner, 0);
            context.state.promissory_faceup.insert(GIFT.to_owned());
            context
                .state
                .faction_marks
                .insert(GIFT_MARK.to_owned(), context.state.round.to_string());
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        ready(context.state, &condition_seat)
    }))
}

/// "Return this card to the Naalu player at the end of the status phase."
fn gift_return(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    let ready = |state: &GameState, who: &PlayerId| {
        state.faction_marks.contains_key(GIFT_MARK)
            && state.promissory_notes.get(GIFT) == Some(who)
            && !is_naalu(state, who)
    };
    Ability::stateful(
        format!("promissory:{owner_name}:gift_return:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            if ready(context.state, &owner) {
                crate::promissory::give_back(context.state, GIFT);
                context.state.faction_marks.remove(GIFT_MARK);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        ready(context.state, &condition_seat)
    }))
}

// -- Hybrid Crystal Fighter II -------------------------------------------------------------------

/// Excess Hybrid Crystal Fighter IIs weigh half a ship against the fleet pool (`fleet::standing`).
fn fighter_fleet_weight_halves(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
    unit_type: &str,
) -> bool {
    unit_type == "naalu_fighter2" && is_naalu(state, player)
}

// -- Matriarch -------------------------------------------------------------------------------

/// Matriarch lets its owner's fighters in this system join an invasion temporarily. The invasion
/// window records that provenance and returns survivors to the space area after combat.
fn matriarch_fighters(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    invader: &PlayerId,
    system: &SystemId,
) -> Vec<Unit> {
    if !state.ships_of(invader, system).into_iter().any(|unit| {
        super::flagship_has_text(state, invader, unit.type_id.as_str(), "naalu_flagship")
    }) {
        return Vec::new();
    }
    let types = catalogue(content, sources);
    state
        .ships_of(invader, system)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(ti4_content::units::UnitType::is_fighter)
        })
        .cloned()
        .collect()
}

fn matriarch_temporary_ground_force(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    _planet: &ti4_model::id::PlanetId,
    unit: &Unit,
) -> bool {
    catalogue(content, sources)
        .get(unit.type_id.as_str())
        .is_some_and(ti4_content::units::UnitType::is_fighter)
        && state.ships_of(player, system).into_iter().any(|ship| {
            super::flagship_has_text(state, player, ship.type_id.as_str(), "naalu_flagship")
        })
}

// -- Neuroglaive ---------------------------------------------------------------------------------

fn neuroglaive_ready(
    context: &TimingContext<'_>,
    event: &crate::event::Event,
    seat: &PlayerId,
) -> Option<PlayerId> {
    if !has_technology(context.state, seat, "ng") {
        return None;
    }
    let activator = PlayerId::new(event.text("player")?);
    let system = SystemId::new(event.text("system")?);
    if &activator == seat
        || context
            .state
            .player(&activator)
            .is_none_or(|other| other.fleet_tokens <= 0)
        || ships_in(
            context.state,
            context.content,
            context.sources,
            seat,
            &system,
        )
        .is_empty()
    {
        return None;
    }
    Some(activator)
}

/// Neuroglaive: mandatory, so it resolves without a question.
fn neuroglaive(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("technology:{owner_name}:ng:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if let Some(activator) = neuroglaive_ready(context, event, &owner)
                && let Some(seat) = context.state.player_mut(&activator)
            {
                // The token returns to reinforcements: the engine counts reinforcements as what
                // is on neither the sheet nor the board.
                seat.spend_token(TokenPool::Fleet);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        neuroglaive_ready(context, event, &condition_seat).is_some()
    }))
}

// -- Foresight -----------------------------------------------------------------------------------

/// Adjacent systems Naalu may relocate into: no Naalu command token there already (a system holds
/// one token per player), no other player's ships, and the relocation itself is legal (checked
/// on a copy of the state, so an offered system is one that resolves).
fn foresight_destinations(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    seat: &PlayerId,
    from: &SystemId,
) -> Vec<SystemId> {
    let ships = ships_in(state, content, sources, seat, from);
    if ships.is_empty() {
        return Vec::new();
    }
    let adjacency = crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, seat);
    adjacency
        .neighbours(from.as_str())
        .into_iter()
        .map(SystemId::new)
        .filter(|to| {
            !state
                .board
                .get(to)
                .is_some_and(|system| system.command_tokens.contains(seat))
        })
        .filter(|to| {
            let mut scratch = state.clone();
            crate::transit::relocate_ships(
                &mut scratch,
                content,
                sources,
                galaxy,
                &crate::transit::Relocation {
                    player: seat,
                    from,
                    to,
                    ships: &ships,
                    require_adjacent: true,
                    forbid_foreign_ships_at_destination: true,
                    refuse_fleet_overflow: false,
                    reason: "foresight",
                },
            )
            .is_ok()
        })
        .collect()
}

fn foresight_window(
    context: &TimingContext<'_>,
    event: &crate::event::Event,
    seat: &PlayerId,
) -> Option<(SystemId, Vec<SystemId>)> {
    let mover = event.text("player")?;
    if mover == seat.as_str() || !is_naalu(context.state, seat) {
        return None;
    }
    // "moves ships into a system": a movement step that brought nothing in does not count.
    if event.integer("ships_moved").unwrap_or(0) <= 0 {
        return None;
    }
    let from = SystemId::new(event.text("system")?);
    if context
        .state
        .player(seat)
        .is_none_or(|player| player.strategic_tokens <= 0)
    {
        return None;
    }
    let galaxy = context.galaxy?;
    let destinations = foresight_destinations(
        context.state,
        context.content,
        context.sources,
        galaxy,
        seat,
        &from,
    );
    (!destinations.is_empty()).then_some((from, destinations))
}

/// Foresight. The ships go (with the ground forces and fighters aboard in the space area, which a
/// ship cannot leave behind), then the strategy token is spent and placed in the new system. The
/// placed token opens the ordinary `COMMAND_TOKEN_PLACED` window immediately.
fn foresight(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:foresight:MOVEMENT_FINISHED:after"),
        seat.clone(),
        "MOVEMENT_FINISHED",
        Relation::After,
        Arc::new(move |event, resolver, context| {
            let Some((from, destinations)) = foresight_window(context, event, &owner) else {
                return Ok(());
            };
            let options: Vec<ChoiceOption> = destinations
                .iter()
                .map(|system| {
                    ChoiceOption::labelled(
                        format!("system|{system}"),
                        "system",
                        format!("move your ships from {from} to {system}"),
                    )
                })
                .collect();
            let choice = Choice::new(
                owner.clone(),
                "Foresight: where do your ships go".to_owned(),
                options,
            )
            .contextualized(decision(
                context.state,
                &owner,
                "foresight",
                "destination",
            ));
            let answer = context
                .ask_seeing(&choice)
                .map_err(TimingError::IllegalChoice)?;
            let Some(picked) = choice.options.iter().position(|o| o.id == answer.id) else {
                return Ok(());
            };
            let to = destinations[picked].clone();
            let ships = ships_in(
                context.state,
                context.content,
                context.sources,
                &owner,
                &from,
            );
            let carried: Vec<Unit> = context
                .state
                .ships_of(&owner, &from)
                .into_iter()
                .filter(|unit| !ships.contains(unit))
                .cloned()
                .collect();
            let Some(galaxy) = context.galaxy else {
                return Ok(());
            };
            if crate::transit::relocate_ships(
                context.state,
                context.content,
                context.sources,
                galaxy,
                &crate::transit::Relocation {
                    player: &owner,
                    from: &from,
                    to: &to,
                    ships: &ships,
                    require_adjacent: true,
                    forbid_foreign_ships_at_destination: true,
                    refuse_fleet_overflow: false,
                    reason: "foresight",
                },
            )
            .is_err()
            {
                return Ok(());
            }
            if !carried.is_empty() {
                context.state.system_mut(&from).remove(&carried);
                context.state.system_mut(&to).add(&carried);
            }
            crate::supply::spend_strategy_token_staged(context.state, &owner, "naalu");
            context.state.system_mut(&to).place_token(owner.clone());
            super::hooks_cards::announce_command_token_placed(
                context,
                resolver,
                &owner,
                &to,
                super::hooks_cards::TokenPool::Strategy,
            )?;
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        foresight_window(context, event, &condition_seat).is_some()
    }))
}

// -- Z'eu (Thunder's Edge agent) -----------------------------------------------------------------

fn agent_ready(
    context: &TimingContext<'_>,
    event: &crate::event::Event,
    seat: &PlayerId,
) -> Option<(PlayerId, SystemId)> {
    if leader_status(context.state, seat, AGENT) != Some(LeaderStatus::Readied) {
        return None;
    }
    let player = PlayerId::new(event.text("player")?);
    let system = SystemId::new(event.text("system")?);
    context
        .state
        .board
        .get(&system)
        .is_some_and(|here| here.command_tokens.contains(&player))
        .then_some((player, system))
}

/// Z'eu, for the placements the engine announces (the activation token, typed `SYSTEM_ACTIVATED`).
fn agent(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{AGENT}:COMMAND_TOKEN_PLACED:after"),
        seat.clone(),
        "COMMAND_TOKEN_PLACED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some((player, system)) = agent_ready(context, event, &owner) else {
                return Ok(());
            };
            if crate::leaders::exhaust(context.state, &owner, &LeaderId::new(AGENT)) {
                context
                    .state
                    .system_mut(&system)
                    .command_tokens
                    .remove(&player);
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        agent_ready(context, event, &condition_seat).is_some()
    }))
}

// -- Iconoclast (Thunder's Edge mech) ------------------------------------------------------------

fn mech_ready(context: &TimingContext<'_>, event: &crate::event::Event, seat: &PlayerId) -> bool {
    is_naalu(context.state, seat)
        && event
            .text("player")
            .is_some_and(|gainer| gainer != seat.as_str())
        && !crate::action_cards::placement_spots(
            context.state,
            context.content,
            context.sources,
            seat,
            crate::action_cards::PlacementTarget::ControlledPlanet,
            None,
        )
        .is_empty()
}

/// DEPLOY, on the typed `RELIC_GAINED` the game announces at the start of the step after any relic
/// is gained (so the window opens at the next step, not mid-effect; a DEPLOY is a free placement,
/// so the delay changes nothing the board can show).
fn mech(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:naalu_mech_te:RELIC_GAINED:after"),
        seat.clone(),
        "RELIC_GAINED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if !mech_ready(context, event, &owner) {
                return Ok(());
            }
            crate::action_cards::place_units_choosing(
                context,
                &owner,
                "mech",
                1,
                crate::action_cards::PlacementTarget::ControlledPlanet,
                None,
                false,
                "naalu_mech_te",
                crate::action_cards::PlacementLimits::Respect,
            )
            .map_err(TimingError::IllegalChoice)?;
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        mech_ready(context, event, &condition_seat)
    }))
}

// -- The Oracle ----------------------------------------------------------------------------------

/// The other players that hold a promissory note in hand.
fn hero_victims(state: &GameState, seat: &PlayerId) -> Vec<PlayerId> {
    state
        .players
        .iter()
        .filter(|other| &other.id != seat && !hand_notes(state, &other.id).is_empty())
        .map(|other| other.id.clone())
        .collect()
}

fn hero_ready(state: &GameState, seat: &PlayerId) -> bool {
    leader_status(state, seat, HERO) == Some(LeaderStatus::Unlocked)
        && !hero_victims(state, seat).is_empty()
}

/// The Oracle: each player with a note in hand chooses which to give (a player with one gives it
/// without being asked). Offered only when someone has a note, so "if you do" is always met.
fn hero(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{HERO}:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            if !hero_ready(context.state, &owner) {
                return Ok(());
            }
            // Every answer first, then the transfer: an illegal answer changes nothing.
            let mut taken: Vec<String> = Vec::new();
            for victim in hero_victims(context.state, &owner) {
                let notes = hand_notes(context.state, &victim);
                let note = if let [only] = notes.as_slice() {
                    only.clone()
                } else {
                    let worded: Vec<(String, Worded)> = notes
                        .iter()
                        .map(|note| {
                            (
                                format!("note|{note}"),
                                Phrase::of("give {note} to the Naalu player").with(
                                    "note",
                                    note,
                                    Subject::note(context.content, note),
                                ),
                            )
                        })
                        .collect();
                    let options: Vec<ChoiceOption> = worded
                        .iter()
                        .map(|(id, worded)| {
                            ChoiceOption::labelled(id, "promissory_note", worded.label())
                        })
                        .collect();
                    let choice = Choice::new(
                        victim.clone(),
                        "The Oracle: which promissory note do you give".to_owned(),
                        options,
                    )
                    .phrased_options(&worded)
                    .contextualized(decision(
                        context.state,
                        &victim,
                        HERO,
                        "give_note",
                    ));
                    let answer = context
                        .ask_seeing(&choice)
                        .map_err(TimingError::IllegalChoice)?;
                    let Some(picked) = choice.options.iter().position(|o| o.id == answer.id) else {
                        continue;
                    };
                    notes[picked].clone()
                };
                taken.push(note);
            }
            for note in &taken {
                crate::promissory::take(context.state, context.content, &owner, note);
            }
            crate::leaders::purge(context.state, &owner, &LeaderId::new(HERO));
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        hero_ready(context.state, &condition_seat)
    }))
}

// -- Mindsieve -----------------------------------------------------------------------------------

const MINDSIEVE_PREFIX: &str = "ms|";

/// One waiver per note the follower could give: the note is part of the choice, since the
/// waiver hooks have no table to ask with.
fn secondary_waivers(
    state: &GameState,
    content: &ContentStore,
    follower: &PlayerId,
    primary: &PlayerId,
    _card: &str,
) -> Vec<SecondaryWaiver> {
    if follower == primary || !crate::breakthroughs::holds(state, follower, "naalubt") {
        return Vec::new();
    }
    hand_notes(state, follower)
        .into_iter()
        .map(|note| {
            let worded =
                Phrase::of("resolve the secondary by giving {primary} the {note} promissory note")
                    .with("primary", primary, Subject::seat(primary))
                    .with("note", &note, Subject::note(content, &note));
            SecondaryWaiver {
                id: format!("{MINDSIEVE_PREFIX}{note}"),
                label: worded.label(),
                worded: Some(worded),
            }
        })
        .collect()
}

fn secondary_waived(
    state: &mut GameState,
    content: &ContentStore,
    follower: &PlayerId,
    primary: &PlayerId,
    waiver: &str,
) {
    let Some(note) = waiver.strip_prefix(MINDSIEVE_PREFIX) else {
        return;
    };
    if follower != primary
        && crate::breakthroughs::holds(state, follower, "naalubt")
        && hand_notes(state, follower).iter().any(|held| held == note)
    {
        crate::promissory::take(state, content, primary, note);
    }
}

// -- timing abilities ----------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        gift(owner_name, seat),
        gift_return(owner_name, seat),
        neuroglaive(owner_name, seat),
        foresight(owner_name, seat),
        mech(owner_name, seat),
        hero(owner_name, seat),
    ];
    abilities.push(agent(owner_name, seat));
    for candidate in &state.players {
        if candidate.id != *seat
            && candidate
                .leaders
                .contains_key(&LeaderId::new("yssarilagent"))
            && state
                .player(seat)
                .is_some_and(|source| source.leaders.contains_key(&LeaderId::new(AGENT)))
        {
            abilities.push(borrowed_agent(owner_name, seat, &candidate.id));
        }
    }
    abilities
}

/// Ssruu copies the Naalu agent's printed timing effect. The source agent may be exhausted; the
/// readied Ssruu is the card that exhausts when its holder uses the copied text.
fn borrowed_agent(owner_name: &str, source: &PlayerId, borrower: &PlayerId) -> Ability {
    let (condition_source, condition_borrower) = (source.clone(), borrower.clone());
    let effect_source = source.clone();
    let effect_borrower = borrower.clone();
    Ability::stateful(
        format!("leader:{owner_name}:{source}:yssarilagent:{AGENT}:COMMAND_TOKEN_PLACED:after"),
        borrower.clone(),
        "COMMAND_TOKEN_PLACED",
        Relation::After,
        Arc::new(move |event, _, context| {
            let Some((player, system)) = agent_target(context, event) else {
                return Ok(());
            };
            if !has_borrowable_naalu_agent(
                context.state,
                context.content,
                &effect_source,
                &effect_borrower,
            ) {
                return Ok(());
            }
            if crate::leaders::exhaust(
                context.state,
                &effect_borrower,
                &LeaderId::new("yssarilagent"),
            ) {
                context
                    .state
                    .system_mut(&system)
                    .command_tokens
                    .remove(&player);
                super::hooks_cards::borrowed_agent_used(
                    context,
                    &effect_borrower,
                    &LeaderId::new(AGENT),
                );
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        agent_target(context, event).is_some()
            && has_borrowable_naalu_agent(
                context.state,
                context.content,
                &condition_source,
                &condition_borrower,
            )
    }))
}

fn has_borrowable_naalu_agent(
    state: &GameState,
    content: &ContentStore,
    source: &PlayerId,
    borrower: &PlayerId,
) -> bool {
    super::hooks_cards::borrowable_agents(state, content, borrower)
        .iter()
        .any(|(owner, agent)| owner == source && agent.as_str() == AGENT)
}

fn agent_target(
    context: &TimingContext<'_>,
    event: &crate::event::Event,
) -> Option<(PlayerId, SystemId)> {
    let player = PlayerId::new(event.text("player")?);
    let system = SystemId::new(event.text("system")?);
    context
        .state
        .board
        .get(&system)
        .is_some_and(|board| board.command_tokens.contains(&player))
        .then_some((player, system))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::TechnologyId;
    use ti4_model::id::{StrategyCardId, UnitTypeId};

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn game() -> GameState {
        let mut state = crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol")], DEFAULT);
        // The fixture deals notes before it swaps the factions in; deal them to the real ones.
        crate::promissory::deal(&mut state, ContentStore::embedded(), DEFAULT);
        state
    }

    #[test]
    fn alliance_maban_rights_are_live_neighbor_scoped_and_recipient_bound() {
        let content = ContentStore::embedded();
        let hub = crate::fixtures::plain_hub();
        let mut state =
            crate::fixtures::seated_game(&[("a", "sol"), ("b", FACTION), ("c", "hacan")], DEFAULT);
        for board in state.board.values_mut() {
            board.units.clear();
            board.planet_units.clear();
            board.planet_control.clear();
        }
        let alliance = "an:naalu";
        state.promissory_notes.insert(alliance.to_owned(), a());
        state.promissory_faceup.insert(alliance.to_owned());
        crate::fixtures::put(
            &mut state,
            &SystemId::new(&hub.outer[0]),
            "cruiser",
            &a(),
            1,
        );
        crate::fixtures::put(&mut state, &SystemId::new(&hub.centre), "cruiser", &b(), 1);
        let far = hub.across(&hub.outer[0]);
        crate::fixtures::put(
            &mut state,
            &SystemId::new(&far),
            "cruiser",
            &PlayerId::new("c"),
            1,
        );
        let may_view = |state: &GameState, viewer: &PlayerId, owner: &PlayerId| {
            may_view_promissory_hand(
                state,
                content,
                DEFAULT,
                Some(&hub.galaxy),
                viewer,
                owner,
                super::super::hooks_cards::RevealKind::PromissoryNotes,
            )
        };

        assert!(
            !may_view(&state, &a(), &b()),
            "locked Alliance grants no rights"
        );
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("naalucommander"), LeaderStatus::Unlocked);
        assert!(
            may_view(&state, &a(), &b()),
            "neighbor's unlocked commander applies"
        );
        assert!(
            !may_view(&state, &a(), &PlayerId::new("c")),
            "permission is neighbor-only"
        );
        assert!(
            !may_view(&state, &PlayerId::new("c"), &b()),
            "permission is recipient-bound"
        );

        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("naalucommander"), LeaderStatus::Locked);
        assert!(
            !may_view(&state, &a(), &b()),
            "locking restores privacy live"
        );
    }

    #[test]
    fn ownerless_commander_grant_enables_neighbor_hand_and_agenda_peeks_for_recipient() {
        let content = ContentStore::embedded();
        let hub = crate::fixtures::plain_hub();
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            content,
            &a(),
            "naalucommander"
        ));
        state.agenda_deck = vec!["agenda_top".to_owned(), "agenda_bottom".to_owned()];
        crate::fixtures::put(&mut state, &SystemId::new(&hub.centre), "cruiser", &a(), 1);
        crate::fixtures::put(
            &mut state,
            &SystemId::new(&hub.outer[0]),
            "cruiser",
            &b(),
            1,
        );
        assert!(commander_has_peek(&state, &a()));
        assert!(!commander_has_peek(&state, &b()));
        assert!(may_view_promissory_hand(
            &state,
            content,
            DEFAULT,
            Some(&hub.galaxy),
            &a(),
            &b(),
            super::super::hooks_cards::RevealKind::PromissoryNotes,
        ));
        let observed = crate::choice::Observed::new(&state, content, DEFAULT, Some(&hub.galaxy));
        assert_eq!(
            crate::choice::SeatObservation::bind(&observed, a()).agenda_deck_ends(),
            Some(("agenda_top".to_owned(), "agenda_bottom".to_owned()))
        );
        assert_eq!(
            crate::choice::SeatObservation::bind(&observed, b()).agenda_deck_ends(),
            None,
            "only the bound grant recipient sees the agenda deck ends"
        );
    }

    fn payload(pairs: &[(&str, &str)]) -> BTreeMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| {
                let value = if *k == "ships_moved" {
                    serde_json::Value::from(v.parse::<i64>().unwrap())
                } else {
                    serde_json::Value::from(*v)
                };
                ((*k).to_owned(), value)
            })
            .collect()
    }

    fn emit_in(
        state: &mut GameState,
        galaxy: Option<&Galaxy>,
        table: &mut crate::choice::Table,
        event_type: &str,
        pairs: &[(&str, &str)],
    ) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, galaxy, table, |ctx| {
            let event = ctx
                .event_sequence
                .next(event_type, payload(pairs))
                .expect("an event id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("the window resolves");
        });
    }

    fn emit(
        state: &mut GameState,
        table: &mut crate::choice::Table,
        event_type: &str,
        pairs: &[(&str, &str)],
    ) {
        emit_in(state, None, table, event_type, pairs);
    }

    fn scripted(answers: &[&str]) -> crate::choice::Table {
        crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|s| (*s).to_owned()),
        )))
    }

    fn flush_staged_token_events(state: &mut GameState, answers: &[&str]) -> usize {
        let content = ContentStore::embedded();
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut table = scripted(answers);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut sequence = crate::event::EventSequence::new();
        let mut ctx = crate::choice::Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        crate::supply::flush_staged_events(state, &mut ctx)
    }

    fn unit(kind: &str, who: &PlayerId) -> Unit {
        Unit::new(UnitTypeId::new(kind), who.clone())
    }

    /// Centre 18 with a ring around it; Naalu's cruisers sit in `from`.
    struct Board {
        state: GameState,
        galaxy: Galaxy,
        from: SystemId,
        beside: SystemId,
    }

    fn board() -> Board {
        let mut state = game();
        let content = ContentStore::embedded();
        let ids: Vec<String> = crate::fixtures::plain_systems(12)
            .into_iter()
            .filter(|id| !state.board.contains_key(&SystemId::new(id.as_str())))
            .take(8)
            .collect();
        let mut tiles = vec!["18"];
        tiles.extend(ids.iter().map(String::as_str));
        let galaxy = Galaxy::build(content, &tiles, DEFAULT, 2).unwrap();
        let from = SystemId::new(ids[0].as_str());
        let beside = galaxy
            .adjacent(from.as_str())
            .into_iter()
            .find(|id| *id != "18" && ids.iter().any(|plain| plain == id))
            .map(SystemId::new)
            .expect("a ring neighbour");
        state
            .system_mut(&from)
            .add(&[unit("cruiser", &a()), unit("cruiser", &a())]);
        Board {
            state,
            galaxy,
            from,
            beside,
        }
    }

    // -- neutrality ------------------------------------------------------------------------------

    #[test]
    fn a_game_without_naalu_is_offered_nothing_and_keeps_its_initiative_order() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        state.card_initiative.insert(StrategyCardId::new("late"), 8);
        state
            .card_initiative
            .insert(StrategyCardId::new("early"), 1);
        state
            .player_mut(&a())
            .unwrap()
            .strategy_cards
            .push(StrategyCardId::new("late"));
        state
            .player_mut(&b())
            .unwrap()
            .strategy_cards
            .push(StrategyCardId::new("early"));
        let order = state.initiative_order();
        assert_eq!(order, vec![b(), a()]);
        crate::factions::hooks_strategy::strategy_phase_ended(&mut state);
        assert_eq!(state.initiative_order(), order);
        assert!(state.initiative_overrides.is_empty());

        let (home, other) = (
            state.player(&a()).unwrap().home_system.clone().unwrap(),
            state.player(&b()).unwrap().home_system.clone().unwrap(),
        );
        crate::fixtures::put(&mut state, &home, "cruiser", &a(), 1);
        let before = state.clone();
        for (event, pairs) in [
            ("STRATEGY_PHASE_ENDED", vec![]),
            ("STATUS_PHASE_ENDED", vec![]),
            (
                "SYSTEM_ACTIVATED",
                vec![("player", "a"), ("system", other.as_str())],
            ),
            (
                "MOVEMENT_FINISHED",
                vec![
                    ("player", "a"),
                    ("system", home.as_str()),
                    ("ships_moved", "1"),
                ],
            ),
            (
                "COMMAND_TOKEN_PLACED",
                vec![("player", "a"), ("system", other.as_str())],
            ),
            ("RELIC_GAINED", vec![("player", "a")]),
        ] {
            emit(&mut state, &mut scripted(&[]), event, &pairs);
        }
        assert!(state == before, "no Naalu window changed the game");
        assert!(
            secondary_waivers(&state, ContentStore::embedded(), &a(), &b(), "Trade").is_empty()
        );
    }

    // -- Telepathic ------------------------------------------------------------------------------

    fn with_cards(state: &mut GameState) {
        state.card_initiative.insert(StrategyCardId::new("late"), 8);
        state
            .card_initiative
            .insert(StrategyCardId::new("early"), 1);
        state
            .player_mut(&a())
            .unwrap()
            .strategy_cards
            .push(StrategyCardId::new("late"));
        state
            .player_mut(&b())
            .unwrap()
            .strategy_cards
            .push(StrategyCardId::new("early"));
    }

    #[test]
    fn telepathic_makes_naalu_first_in_initiative_order() {
        let mut state = game();
        with_cards(&mut state);
        assert_eq!(state.initiative_order(), vec![b(), a()]);
        crate::factions::hooks_strategy::strategy_phase_ended(&mut state);
        assert_eq!(state.initiative_order(), vec![a(), b()]);
        assert_eq!(state.initiative_overrides.get(&b()), None);
    }

    // -- Gift of Prescience ----------------------------------------------------------------------

    const GIFT_ABILITY: &str = "promissory:sol:gift:STRATEGY_PHASE_ENDED:after";

    #[test]
    fn gift_gives_its_holder_the_token_and_switches_telepathic_off_until_status_ends() {
        let mut state = game();
        with_cards(&mut state);
        state.promissory_notes.insert(GIFT.to_owned(), b());
        emit(
            &mut state,
            &mut scripted(&[GIFT_ABILITY]),
            "STRATEGY_PHASE_ENDED",
            &[],
        );
        assert_eq!(state.initiative_overrides.get(&b()), Some(&0));
        assert!(state.promissory_faceup.contains(GIFT));
        // Telepathic is off for the round, so Naalu does not take the token.
        crate::factions::hooks_strategy::strategy_phase_ended(&mut state);
        assert_eq!(state.initiative_overrides.get(&a()), None);
        assert_eq!(state.initiative_order(), vec![b(), a()]);
        // Returned at the end of the status phase.
        emit(&mut state, &mut scripted(&[]), "STATUS_PHASE_ENDED", &[]);
        assert_eq!(state.promissory_notes.get(GIFT), Some(&a()));
        assert!(!state.promissory_faceup.contains(GIFT));
        assert!(!state.faction_marks.contains_key(GIFT_MARK));
    }

    #[test]
    fn gift_is_optional_and_not_offered_to_its_owner_or_a_non_holder() {
        let mut state = game();
        state.promissory_notes.insert(GIFT.to_owned(), b());
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "STRATEGY_PHASE_ENDED",
            &[],
        );
        assert!(state.initiative_overrides.is_empty());
        assert!(!state.faction_marks.contains_key(GIFT_MARK));
        // Telepathic still works for the round.
        crate::factions::hooks_strategy::strategy_phase_ended(&mut state);
        assert_eq!(state.initiative_overrides.get(&a()), Some(&0));
        // Held by its owner: nothing to play (an unscripted answer would diverge).
        let mut home = game();
        emit(&mut home, &mut scripted(&[]), "STRATEGY_PHASE_ENDED", &[]);
        assert!(home.initiative_overrides.is_empty());
        assert!(gift_holder(&home).is_none());
    }

    // -- Neuroglaive -----------------------------------------------------------------------------

    const NG_ABILITY: &str = "technology:naalu:ng:SYSTEM_ACTIVATED:after";

    fn grant_ng(state: &mut GameState) {
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new("ng"));
    }

    #[test]
    fn neuroglaive_costs_the_activator_a_fleet_token_where_naalu_has_ships() {
        let mut t = board();
        grant_ng(&mut t.state);
        let before = t.state.player(&b()).unwrap().fleet_tokens;
        let mine = t.state.player(&a()).unwrap().fleet_tokens;
        emit(
            &mut t.state,
            &mut scripted(&[NG_ABILITY]),
            "SYSTEM_ACTIVATED",
            &[
                ("player", "b"),
                ("system", t.from.as_str()),
                ("ships_moved", "1"),
            ],
        );
        assert_eq!(t.state.player(&b()).unwrap().fleet_tokens, before - 1);
        assert_eq!(t.state.player(&a()).unwrap().fleet_tokens, mine);
    }

    #[test]
    fn neuroglaive_needs_the_technology_ships_there_and_another_player() {
        let mut t = board();
        let before = t.state.player(&b()).unwrap().fleet_tokens;
        // Not researched.
        emit(
            &mut t.state,
            &mut scripted(&[]),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", t.from.as_str())],
        );
        grant_ng(&mut t.state);
        // No Naalu ships in that system.
        emit(
            &mut t.state,
            &mut scripted(&[]),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", t.beside.as_str())],
        );
        // Naalu's own activation.
        let mine = t.state.player(&a()).unwrap().fleet_tokens;
        emit(
            &mut t.state,
            &mut scripted(&[]),
            "SYSTEM_ACTIVATED",
            &[
                ("player", "a"),
                ("system", t.from.as_str()),
                ("ships_moved", "1"),
            ],
        );
        assert_eq!(t.state.player(&b()).unwrap().fleet_tokens, before);
        assert_eq!(t.state.player(&a()).unwrap().fleet_tokens, mine);
    }

    // -- Foresight -------------------------------------------------------------------------------

    const FORESIGHT_ABILITY: &str = "ability:naalu:foresight:MOVEMENT_FINISHED:after";

    #[test]
    fn foresight_moves_the_ships_and_spends_a_strategy_token_into_the_new_system() {
        let mut t = board();
        // Keep this focused on Foresight; the staged-token agent has dedicated tests below.
        t.state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Exhausted);
        let tokens = t.state.player(&a()).unwrap().strategic_tokens;
        assert!(tokens > 0);
        // A carried infantry rides along.
        crate::fixtures::put(&mut t.state, &t.from, "infantry", &a(), 1);
        let answer = format!("system|{}", t.beside);
        emit_in(
            &mut t.state,
            Some(&t.galaxy),
            &mut scripted(&[FORESIGHT_ABILITY, &answer]),
            "MOVEMENT_FINISHED",
            &[
                ("player", "b"),
                ("system", t.from.as_str()),
                ("ships_moved", "1"),
            ],
        );
        assert!(t.state.ships_of(&a(), &t.from).is_empty());
        let there = t.state.ships_of(&a(), &t.beside);
        assert_eq!(there.len(), 3, "two cruisers and the infantry aboard");
        assert_eq!(t.state.player(&a()).unwrap().strategic_tokens, tokens - 1);
        assert!(
            t.state
                .board
                .get(&t.beside)
                .unwrap()
                .command_tokens
                .contains(&a())
        );
    }

    #[test]
    fn foresight_is_refused_without_a_token_a_legal_system_or_for_the_mover_itself() {
        let mut t = board();
        // Own move.
        emit_in(
            &mut t.state,
            Some(&t.galaxy),
            &mut scripted(&[]),
            "MOVEMENT_FINISHED",
            &[
                ("player", "a"),
                ("system", t.from.as_str()),
                ("ships_moved", "1"),
            ],
        );
        assert_eq!(t.state.ships_of(&a(), &t.from).len(), 2);
        // No strategy token.
        t.state.player_mut(&a()).unwrap().strategic_tokens = 0;
        emit_in(
            &mut t.state,
            Some(&t.galaxy),
            &mut scripted(&[]),
            "MOVEMENT_FINISHED",
            &[
                ("player", "b"),
                ("system", t.from.as_str()),
                ("ships_moved", "1"),
            ],
        );
        assert_eq!(t.state.ships_of(&a(), &t.from).len(), 2);
        // Every neighbour holds another player's ships or Naalu's own token.
        let mut full = board();
        let neighbours: Vec<String> = full
            .galaxy
            .adjacent(full.from.as_str())
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
        for system in &neighbours {
            crate::fixtures::put(
                &mut full.state,
                &SystemId::new(system.as_str()),
                "cruiser",
                &b(),
                1,
            );
        }
        assert!(
            foresight_destinations(
                &full.state,
                ContentStore::embedded(),
                DEFAULT,
                &full.galaxy,
                &a(),
                &full.from,
            )
            .is_empty()
        );
        emit_in(
            &mut full.state,
            Some(&full.galaxy),
            &mut scripted(&[]),
            "MOVEMENT_FINISHED",
            &[
                ("player", "b"),
                ("system", full.from.as_str()),
                ("ships_moved", "1"),
            ],
        );
        assert_eq!(full.state.ships_of(&a(), &full.from).len(), 2);
    }

    #[test]
    fn foresight_needs_the_mover_to_have_moved_ships_in() {
        let mut t = board();
        emit_in(
            &mut t.state,
            Some(&t.galaxy),
            &mut scripted(&[]),
            "MOVEMENT_FINISHED",
            &[
                ("player", "b"),
                ("system", t.from.as_str()),
                ("ships_moved", "0"),
            ],
        );
        assert_eq!(t.state.ships_of(&a(), &t.from).len(), 2);
    }

    #[test]
    fn foresight_excludes_systems_that_already_hold_naalus_token() {
        let mut t = board();
        let all = foresight_destinations(
            &t.state,
            ContentStore::embedded(),
            DEFAULT,
            &t.galaxy,
            &a(),
            &t.from,
        );
        assert!(all.contains(&t.beside));
        t.state.system_mut(&t.beside).place_token(a());
        let fewer = foresight_destinations(
            &t.state,
            ContentStore::embedded(),
            DEFAULT,
            &t.galaxy,
            &a(),
            &t.from,
        );
        assert!(!fewer.contains(&t.beside));
    }

    // -- Mindsieve -------------------------------------------------------------------------------

    fn grant_bt(state: &mut GameState) {
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("naalubt"));
    }

    #[test]
    fn mindsieve_offers_one_waiver_per_note_in_hand_and_hands_the_note_over() {
        let mut state = game();
        let content = ContentStore::embedded();
        assert!(secondary_waivers(&state, content, &a(), &b(), "Trade").is_empty());
        grant_bt(&mut state);
        let offered = secondary_waivers(&state, content, &a(), &b(), "Trade");
        assert_eq!(
            offered.len(),
            crate::promissory::held_by(&state, &a()).len()
        );
        assert!(offered.len() >= 2);
        let pick = offered[0].id.clone();
        let note = pick.strip_prefix(MINDSIEVE_PREFIX).unwrap().to_owned();
        secondary_waived(&mut state, content, &a(), &b(), &pick);
        assert_eq!(state.promissory_notes.get(&note), Some(&b()));
        // Gone from the hand, so it cannot be given twice.
        secondary_waived(&mut state, content, &a(), &b(), &pick);
        assert_eq!(state.promissory_notes.get(&note), Some(&b()));
    }

    /// Each waiver names its note and the seat that receives it (`details.phrases`).
    #[test]
    fn mindsieve_names_the_note_and_the_primary() {
        let mut state = game();
        let content = ContentStore::embedded();
        grant_bt(&mut state);
        for waiver in secondary_waivers(&state, content, &a(), &b(), "Trade") {
            let note = waiver.id.strip_prefix(MINDSIEVE_PREFIX).unwrap();
            let phrase = waiver.worded.expect("named").phrase();
            assert_eq!(
                crate::phrases::rendered(&phrase, &[("primary", "b"), ("note", note)]),
                waiver.label
            );
            assert_eq!(phrase.refs["primary"], crate::phrases::Subject::seat(&b()));
            assert_eq!(
                phrase.refs["note"],
                crate::phrases::Subject::note(content, note)
            );
        }
    }

    #[test]
    fn mindsieve_is_not_offered_for_ones_own_card_or_without_the_breakthrough() {
        let mut state = game();
        let content = ContentStore::embedded();
        grant_bt(&mut state);
        assert!(secondary_waivers(&state, content, &a(), &a(), "Trade").is_empty());
        assert!(secondary_waivers(&state, content, &b(), &a(), "Trade").is_empty());
    }

    // -- The Oracle ------------------------------------------------------------------------------

    const HERO_ABILITY: &str = "leader:naalu:naaluhero:STATUS_PHASE_ENDED:after";

    fn unlock_hero(state: &mut GameState) {
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(HERO), LeaderStatus::Unlocked);
    }

    #[test]
    fn the_hero_takes_a_note_from_each_other_player_and_is_purged() {
        let mut state = game();
        unlock_hero(&mut state);
        let theirs = hand_notes(&state, &b());
        assert!(theirs.len() > 1);
        let chosen = theirs[1].clone();
        emit(
            &mut state,
            &mut scripted(&[HERO_ABILITY, &format!("note|{chosen}")]),
            "STATUS_PHASE_ENDED",
            &[],
        );
        assert_eq!(state.promissory_notes.get(&chosen), Some(&a()));
        assert_eq!(hand_notes(&state, &b()).len(), theirs.len() - 1);
        assert_eq!(
            leader_status(&state, &a(), HERO),
            Some(LeaderStatus::Purged)
        );
    }

    /// Each note of a victim's question says which note it is (`details.phrases`).
    #[test]
    fn the_hero_names_each_note_of_its_question() {
        let mut state = game();
        unlock_hero(&mut state);
        let theirs = hand_notes(&state, &b());
        let (mut table, seen) = crate::phrases::capturing(&[HERO_ABILITY]);
        emit(&mut state, &mut table, "STATUS_PHASE_ENDED", &[]);
        let seen = seen.borrow();
        let choice = seen
            .iter()
            .find(|choice| choice.player == b() && choice.prompt.starts_with("The Oracle"))
            .expect("asked");
        let hints = crate::phrases::phrased(choice);
        let content = ContentStore::embedded();
        for note in &theirs {
            let phrase = &hints.options[&format!("note|{note}")];
            assert_eq!(phrase.text, "give {note} to the Naalu player");
            assert_eq!(
                phrase.refs["note"],
                crate::phrases::Subject::note(content, note)
            );
        }
    }

    #[test]
    fn the_hero_needs_to_be_unlocked_may_be_declined_and_keeps_the_card_when_declined() {
        let mut state = game();
        // Locked at the start.
        let before = state.clone();
        emit(&mut state, &mut scripted(&[]), "STATUS_PHASE_ENDED", &[]);
        assert!(state == before);
        unlock_hero(&mut state);
        let kept = hand_notes(&state, &b()).len();
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "STATUS_PHASE_ENDED",
            &[],
        );
        assert_eq!(
            leader_status(&state, &a(), HERO),
            Some(LeaderStatus::Unlocked)
        );
        assert_eq!(hand_notes(&state, &b()).len(), kept);
    }

    // -- Z'eu ------------------------------------------------------------------------------------

    const AGENT_ABILITY: &str = "leader:naalu:naaluagent-te:COMMAND_TOKEN_PLACED:after";

    #[test]
    fn the_agent_returns_an_activation_token_and_exhausts() {
        let mut t = board();
        t.state.system_mut(&t.from).place_token(b());
        emit(
            &mut t.state,
            &mut scripted(&[AGENT_ABILITY]),
            "COMMAND_TOKEN_PLACED",
            &[
                ("player", "b"),
                ("system", t.from.as_str()),
                ("ships_moved", "1"),
            ],
        );
        assert!(
            !t.state
                .board
                .get(&t.from)
                .unwrap()
                .command_tokens
                .contains(&b())
        );
        assert_eq!(
            leader_status(&t.state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[test]
    fn the_agent_needs_a_token_there_and_may_be_declined() {
        let mut t = board();
        // No token was placed in that system.
        emit(
            &mut t.state,
            &mut scripted(&[]),
            "COMMAND_TOKEN_PLACED",
            &[
                ("player", "b"),
                ("system", t.from.as_str()),
                ("ships_moved", "1"),
            ],
        );
        assert_eq!(
            leader_status(&t.state, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        t.state.system_mut(&t.from).place_token(b());
        emit(
            &mut t.state,
            &mut scripted(&["decline"]),
            "COMMAND_TOKEN_PLACED",
            &[
                ("player", "b"),
                ("system", t.from.as_str()),
                ("ships_moved", "1"),
            ],
        );
        assert!(
            t.state
                .board
                .get(&t.from)
                .unwrap()
                .command_tokens
                .contains(&b())
        );
    }

    #[test]
    fn ssruu_copies_readied_or_exhausted_naalu_agent_and_exhausts_only_ssruu() {
        for source_status in [LeaderStatus::Readied, LeaderStatus::Exhausted] {
            let mut t = board();
            t.state
                .player_mut(&b())
                .unwrap()
                .leaders
                .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
            t.state
                .player_mut(&a())
                .unwrap()
                .leaders
                .insert(LeaderId::new(AGENT), source_status);
            t.state.system_mut(&t.from).place_token(b());
            let borrowed =
                format!("leader:naalu:a:yssarilagent:{AGENT}:COMMAND_TOKEN_PLACED:after");
            let mut answers = Vec::new();
            if source_status == LeaderStatus::Readied {
                answers.push("decline");
            }
            answers.push(borrowed.as_str());
            emit(
                &mut t.state,
                &mut scripted(&answers),
                "COMMAND_TOKEN_PLACED",
                &[("player", "b"), ("system", t.from.as_str())],
            );
            assert!(!t.state.board[&t.from].command_tokens.contains(&b()));
            assert_eq!(leader_status(&t.state, &a(), AGENT), Some(source_status));
            assert_eq!(
                leader_status(&t.state, &b(), "yssarilagent"),
                Some(LeaderStatus::Exhausted)
            );
        }
    }

    #[test]
    fn borrowed_naalu_agent_is_inert_without_readied_ssruu() {
        let mut t = board();
        t.state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Exhausted);
        t.state.system_mut(&t.from).place_token(b());
        let before = t.state.clone();
        emit(
            &mut t.state,
            &mut scripted(&[]),
            "COMMAND_TOKEN_PLACED",
            &[("player", "b"), ("system", t.from.as_str())],
        );
        assert_eq!(t.state, before);

        t.state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Exhausted);
        let before = t.state.clone();
        emit(
            &mut t.state,
            &mut scripted(&[]),
            "COMMAND_TOKEN_PLACED",
            &[("player", "b"), ("system", t.from.as_str())],
        );
        assert_eq!(t.state, before);
    }

    #[test]
    fn armed_listener_survives_ssruu_readiness_changing_without_rearming() {
        let mut t = board();
        t.state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Exhausted);
        t.state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Exhausted);
        t.state.system_mut(&t.from).place_token(b());

        let mut resolver = crate::fixtures::armed_resolver(&t.state);
        t.state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        let borrowed = format!("leader:naalu:a:yssarilagent:{AGENT}:COMMAND_TOKEN_PLACED:after");
        let mut table = scripted(&[&borrowed]);
        crate::fixtures::with_context(&mut t.state, DEFAULT, None, &mut table, |context| {
            let event = context
                .event_sequence
                .next(
                    "COMMAND_TOKEN_PLACED",
                    payload(&[("player", "b"), ("system", t.from.as_str())]),
                )
                .expect("event id");
            resolver
                .emit_with_context(context, event, |_, _| {})
                .expect("timing window resolves");
        });

        assert!(!t.state.board[&t.from].command_tokens.contains(&b()));
        assert_eq!(
            leader_status(&t.state, &a(), AGENT),
            Some(LeaderStatus::Exhausted),
            "copying the text never readies or exhausts the source agent"
        );
        assert_eq!(
            leader_status(&t.state, &b(), "yssarilagent"),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[test]
    fn the_agent_returns_a_nonactivation_reinforcement_token_through_the_staged_resolver() {
        let mut t = board();
        t.state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Readied);
        assert!(crate::tokens::place_command_token_from_reinforcements(
            &mut t.state,
            &b(),
            &t.from,
        ));
        assert_eq!(
            crate::supply::staged_event_types(&t.state),
            ["COMMAND_TOKEN_PLACED"]
        );
        assert_eq!(flush_staged_token_events(&mut t.state, &[AGENT_ABILITY]), 1);
        assert!(!t.state.system_state(&t.from).command_tokens.contains(&b()));
        assert_eq!(
            leader_status(&t.state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[test]
    fn the_agent_may_decline_or_be_exhausted_when_a_staged_token_is_placed() {
        for (status, answers) in [
            (LeaderStatus::Readied, Some("decline")),
            (LeaderStatus::Exhausted, None),
        ] {
            let mut t = board();
            t.state
                .player_mut(&a())
                .unwrap()
                .leaders
                .insert(LeaderId::new(AGENT), status);
            assert!(crate::tokens::place_command_token_from_reinforcements(
                &mut t.state,
                &b(),
                &t.from,
            ));
            let scripted_answers: Vec<&str> = answers.into_iter().collect();
            assert_eq!(
                flush_staged_token_events(&mut t.state, &scripted_answers),
                1,
                "the staged placement is still announced"
            );
            assert!(t.state.system_state(&t.from).command_tokens.contains(&b()));
            assert_eq!(leader_status(&t.state, &a(), AGENT), Some(status));
        }
    }

    #[test]
    fn repeated_placements_exhaust_the_agent_once_and_do_not_open_a_duplicate_activation_window() {
        let mut t = board();
        t.state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Readied);
        assert!(crate::tokens::place_command_token_from_reinforcements(
            &mut t.state,
            &b(),
            &t.from,
        ));
        assert!(crate::tokens::place_command_token_from_reinforcements(
            &mut t.state,
            &b(),
            &t.beside,
        ));
        assert_eq!(flush_staged_token_events(&mut t.state, &[AGENT_ABILITY]), 2);
        assert!(!t.state.system_state(&t.from).command_tokens.contains(&b()));
        assert!(
            t.state
                .system_state(&t.beside)
                .command_tokens
                .contains(&b())
        );
        assert_eq!(
            leader_status(&t.state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
    }

    // -- Iconoclast ------------------------------------------------------------------------------

    #[test]
    fn the_mech_is_the_thunders_edge_printing_and_deploys_after_a_relic_gain() {
        let content = ContentStore::embedded();
        assert_eq!(
            ti4_content::units::faction_unit(content, FACTION, "mech", DEFAULT)
                .map(|unit| unit.id().to_owned())
                .as_deref(),
            Some("naalu_mech_te")
        );
        let mut state = game();
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        let planet = state
            .system_state(&home)
            .planet_control
            .iter()
            .find(|(_, owner)| **owner == a())
            .map(|(planet, _)| planet.clone())
            .expect("a controlled planet");
        let count = |state: &GameState| {
            state
                .system_state(&home)
                .on_planet_of(&planet, &a())
                .into_iter()
                .filter(|u| u.type_id.as_str() == "naalu_mech_te")
                .count()
        };
        let before = count(&state);
        emit(
            &mut state,
            &mut scripted(&[
                "unit:naalu:naalu_mech_te:RELIC_GAINED:after",
                &format!("{home}|{planet}"),
            ]),
            "RELIC_GAINED",
            &[("player", "b")],
        );
        assert_eq!(count(&state), before + 1, "one mech on the chosen planet");
        // Naalu's own relic does not trigger it.
        let mut own = game();
        let before = own.clone();
        emit(
            &mut own,
            &mut scripted(&[]),
            "RELIC_GAINED",
            &[("player", "a")],
        );
        assert!(own == before);
    }

    #[test]
    fn a_relic_gained_in_a_running_game_offers_the_deploy_once() {
        use crate::game::Game;
        let content = ContentStore::embedded();
        let mut state = game();
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(a());
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        let planet = state
            .system_state(&home)
            .planet_control
            .iter()
            .find(|(_, owner)| **owner == a())
            .map(|(planet, _)| planet.clone())
            .expect("a controlled planet");
        let table = scripted(&[
            "unit:naalu:naalu_mech_te:RELIC_GAINED:after",
            &format!("{home}|{planet}"),
        ]);
        let mut game = Game::with_table(state, content, table).with_sources(DEFAULT);
        game.state
            .player_mut(&b())
            .unwrap()
            .relics
            .push(ti4_model::id::RelicId::new("the_crown_of_emphidia"));
        assert_eq!(game.step().error, None);
        let mechs = game
            .state
            .system_state(&home)
            .on_planet_of(&planet, &a())
            .into_iter()
            .filter(|u| u.type_id.as_str() == "naalu_mech_te")
            .count();
        assert_eq!(mechs, 1, "events: {:?}", game.events);
        assert_eq!(game.step().error, None);
        let again = game
            .state
            .system_state(&home)
            .on_planet_of(&planet, &a())
            .into_iter()
            .filter(|u| u.type_id.as_str() == "naalu_mech_te")
            .count();
        assert_eq!(again, 1, "announced once per gain");
    }

    // -- units -----------------------------------------------------------------------------------

    #[test]
    fn the_hybrid_crystal_fighter_is_the_printed_unit() {
        let content = ContentStore::embedded();
        let types = catalogue(content, DEFAULT);
        let one = types.get("naalu_fighter").copied().expect("fighter I");
        assert_eq!(one.combat_hits_on(), Some(8));
        assert!((one.cost() - 0.5).abs() < f64::EPSILON);
        assert!(one.is_fighter());
        assert_eq!(one.move_value(), 0);
        let two = types.get("naalu_fighter2").copied().expect("fighter II");
        assert_eq!(two.combat_hits_on(), Some(7));
        assert!(two.move_value() > 0, "it moves without being transported");
    }

    #[test]
    fn matriarch_makes_only_its_own_fighters_temporary_ground_candidates() {
        let content = ContentStore::embedded();
        let mut state = game();
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        crate::fixtures::put(&mut state, &home, "naalu_fighter", &a(), 1);
        assert!(matriarch_fighters(&state, content, DEFAULT, &a(), &home).is_empty());
        crate::fixtures::put(&mut state, &home, "naalu_flagship", &a(), 1);
        let candidates = matriarch_fighters(&state, content, DEFAULT, &a(), &home);
        let fighters_in_space = state
            .ships_of(&a(), &home)
            .into_iter()
            .filter(|unit| unit.type_id.as_str() == "naalu_fighter")
            .count();
        assert_eq!(candidates.len(), fighters_in_space);
        assert!(
            candidates
                .iter()
                .all(|unit| unit.type_id.as_str() == "naalu_fighter")
        );
        assert!(matriarch_fighters(&state, content, DEFAULT, &b(), &home).is_empty());
    }

    #[test]
    fn foresight_resolves_once_after_a_real_movement_step() {
        use crate::game::{Game, TACTICAL_ACTION_ID};
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", FACTION)], DEFAULT);
        crate::promissory::deal(&mut state, content, DEFAULT);
        let ids: Vec<String> = crate::fixtures::plain_systems(12)
            .into_iter()
            .filter(|id| !state.board.contains_key(&SystemId::new(id.as_str())))
            .take(8)
            .collect();
        let mut tiles = vec!["18"];
        tiles.extend(ids.iter().map(String::as_str));
        let galaxy = Galaxy::build(content, &tiles, DEFAULT, 2).unwrap();
        let ring: Vec<String> = ids.clone();
        let from = SystemId::new(ring[0].as_str());
        let near: Vec<String> = galaxy
            .adjacent(from.as_str())
            .into_iter()
            .filter(|id| ring.iter().any(|plain| plain == id))
            .map(ToOwned::to_owned)
            .collect();
        assert!(near.len() >= 2);
        let (src, beside) = (
            SystemId::new(near[0].as_str()),
            SystemId::new(near[1].as_str()),
        );
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(a());
        crate::fixtures::put(&mut state, &from, "cruiser", &b(), 2);
        crate::fixtures::put(&mut state, &src, "destroyer", &a(), 1);
        let tokens = state.player(&b()).unwrap().strategic_tokens;
        let ability = "ability:naalu:foresight:MOVEMENT_FINISHED:after";
        let script: Vec<String> = vec![
            TACTICAL_ACTION_ID.to_owned(),
            from.to_string(),
            // Z'eu is registered only in test builds; decline its activation window.
            "decline".to_owned(),
            format!("move|{src}|0"),
            "done_moving".to_owned(),
            ability.to_owned(),
            format!("system|{beside}"),
            // Foresight's strategy-pool placement also opens the command-token window.
            "decline".to_owned(),
        ];
        let table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(script)));
        let mut game = Game::with_table(state, content, table).with_galaxy(galaxy);
        for _ in 0..12 {
            assert_eq!(game.step().error, None, "log: {:?}", game.events);
            if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                break;
            }
        }
        assert!(game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"));
        assert!(
            game.state.ships_of(&b(), &from).is_empty(),
            "Naalu left before combat"
        );
        assert_eq!(game.state.ships_of(&b(), &beside).len(), 2);
        assert_eq!(game.state.ships_of(&a(), &from).len(), 1);
        assert_eq!(
            game.state.player(&b()).unwrap().strategic_tokens,
            tokens - 1
        );
    }

    #[test]
    fn excess_hybrid_crystal_fighters_weigh_half_a_ship_for_naalu_only() {
        let mut state = game();
        let content = ContentStore::embedded();
        assert!(fighter_fleet_weight_halves(
            &state,
            content,
            &a(),
            "naalu_fighter2"
        ));
        assert!(!fighter_fleet_weight_halves(
            &state,
            content,
            &a(),
            "naalu_fighter"
        ));
        assert!(!fighter_fleet_weight_halves(
            &state,
            content,
            &b(),
            "naalu_fighter2"
        ));
        // Through the real fleet arithmetic: four excess fighter IIs cost two ships, not four.
        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].as_str());
        state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "naalu_fighter2", &a(), 4);
        let standing = crate::fleet::standing(&state, content, DEFAULT, &a(), &system, None);
        assert_eq!(standing.fleet_charged, 2);
        let mut plain = state.clone();
        plain.player_mut(&a()).unwrap().faction = ti4_model::id::FactionId::new("sol");
        let full = crate::fleet::standing(&plain, content, DEFAULT, &a(), &system, None);
        assert_eq!(
            full.fleet_charged, 4,
            "without the hook an excess fighter II costs a ship"
        );
    }

    #[test]
    fn the_claims_are_the_sheet() {
        assert_eq!(MODULE.abilities, ["telepathic", "foresight"]);
        assert!(MODULE.units.contains(&"naalu_fighter"));
        assert!(MODULE.units.contains(&"naalu_flagship"));
    }

    #[test]
    fn a_nekro_flagship_with_the_naalu_z_token_lets_fighters_join_an_invasion() {
        let content = ContentStore::embedded();
        let candidates = |lent: &[&str]| {
            let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], lent);
            let home = state.player(&a()).unwrap().home_system.clone().unwrap();
            crate::fixtures::put(&mut state, &home, "fighter", &a(), 2);
            crate::fixtures::put(&mut state, &home, "nekro_flagship", &a(), 1);
            matriarch_fighters(&state, content, DEFAULT, &a(), &home).len()
        };
        assert_eq!(candidates(&[]), 0, "off by default");
        assert!(
            candidates(&["naalu"]) >= 2,
            "every fighter in the system may join"
        );
    }
}
