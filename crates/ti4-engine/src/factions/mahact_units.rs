//! Mahact flagship (Arvicon Rex), Crimson Legionnaire I/II, Starlancer mech and the hero (Airo Shir
//! Aur). Everything else Mahact is in `mahact.rs`; the foreign tokens in the Mahact fleet pool are
//! read through `mahact::fleet_pool_owners`. Record: `plans/evidence/BF-mahact-units.md`.
//!
//! Card text (content corpus, latest printing):
//!
//! * Arvicon Rex, flagship: "During combat against an opponent whose command token is not in your
//!   fleet pool, apply +2 to the results of this unit's combat rolls."
//! * Crimson Legionnaire I: "After this unit is destroyed, gain 1 commodity or convert 1 of your
//!   commodities to a trade good."
//! * Crimson Legionnaire II (`cl2`): "After this unit is destroyed, gain 1 commodity or convert 1 of
//!   your commodities to a trade good. Then, place the unit on this card. At the start of your next
//!   turn, place each unit that is on this card on a planet you control in your home system."
//! * Starlancer, mech: "After a player whose command token is in your fleet pool activates this
//!   system, you may spend their token from your fleet pool to end their turn; they gain that token."
//! * Airo Shir Aur, hero (`mahacthero`): "ACTION: Move all units in the space area of any system to an
//!   adjacent system that contains a different player's ships. Space combat is resolved in that
//!   system. Neither player can retreat or resolve abilities that would move their ships. Then, purge
//!   this card."
//!
//! Routes:
//!
//! * Arvicon Rex is a roll bonus: `combat::effective_from` adds [`flagship_roll_bonus`] to the roll,
//!   which lowers the threshold. The opponent is the other side of the combat in the active system.
//! * The Legionnaires hang on `GROUND_FORCE_DESTROYED` (invasion, action cards, agendas). CL II's
//!   card is the faction mark [`card_key`] (a count); infantry are uncapped in `supply::plastic`, so a
//!   unit on the card is simply not on the board, which is what keeps it out of the reinforcements
//!   in every other reader. It comes back on the owner's `TURN_BEGAN`.
//! * The Starlancer is a window the game drives right after `SYSTEM_ACTIVATED` (where the Xxcha
//!   Nullification Field and the Mahact commander end a turn): [`offer_starlancer`], called from
//!   `Game` which then ends the turn as it does for them.
//! * The hero is delivered through [`leader_action`] / [`use_leader_timed`]; the move and the combat
//!   run inside the game's timing resolver (`combat::resolve_resolving`) so the combat opens its
//!   usual windows. Retreat, and Skilled Retreat, are barred by [`ship_movement_barred`].

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus};

use super::hooks_combat::CombatHooks;
use crate::action_cards::{PlacementTarget, place_units_counted, placement_spots};
use crate::choice::{
    Choice, ChoiceOption, IllegalChoice, Observed, Resolving, Table, TimingHandle,
};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::timing::{Ability, Relation, Resolver, TimingContext, TimingError};

const FLAGSHIP: &str = "mahact_flagship";
const INFANTRY: &str = "mahact_infantry";
const INFANTRY2: &str = "mahact_infantry2";
const MECH: &str = "mahact_mech";
const HERO: &str = "mahacthero";
/// Roll bonus of Arvicon Rex.
const FLAGSHIP_BONUS: i64 = 2;
/// Faction mark set for the length of the hero's combat; its value is the system.
const BENEDICTION_MARK: &str = "mahact:benediction";
/// Faction mark prefix: Crimson Legionnaire II units on the card (a count, per player).
const CARD_PREFIX: &str = "mahact:cl2:";

/// Leaders claimed: the hero (here), the agent and the commander (`mahact.rs`).
pub const LEADERS: &[&str] = &[HERO, "mahactagent", "mahactcommander"];
/// Units claimed: Arvicon Rex, both Crimson Legionnaires and the Starlancer (`Game` calls
/// [`offer_starlancer`] after an activation).
pub const UNITS: &[&str] = &[FLAGSHIP, INFANTRY, INFANTRY2, MECH];
/// Combat hooks. None: every effect here is a roll bonus read in `combat.rs` or a timing ability.
pub const COMBAT_HOOKS: CombatHooks = CombatHooks::NONE;

/// Timing abilities of the units and hero, for one seat.
///
/// Registered for every seat; each condition is false unless the seat owns the destroyed unit or has
/// units waiting on the card, so a game without Mahact is unchanged.
pub(crate) fn timing_abilities(
    _state: &GameState,
    owner_name: &str,
    seat: &PlayerId,
) -> Vec<Ability> {
    vec![
        legionnaire(owner_name, seat, INFANTRY),
        legionnaire(owner_name, seat, INFANTRY2),
        legionnaire_returns(owner_name, seat),
    ]
}

fn decision(state: &GameState, who: &PlayerId, source: &str, subtype: &str) -> DecisionContext {
    DecisionContext::new(
        who.clone(),
        DecisionSource::FactionAbility(source.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

fn illegal(error: IllegalChoice) -> TimingError {
    TimingError::IllegalChoice(error)
}

// -- Arvicon Rex ---------------------------------------------------------------------------------

/// The roll bonus Arvicon Rex gives when `player`'s `unit_type` rolls in the active system: +2 for
/// the flagship when every opponent in the combat has no command token in the flagship owner's
/// fleet pool (a neutral opponent has none). Zero for everything else, so games without Mahact are
/// unchanged. Read by `combat::effective_from`.
#[must_use]
pub(crate) fn flagship_roll_bonus(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    unit_type: &str,
) -> i64 {
    if !super::flagship_has_text(state, player, unit_type, FLAGSHIP) {
        return 0;
    }
    let Some(system) = state.active_system.as_ref() else {
        return 0;
    };
    let pool = super::mahact::fleet_pool_owners(state, player);
    let opponents: Vec<PlayerId> = crate::combat::combatants(state, content, sources, system)
        .into_iter()
        .filter(|side| side != player)
        .collect();
    if opponents.is_empty() || opponents.iter().any(|side| pool.contains(side)) {
        return 0;
    }
    FLAGSHIP_BONUS
}

// -- Crimson Legionnaire -------------------------------------------------------------------------

fn card_key(player: &PlayerId) -> String {
    format!("{CARD_PREFIX}{player}")
}

/// Crimson Legionnaire II units waiting on the card.
#[must_use]
pub fn units_on_card(state: &GameState, player: &PlayerId) -> usize {
    state
        .faction_marks
        .get(&card_key(player))
        .and_then(|count| count.parse().ok())
        .unwrap_or(0)
}

fn set_on_card(state: &mut GameState, player: &PlayerId, count: usize) {
    if count == 0 {
        state.faction_marks.remove(&card_key(player));
    } else {
        state
            .faction_marks
            .insert(card_key(player), count.to_string());
    }
}

/// "After this unit is destroyed, gain 1 commodity or convert 1 of your commodities to a trade
/// good." Mandatory: the owner chooses between the two only when both are possible (a commodity can
/// be gained below the faction's value, converted when at least one is held); when neither is, there
/// is nothing to take. For the II (`unit == INFANTRY2`) the unit is then placed on the card.
fn legionnaire(owner_name: &str, seat: &PlayerId, unit: &'static str) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:{unit}:GROUND_FORCE_DESTROYED:after"),
        seat.clone(),
        "GROUND_FORCE_DESTROYED",
        Relation::After,
        Arc::new(move |_, _, context| {
            let limit =
                crate::strategy_cards::commodity_limit(context.state, context.content, &owner);
            let held = context
                .state
                .player(&owner)
                .map_or(0, |seat| seat.commodities);
            let (can_gain, can_convert) = (held < limit, held > 0);
            let convert = if can_gain && can_convert {
                let choice = Choice::new(
                    owner.clone(),
                    "Crimson Legionnaire: gain 1 commodity or convert 1 to a trade good",
                    vec![
                        ChoiceOption::labelled("gain", "economy", "gain 1 commodity"),
                        ChoiceOption::labelled(
                            "convert",
                            "economy",
                            "convert 1 commodity to a trade good",
                        ),
                    ],
                )
                .contextualized(decision(context.state, &owner, unit, "legionnaire_payment"))
                // Display only: the owner's commodities now and at the limit.
                .hinted(
                    "commodities",
                    &crate::strategy_cards::commodity_standing(
                        context.state,
                        context.content,
                        &owner,
                    ),
                );
                context.ask_seeing(&choice).map_err(illegal)?.id == "convert"
            } else {
                can_convert
            };
            if convert {
                if let Some(seat) = context.state.player_mut(&owner) {
                    seat.commodities -= 1;
                }
                crate::supply::gain_trade_goods_staged(context.state, &owner, 1, unit);
            } else if can_gain {
                if let Some(seat) = context.state.player_mut(&owner) {
                    seat.commodities += 1;
                }
                crate::supply::note_commodities_gained(context.state, &owner, 1);
            }
            if unit == INFANTRY2 {
                let now = units_on_card(context.state, &owner) + 1;
                set_on_card(context.state, &owner, now);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, _| {
        event.text("player") == Some(condition_owner.as_str()) && event.text("unit") == Some(unit)
    }))
}

/// "At the start of your next turn, place each unit that is on this card on a planet you control in
/// your home system." One landing per unit, the owner choosing the planet when there are several; a
/// unit with no planet to land on stays on the card.
fn legionnaire_returns(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:{INFANTRY2}_return:TURN_BEGAN:after"),
        seat.clone(),
        "TURN_BEGAN",
        Relation::After,
        Arc::new(move |_, _, context| {
            let waiting = units_on_card(context.state, &owner);
            let Some(home) = context
                .state
                .player(&owner)
                .and_then(|seat| seat.home_system.clone())
            else {
                return Ok(());
            };
            let mut landings: Vec<PlanetId> = Vec::new();
            for _ in 0..waiting {
                let planets: Vec<PlanetId> = placement_spots(
                    context.state,
                    context.content,
                    context.sources,
                    &owner,
                    PlacementTarget::ControlledPlanet,
                    Some(&home),
                )
                .into_iter()
                .filter_map(|(_, planet)| planet)
                .collect();
                let planet = match planets.as_slice() {
                    [] => break,
                    [only] => only.clone(),
                    _ => {
                        let options = planets
                            .iter()
                            .map(|planet| {
                                ChoiceOption::labelled(
                                    planet.to_string(),
                                    "cl2",
                                    format!("place a Crimson Legionnaire on {planet}"),
                                )
                            })
                            .collect();
                        let choice = Choice::new(
                            owner.clone(),
                            "Crimson Legionnaire II: where in your home system",
                            options,
                        )
                        .contextualized(decision(
                            context.state,
                            &owner,
                            INFANTRY2,
                            "legionnaire_return",
                        ));
                        let answer = context.ask_seeing(&choice).map_err(illegal)?;
                        let Some(planet) = planets.iter().find(|p| p.as_str() == answer.id) else {
                            break;
                        };
                        planet.clone()
                    }
                };
                landings.push(planet);
            }
            let mut placed = 0;
            for planet in &landings {
                placed += place_units_counted(context, &owner, &home, Some(planet), "infantry", 1);
            }
            set_on_card(context.state, &owner, waiting.saturating_sub(placed));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_owner.as_str())
            && units_on_card(context.state, &condition_owner) > 0
    }))
}

// -- Starlancer ----------------------------------------------------------------------------------

/// Whether `holder` has a Starlancer standing in `system`, in its space area or on a planet.
fn mech_in(state: &GameState, holder: &PlayerId, system: &SystemId) -> bool {
    state.board.get(system).is_some_and(|board| {
        board
            .units
            .iter()
            .chain(board.planet_units.values().flatten())
            .any(|unit| unit.type_id.as_str() == MECH && &unit.owner == holder)
    })
}

/// Take one of `owner`'s tokens out of `mahact`'s fleet pool (the format is documented on
/// `mahact::fleet_mark`). `false`, nothing changed, when there is none.
fn take_token(state: &mut GameState, mahact: &PlayerId, owner: &PlayerId) -> bool {
    super::mahact::remove_token(state, mahact, owner)
}

/// Starlancer: "After a player whose command token is in your fleet pool activates this system, you
/// may spend their token from your fleet pool to end their turn; they gain that token."
///
/// Asks each eligible Mahact holder in seating order (a Starlancer in the system, the activator's
/// token in their fleet pool); the first to say yes spends the token, the activator gains it (into a
/// pool of their choice, like any command token gained) and the holder is returned so the caller ends
/// the turn exactly as for the Nullification Field. A failed gain restores the state and counts as a
/// refusal. Nothing is asked in a game without Mahact.
pub fn offer_starlancer(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    galaxy: Option<&Galaxy>,
    system: &SystemId,
    active: &PlayerId,
) -> Option<PlayerId> {
    let holders: Vec<PlayerId> = state
        .seating_order
        .iter()
        .filter(|holder| *holder != active)
        .filter(|holder| mech_in(state, holder, system))
        .filter(|holder| super::mahact::fleet_pool_owners(state, holder).contains(active))
        .cloned()
        .collect();
    for holder in holders {
        let choice = Choice::new(
            holder.clone(),
            format!("Starlancer: spend {active}'s command token to end their turn"),
            vec![
                ChoiceOption::labelled("use", "unit", "end their turn"),
                ChoiceOption::decline(),
            ],
        )
        .contextualized(decision(state, &holder, MECH, "starlancer_end_turn"));
        let Ok(answer) = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))
        else {
            continue;
        };
        if answer.is_decline() {
            continue;
        }
        let before = state.clone();
        if !take_token(state, &holder, active) {
            continue;
        }
        if crate::strategy_cards::gain_tokens(state, content, sources, galaxy, table, active, 1)
            .is_err()
        {
            *state = before;
            continue;
        }
        return Some(holder);
    }
    None
}

// -- Airo Shir Aur -------------------------------------------------------------------------------

fn unlocked(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.leaders.get(&LeaderId::new(HERO)) == Some(&LeaderStatus::Unlocked))
}

/// Whether the hero's combat is being fought: neither player can retreat or resolve an ability that
/// would move their ships. Read by `combat.rs` (retreats, Skilled Retreat).
#[must_use]
pub(crate) fn ship_movement_barred(state: &GameState) -> bool {
    state.faction_marks.contains_key(BENEDICTION_MARK)
}

/// The `(origin, destination)` pairs the hero can use: `player` has ships in the origin's space area,
/// the destination is adjacent and holds exactly one other seated player's ships and no neutral ones
/// (so the combat has two sides), in board order.
pub(crate) fn hero_pairs(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    player: &PlayerId,
) -> Vec<(SystemId, SystemId)> {
    let mut pairs = Vec::new();
    let adjacency = crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, player);
    for origin in state.board.keys() {
        if crate::combat::ships_of(state, content, sources, player, origin).is_empty() {
            continue;
        }
        for adjacent in adjacency.neighbours(origin.as_str()) {
            let destination = SystemId::new(adjacent.as_str());
            let others: Vec<PlayerId> =
                crate::combat::combatants(state, content, sources, &destination)
                    .into_iter()
                    .filter(|side| side != player)
                    .collect();
            if let [other] = others.as_slice()
                && !crate::neutral_units::is_neutral(other)
                && !pairs.contains(&(origin.clone(), destination.clone()))
            {
                pairs.push((origin.clone(), destination));
            }
        }
    }
    pairs
}

/// Whether the hero is usable now, as far as can be told without a map: unlocked, `player` has ships
/// somewhere and another seated player has ships somewhere. `None` for any other leader. The map
/// (adjacency) is checked by [`use_leader_timed`], which refuses with nothing changed.
pub(crate) fn leader_action(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == HERO).then(|| {
        let sources = SourceSet::all();
        let has_ships = |who: &PlayerId| {
            state.board.keys().any(|system| {
                !crate::combat::ships_of(state, content, sources, who, system).is_empty()
            })
        };
        unlocked(state, player)
            && has_ships(player)
            && state
                .seating_order
                .iter()
                .any(|other| other != player && has_ships(other))
    })
}

/// Airo Shir Aur: choose an origin system holding the owner's ships and an adjacent system holding a
/// different player's ships; move every unit of the owner's in the origin's space area there; fight
/// the space combat in the destination inside the game's timing resolver, with retreats and ship
/// movement barred for both sides. `None` for any other leader; `Some(false)`, nothing changed, when
/// it cannot resolve. The shared code purges the card after `Some(true)` and restores everything on
/// an error.
///
/// The active system is the destination for the length of the combat (the combat cards and the
/// nebula rule read it) and is restored afterwards.
pub(crate) fn use_leader_timed(
    context: &mut TimingContext<'_>,
    resolver: &mut Resolver,
    player: &PlayerId,
    leader: &LeaderId,
) -> Result<Option<bool>, TimingError> {
    if leader.as_str() != HERO {
        return Ok(None);
    }
    if leader_action(context.state, context.content, player, leader) != Some(true) {
        return Ok(Some(false));
    }
    let Some(galaxy) = context.galaxy else {
        return Ok(Some(false));
    };
    let pairs = hero_pairs(
        context.state,
        context.content,
        context.sources,
        galaxy,
        player,
    );
    let (origin, destination) = match pairs.as_slice() {
        [] => return Ok(Some(false)),
        [only] => only.clone(),
        _ => {
            let options = pairs
                .iter()
                .map(|(from, to)| {
                    ChoiceOption::labelled(
                        format!("{from}|{to}"),
                        "mahact_hero",
                        format!("move the fleet in {from} to {to}"),
                    )
                })
                .collect();
            let choice = Choice::new(
                player.clone(),
                "Airo Shir Aur: which fleet moves where",
                options,
            )
            .contextualized(decision(context.state, player, HERO, "mahact_hero_move"));
            let answer = context.ask_seeing(&choice).map_err(illegal)?;
            let Some(found) = pairs
                .iter()
                .find(|(from, to)| format!("{from}|{to}") == answer.id)
            else {
                return Ok(Some(false));
            };
            found.clone()
        }
    };

    let units: Vec<_> = context
        .state
        .system_state(&origin)
        .units_of(player)
        .into_iter()
        .cloned()
        .collect();
    context.state.move_units(&origin, &destination, &units);
    crate::fleet::enforce_seeing(
        context.state,
        context.content,
        context.sources,
        Some(galaxy),
        context.table,
        player,
        &destination,
    )
    .map_err(illegal)?;

    let previous = context.state.active_system.replace(destination.clone());
    context
        .state
        .faction_marks
        .insert(BENEDICTION_MARK.to_owned(), destination.to_string());
    let fought = {
        let TimingContext {
            state,
            content,
            sources,
            table,
            dice,
            rng,
            event_sequence,
            galaxy: map,
        } = context;
        let mut resolving = Resolving {
            content,
            sources: *sources,
            dice,
            rng,
            table,
            timing: Some(TimingHandle {
                resolver,
                sequence: event_sequence,
                galaxy: *map,
            }),
        };
        crate::combat::resolve_resolving(state, &mut resolving, &destination, *map)
    };
    context.state.faction_marks.remove(BENEDICTION_MARK);
    context.state.active_system = previous;
    match fought {
        Ok(_) => Ok(Some(true)),
        Err(crate::combat::CombatError::Timing(error)) => Err(error),
        Err(other) => Err(illegal(other.into_illegal_choice())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::choice::Scripted;
    use ti4_model::content_types::DEFAULT;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn game() -> GameState {
        let mut state = crate::fixtures::seated_game(&[("a", "mahact"), ("b", "sol")], DEFAULT);
        // Sol's own agent has a ground-combat window since rules version 16; these scripts do not
        // answer it.
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("solagent"), LeaderStatus::Exhausted);
        state
    }
    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
    }
    /// `b`'s token goes into `a`'s fleet pool (the shared mark, set directly until Edict lands).
    fn pool(state: &mut GameState, owners: &[&str]) {
        state
            .faction_marks
            .insert(super::super::mahact::fleet_mark(&a()), owners.join(","));
    }
    fn count(state: &GameState, system: &SystemId, player: &PlayerId, kind: &str) -> usize {
        state
            .system_state(system)
            .units
            .iter()
            .filter(|unit| &unit.owner == player && unit.type_id.as_str() == kind)
            .count()
    }
    fn emit(
        state: &mut GameState,
        table: &mut Table,
        kind: &str,
        pairs: &[(&str, serde_json::Value)],
    ) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, table, |ctx| {
            let payload = pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect();
            let event = crate::event::EventSequence::new()
                .next(kind, payload)
                .unwrap();
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("emits");
        });
    }
    fn destroyed(state: &mut GameState, table: &mut Table, unit: &str, who: &str) {
        let (system, planet) = crate::fixtures::a_placed_planet();
        emit(
            state,
            table,
            "GROUND_FORCE_DESTROYED",
            &[
                ("player", who.into()),
                ("unit", unit.into()),
                ("system", system.to_string().into()),
                ("planet", planet.to_string().into()),
            ],
        );
    }
    fn limit(state: &GameState) -> i32 {
        crate::strategy_cards::commodity_limit(state, ContentStore::embedded(), &a())
    }
    fn commodities(state: &GameState) -> i32 {
        state.player(&a()).unwrap().commodities
    }
    fn goods(state: &GameState) -> i32 {
        state.player(&a()).unwrap().trade_goods
    }

    // -- claims and neutrality ------------------------------------------------------------------

    #[test]
    fn the_claims_and_the_hero_are_real_mahact_assets() {
        let sheet = super::super::assets(ContentStore::embedded(), DEFAULT, "mahact");
        for id in UNITS
            .iter()
            .chain(LEADERS)
            .chain(&[MECH, HERO, FLAGSHIP, INFANTRY, INFANTRY2])
        {
            assert!(sheet.iter().any(|asset| asset.id == *id), "{id}");
        }
        assert!(UNITS.contains(&FLAGSHIP) && UNITS.contains(&INFANTRY2));
    }

    #[test]
    fn games_without_mahact_see_nothing_of_the_units_or_the_hero() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let before = state.clone();
        let content = ContentStore::embedded();
        let mut table = scripted(&[]);
        let (system, planet) = crate::fixtures::a_placed_planet();
        for (kind, pairs) in [
            (
                "GROUND_FORCE_DESTROYED",
                vec![
                    ("player", serde_json::Value::from("a")),
                    ("unit", "infantry".into()),
                    ("system", system.to_string().into()),
                    ("planet", planet.to_string().into()),
                ],
            ),
            ("TURN_BEGAN", vec![("player", "a".into())]),
        ] {
            emit(&mut state, &mut table, kind, &pairs);
        }
        assert_eq!(state, before);
        // The flagship bonus is zero for every other flagship, and the mech/hero hooks decline.
        state.active_system = Some(system.clone());
        assert_eq!(
            flagship_roll_bonus(&state, content, DEFAULT, &a(), "sol_flagship"),
            0
        );
        assert_eq!(
            offer_starlancer(
                &mut state,
                content,
                DEFAULT,
                &mut table,
                None,
                &system,
                &b()
            ),
            None
        );
        assert!(!ship_movement_barred(&state));
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new("solhero")),
            None
        );
        state.active_system = None;
        assert_eq!(state, before);
    }

    // -- Arvicon Rex ----------------------------------------------------------------------------

    fn flagship_fight() -> (GameState, SystemId) {
        let mut state = game();
        let system = SystemId::new("18");
        state.system_mut(&system).units.clear();
        state.active = Some(a());
        state.active_system = Some(system.clone());
        crate::fixtures::put(&mut state, &system, FLAGSHIP, &a(), 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        (state, system)
    }

    fn flagship() -> ti4_model::units::Unit {
        ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new(FLAGSHIP), a())
    }

    #[test]
    fn arvicon_rex_rolls_two_better_against_an_opponent_whose_token_is_not_in_the_pool() {
        let content = ContentStore::embedded();
        let (mut state, _) = flagship_fight();
        let printed = crate::combat::hits_on(content, DEFAULT, &flagship()).unwrap();
        assert_eq!(printed, 5);
        let rolled = |state: &GameState| {
            crate::combat::effective_hits_on(state, content, DEFAULT, &a(), &flagship())
        };
        assert_eq!(rolled(&state), Some(3), "no token of b's in the pool");
        pool(&mut state, &["b"]);
        assert_eq!(rolled(&state), Some(5), "b's token is in the pool");
        pool(&mut state, &["c", "d"]);
        assert_eq!(rolled(&state), Some(3), "only other players' tokens");
        // The bonus belongs to the flagship alone, and to a combat in the active system.
        let cruiser = ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new("cruiser"), a());
        let plain = crate::combat::hits_on(content, DEFAULT, &cruiser);
        assert_eq!(
            crate::combat::effective_hits_on(&state, content, DEFAULT, &a(), &cruiser),
            plain
        );
        state.active_system = None;
        assert_eq!(rolled(&state), Some(5), "no combat, no bonus");
    }

    #[test]
    fn arvicon_rex_decides_a_real_combat() {
        let content = ContentStore::embedded();
        let fight = |in_pool: bool| {
            let (mut state, system) = flagship_fight();
            if in_pool {
                pool(&mut state, &["b"]);
            }
            // Round 1: the flagship's two dice show 3 (a hit only with the bonus), the cruiser misses.
            // Round 2: the flagship hits either way.
            let mut dice = crate::dice::Dice::from_faces([3, 3, 1, 10, 10, 1]);
            let mut rng = crate::rng::GameRng::new(0);
            let mut table = Table::with_default(Box::new(crate::choice::AlwaysDecline));
            crate::combat::resolve(
                &mut state, content, DEFAULT, &mut table, &mut dice, &mut rng, &system,
            )
            .unwrap()
        };
        let boosted = fight(false);
        assert_eq!((boosted.winner, boosted.rounds), (Some(a()), 1));
        let plain = fight(true);
        assert_eq!((plain.winner, plain.rounds), (Some(a()), 2));
    }

    // -- Crimson Legionnaire ----------------------------------------------------------------------

    #[test]
    fn legionnaire_gains_a_commodity_or_converts_one_and_asks_only_when_both_are_possible() {
        let mut state = game();
        let cap = limit(&state);
        assert!(cap >= 2);
        // Both possible: asked.
        state.player_mut(&a()).unwrap().commodities = 1;
        let mut gained = state.clone();
        destroyed(&mut gained, &mut scripted(&["gain"]), INFANTRY, "a");
        assert_eq!((commodities(&gained), goods(&gained)), (2, goods(&state)));
        let mut converted = state.clone();
        destroyed(&mut converted, &mut scripted(&["convert"]), INFANTRY, "a");
        assert_eq!(
            (commodities(&converted), goods(&converted)),
            (0, goods(&state) + 1)
        );
        // Full: only convert, not asked.
        state.player_mut(&a()).unwrap().commodities = cap;
        let mut full = state.clone();
        destroyed(&mut full, &mut scripted(&[]), INFANTRY, "a");
        assert_eq!(
            (commodities(&full), goods(&full)),
            (cap - 1, goods(&state) + 1)
        );
        // Empty: only gain.
        state.player_mut(&a()).unwrap().commodities = 0;
        let mut empty = state.clone();
        destroyed(&mut empty, &mut scripted(&[]), INFANTRY, "a");
        assert_eq!((commodities(&empty), goods(&empty)), (1, goods(&state)));
        assert_eq!(
            units_on_card(&empty, &a()),
            0,
            "the I is not placed on a card"
        );
    }

    #[test]
    fn legionnaire_is_not_triggered_for_other_units_or_other_players() {
        let mut state = game();
        state.player_mut(&a()).unwrap().commodities = 1;
        destroyed(&mut state, &mut scripted(&[]), "infantry", "a");
        destroyed(&mut state, &mut scripted(&[]), INFANTRY, "b");
        assert_eq!(
            (
                commodities(&state),
                goods(&state),
                units_on_card(&state, &a())
            ),
            (1, 0, 0)
        );
    }

    #[test]
    fn legionnaire_dies_in_a_real_ground_combat_and_pays() {
        let content = ContentStore::embedded();
        let mut state = game();
        let (system, planet) = crate::fixtures::a_placed_planet();
        let board = state.system_mut(&system);
        board.units.clear();
        board.planet_units.clear();
        board.planet_control.clear();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, INFANTRY, &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        state.active = Some(b());
        state.player_mut(&a()).unwrap().commodities = 0;
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut dice = crate::dice::Dice::from_faces([10, 10]);
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = scripted(&[]);
        let mut sequence = crate::event::EventSequence::new();
        let mut resolving = Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        // b invades; both sides hit, both infantry die.
        crate::invasion::InvasionWindow::fight_committed_planet(
            &mut state,
            &mut resolving,
            &b(),
            &system,
            &planet,
        )
        .unwrap();
        assert_eq!(
            count_on(&state, &system, &planet, &a(), INFANTRY),
            0,
            "destroyed"
        );
        assert_eq!(commodities(&state), 1, "and gained a commodity");
    }

    fn count_on(
        state: &GameState,
        system: &SystemId,
        planet: &PlanetId,
        player: &PlayerId,
        kind: &str,
    ) -> usize {
        state
            .system_state(system)
            .on_planet_of(planet, player)
            .iter()
            .filter(|unit| unit.type_id.as_str() == kind)
            .count()
    }

    #[test]
    fn legionnaire_two_goes_on_the_card_and_returns_at_the_start_of_the_next_turn() {
        let mut state = game();
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        state.player_mut(&a()).unwrap().commodities = 0;
        destroyed(&mut state, &mut scripted(&[]), INFANTRY2, "a");
        destroyed(&mut state, &mut scripted(&[]), INFANTRY2, "a");
        assert_eq!(
            units_on_card(&state, &a()),
            2,
            "two on the card, off the board"
        );
        assert_eq!(commodities(&state), 2);
        let infantry = |state: &GameState| {
            state
                .system_state(&home)
                .planet_units
                .values()
                .flatten()
                .filter(|unit| unit.owner == a() && unit.type_id.as_str().ends_with("infantry"))
                .count()
        };
        let planets: Vec<PlanetId> = state
            .system_state(&home)
            .planet_control
            .iter()
            .filter(|(_, owner)| **owner == a())
            .map(|(planet, _)| planet.clone())
            .collect();
        assert!(!planets.is_empty());
        let before = infantry(&state);
        // Another player's turn starting changes nothing.
        let mut other = state.clone();
        emit(
            &mut other,
            &mut scripted(&[]),
            "TURN_BEGAN",
            &[("player", "b".into())],
        );
        assert_eq!((units_on_card(&other, &a()), infantry(&other)), (2, before));
        // Their own does; with several planets in the home system they choose each landing.
        let answers: Vec<String> = std::iter::repeat_n(planets[0].to_string(), 2).collect();
        let script: Vec<&str> = std::iter::empty::<&str>()
            .chain(answers.iter().map(String::as_str))
            .collect();
        emit(
            &mut state,
            &mut scripted(&script),
            "TURN_BEGAN",
            &[("player", "a".into())],
        );
        assert_eq!(units_on_card(&state, &a()), 0);
        assert_eq!(
            infantry(&state),
            before + 2,
            "both landed in the home system"
        );
    }

    // -- Starlancer -----------------------------------------------------------------------------

    fn mech_game() -> (GameState, SystemId) {
        let mut state = game();
        let system = SystemId::new("19");
        state.system_mut(&system).units.clear();
        crate::fixtures::put(&mut state, &system, MECH, &a(), 1);
        pool(&mut state, &["b", "b", "c"]);
        (state, system)
    }

    #[test]
    fn starlancer_spends_the_token_and_the_activator_gains_it() {
        let content = ContentStore::embedded();
        let (mut state, system) = mech_game();
        let tokens = |state: &GameState| {
            let seat = state.player(&b()).unwrap();
            seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens
        };
        let before = tokens(&state);
        let mut table = scripted(&["use", "tactic_tokens"]);
        let used = offer_starlancer(
            &mut state,
            content,
            DEFAULT,
            &mut table,
            None,
            &system,
            &b(),
        );
        assert_eq!(used, Some(a()));
        assert_eq!(
            crate::factions::mahact::fleet_pool_owners(&state, &a()),
            vec![b(), PlayerId::new("c")],
            "one of b's two tokens spent"
        );
        assert_eq!(tokens(&state), before + 1, "b gained it");
    }

    #[test]
    fn starlancer_is_optional_and_needs_the_mech_the_system_and_the_token() {
        let content = ContentStore::embedded();
        let (state, system) = mech_game();
        let offered = |state: &GameState, system: &SystemId, active: &PlayerId| {
            let mut state = state.clone();
            let before = state.clone();
            let used = offer_starlancer(
                &mut state,
                content,
                DEFAULT,
                &mut scripted(&["decline"]),
                None,
                system,
                active,
            );
            assert_eq!(used, None);
            assert_eq!(state, before, "declined or not offered: unchanged");
        };
        offered(&state, &system, &b());
        // Not offered (a scripted "use" would diverge if it were asked): wrong system, no token,
        // the owner's own activation, mech on another system.
        let silent = |state: &GameState, system: &SystemId, active: &PlayerId| {
            let mut state = state.clone();
            let before = state.clone();
            let used = offer_starlancer(
                &mut state,
                content,
                DEFAULT,
                &mut scripted(&[]),
                None,
                system,
                active,
            );
            assert_eq!(used, None);
            assert_eq!(state, before);
        };
        silent(&state, &SystemId::new("20"), &b());
        silent(&state, &system, &PlayerId::new("d"));
        silent(&state, &system, &a());
        let mut empty = state.clone();
        empty.faction_marks.clear();
        silent(&empty, &system, &b());
    }

    #[test]
    fn starlancer_on_a_planet_counts_as_in_the_system() {
        let content = ContentStore::embedded();
        let mut state = game();
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, MECH, &a(), 1);
        pool(&mut state, &["b"]);
        let used = offer_starlancer(
            &mut state,
            content,
            DEFAULT,
            &mut scripted(&["use", "fleet_tokens"]),
            None,
            &system,
            &b(),
        );
        assert_eq!(used, Some(a()));
        assert!(crate::factions::mahact::fleet_pool_owners(&state, &a()).is_empty());
    }

    // -- Airo Shir Aur --------------------------------------------------------------------------

    fn hero_game() -> (GameState, crate::fixtures::Hub, SystemId, SystemId) {
        let hub = crate::fixtures::plain_hub();
        let mut state = game();
        let origin = SystemId::new(&hub.outer[0]);
        let destination = SystemId::new(&hub.centre);
        for system in [&origin, &destination] {
            state.system_mut(system).units.clear();
        }
        crate::fixtures::put(&mut state, &origin, "cruiser", &a(), 2);
        crate::fixtures::put(&mut state, &origin, "fighter", &a(), 2);
        crate::fixtures::put(&mut state, &destination, "cruiser", &b(), 1);
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(HERO), LeaderStatus::Unlocked);
        state.active = Some(a());
        (state, hub, origin, destination)
    }

    fn use_hero(
        state: &mut GameState,
        hub: &crate::fixtures::Hub,
        answers: &[&str],
        faces: &[u32],
    ) -> Result<Option<bool>, TimingError> {
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut table = scripted(answers);
        let mut dice = crate::dice::Dice::from_faces(faces.iter().copied());
        let mut rng = crate::rng::GameRng::new(5);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: DEFAULT,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: Some(&hub.galaxy),
        };
        use_leader_timed(&mut context, &mut resolver, &a(), &LeaderId::new(HERO))
    }

    #[test]
    fn the_hero_moves_the_whole_space_area_and_fights_without_retreat() {
        let (mut state, hub, origin, destination) = hero_game();
        let content = ContentStore::embedded();
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new(HERO)),
            Some(true)
        );
        // The two cruisers hit (6+... faces are high), b's single cruiser dies in round 1.
        let done = use_hero(&mut state, &hub, &[], &[10, 10, 10, 10, 1]);
        assert_eq!(done, Ok(Some(true)));
        assert_eq!(
            count(&state, &origin, &a(), "cruiser"),
            0,
            "left the origin"
        );
        assert_eq!(count(&state, &origin, &a(), "fighter"), 0, "fighters too");
        assert!(
            count(&state, &destination, &a(), "cruiser") >= 1,
            "the fleet arrived"
        );
        assert_eq!(
            count(&state, &destination, &b(), "cruiser"),
            0,
            "the combat was fought"
        );
        assert!(
            !ship_movement_barred(&state),
            "the bar ends with the combat"
        );
        assert_eq!(state.active_system, None, "the active system is restored");
    }

    #[test]
    fn neither_side_may_retreat_during_the_hero_combat() {
        let (mut state, hub, _, destination) = hero_game();
        // Rounds of misses would offer a retreat; with the bar nobody is ever asked. Scripted
        // answers are empty, so any question would diverge. Round 1 all miss; round 2 a hits.
        let done = use_hero(&mut state, &hub, &[], &[1, 1, 1, 1, 1, 10, 10, 10, 10, 1]);
        assert_eq!(done, Ok(Some(true)));
        // And while the bar is up neither retreat nor Skilled Retreat has anywhere to go.
        state
            .faction_marks
            .insert(BENEDICTION_MARK.to_owned(), destination.to_string());
        let content = ContentStore::embedded();
        assert!(
            crate::combat::skilled_retreat_destinations(
                &state,
                content,
                DEFAULT,
                &hub.galaxy,
                &b(),
                &destination
            )
            .is_empty()
        );
        state.faction_marks.remove(BENEDICTION_MARK);
        assert!(
            !crate::combat::skilled_retreat_destinations(
                &state,
                content,
                DEFAULT,
                &hub.galaxy,
                &b(),
                &destination
            )
            .is_empty()
        );
    }

    #[test]
    fn the_hero_asks_which_pair_and_refuses_when_there_is_none() {
        let (mut state, hub, origin, destination) = hero_game();
        // A second origin (every outer tile touches the centre): two pairs, so the owner is asked.
        let second = SystemId::new(&hub.outer[1]);
        state.system_mut(&second).units.clear();
        crate::fixtures::put(&mut state, &second, "cruiser", &a(), 2);
        let pair = format!("{second}|{destination}");
        let done = use_hero(&mut state, &hub, &[&pair], &[10, 10, 10, 10, 1]);
        assert_eq!(done, Ok(Some(true)));
        assert_eq!(
            count(&state, &second, &a(), "cruiser"),
            0,
            "the chosen fleet moved"
        );
        assert_eq!(
            count(&state, &origin, &a(), "cruiser"),
            2,
            "the other stayed"
        );
        // No other player's ships adjacent: refused, nothing changed.
        let (mut quiet, hub, _, destination) = hero_game();
        quiet.system_mut(&destination).units.clear();
        let before = quiet.clone();
        assert_eq!(use_hero(&mut quiet, &hub, &[], &[]), Ok(Some(false)));
        assert_eq!(quiet, before);
        // Locked: not available.
        let (mut locked, hub, _, _) = hero_game();
        locked
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(HERO), LeaderStatus::Locked);
        let before = locked.clone();
        assert_eq!(use_hero(&mut locked, &hub, &[], &[]), Ok(Some(false)));
        assert_eq!(locked, before);
    }

    #[test]
    fn a_nekro_flagship_with_the_mahact_z_token_rolls_two_better() {
        let content = ContentStore::embedded();
        let nekro_flagship =
            ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new("nekro_flagship"), a());
        let rolled = |lent: &[&str]| {
            let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], lent);
            let system = SystemId::new("18");
            state.system_mut(&system).units.clear();
            state.active = Some(a());
            state.active_system = Some(system.clone());
            crate::fixtures::put(&mut state, &system, "nekro_flagship", &a(), 1);
            crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
            crate::combat::effective_hits_on(&state, content, DEFAULT, &a(), &nekro_flagship)
        };
        let printed = crate::combat::hits_on(content, DEFAULT, &nekro_flagship).unwrap();
        assert_eq!(rolled(&[]), Some(printed), "off by default");
        assert_eq!(rolled(&["mahact"]), Some(printed - 2));
    }
}
