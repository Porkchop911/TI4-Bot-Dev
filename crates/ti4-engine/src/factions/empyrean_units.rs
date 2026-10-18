//! Empyrean flagship (Dynamo), mech (Watcher), agent (Acamar) and the Blood Pact / Dark Pact
//! promissory notes. Everything else Empyrean is in `empyrean.rs`.
//!
//! Card text (content corpus, latest printing):
//!
//! * Dynamo, flagship: "After any player's unit in this system or an adjacent system uses SUSTAIN
//!   DAMAGE, you may spend 2 influence to repair that unit."
//! * Watcher, mech: "You may remove this unit from a system that contains or is adjacent to
//!   another player's units to cancel an action card played by that player."
//! * Acamar, agent: "After a player moves ships into a system that does not contain any planets:
//!   You may exhaust this card: that player gains 1 command token."
//! * Blood Pact, promissory note: "ACTION: Place this card faceup in your play area. When you and
//!   the Empyrean player cast votes for the same outcome, cast 4 additional votes for that
//!   outcome. If you activate a system that contains 1 or more of the Empyrean player's units,
//!   return this card to the Empyrean player."
//! * Dark Pact, promissory note: "ACTION: Place this card faceup in your play area. When you give
//!   a number of commodities to the Empyrean player equal to your maximum commodity value, you
//!   each gain 1 trade good. If you activate a system that contains 1 or more of the Empyrean
//!   player's units, return this card to the Empyrean player."
//!
//! Routes:
//!
//! * Dynamo hangs on `SUSTAIN_DAMAGE_USED` (a ship in a space combat or space cannon step) and on
//!   `GROUND_FORCE_SUSTAINED` (a ground force: `combat.rs` and `invasion.rs` emit them after the unit
//!   is damaged). The repair is paid with `production::pay_seeing`, so the player chooses the
//!   planets exactly as for any influence spend, and nothing changes if they cannot pay.
//! * Watcher hangs on the WHEN window of `ACTION_CARD_PLAYED`, like Instinct Training, and cancels
//!   the event. Adjacency needs the map; without one only the mech's own system counts.
//! * Acamar hangs on `MOVEMENT_FINISHED` (one window per movement step, `ships_moved` > 0, not one
//!   per ship). The command token is placed through `strategy_cards::gain_tokens`, which lets the
//!   player who moved pick the pool.
//! * Blood Pact and Dark Pact wait in the holder's hand until their ACTION
//!   (`promissory::play_action_note`, offered by [`component_actions`] / [`perform_component`]) and go
//!   home when the holder activates a system holding Empyrean units
//!   (`promissory::spend_support_on_activation`). Blood Pact's votes are banked by `vote.rs`
//!   ([`blood_pact_votes`]); Dark Pact's trade goods by `transactions::resolve`
//!   ([`dark_pact_gains`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::{GameState, LeaderStatus};
use ti4_model::units::Unit;

use super::hooks_cards::CardHooks;
use super::hooks_combat::CombatHooks;
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::event::Event;
use crate::movement::PlayerAdjacency;
use crate::production::Spend;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

const FLAGSHIP: &str = "empyrean_flagship";
const MECH: &str = "empyrean_mech";
const AGENT: &str = "empyreanagent";
const BLOOD_PACT: &str = "blood_pact";
const DARK_PACT: &str = "dark_pact";
/// The faction that owns the notes, and its name in note ids.
const FACTION: &str = super::empyrean::FACTION;
/// Influence Dynamo's repair costs.
const DYNAMO_COST: i64 = 2;
/// Votes Blood Pact adds when the holder and the Empyrean vote the same outcome.
const BLOOD_PACT_VOTES: i64 = 4;

/// Leaders claimed by the Empyrean module. `empyrean.rs` reports its leader claims (hero,
/// commander) to the coordinator, who adds them here.
pub const LEADERS: &[&str] = &[AGENT, "empyreancommander", "empyreanhero"];
/// Units claimed here (flagship, mech).
pub const UNITS: &[&str] = &[FLAGSHIP, MECH];
/// Promissory notes claimed by the Empyrean module.
pub const PROMISSORY: &[&str] = &[BLOOD_PACT, DARK_PACT];
/// Combat hooks for the flagship and notes. None: every window here is a timing ability.
pub const COMBAT_HOOKS: CombatHooks = CombatHooks::NONE;
/// Card hooks for the mech. None: the Watcher is a timing ability.
pub const CARD_HOOKS: CardHooks = CardHooks::NONE;

/// Timing abilities of the units, agent and notes, for one seat.
pub(crate) fn timing_abilities(
    _state: &GameState,
    owner_name: &str,
    seat: &PlayerId,
) -> Vec<Ability> {
    vec![
        dynamo(owner_name, seat, "SUSTAIN_DAMAGE_USED"),
        dynamo(owner_name, seat, "GROUND_FORCE_SUSTAINED"),
        watcher(owner_name, seat),
        acamar(owner_name, seat),
    ]
}

/// The one question every ability here asks.
fn ask(
    context: &mut TimingContext<'_>,
    who: &PlayerId,
    ability: &str,
    subtype: &str,
    prompt: String,
    options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, TimingError> {
    let choice = Choice::new(who.clone(), prompt, options).contextualized(DecisionContext::new(
        who.clone(),
        DecisionSource::FactionAbility(ability.to_owned()),
        subtype,
        context.state.phase,
        context.state.round,
    ));
    context
        .ask_seeing(&choice)
        .map_err(TimingError::IllegalChoice)
}

/// Whether `a` is `b` or next to it, as `owner` sees adjacency (breaches, their own links, tethers).
/// Without a map only the same system counts.
fn near(adjacency: Option<&PlayerAdjacency<'_>>, a: &SystemId, b: &SystemId) -> bool {
    a == b || adjacency.is_some_and(|adjacency| adjacency.are_adjacent(a.as_str(), b.as_str()))
}

/// `owner`'s adjacency on the map the context carries.
fn adjacency_of<'g>(context: &TimingContext<'g>, owner: &PlayerId) -> Option<PlayerAdjacency<'g>> {
    context.galaxy.map(|galaxy| {
        PlayerAdjacency::new(context.state, context.content, context.sources, galaxy, owner)
    })
}

// -- Dynamo -------------------------------------------------------------------------------------

/// A sustained unit Dynamo could repair.
struct Repair {
    system: SystemId,
    planet: Option<PlanetId>,
    owner: PlayerId,
    kind: UnitTypeId,
}

/// The unit a sustain event names, if it is still on the board and damaged.
fn repair_target(event: &Event, state: &GameState) -> Option<Repair> {
    let system = SystemId::new(event.text("system")?);
    let owner = PlayerId::new(event.text("player")?);
    let kind = UnitTypeId::new(event.text("unit")?);
    let planet = event.text("planet").map(PlanetId::new);
    let board = state.system_state(&system);
    let units = match &planet {
        Some(planet) => board.on_planet(planet),
        None => board.units.as_slice(),
    };
    units
        .iter()
        .any(|unit| unit.owner == owner && unit.type_id == kind && unit.sustained_damage)
        .then_some(Repair {
            system,
            planet,
            owner,
            kind,
        })
}

/// Whether `owner` has a Dynamo in the space area of `system` or of a system next to it.
pub(crate) fn dynamo_reaches(
    state: &GameState,
    galaxy: Option<&PlayerAdjacency<'_>>,
    owner: &PlayerId,
    system: &SystemId,
) -> bool {
    state.board.iter().any(|(at, board)| {
        near(galaxy, at, system)
            && board.units.iter().any(|unit| {
                &unit.owner == owner
                    && super::flagship_has_text(state, owner, unit.type_id.as_str(), FLAGSHIP)
            })
    })
}

/// The repair Dynamo offers `owner` for `event`: the unit is damaged, the flagship reaches it, and
/// the two influence can be paid.
fn dynamo_window(context: &TimingContext<'_>, owner: &PlayerId, event: &Event) -> Option<Repair> {
    let target = repair_target(event, context.state)?;
    (dynamo_reaches(
        context.state,
        adjacency_of(context, owner).as_ref(),
        owner,
        &target.system,
    )
        && crate::payment::affordable(
            context.state,
            context.content,
            context.sources,
            owner,
            DYNAMO_COST,
            Spend::Influence,
        ))
    .then_some(target)
}

/// Mark the first damaged matching unit repaired. `false` (nothing changed) if there is none.
fn repair(state: &mut GameState, target: &Repair) -> bool {
    let board = state.system_mut(&target.system);
    let units = match &target.planet {
        Some(planet) => board.planet_units.get_mut(planet),
        None => Some(&mut board.units),
    };
    let Some(units) = units else {
        return false;
    };
    let Some(unit) = units.iter_mut().find(|unit| {
        unit.owner == target.owner && unit.type_id == target.kind && unit.sustained_damage
    }) else {
        return false;
    };
    *unit = unit.repaired();
    true
}

/// Dynamo: after any player's unit in this system or an adjacent system uses SUSTAIN DAMAGE, the
/// Empyrean may spend 2 influence to repair it. Choosing the ability is the consent; the
/// planets to exhaust are asked by the payment. Nothing changes unless it is paid and repaired.
fn dynamo(owner_name: &str, seat: &PlayerId, event_type: &'static str) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:{FLAGSHIP}:{event_type}:after"),
        seat.clone(),
        event_type,
        Relation::After,
        Arc::new(move |event, _, context| {
            let Some(target) = dynamo_window(context, &owner, event) else {
                return Ok(());
            };
            let before = context.state.clone();
            match crate::production::pay_seeing(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                &owner,
                DYNAMO_COST,
                Spend::Influence,
            ) {
                Ok(true) if repair(context.state, &target) => Ok(()),
                Ok(_) => {
                    *context.state = before;
                    Ok(())
                }
                Err(error) => {
                    *context.state = before;
                    Err(TimingError::IllegalChoice(error))
                }
            }
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        dynamo_window(context, &condition_owner, event).is_some()
    }))
}

// -- Watcher ------------------------------------------------------------------------------------

/// Where one mech stands: a system, and the planet (`None` in the space area).
type Place = (SystemId, Option<PlanetId>);

/// The places `owner`'s mechs stand in a system that contains or is adjacent to a unit of `actor`,
/// in board order.
pub(crate) fn watcher_sites(
    state: &GameState,
    galaxy: Option<&PlayerAdjacency<'_>>,
    owner: &PlayerId,
    actor: &PlayerId,
) -> Vec<Place> {
    let occupied: BTreeSet<&SystemId> = state
        .board
        .iter()
        .filter(|(_, board)| {
            board
                .units
                .iter()
                .chain(board.planet_units.values().flatten())
                .any(|unit| &unit.owner == actor)
        })
        .map(|(system, _)| system)
        .collect();
    let is_mech = |unit: &&Unit| &unit.owner == owner && unit.type_id.as_str() == MECH;
    let mut sites = Vec::new();
    for (system, board) in &state.board {
        if !occupied.iter().any(|other| near(galaxy, system, other)) {
            continue;
        }
        if board.units.iter().any(|unit| is_mech(&unit)) {
            sites.push((system.clone(), None));
        }
        for (planet, units) in &board.planet_units {
            if units.iter().any(|unit| is_mech(&unit)) {
                sites.push((system.clone(), Some(planet.clone())));
            }
        }
    }
    sites
}

/// The id of the option that removes the mech at one place.
fn watcher_id(place: &Place) -> String {
    format!(
        "watcher|{}|{}",
        place.0,
        place
            .1
            .as_ref()
            .map_or_else(|| "space".to_owned(), ToString::to_string)
    )
}

/// The player whose action card `event` announces, when the Empyrean could cancel it.
fn watcher_window(
    context: &TimingContext<'_>,
    owner: &PlayerId,
    event: &Event,
) -> Option<(PlayerId, Vec<Place>)> {
    let actor = PlayerId::new(event.text("player")?);
    if &actor == owner || context.state.player(&actor).is_none() {
        return None;
    }
    let sites = watcher_sites(
        context.state,
        adjacency_of(context, owner).as_ref(),
        owner,
        &actor,
    );
    (!sites.is_empty()).then_some((actor, sites))
}

/// Remove one mech from where it stands. `false` (nothing changed) if it is not there.
fn remove_mech(state: &mut GameState, owner: &PlayerId, place: &Place) -> bool {
    let board = state.system_mut(&place.0);
    let units = match &place.1 {
        Some(planet) => board.planet_units.get_mut(planet),
        None => Some(&mut board.units),
    };
    let Some(units) = units else {
        return false;
    };
    let Some(index) = units
        .iter()
        .position(|unit| &unit.owner == owner && unit.type_id.as_str() == MECH)
    else {
        return false;
    };
    units.remove(index);
    true
}

/// Watcher: the Empyrean removes the mech and the action card is cancelled. The card is still
/// spent (1.15): `reactions::announce` discards it when the event is cancelled. With several
/// mechs in reach the Empyrean picks which one to remove.
fn watcher(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:{MECH}:ACTION_CARD_PLAYED:when"),
        seat.clone(),
        "ACTION_CARD_PLAYED",
        Relation::When,
        Arc::new(move |event, _, context| {
            let Some((actor, sites)) = watcher_window(context, &owner, event) else {
                return Ok(());
            };
            let place = if let [only] = sites.as_slice() {
                only.clone()
            } else {
                let options = sites
                    .iter()
                    .map(|place| {
                        ChoiceOption::labelled(
                            watcher_id(place),
                            "watcher",
                            format!("remove the Watcher in system {}", place.0),
                        )
                        .with("system", place.0.to_string())
                    })
                    .collect();
                let answer = ask(
                    context,
                    &owner,
                    MECH,
                    "watcher_remove",
                    format!("Watcher: which mech cancels {actor}'s action card"),
                    options,
                )?;
                let Some(place) = sites.iter().find(|place| watcher_id(place) == answer.id) else {
                    return Ok(());
                };
                place.clone()
            };
            if remove_mech(context.state, &owner, &place) {
                event.cancel();
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        watcher_window(context, &condition_owner, event).is_some()
    }))
}

// -- Acamar -------------------------------------------------------------------------------------

/// Whether `player` holds the agent readied.
fn agent_ready(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.leaders.get(&LeaderId::new(AGENT)) == Some(&LeaderStatus::Readied))
}

/// The player who moved ships into a system with no planets, for Acamar's window.
fn acamar_window(context: &TimingContext<'_>, owner: &PlayerId, event: &Event) -> Option<PlayerId> {
    if !agent_ready(context.state, owner) || event.integer("ships_moved").is_none_or(|n| n <= 0) {
        return None;
    }
    let mover = PlayerId::new(event.text("player")?);
    context.state.player(&mover)?;
    let system = event.text("system")?;
    ti4_content::galaxy::all_systems(context.content, context.sources)
        .get(system)
        .is_some_and(|system| system.planets().is_empty())
        .then_some(mover)
}

/// Acamar: after a player moves ships into a system with no planets, the Empyrean may exhaust the
/// agent and that player gains 1 command token, into the pool they choose.
fn acamar(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{AGENT}:MOVEMENT_FINISHED:after"),
        seat.clone(),
        "MOVEMENT_FINISHED",
        Relation::After,
        Arc::new(move |event, _, context| {
            let Some(mover) = acamar_window(context, &owner, event) else {
                return Ok(());
            };
            let before = context.state.clone();
            if !crate::leaders::exhaust(context.state, &owner, &LeaderId::new(AGENT)) {
                return Ok(());
            }
            if let Err(error) = crate::strategy_cards::gain_tokens(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                &mover,
                1,
            ) {
                *context.state = before;
                return Err(TimingError::IllegalChoice(error));
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        acamar_window(context, &condition_owner, event).is_some()
    }))
}

// -- Blood Pact and Dark Pact ---------------------------------------------------------------------

/// The ACTION options for the pacts `player` holds in hand: "Place this card faceup in your play
/// area." Ids are `faction|empyrean|<alias>`; wired by the module's `component_actions` hook.
#[must_use]
pub fn component_actions(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    crate::promissory::action_notes_in_hand(state, player)
        .into_iter()
        .filter_map(|note| {
            let alias = crate::promissory::alias_of(&note);
            matches!(alias, BLOOD_PACT | DARK_PACT).then(|| {
                let name = if alias == BLOOD_PACT {
                    "Blood Pact"
                } else {
                    "Dark Pact"
                };
                ChoiceOption::labelled(
                    format!("faction|{FACTION}|{alias}"),
                    "component_action",
                    format!("{name}: place it faceup in your play area"),
                )
            })
        })
        .collect()
}

/// Perform a pact's ACTION; `true` if the option was one of this module's and the card was placed.
pub fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    let Some(alias) = option.id.strip_prefix(&format!("faction|{FACTION}|")) else {
        return false;
    };
    matches!(alias, BLOOD_PACT | DARK_PACT)
        && crate::promissory::play_action_note(
            context.state,
            player,
            &crate::promissory::note_id(alias, FACTION),
        )
}

/// Votes Blood Pact adds to `voter`'s vote for `outcome`, given the votes already recorded in
/// this ballot (`votes`: who voted for what).
///
/// "When you and the Empyrean player cast votes for the same outcome, cast 4 additional votes for
/// that outcome." The pair completes with whichever of the two votes second, so the 4 are banked
/// exactly once: with the holder's vote when the Empyrean voted first, with the Empyrean's when
/// the holder did (they are the holder's votes either way: they count for that outcome).
/// Only a faceup note counts; one still in hand has not been played.
#[must_use]
pub fn blood_pact_votes(
    state: &GameState,
    votes: &BTreeMap<PlayerId, String>,
    voter: &PlayerId,
    outcome: &str,
) -> i64 {
    state
        .promissory_notes
        .iter()
        .filter(|(note, _)| {
            crate::promissory::alias_of(note) == BLOOD_PACT
                && state.promissory_faceup.contains(*note)
        })
        .filter_map(|(note, holder)| {
            let empyrean = crate::promissory::owner_of(note)
                .and_then(|name| crate::promissory::seat_of(state, &name))?;
            if &empyrean == holder {
                return None;
            }
            let other = if voter == holder {
                &empyrean
            } else if voter == &empyrean {
                holder
            } else {
                return None;
            };
            (votes.get(other).map(String::as_str) == Some(outcome)).then_some(BLOOD_PACT_VOTES)
        })
        .sum()
}

/// A faction's printed commodity value (21.1).
fn commodity_value(state: &GameState, content: &ContentStore, player: &PlayerId) -> i32 {
    state.player(player).map_or(0, |seat| {
        ti4_content::factions::get(content, seat.faction.as_str())
            .map_or(0, |faction| faction.commodities())
    })
}

/// Dark Pact, after a transaction resolved: each side that gave the Empyrean exactly its maximum
/// commodity value while holding a faceup Dark Pact gains 1 trade good, and so does the Empyrean.
///
/// "Equal to your maximum commodity value" is the printed value of the giver's faction (21.1); a
/// smaller or larger gift does not trigger it.
pub fn dark_pact_gains(
    state: &mut GameState,
    content: &ContentStore,
    offer: &crate::transactions::Offer,
) {
    let sides = [
        (&offer.proposer, &offer.partner, &offer.given),
        (&offer.partner, &offer.proposer, &offer.received),
    ];
    let mut gains: Vec<(PlayerId, PlayerId)> = Vec::new();
    for (giver, receiver, terms) in sides {
        let note = crate::promissory::note_id(
            DARK_PACT,
            &crate::promissory::faction_name(state, receiver),
        );
        if terms.commodities > 0
            && giver != receiver
            && state.promissory_notes.get(&note) == Some(giver)
            && state.promissory_faceup.contains(&note)
            && terms.commodities == commodity_value(state, content, giver)
        {
            gains.push((giver.clone(), receiver.clone()));
        }
    }
    for (giver, receiver) in gains {
        crate::supply::gain_trade_goods_staged(state, &giver, 1, DARK_PACT);
        crate::supply::gain_trade_goods_staged(state, &receiver, 1, DARK_PACT);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc as Shared, Mutex};

    use super::*;
    use ti4_content::galaxy::Galaxy;
    use crate::choice::{Decider, IllegalChoice, Scripted, Table};
    use crate::event::EventSequence;
    use serde_json::Value;
    use ti4_model::content_types::DEFAULT;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", "empyrean"), ("b", "sol")], DEFAULT)
    }
    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
    }
    fn payload(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    type Asked = Shared<Mutex<Vec<(String, Vec<String>)>>>;

    /// Records every question (who, option ids), then answers from a script.
    struct Recording(Scripted, Asked);

    impl Decider for Recording {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            self.1.lock().unwrap().push((
                choice.player.to_string(),
                choice.options.iter().map(|o| o.id.clone()).collect(),
            ));
            self.0.choose(choice)
        }
    }

    fn recording(answers: &[&str]) -> (Table, Asked) {
        let asked = Asked::default();
        let table = Table::with_default(Box::new(Recording(
            Scripted::new(answers.iter().copied()),
            Shared::clone(&asked),
        )));
        (table, asked)
    }

    /// Emit one typed event through a resolver armed as the game arms one. Returns whether the
    /// event was cancelled.
    fn emit(
        state: &mut GameState,
        galaxy: Option<&Galaxy>,
        table: &mut Table,
        kind: &str,
        pairs: &[(&str, Value)],
    ) -> bool {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, galaxy, table, |ctx| {
            let event = EventSequence::new().next(kind, payload(pairs)).unwrap();
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("emits")
                .cancelled
        })
    }

    fn offered(asked: &Asked, wanted: &str) -> bool {
        asked
            .lock()
            .unwrap()
            .iter()
            .any(|(_, options)| options.iter().any(|id| id.contains(wanted)))
    }

    fn damaged(state: &GameState, system: &SystemId, kind: &str) -> usize {
        state
            .system_state(system)
            .units
            .iter()
            .filter(|unit| unit.type_id.as_str() == kind && unit.sustained_damage)
            .count()
    }

    fn influence(state: &GameState, player: &PlayerId) -> i64 {
        crate::production::available(
            state,
            ContentStore::embedded(),
            DEFAULT,
            player,
            Spend::Influence,
        )
    }

    // -- claims and neutrality ------------------------------------------------------------------

    #[test]
    fn the_claims_are_the_real_assets_of_the_faction() {
        let content = ContentStore::embedded();
        let sheet = super::super::assets(content, DEFAULT, FACTION);
        for id in UNITS {
            assert!(sheet.iter().any(|a| a.id == *id), "{id}");
        }
        for id in PROMISSORY.iter().chain(LEADERS) {
            assert!(sheet.iter().any(|a| a.id == *id), "{id}");
        }
    }

    #[test]
    fn games_without_the_empyrean_see_nothing_of_the_units_agent_or_pacts() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let before = state.clone();
        let (mut table, asked) = recording(&[]);
        for (kind, pairs) in [
            (
                "SUSTAIN_DAMAGE_USED",
                vec![
                    ("system", Value::from("18")),
                    ("player", "b".into()),
                    ("unit", "dreadnought".into()),
                ],
            ),
            (
                "GROUND_FORCE_SUSTAINED",
                vec![
                    ("system", Value::from("18")),
                    ("planet", "mecatol".into()),
                    ("player", "b".into()),
                    ("unit", "mech".into()),
                ],
            ),
            (
                "ACTION_CARD_PLAYED",
                vec![("player", Value::from("b")), ("card", "sabo1".into())],
            ),
            (
                "MOVEMENT_FINISHED",
                vec![
                    ("player", Value::from("a")),
                    ("system", "18".into()),
                    ("ships_moved", 1.into()),
                ],
            ),
        ] {
            let cancelled = emit(&mut state, None, &mut table, kind, &pairs);
            assert!(!cancelled, "{kind}");
        }
        assert!(asked.lock().unwrap().is_empty(), "nothing offered");
        assert_eq!(state, before, "nothing changed");
        // No pact to place, none to vote with, none to give for.
        assert!(component_actions(&state, ContentStore::embedded(), &a()).is_empty());
        assert_eq!(
            blood_pact_votes(&state, &BTreeMap::from([(b(), "for".into())]), &a(), "for"),
            0
        );
    }

    // -- Dynamo ---------------------------------------------------------------------------------

    const DYNAMO_SPACE: &str = "unit:empyrean:empyrean_flagship:SUSTAIN_DAMAGE_USED:after";
    const DYNAMO_GROUND: &str = "unit:empyrean:empyrean_flagship:GROUND_FORCE_SUSTAINED:after";

    /// The Empyrean's flagship at the hub's centre, and a damaged Sol dreadnought on its ring.
    fn dynamo_game() -> (crate::fixtures::Hub, GameState, SystemId, SystemId) {
        let hub = crate::fixtures::plain_hub();
        let mut state = game();
        let centre = SystemId::new(&hub.centre);
        let ring = SystemId::new(&hub.outer[0]);
        crate::fixtures::put(&mut state, &centre, FLAGSHIP, &a(), 1);
        state
            .system_mut(&ring)
            .units
            .push(Unit::new(UnitTypeId::new("dreadnought"), b()).sustained());
        (hub, state, centre, ring)
    }

    fn sustained_space(system: &SystemId, who: &str, kind: &str) -> Vec<(&'static str, Value)> {
        vec![
            ("system", Value::from(system.to_string())),
            ("player", who.into()),
            ("unit", kind.into()),
        ]
    }

    #[test]
    fn dynamo_repairs_an_adjacent_units_sustain_for_two_influence() {
        let (hub, mut state, _, ring) = dynamo_game();
        let before = influence(&state, &a());
        assert!(before >= 2, "the home planets pay for it");
        assert_eq!(damaged(&state, &ring, "dreadnought"), 1);
        emit(
            &mut state,
            Some(&hub.galaxy),
            &mut scripted(&[DYNAMO_SPACE]),
            "SUSTAIN_DAMAGE_USED",
            &sustained_space(&ring, "b", "dreadnought"),
        );
        assert_eq!(damaged(&state, &ring, "dreadnought"), 0, "repaired");
        assert!(
            influence(&state, &a()) <= before - DYNAMO_COST,
            "planets were exhausted to pay"
        );
    }

    #[test]
    fn dynamo_repairs_a_unit_in_its_own_system_too() {
        let (hub, mut state, centre, _) = dynamo_game();
        state
            .system_mut(&centre)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), b()).sustained());
        emit(
            &mut state,
            Some(&hub.galaxy),
            &mut scripted(&[DYNAMO_SPACE]),
            "SUSTAIN_DAMAGE_USED",
            &sustained_space(&centre, "b", "cruiser"),
        );
        assert_eq!(damaged(&state, &centre, "cruiser"), 0);
    }

    #[test]
    fn dynamo_repairs_a_sustained_ground_force() {
        let (hub, mut state, _, ring) = dynamo_game();
        let planet = PlanetId::new(
            ti4_content::galaxy::all_systems(ContentStore::embedded(), DEFAULT)[ring.as_str()]
                .planets()[0],
        );
        state
            .system_mut(&ring)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(Unit::new(UnitTypeId::new("mech"), b()).sustained());
        emit(
            &mut state,
            Some(&hub.galaxy),
            &mut scripted(&[DYNAMO_GROUND]),
            "GROUND_FORCE_SUSTAINED",
            &[
                ("system", ring.to_string().into()),
                ("planet", planet.to_string().into()),
                ("player", "b".into()),
                ("unit", "mech".into()),
                ("cause", "ground_combat".into()),
            ],
        );
        let left = state
            .system_state(&ring)
            .on_planet(&planet)
            .iter()
            .filter(|unit| unit.sustained_damage)
            .count();
        assert_eq!(left, 0, "the mech is repaired");
    }

    #[test]
    fn dynamo_is_not_offered_out_of_reach_unpaid_or_for_an_undamaged_unit() {
        let (hub, state, _, ring) = dynamo_game();
        let offers = |state: &GameState, galaxy: Option<&Galaxy>, system: &SystemId| {
            let mut state = state.clone();
            let (mut table, asked) = recording(&[DYNAMO_SPACE]);
            emit(
                &mut state,
                galaxy,
                &mut table,
                "SUSTAIN_DAMAGE_USED",
                &sustained_space(system, "b", "dreadnought"),
            );
            offered(&asked, DYNAMO_SPACE)
        };
        assert!(offers(&state, Some(&hub.galaxy), &ring), "baseline");
        // Two systems away: the flagship on the far side of the ring, across the centre.
        let mut far = state.clone();
        let centre = SystemId::new(&hub.centre);
        far.system_mut(&centre).units.retain(|u| u.owner != a());
        let across = SystemId::new(hub.across(&hub.outer[0]));
        crate::fixtures::put(&mut far, &across, FLAGSHIP, &a(), 1);
        assert!(!offers(&far, Some(&hub.galaxy), &ring), "two systems away");
        // No map: only the same system counts.
        assert!(!offers(&state, None, &ring), "no map, not the same system");
        // Nobody can pay.
        let mut broke = state.clone();
        for planet in broke
            .controlled_planets(&a())
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect::<Vec<_>>()
        {
            broke.exhaust_planet(planet);
        }
        assert!(!offers(&broke, Some(&hub.galaxy), &ring), "cannot pay");
        // The unit is not damaged.
        let mut whole = state.clone();
        whole.system_mut(&ring).units = vec![Unit::new(UnitTypeId::new("dreadnought"), b())];
        assert!(
            !offers(&whole, Some(&hub.galaxy), &ring),
            "nothing to repair"
        );
    }

    #[test]
    fn dynamo_declined_changes_nothing() {
        let (hub, mut state, _, ring) = dynamo_game();
        let before = state.clone();
        emit(
            &mut state,
            Some(&hub.galaxy),
            &mut scripted(&["decline"]),
            "SUSTAIN_DAMAGE_USED",
            &sustained_space(&ring, "b", "dreadnought"),
        );
        assert_eq!(state, before);
    }

    // -- Watcher --------------------------------------------------------------------------------

    const WATCHER: &str = "unit:empyrean:empyrean_mech:ACTION_CARD_PLAYED:when";

    /// The Empyrean's mech on the hub's centre, and a Sol cruiser on the ring next to it.
    fn watcher_game() -> (crate::fixtures::Hub, GameState, SystemId, SystemId) {
        let hub = crate::fixtures::plain_hub();
        let mut state = game();
        let centre = SystemId::new(&hub.centre);
        let ring = SystemId::new(&hub.outer[0]);
        crate::fixtures::put(&mut state, &centre, MECH, &a(), 1);
        crate::fixtures::put(&mut state, &ring, "cruiser", &b(), 1);
        (hub, state, centre, ring)
    }

    fn mechs(state: &GameState, system: &SystemId) -> usize {
        state
            .system_state(system)
            .units
            .iter()
            .filter(|unit| unit.type_id.as_str() == MECH)
            .count()
    }

    #[test]
    fn the_watcher_cancels_an_adjacent_players_action_card_and_leaves_the_board() {
        let (hub, mut state, centre, _) = watcher_game();
        let cancelled = emit(
            &mut state,
            Some(&hub.galaxy),
            &mut scripted(&[WATCHER]),
            "ACTION_CARD_PLAYED",
            &[("player", "b".into()), ("card", "sabo1".into())],
        );
        assert!(cancelled);
        assert_eq!(mechs(&state, &centre), 0, "removed from the system");
    }

    #[test]
    fn the_watcher_works_from_a_planet_and_from_the_same_system() {
        let mut state = game();
        let (centre, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &centre, &planet, MECH, &a(), 1);
        crate::fixtures::put(&mut state, &centre, "cruiser", &b(), 1);
        let cancelled = emit(
            &mut state,
            None,
            &mut scripted(&[WATCHER]),
            "ACTION_CARD_PLAYED",
            &[("player", "b".into()), ("card", "sabo1".into())],
        );
        assert!(cancelled);
        assert!(state.system_state(&centre).on_planet(&planet).is_empty());
    }

    #[test]
    fn the_watcher_is_not_offered_out_of_reach_to_the_owner_or_declined() {
        let (hub, state, centre, ring) = watcher_game();
        let offers = |state: &GameState, who: &str| {
            let mut state = state.clone();
            let (mut table, asked) = recording(&[WATCHER]);
            emit(
                &mut state,
                Some(&hub.galaxy),
                &mut table,
                "ACTION_CARD_PLAYED",
                &[("player", who.into()), ("card", "sabo1".into())],
            );
            offered(&asked, WATCHER)
        };
        assert!(offers(&state, "b"), "baseline");
        assert!(!offers(&state, "a"), "never the Empyrean's own card");
        // The other player's units are two systems from the mech.
        let mut far = state.clone();
        far.system_mut(&ring).units.clear();
        let across = SystemId::new(hub.across(&hub.outer[0]));
        crate::fixtures::put(&mut far, &across, "cruiser", &b(), 1);
        far.system_mut(&centre).units.clear();
        for board in far.board.values_mut() {
            board.units.retain(|unit| unit.owner != b());
            for units in board.planet_units.values_mut() {
                units.retain(|unit| unit.owner != b());
            }
        }
        crate::fixtures::put(&mut far, &across, "cruiser", &b(), 1);
        crate::fixtures::put(&mut far, &ring, MECH, &a(), 1);
        assert!(!offers(&far, "b"), "two systems from every unit of theirs");
        // Declining keeps the card and the mech.
        let mut kept = state.clone();
        let cancelled = emit(
            &mut kept,
            Some(&hub.galaxy),
            &mut scripted(&["decline"]),
            "ACTION_CARD_PLAYED",
            &[("player", "b".into()), ("card", "sabo1".into())],
        );
        assert!(!cancelled);
        assert_eq!(kept, state);
        // No mech, no window.
        let mut none = state.clone();
        none.system_mut(&centre).units.clear();
        assert!(!offers(&none, "b"));
    }

    #[test]
    fn the_watcher_asks_which_mech_when_several_are_in_reach() {
        let (hub, mut state, centre, ring) = watcher_game();
        crate::fixtures::put(&mut state, &ring, MECH, &a(), 1);
        let second = watcher_id(&(ring.clone(), None));
        let (mut table, asked) = recording(&[WATCHER, &second]);
        let cancelled = emit(
            &mut state,
            Some(&hub.galaxy),
            &mut table,
            "ACTION_CARD_PLAYED",
            &[("player", "b".into()), ("card", "sabo1".into())],
        );
        assert!(cancelled);
        assert_eq!(mechs(&state, &ring), 0, "the one it picked");
        assert_eq!(mechs(&state, &centre), 1, "the other stays");
        assert!(offered(&asked, "watcher|"));
    }

    // -- Acamar ---------------------------------------------------------------------------------

    const ACAMAR: &str = "leader:empyrean:empyreanagent:MOVEMENT_FINISHED:after";

    fn planetless() -> SystemId {
        let systems = ti4_content::galaxy::all_systems(ContentStore::embedded(), DEFAULT);
        SystemId::new(
            systems
                .iter()
                .find(|(_, system)| system.planets().is_empty() && !system.is_hyperlane())
                .map(|(id, _)| *id)
                .expect("a planetless system"),
        )
    }

    fn movement_finished(system: &SystemId, who: &str, moved: i64) -> Vec<(&'static str, Value)> {
        vec![
            ("player", who.into()),
            ("system", system.to_string().into()),
            ("ships_moved", moved.into()),
        ]
    }

    fn tokens(state: &GameState, who: &PlayerId) -> (i32, i32, i32) {
        let seat = state.player(who).unwrap();
        (seat.tactic_tokens, seat.fleet_tokens, seat.strategic_tokens)
    }

    #[test]
    fn acamar_gives_the_mover_a_command_token_of_their_choosing() {
        let mut state = game();
        let system = planetless();
        let before = tokens(&state, &b());
        let (mut table, asked) = recording(&[ACAMAR, "fleet_tokens"]);
        emit(
            &mut state,
            None,
            &mut table,
            "MOVEMENT_FINISHED",
            &movement_finished(&system, "b", 2),
        );
        let after = tokens(&state, &b());
        assert_eq!(
            after,
            (before.0, before.1 + 1, before.2),
            "the mover chose the fleet pool"
        );
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new(AGENT)),
            Some(&LeaderStatus::Exhausted)
        );
        // The mover, not the Empyrean, was asked for the pool.
        let asked = asked.lock().unwrap();
        assert!(
            asked
                .iter()
                .any(|(who, options)| who == "b" && options.contains(&"fleet_tokens".to_owned()))
        );
    }

    #[test]
    fn acamar_also_serves_the_empyrean_itself() {
        let mut state = game();
        let before = tokens(&state, &a());
        emit(
            &mut state,
            None,
            &mut scripted(&[ACAMAR, "tactic_tokens"]),
            "MOVEMENT_FINISHED",
            &movement_finished(&planetless(), "a", 1),
        );
        assert_eq!(tokens(&state, &a()).0, before.0 + 1);
    }

    #[test]
    fn acamar_is_not_offered_for_a_system_with_planets_no_moved_ships_or_a_spent_card() {
        let state = game();
        let with_planets = SystemId::new(
            state
                .player(&b())
                .unwrap()
                .home_system
                .clone()
                .unwrap()
                .to_string(),
        );
        let offers = |state: &GameState, system: &SystemId, moved: i64| {
            let mut state = state.clone();
            let (mut table, asked) = recording(&[ACAMAR]);
            emit(
                &mut state,
                None,
                &mut table,
                "MOVEMENT_FINISHED",
                &movement_finished(system, "b", moved),
            );
            offered(&asked, ACAMAR)
        };
        assert!(offers(&state, &planetless(), 1), "baseline");
        assert!(!offers(&state, &with_planets, 1), "it has planets");
        assert!(!offers(&state, &planetless(), 0), "no ship moved");
        let mut spent = state.clone();
        assert!(crate::leaders::exhaust(
            &mut spent,
            &a(),
            &LeaderId::new(AGENT)
        ));
        assert!(!offers(&spent, &planetless(), 1), "exhausted");
        // Declined: nothing happens.
        let mut kept = state.clone();
        emit(
            &mut kept,
            None,
            &mut scripted(&["decline"]),
            "MOVEMENT_FINISHED",
            &movement_finished(&planetless(), "b", 1),
        );
        assert_eq!(kept, state);
    }

    // -- Blood Pact and Dark Pact: ACTION, return --------------------------------------------------

    fn blood() -> String {
        crate::promissory::note_id(BLOOD_PACT, FACTION)
    }
    fn dark() -> String {
        crate::promissory::note_id(DARK_PACT, FACTION)
    }

    #[test]
    fn a_pact_waits_in_hand_until_its_action_places_it_faceup() {
        let content = ContentStore::embedded();
        let mut state = game();
        crate::promissory::take(&mut state, content, &b(), &blood());
        crate::promissory::take(&mut state, content, &b(), &dark());
        assert!(
            !state.promissory_faceup.contains(&blood()),
            "not on receipt"
        );
        assert!(
            component_actions(&state, content, &a()).is_empty(),
            "not the owner's"
        );
        let options = component_actions(&state, content, &b());
        let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(
            ids,
            ["faction|empyrean|blood_pact", "faction|empyrean|dark_pact"]
        );
        let mut table = scripted(&[]);
        crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
            assert!(perform_component(ctx, &b(), &options[0]));
            assert!(!perform_component(ctx, &b(), &options[0]), "once");
            assert!(!perform_component(
                ctx,
                &a(),
                &ChoiceOption::labelled("faction|empyrean|dark_pact", "x", "x")
            ));
            assert!(!perform_component(
                ctx,
                &b(),
                &ChoiceOption::labelled("faction|titans|x", "x", "x")
            ));
        });
        assert!(state.promissory_faceup.contains(&blood()));
        assert!(
            !state.promissory_faceup.contains(&dark()),
            "the other is still in hand"
        );
        assert_eq!(component_actions(&state, content, &b()).len(), 1);
    }

    #[test]
    fn a_pact_goes_home_when_its_holder_activates_a_system_with_empyrean_units() {
        let content = ContentStore::embedded();
        let mut state = game();
        for note in [blood(), dark()] {
            crate::promissory::take(&mut state, content, &b(), &note);
            assert!(crate::promissory::play_action_note(&mut state, &b(), &note));
        }
        let elsewhere = planetless();
        assert!(
            crate::promissory::spend_support_on_activation(&mut state, &b(), &elsewhere).is_empty()
        );
        assert_eq!(
            state.promissory_notes.get(&blood()),
            Some(&b()),
            "no Empyrean unit there"
        );
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        crate::promissory::spend_support_on_activation(&mut state, &b(), &home);
        for note in [blood(), dark()] {
            assert_eq!(
                state.promissory_notes.get(&note),
                Some(&a()),
                "{note} returned"
            );
            assert!(!state.promissory_faceup.contains(&note));
        }
        // A pact still in hand is not in play, so activating does not return it.
        crate::promissory::take(&mut state, content, &b(), &blood());
        crate::promissory::spend_support_on_activation(&mut state, &b(), &home);
        assert_eq!(state.promissory_notes.get(&blood()), Some(&b()));
    }

    // -- Blood Pact: votes --------------------------------------------------------------------------

    /// Seats `a`, `b`, `c` with the Empyrean at `empyrean` and Sol at `holder`; the holder has
    /// Blood Pact, faceup when `placed`.
    fn voting_game(empyrean: &str, holder: &str, pact: bool, placed: bool) -> GameState {
        let content = ContentStore::embedded();
        let faction = |name: &str| {
            if name == empyrean {
                "empyrean"
            } else if name == holder {
                "sol"
            } else {
                "hacan"
            }
        };
        let seats = [
            ("a", faction("a")),
            ("b", faction("b")),
            ("c", faction("c")),
        ];
        let mut state = crate::fixtures::seated_game(&seats, DEFAULT);
        if pact {
            let holder = PlayerId::new(holder);
            crate::promissory::take(&mut state, content, &holder, &blood());
            if placed {
                assert!(crate::promissory::play_action_note(
                    &mut state,
                    &holder,
                    &blood()
                ));
            }
        }
        state
    }

    /// Run one vote where each `(player, outcome)` casts every readied planet's influence, and
    /// return the ballot's counts.
    fn vote(state: &mut GameState, casts: &[(&str, &str)]) -> BTreeMap<String, i64> {
        let content = ContentStore::embedded();
        state.speaker = PlayerId::new(casts.last().unwrap().0);
        let mut window = crate::vote::VoteWindow::new(
            state,
            "for_against_dummy",
            vec!["for".to_owned(), "against".to_owned()],
        );
        window.open(state, content, DEFAULT);
        let order: Vec<PlayerId> = window.order().to_vec();
        for who in order {
            let wanted = casts.iter().find(|(p, _)| *p == who.as_str());
            let Some((_, outcome)) = wanted else {
                let decline = window
                    .pending_choice(state, content, DEFAULT)
                    .and_then(|choice| choice.option("decline").cloned())
                    .unwrap();
                window.resolve(state, content, DEFAULT, decline).unwrap();
                continue;
            };
            let choice = window.pending_choice(state, content, DEFAULT).unwrap();
            window
                .resolve(
                    state,
                    content,
                    DEFAULT,
                    choice.option(outcome).cloned().unwrap(),
                )
                .unwrap();
            while let Some(choice) = window.pending_choice(state, content, DEFAULT) {
                if choice.player != who {
                    break;
                }
                let next = choice
                    .options
                    .iter()
                    .find(|o| !o.is_decline())
                    .cloned()
                    .unwrap_or_else(|| choice.option("decline").cloned().unwrap());
                window.resolve(state, content, DEFAULT, next).unwrap();
            }
        }
        window.ballot().counts.clone()
    }

    #[test]
    fn blood_pact_adds_four_votes_when_the_holder_and_the_empyrean_vote_alike() {
        // Whoever votes second completes the pair; either order gives the same total. The seats
        // vote a, b, c (the speaker, c, last): the Empyrean at a votes first, at b second.
        let casts = [("a", "for"), ("b", "for"), ("c", "against")];
        for (empyrean, holder) in [("a", "b"), ("b", "a")] {
            let baseline = vote(&mut voting_game(empyrean, holder, false, false), &casts);
            let counts = vote(&mut voting_game(empyrean, holder, true, true), &casts);
            assert_eq!(
                counts["for"],
                baseline["for"] + 4,
                "{empyrean} then {holder}"
            );
            assert_eq!(counts["against"], baseline["against"], "no one else's");
        }
    }

    #[test]
    fn blood_pact_adds_nothing_unless_both_vote_the_same_outcome_with_the_note_faceup() {
        let same = [("a", "for"), ("b", "for"), ("c", "against")];
        let split = [("a", "for"), ("b", "against"), ("c", "against")];
        let alone = [("b", "for"), ("c", "against")];
        let plain = |casts: &[(&str, &str)]| vote(&mut voting_game("a", "b", false, false), casts);
        assert_eq!(
            vote(&mut voting_game("a", "b", true, true), &split),
            plain(&split),
            "different outcomes"
        );
        assert_eq!(
            vote(&mut voting_game("a", "b", true, true), &alone),
            plain(&alone),
            "the Empyrean did not vote"
        );
        assert_eq!(
            vote(&mut voting_game("a", "b", true, false), &same),
            plain(&same),
            "still in hand, not faceup"
        );
    }

    // -- Dark Pact: transactions ---------------------------------------------------------------------

    fn dark_pact_world(
        commodities: i32,
    ) -> (crate::fixtures::Hub, GameState, crate::transactions::Offer) {
        let content = ContentStore::embedded();
        let hub = crate::fixtures::plain_hub();
        let mut state = game();
        let value = commodity_value(&state, content, &b());
        assert!(value > 0);
        // Next to each other so they are neighbours.
        crate::fixtures::put(&mut state, &SystemId::new(&hub.centre), "cruiser", &a(), 1);
        crate::fixtures::put(
            &mut state,
            &SystemId::new(&hub.outer[0]),
            "cruiser",
            &b(),
            1,
        );
        state.player_mut(&b()).unwrap().commodities = value;
        crate::promissory::take(&mut state, content, &b(), &dark());
        assert!(crate::promissory::play_action_note(
            &mut state,
            &b(),
            &dark()
        ));
        let offer = crate::transactions::Offer {
            proposer: b(),
            partner: a(),
            given: crate::transactions::Terms {
                commodities,
                ..Default::default()
            },
            received: crate::transactions::Terms::default(),
        };
        (hub, state, offer)
    }

    #[test]
    fn dark_pact_gives_each_a_trade_good_for_a_full_commodity_gift() {
        let content = ContentStore::embedded();
        let value = commodity_value(&game(), content, &b());
        let (hub, mut state, offer) = dark_pact_world(value);
        let (ga, gb) = (
            state.player(&a()).unwrap().trade_goods,
            state.player(&b()).unwrap().trade_goods,
        );
        crate::transactions::resolve(&mut state, content, &hub.galaxy, &offer).unwrap();
        assert_eq!(
            state.player(&a()).unwrap().trade_goods,
            ga + value + 1,
            "the gift, and 1"
        );
        assert_eq!(state.player(&b()).unwrap().trade_goods, gb + 1);
    }

    #[test]
    fn dark_pact_needs_exactly_the_maximum_value_and_a_faceup_note() {
        let content = ContentStore::embedded();
        let value = commodity_value(&game(), content, &b());
        // One short of the maximum is not "equal to" it.
        let (hub, mut state, offer) = dark_pact_world(value - 1);
        let before = state.player(&b()).unwrap().trade_goods;
        crate::transactions::resolve(&mut state, content, &hub.galaxy, &offer).unwrap();
        assert_eq!(state.player(&b()).unwrap().trade_goods, before);
        // Held but not placed.
        let (hub, mut state, offer) = dark_pact_world(value);
        state.promissory_faceup.remove(&dark());
        let before = state.player(&b()).unwrap().trade_goods;
        crate::transactions::resolve(&mut state, content, &hub.galaxy, &offer).unwrap();
        assert_eq!(
            state.player(&b()).unwrap().trade_goods,
            before,
            "still in hand"
        );
        // The Empyrean giving the holder its own maximum is not "you give".
        let (hub, mut state, mut offer) = dark_pact_world(value);
        offer.given = crate::transactions::Terms::default();
        offer.received = crate::transactions::Terms {
            commodities: value,
            ..Default::default()
        };
        state.player_mut(&a()).unwrap().commodities = value;
        let before = state.player(&a()).unwrap().trade_goods;
        crate::transactions::resolve(&mut state, content, &hub.galaxy, &offer).unwrap();
        assert_eq!(
            state.player(&a()).unwrap().trade_goods,
            before,
            "the Empyrean gave: no bonus"
        );
    }

    #[test]
    fn a_nekro_flagship_with_the_empyrean_z_token_repairs_for_two_influence() {
        let hub = crate::fixtures::plain_hub();
        let centre = SystemId::new(&hub.centre);
        let ring = SystemId::new(&hub.outer[0]);
        let run = |lent: &[&str], script: &[&str]| {
            let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], lent);
            state.player_mut(&a()).unwrap().trade_goods = 5;
            crate::fixtures::put(&mut state, &centre, "nekro_flagship", &a(), 1);
            state
                .system_mut(&ring)
                .units
                .push(Unit::new(UnitTypeId::new("dreadnought"), b()).sustained());
            emit(
                &mut state,
                Some(&hub.galaxy),
                &mut scripted(script),
                "SUSTAIN_DAMAGE_USED",
                &sustained_space(&ring, "b", "dreadnought"),
            );
            damaged(&state, &ring, "dreadnought")
        };
        assert_eq!(run(&[], &[]), 1, "off by default: nothing is offered");
        assert_eq!(
            run(
                &["empyrean"],
                &["unit:nekro:empyrean_flagship:SUSTAIN_DAMAGE_USED:after"]
            ),
            0
        );
    }
}
